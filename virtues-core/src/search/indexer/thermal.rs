//! Pausing embedding while the CPU is hot.
//!
//! Indexing sends batches back to back, so the embedding server's speed is the
//! only limit on how hard it works the machine. On a desktop that is fine. On
//! a fanless board it is not: measured on a Radxa Dragon Q6A, EmbeddingGemma 2
//! at 4 threads reached the board's 90 °C throttle point within 30 seconds and
//! stayed there, and a model change re-embeds the whole index, which takes days
//! on such a board.
//!
//! So before each batch, [`wait_until_cool`] reads the CPU's thermal zones
//! from sysfs. When one is within 10 °C of its first throttle point, embedding
//! waits until every zone is 10 °C below that again. A machine whose zones
//! can't be read is never paused.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// How far below a zone's throttle point embedding pauses, and how much
/// further it must cool before embedding resumes.
const MARGIN_C: f64 = 10.0;
/// The pause point for a zone that reports no throttle point.
const DEFAULT_PAUSE_C: f64 = 80.0;
/// How often a paused run reads the temperature again.
const RECHECK: Duration = Duration::from_secs(15);
/// The longest one pause lasts. In a room too warm for the board to cool, one
/// batch then goes ahead and the kernel's own throttling protects the chip;
/// the pause only has to keep indexing from holding it there.
const MAX_PAUSE: Duration = Duration::from_secs(10 * 60);

/// One CPU thermal zone: its temperature and where embedding pauses for it.
#[derive(Debug, Clone, PartialEq)]
struct Zone {
    temp_c: f64,
    pause_c: f64,
}

impl Zone {
    fn hot(&self) -> bool {
        self.temp_c >= self.pause_c
    }
    fn cool(&self) -> bool {
        self.temp_c <= self.pause_c - MARGIN_C
    }
}

/// Wait while the CPU is too hot to embed. Returns at once when it isn't.
pub(crate) async fn wait_until_cool() {
    wait_until_cool_at(Path::new("/sys/class/thermal")).await
}

async fn wait_until_cool_at(root: &Path) {
    let zones = cpu_zones(root);
    let Some(hottest) = zones.iter().filter(|z| z.hot()).map(|z| z.temp_c).reduce(f64::max) else {
        return;
    };
    tracing::info!(temp_c = hottest, "CPU is hot; embedding pauses until it cools");
    let started = Instant::now();
    loop {
        tokio::time::sleep(RECHECK).await;
        let zones = cpu_zones(root);
        if zones.iter().all(Zone::cool) {
            tracing::info!(paused_s = started.elapsed().as_secs(), "CPU has cooled; embedding resumes");
            return;
        }
        if started.elapsed() >= MAX_PAUSE {
            tracing::info!("CPU still warm after the longest pause; embedding one more batch");
            return;
        }
    }
}

/// The CPU zones under `root` (`/sys/class/thermal`). Zones for other parts
/// (battery, modem, GPU, the ACPI board sensor) are left out: a phone-sized
/// board reports dozens, and only the CPU's say whether embedding heats it.
fn cpu_zones(root: &Path) -> Vec<Zone> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("thermal_zone")))
        .filter(|p| read(p.join("type")).is_some_and(|t| is_cpu_zone(&t)))
        .filter_map(|p| {
            let temp_c = millideg(&p.join("temp"))?;
            Some(Zone { temp_c, pause_c: throttle_point(&p).map_or(DEFAULT_PAUSE_C, |t| t - MARGIN_C) })
        })
        .collect()
}

/// Whether a zone's `type` names the CPU: `cpu0-thermal`, `cpuss0-thermal`
/// and `cpu-big0-thermal` on Arm boards, `x86_pkg_temp` on Intel. AMD reports
/// its CPU temperature through hwmon (`k10temp`), not a thermal zone, so an AMD
/// machine is never paused.
fn is_cpu_zone(kind: &str) -> bool {
    let k = kind.trim().to_ascii_lowercase();
    k.starts_with("cpu") || k == "x86_pkg_temp" || k.contains("cpu-thermal")
}

/// The lowest `passive` or `hot` trip point: where the kernel starts slowing
/// the chip. `critical` is shutdown and `active` is a fan step, neither of
/// which is the point to stay away from.
fn throttle_point(zone: &Path) -> Option<f64> {
    let entries = std::fs::read_dir(zone).ok()?;
    entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter_map(|p| {
            let name = p.file_name()?.to_string_lossy().into_owned();
            let n = name.strip_prefix("trip_point_")?.strip_suffix("_type")?.to_string();
            let kind = read(p.clone())?;
            matches!(kind.trim(), "passive" | "hot").then(|| zone.join(format!("trip_point_{n}_temp")))
        })
        .filter_map(|p: PathBuf| millideg(&p))
        .filter(|t| *t > 0.0)
        .reduce(f64::min)
}

fn read(p: PathBuf) -> Option<String> {
    std::fs::read_to_string(p).ok()
}

fn millideg(p: &Path) -> Option<f64> {
    std::fs::read_to_string(p).ok()?.trim().parse::<f64>().ok().map(|m| m / 1000.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zone(root: &Path, n: u32, kind: &str, temp_c: u32, trips: &[(&str, u32)]) {
        let z = root.join(format!("thermal_zone{n}"));
        std::fs::create_dir_all(&z).unwrap();
        std::fs::write(z.join("type"), format!("{kind}\n")).unwrap();
        std::fs::write(z.join("temp"), format!("{}\n", temp_c * 1000)).unwrap();
        for (i, (k, t)) in trips.iter().enumerate() {
            std::fs::write(z.join(format!("trip_point_{i}_type")), format!("{k}\n")).unwrap();
            std::fs::write(z.join(format!("trip_point_{i}_temp")), format!("{}\n", t * 1000)).unwrap();
        }
    }

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("virtues-thermal-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// The Dragon's layout: per-core zones with passive trips at 90 and 95
    /// and a critical one at 110, beside zones for other parts.
    #[test]
    fn a_cpu_zone_pauses_ten_degrees_under_its_first_throttle_point() {
        let d = tmp("dragon");
        let trips = [("passive", 90), ("passive", 95), ("critical", 110)];
        zone(&d, 0, "cpu0-thermal", 46, &trips);
        zone(&d, 1, "cpu7-thermal", 81, &trips);
        zone(&d, 2, "gpuss0-thermal", 99, &[("hot", 90)]);
        zone(&d, 3, "pm7325-thermal", 120, &[]);
        let zones = cpu_zones(&d);
        assert_eq!(zones.len(), 2, "only the CPU zones count: {zones:?}");
        assert!(zones.iter().all(|z| z.pause_c == 80.0));
        assert!(zones.iter().any(Zone::hot), "81 °C is past the 80 °C pause point");
        assert!(!zones.iter().all(Zone::cool), "and not yet under 70 °C");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_zone_without_trip_points_pauses_at_eighty() {
        let d = tmp("notrips");
        zone(&d, 0, "x86_pkg_temp", 79, &[]);
        let zones = cpu_zones(&d);
        assert_eq!(zones, vec![Zone { temp_c: 79.0, pause_c: DEFAULT_PAUSE_C }]);
        assert!(!zones[0].hot());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[tokio::test]
    async fn a_cool_or_unreadable_machine_never_waits() {
        let d = tmp("cool");
        zone(&d, 0, "cpu0-thermal", 50, &[("passive", 90)]);
        let started = Instant::now();
        wait_until_cool_at(&d).await;
        wait_until_cool_at(&d.join("missing")).await;
        assert!(started.elapsed() < Duration::from_secs(1));
        let _ = std::fs::remove_dir_all(&d);
    }
}
