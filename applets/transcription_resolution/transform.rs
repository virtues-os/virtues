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
use virtues_registry::models::{default_model_for_slot, ModelSlot};

// The audio model is the registry's Omni slot (currently google/gemini-3-flash),
// resolved at call time — never a hardcoded id here. "Omni" = an audio-native
// model that ingests audio as native tokens (~25/sec, ~7.5K for a 5-min clip —
// cheap) and does scene understanding (speech + ambient sounds + music +
// setting), which is what a life-log wants, NOT bare speech-to-text. gemini-3
// won a controlled 5-clip bench on cost/speed/accuracy/JSON-validity; see
// ModelSlot::Omni. Audio requires reasoning_effort:low (below) — it's the only
// tier whose thinking budget the gateway honors.

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

/// Post-transcription hallucination guard. Gemini's failure mode on near-silent
/// audio is fluent narration of nothing — a "morning routine" invented over a
/// quiet room. We catch it by proportion: a transcript should be roughly as long
/// as there was speech to justify it. Allowed length ≈ measured speech-seconds ×
/// MAX_CHARS_PER_SPEECH_SEC + SLACK; a transcript longer than that, over a chunk
/// the VAD actually measured, is suppressed to a silent row. 40 chars/sec is ~2×
/// real fast speech (~20/sec), so dense legitimate speech clears it comfortably;
/// only text with no acoustic basis trips it. Fires only when the VAD produced a
/// measurement — a missing measurement never suppresses (fail-open).
const MAX_CHARS_PER_SPEECH_SEC: f32 = 40.0;
const HALLUCINATION_SLACK_CHARS: f32 = 80.0;

const SYSTEM_PROMPT: &str = r#"You transcribe short audio clips from a personal wearable mic. Your output is the SOLE source of truth for an automated event timeline and daily summary a person reads about their own life — they never hear the audio; they read what you write as fact. One invented detail — an event that didn't happen, a word no one said, a name no one spoke — is accepted as true and quietly destroys trust in the whole system. Omissions are safe and recoverable: the audio is kept and can be re-processed. Fabrications are not. So report only what you actually hear, and when unsure, leave it out.

Output ONLY a raw JSON object — no markdown, no code fences, no prose.

Schema:
{"title":"<=10 words, literal label of what is audible","summary":"1 sentence, only what is actually heard; '' or 'Mostly quiet' if little is audible","text":"verbatim speech, '' if none","language":"ISO 639-1","confidence":0.0-1.0 in the speech transcript,"speaker_count":integer,"tags":["<=8, for content actually present"],"entities":{"people":[{"name":"string","said":"verbatim clause they were named in, <=15 words"}],"places":[{"name":"string","said":"..."}],"organizations":[{"name":"string","said":"..."}]},"scene":{"sounds":["only distinctly identifiable non-speech sounds actually heard"],"music":"description only if music is clearly present, else null","setting":"place ONLY if a distinctive sound proves it, else 'unknown'"}}

Rules:
- REPORT, DON'T INFER. Write what you hear, never what it implies. A sound is not an activity: list "grinding noise" in scene.sounds, but never write "making coffee"; footsteps are not "a walk"; typing is not "working". Activities enter the record ONLY if spoken aloud.
- text: exact words, keep fillers (um, uh). Use "[Speaker 1]:", "[Speaker 2]:" for multiple voices. Mark unclear speech "[inaudible]" — never substitute a plausible guess. Sung or hummed vocals ARE speech — transcribe the lyrics verbatim.
- Never repeat a phrase more than twice, even if the audio seems to loop — that is a transcription error, not speech.
- Background TV, radio, or podcast is NOT the wearer speaking. Note it as a "background_media" sound; never attribute its words as first-person speech.
- entities: only names explicitly spoken and unambiguous. entities[].said quotes the clause verbatim, <=15 words. Omit anything uncertain — never guess a plausible-sounding name. For tech/AI terms prefer real names (Claude, Cursor, Codex, GPT, Gemini, repo, agent) over acoustically-similar non-words: "Claude", not "claw"/"cloud".
- confidence: confidence in the SPEECH transcript only (0.0 if no speech, 0.9+ if clear).
- MOSTLY-QUIET audio (no clear speech, only faint or indistinct sound): text:"", confidence:0.0, list only sounds you can distinctly name (often none), summary:"Mostly quiet". Do NOT build a narrative, routine, or agenda from faint ambience.
- SILENT audio (no speech, no distinct sound): {"title":"Silence","summary":"Silent audio","text":"","language":"en","confidence":0.0,"speaker_count":0,"tags":["silence"],"entities":{"people":[],"places":[],"organizations":[]},"scene":{"sounds":[],"music":null,"setting":"unknown"}}

