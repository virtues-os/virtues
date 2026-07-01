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

use crate::inference_report::{self, ModelSource};
use crate::server::webhook::AppState;
#[derive(Debug, Clone, Serialize)]
pub struct BoxStatus {
    /// True once the box can serve requests. In the iroh model this is always
    /// satisfiable — the box binds its iroh endpoint + serves `:8000` locally; no
    /// per-box cert to mint first. Kept for the setup state machine's `identity`
    /// gate.
    pub ready: bool,
    pub identity: IdentityStatus,
    pub subscription: SubscriptionStatus,
    pub devices: DeviceStatus,
}

#[derive(Debug, Clone, Serialize)]
pub struct IdentityStatus {
    /// Whether the box's iroh endpoint is bound and homed on the relay (i.e.
    /// reachable by EndpointId from off-LAN). Informational — LAN reach + local
    /// serving don't depend on it.
    pub endpoint_up: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct SubscriptionStatus {
    /// Linked to a subscription: a device `api_key` is present (box↔atlas↔
    /// virtues-api). In the linked model this is the only signal — the same key
    /// authenticates the proxy, and the wallet is credited server-side.
    pub linked: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeviceStatus {
    /// Active paired devices (any kind). Named `paired_wg` for API stability.
    pub paired_wg: i64,
}

/// Whether the box's iroh endpoint is up (bound + homed on the relay).
fn endpoint_up() -> bool {
    crate::relay::is_relay_registered()
}

/// Compute the box's health snapshot. Shared by the CLI (`virtues status`) and
/// the HTTP endpoint.
pub async fn compute_status(pool: &PgPool) -> Result<BoxStatus> {
    let linked = crate::virtues_api::renew::has_api_key(pool)
        .await
        .unwrap_or(false);
    let paired_wg: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM credentials WHERE device_id IS NOT NULL AND status = 'active'",
    )
    .fetch_one(pool)
    .await
    .unwrap_or(0);

    Ok(BoxStatus {
        ready: true,
        identity: IdentityStatus {
            endpoint_up: endpoint_up(),
        },
        subscription: SubscriptionStatus { linked },
        devices: DeviceStatus { paired_wg },
    })
}

/// `GET /api/box/status` — box health for the phone app's status screen.
///
/// Takes `AuthUser` explicitly (not just relying on the protected-route layer)
/// so this identity-bearing response — WG pubkey, billing state —
/// is never served unauthenticated even if the route's layer is ever
/// reordered. The phone reaches it with its device bearer over any transport.
pub async fn box_status_handler(
    _user: crate::middleware::auth::AuthUser,
    State(state): State<AppState>,
) -> impl IntoResponse {
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
    /// Box has a serving TLS identity (== `BoxStatus::ready`). Always satisfiable
    /// in the relay model via the self-signed bootstrap cert; `identity.tls_cert`
    /// carries the finer "ACME cert issued" signal.
    pub identity: bool,
    /// Linked to a Virtues subscription: a device `api_key` is present. This is
    /// "claimed" — ownership is the billing relationship, and the same key makes
    /// AI ready immediately (the wallet is funded server-side at link).
    pub linked: bool,
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

/// The inference stack's resolution, surfaced for the first-run screen.
/// Mirrors `virtues doctor`.
#[derive(Debug, Clone, Serialize)]
pub struct InferenceStatus {
    pub accelerator: String,
    pub precision: String,
    /// True when every GGUF is on disk (healthy install); false when one or
    /// more is missing and the installer needs a re-run.
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
    let r = inference_report::resolution_report();
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
        linked: s.subscription.linked,
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

// ─── Setup / onboarding state machine (docs/onboarding.md) ──────────────────
//
// One source of truth, three renderers: the phone wizard, the appliance
// panel, and `virtues status`. Two distinct lists by design:
//
//   * `setup` — the wizard's REQUIRED core. Ends early ("setup ≠ onboarding"):
//     claimed → account → on-network. Everything else is deferred. (The box
//     keeps its default `virtues.local` name — naming is cosmetic and reach is
//     WireGuard/SPKI + localhost, not mDNS, so there's no rename step.)
//   * `onboarding` — the dashboard's "next wins" checklist for the first
//     week: progressive, abandonable, never blocking.
//
// Every signal is DERIVED from existing state (vault, credentials, runs,
// net_check) — there is intentionally no "wizard progress" table, so the
// state survives reinstalls, restores, and out-of-band changes.

/// One step of setup or onboarding, public-safe (booleans + copy only).
#[derive(Debug, Clone, Serialize)]
pub struct SetupStep {
    pub id: &'static str,
    pub title: &'static str,
    pub done: bool,
    /// Optional one-line human detail (e.g. the reachability verdict).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// Optional machine-readable qualifier for steps with more than two
    /// states (today only `remote_access`: "ipv6_direct" | "byo" | a
    /// net-class string). The frontend treats it as cosmetic only — behavior
    /// keys off `done`, copy comes from `detail`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<&'static str>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SetupState {
    pub setup: Vec<SetupStep>,
    pub setup_complete: bool,
    pub onboarding: Vec<SetupStep>,
}

/// Compute the setup/onboarding state. Reuses [`compute_status`] for the
/// vault-backed signals so the wizard, panel, and CLI can never disagree
/// with `/api/box/status`.
pub async fn compute_setup_state(pool: &PgPool) -> Result<SetupState> {
    let s = compute_status(pool).await?;

    // Claimed = at least one device has paired (the pair token was consumed
    // by an owner's browser or phone). Ownership-by-proximity, see
    // docs/onboarding.md "trust on first boot".
    let claimed: i64 =
        sqlx::query_scalar("SELECT count(*) FROM app_device WHERE revoked_at IS NULL")
            .fetch_one(pool)
            .await
            .unwrap_or(0);

    // First source = an active cloud-source credential (OAuth etc.) — not a
    // paired device's self-credential, not the BYO-key pseudo-source, and not
    // the `virtues_api` billing credential (which is always present once the
    // box is subscribed and is NOT a user-connected data source).
    let first_source: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM credentials \
         WHERE status = 'active' AND device_id IS NULL \
           AND source_id NOT IN ($1, $2, $3)",
    )
    .bind("__device__")
    .bind(crate::api::settings_byo::BYO_SOURCE_ID)
    .bind(crate::virtues_api::renew::SOURCE_ID)
    .fetch_one(pool)
    .await
    .unwrap_or(0);

    // First device = a paired collector (phone/Mac) with its own credential.
    let first_device: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM credentials WHERE device_id IS NOT NULL AND status = 'active'",
    )
    .fetch_one(pool)
    .await
    .unwrap_or(0);

    // A paired phone, specifically (kind = 'mobile_app'). Distinct from
    // `first_device`, which counts ANY paired collector (incl. the Mac) — the
    // onboarding "Add your phone" step must not light up just because the Mac
    // collector paired.
    let first_phone: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM app_device \
         WHERE kind = 'mobile_app' AND revoked_at IS NULL",
    )
    .fetch_one(pool)
    .await
    .unwrap_or(0);

