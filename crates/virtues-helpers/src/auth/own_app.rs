//! Sign-ins made with an app the person creates at the vendor themselves.
//!
//! Spotify lets an app serve only a few people unless the developer is a
//! large company, so one Virtues app cannot serve everyone. Instead each
//! person creates their own Spotify app and pastes its Client ID. The sign-in
//! is the Authorization Code flow with PKCE, which needs no client secret:
//!
//! 1. The box makes a PKCE verifier and keeps it, and sends the browser to
//!    the vendor's authorize page with its challenge.
//! 2. The vendor sends the browser to a fixed HTTPS address virtues-api runs
//!    (`/byo/<source>/callback`), because the vendor accepts only HTTPS or a
//!    loopback address and a box is often neither. That route only bounces
//!    the browser on to the box, with the code and the box's own state.
//! 3. The box exchanges the code with its verifier at the vendor's token
//!    endpoint, and refreshes the token there later. virtues-api never sees a
//!    token: it sees a code that is useless without the verifier.

use base64::Engine;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::error::{AuthError, Result};
use super::proxy::{client, proxy_url, transport_error, upstream_error, ProxyExchangeResponse};

/// One vendor whose sign-in uses the person's own app.
#[derive(Debug)]
pub struct OwnApp {
    pub source: &'static str,
    authorize_url: &'static str,
    token_url: &'static str,
    /// Space-separated, as the vendor expects.
    scopes: &'static str,
    /// The path on virtues-api that bounces the browser back to the box.
    return_path: &'static str,
}

pub const SPOTIFY: OwnApp = OwnApp {
    source: "spotify",
    authorize_url: "https://accounts.spotify.com/authorize",
    token_url: "https://accounts.spotify.com/api/token",
    scopes: "user-read-recently-played",
    return_path: "/byo/spotify/callback",
};

/// The own-app sign-in for a source, if it has one.
pub fn for_source(source_id: &str) -> Option<&'static OwnApp> {
    match source_id {
        "spotify" => Some(&SPOTIFY),
        _ => None,
    }
}

impl OwnApp {
    /// The redirect URI the person registers in their app: virtues-api's
    /// bounce route for this source.
    pub fn redirect_uri(&self) -> String {
        format!("{}{}", proxy_url(), self.return_path)
    }

    /// The vendor's authorize page for this sign-in.
    pub fn authorize_url(&self, client_id: &str, challenge: &str, state: &str) -> String {
        let mut url = reqwest::Url::parse(self.authorize_url).expect("static authorize url");
        url.query_pairs_mut()
            .append_pair("client_id", client_id)
            .append_pair("response_type", "code")
            .append_pair("redirect_uri", &self.redirect_uri())
            .append_pair("code_challenge_method", "S256")
            .append_pair("code_challenge", challenge)
            .append_pair("scope", self.scopes)
            .append_pair("state", state);
        url.to_string()
    }

    /// Exchange the code the vendor returned for tokens.
    pub async fn exchange_code(&self, client_id: &str, code: &str, verifier: &str) -> Result<ProxyExchangeResponse> {
        let redirect_uri = self.redirect_uri();
        let form = [
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", redirect_uri.as_str()),
            ("client_id", client_id),
            ("code_verifier", verifier),
        ];
        let v = self.post_token(&form).await?;
        grant(client_id, &v, None)
    }

    /// Refresh the access token. The vendor may return a new refresh token;
    /// when it does not, the old one is kept.
    pub async fn refresh(&self, client_id: &str, refresh_token: &str) -> Result<ProxyExchangeResponse> {
        let form = [("grant_type", "refresh_token"), ("refresh_token", refresh_token), ("client_id", client_id)];
        let v = self.post_token(&form).await?;
        grant(client_id, &v, Some(refresh_token))
    }

