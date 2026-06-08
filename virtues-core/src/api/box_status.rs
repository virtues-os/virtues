//! Box health status — one source of truth, several surfaces: the `virtues
//! status` CLI, the session-authed `GET /api/box/status` (full identity detail
//! for the phone app), and the public-on-LAN `GET /api/box/health` (the boot
//! state machine as flat gates + the inference resolution report, secrets
//! stripped) that the first-run web page and the appliance screen poll.
//! Composability: the box, a DIY server, the CLI, and the app all report
//! identical health from this one computation.

use anyhow::Result;
use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use serde::Serialize;
use sqlx::PgPool;

use crate::search::model_cache::{self, ModelSource};
use crate::server::webhook::AppState;
use crate::wireguard::box_secrets;

#[derive(Debug, Clone, Serialize)]
pub struct BoxStatus {
    /// True once the box has its full identity (CA + WG keypair + rendezvous).
    pub ready: bool,
    pub identity: IdentityStatus,
    pub subscription: SubscriptionStatus,
    pub devices: DeviceStatus,
}

#[derive(Debug, Clone, Serialize)]
pub struct IdentityStatus {
    pub server_ca: bool,
    pub wg_server_keypair: bool,
    pub wg_public_key: Option<String>,
    pub rendezvous: bool,
    pub publish_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SubscriptionStatus {
    /// Linked to a subscription: a `billing_token` is present (box↔Atlas).
    pub billing_token: bool,
    /// A usage `bearer` has been minted (box↔virtues-api) — i.e. AI is ready.
    /// Distinct from `billing_token`: linked boxes mint the bearer lazily.
    pub bearer: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeviceStatus {
    pub paired_wg: i64,
}

/// Compute the box's health snapshot. Shared by the CLI (`virtues status`) and
/// the HTTP endpoint.
pub async fn compute_status(pool: &PgPool) -> Result<BoxStatus> {
    let server_ca = box_secrets::get(pool, "wg_ca").await?.is_some();
    let rdv = box_secrets::get(pool, "rendezvous_identity").await?;
    let wg_key = box_secrets::get(pool, "wg_server_keypair").await?;
    let billing_token = crate::virtues_api::renew::has_billing_token(pool)
        .await
        .unwrap_or(false);
    let bearer = crate::virtues_api::renew::current_bearer(pool)
        .await
        .unwrap_or(None)
        .is_some();
    let paired_wg: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM credentials WHERE (metadata->'wg') IS NOT NULL AND status = 'active'",
    )
    .fetch_one(pool)
    .await
    .unwrap_or(0);

    let wg_public_key = wg_key.as_ref().and_then(|(_, m)| {
        m.get("public_key").and_then(|v| v.as_str()).map(String::from)
    });
    let publish_id = rdv.as_ref().and_then(|(_, m)| {
        m.get("publish_id").and_then(|v| v.as_str()).map(String::from)
    });

    Ok(BoxStatus {
        ready: server_ca && rdv.is_some() && wg_key.is_some(),
        identity: IdentityStatus {
            server_ca,
            wg_server_keypair: wg_key.is_some(),
            wg_public_key,
            rendezvous: rdv.is_some(),
            publish_id,
        },
        subscription: SubscriptionStatus { billing_token, bearer },
        devices: DeviceStatus { paired_wg },
    })
}

/// `GET /api/box/status` — box health for the phone app's status screen.
pub async fn box_status_handler(State(state): State<AppState>) -> impl IntoResponse {
    match compute_status(state.db.pool()).await {
        Ok(status) => (StatusCode::OK, Json(status)).into_response(),
        Err(e) => {
            tracing::warn!(error = %e, "box status failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response()
        }
    }
}

// ─── Public health (first-run web page + appliance screen) ──────────────────

/// The boot state machine as flat booleans. Each gate is a stage a fresh box
/// passes through on the way to fully usable; the first-run UI renders these in
/// order. Non-sensitive: counts and booleans only, never identity secrets.
#[derive(Debug, Clone, Serialize)]
pub struct ReadinessGates {
    /// DB reachable + migrations applied. True if we can answer at all.
    pub infra: bool,
    /// CA + rendezvous + WG keypair minted (== `BoxStatus::ready`).
    pub identity: bool,
    /// Linked to a Virtues subscription: a `billing_token` is present
    /// (box↔Atlas). This is "claimed" — ownership is the billing relationship.
    pub linked: bool,
    /// A usage bearer has been minted (box↔virtues-api) — AI is ready. Linked
    /// boxes mint this lazily, so it can lag `linked` until the first AI call.
    pub entitled: bool,
    /// At least one device has paired.
    pub paired: bool,
}

/// One model's resolution, public-safe (no on-disk paths).
#[derive(Debug, Clone, Serialize)]
pub struct ModelInfo {
    pub name: String,
    /// "baked" (present on disk now) or "download" (fetched on first use).
    pub source: String,
}

/// The inference stack's hardware resolution, surfaced for the first-run screen
/// ("CUDA · fp16 · models baked"). Mirrors `virtues doctor`.
#[derive(Debug, Clone, Serialize)]
pub struct InferenceStatus {
    pub accelerator: String,
    pub precision: String,
    pub cuda_compiled: bool,
    /// True when every model is baked on disk (appliance); false when one or
    /// more would download on first use (typical DIY).
    pub models_baked: bool,
    pub models: Vec<ModelInfo>,
}

/// Public, non-sensitive box health — the boot gates plus the inference report.
#[derive(Debug, Clone, Serialize)]
pub struct BoxHealth {
    /// True once the box is fully usable (identity + linked + paired).
    pub ready: bool,
    pub gates: ReadinessGates,
    pub inference: InferenceStatus,
}

fn inference_status() -> InferenceStatus {
    let r = model_cache::resolution_report();
    let models: Vec<ModelInfo> = r
        .models
        .iter()
        .map(|m| ModelInfo {
            name: m.name.to_string(),
            source: match m.source {
                ModelSource::Baked(_) => "baked".to_string(),
                ModelSource::Download => "download".to_string(),
            },
        })
        .collect();
    let models_baked = !models.is_empty() && models.iter().all(|m| m.source == "baked");
    InferenceStatus {
        accelerator: r.accelerator.to_string(),
        precision: r.precision.to_string(),
        cuda_compiled: r.cuda_compiled,
        models_baked,
        models,
    }
}

/// Compute the public health snapshot: boot gates + inference resolution. Reuses
/// `compute_status` so the two endpoints can never disagree about identity.
pub async fn compute_health(pool: &PgPool) -> Result<BoxHealth> {
    let s = compute_status(pool).await?;
    let gates = ReadinessGates {
        infra: true,
        identity: s.ready,
        linked: s.subscription.billing_token,
        entitled: s.subscription.bearer,
        paired: s.devices.paired_wg > 0,
    };
    let ready = gates.identity && gates.linked && gates.paired;
    Ok(BoxHealth {
        ready,
        gates,
        inference: inference_status(),
    })
}

/// `GET /api/box/health` — public, LAN-reachable, non-sensitive. The first-run
/// web page and the appliance screen poll this before any owner session exists.
pub async fn box_health_handler(State(state): State<AppState>) -> impl IntoResponse {
    match compute_health(state.db.pool()).await {
        Ok(health) => (StatusCode::OK, Json(health)).into_response(),
        Err(e) => {
            tracing::warn!(error = %e, "box health failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response()
        }
    }
}
