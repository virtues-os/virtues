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
/// std-only (no HTTP dep): a raw GET with short timeouts, retried a few times to
/// ride out the LaunchAgent proxy's startup race.
fn probe_box_session() -> Option<bool> {
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::time::Duration;

    let addr = "127.0.0.1:7117".parse().ok()?;
    for _ in 0..5 {
        if let Ok(mut s) = TcpStream::connect_timeout(&addr, Duration::from_millis(400)) {
            let _ = s.set_read_timeout(Some(Duration::from_millis(800)));
            let req = "GET /auth/session HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n";
            if s.write_all(req.as_bytes()).is_ok() {
                let mut buf = String::new();
                let _ = s.read_to_string(&mut buf);
                if let Some(verdict) = classify_session_response(&buf) {
                    return Some(verdict);
                }
            }
        }
        std::thread::sleep(Duration::from_millis(300));
    }
    None
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

/// Install the localhost proxy after pairing, pointed at the box address you
/// just paired against (`server`, e.g. `http://100.104.55.76:8000`).
///
/// This is the "direct" path: the proxy forwards `localhost:7117` straight to
/// that address over whatever transport reached it (Tailscale / LAN / SSH-forward
/// / IPv6). No WireGuard tunnel, no root daemon, no admin prompt — it works
/// anywhere the box's HTTP port is reachable, which is every case where you were
/// able to pair. (WireGuard remains in the binary for a future encrypted-LAN
/// mode; it's just not on the default path.)
///
/// The collector (data collection) is NOT installed here — it pairs as its own
/// device and needs Full Disk Access / Accessibility grants, so it stays an
/// explicit opt-in via `install_collector`.
#[tauri::command]
async fn install_helpers(app: AppHandle, server: String) -> Result<(), String> {
    let upstream = origin_to_hostport(&server);
    let out = virtues_client_command(&app)?
        .args(["install", "--upstream", &upstream])
        .output()
        .await
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!(
            "proxy install failed: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
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
fn recheck_box() -> bool {
    probe_box_session() == Some(true)
}

/// Relaunch the app — used after Settings → "Disconnect this Mac" so the window
/// comes back up through the launch decision (now unpaired → the connect
/// screen) instead of sitting on a dead localhost:7117 the proxy no longer
/// serves. Never returns.
#[tauri::command]
fn restart_app(app: AppHandle) {
    app.restart();
}

/// Reduce a paired server origin to the `host:port` the proxy forwards to.
/// `http://100.104.55.76:8000` -> `100.104.55.76:8000`; `adam.local:8000` stays.
/// Defaults the port to 8000 (the box's HTTP port) when none is present and the
/// host isn't a bracketed IPv6 literal.
fn origin_to_hostport(origin: &str) -> String {
    let s = origin.trim();
    let s = s
        .strip_prefix("http://")
        .or_else(|| s.strip_prefix("https://"))
        .unwrap_or(s);
    let hostport = s.split('/').next().unwrap_or(s).trim_end_matches('/');
    if hostport.contains(':') || hostport.starts_with('[') {
        hostport.to_string()
    } else {
        format!("{hostport}:8000")
    }
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
// App Setup
// ============================================================================

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            get_client_status,
            discover_servers,
            pair_with_code,
            install_helpers,
            uninstall_helpers,
            forget_pairing,
            recheck_box,
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
            let url = if !is_paired() {
                WebviewUrl::App("pair.html".into())
            } else {
                match probe_box_session() {
                    Some(true) => WebviewUrl::External("http://localhost:7117".parse().unwrap()),
                    Some(false) => WebviewUrl::App("pair.html#reset".into()),
                    None => WebviewUrl::App("pair.html#unreachable".into()),
                }
            };

            let window = WebviewWindowBuilder::new(app, "main", url)
                .title("Virtues")
                .inner_size(1200.0, 800.0)
                .min_inner_size(800.0, 600.0)
                .center()
                .visible(true)
                .build()?;

            // Only used in debug; silence the release-build unused warning.
            #[cfg(debug_assertions)]
            window.open_devtools();
            #[cfg(not(debug_assertions))]
            let _ = window;

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
