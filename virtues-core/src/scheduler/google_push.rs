//! Google Calendar change signals, pulled from virtues-api.
//!
//! The box never takes a call for this. It asks virtues-api for its push id,
//! hands that id to the Google Calendar sync applets, and holds a
//! `/v1/push/wait` request open. The applets give the id to Google as the
//! token of each calendar watch they start, so when a calendar changes,
//! Google calls virtues-api, and the next `/wait` here returns that watch's
//! channel id. This loop then runs the sync applets early, with the channel
//! ids as the payload. The server side is `services/virtues-api/src/routes/push.rs`.
//!
//! Without a linked key, or when virtues-api is unreachable, nothing changes
//! for the applets: they keep syncing on their own schedule.

use std::time::Duration;

use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use sqlx::PgPool;

use crate::applet_runner::{run_applet, AppletRunStatus, RunnerDeps};
use crate::virtues_api::client::{BearerClient, Purpose};

/// Which applets take the signals. One row exists per Google credential.
const APPLET_PREFIX: &str = "applet_google_calendar_sync_cred_";
const ROUTE: &str = "/v1/push/google/calendar";
/// How often the loop re-reads the applet list and push id, so a calendar
/// account connected while the box runs gets the id without a restart.
const REFRESH: Duration = Duration::from_secs(3600);
const NO_APPLETS_SLEEP: Duration = Duration::from_secs(600);
const UNLINKED_SLEEP: Duration = Duration::from_secs(3600);
const MAX_BACKOFF: Duration = Duration::from_secs(600);
/// A signal that arrives during a scheduled run finds the applet busy. Try
/// again after the run would normally have finished.
const BUSY_RETRY: Duration = Duration::from_secs(30);
const BUSY_TRIES: u32 = 4;

pub fn spawn(deps: RunnerDeps) {
    tokio::spawn(async move {
        let client = BearerClient::from_env(deps.db.clone()).with_purpose(Purpose::System);
        let mut backoff = Duration::from_secs(30);
        loop {
            match round(&deps, &client).await {
                Ok(Round::Refreshed) => backoff = Duration::from_secs(30),
                Ok(Round::Idle(d)) => tokio::time::sleep(d).await,
                Err(e) => {
                    tracing::warn!(error = %e, "calendar change signals failed; retrying");
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(MAX_BACKOFF);
                }
            }
        }
    });
}

enum Round {
    /// Waited out a refresh period; start the next one.
    Refreshed,
    /// Nothing to do yet; sleep this long.
    Idle(Duration),
}

async fn round(deps: &RunnerDeps, client: &BearerClient) -> Result<Round> {
    let applets = calendar_applets(&deps.db).await?;
    if applets.is_empty() {
        return Ok(Round::Idle(NO_APPLETS_SLEEP));
    }

    let resp = client.post_json("/v1/push/id", &json!({})).await;
    let push_id = match resp {
        Ok(r) if r.is_success() => r
            .body
            .get("push_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("virtues-api sent no push id"))?
            .to_string(),
        // Not linked, or a virtues-api without this route yet.
        Ok(r) if matches!(r.status, 401 | 404) => return Ok(Round::Idle(UNLINKED_SLEEP)),
        Ok(r) => return Err(anyhow!("push id request answered {}", r.status)),
        // No key yet: the box has not been linked.
        Err(e) if e.to_string().contains("no virtues_api key") => return Ok(Round::Idle(UNLINKED_SLEEP)),
        Err(e) => return Err(e),
    };
    let address = format!("{}{ROUTE}", crate::virtues_api::api_url().trim_end_matches('/'));
    hand_out(&deps.db, &applets, &push_id, &address).await?;

    let started = tokio::time::Instant::now();
    while started.elapsed() < REFRESH {
        let r = client.get_json("/v1/push/wait").await?;
        if !r.is_success() {
            return Err(anyhow!("push wait answered {}", r.status));
        }
        let channels: Vec<Value> = r.body.get("channels").and_then(|v| v.as_array()).cloned().unwrap_or_default();
        if channels.is_empty() {
            continue;
        }
        tracing::info!(count = channels.len(), "calendar changed; syncing early");
        let payload = json!({ "push_channels": channels });
        for id in &applets {
            tokio::spawn(run_when_free(deps.clone(), id.clone(), payload.clone()));
        }
    }
    Ok(Round::Refreshed)
}

async fn calendar_applets(db: &PgPool) -> Result<Vec<String>> {
    Ok(sqlx::query_scalar(
        "SELECT id FROM app_applets WHERE id LIKE $1 || '%' AND enabled AND triggers ? 'webhook'",
    )
    .bind(APPLET_PREFIX)
    .fetch_all(db)
    .await?)
}

/// Put the push id and address where the applets read their saved state.
/// The applet writes its whole state back after each run, keeping keys it
/// does not use, so this survives until the next refresh.
async fn hand_out(db: &PgPool, applets: &[String], push_id: &str, address: &str) -> Result<()> {
    let push = json!({ "token": push_id, "address": address });
    sqlx::query(
        "UPDATE app_applets SET state = jsonb_set(state, '{push}', $2) \
         WHERE id = ANY($1) AND state->'push' IS DISTINCT FROM $2",
    )
    .bind(applets)
    .bind(&push)
    .execute(db)
    .await?;
    Ok(())
}

async fn run_when_free(deps: RunnerDeps, applet_id: String, payload: Value) {
    for _ in 0..BUSY_TRIES {
        match run_applet(&deps, &applet_id, "webhook", Some(&payload)).await {
            Ok(r) if r.status == AppletRunStatus::Skipped => tokio::time::sleep(BUSY_RETRY).await,
            Ok(_) => return,
            Err(e) => {
                tracing::warn!(applet_id, error = %e, "calendar sync after a change signal failed");
                return;
            }
        }
    }
}
