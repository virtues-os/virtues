# The appliance image — how a Dragon becomes a product

> How a Radxa Dragon Q6A goes from a board in a box to a unit a customer can
> plug in, written against the boot chain as measured on hardware.
>
> Companion to [onboarding.md](onboarding.md) (what the owner does),
> [deployment.md](deployment.md) (how the software ships), and
> [recovery.md](recovery.md) (what happens when it breaks).

## The one thing to know first

**Units ship NVMe-for-both: one NVMe carries the boot chain, the OS and the
owner's data.** There is no microSD card in the unit.

The Q6A's UEFI lives in SPI flash on its own chip, and boots whichever medium
carries an ESP (documented order USB > SD > NVMe > eMMC > UFS). Shipping boards
arrive with no OS on internal storage, so flashing is a manufacturing step we
perform. Storage facts that are easy to get wrong:

- **An NVMe with its own ESP boots on its own** — verified on the bench with
  the card pulled. A board whose NVMe carries no ESP falls through to the card,
  which is the only reason it can look as if the NVMe cannot boot.
- **eMMC is a removable module, not soldered.** The Q6A has a shared eMMC/UFS
  module socket; SKUs differ by RAM, not storage. We fit none.
- **`mmcblk1` is not an eMMC.** It looks like one and is the microSD; ask
  `/sys/block/mmcblk1/device/type` (→ `SD`).

## The layouts

`virtues-firstboot.sh` handles two layouts, told apart by where `/` is mounted
from (§1-NVMe runs first; the classic claim skips itself once `$DATA_DIR` is
mounted):

| Layout | Boot + root | `/var/lib/virtues` | Claimed on first boot by |
|---|---|---|---|
| **NVMe-both** (ships) | NVMe `p1 config + p2 ESP + p3 root` | NVMe `p4`, GPT name + fs label `virtues-data` | carving p4 from the space past root, growing it to the disk's end |
| **Card + NVMe** | microSD `p1 config + p2 ESP + p3 root` | the whole separate NVMe | the `for disk` loop, on a blank disk |

**Why NVMe-both.** One medium per unit instead of two. The kernel, initrd, DTB
and the root filesystem that holds `/lib/modules/<ver>` travel together, so
kernel/module skew is impossible by construction — on the card layout the
kernel lives on the card's ESP and the modules on root, and imaging the two at
different times boots a kernel with no modules. The press flow is flashing an
NVMe over a USB-C adapter from a Mac: no board needed to flash, no EDL, no
per-board firmware step.

**What is still split: data from the OS, by partition.** Every continuous write
— Postgres, the lake, the journal, backups, applet state — lands on the
`virtues-data` partition, and the master image carries the OS partitions only
(`cut-image.sh` drops the data partition from the GPT). Each unit carves its
own, so there is no shared data UUID to coordinate.

**The trade.** On the card layout, a dead or unseated NVMe leaves the OS
booting and the panel saying *"I can't find my storage disk. Your record is on
it, not lost."* (`data_disk.rs`). On NVMe-both a dead NVMe is a dark box; the
"Storage disconnected" state still covers a data partition that failed to
claim or mount.

**The first-boot claim has one trap worth knowing.** The image ships an fstab
`LABEL=virtues-data` line, and `/` is mounted shared, so the instant a
partition carries that label systemd mounts it over `$DATA_DIR` — shadowing the
root-side seed mid-copy, and propagating into any parent bind taken earlier. The
partition is therefore made under a temporary label (`virtues-seeding`), seeded
through a *private* bind, and `e2label`ed to `virtues-data` only when the seed
is complete. A partition already labelled `virtues-data` is adopted, never
re-made or re-seeded: it may hold the owner's record.

**Boot entries are generated, never hand-edited.** `kernel-install` regenerates
loader entries on every apt kernel upgrade and preserves nothing written by
hand.

### Moving the writes

Three things write continuously, and each needed pointing at the data disk
explicitly.

