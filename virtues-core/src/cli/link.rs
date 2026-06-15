//! Output helpers for `virtues link` / `virtues init`.
//!
//! With the localhost-daemon trust model (see [[localhost-daemon-trust]] in
//! MEMORY.md), there are no certs to install on the client. The box exposes
//! plain HTTP on :8000 and the only browser that can reach it without a
//! daemon is the Jetson's own Chromium hitting `http://localhost:8000`. For
//! other devices we point at the Virtues client (v0.2 work).
//!
//! This module builds the URL list shown by `virtues link` / `virtues init`.
//!
//! It also auto-notices when this CLI invocation arrived over SSH (env vars,
//! falling back to `/proc` ancestry) and prints a concrete `ssh -L`
//! local-forward recipe. On client-isolated networks the printed LAN URL
//! never loads — but the SSH session the user is already typing in is a
//! proven transport to the box, so we fold it into the handoff and into the
//! `wait_for_pair` 90-second hint (docs/onboarding.md: "auto-enable nothing,
//! auto-notice everything").

use std::net::{IpAddr, SocketAddr, UdpSocket};

use crate::wireguard::INTERNAL_PORT;

/// Local port we suggest for the `ssh -L` forward when the laptop's own
/// `INTERNAL_PORT` (8000) is already taken: `10000 + INTERNAL_PORT`. High
/// enough to be unprivileged and rarely occupied, and visibly derived from
/// the real port so the printed pair of commands reads as one recipe.
const FALLBACK_FWD_PORT: u16 = 18000;

/// One reachable pair URL we print to the user. The label is a short tag
/// ("Local", "LAN") and the `url` is what they paste into a browser.
pub struct ReachableUrl {
    pub label: &'static str,
    pub url: String,
}

/// Discover the box's primary outbound-facing IP address. Bind a UDP socket
/// to a public address (no traffic is sent — `connect()` on UDP only sets
/// the route); the OS then assigns us the local address that would be used
/// for that route. This is the address a client laptop on the same LAN
/// would reach the box on.
///
/// Returns `None` if we can't determine an address (rare; e.g. no network
/// configured at all). The caller falls back to printing only the loopback URL.
pub fn primary_ip() -> Option<IpAddr> {
    // 198.51.100.1 is documented test address space (RFC 5737) — we won't
    // actually send anything, but using a TEST-NET address avoids any
    // possibility of confusion with real traffic.
    let target: SocketAddr = "198.51.100.1:1".parse().ok()?;
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect(target).ok()?;
    socket.local_addr().ok().map(|a| a.ip())
}

/// The box's mDNS name (`<hostname>.local`). The installer registers the box
/// with Avahi, so this resolves on the LAN — it's the name we lead with in
/// every cross-device handoff (onboarding doctrine: `virtues.local`, never
/// `localhost`, for anything meant to be opened on another device).
pub fn mdns_host() -> String {
    let host = std::fs::read_to_string("/proc/sys/kernel/hostname")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| {
            std::process::Command::new("hostname")
                .output()
                .ok()
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .filter(|s| !s.is_empty())
        })
        .unwrap_or_else(|| "virtues".to_string());
    format!("{host}.local")
}

/// Build the URLs `virtues link` / `virtues init` print for the user.
///
/// Order matters — it's a UX statement:
///   1. `Any device` — the mDNS name. The URL a human should actually use,
///      from a phone or laptop on the same network.
///   2. `(if .local fails)` — the raw LAN IP. mDNS is flaky on some clients
///      (notably Android) and filtered on some networks; the IP is the
///      universal fallback.
///   3. `This machine` — loopback, for a browser on the box itself (a W3C
///      Secure Context without TLS). Last because almost nobody runs a
///      browser on the box; the kiosk panel is the exception and doesn't
///      read this output.
pub fn reachable_pair_urls(token: &str, is_dev: bool, web_port: &str) -> Vec<ReachableUrl> {
    if is_dev {
        return vec![ReachableUrl {
            label: "Local",
            url: format!("http://localhost:{web_port}/pair#t={token}"),
        }];
    }
    let mut urls = vec![ReachableUrl {
        label: "Any device",
        url: format!("http://{}:{INTERNAL_PORT}/pair#t={token}", mdns_host()),
    }];
    if let Some(ip) = primary_ip() {
        let host = match ip {
            IpAddr::V4(v4) => v4.to_string(),
            IpAddr::V6(v6) => format!("[{v6}]"),
        };
        urls.push(ReachableUrl {
            label: "(if .local fails)",
            url: format!("http://{host}:{INTERNAL_PORT}/pair#t={token}"),
        });
    }
    urls.push(ReachableUrl {
        label: "This machine",
        url: format!("http://localhost:{INTERNAL_PORT}/pair#t={token}"),
    });
    urls
}

