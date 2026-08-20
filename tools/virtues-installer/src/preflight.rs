//! Pre-flight checks — fail fast on environmental problems before we touch
//! apt/systemd/PG.
//!
//! Catches the failure modes that would otherwise produce an ugly half-
//! install: out of disk mid-GGUF-download, network blocked, port already
//! bound by another service. Mirrors the bash install.sh's preflight_checks
//! function, ported to typed Rust with one place per check.

use std::net::TcpStream;
use std::path::Path;
use std::time::Duration;

use anyhow::Result;
use reqwest::Client;

use crate::ui;

pub async fn run() -> Result<()> {
    // Disk space — the GGUFs are ~0.5 GB (embeddinggemma-300m Q8_0 ~0.3 GB +
    // gte-reranker-modernbert-base Q8_0 ~0.2 GB); PG18 adds another ~1 GB;
    // binaries + web + working room another GB. We want ≥ 4 GB free on /.
    match free_gb(Path::new("/")) {
        Some(gb) if gb >= 4 => ui::ok(&format!("Disk space ({gb} GB free on /)")),
        Some(gb) => {
            ui::warn(&format!("Disk space ({gb} GB free on / — recommend ≥ 4 GB)"));
        }
        None => {
            ui::warn("Disk space (could not query free space)");
        }
    }

    // Network reachability — probe the two hosts the install actually needs.
    // Report as a single "Internet reachable" line when both pass; call out
    // the specific failing host only when something is wrong.
    let http = Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("build http client");
    let mut failed_hosts: Vec<String> = Vec::new();
    for host in &["https://github.com", "https://apt.postgresql.org"] {
        match http.head(*host).send().await {
            Ok(r) if r.status().is_success() || r.status().is_redirection() => {}
            Ok(r) => {
                failed_hosts.push(format!("{host} (HTTP {})", r.status()));
            }
            Err(_) => {
                failed_hosts.push(host.to_string());
            }
        }
    }
    if failed_hosts.is_empty() {
        ui::ok("Internet reachable");
    } else {
        for h in &failed_hosts {
            ui::warn(&format!("Unreachable: {h}"));
        }
    }

    // Port conflicts. These three are the ones a working Virtues install
    // listens on; on a fresh box they should be free. If they're held,
    // either an old Virtues is still running (idempotent re-run, fine) or
    // an unrelated service is squatting on a port we need.
    for (port, name) in [
        (5432u16, "postgres"),
        (8000, "virtues"),
        (18181, "virtues-embed"),
        (18182, "virtues-rerank"),
    ] {
        if port_in_use(port) {
            ui::warn(&format!("Port {port} ({name}) in use — re-run on existing install?"));
        }
    }

    // Reachability class — will *remote* access work on this network? Purely
    // informational: it never blocks the install (local + LAN access always
    // work), it just tells the user up front instead of letting them discover a
    // walled network the hard way after pairing. Not treated as a warning
    // (a home box behind IPv4 NAT is normal, not an install problem).
    // Remote access is the blind relay: the box dials OUT over TCP/443, so
    // there is no inbound port to open and nothing about the local network
    // class (public IPv4, NAT, IPv6) gates it — the port-forward advice this
    // block used to print was left over from the WireGuard era and contradicted
    // the very next paragraph. Local and LAN access always work; remote
    // reachability is verified at runtime and reported by `virtues doctor`.
    ui::ok("Network: local and LAN access always work; remote reachability is verified at runtime (`virtues doctor`)");

    Ok(())
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
