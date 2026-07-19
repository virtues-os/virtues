# Composable Inference — Plan of Record

*Handoff from the 2026-07 design/R&D sessions. This is the single source of
truth for the inference-composability work: what was decided, what already
landed, what's next, and the traps already discovered. Supersedes anything
that contradicts it.*

## The one-sentence architecture

**Virtues consumes two HTTP contracts and defends its index.** An OpenAI-style
`/v1/embeddings` endpoint (required) and a `/v1/rerank` endpoint (optional —
search degrades gracefully to fusion ranking without one). Where those
endpoints come from is decided once, at install, and there are exactly two
answers:

- **Dragon** — our hardware, detected from the device tree, sidecars
  provisioned locally by the installer. Zero questions asked.
- **Manual** — any other machine. The user runs the endpoints (Ollama,
  llama.cpp, LM Studio, a vendor NPU server, a cloud API), the installer
  shows recipes, asks for URLs, probes, and pins a model fingerprint.

There is deliberately **no managed mode** for arbitrary hardware: we never
install or babysit inference software on machines we can't test. Either it's
our board, or the user owns the endpoint and we validate it at the door.
This is also how NPU composability works at all — llama.cpp supports
essentially no NPUs (its Hexagon backend is v73+/Android-only), so vendor
NPU stacks plug in behind Manual mode rather than being ported by us.

## Locked decisions (with the reasoning that locked them)

