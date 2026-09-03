---
title: Setting up inference
description: Virtues searches your record with two local models — an embedder and a reranker — that you run yourself. The contracts they must speak, the llama.cpp commands that serve them, which models work, and how to point the box at them.
updated: 2026-09-03
---

Search over your own life needs two small models running near your data: an
**embedder**, which turns everything in your record into vectors, and a
**reranker**, which re-scores the candidates a search turns up. Neither is
the model that writes — that one is remote, and this page has nothing to do
with it.

On hardware we build, both are provisioned for you and there is nothing to
read here. On your own machine they are **yours to run**, and standing them
up is worth doing *before* you install, because the installer asks for their
URLs and refuses to guess.

## What Virtues consumes

Two HTTP contracts, and nothing else:

| Endpoint | Required | Used for |
|---|---|---|
| `POST /v1/embeddings` | Yes | Indexing your record, and embedding every query |
| `POST /v1/rerank` | No | Precision on the results of a search |

Without an embedder there is no semantic search and no indexing at all.
Without a reranker search still works, ranked by vector similarity and
lexical fusion alone, at slightly lower precision — a real option, not a
degraded mode to be ashamed of.

Both endpoints must be on **your own machine, your LAN, or your VPN**. The
installer refuses a public address: loopback, RFC1918, link-local, CGNAT
(Tailscale and friends), and IPv6 unique-local addresses pass; anything
globally routable does not. Cloud embedding APIs are deliberately
unsupported, which is why nothing here asks you for an API key.
`VIRTUES_ALLOW_REMOTE_INFERENCE=1` overrides the check, is logged when it
does, and means what it says: inference traffic may leave your network.

## Why this is yours to run

We provision inference on exactly one board — our own, where we know the
accelerator, the driver stack, and what happens after a power cut. We do not
install GPU or NPU inference software on hardware we cannot test, because
doing so produces more broken boxes than it saves keystrokes. So the generic
path is that you own the endpoint and we validate it at the door.

The installer offers three answers:

- **Our hardware** — detected from the device tree. Inference is built in.
- **Bring your own endpoint** *(recommended for everything else)* — you run
  the servers; the installer probes them, records what it found, and pins it.
- **Kick the tires** — a bundled CPU-only model server with our two models
  and no configuration. Honestly slow, explicitly not a deployment. It
  exists so you can watch the product move in five minutes; stand up real
  endpoints before you load real data.

Headless installs skip the picker with `VIRTUES_INFERENCE=manual` (plus
`VIRTUES_EMBED_URL`, optionally `VIRTUES_RERANK_URL`) or
`VIRTUES_INFERENCE=bundled`.

## Standing up the endpoints

