#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent};
use tauri_plugin_shell::ShellExt;

/// Collector status returned from CLI
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CollectorStatus {
    pub running: bool,
    pub paused: bool,
    pub pending_events: i64,
    pub pending_messages: i64,
    pub last_sync: Option<String>,
    pub has_full_disk_access: bool,
    pub has_accessibility: bool,
}

/// A Virtues server discovered on the local network.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FoundServer {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub origin: String,
}

// ============================================================================
// Pairing state helpers
// ============================================================================

/// Check whether this machine is paired.
///
/// Tries the keychain first, then falls back to `~/.virtues/bundle.json`. The
/// fallback matters because macOS scopes keychain items to the creating binary:
/// the bundle was written by the `virtues-client` sidecar, so this app binary
/// often can't read that keychain entry. `pair` also writes the bundle to a
/// 0600 file readable by any of the user's processes — the reliable signal.
fn is_paired() -> bool {
    let keychain_ok = keyring::Entry::new("virtues-client", "default-box")
        .and_then(|e| e.get_password())
        .is_ok();
    if keychain_ok {
        return true;
    }
    dirs::home_dir()
        .map(|h| h.join(".virtues").join("bundle.json").exists())
        .unwrap_or(false)
}

/// Paired, but the WG private key file is gone — so the tunnel can't
/// authenticate and the fix is **re-pair**, not "retry the network." This is the
/// exact state that masqueraded as a generic "unreachable" and sent us chasing
/// network ghosts. File-based (no keychain, no shell-out): `~/.virtues/bundle.json`
/// present + `~/.virtues/wg-private.key` absent. The key file is the reliable
/// store the standalone tunnel reads (the macOS keychain silently no-ops for
/// it), so its absence is the truthful signal.
fn wg_key_missing() -> bool {
    dirs::home_dir()
        .map(|h| {
            let d = h.join(".virtues");
            d.join("bundle.json").exists() && !d.join("wg-private.key").exists()
        })
        .unwrap_or(false)
}

/// Ask the local proxy (`localhost:7117`) whether THIS device's pairing is still
/// valid with the box, by reading `/auth/session`. `is_paired()` only checks
/// that a bundle exists on disk — but after a box reinstall/revoke that bundle's
/// bearer is dead, and loading the box web with it dead-ends the user on the
/// box's `/pair` page with no way back. This lets the launch path send a
/// definitively-rejected device to the app's own `pair.html` instead.
///
/// `Some(true)` = authenticated; `Some(false)` = box rejected us (re-pair);
/// `None` = proxy unreachable (can't tell — it may still be starting up, so the
/// caller should NOT bounce a possibly-valid device on this).
///
/// std-only (no HTTP dep): a raw GET with short timeouts.
///
/// Per-attempt budget. Worst case for one attempt = connect + read timeout.
/// The retry COUNT is the caller's lever, not baked in: the launch path waits
/// out the proxy startup race with several attempts, while the connect-screen
/// poll passes 1 (its 4s `setInterval` IS the retry). Keep the total —
/// `attempts × (CONNECT + READ + RETRY_GAP)` — well under that poll interval so
/// a probe can never queue up behind itself.
const PROBE_CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(400);
const PROBE_READ_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(800);
const PROBE_RETRY_GAP: std::time::Duration = std::time::Duration::from_millis(300);

/// BLOCKING. Never call on the main/UI thread — use `probe_box_session()`
/// (the async wrapper) instead, which offloads this to the blocking pool. The
/// `_blocking` suffix is the warning label: a synchronous Tauri command running
/// this freezes the webview for up to `attempts × ~1.5s`.
fn probe_box_session_blocking(attempts: u8) -> Option<bool> {
    use std::io::{Read, Write};
    use std::net::TcpStream;

    let addr = "127.0.0.1:7117".parse().ok()?;
    for i in 0..attempts {
        if let Ok(mut s) = TcpStream::connect_timeout(&addr, PROBE_CONNECT_TIMEOUT) {
            let _ = s.set_read_timeout(Some(PROBE_READ_TIMEOUT));
            let req = "GET /auth/session HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n";
            if s.write_all(req.as_bytes()).is_ok() {
                let mut buf = String::new();
                let _ = s.read_to_string(&mut buf);
                if let Some(verdict) = classify_session_response(&buf) {
                    return Some(verdict);
                }
            }
        }
        // No gap after the final attempt — we're about to return.
        if i + 1 < attempts {
            std::thread::sleep(PROBE_RETRY_GAP);
        }
    }
    None
}

/// Async, main-thread-safe wrapper around [`probe_box_session_blocking`]. Use
/// this from Tauri commands; it runs the blocking probe on the blocking pool so
/// the webview's UI thread is never parked on a socket read.
async fn probe_box_session(attempts: u8) -> Option<bool> {
    tauri::async_runtime::spawn_blocking(move || probe_box_session_blocking(attempts))
        .await
        .unwrap_or(None)
}

/// Classify a raw `/auth/session` HTTP response into rejected (`Some(false)`),
/// authenticated (`Some(true)`), or indeterminate (`None`, retry).
///
/// Parses deliberately: a 401 status line is a definitive rejection, and the
/// `user` check runs ONLY against the body (after the header terminator) so
/// header text can never false-match. `/auth/session` returns a small known
/// shape — `{"user":null}` unauth, `{"user":{…}}` authed — so a body substring
/// test is sufficient without dragging in an HTTP/JSON dep.
fn classify_session_response(raw: &str) -> Option<bool> {
    // Status line first: an explicit 401 means the box rejected this device.
    if let Some(status_line) = raw.lines().next() {
        if status_line.contains(" 401") {
            return Some(false);
        }
    }
    // Body only — split on the header terminator so we never inspect headers.
    let body = raw.split("\r\n\r\n").nth(1)?;
    let body = body.split_whitespace().collect::<String>(); // drop chunk framing/whitespace
    if body.contains("\"user\":null") {
        return Some(false);
    }
    if body.contains("\"user\":{") {
        return Some(true);
    }
    None
}

