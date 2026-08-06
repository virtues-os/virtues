//! On-box voice-activity gate for the transcription pipeline.
//!
//! Runs NVIDIA Frame-VAD MarbleNet v2 — a tiny (90K-param) conv-only VAD — via
//! the pure-Rust `tract` engine on each recording BEFORE the paid Gemini call.
//! ~65% of continuous all-day phone audio has no speech (silence, traffic,
//! music, room tone); we detect those and record them as silent instead of
//! paying Gemini to "transcribe" nothing. This is the dominant cost lever
//! (~$48→~$22/mo at 24/7). The audio itself is still stored, so any skipped
//! chunk stays re-runnable if a local transcription model arrives later.
//!
//! Pipeline: m4a (AAC) --symphonia--> 16kHz mono PCM --> 80-dim NeMo log-mel
//! --tract--> per-frame speech probability --> speech-seconds threshold.
//!
//! MarbleNet is chosen over Silero because it is control-flow-free (Silero's
//! decoder `If` op does not translate in tract) and over Earshot/WebRTC-VAD
//! because those keep loud ambient (music/traffic) as "speech". Known blind
//! spot in the opposite direction: speech buried under loud music (parties,
//! Minecraft-with-friends over speakers) scores near zero, so those chunks are
//! recorded silent — the stored audio keeps them re-runnable. The mel
//! front-end is bit-parity-validated against NVIDIA's NeMo preprocessing: the
//! full pipeline's speech/no-speech decision is identical to the Python +
//! onnxruntime reference on the validation set (0 flips / 24 clips).

use anyhow::{anyhow, Context, Result};
use realfft::RealFftPlanner;
use std::io::Cursor;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use tract_onnx::prelude::*;

// Bundled assets, compiled into the binary (~450KB total, no runtime files).
// mel_fb / hann are exported straight from librosa so the Rust front-end uses
// the *identical* filterbank + window as the validated reference (avoids any
// filterbank-construction mismatch).
static MODEL_ONNX: &[u8] = include_bytes!("assets/marblenet.onnx");
static MEL_FB: &[u8] = include_bytes!("assets/mel_fb.f32"); // librosa mel filterbank [80,257] f32-LE
static HANN: &[u8] = include_bytes!("assets/hann400.f32"); // periodic hann(400) f32-LE

const SR: u32 = 16000;
const N_MELS: usize = 80;
const N_FFT: usize = 512;
const WIN: usize = 400; // 25ms
const HOP: usize = 160; // 10ms
const N_FREQ: usize = N_FFT / 2 + 1; // 257
const PREEMPH: f32 = 0.97;
/// A frame counts as speech at this probability. The original 0.5 proved too
/// permissive in production — overnight snoring/breathing/room-tone frames
/// hover just past even odds, and 60/96 sleeping-hours chunks reached Gemini
/// only for it to return empty text. At 0.65 those collapse to ~0 speech-secs
/// while soft real speech (whispered self-talk, cross-room chatter) retains
/// signal. Validated 2026-07-20 on 32 labeled clips from the box via
/// examples/vad_sweep.rs: overnight false-positives 12→2 passed, quiet-speech
/// retention 9/12, zero loss on clear conversation.
const SPEECH_PROB: f32 = 0.65;
/// Only contiguous speech runs at least this long count toward the total.
/// Kept at 0 (off): in the same validation a 0.1s floor cost a genuine
/// fragmented-speech clip (muffled instructions in a noisy room) without
/// killing any additional false positives. The probability floor does the
/// separating; run length is retained as a knob for the sweep harness.
const MIN_RUN_SECS: f32 = 0.0;
/// A chunk counts as "speech" if qualifying runs total at least this many
/// seconds. Deliberately low: the goal is only to skip chunks with NO
/// speech — anything with a real utterance must reach Gemini.
pub const MIN_SPEECH_SECS: f32 = 0.25;

