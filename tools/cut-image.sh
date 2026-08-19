#!/bin/sh
# Cut a distributable master image from the card `build-dragon.sh` produced.
#
# Runs on the HOST (Mac or Linux) with the master's microSD in a reader, after
# the board has been deprovisioned, image-checked and powered off.
#
#     sudo sh tools/cut-image.sh /dev/disk4 v0.3.1
#
# Produces, in ./masters/:
#     virtues-master-<tag>-<date>.img.zst   the product
#     virtues-master-<tag>-<date>.sha256    checksum of the COMPRESSED file
#     virtues-master-<tag>-<date>.json      the record
#
# ## Why a script rather than a line in a doc
#
# The `dd` is the easy part. What gets skipped is everything around it: naming
# the artifact after the tag it actually contains, recording what went into it,
# and checksumming it so the person flashing can tell a bad download from a bad
# card. A master with no record is unreproducible eight months later, which is
# exactly when a unit misbehaves and you need to know what it started as.
#
# ## What this script CANNOT do, and does not pretend to
#
# It cannot verify the card was deprovisioned. The root filesystem is ext4 and
# this may be a Mac; even on Linux, re-checking would mean re-implementing
# `image-check` against a mounted image. That check already exists and already
# runs — on the board, as a hard gate in `build-dragon.sh`. So this asks the
# operator to paste its verdict into the record rather than guessing at it.
#
# A record that says "the operator asserted it" is honest. One that says
# "verified" because a weaker check passed is worse than no record at all.

set -eu

say()  { printf '\n\033[1m∴  %s\033[0m\n' "$*"; }
warn() { printf '\033[33m⚠  %s\033[0m\n' "$*"; }
die()  { printf '\n\033[1;31m✖  %s\033[0m\n\n' "$*" >&2; exit 1; }

[ "$(id -u)" -eq 0 ] || die "run me as root: sudo sh tools/cut-image.sh <device> <tag>"

DEV="${1:-}"
TAG="${2:-}"
[ -n "$DEV" ] || die "which device? e.g. /dev/disk4 (macOS) or /dev/sdb (Linux)
       macOS:  diskutil list
       Linux:  lsblk"
[ -n "$TAG" ] || die "which tag is on this card? e.g. v0.3.1
       It names the artifact. Guessing here mislabels a master permanently."

command -v zstd >/dev/null 2>&1 || die "zstd not found (brew install zstd / apt install zstd)"

# ── Refuse the system disk ──────────────────────────────────────────────────
# `dd` at the wrong device is unrecoverable and the failure mode is the whole
# machine. The cheap guards are worth more than they look.
case "$DEV" in
    /dev/disk0|/dev/disk1|/dev/sda|/dev/nvme0n1|/dev/mmcblk0)
        die "$DEV is very likely this machine's own disk. Refusing." ;;
esac
[ -e "$DEV" ] || die "$DEV does not exist"

OS="$(uname -s)"
if [ "$OS" = "Darwin" ]; then
    diskutil info "$DEV" 2>/dev/null | grep -qi "Removable Media:.*Removable" \
        || warn "$DEV does not report as removable media — check it twice."
    SIZE=$(diskutil info "$DEV" 2>/dev/null | awk -F'[()]' '/Disk Size/{print $2}' | awk '{print $1}')
    diskutil list "$DEV" || true
    # The raw device skips the buffer cache: same bytes, several times faster.
    READ_DEV="$(echo "$DEV" | sed 's|/dev/disk|/dev/rdisk|')"
else
    lsblk -o NAME,SIZE,TYPE,MOUNTPOINT "$DEV" || true
    SIZE=$(blockdev --getsize64 "$DEV" 2>/dev/null || echo "")
    READ_DEV="$DEV"
fi

say "About to read $DEV (${SIZE:-size unknown}) as the master for $TAG"
printf 'Type the device path again to confirm: '
read -r confirm
[ "$confirm" = "$DEV" ] || die "mismatch — nothing was read"

# ── The evidence ────────────────────────────────────────────────────────────
# Asked, not inferred. See the header.
printf '\nDid `virtues image-check` PASS on the board before poweroff? [yes/no] '
read -r checked
[ "$checked" = "yes" ] || die "then this card is not a master yet. Put it back in the
       board, run: sudo virtues deprovision && sudo virtues image-check"

printf 'Base OS image (e.g. "radxa ubuntu 24.04 b9"), or blank if unknown: '
read -r BASE_IMAGE

