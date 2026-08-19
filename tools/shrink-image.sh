#!/bin/sh
# Shrink a raw master image to its content, so it restores onto any card.
#
#     sh tools/shrink-image.sh masters/virtues-master-<tag>-<date>.img
#
# The problem this solves: the base OS grows its root filesystem to fill the
# card it was installed on, so a master cut from a 64 GB card is a 64 GB image
# even though the content is ~10 GB — and a byte-for-byte restore then demands
# a card at least as large as the ORIGINAL, cut mid-partition on anything
# smaller. Compression hides this (free space squeezes to nothing) right up
# until someone flashes a 32 GB card and gets a truncated partition table.
#
# What it does, in place, to the .img file:
#   1. shrink the last (root) ext4 filesystem to its minimum + 2 GiB margin
#   2. shrink that partition to match
#   3. truncate the image just past the partition
#   4. relocate the GPT backup header to the new end (sfdisk --relocate)
#   5. verify: sfdisk --verify + a forced e2fsck + a mounted spot-check
#
# The margin keeps the OS breathing room even if nothing ever grows it back;
# on real hardware, virtues-firstboot §0b grows the partition and filesystem
# to fill whatever card the image landed on, so nothing is lost.
#
# ext4 tooling does not exist on macOS, so the work runs in a privileged Linux
# container (ubuntu:24.04). Docker (or OrbStack) must be running, and the
# container needs network once per machine: Ubuntu packages sfdisk in `fdisk`,
# which is not in the base image (found the hard way — `sfdisk: not found`).
set -eu

say()  { printf '\n\033[1m∴  %s\033[0m\n' "$*"; }
die()  { printf '\n\033[1;31m✖  %s\033[0m\n\n' "$*" >&2; exit 1; }

IMG="${1:-}"
[ -n "$IMG" ] || die "which image? e.g. sh tools/shrink-image.sh masters/foo.img"
[ -f "$IMG" ] || die "$IMG is not a file (decompress the .zst first: zstd -d <file>.img.zst)"
command -v docker >/dev/null 2>&1 || die "docker not found — the ext4 shrink needs a Linux container"
docker info >/dev/null 2>&1 || die "docker daemon not running"

DIR="$(cd "$(dirname "$IMG")" && pwd)"
NAME="$(basename "$IMG")"
BEFORE=$(wc -c < "$IMG" | tr -d ' ')

say "Shrinking $NAME ($((BEFORE / 1024 / 1024 / 1024)) GiB) in a Linux container"
rm -f "$DIR/.shrink-ok"

docker run --rm --privileged -v "$DIR":/work -e NAME="$NAME" ubuntu:24.04 sh -eu -c '
command -v sfdisk >/dev/null 2>&1 && command -v sgdisk >/dev/null 2>&1 || {
    echo "→ installing fdisk + gdisk (not in the base image)"
    export DEBIAN_FRONTEND=noninteractive
    apt-get update -qq >/dev/null 2>&1
    apt-get install -yqq fdisk gdisk >/dev/null 2>&1
}
command -v sfdisk >/dev/null 2>&1 || { echo "sfdisk unavailable (no network?)"; exit 1; }
command -v sgdisk >/dev/null 2>&1 || { echo "sgdisk unavailable (no network?)"; exit 1; }
IMG="/work/$NAME"
LOOP=$(losetup -f --show -P "$IMG")
trap "losetup -d $LOOP 2>/dev/null || true" EXIT

# No udev in a container: the kernel registers loop partitions in /sys but
# nothing creates their /dev nodes. Make them by hand from the sysfs numbers.
mknodes() {
    for d in "/sys/block/$(basename "$LOOP")/$(basename "$LOOP")"p*; do
        [ -d "$d" ] || continue
        n=$(basename "$d")
        mm=$(cat "$d/dev")
        # Recreate unconditionally: a node left from an earlier attach can
        # point at numbers the kernel has since retired ("No such device or
        # address" from tools that then open it).
        rm -f "/dev/$n"
        mknod "/dev/$n" b "${mm%:*}" "${mm#*:}"
    done
}
mknodes

# The last partition is the rootfs — the one the base OS grew to fill the card.
# Found via sysfs rather than parsing sfdisk -d, whose "start=   N" spacing has
# already eaten one afternoon of awk.
SECTOR=512
PART_NAME=$(lsblk -bnro NAME,START,TYPE "$LOOP" | awk "\$3 == \"part\" { if (\$2+0 > s) { s = \$2+0; n = \$1 } } END { print n }")
[ -n "$PART_NAME" ] || { echo "could not find a partition inside the image"; exit 1; }
PART_DEV="/dev/$PART_NAME"
PART_START=$(cat "/sys/class/block/$PART_NAME/start")
PART_NUM=$(cat "/sys/class/block/$PART_NAME/partition")
[ -b "$PART_DEV" ] || { echo "no partition device $PART_DEV"; exit 1; }
blkid "$PART_DEV" | grep -q "TYPE=\"ext4\"" || { echo "last partition is not ext4 — refusing"; exit 1; }

echo "→ fsck + shrink the filesystem to minimum"
e2fsck -fy "$PART_DEV" >/dev/null || [ $? -le 2 ]
resize2fs -M "$PART_DEV" >/dev/null 2>&1
resize2fs -M "$PART_DEV" >/dev/null 2>&1
BLOCK_SIZE=$(dumpe2fs -h "$PART_DEV" 2>/dev/null | awk "/^Block size:/{print \$3}")
BLOCK_COUNT=$(dumpe2fs -h "$PART_DEV" 2>/dev/null | awk "/^Block count:/{print \$3}")
FS_BYTES=$((BLOCK_COUNT * BLOCK_SIZE))

