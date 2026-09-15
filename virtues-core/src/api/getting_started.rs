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
    /// Integrations in place, for the step that counts them: the ask and
    /// the settled line read differently with two connected than with none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connected: Option<i64>,
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
        "connect_world" => "Integrations",
        "interview" => "Your story",
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

    let profile = sqlx::query_as::<_, (Option<String>, Option<String>, Vec<String>, Option<chrono::DateTime<chrono::Utc>>)>(
        "SELECT preferred_name, full_name, getting_started_dismissed, interview_started_at \
           FROM app_user_profile LIMIT 1",
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("read profile for getting started: {e}")))?;
    // absent-ok: no profile row yet IS the fresh box — nothing named, nothing skipped.
    let (preferred_name, full_name, dismissed, interview_started_at) =
        profile.unwrap_or((None, None, Vec::new(), None));
    // Either name settles the step, the way `get_user_name` reads them: someone
    // who gives only "Nick Ari" and no nickname has still introduced themselves.
    let named = [&preferred_name, &full_name]
        .into_iter()
        .any(|n| n.as_deref().is_some_and(|n| !n.trim().is_empty()));
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
    // `detail` is the one thing still to see to, if any — a collector
    // running with a permission denied. The count has its own field.
    let world_detail = if setup.degraded.is_empty() {
        None
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
            connected: None,
            underway: false,
            acknowledged: skipped("connect_ai"),
        },
        Step {
            id: "introductions",
            title: title("introductions"),
            status: status("introductions", named),
            via: None,
            detail: None,
            connected: None,
            underway: false,
            acknowledged: skipped("introductions"),
        },
        Step {
            id: "connect_world",
            title: title("connect_world"),
            status: status("connect_world", world),
            via: None,
            detail: world_detail,
            connected: Some(integrations),
            underway: false,
            acknowledged: skipped("connect_world"),
        },
        Step {
            id: "interview",
            title: title("interview"),
            status: status("interview", interview_done),
            via: None,
            detail: None,
            connected: None,
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
    answer(speak_then_state(pool).await)
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

// ─── The room's voice ────────────────────────────────────────────────────
//
// Everything the room says is a real message in the transcript, appended
// once, in order, and marked by `subject` so it is never said twice. This
// replaced a client that re-rendered its lines from state on every change
// and so asked a question again underneath the answer to it (2026-09-14).
// Because the lines are turns, the model reading this chat sees exactly what
// the room said, and the client has nothing to place.

/// The marker on every line the room speaks: `gs:<id>`.
fn subject_of(line: &str) -> String {
    format!("gs:{line}")
}

const WELCOME: &str = "# Getting started\n\nWelcome in. Virtues records, remembers, and recounts your life.\n\nFour things have to be in place before it can start: connecting AI, introductions, your integrations, and your story.";

const GRADUATED: &str =
    "That is all four. This room stays open for questions about your setup; the rest of Virtues is yours.";

/// What a step says once it is settled — done or set aside. Two names and
/// only two: "your server" is the box, "your assistant" is who you talk to
/// (Ari, unless renamed). "AI" survives in the step's title and the
/// subscription's pitch; "the model", "it can think", and the rest are out.
fn settled_line(s: &Step) -> String {
    match (s.id, s.status) {
        ("connect_ai", StepStatus::Skipped) => "You went on without connecting AI. Your server can show you its record, but your assistant cannot answer until AI is connected in Settings.".into(),
        ("introductions", StepStatus::Skipped) => "You set introductions aside for now. Say the word whenever you would like to return to them.".into(),
        ("connect_world", StepStatus::Skipped) => "You set your integrations aside for now. They are waiting in Settings whenever you want them.".into(),
        ("interview", StepStatus::Skipped) => "You set your story aside for now. The interview is waiting here whenever you want it.".into(),
        ("connect_ai", _) if s.via == Some("byo") => "Your server is connected to your own models. Your assistant can answer now.".into(),
        ("connect_ai", _) => "Your server is connected to your Virtues subscription. Your assistant can answer now.".into(),
        ("introductions", _) => "Introductions are made.".into(),
        ("connect_world", _) => match &s.detail {
            Some(d) => format!("The record is being written from what you connected. One thing still to see to: {d}."),
            None => "The record is being written from what you connected.".into(),
        },
        ("interview", _) => "Your story is written down, in your own words.".into(),
        _ => String::new(),
    }
}

/// What a step asks when its turn comes.
fn ask_line(s: &Step) -> String {
    match s.id {
        // The buttons under this say Subscribe, Sign in and My own models, so
        // the prose does not enumerate the doors. It spends its words on the
        // one thing that needs arguing.
        "connect_ai" => "AI has to be connected before anything else works. A Virtues subscription gives you the best of Claude, Gemini, GPT and Grok under zero data retention — nothing you send is stored or trained on, by them or by us.".into(),
        // Plainly, as a list: these five are the most important thing on the
        // screen, and buried in a sentence they read as decoration. The
        // person still answers in one message, in their own order.
        "introductions" => "Your assistant would like to know who it is talking to. Answer in one message, however you like to write it:\n\n- Your name, first and last\n- What you would like to be called\n- What to call your assistant, which answers to Ari unless you say otherwise\n- The city you live in\n- Your birth date, with the year\n\nThe birth date is not idle curiosity: the whole record is laid out against it.".into(),
        // What this step is for, the minimum, what happens next, and the
        // one safety fact — not a list of sources: the rows under it say
        // what each one holds. Read differently when some are already in.
        "connect_world" => match s.connected {
            Some(n) if n > 0 => format!(
                "Next, your integrations: what the record is written from. You already have {n} connected, and the record is being written from {}. Add more now, or later from Settings. Nothing they hold ever leaves your server.",
                if n == 1 { "it" } else { "them" }
            ),
            _ => "Next, your integrations: what the record is written from. One is enough to begin; the rest can come later from Settings. Nothing they hold ever leaves your server.".into(),
        },
        "interview" => "Last comes your story. The record can hold what happened; only you can say what it meant. This is an interview with your assistant of about twenty minutes, one question at a time. Stop wherever you like; your place is kept.".into(),
        _ => String::new(),
    }
}

fn promise_line(first_day: Option<chrono::NaiveDate>) -> String {
    match first_day {
        Some(d) => format!(
            "Your first page is on Home: {}, written down from what your integrations hold. There will be one every morning.",
            d.format("%A, %B %-d")
        ),
        None => "Tomorrow morning there will be a page on Home for today, written from what your integrations hold. There will be one every morning after.".into(),
    }
}

/// The lines the room should have spoken by now, in order.
fn script(state: &GettingStartedState) -> Vec<(String, String)> {
    let mut out = vec![("welcome".to_string(), WELCOME.to_string())];
    for s in &state.steps {
        match s.status {
            StepStatus::Open => {
                out.push((format!("ask:{}", s.id), ask_line(s)));
                // One ask at a time: the room says nothing past the step it
                // is waiting on.
                return out;
            }
            _ => {
                out.push((format!("done:{}", s.id), settled_line(s)));
                // Only a real connection earns the promise: nothing is written
                // overnight for someone who set their integrations aside.
                if s.id == "connect_world" && s.status == StepStatus::Done && state.ai_connected {
                    out.push(("promise".to_string(), promise_line(state.first_day)));
                }
            }
        }
    }
    out.push(("graduated".to_string(), GRADUATED.to_string()));
    out
}

/// Speak whatever the room owes, and UNSAY whatever is no longer true.
///
/// The room's lines are a function of the walk, exactly as its steps are, so
/// this reconciles the thread to [`script`] rather than only appending to it.
/// Appending alone was a one-way ratchet: a step that REGRESSES — a lapsed
/// subscription, a credential revoked, an integration disconnected — left its
/// settled line standing and put the new ask at the bottom, so the room said
/// "your server is connected" near the top and "nothing begins until AI is
/// connected" underneath, with the buttons. Seen on the dev box when the core
/// restarted without `VIRTUES_DEV_SKIP_SETUP`; a lapsed subscription does the
/// same thing to a real box.
///
/// Only the room's own lines are touched (`gs:` subjects). The person's turns
/// carry no subject, and other subjects — a stopped turn's `cancelled`, a
/// truncated one's `length` — are not the room's to remove.
pub async fn narrate(pool: &PgPool, state: &GettingStartedState) -> Result<()> {
    let lines = script(state);
    let wanted: Vec<String> = lines
        .iter()
        .filter(|(_, text)| !text.is_empty())
        .map(|(line, _)| subject_of(line))
        .collect();

    // Unsay first, so what remains is only ever a prefix of the script and the
    // appends below land in the script's own order.
    let stale: Vec<String> = sqlx::query_scalar(
        "DELETE FROM app_chat_messages \
         WHERE chat_id = $1 AND subject LIKE 'gs:%' AND subject <> ALL($2) \
         RETURNING subject",
    )
    .bind(GETTING_STARTED_CHAT_ID)
    .bind(&wanted)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("the room could not unsay a line: {e}")))?;
    if !stale.is_empty() {
        sqlx::query(
            "UPDATE app_chats SET message_count = GREATEST(message_count - $2, 0), \
             updated_at = now() WHERE id = $1",
        )
        .bind(GETTING_STARTED_CHAT_ID)
        .bind(stale.len() as i32)
        .execute(pool)
        .await
        .map_err(|e| Error::Database(format!("the room's count: {e}")))?;
    }

    let said: Vec<String> = sqlx::query_scalar(
        "SELECT subject FROM app_chat_messages \
         WHERE chat_id = $1 AND subject IS NOT NULL",
    )
    .bind(GETTING_STARTED_CHAT_ID)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("read what the room has said: {e}")))?;

    for (line, text) in lines {
        let subject = subject_of(&line);
        if text.is_empty() || said.iter().any(|s| *s == subject) {
            continue;
        }
        let msg = crate::api::chats::ChatMessage {
            id: None,
            role: "assistant".to_string(),
            content: text,
            timestamp: crate::types::Timestamp::now(),
            model: None,
            provider: None,
            agent_id: Some("getting_started".to_string()),
            tool_calls: None,
            reasoning: None,
            intent: None,
            subject: Some(subject),
            reasoning_details: None,
            parts: None,
        };
        crate::api::chats::append_message(pool, GETTING_STARTED_CHAT_ID.to_string(), msg)
            .await
            .map_err(|e| Error::Database(format!("the room could not speak: {e}")))?;
    }
    Ok(())
}

