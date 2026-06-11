//! Pre-flight checks — fail fast on environmental problems before we touch
//! apt/systemd/PG/Ollama.
//!
//! Catches the failure modes that would otherwise produce an ugly half-
//! install: out of disk mid-Ollama-pull, network blocked, port already
//! bound by another service. Mirrors the bash install.sh's preflight_checks
//! function, ported to typed Rust with one place per check.

use std::net::TcpStream;
use std::path::Path;
use std::time::Duration;

use anyhow::Result;
use reqwest::Client;

use crate::ui;

pub struct Report {
    pub warnings: u32,
}

pub async fn run() -> Result<Report> {
    let mut warnings = 0u32;

    // Disk space — bge-m3 alone is ~1.2 GB; PG18 + WireGuard + Ollama
    // daemon add another ~1 GB; binary + web + working room another GB.
    // We want ≥ 3 GB free on /.
    match free_gb(Path::new("/")) {
        Some(gb) if gb >= 3 => ui::ok(&format!("Disk space ({gb} GB free on /)")),
        Some(gb) => {
            ui::warn(&format!("Disk space ({gb} GB free on / — recommend ≥ 3 GB)"));
            warnings += 1;
        }
        None => {
            ui::warn("Disk space (could not query free space)");
            warnings += 1;
        }
    }

    // Network reachability — the three hosts the install genuinely needs.
    // 5s timeout per probe; total worst-case ~15s on a fully offline box.
    let http = Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("build http client");
    for host in &["https://github.com", "https://ollama.com", "https://apt.postgresql.org"] {
        match http.head(*host).send().await {
            Ok(r) if r.status().is_success() || r.status().is_redirection() => {
                ui::ok(&format!("Reachable: {host}"));
            }
            Ok(r) => {
                ui::warn(&format!("Reachable: {host} (HTTP {})", r.status()));
                warnings += 1;
            }
            Err(_) => {
                ui::warn(&format!("Unreachable: {host}"));
                warnings += 1;
            }
        }
    }

    // Port conflicts. These three are the ones a working Virtues install
    // listens on; on a fresh box they should be free. If they're held,
    // either an old Virtues is still running (idempotent re-run, fine) or
    // an unrelated service is squatting on a port we need.
    for (port, name) in [(5432u16, "postgres"), (8000, "virtues"), (11434, "ollama")] {
        if port_in_use(port) {
            ui::warn(&format!("Port {port} ({name}) in use — re-run on existing install?"));
            warnings += 1;
        }
    }

    Ok(Report { warnings })
}

/// Free disk space on the given mount, in GB. Returns None on stat error.
fn free_gb(path: &Path) -> Option<u64> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let c = CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    let rc = unsafe { libc::statvfs(c.as_ptr(), &mut stat) };
    if rc != 0 {
        return None;
    }
    let bytes = stat.f_bavail as u64 * stat.f_frsize as u64;
    Some(bytes / 1024 / 1024 / 1024)
}

fn port_in_use(port: u16) -> bool {
    TcpStream::connect_timeout(
        &format!("127.0.0.1:{port}").parse().unwrap(),
        Duration::from_millis(100),
    )
    .is_ok()
}
