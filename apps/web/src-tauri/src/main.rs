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

/// Check whether this machine has a paired bundle in the OS keychain.
fn is_paired() -> bool {
    keyring::Entry::new("virtues-client", "default-box")
        .and_then(|e| e.get_password())
        .is_ok()
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

/// Install the networking helpers after pairing: the user-level localhost proxy
/// (LaunchAgent, silent) and the root WG tunnel + .virtues DNS (LaunchDaemon).
///
/// The daemon needs root, so macOS shows ONE password prompt via osascript. The
/// collector (data collection) is deliberately NOT installed here — it pairs as
/// its own device and needs Full Disk Access / Accessibility grants, so it stays
/// an explicit opt-in via `install_collector`.
#[tauri::command]
async fn install_helpers(app: AppHandle) -> Result<(), String> {
    // 1. User-level proxy LaunchAgent. Runs the bundled sidecar (or an existing
    //    install) which copies itself to ~/.virtues/bin and loads the agent.
    let out = virtues_client_command(&app)?
        .args(["install"])
        .output()
        .await
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!(
            "proxy install failed: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }

    // 2. Root daemon (WG tunnel + .virtues DNS) — one admin prompt. We run the
    //    binary that step 1 just placed at ~/.virtues/bin (absolute path, since
    //    osascript's root shell has a different $HOME). Single-quote each
    //    component so usernames with spaces survive the inner /bin/sh.
    let home = dirs::home_dir().ok_or("no home directory")?;
    let user = std::env::var("USER").unwrap_or_else(|_| {
        home.file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default()
    });
    let client_bin = home.join(".virtues").join("bin").join("virtues-client");
    let bundle = home.join(".virtues").join("bundle.json");
    let script = format!(
        "do shell script \"'{bin}' install-system --user '{user}' --bundle '{bundle}'\" \
         with administrator privileges",
        bin = client_bin.display(),
        user = user,
        bundle = bundle.display(),
    );
    let out = app
        .shell()
        .command("osascript")
        .args(["-e", &script])
        .output()
        .await
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        // Most commonly: the user cancelled the password dialog (-128).
        return Err(format!(
            "tunnel install (admin): {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(())
}

/// Remove the networking helpers (reverse of [`install_helpers`]). Best-effort.
#[tauri::command]
async fn uninstall_helpers(app: AppHandle) -> Result<(), String> {
    // Root daemon first (one admin prompt), then the user-level agent.
    let home = dirs::home_dir().ok_or("no home directory")?;
    let client_bin = home.join(".virtues").join("bin").join("virtues-client");
    let script = format!(
        "do shell script \"'{bin}' uninstall-system\" with administrator privileges",
        bin = client_bin.display(),
    );
    let _ = app
        .shell()
        .command("osascript")
        .args(["-e", &script])
        .output()
        .await;
    let _ = virtues_client_command(&app)?
        .args(["uninstall"])
        .output()
        .await;
    Ok(())
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

    let output = shell
        .sidecar("virtues-collector")
        .map_err(|e| e.to_string())?
        .env("VIRTUES_TOKEN", &token)
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
            let url = if is_paired() {
                WebviewUrl::External("http://localhost:7117".parse().unwrap())
            } else {
                WebviewUrl::App("pair.html".into())
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
