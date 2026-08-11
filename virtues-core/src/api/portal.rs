//! The captive portal — plain HTML, no JavaScript, server-rendered.
//!
//! This exists because `/provision`, the SvelteKit page it replaces, could not
//! be rendered by the browser that actually has to show it.
//!
//! **iOS's Captive Network Assistant is not Safari.** It is a stripped-down
//! WebKit that opens in a sheet, and our whole frontend is `adapter-static`
//! with a `200.html` fallback — no server-side rendering anywhere, so every
//! page is an empty document until ES modules load and a client router boots.
//! On hardware 2026-08-10 that produced a blank white sheet, and because our
//! own captive detection tells iOS the network *is* captive, the OS kept
//! forcing that blank sheet back open and would not let the owner reach Safari
//! to work around it. We built a trap and then locked the door.
//!
//! So: no modules, no router, no fetch, no framework. One `<form>`, a `<meta
//! http-equiv="refresh">` where progress needs reporting, and inline CSS. The
//! test for anything added here is whether it would work in a browser from
//! 2010, because a captive assistant is roughly that.
//!
//! **The response is sent BEFORE the join is attempted**, and that ordering is
//! the whole trick. The box has to drop its AP to join the owner's network
//! (AP+STA does not work on this radio), which kills the very connection the
//! form was submitted over. A handler that joins first and answers second is
//! answering into a socket that no longer exists — the owner sees a spinner
//! that never resolves and no explanation. Answering first means the page
//! saying "your phone is about to drop off this network, that's normal" is
//! already on their screen when it happens.
//!
//! Gates are `api::provision`'s, unchanged: AP subnet or loopback, and only
//! while the box is unclaimed. This is the same surface in a different suit.

use axum::{
    extract::{ConnectInfo, Query, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Response},
    Form,
};
use serde::Deserialize;
use std::net::SocketAddr;
use std::sync::Mutex;

use crate::server::AppState;

/// Outcome of the most recent join attempt, so `/portal/status` can report it
/// after the owner's browser has reconnected.
///
/// In memory and not the database: it is worthless after a reboot (the AP is
/// back and they are starting over anyway), and the box may be mid-restart when
/// it would otherwise be written.
static LAST_JOIN: Mutex<Option<JoinRecord>> = Mutex::new(None);

#[derive(Clone, Debug)]
struct JoinRecord {
    ssid: String,
    error: Option<String>,
}

fn record_join(ssid: &str, error: Option<String>) {
    if let Ok(mut g) = LAST_JOIN.lock() {
        *g = Some(JoinRecord { ssid: ssid.to_string(), error });
    }
}

fn last_join() -> Option<JoinRecord> {
    LAST_JOIN.lock().ok().and_then(|g| g.clone())
}

// ─── shell ──────────────────────────────────────────────────────────────────

