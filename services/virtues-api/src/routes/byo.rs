//! The return leg of a sign-in made with the person's own app at a vendor.
//!
//!   GET /byo/spotify/callback?code=…&state=…   (or ?error=…&state=…)
//!
//! Spotify accepts a redirect URI only on HTTPS or a loopback address, and a
//! box is often neither, so the person registers this address in the app
//! they create. The vendor sends the browser here; this route sends it on to
//! the box, with the code and the box's own state. It keeps nothing and
//! calls nothing: the code is useless without the PKCE verifier, which only
//! the box holds (`virtues_helpers::auth::own_app`).
//!
//! `state` is base64url JSON `{ "r": <the box's callback>, "s": <the box's
//! signed state> }`. `r` passes the same check as the OAuth proxy's return
//! addresses, so this cannot be used to send a browser anywhere else.

use axum::{
    extract::Query,
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
    routing::get,
    Json, Router,
};
use base64::Engine;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;

use crate::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/byo/spotify/callback", get(bounce))
}

async fn bounce(Query(q): Query<HashMap<String, String>>) -> Response {
    let Some((return_url, box_state)) = q.get("state").and_then(|s| decode_state(s)) else {
        return (StatusCode::BAD_REQUEST, Json(json!({ "error": "missing or unreadable state" }))).into_response();
    };
    if !super::oauth::is_valid_return_url(&return_url) {
        return (StatusCode::BAD_REQUEST, Json(json!({ "error": "This sign-in did not start from your Virtues server. Start it again from Virtues." })))
            .into_response();
    }
    Redirect::to(&onward(&return_url, &box_state, &q)).into_response()
}

fn decode_state(raw: &str) -> Option<(String, String)> {
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(raw.trim_end_matches('=')).ok()?;
    let v: Value = serde_json::from_slice(&bytes).ok()?;
    Some((v.get("r")?.as_str()?.to_string(), v.get("s")?.as_str()?.to_string()))
}

/// The box's callback with the code (or the vendor's error) and its state.
fn onward(return_url: &str, box_state: &str, q: &HashMap<String, String>) -> String {
    let mut url = reqwest::Url::parse(return_url).expect("checked by is_valid_return_url");
    {
        let mut pairs = url.query_pairs_mut();
        match (q.get("code"), q.get("error")) {
            (Some(code), _) => pairs.append_pair("code", code),
            (None, Some(error)) => pairs.append_pair("error", error),
            (None, None) => pairs.append_pair("error", "no_code"),
        };
        pairs.append_pair("state", box_state);
    }
    url.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(r: &str, s: &str) -> String {
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(json!({ "r": r, "s": s }).to_string())
    }

    #[test]
    fn the_route_sits_beside_the_oauth_providers() {
        let _ = super::super::oauth::router().merge(router());
    }

    #[test]
    fn the_code_and_the_box_state_go_on_to_the_box() {
        let (r, s) = decode_state(&state("http://192.168.1.20:7117/oauth/callback?via=own_app", "sig.ned")).unwrap();
        let q = HashMap::from([("code".to_string(), "abc".to_string())]);
        let out = onward(&r, &s, &q);
        assert_eq!(out, "http://192.168.1.20:7117/oauth/callback?via=own_app&code=abc&state=sig.ned");

        let q = HashMap::from([("error".to_string(), "access_denied".to_string())]);
        assert!(onward(&r, &s, &q).contains("error=access_denied"));
    }

    #[test]
    fn a_state_that_does_not_decode_is_refused() {
        assert!(decode_state("not base64 !!").is_none());
        assert!(decode_state(&base64::engine::general_purpose::URL_SAFE_NO_PAD.encode("{}")).is_none());
    }

    #[test]
    fn only_a_private_address_or_a_listed_server_is_a_return_address() {
        assert!(super::super::oauth::is_valid_return_url("http://192.168.1.20:7117/oauth/callback"));
        assert!(!super::super::oauth::is_valid_return_url("https://attacker.example.com/steal"));
    }
}
