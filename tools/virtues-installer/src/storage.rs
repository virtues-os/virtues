//! Storage pre-flight — classify and MEASURE the disk that will hold the data
//! dir before we ever put Postgres on it.
//!
//! Why this exists: the single biggest determinant of whether a home box feels
//! fast or miserable is the storage under `/var/lib/virtues`. Vector search is
//! random-read heavy and the WAL is fsync heavy, so an SD card or a lying USB
//! bridge turns "instant" into "multi-second" — or, in the NFS case, into
//! silent corruption. `preflight.rs` already fails fast on *environmental*
//! problems (no disk, no net, port squatted); this module extends that
//! philosophy to *storage quality*: we don't just check there's room, we
//! classify the medium and put real numbers (MB/s, fsync latency) in front of
//! the user so a bad choice is a conscious one.
//!
//! Diagnosis only. Nothing here formats, repartitions, or moves data — it
//! writes a small probe file to a tempdir and deletes it. Every warning is
//! non-blocking (mirrors preflight's "warn, don't abort" stance): a user who
//! knows their SD card is slow is allowed to proceed anyway, and `virtues
//! doctor` re-reports storage status later.

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::Result;

use crate::ui;

/// The medium under the data dir. Drives the verdict tier + copy.
enum DeviceClass {
    /// `nvme*` — the good case.
    Nvme,
    /// rotational==0, not nvme/mmc/usb — a plain SATA/other SSD. Also good.
    SataSsd,
    /// `mmcblk*` with a `mmcblkNboot0` sibling — soldered eMMC. Usable but
    /// slow + wears under sustained writes.
    Emmc,
    /// `mmcblk*` without boot0 — a removable microSD. The worst common case
    /// on SBCs: slow and it wears out under database load.
    SdCard,
    /// Any device whose sysfs path runs through a `usb` controller. The
    /// concern is less speed than bridge honesty (cache-flush handling).
    Usb,
    /// rotational==1 — a spinning disk. Fine for bulk, terrible for the random
    /// reads a vector index does.
    Hdd,
    /// Network filesystem (nfs/cifs/smb). The hard case — Postgres on NFS is a
    /// documented corruption class. Carries the fs type for the message.
    Network(String),
    /// Couldn't map to a block device (overlay/tmpfs/dm-crypt/LVM we didn't
    /// unwrap, or we're not on Linux). Report what we measured, classify
    /// nothing.
    Unknown,
}

/// What the ~5s probe actually observed. All fields optional so a read-only or
/// permission-denied mount degrades to classification-only instead of erroring
/// out the whole pre-flight.
struct Measurement {
    /// Sequential write throughput, MB/s.
    write_mb_s: Option<f64>,
    /// Median fsync latency in microseconds over N small writes. The honesty
    /// probe: a median an order of magnitude faster than the medium can
    /// physically flush means the device is acking flushes it hasn't done.
    fsync_median_us: Option<u128>,
    /// Set when we could not measure (read-only mount, no permission); becomes
    /// a note on the verdict rather than a hard failure.
    unmeasured_reason: Option<String>,
}

/// Entry point. Classify + measure the disk behind `data_dir` and print a
/// verdict with numbers. Never returns Err for a *storage* problem — only for
/// genuinely unexpected failures — because storage quality is advisory, not a
/// gate (matches preflight's non-blocking philosophy).
pub async fn report(data_dir: &Path) -> Result<()> {
    // The install hasn't created /var/lib/virtues yet, so stat the deepest
    // ancestor that *does* exist — that's the mount/medium the data dir will
    // land on, and it's where we're allowed to write a probe file.
    let anchor = nearest_existing_ancestor(data_dir);

    // Map the anchor to its backing block device via the kernel's own view
    // (/proc/self/mountinfo + /sys), then classify the medium.
    let class = classify_device(&anchor);

    // Measure it. Heavy blocking I/O, but pre-flight is strictly sequential
    // here (nothing else is running on the runtime), so an inline blocking
    // probe is fine and keeps the code readable.
    let m = measure(&anchor);

    verdict(data_dir, &anchor, &class, &m);
    Ok(())
}

