# Backup paradigm — surviving the loss of the box

> **Status:** Plan (2026-07-25). Not built. Scopes a durability story for box data:
> an upgrade-owned pre-migration dump, and a single encrypted backup artifact on an
> external drive. Explicitly **excludes** storage tiering, volume routing, and object
> storage — see [Not building](#not-building) for why.
>
> Complements `data-durability.md`, which covers getting data *in* without loss. This
> doc covers keeping it once it's in.

## The gap

A box today holds exactly **one copy** of the user's life archive.

- Nothing schedules a backup. `virtues backup` / `virtues restore` exist
  (`cli/backup.rs`, `cli/restore.rs`) and are manual-only — no timer, no cron, no
  applet, no UI.
- **The restore path has never been executed under test.** No `#[cfg(test)]` in
  either file; no integration test references either command. A restore path that
  has never run is a hope, not a backup.
- No retention. Every run writes a new timestamped tarball into
  `/var/lib/virtues/backups` and nothing ever deletes one.
- The tarball is plaintext gzip and **contains `VIRTUES_ENCRYPTION_KEY`**. It is as
  sensitive as the box itself.
- `rollback` restores the binary but not the data (below).

This is the product's own promise turned against it. Cloud providers hand users
durability for free and they never think about it; the moment data leaves the cloud,
that obligation transfers to the owner. Self-hosting without a backup story is
strictly worse than the cloud on the one axis nobody forgives — a dead NVMe, a theft,
or a house fire is total loss.

### A live bug this plan must fix first

`backup.rs:28` and `restore.rs:33` both hardcode `const LAKE_DIR =
"/var/lib/virtues/lake"`, while nine other sites read `STORAGE_PATH` — with two
different defaults (`./data/lake` at `storage/lake.rs:83`, `/var/lib/virtues/lake` at
`setup/mod.rs:31`).

Today those agree because the installer writes the same path. But **the first thing
anyone does with a bigger drive is set `STORAGE_PATH`** — and at that moment `backup`
silently ships an empty `lake/` and `restore` `rm -rf`s the wrong directory. The
feature in this doc is the exact trigger for the bug.

## What is already right

Not a rewrite. These survive unchanged:

- The manifest concept — `manifest.json` with per-file sha256 + size (`backup.rs:67-82`).
- `pg_dump --format=custom --no-owner --no-acl`.
- **Dump-then-copy-lake ordering** (`backup.rs:119` then `:152`). Files written in
  that window land in the lake with no DB row — orphan bytes, harmless. The reverse
  order would produce DB rows pointing at bytes that aren't in the archive. *Add a
  comment saying so before someone "optimizes" it.*
- All three restore gates: service-inactive (`restore.rs:148`), schema-compat against
  `embedded_migration_max()` (`:169`), digest verify (`:197`).
- Awareness of all four state roots: lake, applet state, env file, DB.

What changes: streaming instead of staging, encryption, retention, destinations,
scheduling, and the `LAKE_DIR` fix.

---

## Pillar 1 — The pre-migration dump belongs to `upgrade`, not to backup

`virtues rollback` re-flips the `current` symlink and restarts. It does not touch
schema, because **migrations roll forward only** — `database/mod.rs:309` sets
`ignore_missing(true)`, and the rollback doc comment states the old binary is
expected to boot against the newer schema.

So a migration that destroys data leaves rollback restoring *code onto damaged data*.
That is the one failure the slot system cannot cover, and the only thing that closes
it is a dump taken before `migrate` runs.

This is **not a backup tier**. It is the data half of a release slot:

| | Release slot | Pre-migration dump |
|---|---|---|
| Owner | `cli/upgrade.rs` | `cli/upgrade.rs` |
| Written | before the flip | before `migrate` |
| Retention | `KEEP_SLOTS - 1` | same policy, same prune call |
| Encrypted | no | no — the box already holds the key |
| Manifest | no | no |
| Visible in backup UI | no | no |

**Placement.** After the `migrate --check` preflight passes (`upgrade.rs:238`) and
before the service stops (`:265`). Taking it after preflight means no cost on a
release that was going to be refused anyway, and it is still inside the
box-untouched window.

**Two additions to the existing preflight block:**

1. **Free-space check.** Refuse the upgrade if the dump won't fit. Filling the disk
   and *then* failing a migration is strictly worse than not upgrading.
2. **`rollback` prints the path.** One line — *"binary rolled back to `<tag>`; if the
   schema is the problem, restore `/var/lib/virtues/backups/pre-upgrade-…`"* — turns a
   hidden artifact into a usable one.

Naming: `pre-upgrade-<from>-<to>.dump`, pruned in lockstep with slots.

---

## Pillar 2 — One backup artifact, split by mutability

Not two tiers. One destination, one format, one restore path, one CI drill.

But a full archive re-copies the entire immutable lake every run. The lake is
append-mostly — ~95% of archive N+1 is byte-identical to archive N. Writing 200 GB
over USB to capture 1 GB of change is hours on a bus that drops under sustained load
on ARM.

So split the artifact by **mutability**, not by tier:

```
/mnt/<drive>/virtues/<box-id>/
  lake/                      additive mirror — each file encrypted once, never
                             rewritten, never pruned
  archives/
    2026-07-25T03:00Z.age    DB + applet state + env + manifest — small, encrypted,
                             chained
```

**The lake half is a sync, not a chain.** Files are append-only with unique keys, so
there is no diffing, no dedup, no chunk store — only *"does this key exist on the
drive."* Lake files are plaintext at rest (`lake_objects.content_encoding` is only
`none | zstd`; `storage/lake.rs` does no encryption), so each is encrypted exactly
once on the way out and never touched again.

