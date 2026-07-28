//! `virtues status --json` — boring/complete diagnostic for support tickets.
//!
//! Different shape from the human dashboard (`handle_status`). The JSON is
//! intentionally flat and stable: someone pasting this into a chat with a
//! support engineer should give us everything we need to triage without a
//! second round-trip.
//!
//! What's in here:
//!   - Binary version + uptime
//!   - Last applied schema migration ID
//!   - Action subprocess health (running / errored / stopped per action)
//!   - Last 10 `app_auth_event` rows (paired / revoked / sudo events)
//!   - Subscription link state (api_key present? account linked?)
//!   - Wallet snapshot (read locally — full wallet balance lives in
//!     virtues-api but we expose what we know from the last 402/200)
//!   - BYO key status (provider + model — never the key itself)
//!   - Pending sudo / pair token counts
//!   - Diagnostic opt-in state
//!
//! Intentionally NOT in here:
//!   - Secrets of any kind (encryption keys, bearers, BYO keys)
//!   - User content (chat history, source data, day pages)
//!   - IPs of paired devices (the audit log shows them; JSON shows
//!     counts and recency, not addresses, to keep paste-into-chat safe)

use anyhow::Result;
use serde::Serialize;
use sqlx::PgPool;

use crate::Virtues;

#[derive(Debug, Serialize)]
struct StatusJson {
    schema_version: String,
    virtues_version: String,
    diag_enabled: bool,
    box_id: String,
    auth: AuthSection,
    sudo: SudoSection,
    pair: PairSection,
    billing: BillingSection,
    actions: ActionsSection,
    network: NetworkSection,
    recent_events: Vec<EventRow>,
}

#[derive(Debug, Serialize)]
struct AuthSection {
    devices_paired: i64,
}

#[derive(Debug, Serialize)]
struct SudoSection {
    pending: i64,
    consumed_24h: i64,
}

#[derive(Debug, Serialize)]
struct PairSection {
    tokens_pending: i64,
    tokens_authorized: i64,
}

#[derive(Debug, Serialize)]
struct BillingSection {
    auto_topup_enabled: bool,
    auto_topup_failures_24h: i32,
    auto_topup_disabled_at: Option<String>,
    byo_configured: bool,
    byo_provider: Option<String>,
    server_status: String,
}