/// The host string for any handoff that must work without mDNS: the raw LAN
/// IP ([bracketed] for v6), falling back to the mDNS name only when no
/// address is discoverable at all. Shared by the QR URL and the SSH
/// local-forward recipe — both target a context (phone camera, foreign
/// laptop) where `.local` resolution can't be assumed.
pub fn forward_host() -> String {
    match primary_ip() {
        Some(IpAddr::V4(v4)) => v4.to_string(),
        Some(IpAddr::V6(v6)) => format!("[{v6}]"),
        None => mdns_host(),
    }
}

/// Pull the box-side IP out of an `SSH_CONNECTION` value
/// ("client_ip client_port server_ip server_port") — the third field is the
/// address the client connected *to*.
fn parse_ssh_server_ip(conn: &str) -> Option<String> {
    conn.split_whitespace()
        .nth(2)
        .map(str::to_string)
        .filter(|s| !s.is_empty())
}

/// The IP the SSH client actually reached this box on — *provably reachable*,
/// because the laptop is connected through it right now. Read from
/// `SSH_CONNECTION` (present in the login shell) or, since the
/// `sudo -u virtues` re-exec strips it, from the `VIRTUES_SSH_SERVER_IP` that
/// `maybe_reexec_as_service_user` threads across that boundary.
fn ssh_server_ip() -> Option<String> {
    std::env::var("SSH_CONNECTION")
        .ok()
        .as_deref()
        .and_then(parse_ssh_server_ip)
        .or_else(|| {
            std::env::var("VIRTUES_SSH_SERVER_IP")
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        })
}

/// The host for the `ssh -L … user@HOST` recipe — the address the laptop can
/// actually reach the box on, in priority order:
///   1. the IP the SSH client connected to (provably reachable),
///   2. a detected user-run overlay address (Tailscale et al.),
///   3. the LAN IP.
/// The LAN IP is **last** on purpose: on the very networks where the forward
/// matters (client-isolated wifi), it's the one address the laptop *can't*
/// reach — which is exactly the bug where the box printed an unusable command.
pub fn ssh_forward_host() -> String {
    if let Some(ip) = ssh_server_ip() {
        return ip;
    }
    if let Some(addr) = crate::net_check::compute_net_status().byo.and_then(|b| b.addr) {
        return addr.to_string();
    }
    forward_host()
}

/// The URL to encode in the handoff QR. Prefer the raw LAN IP — phones
/// (notably Android) fumble `.local` resolution, and the QR is precisely the
/// phone path (docs/onboarding.md: "LAN IP, not mDNS, inside the QR"). Falls
/// back to the mDNS name when no address is discoverable.
pub fn qr_pair_url(token: &str) -> String {
    format!("http://{}:{INTERNAL_PORT}/pair#t={token}", forward_host())
}

// ─── SSH session auto-notice ────────────────────────────────────────────────
// Headless DIY installs happen over SSH (`ssh box`, then `curl | sudo sh`,
// which execs `sudo -u virtues virtues init`). On client-isolated wifi the
// printed LAN URL never loads — but the SSH session itself is a working
// transport: `ssh -L 8000:localhost:8000 user@box` and the pair page opens at
// http://localhost:8000. We never install or enable sshd; we only notice a
// session that already exists and print the concrete forward command.

/// Evidence this CLI invocation arrived over SSH.
pub struct SshContext {
    /// Login user on the box for the printed `ssh user@host`; None when it
    /// can't be determined honestly (the command then omits `user@`).
    pub user: Option<String>,
}

