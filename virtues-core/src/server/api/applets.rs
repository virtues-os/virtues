//! Applets, their runs, credentials and sources, plus the admin and
//! developer surfaces.

use axum::{
    extract::{DefaultBodyLimit, Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, patch, post},
    Json,
    Router,
};
use serde::{Deserialize, Serialize};

use super::error_response;
use crate::error::Error;
use crate::server::AppState;

/// This area's authenticated routes. Merged into the protected router, whose
/// `route_layer` requires a resolved `AuthUser`.
pub fn routes() -> Router<AppState> {
    Router::new()
        // Face-token mint — AUTHENTICATED. The authed app mints a short-lived
        // per-applet token and passes it into the iframe `src` (?vt=). This is
        // the gate on the whole face data door (faces.rs).
        .route("/api/applets/:id/face-token", get(crate::server::faces::mint_face_token_handler))
        // ─── Source OAuth + API-key connect flows ────────────────────
        // Device pairing (iOS / Mac / sensor) lives at /api/pair/* (server/mod.rs).
        .route(
            "/api/connect/:source_id/start",
            post(crate::api::source_auth::oauth_start_handler),
        )
        .route(
            "/api/connect/:source_id/complete",
            post(crate::api::source_auth::apikey_complete_handler),
        )
        .route(
            "/oauth/callback",
            get(crate::api::source_auth::oauth_callback_handler),
        )
        // Actions API
        .route(
            "/api/applets",
            get(list_applets_handler),
        )
        .route(
            "/api/applets/:id",
            get(get_applet_handler)
                .patch(patch_applet_handler)
                .delete(delete_applet_handler),
        )
        .route("/api/applets/:id/run", post(trigger_applet_handler))
        // The owner opened the applet's page, which clears its "new" Inbox item.
        .route("/api/applets/:id/seen", post(applet_seen_handler))
        .route("/api/applets/:id/message", post(message_applet_handler))
        .route("/api/applets/:id/data", get(get_applet_data_handler))
        // Read the applet's own code. Read-only, owner-authed like everything
        // in this group; see api/applet_source.rs for why it guards harder than
        // the face server does.
        .route(
            "/api/applets/:id/source",
            get(crate::api::applet_source::list_handler),
        )
        .route(
            "/api/applets/:id/source/*path",
            get(crate::api::applet_source::file_handler),
        )
        .route(
            "/api/applets/:id/fork",
            post(crate::api::applet_source::fork_handler),
        )
        // Chat-export upload (one-time import). Per-route body limit
        // overrides the router-wide 260MB cap: a ChatGPT zip carries every
        // uploaded file and image, and runs to gigabytes.
        .route(
            "/api/chat-import/upload",
            post(chat_import_upload_handler).layer(DefaultBodyLimit::max(CHAT_IMPORT_MAX_BYTES)),
        )
        .route("/api/chat-import/status", get(chat_import_status_handler))
        .route("/api/applets/:id/runs", get(list_applet_runs_handler))
        .route("/api/applets/:id/log", get(applet_log_handler))
        .route("/api/applets/:id/pages", get(applet_pages_handler))
        .route("/api/runs", get(list_runs_handler))
        // Credentials API
        .route("/api/credentials", get(list_credentials_handler))
        .route(
            "/api/credentials/:id",
            patch(patch_credential_handler).delete(delete_credential_handler),
        )
        // Source catalog (drives the Sources tile grid)
        .route("/api/sources", get(list_sources_handler))
        // Admin API — LLM-authoring on-ramp for new actions
        .route("/api/admin/reconcile", post(admin_reconcile_handler))
        // Developer API
        .route("/api/developer/sql", post(execute_sql_handler))
        .route("/api/developer/tables", get(list_tables_handler))
}


// ============================================================================
// Actions + runs API
// ============================================================================

/// Optional body for manual trigger — forwarded as the action payload.
#[derive(Debug, Deserialize, Default)]
pub struct TriggerAppletBody {
    #[serde(default)]
    pub payload: Option<serde_json::Value>,
}

/// Manually trigger an action run.
pub async fn trigger_applet_handler(
    State(state): State<AppState>,
    Path(applet_id): Path<String>,
    body: Option<Json<TriggerAppletBody>>,
) -> Response {
    let payload = body.and_then(|Json(b)| b.payload);

    let deps = crate::applet_runner::RunnerDeps {
        db: state.db.pool().clone(),
        yjs: state.yjs_state.clone(),
    };

    // Detach the heavy phase (subprocess + agent) onto a tokio task so a
    // client disconnect can't drop the future mid-run and leave the row
    // stuck in `running`. The handler returns 202 with the run_id as soon
    // as the row is created; the UI polls `app_applet_runs` for the final
    // status.
    let result = match crate::applet_runner::run_applet_detached(
        &deps,
        &applet_id,
        "manual",
        payload.as_ref(),
    )
    .await
    {
        Ok(r) => r,
        Err(e) => return error_response(e),
    };

    run_status_response(result, &applet_id)
}

/// The largest chat export the upload takes.
const CHAT_IMPORT_MAX_BYTES: usize = 4 * 1024 * 1024 * 1024;

