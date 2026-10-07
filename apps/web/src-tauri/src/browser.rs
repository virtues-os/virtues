//! Logging in to a source inside the app, so its session becomes the credential.
//!
//! X and Instagram have no API a personal box can use for your own saves; their
//! syncs replay the web client's request with your session cookies. Copying
//! those cookies out of developer tools is the hardest connect step in the
//! product. Here the app opens the site's login page in a window of its own, the
//! owner logs in as they would in Safari, and once the cookies that mean "logged
//! in" exist the app returns the site's cookie jar to the page that asked. That
//! page posts it to the box's ordinary connect route, so the box never learns
//! which door the credential came through.
//!
//! The window is incognito: its cookie store lives in memory and is gone when
//! it closes. The session's only lasting copy is the box's vault. It also has no
//! IPC. The app's capabilities grant nothing to a remote origin on the Mac
//! (`capabilities/mac.json` has no `remote`), and default.json's remote grant is
//! localhost only, so x.com in this window cannot reach a single app command.
//!
//! Measured in agents/record/webkit-agent-spike.md: X's `auth_token`/`ct0` read
//! out of a WKWebView's store after a real login, and the X sync ran on them.

use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

/// How long the window waits for a login before giving up. Long enough for a
/// password manager, a 2FA code and a slow SMS.
const LOGIN_TIMEOUT: Duration = Duration::from_secs(15 * 60);

/// Safari's own user agent. A bare WKWebView omits `Version/… Safari/…`, and
/// some sites treat that as an embedded browser and degrade the login page.
const SAFARI_UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 \
                         (KHTML, like Gecko) Version/26.0 Safari/605.1.15";

#[derive(Serialize)]
pub struct JarCookie {
    name: String,
    value: String,
}

/// Open `url` in a login window and resolve with the site's cookies once every
/// name in `cookies` is set. Errors with `"closed"` if the owner closes the
/// window first, and `"timeout"` after [`LOGIN_TIMEOUT`].
#[tauri::command]
pub async fn browser_login(
    app: AppHandle,
    source_id: String,
    url: String,
    cookies: Vec<String>,
    title: String,
) -> Result<Vec<JarCookie>, String> {
    let page: tauri::Url = url.parse().map_err(|e| format!("bad login url: {e}"))?;
    if page.scheme() != "https" {
        return Err("a login page must be https".into());
    }
    let label = format!(
        "login-{}",
        source_id.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>()
    );

    // A second click while the window is open brings it forward rather than
    // opening another.
    if let Some(open) = app.get_webview_window(&label) {
        let _ = open.set_focus();
        return Err("already open".into());
    }

    let window = WebviewWindowBuilder::new(&app, &label, WebviewUrl::External(page.clone()))
        .title(&title)
        .inner_size(520.0, 760.0)
        .center()
        .incognito(true)
        .user_agent(SAFARI_UA)
        // Sign-in popups (a provider's own window) open with the default
        // behavior. Sign in with Apple never gets here: WebKit hands it to the
        // system's native sheet.
        .on_new_window(|_, _| tauri::webview::NewWindowResponse::Allow)
        .build()
        .map_err(|e| format!("could not open the login window: {e}"))?;

    let started = Instant::now();
    loop {
        tokio::time::sleep(Duration::from_millis(800)).await;
        let Some(win) = app.get_webview_window(&label) else {
            return Err("closed".into());
        };
        if started.elapsed() > LOGIN_TIMEOUT {
            let _ = win.close();
            return Err("timeout".into());
        }
        // Read on this async task, never a sync command: WebView2 deadlocks
        // otherwise (tauri docs for `cookies_for_url`).
        let jar = match win.cookies_for_url(page.clone()) {
            Ok(jar) => jar,
            Err(_) => continue,
        };
        let has = |name: &str| jar.iter().any(|c| c.name() == name && !c.value().is_empty());
        if cookies.iter().all(|n| has(n)) {
            let out = jar
                .iter()
                .map(|c| JarCookie { name: c.name().to_string(), value: c.value().to_string() })
                .collect();
            let _ = window.close();
            return Ok(out);
        }
    }
}
