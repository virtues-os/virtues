# Migrating the local embedding / rerank models

The box runs all local ML through two `llama-server` sidecars — embedding on
`:18181`, rerank on `:18182`. The GGUFs they load are pinned in **three places
that must agree**, or the box serves one model while the runtime expects another
(embeds get rejected at the native-dim check, search silently breaks):

1. **`virtues-core/src/inference_report.rs`** — `EMBED_GGUF` / `RERANK_GGUF`.
   The runtime's dim + pooling expectations (`search/embedder.rs`,
   `search/reranker.rs`) are built around these.
2. **`tools/virtues-installer/src/config.rs`** — `embed_gguf` / `rerank_gguf`.
   What the installer downloads and bakes into the sidecar unit `-m` path.
3. **The `models-1` GitHub release** — the actual GGUF bytes the installer pulls
   (`models_base`). Assets are immutable: ship a new model under a **new file
   name**, never replace one in place.

The `Makefile` (`EMBED_GGUF` / `RERANK_GGUF` + `_embed-run` pooling) mirrors this
for local `make dev`.

## Current models (as of the EmbeddingGemma switch)

| role   | GGUF                                        | native | stored | pooling |
|--------|---------------------------------------------|--------|--------|---------|
| embed  | `embeddinggemma-300m-qat-Q8_0.gguf`         | 768    | 256 (Matryoshka) | mean |
| rerank | `gte-reranker-modernbert-base-Q8_0.gguf`    | —      | —      | rank |

(Previously: `bge-m3-FP16.gguf` 1024-dim cls, `bge-reranker-v2-m3-Q8_0.gguf`.)

## Shipping a model change

1. **Publish the GGUFs** to the models bucket (additive, keeps old ones):
   ```
   gh workflow run models-release.yml \
     -f embed_url='<vetted HF url>' \
     -f rerank_url='<vetted HF url>' \
     -f tag='models-1'
   ```
   Verify: `gh release view models-1 --json assets`.
2. **Update the three pin sites** above (+ the Makefile) so the names match the
   uploaded files.
3. **Cut a code release** (staging tag → CI) carrying the new
   `inference_report` + installer.

## Getting it onto a box

`virtues upgrade` swaps binaries but **does not** migrate the model set (it
neither downloads GGUFs nor rewrites the sidecar unit `-m`/pooling). After an
upgrade that changes models, it now **detects the drift and prints the fix**
rather than degrading silently. To reconcile, re-run the installer pinned to the
release (idempotent; preserves data):

```
curl -sSL https://virtues.com/sh | sudo VIRTUES_VERSION=<tag> sh
```

This fetches the new GGUFs from `models-1`, rewrites both sidecar units, and
restarts them. Confirm with `virtues doctor` (both models should read present).

## Reindex (required on an embedding-model change)

Vectors embedded by the old model are **not comparable** to the new model's, even
when the stored dimension is unchanged (both truncate to `vector(256)`). The
embedding indexer (`search/indexer.rs::run_embedding_job`) is a gap-filler
(`WHERE se.id IS NULL`), so clearing the vector tables makes it re-embed
everything with the new model on its next scheduled tick:

```sql
-- on the box, as the virtues role
TRUNCATE search_vectors;
TRUNCATE search_embeddings;
```

The scheduler re-populates both from scratch. Until it catches up, semantic
search returns fewer/no hits; lexical search is unaffected.
