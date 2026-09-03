---
title: Upgrading
description: How a Virtues server moves between releases — channels, the atomic upgrade, dry runs, and rollback.
updated: 2026-08-27
---

Upgrades are one command, run on the server:

```bash
sudo virtues upgrade
```

It needs root — the upgrade replaces the installed binary and restarts the
service — and it's deliberately safe to run on a server you care about: the new
release is downloaded and staged completely before anything changes, then
activated in one atomic flip. The binary, the web app, and the actions runtime
move together, so you can't end up with a UI newer than the server underneath
it. A failure before the flip leaves the server exactly as it was; a failure
after the flip rolls straight back.

## Channels

There are two release channels:

- **Stable** — `vX.Y.Z` releases. What a server should be on unless you've
  chosen otherwise, and the default for every command below.
- **Prerelease** — the newest staging build, versioned like
  `vX.Y.Z-staging.N`. Explicit opt-in via `--pre`. If you've heard it called
  *edge* or *nightly*, this is the same thing.

Your server remembers its channel, so a plain `sudo virtues upgrade` keeps
following whichever line it's on. `--pre` is a one-off override, not a
permanent switch.

## Checking before you leap

```bash
sudo virtues upgrade --check
```

reports what's available without changing anything. Add `--pre` to check the
prerelease line. This is the dry run — it tells you the version you'd get and
whether the server considers itself ready to take it.

## Pinning a version

```bash
sudo virtues upgrade --version v0.1.4
```

installs a specific tag instead of the newest release. Resolution order is
explicit: `--version` wins over `--pre`, which wins over the stored channel,
which defaults to stable.

## Rolling back

```bash
sudo virtues rollback
```

flips back to the previous release — a pure switch and service restart, with
no database surgery. Releases are kept in slots on disk precisely so the last
known-good one is still there when you want it.

## If upgrade says your install is too old

Very old installs predate the slot layout the upgrader expects. The command
will tell you, and the fix is the installer, which migrates in place:

```bash
curl -sSL https://virtues.com/sh | sudo sh
```
