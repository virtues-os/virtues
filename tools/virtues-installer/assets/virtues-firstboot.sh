#!/bin/sh
# Per-unit first-boot provisioning. Installed by virtues-installer (appliance
# only — install_systemd_unit gates it on `appliance`).
#
# Runs on EVERY boot and is a no-op once each section's own guard is satisfied.
# Several independent jobs with independent guards, deliberately NOT sharing
# one (only §2, the key mint, keys off the deprovision marker):
#
#   1. claim a blank NVMe   — guarded on "the disk is blank"
#   2. mint an encryption key — guarded on the deprovision marker
#
# They are separate because the risks are opposite. Formatting must never key
# off "this is a fresh unit" (a marker can outlive the state it described, and
# reformatting a disk with data on it is unrecoverable); minting must never key
# off "the key is missing" (that would silently rotate a key a working box
# still needs). Each guard is the narrowest true statement about its own job.
set -eu

# Everything this script writes — cluster config, env file, sentinels, fstab —
# is written once and read forever, and a first boot is exactly when a new
# owner is most likely to cut power. A yank minutes after first bringup left
# pg_createcluster's start.conf ZERO-LENGTH (ext4 delayed allocation), and the
# postgresql-common generator then never started the cluster on any later
# boot, while the umbrella unit reported active over the gap (read off a
# wedged unit's NVMe, 2026-08-25). Flush on every exit path, including §1d's.
trap sync EXIT

DATA_DIR=__DATA_DIR__
ENV_FILE="$DATA_DIR/virtues.env"
MARKER="$DATA_DIR/.needs-firstboot"

# Requeue the service chain. THE CLUSTER INSTANCE by name, not just the
# umbrella: postgresql.service is a oneshot wrapper that reports active while
# postgresql@18-main lies dead beside it, and starting an already-active
# oneshot is a no-op. Confirmed from a wedged unit's NVMe (2026-08-25): a
# first-boot power yank truncated start.conf, the generator skipped the
# instance on every later boot, the umbrella said active, and virtues.service
# polled a socket that could never appear until its own start-limit parked
# it. Starting the instance by name does not depend on the generator's
# wants-links, which is what makes this the repair. The names are derived
# from /etc/postgresql so a version bump cannot rot this.
requeue_services() {
    UNITS="postgresql.service virtues.service"
    for conf in /etc/postgresql/*/*/postgresql.conf; do
        [ -e "$conf" ] || continue
        cdir="${conf%/postgresql.conf}"
        cname="${cdir##*/}"
        cver="${cdir%/*}"; cver="${cver##*/}"
        UNITS="postgresql@$cver-$cname.service $UNITS"
    done
    systemctl reset-failed $UNITS 2>/dev/null || true
    systemctl --no-block start $UNITS 2>/dev/null || true
}

# ── 0. Finish an interrupted claim ──────────────────────────────────────────
# The claim below is minutes of work (mkfs, a several-hundred-MB seed copy),
# and a first boot is exactly when a new owner is most likely to cut power.
# The first clone-test boot, 2026-08-19, was interrupted mid-copy: the next
# boot auto-mounted the half-seeded disk (the fstab LABEL entry ships in the
# image) and skipped everything — models present, env and marker gone, no key
# ever minted, box hollow forever. `.claim-complete` is written as the LAST
# act of a claim; a mounted disk without it is a half-claim, and the repair is
# to finish the copy — item by item, only what is missing, because a repair
# boot must never overwrite something a completed step already made.
# ── 0a. A disk that carries our label but will not mount ────────────────────
# A power cut DURING mkfs (the 2026-08-19 yank test) leaves the disk labeled
# virtues-data but structurally corrupt: the claim below sees "not blank" and
# skips, the repair sees "not mounted" and skips, and the box is hollow on
# every boot forever. fsck first, then let the SENTINEL decide — it is written
# after all seeding, so a recovered fs without it provably holds no owner data
# and is safe to wipe for a fresh claim. A recovered fs WITH it is the owner's
# disk that corrupted later in life: keep it, mount it, carry on. A fs fsck
# cannot recover is potentially the owner's record — firstboot must NEVER
# destroy that; it leaves the box up and loud instead (see the postgres
# drop-in: nothing starts without the mount, and the display says so).
if ! mountpoint -q "$DATA_DIR" 2>/dev/null; then
    for disk in /dev/nvme0n1 /dev/nvme1n1; do
        [ -b "${disk}p1" ] || continue
        blkid "${disk}p1" 2>/dev/null | grep -q 'LABEL="virtues-data"' || continue
        if mount "${disk}p1" "$DATA_DIR" 2>/dev/null; then
            umount "$DATA_DIR" 2>/dev/null
            break   # mountable — fstab/normal flow handles it
        fi
        logger -t virtues-firstboot "labeled data disk will not mount - running fsck"
        e2fsck -y "${disk}p1" >/dev/null 2>&1 || true
        if mount "${disk}p1" "$DATA_DIR" 2>/dev/null; then
            if [ -e "$DATA_DIR/.claim-complete" ]; then
                logger -t virtues-firstboot "data disk recovered by fsck - owner data intact"
                umount "$DATA_DIR" 2>/dev/null
            else
                umount "$DATA_DIR" 2>/dev/null
                logger -t virtues-firstboot "recovered fs has no claim sentinel - failed claim, wiping for a fresh one"
                wipefs -a "${disk}p1" >/dev/null 2>&1 || true
                wipefs -a "$disk" >/dev/null 2>&1 || true
                dd if=/dev/zero of="$disk" bs=1M count=16 >/dev/null 2>&1 || true
                partprobe "$disk" 2>/dev/null || true; sleep 2
            fi
        else
            logger -t virtues-firstboot "data disk unrecoverable by fsck - NOT wiping (may hold the owner's record); box stays up without it"
        fi
        break
    done