OVERRIDING RULE: better to report too little than to add anything unclear. Never trade accuracy for completeness. A short, true record is a success; a rich, embellished one is a failure.
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
    /// Audio scene block (sounds/music/setting) — the non-speech "essence"
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
/// The root MUST be resolved exactly as the writer does — default included. A
/// reader that omitted the default while the writer applied it would look for
/// recordings relative to the cwd that had been written to the lake: never
/// found, "audio file missing", silently never transcribed.
///
/// That invariant used to rest on this comment. It now rests on
/// `storage::lake::lake_root`, which both sides call.
fn read_audio(audio_url: &str) -> std::io::Result<Vec<u8>> {
    let in_lake = virtues::storage::lake::lake_root().join(audio_url);
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

    // The box's home timezone, for the per-call ground-truth time anchor. A
    // missing/unparseable profile falls back to UTC — the anchor is only a
    // consistency hint, never load-bearing. Same source as the nightly
    // maintenance-hour logic (app_user_profile.home_timezone).
    let tz: chrono_tz::Tz = sqlx::query_as::<_, (Option<String>,)>(
        "SELECT home_timezone FROM app_user_profile LIMIT 1",
    )
    .fetch_optional(db)
    .await
    .ok()
    .flatten()
    .and_then(|(s,)| s)
    .and_then(|s| s.parse().ok())
    .unwrap_or(chrono_tz::UTC);

    // Build the virtues-api client lazily — only if we have at least one
    // non-silent recording to process.
    let mut virtues_api: Option<BearerClient> = None;

    // On-box voice-activity gate, built lazily and reused across the batch. If
    // it can't load we proceed WITHOUT gating (fail-open) rather than abort the
    // drain — a missing gate just means we pay Gemini for no-speech chunks, not
    // that we drop audio.
    let mut vad: Option<crate::vad::Vad> = None;
    let mut vad_init_failed = false;

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

        // Voice-activity gate: ~65% of all-day audio has no speech (silence,
        // traffic, music, room tone). Detect those on-box and record them
        // silent instead of paying Gemini to transcribe nothing. The audio is
        // still stored, so a skipped chunk stays re-runnable later. Fail-open:
        // if the VAD can't load, transcribe everything as before.
        if vad.is_none() && !vad_init_failed {
            match crate::vad::Vad::new() {
                Ok(v) => vad = Some(v),
                Err(e) => {
                    tracing::error!(error = %e, "VAD init failed; transcribing without speech-gate");
                    vad_init_failed = true;
                }
            }
        }
        // Measure speech seconds once. It both gates the Gemini call (below the
        // minimum → silent, no call) and, when we do call, primes the model's
        // honesty ground-truth and the post-transcription hallucination guard.
        // None = VAD unavailable/errored → fail-open (proceed, never suppress).
        let speech_secs: Option<f32> = vad.as_ref().and_then(|v| v.speech_seconds(&audio_bytes));
        if matches!(speech_secs, Some(s) if s < crate::vad::MIN_SPEECH_SECS) {
            tracing::info!(
                stream_id = %rec.source_stream_id,
                speech_secs = ?speech_secs,
                "no speech detected (VAD); recording silent, skipping Gemini"
            );
            match insert_silent_transcript(db, rec).await {
                Ok(_) => skipped += 1,
                Err(e) => {
                    tracing::warn!(stream_id = %rec.source_stream_id, error = %e,
                        "failed to insert silent transcript (VAD gate)");
                    record_attempt_failure(db, &rec.source_stream_id).await;
                    failed += 1;
                }
            }
            continue;
        }

        let audio_b64 =
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &audio_bytes);

        // Deterministic ground-truth anchor: local time + measured silence, fed
        // as a fenced consistency hint (never as content). Stops the model from
        // stamping a "morning routine" onto an evening chunk, and tells it up
        // front when it is being handed near-silence.
        let ground_truth = build_ground_truth(rec, &tz, speech_secs);

        match transcribe(client, &audio_b64, &rec.audio_format, &ground_truth).await {
            // Cost is captured at the BearerClient chokepoint (post_json records
            // the gateway usage.cost into app_ai_calls, tagged "transcription").
            Ok(t) => {
                // Hallucination guard: a transcript far longer than the measured
                // speech could justify is fluent narration of near-silence — the
                // exact failure that wrote a fake morning routine over a quiet
                // room. Suppress to a silent row rather than trust it. Fires only
                // when the VAD gave a measurement (Some); None never suppresses.
                let text_len = t.text.trim().chars().count() as f32;
                let over_budget = matches!(
                    speech_secs,
                    Some(s) if text_len > s * MAX_CHARS_PER_SPEECH_SEC + HALLUCINATION_SLACK_CHARS
                );
                if over_budget {
                    tracing::warn!(
                        stream_id = %rec.source_stream_id,
                        speech_secs = ?speech_secs,
                        text_len = text_len as usize,
                        title = %t.title.as_deref().unwrap_or("(none)"),
                        "suppressing likely hallucination: transcript far exceeds measured speech"
                    );
                    match insert_silent_transcript(db, rec).await {
                        Ok(_) => skipped += 1,
                        Err(e) => {
                            tracing::warn!(stream_id = %rec.source_stream_id, error = %e,
                                "failed to insert silent transcript (hallucination guard)");
                            record_attempt_failure(db, &rec.source_stream_id).await;
                            failed += 1;
                        }
                    }
                    continue;
                }
                match insert_transcription(db, rec, &t).await {
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
                }
            }
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
    // Persist the audio scene (sounds/music/setting) in metadata so the
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

/// Build the fenced GROUND TRUTH block prepended to the transcription request.
/// Deterministic facts only — local time and measured silence — framed as a
/// consistency/disambiguation hint the model must never treat as content. The
/// time anchor is what catches a "morning routine" hallucinated onto an evening
/// chunk; the speech measurement primes the model for near-silence up front.
fn build_ground_truth(
    rec: &PendingRecording,
    tz: &chrono_tz::Tz,
    speech_secs: Option<f32>,
) -> String {
    let local = rec.started_at.with_timezone(tz);
    let hour: u32 = local.format("%H").to_string().parse().unwrap_or(12);
    let part = match hour {
        5..=11 => "morning",
        12..=16 => "afternoon",
        17..=20 => "evening",
        _ => "night",
    };
    let time_line = format!("- Local time: {} ({part})", local.format("%a %H:%M"));

    let speech_line = match speech_secs {
        Some(s) => {
            let quiet = if s < 2.0 { " — mostly quiet" } else { "" };
            match rec.duration_seconds {
                Some(d) => format!("\n- Measured speech in this clip: {s:.1}s of {d:.0}s{quiet}"),
                None => format!("\n- Measured speech in this clip: {s:.1}s{quiet}"),
            }
        }
        None => String::new(),
    };

    format!(
        "GROUND TRUTH — for consistency and disambiguation ONLY. Never transcribe, \
describe, or infer content from these; if the audio contradicts them, transcribe the \
audio and lower confidence.\n{time_line}{speech_line}\nDo not narrate any scene or \
routine these imply — they exist only to catch contradictions and disambiguate words \
you actually hear."
    )
}

// ─────────────────────────────────────────────────────────────────────────────
// Transcription call (bearer-authed, System purpose → OS reserve)
// ─────────────────────────────────────────────────────────────────────────────

async fn transcribe(
    client: &BearerClient,
    audio_b64: &str,
    audio_format: &str,
    ground_truth: &str,
) -> std::result::Result<TranscriptionResponse, TranscribeError> {
    let mime_type = audio_mime_type(audio_format);
    let request_body = serde_json::json!({
        "model": default_model_for_slot(ModelSlot::Omni),
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
                        "text": format!("{ground_truth}\n\nTranscribe this audio recording and extract structured data.")
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
