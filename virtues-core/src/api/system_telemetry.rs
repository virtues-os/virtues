//! Host telemetry — the "activity monitor" behind the web System view.
//!
//! One session-authed snapshot of the machine the box runs on: CPU, memory,
//! disks, network, thermals, the Jetson GPU (via `tegrastats`), plus the
//! inference resolution + device/identity bits already computed elsewhere. The
//! web System view (`SystemInfoView.svelte`) polls `GET /api/system/telemetry`
//! on a calm cadence and typesets this like a ship's log rather than htop.
//!
//! Everything here is best-effort and cross-platform safe: on a CPU mini-PC or
//! a Mac dev box the GPU block is simply `None` and thermals may be empty. The
//! box serves no TLS and runs single-tenant, so this lives behind the session
//! layer (it carries process/network detail) but computes cheaply.

use std::sync::{Mutex, OnceLock, RwLock};
use std::time::{Duration, Instant};

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde::Serialize;

use crate::inference_report::{self, ModelSource};
use crate::server::webhook::AppState;

// ─── Wire shape ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct Telemetry {
    pub host: HostInfo,
    pub cpu: CpuInfo,
    pub memory: MemoryInfo,
    pub gpu: Option<GpuInfo>,
    pub thermal: Vec<ThermalSensor>,
    pub disks: Vec<DiskInfo>,
    pub network: NetworkInfo,
    pub inference: InferenceInfo,
    pub devices: DevicesInfo,
    pub services: Vec<ServiceInfo>,
    pub pool: PoolInfo,
    /// Top processes by memory. Empty unless requested with `?processes=1`
    /// (the web view asks only while the Detail panel is open — process
    /// enumeration is the heaviest sysinfo call, so the default poll skips it).
    pub processes: Vec<ProcessInfo>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    /// CPU% relative to a single core (so >100 means multi-core, as Activity
    /// Monitor reports it).
    pub cpu_pct: f32,
    pub mem: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct HostInfo {
    pub hostname: Option<String>,
    pub os: Option<String>,
    pub kernel: Option<String>,
    pub arch: String,
    pub uptime_secs: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CpuInfo {
    pub brand: String,
    pub physical_cores: Option<usize>,
    pub logical_cores: usize,
    /// Global utilization 0–100.
    pub usage_pct: f32,
    /// Per-logical-core utilization 0–100.
    pub per_core: Vec<f32>,
    pub frequency_mhz: u64,
    pub load_avg: LoadAvg,
}

#[derive(Debug, Clone, Serialize)]
pub struct LoadAvg {
    pub one: f64,
    pub five: f64,
    pub fifteen: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryInfo {
    pub total: u64,
    pub used: u64,
    pub available: u64,
    pub swap_total: u64,
    pub swap_used: u64,
}

/// Jetson GPU via `tegrastats` (or none on non-Tegra hosts). Fields are
/// individually optional because tegrastats output varies by JetPack/model.
#[derive(Debug, Clone, Serialize)]
pub struct GpuInfo {
    /// "jetson" today; left open for an NVML/Metal shim later.
    pub kind: String,
    pub name: Option<String>,
    /// GR3D (3D engine) utilization 0–100.
    pub usage_pct: Option<f32>,
    pub mem_used: Option<u64>,
    pub mem_total: Option<u64>,
    pub temp_c: Option<f32>,
    pub power_mw: Option<u64>,
    /// True when inference is actually landing on the GPU. The Jetson
    /// failure mode is silent CPU fallback (see GPU-group install note), so
    /// this is the headline signal, not decoration.
    pub offload_active: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ThermalSensor {
    pub label: String,
    pub temp_c: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiskInfo {
    pub name: String,
    pub mount: String,
    pub fs: String,
    pub total: u64,
    pub available: u64,
    pub removable: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct NetworkInfo {
    /// Bytes/sec aggregated across interfaces, sampled over the refresh window.
    pub rx_per_sec: u64,
    pub tx_per_sec: u64,
    pub interfaces: Vec<InterfaceInfo>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InterfaceInfo {
    pub name: String,
    pub rx_total: u64,
    pub tx_total: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct InferenceInfo {
    pub accelerator: String,
    pub precision: String,
    pub models_baked: bool,
    pub models: Vec<InferenceModel>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InferenceModel {
    pub name: String,
    pub repo: String,
    /// "baked" | "download"
    pub source: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DevicesInfo {
    pub paired_wg: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ServiceInfo {
    pub name: String,
    /// host:port probed
    pub endpoint: String,
    pub up: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct PoolInfo {
    pub size: u32,
    pub idle: usize,
}

// ─── Handler ────────────────────────────────────────────────────────────────

/// `GET /api/system/telemetry` — one host snapshot for the web System view.
///
/// Session-authed (carries process/network detail). Takes a brief sample
/// window inside a blocking task so CPU% and network rates are meaningful;
/// the GPU probe runs concurrently. Every sub-collector is fail-soft.
pub async fn telemetry_handler(
    _user: crate::middleware::auth::AuthUser,
    Query(params): Query<TelemetryParams>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let with_processes = params.processes.unwrap_or(0) != 0;

    // Host metrics — a brief blocking sysinfo sample (CPU%/net rates need a
    // window). Kept per-request by design; it's cheap and bounded. Process
    // enumeration is only done when requested (Detail panel open).
    let host_metrics = tokio::task::spawn_blocking(move || collect_host(with_processes))
        .await
        .unwrap_or_else(|_| HostMetrics::default());

    // Jetson GPU — read the latest sample from the background monitor (O(1), no
    // per-request process spawn). `None` on non-Tegra hosts. Reading bumps the
    // monitor's access clock so tegrastats keeps streaming while the view is open.
    let gpu = read_gpu();

    // Inference resolution — reuse the one source of truth the CLI/doctor use.
    let report = inference_report::resolution_report();
    let models_baked = report
        .models
        .iter()
        .all(|m| matches!(m.source, ModelSource::Baked(_)));
    let inference = InferenceInfo {
        accelerator: report.accelerator.clone(),
        precision: report.precision.clone(),
        models_baked,
        models: report
            .models
            .iter()
            .map(|m| InferenceModel {
                name: m.name.to_string(),
                repo: m.repo.to_string(),
                source: match m.source {
                    ModelSource::Baked(_) => "baked".to_string(),
                    ModelSource::Download => "download".to_string(),
                },
            })
            .collect(),
    };

    // Devices/identity — reuse box_status' computation (paired count + endpoint).
    let devices = match crate::api::box_status::compute_status(state.db.pool()).await {
        Ok(s) => DevicesInfo {
            paired_wg: s.devices.paired_wg,
        },
        Err(_) => DevicesInfo { paired_wg: 0 },
    };

    // Sidecar liveness — TCP probe the embedding/rerank llama-servers.
    let services = collect_services().await;

    // GPU offload is "active" when we have a live Jetson GPU sample showing
    // real engine utilization (the anti-silent-CPU-fallback signal).
    let gpu = gpu.map(|mut g| {
        g.offload_active = g.usage_pct.map(|u| u > 0.0).unwrap_or(false)
            || g.power_mw.map(|p| p > 0).unwrap_or(false);
        g
    });

    let telemetry = Telemetry {
        host: host_metrics.host,
        cpu: host_metrics.cpu,
        memory: host_metrics.memory,
        gpu,
        thermal: host_metrics.thermal,
        disks: host_metrics.disks,
        network: host_metrics.network,
        inference,
        devices,
        services,
        pool: PoolInfo {
            size: state.db.pool().size(),
            idle: state.db.pool().num_idle(),
        },
        processes: host_metrics.processes,
    };

    (StatusCode::OK, Json(telemetry)).into_response()
}

#[derive(Debug, Deserialize)]
pub struct TelemetryParams {
    /// `1` to include the top-processes table. Omitted by the default poll.
    pub processes: Option<u8>,
}

// ─── Host collection (sysinfo, blocking) ────────────────────────────────────

#[derive(Default)]
struct HostMetrics {
    host: HostInfo,
    cpu: CpuInfo,
    memory: MemoryInfo,
    thermal: Vec<ThermalSensor>,
    disks: Vec<DiskInfo>,
    network: NetworkInfo,
    processes: Vec<ProcessInfo>,
}

impl Default for HostInfo {
    fn default() -> Self {
        Self {
            hostname: None,
            os: None,
            kernel: None,
            arch: std::env::consts::ARCH.to_string(),
            uptime_secs: 0,
        }
    }
}

impl Default for CpuInfo {
    fn default() -> Self {
        Self {
            brand: String::new(),
            physical_cores: None,
            logical_cores: 0,
            usage_pct: 0.0,
            per_core: Vec::new(),
            frequency_mhz: 0,
            load_avg: LoadAvg {
                one: 0.0,
                five: 0.0,
                fifteen: 0.0,
            },
        }
    }
}

impl Default for MemoryInfo {
    fn default() -> Self {
        Self {
            total: 0,
            used: 0,
            available: 0,
            swap_total: 0,
            swap_used: 0,
        }
    }
}

impl Default for NetworkInfo {
    fn default() -> Self {
        Self {
            rx_per_sec: 0,
            tx_per_sec: 0,
            interfaces: Vec::new(),
        }
    }
}

/// The sysinfo refresh window — long enough for meaningful CPU% and net rates,
/// short enough to keep the request snappy.
const SAMPLE_WINDOW: Duration = Duration::from_millis(320);

fn collect_host(with_processes: bool) -> HostMetrics {
    use sysinfo::{
        Components, CpuRefreshKind, Disks, MemoryRefreshKind, Networks, ProcessRefreshKind,
        ProcessesToUpdate, RefreshKind, System,
    };

    let mut sys = System::new_with_specifics(
        RefreshKind::nothing()
            .with_cpu(CpuRefreshKind::everything())
            .with_memory(MemoryRefreshKind::everything()),
    );
    let mut networks = Networks::new_with_refreshed_list();
    let proc_kind = ProcessRefreshKind::nothing().with_cpu().with_memory();

    // Two samples around SAMPLE_WINDOW: first establishes a baseline, the
    // second yields CPU utilization and per-interface byte deltas. When asked,
    // processes are refreshed at the same two points so their CPU% is a real
    // delta over the window (no extra sleep).
    sys.refresh_cpu_all();
    if with_processes {
        sys.refresh_processes_specifics(ProcessesToUpdate::All, true, proc_kind);
    }
    std::thread::sleep(SAMPLE_WINDOW);
    sys.refresh_cpu_all();
    sys.refresh_memory();
    networks.refresh(false);
    if with_processes {
        sys.refresh_processes_specifics(ProcessesToUpdate::All, true, proc_kind);
    }

    let cpus = sys.cpus();
    let per_core: Vec<f32> = cpus.iter().map(|c| c.cpu_usage()).collect();
    let brand = cpus.first().map(|c| c.brand().trim().to_string()).unwrap_or_default();
    let frequency_mhz = cpus.first().map(|c| c.frequency()).unwrap_or(0);
    let load = System::load_average();

    let cpu = CpuInfo {
        brand,
        physical_cores: sys.physical_core_count(),
        logical_cores: cpus.len(),
        usage_pct: sys.global_cpu_usage(),
        per_core,
        frequency_mhz,
        load_avg: LoadAvg {
            one: load.one,
            five: load.five,
            fifteen: load.fifteen,
        },
    };

    let memory = MemoryInfo {
        total: sys.total_memory(),
        used: sys.used_memory(),
        available: sys.available_memory(),
        swap_total: sys.total_swap(),
        swap_used: sys.used_swap(),
    };

    let host = HostInfo {
        hostname: System::host_name(),
        os: System::long_os_version().or_else(System::name),
        kernel: System::kernel_version(),
        arch: System::cpu_arch(),
        uptime_secs: System::uptime(),
    };

    // Thermals — keep only sensors that report a plausible reading.
    let components = Components::new_with_refreshed_list();
    let mut thermal: Vec<ThermalSensor> = components
        .iter()
        .filter_map(|c| {
            c.temperature().filter(|t| t.is_finite() && *t > 0.0).map(|t| ThermalSensor {
                label: c.label().to_string(),
                temp_c: t,
            })
        })
        .collect();
    thermal.sort_by(|a, b| b.temp_c.partial_cmp(&a.temp_c).unwrap_or(std::cmp::Ordering::Equal));

    // Disks — physical mounts only; skip pseudo/duplicate entries.
    let disks_src = Disks::new_with_refreshed_list();
    let mut seen = std::collections::HashSet::new();
    let disks: Vec<DiskInfo> = disks_src
        .iter()
        .filter(|d| d.total_space() > 0)
        .filter_map(|d| {
            let mount = d.mount_point().to_string_lossy().to_string();
            // Skip the noisy virtual mounts; keep real volumes.
            if mount.starts_with("/dev")
                || mount.starts_with("/sys")
                || mount.starts_with("/proc")
                || mount.starts_with("/run")
            {
                return None;
            }
            if !seen.insert(mount.clone()) {
                return None;
            }
            Some(DiskInfo {
                name: d.name().to_string_lossy().to_string(),
                mount,
                fs: d.file_system().to_string_lossy().to_string(),
                total: d.total_space(),
                available: d.available_space(),
                removable: d.is_removable(),
            })
        })
        .collect();

    let secs = SAMPLE_WINDOW.as_secs_f64().max(0.001);
    let mut rx_per_sec = 0u64;
    let mut tx_per_sec = 0u64;
    let mut interfaces: Vec<InterfaceInfo> = networks
        .iter()
        .map(|(name, data)| {
            rx_per_sec += (data.received() as f64 / secs) as u64;
            tx_per_sec += (data.transmitted() as f64 / secs) as u64;
            InterfaceInfo {
                name: name.to_string(),
                rx_total: data.total_received(),
                tx_total: data.total_transmitted(),
            }
        })
        .collect();
    interfaces.sort_by(|a, b| (b.rx_total + b.tx_total).cmp(&(a.rx_total + a.tx_total)));

    // Top processes by memory (the stable, accurate column). CPU% is best-effort
    // over the sample window. We return a bounded top-N rather than the full
    // list — the view says "top 16 by memory", no silent truncation.
    const TOP_N: usize = 16;
    let processes: Vec<ProcessInfo> = if with_processes {
        let mut all: Vec<ProcessInfo> = sys
            .processes()
            .values()
            .map(|p| ProcessInfo {
                pid: p.pid().as_u32(),
                name: p.name().to_string_lossy().to_string(),
                cpu_pct: p.cpu_usage(),
                mem: p.memory(),
            })
            .collect();
        all.sort_by(|a, b| b.mem.cmp(&a.mem));
        all.truncate(TOP_N);
        all
    } else {
        Vec::new()
    };

    HostMetrics {
        host,
        cpu,
        memory,
        thermal,
        disks,
        network: NetworkInfo {
            rx_per_sec,
            tx_per_sec,
            interfaces,
        },
        processes,
    }
}

// ─── Jetson GPU via a background tegrastats streamer ─────────────────────────
//
// tegrastats streams samples; running it once and reading its lines is how it's
// meant to be used. We keep a single long-lived child and cache the latest
// parsed sample, so the request path never spawns a process. The streamer is
// idle-gated: when nobody has read telemetry for `GPU_IDLE_AFTER`, the child is
// killed and the GPU costs nothing at rest; the next read wakes it back up.

struct GpuMonitor {
    sample: RwLock<Option<GpuInfo>>,
    /// When telemetry was last read. The streamer runs only while this is recent.
    last_access: Mutex<Instant>,
}

static GPU_MONITOR: OnceLock<GpuMonitor> = OnceLock::new();

/// Keep tegrastats streaming for this long after the last read. Longer than the
/// UI poll interval so an open view stays warm; once the view closes, the child
/// is killed within a couple of seconds.
const GPU_IDLE_AFTER: Duration = Duration::from_secs(8);

fn tegrastats_path() -> Option<&'static str> {
    ["/usr/bin/tegrastats", "/usr/local/bin/tegrastats"]
        .into_iter()
        .find(|p| std::path::Path::new(p).exists())
}

impl GpuMonitor {
    fn idle(&self) -> bool {
        self.last_access
            .lock()
            .map(|t| t.elapsed() > GPU_IDLE_AFTER)
            .unwrap_or(true)
    }
}

/// Start the Jetson GPU monitor. Call once at server boot. On a non-Tegra host
/// it initialises the cell but spawns no task — reads then return `None`.
pub fn start_gpu_monitor() {
    let mon = GPU_MONITOR.get_or_init(|| GpuMonitor {
        sample: RwLock::new(None),
        last_access: Mutex::new(Instant::now()),
    });
    if tegrastats_path().is_none() {
        return; // not a Jetson
    }
    tokio::spawn(run_gpu_monitor(mon));
    tracing::info!("Jetson GPU telemetry monitor started");
}

/// The streamer loop: idle-gate → spawn tegrastats → cache each parsed line →
/// kill on idle/stall → repeat. A tegrastats failure never touches the request
/// path; the worst case is a `None` GPU sample.
async fn run_gpu_monitor(mon: &'static GpuMonitor) {
    use tokio::io::{AsyncBufReadExt, BufReader};
    use tokio::process::Command;

    let bin = match tegrastats_path() {
        Some(b) => b,
        None => return,
    };

    loop {
        // Idle gate — while nobody is watching, run nothing at all.
        while mon.idle() {
            tokio::time::sleep(Duration::from_secs(2)).await;
        }

        let mut child = match Command::new(bin)
            .arg("--interval")
            .arg("1000")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(error = %e, "tegrastats spawn failed");
                tokio::time::sleep(Duration::from_secs(5)).await;
                continue;
            }
        };

        let stdout = match child.stdout.take() {
            Some(s) => s,
            None => {
                let _ = child.start_kill();
                continue;
            }
        };
        let mut lines = BufReader::new(stdout).lines();

        // Stream until the view goes idle, or tegrastats stalls/exits.
        while !mon.idle() {
            match tokio::time::timeout(Duration::from_secs(5), lines.next_line()).await {
                Ok(Ok(Some(line))) => {
                    if let Ok(mut w) = mon.sample.write() {
                        *w = Some(parse_tegrastats(&line));
                    }
                }
                _ => break, // EOF / error / stall — respawn on the next loop turn
            }
        }
        let _ = child.start_kill();
    }
}

/// Read the latest cached Jetson GPU sample. O(1) — no process spawn. Bumps the
/// access clock so the streamer stays warm. On a Jetson that is still warming
/// up (no sample yet) it returns a field-less placeholder so the view shows a
/// GPU card ("warming") rather than "no GPU"; on non-Tegra hosts it's `None`.
fn read_gpu() -> Option<GpuInfo> {
    let mon = GPU_MONITOR.get()?;
    if let Ok(mut t) = mon.last_access.lock() {
        *t = Instant::now();
    }
    if let Some(sample) = mon.sample.read().ok().and_then(|s| s.clone()) {
        return Some(sample);
    }
    if tegrastats_path().is_some() {
        return Some(GpuInfo {
            kind: "jetson".to_string(),
            name: Some("Tegra integrated GPU".to_string()),
            usage_pct: None,
            mem_used: None,
            mem_total: None,
            temp_c: None,
            power_mw: None,
            offload_active: false,
        });
    }
    None
}

/// Parse a tegrastats line into a GpuInfo. Tolerant of field ordering and
/// missing fields across JetPack versions. Example fragments:
///   `RAM 3194/7852MB ... GR3D_FREQ 45% ... gpu@43C ... VDD_GPU_SOC 1843mW/...`
fn parse_tegrastats(line: &str) -> GpuInfo {
    let usage_pct = capture_after(line, "GR3D_FREQ")
        .and_then(|s| s.trim_start().split('%').next().map(str::to_string))
        .and_then(|s| s.trim().parse::<f32>().ok());

    // RAM <used>/<total>MB
    let (mem_used, mem_total) = match capture_after(line, "RAM ") {
        Some(rest) => {
            let frag: String = rest.chars().take_while(|c| *c != ' ').collect();
            let frag = frag.trim_end_matches("MB");
            let mut parts = frag.split('/');
            let used = parts
                .next()
                .and_then(|s| s.trim().parse::<u64>().ok())
                .map(|mb| mb * 1024 * 1024);
            let total = parts
                .next()
                .and_then(|s| s.trim().parse::<u64>().ok())
                .map(|mb| mb * 1024 * 1024);
            (used, total)
        }
        None => (None, None),
    };

    let temp_c = capture_after(line, "gpu@")
        .and_then(|s| s.split('C').next().map(str::to_string))
        .and_then(|s| s.trim().parse::<f32>().ok());

    // Power: VDD_GPU_SOC <inst>mW or GPU <inst>mW
    let power_mw = capture_after(line, "VDD_GPU_SOC ")
        .or_else(|| capture_after(line, "GPU "))
        .and_then(|rest| {
            let frag: String = rest.chars().take_while(|c| *c != ' ' && *c != '/').collect();
            frag.trim_end_matches("mW").trim().parse::<u64>().ok()
        });

    GpuInfo {
        kind: "jetson".to_string(),
        name: Some("Tegra integrated GPU".to_string()),
        usage_pct,
        mem_used,
        mem_total,
        temp_c,
        power_mw,
        offload_active: false, // set by caller
    }
}

/// Return the substring immediately following `needle`, or None.
fn capture_after<'a>(haystack: &'a str, needle: &str) -> Option<&'a str> {
    haystack.find(needle).map(|i| &haystack[i + needle.len()..])
}

// ─── Sidecar liveness ───────────────────────────────────────────────────────

/// TCP-probe the two llama-server sidecars (embed :18181, rerank :18182). A
/// successful connect within the timeout means the sidecar is listening.
async fn collect_services() -> Vec<ServiceInfo> {
    const PROBES: &[(&str, &str)] = &[
        ("embedding", "127.0.0.1:18181"),
        ("rerank", "127.0.0.1:18182"),
    ];
    let mut out = Vec::with_capacity(PROBES.len());
    for (name, addr) in PROBES {
        let up = tokio::time::timeout(
            Duration::from_millis(250),
            tokio::net::TcpStream::connect(*addr),
        )
        .await
        .map(|r| r.is_ok())
        .unwrap_or(false);
        out.push(ServiceInfo {
            name: name.to_string(),
            endpoint: addr.to_string(),
            up,
        });
    }
    out
}

// ─── Persisted time-series (app_system_samples) ───────────────────────────────

/// Read the latest cached GPU sample WITHOUT bumping the monitor's access clock.
/// Used by the background sampler so periodic sampling doesn't keep tegrastats
/// streaming 24/7 — when nobody is watching the live view, the GPU columns are
/// simply null for that minute. (`read_gpu` bumps; this one peeks.)
fn peek_gpu() -> Option<GpuInfo> {
    let mon = GPU_MONITOR.get()?;
    mon.sample.read().ok().and_then(|s| s.clone())
}

/// How often the background sampler writes a row. One minute keeps the series
/// readable for a long time on a small disk without losing meaningful detail.
const SAMPLE_INTERVAL: Duration = Duration::from_secs(60);

/// Start the background system sampler. Call once at boot. Writes one
/// `app_system_samples` row per minute (CPU/mem/GPU/net/disk/temp + sidecar
/// liveness) so the Telemetry tab can render history across restarts. Every
/// step is best-effort: a sample failure logs and the loop continues.
pub fn start_system_sampler(pool: sqlx::PgPool) {
    tokio::spawn(async move {
        // Small startup delay so the first sample lands after the sidecars and
        // network counters have settled.
        tokio::time::sleep(Duration::from_secs(10)).await;
        loop {
            if let Err(e) = sample_once(&pool).await {
                tracing::warn!(error = %e, "system sample failed");
            }
            tokio::time::sleep(SAMPLE_INTERVAL).await;
        }
    });
    tracing::info!("system telemetry sampler started (1/min → app_system_samples)");
}

async fn sample_once(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    let host = tokio::task::spawn_blocking(|| collect_host(false))
        .await
        .unwrap_or_else(|_| HostMetrics::default());
    let gpu = peek_gpu();
    let services = collect_services().await;

    // Pick the most relevant disk: the root mount if present, else the largest.
    let disk = host
        .disks
        .iter()
        .find(|d| d.mount == "/")
        .or_else(|| host.disks.iter().max_by_key(|d| d.total));
    let (disk_total, disk_used) = disk
        .map(|d| (d.total as i64, (d.total.saturating_sub(d.available)) as i64))
        .unwrap_or((0, 0));

    // Temperature headline: hottest thermal sensor, falling back to GPU temp.
    let temp_c = host
        .thermal
        .iter()
        .map(|t| t.temp_c)
        .fold(None, |acc: Option<f32>, t| Some(acc.map_or(t, |a| a.max(t))))
        .or_else(|| gpu.as_ref().and_then(|g| g.temp_c));

    let gpu_pct = gpu.as_ref().and_then(|g| g.usage_pct);
    let gpu_offload = gpu.as_ref().map(|g| g.offload_active);
    let embed_up = services.iter().find(|s| s.name == "embedding").map(|s| s.up);
    let rerank_up = services.iter().find(|s| s.name == "rerank").map(|s| s.up);

    sqlx::query(
        r#"
        INSERT INTO app_system_samples
            (cpu_pct, mem_used_bytes, mem_total_bytes, gpu_pct, gpu_offload_active,
             net_rx_bps, net_tx_bps, disk_used_bytes, disk_total_bytes,
             temp_c, load1, sidecar_embed_up, sidecar_rerank_up)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
        "#,
    )
    .bind(host.cpu.usage_pct)
    .bind(host.memory.used as i64)
    .bind(host.memory.total as i64)
    .bind(gpu_pct)
    .bind(gpu_offload)
    .bind(host.network.rx_per_sec as i64)
    .bind(host.network.tx_per_sec as i64)
    .bind(disk_used)
    .bind(disk_total)
    .bind(temp_c)
    .bind(host.cpu.load_avg.one)
    .bind(embed_up)
    .bind(rerank_up)
    .execute(pool)
    .await?;
    Ok(())
}

/// One persisted sample row, as returned by the history endpoint.
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct SystemSample {
    pub sampled_at: chrono::DateTime<chrono::Utc>,
    pub cpu_pct: Option<f32>,
    pub mem_used_bytes: Option<i64>,
    pub mem_total_bytes: Option<i64>,
    pub gpu_pct: Option<f32>,
    pub gpu_offload_active: Option<bool>,
    pub net_rx_bps: Option<i64>,
    pub net_tx_bps: Option<i64>,
    pub disk_used_bytes: Option<i64>,
    pub disk_total_bytes: Option<i64>,
    pub temp_c: Option<f32>,
    pub load1: Option<f64>,
    pub sidecar_embed_up: Option<bool>,
    pub sidecar_rerank_up: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct HistoryParams {
    /// Look-back window in seconds (default 24h, capped at 30 days).
    pub since_secs: Option<i64>,
}

/// `GET /api/system/history` — persisted system samples for the Telemetry tab.
///
/// Returns rows newest-last over the requested window so the front end can plot
/// them directly. Box-local; no egress.
pub async fn history_handler(
    _user: crate::middleware::auth::AuthUser,
    Query(params): Query<HistoryParams>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let window = params.since_secs.unwrap_or(86_400).clamp(60, 2_592_000);
    let rows = sqlx::query_as::<_, SystemSample>(
        r#"
        SELECT sampled_at, cpu_pct, mem_used_bytes, mem_total_bytes, gpu_pct,
               gpu_offload_active, net_rx_bps, net_tx_bps, disk_used_bytes,
               disk_total_bytes, temp_c, load1, sidecar_embed_up, sidecar_rerank_up
        FROM app_system_samples
        WHERE sampled_at >= now() - make_interval(secs => $1::double precision)
        ORDER BY sampled_at ASC
        "#,
    )
    .bind(window as f64)
    .fetch_all(state.db.pool())
    .await;

    match rows {
        Ok(samples) => (StatusCode::OK, Json(samples)).into_response(),
        Err(e) => {
            tracing::warn!(error = %e, "system history query failed");
            (StatusCode::OK, Json(Vec::<SystemSample>::new())).into_response()
        }
    }
}