fi

if mountpoint -q "$DATA_DIR" 2>/dev/null && [ ! -e "$DATA_DIR/.claim-complete" ]; then
    mkdir -p /run/virtues-cardseed
    if mount --bind "$(dirname "$DATA_DIR")" /run/virtues-cardseed 2>/dev/null; then
        CARD="/run/virtues-cardseed/$(basename "$DATA_DIR")"
        [ -d "$DATA_DIR/models" ] || cp -a "$CARD/models" "$DATA_DIR/models" 2>/dev/null || true
        # The env file is LOAD-BEARING — no DATABASE_URL, no server — so unlike
        # the models its copy failure is logged, not swallowed, and the
        # sentinel below stays unwritten without it. An env that exists but
        # carries no DATABASE_URL is the same failure as no env (the v0.1.4
        # first press seeded a zero-length one) and is replaced the same way —
        # but NEVER over a minted key: an env holding VIRTUES_ENCRYPTION_KEY
        # is the owner's, whatever else it lost, and clobbering it strands
        # everything already encrypted.
        if ! grep -q '^DATABASE_URL=' "$DATA_DIR/virtues.env" 2>/dev/null && \
           ! grep -q '^VIRTUES_ENCRYPTION_KEY=' "$DATA_DIR/virtues.env" 2>/dev/null; then
            cp -a "$CARD/virtues.env" "$DATA_DIR/virtues.env" 2>/dev/null || \
                logger -t virtues-firstboot "FAILED to copy virtues.env from the card seed"
        fi
        if [ ! -e "$DATA_DIR/.needs-firstboot" ] && \
           ! grep -q '^VIRTUES_ENCRYPTION_KEY=' "$DATA_DIR/virtues.env" 2>/dev/null && \
           [ -e "$CARD/.needs-firstboot" ]; then
            cp -a "$CARD/.needs-firstboot" "$DATA_DIR/.needs-firstboot" 2>/dev/null || true
        fi
        umount /run/virtues-cardseed
        # The sentinel means "the seed is complete", NOT "this block reached
        # its last line". Written over a hollow disk it disables the very
        # repair path that exists for a failed copy — which is how the
        # 2026-08-19 unit latched hollow: models across, env missing, sentinel
        # written anyway, repair never ran again, status=0/SUCCESS throughout.
        # SAME predicate §1d fails loudly on (DATABASE_URL present), not merely
        # "env is non-empty" — a truncated env with no DATABASE_URL was passing
        # `-s`, writing the sentinel, latching the repair off, and then failing
        # §1d every boot forever.
        if grep -q '^DATABASE_URL=' "$DATA_DIR/virtues.env" 2>/dev/null; then
            # The seed must be ON DISK before the sentinel that says it is —
            # ext4 does not order writes across files, and a yank between the
            # two leaves a sentinel over a hollow disk with the repair latched off.
            sync
            touch "$DATA_DIR/.claim-complete"
            logger -t virtues-firstboot "completed an interrupted disk claim from the card seed"
        else
            logger -t virtues-firstboot "repair did not produce a usable virtues.env - claim left unmarked for the next boot"
        fi
    fi
    rmdir /run/virtues-cardseed 2>/dev/null || true
fi

# ── 1. Claim a blank NVMe for the data directory ────────────────────────────
# We image the BOOT MEDIUM (a microSD card on the Q6A), not the NVMe, so every
# unit boots with a fresh blank disk and no UUID or LABEL that fstab could have
# been written against. The disk has to be claimed here, on the unit, or Postgres
# and the lake land on the card — which has modest write endurance and is exactly
# what we're trying to keep writes off. See agents/build/appliance-image.md.
#
# TWO layouts, told apart by where root is mounted from. If root is on the NVMe
# (NVMe-both, no SD in the unit) the data partition lives on the SAME disk,
# carved from the free space the shrunk master left after root — handled by
# §1-NVMe just below, which runs FIRST and, if it mounts $DATA_DIR, makes the
# classic separate-disk claim skip itself via its own `! mountpoint` guard. If
# root is on the card, the classic claim (the `for disk` loop) takes the whole
# separate NVMe. NB: the seed + fstab + mount + sentinel are duplicated across
# the two paths deliberately — the classic path is load-bearing and tested, so
# it is left untouched; keep the two in step.

