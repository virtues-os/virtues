---
title: Installing
description: Install Virtues OS on your own Linux machine with one command — what it needs, what it does, and how to audit it before you run it.
updated: 2026-08-28
---

One command, run on the machine that will become your server:

```bash
curl -sSL https://virtues.com/sh | sudo sh
```

That fetches a small shell script, which downloads the installer for your
architecture, verifies its checksum, and hands over. Ten minutes later you
have a working box.

## Read it before you run it

Piping a script into `sudo sh` deserves suspicion, so the script is
deliberately tiny and readable:

```bash
curl -sSL https://virtues.com/sh | less
```

It does five things: checks you're on Linux as root, resolves the newest
stable release, downloads the installer binary and its `.sha256` sidecar,
verifies the hash, and executes it. If the checksum can't be fetched or
doesn't match, it refuses to run the binary rather than falling back on
"HTTPS is probably fine" — a missing sidecar is a packaging bug, and this
code is about to run as root.

## What it needs

- **Linux**, `x86_64` or `aarch64`. Not macOS — the Mac app is a client that
  talks to a box, not a box itself.
- **Debian or Ubuntu.** The installer drives `apt`. On Ubuntu releases whose
  default repositories ship an older PostgreSQL, it adds the PGDG repository
  so it can install PG18.
- **Root**, and `curl`.
- **About 4 GB free on `/`.** The local embedding and reranking models are
  roughly half a gigabyte together, PostgreSQL another gigabyte, and the
  binaries and working room the rest.
- **Outbound network** to `github.com` (releases) and `apt.postgresql.org`
  (PostgreSQL). The installer probes both before touching anything.

It also checks whether ports `5432`, `8000`, `18181`, and `18182` are already
bound — Postgres, the server itself, and the two inference sidecars. A warning
there usually means you're reinstalling over an existing box, which is fine.

## Channels

The command above installs the newest **stable** release. To track
prereleases instead:

```bash
curl -sSL https://virtues.com/sh-pre | sudo sh
```

Or pin an exact version, stable or not:

```bash
curl -sSL https://virtues.com/sh | sudo VIRTUES_VERSION=vX.Y.Z sh
```

The channel you install on is remembered, so later upgrades follow the same
line without being asked again. See [Upgrading](/docs/operate/upgrading) for
how a box moves between releases and how to roll one back.

## After it finishes

The server comes up on port `8000`, and the machine is a Virtues box from
that moment — but it holds nothing yet. The next step is pairing a device,
which is what gives you a way in and starts the flow of your own data. Those
pages land as the setup flow settles; until then the installer's own output
is the guide, and `virtues --help` lists everything the CLI can do.
