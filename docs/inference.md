---
title: Setting up inference
description: Run Virtues search on your own embedding server - the contracts it must speak, the llama.cpp commands that serve it, which models work, and how to point Virtues at it.
updated: 2026-10-20
---

Search needs a small model running near your data: an **embedder**, which
turns everything in your record into vectors. This page doesn't cover the
model that writes your days and answers questions, which runs remotely.

You may not need this page. On hardware we build, search is set up for you.
On your own machine, the installer runs search on its CPU unless you choose
otherwise. This page is for running your own server instead: on a GPU or NPU
([the short version](/docs/setup/accelerators)), on another machine, or with
a different model.

## What Virtues consumes

One HTTP contract: `POST /v1/embeddings`, for indexing your record and for
embedding every query. Without it there is no semantic search and no indexing
at all.

Search ranks results by meaning and by exact words together, and doesn't
use a reranker. In our search tests, small local rerankers ranked results
worse than that combination. Hosted rerankers helped a little on this kind of
data, but they would send search results off your server and make every
search slower.

The endpoint must be on **your own machine, your LAN, or your VPN**. The
installer refuses a public address: loopback, RFC1918, link-local, CGNAT
(Tailscale and friends), and IPv6 unique-local addresses pass; anything
globally routable does not. Cloud embedding APIs are not supported, so
nothing here asks you for an API key. `VIRTUES_ALLOW_REMOTE_INFERENCE=1`
overrides the check and is logged when it does. With it set, inference
traffic may leave your network.

## Who runs the embedding server

We set up inference on only one board: our own, where we know the
accelerator, the driver stack, and what happens after a power cut. We don't
install GPU or NPU inference software on hardware we can't test, because it
would break more servers than it would save setup time. On other hardware,
you run the endpoint and the installer checks it before using it.

The installer offers three answers:

- **Our hardware** - detected from the device tree. Inference comes set up.
- **This machine's CPU** *(the default)* - Virtues runs the embedding model
  we recommend, on the CPU, and keeps it current. When a release recommends a
  different model, the update downloads it and rebuilds your index for it in
  the background; search keeps working on the old model until the new index is
  ready, then switches.
- **Your own server** - you run the server and choose the model; the
  installer probes it, records what it found, and pins it. Updates never
  change your model: it's yours to keep running and to upgrade when you
  decide to.

Headless installs skip the picker with `VIRTUES_INFERENCE=manual` (plus
`VIRTUES_EMBED_URL`) or
`VIRTUES_INFERENCE=bundled`.

## Standing up the endpoint

