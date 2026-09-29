# Backup: surviving the loss of the box

Written 2026-09-29. Built 2026-07-25 through August. How a box keeps a copy of
the owner's archive somewhere that is not the box, and why the format and
policies are what they are. The code is `virtues-core/src/cli/backup.rs`,
`backup_volume.rs`, `restore.rs`, `storage/volumes.rs`, the pre-migration dump
in `cli/upgrade.rs`, and the `backup_volumes` applet. User-facing behavior is
in `docs/operate/backup-and-restore.md`. What is still open is
`agents/plan/backup-plan.md`.

## The gap it closed

A box held exactly one copy of the owner's life archive. `virtues backup` and
`virtues restore` existed but ran only by hand, the restore path had never run
under test, nothing pruned the tarballs, and each tarball was plaintext gzip
containing `VIRTUES_ENCRYPTION_KEY`. Cloud providers give durability away
without anyone noticing; the moment data leaves the cloud that obligation
transfers to the owner. Self-hosting without a backup is strictly worse than
the cloud on the one axis nobody forgives — a dead NVMe, a theft, a fire.

**A live bug came first.** Backup and restore each hardcoded the lake at
`/var/lib/virtues/lake`, while nine other sites read `STORAGE_PATH` with two
different defaults. They agreed only because the installer wrote the same
path. The first thing anyone does with a bigger drive is set `STORAGE_PATH` —
at which point backup ships an empty lake and restore `rm -rf`s the wrong
directory. One resolver, `storage::lake::lake_root()`, replaced all of them
before anything else was built. Restore also refuses to replace a directory
that does not look like a lake.

## What survived from the first version

The manifest (per-file sha256 and size, written last); `pg_dump --format=custom
--no-owner --no-acl`; **dump-then-copy-lake ordering** — files landing between
the two steps become orphan bytes with no DB row, which is harmless, whereas
the reverse order yields rows pointing at bytes the archive lacks; and the
three restore gates (service inactive, schema compatible with the binary's
embedded migrations, digests verified).

## Pillar 1 — the pre-migration dump belongs to `upgrade`

`virtues rollback` flips the release symlink back. It cannot touch schema,
because migrations only roll forward and the old binary is expected to boot
against the newer schema. So a migration that damages data leaves rollback
restoring *code onto damaged data* — the one failure release slots cannot
cover. Only a dump taken before `migrate` closes it.

That makes it the data half of a release slot, not a backup tier: owned by
`upgrade`, written after the migration preflight passes (no cost on a release
that was going to be refused) and before the service stops, named
`pre-upgrade-<from>-<to>.dump`, pruned in lockstep with slots, unencrypted
because the box already holds the key. The upgrade refuses up front when the
dump will not fit — filling the disk and *then* failing a migration is strictly
worse than not upgrading — and `rollback` prints the dump's path so the
artifact is usable rather than hidden.

## Pillar 2 — one artifact type, split by mutability

The first design mirrored the lake file by file. Encryption changed the
arithmetic before it was built: an age header and an encrypt call per object,
on a lake of many small stream files, is slower than bundling and the worst
possible USB write pattern. Re-archiving the whole lake every run is no better
— hundreds of gigabytes over a bus that drops under sustained load on ARM, to
capture a day's change.

So two artifacts with opposite lifetimes:

```
<mount>/virtues/<host>/archives/
  full-<ts>.tar.gz.age   DB + applet state + env + manifest. Pruned freely.
  lake-<ts>.tar.gz.age   Lake files added since the last run. NEVER pruned.
```

Increments are never pruned, structurally rather than cautiously: the lake is
append-only, so each file exists in exactly one increment. When a volume fills,
the run refuses instead of pruning, because by then the only things left to
prune are irreplaceable.

**The box cannot read its own increments** — they are encrypted to a key it
does not hold — so `backup_archived_file` records box-side what shipped where.
The drive stays authoritative about which increments exist: each run
reconciles against the directory, and rows for a vanished increment are
dropped and their files re-sent. A wiped or swapped drive heals itself.

