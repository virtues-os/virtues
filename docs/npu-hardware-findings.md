# On-Device NPU Findings — Dragon (Qualcomm) & Cubie (Allwinner)

*Field report from running Virtues' embedding/rerank stack on two edge NPUs.
Every number here was measured on real silicon (or its identical farm/CI
equivalent), not from spec sheets. Last updated 2026-07-09.*

## TL;DR

We evaluated two cheap edge boards as the production inference device for
"embed + rerank a person's whole digital life." The models under test:
**gte-small** (33M, 384-d embedder) and **answerai-colbert-small** / a
cross-encoder reranker.

| Board | NPU | Best embed latency (128 tok) | Fidelity (cosine vs fp32) | Encyclical* | Notes |
|---|---|---|---|---|---|
| **Radxa Dragon Q6A** | Qualcomm Hexagon v68 (12 TOPS) | **3.9 ms** | 0.99+ (w8a16) | **2.5 s** | Fast, but a brutal toolchain; ~$80 |
| **Radxa Cubie A7A** | Allwinner A733 / VeriSilicon VIP9000 (3 TOPS) | **61 ms** (int16) | 0.952 (int16) | **~30 s** | Easy toolchain, ~9× its own CPU; ~$40 |
| Cubie A7A **CPU** (2×A76) | — | ~550 ms | (fp) | 270 s | The floor we're trying to beat |

\*Encyclical = a ~62,215-token document (Pope's encyclical), full embed pass.

**Bottom line:** The Dragon is ~16× faster than the Cubie NPU but costs ~2×
and took weeks of op-level surgery to tame. The Cubie converts almost for
free and runs ~9× its own CPU, but tops out at 61 ms / 0.952 fidelity under
post-training quantization — missing our targets (< 50 ms, ≥ 0.96) on both
axes, though close on fidelity. Neither is a clean win; the decision is a
cost/latency/effort tradeoff documented below.

---

## Board 1 — Radxa Dragon Q6A (Qualcomm QCS6490, Hexagon v68)

### What worked
- **gte-small @ 128 tok: 3.9 ms/embed**, cosine fidelity **0.99+** with the
  right quantization (w8a16 — int8 weights, int16 activations).
- Reranker (answerai-colbert @ 256 tok, late interaction) at comparable
  speed; short-text retrieval **nDCG@10 0.962** on-device (98.8% of fp32).
- Whole encyclical embedded in **2.5 s**. Thermally stable (~41 °C over
  sustained runs, no throttle).

### What it cost
The Dragon's speed came from an *aggressive, fixed-function* NPU with a
brutal rulebook, and a toolchain (QAIRT/QNN) that was young and buggy at the
v68 generation:
1. **The calibration bug.** The local `qairt-quantizer` silently ignored
   calibration data — every activation ran on default ranges. Naive int8
   (w8a8) collapsed to **cosine 0.63**. The fix was an "encoding transplant":
   extract activation encodings from an AI-Hub w8a16 QDQ model and inject them
   as `--quantization_overrides`. This single discovery was weeks of work.
2. **The v68 op-legality table.** In a 16-bit MatMul only `in[1]` may drop to
   8-bit; Gather outputs must match their table; Split ties all branches;
   fixed-point Adds reject extreme scale ratios. Every "mysterious" build
   failure was one of these four rules.
3. **Fidelity thresholds.** Embedders usable ≥ 0.96, ideal ≥ 0.99; a
   cross-encoder's scalar relevance head needs **≥ 0.999** — one more nine
   than PTQ delivers for any 768-wide model (ModernBERT PTQ ceilinged at
   0.9745 → QAT-only). Late interaction (ColBERT) sidesteps the scalar head.

### Status
Retired as the beta device (superseded by newer plans). Its GPU (Adreno 643)
was tried as a llama.cpp Vulkan target but **hangs** (`vk::DeviceLostError`)
under the shader load. The full quantization recipe and thresholds remain the
methodological floor everything else is measured against.

---

## Board 2 — Radxa Cubie A7A (Allwinner A733, VeriSilicon VIP9000)

**The $40 question:** can a cheaper board be the production device? The A733's
"3 TOPS NPU" is a VeriSilicon Vivante **VIP9000** (hardware gen v3,
`cid=0x1000003b` = `VIP9000NANODI_PLUS`), driven by the **ACUITY** toolchain
(offline `pegasus import → quantize → export` → NBG blob → VIPLite runtime).

### The toolchain is *much* easier than Qualcomm's
This is the headline surprise. We are — per a verified literature sweep — the
**first to publicly run a transformer on this NPU family**; the entire public
model zoo is CNN vision (YOLO/ResNet/MobileNet). Yet:
- **The full BERT op set imports as first-class native ops.** ACUITY's IR
  recognized `layernormalize` (fused, not decomposed), `gelu` (Erf fused in),
  `softmax`, `matmul`, `fullconnect`, `gather` — not CPU-fallback
  decompositions.
- **The NBG export mapped the entire graph to NPU ops: Error(0), Warning(0).**
  VeriSilicon warns per CPU-bounced op; there were none. int64 mask
  arithmetic auto-converted to int32 cleanly.
- **No calibration bug, no op-legality surgery.** The two hardest chapters of
  the Dragon saga simply don't exist here. ACUITY's calibration works; its
  compiler is a forgiving general-purpose mapper.

*Why it's easier:* mature, decade-old IP shared across many SoCs (NXP,
Amlogic, Khadas); a general compiler (vs Qualcomm's aggressive
performance-first design); and a broad community. The flip side — see latency.

