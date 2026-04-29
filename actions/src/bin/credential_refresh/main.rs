//! Credential refresh — the one auth-related cron action.
//!
//! Sweeps `via_proxy` credentials whose access tokens are nearing expiry,
//! calls `virtues_helpers::auth::proxy_refresh` for each, and writes the new
//! secrets back via `update_credential_secrets`.
//!
//! All flows hit the same auth helpers crate as the core HTTP handlers —
//! same code path, just invoked from cron instead of a browser.
//!
//! # Failure modes
//!
//! - **Proxy unreachable** (network flake): row is left untouched.
//!   `next_refresh_at` doesn't advance, so the next cron tick retries.
//! - **Proxy returns 4xx** (refresh_token revoked, item login required):
//!   credential is flipped to `reauth_required` so the UI surfaces a
//!   "Reconnect" button. The next cron tick won't pick it up
//!   (`status != 'active'`).
//! - **Proxy returns 5xx**: same as unreachable — leave row, retry later.
//! - **Decrypt failure / missing refresh_token**: flipped to `error` with a
//!   reason so the user can see what's wrong.
//!
//! Triggered every 15 minutes per `templates.toml`.

use anyhow::Result;
use serde::Deserialize;
use sqlx::SqlitePool;
use virtues_helpers::auth::{
    mark_credential_status, proxy_refresh, read_credential_secrets, update_credential_secrets,
    AuthError, CredentialStatus,
};
use virtues_helpers::{connect_from_env, output, read_input};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    let input = read_input()?;
    let pool = connect_from_env().await?;

    let stats = sweep_expiring(&pool).await?;

    let summary = format!(
        "refreshed {} of {} credentials ({} reauth, {} errors, {} skipped)",
        stats.refreshed,
        stats.scanned,
        stats.reauth_required,
        stats.errored,
        stats.skipped,
    );
    output(&summary, &input.config)
}

#[derive(Default)]
struct Stats {
    scanned: usize,
    refreshed: usize,
    reauth_required: usize,
    errored: usize,
    skipped: usize,
}

#[derive(Debug, sqlx::FromRow)]
struct ExpiringRow {
    id: String,
    source_id: String,
}

#[derive(Debug, Deserialize)]
struct OauthSecrets {
    refresh_token: Option<String>,
}

async fn sweep_expiring(pool: &SqlitePool) -> Result<Stats> {
    let rows: Vec<ExpiringRow> = sqlx::query_as(
        r#"SELECT id, source_id FROM credentials
           WHERE status = 'active'
             AND next_refresh_at IS NOT NULL
             AND next_refresh_at < datetime('now')"#,
    )
    .fetch_all(pool)
    .await?;

    let mut stats = Stats {
        scanned: rows.len(),
        ..Default::default()
    };

    for row in rows {
        match refresh_one(pool, &row).await {
            RefreshOutcome::Refreshed => stats.refreshed += 1,
            RefreshOutcome::ReauthRequired => stats.reauth_required += 1,
            RefreshOutcome::Errored => stats.errored += 1,
            RefreshOutcome::Skipped => stats.skipped += 1,
        }
    }

    Ok(stats)
}

enum RefreshOutcome {
    Refreshed,
    ReauthRequired,
    Errored,
    Skipped,
}

async fn refresh_one(pool: &SqlitePool, row: &ExpiringRow) -> RefreshOutcome {
    // 1. Decrypt the existing secrets and pull the refresh_token.
    let secrets = match read_credential_secrets(pool, &row.id).await {
        Ok(s) => s,
        Err(e) => {
            tracing::error!(credential_id = %row.id, error = %e, "decrypt failed during refresh");
            let _ = mark_credential_status(
                pool,
                &row.id,
                CredentialStatus::Error,
                Some("decrypt_failed"),
            )
            .await;
            return RefreshOutcome::Errored;
        }
    };

    let refresh_token = match serde_json::from_value::<OauthSecrets>(secrets) {
        Ok(s) => match s.refresh_token {
            Some(t) if !t.is_empty() => t,
            _ => {
                tracing::warn!(
                    credential_id = %row.id,
                    source_id = %row.source_id,
                    "no refresh_token in secrets; skipping (Plaid-style or paste-once kind)"
                );
                return RefreshOutcome::Skipped;
            }
        },
        Err(e) => {
            tracing::error!(credential_id = %row.id, error = %e, "secrets shape unexpected");
            let _ = mark_credential_status(
                pool,
                &row.id,
                CredentialStatus::Error,
                Some("secrets_shape_invalid"),
            )
            .await;
            return RefreshOutcome::Errored;
        }
    };

    // 2. POST to proxy /<source_id>/refresh.
    let resp = match proxy_refresh(&row.source_id, &refresh_token).await {
        Ok(r) => r,
        Err(AuthError::Proxy(msg)) if msg.contains("upstream 4") => {
            // Provider rejected the refresh_token (revoked, expired, item
            // login required). Flip status so the UI surfaces "Reconnect".
            tracing::warn!(
                credential_id = %row.id,
                source_id = %row.source_id,
                err = %msg,
                "refresh rejected by provider; marking reauth_required"
            );
            let _ = mark_credential_status(
                pool,
                &row.id,
                CredentialStatus::ReauthRequired,
                Some("token_rejected_by_provider"),
            )
            .await;
            return RefreshOutcome::ReauthRequired;
        }
        Err(e) => {
            tracing::warn!(
                credential_id = %row.id,
                source_id = %row.source_id,
                error = %e,
                "proxy refresh failed transiently; will retry next tick"
            );
            return RefreshOutcome::Skipped;
        }
    };

    // 3. Persist the new secrets + advance next_refresh_at.
    if let Err(e) = update_credential_secrets(pool, &row.id, &resp.secrets, resp.expires_in).await {
        tracing::error!(
            credential_id = %row.id,
            error = %e,
            "update_credential_secrets failed after successful refresh"
        );
        return RefreshOutcome::Errored;
    }

    tracing::info!(
        credential_id = %row.id,
        source_id = %row.source_id,
        expires_in = resp.expires_in,
        "credential refreshed"
    );
    RefreshOutcome::Refreshed
}
