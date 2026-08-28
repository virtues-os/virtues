# Inference Benchmark Spec — Jetson vs Qualcomm Q6A

> **Purpose.** A tight, self-contained plan to choose between two candidate
> appliance boards (NVIDIA **Jetson** vs Qualcomm **Q6A**) for Virtues' local-ML
> sidecars. Hand this to a fresh chat/project — all needed context is below.
>
> **Scope decision (locked):** No on-box LLM *generation* this round (revisit in
> a few years as edge chips improve). The big chat model stays in the cloud
> (Vercel AI Gateway). So the board only has to run two small encoder models —
> embedding + rerank — and the choice rides on just **two user-facing moments.**

---

## The two moments that matter (and nothing else)

1. **Live rerank, mid-chat.** Every semantic search the agent runs reranks ~30
   candidates *before* the cloud LLM answers — so rerank latency is additive to
   time-to-first-token. This is the only interactive local-inference path.
2. **A big foreground upload.** Embedding is otherwise invisible (init sync +
   background trickle nobody waits on). The one exception: a user drops a
   ~100-page PDF and waits for it to become searchable. A few seconds is fine and
   understandable; tens of seconds is not.

That's it. Indexing-at-rest, NPU, perf-per-watt sweeps, generative throughput —
**all out of scope** for this decision (see "Explicitly not testing" at the end).

The reframe that keeps this honest: local rerank only matters **relative to the
cloud LLM's time-to-first-token** (seconds). If rerank p95 is well under the
**~300–500ms perceptibility threshold**, the board is a non-factor for chat and
you choose on the PDF number + cost/heat. The benchmark exists to find out which
side of that line each board lands on.

---

## System under test

Two `llama-server` sidecars (llama.cpp, pinned tag **`b9606`**):

| Sidecar | Model / GGUF | Port | Server flags (production) |
|---|---|---|---|
| Embedding | `bge-m3-FP16.gguf` (1024-dim, FP16) | `:18181` | `--embedding --pooling cls -c 8192 -b 8192 -ub 8192` |
| Rerank | `bge-reranker-v2-m3-Q8_0.gguf` (cross-encoder, Q8_0) | `:18182` | `--rerank -c 8192 -b 8192 -ub 8192` |

Boards & the backends each can run **with this engine**:

| Board | CPU | GPU | Reachable backends |
|---|---|---|---|
| **Jetson** (Orin-class, CUDA **sm_87**) | aarch64 | CUDA | CPU, **CUDA** |
| **Q6A** (Dragonwing-class — *confirm exact SoC*) | aarch64 | Adreno (Vulkan) | CPU, **Vulkan** |

> llama.cpp can't use the Q6A's Hexagon **NPU** (needs QNN/ORT-QNN — a different
> runtime + a model port). Out of scope; not a column here.

---

## Prerequisite: confirm the GPU is actually on

`llama-server`/`llama-bench` default `-ngl` (`--n-gpu-layers`) to **0 = CPU**. A
CUDA/Vulkan build alone does nothing — you must pass `-ngl 99` (both models are
~560M params; all layers fit in VRAM). **The production systemd units currently
pass no `-ngl`** (`tools/virtues-installer/src/install.rs`), so the deployed
Jetson may be silently on CPU. For the bench, always set `-ngl 99` on GPU runs
and verify:

```bash
journalctl -u virtues-rerank | grep -i "offloaded\|CUDA\|Vulkan\|layer"   # want "offloaded N/N layers to GPU"
tegrastats   # (Jetson) GPU% should spike during a call
```

---

## ⚠️ The variable that dominates rerank latency: candidate length

The reranker is fed the **full, untruncated** document text of each of the ~30
candidates (`fetch_full_texts` in `virtues-core/src/search/query.rs` — preview is
only a fallback, **no length cap**). A cross-encoder runs one forward pass per
(query, doc) pair, and cost scales with doc tokens. So a batch of 30 short chat
turns and a batch of 30 full pages are *wildly* different latencies on the same
board.

Implications:
- **Pin doc length in the test** or the number is meaningless. Test at a
  realistic length, and run a short/long pair to see the slope.
- **Likely a production fix regardless of board:** cap rerank candidate text to
  ~512 tokens (standard for rerank pipelines). Worth flagging upstream — it
  bounds worst-case chat latency independent of which board wins.

---

## TEST A — Live rerank latency (the interactive number)

**What:** p50/p95 wall-clock to rerank 30 candidates for 1 query, against the
running `--rerank` sidecar (this is the real call path; `llama-bench` can't do it
— it doesn't run the cross-encoder's scoring head).

**Setup** — start the sidecar (add `-ngl 99` for GPU rows, drop it / use `0` for
CPU rows):
```bash
llama-server --rerank -m bge-reranker-v2-m3-Q8_0.gguf \
  --host 127.0.0.1 --port 18182 -c 8192 -b 8192 -ub 8192 -ngl 99
```

**Drive it** — 1 query + 30 docs, ~50 reps, report p50/p95:
```bash
# documents[] = 30 candidates at a PINNED length (see below)
curl -s localhost:18182/v1/rerank -H 'content-type: application/json' \
  -d '{"query":"...","documents":["d1", "...", "d30"],"top_n":30}' \
  -o /dev/null -w '%{time_total}\n'
```

**Run the matrix** (each cell = 50 reps → p50/p95):

| Board | Backend | 30× short (~80 tok) | 30× realistic (~250 tok) | 30× long (~800 tok) |
|---|---|---|---|---|
| Jetson | CUDA | | | |
| Jetson | CPU | | | |
| Q6A | Vulkan | | | |
| Q6A | CPU | | | |

