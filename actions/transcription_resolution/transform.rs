//! Drain logic for the transcription_resolution cron action.
//!
//! Selects untranscribed recordings via LEFT JOIN, calls Gemini for each one,
//! and INSERTs the result into `data_communication_transcription`. Silent
//! recordings are inserted directly with empty text and never hit Gemini.

use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;
use sqlx::{PgPool, Row};
use uuid::Uuid;
use virtues::virtues_api::client::{BearerClient, Purpose};

// gemini-2.5-flash: audio-capable (flash-lite is NOT — it returned empty on
// every clip, which drove the retry/re-bill loop). 2.5-flash ingests audio as
// native audio tokens (~25/sec, ~7.5K for a 5-min clip — cheap) and does full
// scene understanding (speech + ambient sounds + music + mood + setting), which
// is what a life-log wants, not bare ASR. Validated live via the Vercel gateway.
const MODEL: &str = "google/gemini-2.5-flash";

/// Below this, an audio file has no real content (an empty/glitch AAC container
/// is ~28 bytes). Real speech recordings are hundreds of KB. Sub-kilobyte files
/// are recorded as silent rather than sent to Gemini (which returns an empty
/// body → an unrecoverable parse error that otherwise retries forever).
const MIN_AUDIO_BYTES: usize = 1024;

/// Give up on a recording after this many failed transcription attempts. Past
/// this it's never re-selected, so a poison record can't loop-bill Gemini
/// forever AND it stops wedging the head of the oldest-first queue. Counter
/// lives in data_audio_recording.metadata (no schema migration).
const MAX_TRANSCRIBE_ATTEMPTS: i64 = 4;

/// Exponential backoff base: a failed recording isn't re-selected until
/// base * 2^attempts seconds have passed (2m, 4m, 8m, 16m). Spaces retries so a
/// transient failure recovers without re-billing every 2-min cron tick, and the
/// backoff window also lets the queue flow past it to fresh records meanwhile.
const RETRY_BACKOFF_BASE_SECS: i64 = 120;

const SYSTEM_PROMPT: &str = r#"You are an audio SCENE-UNDERSTANDING engine for a personal life-log. You hear a slice of someone's real life: speech, but also music, ambient sound, room tone, the feel of a place. Capture BOTH the words AND the essence of the moment. Output ONLY a raw JSON object — no markdown, no code fences, no prose.

Schema:
{"title":"string, max 10 words, what this moment is","summary":"1-2 sentence narrative of what was happening and how it felt","text":"verbatim speech transcript, empty string if no speech","language":"ISO 639-1","confidence":0.0-1.0,"speaker_count":integer,"tags":["max 8 topical + scene tags"],"entities":{"people":[],"places":[],"organizations":[]},"scene":{"sounds":["non-speech sounds heard: music, laughter, dog barking, traffic, dishes, footsteps, TV..."],"music":"description of any music (genre/energy) or null","mood":"the emotional tone/energy of the moment","setting":"likely place/context (e.g. home kitchen, bar, car, outdoors)"}}

