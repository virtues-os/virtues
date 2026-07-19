//! Text-to-image generation via the AI Gateway.
//!
//! Used by the `generate_image` chat tool: the assistant turns a prompt into an
//! inline image. Routes through virtues-api `/v1/ai/chat/completions` on the
//! device bearer (same as every other AI call) — so the gateway key never lives
//! on the box and image-gen cost is metered through the entitlement.

use sqlx::PgPool;

use crate::error::{Error, Result};

/// Generate an image from `prompt` and return the raw PNG bytes.
///
/// The gateway returns images inline (base64) in the OpenAI-compatible
/// response; we accept the few shapes it uses (Vercel `files[]`, or OpenAI
/// multimodal content parts).
pub async fn generate_image_via_gateway(pool: &PgPool, prompt: &str) -> Result<Vec<u8>> {
    let model = crate::api::assistant_profile::get_image_model(pool).await?;
    let response = crate::virtues_api::client::BearerClient::from_env(pool.clone())
        // User-initiated: the `generate_image` chat tool runs on demand, so it
        // books to the User purpose (the default) — not System, which this file
        // inherited from the deleted nightly day-illustration job.
        .with_feature("generate_image")
        .post_json(
            "/v1/ai/chat/completions",
            &serde_json::json!({
                "model": model,
                "messages": [
                    {"role": "user", "content": prompt}
                ],
                "max_tokens": 4096
            }),
        )
        .await
        .map_err(|e| Error::Network(format!("Image gen request failed: {e}")))?;

    if !response.is_success() {
        return Err(Error::ExternalApi(format!(
            "Image gen error {}: {}",
            response.status, response.body
        )));
    }

    // Vercel AI Gateway returns OpenAI-compat format; for multimodal models
    // images appear as base64 data in content parts.
    let body = response.body;

    // Format 3: files[] array (Vercel AI SDK native format)
    if let Some(files) = body.get("files").and_then(|f| f.as_array()) {
        for file in files {
            if let (Some(data), Some(mime)) = (
                file.get("data").and_then(|d| d.as_str()),
                file.get("mediaType").and_then(|m| m.as_str()),
            ) {
                if mime.starts_with("image/") {
                    let bytes = base64::Engine::decode(
                        &base64::engine::general_purpose::STANDARD,
                        data,
                    )
                    .map_err(|e| Error::ExternalApi(format!("Base64 decode error: {e}")))?;
                    return Ok(bytes);
                }
            }
        }
    }

    // Format 1: OpenAI multimodal content parts
    if let Some(choices) = body.get("choices").and_then(|c| c.as_array()) {
        if let Some(content) = choices
            .first()
            .and_then(|c| c.get("message"))
            .and_then(|m| m.get("content"))
        {
            if let Some(parts) = content.as_array() {
                for part in parts {
                    if let (Some(data), Some(mime)) = (
                        part.get("inline_data").and_then(|d| d.get("data")).and_then(|d| d.as_str()),
                        part.get("inline_data").and_then(|d| d.get("mime_type")).and_then(|m| m.as_str()),
                    ) {
                        if mime.starts_with("image/") {
                            let bytes = base64::Engine::decode(
                                &base64::engine::general_purpose::STANDARD,
                                data,
                            )
                            .map_err(|e| Error::ExternalApi(format!("Base64 decode error: {e}")))?;
                            return Ok(bytes);
                        }
                    }
                }
            }
        }
    }

    Err(Error::ExternalApi(
        "Image gen returned no image data. Response may use an unsupported format.".into(),
    ))
}
