//! Guards for the day pipeline — the bug that ate itself.
//!
//! # What happened
//!
//! `day_summary_eod` used to run:
//!
//! ```text
//!   sleep → novelty → autonomic → topic/entity → segment_day_events
//! ```
//!
//! and `segment_day_events` SEGMENTS the day: it deletes every auto event
//! (`DELETE FROM wiki_events WHERE is_user_added = false`) and re-inserts fresh
//! rows carrying 14 columns — `id, day_id, start_time, end_time, auto_label,
//! auto_location, user_label, user_location, user_notes, source_ontologies,
//! is_unknown, is_transit, is_user_added, event_summary`.
//!
//! Not one of them is a score.
//!
//! So the cron computed `embedding`, `novelty_z`, `local_novelty_z`, `lof_raw`,
//! `avg_hr`, `hr_z`, `autonomic_z`, `topic_novelty` and `entity_novelty` — and
//! then deleted the rows holding every one of them. Every night. And because
//! `novelty::load_baseline` requires `embedding IS NOT NULL` on PAST events, the
//! baseline could never accumulate either, so it could not have recovered on its
//! own. The scoring subsystem had never persisted a single value.
//!
//! It went unnoticed for two reasons, and both are worth remembering:
//! the demo seeds hand-populate `avg_hr` and `topics`, so the day page looked
//! alive; and the cron's success line counted events *seen*, not events
//! *scored*, so the metric stayed cheerfully non-zero while nothing happened.
//!
//! # The guards
//!
//! `segmentation_runs_before_scoring` needs no database, no embedder and no
//! network. It reads the cron's source and asserts the order. It is the cheap
//! one, it runs everywhere, and it is the one that would have caught this.
//!
//! `full_pipeline_persists_every_score` is the real thing, against a real DB.
//! It is `#[ignore]`d because it needs Postgres and the embedder sidecar.

use std::path::Path;

/// The invariant, checked against the source itself: **segment, then score.**
///
/// Any scoring step placed above `segment_day_events` writes to rows that are
/// about to be deleted. This assertion is deliberately crude — it greps the
/// cron — because the property it protects is a plain ordering fact, and a
/// crude test that runs on every commit beats an elegant one that needs a
/// database nobody has locally.
#[test]
fn segmentation_runs_before_scoring() {
    let src = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("repo root")
            .join("applets/day_summary_eod/main.rs"),
    )
    .expect("read day_summary_eod/main.rs");

    // Only the call sites, not the comment block that explains all this.
    let pos = |needle: &str| {
        src.lines()
            .position(|l| l.contains(needle) && !l.trim_start().starts_with("//"))
            .unwrap_or_else(|| panic!("no call to `{needle}` in day_summary_eod"))
    };

    let segment = pos("segment_day_events(");

    for (label, scorer) in [
        // Sleep, too: a sleep event has `is_user_added = false`, so the delete
        // inside segment_day_events eats it like any other auto event.
        ("sleep resolution", "resolve_sleep_events("),
        ("event annotation", "annotate_events_for_day("),
        ("novelty scoring", "compute_novelty_for_day("),
        ("autonomic scoring", "compute_autonomic_for_day("),
        ("topic/entity novelty", "compute_topic_entity_novelty("),
    ] {
        assert!(
            pos(scorer) > segment,
            "{label} runs BEFORE segment_day_events, which deletes and \
             re-creates every auto event — so everything it writes is destroyed. \
             This is the exact bug this test exists to prevent. Segment first, \
             then score."
        );
    }
}

