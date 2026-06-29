//! Nightly Day Illustration Generation
//!
//! Runs once per day after the autobiography has been generated. For each
//! eligible day (has autobiography, no illustration yet), picks the
//! highest-novelty event as the scene subject, crafts a visual scene prompt
//! via a tiny LLM call, and generates a pen-and-ink illustration via the
//! Vercel AI Gateway (Nano Banana / Gemini 2.5 Flash Image).
//!
//! The resulting PNG is alpha-keyed + auto-cropped in pure Rust using the
//! `image` crate, then stored as a BLOB on `wiki_days.illustration`.
//! Served via GET /api/wiki/day/:date/illustration.
//!
//! Cost: ~$0.04/image (Nano Banana) + ~$0.0003/image (scene crafter LLM).

use chrono::NaiveDate;
use image::{ImageEncoder, RgbaImage};
use sqlx::PgPool;

use crate::error::{Error, Result};
use super::wiki::save_day_illustration;

// ── Style preamble (prepended to every scene prompt) ────────────────────────

const STYLE_PREAMBLE: &str = "Pen and ink line drawing, loose gestural journal-sketchbook style, black ink only on plain white background, quick hand-drawn strokes, minimal detail, no color, no shading fills, no frame, no border, no text. Subject fills most of the frame.";

// ── Scene crafter system prompt ─────────────────────────────────────────────

const SCENE_CRAFTER_SYSTEM_PROMPT: &str = r#"You are crafting a one-sentence visual scene description for an ink sketch that will illustrate a day in a personal journal.

You will receive:
- A 1-3 sentence summary of a salient event from the day
- The day's autobiography (2-5 sentences)