    // Chat history imported = the one-time chat_import action has at least one
    // successful run. Server-backed (not a client-local flag) so skipping it is
    // recoverable from the dashboard backlog and survives a refresh. The action
    // row's id is `action_chat_import` (see server::api::chat_import_upload).
    let chat_imported: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM app_action_runs \
         WHERE action_id = 'action_chat_import' AND status = 'success')",
    )
    .fetch_one(pool)
    .await
    .unwrap_or(false);

    // First sync = any action run has ever succeeded (data actually landed).
    let first_sync: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM app_action_runs WHERE status = 'success'",
    )
    .fetch_one(pool)
    .await
    .unwrap_or(0);

    // Narrative-identity reveal: ready once there is a non-empty portrait to show
    // (drafted by the generator or hand-written), generating while a draft run is
    // in flight. Anchored on content presence (not `updated_at`, which the writer
    // doesn't bump) so it's drift-free; the run row gives the in-progress state.
    let nid_ready: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM wiki_narrative_identity \
         WHERE content IS NOT NULL AND length(trim(content)) > 0)",
    )
    .fetch_one(pool)
    .await
    .unwrap_or(false);
    let nid_running: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM app_action_runs \
         WHERE action_id = 'action_narrative_identity_draft' AND status = 'running')",
    )
    .fetch_one(pool)
    .await
    .unwrap_or(false);

    // Tier 2: a living (cloud/OAuth) source that has actually synced — i.e. a
    // non-device, non-BYO credential with at least one successful run. Stronger
    // than `first_source` (which only means "connected"): this means data flows.
    let living_source: bool = sqlx::query_scalar(
        "SELECT EXISTS( \
           SELECT 1 FROM credentials c \
           JOIN app_actions a ON a.credential_id = c.id \
           JOIN app_action_runs r ON r.action_id = a.id AND r.status = 'success' \
           WHERE c.status = 'active' AND c.device_id IS NULL \
             AND c.source_id NOT IN ($1, $2, $3))",
    )
    .bind("__device__")
    .bind(crate::api::settings_byo::BYO_SOURCE_ID)
    .bind(crate::virtues_api::renew::SOURCE_ID)
    .fetch_one(pool)
    .await
    .unwrap_or(false);

    // Tier -1: the owner deliberately named at least one device (doorplate),
    // distinct from the auto-generated label.
    let device_named: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM app_device \
         WHERE named_at IS NOT NULL AND revoked_at IS NULL)",
    )
    .fetch_one(pool)
    .await
    .unwrap_or(false);

    // Tier 0/1: a paired device finished its initial backfill. The FDA gate for
    // the Mac (daemon running + Full Disk Access) is enforced client-side in the
    // CollectorPermissionCard, which polls getCollectorStatus() directly; this
    // derived step reflects that data has actually flowed for the device.
    let device_collecting: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM app_device \
         WHERE init_sync_completed_at IS NOT NULL AND revoked_at IS NULL)",
    )
    .fetch_one(pool)
    .await
    .unwrap_or(false);
    let device_sync_started: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM app_device \
         WHERE init_sync_started_at IS NOT NULL AND init_sync_completed_at IS NULL \
           AND revoked_at IS NULL)",
    )
    .fetch_one(pool)
    .await
    .unwrap_or(false);

    // Dev convenience: `make dev` sets VIRTUES_DEV_SKIP_SETUP=1 so the required
    // setup wizard is pre-satisfied and the browser lands straight in the app
    // shell. Off by default; meaningless (and never set) in prod.
    // Unset it (`make dev VIRTUES_DEV_SKIP_SETUP=`) to walk the real wizard.
    let dev_skip = std::env::var("VIRTUES_DEV_SKIP_SETUP")
        .map(|v| v == "1" || v == "true")
        .unwrap_or(false);

    let mut setup = vec![
        SetupStep {
            id: "claimed",
            title: "Box claimed",
            done: claimed > 0,
            detail: None,
            kind: None,
        },
        SetupStep {
            id: "account",
            title: "Virtues account",
            done: s.subscription.linked,
            detail: None,
            kind: None,
        },
        SetupStep {
            id: "network",
            title: "On your network",
            done: crate::cli::link::primary_ip().is_some(),
            detail: None,
            kind: None,
        },
    ];
    if dev_skip {
        for step in &mut setup {
            step.done = true;
        }
    }
    // `setup_complete` gates the redirect into the app shell, so it must reflect
    // only the user-completable core. Positive allow-list (not a deny-list) so a
    // newly-added step is non-gating by default — you opt a step INTO blocking
    // the gate, never accidentally out of it. `network` ("on your network" =
    // `primary_ip().is_some()`) is deliberately absent: it's an informational
    // weather-report the wizard renders but the user can't "do", and it flips
    // false on any transient LAN blip, which previously bounced a fully-set-up
    // user back into /setup.
    const REQUIRED_SETUP_STEPS: &[&str] = &["claimed", "account"];
    let setup_complete = setup
        .iter()
        .filter(|s| REQUIRED_SETUP_STEPS.contains(&s.id))
        .all(|s| s.done);

    let onboarding = vec![
        SetupStep {
            id: "device_named",
            title: "Name this device",
            done: device_named,
            detail: None,
            kind: None,
        },
        SetupStep {
            id: "device_collecting",
            title: "Start collecting",
            done: device_collecting,
            detail: None,
            kind: collecting_kind(device_collecting, device_sync_started),
        },
        SetupStep {
            id: "first_source",
            title: "Connect a source",
            done: first_source > 0,
            detail: None,
            kind: None,
        },
        SetupStep {
            id: "living_source",
            title: "Sync your living spine",
            done: living_source,
            detail: None,
            kind: None,
        },
        SetupStep {
            id: "first_device",
            title: "Pair a device",
            done: first_device > 0,
            detail: None,
            kind: None,
        },
        SetupStep {
            id: "first_phone",
            title: "Add your phone",
            done: first_phone > 0,
            detail: None,
            kind: None,
        },
        SetupStep {
            id: "chat_imported",
            title: "Bring your chat history",
            done: chat_imported,
            detail: None,
            kind: None,
        },
        remote_access_step(crate::relay::is_relay_registered()),
        SetupStep {
            id: "first_sync",
            title: "First data synced",
            done: first_sync > 0,
            detail: None,
            kind: None,
        },
        SetupStep {
            id: "narrative_identity_ready",
            title: "Your narrative identity",
            done: nid_ready,
            detail: None,
            kind: narrative_identity_kind(nid_ready, nid_running),
        },
    ];

    Ok(SetupState {
        setup,
        setup_complete,
        onboarding,
    })
}

