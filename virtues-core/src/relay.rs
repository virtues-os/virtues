//! Box-side iroh reach integration.
//!
//! The box is an **iroh `Endpoint`** whose Ed25519 `EndpointId` *is* its identity
//! (mutual-key auth, no CA). It serves the box's existing axum app over iroh
//! ([`virtues_iroh::serve`]) and, in prod, homes on our relay so any paired
//! device reaches it by `EndpointId` — LAN-direct, hole-punched, or via the relay
//! — with **no public inbound port**. The plain-HTTP `:8000` listener stays for
//! LAN/loopback and the desktop `:7117` helper.
//!
//! Security is layered: iroh enforces an [`AllowPolicy`](virtues_iroh::AllowPolicy)
//! over paired-device EndpointIds at the transport; the app-layer bearer/cookie
//! remains the authorization keystone on top.
//!
//! Config (env for now; atlas `/relay/config` → relay_url and DB-backed allowlist
//! land in Steps 4/5):
//! - `VIRTUES_RELAY_URL`   — our relay, e.g. `https://relay.virtues.ch`. Unset =
//!   dev mode (n0 relays + discovery).
//! - `VIRTUES_IROH_ALLOW`  — comma-separated device EndpointIds allowed to connect
//!   (interim until pairing populates `app_device.node_id`).

use anyhow::{Context, Result};
use sqlx::PgPool;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock, RwLock};
use virtues_iroh::{build_endpoint, serve, AllowPolicy, EndpointId, RelayUrl, SecretKey, StaticAllow};

/// `box_secrets` key holding this box's persistent iroh secret key (hex of the
/// 32-byte seed) — so the box keeps a stable `EndpointId` across restarts.
const BOX_IROH_SECRET: &str = "iroh_secret_key";

/// Process-wide "iroh endpoint is bound and homed on the relay" flag. Read by
/// pairing to advertise the box's reach ticket only when it's actually up.
static ENDPOINT_UP: OnceLock<Arc<AtomicBool>> = OnceLock::new();
/// This box's own `EndpointId` (hex), set once the endpoint binds — handed to
/// devices at pairing so they can dial by it.
static BOX_ENDPOINT_ID: OnceLock<RwLock<Option<String>>> = OnceLock::new();
/// The relay URL this box homed on (if any) — the other half of the reach ticket.
static BOX_RELAY_URL: OnceLock<RwLock<Option<String>>> = OnceLock::new();

fn endpoint_up_flag() -> Arc<AtomicBool> {
    ENDPOINT_UP.get_or_init(|| Arc::new(AtomicBool::new(false))).clone()
}

/// Whether the box's iroh endpoint is bound (and reachable). Kept under the old
/// name so pairing call sites are unchanged; Step 7 renames to `endpoint_up`.
pub fn is_relay_registered() -> bool {
    ENDPOINT_UP.get().map(|f| f.load(Ordering::Relaxed)).unwrap_or(false)
}

/// This box's iroh `EndpointId` (hex), once bound — for the pairing reach ticket.
pub fn box_endpoint_id() -> Option<String> {
    BOX_ENDPOINT_ID.get().and_then(|c| c.read().ok().and_then(|g| g.clone()))
}

/// The relay URL this box homed on, if any — the other half of the reach ticket.
pub fn box_relay_url() -> Option<String> {
    BOX_RELAY_URL.get().and_then(|c| c.read().ok().and_then(|g| g.clone()))
}