| What | Where it lands by default | How it gets moved |
|---|---|---|
| The lake, backups, applet state | already under `DATA_DIR` | — |
| journald (~140 MB and growing on the lab box) | `/var/log/journal` on root | `virtues-firstboot.sh` symlinks it to `$DATA_DIR/journal` |
| Postgres (3.0 GB on the lab box) | `/var/lib/postgresql/18/main` on root | the installer symlinks `/var/lib/postgresql` → `$DATA_DIR/postgresql` |

### The Postgres move, in detail

It is the largest and busiest of the three — a WAL flush per transaction,
forever — so it is the one the OS medium most needs to be rid of.

**A symlink, not `data_directory`.** Debian's `postgresql.conf` has a
`data_directory` setting and pointing it at the data disk is the obvious move.
It is the wrong one: that path is also known to `pg_createcluster`,
`pg_dropcluster`, `pg_upgradecluster`, the `postgresql@.service` template's own
`RequiresMountsFor`, and every apt maintainer script. Each would need telling,
or would disagree with us at the worst possible moment — a major-version
upgrade. Symlinking `/var/lib/postgresql` moves the whole tree and leaves all
of them working on vanilla paths that resolve through it. (Checked: the unit
carries no `ProtectSystem`/`ReadWritePaths` sandbox that a symlink out of
`/var/lib` would trip.)

**Copied, not moved.** This is the only copy of the owner's database, so the
installer stops Postgres, *copies*, swaps the symlink in, starts, and proves it
serves — and only then is the original redundant. It is left at
`/var/lib/postgresql.pre-move` for the operator to delete, and `image-check`
reports it as a finding so it cannot ship inside an image by being forgotten.
If the new location will not serve, the installer rolls the symlink back and
restarts on the original.

**Relocated before the database exists.** The installer does it immediately
after installing Postgres and before `provision_db`, so the cluster being
copied is a fresh `initdb` with nothing in it. Run later and it would be
relocating the owner's record.

**On a fresh unit the cluster is built, not inherited.** The image carries the
OS partitions only, so it carries the symlink but not the data it points at —
every unit's data partition starts empty. `virtues-firstboot.sh` therefore claims the disk and then
`pg_dropcluster` + `pg_createcluster`s a vanilla cluster on it, creates the
`virtues` role and database, and stops there. Migrations are deliberately *not*
run at first boot: `virtues server` already runs them at startup, and one
migration path for every box beats a first-boot copy of it that could drift.

Its guard is the narrowest true statement about the job, like the other two in
that script: *a symlink whose target holds no cluster*. A DIY box has no
symlink; a second boot has a cluster; a box whose disk failed to mount has
nowhere to write. All three skip. And it is deliberately **not** keyed on the
first-boot marker — that marker licenses key *minting*, which must happen once
ever, while this must happen once per *disk*. A replaced NVMe needs a cluster
and must not get a new encryption key.

**Two ordering facts that are easy to get wrong.** `virtues-firstboot.service`
is `Before=virtues.service postgresql.service`, and the `postgresql@.service`
drop-in is `After=virtues-firstboot.service` — otherwise Postgres races ahead
on a virgin unit, finds the symlink target missing, and fails on the one screen
the owner is watching hardest. And that same drop-in carries
`RequiresMountsFor=<data dir>`, which the template's own
`RequiresMountsFor=/var/lib/postgresql/%I` does **not** cover: the dependency is
taken on the path as written, not on what the symlink resolves to.

**A box that has lost its disk still boots, and that is deliberate.** The guards
below refuse to start Postgres when a disk that fstab declares is absent — but
they do NOT refuse when no disk was ever configured, and neither does anything
else. The reason is a dependency chain worth knowing: `virtues.service` waits on
`pg_isready`, and the panel is served by `virtues.service`. So a Postgres that
refuses takes the display down with it, and the owner gets a black screen
instead of the "Storage disconnected" message written for exactly that moment.
A box running on the wrong disk is recoverable and says so on the glass; a box
that will not boot says nothing at all.