    async fn post_token(&self, form: &[(&str, &str)]) -> Result<Value> {
        let resp = client()
            .post(self.token_url)
            .form(form)
            .send()
            .await
            .map_err(|e| transport_error("token request failed", e))?;
        let status = resp.status().as_u16();
        let body = resp.text().await.map_err(|e| transport_error("token response unreadable", e))?;
        if !(200..300).contains(&status) {
            // A 4xx here (`invalid_grant`: the sign-in expired or was revoked)
            // reads as a refused grant, so the credential asks to reconnect.
            return Err(upstream_error(status, &body));
        }
        serde_json::from_str(&body).map_err(|e| AuthError::Proxy(format!("token response was not JSON: {e}")))
    }
}

/// The stored shape of a token answer: the tokens and the Client ID they
/// belong to, since refreshing needs it.
fn grant(client_id: &str, v: &Value, old_refresh: Option<&str>) -> Result<ProxyExchangeResponse> {
    let access = v
        .get("access_token")
        .and_then(Value::as_str)
        .ok_or_else(|| AuthError::Proxy("token response had no access_token".into()))?;
    let refresh = v.get("refresh_token").and_then(Value::as_str).filter(|s| !s.is_empty()).or(old_refresh);
    let scope = v.get("scope").and_then(Value::as_str);
    Ok(ProxyExchangeResponse {
        secrets: json!({ "access_token": access, "refresh_token": refresh, "client_id": client_id }),
        metadata: json!({ "granted_scopes": scope }),
        expires_in: v.get("expires_in").and_then(Value::as_i64),
        scopes: scope.map(|s| s.split(' ').map(String::from).collect()),
    })
}

/// A PKCE `(verifier, S256 challenge)` pair: 64 hex characters from two
/// random UUIDs, and base64url(sha256(verifier)) without padding (RFC 7636).
pub fn pkce_pair() -> (String, String) {
    let verifier = format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple());
    let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    (verifier, challenge)
}

/// The `state` sent to the vendor: where virtues-api should send the browser
/// back (`r`) and the box's own signed state (`s`), as base64url JSON.
/// virtues-api's bounce route reads it and checks `r` before redirecting.
pub fn bounce_state(return_url: &str, box_state: &str) -> String {
    let body = json!({ "r": return_url, "s": box_state }).to_string();
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_challenge_is_the_hash_of_the_verifier() {
        let (verifier, challenge) = pkce_pair();
        assert_eq!(verifier.len(), 64);
        let expected = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
        assert_eq!(challenge, expected);
        assert_ne!(pkce_pair().0, verifier);
    }

    #[test]
    fn the_authorize_url_asks_for_recent_plays_with_pkce() {
        let url = reqwest::Url::parse(&SPOTIFY.authorize_url("cid-1", "chal", "st")).unwrap();
        let q: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(url.host_str(), Some("accounts.spotify.com"));
        assert_eq!(q["client_id"], "cid-1");
        assert_eq!(q["code_challenge_method"], "S256");
        assert_eq!(q["scope"], "user-read-recently-played");
        assert!(q["redirect_uri"].ends_with("/byo/spotify/callback"), "{}", q["redirect_uri"]);
        assert_eq!(for_source("spotify").map(|a| a.source), Some("spotify"));
        assert!(for_source("strava").is_none());
    }

    #[test]
    fn a_refresh_without_a_new_refresh_token_keeps_the_old_one() {
        let g = grant("cid", &json!({ "access_token": "at2", "expires_in": 3600 }), Some("rt1")).unwrap();
        assert_eq!(g.secrets, json!({ "access_token": "at2", "refresh_token": "rt1", "client_id": "cid" }));
        assert_eq!(g.expires_in, Some(3600));
        let g = grant("cid", &json!({ "access_token": "at3", "refresh_token": "rt2" }), Some("rt1")).unwrap();
        assert_eq!(g.secrets["refresh_token"], "rt2");
    }

    #[test]
    fn the_bounce_state_carries_the_return_address_and_the_box_state() {
        let st = bounce_state("http://192.168.1.20:7117/oauth/callback?via=own_app", "signed.state");
        let raw = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(st).unwrap();
        let v: Value = serde_json::from_slice(&raw).unwrap();
        assert_eq!(v["r"], "http://192.168.1.20:7117/oauth/callback?via=own_app");
        assert_eq!(v["s"], "signed.state");
    }
}
