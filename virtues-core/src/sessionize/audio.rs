//! Audio sessionization: 5-minute recorder slices → coherent context sessions.
//!
//! Ambient audio is captured in ~5-minute chunks — hundreds a day, each
//! transcribed and titled in isolation. That granularity is the recorder's, not
//! the day's. This rolls the chunks up into **sessions**: a conversation, a drive,
//! a stretch of quiet work, ~10 hours of sleep-with-a-fan as one block.
//!
//! **Boundaries come from acoustic context, never topic.** Topic drifts wildly
//! inside one context ("HDMI screens → shipping → lunch → your date", all at one
//! desk, is one session). What marks a real change is *who is around and how loud*:
//!
//!   - `average_db_level` — a car, a quiet room, a loud restaurant read differently
//!   - `speaker_count`, bucketed {silent 0, solo 1, dyad 2, group 3+} — raw
//!     diarization is noisy (it will claim 40 speakers), so the tail is clamped
//!
//! Those two features, z-normalised and speaker-weighted, go through
//! [`changepoint::detect`]. Verified on a real day: 271 chunks → ~24 coherent
//! blocks, with topic drift staying inside its session.
//!
//! This is **mechanical**. It finds boundaries and stitches the chunk summaries it
//! already has; it writes no titles and calls no model. All labelling — turning
//! "quiet, no speakers, 10h, at home, overnight" into "Sleeping" — is the day
//! detective's job, where the full context (time, place, duration) lives.

use super::changepoint;
use crate::error::Result;
use crate::ids;
use sqlx::{PgPool, Row};

/// A transcribed 5-minute recording, the input unit.
#[derive(Debug, Clone)]
pub struct Chunk {
    pub start: chrono::DateTime<chrono::Utc>,
    pub end: chrono::DateTime<chrono::Utc>,
    /// Ambient loudness (dB). `None` when the recording carried no level.
    pub db: Option<f64>,
    /// Distinct speakers this chunk, raw (noisy — clamped internally).
    pub speaker_count: Option<i64>,
    /// The chunk's own summary, stitched into the session verbatim (never a model
    /// call here).
    pub summary: Option<String>,
}

/// A coherent-context session: a span, the chunks it covers, their stitched
/// content, and a speaker profile. No title, no generated summary — see the
/// module doc.
#[derive(Debug, Clone)]
pub struct Session {
    pub start: chrono::DateTime<chrono::Utc>,
    pub end: chrono::DateTime<chrono::Utc>,
    /// Indices into the input slice, in order.
    pub chunk_idx: Vec<usize>,
    /// The chunk summaries, joined — the detective's content clue and the search
    /// document. Empty for a silent session (nothing was said).
    pub stitched: String,
    /// Modal speaker bucket over the session: 0 silent, 1 solo, 2 dyad, 3 group.
    pub speaker_mode: u8,
    /// Mean loudness, for the labeller downstream.
    pub avg_db: Option<f64>,
}

/// Diarization garbage-collector: real social contexts are silent, solo, a pair,
/// or a group. Beyond three the count is noise, and it would only inflate the
/// feature and manufacture boundaries.
fn speaker_bucket(s: Option<i64>) -> f64 {
    s.unwrap_or(0).clamp(0, 3) as f64
}

/// The recall/precision dial, in **BIC units** — the raw penalty is scaled by
/// `ln(n)` inside [`sessionize`], so this one constant behaves the same whether a
/// day has 20 chunks or 300. (An absolute penalty does not: the value that gives
/// ~24 sessions on a 271-chunk day would merge everything in a 10-chunk test.)
///
/// These sessions are CLUES the detective can merge but never un-split, so we bias
/// toward more of them. Calibrated on a real day: base 2.0 → ~18-24 coherent
/// sessions (a startup chat, a car ride, the ~10h sleep block, distinct
/// conversations) with few single-chunk fragments. Lower over-segments on speaker
/// flicker (diarization reads 2,0,2 across one conversation); higher merges real
/// shifts. Overridable per run via `VIRTUES_AUDIO_PENALTY` while it is tuned
/// against a labelled week.
pub const DEFAULT_PENALTY: f64 = 2.0;

/// Speakers weigh more than loudness: a conversation is defined by voices, and dB
/// is only a weak proxy for environment.
const WEIGHTS: [f64; 2] = [0.7, 2.0];

