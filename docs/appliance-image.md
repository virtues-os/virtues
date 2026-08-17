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

| What | Where it lands by default | Status |
|---|---|---|
| The lake, backups, applet state | already under `DATA_DIR` | ✅ |
| journald (~140 MB and growing on the lab box) | `/var/log/journal` on root | ✅ symlinked to `$DATA_DIR/journal` by `virtues-firstboot.sh` |
| **Postgres (3.0 GB on the lab box)** | `/var/lib/postgresql/18/main` on root | ❌ **not yet moved — see below** |

Postgres is the largest and busiest of the three, and it is the one still
landing on the eMMC. Relocating a cluster is not a config edit — the data
directory has to be created on a disk that is blank at first boot, which means
the unit has to `initdb` and migrate on that boot rather than inheriting a
cluster from the image. That is real work with a real failure mode, and it
wants the bench. **Until it lands, the endurance argument above is only
two-thirds true.**

What *is* in place is the guard that makes the failure loud instead of silent.
`postgresql@.service` now carries a `RequiresMountsFor=<data dir>` drop-in, so
a box with no data disk refuses to start Postgres rather than `initdb`-ing a
fresh empty cluster onto the eMMC and reporting itself healthy. `virtues.service`
already had the same guard; it was on the wrong unit, because Postgres starts
first.

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
connections · the `virtues` database is gone · the lake is empty. A Postgres it
cannot reach is a **finding**, not a pass — "I could not check the most
important thing" must never render as a tick.

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

**3. Postgres onto the data disk.**
See the table above. Needs a first-boot `initdb` + migrate path and a real
power-cycle test, because the failure mode is an empty box that looks healthy.

## Two things to fix on the master while you are in there

**`systemd-sysupdate.timer` is enabled and failing.** That is Radxa's own OS
auto-updater — a second, self-updating release channel underneath ours, which
is precisely what we rejected snap Chromium for. Mask it in the appliance
profile.

**The hand-edited loader entry.** `RadxaOS-nvme.conf` will not survive a
kernel upgrade. Whatever the layout ends up being, the boot entry needs to be
generated rather than hand-held, or pinned with the kernel package held.
