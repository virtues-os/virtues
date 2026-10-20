//! `browse`: a browsing task run by its own small loop.
//!
//! Driving a browser is many cheap steps: read the page, click, read it again.
//! Run inside the chat, every step re-reads the chat's whole context. On
//! 2026-10-09 that was 28k tokens of system prompt, tools and history on each
//! of a 20-step task's calls, three quarters of what the task cost. Here the
//! steps run in a loop that sees only the task, a short prompt and the browser
//! tools, and the chat reads one answer.
//!
//! The worker never sees the chat or the owner's record and holds no tool but
//! the browser's, so a page that tells the model what to do can steer only the
//! browsing. Design and measurements: agents/record/browser-subagent.md.

use std::sync::OnceLock;
use std::time::{Duration, Instant};

use futures::StreamExt;
use serde::Serialize;
use serde_json::{json, Value};
use sqlx::PgPool;
use tokio_util::sync::CancellationToken;

use crate::agent::{AgentConfig, AgentEvent, AgentLoop, DelegatedUsage, FinishReason, Thinking, TurnBudget};
use crate::api::compaction::build_context_for_llm;
use crate::tools::{SubagentStatus, SubagentUpdate, ToolContext, ToolResult, WorkerStep};

/// The tool the chat is offered while a browser is connected.
pub const TOOL: &str = "browse";

const MAX_STEPS: u32 = 30;
const MAX_COST_MICROS: i64 = 250_000;
/// Five minutes of browsing, plus the fifteen a handoff may spend waiting on
/// the owner: the budget's clock does not pause for one, and a sign-in that
/// ran it out would leave the worker one step with no tools.
pub const MAX_WALL_CLOCK: Duration = Duration::from_secs(20 * 60);

/// One browser, so one driver. A second task, from this chat or another,
/// waits its turn rather than clicking through the first one's page.
fn driving() -> &'static tokio::sync::Mutex<()> {
    static DRIVING: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    DRIVING.get_or_init(|| tokio::sync::Mutex::new(()))
}

/// What a browse run hands back.
#[derive(Debug, Serialize)]
pub struct Outcome {
    /// The worker's last reply: what the task asked for, as the page showed it.
    pub answer: String,
    /// `done`, `taken_over`, `declined`, `budget`, `max_steps`, `stopped` or
    /// `error`.
    pub ended: &'static str,
    /// The page the browser was last on, as the browser reported it.
    pub url: Option<String>,
    pub steps: Vec<Step>,
    pub usage: DelegatedUsage,
    pub seconds: f64,
}

/// One browser call the worker made.
#[derive(Debug, Serialize)]
pub struct Step {
    pub tool: String,
    pub args: Value,
    pub ok: bool,
}

/// The chat's `browse` call: run the task on the Lite slot's model and hand
/// the chat its answer, with the spend it made.
pub async fn run_tool(pool: &PgPool, args: &Value, context: &ToolContext) -> ToolResult {
    let Some(task) = args.get("task").and_then(|t| t.as_str()).filter(|t| !t.trim().is_empty()) else {
        return ToolResult::error("browse needs a `task`.");
    };
    if !crate::browser::connected() {
        return ToolResult::error(crate::browser::NOT_OPEN);
    }
    let model = match crate::virtues_api::completion::background_model_for_slot(
        pool,
        virtues_registry::models::ModelSlot::Lite,
    )
    .await
    {
        // An empty pin is not a model; the profile can hold one.
        Ok(m) if !m.trim().is_empty() => m,
        Ok(_) => crate::api::model_catalog::model_for_slot(virtues_registry::models::ModelSlot::Lite),
        Err(e) => return ToolResult::error(format!("Could not read which model browses: {e}")),
    };
    // Its own cache session beside the chat's: the provider keeps this
    // chat's browsing prompt warm for its next task, and the chat's own
    // prefix is left alone.
    let session = context.chat_id.as_ref().map(|chat| format!("{chat}:browse"));
    let progress = Progress::new(context.subagent_tx.clone(), &model);
    let outcome = run(pool, task, &model, session, context.cancel_token.clone(), progress).await;

    let usage = DelegatedUsage { feature: Some(TOOL.to_string()), ..outcome.usage };
    ToolResult::success(json!({
        "answer": outcome.answer,
        "ended": outcome.ended,
        "url": outcome.url,
        "steps": outcome.steps.len(),
        "usage": [usage],
    }))
}

