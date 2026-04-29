//! Drain logic for the ios_microphone_transcribe cron action.
//!
//! Selects untranscribed recordings via LEFT JOIN, calls Gemini for each one,
//! and INSERTs the result into `data_communication_transcription`. Silent
//! recordings are inserted directly with empty text and never hit Gemini.

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use serde_json::Value;
use sqlx::{Row, SqlitePool};
use std::time::Duration;
use uuid::Uuid;

const MODEL: &str = "google/gemini-2.5-flash-lite";

const SYSTEM_PROMPT: &str = r#"You are a verbatim audio transcription system. Output ONLY a raw JSON object — no markdown, no code fences, no explanation.

Schema:
{"title":"string max 10 words","summary":"string 1-2 sentences","text":"string verbatim transcript","language":"string ISO 639-1","confidence":0.0-1.0,"speaker_count":integer,"tags":["max 5 strings"],"entities":{"people":[],"places":[],"organizations":[]}}

Rules:
- text: Exact words spoken. No paraphrasing. Include filler words (um, uh, ah). Use "[Speaker 1]:", "[Speaker 2]:" if multiple speakers.
- entities: Only extract names explicitly spoken. Use "[unclear]" if a name is ambiguous.
- confidence: 0.0 for silence/unintelligible, 0.5+ for partial, 0.9+ for clear speech.
- tags: 1-5 topic labels maximum.
- Silence/noise: Return {"title":"Silence","summary":"No speech detected","text":"","language":"en","confidence":0.0,"speaker_count":0,"tags":[],"entities":{"people":[],"places":[],"organizations":[]}}
"#;

#[derive(Debug, thiserror::Error)]
enum TranscribeError {
    #[error("Tollbooth rate limited (429)")]
    RateLimited,
    #[error("{0}")]
    Other(#[from] anyhow::Error),
}

#[derive(Debug, Deserialize)]
struct TranscriptionResponse {
    title: Option<String>,
    summary: Option<String>,
    #[serde(default)]
    text: String,
    language: Option<String>,
    confidence: Option<f64>,
    speaker_count: Option<i32>,
    tags: Option<Vec<String>>,
    entities: Option<Value>,
}

/// One row from the LEFT JOIN selecting untranscribed recordings.
struct PendingRecording {
    source_stream_id: String,
    started_at: String,
    ended_at: Option<String>,
    duration_seconds: Option<f64>,
    audio_url: String,
    audio_format: String,
    is_silent: bool,
}

/// Drain up to `batch_size` untranscribed recordings.
///
/// Returns `(transcribed_via_gemini, skipped_silent, failed)`.
pub async fn drain(db: &SqlitePool, batch_size: i64) -> Result<(usize, usize, usize)> {
    let rows = sqlx::query(
        r#"
        SELECT r.source_stream_id, r.started_at, r.ended_at, r.duration_seconds,
               r.audio_url, r.audio_format, r.is_silent
        FROM data_audio_recording r
        LEFT JOIN data_communication_transcription t
            ON t.source_stream_id = r.source_stream_id
        WHERE t.id IS NULL
        ORDER BY r.created_at ASC
        LIMIT $1
        "#,
    )
    .bind(batch_size)
    .fetch_all(db)
    .await
    .context("failed to query pending recordings")?;

    if rows.is_empty() {
        return Ok((0, 0, 0));
    }

    let pending: Vec<PendingRecording> = rows
        .iter()
        .map(|row| PendingRecording {
            source_stream_id: row.get("source_stream_id"),
            started_at: row.get("started_at"),
            ended_at: row.get("ended_at"),
            duration_seconds: row.get("duration_seconds"),
            audio_url: row.get("audio_url"),
            audio_format: row.get("audio_format"),
            is_silent: row.get::<i64, _>("is_silent") != 0,
        })
        .collect();

    // Build the Tollbooth client lazily — only if we have at least one
    // non-silent recording to process.
    let mut tollbooth: Option<TollboothClient> = None;
    let mut transcribed = 0usize;
    let mut skipped = 0usize;
    let mut failed = 0usize;

    for rec in &pending {
        // Silent recordings: insert an empty transcript directly, no Gemini call
        if rec.is_silent {
            match insert_silent_transcript(db, rec).await {
                Ok(_) => skipped += 1,
                Err(e) => {
                    tracing::warn!(
                        stream_id = %rec.source_stream_id,
                        error = %e,
                        "failed to insert silent transcript"
                    );
                    failed += 1;
                }
            }
            continue;
        }

        // Lazy-init Tollbooth client (errors here are fatal — config issue)
        if tollbooth.is_none() {
            tollbooth = Some(TollboothClient::from_env()?);
        }
        let client = tollbooth.as_ref().unwrap();

        // Read the audio file from disk
        let audio_bytes = match std::fs::read(&rec.audio_url) {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!(
                    stream_id = %rec.source_stream_id,
                    audio_url = %rec.audio_url,
                    error = %e,
                    "audio file missing or unreadable, skipping"
                );
                failed += 1;
                continue;
            }
        };
        let audio_b64 =
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &audio_bytes);