**Read it against the threshold:** any cell **< ~300ms** = invisible in chat;
**> ~500ms** = a felt stall before grounded answers. The "realistic" column is
the headline; the short/long columns tell you how much a production length-cap
would help.

---

## TEST B — 100-page PDF embed (the one foreground burst)

**What:** wall-clock to embed a ~100-page document so it's searchable. A 100-page
PDF ≈ 50k words ≈ ~65k tokens; chunked into passages (~512 tok each → ~130
chunks) and embedded in batches. This is "user uploads a big PDF and waits."

**Setup** — start the embed sidecar (`-ngl 99` for GPU rows):
```bash
llama-server --embedding --pooling cls -m bge-m3-FP16.gguf \
  --host 127.0.0.1 --port 18181 -c 8192 -b 8192 -ub 8192 -ngl 99
```

**Drive it** — embed ~130 passages (one batched call or chunked calls), measure
total wall-clock + derive passages/sec and tokens/sec:
```bash
curl -s localhost:18181/v1/embeddings -H 'content-type: application/json' \
  -d '{"input":["passage 1","passage 2", "... ~130 passages ..."]}' \
  -o /dev/null -w 'total=%{time_total}s\n'
```

**Run the matrix:**

| Board | Backend | 100-page wall-clock | passages/sec | tokens/sec |
|---|---|---|---|---|
| Jetson | CUDA | | | |
| Jetson | CPU | | | |
| Q6A | Vulkan | | | |
| Q6A | CPU | | | |

**Read it:** the question is just "is the foreground wait tolerable?" If CPU does
a 100-page PDF in a handful of seconds, the embed backend barely matters and you
needn't chase the GPU for it. If CPU is tens of seconds and GPU is a few, that's
a real point for the GPU board (or for backgrounding the embed with a progress
indicator).

---

## Corpus (synthetic, deterministic)

Generate in-harness with a fixed seed so it's byte-identical on both boards.
- **Test A docs:** synthetic passages at the three pinned lengths (80 / 250 / 800
  tokens). Content variety matters less than length here — this is a latency
  test, not a quality test.
- **Test B doc:** one synthetic ~65k-token document (or a real 100-page PDF run
  through the same chunker). Reuse the identical file on both boards.

No quality/accuracy testing — model quality is identical on every board (same
GGUF), so it's irrelevant to the board choice. (MTEB/BEIR cover quality if ever
needed; out of scope here.)

---

## Reproducibility checklist

- [ ] llama.cpp tag **`b9606`**; identical GGUFs on both boards.
- [ ] Same synthetic corpus seed / same PDF file on both boards.
- [ ] Production server flags matched (`--pooling cls`, `-c/-b/-ub 8192`).
- [ ] `-ngl 99` on GPU rows; confirm `offloaded N/N` in the log. `-ngl 0` on CPU rows.
- [ ] Doc length pinned per Test-A column (don't let it drift).
- [ ] Run long enough to hit thermal steady state on passively-cooled boards
      (burst vs sustained diverge); note any throttling.

---

## Decision rubric

1. **Test A "realistic" p95** — does either board cross ~300–500ms? If both stay
   under, **rerank is a non-factor for chat** → don't let it drive the choice.
2. **Test B wall-clock** — is the 100-page foreground wait tolerable on CPU? If
   yes on both, the embed backend barely matters either.
3. If 1 and 2 both come back "fine on both boards" (likely), **choose on cost,
   heat, power, availability, and ecosystem — not these benchmarks.** The point
   of running them is to *earn the right* to say the inference perf is a wash.

---

## Explicitly NOT testing (and why)

| Skipped | Why |
|---|---|
| On-box LLM generation | Cloud-hosted this round; revisit in a few years. |
| Q6A Hexagon NPU | Needs a different runtime (QNN) + model port; llama.cpp can't reach it. |
| Indexing-at-rest / steady ingestion | Background, nobody waits on it. |
| Perf-per-watt sweeps, MLPerf, Geekbench | Over-instrumentation for a 2-model decision; revisit only if Tests A/B are a tie *and* power is contested. |
| Model quality (MTEB/BEIR) | Hardware-invariant — identical on both boards. |

---

### Appendix — repo facts (for the fresh-chat reader)

- Engine: `llama-server` from llama.cpp tag `b9606`, built per-arch in
  `.github/workflows/release-linux.yml` (CPU for x86_64 + generic aarch64;
  separate `GGML_CUDA=ON -DCMAKE_CUDA_ARCHITECTURES=87` build for Jetson).
- Sidecar `ExecStart`s carry **no `-ngl`** today → add `-ngl 99` before claiming
  GPU inference.
- Rerank input is **uncapped full document text** (`fetch_full_texts`,
  `virtues-core/src/search/query.rs`) — the dominant latency variable; consider a
  ~512-token cap.
- Search pipeline: `virtues-core/src/search/{embedder,reranker,query}.rs`;
  retriever over-fetches `max(30, 3×limit)` candidates before rerank.
- Models dir default `/var/lib/virtues/models` (`VIRTUES_MODELS_DIR` to override);
  sidecar URLs overridable via `VIRTUES_EMBED_URL` / `VIRTUES_RERANK_URL`.
- llama.cpp backend builds: CPU `-DGGML_NATIVE=ON`; Jetson `-DGGML_CUDA=ON
  -DCMAKE_CUDA_ARCHITECTURES=87`; Q6A Adreno `-DGGML_VULKAN=ON`. Target
  `llama-bench` for raw throughput, but Tests A/B above use the live sidecars.
