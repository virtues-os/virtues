# Migrating the local embedding / rerank models

The box talks to local inference over two loopback HTTP endpoints — embedding
on `:18181`, rerank on `:18182` — and two different servers sit behind them
([composable-inference.md](composable-inference.md)):

| Profile | Server | Files it loads |
|---|---|---|
| **Dragon** (`VIRTUES_INFERENCE=dragon`) | `virtues-qnnd` on the Hexagon NPU, one unit | QAIRT context binaries + tokenizers under `$DATA_DIR/models/qnn/` |
| **Bundled** (`VIRTUES_INFERENCE=bundled`) and `make dev` | two CPU `llama-server` sidecars, `virtues-embed` and `virtues-rerank` | GGUFs under `$DATA_DIR/models/` |

Manual (BYO) endpoints are the owner's; nothing here applies to them.

Each file set is pinned in places that **must agree**, or the box serves one
model while the runtime expects another (embeds rejected at the width check,
search silently broken).

## The pin sites

**GGUFs (Bundled, dev):**

1. `virtues-core/src/inference_report.rs` — `EMBED_GGUF` / `RERANK_GGUF`.
2. `tools/virtues-installer/src/config.rs` — `embed_gguf` / `rerank_gguf`:
   what the installer downloads and writes into the sidecar units' `-m`.
   Model-specific settings ride with the model in `install.rs`
   (`VIRTUES_EMBED_DIMS`, the query/doc prompts).
3. `Makefile` — `EMBED_GGUF` / `RERANK_GGUF` and their download URLs, for
   `make dev`.

**QNN context binaries (Dragon):**

1. `virtues-core/src/inference_report.rs` — `QNN_EMBED_BIN` / `QNN_RERANK_BIN`.
2. `tools/virtues-installer/src/config.rs` — `qnn_embed_bin` / `qnn_rerank_bin`
   and `qnn_tokenizers` (fetched into `tok_gte/` and `tok_colbert/`, the layout
   `crates/virtues-qnnd` loads). The installer writes the binaries' paths into
   the `virtues-qnnd` unit.
3. `tools/publish-qnn-models.sh` — the asset names it uploads.

**Both:** the bytes live on the `models-1` GitHub release (`models_base`),
SHA-256 sidecars beside them. Assets are immutable: ship a new model under a
**new file name**, never replace one in place.

## Shipping a model change

1. **Publish the files** to the models release (additive; old ones stay):
   - GGUFs:
     ```
     gh workflow run models-release.yml \
       -f embed_url='<vetted HF url>' \
       -f rerank_url='<vetted HF url>' \
       -f tag='models-1'
     ```
   - QNN binaries + tokenizers:
     `MODELS_TAG=models-1 SRC=<host:dir> ./tools/publish-qnn-models.sh`

   Verify: `gh release view models-1 --json assets`.
2. **Update every pin site** for that file set so the names match the uploads.
3. **Cut a code release** carrying the new `inference_report` + installer.

## Getting it onto a box

`virtues upgrade` swaps binaries but **does not** migrate the model set: it
neither downloads models nor rewrites the inference units. Reconcile by
re-running the installer pinned to the release (idempotent; preserves data):

```
curl -sSL https://virtues.com/sh | sudo VIRTUES_VERSION=<tag> sh
```

This fetches the new files, rewrites the units (`virtues-embed` +
`virtues-rerank`, or `virtues-qnnd`), and restarts them. Confirm with
`virtues doctor`.

`virtues upgrade` warns about missing models only where the sidecar unit
exists (`/etc/systemd/system/virtues-embed.service`), so **on a Dragon it
prints nothing** — a release that changes the QNN binaries has to say so in
its notes.

## Reindex (required on an embedding-model change)

Vectors from the old model are **not comparable** to the new model's, even at
the same width. Rebuild with:

```
sudo -u virtues virtues reindex
```

It wipes the derived index — `search_embeddings` (cascading to
`search_vectors` and the BM25 postings), `search_topic_cache`, and
`wiki_events`' embedding and scores — **and resets `search_index_meta`**: the
BM25 corpus stats and the recorded model and width. Then it resizes the vector
columns, re-embeds from source inline, and rescores every past day's events.
Source rows are never touched.

**Do not `TRUNCATE` the vector tables by hand.** That leaves
`search_index_meta` recording the old model and width, and the indexer refuses
to write vectors from a model the index was not built with — so nothing
re-embeds. It also leaves BM25 corpus stats describing documents that are gone,
and every past day's event scores null (the nightly job rescores only its own
day).

Until the re-embed catches up, semantic search returns fewer or no hits;
lexical search is unaffected.