/// POST /api/chat-import/upload — multipart upload of a Claude or ChatGPT
/// conversation export: the zip the site emails, or a conversations JSON file
/// from inside it. The file streams into the staging folder, so a large
/// export never sits in memory, and the `chat_import` action starts in the
/// background. The response is 202 with the run id; the UI follows the run in
/// `/api/runs?applet_id=applet_chat_import` for its summary or error.
pub async fn chat_import_upload_handler(
    State(state): State<AppState>,
    mut multipart: axum::extract::Multipart,
) -> Response {
    let staged = match super::imports::stage_upload(
        &mut multipart,
        "chat-import",
        "This file is larger than 4 GB. Upload the conversations.json from inside the zip instead.",
        &[],
    )
    .await
    {
        Ok(s) => s,
        Err(e) => return error_response(e),
    };
    super::imports::start_import(&state, "applet_chat_import", staged).await
}

/// One chat site's history on this server: the months that have messages,
/// oldest first.
#[derive(Debug, Serialize)]
pub struct ChatHistory {
    pub provider: String,
    pub months: Vec<ChatMonth>,
}

#[derive(Debug, Serialize)]
pub struct ChatMonth {
    /// `YYYY-MM`, in UTC.
    pub month: String,
    pub messages: i64,
}

/// GET /api/chat-import/status — what has arrived from other AI chat sites,
/// by site and month: the rows the export import (`chat_import`) and the
/// live syncs (`grok_chats`) wrote. Setup's chats step reads it to say which
/// sites are in and over what span.
pub async fn chat_import_status_handler(
    State(state): State<AppState>,
) -> Result<Json<Vec<ChatHistory>>, Error> {
    let rows: Vec<(String, String, i64)> = sqlx::query_as(
        "SELECT provider, to_char(date_trunc('month', occurred_at AT TIME ZONE 'UTC'), 'YYYY-MM'), \
                count(*)::bigint \
           FROM data_content_conversation \
          WHERE source_table IN ('chat_import', 'grok_chats') \
          GROUP BY 1, 2 \
          ORDER BY 1, 2",
    )
    .fetch_all(state.db.pool())
    .await
    .map_err(|e| Error::Database(format!("read imported chat months: {e}")))?;

    let mut out: Vec<ChatHistory> = Vec::new();
    for (provider, month, messages) in rows {
        match out.last_mut() {
            Some(h) if h.provider == provider => h.months.push(ChatMonth { month, messages }),
            _ => out.push(ChatHistory {
                provider,
                months: vec![ChatMonth { month, messages }],
            }),
        }
    }
    Ok(Json(out))
}


