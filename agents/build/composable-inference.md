# Composable Inference

How the box gets embeddings and reranking, and how the search index defends
itself against a model it was not built with. Code:
`tools/virtues-installer/src/mode.rs` (the choice, validation, fingerprint),
`tools/virtues-installer/src/install.rs` (the env it writes),
`virtues-core/src/search/embedder.rs` (the boot guard, widths),
`virtues-core/src/database/mod.rs::ensure_embedding_dims`,
`virtues-core/src/cli/{configure_inference,reindex}.rs`.

## The architecture in one sentence

**Virtues consumes two HTTP contracts and defends its index.** An OpenAI-style
`/v1/embeddings` endpoint (required) and a `/v1/rerank` endpoint (optional —
search degrades to fusion ranking without one). Where they come from is decided
once, at install (`InferenceMode`), and there are three answers:

| Mode | Where | Serves | Chosen by |
|---|---|---|---|
| **Dragon** | our board (Q6A, Hexagon v68 NPU), detected from the device tree | `virtues-qnnd` on the NPU: gte-small (384-d, native) on `:18181`, answerai-colbert-small@256 MaxSim rerank on `:18182` | detection; zero questions |
| **Bundled** | any other machine, as a quick trial | the CPU `llama-server` sidecars we build and smoke-test in CI: EmbeddingGemma-300M (QAT Q8_0) on `:18181`, gte-reranker-modernbert on `:18182` | the interactive picker, or `VIRTUES_INFERENCE=bundled` |
| **Manual** | any other machine | whatever the user runs — llama.cpp, Ollama, LM Studio, a vendor NPU server | the picker: recipes, then URLs, a probe, and a pinned fingerprint |

There is deliberately **no managed GPU/NPU mode for arbitrary hardware**: we
never install or babysit inference software on machines we cannot test. Either
it is our board (and our daemon), or it is the CPU trial with zero hardware
variance, or the user owns the endpoint and we validate it at the door. The
bundled path is labelled "not for production" because it is slow on large
corpora; the daily-use path off our board is Manual.

`virtues-qnnd` speaks the same contract as the llama-server pair, so core has
no QNN-specific code path: Dragon gets the same probe and width guards as
every other endpoint. Changing the local models is
[model-migration.md](model-migration.md).

## The env contract

Written by the installer into the box env file; read by core.

| Variable | Dragon | Bundled | Manual |
|---|---|---|---|
| `VIRTUES_INFERENCE` | `dragon` | `bundled` | `manual` |
| `VIRTUES_EMBED_URL` | `http://127.0.0.1:18181` | same | user's |
| `VIRTUES_RERANK_URL` | `http://127.0.0.1:18182` | same | user's, if any |
| `VIRTUES_EMBED_MODEL` | — | — | model name sent in every request |
| `VIRTUES_EMBED_FINGERPRINT` | — | — | pinned at install |
| `VIRTUES_EMBED_DIMS` | — | `256` | probed native width |
| `VIRTUES_EMBED_QUERY_PROMPT` / `_DOC_PROMPT` | — | Gemma's | resolved by the installer |
| `VIRTUES_QNND_MODELS_DIR` | the QNN models dir | — | — |

- **Local-only.** Manual mode refuses a public embed/rerank endpoint
  (`mode.rs::ensure_local`: loopback, RFC1918, link-local, CGNAT `100.64/10`
  and IPv6 ULA pass). This *is* the "no cloud embedding APIs" rule.
  `VIRTUES_ALLOW_REMOTE_INFERENCE=1` is the logged override.
- **Validation runs before any system mutation**: dims probe, a p50 latency
  verdict, and the fingerprint.
- **Prompt prefixes are a property of the model**, so they are configured next
  to it and never defaulted: unset means no prefix, the only safe assumption
  about an unknown model. The installer resolves them by a ladder — explicit
  env → the model's own `config_sentence_transformers.json` → a small family
  table → none — and writes them quoted so trailing spaces survive.