// ─── Device resolution ──────────────────────────────────────────────────────

/// Walk up from `p` to the nearest path that exists. `/var/lib/virtues` won't
/// exist on a fresh box, but `/var/lib` (or at worst `/`) will, and it sits on
/// the same filesystem the data dir will be created under.
fn nearest_existing_ancestor(p: &Path) -> PathBuf {
    let mut cur = p;
    loop {
        if cur.exists() {
            return cur.to_path_buf();
        }
        match cur.parent() {
            Some(parent) => cur = parent,
            None => return PathBuf::from("/"),
        }
    }
}

/// A mount as seen in /proc/self/mountinfo.
struct MountInfo {
    /// Filesystem type (field after the " - " separator), e.g. `ext4`, `nfs4`.
    fs_type: String,
    /// The st_dev "major:minor" (field 3). We resolve the device through
    /// /sys/dev/block/<majmin> rather than parsing the /dev/ source string,
    /// because that handles partitions, dm/LVM, and USB uniformly and is what
    /// the kernel itself keys on.
    majmin: String,
}

/// Find the mount covering `path`: the entry whose mount point is the longest
/// path-boundary prefix of `path`. Returns None off-Linux or if mountinfo is
/// unreadable (we then classify Unknown but can still measure).
fn find_mount(path: &Path) -> Option<MountInfo> {
    let content = fs::read_to_string("/proc/self/mountinfo").ok()?;
    let target = path.to_string_lossy();

    let mut best: Option<(usize, MountInfo)> = None;
    for line in content.lines() {
        // Split optional fields from fs fields on the mandatory " - " marker.
        let (left, right) = line.split_once(" - ")?;
        let lf: Vec<&str> = left.split_whitespace().collect();
        let rf: Vec<&str> = right.split_whitespace().collect();
        // left: mountID parentID major:minor root mountpoint options [tags...]
        // right: fstype source superopts
        if lf.len() < 5 || rf.is_empty() {
            continue;
        }
        let majmin = lf[2].to_string();
        let mountpoint = unescape_mount(lf[4]);
        let fs_type = rf[0].to_string();

        // Path-boundary prefix match so /var doesn't spuriously match /varchive.
        let covers = mountpoint == "/"
            || target == mountpoint
            || target.starts_with(&format!("{mountpoint}/"));
        if !covers {
            continue;
        }
        let len = mountpoint.len();
        if best.as_ref().map(|(l, _)| len > *l).unwrap_or(true) {
            best = Some((len, MountInfo { fs_type, majmin }));
        }
    }
    best.map(|(_, mi)| mi)
}

/// mountinfo octal-escapes space/tab/newline/backslash in the mount point.
/// We only really need spaces, but decode the common four to be safe.
fn unescape_mount(s: &str) -> String {
    s.replace("\\040", " ")
        .replace("\\011", "\t")
        .replace("\\012", "\n")
        .replace("\\134", "\\")
}

/// A block device resolved from its major:minor.
struct ResolvedDev {
    /// The leaf device name, e.g. `nvme0n1p1`, `sda1`, `mmcblk1p2`.
    name: String,
    /// Whether the sysfs device path runs through a USB controller.
    is_usb: bool,
}

/// Resolve major:minor → device via /sys/dev/block/<majmin>, which is a symlink
/// deep into the /sys/devices tree. The leaf name is the (partition) device;
/// the presence of "usb" anywhere in the resolved path means it's USB-attached.
fn resolve_device(majmin: &str) -> Option<ResolvedDev> {
    let link = Path::new("/sys/dev/block").join(majmin);
    let real = fs::canonicalize(&link).ok()?;
    let name = real.file_name()?.to_string_lossy().to_string();
    let is_usb = real.to_string_lossy().contains("usb");
    Some(ResolvedDev { name, is_usb })
}