        match client.transcribe(&audio_b64, &rec.audio_format).await {
            Ok(t) => match insert_transcription(db, rec, &t).await {
                Ok(_) => transcribed += 1,
                Err(e) => {
                    tracing::warn!(
                        stream_id = %rec.source_stream_id,
                        error = %e,
                        "failed to insert transcription"
                    );
                    failed += 1;
                }
            },
            Err(TranscribeError::RateLimited) => {
                let remaining = pending.len() - transcribed - skipped - failed;
                tracing::warn!(
                    "rate limited by Tollbooth — stopping cron drain early; {} recordings remain",
                    remaining
                );
                return Ok((transcribed, skipped, failed));
            }
            Err(TranscribeError::Other(e)) => {
                tracing::warn!(
                    stream_id = %rec.source_stream_id,
                    error = %e,
                    "transcription failed; will retry next cron tick"
                );
                failed += 1;
            }
        }
    }

    Ok((transcribed, skipped, failed))
}

async fn insert_silent_transcript(db: &SqlitePool, rec: &PendingRecording) -> Result<()> {
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"INSERT INTO data_communication_transcription (
            id, audio_url, text, title, summary, language,
            duration_seconds, start_time, end_time,
            speaker_count, confidence, tags, entities,
            source_stream_id, source_table, source_provider, metadata
        ) VALUES (
            $1, $2, $3, $4, $5, $6,
            $7, $8, $9,
            $10, $11, $12, $13,
            $14, $15, $16, $17
        ) ON CONFLICT (source_stream_id) DO NOTHING"#,
    )
    .bind(&id)
    .bind(&rec.audio_url)
    .bind("") // empty text — silent
    .bind("Silence")
    .bind("No speech detected")
    .bind("en")
    .bind(rec.duration_seconds)
    .bind(&rec.started_at)
    .bind(rec.ended_at.as_deref())
    .bind(0i32)
    .bind(0.0f64)
    .bind("[]")
    .bind("{}")
    .bind(&rec.source_stream_id)
    .bind("stream_ios_microphone")
    .bind("ios")
    .bind("{}")
    .execute(db)
    .await
    .context("insert silent transcript")?;
    Ok(())
}

