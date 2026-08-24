#!/bin/sh
# Cut a distributable master image from the medium `build-dragon.sh` produced.
#
# Runs on the HOST (Mac or Linux) with the master's boot medium attached — the
# microSD in a reader, or the NVMe in a USB adapter — after the board has been
# deprovisioned, image-checked and powered off.
#
#     sudo sh tools/cut-image.sh /dev/disk4 v0.3.1
#
# NVMe-both masters (root on the NVMe, data partition alongside) are detected
# from the GPT: the read is bounded to the OS partitions and the per-unit
# virtues-data partition is dropped from the image — each unit carves its own
# on first boot. See docs/appliance-image.md.
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
# ── NVMe-both: detect the layout BEFORE anything uses partition guesses ─────
# An NVMe-both master carries `p1 config + p2 ESP + p3 root + pN virtues-data`,
# and the data partition is PER-UNIT state that must not ship — every unit
# carves its own on first boot (firstboot §1-NVMe). This parse runs first
# because three later steps depend on knowing the layout: the history scrub
# must target the real root (the last/largest-Linux-partition heuristic picks
# the DATA partition on this layout and scrubs the disk we throw away —
# 2026-08-21 review finding, confirmed against a shipped master), the
# free-space gate must size the bounded read rather than the whole disk, and
# the dd must stop after the OS partitions. Best-effort by design — no
# python3, a 4Kn-formatted disk (header not at byte 512), or any parse doubt
# falls back to the whole-device read, which is always correct, merely
# slower. Parsed from the BUFFERED node ($DEV): macOS raw devices reject
# unaligned reads.
LAYOUT="whole-device"
DD_COUNT=""
DATA_PART_NUM=""
ROOT_IDX=""
OS_END_BYTES=""
if command -v python3 >/dev/null 2>&1; then
    GPT_INFO=$(python3 - "$DEV" <<'PYEOF' 2>/dev/null
import struct, sys
with open(sys.argv[1], 'rb') as f:
    f.seek(512)                       # LBA 1: primary GPT header (512e only)
    hdr = f.read(92)
    if hdr[0:8] != b'EFI PART':
        sys.exit(1)
    entries_lba = struct.unpack_from('<Q', hdr, 72)[0]
    n, esz = struct.unpack_from('<II', hdr, 80)
    if not (0 < n <= 512 and 128 <= esz <= 4096):
        sys.exit(1)
    f.seek(entries_lba * 512)
    data = f.read(n * esz)
parts = []
for i in range(n):
    e = data[i * esz:(i + 1) * esz]
    if e[0:16] == bytes(16):
        continue                      # unused entry
    first, last = struct.unpack_from('<QQ', e, 32)
    name = e[56:56 + 72].decode('utf-16le', 'ignore').rstrip('\x00')
    parts.append((first, last, name, i + 1))
if len(parts) < 2:
    sys.exit(1)
parts.sort()
if parts[-1][2] != 'virtues-data':
    sys.exit(1)
# byte end of the last OS partition; the data partition's GPT entry number
# (to delete); the root partition's entry number (for the history scrub)
print((parts[-2][1] + 1) * 512, parts[-1][3], parts[-2][3])
PYEOF
) || true
    if [ -n "${GPT_INFO:-}" ]; then
        OS_END_BYTES=$(printf '%s' "$GPT_INFO" | awk '{print $1}')
        DATA_PART_NUM=$(printf '%s' "$GPT_INFO" | awk '{print $2}')
        ROOT_IDX=$(printf '%s' "$GPT_INFO" | awk '{print $3}')
        DD_COUNT=$(( (OS_END_BYTES + 4194303) / 4194304 ))
        LAYOUT="nvme-both (OS partitions only; data partition dropped — each unit carves its own on first boot)"
        say "NVMe-both layout detected — will read the OS partitions only ($((DD_COUNT * 4)) MiB) and drop partition $DATA_PART_NUM"
    fi
fi

# The NVMe-both cut cannot finish without docker (the staged image needs its
# GPT patched), so fail here — before the scrub and the dd — not after
# minutes of reading.
docker_u() {
    # One docker context for every docker use in this script: the invoking
    # user's when running under sudo (Docker Desktop's socket and loop
    # plumbing belong to the login session — a root-run container hit EPERM
    # on loop partitions, 2026-08-20), root's otherwise.
    if [ -n "${SUDO_USER:-}" ] && [ "$SUDO_USER" != "root" ]; then
        sudo -H -u "$SUDO_USER" docker "$@"
    else
        docker "$@"
    fi
}
if [ -n "$DATA_PART_NUM" ]; then
    docker_u info >/dev/null 2>&1 || \
        die "an NVMe-both cut needs docker to drop the data partition from the staged image — start it and re-run (nothing has been read yet)"
