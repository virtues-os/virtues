//! Getting started — one room, one derived truth.
//!
//! After the founder's letter, getting started is a single seeded chat
//! (`chat_getting_started`). Everything about the room that is not the
//! person's own words is DERIVED here, from rows, on every read: which of the
//! four steps are done, whether the box can call a model, whether the app is
//! locked. Nothing stores progress. The only stored state is the skips, in
//! `app_user_profile.getting_started_dismissed` (migration 0014), which this
//! module writes and the old getting-started page used to.
//!
//! The model is a guest in this room: it reads the state as a prompt block,
//! it can open a card or skip a step or play introductions back for
//! confirmation, and it can never mark a step done. Real rows do that. See
//! agents/plan/getting-started-plan.md.

use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::error::{Error, Result};
use crate::middleware::auth::AuthUser;
use crate::server::webhook::AppState;

/// The room. Seeded at boot (`prod_seed`), undeletable and un-retitled by
/// id (`chats.rs`), forced into [`AGENT_MODE`] by id (`chat_handler`).
pub const GETTING_STARTED_CHAT_ID: &str = "chat_getting_started";

/// The chat mode the room runs in: its own prompt, three tools, no data.
pub const AGENT_MODE: &str = "getting_started";

/// The four steps, in walking order. This is the ceiling; a fifth is a plan
/// change, not a registry entry.
pub const STEP_IDS: [&str; 4] = ["connect_ai", "introductions", "connect_world", "interview"];

/// The tools the room's model may call. Registry ids (`virtues_registry::tools`).
pub const TOOLS: &[&str] = &["show_step", "skip_step", "record_introductions"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus {
    Done,
    Open,
    Skipped,
}

#[derive(Debug, Clone, Serialize)]
pub struct Step {
    pub id: &'static str,
    pub title: &'static str,
    pub status: StepStatus,
    /// How a done step got done, where it matters ("subscription" | "byo").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub via: Option<&'static str>,
    /// Server-authored copy for the step's current state — render verbatim.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// Started but not done (the interview has begun, no document yet).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub underway: bool,
    /// The person has moved past this step themselves (the dismissed list
    /// carries it). Read for a step that is done by rows before the walk
    /// reached it — integrations already in place — so the room can still
    /// stop there once and offer more, rather than skip it silently.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub acknowledged: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct GettingStartedState {
    /// Can this box call a model: a linked subscription or an active BYO
    /// route. The dev skip (`VIRTUES_DEV_SKIP_SETUP`) counts, or every
    /// checkout would open locked.
    pub ai_connected: bool,
    /// The app is a room and a door: no AI, and the door has not been used.
    pub locked: bool,
    pub steps: Vec<Step>,
    /// The first narrated day, once one exists — the promise line's payoff.
    pub first_day: Option<chrono::NaiveDate>,
    /// Every step done or skipped: the sidebar card goes, the mast collapses.
    pub graduated: bool,
    /// The interview has begun, inside this room. Set once by
    /// `POST /api/getting-started/interview`; the drafter reads the room's
    /// transcript from this instant on.
    pub interview_started_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl GettingStartedState {
    pub fn step(&self, id: &str) -> Option<&Step> {
        self.steps.iter().find(|s| s.id == id)
    }

    /// The interview is the conversation now: started, and no document yet.
    pub fn interview_underway(&self) -> bool {
        self.interview_started_at.is_some()
            && self.step("interview").map(|s| s.status != StepStatus::Done).unwrap_or(false)
    }

    /// Which prompt answers the room's next turn: the interviewer's while
    /// the interview is underway, the setup guest's otherwise.
    pub fn agent_mode(&self) -> &'static str {
        if self.interview_underway() {
            "interview"
        } else {
            AGENT_MODE
        }
    }

    /// The state as the model reads it: one short block, regenerated per
    /// turn, so the prompt never claims a step done that the rows say is
    /// open. Cache-stable across turns while nothing changes.
    pub fn render_prompt_block(&self) -> String {
        let mut out = String::from("<getting_started>\n");
        for s in &self.steps {
            let status = match s.status {
                StepStatus::Done => "done",
                StepStatus::Open if s.underway => "underway",
                StepStatus::Open => "open",
                StepStatus::Skipped => "skipped",
            };
            out.push_str(&format!("- {}: {} ({})", s.id, s.title, status));
            if let Some(d) = &s.detail {
                out.push_str(&format!(" — {d}"));
            }
            out.push('\n');
        }
        match self.first_day {
            Some(d) => out.push_str(&format!("- first day written up: {d}\n")),
            None => out.push_str("- first day written up: not yet\n"),
        }
        out.push_str("</getting_started>");
        out
    }
}

/// Is this a step id the room knows.
pub fn is_step(id: &str) -> bool {
    STEP_IDS.contains(&id)
}

fn title(id: &str) -> &'static str {
    match id {
        "connect_ai" => "Connect AI",
        "introductions" => "Introductions",
        "connect_world" => "Connect your world",
        "interview" => "In your own words",
        _ => "",
    }
}

