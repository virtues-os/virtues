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
have a working server.

Two pages are worth reading first, and in this order:
[What to run it on](/docs/setup/requirements), because memory and disk decide
whether this is pleasant, and [Setting up inference](/docs/inference), because
on your own hardware the search models are yours to run and the installer
asks for their URLs before it does anything else.

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

- **Linux**, `x86_64` or `aarch64`, with systemd and root. Not macOS — the
  Mac app is a client that talks to a server, not a server itself.
- **Debian or Ubuntu**, where the installer drives `apt` and adds the PGDG
  repository if the distribution's own packages ship an older PostgreSQL than
  the one Virtues needs — or **Fedora**, where it drives `dnf` and takes
  PostgreSQL as shipped.
- **About 4 GB free on `/`**, and `curl`.
- **Outbound network** to `github.com` (releases) and `apt.postgresql.org`
  (PostgreSQL). The installer probes both before touching anything.

Before it installs anything it also measures the disk your record will live
on and tells you, with numbers, which tier you're on — an NVMe drive and a
microSD card produce very different servers, and the difference is worth
knowing in advance rather than discovering later.
[What to run it on](/docs/setup/requirements) has the full picture.

It checks whether ports `5432`, `8000`, `18181`, and `18182` are already
bound — Postgres, the server itself, and the two inference endpoints. A
warning there usually means you're reinstalling over an existing server, which
is fine.

## The first question is inference

The installer asks how you want to run the two models that make your record
searchable before it touches a package, a service, or a disk — so that a
broken endpoint costs you a prompt rather than a half-finished install. On
our own hardware there is no question to ask. On yours, you either point it
at endpoints you already run, or take the bundled CPU-only trial, which is
deliberately labeled as slow and not a deployment.

If you choose your own endpoints, have them running before you start.
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

The server comes up on port `8000`, and the machine is a Virtues server from
that moment — but it holds nothing yet. The next step is pairing a device,
which is what gives you a way in and starts the flow of your own data. Those
pages land as the setup flow settles; until then the installer's own output
is the guide, and `virtues --help` lists everything the CLI can do.

`virtues doctor` is the first thing to run if anything looks wrong. It
reports how inference resolved, whether both endpoints are actually serving,
and whether this server can be reached from away — each finding with the command
that diagnoses it. [The CLI](/docs/operate/cli) covers the rest.