**The archive half is small**, which makes retention trivial and restore cheap:
decrypt archive N → `pg_restore` → point at the lake mirror.

This is *simpler* than full-copy, not more complex: the expensive path becomes
incremental for free, weekly cadence becomes viable, and the "how many full copies
fit" question disappears.

### Consequence to state explicitly

An additive mirror never deletes, so the drive is **not a point-in-time copy of the
lake**. Restoring a 60-day-old archive gives a 60-day-old DB against a current lake —
the same orphan-bytes condition the dump-then-copy ordering already produces, and
coherent for the same reason. The drive can roll the *DB* back, not the lake.

Given the lake is immutable raw capture and nothing has ever deleted from it
(`cli/lake_adopt.rs:11` — "nothing has ever deleted a recording"), this is correct.
Document it rather than let someone discover it during an incident.

---

## Pillar 3 — Destinations, keyed on filesystem UUID

Mount points move between boots and between drives. Identity is the filesystem UUID.

```sql
CREATE TABLE storage_volume (
  id             TEXT PRIMARY KEY,        -- vol_…
  name           TEXT NOT NULL,           -- "Archive 2TB"
  kind           TEXT NOT NULL,           -- removable | internal | network
  roles          TEXT[] NOT NULL,         -- {'backup'} — only value in v1
  fs_uuid        TEXT NOT NULL,           -- /dev/disk/by-uuid/<uuid> — THE identity
  mount_path     TEXT,                    -- runtime observation, NOT identity
  state          TEXT NOT NULL,           -- present | absent | degraded
  last_seen_at   TIMESTAMPTZ,
  capacity_bytes BIGINT, free_bytes BIGINT, probed_at TIMESTAMPTZ
);
```

`roles` is Proxmox's `content` field — a volume declares what it may hold. Keep the
column, populate only `{backup}`, enforce in code.

**The OS mounts; the app reads.** Write an fstab entry or systemd mount unit with
`nofail,x-systemd.automount`, keyed on UUID. Do not shell out to `mount(8)` from the
daemon. A UUID that resolves nowhere is `absent`, not an error.

**Absence is never an outage.** Drive unplugged → skip the run, warn, retry next
cycle. This is the entire reason backup-only is tractable and live storage is not.

The volume probe already exists in the wrong crate:
`tools/virtues-installer/src/storage.rs` classifies `DeviceClass::{Nvme, SataSsd,
Usb}`, finds the covering mount via `/proc/self/mountinfo`, and benchmarks
write+fsync. Move it to core.

---

## Pillar 4 — Retention by space, not by count

Keep-N is the wrong knob: the archive grows ~95 GB/year
(`cli/lake_adopt.rs:11` records ~260 MB/day) and the drive does not grow at all. Pick
N=7 and you either strand a terabyte or overflow in year three.

Policy:

1. Always keep the newest archive.
2. Prune oldest while free space is below a floor.
3. Never prune below 2.
4. If it cannot hold 2, **refuse loudly**. A backup system that fails quiet is worse
   than none.