# ── 1-NVMe. NVMe-both: the data partition is p4 on the boot disk ─────────────
if ! mountpoint -q "$DATA_DIR" 2>/dev/null; then
    ROOT_SRC="$(findmnt -no SOURCE / 2>/dev/null || true)"
    case "$ROOT_SRC" in
      /dev/nvme*)
        ROOT_NAME="${ROOT_SRC##*/}"
        ROOT_NUM="${ROOT_NAME##*[!0-9]}"
        BOOTDISK="/dev/${ROOT_NAME%"$ROOT_NUM"}"; BOOTDISK="${BOOTDISK%p}"
        # Adopt an existing CLAIMED data partition — fs label virtues-data means
        # a prior boot completed mkfs, seed and relabel, and the fs may hold the
        # owner's data: adopted, never re-made, never re-seeded.
        NEEDS_SEED=""
        DATA_PART="$(lsblk -rno NAME,LABEL "$BOOTDISK" 2>/dev/null | awk '$2=="virtues-data"{print "/dev/"$1; exit}')"
        if [ -z "$DATA_PART" ]; then
            # No claimed fs. Find the partition by its GPT name (an interrupted
            # claim leaves it carved but fs-labeled virtues-seeding — provably
            # unseeded, safe to re-make), else carve it from the space past
            # root. NEVER touch config/ESP/root.
            DATA_PART="$(lsblk -rno NAME,PARTLABEL "$BOOTDISK" 2>/dev/null | awk '$2=="virtues-data"{print "/dev/"$1; exit}')"
            if [ -z "$DATA_PART" ] && command -v sgdisk >/dev/null 2>&1; then
                # The shrunk master's GPT backup sits just past root; move it to
                # the real end of this larger disk, then carve one partition to
                # the end. Adding a partition to a disk whose root is mounted is
                # allowed — the kernel refuses only to MODIFY a busy partition —
                # and partx -a adds just the new one when partprobe declines the
                # busy disk.
                sgdisk -e "$BOOTDISK" >/dev/null 2>&1 || true
                sgdisk -n 0:0:0 -c 0:virtues-data "$BOOTDISK" >/dev/null 2>&1
                partprobe "$BOOTDISK" 2>/dev/null || partx -a "$BOOTDISK" 2>/dev/null || true
                sleep 2
                DATA_PART="$(lsblk -rno NAME,PARTLABEL "$BOOTDISK" 2>/dev/null | awk '$2=="virtues-data"{print "/dev/"$1; exit}')"
            fi
            # Make the filesystem under a TEMPORARY label. The image ships an
            # fstab `LABEL=virtues-data` line and / is mounted shared, so the
            # moment that label exists systemd auto-mounts this partition over
            # $DATA_DIR — shadowing the root-side seed MID-COPY, and mount
            # propagation carries the shadow into a parent bind taken earlier
            # (both observed on the 2026-08-20 bench, two boots running). Under
            # virtues-seeding nothing matches the fstab line, the seed below
            # runs undisturbed, and e2label flips it to virtues-data only once
            # the seed is complete.
            if [ -n "$DATA_PART" ] && [ -b "$DATA_PART" ]; then
                mkfs.ext4 -q -F -L virtues-seeding "$DATA_PART"
                NEEDS_SEED=1
            fi
            logger -t virtues-firstboot "NVMe-both: data partition ${DATA_PART:-<none>} on $BOOTDISK"
        fi
        # Give the space back: the master ships the data partition shrunk
        # (shrink-image trims the trailing partition), so grow it to fill the
        # unit's disk here — the NVMe-both analogue of the root grow a card unit
        # does in §1e. growpart no-ops when already full (a freshly-carved p4).
        if [ -n "$DATA_PART" ] && [ -b "$DATA_PART" ] && command -v growpart >/dev/null 2>&1; then
            DP_NUM="${DATA_PART##*[!0-9]}"
            if growpart "$BOOTDISK" "$DP_NUM" >/dev/null 2>&1; then
                e2fsck -fy "$DATA_PART" >/dev/null 2>&1 || [ $? -le 2 ]
                resize2fs "$DATA_PART" >/dev/null 2>&1 || true
                logger -t virtues-firstboot "NVMe-both: grew the data partition to fill $BOOTDISK"
            fi
        fi
        if [ -n "$DATA_PART" ] && [ -b "$DATA_PART" ] && [ -n "$NEEDS_SEED" ]; then
            mkdir -p /run/virtues-seed /run/virtues-cardseed
            if mount "$DATA_PART" /run/virtues-seed 2>/dev/null; then
                # Belt and suspenders on top of the temporary label: read the
                # seed through a PRIVATE bind of the parent, so no later mount
                # over $DATA_DIR — by anything — can shadow the source, and no
                # peer-group propagation can carry one into the bind.
                SEEDSRC="$DATA_DIR"
                if mount --bind "$(dirname "$DATA_DIR")" /run/virtues-cardseed 2>/dev/null; then
                    mount --make-private /run/virtues-cardseed 2>/dev/null || true
                    SEEDSRC="/run/virtues-cardseed/$(basename "$DATA_DIR")"
                fi
                # Do NOT swallow the copy error — it is the difference between a
                # working unit and a hollow one, and it was invisible once
                # already (2026-08-20: two hollow first boots before the error
                # text was ever seen).
                CP_ERR="$(cp -a "$SEEDSRC/." /run/virtues-seed/ 2>&1)" || \
                    logger -t virtues-firstboot "seed copy reported errors - the sentinel gate below decides: $CP_ERR"
                umount /run/virtues-cardseed 2>/dev/null || true
                rm -rf /run/virtues-seed/postgresql /run/virtues-seed/secrets /run/virtues-seed/backups
                mkdir -p /run/virtues-seed/journal /run/virtues-seed/lake
                chown root:systemd-journal /run/virtues-seed/journal 2>/dev/null || true
                chmod 2755 /run/virtues-seed/journal 2>/dev/null || true
                # The lake needs an owner too. It sat directly beside the two
                # journal lines above with nothing of its own, so a seeded box
                # booted with a root-owned lake and its ingest applet failed
                # with EACCES every five minutes — indistinguishable, from the
                # outside, from a source with nothing to say.
                chown -R virtues:virtues /run/virtues-seed/lake 2>/dev/null || true
                umount /run/virtues-seed
                # The seed is complete — only now may the real label exist,
                # which is what lets the fstab line (and the mount below) find
                # the partition.
                e2label "$DATA_PART" virtues-data 2>/dev/null || \
                    logger -t virtues-firstboot "e2label to virtues-data FAILED on $DATA_PART - the box will stay hollow"
            fi
            rmdir /run/virtues-seed /run/virtues-cardseed 2>/dev/null || true
        fi
        if [ -n "$DATA_PART" ] && [ -b "$DATA_PART" ]; then
            mkdir -p "$DATA_DIR"
            grep -q '^LABEL=virtues-data' /etc/fstab 2>/dev/null || \
                echo "LABEL=virtues-data $DATA_DIR ext4 defaults,nofail,x-systemd.device-timeout=300s 0 2" >> /etc/fstab
            sed -i 's/x-systemd.device-timeout=10s/x-systemd.device-timeout=300s/' /etc/fstab 2>/dev/null || true
            systemctl daemon-reload
            mount "$DATA_DIR" || logger -t virtues-firstboot "mount $DATA_DIR failed (NVMe-both)"
            if mountpoint -q "$DATA_DIR"; then
                requeue_services
            fi
            if mountpoint -q "$DATA_DIR" && grep -q '^DATABASE_URL=' "$DATA_DIR/virtues.env" 2>/dev/null; then
                sync   # seed durable before the sentinel that says so
                touch "$DATA_DIR/.claim-complete"
                logger -t virtues-firstboot "NVMe-both: claimed data partition $DATA_PART"
            fi
        fi
        ;;
    esac