/// Total qualifying speech seconds over per-frame probabilities: frames ≥
/// `p_speech` form runs; runs shorter than `min_run_secs` are discarded; the
/// survivors' seconds are summed. The drain treats a recording as no-speech when
/// this total is below `MIN_SPEECH_SECS` (skip Gemini), and otherwise feeds the
/// magnitude into the honesty ground-truth and the hallucination guard.
pub fn speech_total(probs: &[f32], dur: f32, p_speech: f32, min_run_secs: f32) -> f32 {
    let frame_secs = dur / probs.len().max(1) as f32;
    let mut total = 0f32;
    let mut run = 0usize;
    for (i, &p) in probs.iter().enumerate() {
        if p >= p_speech {
            run += 1;
        }
        if p < p_speech || i == probs.len() - 1 {
            let run_secs = run as f32 * frame_secs;
            if run_secs >= min_run_secs {
                total += run_secs;
            }
            run = 0;
        }
    }
    total
}

pub struct Vad {
    model: InferenceModel,
    mel_fb: Vec<f32>, // [80,257] row-major
    hann: Vec<f32>,   // [400]
}

fn as_f32(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4)
        .map(|c| f32::from_le_bytes(c.try_into().unwrap()))
        .collect()
}

impl Vad {
    /// Load the bundled MarbleNet graph + mel assets. Cheap (~ms); build once
    /// per drain and reuse across recordings.
    pub fn new() -> Result<Self> {
        let model = tract_onnx::onnx()
            .model_for_read(&mut Cursor::new(MODEL_ONNX))
            .context("load bundled MarbleNet ONNX")?;
        Ok(Self {
            model,
            mel_fb: as_f32(MEL_FB),
            hann: as_f32(HANN),
        })
    }

    /// Total measured speech seconds in the recording. Drives the drain's
    /// no-speech skip (total < `MIN_SPEECH_SECS` → silent, no Gemini call) and
    /// the honesty ground-truth handed to the model. `None` on any
    /// decode/inference error — callers treat that as "unknown" and never
    /// suppress on it (fail-open: a VAD problem can never silently drop real
    /// speech, at worst one unnecessary Gemini call).
    ///
    /// PRESENCE, NOT DURATION. Treat this as "was there speech at all", never
    /// as "how much speech there was". MarbleNet fires on onsets, so on real
    /// conversation it recovers only a fraction of the true speech time —
    /// measured 5.6-31.5s against 50-86s of actual voice activity on a 17-clip
    /// corpus from this box, i.e. undermeasuring by 1.5-14×, with a longest
    /// contiguous run of ~3-6s even at p=0.5. As a presence signal it is
    /// excellent on that same corpus (17/17 speech pass, 0/21 silence pass, and
    /// stable across p=0.6..0.75). Budgeting anything proportional to this
    /// value silently deleted 16 days of transcripts once already — see
    /// MAX_CHARS_PER_AUDIO_SEC in transform.rs.
    pub fn speech_seconds(&self, m4a: &[u8]) -> Option<f32> {
        match self.speech_probs(m4a) {
            Ok(Some((probs, dur))) => {
                Some(speech_total(&probs, dur, SPEECH_PROB, MIN_RUN_SECS))
            }
            Ok(None) => Some(0.0), // too short to contain speech
            Err(e) => {
                tracing::warn!(error = %e, "VAD speech_seconds failed; measurement unknown");
                None
            }
        }
    }

    /// Per-frame speech probability for the whole recording, plus its duration
    /// in seconds. `None` if the audio is shorter than one analysis frame.
    /// Exposed for the offline threshold-sweep harness (examples/vad_sweep.rs);
    /// production goes through `has_speech`.
    pub fn speech_probs(&self, m4a: &[u8]) -> Result<Option<(Vec<f32>, f32)>> {
        let pcm = decode_m4a_16k_mono(m4a)?;
        if pcm.len() < N_FFT {
            return Ok(None);
        }
        let (feat, frames_in) = self.logmel(&pcm);

        // MarbleNet is conv-only + stateless; rebuild the runnable for this
        // frame count (~10ms) and run once.
        let mut m = self.model.clone();
        m.set_input_fact(0, f32::fact([1, N_MELS, frames_in]).into())?;
        let runnable = m.into_optimized()?.into_runnable()?;
        let input = Tensor::from_shape(&[1, N_MELS, frames_in], &feat)?;
        let out = runnable.run(tvec!(input.into()))?;
        let scores = out[0].to_array_view::<f32>()?; // [1, frames_out, 2]
        let frames_out = scores.shape()[1];

        let mut probs = Vec::with_capacity(frames_out);
        for i in 0..frames_out {
            let (non, sp) = (scores[[0, i, 0]], scores[[0, i, 1]]);
            // softmax over the 2 logits; class 1 is speech.
            probs.push(sp.exp() / (non.exp() + sp.exp()));
        }
        let dur = pcm.len() as f32 / SR as f32;
        Ok(Some((probs, dur)))
    }