Consequences, stated so nobody discovers them mid-incident: a restore is the
newest `full-*` plus **every** increment in order; and an old full replayed
against all increments yields an old database and a current lake — the same
orphan-bytes condition as the dump ordering, coherent for the same reason.
Restore verifies every increment before destroying anything; an earlier
version wiped the lake first, so increment 3 of 7 failing left a destroyed
lake and no way back.

## Pillar 3 — destinations keyed on filesystem UUID

Mount points move between boots and between drives, so a volume's identity is
its filesystem UUID (`storage_volume.fs_uuid`), resolved through
`/dev/disk/by-uuid`; `mount_path` is an observation, not identity. The OS
mounts, the daemon reads a path; it never shells out to `mount(8)`. `roles`
exists and holds only `backup`. **Absence is never an outage**: an unplugged
drive is a skipped run and a warning, which is the entire reason removable
media is acceptable for backups and not for live storage.

## Pillar 4 — retention by space, not by count

Keep-N is the wrong knob. The archive grows around 95 GB a year and the drive
does not grow at all; pick N=7 and you either strand a terabyte or overflow in
year three. Fulls are pruned oldest-first only while free space is tight,
pruning happens *before* the write (pruning after can never free room for the
write that needed it), the space guard budgets both the increment and the full
archive's database dump, and below a floor the run refuses loudly.

## Pillar 5 — do not format the drive

Because the archive is encrypted rather than the volume, the filesystem is
irrelevant: ext4, exFAT, whatever NTFS it shipped with. Backups write under
`virtues/<host>/` and never own the volume root, so the drive stays usable for
the owner's other files and one drive can serve two boxes. Torn writes are
`.partial` plus `rename(2)`; restore ignores a `.partial`.

## Not building

| | Why |
|---|---|
| Volume router, `volume` columns on lake tables | Backup-only needs one root |
| Mover / tiering engine | At ~95 GB/year a 2 TB drive is 20 years of lake; tiering bites only at 8–20 TB, a permanent appliance component |
| Removable volumes holding live lake data | USB on ARM drops under load: a dropped backup write retries, a dropped lake write corrupts. Synology and TrueNAS both refuse USB in pools |
| Changes to storage keys | Keys are load-bearing (encryption-key derivation reads the date out of the key) |
| A restore button | Restore needs the service stopped; the box cannot do it to itself. The UI shows the CLI recipe instead |

The composability rule: everything the volume system does must reduce to "the
OS mounted something and virtues wrote to a path." If `storage_volume` ever
becomes *required* — if virtues cannot run pointed at a directory — DIY is
broken. DIY users already have LVM, ZFS or mergerfs and want `STORAGE_PATH` on
their pool; an app-level abstraction would fight them.

## Key escrow: virtues never holds the key

Archives are encrypted with `age`, and the box stores only the public half. It
cannot read its own backups: a stolen box yields a key and nothing to decrypt,
a stolen drive yields ciphertext and no key. The recovery secret is printed
once and cannot be printed again, by construction.

The invariant is absolute on purpose — not for any user, not opt-in. The moment
virtues holds keys for *some* users, a hosted archive can no longer say "we
cannot read your data", only "we choose not to". The first is a property of
the system; the second is a promise, and promises erode. The answer to a lost
key is not a better code but more copies in places the owner controls that are
not the box.

## The one-way door

**The archive format closed on 2026-07-25.** Encryption, streaming and
manifest-last all landed before anything scheduled a backup, while there were
no artifacts in the wild worth preserving. Any layout change now needs a
compatibility shim; `restore` already carries the first, sniffing the age magic
so pre-encryption archives still open. Manifest signing was dropped from this
door rather than built: age's AEAD authenticates the whole archive, so
tampering fails at decryption, and the per-member digests catch our own bugs.