/// List all actions with their latest run status.
pub async fn list_applets_handler(State(state): State<AppState>) -> Response {
    let pool = state.db.pool();

    let rows = sqlx::query(
        r#"SELECT
            t.id, t.owner, t.name, t.description, t.agent, t.schedule,
            t.enabled, t.config, t.triggers,
            t.memory, t.credential_id, t.device_id,
            t.command,
            t.until, t.archived_at, t.watch,
            t.next_due_at, t.last_slot_at,
            t.created_at, t.updated_at,
            r.status AS last_run_status,
            r.started_at AS last_run_at,
            r.records_processed AS last_run_records,
            r.error AS last_run_error,
            r.result_summary AS last_run_summary,
            -- The card's pulse and its excerpt, fetched with the row instead
            -- of by the client afterwards. The list was firing two extra
            -- requests PER APPLET — around fifty on a page — for two small
            -- facts that the database can hand over in the same pass.
            p.pulse,
            -- The row in the task list that stands for it, when it is a task
            -- of the assistant's that repeats.
            (SELECT st.id FROM app_tasks st WHERE st.applet_id = t.id) AS task_id,
            s.summary AS last_success_summary,
            w.cost_micros AS spend_week_micros
           FROM app_applets t
           LEFT JOIN LATERAL (
               SELECT array_agg(status ORDER BY started_at DESC) AS pulse
                 FROM (SELECT status, started_at FROM app_applet_runs
                        WHERE applet_id = t.id
                        ORDER BY started_at DESC LIMIT 10) recent
           ) p ON TRUE
           LEFT JOIN LATERAL (
               SELECT result_summary AS summary FROM app_applet_runs
                WHERE applet_id = t.id AND status = 'success'
                  AND result_summary IS NOT NULL AND btrim(result_summary) <> ''
                ORDER BY started_at DESC LIMIT 1
           ) s ON TRUE
           -- What this applet has actually spent on AI in the last week.
           -- Deterministic applets (every sync, every indexer) sum to zero,
           -- which is the point: the number only appears where something is
           -- burning money, so an AI-authored applet that runs hourly on a big
           -- model is visible as such before the bill is.
           --
           -- Joined through the run, the same path `spend_micros_last_day`
           -- takes for the cap. The `::bigint` cast is load-bearing —
           -- SUM(bigint) is NUMERIC in Postgres and sqlx will not decode that
           -- as i64.
           LEFT JOIN LATERAL (
               SELECT COALESCE(SUM(c.cost_micros), 0)::bigint AS cost_micros
                 FROM app_ai_calls c
                 JOIN app_applet_runs ar ON ar.id = c.applet_run_id
                WHERE ar.applet_id = t.id
                  AND c.created_at > now() - interval '7 days'
           ) w ON TRUE
           LEFT JOIN app_applet_runs r ON r.id = (
               SELECT id FROM app_applet_runs
               WHERE applet_id = t.id
               ORDER BY created_at DESC LIMIT 1
           )
           ORDER BY t.name"#,
    )
    .fetch_all(pool)
    .await;

    match rows {
        Ok(rows) => {
            use sqlx::Row;
            let actions: Vec<serde_json::Value> = rows
                .iter()
                .map(|r| {
                    let id: String = r.try_get("id").unwrap_or_default();
                    let owner: String = r.try_get("owner").unwrap_or_else(|_| "user".to_string());
                    let name: String = r.try_get("name").unwrap_or_default();
                    let description: Option<String> = r.try_get("description").unwrap_or(None);
                    let agent: Option<String> = r.try_get("agent").unwrap_or(None);
                    let cron: Option<String> = r.try_get("schedule").unwrap_or(None);
                    let enabled: bool = r.try_get("enabled").unwrap_or(false);
                    // `config`/`triggers` are JSONB — decode straight to a Value
                    // (decoding to String fails and the `unwrap_or` swallowed it,
                    // so every action came back with empty config/triggers).
                    let config: serde_json::Value =
                        r.try_get("config").unwrap_or_else(|_| serde_json::json!({}));
                    let triggers_val: serde_json::Value =
                        r.try_get("triggers").unwrap_or_else(|_| serde_json::json!([]));
                    let triggers: Vec<String> =
                        serde_json::from_value(triggers_val).unwrap_or_default();
                    let memory: Option<String> = r.try_get("memory").unwrap_or(None);
                    let credential_id: Option<String> = r.try_get("credential_id").unwrap_or(None);
                    let device_id: Option<String> = r.try_get("device_id").unwrap_or(None);
                    let command_raw: Option<String> = r.try_get("command").unwrap_or(None);
                    let command: Option<Vec<String>> = command_raw
                        .as_deref()
                        .and_then(|s| serde_json::from_str(s).ok());
                    let until: Option<String> = r.try_get("until").unwrap_or(None);
                    let watch: Option<serde_json::Value> = r.try_get("watch").unwrap_or(None);
                    let archived_at: Option<chrono::DateTime<chrono::Utc>> =
                        r.try_get("archived_at").unwrap_or(None);
                    let next_due_at: Option<chrono::DateTime<chrono::Utc>> =
                        r.try_get("next_due_at").unwrap_or(None);
                    let last_slot_at: Option<chrono::DateTime<chrono::Utc>> =
                        r.try_get("last_slot_at").unwrap_or(None);
                    let has_face = crate::server::faces::face_for(&id).is_some();
                    let origin = crate::scheduler::applets::origin_of(
                        &owner,
                        credential_id.is_some() || device_id.is_some(),
                    );
                    let kind = crate::scheduler::applets::kind_of(
                        has_face,
                        command.as_ref().is_some_and(|c| !c.is_empty()),
                        crate::scheduler::applets::starts_itself(cron.as_deref(), watch.is_some(), &triggers),
                    );
                    // TIMESTAMPTZ columns decode to DateTime<Utc>; serde emits
                    // RFC3339 in the JSON. Reading them as String failed (empty).
                    let created: chrono::DateTime<chrono::Utc> =
                        r.try_get("created_at").unwrap_or_else(|_| chrono::Utc::now());
                    let updated: chrono::DateTime<chrono::Utc> =
                        r.try_get("updated_at").unwrap_or_else(|_| chrono::Utc::now());

                    let pulse: Vec<String> = r
                        .try_get::<Option<Vec<String>>, _>("pulse")
                        .unwrap_or(None)
                        .unwrap_or_default();
                    let last_success_summary: Option<String> =
                        r.try_get("last_success_summary").unwrap_or(None);
                    // A row that cannot be read is logged. Without that, a
                    // task of the assistant's would show as one with no row.
                    let task_id: Option<String> = r.try_get("task_id").unwrap_or_else(|e| {
                        tracing::error!(applet_id = %id, error = %e, "could not read the applet's task row id");
                        None
                    });
                    // The SQL COALESCEs to 0, so a real "spent nothing" arrives
                    // as 0 and NULL can only mean the decode failed. Those must
                    // not collapse into each other: this is a money figure, and
                    // a silently-zero money column reads as good news. So it
                    // stays Option — null travels to the client as "unknown" —
                    // and a failure says so once per list rather than never.
                    let spend_week_micros: Option<i64> =
                        match r.try_get::<Option<i64>, _>("spend_week_micros") {
                            Ok(v) => v,
                            Err(e) => {
                                tracing::warn!(applet_id = %id, error = %e,
                                    "could not decode applet weekly spend");
                                None
                            }
                        };
                    let last_run_status: Option<String> =
                        r.try_get("last_run_status").unwrap_or(None);
                    let last_run = last_run_status.map(|s| {
                        let at: Option<chrono::DateTime<chrono::Utc>> =
                            r.try_get("last_run_at").unwrap_or(None);
                        let records: Option<i64> = r.try_get("last_run_records").unwrap_or(None);
                        let err: Option<String> = r.try_get("last_run_error").unwrap_or(None);
                        let sum: Option<String> = r.try_get("last_run_summary").unwrap_or(None);
                        serde_json::json!({
                            "status": s,
                            "started_at": at,
                            "records_processed": records,
                            "error": err,
                            "summary": sum,
                        })
                    });

                    serde_json::json!({
                        "id": id,
                        "owner": owner,
                        "name": name,
                        "description": description,
                        "agent": agent,
                        "schedule": cron,
                        "enabled": enabled,
                        "config": config,
                        "triggers": triggers,
                        "memory": memory,
                        "credential_id": credential_id,
                        "device_id": device_id,
                        "origin": origin,
                        "kind": kind,
                        "task_id": task_id,
                        "command": command,
                        "until": until,
                        "watch": watch,
                        "archived_at": archived_at,
                        "next_due_at": next_due_at,
                        "last_slot_at": last_slot_at,
                        "has_face": has_face,
                        "pulse": pulse,
                        "last_success_summary": last_success_summary,
                        "spend_week_micros": spend_week_micros,
                        "created_at": created,
                        "updated_at": updated,
                        "last_run": last_run,
                    })
                })
                .collect();

            (StatusCode::OK, Json(actions)).into_response()
        }
        Err(e) => error_response(e.into()),
    }
}