### Fidelity (cosine vs onnxruntime fp32, mean-pooled, 6 personal-data texts)

| Quantization | mean | min | verdict |
|---|---|---|---|
| int8 (per-channel) | 0.909 | 0.877 | below bar |
| int8 + KL calibration | 0.911 | 0.884 | no help |
| **int16 (dynamic fixed point)** | **0.952** | **0.933** | near (< 0.96) |
| bf16 / float16 | 0.952 | 0.933 | plateau |
| **hybrid** (144 int8 + 78 int16, entropy-promoted) | 0.910 | 0.879 | **no better than int8** |

Key finding: **mixed precision does not help.** Auto-promoting the
highest-entropy layers to int16 left fidelity at int8 levels — the accuracy
loss is *distributed* across the deep transformer, not concentrated in a few
sensitive layers. Full int16 is required; PTQ tops out at **0.952**. Beating
it needs quantization-aware training (QAT), which we don't have bandwidth for.

### Latency (measured on the real board, `vpm_run -l 100`)

- **gte-small int16: ~61 ms/embed** (steady-state 61–63 ms; 44.2M NPU cycles
  @ ~725 MHz; one-time create-network 35 ms + prepare 6 ms).
- Reference: resnet50 (a ~25M CNN) ran at **~8 ms** on the same NPU — the
  transformer is ~8× heavier per inference (attention + layernorm over 128
  tokens, plus int16 = 2× the compute of int8, plus graph overhead: the IR
  carries 194 reshapes + 123 permutes).
- Throughput: ~2,100 tok/s (fixed 128-token graph) → encyclical ≈ **30 s**.

### The scorecard

| | embed / 128 tok | tok/s | encyclical |
|---|---|---|---|
| A733 **CPU** (EmbeddingGemma, llama.cpp) | ~550 ms | 231 | 270 s |
| A733 **NPU** (gte-small int16) | **61 ms** | ~2,100 | ~30 s |
| Q6A **NPU** (gte-small w8a16) | 3.9 ms | ~33k | 2.5 s |

**A733 NPU is ~9× its own CPU** (the real win — CPU is a non-starter at
270 s/document, days for a whole life) but **~16× slower than the Dragon**,
and misses both targets: 61 ms > 50 ms budget, 0.952 < 0.96 fidelity.

### Board specs (all fine)
Allwinner A733 (2×A76 + 6×A55), 8 GB LPDDR5, 117 GB eMMC, Debian 11, kernel
5.15-a733, native gcc/cmake. NPU driver (`vipcore`) + `/dev/vipcore` present;
VIPLite v2.0 runtime from the `ZIFENG278/ai-sdk` GitHub repo (no proprietary
download needed for the runtime — the ACUITY *compiler* is a closed Docker
image from Allwinner netstorage).

---

## Verdict & recommendation

| | Dragon Q6A | Cubie A7A |
|---|---|---|
| Cost | ~$80 | ~$40 |
| Embed latency | 3.9 ms ✓ | 61 ms ✗ (target < 50) |
| Fidelity | 0.99+ ✓ | 0.952 ~ (target ≥ 0.96) |
| Toolchain effort | weeks (calibration bug, op rules) | ~a day (converts clean) |
| vs its own CPU | huge | ~9× |
| Query UX | instant | borderline |
| Bulk onboarding | trivial | usable (~8 min/1M tok) |

**The honest read:** the A733 is a legitimate *option*, not a slam dunk. It's
genuinely usable for bulk onboarding and ~9× the CPU, and the toolchain is
far friendlier than Qualcomm's — but at 61 ms / 0.952 it's borderline for
interactive query latency and just under our fidelity bar, with **no PTQ
headroom left** (int16 is the ceiling; mixed precision proved useless).

**Decision inputs:**
- If interactive query latency < 50 ms is non-negotiable → the Dragon's
  3.9 ms justifies its price; the A733 doesn't get there without QAT.
- If ~60 ms query + fast onboarding on a $40 board is acceptable → the A733
  ships, and the toolchain ease is a real ongoing cost saving.
- The one unexplored lever with real upside is **graph cleanup** (the 194
  reshapes / 123 permutes suggest layout inefficiency) — could shave the
  61 ms, but unproven and uncertain.

**What's settled regardless of board:** the retrieval architecture (128-token
chunks, small-to-big, hybrid dense+BM25 z-fusion, conditional rerank) and its
quality numbers transfer to any backend — inference is a swappable sidecar
(see `composability-plan.md`).

---

## Reproducibility notes

- ACUITY spike harness (import → quantize → fidelity → NBG export): run on
  x86 CI because the toolkit's TensorFlow build aborts on Apple-silicon
  (no AVX). Docker image `ubuntu-npu:v2.0.10.2`, `acuity-toolkit-whl-6.30.22`.
- Correct A733 export target: `--optimize VIP9000NANODI_PLUS_PID0X1000003B`.
- NBG arg gotchas: `--input-size-list "1,128#1,128#1,128"` (# between inputs,
  comma within), `--size-with-batch "True#True#True"` (per-input),
  `--target-ide-project linux64 --pack-nbg-unify --output-path ...`; token
  int64 inputs auto-convert to int32 (BERT vocab 30522 < int32).
- On-device run: `ZIFENG278/ai-sdk` → build `vpm_run` (`AI_SDK_PLATFORM=a733
  NPU_SW_VERSION=v2 make`) → `sample.txt` ([network]/[input] sections) →
  `LD_LIBRARY_PATH=.../viplite-tina/lib/aarch64-none-linux-gnu/v2.0
  vpm_run -s sample.txt -l 100`. NBG inputs are raw int32 `.dat`.