    /// 80-dim NeMo-style log-mel: preemphasis → centered STFT (zero-pad, hann)
    /// → mel filterbank → log → per-feature normalization. Matches NVIDIA's
    /// `AudioToMelSpectrogramPreprocessor` (validated bit-parity on mid-frames;
    /// decision-parity end-to-end).
    fn logmel(&self, pcm: &[f32]) -> (Vec<f32>, usize) {
        let n = pcm.len();
        let mut y = vec![0f32; n];
        y[0] = pcm[0];
        for i in 1..n {
            y[i] = pcm[i] - PREEMPH * pcm[i - 1];
        }
        // center=True with 'constant' (zero) padding — librosa's default.
        let pad = N_FFT / 2;
        let mut yp = vec![0f32; n + 2 * pad];
        yp[pad..pad + n].copy_from_slice(&y);
        let nf = 1 + (yp.len() - N_FFT) / HOP;
        let woff = (N_FFT - WIN) / 2; // hann(400) centered in the 512-pt frame

        let mut planner = RealFftPlanner::<f32>::new();
        let r2c = planner.plan_fft_forward(N_FFT);
        let mut inb = r2c.make_input_vec();
        let mut outb = r2c.make_output_vec();
        let mut lm = vec![0f32; N_MELS * nf]; // [80, nf] row-major
        let guard = 2f32.powi(-24); // NeMo log_zero_guard

        for t in 0..nf {
            let s = t * HOP;
            for v in inb.iter_mut() {
                *v = 0.0;
            }
            for k in 0..WIN {
                inb[woff + k] = yp[s + woff + k] * self.hann[k];
            }
            r2c.process(&mut inb, &mut outb).expect("fft");
            for m in 0..N_MELS {
                let mut e = 0f32;
                for f in 0..N_FREQ {
                    e += self.mel_fb[m * N_FREQ + f] * outb[f].norm_sqr();
                }
                lm[m * nf + t] = (e + guard).ln();
            }
        }
        // per-feature (per-mel-bin) normalization over time, ddof=0.
        for m in 0..N_MELS {
            let s = m * nf;
            let mean = lm[s..s + nf].iter().sum::<f32>() / nf as f32;
            let var =
                lm[s..s + nf].iter().map(|x| (x - mean) * (x - mean)).sum::<f32>() / nf as f32;
            let std = var.sqrt();
            for t in 0..nf {
                lm[s + t] = (lm[s + t] - mean) / (std + 1e-5);
            }
        }
        (lm, nf)
    }
}

/// Decode m4a (AAC) bytes to mono f32 PCM. The box records at 16kHz mono, so no
/// resampling is needed; a differing rate is treated as an error (→ fail-open).
fn decode_m4a_16k_mono(bytes: &[u8]) -> Result<Vec<f32>> {
    let mss = MediaSourceStream::new(Box::new(Cursor::new(bytes.to_vec())), Default::default());
    let mut hint = Hint::new();
    hint.with_extension("m4a");
    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .context("probe m4a")?;
    let mut format = probed.format;
    let track = format
        .default_track()
        .ok_or_else(|| anyhow!("no audio track in m4a"))?;
    let track_id = track.id;
    let sr = track.codec_params.sample_rate.unwrap_or(SR);
    if sr != SR {
        return Err(anyhow!("unexpected sample rate {sr} (expected {SR})"));
    }
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .context("make AAC decoder")?;

    let mut out: Vec<f32> = Vec::new();
    while let Ok(packet) = format.next_packet() {
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            Err(_) => continue, // skip a bad packet, keep going
        };
        let spec = *decoded.spec();
        let ch = spec.channels.count().max(1);
        let mut buf = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
        buf.copy_interleaved_ref(decoded);
        for frame in buf.samples().chunks(ch) {
            out.push(frame.iter().sum::<f32>() / ch as f32); // downmix to mono
        }
    }
    Ok(out)
}

