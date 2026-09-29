# Speaker attribution — who said it

**Status: PARKED 2026-09-29.** Transcripts now write `[Speaker]:` with no
number, and old numbered tags are flattened where narrators read them
(`virtues-core/src/transcript.rs`). Everything below is paused until day
articles show they still invent who-said-what without numbered tags. The
measured findings that outlive this plan (the sherpa-onnx front-end bug, the
QAIRT 2.45 compatibility, tract running segmentation-3.0) are recorded in
[`agents/record/speaker-attribution.md`](../record/speaker-attribution.md). If
reopened, start from section 12 and the chunk-level labeling suggested there.

## The problem

A transcript line is written as fact about a life, and the most common thing
it gets wrong is *whose* line it is. `transcription_resolution` asks Gemini
for `[Speaker 1]:` / `[Speaker 2]:` tags. Those tags are local to one 5-minute
chunk: `Speaker 1` in one chunk is whoever Gemini heard first, so across a
conversation the owner's lines and the other person's swap. In the 2026-09-24
day-article bake-off, speaker attribution was the largest single source of
invented claims (who wanted the bath, who ordered what, who sent the flowers).
The narrator cannot repair this, because the input does not say which voice
is the owner.

The fix is acoustic, not a prompt. The 2026-07-16 tests already showed Gemini
does no real voice matching: it keeps the same confidence when handed a
synthetic decoy voice as a reference.

## What the spike measured

Real audio from the box: 38 chunks over 19 days, including 4 consecutive
conversations.

**1. tract can now run a speaker model.** In the 08-27 spike, tract 0.21 could
not load ECAPA or TitaNet. The blocker was those two graphs, not tract.
WeSpeaker ResNet34, WeSpeaker CAM++ and 3D-Speaker CAM++ all load in
`tract-onnx 0.21.17` once the input is fixed at `[1, T, 80]`. They match
onnxruntime to about 1e-5. TitaNet still fails in type inference, and ERes2Net
loads but is slow.

**2. Cost on dragon, one A78 core** (`taskset -c 7`, scratch binary under
`/tmp`, removed afterwards):

| Model | 1.5 s window | 3 s window | load + optimize |
|---|---|---|---|
| 3D-Speaker / WeSpeaker CAM++ | 116 ms | 233 ms | ~1.0 s |
| WeSpeaker ResNet34 | 206 ms | 425 ms | ~0.1 s |

CAM++ costs about 0.08 s of CPU per second of audio. The box averages 114
speech chunks a day, and a speech chunk has about 200 s of voiced audio after
the energy gate. That comes to about 25 s per chunk, or about 50 min of one
core per day, spread across the two-minute cron.

**3. The embeddings separate voices and stay stable across chunks.** We
clustered 3,800 three-second windows from all 38 chunks together. Two voices
recur across the whole 19 days:

| Model | Voice A: chunks | Voice B: chunks | Both in one chunk | Centroid cosine |
|---|---|---|---|---|
| 3D-Speaker CAM++ | 29 / 38 | 23 / 38 | 15 | **0.22** |
| WeSpeaker ResNet34 | 28 / 38 | 24 / 38 | 17 | 0.39 |

3D-Speaker CAM++ separates the two voices more widely and holds its clusters
across distance thresholds 0.70–0.85, where ResNet34 merges them at 0.85.
~~Pick 3D-Speaker CAM++.~~ **Superseded by section 7.** These embeddings came
through sherpa-onnx's extractor, which feeds both models the wrong features, so
the comparison has to be rerun on this 38-chunk set with the corrected
front-end before either model is picked on box data.

**4. "The dominant voice is the owner" is false on this box.** The 08-27 spike
found one dominant voice *within* each of 4 single clips. Across days there
are two voices of comparable prevalence. Voice B makes up most of several
chunks and appears alone in some of them. Voice A is 5–13 dB louder in the
chunks where the two differ, so A is probably the wearer, but that is a prior,
not proof. **Without enrollment, nothing may be labeled "You".** A wrong label is
worse than no label.

**5. Gemini's timestamps cannot carry attribution.** We asked `gemini-3-flash`
for per-turn `{start, end, speaker, text}` on 5 conversation chunks. On 300 s
clips, the returned timelines end at 177–191 s, even where 39–45% of the loud
audio comes after that point. We embedded each returned segment and compared
it to the two voice centroids. Gemini's S1/S2 matched the voices with **52–72%
purity**, where 50% is chance. Stretching the timeline to the clip length did
not help. Any design of the form "Gemini transcribes, then we map lines to
times" is dead with this model.