| decision | rationale |
|---|---|
| Models: **EmbeddingGemma-300M-QAT** (embed) + **gte-reranker-modernbert** (rerank) | QAT survives int4/int8 without calibration work → drops into Q4_0 (llama.cpp) or w4a16 (QAIRT) whenever an NPU door opens; already wired in the repo; multilingual; MRL dims. The reranker is a cross-encoder, natively servable by llama.cpp `/v1/rerank`. |
| Chunking: **128-token-target windows** (96 words, 15% overlap, 2048-char cap) | 2025-26 retrieval research converges on 64–128 tokens for short factual content; model-agnostic; greenfield (nothing launched → no migration). |
| Beta hardware: **Radxa Dragon Q8B** (10 units, ships ~Jul 31) + a few manual-mode testers | Q8B acceleration = **Adreno 690 GPU via llama.cpp Vulkan or OpenCL** (the a690 is the ThinkPad X13s GPU — mature turnip support). The Q8B NPU is a day-one timeboxed experiment, never a dependency (8cx Gen 3's DSP stack never shipped for Linux). |
| **Q6A retired; Jetson = dev box, through Manual mode** | The Jetson runs its own CUDA llama-server and goes through the exact BYO flow — every day of dev work dogfoods the customer path. All Jetson special-casing deleted from installer/upgrade. |
| Manual mode is **recipes only** (no GPU/driver babysitting) — but a **bundled CPU "quick trial"** is allowed | We never install *GPU/NPU* inference for the user (that re-inherits the driver-support business the split exists to escape). But the **CPU llama-server we already build + smoke-test in CI** has zero hardware variance, so offering it as a zero-setup, honestly-labeled *"quick trial (CPU, not for production)"* is doctrine-safe. GPU/scale/other-model all stay behind Manual/BYO. Rationale: EmbeddingGemma is actually *faster on CPU* than GPU on the Orin (fp32 activations), so CPU-bundled isn't a big compromise for the embed path. |
| Fingerprint mismatch → **hard-stop writes + user-choice recovery** | Never automatic, never silent. See `configure-inference` below. |
| v1 ships **without disk encryption**, but user data lives on **its own partition** | Matches the entire consumer market; the partition hedge makes LUKS a later migration instead of a nightmare. (Design for later: LUKS2 + vendor-hosted Tang over the relay; QCS-class boards have no TPM.) |
| No fleet machinery for beta | 10 reachable boxes: heartbeat timer + ssh, not A/B OS images, not observability stacks. The `upgrade.rs` self-updater gets auto-rollback + health probe + signing instead. |
| Next-board criterion: **Hexagon v73+** | v73+ gets mainline llama.cpp NPU (Q4_0) for free. (e.g. Dragonwing IQ-9075 class.) |
| Canonical install URL: **`virtues.com/sh`** | `/sh-pre` = edge channel. |

## Already landed (staging @ `d250ab6`, branch `feat/composability`)

- `tools/virtues-installer/src/mode.rs` — `InferenceMode` resolution (env
  override → Dragon device-tree detect → interactive cliclack flow with
  recipes), manual-endpoint validation: dims probe, p50 latency verdict
  (tiered copy), quantized-sha256 fingerprint over two canonical probe
  strings. Validation runs **before** any system mutation.
- Env contract: `VIRTUES_INFERENCE`, `VIRTUES_EMBED_URL`,
  `VIRTUES_EMBED_MODEL`, `VIRTUES_EMBED_FINGERPRINT`, `VIRTUES_EMBED_DIMS`,
  `VIRTUES_RERANK_URL`.
- `embedder.rs` boot guard: re-embeds the probes, refuses to serve on
  fingerprint mismatch (exact same request body as setup — see traps).
- 128-token chunking in `indexer.rs` (+tests) and **drain mode** in the
  embedding_index action (full-batch loop, pg advisory single-flight, 2h
  internal ceiling) with a matching per-action subprocess timeout in
  `action_runner` (keyed on `action.dir`). Onboarding: weeks → hours.
- Jetson machinery deleted (installer download.rs, cli/upgrade.rs).

## Landed 2026-07-06 (branch `feat/iroh-pivot`)

The DIY-composability decisions from the 2026-07-06 strategy session, built and
merged:

- **Local-only enforcement** (`installer/mode.rs::ensure_local`): manual mode
  refuses a public embed/rerank endpoint (loopback / RFC1918 / link-local /
  CGNAT-100.64 / IPv6-ULA pass; global fails). This *is* the "no cloud embedding
  APIs" rule — and it let us skip API-key/cost plumbing entirely.
  `VIRTUES_ALLOW_REMOTE_INFERENCE=1` is the logged expert override.
- **Recipes**: two per endpoint — embeddings (llama.cpp, Ollama) + rerank
  (llama.cpp GPU/CPU), all on the pinned contracts.
- **Inference picker** (non-Dragon, interactive): choose **Bring your own
  endpoint** (BYO — GPU/Ollama/another box, the daily-use path) or **Quick
  trial (bundled, CPU-only)** (`InferenceMode::Bundled` → same local-sidecar
  provisioning as Dragon, honestly labeled "not for production; slow on large
  data"). Fixes the dead-end where Manual demanded a URL before any endpoint
  existed. Headless: `VIRTUES_INFERENCE=bundled`. GPU is never auto-managed on
  non-Dragon hardware (compile-time + per-vendor) — it stays BYO.
- **Composable prompt prefixes**: runtime reads
  `VIRTUES_EMBED_QUERY_PROMPT`/`_DOC_PROMPT` (empty = none); installer resolves
  them with a **maintenance-free ladder** — explicit env → the model's own HF
  `config_sentence_transformers.json` (optional repo-id, substring key match) →
  a 5-family table (embeddinggemma/e5/nomic/bge/gte) → none. Written quoted so
  trailing spaces survive systemd/dotenv. Never touches the fingerprint.
- **Step 2 done — dims-at-setup**: `search::embedder::configured_embed_dim()` is
  the single source of truth; manual stores native dims (no truncation), Dragon
  keeps 768→256. `database::ensure_embedding_dims` (runs after migrations) sizes
  `search_vectors` + `search_topic_cache` and rebuilds the HNSW index; no-op when
  correct, safe only on an empty index, refuses >2000 dims. **`halfvec` (>2000)
  is the one deferred piece** — cloud is blocked, so local >2000-dim models are
  rare; revisit if a beta tester needs one.
- **Step 1 done — `virtues configure-inference`**: the recovery command the boot
  guard/dims errors point at. Re-probes the endpoint (guard-free), reports
  fingerprint/dims changes, and on confirmation wipes the derived index (source
  safe), re-pins fingerprint+dims, and resizes columns. **Refinement still open:**
  it compares fingerprints (exact match), not the plan's *cosine verdict* — the
  "same model, different quantization → keep your index" optimization needs the
  probe **vectors** stored at setup (today only the hash is). A quant change
  currently reads as "different model" → a (safe but unnecessary) re-embed.

## Next steps, in order

### 1. `virtues configure-inference` + recovery ✅ SHIPPED (fingerprint-based)
See "Landed 2026-07-06" above. Built with exact-fingerprint comparison + safe
re-embed. The cosine-verdict refinement below is **still open** (needs probe
vectors stored at setup). Original design (refined 2026-07-06):

- **Store the probe VECTORS at setup** (2×768 floats, alongside the hash),
  not hash-only. A mismatch then gets a cosine verdict:
  - fingerprint matches → URL/config change only, touch nothing (the 90%
    case: user moved their server);
  - cosine ≥ ~0.99 → "same model, different quantization — keep your index
    (tiny quality cost) or re-embed for exactness";
  - below → "different model — re-embed required."
- Re-embed v1 = wipe vectors (NEVER source rows) + re-pin fingerprint + let
  drain-mode indexing rebuild; print the time estimate from corpus size.
  Embeddings are a derived cache — frame it that way in all copy.
- v2 (labeled successor, not now): blue/green — `search_embeddings.model`
  is already stamped per row, so old-model vectors can keep serving while
  the new set builds, then flip atomically.

### 2. Dims-at-setup ✅ SHIPPED
See "Landed 2026-07-06" above (`halfvec` >2000 deferred). Original scope:

The installer records `VIRTUES_EMBED_DIMS` and the embedder skips the
`NATIVE_DIM` check when a fingerprint is pinned, **but the schema is still
`vector(256)` with a hardcoded 768→256 Matryoshka truncation**
(`migrations/0017`; `search_vectors.embedding` and
`search_topic_cache.embedding`; `NATIVE_DIM`/`EMBED_DIM` consts +
`matryoshka_truncate()` in `embedder.rs`). A manual endpoint with e.g.
384-dim output will misbehave.

For **any** embedding model to work, four things generalize — this is the
full scope of "dims-at-setup":

1. **Dynamic column width.** Size `search_vectors.embedding` +
   `search_topic_cache.embedding` from the probed `VIRTUES_EMBED_DIMS` at
   bringup, not the literal `256`. Migration 0017's `ALTER COLUMN … TYPE
   vector(N)` is the exact pattern to parameterize.
2. **Column *type* chosen by dims — the pgvector index ceiling.** A plain
   `vector` HNSW index caps at **2000 dimensions**; `halfvec` (half-precision)
   is HNSW-indexable to **4000**. So bringup must pick the type, not just the
   width:
   - `N ≤ 2000` → `vector(N)`
   - `2000 < N ≤ 4000` → `halfvec(N)`
   - `N > 4000` → require MRL truncation (below), else no ANN index
     (brute-force fallback, logged).
   Without this, a large model (e.g. OpenAI `text-embedding-3-large`, 3072)
   silently can't build an index.
3. **Conditional truncation.** The 768→256 Matryoshka slice only works for
   **MRL-trained** models (EmbeddingGemma, nomic). A non-MRL model
   (bge, e5, MiniLM, most OpenAI) **cannot** be truncated without destroying
   the vector. Default = store **native dims, no truncation**; only truncate
   when the model is known-MRL *and* the user opts into a smaller target.
   `EMBED_DIM`/`NATIVE_DIM` stop being consts → config-driven (stored width =
   the truncation target, or native dims); keep 768→256 as the Dragon
   default only.
4. **Index rebuild on any width/type change.** HNSW is bound to the column's
   dims + opclass, so a model change = drop index → `ALTER COLUMN TYPE` →
   recreate index → re-embed corpus (wipe vectors, keep source rows — same
   path as `configure-inference` recovery). Keep `vector_cosine_ops` +
   normalize-on-write: cosine fits ~every sentence embedder, so the *metric*
   stays model-agnostic even as dims vary.

**Invariant that keeps this safe:** one active embedding model per column.
Two models' vectors are geometrically incomparable — never mixed in one
space. Enforced by the fingerprint boot-guard + the per-row
`search_embeddings.model` stamp. "Any model" means "any *one* model at a
time," swapped by a deliberate re-embed.

**Rerankers need none of this** — they are pure runtime request/response
(`(query, docs) → scores`), persist nothing, and are composable today via
the Manual-mode URL alone.

### 3. Storage preflight (diagnosis-only module in the installer)
Classify the data disk (NVMe/eMMC/SD/USB/HDD via sysfs + rotational flag +
mmc boot0), **measure** it (~5s: seq write, 4k random, fsync-honesty — a
suspiciously fast fdatasync means a lying volatile cache), free space vs
corpus projection, tiered warnings **with numbers** ("searches ~3–5s on
this disk"), SMART baseline capture. Clean-disk offer: only whole, provably
empty devices, typed confirmation. Hard refuse: Postgres on NFS/SMB. Never:
repartition non-empty disks, migrate data, install proprietary drivers.
Every guardrail has an env/flag override that is logged and visible in
`virtues doctor`.

### 4. Remaining small essentials
- Postgres systemd unit: `Type=notify` + `TimeoutSec=infinity` (default 90s
  kills WAL replay after power loss — the classic appliance data-loss
  path), `wal_compression=lz4`, data checksums on fresh initdb.
- `upgrade.rs`: auto-rollback using the existing `.bak` (on failed start OR
  failed health probe: swap back + restart, no human), post-start health
  probe (daemon answers + one search succeeds), ed25519 signature check on
  release artifacts (sha256-from-same-origin only catches corruption).
- Heartbeat: systemd timer POSTing uptime/temp/disk to our server (~20
  lines; not a metrics stack).

### 5. Jetson end-to-end dress rehearsal
Wipe the Jetson → real `curl virtues.com/sh | sudo sh` → Manual flow as a
stranger (it runs its own CUDA llama-server) → pair → onboard a corpus via
drain → search → deliberately swap the model behind the endpoint → confirm
the boot guard fires → recover via `configure-inference`. Whatever breaks
is the beta punch list.

### 6. Q8B arrival (~Jul 31)
Day 1 on unit #1: run our preflight against reality; llama.cpp Vulkan AND
OpenCL on the a690 (bench embed pp128 + rerank; pick backend + `-ngl` per
measurement, not ideology — note the repo's own finding that embedding ran
faster on CPU on the Orin: bench decides per model×hardware). Timeboxed NPU
experiment. Then: golden image (env-pinned Dragon mode, data partition
separate), flash 10, overnight soak, power-pull test, ship 9, keep 1.

## Traps already hit (don't rediscover these)

- **Ollama routes by model name**: `"model": "default"` 404s. That's why
  `VIRTUES_EMBED_MODEL` exists and is threaded through picker → probes →
  env → every runtime request. Setup-time and boot-time fingerprint
  requests must be **byte-identical** or the guard compares apples/oranges.
- The fingerprint quantizes components to `(x*10000).round() as i32` LE
  bytes before sha256 (float-formatting stability). Two copies exist —
  installer `mode.rs` and core `embedder.rs` — with MUST-MATCH comments;
  the installer can't depend on the core crate.
- The action runner SIGKILLs subprocesses at 300s; the embedding_index
  override (2h+5min, keyed on `action.dir` because `action.id` gets
  collision-suffixed) exists so drains aren't killed mid-run. The drain's
  own 2h ceiling exits cleanly first, by design.
- The runner's `has_active_run` gate treats runs >600s as stale — the pg
  advisory lock inside the drain is what actually prevents double-indexing.
- llama.cpp Vulkan on the Q6A's Adreno 643 GPU-hangs (`vk::DeviceLost`).
  Retired board, don't debug — but remember OpenCL is the fallback backend
  on Adreno if the a690 misbehaves too.
- `curl | sh` can't prompt via stdin; cliclack reads `/dev/tty` directly
  (that's why the interactive flow works). No TTY + no env override = clear
  error, not a hang.
- **pgvector HNSW dims ceiling: `vector` ≤ 2000, `halfvec` ≤ 4000.** A model
  above 2000 dims silently can't index as plain `vector` — see step 2.

## Quality reference numbers (from the validated R&D floor)

128-token chunks + small-to-big (retrieve small, rerank with wider context)
+ hybrid dense+lexical fusion + conditional rerank are all carried by
measurement, not fashion: short-text nDCG@10 0.962 on-device, hybrid fusion
+0.053 over dense-only, conditional rerank beating always-rerank. Ingest
pipeline sustains ~200 windows/s for hours on modest hardware (3h soak,
flat, zero leak) → 1M-item onboarding ≈ 1.5–2h. UX budgets: query embed
<50ms p50, rerank@10 <500ms, E2E search <1s, ingest ≥50 win/s.
