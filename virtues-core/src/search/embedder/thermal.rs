//! Pausing background embedding while the CPU is hot.
//!
//! Indexing, a model change and the day rescoring that follows one send
//! requests back to back, so the embedding server's speed is the only limit on
//! how hard they work the machine. On a desktop that is fine. On a fanless
//! board it is not: measured on a Radxa Dragon Q6A, EmbeddingGemma 2 at 4
//! threads reached the board's 90 °C throttle point within 30 seconds and
//! stayed there, and a model change re-embeds the whole index, which takes days
//! on such a board.
//!
//! So before each document request ([`super::LocalEmbedder::embed_batch_async`]
//! and `embed_async`), [`wait_until_cool`] reads the CPU's thermal zones from
//! sysfs. When one is within [`PAUSE_BELOW_C`] of its first throttle point,
//! embedding waits until every such zone is [`RESUME_BELOW_C`] below it.
//! Search queries never wait.
//!
//! Only zones that declare a throttle point (a `passive` or `hot` trip) count.
//! Arm boards declare one in their device tree. Intel's `x86_pkg_temp` usually
//! reports none, and its firmware throttles on its own at a TjMax that busy
//! machines run close to, so a guessed threshold would pause healthy machines
//! all day. A machine with no such zone is never paused.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// How far below a zone's throttle point embedding pauses.
const PAUSE_BELOW_C: f64 = 5.0;
/// How far below it the zone must cool before embedding resumes.
const RESUME_BELOW_C: f64 = 10.0;
/// How often a paused run reads the temperature again.
const RECHECK: Duration = Duration::from_secs(15);
/// The longest one pause lasts. In a room too warm for the board to cool, one
/// request then goes ahead and the kernel's own throttling protects the chip;
/// the pause only has to keep embedding from holding it there.
const MAX_PAUSE: Duration = Duration::from_secs(10 * 60);

/// One CPU thermal zone that declares a throttle point.
#[derive(Debug, Clone, PartialEq)]
struct Zone {
    temp_c: f64,
    throttle_c: f64,
}

impl Zone {
    fn hot(&self) -> bool {
        self.temp_c >= self.throttle_c - PAUSE_BELOW_C
    }
    fn cool(&self) -> bool {
        self.temp_c <= self.throttle_c - RESUME_BELOW_C
    }
}

/// Wait while the CPU is too hot to embed. Returns at once when it isn't.
pub(super) async fn wait_until_cool() {
    wait_until_cool_at(Path::new("/sys/class/thermal")).await
}

async fn wait_until_cool_at(root: &Path) {
    let zones = cpu_zones(root);
    let Some(hottest) = zones.iter().filter(|z| z.hot()).map(|z| z.temp_c).reduce(f64::max) else {
        return;
    };
    tracing::info!(temp_c = hottest, "CPU is near its throttle point; embedding pauses until it cools");
    let started = Instant::now();
    loop {
        tokio::time::sleep(RECHECK).await;
        if cpu_zones(root).iter().all(Zone::cool) {
            tracing::info!(paused_s = started.elapsed().as_secs(), "CPU has cooled; embedding resumes");
            return;
        }
        if started.elapsed() >= MAX_PAUSE {
            tracing::info!("CPU still warm after the longest pause; embedding goes ahead");
            return;
        }
    }
}

/// The CPU zones under `root` (`/sys/class/thermal`) that declare a throttle
/// point. Zones for other parts (battery, modem, GPU, the ACPI board sensor)
/// are left out: a phone-sized board reports dozens, and only the CPU's say
/// whether embedding heats it.
fn cpu_zones(root: &Path) -> Vec<Zone> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("thermal_zone")))
        .filter(|p| read(p.join("type")).is_some_and(|t| is_cpu_zone(&t)))
        .filter_map(|p| Some(Zone { temp_c: millideg(&p.join("temp"))?, throttle_c: throttle_point(&p)? }))
        .collect()
}

/// Whether a zone's `type` names the CPU: `cpu0-thermal`, `cpuss0-thermal`
/// and `cpu-thermal` (Qualcomm, Raspberry Pi), `bigcore0-thermal`,
/// `littlecore-thermal` and `soc-thermal` (Rockchip), `x86_pkg_temp` (Intel).
/// AMD reports its CPU temperature through hwmon (`k10temp`), not a thermal
/// zone, so an AMD machine is never paused.
fn is_cpu_zone(kind: &str) -> bool {
    let k = kind.trim().to_ascii_lowercase();
    k.starts_with("cpu")
        || k.starts_with("bigcore")
        || k.starts_with("littlecore")
        || k == "soc-thermal"
        || k == "x86_pkg_temp"
}

/// The lowest `passive` or `hot` trip point: where the kernel starts slowing
/// the chip. `critical` is shutdown and `active` is a fan step, neither of
/// which is the point to stay away from. Trips of 0 or below are unset.
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
    fn a_dragon_pauses_at_85_and_resumes_at_80() {
        let d = tmp("dragon");
        let trips = [("passive", 90), ("passive", 95), ("critical", 110)];
        zone(&d, 0, "cpu0-thermal", 46, &trips);
        zone(&d, 1, "cpu7-thermal", 86, &trips);
        zone(&d, 2, "gpuss0-thermal", 99, &[("hot", 90)]);
        zone(&d, 3, "pm7325-thermal", 120, &[("passive", 100)]);
        let zones = cpu_zones(&d);
        assert_eq!(zones.len(), 2, "only the CPU zones count: {zones:?}");
        assert!(zones.iter().all(|z| z.throttle_c == 90.0), "the lowest passive trip: {zones:?}");
        assert!(zones.iter().any(Zone::hot), "86 °C is past the 85 °C pause point");
        assert!(!zones.iter().all(Zone::cool), "and not yet under 80 °C");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Intel's package zone with unset trips, a zone with only a shutdown
    /// trip, and a Rockchip big-core zone with a real one.
    #[test]
    fn only_zones_with_a_throttle_point_count() {
        let d = tmp("trips");
        zone(&d, 0, "x86_pkg_temp", 95, &[("passive", 0)]);
        zone(&d, 1, "cpu-thermal", 95, &[("critical", 110)]);
        zone(&d, 2, "bigcore0-thermal", 60, &[("passive", 85), ("critical", 115)]);
        zone(&d, 3, "acpitz", 99, &[("passive", 90)]);
        assert_eq!(cpu_zones(&d), vec![Zone { temp_c: 60.0, throttle_c: 85.0 }]);
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
