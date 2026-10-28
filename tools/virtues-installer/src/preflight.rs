//! Pre-flight checks — fail fast on environmental problems before we touch
//! apt/systemd/PG.
//!
//! Catches the failure modes that would otherwise produce an ugly half-
//! install: out of disk mid-GGUF-download, network blocked, port already
//! bound by another service. A check whose failure would stop the install
//! partway (no systemd, no Postgres 18 for this release) refuses here,
//! before anything on the machine changes; the rest warn.

use std::net::TcpStream;
use std::path::Path;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use reqwest::Client;
use tokio::process::Command;

use crate::steps::{run_step, PkgMgr, Target};
use crate::ui;

pub async fn run(target: &mut Target) -> Result<()> {
    // systemd. Every service the install creates is a systemd unit, so
    // without it (a container's own init, WSL with systemd off, OpenRC) the
    // packages install and the first `systemctl` call fails. This directory
    // exists exactly when systemd is PID 1 (sd_booted(3)).
    if !Path::new("/run/systemd/system").is_dir() {
        bail!(
            "This machine isn't running systemd, which Virtues needs to run its services. \
             Nothing was changed. Install on a machine that boots with systemd. On WSL, \
             set systemd=true under [boot] in /etc/wsl.conf and restart WSL."
        );
    }
    ui::ok("systemd");

    // Memory. 8 GB is what the requirements page asks for, and an 8 GB
    // machine reports about 7.6 GiB once the kernel and firmware take theirs,
    // hence the threshold. A warning only: some machines run below it.
    match mem_total_gib() {
        Some(gib) if gib >= 7.5 => ui::ok(&format!("Memory ({gib:.1} GiB)")),
        Some(gib) => ui::warn(&format!(
            "Memory ({gib:.1} GiB). Virtues needs 8 GB, and with less it can run out of \
             memory while it indexes. See virtues.com/docs/setup/requirements"
        )),
        None => ui::warn("Memory (couldn't read /proc/meminfo)"),
    }

    // Disk space — the GGUF is ~0.3 GB (embeddinggemma-2 Q8_0), and a model
    // change briefly holds two; PG18 adds another ~1 GB;
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

    if target.pkg_mgr == PkgMgr::Apt {
        target.pgdg = postgres_source(target, &http).await?;
    }

    // Port conflicts. These are the ones a working Virtues install
    // listens on; on a fresh box they should be free. If they're held,
    // either an old Virtues is still running (idempotent re-run, fine) or
    // an unrelated service is squatting on a port we need.
    for (port, name) in [
        (5432u16, "postgres"),
        (8000, "virtues"),
        (18181, "virtues-embed"),
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
    // Remote access goes through the relay: the box dials OUT over TCP/443, so
    // there is no inbound port to open and nothing about the local network
    // class (public IPv4, NAT, IPv6) gates it — the port-forward advice this
    // block used to print was left over from the WireGuard era and contradicted
    // the very next paragraph. Local and LAN access always work; remote
    // reachability is verified at runtime and reported by `virtues doctor`.
    ui::ok("Network: local and LAN access always work; remote reachability is verified at runtime (`virtues doctor`)");

    Ok(())
}

/// Where Postgres 18 comes from on an apt machine: the distro's own archive
/// when it carries both packages, else PGDG for this release's codename.
/// Returns the codename to add PGDG for, or `None` when the archive has them
/// (which includes a re-run, where PGDG is already in the lists). Refuses
/// when neither source has them, since `apt-get install` would otherwise stop
/// partway through the system packages.
async fn postgres_source(target: &Target, http: &Client) -> Result<Option<String>> {
    // Refresh first: a fresh cloud image ships with empty lists, which would
    // make every package look missing. Only the package index changes.
    let mut cmd = Command::new("apt-get");
    cmd.env("DEBIAN_FRONTEND", "noninteractive").args(["update", "-qq"]);
    run_step("apt index", cmd)
        .await
        .context("apt-get update failed, so packages can't be installed. Nothing else was changed")?;

    if apt_candidate("postgresql-18").await && apt_candidate("postgresql-18-pgvector").await {
        ui::ok("PostgreSQL 18 (from this release's archive)");
        return Ok(None);
    }

    let release = format!("{} {}", target.distro, target.distro_version);
    let Some(codename) = target.codename.as_deref() else {
        bail!(
            "{release} doesn't carry PostgreSQL 18, and /etc/os-release names no codename \
             to get it from apt.postgresql.org. Nothing was changed."
        );
    };
    let url = format!("https://apt.postgresql.org/pub/repos/apt/dists/{codename}-pgdg/Release");
    match http.head(&url).send().await {
        Ok(r) if r.status().is_success() => {
            ui::ok(&format!("PostgreSQL 18 (from apt.postgresql.org, {codename})"));
            Ok(Some(codename.to_string()))
        }
        Ok(r) => bail!(
            "{release} doesn't carry PostgreSQL 18, and apt.postgresql.org has no packages \
             for \"{codename}\" (HTTP {}). Nothing was changed. Install on a release listed \
             at virtues.com/docs/setup/requirements",
            r.status()
        ),
        Err(e) => bail!(
            "{release} doesn't carry PostgreSQL 18, and apt.postgresql.org couldn't be \
             reached to get it ({e}). Nothing was changed. Check this machine's internet \
             connection, then run the installer again."
        ),
    }
}

/// Does apt have an installable version of `pkg`? `apt-cache policy` prints
/// `Candidate: (none)` for a known package with nothing installable, and
/// nothing at all for an unknown one.
async fn apt_candidate(pkg: &str) -> bool {
    let Ok(out) = Command::new("apt-cache").args(["policy", pkg]).output().await else {
        return false;
    };
    String::from_utf8_lossy(&out.stdout).lines().any(|l| {
        l.trim()
            .strip_prefix("Candidate:")
            .is_some_and(|v| v.trim() != "(none)")
    })
}

/// Total memory in GiB, from /proc/meminfo's `MemTotal` (reported in KiB).
fn mem_total_gib() -> Option<f64> {
    let meminfo = std::fs::read_to_string("/proc/meminfo").ok()?;
    let kib: f64 = meminfo
        .lines()
        .find_map(|l| l.strip_prefix("MemTotal:"))?
        .split_whitespace()
        .next()?
        .parse()
        .ok()?;
    Some(kib / 1024.0 / 1024.0)
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
