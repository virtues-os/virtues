# The appliance image — how a Dragon becomes a product

> How a Radxa Dragon Q6A goes from a board in a box to a unit a customer can
> plug in. Written 2026-08-17, after reading the boot chain off a live board
> rather than off the docs — which described the opposite layout.
>
> Companion to [onboarding.md](onboarding.md) (what the owner does),
> [deployment.md](deployment.md) (how the software ships), and
> [recovery.md](recovery.md) (what happens when it breaks).

## The one thing to know first

**The Q6A does not boot from the NVMe.** It boots systemd-boot from an ESP on
the **eMMC**, and that ESP holds the kernel, the initrd and the device tree.
The NVMe only supplies the root filesystem, named by UUID in a loader entry
that also lives on the eMMC.

Measured on the lab board:

```
mmcblk1 (eMMC, 29 GiB)                nvme0n1 (119 GiB)
├─p1   16M vfat  /config          ← Qualcomm boot config (vendor; never touch)
├─p2    1G vfat  /boot/efi        ← ESP: systemd-boot + kernel + initrd + DTB
└─p3  28.1G ext4                  ← the stock Ubuntu root (unused today)
                                      └─p1  119.2G ext4  /   ← our root today
```

`efibootmgr` reports exactly one entry — `\EFI\systemd\systemd-bootaa64.efi` on
eMMC p2 — and the NVMe carries a single partition with no ESP.

Two consequences fall out of this, and every decision below is downstream of
them:

1. **Flashing only the NVMe cannot boot a virgin board.** The kernel is not on
   the disk you flashed. Each board's eMMC must be prepared regardless.
2. **The kernel and its modules can drift apart silently.** `/lib/modules/<ver>`
   travels with the NVMe root; the matching kernel travels with the eMMC. Image
   them at different times and you get a box that boots into a rootfs with no
   modules for its kernel.

There is a third, subtler one. The lab board's `RadxaOS-nvme.conf` is
hand-edited — it declares `version 6.18.2-99-qcom` while pointing at the
`6.18.2-3-qcom` kernel, a sort-key hack to win the default. `kernel-install`
regenerates loader entries on every apt kernel upgrade and will not preserve
it. That is a landmine on the master, not just on clones.

## The layout we ship

**Split by write rate, not by size.** The eMMC is soldered — if it wears out,
the board is scrap — and its endurance is a function of writes. The NVMe is
replaceable and has real endurance. So:

| Medium | Holds | Write rate |
|---|---|---|
| **eMMC** | `/config`, the ESP, and the **root filesystem**: OS, `virtues` binary, web assets, models | once per release |
| **NVMe** | `/var/lib/virtues` — Postgres, the lake, the journal, backups, applet state | continuous |

This inverts what the lab board does today, and the inversion is the point.

**Why not NVMe-only root** (what the lab board runs): a box whose NVMe is dead,
unseated, or never fitted becomes a black screen with no way to tell anyone
what is wrong. That is exactly the "dead and unrecoverable for support" case.
With the OS on the eMMC the same box boots, the panel comes up, and it says
*"I can't find my storage disk. Your record is on it, not lost."*
(`data_disk.rs`) — a mail-out instead of an RMA.

**Why not eMMC-only:** 29 GiB total, and the database plus the lake would fill
it and then wear it out.

**Why this is also the simplest manufacturing story:** the ship image is the
eMMC — small, identical across units, one `dd` target. The NVMe ships blank and
is claimed on first boot (`virtues-firstboot.sh`), so there is no shared root
UUID to coordinate and no kernel/module skew possible.

### Moving the writes

Three things write continuously, and each needed pointing at the data disk
explicitly. Two are done; one is not.

| What | Where it lands by default | How it gets moved |
|---|---|---|
| The lake, backups, applet state | already under `DATA_DIR` | — |
| journald (~140 MB and growing on the lab box) | `/var/log/journal` on root | `virtues-firstboot.sh` symlinks it to `$DATA_DIR/journal` |
| Postgres (3.0 GB on the lab box) | `/var/lib/postgresql/18/main` on root | the installer symlinks `/var/lib/postgresql` → `$DATA_DIR/postgresql` |

### The Postgres move, in detail

