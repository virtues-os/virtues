---
title: Backup & restore
description: How to protect a Virtues server — minting the recovery key the server will never hold, backing up to a drive, verifying archives, and restoring.
updated: 2026-08-28
---

Your server holds things that exist nowhere else. This page is how you keep them
when the hardware doesn't.

Read the first section before the others. Backups here are encrypted to a key
the server deliberately cannot keep, so the order of operations matters more than
usual: get the key wrong and every archive you ever make is unreadable.

## First: mint the recovery key

Backups are encrypted with [age](https://age-encryption.org), and your server
stores only the *public* half — enough to write an archive, never enough to
read one back. Create the keypair once:

```bash
sudo virtues backup --init-key
```

This prints the secret key **once**, in a banner, and writes it nowhere. That
is the whole design: an attacker who takes the server, and Virtues as a company,
are equally unable to decrypt your archives. It also means the consequence
lands entirely on you.

**Put the key somewhere it will outlive the server, and make more than one copy.**
A password manager, a printout in a drawer, a note in a safe — the failure to
plan for is a house fire or a dropped laptop, not a burglar. There is no
escrow, no "email me a reset", and no support path that recovers it. Losing
this key turns every backup you own into noise.

Until you run that command, `virtues backup` refuses to do anything. It used
to mint a key automatically, which meant people ended up with archives whose
key had scrolled off a terminal months earlier.

## Making a backup

```bash
sudo virtues backup
```

Writes one self-contained encrypted archive to
`/var/lib/virtues/backups/`, named for the moment it was taken and ending
`.tar.gz.age`. Use `--output <path>` to put it somewhere else — an external
drive you mounted by hand, for instance — and `--force` to overwrite an
existing file.

### What's inside

- The **database**, as a full dump. Your server's identity lives in here too —
  the network key that *is* this server, the certificate authority, the list of
  paired devices — so the archive carries who your server is as well as what it
  knows.
- The **environment file**, which holds the encryption key that makes the
  database's stored credentials readable. Without it a dump restores into
  gibberish, which is why a backup refuses to run when it can't find one.
- The **data lake** — recordings, uploads, files.
- **Authored applets**, the ones written on the server rather than shipped with it.
- A **manifest** recording the binary and schema versions, plus a SHA-256 for
  every member, which is what makes verification and restore able to detect
  a damaged archive.

Because the environment file rides along, **the archive is exactly as
sensitive as the server itself.** Treat a backup tarball the way you'd treat the
machine.

Deliberately *not* included: the downloaded model files, which are large and
freely re-fetchable, and a handful of machine-local scraps like the saved
Wi-Fi passphrase and the release channel. A restored server re-downloads and
re-derives those.

## Backing up to a drive

Plug in an external disk, mount it, and register it once:

```bash
sudo virtues volumes add /path/to/mount --name "study drive"
```

The drive is remembered by its filesystem UUID rather than its mount path, so
it's still recognized after a reboot moves it. Nothing outside the server's own
subdirectory is ever read or written, so the disk stays usable for whatever
else lives on it.

```bash
virtues volumes ls                              # what's registered, and how fresh
sudo virtues backup --volume all     # write to every attached drive
```

**This already runs on its own.** A nightly job at 04:00 backs up to every
registered drive that happens to be attached; drives that aren't plugged in
are skipped quietly rather than failing. Registering a drive is, in practice,
the entire setup.

On a drive the archive is split: a full snapshot of the database, environment
file and applets, plus separate incremental archives of the lake. Old full
snapshots are pruned only when the drive starts filling up, and the newest is
never removed — on a roomy disk you keep a run of them. Lake increments are
never pruned at all, because the lake is the part that only ever grows and
can't be re-derived. A run refuses to start if it would leave under a gigabyte
free.

## Checking that a backup is real

An unverified backup is a hope, not a plan:

```bash
virtues backup --verify /path/to/archive.tar.gz.age --key-file /path/to/key
```

This decrypts the archive, extracts it, and re-hashes every file against the
manifest. It writes nothing and doesn't need a working database, so it's safe
to run against an old archive on a different machine — which is also the way
to prove your saved key actually works before you need it to.

Do this by hand from time to time. Nothing verifies archives on a schedule
yet, so silent rot on an old drive would otherwise surface only during a
restore.

## Restoring

Restoring **replaces this server's state** with the archive's. It is not
reversible and there is no dry run.

```bash
sudo virtues restore /path/to/archive.tar.gz.age --key-file /path/to/key
```

Unlike backup, this runs as root. From a registered drive, pass the mount
path — not the volume's name or id:

```bash
sudo virtues restore --from-volume /path/to/mount --key-file /path/to/key
```

Before touching anything, restore checks that the service is stopped, reads
the manifest, refuses an archive written by a *newer* release than the binary
you're running, verifies every checksum, and proves it can reach the database.
Then it gives you five seconds to press Ctrl-C. The version check and the
checksums cannot be bypassed; if an archive came from a newer release,
[upgrade](/docs/operate/upgrading) first and restore afterward.

Restoring from a drive applies the newest full snapshot, then every lake
increment in order. One caveat worth knowing: if an increment has been deleted
from the drive, the restore proceeds with the ones it can see rather than
stopping to tell you a window is missing. Verify the drive's contents before
relying on a restore that matters.

## What this doesn't do yet

Being precise about the edges, since backups are exactly the wrong place for
optimism:

- **There is no cloud or network destination.** "Off-server" today means a drive
  you physically plug in. Registering remote storage isn't implemented, so if
  your only copies live in the same building as the server, a fire takes both.
- **Nothing escrows your recovery key.** It is a banner printed once; copies
  are entirely your responsibility.
- **Nothing verifies archives on a schedule.** `--verify` is manual.
- **The Settings panel is read-only.** It shows whether backups are current,
  stale, or failing, but there is no button — backup and restore are terminal
  verbs today.

If you're moving to new hardware rather than recovering the same server, ask
first: the archive carries the old server's identity, and what that means for
your paired devices isn't settled yet.
