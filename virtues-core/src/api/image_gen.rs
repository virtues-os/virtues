//! Text-to-image generation via the AI Gateway.
//!
//! Used by the `generate_image` chat tool: the assistant turns a prompt into an
//! inline image. Routes through virtues-api `/v1/ai/chat/completions` on the
//! device bearer (same as every other AI call) — so the gateway key never lives
//! on the box and image-gen cost is metered through the entitlement.
//!
//! The gateway returns the image inline (base64) in an OpenAI-compatible
//! response body, but the exact shape has drifted across providers, so
//! [`extract_image_bytes`] accepts three:
//!
//! 1. `choices[0].message.images[].image_url.url` as a `data:image/…;base64,`
//!    URL. **This is what the current gateway actually emits** (observed
//!    2026-09-04 against the `ModelSlot::Image` model). The parser fell through
//!    to "no image data" on it for a while because only the other two shapes
//!    were handled.
//! 2. `choices[0].message.content[]` parts carrying `inline_data{mime_type,data}`
//!    (OpenAI multimodal content parts).
//! 3. Top-level `files[]` with `mediaType` + `data` (Vercel AI SDK native).

use serde_json::Value;
use sqlx::PgPool;

use crate::error::{Error, Result};

/// Generate an image from `prompt` and return the raw image bytes.
///
/// Model choice goes through the slot system (`ModelSlot::Image`); this
/// function only owns the transport and the response parsing.
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

    extract_image_bytes(&response.body)
}

/// Pull the first image out of a gateway response body, whichever of the
/// three known shapes it uses (see the module comment). Pure: no network, no
/// pool — so each shape has a unit test below.
pub fn extract_image_bytes(body: &Value) -> Result<Vec<u8>> {
    // Shape 1: choices[0].message.images[].image_url.url — a data URL.
    if let Some(images) = first_message(body)
        .and_then(|m| m.get("images"))
        .and_then(|i| i.as_array())
    {
        for image in images {
            if let Some(url) = image
                .get("image_url")
                .and_then(|u| u.get("url"))
                .and_then(|u| u.as_str())
            {
                if let Some(b64) = image_data_url_payload(url) {
                    return decode_base64(b64);
                }
            }
        }
    }

    // Shape 2: choices[0].message.content[] parts with inline_data.
    if let Some(parts) = first_message(body)
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_array())
    {
        for part in parts {
            let inline = part.get("inline_data");
            if let (Some(data), Some(mime)) = (
                inline.and_then(|d| d.get("data")).and_then(|d| d.as_str()),
                inline.and_then(|d| d.get("mime_type")).and_then(|m| m.as_str()),
            ) {
                if mime.starts_with("image/") {
                    return decode_base64(data);
                }
            }
        }
    }

    // Shape 3: top-level files[] (Vercel AI SDK native format).
    if let Some(files) = body.get("files").and_then(|f| f.as_array()) {
        for file in files {
            if let (Some(data), Some(mime)) = (
                file.get("data").and_then(|d| d.as_str()),
                file.get("mediaType").and_then(|m| m.as_str()),
            ) {
                if mime.starts_with("image/") {
                    return decode_base64(data);
                }
            }
        }
    }

    Err(Error::ExternalApi(
        "Image gen returned no image data. Response may use an unsupported format.".into(),
    ))
}

/// `choices[0].message`, if present.
fn first_message(body: &Value) -> Option<&Value> {
    body.get("choices")?.as_array()?.first()?.get("message")
}

/// For `data:image/<subtype>;base64,<payload>`, return `<payload>`. Anything
/// that is not a base64 *image* data URL yields `None` — a gateway that ever
/// hands back an `https://` URL here must be handled deliberately, not by
/// trying to base64-decode the URL.
fn image_data_url_payload(url: &str) -> Option<&str> {
    let rest = url.strip_prefix("data:image/")?;
    let (_, payload) = rest.split_once(";base64,")?;
    Some(payload)
}

fn decode_base64(data: &str) -> Result<Vec<u8>> {
    base64::Engine::decode(&base64::engine::general_purpose::STANDARD, data)
        .map_err(|e| Error::ExternalApi(format!("Base64 decode error: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // An 8-byte PNG signature is enough to prove the bytes round-tripped.
    const PNG_SIG: &[u8] = b"\x89PNG\r\n\x1a\n";
    const PNG_SIG_B64: &str = "iVBORw0KGgo=";

    #[test]
    fn parses_message_images_data_url() {
        // The shape the live gateway emitted on 2026-09-04.
        let body = json!({
            "choices": [{
                "finish_reason": "stop",
                "index": 0,
                "message": {
                    "content": "",
                    "images": [{
                        "image_url": {"url": format!("data:image/png;base64,{PNG_SIG_B64}")}
                    }]
                }
            }]
        });
        assert_eq!(extract_image_bytes(&body).unwrap(), PNG_SIG);
    }

    #[test]
    fn parses_openai_inline_data_content_parts() {
        let body = json!({
            "choices": [{
                "message": {
                    "content": [
                        {"type": "text", "text": "Here you go"},
                        {"inline_data": {"mime_type": "image/png", "data": PNG_SIG_B64}}
                    ]
                }
            }]
        });
        assert_eq!(extract_image_bytes(&body).unwrap(), PNG_SIG);
    }

    #[test]
    fn parses_vercel_files_array() {
        let body = json!({
            "files": [
                {"mediaType": "text/plain", "data": "bm90IGFuIGltYWdl"},
                {"mediaType": "image/png", "data": PNG_SIG_B64}
            ]
        });
        assert_eq!(extract_image_bytes(&body).unwrap(), PNG_SIG);
    }

    #[test]
    fn rejects_non_image_data_url_and_empty_body() {
        // A text data URL in images[] is not an image; an https URL is not a
        // payload we can decode. Both must fall through, not decode garbage.
        let body = json!({
            "choices": [{"message": {"content": "", "images": [
                {"image_url": {"url": "data:text/plain;base64,aGk="}},
                {"image_url": {"url": "https://example.com/a.png"}}
            ]}}]
        });
        assert!(extract_image_bytes(&body).is_err());
        assert!(extract_image_bytes(&json!({"choices": []})).is_err());
        assert!(extract_image_bytes(&json!({})).is_err());
    }

    #[test]
    fn data_url_payload_extraction() {
        assert_eq!(image_data_url_payload("data:image/jpeg;base64,QUJD"), Some("QUJD"));
        assert_eq!(image_data_url_payload("data:image/png,raw"), None);
        assert_eq!(image_data_url_payload("https://example.com/x.png"), None);
    }
}
