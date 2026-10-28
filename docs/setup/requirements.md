---
title: System requirements
description: What your server needs to run Virtues - operating system, RAM, storage, GPU and network - and what affects how fast it feels.
updated: 2026-10-26
---

Each requirement includes the command to check it. Run them on the machine
that will become your server.

## Hardware

- **8 GB of RAM.** To check, run `free -h` and read the `total` column of the
  `Mem` row. A machine with 8 GB shows a little under 8, such as `7.6Gi`
  or `7.7Gi`, because the system reserves some RAM. That's enough.
- **4 GB of free disk space** to install, then more as your data grows. An SSD
  is strongly recommended. To check, run `df -h /` and read the `Avail`
  column.
- **An `x86_64` or `aarch64` processor.** To check, run `uname -m`.
- **No GPU is required.** To make search faster with a GPU or NPU, follow
  [GPU and NPU acceleration](/docs/setup/accelerators).

## Operating system

- **Ubuntu 22.04 or later.** To check, run `cat /etc/os-release` and read the
  `VERSION` line. Ubuntu 26.04 hasn't been tested yet.
- **systemd, the Linux service manager.** Standard Ubuntu installs include it.
  To check, run `systemctl --version`. If it prints a version number, you
  have it.
- **A virtual machine works.** A Docker or LXC container usually doesn't,
  because the installer sets up system services of its own and needs to
  manage the machine's services.

Any of these can be your server: an old laptop, a mini PC, a rack server you
already have, or a single-board computer with enough RAM. Your server's
speed depends mostly on its RAM, then on its storage.

## Devices you connect with

To use Virtues, open the app on one of these devices. For the first pairing,
your device must be on the same network as your server.

- **A Mac with macOS 26.2 or later**, with the Virtues desktop app.
- **A Windows PC**, with the Virtues desktop app.
- **An iPhone with iOS 26.2 or later**, with the Virtues app.

A web browser on another computer can't connect to your server; use the app
instead.

[Connecting your devices](/docs/setup/devices) explains how to pair each
one.

## Tested hardware

We test on `x86_64` Ubuntu servers and on the `aarch64` Radxa
Dragon. To measure other hardware, check the storage and search-speed grades
the installer shows, then run `virtues doctor` once your server is running.

## Next steps

When your machine meets these requirements, continue with
[Install script](/docs/setup/install).

---

## How Virtues uses RAM

| Process | RAM |
|---|---|
| PostgreSQL and the Virtues server | About 2 GB together |
| Embedding model server, on the same machine | About 1 GB |
| A second embedding model server, while search switches to a new model | About 1 GB more, until the switch finishes |

The embedding model server stays at about 1 GB because of the flags Virtues
starts it under: one request slot, no prompt cache, a 2048-token context. With
a model server's own defaults, it would use about 2.5 GB, which can push an
8 GB server into swap.

If your embedding server runs on another computer, your Virtues server needs
about 1 GB less RAM, so the suggested minimum of 8 GB leaves plenty to
spare.

## How Virtues uses the network

- **Outgoing connections only.** Virtues doesn't need any incoming port, so
  there's no port forwarding to set up on your router. Your paired devices
  reach your server by its key, as [Reaching your server](/docs/operate/reach)
  explains.
- **Two sites during the install.** The installer downloads the release from
  `github.com` and PostgreSQL from `apt.postgresql.org`, and checks that it
  can reach both before it changes anything. On a network that only allows
  sites by name, both need to be allowed.

## How search uses the GPU

- **The model that writes your day pages and answers your questions doesn't
  run on your server,** so it needs no GPU. It runs in the cloud, through our
  gateway, which asks every model provider for zero data retention, except for
  models marked as not zero retention. If you bring your own AI key, your
  provider's own terms apply.
- **The CPU engine doesn't offload to the GPU.** The model's math overflows
  16-bit arithmetic, which some GPU paths use. If you set up your own GPU
  server, Virtues checks it when you connect it.
- **Search doesn't use a reranker**, a second model that re-sorts results. The
  small rerankers we tested sorted results worse than search alone, so search
  runs one model.
- **Search speed depends on how fast your embedding server answers.** The
  installer measures the median (p50) latency of your endpoint and grades it:
  under 100 ms, searches feel instant. Up to 400 ms, they feel a little slow.
  Above that, every search has a noticeable wait. That measurement says more
  about a machine's search speed than its spec sheet does.

## How Virtues uses storage

- **Installing** uses about 4 GB on `/`, for PostgreSQL, the Virtues programs,
  the search model and working space.
- **After that,** your server uses more storage as data comes in. Virtues
  stores it in two places under `/var/lib/virtues`: the PostgreSQL database,
  and a file store for attachments, recordings and documents.

**The type of disk matters more than its size.** Vector search does a lot of
random reads and the write-ahead log does a lot of fsyncs, so the disk is the
biggest factor in how fast search feels. Before it installs anything, the
installer identifies and measures the disk that holds the data directory, and
tells you which type it is:

| Disk type | What to expect |
|---|---|
| NVMe or SATA SSD | The best choice. Virtues is designed for it. |
| eMMC | Works up to about 100,000 items. Building a large search index is slow, and the storage wears out over time. |
| microSD | Search feels slow, and the card wears out under the database's constant writes. |
| USB-attached disk | Works, but some USB adapters report data as saved before it is, so a power cut can corrupt it. |
| Hard disk | Each search takes several seconds. |
| NFS or SMB network share | Unsafe. PostgreSQL on a network share can corrupt your data. |

Note: some disks confirm a write before the data is physically saved, so a
power cut can still lose it. The installer times `fsync` to catch this, and
warns you when a disk's numbers are impossible for its type.

None of these warnings stop the install, and `virtues doctor` reports on
storage again later. The data location comes from `DATA_DIR` at install time,
and defaults to `/var/lib/virtues`.