/// One shape for "a run was requested" — shared by the manual-run and the
/// message handlers so the two can never disagree about what a status means.
pub(crate) fn run_status_response(
    result: crate::applet_runner::AppletRunResult,
    applet_id: &str,
) -> Response {
    use crate::applet_runner::AppletRunStatus;
    let (status_code, status_label) = match result.status {
        AppletRunStatus::Running => (StatusCode::ACCEPTED, "running"),
        AppletRunStatus::Success => (StatusCode::OK, "success"),
        AppletRunStatus::Skipped => (StatusCode::OK, "skipped"),
        AppletRunStatus::Failed => (StatusCode::INTERNAL_SERVER_ERROR, "error"),
        // Not an error: a ceiling the owner set was reached. 200 so the UI
        // renders the run and its explanation rather than a failure toast.
        AppletRunStatus::BudgetExceeded => (StatusCode::OK, "budget_exceeded"),
        AppletRunStatus::NotFound => (StatusCode::NOT_FOUND, "not_found"),
        AppletRunStatus::Forbidden => (StatusCode::FORBIDDEN, "forbidden"),
    };
    (
        status_code,
        Json(serde_json::json!({
            "run_id": result.run_id,
            "applet_id": applet_id,
            "status": status_label,
            "summary": result.summary,
            "error": result.error,
        })),
    )
        .into_response()
}

/// GET /api/applets/:id/pages — the pages this applet's runs wrote, newest
/// first, for its page to show what it made.
pub async fn applet_pages_handler(
    State(state): State<AppState>,
    Path(applet_id): Path<String>,
) -> Result<Json<Vec<crate::scheduler::applets::AppletPage>>, Error> {
    Ok(Json(crate::scheduler::applets::applet_pages(state.db.pool(), &applet_id, 30).await?))
}

/// GET /api/applets/:id/log — the run log with identical outcomes collapsed.
///
/// 99% of run history on a real box is "the machine ticked and there was
/// nothing to do": transcription_resolution alone wrote 1,294 runs in a week,
/// every one of them a successful no-op. A raw list is unreadable, and worse,
/// it hides things — 3,449 consecutive errors sat in this box's history behind
/// a window of recent successes.
///
/// So consecutive runs sharing an outcome become one row with a count. The
/// grouping is mechanical, not semantic: same status, same summary, same
/// message. Nothing here decides what "did nothing" means, which is good,
/// because `records_processed = 0` does not reliably mean it. An applet whose
/// output varies — any agent, every message exchange — never collapses at all.
pub async fn applet_log_handler(
    State(state): State<AppState>,
    Path(applet_id): Path<String>,
    axum::extract::Query(q): axum::extract::Query<RunsQuery>,
) -> Result<Json<Vec<crate::scheduler::applets::LogEntry>>, Error> {
    let limit = q.limit.unwrap_or(50).clamp(1, 200);
    Ok(Json(
        crate::scheduler::applets::collapsed_log(state.db.pool(), &applet_id, limit).await?,
    ))
}

/// POST /api/applets/:id/message — say something to an applet.
///
/// The sixth wake. Until this existed every trigger was the box acting on
/// itself — a clock, a poll, a device, a tool call — and a person could turn an
/// applet on, off, or run it, but could not tell it anything.
pub async fn message_applet_handler(
    State(state): State<AppState>,
    Path(applet_id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> Response {
    let text = body
        .get("message")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let Some(text) = text else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "message is required" })),
        )
            .into_response();
    };

    let deps = crate::applet_runner::RunnerDeps {
        db: state.db.pool().clone(),
        yjs: state.yjs_state.clone(),
    };
    // Detached, like the manual-run handler: a long agent turn must not hang on
    // the client staying connected. The run row exists before this returns, so
    // the UI can show the exchange immediately.
    let payload = serde_json::json!({ "message": text });
    match crate::applet_runner::run_applet_detached(&deps, &applet_id, "message", Some(&payload))
        .await
    {
        Ok(result) => run_status_response(result, &applet_id),
        Err(e) => error_response(e),
    }
}

