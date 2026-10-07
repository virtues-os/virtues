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

    // ...and nothing else can null them without that rescore. The wipe is private
    // to reindex.rs, reachable only through `rebuild`, and no other source file
    // nulls the scores on its own.
    assert!(
        !code.lines().any(|l| l.trim_start().starts_with("pub") && l.contains("fn wipe(")),
        "reindex's wipe is public — a caller can now null every event score without \
         the rescore `rebuild` does after it"
    );
    let mut elsewhere = Vec::new();
    find_score_nulling(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut elsewhere);
    elsewhere.retain(|p| !p.ends_with("src/cli/reindex.rs"));
    assert!(
        elsewhere.is_empty(),
        "{elsewhere:?} null event scores outside `reindex::rebuild`, the only path \
         that rescores every day afterwards"
    );
}

/// Every `.rs` file under `dir` whose code (comments aside) nulls `novelty_z`.
fn find_score_nulling(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("read src dir") {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            find_score_nulling(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            let src = std::fs::read_to_string(&path).expect("read source file");
            if src
                .lines()
                .any(|l| !l.trim_start().starts_with("//") && l.contains("novelty_z = NULL"))
            {
                out.push(path);
            }
        }
    }
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
/// functions, two distinct prompts, neither calling the other.
///
/// Since 2549d156 the article is written from the day's own record (transcripts,
/// messages, the owner's words) in `day_article::write_day`, not from the
/// detective's events or the scores: each summarizing hop had stripped what made
/// the day matter. So narration no longer reads `novelty_z`; the ordering
/// (segment, score, then narrate) is still pinned by the two tests around this one.
#[test]
fn segmenting_is_not_narrating() {
    let read = |rel: &str| {
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(rel))
            .unwrap_or_else(|e| panic!("read {rel}: {e}"))
    };
    let src = read("src/api/day_summary.rs");
    let article = read("src/api/day_article.rs");

    // The body of the function that starts at `anchor`, up to the next top-level fn.
    let body = |src: &str, anchor: &str| -> Option<String> {
        let i = src.find(anchor)?;
        let tail = &src[i..];
        let end = tail[1..]
            .find("\npub async fn ")
            .into_iter()
            .chain(tail[1..].find("\npub(crate) async fn "))
            .chain(tail[1..].find("\nasync fn "))
            .chain(tail[1..].find("\nfn "))
            .min()
            .map(|e| e + 1)
            .unwrap_or(tail.len());
        Some(tail[..end].to_string())
    };
    let after = |anchor: &str, needle: &str| -> bool {
        body(&src, anchor).is_some_and(|b| b.contains(needle))
    };
    let writer = body(&article, "pub(crate) async fn write_day").expect("day_article::write_day exists");

    // Both are best-model: the detective fuses noisy witnesses (adjudication), the
    // day summary writes prose. Neither is a Lite job. The Standard SLOT DEFAULT,
    // not the profile's pinned Standard model (`get_standard_model`) — a ZDR-incapable
    // pin fails these background writes; see the comments at the call sites.
    assert!(
        after("pub async fn segment_day_events", "ModelSlot::Standard"),
        "the detective fuses noisy witnesses into a gapless timeline — a best-model job"
    );
    assert!(
        after("pub async fn narrate_day", "day_article::write_day"),
        "narrate_day writes the article through day_article::write_day"
    );
    assert!(
        writer.contains("ModelSlot::Standard"),
        "narration is the narrative call; it earns the Standard slot"
    );
    assert!(
        !after("pub async fn segment_day_events", "get_standard_model")
            && !after("pub async fn narrate_day", "get_standard_model")
            && !article.contains("get_standard_model"),
        "background writes must not read the user's Standard pin — a ZDR-incapable \
         pin (grok) makes virtues-api refuse with no_zdr_providers_available"
    );

    // They stay SEPARATE — two prompts, and neither function calls the other.
    assert!(
        after("pub async fn segment_day_events", "SEGMENT_PROMPT"),
        "the detective must use its own detective prompt"
    );
    assert!(
        writer.contains("WRITER_PROMPT") && !writer.contains("SEGMENT_PROMPT"),
        "the article must use its own writer prompt, not the detective's"
    );
    assert!(
        !after("pub async fn segment_day_events", "narrate_day(")
            && !after("pub async fn segment_day_events", "write_day("),
        "segmentation must not narrate — they are two calls, with scoring between"
    );
}

/// The attempt is counted BEFORE the chain runs.
///
/// `record_narration_attempt` is the catch-up queue's memory. Counted after the
/// chain, a run that times out, is killed, or panics mid-way leaves no trace, and
/// the queue offers the same day back next hour as if nothing had happened —
/// which is the hourly-forever retry the budget exists to stop. The call must
/// come before the first step of the chain (audio sessionization).
#[test]
fn attempt_is_recorded_before_the_chain_runs() {
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
        pos("record_narration_attempt(") < pos("sessionize_day("),
        "the attempt must be counted before the chain starts, or a run that dies \
         mid-chain is never counted and the day is retried every hour"
    );
    assert!(
        !src.contains("wiki_events e"),
        "the queue must not live in the applet and must not key on the event \
         count — a day whose cut failed has zero events and would be invisible; \
         use day_summary::next_catchup_day"
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