[llama.cpp](https://github.com/ggml-org/llama.cpp)'s `llama-server` speaks
both contracts and is what we run in production, so the recipes below are the
ones our own units use rather than a plausible guess. One server per model —
a single `llama-server` process hosts one model, and the two want different
flags.

**The embedder**, on port 18181:

```bash
llama-server --embedding --pooling mean \
  -m embeddinggemma-300m-qat-Q8_0.gguf \
  --host 127.0.0.1 --port 18181 \
  -c 2048 -b 2048 -ub 2048 -np 1 --cache-ram 0 -ngl 0
```

**The reranker**, on port 18182:

```bash
llama-server --rerank --pooling rank \
  -m gte-reranker-modernbert-base-Q8_0.gguf \
  --host 127.0.0.1 --port 18182 \
  -c 2048 -b 2048 -ub 2048 -np 1 --cache-ram 0 -ngl 99
```

What the flags are doing, since these are the ones that matter:

- **`--pooling mean` for the embedder, `--pooling rank` for the reranker.**
  Not interchangeable. The reranker is a cross-encoder; `rank` is what makes
  `/v1/rerank` exist at all.
- **`-ngl 0` on the embedder, `-ngl 99` on the reranker.** The two workloads
  want opposite hardware — see the reasoning in
  [What to run it on](/docs/setup/requirements#the-accelerator-question). If
  you have no GPU, `-ngl 99` is harmless; if you do, the reranker needs
  whatever group membership your distribution puts on the GPU device nodes,
  or the backend quietly falls back to CPU.
- **`-c/-b/-ub 2048`.** Both models handle longer, but Virtues indexes in
  windows of about 128 tokens and caps rerank documents near 256, so a larger
  context buys nothing and costs half a gigabyte of buffers.
- **`-np 1` and `--cache-ram 0`.** One request slot instead of the automatic
  four, and no prompt cache — every input here is unique, so the cache is
  pure reservation. Together with the context size these cut each server from
  roughly 2.5 GB resident to about 1 GB.

If you want the servers to survive a reboot, run each as a systemd unit. The
two units Virtues writes on the bundled path — `virtues-embed.service` and
`virtues-rerank.service`, both in `/etc/systemd/system/` — are a fair
template: loopback-only, unprivileged, `ProtectSystem=strict`,
`Restart=on-failure` with a start limit so a permanently broken server ends
up visibly `failed` rather than restarting forever.

Other servers work too, as long as they speak the contract below —
[Ollama](https://ollama.com), vLLM, or a vendor's NPU runtime. Read
[the contract](#the-contract-in-full) before you commit to one; the
requirement people trip over is `GET /health`.

You do not need to build llama.cpp to try it: every Virtues release ships a
portable **CPU-only** `llama-server` and puts it at
`/usr/local/bin/llama-server`. That's enough for the embedder. For a GPU
reranker, use a build made for your hardware.

## Choosing models

Anything that emits sentence embeddings will work. These are the ones we run
or would reach for, with the two properties that actually matter for
configuration — the width of the vector, and whether the model
wants a prefix on its inputs:

| Embedding model | Dims | Prompt prefixes |
|---|---|---|
| **EmbeddingGemma-300M** *(what we ship)* | 768, truncatable to 256 | `task: search result \| query: ` / `title: none \| text: ` |
| gte-small | 384 | none |
| bge-small-en-v1.5 | 384 | query only: `Represent this sentence for searching relevant passages: ` |
| e5-small-v2 | 384 | `query: ` / `passage: ` |
| nomic-embed-text-v1.5 | 768, truncatable | `search_query: ` / `search_document: ` |

| Reranker | Note |
|---|---|
| **gte-reranker-modernbert-base** *(what we ship)* | Cross-encoder, served by llama.cpp directly |
| bge-reranker-v2-m3 | Multilingual, larger |
| jina-reranker-v2 | Multilingual |

GGUF builds of all of these are on Hugging Face; search the model name plus
`gguf`. Quantized to Q8_0, the pair we ship is about half a gigabyte
together, and these are the exact builds we run:

```bash
curl -fLO https://huggingface.co/ggml-org/embeddinggemma-300m-qat-q8_0-GGUF/resolve/main/embeddinggemma-300m-qat-Q8_0.gguf
curl -fLO https://huggingface.co/keisuke-miyako/gte-reranker-modernbert-base-gguf-q8_0/resolve/main/gte-reranker-modernbert-base-Q8_0.gguf
```

Four things to know before you pick:

**Prefixes are a property of the model, not of Virtues.** An asymmetric model
embedded without its prefixes loses recall quality; a symmetric model given
prefixes gains noise in every vector. The installer resolves them from the
model name for the five families above, and you can always set
`VIRTUES_EMBED_QUERY_PROMPT` / `VIRTUES_EMBED_DOC_PROMPT` yourself — your
model's `config_sentence_transformers.json` is where its own answer lives.
Unknown model, no prefix, is the safe default and what you get.

**Width is probed, not assumed.** Whatever your model emits becomes the width
of the vector column, up to **4000 dimensions**, which is the ceiling
pgvector's index supports. Above that you must truncate.

**Truncation is only safe for models trained for it.** Setting
`VIRTUES_EMBED_DIMS` slices vectors to a narrower width — a third of the
storage and a faster index for very little quality on a Matryoshka-trained
model like EmbeddingGemma or nomic. On a model that was *not* trained that
way it destroys the vector. It is opt-in per model, never a default, and
asking for a width wider than the model emits is an error rather than
something we pad.

**One model at a time.** Two models' vectors are geometrically
incomparable, so a single embedding model owns the index. Changing it is a
deliberate re-embed, described below.

## Pointing Virtues at them

With the servers running, install:

```bash
curl -sSL https://virtues.com/sh | sudo sh
```

The first thing it asks — before it touches a package, a service, or a disk —
is how you want inference. Choose bring-your-own and give it the two URLs
(`http://localhost:18181` and `http://localhost:18182` for the recipes
above; the rerank prompt takes an empty answer). It then probes what you gave
it and prints what it found: the vector width, a latency verdict, and whether
the reranker answered.

Everything it learned lands in the box's environment file at
`/var/lib/virtues/virtues.env`, which is where you go to change any of it
later:

| Variable | Meaning |
|---|---|
| `VIRTUES_EMBED_URL` | Base URL of the embedding server |
| `VIRTUES_RERANK_URL` | Base URL of the rerank server, if you have one |
| `VIRTUES_EMBED_MODEL` | The `model` name sent in each request. llama.cpp ignores it; Ollama routes by it and 404s on a name it hasn't pulled |
| `VIRTUES_EMBED_DIMS` | Stored vector width — set only to truncate a Matryoshka model |
| `VIRTUES_EMBED_QUERY_PROMPT` / `_DOC_PROMPT` | The model's prefixes, quoted so trailing spaces survive |
| `VIRTUES_EMBED_FINGERPRINT` | Pinned identity of the model behind the URL |

## What gets pinned, and why

At setup the installer embeds two fixed probe strings and hashes the vectors
that come back. That hash is the model's fingerprint, and the box recomputes
it at every boot.

This exists because of a failure that is otherwise silent. Swap the model
behind an unchanged URL and every vector already in the index belongs to a
different geometry: search doesn't error, it just quietly returns the wrong
things forever. So on a mismatch the box **refuses to serve search** rather
than answering from an index it can no longer trust.

Recovering from that is one command:

```bash
virtues configure-inference
```

It re-probes the endpoint, reports what changed, and — on your confirmation —
clears the derived index, re-pins the fingerprint and dimensions, resizes the
vector column, and lets indexing rebuild. **Your source data is never
touched.** Embeddings are a cache; treat them as one. `virtues reindex`
rebuilds the same index without the endpoint having changed.

Two caveats worth knowing. A quantization change reads as a different model,
so re-quantizing the same weights currently costs you a re-embed you didn't
strictly need. And moving a working endpoint to a new address is *not* a
model change: the fingerprint matches and nothing is rebuilt.

## The contract in full

If you're bringing a server we haven't named, this is exactly what it must
do.

**`GET /health` must return 2xx.** This one is not optional and is not part
of the OpenAI shape. Virtues probes it when the embedder and reranker start,
so it can fail with a clear message instead of a transport error on every
call, and `virtues doctor` probes it again on the running box.
`llama-server` answers it once the model is loaded. A server that doesn't
implement the route at all will read as *not serving* even when its
embeddings work perfectly — verify with a real embedding call before you
believe the row.

**`POST /v1/embeddings`** takes `{"input": [...], "model": "..."}` — always an
array, `model` always present — and must return:

```json
{ "data": [ { "index": 0, "embedding": [0.01, -0.02] } ] }
```

Rows are matched by `index`, so order on the wire doesn't matter, but a
top-level array with no `data` object will not parse.

**`POST /v1/rerank`** takes `{"query": "...", "documents": [...], "top_n": N}`
and must return:

```json
{ "results": [ { "index": 0, "relevance_score": 0.87 } ] }
```

The score field must be `relevance_score` — llama.cpp's and Jina's spelling.
A Cohere-style server that returns `score` instead passes the installer's
probe and then fails at search time, which is a sharp enough edge to be worth
naming.

**`GET /v1/models`** is optional. When a server offers it, Virtues records
what it says it is serving and stamps that on every indexed row.

## Verifying

Against the servers directly:

```bash
curl -s localhost:18181/health
curl -s localhost:18181/v1/embeddings \
  -H 'content-type: application/json' \
  -d '{"model":"default","input":["hello"]}' | head -c 200
curl -s localhost:18182/v1/rerank \
  -H 'content-type: application/json' \
  -d '{"query":"cat","documents":["a cat","a bicycle"],"top_n":2}'
```

Then against the box, which is the answer that counts:

```bash
virtues doctor
```

Its Inference section names the accelerator it resolved, the models on disk,
and — separately, because the two questions are not the same — whether
anything is actually *serving* at each URL. A box whose model server has been
crash-looping for a week still has both model files exactly where they were
put; only the live rows can tell you that search is broken. Every finding
comes with the command that diagnoses it.

## Changing your mind later

Moving from the bundled trial to your own endpoints, or from one model to
another, is the same short path: start the new servers, edit the URLs in
`/var/lib/virtues/virtues.env`, run `virtues configure-inference`, and let it
re-embed. Nothing about the decision is baked in at install time except which
services the installer provisioned, and on the bundled path those are two
ordinary systemd units you can stop and disable.
