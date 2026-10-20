---
title: Installing
description: Install Virtues OS on your own Linux machine with one command - what it needs, what it does, and how to audit it before you run it.
updated: 2026-10-20
---

One command, run on the machine that will become your server:

```bash
curl -sSL https://virtues.com/sh | sudo sh
```

That fetches a small shell script, which downloads the installer for your
architecture, verifies its checksum, and runs it. The install takes about ten
minutes.

Before installing, read these two pages:

1. [What to run it on](/docs/setup/requirements) for details on minimum
   specs and supported OS.
2. [Faster search on a GPU or NPU](/docs/setup/accelerators), if the machine
   has one. Search runs on the CPU unless you set one up, and the installer
   tells you when it finds one.

## Read it before you run it

You should check a script before piping it into `sudo sh`. This one is short
and readable:

```bash
curl -sSL https://virtues.com/sh | less
```

It does five things: checks you're on Linux as root, resolves the newest
stable release, downloads the installer binary and its `.sha256` sidecar,
verifies the hash, and executes it. If the checksum can't be fetched or
doesn't match, it refuses to run the binary. It doesn't fall back to trusting
HTTPS alone, because a missing checksum file means a packaging bug and the
binary would run as root.

## What it needs

- **Linux**, `x86_64` or `aarch64`, with systemd and root. macOS isn't
  supported; the Mac app is a client that connects to a server.
- **Debian or Ubuntu**, where the installer drives `apt` and adds the PGDG
  repository if the distribution's own packages ship an older PostgreSQL than
  the one Virtues needs - or **Fedora**, where it drives `dnf` and takes
  PostgreSQL as shipped.
- **About 4 GB free on `/`**, and `curl`.
- **Outbound network** to `github.com` (releases) and `apt.postgresql.org`
  (PostgreSQL). The installer probes both before touching anything.

Before it installs anything, it also measures the disk your record will be
stored on and tells you, with numbers, which tier it's in. A server on an NVMe
drive performs very differently from one on a microSD card.
[What to run it on](/docs/setup/requirements) has the full picture.

It checks whether ports `5432`, `8000` and `18181` are already in use, for
Postgres, the server itself and the embedding endpoint. A warning there
usually means you're reinstalling over an existing server, which is fine.

## The first question is inference

The installer asks how search should run before it touches a package, a
service or a disk, so a broken endpoint is caught before anything is
installed. On Virtues hardware this is already configured.

On yours, search runs on the server's CPU unless you choose otherwise:
Virtues installs the embedding model it recommends and keeps it current.
If the installer finds a GPU or NPU, it links to
[Faster search on a GPU or NPU](/docs/setup/accelerators), and you can set
that up at any time. Or point it at an embedding server you already run.

If you choose your own server, have it running before you start.
[Setting up inference](/docs/inference) gives the commands, the models, and
the contract those servers have to speak. To skip the prompt entirely on an
unattended install, set `VIRTUES_INFERENCE` (with `VIRTUES_EMBED_URL`) in the
environment.

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
how a server moves between releases and how to roll one back.

## After it finishes

The server starts on port `8000`. It has no data yet.

The next step is pairing a device, which starts bringing in your data.
[Reaching your server](/docs/operate/reach) covers pairing. The installer's
output also prints the first pairing code.

## Troubleshooting

If anything looks wrong during the install, run `virtues doctor` first. It
reports how inference was configured, whether the endpoints are actually
serving, and whether this server can be reached from outside your network.
Each finding includes the command to investigate it.
[The CLI](/docs/operate/cli) covers the other commands, and `virtues --help`
lists all of them.
