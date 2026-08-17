#!/bin/sh
# Build a Virtues appliance master image, on the board.
#
# Run this on a Dragon that has been flashed with a stock Radxa image and put on
# a network. It installs Virtues, verifies it, strips every per-unit identity,
# proves the strip worked, and powers off — leaving a microSD card that is the
# product. See docs/appliance-image.md.
#
#     sudo VIRTUES_VERSION=v0.3.1 sh tools/build-dragon.sh
#
# ## Why this exists rather than a list of commands in a doc
#
# THE VERSION. `virtues.com/sh` serves the newest STABLE release, and the box
# work moves on prereleases — so the obvious one-liner silently builds a master
# running whatever stable happened to be current, which at the time of writing
# is two weeks behind. That is invisible until a customer's box is old on
# arrival. This script refuses to run without an explicit version.
#
# THE TAIL. Steps 4-6 (deprovision, image-check, power off) are the ones a human
# skips, because by then the box is working and the interesting part is over.
# Skipping them ships the master's iroh secret, encryption key, SSH host keys
# and your workshop's wifi password to every customer, identically, invisibly.
# Here they are not optional and not last-thing-on-a-checklist; they are the
# back half of the same command.
#
# NOT AN IMAGE BUILDER. This is the golden-master workflow written down, not a
# pipeline: it runs ON a board and its output is that board's card. If a real
# image pipeline is ever built, this script is what it would run.

set -eu

say() { printf '\n\033[1m∴  %s\033[0m\n' "$*"; }
die() { printf '\n\033[1;31m✖  %s\033[0m\n\n' "$*" >&2; exit 1; }

[ "$(id -u)" -eq 0 ] || die "run me as root: sudo sh tools/build-dragon.sh"

# ── The version is mandatory ────────────────────────────────────────────────
# No default, deliberately. A default would be either `latest` (wrong: stable
# is behind) or a pinned tag (wrong: it rots in this file). Making it an
# argument makes the operator state, once, which build they are pressing.
: "${VIRTUES_VERSION:=}"
[ -n "$VIRTUES_VERSION" ] || die "set VIRTUES_VERSION to the release tag you are pressing,
   e.g.  sudo VIRTUES_VERSION=v0.3.1 sh tools/build-dragon.sh
   Stable tags look like v0.3.1; prereleases like v0.1.0-staging.59 or edge.
   'virtues.com/sh' would give you the newest STABLE, which is usually not what
   you want for a master. Check: gh release list --limit 5"

# ── Confirm, because step 5 wipes this box ──────────────────────────────────
say "Master build — $VIRTUES_VERSION"
cat <<EOF

  This will install Virtues on THIS board, then DESTROY its identity:
  the database, the data lake, the encryption key, machine-id, SSH host
  keys, and every saved wifi network. That is what makes the card safe to
  clone. It is not reversible and this board will need setting up again.

  Do not run it on a box with anything on it you want.

EOF
printf '  Type MASTER to continue: '
read -r confirm
[ "$confirm" = "MASTER" ] || die "aborted — nothing was changed"

# ── 1. The OS, brought current deliberately ─────────────────────────────────
# `apt upgrade` is here and NOT in the installer, and the distinction matters.
# A kernel upgrade makes `kernel-install` regenerate the systemd-boot loader
# entries, which is exactly where a hand-edited entry gets silently dropped
# (see docs/appliance-image.md). That is fine to do once, on a bench, with a
# human present who can boot the board afterwards and find out. It is not fine
# to do unattended on every customer install, which is why the installer does
# `apt-get update` and targeted installs only.
say "System packages"
export DEBIAN_FRONTEND=noninteractive
apt-get update
apt-get -y upgrade
apt-get -y autoremove

# If that pulled a kernel, STOP. Everything after this point verifies the box —
# `doctor`, and a human walking the setup flow — and verifying a box running the
# old kernel while the card now holds a new one tests something that will never
# be shipped. It is also precisely when the loader entries were just regenerated,
# which is the failure this whole ordering exists to surface early.
#
# Exit rather than reboot-and-resume: re-running this script is safe (apt is a
# no-op the second time, and the installer is idempotent), and a script that
# reboots the machine it is running on has to persist state to come back, which
# is more machinery than one `reboot` is worth.
if [ -e /var/run/reboot-required ] || [ -e /run/reboot-required ]; then
    cat <<'REBOOT'

  The upgrade needs a reboot before anything can be verified — most likely a new
  kernel, which also means the boot loader entries were just regenerated.

      sudo reboot

  Then run this script again. It will pick up from here.