[ "$OS" = "Darwin" ] && diskutil unmountDisk "$DEV" >/dev/null 2>&1

# ── The operator's last session, which cannot delete itself ─────────────────
# deprovision removes shell histories, but every shell still alive at that
# moment flushes its history again at shutdown — including the very session
# that ran the seal. Both masters cut so far carried them. No box-side fix can
# win that race; the cutting bench is the only place that outlives the shells,
# so scrub here, on the raw partition, when debugfs is available. Best-effort:
# the histories hold typed commands, not secrets (passwords go through
# prompts), so a host without e2fsprogs still cuts — with the residue noted.
DEBUGFS=""
for cand in debugfs /opt/homebrew/opt/e2fsprogs/sbin/debugfs /usr/sbin/debugfs; do
    command -v "$cand" >/dev/null 2>&1 && { DEBUGFS="$cand"; break; }
done
ROOT_PART=""
if [ "$OS" = "Darwin" ]; then
    # Largest Linux partition = the rootfs. Buffered node: raw rdisk rejects
    # debugfs/e2fsck's unaligned writes with EINVAL, observed 2026-08-19.
    ROOT_PART=$(diskutil list "$DEV" 2>/dev/null | awk '/Linux Filesystem/{p=$NF} END{if(p) print "/dev/"p}')
fi
if [ -n "$DEBUGFS" ] && [ -n "$ROOT_PART" ]; then
    say "Scrubbing shell histories from the sealed card"
    for f in /root/.bash_history /home/radxa/.bash_history /root/build-dragon.sh /root/bd.sh /root/build.log; do
        "$DEBUGFS" -w -R "rm $f" "$ROOT_PART" >/dev/null 2>&1 || true
    done
    command -v e2fsck >/dev/null 2>&1 && FSCK=e2fsck || FSCK="$(dirname "$DEBUGFS")/e2fsck"
    "$FSCK" -fy "$ROOT_PART" >/dev/null 2>&1 || true
else
    warn "no debugfs available — if a shell was open during the seal, its history is in this image"
fi

DATE="$(date -u +%Y%m%d)"
OUT="masters/virtues-master-$TAG-$DATE"
mkdir -p masters

# ── Read, compress, checksum — in one pass ──────────────────────────────────
# Piped rather than staged: a 64 GB card written out raw and compressed
# afterwards needs 64 GB of scratch and twice the wall clock, for a file that is
# deleted immediately. `-T0` uses every core; `-19` is worth it because this is
# written once and downloaded repeatedly.
#
# The card is read WHOLE, including free space. Zero it on the board first
# (fstrim, or fill-and-delete) or the image carries gigabytes of noise —
# including everything deprovision just deleted, still recoverable.
say "Reading and compressing → $OUT.img.zst"
warn "This takes a while. A 32 GB card is ~10 minutes on a fast reader."
dd if="$READ_DEV" bs=4m 2>/dev/null | zstd -19 -T0 -o "$OUT.img.zst"

say "Checksumming"
if command -v shasum >/dev/null 2>&1; then
    ( cd masters && shasum -a 256 "$(basename "$OUT").img.zst" > "$(basename "$OUT").sha256" )
else
    ( cd masters && sha256sum "$(basename "$OUT").img.zst" > "$(basename "$OUT").sha256" )
fi

# ── The record ──────────────────────────────────────────────────────────────
cat > "$OUT.json" <<EOF
{
  "tag": "$TAG",
  "cut_utc": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "cut_by": "$(id -un)@$(hostname)",
  "base_os_image": "${BASE_IMAGE:-unrecorded}",
  "card_bytes": "${SIZE:-unknown}",
  "source_device": "$DEV",
  "image_check": "asserted PASS by operator on the board before poweroff",
  "artifact": "$(basename "$OUT").img.zst",
  "sha256_file": "$(basename "$OUT").sha256"
}
EOF

say "Done"
ls -la "$OUT".*
cat <<'EOF'

  Next:
    • Keep all three files together. An image with no record is
      unreproducible, and eight months from now that is when you need it.
    • Store PRIVATELY — the image contains Qualcomm firmware from the vendor
      BSP, which we do not redistribute. Not GitHub Releases (public assets,
      2 GB cap), not virtues.com/downloads (that is the installer's path).
    • Hand it over as an expiring presigned URL, with the .sha256, and have
      the recipient verify BEFORE flashing. A corrupt card fails strangely
      rather than loudly.
    • Keep every master you ever ship.

EOF
