---
title: When something breaks
description: Diagnosing a Virtues box that won't start, won't answer, or is behaving strangely — services, logs, health commands, and where everything lives.
updated: 2026-08-28
---

Start here when the box is misbehaving. You're reading this on the web rather
than on the box for a reason: the machine that's broken is the worst place to
keep its own troubleshooting guide.

Almost everything below needs a terminal on the box, over SSH or at the
keyboard.

## Is it running?

```bash
sudo systemctl status virtues
```

The server runs as a systemd unit called `virtues`, restarts itself on
failure, and waits for PostgreSQL before starting. If it's in a restart loop,
the reason is in the log:

```bash
sudo journalctl -u virtues -n 100 --no-pager
```

Everything logs to the journal. There is no Virtues log file to hunt for.

A restart fixes more than it should:

```bash
sudo systemctl restart virtues
```

## What the box thinks of itself

```bash
virtues status
```

Identity, subscription, and paired devices in one screen. When you want to
hand someone the whole picture rather than describe it, `virtues status --json`
prints the same thing in a stable form that's easy to paste.

```bash
virtues doctor
```

Reports how the inference stack resolved on this hardware — which accelerator
was found, whether this build links CUDA, and whether each model is present or
would need downloading. It doesn't touch the database, so it still answers
when other things are broken.

These commands read the database, and the database belongs to the `virtues`
service user. You don't have to think about that: run them as yourself and
they re-launch themselves as the right user, printing a line to say so.

## The pieces

Beyond the main service, a box runs the inference sidecars — and on hardware
with an NPU, one daemon replaces both:

| Unit | What it is |
|---|---|
| `virtues` | the server itself, on port 8000 |
| `virtues-embed` | embedding model, on local port 18181 |
| `virtues-rerank` | reranking model, on local port 18182 |
| `virtues-qnnd` | on NPU hardware, replaces both sidecars and serves both ports |
| `virtues-display` | the on-box screen, if your box has one |

If search returns nothing or feels broken while the box is otherwise healthy,
suspect a sidecar:

```bash
systemctl status virtues-embed
journalctl -u virtues-embed -n 50
```

**If the on-box screen shows an old version of the interface after an
upgrade**, it's the kiosk holding a cached copy rather than anything deeper:

```bash
sudo systemctl restart virtues-display
```

## Where things live

| What | Path |
|---|---|
| Everything the box owns | `/var/lib/virtues` |
| Configuration and secrets | `/var/lib/virtues/virtues.env` |
| Your files and recordings | `/var/lib/virtues/lake` |
| Models | `/var/lib/virtues/models` |
| Backups | `/var/lib/virtues/backups` |
| Release channel | `/var/lib/virtues/channel` |
| The binary | `/usr/local/bin/virtues` |

The binary is a symlink into the currently active release, which is what lets
an upgrade swap versions atomically and roll back with one flip.

## An upgrade went wrong

The upgrade path is built so that failures before the switch leave the box
untouched, and failures after it flip straight back. If you're on a release
that's misbehaving:

```bash
sudo virtues rollback
```

That returns the binary, the web app, and the actions runtime together. The
database is not rolled back — migrations only move forward, and the previous
release tolerates a newer schema. [Upgrading](/docs/operate/upgrading) has the
full model.

## You can't reach the box

If the box is healthy but your phone or laptop can't get to it, that's a
different problem with its own page —
see [Reaching your server](/docs/operate/reach). The short version: check that
the device is still on the allowlist with `virtues device ls`, and re-pair
with `virtues pair` if it isn't.

## Search results are wrong or empty

If the box reports a model fingerprint or dimension mismatch — usually after
changing models — the index was built by a different model than the one now
answering:

```bash
virtues configure-inference
```

To rebuild the index from your source data:

```bash
virtues reindex
```

Your data isn't touched by either. The index is derived, so rebuilding it is
recoverable by definition, just slow.

## Starting over

`virtues restore` replaces the box's state from a backup. It's destructive and
there's no dry run, so read [Backup & restore](/docs/operate/backup-and-restore)
before reaching for it — particularly the part about needing the key you were
shown once.

To remove Virtues from the machine entirely, `sudo virtues uninstall` prints
everything it found before touching any of it and asks you to type the box's
hostname to confirm.
