---
title: When something breaks
description: Diagnosing a Virtues server that won't start, won't answer, or is behaving strangely - services, logs, health commands, and where everything lives.
updated: 2026-10-20
---

Start here when the server isn't working properly. This guide is on the web
so you can read it when the server itself is down.

Almost everything below needs a terminal on the server, over SSH or at the
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

A restart often fixes the problem:

```bash
sudo systemctl restart virtues
```

## What the server thinks of itself

```bash
virtues status
```

This shows identity, subscription and paired devices on one screen. To send
someone the full details, `virtues status --json` prints the same information
in a stable format that's easy to paste.

```bash
virtues doctor
```

Reports how the inference stack resolved on this hardware - which accelerator
was found, whether this build links CUDA, and whether each model is present or
would need downloading. It doesn't touch the database, so it still answers
when other things are broken.

These commands read the database, which belongs to the `virtues` service
user. You can run them as yourself: they restart as the right user and print
a line to say so.

## When the server crashes

If the server exits abnormally, it sends us the exit status and the last 50
lines of the journal, so we can fix the crash without you having to report
it. Nothing else on a running
server is sent to us unprompted, apart from checking for updates.

To turn the crash report off, set it in the server's configuration and restart:

```bash
sudo sh -c 'echo VIRTUES_DIAG=off >> /var/lib/virtues/virtues.env'
sudo systemctl restart virtues
```

Either way, the full log stays in the journal on your machine:

```bash
sudo journalctl -u virtues -n 200 --no-pager
```

## The pieces

Besides the main service, a server runs an inference sidecar. On hardware
with an NPU, a single daemon replaces it:

| Unit | What it is |
|---|---|
| `virtues` | the server itself, on port 8000 |
| `virtues-embed` | embedding model, on local port 18181 |
| `virtues-embed-next` | only while search moves to a new model, on local port 18183 |
| `virtues-qnnd` | on NPU hardware, replaces the sidecar and serves ports 18181 and 18182 |
| `virtues-display` | the on-server screen, if your server has one |

If search returns nothing or feels broken while the server is otherwise healthy,
suspect a sidecar:

```bash
systemctl status virtues-embed
journalctl -u virtues-embed -n 50
```

**If the on-server screen shows an old version of the interface after an
upgrade**, the kiosk is showing a cached copy. Restart it:

```bash
sudo systemctl restart virtues-display
```

## Where things live

| What | Path |
|---|---|
| Everything the server owns | `/var/lib/virtues` |
| Configuration and secrets | `/var/lib/virtues/virtues.env` |
| Your files and recordings | `/var/lib/virtues/lake` |
| Models | `/var/lib/virtues/models` |
| Backups | `/var/lib/virtues/backups` |
| Release channel | `/var/lib/virtues/channel` |
| The binary | `/usr/local/bin/virtues` |

The binary is a symlink into the currently active release, which is what lets
an upgrade swap versions atomically and roll back with one flip.

## An upgrade went wrong

A failure before an upgrade switches releases leaves the server unchanged,
and a failure after the switch rolls straight back. If the release you're on
isn't working properly:

```bash
sudo virtues rollback
```

That returns the binary, the web app, and the actions runtime together. The
database is not rolled back - migrations only move forward, and the previous
release tolerates a newer schema. [Upgrading](/docs/operate/upgrading) has the
full model.

## You can't reach the server

If the server is healthy but your phone or laptop can't reach it, see
[Reaching your server](/docs/operate/reach). In short: check that
the device is still on the allowlist with `virtues device ls`, and re-pair
with `virtues pair` if it isn't.

## Search results are wrong or empty

If the server reports a model fingerprint or dimension mismatch - usually after
changing models - the index was built by a different model than the one now
answering:

```bash
virtues configure-inference
```

To rebuild the index from your source data:

```bash
virtues reindex
```

Neither command changes your data. The index is derived from it, so
rebuilding is always safe, just slow.

## Starting over

`virtues restore` replaces the server's state from a backup. It overwrites
your current data and there's no dry run, so read
[Backup & restore](/docs/operate/backup-and-restore) before you use it,
especially the part about the recovery key, which is shown only once.

To remove Virtues from the machine entirely, `sudo virtues uninstall` prints
everything it found before touching any of it and asks you to type the server's
hostname to confirm.