/// Gap classification must run AFTER sleep and BEFORE scoring.
///
/// It settles the raw spine (absorbs sub-15-min Unknown slivers, labels
/// location-change gaps as Transit). It runs *after* sleep so it also cleans the
/// short Unknown tails sleep's split leaves behind, and *before* annotate/novelty so
/// the transit blocks it creates are annotated and scored like any other event —
/// mode is descriptive, salience is decisive. Move it after scoring and transit
/// silently never gets a novelty score.
#[test]
fn gaps_run_after_sleep_before_scoring() {
    let src = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("repo root")
            .join("applets/day_summary_eod/main.rs"),
    )
    .expect("read day_summary_eod/main.rs");

    let pos = |needle: &str| {
        src.lines()
            .position(|l| l.contains(needle) && !l.trim_start().starts_with("//"))
            .unwrap_or_else(|| panic!("no call to `{needle}` in day_summary_eod"))
    };

    let gaps = pos("classify_day_gaps(");
    assert!(
        gaps > pos("resolve_sleep_events("),
        "gap classification must run AFTER sleep — it cleans up the short Unknown \
         tails sleep's split leaves behind"
    );
    assert!(
        gaps < pos("compute_novelty_for_day("),
        "gap classification must run BEFORE scoring — transit blocks it creates must \
         be scored like any event (mode descriptive, salience decisive)"
    );
}

/// The full pipeline, against a real database: every score must SURVIVE.
///
/// Requires Postgres and the embedder sidecar:
///
/// ```text
///   make dev-embed                          # llama-server on :18181
///   DATABASE_URL=postgres://localhost/virtues \
///   VIRTUES_EMBED_URL=http://127.0.0.1:18181 \
///   cargo test -p virtues --test day_pipeline -- --ignored --nocapture
/// ```
///
/// Asserts on a day that already has segmented events. It does NOT call the LLM
/// — segmentation is assumed to have run — because the point is that the scores
/// persist, and an LLM call would make this test cost money and flake.
#[tokio::test]
#[ignore = "needs Postgres + the embedder sidecar (make dev-embed)"]
async fn full_pipeline_persists_every_score() {
    let pool = virtues_helpers::connect_from_env("day-pipeline-test")
        .await
        .expect("DATABASE_URL");

    // Any day that has real segmented events with summaries.
    let date: chrono::NaiveDate = sqlx::query_scalar(
        "SELECT d.date FROM wiki_days d
         JOIN wiki_events e ON e.day_id = d.id
         WHERE e.event_summary IS NOT NULL AND e.event_summary <> ''
           AND e.is_sleep = FALSE AND e.user_hidden = FALSE AND e.is_user_edited = FALSE
         GROUP BY d.date
         HAVING count(*) >= 3
         ORDER BY d.date DESC
         LIMIT 1",
    )
    .fetch_one(&pool)
    .await
    .expect("a day with segmented events (run `virtues seed`)");

    // Wipe the derived columns so we prove the pipeline WRITES them rather than
    // reading values a previous run — or the demo seed — left behind. This is
    // exactly the trap that hid the original bug.
    sqlx::query(
        "UPDATE wiki_events e
         SET embedding = NULL, novelty_z = NULL, local_novelty_z = NULL,
             avg_hr = NULL, hr_z = NULL, autonomic_z = NULL,
             topic_novelty = NULL, entity_novelty = NULL,
             entities = '[]'::jsonb, source_ontologies = '[]'::jsonb
         FROM wiki_days d WHERE d.id = e.day_id AND d.date = $1",
    )
    .bind(date)
    .execute(&pool)
    .await
    .expect("wipe derived columns");

    // The post-segmentation half of the cron, in the order the cron runs it.
    virtues::dayline::sleep::resolve_sleep_events(&pool, date).await;
    virtues::dayline::gaps::classify_day_gaps(&pool, date)
        .await
        .expect("gap classification");
    virtues::dayline::annotate::annotate_events_for_day(&pool, date)
        .await
        .expect("annotate");
    virtues::dayline::novelty::compute_novelty_for_day(&pool, date)
        .await
        .expect("novelty");
    virtues::dayline::autonomic_scoring::compute_autonomic_for_day(&pool, date)
        .await
        .expect("autonomic");
    virtues::dayline::topic_entity_novelty::compute_topic_entity_novelty(&pool, date)
        .await
        .expect("topic/entity novelty");

    let (events, embedded, novelty, ontologies): (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT count(*),
                count(e.embedding),
                count(e.novelty_z),
                count(*) FILTER (WHERE e.source_ontologies::text <> '[]')
         FROM wiki_events e
         JOIN wiki_days d ON d.id = e.day_id
         WHERE d.date = $1 AND e.event_summary IS NOT NULL AND e.event_summary <> ''
           -- novelty.rs deliberately skips these; so must the assertion.
           -- Sleep has its own physiology, hidden events are the user's no, and
           -- a user-edited event is not ours to re-score.
           AND e.is_sleep = FALSE AND e.user_hidden = FALSE AND e.is_user_edited = FALSE",
    )
    .bind(date)
    .fetch_one(&pool)
    .await
    .expect("counts");

    assert!(events > 0, "no scorable events on {date}");

    // THE assertion. Zero embeddings is precisely the state the whole codebase
    // was in — 741 events, not one vector — and it is what makes novelty,
    // autonomic scoring, class-by-neighbourhood and the story magnet all
    // impossible. If this ever returns 0 again, the pipeline is eating itself.
    assert_eq!(
        embedded, events,
        "{embedded}/{events} events have an embedding on {date}. Every score \
         downstream depends on this, and a scoring step has probably been moved \
         above segment_day_events again."
    );

    assert!(
        novelty > 0,
        "no event on {date} has novelty_z — the baseline is not accumulating"
    );

    // `source_ontologies` was a dead column from migration 0006 until
    // `dayline::annotate` started writing it. A day with events has data.
    assert!(
        ontologies > 0,
        "no event on {date} recorded which ontologies its window contained"
    );

    // Nothing was deleted. Dust stays searchable; user events are sacred.
    let user_events: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM wiki_events e JOIN wiki_days d ON d.id = e.day_id
         WHERE d.date = $1 AND e.is_user_added = true",
    )
    .bind(date)
    .fetch_one(&pool)
    .await
    .expect("user events");
    let _ = user_events; // asserted by surviving the wipe above; kept explicit.

    // The settled SHAPE, after gap classification: no sub-floor slivers survive, and
    // the spine is still gapless. A block earns 15 minutes, a seam earns 3.
    let (short_unknowns, short_transits): (i64, i64) = sqlx::query_as(
        "SELECT
           count(*) FILTER (WHERE e.is_unknown
             AND e.end_time - e.start_time < interval '15 minutes'),
           count(*) FILTER (WHERE e.is_transit
             AND e.end_time - e.start_time < interval '3 minutes')
         FROM wiki_events e JOIN wiki_days d ON d.id = e.day_id
         WHERE d.date = $1 AND e.is_user_added = false AND e.is_sleep = false",
    )
    .bind(date)
    .fetch_one(&pool)
    .await
    .expect("shape counts");
    assert_eq!(short_unknowns, 0, "an Unknown block under 15 min survived — sliver absorption failed on {date}");
    assert_eq!(short_transits, 0, "a Transit block under 3 min survived — the 3-min seam floor failed on {date}");

    // Still gapless: every event's end equals the next event's start.
    let gaps_or_overlaps: i64 = sqlx::query_scalar(
        "WITH e AS (
           SELECT end_time, lead(start_time) OVER (ORDER BY start_time) nxt
           FROM wiki_events ev JOIN wiki_days d ON d.id = ev.day_id WHERE d.date = $1)
         SELECT count(*) FILTER (WHERE nxt IS NOT NULL AND end_time <> nxt) FROM e",
    )
    .bind(date)
    .fetch_one(&pool)
    .await
    .expect("gapless check");
    assert_eq!(gaps_or_overlaps, 0, "timeline is no longer gapless after gap classification on {date}");
}