/// Detect whether this process is running inside an SSH session.
///
/// Tier 1: `SSH_CONNECTION` / `SSH_CLIENT` / `SSH_TTY` in the environment.
/// sudo's `env_reset` strips all three — which is exactly why tier 2 exists:
/// the install chain is `curl | sudo sh → exec sudo -u virtues virtues init`,
/// two sudo hops away from the shell sshd spawned.
///
/// Tier 2 (Linux only): walk the parent-process chain through `/proc` looking
/// for an `sshd` ancestor. Known gaps, both deliberate:
///   - containers: the PPid chain dead-ends at the namespace's init (pid 1),
///     so the sshd on the host is invisible — we silently report "no SSH",
///     which is correct from inside the container.
///   - mosh: the server detaches from sshd after login, so there is no sshd
///     ancestor — not detected. mosh users forfeit the hint, nothing breaks.
pub fn ssh_context() -> Option<SshContext> {
    let env_says_ssh = ["SSH_CONNECTION", "SSH_CLIENT", "SSH_TTY"]
        .iter()
        .any(|v| std::env::var_os(v).is_some());

    #[cfg(target_os = "linux")]
    let detected = env_says_ssh || sshd_in_ancestry();
    #[cfg(not(target_os = "linux"))]
    let detected = env_says_ssh;

    if !detected {
        return None;
    }
    Some(SshContext { user: ssh_login_user() })
}

/// Walk the PPid chain from this process upward, matching each ancestor's
/// `/proc/<pid>/comm` against sshd. Stops at pid ≤ 1 (init / namespace
/// root), after 32 hops (paranoia guard against a corrupt chain), or on any
/// read error (ancestor exited mid-walk → the chain honestly ends → false).
#[cfg(target_os = "linux")]
fn sshd_in_ancestry() -> bool {
    let mut pid = std::process::id();
    for _ in 0..32 {
        if pid <= 1 {
            return false;
        }
        let Ok(comm) = std::fs::read_to_string(format!("/proc/{pid}/comm")) else {
            return false;
        };
        if comm_is_sshd(&comm) {
            return true;
        }
        let Ok(status) = std::fs::read_to_string(format!("/proc/{pid}/status")) else {
            return false;
        };
        let Some(ppid) = parse_ppid(&status) else {
            return false;
        };
        pid = ppid;
    }
    false
}

/// Extract `PPid:` from `/proc/<pid>/status` contents. Pure for tests.
#[cfg(any(target_os = "linux", test))]
fn parse_ppid(status: &str) -> Option<u32> {
    status
        .lines()
        .find_map(|line| line.strip_prefix("PPid:"))
        .and_then(|rest| rest.trim().parse().ok())
}

/// Does a `/proc/<pid>/comm` value name an sshd process? `starts_with`
/// (after trimming the trailing newline) rather than equality: OpenSSH ≥ 9.8
/// splits into `sshd-session` / `sshd-auth` per connection, and comm
/// truncates at 15 chars anyway. Plain `ssh` (a client) must NOT match.
#[cfg(any(target_os = "linux", test))]
fn comm_is_sshd(comm: &str) -> bool {
    comm.trim().starts_with("sshd")
}

/// Best-effort login user for the printed `ssh user@host`.
///
/// `SUDO_USER` is NOT the first choice: under the installer's nested sudo
/// (`curl | sudo sh → exec sudo -u virtues …`) the inner sudo sees the outer
/// one's root, so `SUDO_USER=root` — useless. The controlling tty's owner
/// survives both hops: sshd chowns the pty to the login user, and sudo
/// (without `use_pty`) leaves the ctty alone. Fallback chain: ctty owner →
/// `SUDO_USER` (only if set and ≠ "root") → None.
fn ssh_login_user() -> Option<String> {
    #[cfg(target_os = "linux")]
    if let Some(name) = ctty_owner_name() {
        return Some(name);
    }
    std::env::var("SUDO_USER")
        .ok()
        .filter(|u| !u.is_empty() && u != "root")
}

/// Resolve the controlling tty's owner to a username via /proc + /etc/passwd.
///
/// Returns None when there's no ctty, it isn't a pts, the pty is root-owned
/// (newer sudo's `use_pty` — Ubuntu 24.04 default — reallocates a fresh
/// root-owned pty, so uid 0 here tells us nothing about the login user), or
/// the uid has no passwd entry. The caller then falls back to `SUDO_USER`.
#[cfg(target_os = "linux")]
fn ctty_owner_name() -> Option<String> {
    use std::os::unix::fs::MetadataExt;
    let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
    let tty_nr = tty_nr_from_stat(&stat)?;
    if tty_nr == 0 {
        return None; // no controlling tty at all
    }
    // Decode the kernel's packed dev_t: major is bits 8-19, minor is bits
    // 0-7 plus the high bits parked at 20+.
    let major = (tty_nr >> 8) & 0xfff;
    let minor = (tty_nr & 0xff) | ((tty_nr >> 12) << 8);
    // Unix98 pty slaves own majors 136-143; each major carries 256 minors.
    if !(136..=143).contains(&major) {
        return None;
    }
    let pts = minor + (major - 136) * 256;
    let uid = std::fs::metadata(format!("/dev/pts/{pts}")).ok()?.uid();
    if uid == 0 {
        return None;
    }
    let passwd = std::fs::read_to_string("/etc/passwd").ok()?;
    passwd_name_for_uid(&passwd, uid)
}

