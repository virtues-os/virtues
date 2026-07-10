//! Install / uninstall the `virtues-client` proxy as a durable background
//! service — the peer of the collector's LaunchAgent.
//!
//! ## Why a service (and not an app-session child)
//!
//! The proxy (`virtues-client up`) binds `localhost:7117` and tunnels to the
//! paired box over iroh. Two things depend on it being *always on*, independent
//! of whether the desktop app happens to be open:
//!
//!   1. The **collector** LaunchAgent uploads through `http://localhost:7117`.
//!      If the proxy only lived while the app was open, background collection
//!      couldn't reach the box whenever the app was closed.
//!   2. Re-opening the app should reconnect *silently* — a proxy that died with
//!      the last quit forces the connect screen every launch.
//!
//! So the proxy is modelled exactly like the collector: a per-user service that
//! `RunAtLoad`s at login, restarts on crash, and is copied into `~/.virtues/bin`
//! so the app's launch-time reconcile can swap it on update.
//!
//!   - **macOS**: a LaunchAgent at `~/Library/LaunchAgents/com.virtues.client.plist`.
//!   - **Linux**: a systemd *user* unit at `~/.config/systemd/user/virtues-client.service`.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// LaunchAgent / systemd label. Must match `reconcile_helpers` in the Tauri app.
pub const LABEL: &str = "com.virtues.client";

/// `~/.virtues`.
fn virtues_dir() -> Result<PathBuf> {
    let home = std::env::var("HOME").context("$HOME not set")?;
    Ok(PathBuf::from(home).join(".virtues"))
}

/// `~/.virtues/bin/virtues-client` — the installed, reconcile-managed binary the
/// service actually runs (never the app bundle, which can move or be replaced).
fn installed_bin() -> Result<PathBuf> {
    Ok(virtues_dir()?.join("bin").join("virtues-client"))
}

/// Copy this running binary into `~/.virtues/bin/virtues-client` (0755, atomic).
/// No-op if we're already running from that path (install run from the installed
/// copy). Returns the destination.
fn stage_binary() -> Result<PathBuf> {
    let dst = installed_bin()?;
    let src = std::env::current_exe().context("locate current executable")?;
    if src == dst {
        return Ok(dst);
    }
    let dir = dst.parent().context("bin dir has no parent")?;
    std::fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
    let tmp = dst.with_extension("new");
    std::fs::copy(&src, &tmp)
        .with_context(|| format!("copy {} -> {}", src.display(), tmp.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))
            .context("chmod 0755")?;
    }
    std::fs::rename(&tmp, &dst)
        .with_context(|| format!("rename {} -> {}", tmp.display(), dst.display()))?;
    Ok(dst)
}

/// Install (or reinstall) the proxy service and start it now.
pub fn install() -> Result<()> {
    let bin = stage_binary()?;
    // Ensure the log dir exists before launchd/systemd tries to write into it.
    std::fs::create_dir_all(virtues_dir()?.join("logs")).ok();
    install_platform(&bin)
}

/// Stop and remove the proxy service. Best-effort throughout: a partly-installed
/// or already-removed service must still leave a clean state.
pub fn uninstall() -> Result<()> {
    uninstall_platform();
    // Drop the staged binary so the app's reconcile no longer treats the service
    // as installed. The live process (if any) was already booted out above; on
    // unix removing an open file is fine.
    if let Ok(bin) = installed_bin() {
        let _ = std::fs::remove_file(bin);
    }
    Ok(())
}

// ============================================================================
// macOS — LaunchAgent
// ============================================================================

#[cfg(target_os = "macos")]
fn plist_path() -> Result<PathBuf> {
    let home = std::env::var("HOME").context("$HOME not set")?;
    Ok(PathBuf::from(home)
        .join("Library")
        .join("LaunchAgents")
        .join(format!("{LABEL}.plist")))
}

/// GUI-session launchd domain target, e.g. `gui/501`. Mirrors the app's
/// reconcile, which addresses the agent the same way.
#[cfg(target_os = "macos")]
fn gui_domain() -> Result<String> {
    let uid = std::process::Command::new("id")
        .arg("-u")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .context("resolve uid via `id -u`")?;
    Ok(format!("gui/{uid}"))
}