/// Run one browsing task on `model` against the connected browser. `session`
/// keys the provider's cache; the worker's calls log under it.
pub async fn run(
    pool: &PgPool,
    task: &str,
    model: &str,
    session: Option<String>,
    cancel: Option<CancellationToken>,
    progress: Progress,
) -> Outcome {
    let _driving = driving().lock().await;
    let started = Instant::now();
    let mut messages = build_context_for_llm(&[], None, 0, Some(PROMPT), None, None);
    // The task goes after the prompt, so successive tasks share the prompt's
    // cache.
    messages.push(json!({ "role": "user", "content": task }));

    let agent = AgentLoop::new(pool.clone())
        .with_config(AgentConfig {
            max_steps: MAX_STEPS,
            // The browser does one thing at a time, in arrival order; calls
            // sent together would race to arrive.
            parallel_tools: false,
            thinking: Thinking::Low,
            ..Default::default()
        })
        .with_budget(TurnBudget {
            max_cost_micros: Some(MAX_COST_MICROS),
            max_wall_clock: Some(MAX_WALL_CLOCK),
            tool_caps: &[],
        });
    // Not the chat's id: none of the browser tools is gated, and the loop
    // reads this only to key the provider's cache and to label its logs.
    let context = ToolContext {
        user_id: Some(TOOL.to_string()),
        chat_id: session,
        cancel_token: cancel.clone(),
        ..Default::default()
    };
    let tools = crate::tools::tools_named(crate::browser::TOOLS);
    let mut stream = agent.run(model.to_string(), messages, tools, context, cancel);

    let mut answer = String::new();
    let mut ended = "error";
    let mut url = None;
    let mut steps: Vec<Step> = Vec::new();
    let mut pending: Vec<(String, String, Value)> = Vec::new();
    let mut usage = DelegatedUsage { model: model.to_string(), ..Default::default() };

    while let Some(event) = stream.next().await {
        match event {
            AgentEvent::TextDelta { content } => answer.push_str(&content),
            // Text before a tool call narrates the step; only the text after
            // the last call is the answer.
            AgentEvent::ToolCallStart { id, name, args } => {
                answer.clear();
                pending.push((id, name, args.unwrap_or(Value::Null)));
            }
            AgentEvent::ToolCallArgsComplete { id, args } => {
                if let Some(p) = pending.iter_mut().find(|p| p.0 == id) {
                    progress.step(&p.1, &args);
                    p.2 = args;
                }
            }
            AgentEvent::ToolCallResult { id, result, success, error } => {
                if let Some(page) = result.get("url").and_then(|u| u.as_str()) {
                    url = Some(page.to_string());
                }
                if let Some(i) = pending.iter().position(|p| p.0 == id) {
                    let (_, tool, args) = pending.remove(i);
                    if !success {
                        ended = failure_ending(&tool, error.as_deref().unwrap_or("")).unwrap_or(ended);
                    }
                    steps.push(Step { tool, args, ok: success });
                }
            }
            AgentEvent::Usage {
                prompt_tokens,
                completion_tokens,
                reasoning_tokens,
                cache_read_tokens,
                cost_micros,
                ..
            } => {
                usage.input_tokens += u64::from(prompt_tokens);
                usage.output_tokens += u64::from(completion_tokens);
                usage.reasoning_tokens += u64::from(reasoning_tokens.unwrap_or(0));
                usage.cache_read_tokens += u64::from(cache_read_tokens.unwrap_or(0));
                usage.cost_micros += cost_micros.unwrap_or(0);
            }
            AgentEvent::Done { finish_reason, .. } => {
                if ended == "error" {
                    ended = finished(finish_reason);
                }
            }
            AgentEvent::Error { message, .. } => {
                tracing::warn!(error = %message, "browse worker error");
            }
            _ => {}
        }
    }

    progress.finish(ended == "done");
    Outcome {
        answer: answer.trim().to_string(),
        ended,
        url,
        steps,
        usage,
        seconds: started.elapsed().as_secs_f64(),
    }
}

/// The worker's steps as they happen, for the chat's status line. The chat
/// drains this channel while the tool runs; a full channel drops a step
/// rather than holding the browser up, since the next step replaces it.
pub struct Progress {
    tx: Option<tokio::sync::mpsc::Sender<SubagentUpdate>>,
    dispatch_id: u64,
    model: String,
}

impl Progress {
    pub fn new(tx: Option<tokio::sync::mpsc::Sender<SubagentUpdate>>, model: &str) -> Self {
        static RUNS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        Self { tx, dispatch_id: RUNS.fetch_add(1, std::sync::atomic::Ordering::Relaxed), model: model.to_string() }
    }

    fn send(&self, status: SubagentStatus, step: Option<WorkerStep>) {
        if let Some(tx) = &self.tx {
            let _ = tx.try_send(SubagentUpdate {
                dispatch_id: self.dispatch_id,
                id: 0,
                title: TOOL.to_string(),
                model: self.model.clone(),
                status,
                tokens: 0,
                step,
            });
        }
    }

    fn step(&self, tool: &str, input: &Value) {
        self.send(SubagentStatus::Thinking, Some(WorkerStep { tool: tool.to_string(), input: input.clone() }));
    }

    fn finish(&self, ok: bool) {
        self.send(if ok { SubagentStatus::Done } else { SubagentStatus::Failed }, None);
    }
}