## Widths

A width is a property of a model; nothing in the binary asserts one.

- **The index remembers its width** in `search_index_meta.dim`, recorded by the
  first embed. Bringup sizes the columns from that, never from a constant and
  never from the network (it runs on boxes whose embedder is down).
- **Stored width = the model's native width**, unless `VIRTUES_EMBED_DIMS` asks
  to truncate. Truncation is only safe for Matryoshka-trained models
  (EmbeddingGemma: 768 → 256); lopping dimensions off any other model destroys
  it, so it is opt-in per model.
- **The columns are `halfvec`.** pgvector's HNSW caps `halfvec` at 4000 dims
  (`MAX_INDEXED_DIM`), and plain `vector` at 2000. `search_vectors`,
  `search_topic_cache` and `app_projects.centroid` share one geometry and are
  resized together; `ensure_embedding_dims` checks all three, not just the
  first.
- **One model per index.** Two models' vectors live in different geometries;
  the cosine between them means nothing. `ensure_embedding_dims` refuses to
  change the width of a populated index — that is a re-embed.

## The boot guard and recovery

In Manual mode the embedder re-embeds two fixed probe strings at boot and
refuses to serve if their fingerprint differs from `VIRTUES_EMBED_FINGERPRINT`.
Mismatch is **a hard stop on writes plus a user-chosen recovery** — never
automatic, never silent.

| Command | When | What it does |
|---|---|---|
| `virtues configure-inference` | the model behind a Manual endpoint changed | re-probes (bypassing the guard), reports fingerprint/dims changes, and on confirmation wipes the derived index, re-pins fingerprint + dims, and resizes |
| `virtues reindex` | same model, index stale or a schema change needs a re-embed | wipes vectors **and** BM25, resizes, re-embeds; works in every mode |

Both wipe the **derived** index only — source rows are never touched, and all
copy frames embeddings as a cache. Not built: comparing stored probe *vectors*
by cosine, so a quantization change of the same model could keep its index
instead of re-embedding.

## Chunking and indexing

- **~128-token windows**: 96 words, 14-word overlap, 2048-char cap
  (`search/indexer.rs`). Short personal records are usually one window.
- **Drain mode.** The `embedding_index` applet loops full batches under a pg
  advisory lock with a 2-hour internal ceiling, and its manifest raises its
  subprocess timeout to `timeout_s = 7500` so the runner does not SIGKILL it
  mid-drain. First indexing of a large corpus takes hours, not weeks.

## Traps

- **Ollama routes by model name**: `"model": "default"` 404s. That is why
  `VIRTUES_EMBED_MODEL` exists and is threaded through picker → probes → env →
  every runtime request. Install-time and boot-time fingerprint requests must be
  **byte-identical**, or the guard compares different things.
- **The fingerprint has two copies**, installer `mode.rs` and core
  `embedder.rs` (the installer cannot depend on the core crate). Both quantize
  each component to `(x * 10000).round() as i32` LE bytes before SHA-256, for
  float-formatting stability. Change one, change both.
- **The applet runner's default subprocess timeout is 300s**
  (`applet_runner/limits.rs`); a long-running applet declares its own
  `[config.limits] timeout_s`. The concurrency gate (`has_active_run`) treats a
  `running` row older than 600s as dead, so for anything longer the applet's
  own advisory lock is what prevents a second concurrent run.
- **`curl | sh` cannot prompt on stdin**; cliclack reads `/dev/tty` directly.
  No TTY and no env override is a clear error, not a hang.
- **llama.cpp Vulkan on the Q6A's Adreno 643 GPU-hangs** (`vk::DeviceLost`).
  The Q6A's inference is the NPU via `virtues-qnnd`; do not route it back
  through the GPU.
- **EmbeddingGemma is faster on CPU than GPU** on the hardware we measured: its
  activations want bf16/fp32, so fp16 GPU paths force fp32. Pick backend per
  model × hardware by measurement.