/// Extract `tty_nr` (field 7) from `/proc/<pid>/stat` contents. Pure.
///
/// The comm field (field 2) is the process name in parens and may itself
/// contain spaces and parens — `a.out (deleted))` is legal — so the only
/// safe parse splits AFTER THE LAST `)`. Fields after comm are
/// `state ppid pgrp session tty_nr`, so tty_nr is whitespace token 4.
///
/// Deliberately a dumb extractor: tty_nr 0 ("no ctty") is returned as
/// `Some(0)` — interpreting 0 is the caller's job.
#[cfg(any(target_os = "linux", test))]
fn tty_nr_from_stat(stat: &str) -> Option<i32> {
    let (_, after_comm) = stat.rsplit_once(')')?;
    after_comm.split_whitespace().nth(4)?.parse().ok()
}

/// Find the username for a uid in /etc/passwd contents. Pure; malformed
/// lines are skipped rather than failing the whole lookup.
#[cfg(any(target_os = "linux", test))]
fn passwd_name_for_uid(passwd: &str, uid: u32) -> Option<String> {
    passwd.lines().find_map(|line| {
        // A passwd record is exactly 7 ':'-fields (name:pw:uid:gid:gecos:
        // home:shell) — anything shorter is garbage and must never yield a
        // username we'd print into an ssh command.
        let fields: Vec<&str> = line.split(':').collect();
        if fields.len() != 7 || fields[0].is_empty() {
            return None;
        }
        let line_uid: u32 = fields[2].trim().parse().ok()?;
        (line_uid == uid).then(|| fields[0].to_string())
    })
}

/// `user@host` for the printed ssh command — or bare `host` when the login
/// user couldn't be determined honestly. Shared by the init/login handoff
/// block and the `wait_for_pair` hint so the two never drift.
fn forward_target(ctx: &SshContext, host: &str) -> String {
    match &ctx.user {
        Some(user) => format!("{user}@{host}"),
        None => host.to_string(),
    }
}

/// The SSH local-forward recipe printed after the QR in the init/login
/// handoff. Pure — returns the lines so it's testable; the caller prints
/// them. Two-space indent matches `print_link_output`'s style.
pub fn ssh_handoff_block(ctx: &SshContext, host: &str, token: &str) -> Vec<String> {
    let target = forward_target(ctx, host);
    vec![
        "  On SSH — the app needs to reach your box on the same network.".to_string(),
        "  If your network blocks device traffic (office/hotel), forward the port:".to_string(),
        String::new(),
        format!("    ssh -L {INTERNAL_PORT}:localhost:{INTERNAL_PORT} {target}"),
        format!("    then open  http://localhost:{INTERNAL_PORT} in the app or browser"),
        String::new(),
        format!(
            "    (if port {INTERNAL_PORT} is busy: ssh -L {FALLBACK_FWD_PORT}:localhost:{INTERNAL_PORT} {target}"
        ),
        format!("     → http://localhost:{FALLBACK_FWD_PORT}/pair#t={token})"),
    ]
}

/// Outcome of waiting on a minted pair token.
pub enum PairWaitOutcome {
    /// The human opened the link — a device/session consumed the token.
    Consumed,
    /// The token expired (or was denied) before anyone arrived.
    Expired,
}