/// How the run ended, when the loop ended it.
fn finished(reason: FinishReason) -> &'static str {
    match reason {
        FinishReason::EndTurn => "done",
        FinishReason::MaxSteps => "max_steps",
        FinishReason::BudgetExceeded => "budget",
        FinishReason::Cancelled => "stopped",
        _ => "error",
    }
}

/// A failed browser call that decides how the run ended: the owner took the
/// browser (the Mac app's `TAKEN_OVER` text, `browser_host.rs`), or did not
/// finish a step handed to them.
fn failure_ending(tool: &str, error: &str) -> Option<&'static str> {
    if error.contains("has taken over the browser") {
        Some("taken_over")
    } else if tool == "browser_handoff" {
        Some("declined")
    } else {
        None
    }
}

/// What the worker knows beyond the tool descriptions. Each part is a slot
/// the reply fills or a habit with a cost behind it, not a prohibition.
const PROMPT: &str = "\
You operate the owner's web browser to do one task, given in the next message. \
The browser is a window in the Virtues app on their Mac, signed in to the sites \
they use. You have only the browser tools. When the task is done, your reply \
goes back to the assistant that asked, which talks to the owner.

How to read a page:
- Start with browser_snapshot to see where the browser is, unless the task \
gives a URL to open.
- On a long page, snapshot with `depth` 6 for the overview, then with `ref` \
for the part you need.
- After scrolling or a click that changes the page, read it with a fresh \
snapshot. A screenshot is for what the outline cannot show: a seat map, a \
chart, an image, an empty outline. It costs several times a snapshot. \
Words drawn inside an image, such as a comic, a chart or a scanned page, are \
not in the outline at all: read them from a screenshot.

Steps that belong to the owner go to browser_handoff: signing in, passwords, \
one-time codes, CAPTCHAs, payment details. When it returns, snapshot again.

The task says how far to go. When finishing it would buy, send, post, \
cancel, delete or accept something the task did not name, stop there and \
reply with what the owner needs to decide.

Your reply, once the task is answered or cannot go further:
- First, the answer: the values the task asked for, copied exactly as the \
page shows them (codes, names, dates, times, seats, prices), each with where \
you read it: the outline or a screenshot.
- Then one line on what you changed, if you changed anything (a form \
submitted, a setting switched).
- If you stopped early, one line on why and what the owner can do.
Write it for the assistant, plainly, with no greeting.";

/// `POST /api/debug/browse {task, model}` — run a task without a chat, for
/// measuring the worker against a model. Debug builds only.
#[cfg(debug_assertions)]
pub fn debug_routes() -> axum::Router<crate::server::AppState> {
    use axum::extract::State;
    use axum::routing::post;
    use axum::Json;

    #[derive(serde::Deserialize)]
    struct Request {
        task: String,
        model: Option<String>,
    }

    async fn handler(State(state): State<crate::server::AppState>, Json(req): Json<Request>) -> Json<Outcome> {
        let model = req.model.unwrap_or_else(|| {
            crate::api::model_catalog::model_for_slot(virtues_registry::models::ModelSlot::Lite)
        });
        Json(run(state.db.pool(), &req.task, &model, None, None, Progress::new(None, &model)).await)
    }

    axum::Router::new().route("/api/debug/browse", post(handler))
}

#[cfg(not(debug_assertions))]
pub fn debug_routes() -> axum::Router<crate::server::AppState> {
    axum::Router::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_owner_taking_the_browser_ends_the_run_as_taken_over() {
        let taken = "The owner has taken over the browser. Leave it alone until they hand it back: end your turn.";
        assert_eq!(failure_ending("browser_click", taken), Some("taken_over"));
        assert_eq!(failure_ending("browser_handoff", taken), Some("taken_over"));
    }

    #[test]
    fn a_handoff_the_owner_did_not_finish_is_declined() {
        assert_eq!(failure_ending("browser_handoff", "The owner could not do this step."), Some("declined"));
    }

    #[test]
    fn an_ordinary_failed_step_does_not_end_the_run() {
        assert_eq!(failure_ending("browser_click", "No element with ref e12."), None);
    }

    #[tokio::test]
    async fn each_step_reaches_the_chat_and_the_run_ends_the_line() {
        let (tx, mut rx) = tokio::sync::mpsc::channel(8);
        let progress = Progress::new(Some(tx), "lite");
        progress.step("browser_click", &json!({"ref": "e7"}));
        progress.finish(true);
        let first = rx.recv().await.unwrap();
        assert_eq!(first.title, TOOL);
        assert_eq!(first.step, Some(WorkerStep { tool: "browser_click".into(), input: json!({"ref": "e7"}) }));
        let last = rx.recv().await.unwrap();
        assert!(matches!(last.status, SubagentStatus::Done));
        assert_eq!(last.step, None);
    }

    #[test]
    fn a_stopped_loop_reads_as_stopped_not_as_an_error() {
        assert_eq!(finished(FinishReason::Cancelled), "stopped");
        assert_eq!(finished(FinishReason::EndTurn), "done");
        assert_eq!(finished(FinishReason::BudgetExceeded), "budget");
    }
}