/// A missing dB reads as a low floor (silence), not as "unknown".
const DB_FLOOR: f64 = -60.0;

/// Cut a day's chunks into sessions. Pure — no I/O, no model. Chunks must be in
/// time order.
pub fn sessionize(chunks: &[Chunk], penalty: f64) -> Vec<Session> {
    if chunks.is_empty() {
        return Vec::new();
    }

    let features: Vec<Vec<f64>> = chunks
        .iter()
        .map(|c| vec![c.db.unwrap_or(DB_FLOOR), speaker_bucket(c.speaker_count)])
        .collect();
    let norm = changepoint::normalize(features, &WEIGHTS);
    // BIC scaling: penalty grows with ln(n) so one `penalty` base is portable
    // across days of wildly different length.
    let effective = penalty * (chunks.len() as f64).ln().max(1.0);
    let bounds = changepoint::detect(&norm, effective, 1);

    // Turn boundary indices into [start, end) index ranges.
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    let mut prev = 0usize;
    for &b in &bounds {
        ranges.push((prev, b));
        prev = b;
    }
    ranges.push((prev, chunks.len()));

    ranges
        .into_iter()
        .map(|(a, b)| {
            let seg = &chunks[a..b];
            let stitched = seg
                .iter()
                .filter_map(|c| c.summary.as_deref().map(str::trim).filter(|s| !s.is_empty()))
                .collect::<Vec<_>>()
                .join(" ");

            // The PEAK speaker bucket — was this context social at all, and how.
            // Not the mode: most chunks in a day are silent (only ~1 in 4 carry
            // speech), so a real conversation of 3 talking + 5 quiet chunks would
            // read "silent" by mode. Peak captures "the busiest this context got",
            // which is what the label needs.
            let speaker_mode = seg
                .iter()
                .map(|c| speaker_bucket(c.speaker_count) as u8)
                .max()
                .unwrap_or(0);

            let dbs: Vec<f64> = seg.iter().filter_map(|c| c.db).collect();
            let avg_db = if dbs.is_empty() {
                None
            } else {
                Some(dbs.iter().sum::<f64>() / dbs.len() as f64)
            };

            Session {
                start: seg[0].start,
                end: seg[seg.len() - 1].end,
                chunk_idx: (a..b).collect(),
                stitched,
                speaker_mode,
                avg_db,
            }
        })
        .collect()
}