**What the guards do and do not cover.** `RequiresMountsFor` only binds Postgres
to a mount unit that *exists* — a disk fstab already knows about — so it catches
"the data disk is configured and absent" and not "the claim never ran at all".
The second is the likelier failure on a virgin unit (no NVMe fitted, or the
blank-disk check declining), and there the dependency resolves to the root mount
and is trivially satisfied. That is why the drop-in also carries an `ExecStartPre` — but one CONDITIONED on
fstab declaring a data disk. Asking `mountpoint -q` flatly bricks a board whose
root is already the NVMe and whose state root is a plain directory on it (a
DIY install on an NVMe-root board): Postgres would refuse to start with nothing
saying why. The panel's "Storage disconnected" state checks mount-ness
directly and so covers both from the start.

**Deprovision removes the cluster,** because it is per-unit state — it is where
the record lived. Wherever the master's data dir sits on the OS medium, a
surviving cluster would ship inside every image under a path each unit then
hides with a mount and never reads.

## Building the image

The whole of it is `tools/build-dragon.sh`, which exists so the tail cannot be
skipped and the version cannot be left to chance:

```sh
sudo VIRTUES_VERSION=v0.3.1 sh tools/build-dragon.sh
```

What it does, and what you would otherwise be doing by hand:

```
 1. Flash a stock Radxa image to the boot medium        (per master, once)
 2. Boot it, install Virtues                            curl virtues.com/sh | sudo sh
 3. Verify the box works                                virtues doctor
 4. Strip per-unit identity                             sudo virtues deprovision
 5. Prove it is stripped                                sudo virtues image-check
 6. Power off WITHOUT booting again                     sudo poweroff
 7. Image the boot medium                               tools/cut-image.sh
```

The script finds the boot medium from where `/boot/efi` is mounted rather than
naming one, so the same script builds either layout.

Steps 1-2 are yours; the script starts at the `apt` and does the rest, stopping
twice for a human: once to confirm it is about to destroy this board's identity,
and once to make you actually walk the setup flow before the master is sealed.
Nothing it runs tests onboarding, and onboarding is the entire product — a
master that installs cleanly and cannot be set up is the most expensive thing
to discover after pressing a hundred units.

Step 5 exists because `deprovision` printing "safe to image" is not proof: an
operator who boots the box once more (to check something, to be sure) ships a
master whose machine-id and SSH host keys have been re-minted, with no signal
that anything is wrong. `virtues image-check` is read-only, exits non-zero on
any finding, and is meant to be the last line of a manufacturing script:

```bash
sudo virtues deprovision --yes && sudo virtues image-check && sudo poweroff
```

It checks: no `VIRTUES_ENCRYPTION_KEY` in the env file · the first-boot marker
is armed · `/etc/machine-id` is empty · no SSH host keys · no saved wifi
connections **in either place they live** · no wifi password left in
`/etc/netplan` · the journal actually got vacuumed · no Tailscale node identity
· no leftover `/var/lib/postgresql.pre-move` · no Postgres cluster on the disk,
or if there is one, no `virtues` database in it · the lake is empty.

Three of those exist because a remedy and its check once disagreed about where
to look:

**The wifi password was not where either of them looked.** On Ubuntu,
NetworkManager is a netplan *renderer*, not the system of record. Joining a
network writes `/etc/netplan/90-NM-<uuid>.yaml` holding the SSID, the password
in plain text, and on a corporate network the 802.1X identity; NM's own profile
directory stays empty and the copy it runs from lives in `/run`, which is
tmpfs. So deprovision wiped an empty directory, reported success, and
image-check inspected the same empty directory and signed the master off — with
the build board's corporate wifi account readable on every unit.

**The journal vacuum could not have worked.** journald names its directory
after the machine-id, and deprovision cleared machine-id one step *before*
vacuuming — so `journalctl` answered "No journal files were found", freed 0 B,
and the master's history stayed on the image while the tick printed, because
the result was discarded. Deprovision now vacuums before clearing machine-id,
and image-check reads the journal itself.

