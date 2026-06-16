//! macOS self-install — lay down `virtues-client` and its two launchd jobs.
//!
//! Mirrors the collector's self-install pattern
//! (`apps/mac-source/Sources/Commands/InstallCommand.swift`): copy our own
//! executable into place, generate a plist, `launchctl bootstrap`. The split:
//!
//! - **User LaunchAgent** (`com.virtues.client`) — runs the localhost reverse
//!   proxy (`up --no-tunnel`). No root needed; installed by [`run_user`] via
//!   `launchctl bootstrap gui/$UID`.
//! - **Root LaunchDaemon** (`com.virtues.daemon`) — runs the WireGuard tunnel
//!   + `.virtues` DNS, both of which require root. Installed by [`run_system`],
//!   which is invoked once via `osascript … with administrator privileges` from
//!   the Tauri app (the single password prompt).
//!
//! The static plists in `apps/desktop/macos/*.plist` are reference templates;
//! the authoritative plists are generated here so paths/upstream are derived at
//! install time rather than sed-substituted.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};

use crate::keychain;

const AGENT_LABEL: &str = "com.virtues.client";
const DAEMON_LABEL: &str = "com.virtues.daemon";
/// Root-stable location the LaunchDaemon runs the binary from.
const SYSTEM_BIN: &str = "/usr/local/bin/virtues-client";

// ─────────────────────────────────────────────────────────────────────────
// User-level: localhost proxy LaunchAgent (no root)
// ─────────────────────────────────────────────────────────────────────────

/// `virtues-client install [--upstream <host:port>]` — copy the binary to
/// `~/.virtues/bin` and install + load the LaunchAgent that runs the localhost
/// proxy. No root.
///
/// `upstream` is the box address the proxy forwards to. Normally this is the
/// address you paired against (Tailscale/LAN/SSH/IPv6) — the "direct" path,
/// which needs no WireGuard tunnel. When omitted, we fall back to the bundle's
/// WG-internal address (the legacy tunnel path, which needs the root daemon up).
pub fn run_user(upstream: Option<&str>) -> Result<()> {
    let home = home_dir()?;
    let bin_dir = home.join(".virtues").join("bin");
    std::fs::create_dir_all(&bin_dir).context("create ~/.virtues/bin")?;
    let install_path = bin_dir.join("virtues-client");
    copy_self_to(&install_path).context("install binary to ~/.virtues/bin")?;

    let upstream = match upstream {
        Some(u) => u.to_string(),
        None => {
            // Legacy fallback: the box's WG-internal address from the bundle,
            // reachable only once the root daemon's tunnel is up.
            let bundle = keychain::load_bundle()
                .context("read paired bundle from keychain")?
                .ok_or_else(|| {
                    anyhow::anyhow!("not paired — run pairing first, then install")
                })?;
            let ip = bundle
                .internal_ip
                .parse()
                .with_context(|| format!("parse internal_ip `{}`", bundle.internal_ip))?;
            // SocketAddr renders IPv6 with brackets, e.g. [fd00:5654::1]:8000.
            SocketAddr::new(ip, bundle.http_port).to_string()
        }
    };

    let logs_dir = home.join(".virtues").join("logs");
    std::fs::create_dir_all(&logs_dir).context("create ~/.virtues/logs")?;

    let agents_dir = home.join("Library").join("LaunchAgents");
    std::fs::create_dir_all(&agents_dir).context("create ~/Library/LaunchAgents")?;
    let plist_path = agents_dir.join(format!("{AGENT_LABEL}.plist"));
    let plist = agent_plist(&install_path, &upstream, &logs_dir);
    std::fs::write(&plist_path, plist)
        .with_context(|| format!("write {}", plist_path.display()))?;

    let domain = format!("gui/{}", current_uid()?);
    reload_launchd(&domain, &plist_path);

    eprintln!("✓ installed LaunchAgent {AGENT_LABEL} (proxy → {upstream})");
    Ok(())
}

/// `virtues-client uninstall` — user-level: bootout + remove the LaunchAgent and
/// the `~/.virtues/bin` binary. Idempotent, best-effort.
pub fn uninstall_user() -> Result<()> {
    let home = home_dir()?;
    let plist_path = home
        .join("Library")
        .join("LaunchAgents")
        .join(format!("{AGENT_LABEL}.plist"));
    let domain = format!("gui/{}", current_uid()?);
    let _ = launchctl(&["bootout", &domain, &plist_path.to_string_lossy()]);
    let _ = std::fs::remove_file(&plist_path);
    let _ = std::fs::remove_file(home.join(".virtues").join("bin").join("virtues-client"));
    eprintln!("✓ removed LaunchAgent {AGENT_LABEL}");
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────
// Root-level: WG + DNS LaunchDaemon (run via osascript admin)
// ─────────────────────────────────────────────────────────────────────────

/// `virtues-client install-system --user <name> --bundle <path>` — root-only.
/// Copy the binary to `/usr/local/bin` and install + load the LaunchDaemon that
/// runs the WG tunnel + `.virtues` DNS. Invoked once via osascript admin.
pub fn run_system(user: &str, bundle_path: &Path) -> Result<()> {
    if !is_root() {
        bail!("install-system must run as root (it writes /Library/LaunchDaemons)");
    }
    let bin = PathBuf::from(SYSTEM_BIN);
    if let Some(parent) = bin.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create {}", parent.display()))?;
    }
    copy_self_to(&bin).context("install binary to /usr/local/bin")?;

    let plist_path = PathBuf::from(format!("/Library/LaunchDaemons/{DAEMON_LABEL}.plist"));
    let plist = daemon_plist(&bin, bundle_path);
    std::fs::write(&plist_path, plist)
        .with_context(|| format!("write {} (need root)", plist_path.display()))?;
    // launchd requires LaunchDaemon plists to be root-owned, 0644.
    set_mode(&plist_path, 0o644);

    reload_launchd("system", &plist_path);

    // The daemon writes /etc/resolver/virtues on start; nudge mDNSResponder to
    // re-read resolvers so `.virtues` resolves without a reboot. Best-effort.
    flush_dns();

    let _ = user; // reserved for future per-user daemon config
    eprintln!("✓ installed LaunchDaemon {DAEMON_LABEL} (WG tunnel + .virtues DNS)");
    Ok(())
}

