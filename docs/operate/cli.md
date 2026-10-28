---
title: The CLI
description: The virtues command - checking the server's health, pairing devices, moving between releases, and the maintenance verbs you'll actually use.
updated: 2026-10-20
---

Installing puts a `virtues` binary on the server. It's the same program that
serves the web app, and it's how you do anything that needs to happen on the
machine itself rather than through a paired device.

`virtues --help` lists every command. This page covers the ones you're
likely to use. The rest are used internally by the service or by people
working on Virtues itself.

## Seeing what's going on

```bash
virtues status
```

Server health in one screen: identity, subscription, and which devices are
paired. When something is wrong and you want to send someone the full
details, `virtues status --json` prints the same information in a stable
machine-readable format.

```bash
virtues doctor
```

Reports how the inference stack resolved on this hardware: which accelerator
was detected, whether the build links CUDA, and whether each model is baked in
or would be downloaded. It doesn't use the database, so it can tell you
whether the machine is set up correctly even when other things are broken.

## Devices

To connect a new device to your server, pair it:

```bash
virtues pair
```

It prints a code to type into the app, then waits. On a server that's already
yours, each code is new, works once, and expires after thirty minutes.
Running the command again creates a new code instead of reprinting the last
one, so use the code from the most recent run. (Only an unclaimed server, during setup, shows a
standing code that pairs more than one device.) `login` and `link` still work
as aliases.

The server keeps an allowlist of devices that can reach it, and that
allowlist is the only authentication:

```bash
virtues device ls          # who can reach this server
virtues device add         # print a pair code for a new one
virtues device rm <id>     # revoke one — its next connection is refused
```

## Approving sensitive actions

A few actions are sensitive enough that a paired device can't do them alone:
exporting all your data, swapping the key for a bring-your-own AI provider,
wiping the server, revoking the last device other than itself, and importing an
applet package. Asking for one from the app raises a request that you approve
at the machine:

```bash
virtues sudo
```

With no arguments, it lists open requests and asks you about each one. This
requires physical access to the server: someone who has stolen your laptop
can't approve a request remotely, but you can approve it at the server.

## Releases

```bash
sudo virtues upgrade       # move to the newest release on your channel
sudo virtues rollback      # flip back to the previous one
virtues channel            # print the channel this server follows
virtues channel pre        # follow prereleases from now on
```

Setting the channel saves it. `--pre` applies to a single upgrade only, so a
server meant to follow prereleases goes back to stable the first time someone
runs a plain `virtues upgrade`. Set the channel once instead. [Upgrading](/docs/operate/upgrading) covers the whole
model, including pinning a version.

To split the work, doing the slow download now and installing later,
`virtues prepare` stages and verifies a release without touching the running
server, and `virtues activate` installs what it staged.

## Data

```bash
sudo virtues backup     # snapshot the server into one tarball
sudo virtues restore <tarball>     # replace this server's state from one
virtues volumes ls                 # registered backup destinations
```

The backup contains the encryption key needed to read its own contents, which
makes the tarball **as sensitive as the server itself**. See
[Backup & restore](/docs/operate/backup-and-restore) before you rely on any of
this.

## Maintenance

```bash
virtues reindex
```

Rebuilds the derived search index from your source data with the current
model. It changes only the index, never your data, so it's safe to run when
the index is stale or doesn't match the model.

```bash
virtues configure-inference
virtues configure-inference --embed-url http://127.0.0.1:8080
sudo virtues configure-inference --recommended
```

With `--embed-url`, the command moves search to another embedding server,
such as llama.cpp on your GPU
([GPU and NPU acceleration](/docs/setup/accelerators)). It keeps your
index when the new server runs the same model, and re-embeds from source when
it doesn't. After that, you manage the model and updates don't change it.

With `--recommended`, it switches back: Virtues runs the model it recommends
on the server's CPU and keeps it current, and search keeps using your server
until the index is rebuilt for the new model.

With no options, it re-checks the current server after you change models and
offers to re-embed. Run this form when the server reports a fingerprint or
dimension mismatch.

## Leaving

```bash
sudo virtues uninstall
```

Prints the exact manifest of everything it found before touching anything, and
asks you to type the server's hostname to confirm. Shared infrastructure it
didn't install - the PostgreSQL server, Avahi - is left alone.
