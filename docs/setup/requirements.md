---
title: What to run it on
description: The machine Virtues needs — OS, memory, disk, and the accelerator question. What actually decides whether it feels fast, and which corners are safe to cut.
updated: 2026-09-03
---

The short answer: a Linux machine with 8 GB of RAM, a real SSD, and an
ordinary internet connection. **No GPU is required.** The rest of this page is
what each of those is for, so you can tell which corner you're cutting when
you cut one.

## The machine

- **Linux**, `x86_64` or `aarch64`, with **systemd** and root.
- **Debian or Ubuntu**, or **Fedora**. The installer drives `apt` on the
  first two — adding the PGDG repository where the distribution's own
  packages ship an older PostgreSQL than the one Virtues needs — and `dnf` on
  Fedora, whose PostgreSQL is recent enough as shipped.
- **Not macOS or Windows.** Installing needs root, a native PostgreSQL
  cluster, and systemd units of its own. The Mac and iPhone apps are
  *clients* that talk to a server; neither is one.
- A **virtual machine is fine**. A Docker or LXC container generally is not:
  the installer writes systemd units and expects to own the machine's
  services.

An old laptop, a NUC-class mini PC, a rack server you already have, or a
capable single-board computer all qualify. What separates a pleasant server from
a miserable one is almost entirely memory and disk, in that order.

## Memory

Eight gigabytes is the floor, and here is where it goes. PostgreSQL and the
Virtues server itself want a couple of gigabytes between them. The two model
servers — the embedder and the reranker, if you run them on this machine —
sit at roughly a gigabyte of resident memory *each* with the flags Virtues
starts them under: one request slot, no prompt cache, a 2048-token context.
Started with a model server's own defaults they would take about two and a
half gigabytes each, which is the difference between fitting and swapping on
an 8 GB board.

If you run the models on a different machine — the recommended arrangement,
and the subject of [Setting up inference](/docs/inference) — then 8 GB here is
roomy rather than tight.

## Disk

Installing wants **about 4 GB free on `/`**: PostgreSQL, the binaries, the
local models if they live here, and working room. After that the record grows
for as long as you feed it, in two places under `/var/lib/virtues` — the
PostgreSQL cluster and the file store that holds raw attachments, recordings,
and documents.

**The medium matters more than the capacity.** Vector search is random-read
heavy and the write-ahead log is fsync heavy, so storage is the single
loudest determinant of whether searching your own life feels instant. The
installer classifies and measures the disk under the data directory *before*
it provisions anything, and tells you which tier you're on:

| What it finds | What it says |
|---|---|
| NVMe, or a SATA/other SSD | Great — this is the intended case |
| eMMC | Workable to around a hundred thousand items; large index builds are slow and the flash wears |
| microSD | Searches will feel slow and the card will wear out under database load |
| USB-attached | Works, but some USB bridges don't honor cache flushes, which risks corruption on power loss |
| Spinning disk | Expect multi-second searches |
| NFS or SMB | Don't. PostgreSQL on a network filesystem is a known corruption risk |

It also times `fsync` and says so when the number is physically impossible
for the medium — a disk that acknowledges flushes without performing them
will lose data on a power cut, and that is worth knowing before your record
is on it rather than after.

None of these warnings block the install; they inform it. `virtues doctor`
re-reports storage later. To put the data somewhere other than
`/var/lib/virtues`, set `DATA_DIR` when you run the installer, and point it
at a **local** disk.

## The accelerator question

This is where people overbuy, so it's worth being precise about which models
run where.

**The model that writes does not run on your server.** Composing an account of
your day, answering a question, transcribing a recording — that work goes to
a model provider, through our gateway by default or through an endpoint you
choose. So there is no VRAM budget to plan for it.

**What runs locally is retrieval:** one embedding model, which turns your
record into vectors, and one reranker, which re-scores search results for
precision. Both are small, and they want *opposite* hardware:

- **Embedding is CPU-friendly.** The model Virtues ships has fp32
  activations, so fp16 GPU paths fall back to fp32 and come out slower than
  the CPU. The unit Virtues installs runs the embedder with GPU offload
  explicitly disabled, on purpose.
- **Reranking is the half that gains from a GPU.** The installed unit offloads
  it fully, and on hardware with a usable GPU backend it is markedly faster
  there than on CPU.

**An NPU is only useful if its vendor ships a server.** llama.cpp supports
essentially no NPUs today — its Hexagon backend is a newer-generation,
Android-only affair — so neural accelerators reach Virtues by speaking the
two HTTP contracts from behind a vendor's own runtime, which is exactly what
bring-your-own inference is for.

**The number to care about is embedding latency.** The installer measures the
p50 of your endpoint and grades it: under 100 ms and searches feel instant, up
to 400 ms and they feel a little slow, past that every search in the product
waits on it. That measurement, not a spec sheet, is the answer to "is this
machine fast enough".

## Network

Outbound only. Virtues opens **no inbound port** and needs no forwarding
rule; a paired device reaches the server by key, over paths described in
[Reaching your server](/docs/operate/reach). During the install it needs to
reach `github.com` for the release and `apt.postgresql.org` for PostgreSQL,
and it probes both before touching anything.

Locally it binds four ports: `8000` for the server, `5432` for PostgreSQL,
and `18181`/`18182` for the embedding and rerank endpoints when those run on
the same machine. The installer warns if something already holds them.

## What we actually test

`x86_64` Debian and Ubuntu servers, and our own `aarch64` board. Everything
else is yours to measure — which is a real answer rather than a dodge,
because the tools to measure it ship with the installer: the storage verdict
and the embedding-latency verdict during setup, then `virtues doctor` on the
running server.

Next: [Setting up inference](/docs/inference), which is the one piece worth
standing up *before* you install, and then
[Installing](/docs/setup/install).