async fn insert_transcription(
    db: &SqlitePool,
    rec: &PendingRecording,
    t: &TranscriptionResponse,
) -> Result<()> {
    let id = Uuid::new_v4().to_string();
    let tags_json = t
        .tags
        .as_ref()
        .map(|tags| serde_json::to_string(tags).unwrap_or_else(|_| "[]".to_string()))
        .unwrap_or_else(|| "[]".to_string());
    let entities_json = t
        .entities
        .as_ref()
        .map(|e| serde_json::to_string(e).unwrap_or_else(|_| "{}".to_string()))
        .unwrap_or_else(|| "{}".to_string());

    sqlx::query(
        r#"INSERT INTO data_communication_transcription (
            id, audio_url, text, title, summary, language,
            duration_seconds, start_time, end_time,
            speaker_count, confidence, tags, entities,
            source_stream_id, source_table, source_provider, metadata
        ) VALUES (
            $1, $2, $3, $4, $5, $6,
            $7, $8, $9,
            $10, $11, $12, $13,
            $14, $15, $16, $17
        ) ON CONFLICT (source_stream_id) DO NOTHING"#,
    )
    .bind(&id)
    .bind(&rec.audio_url)
    .bind(&t.text)
    .bind(&t.title)
    .bind(&t.summary)
    .bind(&t.language)
    .bind(rec.duration_seconds)
    .bind(&rec.started_at)
    .bind(rec.ended_at.as_deref())
    .bind(t.speaker_count)
    .bind(t.confidence)
    .bind(&tags_json)
    .bind(&entities_json)
    .bind(&rec.source_stream_id)
    .bind("stream_ios_microphone")
    .bind("ios")
    .bind("{}")
    .execute(db)
    .await
    .context("insert transcription")?;
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Tollbooth client (mirrors the one removed from ios_microphone)
// ─────────────────────────────────────────────────────────────────────────────

struct TollboothClient {
    url: String,
    secret: String,
    http: reqwest::Client,
}

impl TollboothClient {
    fn from_env() -> Result<Self> {
        let url = std::env::var("TOLLBOOTH_URL")
            .unwrap_or_else(|_| "http://localhost:9002".to_string());
        let secret = std::env::var("TOLLBOOTH_INTERNAL_SECRET")
            .context("TOLLBOOTH_INTERNAL_SECRET not set")?;
        if secret.len() < 32 {
            return Err(anyhow!(
                "TOLLBOOTH_INTERNAL_SECRET too short (need >=32 chars)"
            ));
        }
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(300))
            .build()?;
        Ok(Self { url, secret, http })
    }

    async fn transcribe(
        &self,
        audio_b64: &str,
        audio_format: &str,
    ) -> std::result::Result<TranscriptionResponse, TranscribeError> {
        let mime_type = audio_mime_type(audio_format);
        let request_body = serde_json::json!({
            "model": MODEL,
            "messages": [
                { "role": "system", "content": SYSTEM_PROMPT },
                {
                    "role": "user",
                    "content": [
                        {
                            "type": "image_url",
                            "image_url": { "url": format!("data:{mime_type};base64,{audio_b64}") }
                        },
                        {
                            "type": "text",
                            "text": "Transcribe this audio recording and extract structured data."
                        }
                    ]
                }
            ],
            // 30s of speech produces ~100-300 words = ~150-500 tokens of text.
            // Plus JSON wrapper (title/summary/entities/tags) ~500 more.
            // 8192 is generous for real audio. If Gemini exceeds this, it's
            // almost certainly hallucinating/looping on quiet audio — handled
            // by the salvage path below rather than by raising the cap.
            "max_tokens": 8192,
            "temperature": 0.0,
            "response_format": { "type": "json_object" }
        });

        let endpoint = format!("{}/v1/chat/completions", self.url);
        let response = self
            .http
            .post(&endpoint)
            .header("X-Internal-Secret", &self.secret)
            .header("X-User-Id", "system")
            .header("Content-Type", "application/json")
            .json(&request_body)
            .send()
            .await
            .map_err(|e| TranscribeError::Other(anyhow!("Tollbooth request failed: {e}")))?;

        let status = response.status();
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(TranscribeError::RateLimited);
        }
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(TranscribeError::Other(anyhow!(
                "Tollbooth returned {status}: {body}"
            )));
        }

        let resp_json: Value = response
            .json()
            .await
            .map_err(|e| TranscribeError::Other(anyhow!("failed to parse Tollbooth response: {e}")))?;

        let content_str = resp_json
            .get("choices")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("message"))
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
            .ok_or_else(|| TranscribeError::Other(anyhow!("missing choices[0].message.content")))?;

        // Strip markdown code fencing if Gemini wraps in ```json ... ```
        let json_str = content_str.trim();
        let json_str = if json_str.starts_with("```") {
            let stripped = json_str
                .strip_prefix("```json")
                .or_else(|| json_str.strip_prefix("```"))
                .unwrap_or(json_str);
            stripped.strip_suffix("```").unwrap_or(stripped).trim()
        } else {
            json_str
        };

        // Try the strict parse first. If it fails (Gemini hit max_tokens and
        // truncated mid-string, or hallucinated past the cap), try to salvage
        // the partial response so we don't loop forever on poison records.
        match serde_json::from_str::<TranscriptionResponse>(json_str) {
            Ok(t) => Ok(t),
            Err(parse_err) => {
                if let Some(salvaged) = salvage_truncated_response(json_str) {
                    tracing::warn!(
                        original_error = %parse_err,
                        title = %salvaged.title.as_deref().unwrap_or("(none)"),
                        text_len = salvaged.text.len(),
                        "salvaged truncated Gemini response"
                    );
                    Ok(salvaged)
                } else {
                    Err(TranscribeError::Other(anyhow!(
                        "failed to parse Gemini JSON: {parse_err}. raw: {}",
                        &json_str[..json_str.len().min(200)]
                    )))
                }
            }
        }
    }
}

