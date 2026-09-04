---
title: The CLI
description: The virtues command — checking the server's health, pairing devices, moving between releases, and the maintenance verbs you'll actually use.
updated: 2026-08-28
---

Installing puts a `virtues` binary on the server. It's the same program that
serves the web app, and it's how you do anything that needs to happen on the
machine itself rather than through a paired device.

`virtues --help` lists everything. This page covers the parts an owner
actually reaches for; the rest is either service-internal or for people
working on Virtues itself.

## Seeing what's going on

```bash
virtues status
```

Server health in one screen: identity, subscription, and which devices are
paired. When something is wrong and you want to hand someone the complete
picture, `virtues status --json` prints the same thing in a stable
machine-readable form — the boring-but-complete diagnostic.

```bash
virtues doctor
```

Reports how the inference stack resolved on this hardware: which accelerator
was detected, whether the build links CUDA, and whether each model is baked in
or would be downloaded. It touches no database, so it answers the "is this
machine set up right" question even when other things are broken.

## Devices

Pairing a device is the one human verb for connecting something to your server:

```bash
virtues pair
```

It prints a code to type into the app, then waits. On a server that's already
yours each code is fresh, single-use, and good for thirty minutes — running
this again mints a new one rather than reprinting the last, so use the code
from the run you're looking at. (Only an unclaimed server, during setup, shows a
standing code that pairs more than one device.) `login` and `link` still work
as aliases.

The devices allowed to reach your server are an explicit allowlist, and that
allowlist *is* the authentication boundary:

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

With no arguments it lists open requests and prompts for each. The point is
physical access — someone who has your laptop can't approve from wherever they
are, and you can, by sitting down at the server.

## Releases

```bash
sudo virtues upgrade       # move to the newest release on your channel
sudo virtues rollback      # flip back to the previous one
virtues channel            # print the channel this server follows
virtues channel pre        # follow prereleases from now on
```

Setting the channel persists it, which matters more than it sounds: `--pre` is
a one-off override that forgets itself, so a server meant to track staging drifts
back to stable the first time anyone types a bare `virtues upgrade`. Set the
channel once instead. [Upgrading](/docs/operate/upgrading) covers the whole
model, including pinning a version.

If you'd rather split the work — do the slow download now, install later —
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
model. Your data isn't touched — only the index built from it — so this is the
recovery path for a stale or mismatched index rather than something to fear.

```bash
virtues configure-inference
```

Re-validates the embedding endpoint after you change models, and offers to
re-embed from source. Run it when the server reports a fingerprint or dimension
mismatch.

## Leaving

```bash
sudo virtues uninstall
```

Prints the exact manifest of everything it found before touching anything, and
asks you to type the server's hostname to confirm. Shared infrastructure it
didn't install — the PostgreSQL server, Avahi — is left alone.
