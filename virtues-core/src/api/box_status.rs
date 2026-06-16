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
///
/// Takes `AuthUser` explicitly (not just relying on the protected-route layer)
/// so this identity-bearing response — WG pubkey, publish_id, billing state —
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

// ─── Setup / onboarding state machine (docs/onboarding.md) ──────────────────
//
// One source of truth, three renderers: the phone wizard, the appliance
// panel, and `virtues status`. Two distinct lists by design:
//
//   * `setup` — the wizard's REQUIRED core. Ends early ("setup ≠ onboarding"):
//     claimed → account → named → on-network. Everything else is deferred.
//   * `onboarding` — the dashboard's "next wins" checklist for the first
//     week: progressive, abandonable, never blocking.
//
// Every signal is DERIVED from existing state (vault, credentials, runs,
// hostname, net_check) — there is intentionally no "wizard progress" table,
// so the state survives reinstalls, restores, and out-of-band changes.

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

/// The hostname the installer assigns before the user names the box.
const DEFAULT_HOSTNAME: &str = "virtues";

/// Compute the setup/onboarding state. Reuses [`compute_status`] for the
/// vault-backed signals so the wizard, panel, and CLI can never disagree
/// with `/api/box/status`.
pub async fn compute_setup_state(pool: &PgPool) -> Result<SetupState> {
    let s = compute_status(pool).await?;
    let net = crate::net_check::compute_net_status();

    // Claimed = at least one device has paired (the pair token was consumed
    // by an owner's browser or phone). Ownership-by-proximity, see
    // docs/onboarding.md "trust on first boot".
    let claimed: i64 =
        sqlx::query_scalar("SELECT count(*) FROM app_device WHERE revoked_at IS NULL")
            .fetch_one(pool)
            .await
            .unwrap_or(0);

    // Named = the operator replaced the installer's default hostname (the
    // wizard's "name your box" step sets the Avahi/mDNS name).
    let hostname = std::fs::read_to_string("/proc/sys/kernel/hostname")
        .map(|h| h.trim().to_string())
        .unwrap_or_default();
    let named = !hostname.is_empty() && hostname != DEFAULT_HOSTNAME;

    // First source = an active cloud-source credential (OAuth etc.) — not a
    // paired device's self-credential, not the BYO-key pseudo-source.
    let first_source: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM credentials \
         WHERE status = 'active' AND device_id IS NULL \
           AND source_id NOT IN ($1, $2)",
    )
    .bind("__device__")
    .bind(crate::api::settings_byo::BYO_SOURCE_ID)
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

    // First sync = any action run has ever succeeded (data actually landed).
    let first_sync: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM app_action_runs WHERE status = 'success'",
    )
    .fetch_one(pool)
    .await
    .unwrap_or(0);

    // Tier 2: a living (cloud/OAuth) source that has actually synced — i.e. a
    // non-device, non-BYO credential with at least one successful run. Stronger
    // than `first_source` (which only means "connected"): this means data flows.
    let living_source: bool = sqlx::query_scalar(
        "SELECT EXISTS( \
           SELECT 1 FROM credentials c \
           JOIN app_actions a ON a.credential_id = c.id \
           JOIN app_action_runs r ON r.action_id = a.id AND r.status = 'success' \
           WHERE c.status = 'active' AND c.device_id IS NULL \
             AND c.source_id NOT IN ($1, $2))",
    )
    .bind("__device__")
    .bind(crate::api::settings_byo::BYO_SOURCE_ID)
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

    let setup = vec![
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
            done: s.subscription.billing_token,
            detail: None,
            kind: None,
        },
        SetupStep {
            id: "named",
            title: "Box named",
            done: named,
            detail: named.then(|| format!("{hostname}.local")),
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
    let setup_complete = setup.iter().all(|s| s.done);

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
        remote_access_step(&net),
        SetupStep {
            id: "first_sync",
            title: "First data synced",
            done: first_sync > 0,
            detail: None,
            kind: None,
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

/// The `remote_access` onboarding step, three-state. Pure so the states are
/// unit-testable without a network.
///
/// "Auto-notice, never auto-enable" (docs/byo-networking.md): Virtues never
/// starts or recommends an overlay, but a NAT'd box on a user-run one
/// (Tailscale, a foreign WireGuard, …) IS reachable — at the overlay address —
/// so the step is honestly `done`. `kind` qualifies the three states for
/// renderers; behavior keys off `done`, copy off `detail`.
fn remote_access_step(net: &crate::net_check::NetStatus) -> SetupStep {
    let (done, kind) = if net.ipv6_global.is_some() {
        // The doctrine's happy path: a global IPv6 to be reached at directly.
        (true, "ipv6_direct")
    } else if net.byo.is_some() {
        // No direct path, but the user already runs their own transport.
        (true, "byo")
    } else {
        // Not reachable from here — a weather report, not an error. The box
        // re-checks per poll, so the step flips on its own wherever it lives.
        (false, net.class.as_str())
    };
    SetupStep {
        id: "remote_access",
        title: "Reachable from anywhere",
        done,
        // `verdict_line` already prefers IPv6-direct, then the BYO transport,
        // then the class headline — one copy source, no drift.
        detail: Some(net.verdict_line()),
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
    fn remote_access_three_states() {
        // Global IPv6 → done via the direct path, headline as detail.
        let step = remote_access_step(&net(NetClass::Ipv6Direct, Some("2001:db8::1"), None));
        assert!(step.done);
        assert_eq!(step.kind, Some("ipv6_direct"));
        assert_eq!(step.detail.as_deref(), Some("headline"));

        // NAT'd but on a user-run overlay → honestly done ("auto-notice,
        // never auto-enable"), with the BYO verdict as detail.
        let step = remote_access_step(&net(NetClass::NatNoIpv6, None, Some("tailscale0")));
        assert!(step.done);
        assert_eq!(step.kind, Some("byo"));
        assert_eq!(
            step.detail.as_deref(),
            Some("Available via your own network (tailscale0).")
        );

        // No direct path, no overlay → not done; kind carries the net class.
        let step = remote_access_step(&net(NetClass::NatNoIpv6, None, None));
        assert!(!step.done);
        assert_eq!(step.kind, Some("behind_nat"));
        assert_eq!(step.detail.as_deref(), Some("headline"));

        // id/title are stable across all three states.
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
}