It is the largest and busiest of the three — a WAL flush per transaction,
forever — so it is the one the eMMC most needs to be rid of.

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

**On a fresh unit the cluster is built, not inherited.** The image is the eMMC,
so it carries the symlink but not the disk it points at — every unit's NVMe is
blank. `virtues-firstboot.sh` therefore claims the disk and then
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

**Deprovision removes the cluster,** because it is per-unit state — it is where
the record lived. It has to: on the master the data dir is a plain directory on
the eMMC (the master never had a claimed NVMe), so a surviving cluster would
ship inside every image under a path each unit then hides with a mount and
never reads.

## Building the image

```
 1. Flash a stock Radxa image to the eMMC              (per board, once)
 2. Boot it, install Virtues                            curl virtues.com/sh | sudo sh
 3. Verify the box works                                virtues doctor
 4. Strip per-unit identity                             sudo virtues deprovision
 5. Prove it is stripped                                sudo virtues image-check
 6. Power off WITHOUT booting again                     sudo poweroff
 7. Image the eMMC                                      dd
```

Step 5 is new and is the one that was missing. `deprovision` prints
"safe to image" and nothing ever re-read the disk — so an operator who booted
the box once more (to check something, to be sure) shipped a master whose
machine-id and SSH host keys had been re-minted, with no signal that anything
was wrong. `virtues image-check` is read-only, exits non-zero on any finding,
and is meant to be the last line of a manufacturing script:

```bash
sudo virtues deprovision --yes && sudo virtues image-check && sudo poweroff
```

It checks: no `VIRTUES_ENCRYPTION_KEY` in the env file · the first-boot marker
is armed · `/etc/machine-id` is empty · no SSH host keys · no saved wifi
connections · no leftover `/var/lib/postgresql.pre-move` · no Postgres cluster
on the disk, or if there is one, no `virtues` database in it · the lake is
empty.

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

## The M.2 → USB flasher

Under this layout the flasher is **not** part of manufacturing. Every unit's
NVMe ships blank and is claimed on first boot, so there is nothing to write to
it. The flasher's job is **field repair and recovery**: image a replacement
NVMe with a known-good data skeleton, or read a customer's disk when their box
will not boot.

That is a better use for it than the alternative, which would have required
per-unit coordination of a shared root filesystem UUID.

## Open — needs the bench

Three things cannot be settled by reading code, in the order they gate the plan.

**1. Does a `dd`'d eMMC boot on a board it was not imaged on?**
Twenty minutes with two boards, and it de-risks the entire manufacturing plan.
The firmware in `/config` and the ESP have to travel correctly. If they do not,
step 7 above needs a per-board firmware flash over USB-C (`rsetup` / EDL)
before the `dd`.

**2. Can the NVMe be made self-contained?**
Partition it `p1 = ESP` (with `\EFI\BOOT\BOOTAA64.EFI` and its own loader
entries) `+ p2 = root`, and see whether the Q6A's EDK2 enumerates NVMe via the
removable fallback path. If it does, the kernel, initrd, DTB and root travel
together, kernel/module skew becomes impossible by construction, and the M.2
flasher becomes the *entire* per-unit process. Worth knowing even though the
recommended layout does not need it — it is the fallback if question 1 goes
badly.

**3. A real power-cycle test of the first-boot cluster build.**
The Postgres move is written and the guards are in place, but the path that
matters most has only been reasoned about: image a unit, boot it with a blank
NVMe, and confirm `pg_createcluster` runs before Postgres is wanted, the role
and database appear, and `virtues server` migrates into them. Then do it again
with the NVMe physically absent and confirm the box boots, Postgres refuses to
start, and the panel says *"Storage disconnected"* rather than reporting itself
healthy.

## Two things to fix on the master while you are in there

**`systemd-sysupdate.timer` is enabled and failing.** That is Radxa's own OS
auto-updater — a second, self-updating release channel underneath ours, which
is precisely what we rejected snap Chromium for. Mask it in the appliance
profile.

**The hand-edited loader entry.** `RadxaOS-nvme.conf` will not survive a
kernel upgrade. Whatever the layout ends up being, the boot entry needs to be
generated rather than hand-held, or pinned with the kernel package held.