Output a single sentence (15-30 words) describing a concrete visual scene that an illustrator could draw. The scene should:
- Depict a SINGLE moment or object drawn from the event — not a montage
- Use concrete nouns and spatial details (what's in the foreground, background, light direction)
- Prefer one human figure OR one object OR one place — never all three
- Avoid emotional/abstract words; render feelings through posture, light, and objects

Output ONLY the scene sentence. No preamble, no quotes, no explanation."#;

// ── Main entry point ────────────────────────────────────────────────────────

/// Run the illustration job. Finds one eligible day and generates its illustration.
pub async fn run_illustration_job(pool: &PgPool) -> Result<()> {
    let Some(eligible) = find_eligible_day(pool).await? else {
        tracing::debug!("No eligible days need illustration");
        return Ok(());
    };

    tracing::info!(date = %eligible.date, "Generating day illustration");

    // 1. Pick the top-novelty event as the scene subject
    let subject = match pick_top_novelty_subject(pool, eligible.date).await? {
        Some(s) => s,
        None => {
            tracing::info!(date = %eligible.date, "No eligible event; skipping illustration");
            return Ok(());
        }
    };

    // 2. Craft the scene prompt via LLM
    let scene = craft_scene_prompt(pool, &subject, &eligible.autobiography).await?;
    let full_prompt = format!("{STYLE_PREAMBLE} Subject: {scene}");

    // 3. Generate the image via Vercel AI Gateway
    let raw_png = generate_image_via_gateway(pool, &full_prompt).await?;

    // 4. Post-process: alpha-key white → transparent, auto-crop to content bbox
    let processed_png = post_process_illustration(&raw_png)?;

    // 5. Store BLOB directly on wiki_days
    save_day_illustration(pool, eligible.date, &processed_png).await?;

    tracing::info!(
        date = %eligible.date,
        scene = %scene,
        size_kb = processed_png.len() / 1024,
        "Day illustration generated and stored"
    );

    Ok(())
}

// ── Finding eligible days ───────────────────────────────────────────────────

struct EligibleDay {
    date: NaiveDate,
    autobiography: String,
}

/// Find the most recent day that has autobiography but no illustration.
/// Walks back up to 7 days from today.
async fn find_eligible_day(pool: &PgPool) -> Result<Option<EligibleDay>> {
    let row = sqlx::query_as::<_, (String, Option<String>)>(
        r#"
        SELECT date, autobiography
        FROM wiki_days
        WHERE autobiography IS NOT NULL
          AND illustration IS NULL
          AND date >= current_date - 7
        ORDER BY date DESC
        LIMIT 1
        "#,
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to query eligible days: {e}")))?;

    Ok(row.and_then(|(date_str, auto)| {
        let date = NaiveDate::parse_from_str(&date_str, "%Y-%m-%d").ok()?;
        let autobiography = auto?;
        Some(EligibleDay { date, autobiography })
    }))
}

// ── Subject picking ─────────────────────────────────────────────────────────

struct Subject {
    event_summary: String,
}

/// Pick the event with the highest novelty_z for the given day.
async fn pick_top_novelty_subject(pool: &PgPool, date: NaiveDate) -> Result<Option<Subject>> {
    let date_str = date.format("%Y-%m-%d").to_string();
    let row = sqlx::query_as::<_, (Option<String>, Option<String>)>(
        r#"
        SELECT e.event_summary, e.auto_label
        FROM wiki_events e
        JOIN wiki_days d ON e.day_id = d.id
        WHERE d.date = $1
          AND e.is_sleep = FALSE
          AND e.is_transit = FALSE
          AND e.is_unknown = FALSE
          AND e.user_hidden = FALSE
          AND e.novelty_z IS NOT NULL
        ORDER BY e.novelty_z DESC
        LIMIT 1
        "#,
    )
    .bind(&date_str)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to query events: {e}")))?;

    Ok(row.and_then(|(summary, label)| {
        summary
            .filter(|s| !s.trim().is_empty())
            .or(label)
            .map(|s| Subject { event_summary: s })
    }))
}

// ── Scene crafter (LLM via virtues-api) ───────────────────────────────────────

async fn craft_scene_prompt(
    pool: &PgPool,
    subject: &Subject,
    autobiography: &str,
) -> Result<String> {
    let chat_model = crate::api::assistant_profile::get_chat_model(pool).await?;

    let user_prompt = format!(
        "EVENT SUMMARY:\n{}\n\nDAY AUTOBIOGRAPHY:\n{}",
        subject.event_summary.trim(),
        autobiography.trim(),
    );

    // System purpose — day illustration is automated, debits OS reserve.
    let client = crate::virtues_api::client::BearerClient::from_env(pool.clone())
        .with_purpose(crate::virtues_api::client::Purpose::System)
        .with_feature("day_illustration");
    let response = client
        .post_json(
            "/v1/ai/chat/completions",
            &serde_json::json!({
                "model": chat_model,
                "messages": [
                    {"role": "system", "content": SCENE_CRAFTER_SYSTEM_PROMPT},
                    {"role": "user", "content": user_prompt}
                ],
                "max_tokens": 100,
                "temperature": 0.7
            }),
        )
        .await
        .map_err(|e| Error::Network(format!("Scene crafter request failed: {e}")))?;

    if !response.is_success() {
        return Err(Error::ExternalApi(format!(
            "Scene crafter error {}: {}",
            response.status, response.body
        )));
    }

    response.body["choices"][0]["message"]["content"]
        .as_str()
        .map(|c| c.trim().trim_matches(['"', '\'']).trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| Error::ExternalApi("Scene crafter returned empty".into()))
}

// ── Image generation (Vercel AI Gateway) ────────────────────────────────────

/// Generate a day illustration with Nano Banana (Gemini 2.5 Flash Image) and
/// return the raw PNG bytes.
///
/// Routes through virtues-api `/v1/ai/chat/completions` on the device bearer
/// (same as every other AI call) — so the gateway key never lives on the box
/// and image-gen cost is metered through the entitlement. The gateway returns
/// images inline (base64) in the OpenAI-compatible response.
pub async fn generate_image_via_gateway(pool: &PgPool, prompt: &str) -> Result<Vec<u8>> {
    let response = crate::virtues_api::client::BearerClient::from_env(pool.clone())
        .with_purpose(crate::virtues_api::client::Purpose::System)
        .with_feature("day_illustration")
        .post_json(
            "/v1/ai/chat/completions",
            &serde_json::json!({
                "model": "google/gemini-2.5-flash-image",
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

    // Try multiple response formats (gateway may return different shapes):
    // Format 1: choices[0].message.content is an array with image parts
    // Format 2: choices[0].message.content is a string (text only — no image)
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
        if let Some(content) = choices.first()
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

// ── Image post-processing (pure Rust, no ffmpeg) ────────────────────────────

/// Alpha-key white → transparent, then auto-crop to content bounding box + padding.
/// Input: raw PNG bytes (opaque white background).
/// Output: processed PNG bytes (transparent background, tight crop).
fn post_process_illustration(raw_png: &[u8]) -> Result<Vec<u8>> {
    let img = image::load_from_memory(raw_png)
        .map_err(|e| Error::Other(format!("Failed to decode PNG: {e}")))?
        .to_rgba8();

    // Step 1: alpha-key — luminance → alpha (black stays opaque, white → transparent)
    let (w, h) = img.dimensions();
    let mut keyed = RgbaImage::new(w, h);
    for (x, y, pixel) in img.enumerate_pixels() {
        let [r, g, b, _] = pixel.0;
        // Luminance (BT.601 weighted)
        let luma = (r as f32) * 0.299 + (g as f32) * 0.587 + (b as f32) * 0.114;
        let alpha = (255.0 - luma).max(0.0).min(255.0) as u8;
        keyed.put_pixel(x, y, image::Rgba([r, g, b, alpha]));
    }

    // Step 2: find alpha bounding box
    let mut min_x = w;
    let mut min_y = h;
    let mut max_x = 0u32;
    let mut max_y = 0u32;
    for (x, y, pixel) in keyed.enumerate_pixels() {
        if pixel.0[3] > 2 {
            // Non-transparent pixel
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }
    }

    // Step 3: crop with padding
    if max_x <= min_x || max_y <= min_y {
        // No content found — return as-is
        return encode_png(&keyed);
    }
    let pad = 40u32;
    let crop_x = min_x.saturating_sub(pad);
    let crop_y = min_y.saturating_sub(pad);
    let crop_w = ((max_x + pad + 1).min(w) - crop_x).max(1);
    let crop_h = ((max_y + pad + 1).min(h) - crop_y).max(1);

    let cropped = image::imageops::crop_imm(&keyed, crop_x, crop_y, crop_w, crop_h).to_image();

    encode_png(&cropped)
}

fn encode_png(img: &RgbaImage) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    let encoder = image::codecs::png::PngEncoder::new(&mut buf);
    encoder
        .write_image(
            img.as_raw(),
            img.width(),
            img.height(),
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|e| Error::Other(format!("PNG encode error: {e}")))?;
    Ok(buf)
}