Rules:
- text: exact words spoken, no paraphrasing, keep fillers (um, uh). Use "[Speaker 1]:", "[Speaker 2]:" when multiple voices. Empty "" if no intelligible speech.
- ALWAYS fill scene.* even when there is no speech — ambient-only moments are valuable. Describe what you actually hear; do not invent.
- entities: only names/places/orgs explicitly spoken or clearly identifiable. "[unclear]" if ambiguous.
- confidence: confidence in the SPEECH transcript (0.0 if no speech, 0.9+ if clear).
- tags: blend topic (what's discussed) and scene (sounds/mood/setting) labels.
- Truly silent/empty audio (no speech AND no discernible ambient sound): {"title":"Silence","summary":"Silent audio","text":"","language":"en","confidence":0.0,"speaker_count":0,"tags":["silence"],"entities":{"people":[],"places":[],"organizations":[]},"scene":{"sounds":[],"music":null,"mood":"quiet","setting":"unknown"}}
"#;

#[derive(Debug, thiserror::Error)]
enum TranscribeError {
    #[error("virtues-api rate limited (429)")]
    RateLimited,
    /// Gemini returned an empty response body — the recording has no
    /// transcribable speech (silent/near-silent audio). Deterministic, NOT
    /// transient, so the caller records it as a silent transcript and marks it
    /// DONE rather than retrying it forever (which re-bills the audio input on
    /// every cron tick — the cause of the runaway auto-top-up drain).
    #[error("empty transcription response (silent audio)")]
    EmptyResponse,
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
    /// Audio scene block (sounds/music/mood/setting) — the non-speech "essence"
    /// of the moment. Stored in the transcription row's metadata JSONB.
    scene: Option<Value>,
}

/// One row from the LEFT JOIN selecting untranscribed recordings.
struct PendingRecording {
    source_stream_id: String,
    started_at: DateTime<Utc>,
    ended_at: Option<DateTime<Utc>>,
    duration_seconds: Option<f64>,
    audio_url: String,
    audio_format: String,
    is_silent: bool,
}

/// Resolve `audio_url` against both the lake and the legacy layout.
///
/// New rows store a lake `storage_key` (relative to the storage root). Rows
/// written before the lake landed store a path relative to the server's cwd — the
/// old `data/lake/ios_microphone/…`, which ignored STORAGE_PATH and parked the
/// audio outside the configured lake entirely. Try the lake first, then fall back,
/// so the ~858 existing recordings keep transcribing without a data migration.
///
/// The root MUST be resolved exactly as the writer does (`storage::lake`): default
/// included. If the reader skipped the default while the writer used it, then on
/// any box without STORAGE_PATH set every new recording would be written to the
/// lake and then looked for relative to cwd — never found, "audio file missing",
/// and silently never transcribed.
fn read_audio(audio_url: &str) -> std::io::Result<Vec<u8>> {
    let root = std::env::var("STORAGE_PATH").unwrap_or_else(|_| "./data/lake".to_string());
    let in_lake = std::path::Path::new(&root).join(audio_url);
    if in_lake.exists() {
        return std::fs::read(in_lake);
    }
    std::fs::read(audio_url)
}

/// Decode one queried row into a `PendingRecording`, surfacing a column-decode
/// failure as an `Err` instead of panicking — `Row::get` unwraps internally, so
/// any schema/type drift would otherwise abort the whole drain. Callers count a
/// failure here as a failed record and move on.
fn decode_pending(row: &sqlx::postgres::PgRow) -> Result<PendingRecording> {
    Ok(PendingRecording {
        source_stream_id: row.try_get("source_stream_id")?,
        started_at: row.try_get("started_at")?,
        ended_at: row.try_get("ended_at")?,
        duration_seconds: row.try_get("duration_seconds")?,
        audio_url: row.try_get("audio_url")?,
        audio_format: row.try_get("audio_format")?,
        is_silent: row.try_get("is_silent")?,
    })
}

/// Drain up to `batch_size` untranscribed recordings.
///
/// Returns `(transcribed_via_gemini, skipped_silent, failed)`.
pub async fn drain(db: &PgPool, batch_size: i64) -> Result<(usize, usize, usize)> {
    let rows = sqlx::query(
        r#"
        SELECT r.source_stream_id, r.started_at, r.ended_at, r.duration_seconds,
               r.audio_url, r.audio_format, r.is_silent
        FROM data_audio_recording r
        LEFT JOIN data_communication_transcription t
            ON t.source_stream_id = r.source_stream_id
        WHERE t.id IS NULL
          -- Give-up cap: stop re-selecting (and re-billing) a recording after
          -- $2 failures. Also unblocks head-of-line — a poison record at the
          -- front no longer wedges the whole oldest-first queue.
          AND COALESCE((r.metadata->>'transcribe_attempts')::int, 0) < $2
          -- Exponential backoff: skip a recently-failed recording until
          -- base * 2^attempts seconds have elapsed.
          AND (
            r.metadata->>'transcribe_last_attempt' IS NULL
            OR (r.metadata->>'transcribe_last_attempt')::timestamptz
               < now() - make_interval(secs =>
                   $3::double precision
                   * power(2, COALESCE((r.metadata->>'transcribe_attempts')::int, 0)))
          )
        ORDER BY r.created_at ASC
        LIMIT $1
        "#,
    )
    .bind(batch_size)
    .bind(MAX_TRANSCRIBE_ATTEMPTS)
    .bind(RETRY_BACKOFF_BASE_SECS)
    .fetch_all(db)
    .await
    .context("failed to query pending recordings")?;

    if rows.is_empty() {
        return Ok((0, 0, 0));
    }

    let mut transcribed = 0usize;
    let mut skipped = 0usize;
    let mut failed = 0usize;

    // Decode each queried row into a PendingRecording. A column-decode failure
    // (a schema/type drift, like the stale `started_at: String` decoder after
    // the SQLite→Postgres migration) used to panic via `Row::get` and take down
    // the whole batch before a single record was processed — surfacing only as
    // an opaque subprocess crash. `try_get` degrades the one bad row instead:
    // log it, count it failed, and keep draining the rest.
    let mut pending: Vec<PendingRecording> = Vec::with_capacity(rows.len());
    for row in &rows {
        match decode_pending(row) {
            Ok(rec) => pending.push(rec),
            Err(e) => {
                tracing::warn!(error = %e, "skipping recording: failed to decode row");
                failed += 1;
            }
        }
    }

    // Build the virtues-api client lazily — only if we have at least one
    // non-silent recording to process.
    let mut virtues_api: Option<BearerClient> = None;

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
                    record_attempt_failure(db, &rec.source_stream_id).await;
                    failed += 1;
                }
            }
            continue;
        }

        // Lazy-init the api_key client. The device's own key funds this
        // background call, with one auto-top-up-and-retry on a 402 wallet_empty.
        if virtues_api.is_none() {
            virtues_api = Some(
                BearerClient::from_env(db.clone())
                    .with_purpose(Purpose::System)
                    .with_feature("transcription"),
            );
        }
        let client = virtues_api.as_ref().unwrap();

        // Read the audio file from disk.
        let audio_bytes = match read_audio(&rec.audio_url) {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!(
                    stream_id = %rec.source_stream_id,
                    audio_url = %rec.audio_url,
                    error = %e,
                    "audio file missing or unreadable, skipping"
                );
                record_attempt_failure(db, &rec.source_stream_id).await;
                failed += 1;
                continue;
            }
        };
        // Empty/glitch recordings (a few-byte AAC container with no samples)
        // make Gemini return an empty body → "EOF while parsing ... raw:" →
        // counted `failed` and retried every cron tick FOREVER, burning a paid
        // Gemini call each time. Real speech audio is hundreds of KB; anything
        // sub-kilobyte has no content. Record it as a silent transcript so it's
        // marked done and never re-sent.
        if audio_bytes.len() < MIN_AUDIO_BYTES {
            tracing::info!(
                stream_id = %rec.source_stream_id,
                bytes = audio_bytes.len(),
                "audio below minimum size; recording as silent (no Gemini call)"
            );
            match insert_silent_transcript(db, rec).await {
                Ok(_) => skipped += 1,
                Err(e) => {
                    tracing::warn!(stream_id = %rec.source_stream_id, error = %e,
                        "failed to insert silent transcript for tiny audio");
                    record_attempt_failure(db, &rec.source_stream_id).await;
                    failed += 1;
                }
            }
            continue;
        }

        let audio_b64 =
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &audio_bytes);

        match transcribe(client, &audio_b64, &rec.audio_format).await {
            // Cost is captured at the BearerClient chokepoint (post_json records
            // the gateway usage.cost into app_ai_calls, tagged "transcription").
            Ok(t) => match insert_transcription(db, rec, &t).await {
                Ok(_) => transcribed += 1,
                Err(e) => {
                    tracing::warn!(
                        stream_id = %rec.source_stream_id,
                        error = %e,
                        "failed to insert transcription"
                    );
                    record_attempt_failure(db, &rec.source_stream_id).await;
                    failed += 1;
                }
            },
            Err(TranscribeError::RateLimited) => {
                let remaining = pending.len() - transcribed - skipped - failed;
                tracing::warn!(
                    "rate limited by virtues-api — stopping cron drain early; {} recordings remain",
                    remaining
                );
                return Ok((transcribed, skipped, failed));
            }
            Err(TranscribeError::EmptyResponse) => {
                // Silent/no-speech audio: record an empty transcript so it's
                // marked DONE and never re-sent. Without this the same recording
                // is re-billed to Gemini every cron tick forever.
                match insert_silent_transcript(db, rec).await {
                    Ok(_) => skipped += 1,
                    Err(e) => {
                        tracing::warn!(stream_id = %rec.source_stream_id, error = %e,
                            "failed to insert silent transcript for empty response");
                        record_attempt_failure(db, &rec.source_stream_id).await;
                        failed += 1;
                    }
                }
            }
            Err(TranscribeError::Other(e)) => {
                tracing::warn!(
                    stream_id = %rec.source_stream_id,
                    error = %e,
                    "transcription failed; will retry (capped + backed off)"
                );
                record_attempt_failure(db, &rec.source_stream_id).await;
                failed += 1;
            }
        }
    }

    Ok((transcribed, skipped, failed))
}