/// The keys an applet declares (`secrets = [...]` in its manifest), each with
/// whether the owner has set it. Names only: a value never leaves the box here.
async fn declared_secrets_status(pool: &sqlx::PgPool, applet_id: &str) -> anyhow::Result<Vec<serde_json::Value>> {
    let declared = crate::applet_templates::declared_secrets(applet_id);
    if declared.is_empty() {
        return Ok(Vec::new());
    }
    let set: std::collections::HashSet<String> = crate::secrets::list(pool, crate::secrets::Kind::Env)
        .await?
        .into_iter()
        .map(|n| n.key)
        .collect();
    Ok(declared
        .into_iter()
        .map(|name| {
            let is_set = set.contains(&name);
            serde_json::json!({ "name": name, "is_set": is_set })
        })
        .collect())
}

/// One applet as the app reads it: its row, with what the row cannot say on
/// its own (whose it is, what kind of thing it is, whether it has a view, the
/// keys it names and its last run). The GET and the PATCH both answer with
/// this, so a page that takes the PATCH reply as the applet loses nothing.
async fn applet_detail(pool: &sqlx::PgPool, action: &crate::scheduler::applets::Applet) -> Result<serde_json::Value, Error> {
    // A failed read is an error here. Answering with no last run would say
    // the applet never ran.
    let last_run = crate::scheduler::applets::last_run(pool, &action.id).await?;
    let secrets = declared_secrets_status(pool, &action.id)
        .await
        .map_err(|e| Error::Other(format!("{e:#}")))?;
    let has_view = crate::server::faces::face_for(&action.id).is_some();
    let task_id = crate::api::tasks::standing_task_id(pool, &action.id).await?;
    Ok(serde_json::json!({
        "id": action.id,
        "owner": action.owner,
        "name": action.name,
        "description": action.description,
        "agent": action.agent,
        "schedule": action.schedule,
        "enabled": action.enabled,
        "config": action.config,
        "triggers": action.triggers,
        "memory": action.memory,
        "command": action.command,
        "credential_id": action.credential_id,
        "device_id": action.device_id,
        "origin": crate::scheduler::applets::derived_origin(action),
        "kind": crate::scheduler::applets::derived_kind(action, has_view),
        "task_id": task_id,
        "until": action.until,
        "watch": action.watch,
        "archived_at": action.archived_at,
        "next_due_at": action.next_due_at,
        "last_slot_at": action.last_slot_at,
        "has_face": has_view,
        "has_override": action.has_override,
        "secrets": secrets,
        "created_at": action.created_at,
        "updated_at": action.updated_at,
        "last_run": last_run,
    }))
}