/// The state, with whatever the room owes said first. Every handler answers
/// through this: a step settled by a POST has to leave its line in the
/// thread, or the conversation silently skips a beat.
async fn speak_then_state(pool: &PgPool) -> Result<GettingStartedState> {
    let s = compute(pool).await?;
    if let Err(e) = narrate(pool, &s).await {
        // The state is still true if the room could not speak: say so and
        // answer, rather than failing the read over a line of dialogue.
        tracing::warn!(error = %e, "getting-started narration failed");
    }
    Ok(s)
}

fn answer(r: Result<GettingStartedState>) -> axum::response::Response {
    match r {
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

/// `GET /api/getting-started`
pub async fn state_handler(State(state): State<AppState>, _user: AuthUser) -> impl IntoResponse {
    match compute(state.db.pool()).await {
        Ok(s) => {
            // The room says what it owes before answering: this endpoint is
            // read after every change, so it is where the thread catches up.
            if let Err(e) = narrate(state.db.pool(), &s).await {
                tracing::warn!(error = %e, "getting-started narration failed");
            }
            (StatusCode::OK, Json(s)).into_response()
        }
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
    answer(speak_then_state(pool).await)
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
                connected: None,
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
        assert!(block.contains("- connect_world: Integrations (skipped)"));
        assert!(block.contains("first day written up: not yet"));
    }

    /// A step that regresses must not leave its settled line standing: the
    /// script is the whole truth, so what the room has said is reconciled to
    /// it, not merely added to. (The delete itself is exercised by the
    /// endpoint; this pins the script, which is what drives it.)
    #[test]
    fn a_regressed_step_drops_the_lines_after_it() {
        use StepStatus::*;
        let forward = state([Done, Open, Open, Open], true, false);
        let subjects = |st: &GettingStartedState| -> Vec<String> {
            script(st).into_iter().map(|(l, _)| l).collect()
        };
        assert_eq!(
            subjects(&forward),
            vec!["welcome", "done:connect_ai", "ask:introductions"]
        );

        // AI goes away again: the settled line and the next ask are no longer
        // in the script, so narrate() removes them.
        let back = state([Open, Open, Open, Open], false, false);
        assert_eq!(subjects(&back), vec!["welcome", "ask:connect_ai"]);
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