**Tailscale's `tailscaled.state` is a node key** — every clone would join the
tailnet as the same node. It is flagged rather than removed: logging out a
remote operator mid-run strands a half-deprovisioned board nobody can reach,
and it is not our product to silently delete.

The lesson is worth more than the three fixes. **A check that looks somewhere
its own remedy does not is worse than no check, because it signs off.** Where
both need a path, one of them now owns the constant and the other imports it.

### Cutting the artifact

Everything above happens on the board and ends at `poweroff`. What follows —
medium out, `dd`, compress, checksum, record — is `tools/cut-image.sh`, run on
the host with the NVMe in a USB adapter (or a card in a reader):

```bash
sudo sh tools/cut-image.sh /dev/disk4 v0.3.1
```

It refuses the obvious system disks, makes you retype the device, asks whether
`image-check` passed (it **cannot** verify that itself — the root filesystem is
ext4 and the host may be a Mac, so it records an operator assertion rather than
guessing), then reads and compresses in one pass and writes three files that
must travel together: the `.img.zst`, its `.sha256`, and a `.json` record
naming the tag, the base OS image and the medium size.

On an NVMe-both master it parses the GPT first: the read stops after the OS
partitions and the `virtues-data` partition is dropped from the staged image
(this needs docker). Any parse doubt falls back to a whole-device read, which
is always correct, merely slower. Shrinking runs as the user
(`tools/shrink-image.sh`), because its docker loop fails under sudo.

**Zero or trim the free space on the board first** (`build-dragon.sh` runs
`fstrim`). A whole-device read otherwise carries gigabytes of noise —
including everything deprovision just deleted, still recoverable.

Store masters **privately**, and keep every one you ship. The image contains
Qualcomm firmware from the vendor BSP, which we do not redistribute — the same
reason the box fetches QAIRT libraries rather than us hosting them. Not GitHub
Releases (public assets, 2 GB cap); not `virtues.com/downloads`, which is the
installer's path. A private bucket and an expiring presigned URL.

That last one has two ways to pass and the order matters. On a relocated
appliance, deprovision removes the whole **cluster**, so there is no server left
to ask and "Postgres is unreachable" is the correct end state. Asking first
whether a cluster exists is what separates that from the case where one does
exist and will not answer — which is a **finding**, because "I could not check
the most important thing" must never render as a tick.

### Why each of those matters

The iroh secret in `box_secrets` **is** the box's network identity. Two units
flashed from a master that still had one are not similar boxes; they are the
same box — a device paired to one dials the other, and the relay cannot tell
them apart. One surviving encryption key decrypts every unit ever shipped. A
shared machine-id collides in DHCP and journald; shared SSH host keys make
every unit impersonable as every other. And saved wifi ships the workshop's
password to customers.

All five are invisible on the bench and unfixable in the field. That asymmetry
is why the check is a hard gate rather than a warning.

## The M.2 → USB adapter

On NVMe-both it **is** the manufacturing step: the master is flashed onto each
unit's NVMe over a USB-C adapter from the host, the NVMe is fitted, and the
unit carves its own data partition on first boot. It is also field repair: image
a replacement NVMe, or read a customer's disk when their box will not boot.

## Open — needs the bench

**A power-cycle test with the data partition missing.** Confirm that a unit
whose `virtues-data` partition cannot be claimed or mounted still boots,
Postgres refuses to start (the conditioned `ExecStartPre`), and the panel says
*"Storage disconnected"* rather than reporting itself healthy.

## The vendor OS updater is masked

`systemd-sysupdate` is Radxa's own OS auto-updater — a second, self-updating
release channel underneath ours, which is precisely what we rejected snap
Chromium for. The installer masks its timers and services
(`tools/virtues-installer/src/install.rs`), masked rather than disabled so a
distro package update cannot re-enable them.