/// Block until the minted pair token is consumed or expires, polling the DB.
///
/// After ~90s of silence, print the client-isolation hint
/// (docs/onboarding.md "hostile networks"): the only reliable box-side signal
/// for a network that blocks device-to-device traffic is "the link was
/// printed and nobody arrived." Two branches: when we auto-noticed an SSH
/// session ([`ssh_context`]), lead with the concrete `ssh -L` forward — the
/// session the user is staring at works regardless of client isolation.
/// Otherwise the copy stays setup-scoped — hotspot or a network you control,
/// plus a self-excluding SSH pointer; no VPN/overlay talk at the moment of
/// max fragility.
pub async fn wait_for_pair(
    pool: &sqlx::PgPool,
    token_id: &str,
    token: &str,
) -> anyhow::Result<PairWaitOutcome> {
    const HINT_AFTER: std::time::Duration = std::time::Duration::from_secs(90);
    let start = std::time::Instant::now();
    let mut hinted = false;
    // Auto-notice once, up front — the answer can't change mid-wait, and the
    // hint must print instantly when the 90s mark hits.
    let ssh = ssh_context();
    let host = ssh_forward_host();
    loop {
        let row: Option<(String, chrono::DateTime<chrono::Utc>)> = sqlx::query_as(
            "SELECT status, expires_at FROM app_pair_token WHERE id = $1",
        )
        .bind(token_id)
        .fetch_optional(pool)
        .await?;
        let Some((status, expires_at)) = row else {
            return Ok(PairWaitOutcome::Expired);
        };
        match status.as_str() {
            "consumed" => return Ok(PairWaitOutcome::Consumed),
            "expired" | "denied" => return Ok(PairWaitOutcome::Expired),
            _ => {}
        }
        if chrono::Utc::now() > expires_at {
            return Ok(PairWaitOutcome::Expired);
        }
        if !hinted && start.elapsed() >= HINT_AFTER {
            hinted = true;
            println!();
            match &ssh {
                Some(ctx) => {
                    let target = forward_target(ctx, &host);
                    println!("  Still waiting — this network may block device-to-device traffic");
                    println!("  (common in offices, hotels, WeWork). You're on SSH, so forward");
                    println!("  the port from your laptop and open it there:");
                    println!("    ssh -L {INTERNAL_PORT}:localhost:{INTERNAL_PORT} {target}");
                    println!("    then open http://localhost:{INTERNAL_PORT} in the app or browser");
                }
                None => {
                    println!("  Still waiting — if the app or page won't load, this network may");
                    println!("  block device-to-device traffic (offices, hotels, WeWork).");
                    println!("  → Use your phone's hotspot, or a network you control.");
                    println!("  → Or SSH in: ssh -L {INTERNAL_PORT}:localhost:{INTERNAL_PORT} {host}");
                    println!("  You can move the box to a different network after setup.");
                }
            }
        }
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── parse_ppid ──────────────────────────────────────────────────────────

    #[test]
    fn parse_ppid_tab_separated() {
        let status = "Name:\tbash\nUmask:\t0022\nPPid:\t4242\nTracerPid:\t0\n";
        assert_eq!(parse_ppid(status), Some(4242));
    }

    #[test]
    fn parse_ppid_space_separated() {
        assert_eq!(parse_ppid("PPid:   17\n"), Some(17));
    }

    #[test]
    fn parse_ppid_missing_line() {
        // `Pid:` and `TracerPid:` must not match the `PPid:` prefix.
        let status = "Name:\tbash\nPid:\t9\nTracerPid:\t0\n";
        assert_eq!(parse_ppid(status), None);
    }

    // ── comm_is_sshd ────────────────────────────────────────────────────────

    #[test]
    fn comm_matches_sshd_family() {
        // /proc comm comes with a trailing newline.
        assert!(comm_is_sshd("sshd\n"));
        // OpenSSH ≥ 9.8 per-connection processes.
        assert!(comm_is_sshd("sshd-session\n"));
        assert!(comm_is_sshd("sshd-auth\n"));
    }

    #[test]
    fn comm_rejects_ssh_client_and_others() {
        assert!(!comm_is_sshd("ssh\n")); // the *client* is not a session proof
        assert!(!comm_is_sshd("bash\n"));
        assert!(!comm_is_sshd("\n"));
    }

    // ── tty_nr_from_stat ────────────────────────────────────────────────────

    #[test]
    fn tty_nr_normal_stat_line() {
        // 34816 = 0x8800 → major 136, minor 0 → /dev/pts/0
        let stat = "12345 (virtues) S 1 12345 12345 34816 12345 4194304 1000 0 0 0";
        assert_eq!(tty_nr_from_stat(stat), Some(34816));
    }

    #[test]
    fn tty_nr_survives_paren_bomb_comm() {
        // comm may contain spaces and parens; only splitting after the LAST
        // ')' parses this correctly.
        let stat = "999 (weird name)) S 1 2 3 34817 5 6";
        assert_eq!(tty_nr_from_stat(stat), Some(34817));
    }

    #[test]
    fn tty_nr_zero_is_some_zero() {
        // Dumb extractor by design: 0 ("no ctty") is the CALLER's call.
        let stat = "1 (init) S 0 1 1 0 -1 4194560";
        assert_eq!(tty_nr_from_stat(stat), Some(0));
    }

    #[test]
    fn tty_nr_malformed_stat_is_none() {
        assert_eq!(tty_nr_from_stat("garbage with no close paren"), None);
        assert_eq!(tty_nr_from_stat("1 (init) S 0 1"), None); // too few fields
    }

    // ── passwd_name_for_uid ─────────────────────────────────────────────────

    const PASSWD: &str = "root:x:0:0:root:/root:/bin/bash\n\
                          daemon:x:1:1:daemon:/usr/sbin:/usr/sbin/nologin\n\
                          adam:x:1000:1000:Adam:/home/adam:/bin/zsh\n";

    #[test]
    fn passwd_hit() {
        assert_eq!(passwd_name_for_uid(PASSWD, 1000), Some("adam".to_string()));
        assert_eq!(passwd_name_for_uid(PASSWD, 0), Some("root".to_string()));
    }

    #[test]
    fn passwd_miss() {
        assert_eq!(passwd_name_for_uid(PASSWD, 4321), None);
    }

    #[test]
    fn passwd_malformed_lines_are_skipped() {
        let mangled = "not a passwd line\n::\nadam:x:1000:1000::/home/adam:/bin/zsh\n";
        assert_eq!(passwd_name_for_uid(mangled, 1000), Some("adam".to_string()));
        assert_eq!(passwd_name_for_uid("uid only::1000", 1000), None);
    }

    // ── parse_ssh_server_ip ──────────────────────────────────────────────────

    #[test]
    fn ssh_server_ip_extraction() {
        // SSH_CONNECTION = "client_ip client_port server_ip server_port"
        assert_eq!(
            parse_ssh_server_ip("10.1.4.22 53412 10.0.0.5 22").as_deref(),
            Some("10.0.0.5")
        );
        // Reached over Tailscale → the overlay IP is the server field.
        assert_eq!(
            parse_ssh_server_ip("100.107.249.93 51000 100.104.55.76 22").as_deref(),
            Some("100.104.55.76")
        );
        // IPv6 server address, bare (ssh accepts `user@<bare v6>`).
        assert_eq!(
            parse_ssh_server_ip("2001:db8::1 51000 2001:db8::5 22").as_deref(),
            Some("2001:db8::5")
        );
        // Malformed / truncated → None, never a bogus target.
        assert_eq!(parse_ssh_server_ip("10.1.4.22 53412"), None);
        assert_eq!(parse_ssh_server_ip(""), None);
    }

    // ── forward_target ──────────────────────────────────────────────────────

    #[test]
    fn forward_target_with_and_without_user() {
        let with = SshContext { user: Some("adam".to_string()) };
        let without = SshContext { user: None };
        assert_eq!(forward_target(&with, "10.1.4.22"), "adam@10.1.4.22");
        assert_eq!(forward_target(&without, "10.1.4.22"), "10.1.4.22");
    }

    #[test]
    fn forward_target_ipv6_bracketed_host() {
        let ctx = SshContext { user: Some("adam".to_string()) };
        assert_eq!(
            forward_target(&ctx, "[2001:db8::42]"),
            "adam@[2001:db8::42]"
        );
    }

    // ── ssh_handoff_block ───────────────────────────────────────────────────

    #[test]
    fn handoff_block_with_user() {
        let ctx = SshContext { user: Some("adam".to_string()) };
        let lines = ssh_handoff_block(&ctx, "10.1.4.22", "tok123");
        let joined = lines.join("\n");
        assert!(lines[0].contains("On SSH"));
        assert!(joined.contains("ssh -L 8000:localhost:8000 adam@10.1.4.22"));
        assert!(joined.contains("http://localhost:8000"));
        // Fallback recipe: forward from 18000 on the laptop to 8000 on the box.
        assert!(joined.contains("ssh -L 18000:localhost:8000 adam@10.1.4.22"));
        assert!(joined.contains("http://localhost:18000/pair#t=tok123"));
    }

    #[test]
    fn handoff_block_without_user_omits_at() {
        let ctx = SshContext { user: None };
        let joined = ssh_handoff_block(&ctx, "10.1.4.22", "tok123").join("\n");
        assert!(joined.contains("ssh -L 8000:localhost:8000 10.1.4.22"));
        // The ssh command itself has no @, but the fallback URL still has #t=
        assert!(!joined.split("ssh").nth(1).unwrap_or("").contains('@'));
    }
}
