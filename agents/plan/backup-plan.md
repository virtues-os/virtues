# Backup paradigm — surviving the loss of the box

> **STATUS 2026-08-28, verified against code: SHIPPED THROUGH PHASE C/13.**
> The header this replaces claimed "Phase A is the gate — both ends of the
> pipeline are still missing." Both ends shipped. So did the scheduler, the
> age encryption, the restore tests, and the Settings panel.
>
> **What genuinely remains:** missing-increment detection on volume restore
> (a hole restores silently instead of failing loudly and naming the window);
> the Home warning line and the onboarding unfinished-step framing; scheduled
> random verification; volume probing in core (`record_probe` has zero
> callers, so capacity and free space are written once at registration and
> never refreshed); and the format offer for an unmountable drive, whose
> exFAT/NTFS driver dependency in the Dragon image is still unverified.
>
> **Two policies stated here that the code does not honor** — gaps, not plan:
> "never prune below 2" is not implemented (`prune_full_archives` protects
> only the single newest full, so a tight drive can be pruned to one), and the
> local `/var/lib/virtues/backups` directory has no retention at all.
>
> **False below:** the tarball is age-encrypted, not plaintext gzip; the
> restore path has tests; migrations 0063/0064 do not exist (both tables live
> in the squashed `0001_initial.sql`); Pillar 3's DDL omits the `NOT NULL`
> `prefix` column; Pillar 5's torn-write note describes a mirror design that
> Pillar 2's own amendment replaced with increments.
>
> User-facing behavior is documented at
> [`../../docs/operate/backup-and-restore.md`](../../docs/operate/backup-and-restore.md).

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

## Pillar 2 — One artifact type, split by mutability

> **Amended 2026-07-25 (BUILT).** This pillar originally specified an *additive
> mirror* of the lake — each file copied and encrypted individually. Encryption
> changed the arithmetic: an age header and an encrypt call per object, on a lake
> of many small stream files, is both slower than bundling and the worst possible
> USB write pattern. Replaced with incremental archives before building. The
> reasoning below is the amended version.

Not two tiers. One format, one restore path, one drill.

But a full archive re-copies the entire immutable lake every run. The lake is
append-only — ~95% of archive N+1 is byte-identical to archive N. Writing 200 GB
over USB to capture 1 GB of change is hours, on a bus that drops under sustained
load on ARM.

So split by **mutability**, into two artifact kinds with opposite lifetimes:

```
<mount>/<prefix>/archives/
  full-<ts>.tar.gz.age    DB + applet state + env + manifest.
                          Complete every run. Pruned freely — newest supersedes.
  lake-<ts>.tar.gz.age    Lake files added since the last run.
                          NEVER pruned.
```

**Increments are never pruned, and that is structural rather than cautious.** The
lake is append-only, so each file exists in exactly one increment; deleting one
loses everything it holds with no other copy anywhere. When a volume fills, the
run refuses loudly instead of pruning, because by then the only things left to
prune are the irreplaceable ones.

**The box cannot read its own increments.** Archives are encrypted to a key it
does not hold ([open decision 1](#open-decisions)), so it cannot inspect a drive to learn what is already
there. `backup_archived_file` (migration 0064) is the box-side record of what has
shipped where — a direct cost of the encryption decision, not a convenience.

**The drive stays authoritative about which increments exist.** Filenames are
plain timestamps and leak nothing, so each run reconciles against the directory:
any increment the table references that is no longer present has its rows dropped
and its files re-sent. A wiped or swapped drive heals itself rather than leaving
a hole that would surface only as a short restore.

### Consequence to state explicitly

Restoring means the newest `full-*` **plus every increment, in order** — not a
single file. A missing increment is a real hole, and must fail loudly naming the
window it covered rather than restoring short and silent.

The drive is also not a point-in-time copy of the lake: increments accumulate, so
an old `full-*` replayed against all increments gives an old DB and a current
lake. That is the same orphan-bytes condition the dump-then-copy ordering already
produces, and coherent for the same reason — the drive rolls the *database* back,
not the lake. Given the lake is immutable raw capture and nothing has ever deleted
from it (`cli/lake_adopt.rs:11`), this is correct. Document it rather than let
someone discover it mid-incident.

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

### Shipped (branch `feat/backup-durability`, 2026-07-25)

1. **One path resolver** — `storage::lake::lake_root()`; both hardcoded `LAKE_DIR`
   constants and nine ad-hoc `STORAGE_PATH` reads gone. Fixed the live bug where a
   custom `STORAGE_PATH` made backup ship an empty lake and restore `rm -rf` the
   wrong directory.
2. **Round-trip drill** — real functions, real Postgres, state destroyed before
   restore so it cannot pass without restore working.
3. **Streaming archive** — no staging copy, digests computed in flight, manifest
   written last.
4. **Pre-migration dump** in `upgrade`, pruned with `KEEP_SLOTS`, surfaced by
   `rollback`.
5. **Encryption** — `age`, box holds only the public half.
6. **Volume registry** — `storage_volume` (0063), UUID-keyed.
7. **Incremental volume backups** — `full-*` + `lake-*`, box-side tracking (0064),
   drive-authoritative reconciliation, pre-write space guard.

### Phase A — close the loop

Both ends of the pipeline are missing; the middle works. **Nothing else matters
until this is done, because a write path whose restore half is unproven is the
exact failure this document was written to remove — and this one looks finished.**

8. **`virtues restore --from-volume`** — newest `full-*`, then every `lake-*` in
   order, verifying each manifest. A missing increment fails loudly, naming the
   window.
9. **`virtues backup --add-volume <path>`** — derive the UUID via `uuid_for_path`,
   set the prefix, write the row. Without it the registry is unreachable.
10. **Extend the drill to the volume round trip** — the test that would have caught
    the gap above.

Merge to `staging` at the end of Phase A. This branch is already nine commits;
CLAUDE.md records `feat/composability` reaching 181 behind and having to be
largely dropped.

### Phase B — run it without a human

11. **Scheduler wiring** through the applet system (one-scheduler doctrine), not a
    systemd timer.
12. **`virtues backup verify`** — digests without a restore; verify one at random
    on a schedule so rot surfaces before an incident.

### Phase C — make it visible

13. **Settings** — backup age, drive state, last error.
14. **Home** — a line only when it is bad.

### Phase D — housekeeping

15. `agents/build/recovery.md:274` still documents the deleted `/usr/local/bin/virtues.bak`
    rollback mechanism.

### One-way door

**The archive format — CLOSED 2026-07-25.** Encryption, streaming, and the
manifest-last ordering all landed before anything schedules a backup, which was
the point: there were no artifacts in the wild worth preserving, so the format
could change freely. That window is now shut. Any future change to the archive
layout needs a compatibility shim, and `restore` already carries the first one —
it sniffs the age magic so pre-encryption archives still open.

Manifest signing was dropped from this door rather than built. age's AEAD
authenticates the whole archive, so tampering fails at decryption; the per-member
digests catch corruption inside a validly-decrypted archive — our own bugs — not
tampering.

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

- `agents/record/update-paradigm.md` — release slots, migration preflight, rollback
- `agents/record/data-durability.md` — ingestion reliability (device → box)
- `agents/build/recovery.md` — operator runbook. **Stale:** `:274-291` documents the deleted
  `/usr/local/bin/virtues.bak` mechanism, and `:202` cites an env path the installer
  does not use. Fix alongside step 1.