/// GET /api/applets/:id — single applet with its last run inlined.
pub async fn get_applet_handler(
    State(state): State<AppState>,
    Path(applet_id): Path<String>,
) -> Response {
    let pool = state.db.pool();
    match crate::scheduler::applets::get_applet(pool, &applet_id).await {
        Ok(action) => match applet_detail(pool, &action).await {
            Ok(detail) => (StatusCode::OK, Json(detail)).into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response(),
        },
        Err(e) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

/// POST /api/applets/:id/seen — the owner opened the applet's page.
pub async fn applet_seen_handler(State(state): State<AppState>, Path(applet_id): Path<String>) -> Result<StatusCode, Error> {
    crate::scheduler::applets::mark_seen(state.db.pool(), &applet_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// PATCH /api/applets/:id — partial update. Enforces system-owner guard.
pub async fn patch_applet_handler(
    State(state): State<AppState>,
    caller: Option<axum::Extension<crate::console_socket::ConsoleCaller>>,
    Path(applet_id): Path<String>,
    Json(patch): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, Error> {
    // An outside agent reaches this route for `virtues applet enable` and
    // nothing else (`console_socket::agent_may`): it may turn an applet on
    // or off, never rewrite its schedule, config or triggers.
    if matches!(caller, Some(axum::Extension(crate::console_socket::ConsoleCaller::Agent)))
        && !agent_patch_ok(&patch)
    {
        return Err(Error::Unauthorized("an agent key may only turn an applet on or off".into()));
    }
    let pool = state.db.pool();
    let applet = crate::scheduler::applets::update_applet(pool, &applet_id, &patch).await?;
    // A change can make it a task of the assistant's (a schedule added to a
    // prompt you made), which gives it its row in the task list. The change
    // is saved by now, so a row that fails to open is logged and the reply
    // still says what was saved; the next start of the server opens it.
    if let Err(e) = crate::api::repeating_tasks::ensure(pool, &applet, None).await {
        tracing::error!(applet_id = %applet.id, error = %e, "could not open or close the applet's task row");
    }
    // The row as it serializes, with the detail's fields over it. The app
    // replaces the applet it holds with this reply, so a reply without
    // `has_face` or `kind` would turn a site's page into a list of runs.
    let mut reply = serde_json::to_value(&applet).map_err(|e| Error::Other(e.to_string()))?;
    if let (Some(row), serde_json::Value::Object(detail)) = (reply.as_object_mut(), applet_detail(pool, &applet).await?) {
        row.extend(detail);
    }
    Ok(Json(reply))
}

/// An agent's patch: exactly `{"enabled": <bool>}`.
fn agent_patch_ok(patch: &serde_json::Value) -> bool {
    patch.as_object().is_some_and(|o| o.len() == 1 && o.get("enabled").is_some_and(|v| v.is_boolean()))
}

#[cfg(test)]
mod agent_patch_tests {
    use super::agent_patch_ok;
    use serde_json::json;

    #[test]
    fn an_agent_may_only_flip_enabled() {
        assert!(agent_patch_ok(&json!({ "enabled": true })));
        assert!(agent_patch_ok(&json!({ "enabled": false })));
        assert!(!agent_patch_ok(&json!({ "enabled": true, "schedule": "* * * * *" })));
        assert!(!agent_patch_ok(&json!({ "config": {} })));
        assert!(!agent_patch_ok(&json!({ "enabled": "yes" })));
        assert!(!agent_patch_ok(&json!([])));
    }
}

/// DELETE /api/applets/:id?drop_data= — delete a user-owned applet. System rows
/// refused. `drop_data=true` also drops the applet's private `applet_<slug>`
/// schema; the default keeps its data.
#[derive(Debug, Deserialize)]
pub struct DeleteAppletQuery {
    #[serde(default)]
    pub drop_data: bool,
}

pub async fn delete_applet_handler(
    State(state): State<AppState>,
    Path(applet_id): Path<String>,
    axum::extract::Query(q): axum::extract::Query<DeleteAppletQuery>,
) -> Result<StatusCode, Error> {
    crate::scheduler::applets::delete_applet(state.db.pool(), &applet_id, q.drop_data).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// GET /api/applets/:id/data — the tables an applet owns in its private
/// `applet_<slug>` schema, so the delete confirm can show what `drop_data`
/// would remove. Empty `tables` (and null `schema`) when it owns none.
pub async fn get_applet_data_handler(
    State(state): State<AppState>,
    Path(applet_id): Path<String>,
) -> Result<Json<serde_json::Value>, Error> {
    let tables = crate::scheduler::applets::applet_data_tables(state.db.pool(), &applet_id).await?;
    let schema = crate::scheduler::applets::applet_schema_name(&applet_id);
    Ok(Json(serde_json::json!({ "schema": schema, "tables": tables })))
}

/// GET /api/applets/:id/runs?limit= — run history, newest first.
#[derive(Debug, Deserialize)]
pub struct RunsQuery {
    pub limit: Option<i64>,
    pub status: Option<String>,
    pub applet_id: Option<String>,
}

pub async fn list_applet_runs_handler(
    State(state): State<AppState>,
    Path(applet_id): Path<String>,
    axum::extract::Query(q): axum::extract::Query<RunsQuery>,
) -> Result<Json<Vec<crate::scheduler::applets::AppletRun>>, Error> {
    let limit = q.limit.unwrap_or(20).clamp(1, 200);
    let runs = crate::scheduler::applets::query_runs(
        state.db.pool(),
        Some(&applet_id),
        q.status.as_deref(),
        limit,
    )
    .await?;
    Ok(Json(runs))
}

/// GET /api/runs?status=&applet_id=&limit= — global run history.
pub async fn list_runs_handler(
    State(state): State<AppState>,
    axum::extract::Query(q): axum::extract::Query<RunsQuery>,
) -> Result<Json<Vec<crate::scheduler::applets::AppletRun>>, Error> {
    let limit = q.limit.unwrap_or(50).clamp(1, 200);
    let runs = crate::scheduler::applets::query_runs(
        state.db.pool(),
        q.applet_id.as_deref(),
        q.status.as_deref(),
        limit,
    )
    .await?;
    Ok(Json(runs))
}

// ============================================================================
// Credentials API
// ============================================================================

/// GET /api/credentials — list all credentials (active + pending + revoked).
pub async fn list_credentials_handler(
    State(state): State<AppState>,
) -> Result<Json<Vec<crate::api::credentials::CredentialListItem>>, Error> {
    Ok(Json(crate::api::credentials::list_credentials(state.db.pool()).await?))
}

// ============================================================================
// Sources API
// ============================================================================

/// One catalog tile, derived from a `[[source]]` row in `actions/templates.toml`
/// plus live credential counts.
#[derive(Debug, Serialize)]
pub struct SourceCatalogItem {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub description: String,
    /// `'self_issued_bearer' | 'via_proxy' | 'api_key'`.
    pub auth_kind: &'static str,
    /// Number of `active` credentials (passwords) for this source.
    pub credential_count: i64,
    /// Secret names an `api_key` source expects, in manifest order. Empty for
    /// every other auth kind. Without it the connect form had to guess, and it
    /// guessed `["token"]` — so a source declaring two fields could not be
    /// connected from the UI at all.
    pub fields: Vec<String>,
    /// Where this source's code can be read. Provenance only; never an install
    /// or update path. Null for sources whose code is entirely the box's.
    pub repo: Option<String>,
    pub repo_ref: Option<String>,
    /// Ontologies this source can produce, by display name — "what would
    /// connecting this give me". The catalog could previously only say what a
    /// source *is*, never what it would deliver.
    pub provides: Vec<String>,
    /// The life-domains those ontologies fall in (`health`, `financial`, …),
    /// which is the coarser grain worth showing in a table cell.
    pub domains: Vec<String>,
    /// How to connect by logging in instead of pasting (see `SourceLogin`).
    /// The app offers it where it can open a browser window; everywhere else
    /// the form falls back to the fields.
    pub login: Option<crate::applet_templates::SourceLogin>,
}

/// GET /api/sources — catalog tiles for the Sources UI.
///
/// Reads from the `[[source]]` rows in `actions/templates.toml`. Adding a new
/// provider = a TOML edit; no code change. Frontend dispatches the Connect
/// button on `auth_kind`:
///
///   self_issued_bearer → DevicePairModal
///   via_proxy          → server-side redirect to the proxy
///   api_key            → text-input modal
pub async fn list_sources_handler(State(state): State<AppState>) -> Response {
    let pool = state.db.pool();

    let sources = crate::applet_templates::list_sources_sorted();
    let mut items = Vec::with_capacity(sources.len());

    // One COUNT query per source — cheap; the catalog has at most a handful
    // of entries even at scale.
    for s in sources {
        let credential_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM credentials WHERE kind = 'source' AND key = $1 AND status = 'active'",
        )
        .bind(&s.id)
        .fetch_one(pool)
        .await
        .unwrap_or(0);

        let written = crate::applet_templates::ontologies_written_by(&s.id);
        items.push(SourceCatalogItem {
            id: s.id.clone(),
            name: s.display_name.clone(),
            icon: s.icon.clone(),
            description: s.description.clone(),
            auth_kind: s.auth.kind_str(),
            credential_count,
            fields: match &s.auth {
                crate::applet_templates::SourceAuth::ApiKey { fields } => fields.clone(),
                _ => Vec::new(),
            },
            repo: s.repo.clone(),
            repo_ref: s.repo_ref.clone(),
            login: s.login.clone(),
            provides: written
                .iter()
                .map(|n| {
                    virtues_registry::ontologies::registered_ontologies()
                        .into_iter()
                        .find(|o| o.name == n)
                        .map(|o| o.display_name.to_string())
                        .unwrap_or_else(|| n.clone())
                })
                .collect(),
            domains: {
                let mut d: Vec<String> = written
                    .iter()
                    .filter_map(|n| n.split('_').next().map(str::to_string))
                    .collect();
                d.sort();
                d.dedup();
                d
            },
        });
    }

    (StatusCode::OK, Json(items)).into_response()
}


/// PATCH /api/credentials/:id — rename or toggle active.
#[derive(Debug, Deserialize)]
pub struct PatchCredentialBody {
    pub name: Option<String>,
    pub is_active: Option<bool>,
}

pub async fn patch_credential_handler(
    State(state): State<AppState>,
    Path(credential_id): Path<String>,
    Json(body): Json<PatchCredentialBody>,
) -> Response {
    let pool = state.db.pool();
    if let Some(name) = &body.name {
        if let Err(e) = crate::api::credentials::rename_credential(pool, &credential_id, name).await {
            let status = if e.http_status() == 404 {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::BAD_REQUEST
            };
            return (status, Json(serde_json::json!({ "error": e.to_string() }))).into_response();
        }
    }
    if let Some(active) = body.is_active {
        if !active {
            if let Err(e) = crate::api::credentials::revoke_credential(pool, &credential_id).await {
                return error_response(e);
            }
        } else {
            // Re-activating is not supported via PATCH. A revoked device must
            // be re-paired via the QR / manual-link flow so a fresh
            // device_token and applet_ids fan-out are generated.
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "re-activating a revoked credential requires re-pairing"
                })),
            )
                .into_response();
        }
    }
    StatusCode::NO_CONTENT.into_response()
}

/// DELETE /api/credentials/:id
///
/// Dispatch by current status:
/// - `pending`  → hard-delete the row (used when the user cancels mid-pair
///   modal; the row never had a token or fan-out actions, so nothing to
///   preserve).
/// - `active`   → revoke (drop fan-out actions, keep history with
///   `applet_id = NULL`).
/// - `revoked`  → already revoked, idempotent 204.
pub async fn delete_credential_handler(
    State(state): State<AppState>,
    Path(credential_id): Path<String>,
) -> Response {
    let pool = state.db.pool();

    let status: Option<(String,)> =
        match sqlx::query_as("SELECT status FROM credentials WHERE id = $1 AND kind = 'source'")
            .bind(&credential_id)
            .fetch_optional(pool)
            .await
        {
            Ok(s) => s,
            Err(e) => return error_response(e.into()),
        };

    let result = match status.as_ref().map(|s| s.0.as_str()) {
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "credential not found" })),
            )
                .into_response()
        }
        Some("pending") => crate::api::credentials::delete_pending_credential(pool, &credential_id).await,
        Some("revoked") => return StatusCode::NO_CONTENT.into_response(),
        _ => crate::api::credentials::revoke_credential(pool, &credential_id).await,
    };

    match result {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => error_response(e),
    }
}