fi

# Build the partition node for an entry number: /dev/disk6 + 3 -> /dev/disk6s3
# (macOS), /dev/nvme0n1 + 3 -> /dev/nvme0n1p3, /dev/sdb + 3 -> /dev/sdb3.
part_node() {
    case "$1" in
        /dev/disk*) printf '%ss%s' "$1" "$2" ;;
        *[0-9])     printf '%sp%s' "$1" "$2" ;;
        *)          printf '%s%s'  "$1" "$2" ;;
    esac
}

ROOT_PART=""
if [ -n "$ROOT_IDX" ]; then
    # NVMe-both: the GPT parse identified the real root — the heuristics below
    # would pick the data partition on this layout.
    ROOT_PART="$(part_node "$DEV" "$ROOT_IDX")"
elif [ "$OS" = "Darwin" ]; then
    # Largest Linux partition = the rootfs. Buffered node: raw rdisk rejects
    # debugfs/e2fsck's unaligned writes with EINVAL, observed 2026-08-19.
    ROOT_PART=$(diskutil list "$DEV" 2>/dev/null | awk '/Linux Filesystem/{p=$NF} END{if(p) print "/dev/"p}')
else
    # Linux: the largest ext-type partition on the device is the rootfs.
    ROOT_PART=$(lsblk -bnro NAME,FSTYPE,SIZE "$DEV" 2>/dev/null \
        | awk '$2 ~ /^ext/ { if ($3+0 > s) { s=$3+0; n=$1 } } END { if (n) print "/dev/"n }')
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

# ── Read, shrink, compress, checksum ────────────────────────────────────────
# Staged to a raw file rather than piped straight into zstd, because the
# shrink needs a seekable image it can resize and truncate. The stage briefly
# costs the card's full size in scratch space; the shrink is what frees every
# future flash from needing a card as large as the master's — the base OS
# grows its rootfs to fill the build card, so an unshrunk 64 GB master
# compresses to ~5 GB and then still demands a >= 64 GB card to restore
# (a 32 GB card gets a truncated partition table; learned 2026-08-19).
#
# The card is read WHOLE, including free space. Zero it on the board first
# (fstrim, or fill-and-delete) or the image carries gigabytes of noise —
# including everything deprovision just deleted, still recoverable.
RAW="$OUT.img"
# Gate on what will actually be staged: the bounded OS-only read for an
# NVMe-both cut (a 238 GB disk stages ~9 GiB), the whole device otherwise.
STAGE_BYTES="${SIZE:-}"
[ -n "$DD_COUNT" ] && STAGE_BYTES=$((DD_COUNT * 4194304))
if [ -n "${STAGE_BYTES:-}" ]; then
    AVAIL_KB=$(df -k masters | awk 'NR==2{print $4}')
    [ $((AVAIL_KB * 1024)) -gt $((STAGE_BYTES + 2147483648)) ] || \
        die "not enough free space here to stage the raw image ($STAGE_BYTES bytes + margin needed)"
fi

say "Reading → $RAW"
[ -n "$DD_COUNT" ] || warn "This takes a while. A 64 GB card is ~15-25 minutes on a fast reader."
# bs=4M (uppercase) works on BOTH GNU and BSD dd; lowercase 'm' is a BSD-ism
# that GNU dd rejects with "invalid number", which under set -eu killed this
# script silently on the advertised Linux path (2026-08-19). No 2>/dev/null —
# a read failure of the master card must be loud.
dd if="$READ_DEV" of="$RAW" bs=4M ${DD_COUNT:+count=$DD_COUNT}

