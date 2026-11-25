//! Turning a page or a picture into words that can be searched.
//!
//! The record both passes write, and the one path that shows a stored picture
//! to the Omni slot. Bookmarks (`bookmark_enrichment`) and photo favorites
//! (`photo_enrichment`) each have their own queue, prompt and table, and share
//! this: an image is loaded, sent and parsed here and nowhere else.

use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::error::{Error, Result};
use virtues_registry::models::ModelSlot;

/// Largest image the pass sends. This is one background call, not a chat turn
/// whose context carries the image into every later turn — so it is roomier
/// than chat's 5MB. Base64 inflates by 4/3, and 10MB raw lands near 13MB
/// encoded, inside the ~20MB inline-request ceiling vision providers enforce.
/// Above it the row is skipped with the size stated, never sent to fail.
const MAX_IMAGE_BYTES: usize = 10 * 1024 * 1024;

/// The structured record a pass produces, for a page or an image. Every field is optional: a model that
/// cannot tell should say so, and "unknown" must be cheaper than a guess.
///
/// **`why` is deliberately absent.** Significance is user-sourced, never
/// inferred — enforced here by the shape of the type rather than by asking the
/// prompt nicely.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExtractionRecord {
    /// Free prose, first in the struct so the model describes before it
    /// classifies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub medium: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subject: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entities: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,
    /// The main colors, in plain words. Photos only.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub colors: Vec<String>,
    /// Where it is, as a kind of place, and the light: "beach at sunset",
    /// "kitchen, morning". Photos only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub setting: Option<String>,
    /// Words readable in the picture. Photos only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visible_text: Option<String>,
    /// What the person would plausibly type to find this again. The single
    /// highest-leverage field in the record: query-shaped text matches query
    /// language far better than a literal description does.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub likely_queries: Vec<String>,
}

impl ExtractionRecord {
    /// The record as searchable prose.
    ///
    /// Labelled per line so a chunk that lands mid-record still reads as
    /// something, and so the aspects stay legible to a human debugging a bad
    /// result. This is what step 4 concatenates into the embed text.
    pub fn to_embed_text(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if let Some(d) = self.description.as_deref().filter(|s| !s.trim().is_empty()) {
            parts.push(d.trim().to_string());
        }
        if let Some(m) = self.medium.as_deref().filter(|s| !s.trim().is_empty()) {
            parts.push(format!("Medium: {}", m.trim()));
        }
        if !self.subject.is_empty() {
            parts.push(format!("Subject: {}", self.subject.join(", ")));
        }
        if !self.entities.is_empty() {
            parts.push(format!("Mentions: {}", self.entities.join(", ")));
        }
        if let Some(s) = self.style.as_deref().filter(|s| !s.trim().is_empty()) {
            parts.push(format!("Style: {}", s.trim()));
        }
        if let Some(s) = self.setting.as_deref().filter(|s| !s.trim().is_empty()) {
            parts.push(format!("Setting: {}", s.trim()));
        }
        if !self.colors.is_empty() {
            parts.push(format!("Colors: {}", self.colors.join(", ")));
        }
        if let Some(t) = self.visible_text.as_deref().filter(|t| !t.trim().is_empty()) {
            parts.push(format!("Words in it: {}", t.trim()));
        }
        if !self.likely_queries.is_empty() {
            parts.push(self.likely_queries.join(". "));
        }
        parts.join("\n")
    }
}

/// What loading a stored picture found.
pub enum Loaded {
    Image { bytes: Vec<u8>, mime: String },
    /// There is nothing here a model can read, and a retry cannot change that:
    /// the file left Drive, is not an image, or is too large.
    Unreadable(String),
}

/// Load a picture from Drive. A storage error is transient until proven
/// otherwise, so it comes back as an error for the caller's backoff.
pub async fn load(db: &PgPool, file_id: &str) -> Result<Loaded> {
    let config = crate::api::DriveConfig::new(std::sync::Arc::new(crate::storage::lake::storage_from_env()?));
    let (file, bytes) = match crate::api::drive::download_file(db, &config, file_id).await {
        Ok(v) => v,
        Err(Error::NotFound(reason)) | Err(Error::InvalidInput(reason)) => return Ok(Loaded::Unreadable(reason)),
        Err(e) => return Err(e),
    };
    let mime = file.mime_type.clone().unwrap_or_default();
    if !mime.starts_with("image/") {
        return Ok(Loaded::Unreadable(format!(
            "the saved file is {}, not an image",
            if mime.is_empty() { "an unknown type" } else { &mime }
        )));
    }
    if bytes.len() > MAX_IMAGE_BYTES {
        return Ok(Loaded::Unreadable(format!(
            "the image is {:.1}MB, over the {}MB limit for reading it",
            bytes.len() as f64 / 1_048_576.0,
            MAX_IMAGE_BYTES / 1_048_576
        )));
    }
    Ok(Loaded::Image { bytes, mime })
}

/// Show the Omni slot one image and get its record back. `feature` is what the
/// spend is filed under.
pub async fn read(
    db: &PgPool,
    feature: &'static str,
    system_prompt: &str,
    context: &str,
    bytes: &[u8],
    mime: &str,
) -> Result<ExtractionRecord> {
    let encoded = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, bytes);
    let content = serde_json::json!([
        { "type": "text", "text": context },
        { "type": "image_url", "image_url": { "url": format!("data:{mime};base64,{encoded}") } }
    ]);

    // Description, not invention: thinking off, temperature zero.
    let raw = crate::virtues_api::completion::system_completion_content(
        db,
        ModelSlot::Omni,
        feature,
        system_prompt,
        content,
        crate::virtues_api::request::Thinking::Off,
        0.0,
    )
    .await?;

    parse_record(&raw)
}

/// Parse the model's reply into a record.
///
/// Tolerates code fences and leading prose, because "return only JSON" is an
/// instruction models mostly follow rather than always follow, and a fenced
/// reply is a formatting slip, not a reason to retry and re-bill.
pub fn parse_record(raw: &str) -> Result<ExtractionRecord> {
    let trimmed = raw.trim();
    let candidate = match (trimmed.find('{'), trimmed.rfind('}')) {
        (Some(start), Some(end)) if end > start => &trimmed[start..=end],
        _ => {
            return Err(Error::ExternalApi(format!(
                "enrichment reply was not JSON: {}",
                trimmed.chars().take(200).collect::<String>()
            )))
        }
    };
    serde_json::from_str::<ExtractionRecord>(candidate)
        .map_err(|e| Error::ExternalApi(format!("enrichment reply did not parse: {e}")))
}