/// Root-level removal: bootout + remove the LaunchDaemon and `/usr/local/bin`
/// binary. Idempotent, best-effort. Invoked via osascript admin.
pub fn uninstall_system() -> Result<()> {
    if !is_root() {
        bail!("uninstall-system must run as root");
    }
    let plist_path = PathBuf::from(format!("/Library/LaunchDaemons/{DAEMON_LABEL}.plist"));
    let _ = launchctl(&["bootout", "system", &plist_path.to_string_lossy()]);
    let _ = std::fs::remove_file(&plist_path);
    let _ = std::fs::remove_file(SYSTEM_BIN);
    eprintln!("✓ removed LaunchDaemon {DAEMON_LABEL}");
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────
// plist generation
// ─────────────────────────────────────────────────────────────────────────

fn agent_plist(bin: &Path, upstream: &str, logs_dir: &Path) -> String {
    let bin = bin.display();
    let out = logs_dir.join("client.log");
    let err = logs_dir.join("client.error.log");
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{AGENT_LABEL}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{bin}</string>
        <string>up</string>
        <string>--no-tunnel</string>
        <string>--upstream</string>
        <string>{upstream}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>ProcessType</key>
    <string>Background</string>
    <key>StandardOutPath</key>
    <string>{out}</string>
    <key>StandardErrorPath</key>
    <string>{err}</string>
    <key>ThrottleInterval</key>
    <integer>10</integer>
</dict>
</plist>
"#,
        out = out.display(),
        err = err.display(),
    )
}

fn daemon_plist(bin: &Path, bundle_path: &Path) -> String {
    let bin = bin.display();
    let bundle = bundle_path.display();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{DAEMON_LABEL}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{bin}</string>
        <string>daemon</string>
        <string>--bundle-path</string>
        <string>{bundle}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardOutPath</key>
    <string>/var/log/virtues-daemon.log</string>
    <key>StandardErrorPath</key>
    <string>/var/log/virtues-daemon.log</string>
    <key>ThrottleInterval</key>
    <integer>30</integer>
</dict>
</plist>
"#
    )
}

// ─────────────────────────────────────────────────────────────────────────
// helpers
// ─────────────────────────────────────────────────────────────────────────

fn home_dir() -> Result<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| anyhow::anyhow!("$HOME not set"))
}

fn is_root() -> bool {
    // `id -u` == 0. Avoids pulling in libc just for getuid().
    matches!(
        Command::new("id").arg("-u").output(),
        Ok(o) if o.status.success() && String::from_utf8_lossy(&o.stdout).trim() == "0"
    )
}

fn current_uid() -> Result<String> {
    let out = Command::new("id")
        .arg("-u")
        .output()
        .context("run `id -u`")?;
    if !out.status.success() {
        bail!("`id -u` failed");
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Copy our own executable to `dest` (0755), unless we're already running from
/// `dest` (re-install). Removes any stale copy first so the replace is atomic-ish.
fn copy_self_to(dest: &Path) -> Result<()> {
    let current = std::env::current_exe().context("resolve current_exe")?;
    let current = current.canonicalize().unwrap_or(current);
    let dest_canon = dest.canonicalize().ok();
    if dest_canon.as_deref() == Some(current.as_path()) {
        return Ok(()); // already running from the install location
    }
    if dest.exists() {
        std::fs::remove_file(dest)
            .with_context(|| format!("remove existing {}", dest.display()))?;
    }
    std::fs::copy(&current, dest)
        .with_context(|| format!("copy {} -> {}", current.display(), dest.display()))?;
    set_mode(dest, 0o755);
    Ok(())
}

fn set_mode(path: &Path, mode: u32) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode));
    }
}

/// Bootout (ignore "not loaded") then bootstrap a plist into `domain`
/// (`gui/$UID` or `system`). Warnings only — a failed bootstrap is logged but
/// not fatal so the caller can surface a single combined status.
fn reload_launchd(domain: &str, plist: &Path) {
    let p = plist.to_string_lossy();
    let _ = launchctl(&["bootout", domain, &p]);
    match launchctl(&["bootstrap", domain, &p]) {
        Ok((true, _)) => {}
        Ok((false, msg)) if msg.contains("already") => {}
        Ok((false, msg)) => tracing::warn!("launchctl bootstrap {domain}: {}", msg.trim()),
        Err(e) => tracing::warn!("launchctl bootstrap {domain} failed to run: {e}"),
    }
}

/// Run `launchctl <args>`, returning (success, combined stdout+stderr).
fn launchctl(args: &[&str]) -> Result<(bool, String)> {
    let out = Command::new("/bin/launchctl")
        .args(args)
        .output()
        .with_context(|| format!("spawn launchctl {args:?}"))?;
    let mut s = String::from_utf8_lossy(&out.stdout).into_owned();
    s.push_str(&String::from_utf8_lossy(&out.stderr));
    Ok((out.status.success(), s))
}

fn flush_dns() {
    let _ = Command::new("dscacheutil").arg("-flushcache").status();
    let _ = Command::new("killall").args(["-HUP", "mDNSResponder"]).status();
}