#[derive(Debug, Serialize)]
struct ActionsSection {
    total: i64,
    enabled: i64,
    last_run_status_counts: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct NetworkSection {
    /// Reachability class: `ipv6_direct` / `ipv4_public` / `behind_nat` /
    /// `unknown`. The IPv6-direct doctrine's "can a device reach this box?".
    class: String,
    /// Does the box have a globally-routable IPv6 (the direct path)?
    has_global_ipv6: bool,
    /// One-line verdict. No literal addresses — keep paste-into-chat safe.
    headline: String,
    /// Auto-noticed user-run overlay (Tailscale etc.) — interface name ONLY,
    /// never its address (same paste-into-chat rule as above).
    byo_ifname: Option<String>,
}

#[derive(Debug, Serialize)]
struct EventRow {
    event_type: String,
    occurred_at: String,
}

pub async fn print(virtues: &Virtues) -> Result<()> {
    let pool = virtues.database.pool();
    let json = collect(pool).await?;
    println!("{}", serde_json::to_string_pretty(&json)?);
    Ok(())
}

async fn collect(pool: &PgPool) -> Result<StatusJson> {
    Ok(StatusJson {
        schema_version: schema_version(pool).await,
        virtues_version: env!("CARGO_PKG_VERSION").to_string(),
        diag_enabled: super::super::diag::enabled(),
        box_id: super::super::diag::box_id(),
        auth: collect_auth(pool).await,
        sudo: collect_sudo(pool).await,
        pair: collect_pair(pool).await,
        billing: collect_billing(pool).await,
        actions: collect_actions(pool).await,
        network: collect_network(),
        recent_events: collect_recent_events(pool).await,
    })
}

/// Reachability snapshot — class + a boolean, no literal addresses (paste-safe).
fn collect_network() -> NetworkSection {
    let s = crate::net_check::compute_net_status();
    NetworkSection {
        class: s.class.as_str().to_string(),
        has_global_ipv6: s.ipv6_global.is_some(),
        byo_ifname: s.byo.as_ref().map(|b| b.ifname.clone()),
        headline: s.headline,
    }
}

async fn schema_version(pool: &PgPool) -> String {
    let row: Option<(i64,)> = sqlx::query_as(
        "SELECT version FROM _sqlx_migrations WHERE success = TRUE \
         ORDER BY version DESC LIMIT 1",
    )
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();
    row.map(|(v,)| v.to_string()).unwrap_or_else(|| "unknown".to_string())
}

async fn collect_auth(pool: &PgPool) -> AuthSection {
    let devices: Option<(i64,)> = sqlx::query_as(
        "SELECT COUNT(*) FROM app_device WHERE revoked_at IS NULL",
    )
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();
    AuthSection {
        devices_paired: devices.map(|(n,)| n).unwrap_or(0),
    }
}

async fn collect_sudo(pool: &PgPool) -> SudoSection {
    let pending: Option<(i64,)> = sqlx::query_as(
        "SELECT COUNT(*) FROM app_sudo_request \
         WHERE status = 'pending' AND expires_at > now()",
    )
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();
    let consumed: Option<(i64,)> = sqlx::query_as(
        "SELECT COUNT(*) FROM app_sudo_request \
         WHERE status = 'consumed' AND consumed_at > now() - interval '24 hours'",
    )
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();
    SudoSection {
        pending: pending.map(|(n,)| n).unwrap_or(0),
        consumed_24h: consumed.map(|(n,)| n).unwrap_or(0),
    }
}

async fn collect_pair(pool: &PgPool) -> PairSection {
    let pending: Option<(i64,)> = sqlx::query_as(
        "SELECT COUNT(*) FROM app_pair_token \
         WHERE status = 'pending' AND expires_at > now()",
    )
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();
    let authorized: Option<(i64,)> = sqlx::query_as(
        "SELECT COUNT(*) FROM app_pair_token \
         WHERE status = 'authorized' AND expires_at > now()",
    )
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();
    PairSection {
        tokens_pending: pending.map(|(n,)| n).unwrap_or(0),
        tokens_authorized: authorized.map(|(n,)| n).unwrap_or(0),
    }
}

async fn collect_billing(pool: &PgPool) -> BillingSection {
    let profile: Option<(bool, i32, Option<chrono::DateTime<chrono::Utc>>, String)> =
        sqlx::query_as(
            "SELECT auto_topup_enabled, auto_topup_failures_24h, \
                    auto_topup_disabled_at, server_status \
             FROM app_user_profile \
             WHERE id = '00000000-0000-0000-0000-000000000001'",
        )
        .fetch_optional(pool)
        .await
        .ok()
        .flatten();
    let (enabled, failures, disabled_at, server_status) =
        profile.unwrap_or((true, 0, None, "unknown".to_string()));

    let byo: Option<(serde_json::Value,)> = sqlx::query_as(
        "SELECT metadata FROM credentials \
         WHERE source_id = $1 AND status = 'active' LIMIT 1",
    )
    .bind(crate::api::settings_byo::BYO_SOURCE_ID)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();
    let (byo_configured, byo_provider) = match byo {
        Some((meta,)) => (
            true,
            meta.get("provider").and_then(|v| v.as_str()).map(String::from),
        ),
        None => (false, None),
    };

    BillingSection {
        auto_topup_enabled: enabled,
        auto_topup_failures_24h: failures,
        auto_topup_disabled_at: disabled_at.map(|d| d.to_rfc3339()),
        byo_configured,
        byo_provider,
        server_status,
    }
}

async fn collect_actions(pool: &PgPool) -> ActionsSection {
    let total: Option<(i64,)> =
        sqlx::query_as("SELECT COUNT(*) FROM app_applets")
            .fetch_optional(pool)
            .await
            .ok()
            .flatten();
    let enabled: Option<(i64,)> =
        sqlx::query_as("SELECT COUNT(*) FROM app_applets WHERE enabled = TRUE")
            .fetch_optional(pool)
            .await
            .ok()
            .flatten();
    let counts: Vec<(String, i64)> = sqlx::query_as(
        "SELECT status, COUNT(*) FROM ( \
            SELECT DISTINCT ON (action_id) status \
            FROM app_applet_runs \
            WHERE action_id IS NOT NULL \
            ORDER BY action_id, created_at DESC \
         ) recent GROUP BY status",
    )
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    let mut by: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
    for (status, n) in counts {
        by.insert(status, serde_json::Value::Number(n.into()));
    }
    ActionsSection {
        total: total.map(|(n,)| n).unwrap_or(0),
        enabled: enabled.map(|(n,)| n).unwrap_or(0),
        last_run_status_counts: serde_json::Value::Object(by),
    }
}

async fn collect_recent_events(pool: &PgPool) -> Vec<EventRow> {
    let rows: Vec<(String, chrono::DateTime<chrono::Utc>)> = sqlx::query_as(
        "SELECT event_type, occurred_at FROM app_auth_event \
         ORDER BY occurred_at DESC LIMIT 10",
    )
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    rows.into_iter()
        .map(|(event_type, occurred_at)| EventRow {
            event_type,
            occurred_at: occurred_at.to_rfc3339(),
        })
        .collect()
}
