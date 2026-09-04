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
/// An empty completion is an error, not an empty string: the client has
/// already resent genuinely empty answers (reasoning models that spend the
/// whole budget thinking — see `EMPTY_COMPLETION_ATTEMPTS`), so an empty body
/// reaching here means the model has stopped answering, and every current
/// caller treats that as failure anyway.
pub async fn system_completion(
    pool: &PgPool,
    slot: ModelSlot,
    feature: &'static str,
    system_prompt: &str,
    user_prompt: &str,
    max_tokens: u32,
    temperature: f32,
) -> Result<String> {
    let model = background_model_for_slot(pool, slot).await?;

    let client = BearerClient::from_env(pool.clone())
        .with_purpose(Purpose::System)
        .with_feature(feature);

    let response = client
        .post_json(
            "/v1/ai/chat/completions",
            &json!({
                "model": model,
                "messages": [
                    {"role": "system", "content": system_prompt},
                    {"role": "user", "content": user_prompt}
                ],
                "max_tokens": max_tokens,
                "temperature": temperature
            }),
        )
        .await
        .map_err(|e| Error::Network(format!("virtues-api request failed: {e}")))?;

    if !response.is_success() {
        return Err(Error::ExternalApi(match response.status {
            402 => format!("Usage limit reached ({feature})"),
            429 => "Rate limited. Please try again later.".to_string(),
            _ => format!("virtues-api error {}: {}", response.status, response.body),
        }));
    }

    let content = response.body["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("")
        .trim()
        .to_string();

    if content.is_empty() {
        return Err(Error::ExternalApi(format!(
            "empty completion from {model} ({feature})"
        )));
    }

    Ok(content)
}