**6. The inverse works: diarize on the box, then have Gemini transcribe the
turns.** We cut a chunk into voice turns on the box and sent them as ordered
audio parts in *one* request, each part preceded by a text tag `[T7]`. The
gateway accepted 39 and 42 parts. Gemini returned exactly one entry per part.
Word count reached **92% and 76%** of the single-shot transcript, and the
call was cheaper (\$0.0053–0.0061 vs \$0.009–0.010) because silence is not
sent. With this approach alignment holds by construction: the label on each
line comes from the part it was transcribed from. One short part came back at
13 words per second, which means words bled across a part boundary. Guard
against this; do not ignore it.

### Second spike: a labeled benchmark (2026-09-24, evening)

The box has no ground truth, so the second spike used the AMI meeting corpus,
whose lapel mics are the closest public proxy for a phone worn on the body.
Each participant's own lapel stands in for the owner's phone. Only lapels
where the wearer is at least 5 dB louder than everyone else were kept, which
matches the 5–13 dB gap seen on the box. That left 15 lapels, 7 distinct
owners, 3 sites and 92 five-minute chunks. Enrollment always comes from a
*different* meeting of the same person, never the one being scored.

**7. sherpa-onnx computes the wrong features for both speaker models.** On
1.5 s windows, owner against everyone else (408 trials, 7 owners):

| Model | Via sherpa-onnx's extractor | Model's own front-end |
|---|---|---|
| WeSpeaker ResNet34 | EER 15.2% | **EER 0.7%** |
| WeSpeaker CAM++ | 21.8% | 2.9% |
| 3D-Speaker CAM++ | 12.2% | 2.2% |

The front-end that works is Kaldi fbank with **per-window mean subtraction**
and the full band (`high_freq = 0`, meaning Nyquist). sherpa skips the mean
subtraction for the WeSpeaker models, and for 3D-Speaker CAM++ its output
matches none of the settings tried (cosine 0.24–0.41 to every variant). Other
knobs barely matter once the mean is subtracted:

- `snip_edges` changes 3D-Speaker CAM++'s EER by less than 0.1 pt.
- Input scale cancels out entirely.
- `high_freq = -400` costs 0.7–1.5 pt.
- Dropping mean subtraction costs 5 pt on CAM++ and 14 pt on ResNet34.

End to end, the section-3 window design with 3D-Speaker CAM++ scores turn F1
**0.58** with sherpa's features and **0.79** with its own. That score uses a
threshold chosen on the other two sites (frame precision 0.72 → 0.89).
ResNet34 with its corrected front-end lands at the same place, 0.78. Sections
10–11 explain why every embedder converges there.

**8. ResNet34 runs entirely on the dragon's NPU.** It was compiled through
Qualcomm AI Hub for the QCS6490 and profiled on a real device. All 102 of
102 layers ran on the NPU. The 548 benchmark windows were run on the device
and scored against float:

| | Per 1.5 s window | Cosine to float | EER (sherpa features) |
|---|---|---|---|
| CPU float, one A78 | 219 ms | 1 | 15.2% |
| NPU w8a16 | **2.2 ms** | 0.978 mean, 0.916 min | 15.9% |
| NPU int8 | 1.4 ms | 0.952 mean, 0.792 min | 17.2% |

w8a16 costs little, and int8 costs enough to rule it out. Recompiled with
the corrected, mean-subtracted features and their own calibration, the
**NPU w8a16 model scores EER 1.5% against 0.7% in float**, with mean cosine
0.989. On the spare dragon it matches float at 0.983–0.995 over real windows,
in 3.0 ms per call through `virtues-qnnd` (phase 1b).

3D-Speaker CAM++ crashes Qualcomm's converter inside
`keep_graph_output_order` at its final pooling step, a tooling bug rather than
a missing op. It compiles once the graph is split at the last ReLU (a
`[1, 512, 74]` frame map): the backbone runs on the NPU, and statistics
pooling plus the dense layer run on the CPU. That split reproduces the
full model exactly in float, but **on the device it is broken**: all 1,406
layers run on the NPU at 4.6 ms, yet the output matches float at a cosine of
only 0.66–0.77. EER goes 2.2% → 12.2%, the same at int8 and w8a16. The
layout checks out, so this is the conversion, not precision. CAM++ is
CPU-only.