/// Reduce a partition device name to its parent whole-disk name for the sysfs
/// lookups: `nvme0n1p3` → `nvme0n1`, `mmcblk1p2` → `mmcblk1`, `sda1` → `sda`.
/// nvme/mmc use a `p<N>` partition suffix; everything else appends bare digits.
fn parent_disk(dev: &str) -> String {
    if dev.starts_with("nvme") || dev.starts_with("mmcblk") {
        if let Some(idx) = dev.rfind('p') {
            let tail = &dev[idx + 1..];
            if !tail.is_empty() && tail.bytes().all(|b| b.is_ascii_digit()) {
                return dev[..idx].to_string();
            }
        }
        return dev.to_string();
    }
    let trimmed = dev.trim_end_matches(|c: char| c.is_ascii_digit());
    if trimmed.is_empty() {
        dev.to_string()
    } else {
        trimmed.to_string()
    }
}

/// Prefer the parent disk if it's a real entry in /sys/block; otherwise fall
/// back to the leaf (whole-disk mounts, or names we couldn't reduce).
fn disk_for_sysfs(dev: &str) -> String {
    let parent = parent_disk(dev);
    if Path::new("/sys/block").join(&parent).exists() {
        parent
    } else if Path::new("/sys/block").join(dev).exists() {
        dev.to_string()
    } else {
        parent
    }
}

fn classify_device(anchor: &Path) -> DeviceClass {
    let Some(mi) = find_mount(anchor) else {
        return DeviceClass::Unknown;
    };

    // Network filesystems never map to a local block device — decide purely on
    // the fs type. This is the case we most want to catch.
    let fs = mi.fs_type.to_ascii_lowercase();
    if matches!(fs.as_str(), "nfs" | "nfs4" | "cifs" | "smb" | "smbfs" | "smb3") {
        return DeviceClass::Network(mi.fs_type);
    }

    let Some(dev) = resolve_device(&mi.majmin) else {
        return DeviceClass::Unknown;
    };
    let disk = disk_for_sysfs(&dev.name);
    let sysblock = Path::new("/sys/block").join(&disk);
    if !sysblock.exists() {
        // overlay / tmpfs / dm-crypt / LVM we didn't unwrap — punt to Unknown.
        return DeviceClass::Unknown;
    }

    if disk.starts_with("nvme") {
        return DeviceClass::Nvme;
    }
    if disk.starts_with("mmcblk") {
        // eMMC exposes hardware boot partitions (mmcblkNboot0/boot1); a bare
        // SD card does not. This is the reliable eMMC-vs-SD discriminator.
        let boot0 = Path::new("/sys/block").join(format!("{disk}boot0"));
        return if boot0.exists() {
            DeviceClass::Emmc
        } else {
            DeviceClass::SdCard
        };
    }
    // USB before rotational: a USB-attached SSD is rotational==0 but we still
    // want to warn about the bridge, so USB classification wins.
    if dev.is_usb {
        return DeviceClass::Usb;
    }
    if read_flag(&sysblock.join("queue/rotational")) == Some(true) {
        return DeviceClass::Hdd;
    }
    DeviceClass::SataSsd
}

/// Read a sysfs boolean flag file (`"1"`/`"0"`).
fn read_flag(p: &Path) -> Option<bool> {
    Some(fs::read_to_string(p).ok()?.trim() == "1")
}

// ─── Measurement ────────────────────────────────────────────────────────────

/// Probe budget knobs. The whole probe must stay comfortably under ~5s so it
/// doesn't dominate pre-flight even on a dreadful SD card.
const SEQ_TARGET_BYTES: usize = 128 * 1024 * 1024; // 128 MB sample
const SEQ_TIME_CAP: Duration = Duration::from_secs(3);
const FSYNC_ITERS: usize = 200;
const FSYNC_TIME_CAP: Duration = Duration::from_millis(1500);

