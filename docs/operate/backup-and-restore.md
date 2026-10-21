---
title: Backup & restore
description: How to protect a Virtues server - creating the recovery key the server never stores, backing up to a drive, verifying archives, and restoring.
updated: 2026-10-20
---

Your server stores data that exists nowhere else. This page explains how to
keep it if the hardware fails.

Read the first section before the others. Backups are encrypted to a key the
server does not keep, so the order of steps matters: if you lose the key,
every archive you have made is unreadable.

## First: create the recovery key

Backups are encrypted with [age](https://age-encryption.org). Your server
stores only the *public* half of the key, which is enough to write an archive
but not to read one. Create the keypair once:

```bash
sudo virtues backup --init-key
```

This prints the secret key **once**, in a banner, and doesn't write it
anywhere. Neither someone who steals the server nor Virtues as a company can
decrypt your archives. It also means that if you lose the key, nobody can
recover it for you.

**Store the key somewhere that will survive losing the server, and make more
than one copy.** Use a password manager, a printout in a drawer, or a note in
a safe. Plan for a house fire or a dropped laptop more than for a burglar.
There is no escrow, no reset email and no support process that can recover
the key. Without it, your backups can't be decrypted.

`virtues backup` refuses to run until you have created the key. It used to
create one automatically, and people ended up with archives whose key had
scrolled off a terminal months earlier.

## Making a backup

```bash
sudo virtues backup
```

This writes one self-contained encrypted archive to
`/var/lib/virtues/backups/`, named with the time it was taken and ending in
`.tar.gz.age`. Use `--output <path>` to write it somewhere else, such as an
external drive you mounted by hand, and `--force` to overwrite an existing
file.

### What's inside

- The **database**, as a full dump. It also contains your server's identity:
  the server's network key, the certificate authority and the list of paired
  devices. So a restore brings back the server's identity along with its data.
- The **environment file**, which contains the encryption key for the
  credentials stored in the database. A dump restored without it can't decrypt
  those credentials, so a backup refuses to run when it can't find the file.
- The **data lake**: recordings, uploads and files.
- **Authored applets**, the ones written on the server rather than shipped with
  it.
- A **manifest** recording the binary and schema versions, plus a SHA-256 hash
  of every file. Verification and restore use these hashes to detect a damaged
  archive.

Because the archive includes the environment file, **it is as sensitive as the
server itself.** Protect a backup file the way you protect the machine.

Not included: the downloaded model files, which are large and can be
downloaded again, and some machine-local settings such as the saved Wi-Fi
passphrase and the release channel. A restored server downloads or recreates
those.

## Backing up to a drive

Plug in an external disk, mount it, and register it once:

```bash
sudo virtues volumes add /path/to/mount --name "study drive"
```

The server identifies the drive by its filesystem UUID rather than its mount
path, so it still recognizes the drive if a reboot mounts it somewhere else.
The server reads and writes only inside its own subdirectory on the drive, so
you can keep using the disk for other files.

```bash
virtues volumes ls                   # registered drives, and when each was last backed up
sudo virtues backup --volume all     # write to every attached drive
```

**Backups to drives run automatically.** A nightly job at 04:00 backs up to
every registered drive that is attached. Drives that aren't plugged in are
skipped without an error. Registering a drive is all the setup you need.

On a drive, the backup is split into a full snapshot of the database,
environment file and applets, plus separate incremental archives of the lake.
Old full snapshots are deleted only when the drive starts filling up, and the
newest one is never deleted, so a large disk keeps several. Lake increments
are never deleted, because the lake only grows and can't be rebuilt from
anything else. A backup refuses to start if it would leave less than a
gigabyte free.

## Verifying a backup

Check that an archive decrypts and is intact:

```bash
virtues backup --verify /path/to/archive.tar.gz.age --key-file /path/to/key
```

This decrypts the archive, extracts it, and re-hashes every file against the
manifest. It writes nothing and doesn't need a working database, so you can
run it against an old archive on a different machine. It is also how you
confirm that your saved key works before you need it.

Run this by hand from time to time. Nothing verifies archives on a schedule
yet, so corruption on an old drive would otherwise only show up during a
restore.

## Restoring

Restoring **replaces this server's state** with the archive's. You can't undo
it, and there is no dry run.

```bash
sudo virtues restore /path/to/archive.tar.gz.age --key-file /path/to/key
```

Unlike backup, this runs as root. To restore from a registered drive, pass the
mount path, not the volume's name or id:

```bash
sudo virtues restore --from-volume /path/to/mount --key-file /path/to/key
```

Before changing anything, restore checks that the service is stopped, reads
the manifest, refuses an archive written by a *newer* release than the binary
you're running, verifies every checksum, and checks that it can reach the
database. Then it gives you five seconds to press Ctrl-C. You can't skip the
version check or the checksums. If an archive came from a newer release,
[upgrade](/docs/operate/upgrading) first and then restore.

Restoring from a drive applies the newest full snapshot, then every lake
increment in order. If an increment has been deleted from the drive, the
restore continues with the ones it finds and doesn't warn you that part of
the lake is missing. Check the drive's contents before relying on an
important restore.

## Current limitations

- **There is no cloud or network destination.** Backing up off the server
  currently means a drive you physically plug in. Remote storage isn't
  implemented, so if all your copies are in the same building as the server,
  a fire can destroy all of them.
- **Nothing escrows your recovery key.** It is printed once in a banner, and
  keeping copies is up to you.
- **Nothing verifies archives on a schedule.** `--verify` is manual.
- **The Settings panel is read-only.** It shows whether backups are current,
  stale or failing, but has no buttons. Backup and restore are terminal
  commands.

If you're moving to new hardware rather than recovering the same server, ask
us first. The archive carries the old server's identity, and we haven't yet
decided what that means for your paired devices.
