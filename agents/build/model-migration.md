# Migrating the local embedding model

The box talks to local inference over loopback HTTP: embedding on `:18181`
([composable-inference.md](composable-inference.md)). Search doesn't rerank.

| Profile | Server | Files it loads |
|---|---|---|
| **Dragon** (`VIRTUES_INFERENCE=dragon`), installs before the move only | `virtues-qnnd` on the Hexagon NPU, one unit | QAIRT context binaries + tokenizers under `$DATA_DIR/models/qnn/` |
| **Bundled** (`VIRTUES_INFERENCE=bundled`) | one CPU `llama-server` sidecar, `virtues-embed` (plus `virtues-embed-next` on `:18183` while a model change runs) | GGUFs under `$DATA_DIR/models/` |
| `make dev` | `llama-server` sidecars the Makefile starts | GGUFs under `.data/models/` |

Manual (BYO) endpoints are the owner's: nothing here touches them, and
updates never change their model. `sudo virtues configure-inference
--recommended` moves one back to the bundled setup, through the same
background change described below.

Each file set is pinned in places that **must agree**, or the box serves one
model while the runtime expects another.

## The pin sites

**GGUFs (Bundled, dev):**

1. `virtues-core/src/inference_report.rs` — `EMBED_GGUF`, `EMBED_GGUF_SHA256`
   (what `virtues upgrade` downloads and verifies), and the model's prompts
   `EMBED_QUERY_PROMPT` / `EMBED_DOC_PROMPT`.
2. `tools/virtues-installer/src/config.rs` — `embed_gguf`: what a fresh install
   downloads and writes into the unit's `-m`. Its prompts are `GEMMA_*_PROMPT`
   in `install.rs`.
3. `Makefile` — `EMBED_GGUF` and its download URL, for `make dev`.
4. `.github/workflows/models-release.yml` — the default `embed_url`.

**QNN context binaries (Dragon):** frozen. The installer no longer sets up
`virtues-qnnd`, and Dragons still on it move to the bundled setup (below), so
nothing fetches these again. `inference_report.rs` names them
(`QNN_EMBED_BIN` / `QNN_RERANK_BIN`) only to report what an unmoved Dragon
runs.

**GGUFs:** the bytes live on the `models-1` GitHub release (`models_base`),
SHA-256 sidecars beside them. Assets are immutable: ship a new model under a
**new file name**, never replace one in place. Older installers still fetch
the old names, so they stay.

## Shipping a model change

1. **Publish the file** to the models release (additive; old ones stay). One
   new GGUF can go up on its own:
   ```
   gh release upload models-1 <file>.gguf <file>.gguf.sha256
   ```
   or through `models-release.yml`, which re-fetches every asset it names.
   QNN binaries: `MODELS_TAG=models-1 SRC=<host:dir> ./tools/publish-qnn-models.sh`.
   Verify: `gh release view models-1 --json assets`.
2. **Update every pin site** for that file set, including the SHA-256.
3. **Cut a code release.**

## Getting it onto a box

**Bundled boxes switch models automatically.** On `virtues upgrade` and the nightly
`virtues auto-update`, `cli::model_set` (root):

1. retires the reranker sidecar older installs ran;
2. if `virtues-embed`'s `-m` isn't `EMBED_GGUF`, downloads it (checked against
   `EMBED_GGUF_SHA256`), runs it as `virtues-embed-next` on `:18183`, and sets
   `VIRTUES_EMBED_NEXT_URL` and the next prompts in the box env file;
3. after the index has moved, moves the new model onto `virtues-embed` and
   `:18181` and deletes the old file (on the next activation, or the nightly
   pass with a server restart).

Between 2 and 3, the indexer (`search::next_index`, inside the embedding
applet's run, under its lock) builds the new model's vectors from the chunk
text into `search_vectors_next` while search keeps using the old model. When
every chunk has one, it swaps the two tables in one short transaction
(`lock_timeout` 5s, so it retries rather than holds search), promotes
`VIRTUES_EMBED_NEXT_*` to `VIRTUES_EMBED_*` in the env file (every process
reads embedding settings from there through `box_env::var`), and then, under
`search_index_meta.rescore_pending`, forgets the old event embeddings and
rescores every day. Settings → Search shows the progress.

**A Dragon on its NPU moves to the bundled setup** when its owner runs
`sudo virtues configure-inference --recommended` (or re-runs the installer,
which ends with that). For now nothing starts it automatically, so the first
boxes to move are ones someone watches (`start_recommended_change` has a dated
TODO). The command starts the recommended model as `virtues-embed-next` and
sets `VIRTUES_INFERENCE=bundled`, while `virtues-qnnd` keeps serving search.
The settle after the swap retires `virtues-qnnd` and its settings before the
new model takes `:18181`. A Dragon's sidecar unit is always written fresh with
its pacing (two threads on cores 4 to 6, `CPUWeight=20`), never retargeted
from an older unit. A Dragon on `manual` is left alone like any other.

**Re-running the installer on an existing bundled box** rewrites
`virtues-embed` to the new model in place and keeps the box's env values,
including any `VIRTUES_EMBED_DIMS`, so search refuses the old index until
`virtues reindex` runs. Prefer `virtues upgrade`.

## Reindex

`sudo -u virtues virtues reindex` rebuilds the index with the current model
in place: wipe, re-embed, rescore. Search returns fewer or no semantic hits
until it catches up (lexical search is unaffected), so it's the recovery path
for a stale or broken index, not the way to change models.

It wipes the derived index — `search_embeddings` (cascading to
`search_vectors`, `search_vectors_next` and the BM25 postings),
`search_topic_cache`, and `wiki_events`' embedding and scores — **and resets
`search_index_meta`**: the BM25 corpus stats, the recorded model and width, and
any model change under way. Then it resizes the vector columns, re-embeds from
source inline, and rescores every past day's events. Source rows are never
touched.

**Do not `TRUNCATE` the vector tables by hand.** That leaves
`search_index_meta` recording the old model and width, and the indexer refuses
to write vectors from a model the index was not built with — so nothing
re-embeds. It also leaves BM25 corpus stats describing documents that are gone,
and every past day's event scores null (the nightly job rescores only its own
day).