/// Spawn the iroh reach subsystem: bind the endpoint and serve `app` over it.
/// `app` is the box's fully-built axum `Router` (cloned from the one served on
/// `:8000`). No-op-safe: logs and exits on fatal setup errors (box stays LAN-only).
pub fn maybe_spawn(db: PgPool, app: axum::Router) {
    tokio::spawn(async move {
        let secret = match load_or_create_secret(&db).await {
            Ok(s) => s,
            Err(e) => {
                tracing::error!(error = %format!("{e:#}"), "iroh: failed to load/create secret key — reach disabled");
                return;
            }
        };
        let relay_url = resolve_relay_url(&db).await;
        if let Some(u) = &relay_url {
            BOX_RELAY_URL.get_or_init(|| RwLock::new(None));
            if let Some(cell) = BOX_RELAY_URL.get() {
                if let Ok(mut g) = cell.write() {
                    *g = Some(u.to_string());
                }
            }
        }
        let endpoint = match build_endpoint(secret, relay_url.clone()).await {
            Ok(e) => e,
            Err(e) => {
                tracing::error!(error = %format!("{e:#}"), "iroh: failed to bind endpoint — reach disabled");
                return;
            }
        };
        let eid = endpoint.id().to_string();
        BOX_ENDPOINT_ID.get_or_init(|| RwLock::new(None));
        if let Some(cell) = BOX_ENDPOINT_ID.get() {
            if let Ok(mut g) = cell.write() {
                *g = Some(eid.clone());
            }
        }
        endpoint_up_flag().store(true, Ordering::Relaxed);
        match &relay_url {
            Some(u) => tracing::info!(endpoint_id = %eid, relay = %u, "iroh endpoint bound; box reachable by EndpointId via our relay"),
            None => tracing::info!(endpoint_id = %eid, "iroh endpoint bound (dev: n0 relays + discovery)"),
        }

        let allow = load_allowlist(&db).await;
        // Register this box + its paired devices with atlas BEFORE homing on the
        // relay, so the relay's active-sub gate already recognises the box when it
        // connects (best-effort; retried on each pairing change).
        report_endpoints(&db).await;
        // Serve the existing axum app over iroh. Hold the router handle for the
        // life of the process (dropping it aborts the accept loop).
        let _router = serve(endpoint, app, allow);
        std::future::pending::<()>().await;
    });
}

/// Resolve our relay URL: the atlas-provisioned config (stored at claim/link)
/// first, then the `VIRTUES_RELAY_URL` env fallback (dev/manual). `None` → dev
/// mode (n0 relays + discovery).
async fn resolve_relay_url(db: &PgPool) -> Option<RelayUrl> {
    if let Ok(Some(rc)) = crate::virtues_api::relay::load(db).await {
        if let Ok(u) = RelayUrl::from_str(&rc.relay_url) {
            return Some(u);
        }
    }
    let raw = std::env::var("VIRTUES_RELAY_URL").ok().filter(|s| !s.is_empty())?;
    match RelayUrl::from_str(&raw) {
        Ok(u) => Some(u),
        Err(e) => {
            tracing::warn!(error = %e, url = %raw, "VIRTUES_RELAY_URL is not a valid relay URL — dev mode");
            None
        }
    }
}

/// Load the box's persistent iroh secret key from `box_secrets`, generating and
/// persisting a fresh one on first boot so the `EndpointId` is stable.
async fn load_or_create_secret(db: &PgPool) -> Result<SecretKey> {
    if let Some((hex_seed, _meta)) = crate::box_secrets::get(db, BOX_IROH_SECRET).await? {
        let bytes = hex::decode(hex_seed.trim()).context("decode stored iroh secret")?;
        let arr: [u8; 32] = bytes.as_slice().try_into().context("iroh secret is not 32 bytes")?;
        return Ok(SecretKey::from_bytes(&arr));
    }
    let mut seed = [0u8; 32];
    {
        use rand::RngCore;
        rand::rng().fill_bytes(&mut seed);
    }
    let secret = SecretKey::from_bytes(&seed);
    crate::box_secrets::put(db, BOX_IROH_SECRET, &hex::encode(seed), &serde_json::json!({}))
        .await
        .context("persist iroh secret key")?;
    tracing::info!("iroh: generated + persisted new box secret key");
    Ok(secret)
}