fi

if ! mountpoint -q "$DATA_DIR" 2>/dev/null; then
    for disk in /dev/nvme0n1 /dev/nvme1n1; do
        [ -b "$disk" ] || continue
        # Not empty? Decide whether it is ours to clear. §0a has already adopted
        # or repaired any mountable virtues-data disk, so contents remaining here
        # are one of two things, told apart by the LABEL. A disk carrying
        # 'virtues-data' that §0a could not mount is a possible owner record fsck
        # could not save — NEVER wipe it; the box stays up and loud without it.
        # Anything else is FOREIGN — a prior unit's leftover, a bench disk, an
        # unrelated fs — and on a single-owner appliance that is ours to clear by
        # default, so first boot is not wedged forever on a dirty disk (the
        # 2026-08-20 bench hit exactly this: a stale prior-install fs whose label
        # did not match left the box hollow until a hand `wipefs`). Clearing
        # foreign contents is the DEFAULT; the labeled-but-unrecoverable disk is
        # the one carve-out. After the wipe the disk is blank and the claim below
        # runs normally.
        if [ -n "$(lsblk -no FSTYPE,PTTYPE "$disk" 2>/dev/null | tr -d ' \n')" ]; then
            if lsblk -no LABEL "$disk" 2>/dev/null | grep -qx 'virtues-data'; then
                logger -t virtues-firstboot "$disk is labeled virtues-data but did not mount - possible owner record, NOT wiping; box stays up without it"
                break
            fi
            logger -t virtues-firstboot "clearing foreign/stale contents on $disk (no virtues-data label) for a fresh claim"
            for p in "${disk}"p*; do [ -b "$p" ] && wipefs -a "$p" >/dev/null 2>&1 || true; done
            wipefs -a "$disk" >/dev/null 2>&1 || true
            dd if=/dev/zero of="$disk" bs=1M count=16 >/dev/null 2>&1 || true
            partprobe "$disk" 2>/dev/null || true; sleep 2
        fi
        # Blank means: no partition table AND no filesystem anywhere on it.
        # `lsblk` over the whole device catches both in one shot; any non-empty
        # output means something is already there and we keep our hands off.
        if [ -z "$(lsblk -no FSTYPE,PTTYPE "$disk" 2>/dev/null | tr -d ' \n')" ]; then
            logger -t virtues-firstboot "claiming blank $disk for $DATA_DIR"
            parted -s "$disk" mklabel gpt mkpart virtues 1MiB 100%
            sleep 2; partprobe "$disk" 2>/dev/null || true; sleep 2
            mkfs.ext4 -q -F -L virtues-data "${disk}p1"
            # Seed the fresh disk from the card BEFORE mounting shadows it.
            # The card-side $DATA_DIR is the only one a clone has: deprovision
            # wiped it and laid down exactly what a unit needs (stripped env,
            # models, the first-boot marker). Without this copy a clone boots
            # hollow — no env, no models, no licence to mint a key — which is
            # precisely how the first master's clones would have come up
            # (found 2026-08-19). Mounted at a private point so the copy reads
            # the card and writes the disk; then the real mount takes over.
            mkdir -p /run/virtues-seed
            if mount "${disk}p1" /run/virtues-seed 2>/dev/null; then
                cp -a "$DATA_DIR/." /run/virtues-seed/ 2>/dev/null || \
                    logger -t virtues-firstboot "seed copy reported errors - the sentinel gate below decides"
                # A properly sealed card carries no cluster, but a half-sealed
                # one (they exist) would hand every clone the SAME database and
                # box identity. The cluster is per-disk by doctrine — 1c below
                # builds it fresh — so whatever came over in the copy, goes.
                rm -rf /run/virtues-seed/postgresql /run/virtues-seed/secrets /run/virtues-seed/backups
                mkdir -p /run/virtues-seed/journal /run/virtues-seed/lake
                # journald refuses a persistent directory it does not own:
                # root:root 755 here means no journal ever lands, silently.
                chown root:systemd-journal /run/virtues-seed/journal 2>/dev/null || true
                chmod 2755 /run/virtues-seed/journal 2>/dev/null || true
                # Same for the lake, whose owner is the service user. The
                # comment above explains why journal needs this; the lake sat
                # beside it with nothing, and a root-owned lake fails the ingest
                # applet with EACCES on every run while looking merely idle.
                chown -R virtues:virtues /run/virtues-seed/lake 2>/dev/null || true
                umount /run/virtues-seed
                logger -t virtues-firstboot "seeded new disk from the card-side $DATA_DIR"
            fi
            rmdir /run/virtues-seed 2>/dev/null || true
            mkdir -p "$DATA_DIR"
            # 300s, not 10s: this fstab line SHIPS IN THE IMAGE, so on a
            # clone's first boot the generated mount unit starts waiting for a
            # label that only exists once this claim creates it (~21s today,
            # minutes once models grow). At 10s the device job timed out,
            # systemd marked postgresql and virtues dependency-failed, and
            # never retried even though the mount landed seconds later
            # (2026-08-19, first shrunk-image boot). nofail keeps a truly
            # absent disk from blocking boot regardless of the timeout.
            grep -q '^LABEL=virtues-data' /etc/fstab 2>/dev/null || \
                echo "LABEL=virtues-data $DATA_DIR ext4 defaults,nofail,x-systemd.device-timeout=300s 0 2" >> /etc/fstab
            # Converge an already-shipped 10s line to the same answer.
            sed -i 's/x-systemd.device-timeout=10s/x-systemd.device-timeout=300s/' /etc/fstab 2>/dev/null || true
            systemctl daemon-reload
            mount "$DATA_DIR" || logger -t virtues-firstboot "mount $DATA_DIR failed"
            # The belt for the same race: if the device job already timed out
            # this boot, every dependent is sitting in dependency-failed and
            # nothing will retry it. The disk is mounted now, so clear the
            # failures and queue the chain — no-block, because those units are
            # ordered After this very script and a blocking start would
            # deadlock exactly like pg_createcluster --start once did.
            if mountpoint -q "$DATA_DIR"; then
                # SCOPED: bare `reset-failed` clears every failed unit on the
                # box, erasing the evidence an operator or `virtues doctor`
                # needs from the first boot. Only clear the ones this race
                # actually failed.
                requeue_services
            fi
            # LAST act, only on the mounted disk, and ONLY when the seed's
            # load-bearing file made it across — same predicate §1d and the
            # repair block use (DATABASE_URL present), so "claim complete" and
            # "provisioning incomplete" can never both be true of one disk.
            if mountpoint -q "$DATA_DIR" && grep -q '^DATABASE_URL=' "$DATA_DIR/virtues.env" 2>/dev/null; then
                sync   # seed durable before the sentinel that says so
                touch "$DATA_DIR/.claim-complete"
            fi
            break
        fi
    done
