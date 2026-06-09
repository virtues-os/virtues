//! Council mode — ask the same question many ways across many models, then synthesize.
//!
//! A "mixture of agents" pattern. Each member is a single-shot, non-streaming completion
//! (the same path as [`crate::api::chats::generate_title`] / compaction summaries), so members
//! are billed per-call automatically and run fully in parallel. The collected answers are then
//! handed to a streamed synthesis turn (see [`build_synthesis_messages`]).

use serde_json::{json, Value};
use sqlx::PgPool;
use tokio::sync::mpsc;

use crate::error::Result;
use crate::virtues_api::client::BearerClient;

/// Framing lenses applied to council members. Member `i` gets `LENSES[i % LENSES.len()]`, so
/// members spread across distinct framings (and, combined with model round-robin, distinct
/// providers) to neutralize any single model's blind spots.
///
/// `(name, system instruction)` — the instruction is layered on top of the normal system prompt.
const LENSES: &[(&str, &str)] = &[
    (
        "technical",
        "Answer with rigorous technical depth: mechanisms, trade-offs, and concrete specifics. Prefer precision over breadth.",
    ),
    (
        "contrarian",
        "Take a deliberately contrarian stance. Challenge the obvious answer, surface what most people get wrong, and argue the under-considered side.",
    ),
    (
        "first-principles",
        "Reason from first principles. Set aside convention and received wisdom; build the answer up from fundamentals.",
    ),
    (
        "practical",
        "Answer for someone who has to act today. Be concrete, pragmatic, and bias toward what actually works in practice.",
    ),
    (
        "creative",
        "Answer with lateral, creative thinking. Offer non-obvious angles, analogies, and possibilities others would miss.",
    ),
    (
        "skeptical",
        "Be rigorously skeptical. Demand evidence, flag assumptions, and note where confidence should be low or claims are unverifiable.",
    ),
    (
        "holistic",
        "Take a holistic, systems view. Consider second-order effects, context, stakeholders, and how the parts interact.",
    ),
    (
        "concrete",
        "Answer with concrete examples, numbers, and specifics. Avoid abstraction; ground every point in something tangible.",
    ),
];

/// Lifecycle status of a single council member, streamed to the client for the live panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemberStatus {
    Thinking,
    Done,
    Failed,
}

impl MemberStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            MemberStatus::Thinking => "thinking",
            MemberStatus::Done => "done",
            MemberStatus::Failed => "failed",
        }
    }
}

/// An update emitted as council members start and finish. Drives the live deliberation panel and,
/// on `Done`, carries the member's full answer for the synthesis step.
#[derive(Debug, Clone)]
pub struct CouncilUpdate {
    pub member_id: usize,
    pub model: String,
    pub lens: String,
    pub status: MemberStatus,
    pub tokens: u32,
    /// Present only on `Done` — the member's answer, collected to build the synthesis.
    pub answer: Option<String>,
}

/// Minimum members that must succeed before we synthesize. Below this the agreement signal is too
/// weak to be worth it, so the caller falls back to a normal single-model answer.
pub const MIN_SURVIVORS: usize = 2;

/// Default council models when the user hasn't chosen any: every enabled registry model.
pub fn default_council_models() -> Vec<String> {
    virtues_registry::models::default_models()
        .into_iter()
        .filter(|m| m.enabled)
        .map(|m| m.model_id)
        .collect()
}