if [ -n "$DATA_PART_NUM" ]; then
    # The 4 MiB-block read overshoots the last OS partition's end by up to one
    # block, staging the first bytes of the live data partition. The shrink
    # would truncate them away, but the shrink-failure fallback ("compress by
    # hand") would ship them — so cut the file to the exact OS end now.
    # (python3 is guaranteed on this path: the detection required it. macOS
    # has no truncate(1).)
    python3 -c 'import os, sys; os.truncate(sys.argv[1], int(sys.argv[2]))' "$RAW" "$OS_END_BYTES"
    # Give the staged file to the invoking user before any container touches
    # it — the same ownership the shrink below needs.
    if [ -n "${SUDO_USER:-}" ] && [ "$SUDO_USER" != "root" ]; then
        chown "$SUDO_USER" "$RAW" masters 2>/dev/null || true
    fi
    # The stage still lists the data partition and has no backup GPT at its
    # (now exact) end. Fix both before anything else touches it: sgdisk on the
    # FILE — not sfdisk, which stops recognizing a truncated GPT (see
    # shrink-image.sh) — and refuse to continue on any doubt, because a master
    # that ships a phantom virtues-data entry would break every unit's
    # first-boot carve.
    docker_u run --rm -v "$(cd masters && pwd)":/work -e RAWNAME="$(basename "$RAW")" -e PNUM="$DATA_PART_NUM" ubuntu:24.04 sh -eu -c '
        command -v sgdisk >/dev/null 2>&1 || { apt-get update -qq >/dev/null 2>&1; apt-get install -yqq gdisk >/dev/null 2>&1; }
        sgdisk -e "/work/$RAWNAME" >/dev/null 2>&1 || true
        sgdisk -d "$PNUM" "/work/$RAWNAME" >/dev/null
        sgdisk -e "/work/$RAWNAME" >/dev/null 2>&1 || true
        sgdisk -v "/work/$RAWNAME" | grep -q "No problems found"
    ' || die "GPT patch failed — the staged image at $RAW still lists the data partition; do not ship it"
    say "Data partition dropped; GPT verified clean"
fi

# Probe docker in the SAME context the shrink will run in (docker_u), so the
# gate and the execution cannot disagree — a root-side probe passing while the
# demoted shrink cannot reach the daemon (or vice versa) turns a config issue
# into a "shrink failed" after the whole dd.
if command -v docker >/dev/null 2>&1 && docker_u info >/dev/null 2>&1; then
    # Under sudo, hand the shrink to the invoking user. Docker Desktop's
    # privileged-loop plumbing belongs to the login session: a root-run shrink
    # container hit EPERM opening the image's loop partitions (2026-08-20,
    # both masters), while the same shrink run as the user worked first try.
    # The user also needs to own the staged file and the sentinel's directory
    # (.shrink-ok lives next to the image), or the run "fails" after doing all
    # the work. -H so docker resolves the USER'S context, not root's.
    if [ -n "${SUDO_USER:-}" ] && [ "$SUDO_USER" != "root" ]; then
        chown "$SUDO_USER" "$RAW" masters 2>/dev/null || true
        sudo -H -u "$SUDO_USER" sh "$(dirname "$0")/shrink-image.sh" "$RAW" || \
            die "shrink failed — the raw image is intact at $RAW; inspect, re-run
       tools/shrink-image.sh (as your normal user, not root), then compress by hand (zstd -19 -T0 --rm)"
    else
        sh "$(dirname "$0")/shrink-image.sh" "$RAW" || \
            die "shrink failed — the raw image is intact at $RAW; inspect, re-run
       tools/shrink-image.sh, then compress by hand (zstd -19 -T0 --rm)"
    fi
else
    warn "docker unavailable — shipping FULL SIZE; restoring will need a card >= the source card (${SIZE:-unknown} bytes)"
fi
IMAGE_BYTES=$(wc -c < "$RAW" | tr -d ' ')

say "Compressing → $OUT.img.zst"
zstd -19 -T0 -q --rm "$RAW" -o "$OUT.img.zst"

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
  "image_bytes": "$IMAGE_BYTES",
  "restore_min_card_bytes": "$IMAGE_BYTES",
  "layout": "$LAYOUT",
  "source_device": "$DEV",
  "image_check": "asserted PASS by operator on the board before poweroff",
  "artifact": "$(basename "$OUT").img.zst",
  "sha256_file": "$(basename "$OUT").sha256"
}
EOF

# The script ran under sudo; the artifacts should belong to the human who will
# upload and manage them, not to root.
if [ -n "${SUDO_USER:-}" ] && [ "$SUDO_USER" != "root" ]; then
    chown "$SUDO_USER" "$OUT.img.zst" "$OUT.sha256" "$OUT.json" 2>/dev/null || true
fi

say "Done"
ls -la "$OUT".*
cat <<'EOF'

  Next:
    • Keep all three files together. An image with no record is
      unreproducible, and eight months from now that is when you need it.
    • Restore needs a card >= image_bytes in the record (NOT the original
      card size — the image is shrunk; first boot grows it back to fill):
          zstd -dc <file>.img.zst | sudo dd of=/dev/rdiskN bs=4M
    • Store PRIVATELY — the image contains Qualcomm firmware from the vendor
      BSP, which we do not redistribute. Not GitHub Releases (public assets,
      2 GB cap), not virtues.com/downloads (that is the installer's path).
    • Hand it over as an expiring presigned URL, with the .sha256, and have
      the recipient verify BEFORE flashing. A corrupt card fails strangely
      rather than loudly.
    • Keep every master you ever ship.

EOF