**9. Cost on the dragon's CPU, measured with onnxruntime:**

| Model | One A78 core | Four threads |
|---|---|---|
| pyannote segmentation-3.0, 10 s window | 102 ms | 46 ms |
| 3D-Speaker CAM++, 1.5 s window | 67 ms | 26 ms |
| WeSpeaker ResNet34, 1.5 s window | 219 ms | 60 ms |

End-to-end diarizers are out. DiariZen (WavLM plus a Conformer) ran slower
than real time on one Mac core, and it and Sortformer are transformers that
the v68 NPU can only take as integers, the arrangement that lost accuracy in
the OCR spike.

**10. Speech detection, not the voice model, sets the frame ceiling.** All
sliding-window systems, whatever the embedder or clustering, plateau at the
same frame F1 (~0.70). Given *perfect* speech boundaries from the annotation,
the same ResNet34 windows reach **0.90**. Detectors on all 92 chunks (frame
precision / recall of anyone speaking, and recall of the owner's frames):

| Speech detector | P | R | Owner recall | ResNet34 windows: frame F1 / turn F1 |
|---|---|---|---|---|
| RMS energy gate | 0.97 | 0.45 | 0.67 | 0.70 / 0.78 |
| MarbleNet, the box's p ≥ 0.65 | 0.97 | 0.33 | 0.57 | 0.68 / 0.74 |
| MarbleNet, p ≥ 0.30 | 0.89 | 0.66 | 0.87 | 0.84 / 0.77 |
| **pyannote segmentation-3.0, p ≥ 0.5** | **0.97** | **0.96** | **0.98** | **0.88 / 0.79** |
| Perfect boundaries (the ceiling) | 1 | 1 | 1 | 0.90 / 0.80 |
| Full pyannote 3.1 pipeline, for comparison | | | | 0.88 / 0.79, but 3.1× real time on one Mac core |

MarbleNet here is reproduced from `vad.rs` exactly, including its assets. It
confirms what that file already says: MarbleNet is a *presence* detector that
fires on onsets. It is right for "skip Gemini on a silent chunk" and wrong for
boundaries. Segmentation-3.0 plus ResNet34 windows matches the full pyannote
pipeline at about 1/200th of the CPU.

**11. Overlap sets the turn ceiling, and it is crosstalk, not confusion.**
Turn F1 stays at ~0.80 even with perfect boundaries.
- 96% of false "You" turns are another person's turn that the owner talks over
  for more than 30% of it.
- 83% of missed owner turns are overlapped by someone else.

Segmentation-3.0 also outputs overlap, as its two-speaker powerset classes.
At p ≥ 0.2 it finds overlap at 69% precision and 71% recall.

Marking overlapped stretches **unsure** instead of forcing a label changes
the picture. At the frame level, with no gold turns, as production would run
(pyannote speech mask, 0.5 s median filter, You runs under 1 s dropped, τ
chosen on the other sites):

| Rule | "You" while owner silent | …more than 0.75 s from any owner speech | "You" during crosstalk | Owner frames labeled You |
|---|---|---|---|---|
| No overlap gate | 7.7% | 2.6% | 20% | 83% |
| Unsure where p_overlap ≥ 0.3 | 5.2% | 1.4% | 9.5% | 70% |
| Unsure where p_overlap ≥ 0.2 | 4.7% | 1.3% | 7.8% | 67% |
| Unsure where p_overlap ≥ 0.1 | 3.3% | **0.8%** | 6.0% | 63% |

Most "owner silent" frames sit within 0.75 s of the owner's own speech. That
is the 1.5 s window smearing across a boundary, and it matters little for a
transcript line. The rest were checked by hand on turns. Under the strictest
rule, every one of the 20 remaining false "You" turns had the owner annotated
speaking through **at least 30%** of it (median about 90%). **None** was a
turn where the owner was silent.

A second abstain signal, needing no extra model, is the share of a turn's
windows above τ, which is mixed under crosstalk. It adds about 8 pt of turn
precision on its own, and it stacks with the overlap gate (0.76 → 0.94).

AMI meetings overlap far more than pocket audio (16% of speech here), so
these rates are a pessimistic bound for the box.

**12. Real audio from the box, with the owner listening (2026-09-27).** 40
random 5-minute chunks from the last 30 days: 25 that Gemini called
multi-speaker, 15 single-speaker. Corrected ResNet34 windows, pyannote mask.
- **Two voices recur across the month**, in 25 and 28 of 40 chunks, at a
  centroid cosine of 0.29. Beyond them is a long tail of one-off voices.
- **Loudness does not identify the owner.** Both voices have the same median
  level. Within the 8 chunks holding both, one is louder in 6, by a median of
  5.5 dB. That is a prior, not an answer.
- **Unsure is 8% of speech,** half of AMI's crosstalk, as expected.
- **The owner picked their voice from clips, and picked two clusters.** One is
  the loud voice (−22 dB). The other is a **quiet variant** of the same
  voice (−41 dB, phone in a pocket or bag), at cosine 0.48 to the loud one and
  seen in 10 of 40 chunks, 9 of them alongside the loud one.
  - "Which voice is yours?" works as designed.
  - The **multi-centroid print is required, not an optimization**: a
    loud-only print labels just 13% of the quiet voice as the owner.
- **Blind spot check of 15 pipeline turns, owner listening:**
  - "You": 6 of 6 correct.
  - "other": 4 of 5 correct; the fifth was the owner, quiet.
  - "unsure": 2 of 3 were the owner alone.
  - Every error was the pipeline withholding "You", never asserting it.
- **Adding the quiet centroid alone is unsafe.** Then 13% of the other
  recurring voice clears τ. The margin rule fixes it: the owner's best
  centroid must beat every *known* other voice by 0.1, the voices the owner
  said "not me" to in the same question.

  | Owner print | Other recurring voice → You | Owner loud → You | Owner quiet → You |
  |---|---|---|---|
  | loud centroid only | 0.7% | 98% | 13% |
  | loud + quiet | 13% | 99% | 96% |
  | **loud + quiet, beat known others by 0.1** | **0%** | **95%** | **91%** |

  One-off voices, which no margin can cover, clear τ in 8–14% of windows.
  Some of those are the owner in rare conditions. Measuring the rest takes a
  second listening round on that class specifically.

## Design

### Runtime

Everything runs inside `transcription_resolution` and reuses the VAD's own
path: symphonia decode, a Rust fbank front-end, then tract.

- **Model: WeSpeaker ResNet34 (VoxCeleb), provisionally.** It has a 256-d
  output and input fixed at `T = 148` (1.5 s with `snip_edges`). It has the
  best EER measured (0.7%, section 7) and runs fully on the NPU (section 8).
  3D-Speaker CAM++ (2.2%) is the fallback. The pick is confirmed only after
  both are rerun on the box's 38-chunk set with the corrected front-end.
- **Where it runs.**
  - **NPU first, through `virtues-qnnd`**, as a third model index beside gte
    and ColBERT, at **w8a16 only** (int8 loses too much, section 8). The daemon
    already exists, and 2.2 ms per window makes scoring every window
    effectively free.
  - **CPU fallback in pure Rust via tract**, the same graph at float, for
    boards without the NPU. It loads in tract (section 1) and costs about
    210 ms per window, twice CAM++. That doubles the CPU cost below, which is
    one more reason to prefer the NPU.
  - Either way there is no ONNX Runtime, so the existing rule holds.
  - The ONNX file is 25 MB, too big for `include_bytes!`. Fetch it with a
    pinned SHA in the release build, as the QAIRT libraries are fetched, and
    ship it next to `applets-bin/`.
- **Front-end: this is where the first spike went wrong, so it is pinned
  exactly.**
  - Kaldi fbank: 80 bins, 25/10 ms, povey window, pre-emphasis 0.97, DC
    removal, dither 0, `low_freq = 20`, **`high_freq = 0` (Nyquist)**,
    `snip_edges = true`.
  - Then **subtract the per-window mean of each bin. This is mandatory:**
    leaving it out costs 5–14 pt of EER (section 7).
  - Input scale does not matter after mean subtraction.
  - This is not the NeMo mel that `vad.rs` computes, but the framing, STFT
    (`realfft`) and bundled-filterbank method are the same. Export the Kaldi
    filterbank as a blob, as `mel_fb.f32` was exported. About 120 LOC.
  - **The parity reference is `kaldi-native-fbank` with exactly these options.
    It is never sherpa-onnx's `SpeakerEmbeddingExtractor`.** sherpa has no
    mean subtraction for WeSpeaker and a mismatched front-end for 3D-Speaker,
    so matching it would ship the 15% EER model.
- **Speech and overlap: pyannote segmentation-3.0** (MIT, 1.5M params, the
  sherpa-onnx export), **run by tract in pure Rust.**
  - It gives p_speech = 1 − p(nobody) and p_overlap = the sum of the
    two-speaker classes (section 10).
  - **Measured on the spare dragon on 2026-09-27:**
    - tract 0.21.17 runs it, all four LSTMs included, once the export is
      prepared: pin the input to `[1, 1, 160000]`, strip `value_info`, and
      constant-fold with onnxruntime's BASIC level. That drops a shape-only
      `If` node whose symbolic TDim string tract cannot parse.
    - The prepared graph is 40 nodes, and it matches onnxruntime to 5e-6 in
      probability with identical frame argmax.
    - One 10 s window takes 158 ms on the A78 prime core (onnxruntime: 102
      ms), 173 ms on another A78, and 757 ms on an A55. Pin it to the A78s.
  - **Step 10 s, meaning non-overlapping windows.** On 31 AMI chunks, a
    10 s step loses nothing measurable against 2.5 s: speech recall 0.952 vs
    0.956, owner recall 0.976 vs 0.979, overlap unchanged. That is 30 windows
    per chunk, about **4.7 s of one A78 per chunk**.
  - Ship the prepared ONNX (5.9 MB) with a pinned SHA like the embedder,
    along with the script that prepares it, since the graph fix is not in
    upstream's file. No sidecar, and no ONNX Runtime at runtime.
  - MarbleNet stays where it is, as the presence gate that skips Gemini on
    silent chunks. It is not used for boundaries (section 10).
- **Labeling:**
  - 1.5 s windows at a 0.75 s hop, only where p_speech ≥ 0.5. Score each
    window against every known voiceprint by cosine.
  - Every frame gets one of three labels:
    - **unsure** where p_overlap ≥ 0.2;
    - **You** where the owner cosine clears τ;
    - otherwise **other**.
  - Apply a 0.5 s median filter, drop You runs under 1 s, and merge the rest
    into turns.
  - A turn is **You** only when at least 80% of its windows clear τ, it is
    not mostly unsure, *and* the owner print beats every other print by a
    margin.
  - A turn that is mostly unsure is written with no owner claim (see Open
    decisions for the label).
  - Every remaining turn is an unknown voice, clustered within the chunk.
- **What this buys on AMI (section 11):**
  - A "You" line far from any owner speech is under 1.5% of You frames.
  - Every false-You turn checked had the owner really talking.
  - The owner's own speech is labeled You about two-thirds of the time. The
    rest is unsure or crosstalk, and that is the price of never guessing.

### Transcription paths

| Chunk | Path | Labels |
|---|---|---|
| No owner voiceprint yet | Today's single-shot call, unchanged | `[Speaker N]` as today |
| One voice | Single-shot call (best recall) | Every line becomes `[You]` or `[Speaker N]` from that one voice |
| Two or more voices | **Turn-parts call** (one request, N ordered parts) | From the diarization turn, per part |

Of the box's 114 speech chunks a day, 78 have more than one voice, so the
turn-parts path is the main path. The turn-parts prompt is the production
`SYSTEM_PROMPT` with `text` replaced by `parts: [{id, text}]`. Title,
summary, entities and scene keep the same schema and are not dropped.

### How labels are written

- `text` is rewritten in place: `[You]: …` / `[Speaker 2]: …`, with
  consecutive parts from the same voice merged. Every reader already reads
  `text`, including the day narrator, `dayline::context`, search and
  `sessionize`, so no reader changes.
- The turn structure goes in `metadata.turns: [{id, start, end, voice,
  score, text}]`. This also gives transcripts real timing for the first time,
  which citations and playback can use later.
- **Stable across chunks.** The owner is stable by construction. For other
  voices, keep a rolling cache of the last 30 minutes of voice centroids from
  earlier chunks' `metadata.turns`. A new unknown voice that matches one of
  those centroids takes that voice's label, so `[Speaker 2]` means the same
  person for a whole conversation.
- **Named voices (phase 3).** A voice that recurs across days can be named
  once and is written with that name from then on.
- The narrator's instructions change by one line: `[You]` is the owner,
  identified by voice, and other labels are stable within a conversation but
  are not identities.

### Enrollment

This design stores a biometric. It stays on the box, is never sent to Gemini
or the relay, can be deleted from Settings, and deleting it re-labels nothing
that has already been written.

1. **Primary: "Which voice is yours?"** The box already clusters its own audio
   (step 3 above). After a few days it holds 2–3 recurring voices. The app
   plays three 4-second clips of each and the owner taps the one that is
   theirs. These clips are recorded in the conditions the model will see
   (pocket, table, car), which a read-aloud sample is not. The owner can name
   the other voices in the same question. This is an identity question in the
   shape of [narrative-resolution-plan.md](narrative-resolution-plan.md).
2. **Fallback: read aloud.** Settings › Microphone (or Setup's optional
   interview step) records 30 s of the owner reading a passage on the same
   phone mic, for a box that has no history yet.
3. **Several centroids from day one, and the "not me" answers are kept.**
   The owner's print holds one centroid per condition (up to 5), because
   pocket audio is a different channel. On the box that was a separate cluster
   at −41 dB against −22 dB (section 12). Ask about every recurring voice,
   including a quiet one that sounds like the owner muffled.
   - The voices the owner rejects are stored as **known others**, and "You"
     requires beating each of them by a margin (0.1 measured).
   - Without that margin, the quiet centroid pulls in 13% of another
     recurring voice.
   - Later, turns scored far above threshold can refine the centroids.

Until a voiceprint exists, output is exactly what it is today.

### Storage

A migration (claim its number first) adds `app_voiceprint (id, label,
is_owner, model, centroids real[][], source, created_at, updated_at)`.
`centroids` holds 1–5 embeddings: 256 floats for ResNet34, 512 for CAM++.
`model` names the embedder and its front-end, because a print is meaningless
to any other model. A model change means re-enrolling from stored clips, not
converting prints. Prints from the NPU and CPU paths of the same model are
compatible: they differ by a cosine of about 0.98. The table is not indexed for search and is never
exported to a model.

## Cost

- **NPU (dragon):** ResNet34 at 2.2 ms per window (3.0 ms including the
  daemon round trip), about 0.6 s per chunk, well under a minute a day.
- **Segmentation-3.0 on the CPU (tract):** about 4.7 s of one A78 per
  5-minute chunk at a 10 s step, about 9 min a day at 114 chunks.
- **CPU fallback:** about 0.14 s of one A78 per second of audio with ResNet34,
  so about 45 s per multi-voice chunk and about 1.5 h of one core a day. With
  CAM++ it would be about 25 s per chunk and 50 min a day. The model is kept
  loaded across the drain, the way `Vad` is.
- **Gemini:** the turn-parts call is *cheaper* than single-shot (0.53–0.61¢ vs
  0.9–1.0¢ in the spike), since silence between turns is not sent.
- **Backfill (optional):** 4,485 multi-speaker chunks with audio on disk.
  About \$25 of Gemini, and about 31 h of one core spread across nights. Offer
  it after enrollment. Do not run it automatically.

## Gates before it ships

1. **Fbank parity and a guard against the wrong front-end.** The Rust
   front-end matches `kaldi-native-fbank` with the pinned options. A committed
   test holds the EER on a fixed labeled window set. The set cannot be
   box audio (voiceprints never leave the box), so it comes from AMI or
   VoxCeleb. The test fails if EER drifts above its baseline, so a silent
   front-end change like section 7's breaks CI instead of shipping.
2. **Quantization parity (NPU path).** On the same set, NPU embeddings keep a
   mean cosine of at least 0.97 to float and at most 1 pt of EER loss.
   Check it again whenever the QAIRT version or the calibration set changes.
3. **Recall.** On 30 multi-speaker chunks, turn-parts mode reaches ≥ 95% of
   the single-shot word count. The spike measured 76–92%, so closing that gap
   is real work. Levers: pad parts to the nearest low-energy frame instead of
   ±0.25 s, keep "unknown voice" turns, and merge turns under 2 s. If the gate
   fails, stop and do not ship lower recall.
4. **Bleed guard.** Hold each part to a characters-per-second budget, as the
   existing hallucination guard does for the whole chunk. A part over budget
   gets its text set to `""` and is flagged in `metadata`.
5. **Attribution accuracy on labeled turns.** The owner labels about 50 turns
   through the enrollment UI, from at least 3 conditions.
   - Set the "You" threshold so that **no turn where the owner is silent is
     labeled You.** Count that the way section 11 does: You frames more than
     0.75 s from any owner speech stay under 1%.
   - Let recall fall where it falls.
   - Report crosstalk separately, as turns labeled You where the owner and
     someone else both talk. On AMI, turn precision cannot reach 98% for a
     reason unrelated to confusing voices: the "errors" are the owner really
     talking over someone.
   - The AMI harness, which computes both numbers, is the regression check.
6. **Shadow week on the box.** Run the new path alongside the old one, write
   `metadata.shadow_turns` only, then compare.

## Phases

0. **Close the model pick.**
   - Rerun section 3 on the box's 38 chunks with the corrected front-end, for
     ResNet34 and 3D-Speaker CAM++.
   - On AMI:
     - ResNet34 end to end: done, frame F1 0.88 with the pyannote mask.
     - ResNet34 on the NPU: done, EER 1.5%.
     - The CAM++ backbone on the NPU: broken (cosine 0.66–0.77) and dropped.
   - Segmentation-3.0 under tract: done (see Runtime).
   - Still open: measure the full pipeline's harm rate with NPU scores rather
     than float.
1. `speaker.rs`: the fbank front-end, the embedder via tract, windows to
   turns, and gates 1–2. Includes the migration for `app_voiceprint`.
1b. NPU path: add ResNet34 to `virtues-qnnd` as a third model and deliver it
   the way gte and ColBERT are. Measured on the spare dragon on 2026-09-25:
   - **Getting the binary:** AI Hub no longer emits `qnn_context_binary`.
     Compile with `--target_runtime precompiled_qnn_onnx`, and the output zip
     holds `model.bin`. That file is the raw QNN context binary and loads in
     `virtues-qnnd` unchanged. The wrapper ONNX's Q/DQ nodes carry the
     input/output scale and offset, which the daemon also reads from the
     binary.
   - **Input and output types:** input is uint16 (scale 3.5e-4, zero point
     34381 for mean-subtracted fbank), and output is uint16 that the daemon
     dequantizes to fp32. The Rust client quantizes the fbank itself.
   - **QAIRT has to move to 2.45.** AI Hub now offers only 2.45, 2.49 and
     2.50. The 2.42 runtime refuses the 2.45 binary with "Using newer context
     binary on old SDK" (0x1388).
   - **2.45 is safe for the existing models:**
     - The 2.45 runtime (`2.45.0.260326`, the same public zip URL pattern)
       loads the **2.42-built gte and ColBERT binaries unchanged**.
     - Their outputs match the live 2.42 service at cosine 0.99997 and
       0.99992.
     - ResNet34 on the dragon's NPU matches float at cosine 0.983–0.995 over 5
       real windows, the same as on AI Hub's devices.
     - It takes 3.0 ms per call including the TCP round trip, without
       `--burst`.
   - **The bump is therefore:**
     - `QAIRT_VERSION` 2.42 → 2.45.0.260326 in `qairt.rs` and
       `release-linux.yml`, with new SHAs for the five libraries.
     - The v68 Skel changes too: 9,459,724 bytes against 9,040,240.
     - gte and ColBERT binaries do not need recompiling.
     - It is backward-compatible, not forward: a box still on 2.42 must never
       receive a 2.45 binary. So the libraries ship before, or with, the new
       model.
2. Enrollment: the clustering job, the "Which voice is yours?" question and
   its clip endpoint (clips are cut from the lake on request and never
   stored), and the read-aloud fallback.
3. The turn-parts transcription path with gates 3–5, then the shadow week.
4. Named voices, the cross-chunk centroid cache, and the backfill offer.

## Open decisions

- Model delivery: a SHA-pinned fetch in CI (recommended), Git LFS, or a
  download on first use. The NPU context binary follows the gte and ColBERT
  path through `publish-qnn-models.sh`.
- ResNet34 or CAM++: settled by phase 0, not by this document.
- License check before shipping. The 3D-Speaker repository is Apache-2.0 and
  the model card should say the same; WeSpeaker is Apache-2.0 too. VoxCeleb
  training data is CC BY 4.0.
  Record both in `assets/NOTICE`.
- How to label a turn that fails the threshold: `[Speaker N]` (recommended;
  it never claims to be the owner) or a distinct `[Unclear]`.
- How to write an **overlapped** turn, which is a third of the owner's speech
  on AMI: `[You and Speaker 2]`, `[Unclear]`, or the part sent as is and
  labeled `[Speaker N]`. Whichever wins, it must never assert `[You]` alone.
- Whether TV and podcast voices, which `scene` already flags as
  `background_media`, should get a label of their own rather than
  `[Speaker N]`.