# Partition target = minimum fs + 2 GiB margin, rounded up to 1 MiB.
TARGET_BYTES=$((FS_BYTES + 2 * 1024 * 1024 * 1024))
TARGET_SECTORS=$(( (TARGET_BYTES + 1048575) / 1048576 * 1048576 / SECTOR ))
echo "→ fs content $((FS_BYTES / 1024 / 1024)) MiB; partition target $((TARGET_SECTORS * SECTOR / 1024 / 1024)) MiB"

echo "→ shrink partition $PART_NUM"
printf ",%s\n" "$TARGET_SECTORS" | sfdisk --force -N "$PART_NUM" "$LOOP" >/dev/null
# The sfdisk rewrite drops the kernel view of the loop partitions; re-attach
# for a clean one rather than negotiating with partx. (NO APOSTROPHES in this
# whole container script — it is one single-quoted shell string, and one
# apostrophe in a comment truncated it mid-run, silently, exit 0.)
losetup -d "$LOOP"
LOOP=$(losetup -f --show -P "$IMG")
trap "losetup -d $LOOP 2>/dev/null || true" EXIT
PART_DEV="/dev/$(basename "$LOOP")p$PART_NUM"
mknodes

echo "→ grow the fs back to fill its (smaller) partition"
e2fsck -fy "$PART_DEV" >/dev/null || [ $? -le 2 ]
resize2fs "$PART_DEV"

NEW_END=$((PART_START + TARGET_SECTORS))
losetup -d "$LOOP"; trap - EXIT

# Truncate just past the partition, leaving room for the 33-sector GPT backup,
# rounded to 1 MiB so the size reads sanely everywhere.
NEW_BYTES=$(( (NEW_END + 34) * SECTOR ))
NEW_BYTES=$(( (NEW_BYTES + 1048575) / 1048576 * 1048576 ))
echo "→ truncate image to $((NEW_BYTES / 1024 / 1024)) MiB"
truncate -s "$NEW_BYTES" "$IMG"

# Fix the GPT for the new size with sgdisk, ON THE FILE, no loop attached.
# NOT sfdisk: after truncation libfdisk stops recognizing the label as GPT at
# all — it sees only the protective MBR, calls it a DOS disk with one ee
# partition, "verifies" that trivially, and a dump/rewrite "fix" then writes
# a real DOS label over the GPT (learned by doing exactly that). sgdisk -e is
# built for resized disks: moves the backup structures to the new end and
# rewrites both headers and the protective MBR against the current size.
echo "→ move GPT backup structures to the new end (sgdisk -e)"
sgdisk -e "$IMG" >/dev/null

LOOP=$(losetup -f --show -P "$IMG")
trap "losetup -d $LOOP 2>/dev/null || true" EXIT
mknodes

echo "→ verify"
# The one verification that matters first: did the KERNEL accept the table?
# (sfdisk --verify happily blessed a broken image by verifying the protective
# MBR as a one-partition DOS disk, so tool-level verifies are not trusted.)
[ -d "/sys/block/$(basename "$LOOP")/$(basename "$LOOP")p${PART_NUM}" ] || \
    { echo "VERIFY FAILED: kernel rejected the partition table"; exit 1; }
sgdisk -v "$IMG" | grep -q "No problems found" || \
    { echo "VERIFY FAILED: sgdisk reports GPT problems"; sgdisk -v "$IMG"; exit 1; }
PART_DEV="/dev/$(basename "$LOOP")p${PART_NUM}"
e2fsck -fn "$PART_DEV" >/dev/null || { echo "VERIFY FAILED: e2fsck"; exit 1; }
mkdir -p /mnt/verify
mount -o ro "$PART_DEV" /mnt/verify
# -L before -e: the binary is an absolute symlink into the release slots, and
# an absolute symlink under /mnt/verify resolves against the CONTAINER root,
# where the target cannot exist — -e alone reads a healthy image as broken.
[ -L /mnt/verify/usr/local/bin/virtues ] || [ -e /mnt/verify/usr/local/bin/virtues ] || \
    { echo "VERIFY FAILED: virtues binary missing after shrink"; exit 1; }
[ -f /mnt/verify/var/lib/virtues/virtues.env ] || { echo "VERIFY FAILED: card seed env missing after shrink"; exit 1; }
umount /mnt/verify
echo "→ verified: table clean, fsck clean, binary + seed present"
touch /work/.shrink-ok
' || die "container shrink FAILED — do not trust $IMG unless the failure was
       before the first write; when in doubt, re-decompress from the .zst"

# The sentinel guards against the container script being silently truncated
# (a quoting accident once ended it mid-run with exit 0): only the last line
# of the inner script writes it, so its absence means the run did not finish.
[ -f "$DIR/.shrink-ok" ] || die "container script did not run to completion — do not trust $IMG"
rm -f "$DIR/.shrink-ok"

AFTER=$(wc -c < "$IMG" | tr -d ' ')
say "Done: $((BEFORE / 1024 / 1024 / 1024)) GiB → $((AFTER / 1024 / 1024)) MiB (restore card must be ≥ that)"
