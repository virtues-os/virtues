---
title: What to run it on
description: The machine Virtues needs - OS, memory, disk, and whether you need a GPU. What determines whether it feels fast, and where you can save money.
updated: 2026-10-20
---

You need a Linux machine with 8 GB of RAM, an SSD, and an ordinary internet
connection. **No GPU is required.**

## The machine

- **Linux**, `x86_64` or `aarch64`, with **systemd** and root.
- **Debian or Ubuntu**, or **Fedora**. The installer drives `apt` on the
  first two - adding the PGDG repository where the distribution's own
  packages ship an older PostgreSQL than the one Virtues needs - and `dnf` on
  Fedora, whose PostgreSQL is recent enough as shipped.
- **Not macOS or Windows.** Installing needs root, a native PostgreSQL
  cluster, and systemd units of its own. The Mac and iPhone apps are
  *clients* that talk to a server; neither is one.
- A **virtual machine is fine**. A Docker or LXC container generally is not:
  the installer writes systemd units and expects to own the machine's
  services.

An old laptop, a NUC-class mini PC, a rack server you already have, or a
capable single-board computer all work. How well the server performs depends
almost entirely on memory and disk, in that order.

## Memory

Eight gigabytes is the minimum. PostgreSQL and the Virtues server itself use
a couple of gigabytes between them. The embedding
model server, when it runs on this machine, sits at roughly a gigabyte of
resident memory with the flags Virtues starts it under: one request slot, no
prompt cache, a 2048-token context. With a model server's own defaults it
would use about two and a half gigabytes, which on an 8 GB board can push the
machine into swap. While search moves to a new model, the old and new servers
run side by side for a while, so plan for an extra gigabyte during that time.

If you run the model on a different machine - see
[Setting up inference](/docs/inference) - then 8 GB on this machine leaves
plenty of room.

## Disk

Installing needs **about 4 GB free on `/`** for PostgreSQL, the binaries, the
local models if they're stored here, and working space. After that, the record
keeps growing as data comes in, in two places under `/var/lib/virtues`: the
PostgreSQL cluster, and the file store for raw attachments, recordings and
documents.

**The type of disk matters more than its size.** Vector search does a lot of
random reads and the write-ahead log does a lot of fsyncs, so the disk is the
biggest factor in how fast search feels. Before it installs anything, the
installer identifies and measures the disk under the data directory and tells
you which tier it's in:

| What it finds | What it says |
|---|---|
| NVMe, or a SATA/other SSD | Great - this is the intended case |
| eMMC | Workable to around a hundred thousand items; large index builds are slow and the flash wears |
| microSD | Searches will feel slow and the card will wear out under database load |
| USB-attached | Works, but some USB bridges don't honor cache flushes, which risks corruption on power loss |
| Spinning disk | Expect multi-second searches |
| NFS or SMB | Don't. PostgreSQL on a network filesystem is a known corruption risk |

Some disks confirm a write before the data is physically saved, so a power
cut can still lose it. The installer times `fsync` to catch this and
flags a disk whose numbers are impossible for its medium.

None of these warnings stop the install. `virtues doctor`
re-reports storage later. To put the data somewhere other than
`/var/lib/virtues`, set `DATA_DIR` when you run the installer, and point it
at a **local** disk.

## Do you need a GPU?

Most people don't. This section lists which models run where.

**The model that writes does not run on your server.** Composing an account of
your day, answering a question, transcribing a recording - that work goes to
a model provider, through our gateway by default or through an endpoint you
choose. You don't need GPU memory for it.

**The only local model is for search:** one embedding model, which turns your
record into vectors. It runs on the CPU by default, and for everyday use that
is enough. A GPU or NPU can make every search and the first index of your
history faster, so if the machine has one, consider setting it up after
installing:
[Faster search on a GPU or NPU](/docs/setup/accelerators).

- **The CPU engine doesn't offload to the GPU.** The model's math overflows
  16-bit arithmetic, which some GPU paths use. A GPU server you set
  up yourself gets checked when you point Virtues at it.
- **Search doesn't use a reranker.** The small rerankers we tested ranked
  results worse than search without them, so there is no second model to plan
  hardware for.

**Most NPUs need their vendor's runtime.** They reach Virtues through any
server that offers the embedding contract, which is what bring-your-own
inference is for. On a Radxa Dragon, Virtues runs search on the NPU itself.

**Embedding latency is the number that matters.** The installer measures the
median (p50) latency of your endpoint and grades it: under 100 ms, searches
feel instant; up to 400 ms, they feel a little slow; above that, every search
waits on it noticeably. Use that measurement, rather than a spec sheet, to
decide whether the machine is fast enough.

## Network

Virtues only makes outbound connections. It doesn't open **any inbound port**
and needs no forwarding rule; a paired device reaches the server by key, over paths described in
[Reaching your server](/docs/operate/reach). During the install it needs to
reach `github.com` for the release and `apt.postgresql.org` for PostgreSQL,
and it probes both before touching anything.

Locally it binds three ports: `8000` for the server, `5432` for PostgreSQL,
and `18181` for the embedding endpoint when that runs on the same machine.
The installer warns if something is already using them.

## What we test on

We test on `x86_64` Debian and Ubuntu servers and on our own `aarch64` board.
On other hardware, measure it yourself with the tools the installer includes:
the storage and embedding-latency grades during setup, then `virtues doctor`
on the running server.

Next: [Setting up inference](/docs/inference), the one part to set up
*before* you install if you run your own embedding server, and then
[Installing](/docs/setup/install).