fi

# ── 1b. Send the journal to the data disk ───────────────────────────────────
# journald writes continuously and forever, which makes it the third-largest
# write source on the box after Postgres and the lake — and the only one that
# keeps going when nothing is happening. Left alone it lands in
# /var/log/journal on the boot card: modest endurance, and the one medium we
# cannot let a continuous writer sit on.
#
# A symlink rather than `Storage=` in journald.conf, because the config only
# chooses persistent-vs-volatile, never where. Only when the data dir is really
# mounted — a symlink into an unmounted directory would put the journal on the
# boot card anyway, under a path that claims otherwise, which is worse than not
# trying. And only when /var/log/journal is not already a symlink, so a
# reboot is a no-op.
if mountpoint -q "$DATA_DIR" 2>/dev/null && [ ! -L /var/log/journal ]; then
    mkdir -p "$DATA_DIR/journal"
    chown root:systemd-journal "$DATA_DIR/journal" 2>/dev/null || true
    chmod 2755 "$DATA_DIR/journal" 2>/dev/null || true
    # Move what is already there rather than orphaning it: this runs on the
    # first boot AFTER the disk is claimed, and the boot that claimed the disk
    # logged the claim itself.
    if [ -d /var/log/journal ]; then
        cp -a /var/log/journal/. "$DATA_DIR/journal/" 2>/dev/null || true
        rm -rf /var/log/journal
    fi
    ln -s "$DATA_DIR/journal" /var/log/journal
    systemd-tmpfiles --create --prefix /var/log/journal >/dev/null 2>&1 || true
    systemctl kill --kill-who=main --signal=SIGUSR2 systemd-journald 2>/dev/null || true
    logger -t virtues-firstboot "journal relocated to $DATA_DIR/journal"
fi

# ── 1b'. Undo the pre-mount shadow, once per boot ───────────────────────────
# journald starts long before this script and opens /var/log/journal at once.
# With the symlink in place but the data disk not yet mounted, the path
# resolves to the CARD-side directory; journald holds that fd while the NVMe
# mounts over it. Diagnosed on hardware 2026-08-19: 25 MB of journal landing
# on the boot card — the exact wear 1b exists to prevent — while journalctl,
# resolving the same path post-mount, read the empty NVMe directory and
# reported nothing at all. So after the mount is real: fix ownership (journald
# refuses root:root), copy anything stranded card-side across (no-clobber, and
# the card copy is left in place — it stops growing the moment journald is
# bounced, and deleting under a live writer risks the history we came for),
# and restart journald so it reopens through the mounted path.
if mountpoint -q "$DATA_DIR" 2>/dev/null && [ -L /var/log/journal ] && [ ! -e /run/virtues-journal-rehomed ]; then
    chown root:systemd-journal "$DATA_DIR/journal" 2>/dev/null || true
    chmod 2755 "$DATA_DIR/journal" 2>/dev/null || true
    mkdir -p /run/virtues-cardshadow
    if mount --bind "$(dirname "$DATA_DIR")" /run/virtues-cardshadow 2>/dev/null; then
        CARDJ="/run/virtues-cardshadow/$(basename "$DATA_DIR")/journal"
        if [ -d "$CARDJ" ] && [ -n "$(ls -A "$CARDJ" 2>/dev/null)" ]; then
            cp -an "$CARDJ/." "$DATA_DIR/journal/" 2>/dev/null || true
            logger -t virtues-firstboot "copied a card-stranded journal onto the data disk"
        fi
        umount /run/virtues-cardshadow
    fi
    rmdir /run/virtues-cardshadow 2>/dev/null || true
    systemctl try-restart systemd-journald 2>/dev/null || true
    touch /run/virtues-journal-rehomed