[llama.cpp](https://github.com/ggml-org/llama.cpp)'s `llama-server`
implements the contract and is what we run in production. The commands below
are the ones our own hardware uses.

**The embedder**, on port 18181:

```bash
llama-server --embedding --pooling mean \
  -m embeddinggemma-2-Q8_0.gguf \
  --host 127.0.0.1 --port 18181 \
  -c 2048 -b 2048 -ub 2048 -np 1 --cache-ram 0 -ngl 0
```

EmbeddingGemma 2 needs a recent llama.cpp. An older build can't load the
model file, so check your version first. The build number it prints should be
at least the one we run:

```bash
llama-server --version   # build 11507 or later
```

What the important flags do:

- **`--pooling mean`** sets how EmbeddingGemma combines its token vectors
  into one vector per input. Other pooling modes produce the wrong vector.
- **`-ngl 0` runs the model on the CPU, `-ngl 99` on a GPU.** The CPU
  engine Virtues installs keeps the embedder on the CPU, because
  EmbeddingGemma's math overflows the 16-bit arithmetic some GPU paths use
  (see [What to run it on](/docs/setup/requirements#do-you-need-a-gpu)).
  On a GPU, the server needs whatever group membership your distribution puts
  on the GPU device nodes, or the backend silently falls back to the CPU.
  When you point Virtues at a GPU server, it checks that the vectors match the
  CPU's before switching.
- **`-c/-b/-ub 2048`.** The model accepts longer inputs, but Virtues indexes
  in windows of about 128 tokens, so a larger context adds nothing and uses
  half a gigabyte more for buffers.
- **`-np 1` and `--cache-ram 0`** set one request slot instead of the
  default four, and turn off the prompt cache. Every input here is unique, so
  the cache would only reserve memory. Together with the context size, these
  cut the server's resident memory from roughly 2.5 GB to about 1 GB.

If you want the server to survive a reboot, run it as a systemd unit. The
one Virtues writes when search runs on the CPU, `virtues-embed.service` in
`/etc/systemd/system/`, is a good template. It listens on loopback only,
runs unprivileged with `ProtectSystem=strict`, and uses `Restart=on-failure`
with a start limit, so a server that keeps crashing ends up marked `failed`
instead of restarting forever.

Other servers work too if they implement the contract below, for example
[Ollama](https://ollama.com), vLLM, or a vendor's NPU runtime. Read
[the contract](#the-contract-in-full) before you choose one. Only one route,
`POST /v1/embeddings`, is required.

You do not need to build llama.cpp to try it: every Virtues release ships a
portable **CPU-only** `llama-server` and puts it at
`/usr/local/bin/llama-server`. For a GPU, use a build made for your hardware
([Faster search on a GPU or NPU](/docs/setup/accelerators)).

## Choosing models

Any model that produces sentence embeddings will work. These are the ones we
run or recommend, with the two properties that affect configuration: the
width of the vector, and whether the model expects a prefix on its inputs:

| Embedding model | Dims | Prompt prefixes |
|---|---|---|
| **EmbeddingGemma 2** *(what we ship)* | 768, truncatable to 256 | `task: search result \| query: ` / `title: none \| text: ` |
| gte-small | 384 | none |
| bge-small-en-v1.5 | 384 | query only: `Represent this sentence for searching relevant passages: ` |
| e5-small-v2 | 384 | `query: ` / `passage: ` |
| nomic-embed-text-v1.5 | 768, truncatable | `search_query: ` / `search_document: ` |

GGUF builds of all of these are on Hugging Face; search the model name plus
`gguf`. Quantized to Q8_0, the embedder we ship is about 300 MB. This is the
exact build we run:

```bash
curl -fLO https://huggingface.co/ggml-org/embeddinggemma-2-GGUF/resolve/bfcd298762cc34d0357ece5ebdd31791a3a374d8/embeddinggemma-2-Q8_0.gguf
```

Four things to know before you choose:

**Each model defines its own prefixes.** An asymmetric model used without its
prefixes finds fewer relevant results, and a symmetric model given prefixes
gets noise in every vector. The installer sets the prefixes from the model
name for the five families above. You can also set
`VIRTUES_EMBED_QUERY_PROMPT` / `VIRTUES_EMBED_DOC_PROMPT` yourself; your
model's `config_sentence_transformers.json` lists the prefixes it expects.
For a model the installer doesn't recognize, it uses no prefix.

**The installer measures the vector width.** Whatever width your model
produces becomes the width of the vector column, up to **4000 dimensions**,
the most pgvector's index supports. Above that you must truncate.

**Truncation is only safe for models trained for it.** Setting
`VIRTUES_EMBED_DIMS` cuts vectors to a narrower width. On a
Matryoshka-trained model like EmbeddingGemma or nomic, that uses a third of
the storage and gives a faster index, but search gets worse: in our tests,
EmbeddingGemma cut to 256 found noticeably fewer results than at its full 768,
so we store all 768. On a model that was *not* trained for it, truncation
ruins the vectors. It is off by default and you set it per model. Asking for
a width larger than the model produces is an error; Virtues doesn't pad
vectors.

**One model at a time.** Vectors from two different models can't be
compared, so the index uses a single embedding model. Changing the model
means re-embedding your record, as described below.

## Pointing Virtues at them

With the servers running, install:

```bash
curl -sSL https://virtues.com/sh | sudo sh
```

The first thing it asks - before it touches a package, a service, or a disk -
is how search should run. Choose **On a server I've already set up** (or **On
my own server**) and give it the URL (`http://localhost:18181` for the recipe
above). If Virtues is already installed, point the running server at yours
with `virtues configure-inference --embed-url <URL>` instead. The installer
tests the URL and prints what it found: the vector width, a latency grade, and
whether it has a `/health` route. `configure-inference` prints the width and
whether the new server runs the same model as the current one.

The results are saved in the server's environment file at
`/var/lib/virtues/virtues.env`. Edit that file to change any of them later:

| Variable | Meaning |
|---|---|
| `VIRTUES_EMBED_URL` | Base URL of the embedding server |
| `VIRTUES_EMBED_MODEL` | The `model` name sent in each request. llama.cpp ignores it; Ollama routes by it and 404s on a name it hasn't pulled |
| `VIRTUES_EMBED_DIMS` | Stored vector width - set only to truncate a Matryoshka model |
| `VIRTUES_EMBED_QUERY_PROMPT` / `_DOC_PROMPT` | The model's prefixes, quoted so trailing spaces survive |
| `VIRTUES_EMBED_FINGERPRINT` | Pinned identity of the model behind the URL |

## What gets pinned, and why

At setup the installer embeds two fixed probe strings and hashes the vectors
that come back. That hash is the model's fingerprint, and the server recomputes
it at every boot.

This catches a failure that would otherwise be silent. If the model behind
an unchanged URL is swapped, every vector already in the index came from a
different model. Search doesn't return an error; it returns the wrong results.
So when the fingerprint doesn't match, the server **stops serving search**
instead of answering from an index that no longer matches the model.

To recover, run:

```bash
virtues configure-inference
```

It re-probes the endpoint, reports what changed, and - on your confirmation -
re-pins the fingerprint and dimensions, rebuilds the derived index from your
source data with the new model, and rescores every day's events. Then restart
the server so search uses the new model. **Your source data is never
changed.** The embeddings are derived from it and can always be rebuilt.
`virtues reindex` rebuilds the same index when the endpoint hasn't changed.

Two caveats. A quantization change counts as a different model, so
re-quantizing the same weights currently triggers a re-embed that isn't
strictly needed. Moving a working endpoint to a new address is *not* a model
change: the fingerprint matches and nothing is rebuilt.

## The contract in full

If you use a server not listed on this page, this is what it must do.

**`GET /health` is optional.** It isn't part of the OpenAI API, and many
servers don't have it; Ollama returns 404 there while serving embeddings
normally. Virtues requests it when the embedder starts as a shortcut: a 2xx
means "ready" without running an inference, and `llama-server` returns one
once the model is loaded. Any other response is treated as *unknown*, and
Virtues then checks the endpoint by asking it to embed, which it does right
afterwards in every case. The only fatal result is when nothing accepts a
connection.

`virtues doctor` uses the same checks, so a server without the route shows as
`✓ serving (no /health)` and isn't reported as a failure.

**`POST /v1/embeddings`** takes `{"input": [...], "model": "..."}` - always an
array, `model` always present - and must return:

```json
{ "data": [ { "index": 0, "embedding": [0.01, -0.02] } ] }
```

Rows are matched by `index`, so order on the wire doesn't matter, but a
top-level array with no `data` object will not parse.

**`GET /v1/models`** is optional. When a server offers it, Virtues records
what it says it is serving and stamps that on every indexed row.

## Verifying

Against the server directly:

```bash
curl -s localhost:18181/health
curl -s localhost:18181/v1/embeddings \
  -H 'content-type: application/json' \
  -d '{"model":"default","input":["hello"]}' | head -c 200
```

Then check from Virtues, which is the test that matters:

```bash
virtues doctor
```

Its Inference section shows the accelerator it detected, the models on disk,
and, separately, whether anything is actually *serving* at the URL. These are
reported separately because a model server that has been crash-looping for a
week still has its model file on disk; only the serving check shows that
search is broken. Each finding includes the command to investigate it.

## Changing your mind later

To move search from the CPU to your own server, start the server and run
`virtues configure-inference --embed-url <URL>`. It keeps your index when
the new server runs the same model, and rebuilds it when the model differs.
To come back, run `sudo virtues configure-inference --recommended`: Virtues
starts its recommended model on the CPU, and search keeps using your server
until the index is rebuilt for it. Virtues never switches back on its own,
even when your server stops answering. The only part of this choice fixed at
install time is which services the installer set up, and on the CPU path that
is one ordinary systemd unit you can stop and disable.

On the CPU path, Virtues changes models for you. An update that recommends a
new model starts it beside the current one (`virtues-embed-next.service`,
on port 18183), and the indexer builds the new index from the text already
stored while search keeps answering from the old one. Settings → Search
shows the progress. When every chunk has a new vector, the two indexes swap
immediately, and the next update moves the new model onto the usual service
and removes the old one.