/// Record a failed transcription attempt on the recording so the give-up cap +
/// backoff in `drain`'s SELECT can see it. Best-effort: a write failure is
/// logged, not propagated — bookkeeping must never abort the drain. Counters
/// live in the existing metadata JSONB, so no schema migration is needed.
async fn record_attempt_failure(db: &PgPool, stream_id: &str) {
    let res = sqlx::query(
        r#"UPDATE data_audio_recording
           SET metadata = jsonb_set(
                 jsonb_set(COALESCE(metadata, '{}'::jsonb),
                   '{transcribe_attempts}',
                   to_jsonb(COALESCE((metadata->>'transcribe_attempts')::int, 0) + 1)),
                 '{transcribe_last_attempt}', to_jsonb(now()))
           WHERE source_stream_id = $1"#,
    )
    .bind(stream_id)
    .execute(db)
    .await;
    if let Err(e) = res {
        tracing::warn!(stream_id, error = %e, "failed to record transcription attempt counter");
    }
}

async fn insert_silent_transcript(db: &PgPool, rec: &PendingRecording) -> Result<()> {
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
    .bind(rec.started_at)
    .bind(rec.ended_at)
    .bind(0i32)
    .bind(0.0f64)
    .bind(serde_json::json!([]))
    .bind(serde_json::json!({}))
    .bind(&rec.source_stream_id)
    .bind("stream_ios_microphone")
    .bind("ios")
    .bind(serde_json::json!({}))
    .execute(db)
    .await
    .context("insert silent transcript")?;
    Ok(())
}