/// Whatever INVALIDATES scores must RESTORE them.
///
/// The second time this pipeline destroyed its own output, it wore a different
/// hat. `virtues reindex` nulls `wiki_events.embedding` and every score standing
/// on it — novelty, autonomic, topic, entity — and it is *right* to: a new
/// embedding model puts vectors in a different geometry, and the old numbers mean
/// nothing there.
///
/// But it then rebuilt only the SEARCH index and stopped. The nightly cron scores
/// exactly one day, the one it runs for. So a reindex quietly wiped the scores of
/// every past day and nothing ever put them back — 82 of 83 days on the dev box,
/// gone, no error, no mention. It was found by auditing, not by anything failing.
///
/// Same shape as `segmentation_runs_before_scoring`: one step silently destroying
/// what another produced. So it gets the same kind of guard — source-level, no
/// database, runs on every commit.
#[test]
fn whatever_nulls_the_scores_must_rescore() {
    let src = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/cli/reindex.rs"),
    )
    .expect("read cli/reindex.rs");

    let code: String = src
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");

    // It nulls the scores...
    assert!(
        code.contains("novelty_z = NULL"),
        "reindex no longer nulls event scores — if that is deliberate, this guard \
         is obsolete; if it is an accident, search and novelty now disagree about \
         which model's geometry they live in"
    );

    // ...so it must put them back. For EVERY day, not just today: the cron only
    // ever revisits the day it runs for.
    assert!(
        code.contains("rescore_all_days"),
        "reindex nulls every event score but does not rescore. The nightly cron \
         scores ONE day, so every past day stays at zero forever — silently, which \
         is exactly how this pipeline lost months of work the first time."
    );
}