/// Spawn `member_count` council members and return a receiver of their live updates.
///
/// Each member fires immediately and runs concurrently; the receiver yields a `Thinking` update as
/// each starts and a `Done`/`Failed` update as each finishes. The channel closes once every member
/// has reported, at which point the caller has all answers.
pub fn run_council(
    pool: PgPool,
    base_messages: Vec<Value>,
    models: Vec<String>,
    member_count: usize,
) -> mpsc::Receiver<CouncilUpdate> {
    let member_count = member_count.max(1);
    let (tx, rx) = mpsc::channel(member_count * 2 + 1);

    let models = if models.is_empty() {
        default_council_models()
    } else {
        models
    };
    // Guard against an empty registry so the modulo below can't panic.
    let models = if models.is_empty() {
        vec![virtues_registry::models::default_model_for_slot(
            virtues_registry::models::ModelSlot::Chat,
        )
        .to_string()]
    } else {
        models
    };

    for i in 0..member_count {
        let (lens_name, lens_instruction) = LENSES[i % LENSES.len()];
        let model = models[i % models.len()].clone();
        let messages = build_member_messages(&base_messages, lens_instruction);
        let tx = tx.clone();
        let pool = pool.clone();
        let lens_name = lens_name.to_string();

        tokio::spawn(async move {
            // Member started — the panel shows it thinking immediately.
            let _ = tx
                .send(CouncilUpdate {
                    member_id: i,
                    model: model.clone(),
                    lens: lens_name.clone(),
                    status: MemberStatus::Thinking,
                    tokens: 0,
                    answer: None,
                })
                .await;

            let update = match call_member(&pool, &model, &messages).await {
                Ok((answer, tokens)) => CouncilUpdate {
                    member_id: i,
                    model,
                    lens: lens_name,
                    status: MemberStatus::Done,
                    tokens,
                    answer: Some(answer),
                },
                Err(e) => {
                    tracing::warn!(member = i, error = %e, "Council member failed");
                    CouncilUpdate {
                        member_id: i,
                        model,
                        lens: lens_name,
                        status: MemberStatus::Failed,
                        tokens: 0,
                        answer: None,
                    }
                }
            };
            let _ = tx.send(update).await;
        });
    }

    rx
}

/// One council member: a single-shot, non-streaming completion through virtues-api (auto-renews on
/// 402, billed per-call). Returns `(answer, completion_tokens)`.
async fn call_member(pool: &PgPool, model: &str, messages: &[Value]) -> Result<(String, u32)> {
    let client = BearerClient::from_env(pool.clone());
    let response = client
        .post_json(
            "/v1/ai/chat/completions",
            &json!({
                "model": model,
                "messages": messages,
                "max_tokens": 1024,
            }),
        )
        .await
        .map_err(|e| crate::Error::Network(format!("council member request failed: {e}")))?;

    if !response.is_success() {
        return Err(crate::Error::ExternalApi(format!(
            "council member error {}: {}",
            response.status, response.body
        )));
    }

    let body = response.body;
    let answer = body["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("")
        .trim()
        .to_string();
    if answer.is_empty() {
        return Err(crate::Error::ExternalApi(
            "council member returned empty content".to_string(),
        ));
    }
    let tokens = body["usage"]["completion_tokens"].as_u64().unwrap_or(0) as u32;
    Ok((answer, tokens))
}

/// Layer a member's lens on top of the normal conversation: insert the lens as a system message
/// right after the primary system prompt, keeping the user's question last.
fn build_member_messages(base: &[Value], lens_instruction: &str) -> Vec<Value> {
    let lens_msg = json!({
        "role": "system",
        "content": format!("Council framing for your answer: {lens_instruction}"),
    });

    let mut out = Vec::with_capacity(base.len() + 1);
    if base.first().and_then(|m| m["role"].as_str()) == Some("system") {
        out.push(base[0].clone());
        out.push(lens_msg);
        out.extend(base[1..].iter().cloned());
    } else {
        out.push(lens_msg);
        out.extend(base.iter().cloned());
    }
    out
}

/// Build the synthesis turn: the original conversation plus every member answer and the synthesis
/// instruction, injected as a system message after the primary system prompt (user question stays
/// last). This is streamed to the user as the final answer.
pub fn build_synthesis_messages(base: &[Value], answers: &[String]) -> Vec<Value> {
    use crate::agent::prompt::COUNCIL_SYNTHESIS_PROMPT;

    let mut block = String::from("<council_responses>\n");
    for (i, a) in answers.iter().enumerate() {
        block.push_str(&format!("<response n=\"{}\">\n{}\n</response>\n", i + 1, a));
    }
    block.push_str("</council_responses>");

    let synthesis_system = json!({
        "role": "system",
        "content": format!("{COUNCIL_SYNTHESIS_PROMPT}\n\n{block}"),
    });

    let mut out = Vec::with_capacity(base.len() + 1);
    if base.first().and_then(|m| m["role"].as_str()) == Some("system") {
        out.push(base[0].clone());
        out.push(synthesis_system);
        out.extend(base[1..].iter().cloned());
    } else {
        out.push(synthesis_system);
        out.extend(base.iter().cloned());
    }
    out
}