/// Sessionize one day and rebuild its `data_audio_session` rows.
///
/// Wipe-and-rebuild for the day: sessions are a derived, re-derivable projection,
/// and this runs nightly on a complete day, so recomputing from scratch is the
/// simplest idempotent contract — no open-session bookkeeping needed (that is for
/// the intra-day rollups; this is nightly, the day is done).
///
/// Returns the number of sessions written.
pub async fn sessionize_day(pool: &PgPool, date: chrono::NaiveDate) -> Result<u32> {
    // The day is a tz-aware window, not `start_time::date` — that cast uses the
    // session timezone and silently shifts the day boundary (it cut a UTC-full day
    // of 271 chunks down to 223 in the wrong zone). Use the same "where you woke
    // up" boundary the rest of the day pipeline uses.
    let home_tz = crate::api::profile::get_timezone(pool)
        .await
        .unwrap_or(None)
        .unwrap_or_else(|| "UTC".to_string());
    let tz = crate::timezone::resolve_day_timezone(pool, date, &home_tz).await;
    let (start_str, end_str) = crate::api::day_summary::day_boundaries_utc(date, Some(&tz));

    // The day's chunks: transcription (speech + summary + speaker count) joined to
    // its recording (loudness), in time order. Left join so a chunk with no
    // recording row still contributes its speaker/summary.
    let rows = sqlx::query(
        "SELECT t.start_time, t.end_time, t.speaker_count, t.summary, r.average_db_level AS db \
         FROM data_communication_transcription t \
         LEFT JOIN data_audio_recording r ON r.audio_url = t.audio_url \
         WHERE t.start_time >= $1::timestamptz AND t.start_time < $2::timestamptz \
         ORDER BY t.start_time",
    )
    .bind(&start_str)
    .bind(&end_str)
    .fetch_all(pool)
    .await?;

    let chunks: Vec<Chunk> = rows
        .iter()
        .map(|row| Chunk {
            start: row.get("start_time"),
            end: row.get("end_time"),
            db: row.get::<Option<f64>, _>("db"),
            // `speaker_count` is int4, not int8 — decoding it as i64 fails, and a
            // `.ok()` there silently turned EVERY speaker into None, so every
            // session read as "silent" even mid-conversation. Decode the real type.
            speaker_count: row.get::<Option<i32>, _>("speaker_count").map(|n| n as i64),
            summary: row.get::<Option<String>, _>("summary"),
        })
        .collect();

    // Env override is a tuning hook while the penalty is being calibrated against
    // real days; unset uses the default.
    let penalty = std::env::var("VIRTUES_AUDIO_PENALTY")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_PENALTY);
    let sessions = sessionize(&chunks, penalty);

    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM data_audio_session WHERE start_time >= $1::timestamptz AND start_time < $2::timestamptz")
        .bind(&start_str)
        .bind(&end_str)
        .execute(&mut *tx)
        .await?;

    for s in &sessions {
        // Deterministic id from the session's boundaries, so a re-run of an
        // unchanged day produces the same ids.
        let id = ids::generate_id(
            "aud",
            &[&s.start.to_rfc3339(), &s.end.to_rfc3339()],
        );
        sqlx::query(
            "INSERT INTO data_audio_session \
             (id, start_time, end_time, speaker_mode, avg_db, chunk_count, content) \
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(&id)
        .bind(s.start)
        .bind(s.end)
        .bind(s.speaker_mode as i16)
        .bind(s.avg_db)
        .bind(s.chunk_idx.len() as i32)
        .bind(if s.stitched.is_empty() { None } else { Some(&s.stitched) })
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;

    tracing::info!(date = %date, chunks = chunks.len(), sessions = sessions.len(), "audio sessionized");
    Ok(sessions.len() as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(min: i64, db: f64, spk: i64, s: &str) -> Chunk {
        let base = chrono::DateTime::from_timestamp(1_700_000_000, 0).unwrap();
        Chunk {
            start: base + chrono::Duration::minutes(min),
            end: base + chrono::Duration::minutes(min + 5),
            db: Some(db),
            speaker_count: Some(spk),
            summary: Some(s.to_string()),
        }
    }

    #[test]
    fn topic_drift_within_one_context_stays_one_session() {
        // The case that killed the embedding approach: same desk, same 2 speakers,
        // same loudness — wildly different topics. Topic is not even a feature; the
        // acoustic context is flat, so it is one session no matter what is said.
        let chunks = vec![
            chunk(0, -22.0, 2, "HDMI screens"),
            chunk(5, -22.0, 2, "power cords"),
            chunk(10, -22.0, 2, "shipping"),
            chunk(15, -22.0, 2, "lunch plans"),
            chunk(20, -22.0, 2, "the date on Friday"),
        ];
        let s = sessionize(&chunks, DEFAULT_PENALTY);
        assert_eq!(s.len(), 1, "topic drift must not split a single context");
        assert_eq!(s[0].speaker_mode, 2);
        assert!(s[0].stitched.contains("HDMI") && s[0].stitched.contains("date"));
    }

    #[test]
    fn a_real_context_shift_gets_its_own_session() {
        // Quiet writing → a conversation → quiet again. Two boundaries.
        let mut chunks = Vec::new();
        for i in 0..4 {
            chunks.push(chunk(i * 5, -42.0, 0, "writing"));
        }
        for i in 4..7 {
            chunks.push(chunk(i * 5, -20.0, 2, "talking about chess"));
        }
        for i in 7..11 {
            chunks.push(chunk(i * 5, -43.0, 0, "writing again"));
        }
        let s = sessionize(&chunks, DEFAULT_PENALTY);
        assert_eq!(s.len(), 3, "the conversation is its own session");
        assert_eq!(s[1].speaker_mode, 2, "middle session is the conversation");
        assert_eq!(s[0].speaker_mode, 0);
    }

    #[test]
    fn a_long_flat_stretch_stays_one_block() {
        // 120 chunks of quiet (sleep-with-a-fan). Flat features → one session, no
        // matter how long.
        let chunks: Vec<Chunk> =
            (0..120).map(|i| chunk(i * 5, -49.0, 0, "")).collect();
        let s = sessionize(&chunks, DEFAULT_PENALTY);
        assert_eq!(s.len(), 1, "a coherent 10h stretch is one block, not 120");
        assert_eq!(s[0].chunk_idx.len(), 120);
        assert_eq!(s[0].stitched, "", "silent session stitches to nothing");
    }
}

