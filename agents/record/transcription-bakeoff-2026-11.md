# Transcription bake-off, November 2026

Measured on 2026-11-20 to find a cheaper way to transcribe the wearable mic.
The applet (`applets/transcription_resolution`) sends each VAD-passed 5-minute
chunk to Gemini 3 Flash and asks for a transcript plus title, summary, entities
and scene. Audio input is about three quarters of the cost of a call and the
JSON output the rest. At list prices a standard call costs about $0.005 per
chunk, which made transcription the largest AI cost for anyone who wears the
mic all day.

Two changes shipped from this work in `8af31e169`, described in the last
section. The rest is a comparison for a later decision.

## Method

72 real 5-minute chunks from one box, in two sets. The first set was
stratified by recording level and by what production had stored: quiet, mid
and loud chunks with speech, chunks where production stored only a scene, and
chunks the VAD skipped. The second set was all quiet or scene-only. The audio
and every transcript stayed on a development Mac and are not in the repo.

There were no human labels. Three measures stand in for them:

- **Cross-family agreement.** Every pair of outputs was aligned with `jiwer`
  after `whisper-normalizer`, and a model was compared only against models from
  other companies. P is the share of a model's words that other families also
  produced. R is the share of the other families' words it produced. This
  avoids the error from the August spike, where Gemini Flash was graded against
  Gemini Pro.
- **Words on no-speech chunks.** Any words a model emits on a chunk that every
  other model leaves empty are treated as invented.
- **Loops.** An output of 120 or more words with a distinct-word ratio under
  0.08, the same test as the applet's `is_degenerate`.

Each local model ran on three versions of each chunk: the raw audio, a
gain-normalized copy (`highpass=f=80,dynaudnorm=f=250:g=15:m=30:p=0.9`), and the
normalized copy with `afftdn` denoising.

## Results

| Model | Size | Runs on | Silent chunks | Loops | Agreement on mid-level speech (P / R) | Verdict |
|---|---|---|---|---|---|---|
| Granite Speech 5.0 TurboCTC (IBM) | 470M | CPU or GPU | 0-2 words | none | 0.65 / 0.55 | Best local model |
| Parakeet TDT 0.6B v2 (NVIDIA) | 600M | CPU or GPU | 0 words | none | 0.61 / 0.44 | Close second |
| Parakeet TDT 0.6B v3 (NVIDIA) | 600M | CPU or GPU | 0-2 words | none | 0.64 / 0.35 | Drops quiet speech |
| Qwen3-ASR 1.7B (Alibaba) | 1.7B | GPU | over 1,000 words | frequent | 0.55 / 0.74 | Out: loops on quiet audio |
| Nemotron 3.5 Transcribe (NVIDIA) | 30B, 4B active | Mac only | over 10,000 words | half of runs | 0.50 / 0.43 | Out: loops, minutes per loop |
| Cohere Transcribe 03-2026 | ~2B | GPU | n/a | n/a | n/a | Not evaluated |
| Gemini 3 Flash, standard | cloud | | occasional | rare | 0.63 / 0.50 | Incumbent |
| Gemini 3 Flash, flex tier | cloud | | rare | rare | 0.66 / 0.34-0.49 | Shipped |
| Gemini 3.5 Flash-Lite | cloud | | rare | rare | 0.65 / 0.35 | Out: dearer than flex, looped |

Agreement figures are scored against the other families, so they are relative.
Fresh Gemini runs agree with each other at about the same level as Granite
agrees with Gemini.

Cohere was not evaluated. The community MLX conversion
(`mlx-community/cohere-transcribe-03-2026-mlx-8bit`) produced mixed-language
fragments on every chunk, including silent ones, and the official weights are
gated behind a Hugging Face login.

### Granite in more detail

- Speed: 0.26 s per 5-minute chunk on an M5 Pro GPU and about 9 s on its CPU
  through MLX. Box CPU speed was not measured; building `transcribe.cpp` on the
  test box was not permitted in this session.
- Gain normalization roughly doubled what it recovered on the quietest chunks
  with speech. Denoising added nothing beyond normalization.
- CTC decoding cannot loop, and it never produced more than 2 words on a chunk
  every other model left empty.
- It cannot tell a television from people in the room. On a chunk Gemini tagged
  `background_media` and left empty, Granite transcribed the broadcast.
- It gives no speaker turns and no scene (sounds, music, setting).

### Granite as a pre-filter

Skipping the Gemini call whenever Granite returns fewer than 5 words would have
skipped 42 of the 72 test chunks. On the one skipped chunk where a Gemini run
reported speech, two other Gemini runs and Granite reported none, so that run
most likely invented it.

### Granite text plus a text model

Granite's normalized transcript was sent to a text model with a prompt asking
for title, summary, tags, entities and whether the text reads like background
media. Across the test chunks with 20 or more words:

| Text model | Mean cost per chunk | Reasoning tokens |
|---|---|---|
| DeepSeek V4.1 Flash, default | $0.00108 | yes |
| DeepSeek V4.1 Flash, `reasoning_effort: none` | $0.00030 | 0 |
| Gemini 3.1 Flash-Lite | $0.00060 | some |
| GPT-6 Luna | $0.00028 | many |

Titles named the same subject as Gemini's audio-based titles on most chunks,
and the text models flagged the broadcast chunk as background media. For
comparison, one Gemini 3 Flash flex call on a 5-minute chunk costs about
$0.0024.

## What shipped in 8af31e169

**The VAD speech count no longer reaches the prompt.** The ground-truth block
told Gemini "Measured speech in this clip: Xs, mostly quiet" whenever MarbleNet
measured under 2 s. MarbleNet detects that speech is present but undercounts it
by 1.5-14x (the reason the 2026-08-06 guard fix budgets against recording
length). On a chunk where four model families agree on a full conversation, the
hint made Gemini return an empty transcript in 3 of 3 runs; without it, or with
a 45 s figure, every run transcribed the conversation. Most of the loud
scene-only chunks in the second set were conversations stored as "Mostly
quiet". The audio is kept, so these rows can be requeued once a box runs the
fix: delete the empty transcript row and clear `transcribe_attempts` and
`transcribe_last_attempt` from the recording's metadata. A plain delete writes
no tombstone, so the applet may insert the new transcript.

**Transcription uses Google's flex tier.** The request carries
`providerOptions.google.serviceTier: "flex"`. A top-level `service_tier` field
was not reliably billed at the flex rate. An uncached 5-minute chunk cost
$0.00253 on flex against $0.00534 standard, and flex still works with the
zero-data-retention flag virtues-api adds. Flex calls took a median of 17 s
against 10 s, so `BATCH_SIZE` dropped from 30 to 10 to keep a sequential run
inside the runner's 300 s timeout.

## Open items

- Gemini looped to the 8,192-token cap on about 1 in 70 cloud calls, each
  costing 5-8x a normal call. A 5-minute chunk needs about 2,300 tokens at the
  applet's 30 characters per second guard, so a lower `max_tokens` would cap
  that cost.
- Box CPU speed for Granite, through `transcribe.cpp` and a Q8 GGUF.
- Whether to ship Granite as a pre-filter in front of Gemini or as the whole
  transcription path with a text model for structure. The second loses scene
  data and speaker turns and relies on the text model to catch background media.
