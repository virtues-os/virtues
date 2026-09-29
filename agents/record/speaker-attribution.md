# Speaker attribution: measured, then paused

**Written 2026-09-29.** Four days of spikes (2026-09-24 to 09-27) on telling
the owner's voice apart from everyone else's in the phone's ambient audio, and
the decision that came out of them. What shipped is the small part:
transcripts mark a change of voice as `[Speaker]:` with no number, and older
numbered tags are flattened where narrators read them
(`virtues-core/src/transcript.rs`). The design for the rest is parked in
[`agents/plan/speaker-attribution-plan.md`](../plan/speaker-attribution-plan.md).

---

## Why it was looked at

In a day-article bake-off, who-said-what was the largest single source of
invented claims. Transcription asked Gemini for `[Speaker 1]:` /
`[Speaker 2]:`, and those numbers are local to one 5-minute chunk:
`Speaker 1` is whoever the model heard first. Across chunks the numbers look
consistent and are not, and the narrator built stories on that consistency.

## What was measured

**Gemini cannot carry attribution.** Asked for per-turn timestamps on 300 s
clips, `gemini-3-flash` returned timelines ending at 177–191 s. Its speaker
labels matched real voices with 52–72% purity, where 50% is chance. The inverse
does work: split a chunk into voice turns on the box and send them as ordered
audio parts in one request, and each part comes back transcribed. That path
recovered 76–92% of the single-shot word count, and that recall gap is its
open cost.

**sherpa-onnx computes the wrong features for common speaker models.** On a
labeled benchmark (below), owner vs everyone else in 1.5 s windows:

| Model | Through sherpa-onnx's extractor | Model's own front-end |
|---|---|---|
| WeSpeaker ResNet34 | EER 15.2% | EER 0.7% |
| WeSpeaker CAM++ | 21.8% | 2.9% |
| 3D-Speaker CAM++ | 12.2% | 2.2% |

The front-end that works is Kaldi fbank (80 bins, `high_freq = 0`) with the
per-window mean subtracted from every bin. sherpa skips that subtraction for
the WeSpeaker models and feeds 3D-Speaker CAM++ features that match no setting
tried. **Never use sherpa-onnx's extractor as a parity reference for these
models.** Every earlier result that went through it was measured on degraded
features.

**The benchmark.** The AMI meeting corpus's lapel mics are the closest public
proxy for a phone worn on the body: each participant's own lapel stands in for
the owner's phone. 15 lapels where the wearer is at least 5 dB louder than
anyone else, 7 owners, 3 sites, 92 five-minute chunks. Enrollment always came
from a different meeting of the same person. Thresholds were chosen
leave-one-site-out.

**The voice model stopped being the limit once the features were right.**
Every sliding-window system converged on frame F1 ≈ 0.70 and turn F1 ≈ 0.79,
whatever the embedder. The ceiling was elsewhere:

| Speech detector | Speech P / R | ResNet34 windows: frame F1 / turn F1 |
|---|---|---|
| RMS energy gate | 0.97 / 0.45 | 0.70 / 0.78 |
| MarbleNet, the box's p ≥ 0.65 | 0.97 / 0.33 | 0.68 / 0.74 |
| pyannote segmentation-3.0, p ≥ 0.5 | 0.97 / 0.96 | 0.88 / 0.79 |
| Perfect boundaries from the annotation | 1 / 1 | 0.90 / 0.80 |

MarbleNet is a presence detector that fires on onsets; it is right for
skipping silent chunks and wrong for boundaries. The remaining turn-level gap
is crosstalk: 96% of false "owner" turns were another person's turn that the
owner talked over. Marking stretches where segmentation-3.0 reports overlap as
*unsure* kept frames labeled "owner" that sit more than 0.75 s from any owner
speech under 1.5% of all owner-labeled frames. The owner was never labeled while silent in any turn checked by hand.

**Real audio, with the owner listening.** 40 random chunks from one box over
30 days, with the owner rating clips blind:
- Two voices recur across the month. Loudness alone did not say which one
  was the owner.
- The owner picked two clusters as their own: their normal voice and a
  quiet variant, about 19 dB lower (phone in a pocket or bag).
- An owner print must hold one centroid per recording condition. A
  loud-only print recognized 13% of the quiet voice.
- Adding the quiet centroid alone made another recurring voice clear the
  threshold 13% of the time. Requiring the owner to beat the voices they
  rejected by a margin of 0.1 fixed it: 0% for that voice, 95% / 91% for
  the owner's two conditions.
- In the blind check, every "owner" label was correct (6 of 6). Every error
  was the pipeline withholding the label, never asserting it.

**Runtime facts, all measured on a Dragon Q6A:**
- WeSpeaker ResNet34 compiles for the Hexagon v68 NPU through Qualcomm AI Hub
  with all 102 layers on the NPU.
  - At w8a16: 2.2 ms per 1.5 s window, EER 0.7% → 1.5%. int8 loses more and
    is ruled out.
  - 3D-Speaker CAM++ crashes the converter at its pooling step. Split there,
    it compiles, but the conversion is broken, the same at int8 and w8a16:
    - the backbone's frame maps match float at only 0.66–0.77 cosine;
    - the final embeddings match at 0.30;
    - EER goes from 2.2% to 12.2%.
- AI Hub now offers only QAIRT 2.45 and newer, and only through
  `precompiled_qnn_onnx`. That zip's `model.bin` is a raw context binary that
  `virtues-qnnd` loads unchanged.
  - **The 2.45 runtime loads the 2.42-built gte and ColBERT binaries**, with
    output matching the 2.42 runtime at cosine ≥ 0.9999.
  - **The 2.42 runtime refuses a 2.45 binary** ("newer context binary on old
    SDK").
  - So the box's runtime must move to 2.45 before any new NPU model ships.
- **tract runs pyannote segmentation-3.0, four LSTMs included,** after pinning
  the input to `[1, 1, 160000]`, stripping `value_info`, and constant-folding
  with onnxruntime at BASIC level. That removes a shape-only `If` whose
  symbolic dimension tract cannot parse.
  - It matches onnxruntime to 5e-6, at 158 ms per 10 s window on an A78 core.
  - Non-overlapping 10 s windows lose nothing measurable against a 2.5 s
    step, so it costs about 4.7 s of one core per 5-minute chunk.
- DiariZen and Sortformer, the end-to-end diarizers, are out: too slow on the
  CPU, and transformers the integer-only NPU cannot run accurately.

## The decision

The pipeline worked, and its errors fell on the safe side. It was still
paused. What it would change is narrator output, and that was never measured.
Building it means:
- enrollment,
- a stored biometric,
- stored prints of *other* people (needed for the margin, and a consent
  question of its own),
- a QAIRT bump,
- two new models,
- a Rust front-end,
- the turn-parts transcription path and its recall gap.

The one known harm, numbered tags implying the same people across chunks, is
removed by dropping the numbers. That shipped instead. The question to reopen
it: do day articles *still* invent who-said-what once the tags carry no false
continuity? If they do, the cheapest next step the data supports is to label
whole chunks that are nearly one voice. 22 of the 40 sampled chunks were at
least 90% or at most 7% owner, even with the loud-only print. That keeps the single-shot transcription and needs
no turn splitting.