fi

# ── 1b'' state ownership, every boot ────────────────────────────────────────
# Every directory under the data dir is written by the `virtues` service user
# and must be owned by it. The installer establishes that (see the `find`
# with -prune at install time) but only on the machine it runs on — a disk
# seeded afterwards by firstboot never passes through it, so a box cloned from
# a card can boot with root-owned state that nothing ever repairs.
#
# That is not hypothetical: `lake` shipped that way, and the ingest applet
# failed with `Permission denied (os error 13)` on every run, for days, while
# every surface reported the source as merely idle.
#
# `-prune` on the Postgres cluster and a list-free sweep, deliberately copying
# the installer's shape rather than naming `lake`: the reason that form was
# chosen there is that enumerating siblings means the NEXT one added gets
# forgotten, which is exactly how this bug happened. `-print` so the repair is
# never silent — a boot that had to fix something says which paths.
if mountpoint -q "$DATA_DIR" 2>/dev/null; then
    FIXED="$(find "$DATA_DIR" -path "$DATA_DIR/postgresql" -prune -o \
                  \( -path "$DATA_DIR/journal" -o -path "$DATA_DIR/journal/*" \) -prune -o \
                  ! -user virtues -print 2>/dev/null | head -20)"
    if [ -n "$FIXED" ]; then
        # Repair separately from the listing above, and WITHOUT `head`: piping
        # the repairing find into head lets SIGPIPE kill it once 20 paths have
        # been printed, which would silently fix a prefix and leave the rest —
        # the same shape of half-truth this block exists to end. The cap is for
        # the log line only.
        find "$DATA_DIR" -path "$DATA_DIR/postgresql" -prune -o \
             \( -path "$DATA_DIR/journal" -o -path "$DATA_DIR/journal/*" \) -prune -o \
             ! -user virtues -exec chown virtues:virtues {} + 2>/dev/null || true
        logger -t virtues-firstboot "repaired state ownership (was not virtues): $(echo "$FIXED" | tr '\n' ' ')"
    fi
fi

# sudo resolves the hostname on every invocation; without a hosts entry each
# privileged command eats a resolver timeout and prints a warning (2026-08-19).
# Read /etc/hostname, NOT `hostname`: firstboot runs before systemd-hostnamed
# applies the persistent name, so `hostname` returns the transient boot/DHCP
# name (e.g. "radxa-dragon-q6a") and the entry then does not match the name
# sudo actually resolves ("virtues"). Verified on the bench 2026-08-20.
VIRT_HOSTNAME="$(cat /etc/hostname 2>/dev/null | tr -d '[:space:]')"
[ -n "$VIRT_HOSTNAME" ] || VIRT_HOSTNAME="$(hostname)"
grep -q "127.0.1.1[[:space:]].*$VIRT_HOSTNAME" /etc/hosts 2>/dev/null || \
    printf '127.0.1.1 %s\n' "$VIRT_HOSTNAME" >> /etc/hosts

# ── 1e. Grow the root filesystem to the card it landed on ───────────────────
# Masters are cut SHRUNK (tools/shrink-image.sh) so one image restores onto
# any card; the give-back happens here, on real hardware. growpart exits 0 on
# change, 1 on nothing-to-do, 2 on error — only a real change is followed by
# the online ext4 resize, so this logs once in an image's life and is silent
# forever after. Best-effort by design: a rootfs that never grows still has
# the shrink's 2 GiB margin, and everything heavy (Postgres, the lake, the
# journal) lives on the NVMe anyway.
ROOT_SRC="$(findmnt -no SOURCE / 2>/dev/null || true)"
case "$ROOT_SRC" in
    /dev/nvme*)
        # NVMe-both: root shares the disk with the data partition, which §1
        # carved from the free space the shrunk master left AFTER root. Growing
        # root to fill would swallow exactly that space. Leave root at its
        # shrunk size (+2 GiB margin); the data partition owns the remainder,
        # and everything heavy lives there anyway.
        logger -t virtues-firstboot "root is on the NVMe (NVMe-both) - not growing it; the data partition owns the free space"
        ;;
    /dev/*[0-9])
        ROOT_PART_NAME="${ROOT_SRC##*/}"
        ROOT_PART_NUM="${ROOT_PART_NAME##*[!0-9]}"
        ROOT_BASE="${ROOT_PART_NAME%"$ROOT_PART_NUM"}"
        ROOT_DISK="/dev/${ROOT_BASE%p}"
        if command -v growpart >/dev/null 2>&1; then
            if growpart "$ROOT_DISK" "$ROOT_PART_NUM" >/dev/null 2>&1; then
                resize2fs "$ROOT_SRC" >/dev/null 2>&1 || true
                logger -t virtues-firstboot "grew the root filesystem to fill $ROOT_DISK"
            fi
        else
            logger -t virtues-firstboot "growpart not installed - root stays at its shrunk size"
        fi
        ;;
esac