/// The live allowlist handle, so pairing/revocation can hot-swap it without
/// restarting the endpoint (both this and the `Arc<dyn AllowPolicy>` handed to
/// `serve` share the same inner set).
static ALLOW: OnceLock<StaticAllow> = OnceLock::new();

/// Non-revoked device EndpointIds from the DB, plus any `VIRTUES_IROH_ALLOW`
/// (dev/manual). The box's own EndpointId is implicitly trusted (it never dials
/// itself) so it's not included.
async fn allowed_ids(db: &PgPool) -> Vec<EndpointId> {
    let mut ids = Vec::new();
    let rows: Result<Vec<(String,)>, _> = sqlx::query_as(
        "SELECT node_id FROM app_device WHERE node_id IS NOT NULL AND revoked_at IS NULL",
    )
    .fetch_all(db)
    .await;
    match rows {
        Ok(rows) => {
            for (nid,) in rows {
                if let Ok(id) = EndpointId::from_str(nid.trim()) {
                    ids.push(id);
                }
            }
        }
        Err(e) => tracing::warn!(error = %e, "iroh allowlist DB query failed"),
    }
    if let Ok(raw) = std::env::var("VIRTUES_IROH_ALLOW") {
        for tok in raw.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            match EndpointId::from_str(tok) {
                Ok(id) => ids.push(id),
                Err(e) => tracing::warn!(error = %e, token = %tok, "VIRTUES_IROH_ALLOW: skipping invalid EndpointId"),
            }
        }
    }
    ids
}

/// Build + install the live allowlist for `serve`.
async fn load_allowlist(db: &PgPool) -> Arc<dyn AllowPolicy> {
    let ids = allowed_ids(db).await;
    tracing::info!(count = ids.len(), "iroh allowlist loaded");
    let allow = ALLOW.get_or_init(StaticAllow::default).clone();
    allow.replace(ids);
    Arc::new(allow)
}

/// Report this box's EndpointId + its paired devices' EndpointIds to atlas so the
/// relay's active-sub gate (`/relay/authorize`) recognises them. Best-effort.
pub async fn report_endpoints(db: &PgPool) {
    report_endpoints_with(db, &allowed_ids(db).await).await;
}

/// As [`report_endpoints`], but reusing an allowlist already read from the DB so
/// callers that just fetched it (e.g. [`after_pairing_change`]) don't query twice.
async fn report_endpoints_with(db: &PgPool, device_ids: &[EndpointId]) {
    let Some(box_id) = box_endpoint_id() else { return };
    let Ok(Some(api_key)) = crate::virtues_api::renew::read_api_key(db).await else { return };
    let mut endpoint_ids = vec![box_id];
    for id in device_ids {
        endpoint_ids.push(id.to_string());
    }
    let http = crate::http_client::virtues_api_client();
    let atlas = crate::virtues_api::atlas_url();
    let resp = http
        .post(format!("{}/iroh/register", atlas.trim_end_matches('/')))
        .json(&serde_json::json!({ "api_key": api_key, "endpoint_ids": endpoint_ids }))
        .send()
        .await;
    match resp {
        Ok(r) if r.status().is_success() => tracing::debug!("iroh endpoints registered with atlas"),
        Ok(r) => tracing::debug!(status = %r.status(), "iroh endpoint register non-success"),
        Err(e) => tracing::debug!(error = %e, "iroh endpoint register skipped"),
    }
}

/// Fire-and-forget: refresh the local allowlist and re-report endpoints to atlas
/// after a pairing or revocation. Non-blocking so pairing handlers don't wait.
pub fn after_pairing_change(db: PgPool) {
    tokio::spawn(async move {
        // Read the allowlist once, then use it for BOTH the local hot-swap and the
        // atlas report (they need the same set — no reason to query twice).
        let ids = allowed_ids(&db).await;
        if let Some(allow) = ALLOW.get() {
            tracing::debug!(count = ids.len(), "iroh allowlist refreshed");
            allow.replace(ids.clone());
        }
        report_endpoints_with(&db, &ids).await;
    });
}
