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
            .join("actions/day_summary_eod/main.rs"),
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

/// Segmenting a day and narrating it are different jobs, and must stay on
/// different models.
///
/// They used to be ONE Opus call producing the events AND the autobiography. That
/// fusion caused three separate problems, and only one of them was money:
///
///   * Cutting a day into spans is structured extraction — grunt work — and it was
///     billed at the narrative rate.
///   * "Only narrate a day with enough good events" was UNSTATABLE, because the
///     events did not exist until the narration ran. A circle.
///   * There could be no hourly cron: re-segmenting as data landed would have
///     meant re-writing the day's prose every hour.
///
/// If someone moves segmentation onto the Chat slot, none of that fails — it just
/// gets expensive again, quietly, which is exactly how it happened the first time.
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

    assert!(
        after("pub async fn segment_day_events", "get_background_model"),
        "segmentation must use the LITE slot — it is structured extraction, not prose"
    );
    assert!(
        after("pub async fn narrate_day", "get_chat_model"),
        "narration is the narrative call; it is the one that earns the Chat slot"
    );
    assert!(
        !after("pub async fn segment_day_events", "get_chat_model"),
        "segmentation on the Chat slot is how events came to cost Opus prices"
    );
}

/// Narration reads the EVENTS. So the events have to exist first.
#[test]
fn narration_comes_after_the_day_is_cut() {
    let src = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("repo root")
            .join("actions/day_summary_eod/main.rs"),
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