fn measure(anchor: &Path) -> Measurement {
    // Everything happens in a self-cleaning tempdir on the target medium. If we
    // can't even create it (read-only mount, no permission), degrade to
    // classification-only rather than failing the install.
    let dir = match tempfile::Builder::new()
        .prefix(".virtues-storage-probe")
        .tempdir_in(anchor)
    {
        Ok(d) => d,
        Err(e) => {
            return Measurement {
                write_mb_s: None,
                fsync_median_us: None,
                unmeasured_reason: Some(format!("cannot write here ({e})")),
            }
        }
    };

    let write_mb_s = match seq_write(dir.path()) {
        Ok(v) => Some(v),
        Err(_) => None,
    };
    let fsync_median_us = fsync_median(dir.path()).ok();

    let unmeasured_reason = if write_mb_s.is_none() && fsync_median_us.is_none() {
        Some("probe writes failed".to_string())
    } else {
        None
    };

    // dir (and its contents) removed on drop — nothing destructive left behind.
    Measurement {
        write_mb_s,
        fsync_median_us,
        unmeasured_reason,
    }
}

/// Sequential write throughput in MB/s. Writes 1 MB chunks up to SEQ_TARGET or
/// the time cap (whichever first, so a slow card doesn't blow the budget), then
/// fsyncs so the number reflects real durable throughput, not page cache.
fn seq_write(dir: &Path) -> Result<f64> {
    let path = dir.join("seq.bin");
    let mut f = File::create(&path)?;
    let buf = vec![0xA5u8; 1024 * 1024]; // non-zero to defeat sparse/compress fs

    let start = Instant::now();
    let mut written = 0usize;
    while written < SEQ_TARGET_BYTES && start.elapsed() < SEQ_TIME_CAP {
        f.write_all(&buf)?;
        written += buf.len();
    }
    f.sync_all()?; // durability: fold the flush into the timed window
    let secs = start.elapsed().as_secs_f64();
    let mb = written as f64 / (1024.0 * 1024.0);
    Ok(if secs > 0.0 { mb / secs } else { 0.0 })
}

/// Median fsync latency (µs) over many tiny write+fsync cycles. This is the
/// honesty probe: real durable flushes on flash/spinning media cost tens of µs
/// minimum, so a suspiciously tiny median means the device is acking flushes it
/// buffered in volatile cache.
fn fsync_median(dir: &Path) -> Result<u128> {
    let path = dir.join("fsync.bin");
    let mut f = File::create(&path)?;
    let block = vec![0x5Au8; 4096];

    let mut samples: Vec<u128> = Vec::with_capacity(FSYNC_ITERS);
    let overall = Instant::now();
    for _ in 0..FSYNC_ITERS {
        if overall.elapsed() > FSYNC_TIME_CAP {
            break;
        }
        f.write_all(&block)?;
        let t = Instant::now();
        f.sync_all()?;
        samples.push(t.elapsed().as_micros());
    }
    if samples.is_empty() {
        anyhow::bail!("no fsync samples");
    }
    samples.sort_unstable();
    Ok(samples[samples.len() / 2])
}

// ─── Verdict ────────────────────────────────────────────────────────────────

/// fsync medians below this on media that physically cannot flush that fast are
/// evidence of a lying write cache. Sub-100µs is legitimate on NVMe with
/// power-loss protection, so on SSD/NVMe/USB we only *soft*-warn below this.
const SUSPICIOUS_FSYNC_US: u128 = 50;