/// Recover what we can from a truncated Gemini JSON response.
///
/// Gemini occasionally exceeds `max_tokens` mid-string (especially on quiet
/// audio where it hallucinates) and the JSON parser EOFs trying to find the
/// closing quote. Rather than retry forever, we extract the title/summary/text
/// fields by string scanning and return a partial transcript with reduced
/// confidence so the row lands and the cron drainer moves on.
fn salvage_truncated_response(raw: &str) -> Option<TranscriptionResponse> {
    let title = extract_string_field(raw, "title");
    let summary = extract_string_field(raw, "summary");
    let text = extract_string_field(raw, "text").unwrap_or_default();
    let language = extract_string_field(raw, "language");

    // If we couldn't even find a title or any text, give up — this isn't a
    // truncated response, it's malformed from the start.
    if title.is_none() && text.is_empty() {
        return None;
    }

    Some(TranscriptionResponse {
        title,
        summary,
        text,
        language,
        confidence: Some(0.3), // partial — confidence reduced
        speaker_count: None,
        tags: None,
        entities: None,
    })
}

/// Extract the value of a `"field": "..."` pair from a JSON-ish string.
///
/// Tolerant: handles unescaped truncation, finds the field by name, walks
/// forward until the next unescaped closing quote (or end of string if
/// truncated). Returns None if the field isn't found.
fn extract_string_field(raw: &str, field: &str) -> Option<String> {
    let needle = format!("\"{field}\"");
    let field_start = raw.find(&needle)?;
    // Skip past `"field"` then find the colon and the opening quote of the value
    let after_field = &raw[field_start + needle.len()..];
    let colon = after_field.find(':')?;
    let after_colon = &after_field[colon + 1..];
    let open_quote = after_colon.find('"')?;
    let value_start = open_quote + 1;
    let value_region = &after_colon[value_start..];

    // Walk byte-by-byte to find the closing quote, respecting backslash escapes.
    let bytes = value_region.as_bytes();
    let mut i = 0;
    let mut out = String::new();
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'\\' && i + 1 < bytes.len() {
            // JSON escape sequence — copy the next char literally
            match bytes[i + 1] {
                b'n' => out.push('\n'),
                b't' => out.push('\t'),
                b'r' => out.push('\r'),
                b'"' => out.push('"'),
                b'\\' => out.push('\\'),
                b'/' => out.push('/'),
                other => {
                    out.push('\\');
                    out.push(other as char);
                }
            }
            i += 2;
        } else if b == b'"' {
            // Unescaped closing quote — done
            return Some(out);
        } else {
            // Copy the byte (handle multi-byte UTF-8 sequences naively)
            // Safe because we never break in the middle of a JSON escape.
            out.push(b as char);
            i += 1;
        }
    }
    // String never closed (truncated). Return what we got.
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

fn audio_mime_type(format: &str) -> &'static str {
    match format {
        "m4a" | "mp4" | "aac" => "audio/mp4",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" => "audio/ogg",
        "flac" => "audio/flac",
        _ => "audio/mp4",
    }
}