/// Coarse internet check: can this Mac open a TCP connection to a public
/// anchor? std-only, short timeout. Distinguishes "this device is offline" from
/// "the box is unreachable."
fn device_online() -> bool {
    use std::net::{TcpStream, ToSocketAddrs};
    use std::time::Duration;
    // Well-known always-on anchors on :443/:53. Any success = online.
    for anchor in ["1.1.1.1:443", "8.8.8.8:53"] {
        if let Ok(addrs) = anchor.to_socket_addrs() {
            for addr in addrs {
                if TcpStream::connect_timeout(&addr, Duration::from_millis(700)).is_ok() {
                    return true;
                }
            }
        }
    }
    false
}

/// Diagnose why a paired box can't be reached, for the connect screen's
/// network-aware callout. Returns one of:
///   - `"ok"`            — recovered; the caller should load the box.
///   - `"stale_bearer"`  — box reachable but rejected this device → re-pair.
///   - `"device_offline"`— this Mac has no usable internet.
///   - `"box_unreachable"`— box off/asleep OR this network blocks
///     device-to-device traffic (work/café Wi-Fi). Distinguishing the two
///     precisely needs a reachability probe (future, via virtues-client); the
///     callout copy covers both honestly for now.
#[tauri::command]
async fn diagnose_box() -> String {
    // A few attempts here: this backs an explicit diagnosis, so it's worth
    // riding out a transient blip rather than reporting "unreachable" too eagerly.
    let verdict = probe_box_session(3).await;
    // device_online() is blocking; keep it off the UI thread too.
    tauri::async_runtime::spawn_blocking(move || {
        match verdict {
            Some(true) => "ok",
            Some(false) => "stale_bearer",
            None => {
                if wg_key_missing() {
                    "needs_repair"
                } else if device_online() {
                    "box_unreachable"
                } else {
                    "device_offline"
                }
            }
        }
        .to_string()
    })
    .await
    .unwrap_or_else(|_| "box_unreachable".to_string())
}

#[cfg(test)]
mod session_probe_tests {
    use super::classify_session_response;

    fn resp(status: &str, body: &str) -> String {
        format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{body}")
    }

    #[test]
    fn unauth_user_null_is_rejected() {
        assert_eq!(classify_session_response(&resp("200 OK", "{\"user\":null}")), Some(false));
    }

    #[test]
    fn authed_user_object_is_ok() {
        assert_eq!(
            classify_session_response(&resp("200 OK", "{\"user\":{\"device_id\":\"d1\"}}")),
            Some(true)
        );
    }

    #[test]
    fn status_401_is_rejected() {
        assert_eq!(classify_session_response(&resp("401 Unauthorized", "{}")), Some(false));
    }

    #[test]
    fn header_text_never_false_matches() {
        // A header value containing the sentinel must not be read as the body.
        let raw = "HTTP/1.1 200 OK\r\nX-Note: \"user\":null\r\n\r\n{\"user\":{\"device_id\":\"d\"}}";
        assert_eq!(classify_session_response(raw), Some(true));
    }

    #[test]
    fn indeterminate_when_no_body() {
        assert_eq!(classify_session_response("HTTP/1.1 200 OK\r\n\r\n"), None);
    }
}

/// Build a shell `Command` for virtues-client. Resolution order:
///   1. `~/.virtues/bin/virtues-client` (system installer's location)
///   2. `/usr/local/bin/virtues-client` (alt install location)
///   3. the bundled sidecar (`binaries/virtues-client`)
///
/// The sidecar fallback matters for first-run: a freshly-installed app can pair
/// before virtues-client has been installed system-wide. Mirrors the collector.
fn virtues_client_command(
    app: &AppHandle,
) -> Result<tauri_plugin_shell::process::Command, String> {
    let shell = app.shell();
    let home_bin = dirs::home_dir()
        .unwrap_or_default()
        .join(".virtues")
        .join("bin")
        .join("virtues-client");
    if home_bin.exists() {
        return Ok(shell.command(home_bin.to_string_lossy().to_string()));
    }
    let usr_local = std::path::Path::new("/usr/local/bin/virtues-client");
    if usr_local.exists() {
        return Ok(shell.command(usr_local.to_string_lossy().to_string()));
    }
    shell.sidecar("virtues-client").map_err(|e| e.to_string())
}

// ============================================================================
// Tauri Commands (IPC from web frontend)
// ============================================================================

/// Returns whether the machine is currently paired to a Virtues server.
#[tauri::command]
fn get_client_status() -> bool {
    is_paired()
}

/// Discover Virtues servers on the local network by shelling to virtues-client.
#[tauri::command]
async fn discover_servers(app: AppHandle) -> Result<Vec<FoundServer>, String> {
    let output = virtues_client_command(&app)?
        .args(["discover", "--json"])
        .output()
        .await
        .map_err(|e| e.to_string())?;

    if output.status.success() {
        serde_json::from_slice(&output.stdout).map_err(|e| e.to_string())
    } else {
        Ok(vec![])
    }
}