# ── 1f. Mint per-unit SSH host keys ─────────────────────────────────────────
# deprovision strips /etc/ssh/ssh_host_* so clones never share an identity —
# correctly — but stock Ubuntu has no ssh-keygen.service to make new ones, so
# sshd restart-looped five times and gave up on every unit ever imaged
# (found 2026-08-19). The keys are per-unit state, so they are minted here,
# like the encryption key.
if [ ! -f /etc/ssh/ssh_host_ed25519_key ] && command -v ssh-keygen >/dev/null 2>&1; then
    ssh-keygen -A >/dev/null 2>&1 || true
    systemctl reset-failed ssh.service 2>/dev/null || true
    systemctl --no-block restart ssh.service 2>/dev/null || true
    logger -t virtues-firstboot "minted per-unit SSH host keys"
fi

# ── 1c. Recreate the Postgres cluster on the claimed disk ───────────────────
# /var/lib/postgresql is a SYMLINK into the data dir on an appliance — the
# installer moved the cluster there so the busiest writer on the box lands on
# the replaceable NVMe rather than the boot card. The image carries the
# symlink; the disk it points at is blank on every unit. So the cluster has to
# be made here, once, on the unit.
#
# GUARDED ON THE MOUNT, like every sibling section (§1b/§1b'/§1d). The earlier
# claim that "a box whose disk failed to mount has no symlink target it can
# write to and skips" was FALSE: /var/lib/virtues exists as the mountpoint
# directory even unmounted, so `readlink -f` resolves and `mkdir -p` would build
# the cluster ON THE BOOT CARD — the exact silent-divergence the postgresql
# mount-guard drop-in exists to prevent, done by the provisioner itself in the
# window before that guard applies. So: only when $DATA_DIR is genuinely
# mounted. A DIY box has no symlink and skips regardless.
#
# NOT guarded on the first-boot marker. The marker licenses key MINTING, which
# must happen exactly once ever; this must happen once per DISK, and those are
# different events — a replaced NVMe needs a cluster and must not get a new
# encryption key.
PG_VER="$(ls /etc/postgresql 2>/dev/null | sort -n | tail -1)"
PG_LINK=/var/lib/postgresql
if mountpoint -q "$DATA_DIR" 2>/dev/null && [ -L "$PG_LINK" ] && [ -n "$PG_VER" ] && [ ! -e "$PG_LINK/$PG_VER/main/PG_VERSION" ]; then
    PG_TARGET="$(readlink -f "$PG_LINK" 2>/dev/null || true)"
    if [ -n "$PG_TARGET" ] && mkdir -p "$PG_TARGET" 2>/dev/null; then
        chown postgres:postgres "$PG_TARGET"
        logger -t virtues-firstboot "creating the Postgres cluster on the data disk"
        # Drop first: the image carries /etc/postgresql/$PG_VER/main from the
        # master, and pg_createcluster refuses to write over an existing
        # config. Dropping regenerates it, so the cluster ends up vanilla —
        # same paths, same conf, nothing hand-edited that an apt upgrade could
        # disagree with later.
        pg_dropcluster "$PG_VER" main >/dev/null 2>&1 || true
        # Created WITHOUT --start, and started with pg_ctl directly, NOT
        # systemd — because this script runs INSIDE a unit that is ordered
        # Before=postgresql. `pg_createcluster --start` asks systemd to start
        # the cluster and then waits for it; systemd queues that start behind
        # this very unit finishing; deadlock, forever, on the first boot of
        # every unit. Found on the first virgin-board boot, 2026-08-18 — this
        # branch had never executed before that night (every earlier box got
        # its cluster from the installer, not from first boot). The temp
        # server below is stopped again before this script exits; systemd
        # then starts the cluster through its own ordering, cleanly.
        PG_CTL="/usr/lib/postgresql/$PG_VER/bin/pg_ctl"
        PG_MAIN="$PG_LINK/$PG_VER/main"
        if pg_createcluster "$PG_VER" main >/dev/null 2>&1 && \
           su -s /bin/sh postgres -c "$PG_CTL -D $PG_MAIN -o '-c config_file=/etc/postgresql/$PG_VER/main/postgresql.conf' -w -t 60 start" >/dev/null 2>&1; then
            # The role and database the app connects as. Peer auth over the
            # Unix socket maps OS user -> role, so no password exists to set.
            #
            # NOT A SUPERUSER, and this must stay in step with `provision_db`,
            # which is the DIY path's version of these same four statements. It
            # said `CREATE ROLE virtues WITH LOGIN SUPERUSER` until 2026-08-18,
            # while `provision_db` had always used `--no-superuser`. So the
            # appliance — the shape that ships to people — was the one running
            # with the privilege the other path deliberately refuses.
            #
            # What that cost: DATABASE_URL is in the environment of every applet
            # subprocess, so an applet inherited superuser, and superuser means
            # `pg_read_file`. That reads /var/lib/virtues/virtues.env, which
            # holds VIRTUES_ENCRYPTION_KEY in plaintext — every credential in
            # the vault, and the iroh secret that IS this box's identity. It was
            # reachable from the SQL agent too, over content nobody reviewed.
            #
            # CREATEDB and CREATEROLE are NOT granted. The database is created
            # below as postgres, and the two separation roles are created and
            # granted here with ADMIN OPTION, which is what `server/faces.rs`
            # needs to `SET LOCAL ROLE` into them. In PG16+ a role may only
            # grant membership it has ADMIN on; without these two lines faces.rs
            # cannot grant them to itself, no applet table is readable, and the
            # failure reads as a bug in the applet rather than in bootstrap.
            su -s /bin/sh postgres -c "psql -tAc \"SELECT 1 FROM pg_roles WHERE rolname='virtues'\"" \
                2>/dev/null | grep -q 1 || \
                su -s /bin/sh postgres -c "psql -c \"CREATE ROLE virtues LOGIN\"" >/dev/null 2>&1
            # Idempotent downgrade, for a box imaged from a master built before
            # this change. Re-running costs nothing; not running it leaves a
            # superuser in the field forever.
            su -s /bin/sh postgres -c "psql -c \"ALTER ROLE virtues NOSUPERUSER NOCREATEDB NOCREATEROLE\"" >/dev/null 2>&1
            for sep_role in virtues_face_reader virtues_applet_writer; do
                su -s /bin/sh postgres -c "psql -tAc \"SELECT 1 FROM pg_roles WHERE rolname='$sep_role'\"" \
                    2>/dev/null | grep -q 1 || \
                    su -s /bin/sh postgres -c "psql -c \"CREATE ROLE $sep_role NOLOGIN\"" >/dev/null 2>&1
                # Unconditional: a cluster may carry the role without the grant.
                su -s /bin/sh postgres -c "psql -c \"GRANT $sep_role TO virtues WITH ADMIN OPTION\"" >/dev/null 2>&1
            done
            su -s /bin/sh postgres -c "psql -tAc \"SELECT 1 FROM pg_database WHERE datname='virtues'\"" \
                2>/dev/null | grep -q 1 || \
                su -s /bin/sh postgres -c "createdb -O virtues virtues" >/dev/null 2>&1
            # As postgres, so the app role never needs the elevation: pgvector
            # is not a trusted extension, and migration 0001's
            # `CREATE EXTENSION IF NOT EXISTS vector` is then a no-op.
            su -s /bin/sh postgres -c "psql -d virtues -c 'CREATE EXTENSION IF NOT EXISTS vector'" >/dev/null 2>&1
            # No migrations here. `virtues server` runs them at startup, which
            # keeps ONE migration path for every box rather than a first-boot
            # copy of it that could drift.
            # Hand the running server back to systemd: stop the pg_ctl one so
            # the ordinary unit start (queued behind this script) finds the
            # cluster stopped and owns it from here on.
            su -s /bin/sh postgres -c "$PG_CTL -D $PG_MAIN -w -t 60 stop" >/dev/null 2>&1 || true
            logger -t virtues-firstboot "Postgres cluster $PG_VER/main created on the data disk"
        else
            logger -t virtues-firstboot "pg_createcluster FAILED - the box will not serve until this is fixed"
        fi
    fi
