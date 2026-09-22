//! One door for background (system-initiated) chat completions.
//!
//! Every nightly/maintenance writer used to hand-roll the same call: resolve a
//! model, tag `Purpose::System`, post to `/v1/ai/chat/completions`, map the
//! status, dig `choices[0].message.content` out of the body. The dangerous
//! step was the first one — "which model" and "is this a background call" were
//! decided separately at every site, and they had to agree. Three sites got it
//! wrong the same way (narrative_draft, then both day_summary calls): they
//! read the owner's pinned chat model, and a pin no ZDR provider serves
//! (grok, notably) failed every such write with `no_zdr_providers_available`.
//! The pin governs the chat the owner watches, not a background write.
//!
//! Here the model follows from the slot, decided once:
//!
//! - `Chat` — the SLOT DEFAULT via `model_catalog::model_for_slot`, never the
//!   profile pin. The Virtues-curated slot map stays ZDR-capable.
//! - `Lite` — the profile's background pin (`get_background_model`), because
//!   the Lite pin exists FOR background work: it is the owner's one cost lever
//!   over these jobs (see entity_article_gen's history — a premium chat pin
//!   once made every applet call premium too). Honoring it here is the point.
//! - anything else — the slot default.
//!
//! A new background caller goes through [`system_completion`] and never
//! touches model resolution at all.

use serde_json::json;
use sqlx::PgPool;
use virtues_registry::models::ModelSlot;

use super::client::{BearerClient, Purpose};
use crate::error::{Error, Result};

/// Resolve the model a background job should use for `slot`.
///
/// See the module doc for why `Chat` resolves to the slot default while `Lite`
/// honors the profile's background pin.
pub async fn background_model_for_slot(pool: &PgPool, slot: ModelSlot) -> Result<String> {
    Ok(match slot {
        ModelSlot::Lite => crate::api::assistant_profile::get_background_model(pool).await?,
        other => crate::api::model_catalog::model_for_slot(other),
    })
}

/// One background chat completion: system prompt + user prompt in, prose out.
///
/// `feature` tags the spend into `app_ai_calls` so Usage can attribute it.
///
/// `thinking` is what the job wants from the model's reasoning, and the ONLY
/// lever a caller has. There is no `max_tokens`: on a model that thinks, the
/// thinking is counted inside that cap, and a cap sized for the answer
/// returned nothing. Measured on the box 2026-09-04..08, the Chat slot's
/// model segmenting a day under a 4000 cap spent exactly 4000 tokens
/// reasoning and returned no content on 237 of 276 calls, every one billed.
/// The interview drafter failed the same way four days later. The window is
/// the model's own; the prompt bounds the output.
///
/// The thinking mode becomes a request through `request::reasoning_for`,
/// from what the catalog says about the model and whether the call goes to
/// the owner's own endpoint. A model without the lever ignores the hint.
///
/// A completion the model did not finish (`finish_reason: length`) is an
/// error naming that, distinct from an empty one: "ran out of room" and
/// "said nothing" used to be the same message.
pub async fn system_completion(
    pool: &PgPool,
    slot: ModelSlot,
    feature: &'static str,
    system_prompt: &str,
    user_prompt: &str,
    thinking: super::request::Thinking,
    temperature: f32,
) -> Result<String> {
    system_completion_content(
        pool,
        slot,
        feature,
        system_prompt,
        serde_json::Value::String(user_prompt.to_string()),
        thinking,
        temperature,
    )
    .await
}

/// [`system_completion`] with the user turn as raw message content: a string,
/// or an OpenAI-shaped array of content parts. The parts form is how a
/// background job shows the model pixels — a `text` part followed by
/// `{"type":"image_url","image_url":{"url":"data:<mime>;base64,..."}}`, the
/// same blocks a pasted screenshot produces in chat (see
/// `agent::executor::build_attachment_message`).
///
/// A second entry point rather than a second function: model resolution, the
/// thinking lever, spend tagging and every failure mode stay in one place, and
/// only the shape of one message differs.
pub async fn system_completion_content(
    pool: &PgPool,
    slot: ModelSlot,
    feature: &'static str,
    system_prompt: &str,
    user_content: serde_json::Value,
    thinking: super::request::Thinking,
    temperature: f32,
) -> Result<String> {
    let model = background_model_for_slot(pool, slot).await?;
    let byo = crate::api::settings_byo::byo_is_active(pool).await;
    let facts = crate::api::model_catalog::reasoning_facts(&model);
    let (reasoning, reasoning_effort) = super::request::reasoning_for(thinking, facts.as_ref(), byo);

    let client = BearerClient::from_env(pool.clone())
        .with_purpose(Purpose::System)
        .with_feature(feature);

    // An empty system prompt is no system message at all (a title asks in
    // one user turn), not an empty block some providers reject.
    let mut messages = Vec::with_capacity(2);
    if !system_prompt.trim().is_empty() {
        messages.push(json!({"role": "system", "content": system_prompt}));
    }
    messages.push(json!({"role": "user", "content": user_content}));
    let request = super::request::ChatCompletionRequest {
        model: model.clone(),
        messages,
        temperature: Some(temperature),
        reasoning,
        reasoning_effort,
        ..Default::default()
    };
    let body = serde_json::to_value(&request)
        .map_err(|e| Error::Other(format!("encode completion request: {e}")))?;

    let response = client
        .post_json("/v1/ai/chat/completions", &body)
        .await
        .map_err(|e| Error::Network(format!("virtues-api request failed: {e}")))?;

    if !response.is_success() {
        return Err(Error::ExternalApi(match response.status {
            402 => crate::virtues_api::client::payment_required_message(&response.body, feature),
            429 => "Rate limited. Please try again later.".to_string(),
            _ => format!("virtues-api error {}: {}", response.status, response.body),
        }));
    }

    let choice = &response.body["choices"][0];
    let content = choice["message"]["content"]
        .as_str()
        .unwrap_or("")
        .trim()
        .to_string();

    if choice["finish_reason"].as_str() == Some("length") {
        return Err(Error::ExternalApi(format!(
            "{feature}: {model} ran out of room before it finished (finish_reason=length, \
             {} chars arrived)",
            content.chars().count()
        )));
    }

    if content.is_empty() {
        return Err(Error::ExternalApi(format!(
            "empty completion from {model} ({feature})"
        )));
    }

    Ok(content)
}