// ============================================================================
// Admin
// ============================================================================

/// `POST /api/admin/reconcile`
///
/// Re-reads `actions/sources.toml` + every `actions/<name>/manifest.toml` from
/// disk and upserts `app_applets` rows accordingly.
///
/// This is the LLM-authoring on-ramp: an LLM creates a new action folder,
/// hits this endpoint, and the action is live without restarting core.
///
/// Response shape: `{ "upserted": <count> }`
pub async fn admin_reconcile_handler(State(state): State<AppState>) -> Response {
    // 1. Force a re-read of the on-disk catalog (sources.toml + per-action
    //    manifests). Subsequent lookup_source / list_sources_sorted /
    //    reconcile calls see the new data.
    crate::applet_templates::reload_catalog();

    // 2. Reconcile `app_applets` SQL rows against the fresh catalog. Manifest
    //    fields overwrite for system actions; user-managed runtime state
    //    (enabled, schedule, config) is preserved per the field-ownership
    //    rule documented in applet_templates/mod.rs.
    let upserted = match crate::applet_templates::reconcile_templates(state.db.pool()).await {
        Ok(n) => n,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": format!("reconcile_templates failed: {e}")
                })),
            )
                .into_response();
        }
    };

    (
        StatusCode::OK,
        Json(serde_json::json!({ "upserted": upserted })),
    )
        .into_response()
}