The lake mirror is never pruned — it is one copy, always current.

---

## Pillar 5 — Drive handling: do not format

Because the *archive* is encrypted rather than the volume, the filesystem is
irrelevant. ext4, exFAT, the NTFS it shipped with — all fine.

- **Write to `virtues/<box-id>/` on whatever is already there.** Never own the volume
  root; never touch anything outside that directory. The drive stays usable for the
  owner's other files, and one drive can serve two boxes.
- Offer to format **only** when there is no mountable filesystem. Then require typed
  confirmation, matching `cli/uninstall.rs`'s hostname prompt.
- **Dependency to verify:** the Dragon image must ship exFAT/NTFS fuse drivers.
- Torn writes: archives write `.partial` + `rename(2)` (existing repo doctrine). The
  lake mirror is additive, so an interrupted file is simply re-copied next run. No
  corruption path.

---

## Pillar 6 — UI

Lives in Settings (one room, flat nav), not a new top-level page.

- **One number: age of last successful backup.** Not size, not count.
- States: no destination → destination set, never run → running → last run *N* days
  ago → failing.
- Home surfaces a line only when it is bad. Data-honest voice: *"Last backup: 9 days
  ago"*, *"No backup destination"*. Not *"⚠️ Protect your data!"*
- **No restore button.** Restore requires the service stopped — the box cannot do it
  to itself while running. Show the CLI recipe and the archive path instead. A button
  would imply a capability that cannot exist.
- Framing is **incomplete setup**, not extra protection. "Extra protection" reads as
  an upsell and gets declined. The state to surface is a fact: *your data exists in
  one place.* Nag, never gate.
- Onboarding treats "no backup destination" as an unfinished step.
- **Known weakness:** there is no push path (APNs is noted-not-built), so the nag is
  in-app only. A box whose owner does not open the UI will not hear about a failing
  backup.

Also ship `virtues backup verify <path>` — check manifest digests without restoring —
and verify one random archive on a schedule, so bit rot surfaces before an incident
rather than during one.

---

## Not building

| | Why |
|---|---|
| Volume router (`HashMap<VolumeId, PathBuf>`) | Backup-only needs one root |
| `volume` column on `lake_objects` / `app_drive_files` / `data_audio_recording` | ditto |
| Mover / tier policy engine | Same machinery as retention, which doesn't exist yet |
| Object storage (S3) as a destination | Later; the destination trait leaves room |
| Any change to `FileStorage` or `StreamKeyParser` | Storage keys are semantically load-bearing (encryption-key derivation reads the date out of the key). Don't disturb it |
| Removable volumes holding lake data | Synology and TrueNAS both refuse USB in pools. USB on ARM SBCs drops under load — a dropped backup write retries, a dropped lake write corrupts |

**On tiering generally.** At ~95 GB/year, a 2 TB drive is 20 years of lake. The
cost-per-TB argument only bites at 8–20 TB — a powered 3.5" drive, i.e. a permanent
appliance component, not a drive you plug in. And neither deployment path needs
app-level tiering: **DIY** users already have LVM/ZFS/mergerfs and want `STORAGE_PATH`
pointed at their pool, where an app-level abstraction would actively fight them;
**Dragon** is a BOM decision — if boxes fill up, spec a bigger NVMe.

The composability rule that follows: *everything the volume system does must reduce to
"the OS mounted something and virtues wrote to a path."* If the `storage_volume` table
ever becomes **required** — if you cannot run virtues by pointing it at a directory —
DIY is broken. Same doctrine as BYO-key and transport-agnostic-box: opinionated
default, always an escape hatch.

---

## Sequence

1. **One path resolver.** Delete both `LAKE_DIR` constants and the nine ad-hoc
   `STORAGE_PATH` reads; single `storage::lake_root()`, one default. Fixes the live
   data-loss bug and unblocks everything else.
2. **Restore drill in CI.** `sqlx::test` already exists in the workspace. Seed → backup
   → restore into a scratch DB → assert row counts and file digests. **Until this is
   green, treat backups as unproven.**
