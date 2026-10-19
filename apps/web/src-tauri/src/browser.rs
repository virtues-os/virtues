//! Logging in to a source inside the app, so its session becomes the credential.
//!
//! X and Instagram have no API a personal box can use for your own saves; their
//! syncs replay the web client's request with your session cookies. Copying
//! those cookies out of developer tools is the hardest connect step in the
//! product. Here the app opens the site's login page, the owner logs in as they
//! would in Safari, and once the cookies that mean "logged in" exist the app
//! returns the site's cookie jar to the page that asked. That page posts it to
//! the box's ordinary connect route, so the box never learns which door the
//! credential came through.
//!
//! On the Mac the page opens in the Browser pane beside the app's view
//! (`browser_host.rs`), and the session also stays in that browser's jar, so the
//! assistant browsing the site afterwards is logged in. Other desktops open an
//! incognito window of their own, whose cookies are gone when it closes.
//!
//! Neither has IPC. The app's capabilities grant nothing to a remote origin on
//! the Mac (`capabilities/mac.json` has no `remote`), and default.json's remote
//! grant is localhost only, so x.com in either cannot reach a single app command.
//!
//! Measured in agents/record/webkit-agent-spike.md: X's `auth_token`/`ct0` read
//! out of a WKWebView's store after a real login, and the X sync ran on them.

use std::time::Duration;
#[cfg(not(target_os = "macos"))]
use std::time::Instant;

use serde::Serialize;
use tauri::AppHandle;
#[cfg(not(target_os = "macos"))]
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

/// How long the window waits for a login before giving up. Long enough for a
/// password manager, a 2FA code and a slow SMS.
const LOGIN_TIMEOUT: Duration = Duration::from_secs(15 * 60);

/// Safari's own user agent. A bare WKWebView omits `Version/… Safari/…`, and
/// some sites treat that as an embedded browser and degrade the login page.
#[cfg(not(target_os = "macos"))]
const SAFARI_UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 \
                         (KHTML, like Gecko) Version/26.0 Safari/605.1.15";

/// The cookies a request to `page` carries, from every cookie in a webview's
/// store. Tauri's `cookies_for_url` keeps only cookies whose domain equals the
/// page's host exactly, so Instagram's `.instagram.com` session never matched
/// its login page on www.instagram.com and the login waited forever. A cookie
/// for a parent domain belongs to its subdomains, as in any browser.
pub(crate) fn jar_for(all: Vec<tauri::webview::Cookie<'static>>, page: &tauri::Url) -> Vec<tauri::webview::Cookie<'static>> {
    let host = page.host_str().unwrap_or("").to_ascii_lowercase();
    all.into_iter()
        .filter(|c| {
            let domain = c.domain().unwrap_or("").trim_start_matches('.').to_ascii_lowercase();
            let on_host = !domain.is_empty() && (host == domain || host.ends_with(&format!(".{domain}")));
            let path_ok = page.path().starts_with(c.path().unwrap_or("/"));
            on_host && path_ok && (page.scheme() == "https" || !c.secure().unwrap_or(false))
        })
        .collect()
}

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
    // On the Mac the login opens in the Browser pane beside the app's view, and
    // the session stays in the browser's own jar.
    #[cfg(target_os = "macos")]
    {
        let _ = (&source_id, &title);
        let jar = crate::browser_host::login(&app, &url, &cookies, LOGIN_TIMEOUT).await?;
        return Ok(jar.into_iter().map(|(name, value)| JarCookie { name, value }).collect());
    }
    // Other desktops: a login window of its own.
    #[cfg(not(target_os = "macos"))]
    {
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
            // otherwise (tauri docs for `cookies`).
            let jar = match win.cookies() {
                Ok(all) => jar_for(all, &page),
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
}

// ─── The Browser pane (macOS) ───────────────────────────────────────────────
//
// The UI's Browser tab drives the native view in `browser_host.rs`: it asks for
// a page, reports where its pane sits as layout changes, and forwards its
// toolbar. Other desktops have no pane yet, and say so.

#[cfg(not(target_os = "macos"))]
const NO_PANE: &str = "The in-app browser is on the Mac for now.";

/// Show `url` in the Browser pane (the tab is already open; nothing announced).
#[tauri::command]
pub async fn browser_pane_open(app: AppHandle, url: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let page: tauri::Url = url.parse().map_err(|_| format!("`{url}` is not a URL"))?;
        if !matches!(page.scheme(), "https" | "http") {
            return Err("Only http and https pages can be opened.".into());
        }
        return crate::browser_host::show(&app, &page, false).await.map(|_| ());
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (app, url);
        Err(NO_PANE.into())
    }
}

/// Where the Browser pane is on screen, in the main window's points, or that
/// it is not on screen at all (tab hidden, a modal open, the window too narrow).
#[tauri::command]
pub async fn browser_pane_bounds(app: AppHandle, x: f64, y: f64, width: f64, height: f64, visible: bool) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        return crate::browser_host::set_bounds(&app, x, y, width, height, visible).await;
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (app, x, y, width, height, visible);
        Err(NO_PANE.into())
    }
}

/// The Browser tab's bar while the assistant drives: `take` control, `resume`,
/// `done` or `decline` a handoff, or `stop`.
#[tauri::command]
pub async fn browser_pane_agent(app: AppHandle, action: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        return crate::browser_host::control(&app, &action);
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (app, action);
        Err(NO_PANE.into())
    }
}

/// The Browser tab's toolbar: `back`, `forward` or `reload`.
#[tauri::command]
pub async fn browser_pane_go(app: AppHandle, action: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        return crate::browser_host::go(&app, &action).await;
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (app, action);
        Err(NO_PANE.into())
    }
}

#[cfg(test)]
mod tests {
    use super::jar_for;
    use tauri::webview::Cookie;

    fn cookie(name: &str, domain: &str, secure: bool) -> Cookie<'static> {
        Cookie::build((name.to_string(), "v".to_string())).domain(domain.to_string()).path("/").secure(secure).build()
    }

    #[test]
    fn a_parent_domain_cookie_belongs_to_its_subdomain() {
        let page: tauri::Url = "https://www.instagram.com/accounts/login/".parse().unwrap();
        let all = vec![
            cookie("sessionid", ".instagram.com", true),
            cookie("csrftoken", "instagram.com", true),
            cookie("mid", "www.instagram.com", false),
            cookie("auth_token", ".x.com", true),
            cookie("evil", ".notinstagram.com", false),
        ];
        let names: Vec<String> = jar_for(all, &page).iter().map(|c| c.name().to_string()).collect();
        assert_eq!(names, ["sessionid", "csrftoken", "mid"]);
    }

    #[test]
    fn a_secure_cookie_stays_off_plain_http() {
        let page: tauri::Url = "http://x.com/".parse().unwrap();
        let names: Vec<String> = jar_for(vec![cookie("auth_token", ".x.com", true), cookie("lang", "x.com", false)], &page)
            .iter()
            .map(|c| c.name().to_string())
            .collect();
        assert_eq!(names, ["lang"]);
    }
}
