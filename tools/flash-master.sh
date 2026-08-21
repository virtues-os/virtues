#!/bin/sh
# Flash a pressed master onto a unit's boot medium (the NVMe in the USB
# adapter, or a microSD in a reader).
#
#     sudo sh tools/flash-master.sh                 # newest master, auto-detect disk
#     sudo sh tools/flash-master.sh /dev/disk6      # explicit device
#     sudo sh tools/flash-master.sh /dev/disk6 masters/virtues-master-v0.1.2-20260821.img.zst
#
# This exists because the per-unit flash is the step that repeats — once per
# shipped unit — and every repetition is one mistyped disk number away from
# overwriting the wrong drive. So the script does the three things a tired
# human skips: verifies the artifact's checksum BEFORE writing (a corrupt
# master flashed to ten units is ten strange field failures), confirms the
# target is the one external disk it detected, and checks the medium is big
# enough for the shrunk image (first boot grows everything to fill).
#
# A blank NVMe shows up on macOS with a "disk not readable" dialog — that is
# what blank looks like; click Ignore and run this.
set -eu

say()  { printf '\n\033[1m∴  %s\033[0m\n' "$*"; }
warn() { printf '\033[33m⚠  %s\033[0m\n' "$*"; }
die()  { printf '\n\033[1;31m✖  %s\033[0m\n\n' "$*" >&2; exit 1; }

[ "$(id -u)" -eq 0 ] || die "run me as root: sudo sh tools/flash-master.sh [device] [image]"
command -v zstd >/dev/null 2>&1 || die "zstd not found (brew install zstd / apt install zstd)"

DEV="${1:-}"
IMG="${2:-}"

# ── Pick the image: newest master unless told otherwise ─────────────────────
if [ -z "$IMG" ]; then
    IMG=$(ls -t masters/*.img.zst 2>/dev/null | head -1 || true)
    [ -n "$IMG" ] || die "no masters/*.img.zst found — press one first (build-dragon.sh, then cut-image.sh)"
fi
[ -f "$IMG" ] || die "$IMG is not a file"
BASE="${IMG%.img.zst}"

# ── Verify the artifact before it touches a unit ────────────────────────────
if [ -f "$BASE.sha256" ]; then
    say "Verifying $(basename "$IMG") against its checksum"
    if command -v shasum >/dev/null 2>&1; then
        ( cd "$(dirname "$IMG")" && shasum -a 256 -c "$(basename "$BASE").sha256" >/dev/null ) \
            || die "checksum MISMATCH — do not flash this file; re-download or re-cut it"
    else
        ( cd "$(dirname "$IMG")" && sha256sum -c "$(basename "$BASE").sha256" >/dev/null ) \
            || die "checksum MISMATCH — do not flash this file; re-download or re-cut it"
    fi
else
    warn "no $(basename "$BASE").sha256 next to the image — flashing unverified"
fi

# ── Pick the device: the one external disk, or an explicit argument ─────────
OS="$(uname -s)"
if [ -z "$DEV" ]; then
    [ "$OS" = "Darwin" ] || die "on Linux, name the device explicitly: sudo sh tools/flash-master.sh /dev/sdX"
    EXTERNALS=$(diskutil list external physical 2>/dev/null | awk '/^\/dev\/disk/{print $1}')
    COUNT=$(printf '%s\n' "$EXTERNALS" | grep -c . || true)
    [ "$COUNT" -eq 1 ] || die "expected exactly ONE external disk, found ${COUNT:-0}:
$(diskutil list external physical 2>/dev/null | sed 's/^/       /')
       Plug in exactly the medium to flash, or name it: sudo sh tools/flash-master.sh /dev/diskN"
    DEV="$EXTERNALS"
fi
case "$DEV" in
    /dev/disk0|/dev/disk1|/dev/sda|/dev/nvme0n1|/dev/mmcblk0)
        die "$DEV is very likely this machine's own disk. Refusing." ;;
esac
[ -e "$DEV" ] || die "$DEV does not exist"

if [ "$OS" = "Darwin" ]; then
    diskutil info "$DEV" 2>/dev/null | grep -qi "Removable Media:.*Removable" \
        || warn "$DEV does not report as removable media — check it twice."
    SIZE=$(diskutil info "$DEV" 2>/dev/null | awk -F'[()]' '/Disk Size/{print $2}' | awk '{print $1}')
    WRITE_DEV="$(echo "$DEV" | sed 's|/dev/disk|/dev/rdisk|')"
else
    SIZE=$(blockdev --getsize64 "$DEV" 2>/dev/null || echo "")
    WRITE_DEV="$DEV"
fi

# The record knows the minimum medium the shrunk image restores onto; a
# too-small target gets a truncated partition table and fails strangely later.
MIN_BYTES=$(sed -n 's/.*"restore_min_card_bytes": "\([0-9]*\)".*/\1/p' "$BASE.json" 2>/dev/null | head -1)
if [ -n "${MIN_BYTES:-}" ] && [ -n "${SIZE:-}" ] && [ "$SIZE" -lt "$MIN_BYTES" ]; then
    die "$DEV is $SIZE bytes but this master needs >= $MIN_BYTES — use a bigger medium"
fi

say "About to WIPE $DEV (${SIZE:-size unknown} bytes) and flash $(basename "$IMG")"
diskutil list "$DEV" 2>/dev/null || lsblk "$DEV" 2>/dev/null || true
printf 'Type the device path again to confirm: '
read -r confirm
[ "$confirm" = "$DEV" ] || die "mismatch — nothing was written"

[ "$OS" = "Darwin" ] && diskutil unmountDisk "$DEV" >/dev/null 2>&1 || true

say "Flashing"
zstd -dc "$IMG" | dd of="$WRITE_DEV" bs=4M
sync

if [ "$OS" = "Darwin" ]; then
    diskutil eject "$DEV" >/dev/null 2>&1 || true
fi

say "Done — safe to unplug"
cat <<'EOF'

  Fit it in the unit and power on. First boot carves the data partition to
  fill the medium, mints this unit's identity, and shows the setup screen.
  Repeat with the next medium: plug in, Ignore the "not readable" dialog,
  run this again.

EOF