3. **Format v2.** Streaming tar (kills the 2× staging copy at `backup.rs:111` and the
   whole-file hashing at `:271`), `age` encryption, signed manifest, stable naming.
   **Must land before scheduling ships** — see [one-way door](#one-way-door).
4. **Pre-migration dump** in `upgrade.rs` + free-space preflight + `rollback` hint.
   Independent of 1–3; can land in parallel.
5. **`storage_volume` table**, role `{backup}` only, UUID-keyed, mount unit generation.
6. **Backup applet** — scheduled through the existing scheduler primitive, not a
   systemd timer. Retention lives with it.
7. **UI** + `backup verify`.

### One-way door

**The archive format.** Once encrypted archives exist on owners' drives, the layout
cannot change without a compatibility shim forever. Right now nothing schedules
backups and restore has never been tested, so there are effectively no artifacts in
the wild worth preserving. That freedom ends the day step 6 ships. Everything else
here is reversible; this is not.

---

## Open decisions

**1. Key escrow — DECIDED 2026-07-25.** An encrypted archive needs a recovery key
that is not inside it (the current tarball's flaw: it contains
`VIRTUES_ENCRYPTION_KEY`).

**Virtues never holds that key. Not for any user, not opt-in, not "for
convenience."** The invariant is absolute on purpose: the moment it holds keys for
*some* users, a hosted archive service can no longer say *we cannot read your data*,
only *we choose not to*. The first claim is a property of the system; the second is a
promise, and promises erode. Keeping the invariant absolute is what makes an
S3-backed offering safe by construction — the service stores ciphertext it provably
cannot read, which is a feature to sell rather than a liability to manage.

The cost of that invariant is real: recovery codes get lost, and "write this down"
has a high failure rate even among people who know better. But the answer to a lost
code is not a better code — it is **more places the key lives that are not the box
and not virtues.** The box is specifically not a safe place for it, since the whole
point is surviving the box.

So: generate on the box, and get it *off* the box into places the owner already
controls.

1. **Paired devices, automatically.** Write the key into the platform keychain of
   each paired device (iOS/macOS Keychain). Apple syncs and backs that up; virtues
   does not. Zero user effort, and it is the layer that actually moves the loss rate.
2. **Password manager, deliberately.** Show the code once with a save-to-manager
   affordance and an explicit acknowledgement gate before backups can be enabled.
3. **Track copies, don't track compliance.** Settings reports *how many independent
   copies exist* — "recovery key: 2 devices" vs "only on this box — losing it loses
   the archive." Same honest-status pattern as backup age. Asking "did you write it
   down?" measures nothing; counting copies measures the thing that matters.

Still open below this decision: which KDF and format (argon2id + `age` passphrase
mode is the obvious default), and whether a user-chosen passphrase is offered as an
alternative to a generated code — it is more memorable and much weaker, so it would
need a strength floor.

**2. Box identity on restore-to-new-hardware.** `restore.rs` ends by printing `virtues
pair`, implying devices must re-enroll — but the env file is restored, so what
identity the new box claims is ambiguous. Preserve the NodeId and two boxes can claim
one identity; regenerate it and every paired device silently stops syncing until
someone notices. This is a UX decision, not an implementation detail.

**3. RPO / RTO have never been stated.** "Back on new hardware within an hour, losing
at most a day" is a different system from "eventually recoverable" — it determines
cadence, whether incrementals are needed, and whether models belong in the archive
(excluded today as re-downloadable, which is fine until someone restores without
internet).

**4. Measure the DB before committing to archive sizing.** Embeddings live in Postgres
(`0008_search_and_vectors.sql`, `vector(1024)`, Matryoshka-reduced in
`0017_embeddinggemma_256.sql`), so the vector tables plus HNSW index overhead may make
the DB the dominant term rather than the lake — which would invert the "archives are
small" premise this design rests on.

```sh
sudo -u virtues psql -d virtues -c "SELECT pg_size_pretty(pg_database_size(current_database()));"
```

If vectors dominate, the archive needs `--exclude-table-data` on the embedding tables
plus a reindex on restore — and since re-embedding on-box runs in hours, that is a
real RTO tradeoff to make deliberately rather than discover mid-incident.

---

## Related

- `docs/update-paradigm.md` — release slots, migration preflight, rollback
- `docs/data-durability.md` — ingestion reliability (device → box)
- `docs/recovery.md` — operator runbook. **Stale:** `:274-291` documents the deleted
  `/usr/local/bin/virtues.bak` mechanism, and `:202` cites an env path the installer
  does not use. Fix alongside step 1.
