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

/// TTL for the `_acme-challenge` TXT record. Short — it exists only for the
/// minutes of a DNS-01 validation, and a short TTL lets a stale record clear fast.
const ACME_TXT_TTL: i64 = 60;
/// Max time to wait for the Route 53 change to reach `INSYNC` (propagated to all
/// of the zone's authoritative servers) before returning to the box. Bounds the
/// request; if it lags past this we return anyway (best-effort) — the box also
/// sleeps a propagation slack before telling the CA the challenge is ready.
const ACME_INSYNC_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(45);

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/relay/config", post(relay_config))
        .route("/relay/acme-challenge", post(relay_acme_challenge))
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
    // Mint the *current bucket's* token only. The box re-fetches each bucket; if
    // this account is later revoked/lapses, resolve_active_customer above fails
    // and we stop minting, so the box's token expires within ~2 buckets (the
    // relay accepts only current/previous). That's revocation without relay state.
    let token = virtues_protocol::relay::derive_token(
        &state.relay.secret,
        &sni,
        virtues_protocol::relay::current_bucket(),
    );

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

#[derive(Debug, Deserialize)]
struct AcmeChallengeBody {
    api_key: String,
    /// DNS-01 key-authorization digests to publish. One per authorization (apex
    /// only in v1 → exactly one; the field is a list so a future wildcard cert,
    /// which shares the TXT name, can publish both values as one RRset).
    values: Vec<String>,
}

/// `POST /relay/acme-challenge { api_key, values } -> 200`
///
/// Per-box-scoped DNS-01 TXT writer. The box runs ACME and holds its own key; its
/// **only** privileged need is writing `_acme-challenge.<sni>`, which it cannot do
/// itself (it has no Route 53 creds, by design). atlas does it on the box's behalf
/// — and derives the record **name** from the authenticated account, so a box can
/// only ever write *its own* challenge. This is the sandcats "authority writes the
/// TXT" model. The RRset is replaced wholesale with `values` (UPSERT).
async fn relay_acme_challenge(
    State(state): State<AppState>,
    Json(body): Json<AcmeChallengeBody>,
) -> axum::response::Response {
    let (_customer_id, account_id) = match resolve_active_customer(&state, &body.api_key).await {
        Ok(v) => v,
        Err(resp) => return resp,
    };

    let (Some(client), false) = (
        state.relay.route53.as_ref(),
        state.relay.route53_zone_id.is_empty(),
    ) else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({
                "error": "acme_dns_not_configured",
                "message": "DNS-01 challenge writing is not enabled on this deployment",
            })),
        )
            .into_response();
    };

    if body.values.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "no_values", "message": "values must be non-empty" })),
        )
            .into_response();
    }

    // Name is derived from the authenticated account — never taken from the
    // request — so a compromised/hostile box can only write its own record.
    let sni = format!("{}.{}", boxhash(&account_id), state.relay.base_domain);
    let name = format!("_acme-challenge.{sni}");

    match write_txt(client, &state.relay.route53_zone_id, &name, &body.values).await {
        Ok(()) => (StatusCode::OK, Json(json!({ "ok": true, "name": name }))).into_response(),
        Err(e) => {
            tracing::error!(error = %e, %name, "Route 53 TXT publish failed");
            (
                StatusCode::BAD_GATEWAY,
                Json(json!({ "error": "dns_publish_failed" })),
            )
                .into_response()
        }
    }
}

/// UPSERT the TXT RRset `name` = `values` in the hosted zone, then wait until the
/// change is `INSYNC` (or the timeout). Each TXT value is wrapped in double quotes
/// as Route 53 requires; ACME digests are base64url so contain no quotes to escape.
async fn write_txt(
    client: &aws_sdk_route53::Client,
    zone_id: &str,
    name: &str,
    values: &[String],
) -> anyhow::Result<()> {
    use aws_sdk_route53::types::{
        Change, ChangeAction, ChangeBatch, ChangeStatus, ResourceRecord, ResourceRecordSet, RrType,
    };

    let records = values
        .iter()
        .map(|v| ResourceRecord::builder().value(format!("\"{v}\"")).build())
        .collect::<Result<Vec<_>, _>>()?;
    let rrset = ResourceRecordSet::builder()
        .name(name)
        .r#type(RrType::Txt)
        .ttl(ACME_TXT_TTL)
        .set_resource_records(Some(records))
        .build()?;
    let change = Change::builder()
        .action(ChangeAction::Upsert)
        .resource_record_set(rrset)
        .build()?;
    let batch = ChangeBatch::builder().changes(change).build()?;

    let resp = client
        .change_resource_record_sets()
        .hosted_zone_id(zone_id)
        .change_batch(batch)
        .send()
        .await?;

    // Poll the change to INSYNC so we don't hand control back to the box (which
    // then tells the CA to validate) before the record is live on every
    // authoritative server. Best-effort past the timeout.
    let Some(change_id) = resp.change_info().map(|ci| ci.id().to_string()) else {
        return Ok(());
    };
    let deadline = tokio::time::Instant::now() + ACME_INSYNC_TIMEOUT;
    loop {
        let status = client.get_change().id(&change_id).send().await?;
        if matches!(status.change_info().map(|ci| ci.status()), Some(ChangeStatus::Insync)) {
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            tracing::warn!(%name, "Route 53 change not INSYNC within timeout; returning best-effort");
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
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
        // The token atlas mints (current bucket) must equal what the relay
        // re-derives for the same bucket.
        let secret = "relay-master-secret";
        let sni = format!("{}.virtues.ch", boxhash("acct_xyz"));
        let bucket = virtues_protocol::relay::current_bucket();
        let minted = virtues_protocol::relay::derive_token(secret, &sni, bucket);
        let verified = virtues_protocol::relay::derive_token(secret, &sni, bucket);
        assert_eq!(minted, verified);
    }
}