fi

# ── 1c'. Repair a truncated start.conf, then requeue the chain — every boot ─
# The postgresql-common generator reads start.conf at EARLY boot: "auto"
# starts the cluster, anything else — including the zero-length file a
# first-boot power yank left behind (2026-08-25 forensics; see the trap at
# the top) — silently doesn't, and the postgresql.service umbrella reports
# active over the missing instance forever. Repair the file for future boots'
# generators; requeue BY NAME for this one — requeue_services does not depend
# on the generator's wants-links, so this heals a wedged unit in place. The
# repair rewrites only a file that says nothing; a deliberate manual/disabled
# is kept. Unconditional requeue is idempotent: starting active units is a
# no-op, and on a first boot this runs after §1c so the cluster exists.
for sc in /etc/postgresql/*/*/start.conf; do
    [ -e "$sc" ] || continue
    if ! grep -qE '^[[:space:]]*(auto|manual|disabled)' "$sc" 2>/dev/null; then
        printf 'auto\n' > "$sc"
        logger -t virtues-firstboot "repaired empty/invalid $sc to 'auto'"
    fi
done
requeue_services

# ── 1d. A provisioned disk with no env file is a FAILURE, said out loud ─────
# The 2026-08-19 hollow unit reported status=0/SUCCESS on every boot while
# virtues.service crash-looped beside it, because nothing here ever checked
# the one file everything downstream needs. A red unit in `systemctl --failed`
# is the difference between a glance and a UART cable. Appliance-shaped boxes
# only: on DIY, $DATA_DIR is a plain directory on the root fs and the env
# file's absence is the installer's business, not first boot's.
if mountpoint -q "$DATA_DIR" 2>/dev/null && ! grep -q '^DATABASE_URL=' "$ENV_FILE" 2>/dev/null; then
    logger -t virtues-firstboot "PROVISIONING INCOMPLETE: $ENV_FILE is missing or has no DATABASE_URL - the box cannot serve"
    exit 1
fi

# ── 2. Mint this unit's encryption key ──────────────────────────────────────
[ -e "$MARKER" ] || exit 0

if grep -q '^VIRTUES_ENCRYPTION_KEY=' "$ENV_FILE" 2>/dev/null; then
    # Marker present but a key already exists: do NOT rotate it — that would
    # strand whatever is already encrypted. Just disarm and carry on.
    rm -f "$MARKER"
    logger -t virtues-firstboot "marker present but key already set - disarming, not rotating"
    exit 0
fi

umask 077
KEY="$(openssl rand -base64 32)"
printf 'VIRTUES_ENCRYPTION_KEY=%s\n' "$KEY" >> "$ENV_FILE"
chown virtues:virtues "$ENV_FILE" 2>/dev/null || true
chmod 600 "$ENV_FILE"

# The key must be ON DISK before the marker leaves: the reverse order with a
# power cut between them is a box with no key and no licence to mint one —
# and everything encrypted so far is garbage with nothing in the logs to say why.
sync
rm -f "$MARKER"
logger -t virtues-firstboot "minted per-unit encryption key"
