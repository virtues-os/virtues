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

pub struct Report {
    pub warnings: u32,
}

pub async fn run() -> Result<Report> {
    let mut warnings = 0u32;

    // Disk space — the GGUFs are ~1.8 GB (bge-m3 F16 ~1.2 GB + reranker
    // Q8_0 ~0.6 GB); PG18 + WireGuard add another ~1 GB; binaries + web +
    // working room another GB. We want ≥ 4 GB free on /.
    match free_gb(Path::new("/")) {
        Some(gb) if gb >= 4 => ui::ok(&format!("Disk space ({gb} GB free on /)")),
        Some(gb) => {
            ui::warn(&format!("Disk space ({gb} GB free on / — recommend ≥ 4 GB)"));
            warnings += 1;
        }
        None => {
            ui::warn("Disk space (could not query free space)");
            warnings += 1;
        }
    }

    // Network reachability — the two hosts the install genuinely needs
    // (binaries AND model GGUFs both come from GitHub releases now).
    // 5s timeout per probe; total worst-case ~10s on a fully offline box.
    let http = Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("build http client");
    for host in &["https://github.com", "https://apt.postgresql.org"] {
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
    for (port, name) in [
        (5432u16, "postgres"),
        (8000, "virtues"),
        (18181, "virtues-embed"),
        (18182, "virtues-rerank"),
    ] {
        if port_in_use(port) {
            ui::warn(&format!("Port {port} ({name}) in use — re-run on existing install?"));
            warnings += 1;
        }
    }

    // Reachability class — will *remote* access work on this network? Purely
    // informational: it never blocks the install (local + LAN access always
    // work), it just tells the user up front instead of letting them discover a
    // walled network the hard way after pairing. Does NOT count toward
    // `warnings` (a home box behind IPv4 NAT is normal, not an install problem).
    match egress_class() {
        EgressClass::Ipv6 => {
            ui::ok("Network: global IPv6 — direct remote access will work")
        }
        EgressClass::Ipv4Public => {
            ui::ok("Network: global IPv4 — direct remote access via a router port-forward")
        }
        EgressClass::Nat => ui::warn(
            "Network: behind NAT, no global IPv6 — local + LAN access work fine, but \
             remote-from-anywhere needs a router port-forward (home) or your own overlay \
             (dorm/office). See docs/byo-networking.md; run `virtues doctor` anytime.",
        ),
        EgressClass::Unknown => {}
    }

    Ok(Report { warnings })
}

/// The box's outbound reachability class, for the preflight verdict. Mirrors
/// `virtues-core`'s `net_check` (the installer can't depend on the box binary,
/// which isn't downloaded yet, so the global-routability logic is duplicated).
enum EgressClass {
    /// Has a globally-routable IPv6 source — the doctrine's direct path.
    Ipv6,
    /// Global IPv4 source, no global IPv6 (rare static home IP / a VPS).
    Ipv4Public,
    /// Private/CGNAT IPv4 source and no global IPv6 — behind NAT.
    Nat,
    /// No egress detected.
    Unknown,
}

fn egress_class() -> EgressClass {
    use std::net::{IpAddr, UdpSocket};

    let probe = |dest: &str, bind: &str| -> Option<IpAddr> {
        let s = UdpSocket::bind(bind).ok()?;
        s.connect(dest).ok()?;
        let ip = s.local_addr().ok()?.ip();
        if ip.is_loopback() || ip.is_unspecified() {
            None
        } else {
            Some(ip)
        }
    };

    // Global IPv6? (not loopback/unspecified/multicast/link-local/ULA)
    if let Some(IpAddr::V6(v)) = probe("[2606:4700:4700::1111]:53", "[::]:0") {
        let seg0 = v.segments()[0];
        let global = !v.is_loopback()
            && !v.is_unspecified()
            && !v.is_multicast()
            && (seg0 & 0xffc0) != 0xfe80
            && (seg0 & 0xfe00) != 0xfc00;
        if global {
            return EgressClass::Ipv6;
        }
    }

    // IPv4 — global vs NAT (private/CGNAT/link-local).
    match probe("1.1.1.1:53", "0.0.0.0:0") {
        Some(IpAddr::V4(v)) => {
            let o = v.octets();
            let cgnat = o[0] == 100 && (o[1] & 0xc0) == 0x40;
            let global = !v.is_loopback()
                && !v.is_unspecified()
                && !v.is_private()
                && !v.is_link_local()
                && !v.is_broadcast()
                && !v.is_multicast()
                && !cgnat;
            if global {
                EgressClass::Ipv4Public
            } else {
                EgressClass::Nat
            }
        }
        _ => EgressClass::Unknown,
    }
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