fn verdict(data_dir: &Path, anchor: &Path, class: &DeviceClass, m: &Measurement) {
    // A compact "measured NN MB/s write" fragment, or a note when we couldn't.
    let speed = match m.write_mb_s {
        Some(v) => format!("measured {v:.0} MB/s write"),
        None => "write speed unmeasured".to_string(),
    };
    // The dim tail every severe warning carries: matches preflight's stance
    // that these inform, they don't gate.
    let nonblock = "Won't block the install; `virtues doctor` shows storage status.";

    match class {
        DeviceClass::Nvme => {
            ui::ok(&format!("Storage: NVMe SSD ({speed}) — great"));
        }
        DeviceClass::SataSsd => {
            ui::ok(&format!("Storage: SATA/SSD ({speed}) — great"));
        }
        DeviceClass::Emmc => {
            ui::warn(&format!(
                "Storage: eMMC ({speed}) — fine up to ~100k items; large index \
                 builds will be slow and the flash wears under sustained writes."
            ));
            ui::skip(nonblock);
        }
        DeviceClass::SdCard => {
            ui::warn(&format!(
                "Storage: microSD ({speed}) — searches will feel slow and the \
                 card will wear out under database load; an NVMe/SSD is strongly \
                 recommended."
            ));
            ui::skip(nonblock);
        }
        DeviceClass::Usb => {
            ui::warn(&format!(
                "Storage: USB-attached ({speed}) — some USB bridges don't honor \
                 cache-flush, risking corruption on power loss."
            ));
            ui::skip(nonblock);
        }
        DeviceClass::Hdd => {
            ui::warn(&format!(
                "Storage: spinning disk ({speed}) — vector search does many \
                 random reads; expect multi-second searches."
            ));
            ui::skip(nonblock);
        }
        DeviceClass::Network(fs) => {
            // The refuse-in-spirit case. We don't abort (this task is
            // diagnosis-only), but we make it as loud as a plain warn allows.
            ui::warn(&format!(
                "Storage: NETWORK filesystem ({fs}) — Postgres on {fs} is a known \
                 data-corruption risk (broken locking/flush semantics). Point \
                 DATA_DIR at a LOCAL disk before going further."
            ));
            ui::warn(&format!("Detected at {}.", anchor.display()));
            ui::skip(nonblock);
        }
        DeviceClass::Unknown => {
            ui::warn(&format!(
                "Storage: could not identify the medium under {} ({speed}) — \
                 if this is a network/overlay/encrypted volume, verify it's a \
                 real local disk.",
                data_dir.display()
            ));
            ui::skip(nonblock);
        }
    }

    // fsync-honesty line, independent of the medium verdict above.
    if let Some(us) = m.fsync_median_us {
        let suspicious = us < SUSPICIOUS_FSYNC_US;
        match class {
            // Physically impossible for these media → the device is lying.
            DeviceClass::Hdd | DeviceClass::SdCard | DeviceClass::Emmc if suspicious => {
                ui::warn(&format!(
                    "fsync median {us}µs — impossible for this medium; the device \
                     is acking flushes without doing them (volatile write cache). \
                     Real risk of corruption on power loss."
                ));
                ui::skip(nonblock);
            }
            // Legit on enterprise flash, but worth a soft nudge to verify.
            DeviceClass::Nvme | DeviceClass::SataSsd | DeviceClass::Usb if suspicious => {
                ui::warn(&format!(
                    "fsync median {us}µs — very fast; verify this drive honors \
                     cache flushes (power-loss protection) rather than buffering \
                     in volatile cache."
                ));
            }
            _ => {}
        }
    }

    // Degradation note when we classified but couldn't measure.
    if let Some(reason) = &m.unmeasured_reason {
        ui::skip(&format!("Storage probe skipped: {reason} — classification only."));
    }

    // Always close with free space + a rough capacity projection, folding in
    // the space check preflight also does (kept there for the / partition; this
    // one is specifically the DATA_DIR medium).
    if let Some(bytes) = free_bytes(anchor) {
        let free_gb = bytes / 1024 / 1024 / 1024;
        // ~4 KB/item is a rough all-in figure (vector + index + WAL headroom).
        let items_millions = bytes as f64 / 4096.0 / 1_000_000.0;
        ui::ok(&format!(
            "Free space on data volume: {free_gb} GB (~{items_millions:.0}M items, rough)"
        ));
    } else {
        ui::warn("Free space on data volume: could not query");
    }
}

/// Free bytes available on the filesystem containing `path`. Same statvfs call
/// preflight uses, kept local so this module stands alone.
fn free_bytes(path: &Path) -> Option<u64> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let c = CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    let rc = unsafe { libc::statvfs(c.as_ptr(), &mut stat) };
    if rc != 0 {
        return None;
    }
    Some(stat.f_bavail as u64 * stat.f_frsize as u64)
}
