//! The loader's engine: an iroh endpoint in the visitor's browser.
//!
//! A shared link carries the sharer's door key and a page token after `#`,
//! which the browser never sends to a server. This dials that key through
//! the virtues relay, asks the door for the page, and hands it to the page
//! script. A browser cannot send UDP, so the endpoint is relay-only; the
//! connection is still end to end encrypted to the door's key, so a server
//! that does not hold the key cannot answer, virtues included.
//!
//! The wire protocol is `virtues-door`'s (`crates/virtues-door/src/protocol.rs`):
//! one JSON request line in, one JSON status line and the body out. The
//! connection stays open after the page loads, so a live page's queries
//! ([`query`]) reuse it.

use std::str::FromStr;
use std::time::Duration;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use std::cell::RefCell;

use iroh::endpoint::{presets, Connection};
use iroh::{Endpoint, EndpointAddr, EndpointId, RelayMode, RelayUrl};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen::{JsError, JsValue};

const ALPN: &[u8] = b"virtues/publish/1";
const RELAY_URL: &str = "https://relay.virtues.ch";
const MAX_PAGE_BYTES: usize = 8 * 1024 * 1024;

/// How long to wait for the door before calling the sharer's server offline.
/// The relay does not report an absent peer, so without this a visit to a
/// switched-off box waits forever.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(8);

#[wasm_bindgen(start)]
fn start() {
    console_error_panic_hook::set_once();
}

/// The open link: the endpoint, the connection to the door, and the token.
struct Session {
    _endpoint: Endpoint,
    conn: Connection,
    token: String,
}

thread_local! {
    static SESSION: RefCell<Option<Session>> = const { RefCell::new(None) };
}

/// The door key from a link: 43 characters of base64url, or 64 of hex.
fn parse_door(s: &str) -> Option<EndpointId> {
    let s = s.trim();
    if s.len() == 64 {
        return EndpointId::from_str(s).ok();
    }
    let bytes = URL_SAFE_NO_PAD.decode(s).ok()?;
    let arr: [u8; 32] = bytes.try_into().ok()?;
    EndpointId::from_bytes(&arr).ok()
}

#[derive(Deserialize)]
struct Head {
    status: String,
}

#[derive(Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
enum Outcome {
    Page { html: String, bytes: usize, connect_ms: f64, total_ms: f64 },
    /// The door answered: no page at this link (never shared, revoked, or
    /// expired; the door does not say which).
    Gone,
    /// The door did not answer in time: the sharer's server is off or
    /// unreachable.
    Offline,
    /// The link itself is malformed.
    BadLink,
}

fn err(e: impl std::fmt::Display) -> JsError {
    JsError::new(&e.to_string())
}

/// Fetch the page a link names. Resolves to `{ outcome, ... }`.
#[wasm_bindgen]
pub async fn open_link(door: String, token: String) -> Result<JsValue, JsError> {
    let outcome = fetch(&door, &token).await?;
    serde_wasm_bindgen::to_value(&outcome).map_err(err)
}

async fn fetch(door: &str, token: &str) -> Result<Outcome, JsError> {
    let t0 = js_sys::Date::now();
    let Some(id) = parse_door(door) else { return Ok(Outcome::BadLink) };
    if token.is_empty() || token.len() > 64 {
        return Ok(Outcome::BadLink);
    }
    let relay = RelayUrl::from_str(RELAY_URL).map_err(err)?;
    let endpoint = Endpoint::builder(presets::Minimal)
        .relay_mode(RelayMode::Custom(relay.clone().into()))
        .bind()
        .await
        .map_err(err)?;

    let addr = EndpointAddr::new(id).with_relay_url(relay);
    let conn = match n0_future::time::timeout(CONNECT_TIMEOUT, endpoint.connect(addr, ALPN)).await {
        Ok(Ok(conn)) => conn,
        _ => {
            endpoint.close().await;
            return Ok(Outcome::Offline);
        }
    };
    let t1 = js_sys::Date::now();

    let request = serde_json::json!({ "op": "page", "token": token });
    let Some(body) = ask(&conn, &request).await? else {
        conn.close(0u8.into(), b"done");
        endpoint.close().await;
        return Ok(Outcome::Gone);
    };
    // Kept open for the page's own queries, if it makes any.
    SESSION.with(|s| {
        *s.borrow_mut() = Some(Session { _endpoint: endpoint, conn, token: token.to_string() })
    });
    Ok(Outcome::Page {
        bytes: body.len(),
        html: String::from_utf8_lossy(&body).into_owned(),
        connect_ms: t1 - t0,
        total_ms: js_sys::Date::now() - t0,
    })
}

/// One request on its own stream: the body if the door said ok, `None` if
/// it said anything else.
async fn ask(conn: &Connection, request: &serde_json::Value) -> Result<Option<Vec<u8>>, JsError> {
    let line = request.to_string() + "\n";
    let (mut send, mut recv) = conn.open_bi().await.map_err(err)?;
    send.write_all(line.as_bytes()).await.map_err(err)?;
    send.finish().map_err(err)?;
    let raw = recv.read_to_end(MAX_PAGE_BYTES + 1024).await.map_err(err)?;
    let Some(newline) = raw.iter().position(|b| *b == b'\n') else {
        return Err(JsError::new("the door's answer had no status line"));
    };
    let head: Head = serde_json::from_slice(&raw[..newline]).map_err(err)?;
    if head.status != "ok" {
        return Ok(None);
    }
    Ok(Some(raw[newline + 1..].to_vec()))
}

/// Rows for one of the open page's queries, by key, as a JSON string; or
/// null when the server would not or could not answer.
#[wasm_bindgen]
pub async fn query(key: String) -> Result<JsValue, JsError> {
    let session = SESSION.with(|s| s.borrow().as_ref().map(|s| (s.conn.clone(), s.token.clone())));
    let Some((conn, token)) = session else { return Ok(JsValue::NULL) };
    let request = serde_json::json!({ "op": "query", "token": token, "key": key });
    match ask(&conn, &request).await? {
        Some(body) => Ok(JsValue::from_str(&String::from_utf8_lossy(&body))),
        None => Ok(JsValue::NULL),
    }
}