/// Every page, wrapped the same way.
///
/// Inline CSS with a system font stack and nothing else — no webfont, no
/// stylesheet request, no icon. A captive sheet has no internet by definition,
/// so every external reference is a guaranteed broken request and a slower
/// paint. `viewport` is the one meta tag that matters: without it the sheet
/// renders at desktop width and the text is unreadable.
fn page(title: &str, body: &str, head_extra: &str) -> Html<String> {
    Html(format!(
        r#"<!doctype html>
<html lang="en"><head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title}</title>{head_extra}
<style>
*{{box-sizing:border-box}}
body{{margin:0;padding:28px 20px 40px;background:#0b0f14;color:#f4f1ea;
     font-family:-apple-system,BlinkMacSystemFont,"Segoe UI",system-ui,sans-serif;
     font-size:17px;line-height:1.5}}
.mark{{color:#54606c;font-size:20px;margin-bottom:22px}}
h1{{font-size:24px;font-weight:600;margin:0 0 8px;letter-spacing:-.01em}}
p{{color:#93a0ad;margin:0 0 22px}}
p.tight{{margin-bottom:12px}}
a.net,button.net{{display:flex;align-items:center;gap:12px;width:100%;text-align:left;
     background:#161b22;border:1px solid #232c37;border-radius:12px;padding:15px 16px;
     margin-bottom:9px;color:#f4f1ea;font-size:17px;text-decoration:none;
     font-family:inherit;-webkit-appearance:none;appearance:none}}
.bars{{font-family:ui-monospace,Menlo,monospace;color:#54606c;font-size:14px;
     letter-spacing:-1px;flex:none}}
.name{{overflow:hidden;text-overflow:ellipsis;white-space:nowrap}}
.lock{{margin-left:auto;color:#54606c;font-size:13px;flex:none}}
input[type=password],input[type=text]{{width:100%;background:#161b22;color:#f4f1ea;
     border:1px solid #232c37;border-radius:12px;padding:15px;font-size:17px;
     margin:0 0 16px;font-family:inherit}}
button.go{{width:100%;background:#2b6cff;color:#fff;border:0;border-radius:12px;
     padding:16px;font-size:17px;font-weight:600;font-family:inherit;
     -webkit-appearance:none;appearance:none}}
.err{{background:#2a1416;border:1px solid #5c2126;border-radius:12px;padding:13px 15px;
     margin:0 0 20px;color:#ffb4b4;font-size:15px}}
.ok{{color:#5fb07e}}
.back{{display:inline-block;margin-top:24px;color:#54606c;font-size:15px;
     text-decoration:none}}
.steps{{margin:22px 0 0;padding:0;list-style:none;color:#54606c;font-size:15px}}
.steps li{{padding-left:16px;position:relative;margin-bottom:7px}}
.steps li:before{{content:"·";position:absolute;left:4px}}
code{{font-family:ui-monospace,Menlo,monospace;color:#f4f1ea}}
</style></head><body>
<div class="mark">&#8756;</div>
{body}
</body></html>"#
    ))
}

/// Escape for TEXT content (between tags).
fn esc(s: &str) -> String {
    html_escape::encode_text(s).into_owned()
}

/// Escape for a double-quoted ATTRIBUTE value.
///
/// A separate function because `encode_text` leaves `"` alone — correct between
/// tags, and a break-out inside `value="…"`. An SSID goes into exactly such an
/// attribute on the password form, and an SSID is attacker-controlled: anyone
/// within radio range of the box can name a network, and the box renders every
/// name it can see. Caught by the test below, not by review.
fn esc_attr(s: &str) -> String {
    html_escape::encode_double_quoted_attribute(s).into_owned()
}

fn bars(signal: u8) -> &'static str {
    if signal >= 70 {
        "▂▄▆"
    } else if signal >= 40 {
        "▂▄&nbsp;"
    } else {
        "▂&nbsp;&nbsp;"
    }
}

// ─── routes ─────────────────────────────────────────────────────────────────

/// `GET /portal` — the network list.
pub async fn index_handler(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Response {
    if let Some(r) = crate::api::provision::refuse_portal(&state, &peer, &headers).await {
        return r;
    }

    // A failed attempt is the first thing the owner needs to see, because the
    // AP coming back is what returned them to this page.
    let banner = match last_join() {
        Some(JoinRecord { ssid, error: Some(e) }) => format!(
            r#"<div class="err"><b>Couldn't join {}</b><br>{}</div>"#,
            esc(&ssid),
            esc(&e)
        ),
        _ => String::new(),
    };

    let body = match crate::api::provision::scan_or_cached().await {
        Ok(nets) if !nets.is_empty() => {
            let rows: String = nets
                .iter()
                .map(|n| {
                    format!(
                        r#"<a class="net" href="/portal/network?ssid={}{}"><span class="bars">{}</span><span class="name">{}</span><span class="lock">{}</span></a>"#,
                        urlencoding::encode(&n.ssid),
                        if n.enterprise { "&e=1" } else { "" },
                        bars(n.signal),
                        esc(&n.ssid),
                        if n.enterprise { "work" } else if n.secured { "locked" } else { "open" }
                    )
                })
                .collect();
            format!(
                r#"{banner}<h1>Choose your Wi-Fi</h1>
<p>These are the networks <b>your box</b> can see from where it's plugged in — not the ones your phone can see.</p>
{rows}
<a class="back" href="/portal">Search again</a>"#
            )
        }
        Ok(_) => format!(
            r#"{banner}<h1>No networks found</h1>
<p>Your box couldn't see any Wi-Fi from where it's plugged in. Move it closer to your router, or connect it with an ethernet cable instead — that skips this step entirely.</p>
<a class="back" href="/portal">Search again</a>"#
        ),
        Err(e) => format!(
            r#"{banner}<h1>Couldn't scan</h1><div class="err">{}</div>
<a class="back" href="/portal">Try again</a>"#,
            esc(&e)
        ),
    };
    page("Connect your box", &body, "").into_response()
}

#[derive(Deserialize)]
pub struct NetworkQuery {
    ssid: String,
    /// `e=1`: 802.1X — render username + password, not password alone.
    #[serde(default)]
    e: Option<String>,
}

/// `GET /portal/network?ssid=…` — the password form.
pub async fn network_handler(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Query(q): Query<NetworkQuery>,
) -> Response {
    if let Some(r) = crate::api::provision::refuse_portal(&state, &peer, &headers).await {
        return r;
    }
    // Two escapings, because these are two different HTML contexts and only
    // one of them escapes quotes. See `esc_attr`.
    let ssid = esc(&q.ssid);
    let ssid_attr = esc_attr(&q.ssid);
    let enterprise = q.e.as_deref() == Some("1");
    // 802.1X: credential-per-user. Two fields, and the copy says whose
    // credentials — the network's operator issued them, not us.
    let identity_field = if enterprise {
        r#"<label for="identity">Username</label>
  <input type="text" name="identity" autocapitalize="off" autocorrect="off"
         autocomplete="username" placeholder="Your account for this network">
  <label for="psk">Password</label>"#
    } else {
        ""
    };
    // `autocomplete="current-password"` so iOS offers the saved password for
    // this very network — the one real advantage a captive sheet can still
    // offer, and free.
    let intro = if enterprise {
        "This network uses per-person sign-in. Enter the username and password its operator gave you - not the code on the box's screen."
    } else {
        "Enter the password for this network - your own Wi-Fi password, not the code on the box's screen."
    };
    let pwd_placeholder = if enterprise { "Account password" } else { "Wi-Fi password" };
    let body = format!(
        r#"<h1>{ssid}</h1>
<p class="tight">{intro}</p>
<form method="post" action="/portal/join">
  <input type="hidden" name="ssid" value="{ssid_attr}">
  {identity_field}
  <input type="password" name="psk" autocomplete="current-password"
         autocapitalize="off" autocorrect="off" placeholder="{pwd_placeholder}">
  <button class="go" type="submit">Join network</button>
</form>
<ul class="steps">
  <li>Your box turns off its setup network to join yours</li>
  <li>So this phone drops off it — that's normal</li>
  <li>Your phone returns to your own Wi-Fi on its own</li>
</ul>
<a class="back" href="/portal">&larr; Networks</a>"#
    );
    page("Wi-Fi password", &body, "").into_response()
}

#[derive(Deserialize)]
pub struct JoinForm {
    ssid: String,
    #[serde(default)]
    psk: String,
}

/// `POST /portal/join` — answer first, then join.
///
/// The join runs in a detached task and the response goes out immediately. See
/// the module docs: joining first would tear down the AP, and therefore this
/// TCP connection, before the reply could be written — leaving the owner
/// staring at a hung page at the exact moment they most need to be told what is
/// happening.
pub async fn join_handler(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Form(form): Form<JoinForm>,
) -> Response {
    if let Some(r) = crate::api::provision::refuse_portal(&state, &peer, &headers).await {
        return r;
    }
    if form.ssid.trim().is_empty() {
        return page("Connect your box", r#"<div class="err">Pick a network first.</div><a class="back" href="/portal">&larr; Networks</a>"#, "").into_response();
    }

    let ssid = form.ssid.clone();
    let psk = (!form.psk.is_empty()).then_some(form.psk.clone());
    tokio::spawn(async move {
        // A beat, so the response is on the wire before the radio is touched.
        tokio::time::sleep(std::time::Duration::from_millis(600)).await;
        let err = crate::api::provision::perform_join(&ssid, psk.as_deref()).await;
        record_join(&ssid, err);
    });

    // Meta-refresh, because there is no JS here. On failure the AP returns and
    // this lands on the status page with the reason. On success it simply never
    // loads — by then the phone is back on the owner's own network, which is
    // what the copy has just told them to expect.
    let body = format!(
        r#"<h1>Joining {}</h1>
<p>Your box is switching over now. Your phone will drop off its setup network in a moment — that's meant to happen, and your phone will go back to your normal Wi-Fi by itself.</p>
<p>When it's done, the box's screen shows a 6-digit code. Enter that in the Virtues app.</p>
<p class="tight"><b>If this page comes back</b>, the join didn't work and it'll say why.</p>"#,
        esc(&form.ssid)
    );
    page(
        "Joining…",
        &body,
        r#"<meta http-equiv="refresh" content="12;url=/portal/status">"#,
    )
    .into_response()
}

/// `GET /portal/status` — what happened, for a browser that made it back.
///
/// Only reachable when the AP is up, which on the current rules means the box
/// is offline and unclaimed — i.e. the join did not work. That makes this page
/// mostly a failure report, and it says so plainly rather than spinning.
pub async fn status_handler(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Response {
    if let Some(r) = crate::api::provision::refuse_portal(&state, &peer, &headers).await {
        return r;
    }
    let online = crate::cli::link::primary_ip().is_some();
    if online {
        let body = r#"<h1 class="ok">Your box is online</h1>
<p>Open the Virtues app and enter the 6-digit code shown on the box's screen.</p>
<p>You can leave this network now — your phone will do it on its own.</p>"#;
        return page("Connected", body, "").into_response();
    }
    match last_join() {
        Some(JoinRecord { ssid, error: Some(e) }) => {
            let body = format!(
                r#"<div class="err"><b>Couldn't join {}</b><br>{}</div>
<h1>Let's try again</h1>
<p>A wrong password is the usual reason. Pick the network again and re-enter it.</p>
<a class="back" href="/portal">&larr; Networks</a>"#,
                esc(&ssid),
                esc(&e)
            );
            page("Didn't connect", &body, "").into_response()
        }
        // Still switching: refresh rather than guess.
        _ => page(
            "Working…",
            r#"<h1>Still working</h1><p>Give it a few more seconds.</p>"#,
            r#"<meta http-equiv="refresh" content="6;url=/portal/status">"#,
        )
        .into_response(),
    }
}

/// `GET /portal/health` — is this portal reachable at all?
///
/// Deliberately trivial and ungated: it is what a support conversation can ask
/// someone to open when everything else is blank, and a 200 here separates "the
/// box is not answering" from "the page did not render".
pub async fn health_handler() -> Response {
    (StatusCode::OK, [(axum::http::header::CONTENT_TYPE, "text/plain")], "portal ok\n")
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rendered(body: &str, head: &str) -> String {
        let Html(s) = page("t", body, head);
        s
    }

    #[test]
    fn the_portal_never_contains_script() {
        // THE INVARIANT THIS FILE EXISTS FOR. The browser that has to render
        // this is iOS's Captive Network Assistant, which showed a blank white
        // sheet for the SvelteKit page this replaced — and which the OS would
        // not let the owner escape from. The moment anything here needs JS, it
        // is the same bug again.
        let html = rendered("<h1>hi</h1>", "");
        assert!(!html.contains("<script"), "portal must not need JavaScript");
        assert!(!html.contains("type=\"module\""));
    }

    #[test]
    fn the_portal_requests_nothing_from_the_network() {
        // A captive sheet has no internet by definition, so every external
        // reference is a guaranteed-failed request. Inline CSS, no webfont, no
        // icon, no CDN.
        let html = rendered("<h1>hi</h1>", "");
        assert!(!html.contains("http://"), "no absolute URLs: {html}");
        assert!(!html.contains("https://"));
        assert!(!html.contains("<link"), "no external stylesheet or icon");
    }

    #[test]
    fn it_is_readable_on_a_phone() {
        // Without this the sheet lays out at desktop width and the text is too
        // small to read, which is indistinguishable from "broken" to an owner.
        assert!(rendered("x", "").contains(r#"name="viewport""#));
    }

    #[test]
    fn an_ssid_cannot_inject_markup() {
        // An SSID is attacker-controlled: anyone within radio range of the box
        // can name a network. It is rendered into this page and into an href.
        let nasty = r#"<script>alert(1)</script>"#;
        let escaped = esc(nasty);
        assert!(!escaped.contains("<script"), "got {escaped}");
        assert!(escaped.contains("&lt;"));

        // Attribute context: quotes are the break-out character, and
        // `encode_text` deliberately leaves them alone — which is exactly why
        // the SSID in `value="…"` goes through `esc_attr`, not `esc`. This
        // test originally used `esc` there and failed, catching the hole.
        let quote_break = r#"" onmouseover="x"#;
        assert!(!esc_attr(quote_break).contains('"'), "quotes must not survive in attributes");
    }

    #[test]
    fn meta_refresh_is_how_progress_is_reported() {
        // The no-JS substitute for polling. If this ever disappears, the
        // "joining" page becomes a dead end whenever a join fails.
        let html = rendered("x", r#"<meta http-equiv="refresh" content="12;url=/portal/status">"#);
        assert!(html.contains("http-equiv=\"refresh\""));
    }

    #[test]
    fn signal_bars_degrade_with_strength() {
        assert_eq!(bars(90), "▂▄▆");
        assert_ne!(bars(50), bars(90));
        assert_ne!(bars(10), bars(50));
    }
}