#[cfg(target_os = "macos")]
fn install_platform(bin: &Path) -> Result<()> {
    let home = std::env::var("HOME").context("$HOME not set")?;
    let logs = format!("{home}/.virtues/logs");
    let plist = plist_path()?;
    let contents = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{LABEL}</string>

    <key>ProgramArguments</key>
    <array>
        <string>{bin}</string>
        <string>up</string>
    </array>

    <key>RunAtLoad</key>
    <true/>

    <key>KeepAlive</key>
    <dict>
        <key>SuccessfulExit</key>
        <false/>
        <key>Crashed</key>
        <true/>
    </dict>

    <key>ProcessType</key>
    <string>Background</string>

    <key>StandardOutPath</key>
    <string>{logs}/client.log</string>

    <key>StandardErrorPath</key>
    <string>{logs}/client.error.log</string>

    <key>WorkingDirectory</key>
    <string>{home}</string>

    <key>ThrottleInterval</key>
    <integer>10</integer>
</dict>
</plist>
"#,
        bin = bin.display(),
    );

    let dir = plist.parent().context("plist has no parent dir")?;
    std::fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
    std::fs::write(&plist, contents).with_context(|| format!("write {}", plist.display()))?;

    // Reload cleanly: bootout any prior instance (ignore "not loaded"), then
    // bootstrap the fresh plist and kickstart it so it's serving immediately.
    let domain = gui_domain()?;
    let _ = std::process::Command::new("/bin/launchctl")
        .args(["bootout", &format!("{domain}/{LABEL}")])
        .output();
    let out = std::process::Command::new("/bin/launchctl")
        .args(["bootstrap", &domain, &plist.to_string_lossy()])
        .output()
        .context("launchctl bootstrap")?;
    if !out.status.success() {
        // Fall back to legacy `load -w` for older macOS where bootstrap is fussy.
        let _ = std::process::Command::new("/bin/launchctl")
            .args(["load", "-w", &plist.to_string_lossy()])
            .output();
    }
    let _ = std::process::Command::new("/bin/launchctl")
        .args(["kickstart", "-k", &format!("{domain}/{LABEL}")])
        .output();
    Ok(())
}

#[cfg(target_os = "macos")]
fn uninstall_platform() {
    if let Ok(domain) = gui_domain() {
        let _ = std::process::Command::new("/bin/launchctl")
            .args(["bootout", &format!("{domain}/{LABEL}")])
            .output();
    }
    if let Ok(plist) = plist_path() {
        // Legacy unload too, so an old `load`-installed agent is also cleared.
        let _ = std::process::Command::new("/bin/launchctl")
            .args(["unload", "-w", &plist.to_string_lossy()])
            .output();
        let _ = std::fs::remove_file(plist);
    }
}

// ============================================================================
// Linux — systemd user unit
// ============================================================================

#[cfg(target_os = "linux")]
fn unit_path() -> Result<PathBuf> {
    let home = std::env::var("HOME").context("$HOME not set")?;
    Ok(PathBuf::from(home)
        .join(".config")
        .join("systemd")
        .join("user")
        .join("virtues-client.service"))
}

#[cfg(target_os = "linux")]
fn install_platform(bin: &Path) -> Result<()> {
    let unit = unit_path()?;
    let contents = format!(
        r#"[Unit]
Description=Virtues desktop proxy — localhost:7117 over iroh
Documentation=https://virtues.com/docs
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
ExecStart={bin} up
Restart=on-failure
RestartSec=3s

[Install]
WantedBy=default.target
"#,
        bin = bin.display(),
    );
    let dir = unit.parent().context("unit has no parent dir")?;
    std::fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
    std::fs::write(&unit, contents).with_context(|| format!("write {}", unit.display()))?;

    let _ = std::process::Command::new("systemctl")
        .args(["--user", "daemon-reload"])
        .output();
    let _ = std::process::Command::new("systemctl")
        .args(["--user", "enable", "--now", "virtues-client.service"])
        .output();
    // Restart in case it was already enabled with a stale ExecStart.
    let _ = std::process::Command::new("systemctl")
        .args(["--user", "restart", "virtues-client.service"])
        .output();
    Ok(())
}

#[cfg(target_os = "linux")]
fn uninstall_platform() {
    let _ = std::process::Command::new("systemctl")
        .args(["--user", "disable", "--now", "virtues-client.service"])
        .output();
    if let Ok(unit) = unit_path() {
        let _ = std::fs::remove_file(unit);
    }
    let _ = std::process::Command::new("systemctl")
        .args(["--user", "daemon-reload"])
        .output();
}

// ============================================================================
// Other targets
// ============================================================================

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn install_platform(_bin: &Path) -> Result<()> {
    anyhow::bail!("virtues-client service install is only supported on macOS and Linux")
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn uninstall_platform() {}
