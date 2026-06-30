//! Relay control plane (Option A): mint this box's per-SNI registration token.
//!
//! `POST /relay/config { api_key } -> { relay_addr, sni, token }`
//!
//! atlas holds the relay master secret and is the **only** minter. The box's
//! name is derived deterministically from its stable, opaque `account_id`, so a
//! reinstall/recovery that keeps the account keeps the same name (matches the
//! re-point-key recovery doctrine). The box never sees `RELAY_SECRET` — it only
//! receives its own derived token, so a compromised box can't mint another
//! tenant's name. The relay verifies with the same secret + `derive_token`, so
//! it stays stateless (no per-box table). See `docs/relay-control-plane.md`.

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::post,
    Router,
};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::routes::{credits::resolve_active_customer, AppState};

pub fn router() -> Router<AppState> {
    Router::new().route("/relay/config", post(relay_config))
}

#[derive(Debug, Deserialize)]
struct RelayConfigBody {
    api_key: String,
}

async fn relay_config(
    State(state): State<AppState>,
    Json(body): Json<RelayConfigBody>,
) -> axum::response::Response {
    // Auth + entitlement: a valid api_key on an active subscription. Relay reach
    // is a paid capability, so an inactive sub is refused here just like top-ups.
    let (_customer_id, account_id) = match resolve_active_customer(&state, &body.api_key).await {
        Ok(v) => v,
        Err(resp) => return resp,
    };

    if state.relay.secret.is_empty() || state.relay.control_addr.is_empty() {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({
                "error": "relay_not_configured",
                "message": "relay reachability is not enabled on this deployment",
            })),
        )
            .into_response();
    }

    let sni = format!("{}.{}", boxhash(&account_id), state.relay.base_domain);
    let token = virtues_protocol::relay::derive_token(&state.relay.secret, &sni);

    (
        StatusCode::OK,
        Json(json!({
            "relay_addr": state.relay.control_addr,
            "sni": sni,
            "token": token,
        })),
    )
        .into_response()
}

/// Deterministic, domain-separated box label from the stable `account_id`.
/// 20 hex chars (80 bits) — collision-safe at any realistic fleet size and not
/// reversible to the account id. Versioned prefix so the scheme can evolve.
fn boxhash(account_id: &str) -> String {
    let mut h = Sha256::new();
    h.update(b"virtues-boxhash:v1:");
    h.update(account_id.as_bytes());
    hex::encode(h.finalize())[..20].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boxhash_is_deterministic_and_shaped() {
        let a = boxhash("acct_123");
        assert_eq!(a, boxhash("acct_123"), "same account → same boxhash");
        assert_ne!(a, boxhash("acct_456"), "different account → different boxhash");
        assert_eq!(a.len(), 20);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn minted_token_matches_relay_verification() {
        // The token atlas mints must equal what the relay re-derives.
        let secret = "relay-master-secret";
        let sni = format!("{}.virtues.ch", boxhash("acct_xyz"));
        let minted = virtues_protocol::relay::derive_token(secret, &sni);
        let verified = virtues_protocol::relay::derive_token(secret, &sni);
        assert_eq!(minted, verified);
    }
}