// ============================================================================
// Developer API
// ============================================================================

/// Execute a read-only SQL query.
///
/// Takes the owner proof (`api::owner_proof`): the query runs as the server's
/// own database role and can read every table, the ones that authenticate
/// the owner included, so a process that only reaches the app's loopback
/// port must not be able to call it.
pub async fn execute_sql_handler(
    State(state): State<AppState>,
    _proof: crate::api::owner_proof::OwnerProven,
    Json(request): Json<crate::api::developer::ExecuteSqlRequest>,
) -> Response {
    match crate::api::developer::execute_sql(state.db.pool(), request).await {
        Ok(results) => (StatusCode::OK, Json(results)).into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": e.to_string()
            })),
        )
            .into_response(),
    }
}

/// List all tables
pub async fn list_tables_handler(
    State(state): State<AppState>,
) -> Result<Json<Vec<String>>, Error> {
    Ok(Json(crate::api::developer::list_tables(state.db.pool()).await?))
}


/// GET /api/devices/applet-ids — devices refresh their applet_id routing map.
///
/// Used by paired devices when their local routing table goes stale (e.g. after
/// templates.toml adds a new stream, or the device reinstalls). Authenticated by
/// the proven iroh key (`AuthUser`, a hard extractor) — the map is the device's
/// own ingest actions, keyed on its `device_id`.
pub async fn device_applet_ids_handler(
    State(state): State<AppState>,
    user: crate::middleware::auth::AuthUser,
) -> Response {
    match virtues_helpers::auth::fanout_applet_ids(state.db.pool(), &user.device_id).await {
        Ok(applet_ids) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "device_id": user.device_id,
                "applet_ids": applet_ids,
            })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

/// Sends the SQL console a request through the whole app the box serves, as
/// a paired Mac with no owner proof, and expects the refusal.
// TODO(2026-11-25): move this route into `owner_proof_route_tests::GATED`
// (server/mod.rs) and delete this module.
#[cfg(test)]
mod sql_console_tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use std::sync::Arc;
    use tower::Service;

    #[sqlx::test(migrations = "./migrations")]
    async fn the_sql_console_needs_the_owner_proof(pool: sqlx::PgPool) {
        let mac = virtues_iroh::SecretKey::from_bytes(&[43; 32]);
        sqlx::query(
            "INSERT INTO app_device (id, user_id, kind, label, endpoint_id) \
             VALUES ('dev_sql_mac', $1, 'desktop_app', 'Mac', $2)",
        )
        .bind(crate::middleware::http::OWNER_USER_ID)
        .bind(mac.public().to_string())
        .execute(&pool)
        .await
        .unwrap();

        let dir = std::env::temp_dir().join(format!("virtues-sql-console-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let storage = Arc::new(crate::storage::Storage::file(dir.to_string_lossy().into_owned()).unwrap());
        let mut app = crate::server::build_app(AppState {
            db: Arc::new(crate::database::Database::from_pool(pool.clone())),
            storage: storage.clone(),
            drive_config: crate::api::drive::DriveConfig::new(storage),
            tool_executor: None,
            yjs_state: crate::server::yjs::YjsState::new(pool.clone()),
            chat_cancel_state: crate::api::chat::ChatCancellationState::new(),
            ghost_permissions: crate::api::chat_permissions::GhostPermissions::new(),
            live_turns: crate::api::live_turn::LiveTurns::new(),
            day_rewrites: crate::api::day_rewrites::DayRewrites::new(),
        });
        let mut req = Request::builder()
            .method("POST")
            .uri("/api/developer/sql")
            .header("host", "127.0.0.1:7117")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"sql":"SELECT 1"}"#))
            .unwrap();
        req.extensions_mut().insert(virtues_iroh::ProvenPeer(mac.public()));
        let res = app.call(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::FORBIDDEN);
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v["code"], "owner_proof_required");
    }
}