/// Can the box call a model right now. The light predicate for the chat
/// turn: the subscription key or an active BYO row, plus the dev skip.
pub async fn ai_connected(pool: &PgPool) -> bool {
    if dev_skip() {
        return true;
    }
    // absent-ok: no readable key is no key (see `box_status::compute_status`).
    let linked = crate::virtues_api::renew::has_api_key(pool)
        .await
        .unwrap_or(false);
    linked || crate::api::settings_byo::byo_is_active(pool).await
}

fn dev_skip() -> bool {
    std::env::var("VIRTUES_DEV_SKIP_SETUP")
        .map(|v| v == "1" || v == "true")
        .unwrap_or(false)
}

/// The whole derived state. Reads the setup state the wizard and the CLI
/// already agree on, then the few rows the four steps key on.
pub async fn compute(pool: &PgPool) -> Result<GettingStartedState> {
    let setup = crate::api::box_status::compute_setup_state(pool)
        .await
        .map_err(|e| Error::Database(format!("setup state: {e}")))?;
    let done = |id: &str| -> bool {
        setup
            .setup
            .iter()
            .chain(setup.onboarding.iter())
            .find(|s| s.id == id)
            .map(|s| s.done)
            .unwrap_or(false)
    };

    let byo = crate::api::settings_byo::byo_is_active(pool).await;
    // `account` is the linked subscription, or the dev skip marking every
    // setup step done — which is exactly the dev arm this needs.
    let account = done("account");
    let ai_connected = account || byo;

    let profile = sqlx::query_as::<_, (Option<String>, Vec<String>, Option<chrono::DateTime<chrono::Utc>>)>(
        "SELECT preferred_name, getting_started_dismissed, interview_started_at FROM app_user_profile LIMIT 1",
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("read profile for getting started: {e}")))?;
    // absent-ok: no profile row yet IS the fresh box — nothing named, nothing skipped.
    let (preferred_name, dismissed, interview_started_at) = profile.unwrap_or((None, Vec::new(), None));
    let skipped = |id: &str| dismissed.iter().any(|d| d == id);

    let status = |id: &str, is_done: bool| {
        if is_done {
            StepStatus::Done
        } else if skipped(id) {
            StepStatus::Skipped
        } else {
            StepStatus::Open
        }
    };

    // Connect your world: something flowing, by the rule the old page used —
    // a source row, or a device that has landed data. A collector running
    // with a denied permission is said, not hidden behind the check.
    let world = done("first_source") || done("device_collecting");
    // What is already in place, said in the ask when the walk reaches this
    // step with rows already there: "3 integrations connected".
    let integrations: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM credentials WHERE status = 'active' \
           AND source_id NOT IN ($1, $2, '__device__')",
    )
    .bind(crate::api::settings_byo::BYO_SOURCE_ID)
    .bind(crate::virtues_api::renew::SOURCE_ID)
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("count integrations: {e}")))?;
    let world_detail = if setup.degraded.is_empty() {
        (integrations > 0).then(|| {
            if integrations == 1 {
                "1 integration connected".to_string()
            } else {
                format!("{integrations} integrations connected")
            }
        })
    } else {
        let names: Vec<String> = setup
            .degraded
            .iter()
            .map(|d| {
                format!(
                    "{} is running without {}",
                    d.label.clone().unwrap_or_else(|| "a device".to_string()),
                    d.denied.join(", ")
                )
            })
            .collect();
        Some(names.join("; "))
    };

    let interview_done = done("narrative_identity_ready");
    let interview_started = interview_started_at.is_some();

    let steps = vec![
        Step {
            id: "connect_ai",
            title: title("connect_ai"),
            status: status("connect_ai", ai_connected),
            via: if account {
                Some("subscription")
            } else if byo {
                Some("byo")
            } else {
                None
            },
            detail: None,
            underway: false,
            acknowledged: skipped("connect_ai"),
        },
        Step {
            id: "introductions",
            title: title("introductions"),
            status: status(
                "introductions",
                preferred_name.as_deref().is_some_and(|n| !n.trim().is_empty()),
            ),
            via: None,
            detail: None,
            underway: false,
            acknowledged: skipped("introductions"),
        },
        Step {
            id: "connect_world",
            title: title("connect_world"),
            status: status("connect_world", world),
            via: None,
            detail: world_detail,
            underway: false,
            acknowledged: skipped("connect_world"),
        },
        Step {
            id: "interview",
            title: title("interview"),
            status: status("interview", interview_done),
            via: None,
            detail: None,
            underway: interview_started && !interview_done,
            acknowledged: skipped("interview"),
        },
    ];

    // Locked = no model and the door unused. The door's skip lands in the
    // same dismissed list as every other skip; there is no second flag.
    let locked = !ai_connected && !skipped("connect_ai");
    let graduated = steps.iter().all(|s| s.status != StepStatus::Open);
    let first_day = crate::api::census::first_narrated_day(pool).await;

    Ok(GettingStartedState {
        ai_connected,
        locked,
        steps,
        first_day,
        graduated,
        interview_started_at,
    })
}