/// Pair with a server using a 6-character code.
#[tauri::command]
async fn pair_with_code(
    app: AppHandle,
    server: String,
    code: String,
) -> Result<(), String> {
    let output = virtues_client_command(&app)?
        .args(["pair-code", &code, "--server", &server])
        .output()
        .await
        .map_err(|e| e.to_string())?;

    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

/// Start the localhost helper after pairing. In the iroh model the helper
/// (`virtues-client up`) reaches the box by its stored EndpointId reach ticket
/// (from pairing) over iroh — LAN-direct → hole-punched → relay — and serves it
/// at `http://localhost:7117`. The browser then talks plain loopback HTTP, so
/// same-origin cookies/CSRF are untouched. No WireGuard, no root daemon.
///
/// `up` is long-running (it's the proxy), so we spawn it rather than await it.
/// TODO(iroh): persist it across app restarts via a LaunchAgent/Tauri sidecar so
/// it survives an app quit; today it runs for the app session.
///
/// The collector (data collection) is NOT installed here — it pairs as its own
/// device and needs Full Disk Access / Accessibility grants, so it stays an
/// explicit opt-in via `install_collector`.
#[tauri::command]
async fn install_helpers(app: AppHandle, _server: String) -> Result<(), String> {
    let (_events, _child) = virtues_client_command(&app)?
        .args(["up"])
        .spawn()
        .map_err(|e| format!("helper spawn failed: {e}"))?;
    // Keep the proxy running for the app session; the child is managed by the
    // Tauri shell plugin and torn down with the app.
    std::mem::forget(_child);
    Ok(())
}

/// Remove the localhost proxy LaunchAgent (reverse of [`install_helpers`]).
#[tauri::command]
async fn uninstall_helpers(app: AppHandle) -> Result<(), String> {
    let _ = virtues_client_command(&app)?
        .args(["uninstall"])
        .output()
        .await;
    Ok(())
}

/// Clear THIS Mac's pairing — the productized `make dev-wipe-mac`. Clears the
/// stored bundle/keys (keychain + ~/.virtues/bundle.json) and removes the proxy
/// LaunchAgent. Local-only: never needs the box to be reachable.
///
/// Called (a) by the connect screen *before* a re-pair, so re-pairing a box
/// that was RESET starts clean — otherwise the box's new SPKI key trips the
/// TOFU pin from the old pairing and the daemon refuses; and (b) by Settings →
/// "Disconnect this Mac" for a deliberate hand-off / box switch.
#[tauri::command]
async fn forget_pairing(app: AppHandle) -> Result<(), String> {
    // `revoke` clears the local creds (keychain + bundle.json), best-effort
    // box-side credential delete first but clears locally regardless.
    let _ = virtues_client_command(&app)?
        .args(["revoke"])
        .output()
        .await;
    // Drop the proxy LaunchAgent so a stale bearer can't keep serving.
    let _ = virtues_client_command(&app)?
        .args(["uninstall"])
        .output()
        .await;
    Ok(())
}

/// Re-check whether the box now accepts this device — backs the connect
/// screen's "Retry" on the unreachable state (box was off/asleep/elsewhere).
/// `true` → load the box; `false` → still not reachable/accepted.
#[tauri::command]
async fn recheck_box() -> bool {
    // Single attempt: the connect screen polls this on a timer, so the poll
    // interval IS the retry. The async wrapper keeps the probe off the UI thread.
    probe_box_session(1).await == Some(true)
}

/// Relaunch the app — used after Settings → "Disconnect this Mac" so the window
/// comes back up through the launch decision (now unpaired → the connect
/// screen) instead of sitting on a dead localhost:7117 the proxy no longer
/// serves. Never returns.
#[tauri::command]
fn restart_app(app: AppHandle) {
    app.restart();
}

/// Get collector daemon status by invoking CLI
#[tauri::command]
async fn get_collector_status(app: AppHandle) -> Result<CollectorStatus, String> {
    let shell = app.shell();

    let installed_path = dirs::home_dir()
        .unwrap_or_default()
        .join(".virtues")
        .join("bin")
        .join("virtues-collector");

    let output = if installed_path.exists() {
        shell
            .command(installed_path.to_string_lossy().to_string())
            .args(["status", "--json"])
            .output()
            .await
            .map_err(|e| e.to_string())?
    } else {
        match shell.sidecar("virtues-collector") {
            Ok(cmd) => cmd.args(["status", "--json"]).output().await.map_err(|e| e.to_string())?,
            Err(_) => return Ok(CollectorStatus::default()),
        }
    };

    if output.status.success() {
        serde_json::from_slice(&output.stdout).map_err(|e| e.to_string())
    } else {
        Ok(CollectorStatus::default())
    }
}

/// Install the collector as a LaunchAgent
#[tauri::command]
async fn install_collector(app: AppHandle, token: String) -> Result<(), String> {
    let shell = app.shell();

    // Point the collector at the LOCAL PROXY (:7117), not its built-in
    // localhost:8000 fallback. The proxy (installed by `install_helpers`) is
    // already up by the time "Turn on this Mac" runs and tunnels to the box
    // over WireGuard — so this works whether the box is on this LAN or remote.
    // Without it the collector pairs against localhost:8000 and a remote-box
    // user gets "Could not connect" (ECONNREFUSED). The endpoint returned by
    // pair/consume is persisted, so this also routes ongoing uploads.
    let output = shell
        .sidecar("virtues-collector")
        .map_err(|e| e.to_string())?
        .env("VIRTUES_TOKEN", &token)
        .env("VIRTUES_API_URL", "http://localhost:7117")
        .args(["install", "--token-from-env"])
        .output()
        .await
        .map_err(|e| e.to_string())?;

    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

/// Uninstall the collector LaunchAgent
#[tauri::command]
async fn uninstall_collector(app: AppHandle) -> Result<(), String> {
    let installed_path = dirs::home_dir()
        .unwrap_or_default()
        .join(".virtues")
        .join("bin")
        .join("virtues-collector");

    if !installed_path.exists() {
        return Ok(());
    }

    let shell = app.shell();
    let output = shell
        .command(installed_path.to_string_lossy().to_string())
        .args(["uninstall"])
        .output()
        .await
        .map_err(|e| e.to_string())?;

    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

/// Pause collector
#[tauri::command]
async fn pause_collector(app: AppHandle) -> Result<(), String> {
    let installed_path = dirs::home_dir()
        .unwrap_or_default()
        .join(".virtues")
        .join("bin")
        .join("virtues-collector");

    if !installed_path.exists() {
        return Err("Collector not installed".to_string());
    }

    let shell = app.shell();
    let output = shell
        .command(installed_path.to_string_lossy().to_string())
        .args(["pause"])
        .output()
        .await
        .map_err(|e| e.to_string())?;

    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

/// Resume collector
#[tauri::command]
async fn resume_collector(app: AppHandle) -> Result<(), String> {
    let installed_path = dirs::home_dir()
        .unwrap_or_default()
        .join(".virtues")
        .join("bin")
        .join("virtues-collector");

    if !installed_path.exists() {
        return Err("Collector not installed".to_string());
    }

    let shell = app.shell();
    let output = shell
        .command(installed_path.to_string_lossy().to_string())
        .args(["resume"])
        .output()
        .await
        .map_err(|e| e.to_string())?;

    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

/// Stop collector daemon
#[tauri::command]
async fn stop_collector(app: AppHandle) -> Result<(), String> {
    let installed_path = dirs::home_dir()
        .unwrap_or_default()
        .join(".virtues")
        .join("bin")
        .join("virtues-collector");

    if !installed_path.exists() {
        return Err("Collector not installed".to_string());
    }

    let shell = app.shell();
    let output = shell
        .command(installed_path.to_string_lossy().to_string())
        .args(["stop"])
        .output()
        .await
        .map_err(|e| e.to_string())?;

    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

/// Open System Preferences to Full Disk Access pane
#[tauri::command]
async fn open_full_disk_access(app: AppHandle) -> Result<(), String> {
    app.shell()
        .command("open")
        .args(["x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles"])
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Open System Preferences to Accessibility pane
#[tauri::command]
async fn open_accessibility_settings(app: AppHandle) -> Result<(), String> {
    app.shell()
        .command("open")
        .args(["x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"])
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

// ============================================================================
// Self-updater
// ============================================================================

/// A detected, ready-to-apply app update, surfaced *ambiently* — one native
/// notification + a tray "Restart to update" line. We never force a relaunch
/// (the Chrome model): the user applies it when convenient. Apply re-checks and
/// runs download+install, so we never stash a non-Send `Update` across threads.
#[derive(Default)]
struct UpdateState {
    ready: Option<ReadyUpdate>,
}

struct ReadyUpdate {
    version: String,
    /// When we first saw it ready — drives the escalating amber→red tray nudge.
    ready_at: std::time::Instant,
}

/// Check the **stable** channel (mac-latest `latest.json`) for a newer version.
/// On a hit: record it + fire ONE native notification; the tray's own poll then
/// surfaces the "Restart to update" line. Silent best-effort — `None`/errors are
/// no-ops (up to date, or offline; retried next tick). The actual download runs
/// on the user's click ([`apply_update`]) so we don't hold an `Update` in state.
async fn check_for_update(app: &AppHandle) {
    use tauri_plugin_notification::NotificationExt;
    use tauri_plugin_updater::UpdaterExt;

    let Ok(updater) = app.updater() else { return };
    let update = match updater.check().await {
        Ok(Some(u)) => u,
        _ => return,
    };
    let version = update.version.clone();
    let note = update
        .body
        .as_deref()
        .and_then(|b| b.lines().map(str::trim).find(|l| !l.is_empty()))
        .unwrap_or("")
        .to_string();

    // Record + dedupe so we notify at most once per version.
    {
        let state = app.state::<std::sync::Mutex<UpdateState>>();
        let mut g = state.lock().unwrap();
        if g.ready.as_ref().map(|r| r.version.as_str()) == Some(version.as_str()) {
            return;
        }
        g.ready = Some(ReadyUpdate {
            version: version.clone(),
            ready_at: std::time::Instant::now(),
        });
    }

    let body = if note.is_empty() {
        "Restart Virtues to apply.".to_string()
    } else {
        format!("{note}\n\nRestart Virtues to apply.")
    };
    let _ = app
        .notification()
        .builder()
        .title(format!("Virtues {version} is ready"))
        .body(body)
        .show();
}

/// Apply a pending update: re-check (cheap), download + install, then relaunch.
/// On relaunch the helper-reconcile redeploys the new sidecars — loop closed.
/// `app.restart()` never returns.
async fn apply_update(app: AppHandle) {
    use tauri_plugin_updater::UpdaterExt;
    let Ok(updater) = app.updater() else { return };
    if let Ok(Some(update)) = updater.check().await {
        if update.download_and_install(|_, _| {}, || {}).await.is_ok() {
            app.restart();
        }
    }
}

// ============================================================================
// Menu-bar tray
// ============================================================================

/// The collector is "installed" iff its LaunchAgent binary exists at the path
/// `install_collector` writes to. `get_collector_status` returns a default
/// (all-false) struct both when stopped AND when never installed, so the tray
/// needs this to tell "off because not set up" from "installed but paused".
fn collector_installed() -> bool {
    dirs::home_dir()
        .map(|h| h.join(".virtues").join("bin").join("virtues-collector").exists())
        .unwrap_or(false)
}

/// Status-dot color, drawn as a colored `IconMenuItem` icon (not a template, so
/// it keeps its color): 🟢 working · 🟡 transient/attention · 🔴 needs action ·
/// ⚪ inactive — but as real PNG dots, no emoji.
#[derive(Clone, Copy)]
enum Dot {
    Green,
    Amber,
    Red,
    Grey,
}

/// Decode the bundled dot PNG for a state. `None` only on a decode failure, in
/// which case the line just shows text with no icon rather than crashing.
fn dot_image(dot: Dot) -> Option<tauri::image::Image<'static>> {
    let bytes: &[u8] = match dot {
        Dot::Green => include_bytes!("../icons/dot-green.png"),
        Dot::Amber => include_bytes!("../icons/dot-amber.png"),
        Dot::Red => include_bytes!("../icons/dot-red.png"),
        Dot::Grey => include_bytes!("../icons/dot-grey.png"),
    };
    tauri::image::Image::from_bytes(bytes).ok()
}

/// First line of the tray menu: where this device stands with the box. Returns
/// (status dot, label).
fn box_label() -> (Dot, &'static str) {
    if !is_paired() {
        return (Dot::Grey, "Box: not connected");
    }
    match probe_box_session_blocking(1) {
        Some(true) => (Dot::Green, "Box: connected"),
        Some(false) => (Dot::Red, "Box: needs re-pairing"),
        None if wg_key_missing() => (Dot::Red, "Box: needs re-pairing"),
        None => (Dot::Amber, "Box: unreachable"),
    }
}

/// Second line: the collector daemon's state, with the same dot vocabulary.
fn collector_label(installed: bool, status: &CollectorStatus) -> (Dot, &'static str) {
    if !installed {
        (Dot::Grey, "Collector: off")
    } else if status.paused {
        (Dot::Amber, "Collector: paused")
    } else if status.running {
        (Dot::Green, "Collector: collecting")
    } else {
        (Dot::Red, "Collector: stopped")
    }
}

/// Third line (a dim subtitle): when the collector last flushed to the box, plus
/// any backlog. `—` when there's no collector, `never` when it has one but has
/// not synced yet.
fn last_sync_label(installed: bool, status: &CollectorStatus) -> String {
    if !installed {
        return "Last sync: —".to_string();
    }
    let when = match status.last_sync.as_deref() {
        Some(iso) => format_clock(iso).unwrap_or_else(|| "unknown".to_string()),
        None => "never".to_string(),
    };
    let pending = status.pending_events + status.pending_messages;
    if pending > 0 {
        format!("Last sync: {when} · {pending} queued")
    } else {
        format!("Last sync: {when}")
    }
}

/// Format an RFC3339 timestamp (the workspace's wire format for `DateTime<Utc>`)
/// as a local clock time — bare "2:34 PM" for today, "Jun 22, 2:34 PM" otherwise.
/// `None` if it doesn't parse, so the caller can fall back rather than show junk.
fn format_clock(iso: &str) -> Option<String> {
    use chrono::{DateTime, Local};
    let dt = DateTime::parse_from_rfc3339(iso).ok()?.with_timezone(&Local);
    if dt.date_naive() == Local::now().date_naive() {
        Some(dt.format("%-I:%M %p").to_string())
    } else {
        Some(dt.format("%b %-d, %-I:%M %p").to_string())
    }
}

/// The tray's mutable menu items, bundled so the poll loop and the menu-event
/// handler can each hold a clone and refresh the same lines.
#[derive(Clone)]
struct TrayItems {
    box_status: tauri::menu::IconMenuItem<tauri::Wry>,
    collector_status: tauri::menu::IconMenuItem<tauri::Wry>,
    last_sync: tauri::menu::MenuItem<tauri::Wry>,
    toggle: tauri::menu::MenuItem<tauri::Wry>,
    /// "Restart to update (vX)" when an update is staged, else a disabled
    /// "Virtues is up to date". Driven by [`UpdateState`] on each poll.
    update: tauri::menu::IconMenuItem<tauri::Wry>,
    /// "Check for Updates…" — a manual trigger for [`check_for_update`]. Its
    /// label flips to "Checking…" then "Up to date ✓" for transient feedback.
    check_now: tauri::menu::MenuItem<tauri::Wry>,
}

/// Set the "Check for Updates…" item's label + enabled state on the main
/// thread (AppKit requires UI mutation there). Used for the transient
/// "Checking…" / "Up to date ✓" feedback on a manual check.
fn set_check_label(app: &AppHandle, item: &tauri::menu::MenuItem<tauri::Wry>, text: &str, enabled: bool) {
    let item = item.clone();
    let text = text.to_string();
    let _ = app.run_on_main_thread(move || {
        let _ = item.set_text(&text);
        let _ = item.set_enabled(enabled);
    });
}

/// Recompute the status lines + toggle and apply them. Spawns its OWN thread
/// because computing the state BLOCKS (probing the box over TCP, and shelling
/// out to the collector CLI when installed), then hops to the main thread for
/// the actual menu mutation — AppKit requires UI changes on the main thread.
/// Safe to call from anywhere, including the (main-thread) menu-event handler.
fn refresh_tray(app: &AppHandle, items: TrayItems) {
    let app = app.clone();
    std::thread::spawn(move || {
        let installed = collector_installed();
        // Skip the CLI call entirely when nothing's installed: its sidecar
        // fallback would fork a process on every poll just to hand back defaults.
        let status = if installed {
            tauri::async_runtime::block_on(get_collector_status(app.clone())).unwrap_or_default()
        } else {
            CollectorStatus::default()
        };
        let (box_dot, box_text) = box_label();
        let (coll_dot, coll_text) = collector_label(installed, &status);
        let sync_text = last_sync_label(installed, &status);
        let toggle_text = if status.paused { "Resume collecting" } else { "Pause collecting" };

        // Update line: amber "Restart to update (vX)" when staged, escalating to
        // red after ~3 days unapplied (Chrome's green→orange→red nudge); a
        // disabled "up to date" otherwise.
        let update = {
            let st = app.state::<std::sync::Mutex<UpdateState>>();
            let g = st.lock().unwrap();
            g.ready.as_ref().map(|r| {
                let dot = if r.ready_at.elapsed() > std::time::Duration::from_secs(3 * 24 * 3600) {
                    Dot::Red
                } else {
                    Dot::Amber
                };
                (dot, format!("Restart to update ({})", r.version))
            })
        };

        let _ = app.run_on_main_thread(move || {
            let _ = items.box_status.set_text(box_text);
            let _ = items.box_status.set_icon(dot_image(box_dot));
            let _ = items.collector_status.set_text(coll_text);
            let _ = items.collector_status.set_icon(dot_image(coll_dot));
            let _ = items.last_sync.set_text(sync_text);
            let _ = items.toggle.set_text(toggle_text);
            // Disabled when not installed so pause/resume can't be invoked in a
            // state where the CLI would just error — keeps the user from a
            // no-op foot-gun.
            let _ = items.toggle.set_enabled(installed);
            match &update {
                Some((dot, text)) => {
                    let _ = items.update.set_text(text);
                    let _ = items.update.set_icon(dot_image(*dot));
                    let _ = items.update.set_enabled(true);
                }
                None => {
                    let _ = items.update.set_text("Virtues is up to date");
                    let _ = items.update.set_icon(dot_image(Dot::Grey));
                    let _ = items.update.set_enabled(false);
                }
            }
        });
    });
}

/// Build the macOS menu-bar item: two colored status lines, a dim last-sync
/// subtitle, a pause/resume toggle, show, and a real Quit (the window close
/// button only HIDES — see `on_window_event` — so this is the only way to exit).
fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    use tauri::menu::{IconMenuItem, Menu, MenuItem, PredefinedMenuItem};
    use tauri::tray::TrayIconBuilder;

    // Status lines are ENABLED (so the text reads at full strength, not greyed),
    // carry a colored status dot, and clicking either one opens the app — handy
    // when it says "needs re-pairing"/"unreachable" and the fix lives in-window.
    let status_box = IconMenuItem::with_id(
        app, "status_box", "Box: checking…", true, dot_image(Dot::Grey), None::<&str>,
    )?;
    let status_collector = IconMenuItem::with_id(
        app, "status_collector", "Collector: checking…", true, dot_image(Dot::Grey), None::<&str>,
    )?;
    // A dim, non-interactive subtitle (disabled = greyed, which is what we want
    // for secondary info).
    let last_sync = MenuItem::with_id(app, "last_sync", "Last sync: —", false, None::<&str>)?;
    // Starts disabled; the first poll enables it iff the collector is installed.
    let toggle = MenuItem::with_id(app, "toggle_collector", "Pause collecting", false, None::<&str>)?;
    let show = MenuItem::with_id(app, "show", "Show Virtues", true, None::<&str>)?;
    // Update line: disabled "up to date" until the check loop stages one, then
    // the poll flips it to an enabled amber "Restart to update (vX)".
    let update = IconMenuItem::with_id(
        app, "update_item", "Virtues is up to date", false, dot_image(Dot::Grey), None::<&str>,
    )?;
    // A dim, non-interactive line showing the running version — so a glance at
    // the menu answers "am I current?" without opening anything.
    let version_label = MenuItem::with_id(
        app,
        "version_label",
        format!("Virtues v{}", app.package_info().version),
        false,
        None::<&str>,
    )?;
    // Manual "check now" — runs the same check the 6h poll runs, for the
    // impatient/debugging case. Its label gives transient feedback on click.
    let check_now = MenuItem::with_id(app, "check_now", "Check for Updates…", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Virtues", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[
            &status_box,
            &status_collector,
            &last_sync,
            &PredefinedMenuItem::separator(app)?,
            &toggle,
            &show,
            &PredefinedMenuItem::separator(app)?,
            &version_label,
            &check_now,
            &update,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    let items = TrayItems {
        box_status: status_box,
        collector_status: status_collector,
        last_sync,
        toggle,
        update,
        check_now,
    };

    // The ∴ mark as a TEMPLATE image: monochrome black+alpha that AppKit recolors
    // to fit light/dark menu bars. The full-color app icon would not adapt and
    // would look wrong up there.
    let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray.png"))
        .expect("decode bundled tray icon");

    // Cloned into the menu-event handler so a pause/resume can refresh the menu
    // at once instead of waiting out the ~10s poll.
    let event_items = items.clone();

    TrayIconBuilder::with_id("main-tray")
        .icon(icon)
        .icon_as_template(true)
        .tooltip("Virtues")
        .menu(&menu)
        .on_menu_event(move |app, event| match event.id.as_ref() {
            // The two status lines double as a shortcut into the app.
            "show" | "status_box" | "status_collector" => {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
            }
            "quit" => app.exit(0),
            // "Restart to update" — re-check, download+install, relaunch. The
            // item is only enabled once an update is staged, so a click here
            // always has something to apply.
            "update_item" => {
                let app = app.clone();
                tauri::async_runtime::spawn(async move { apply_update(app).await });
            }
            // Manual update check. Flip the label to "Checking…" while it runs,
            // then either let the staged-update path take over (refresh_tray
            // surfaces the amber "Restart to update" line) or flash "Up to date
            // ✓" for a couple seconds before reverting.
            "check_now" => {
                let app = app.clone();
                let items = event_items.clone();
                tauri::async_runtime::spawn(async move {
                    set_check_label(&app, &items.check_now, "Checking…", false);
                    check_for_update(&app).await;
                    let staged = {
                        let st = app.state::<std::sync::Mutex<UpdateState>>();
                        let staged = st.lock().unwrap().ready.is_some();
                        staged
                    };
                    refresh_tray(&app, items.clone());
                    if staged {
                        // Amber "Restart to update" line now carries the signal;
                        // just restore the trigger label.
                        set_check_label(&app, &items.check_now, "Check for Updates…", true);
                    } else {
                        set_check_label(&app, &items.check_now, "Up to date ✓", false);
                        let app = app.clone();
                        let item = items.check_now.clone();
                        // Revert to the actionable label after a brief beat.
                        std::thread::spawn(move || {
                            std::thread::sleep(std::time::Duration::from_secs(3));
                            set_check_label(&app, &item, "Check for Updates…", true);
                        });
                    }
                });
            }
            "toggle_collector" => {
                // Flip based on the LIVE state, not the (possibly stale) label,
                // then refresh immediately so the menu shows the new state.
                let app = app.clone();
                let items = event_items.clone();
                tauri::async_runtime::spawn(async move {
                    if let Ok(status) = get_collector_status(app.clone()).await {
                        let _ = if status.paused {
                            resume_collector(app.clone()).await
                        } else {
                            pause_collector(app.clone()).await
                        };
                    }
                    refresh_tray(&app, items);
                });
            }
            _ => {}
        })
        .build(app)?;

    // Keep the labels honest. A poll (not an event subscription) because the
    // collector is a separate daemon with no push channel back to this app.
    let app = app.clone();
    std::thread::spawn(move || loop {
        refresh_tray(&app, items.clone());
        std::thread::sleep(std::time::Duration::from_secs(10));
    });

    Ok(())
}

// ============================================================================
// Helper reconcile — keep installed helpers matching the app's bundled ones
// ============================================================================

/// On launch, make the installed background helpers (`virtues-client`,
/// `virtues-collector` in `~/.virtues/bin/`) match the versions THIS app bundle
/// ships. The app and its helpers are built + signed together, but the helpers
/// are *copied* into `~/.virtues/bin` at install/pair time and run as
/// LaunchAgents — so a plain app update leaves the OLD helper running. That bit
/// us hard: after updating the app, a stale proxy couldn't read a freshly-paired
/// WG key and silently fell back to a relay forever. Reconciling here turns
/// "install a new app" into "all its parts update," with zero user action.
///
/// Only touches helpers that are ALREADY installed — first-run install (which
/// needs a paired upstream / permission grants) stays with the pair/opt-in flow.
/// A no-op once everything's in sync, so it's safe to run on every launch.
///
/// Returns true if anything was actually redeployed (the caller pauses briefly
/// so a just-restarted proxy can settle before the launch probe).
fn reconcile_helpers() -> bool {
    let mut changed = false;
    for (name, agent) in [
        ("virtues-client", "com.virtues.client"),
        ("virtues-collector", "com.virtues.collector"),
    ] {
        match reconcile_one(name, agent) {
            Ok(true) => {
                changed = true;
                eprintln!("[reconcile] {name}: redeployed to match app bundle + restarted {agent}");
            }
            Ok(false) => {}
            Err(e) => eprintln!("[reconcile] {name}: {e}"),
        }
    }
    changed
}

/// Reconcile one helper. `Ok(true)` = it was stale and got redeployed.
fn reconcile_one(name: &str, agent: &str) -> Result<bool, String> {
    let installed = dirs::home_dir()
        .ok_or("no home dir")?
        .join(".virtues")
        .join("bin")
        .join(name);
    // Not installed → nothing to reconcile (the pair / "turn on this Mac" flow
    // installs it fresh, with the upstream/permissions it needs).
    if !installed.exists() {
        return Ok(false);
    }
    // The bundled helper sits next to this app binary in `Contents/MacOS/`.
    let bundled = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .parent()
        .ok_or("no exe dir")?
        .join(name);
    // Dev build / sidecar not alongside → nothing we can reconcile against.
    if !bundled.exists() || !files_differ(&bundled, &installed) {
        return Ok(false);
    }
    copy_executable(&bundled, &installed).map_err(|e| e.to_string())?;
    // Kick the LaunchAgent so launchd drops the old process and runs the new
    // binary now (rename-over-running is fine on macOS; the live process holds
    // the old inode until this restart). Best-effort: if the agent isn't loaded
    // the next login / install picks it up.
    let uid = std::process::Command::new("id")
        .arg("-u")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    if !uid.is_empty() {
        let _ = std::process::Command::new("/bin/launchctl")
            .args(["kickstart", "-k", &format!("gui/{uid}/{agent}")])
            .output();
    }
    Ok(true)
}

/// Byte-equal? Cheap size check first, content compare only if sizes match (the
/// up-to-date case). Any IO error → treat as "differ" so we err toward refresh.
fn files_differ(a: &std::path::Path, b: &std::path::Path) -> bool {
    match (a.metadata(), b.metadata()) {
        (Ok(ma), Ok(mb)) if ma.len() == mb.len() => {
            match (std::fs::read(a), std::fs::read(b)) {
                (Ok(x), Ok(y)) => x != y,
                _ => true,
            }
        }
        _ => true,
    }
}

/// Atomic replace of an executable (temp copy → chmod 0755 → rename). Renaming
/// over a running binary is safe on macOS; the kickstart that follows restarts
/// the process onto the new file.
fn copy_executable(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    let tmp = dst.with_extension("new");
    std::fs::copy(src, &tmp)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))?;
    }
    std::fs::rename(&tmp, dst)
}

// ============================================================================
// App Setup
// ============================================================================

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_notification::init())
        .invoke_handler(tauri::generate_handler![
            get_client_status,
            discover_servers,
            pair_with_code,
            install_helpers,
            uninstall_helpers,
            forget_pairing,
            recheck_box,
            diagnose_box,
            restart_app,
            get_collector_status,
            install_collector,
            uninstall_collector,
            pause_collector,
            resume_collector,
            stop_collector,
            open_full_disk_access,
            open_accessibility_settings,
        ])
        .setup(|app| {
            // Keep the installed background helpers matching what this app
            // bundle ships, BEFORE we probe the proxy below — otherwise a stale
            // helper (e.g. a proxy that can't read a freshly-paired key) would
            // skew the launch decision. If anything was actually swapped, give
            // the just-restarted proxy a moment to bind :7117 before probing.
            if reconcile_helpers() {
                std::thread::sleep(std::time::Duration::from_millis(700));
            }

            // Shared state for the self-updater (read by the tray poll).
            app.manage(std::sync::Mutex::new(UpdateState::default()));

            // Paired → the local virtues-client proxy on :7117 (NOT the box's
            // own 8000; the proxy listens on 7117 to avoid squatting a common
            // dev port). Keep in sync with LOCAL_PROXY_PORT in
            // apps/desktop/src/proxy.rs, the CSP, and pair.html.
            // Decide where to land. A valid pairing reconnects SILENTLY (the
            // 90% reinstall case); we only ever interrupt when something's
            // actually wrong, and the connect screen is the single recovery
            // surface. The verdict is passed to pair.html via the URL hash so it
            // can show the right one-line banner:
            //   not paired        → fresh connect screen
            //   box accepts us    → load the box
            //   box rejects us    → #reset      ("your box was reset, reconnect")
            //   box unreachable   → #unreachable ("can't reach it" + Retry)
            // A SINGLE fast probe (not the multi-retry loop): reachable boxes
            // reconnect silently with no connect-screen flash (the
            // silent-reconnect doctrine), and an unreachable box bounds the
            // pre-window delay to ~1.2s instead of blocking launch for ~7.5s.
            // We never retry here — the connect screen polls asynchronously off
            // the UI thread, so recovery doesn't cost main-thread time.
            let url = if !is_paired() {
                WebviewUrl::App("pair.html".into())
            } else {
                match probe_box_session_blocking(1) {
                    Some(true) => WebviewUrl::External("http://localhost:7117".parse().unwrap()),
                    Some(false) => WebviewUrl::App("pair.html#reset".into()),
                    None if wg_key_missing() => WebviewUrl::App("pair.html#repair".into()),
                    None => WebviewUrl::App("pair.html#unreachable".into()),
                }
            };

            let window = WebviewWindowBuilder::new(app, "main", url)
                .title("Virtues")
                .inner_size(1200.0, 800.0)
                .min_inner_size(800.0, 600.0)
                .center()
                .visible(true)
                // Tauri's native OS drag-drop handler is on by default and swallows
                // file drops before they reach the webview, so the chat composer's
                // HTML5 ondrop/dataTransfer.files never fires. Disable it to let
                // drops fall through to the web layer (works in-browser already).
                .disable_drag_drop_handler()
                .build()?;

            // Only used in debug; silence the release-build unused warning.
            #[cfg(debug_assertions)]
            window.open_devtools();
            #[cfg(not(debug_assertions))]
            let _ = window;

            setup_tray(app.handle())?;

            // Self-update check loop: first pass ~5s after launch (off the
            // critical path, after reconcile), then every 6h. The stable channel
            // (mac-latest latest.json) is the source; download is deferred to the
            // user's "Restart to update" click. The tray's own poll surfaces the
            // staged state within its interval.
            let updater_handle = app.handle().clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(5));
                loop {
                    tauri::async_runtime::block_on(check_for_update(&updater_handle));
                    std::thread::sleep(std::time::Duration::from_secs(6 * 3600));
                }
            });

            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            if let tauri::RunEvent::Reopen { has_visible_windows, .. } = event {
                if !has_visible_windows {
                    if let Some(window) = app_handle.get_webview_window("main") {
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                }
            }
        });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    main();
}