async fn insert_transcription(
    db: &PgPool,
    rec: &PendingRecording,
    t: &TranscriptionResponse,
) -> Result<()> {
    let id = Uuid::new_v4().to_string();
    let tags_json = t
        .tags
        .as_ref()
        .map(|tags| serde_json::json!(tags))
        .unwrap_or_else(|| serde_json::json!([]));
    let entities_json = t.entities.clone().unwrap_or_else(|| serde_json::json!({}));
    // Persist the audio scene (sounds/music/mood/setting) in metadata so the
    // non-speech "essence" is queryable alongside the transcript.
    let metadata_json = serde_json::json!({
        "scene": t.scene.clone().unwrap_or(serde_json::Value::Null)
    });

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
    .bind(rec.started_at)
    .bind(rec.ended_at)
    .bind(t.speaker_count)
    .bind(t.confidence)
    .bind(&tags_json)
    .bind(&entities_json)
    .bind(&rec.source_stream_id)
    .bind("stream_ios_microphone")
    .bind("ios")
    .bind(&metadata_json)
    .execute(db)
    .await
    .context("insert transcription")?;
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Transcription call (bearer-authed, System purpose → OS reserve)
// ─────────────────────────────────────────────────────────────────────────────

async fn transcribe(
    client: &BearerClient,
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
        // Trim Gemini's thinking budget: scene-understanding transcription needs
        // almost no chain-of-thought, and "low" cut reasoning tokens ~332→18 in
        // live probes — a direct per-call cost saving with no quality loss here.
        "reasoning_effort": "low"
        // NOTE: no `response_format` — the Vercel gateway rejects it for Gemini
        // (HTTP 400 "Invalid input" on param response_format). The system prompt
        // enforces raw-JSON output, and the parse path below strips ```json
        // fences and salvages partials, so JSON mode isn't needed.
    });

    let response = client
        .post_json("/v1/ai/chat/completions", &request_body)
        .await
        .map_err(|e| TranscribeError::Other(anyhow!("virtues-api request failed: {e}")))?;

    if response.status == 429 {
        return Err(TranscribeError::RateLimited);
    }
    if !response.is_success() {
        return Err(TranscribeError::Other(anyhow!(
            "virtues-api returned {}: {}",
            response.status,
            response.body
        )));
    }

    let content_str = response
        .body
        .get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .ok_or_else(|| TranscribeError::Other(anyhow!("missing choices[0].message.content")))?;

    // Gemini returns an empty body for silent/no-speech audio. Parsing "" panics
    // the strict parse with "EOF at column 0" → counted failed → retried every
    // cron tick forever, re-billing the audio input each time. Treat empty as a
    // deterministic "silent" signal so the caller can mark it done.
    if content_str.trim().is_empty() {
        return Err(TranscribeError::EmptyResponse);
    }

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
        scene: None,
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