/// The interview begins, inside this room: the instant is recorded once, and
/// from here the room's turns are the interviewer's and the drafter's material.
pub async fn start_interview(pool: &PgPool) -> Result<()> {
    sqlx::query(
        "UPDATE app_user_profile SET interview_started_at = COALESCE(interview_started_at, now())",
    )
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("start interview: {e}")))?;
    Ok(())
}

/// Where the interview's transcript lives and where it starts: the
/// getting-started room from `interview_started_at` on, or, on a box that
/// held its interview in the old standalone room, that room whole.
pub async fn interview_source(
    pool: &PgPool,
) -> Result<(&'static str, Option<chrono::DateTime<chrono::Utc>>)> {
    let since: Option<Option<chrono::DateTime<chrono::Utc>>> =
        sqlx::query_scalar("SELECT interview_started_at FROM app_user_profile LIMIT 1")
            .fetch_optional(pool)
            .await
            .map_err(|e| Error::Database(format!("read interview start: {e}")))?;
    Ok(match since.flatten() {
        Some(ts) => (GETTING_STARTED_CHAT_ID, Some(ts)),
        None => (crate::api::narrative_draft::INTERVIEW_CHAT_ID, None),
    })
}

/// `POST /api/getting-started/interview` — begin the interview here.
pub async fn start_interview_handler(
    State(state): State<AppState>,
    _user: AuthUser,
) -> impl IntoResponse {
    let pool = state.db.pool();
    if let Err(e) = start_interview(pool).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response();
    }
    match compute(pool).await {
        Ok(s) => (StatusCode::OK, Json(s)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

/// Skip (or un-skip) a step: the one stored fact about getting started.
/// `connect_ai` is the door's business — the model's `skip_step` tool refuses
/// it (see `tools/executor.rs`), but the door and the slash command land
/// here with it.
pub async fn set_skipped(pool: &PgPool, step: &str, skipped: bool) -> Result<()> {
    if !is_step(step) {
        return Err(Error::InvalidInput(format!("unknown getting-started step: {step}")));
    }
    let sql = if skipped {
        "UPDATE app_user_profile SET getting_started_dismissed = \
         array_append(array_remove(getting_started_dismissed, $1), $1)"
    } else {
        "UPDATE app_user_profile SET getting_started_dismissed = \
         array_remove(getting_started_dismissed, $1)"
    };
    sqlx::query(sql)
        .bind(step)
        .execute(pool)
        .await
        .map_err(|e| Error::Database(format!("skip getting-started step: {e}")))?;
    Ok(())
}

/// `GET /api/getting-started`
pub async fn state_handler(State(state): State<AppState>, _user: AuthUser) -> impl IntoResponse {
    match compute(state.db.pool()).await {
        Ok(s) => (StatusCode::OK, Json(s)).into_response(),
        Err(e) => {
            tracing::warn!(error = %e, "getting-started state failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response()
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct SkipRequest {
    pub step: String,
    #[serde(default = "default_true")]
    pub skipped: bool,
}

fn default_true() -> bool {
    true
}

/// `POST /api/getting-started/skip` — `{step, skipped?}`; answers with the
/// new state so the client has nothing to re-fetch.
pub async fn skip_handler(
    State(state): State<AppState>,
    _user: AuthUser,
    Json(req): Json<SkipRequest>,
) -> impl IntoResponse {
    let pool = state.db.pool();
    if let Err(e) = set_skipped(pool, &req.step, req.skipped).await {
        let code = match e {
            Error::InvalidInput(_) => StatusCode::BAD_REQUEST,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        return (code, Json(serde_json::json!({ "error": e.to_string() }))).into_response();
    }
    match compute(pool).await {
        Ok(s) => (StatusCode::OK, Json(s)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(statuses: [StepStatus; 4], ai: bool, door_used: bool) -> GettingStartedState {
        let steps = STEP_IDS
            .iter()
            .zip(statuses)
            .map(|(id, status)| Step {
                id,
                title: title(id),
                status,
                via: None,
                detail: None,
                underway: false,
                acknowledged: false,
            })
            .collect::<Vec<_>>();
        GettingStartedState {
            ai_connected: ai,
            locked: !ai && !door_used,
            graduated: steps.iter().all(|s| s.status != StepStatus::Open),
            steps,
            first_day: None,
            interview_started_at: None,
        }
    }

    #[test]
    fn graduation_counts_skips_as_settled() {
        use StepStatus::*;
        assert!(state([Done, Skipped, Done, Skipped], true, false).graduated);
        assert!(!state([Done, Open, Done, Skipped], true, false).graduated);
    }

    #[test]
    fn prompt_block_never_upgrades_an_open_step() {
        use StepStatus::*;
        let s = state([Done, Open, Skipped, Open], true, false);
        let block = s.render_prompt_block();
        assert!(block.contains("- connect_ai: Connect AI (done)"));
        assert!(block.contains("- introductions: Introductions (open)"));
        assert!(block.contains("- connect_world: Connect your world (skipped)"));
        assert!(block.contains("first day written up: not yet"));
    }

    #[test]
    fn step_ids_are_the_four() {
        assert!(is_step("connect_ai"));
        assert!(is_step("interview"));
        assert!(!is_step("further"));
        assert!(!is_step("first_day"));
    }

    /// A fresh box: no key, no BYO, no name, nothing flowing, no document.
    /// Everything open, the app locked; one skip of connect_ai opens it.
    #[sqlx::test(migrations = "./migrations")]
    async fn fresh_box_is_locked_and_all_open(pool: PgPool) -> sqlx::Result<()> {
        // The dev skip would mark the account done and defeat the test.
        std::env::remove_var("VIRTUES_DEV_SKIP_SETUP");
        sqlx::query("INSERT INTO app_user_profile DEFAULT VALUES")
            .execute(&pool)
            .await
            // absent-ok: the migrations may already seed the one profile row.
            .ok();
        let s = compute(&pool).await.expect("compute on a fresh box");
        assert!(!s.ai_connected);
        assert!(s.locked, "no model and the door unused = locked");
        assert!(!s.graduated);
        assert!(s.steps.iter().all(|st| st.status == StepStatus::Open));
        assert_eq!(s.steps.iter().map(|st| st.id).collect::<Vec<_>>(), STEP_IDS);

        set_skipped(&pool, "connect_ai", true).await.unwrap();
        let s = compute(&pool).await.unwrap();
        assert!(!s.ai_connected, "the door does not connect anything");
        assert!(!s.locked, "the door opens the app");
        assert_eq!(s.step("connect_ai").unwrap().status, StepStatus::Skipped);
        Ok(())
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn skip_is_the_only_stored_state(pool: PgPool) -> sqlx::Result<()> {
        sqlx::query("INSERT INTO app_user_profile DEFAULT VALUES")
            .execute(&pool)
            .await
            // absent-ok: the migrations may already seed the one profile row.
            .ok();
        set_skipped(&pool, "introductions", true).await.unwrap();
        set_skipped(&pool, "introductions", true).await.unwrap();
        let d: Vec<String> =
            sqlx::query_scalar("SELECT getting_started_dismissed FROM app_user_profile LIMIT 1")
                .fetch_one(&pool)
                .await?;
        assert_eq!(d, vec!["introductions".to_string()], "idempotent, no duplicates");
        set_skipped(&pool, "introductions", false).await.unwrap();
        let d: Vec<String> =
            sqlx::query_scalar("SELECT getting_started_dismissed FROM app_user_profile LIMIT 1")
                .fetch_one(&pool)
                .await?;
        assert!(d.is_empty());
        assert!(set_skipped(&pool, "further", true).await.is_err(), "dead ids are refused");
        Ok(())
    }
}