/// Three-state qualifier for the `device_collecting` step (behavior keys off
/// `done`; this is cosmetic for renderers): "collecting" once a device's
/// backfill completed, "syncing" while one is in flight, else none. Pure so the
/// states are unit-testable without a DB.
fn collecting_kind(completed: bool, started: bool) -> Option<&'static str> {
    if completed {
        Some("collecting")
    } else if started {
        Some("syncing")
    } else {
        None
    }
}

/// Three-state qualifier for the `narrative_identity_ready` step: "ready" once a
/// portrait exists, "generating" while a draft run is in flight, else none
/// (not started). Behavior keys off `done`; this is cosmetic for renderers.
/// Pure so the states are unit-testable without a DB.
fn narrative_identity_kind(ready: bool, running: bool) -> Option<&'static str> {
    if ready {
        Some("ready")
    } else if running {
        Some("generating")
    } else {
        None
    }
}

/// The `remote_access` onboarding step, three-state. Pure so the states are
/// unit-testable without a network.
///
/// "Auto-notice, never auto-enable" (docs/byo-networking.md): Virtues never
/// starts or recommends an overlay, but a NAT'd box on a user-run one
/// (Tailscale, a foreign WireGuard, …) IS reachable — at the overlay address —
/// so the step is honestly `done`. `kind` qualifies the three states for
/// renderers; behavior keys off `done`, copy off `detail`.
fn remote_access_step(endpoint_up: bool) -> SetupStep {
    // iroh model: "reachable from anywhere" == the box's iroh endpoint is bound
    // and homed on our relay. From there any paired device reaches it by
    // EndpointId — via the relay, upgrading to hole-punched direct when possible
    // — so NAT/IPv6 no longer gate reach (iroh traverses them).
    let (done, kind, detail) = if endpoint_up {
        (
            true,
            "iroh_relay",
            "Reachable from anywhere — connections go direct when possible, via the relay otherwise.",
        )
    } else {
        (
            false,
            "pending",
            "Connecting to the relay so your box is reachable from anywhere…",
        )
    };
    SetupStep {
        id: "remote_access",
        title: "Reachable from anywhere",
        done,
        detail: Some(detail.to_string()),
        kind: Some(kind),
    }
}