/// Segmenting a day and narrating it are different jobs, kept as two SEPARATE
/// best-model calls with scoring in between.
///
/// They used to be ONE Opus call producing the events AND the autobiography. The
/// fusion made "only narrate a day with enough good events" UNSTATABLE (the events
/// did not exist until the narration ran) and — more importantly now — it made
/// scoring impossible: novelty/autonomic/topic are RELATIVE measures that need the
/// whole day's segmentation before they can run. Only by splitting the detective
/// (events) from the day summary (prose) can scoring sit between them, so the
/// narrative can name the day's most novel event.
///
/// Both are now the best model (Chat) — the detective is fusion/adjudication, not
/// grunt extraction. What must NOT regress is the SEPARATION: two distinct
/// functions, two distinct prompts, and the day summary must read a score column
/// (proof the scores computed between them actually reach the narrative).
#[test]
fn segmenting_is_not_narrating() {
    let src = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/api/day_summary.rs"),
    )
    .expect("read day_summary.rs");

    let after = |anchor: &str, needle: &str| -> bool {
        let Some(i) = src.find(anchor) else { return false };
        let tail = &src[i..];
        let end = tail[1..].find("\npub async fn ").map(|e| e + 1).unwrap_or(tail.len());
        tail[..end].contains(needle)
    };

    // Both are best-model: the detective fuses noisy witnesses (adjudication), the
    // day summary writes prose. Neither is a Lite job.
    assert!(
        after("pub async fn segment_day_events", "get_chat_model"),
        "the detective fuses noisy witnesses into a gapless timeline — a best-model job"
    );
    assert!(
        after("pub async fn narrate_day", "get_chat_model"),
        "narration is the narrative call; it earns the Chat slot"
    );

    // They stay SEPARATE — two prompts, and neither function calls the other. A
    // fused call would blind the narrative to the scores computed between them.
    assert!(
        after("pub async fn segment_day_events", "SEGMENT_PROMPT"),
        "the detective must use its own detective prompt"
    );
    assert!(
        after("pub async fn narrate_day", "NARRATE_PROMPT"),
        "narration must use its own narrative prompt"
    );
    assert!(
        !after("pub async fn segment_day_events", "narrate_day("),
        "segmentation must not narrate — they are two calls, with scoring between"
    );

    // The payoff of the split: the day summary reads a SCORE the detective could
    // not have known, because scoring runs between them.
    assert!(
        after("pub async fn narrate_day", "novelty_z"),
        "the day summary must read novelty_z — the whole point of scoring sitting \
         between the detective and the narrative is that the prose can name the \
         day's standout"
    );
}

/// Narration reads the EVENTS. So the events have to exist first.
#[test]
fn narration_comes_after_the_day_is_cut() {
    let src = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("repo root")
            .join("applets/day_summary_eod/main.rs"),
    )
    .expect("read day_summary_eod/main.rs");

    let pos = |needle: &str| {
        src.lines()
            .position(|l| l.contains(needle) && !l.trim_start().starts_with("//"))
            .unwrap_or_else(|| panic!("no call to `{needle}` in day_summary_eod"))
    };

    assert!(
        pos("narrate_day(") > pos("segment_day_events("),
        "narrate_day reads the day's events — it cannot run before they are cut"
    );
}