REBOOT
    exit 0
fi

# ── 2. Virtues ──────────────────────────────────────────────────────────────
# The same installer a DIY self-hoster runs. The Dragon is detected from its
# device tree, which implies the appliance profile — kiosk, BLE provisioning,
# the polkit grant, the Postgres mount guard, the power-key drop-in. There is
# deliberately no `--appliance` flag here: if the detector fails, that is a bug
# to fix rather than paper over, because every customer install depends on it.
#
# `--no-init` IS load-bearing. Without it the installer finishes by `exec`ing
# into `virtues init` — which REPLACES this shell, so steps 3-6 below would
# never run and the card would be pressed with the master's identity still on
# it. The only visible symptom would be the script appearing to end early and
# successfully. Nothing init does is wanted here anyway: migrations already ran
# in `bringup`, and the pair code it mints is per-unit state deprovision wipes.
say "Installing Virtues $VIRTUES_VERSION"
curl -sSL "https://raw.githubusercontent.com/virtues-os/virtues/${VIRTUES_VERSION}/tools/bootstrap.sh" \
    -o /tmp/virtues-bootstrap.sh \
    || die "could not fetch bootstrap.sh for $VIRTUES_VERSION — is the tag right?"
VIRTUES_VERSION="$VIRTUES_VERSION" sh /tmp/virtues-bootstrap.sh --no-init
rm -f /tmp/virtues-bootstrap.sh

# ── 3. Prove it works, before destroying the evidence ───────────────────────
# `doctor` is a report, not a gate — it exits non-zero on real problems, and on
# a master build that must stop everything: a broken box cloned a hundred times
# is a hundred broken boxes.
say "Verifying"
virtues doctor || die "virtues doctor found problems — fix them before imaging"

cat <<'EOF'

  Before the card is sealed, WALK THE SETUP FLOW on this board:

    · the panel shows a name and four words
    · the Mac app finds it over Bluetooth and accepts those words
    · wifi, account link, and pairing all complete
    · the box opens

  Nothing below tests any of that, and it is the entire product. A master
  that installs cleanly and cannot be set up is the most expensive possible
  thing to discover after pressing a hundred cards.

EOF
printf '  Type SETUP-OK once you have done that: '
read -r walked
[ "$walked" = "SETUP-OK" ] || die "stopped — nothing has been destroyed; re-run when ready"

# ── 4. Strip this board's identity ──────────────────────────────────────────
say "Deprovisioning"
virtues deprovision --yes

# ── 5. Prove the strip worked ───────────────────────────────────────────────
# deprovision cannot be its own witness: it prints "safe to image" and nothing
# ever re-reads the disk. This does, and it exits non-zero on any finding, so a
# master that kept a secret cannot reach the `dd`.
say "Checking"
virtues image-check || die "image-check found per-unit identity — DO NOT image this card"

# ── 6. Off, without booting again ───────────────────────────────────────────
# Work out which device to image rather than naming one. The card enumerates as
# mmcblk1 on the lab board and mmcblk0 on plenty of others, and a `dd` line in a
# closing message is exactly the kind of thing that gets copied without being
# read. Ask the running system where its ESP is and strip the partition suffix —
# `mmcblk1p2` -> `mmcblk1`, `sda2` -> `sda`.
ESP_PART="$(findmnt -no SOURCE /boot/efi 2>/dev/null || true)"
BOOT_DEV="$(printf '%s' "$ESP_PART" | sed -E 's|p?[0-9]+$||')"
[ -n "$BOOT_DEV" ] && [ -b "$BOOT_DEV" ] || BOOT_DEV="<your boot device — check lsblk>"

# A boot re-mints machine-id and SSH host keys, which then travel into every
# clone — and `image-check` would have passed before that boot, so nothing
# downstream would ever notice. Powering off from inside this script is what
# makes "don't boot it again" enforceable rather than advisory.
cat <<EOF

  ✓ This board is a master for $VIRTUES_VERSION.

  Powering off now. Do NOT boot it again — a boot re-mints machine-id and
  host keys, and they would be baked into every card you press.

  Then, from another machine:

      sudo dd if=$BOOT_DEV of=virtues-$VIRTUES_VERSION.img bs=4M status=progress
      sha256sum virtues-$VIRTUES_VERSION.img > virtues-$VIRTUES_VERSION.img.sha256

  Every unit gets that card image and a BLANK NVMe. First boot claims the
  NVMe, builds its Postgres cluster, and mints that unit's own identity.

EOF
sleep 5
poweroff