/// `GET /api/setup/state` — public-on-LAN like `/api/box/health`, and by the
/// same argument: the wizard and the appliance panel must render it before
/// any owner session exists, and it carries only booleans, step copy, and the
/// already-public reachability verdict (plus the mDNS name once the owner has
/// chosen it — which mDNS broadcasts to the LAN anyway).
pub async fn setup_state_handler(State(state): State<AppState>) -> impl IntoResponse {
    match compute_setup_state(state.db.pool()).await {
        Ok(setup) => (StatusCode::OK, Json(setup)).into_response(),
        Err(e) => {
            tracing::warn!(error = %e, "setup state failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net_check::{ByoTransport, NetClass, NetStatus};

    fn net(class: NetClass, ipv6: Option<&str>, byo: Option<&str>) -> NetStatus {
        NetStatus {
            class,
            ipv6_global: ipv6.map(|a| a.parse().unwrap()),
            ipv4_source: Some("192.168.1.20".parse().unwrap()),
            byo: byo.map(|ifname| ByoTransport {
                ifname: ifname.to_string(),
                addr: None,
            }),
            headline: "headline".to_string(),
            guidance: String::new(),
        }
    }

    #[test]
    fn remote_access_reflects_iroh_endpoint() {
        // Endpoint up + homed on the relay → reachable from anywhere.
        let step = remote_access_step(true);
        assert!(step.done);
        assert_eq!(step.kind, Some("iroh_relay"));

        // Endpoint not yet up → pending (a weather report, flips on its own).
        let step = remote_access_step(false);
        assert!(!step.done);
        assert_eq!(step.kind, Some("pending"));

        // id/title stable across states.
        assert_eq!(step.id, "remote_access");
        assert_eq!(step.title, "Reachable from anywhere");
    }

    #[test]
    fn device_collecting_kind_states() {
        // Backfill done → "collecting" (done=true; qualifier is cosmetic).
        assert_eq!(collecting_kind(true, false), Some("collecting"));
        // Started but not finished → "syncing".
        assert_eq!(collecting_kind(false, true), Some("syncing"));
        // Not started → no qualifier (renders as a plain not-done step).
        assert_eq!(collecting_kind(false, false), None);
        // Completed wins even if a later sync is mid-flight.
        assert_eq!(collecting_kind(true, true), Some("collecting"));
    }

    #[test]
    fn narrative_identity_kind_states() {
        // A portrait exists → "ready" (wins even if a redraft is running).
        assert_eq!(narrative_identity_kind(true, false), Some("ready"));
        assert_eq!(narrative_identity_kind(true, true), Some("ready"));
        // No portrait yet, a draft in flight → "generating".
        assert_eq!(narrative_identity_kind(false, true), Some("generating"));
        // Not started → no qualifier.
        assert_eq!(narrative_identity_kind(false, false), None);
    }
}
