//! The registry of observation types, and how each takes part in the pipeline.
//!
//! ## What "ontology" means here — and what it does not
//!
//! It is a misnomer we have kept, and it is worth being precise about because the
//! word invites the wrong expectations. There is no concept hierarchy in this
//! module, no relations between concepts, no inference. What an
//! `OntologyDescriptor` actually declares is a **participation contract**: given
//! one table of observations, how to read its time, whether to embed it for
//! search, whether it carries prose worth extracting entities from, how it
//! reaches a day page, which lifeline lane it plots on, and whether its presence
//! means the owner was DOING something rather than merely being measured.
//!
//! The accurate vocabulary, if you are reasoning about the design rather than
//! reading the code:
//!
//!   * A `data_*` table is an **observation type** — one kind of thing that can
//!     be observed about a life. One row is one observation at one time, with
//!     provenance back to the stream that delivered it. It is never derived from
//!     another table (that is `wiki_*`) and never product state (that is
//!     `app_*`). It is the only layer that can be re-collected; everything
//!     downstream is rebuildable from it.
//!   * A descriptor here is that type's **participation contract**.
//!   * The thing with actual ontological commitments is the PIPELINE —
//!     ingest → resolve → index → cite → narrate — not this table of metadata.
//!
//! The name is not being changed today because `ontology` appears ~340 times and,
//! more to the point, is a stored value in `search_embeddings.ontology` — so a
//! rename is a data migration plus a full reindex, for a word. Written down
//! instead, so at least it means one thing.
//!
//! ## The contract is load-bearing, and silence is its failure mode
//!
//! `search/indexer.rs` decides what is searchable by iterating
//! `registered_ontologies()`. A `data_*` table with no descriptor is therefore
//! collected and then invisible: not searchable, absent from the lifeline, the
//! dayline and day summaries. The collector succeeds, the rows are there, and the
//! data never appears anywhere. That had happened to SEVEN tables before anyone
//! noticed — including imported AI chat history, which is prose.
//!
//! `every_data_table_participates_or_is_exempted` now fails the build for a new
//! one. Exemptions are named individually there, so leaving a table out is a
//! decision rather than an oversight.
//!
//! The actual SQL schema lives in Core migrations.

use serde::{Deserialize, Serialize};

/// Entity-extraction configuration — which ontologies carry PROSE.
///
/// Keyed on the **ontology**, not the source, and that is the whole point:
/// Gmail, Fastmail and an mbox import all normalize into
/// `data_communication_email`, so entity extraction is configured once and
/// every present and future source inherits it. Slack lands in
/// `data_communication_message` and works with no new code at all.
///
/// Of 23 ontologies, five carry prose. The other eighteen — every health,
/// location, financial and activity table — have no free text and never enter
/// extraction. That is the real size of this problem.
///
/// `data_communication_transcription` deliberately has NO config here: its
/// entities are already extracted by the transcription action's own LLM call,
/// so it is drained for free rather than re-extracted (see
/// `entity_resolution::extract`). Paying twice for the same names would be
/// absurd.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractionConfig {
    /// SQL expression for the prose to read (`t.` prefix; the query aliases the
    /// table as `t`). Usually the same text we embed — it is the same text.
    pub text_sql: &'static str,
    /// SQL predicate that excludes records not worth reading (`t.` prefix).
    ///
    /// This is where money is saved and precision is bought, and for email it
    /// is not a heuristic: Gmail's own `labelIds` are stored verbatim, so
    /// Google's classifier — the one that fills your Promotions tab — does the
    /// work. Excluded records are NOT deleted; they remain as dust, searchable,
    /// merely never read for names.
    pub filter_sql: Option<&'static str>,
    /// Hard cap on prose sent to the model, in characters.
    ///
    /// Email and documents are where token mass hides: a newsletter is mostly
    /// boilerplate, a reply chain is mostly the quoted reply. Names appear
    /// early. Truncation costs almost no recall and bounds the bill.
    pub max_chars: usize,
}

/// Embedding configuration for semantic search
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingConfig {
    /// SQL expression for text to embed (use `t.` prefix for column refs — query aliases table as `t`)
    pub embed_text_sql: &'static str,
    /// Content type label for search results (e.g., "email", "document")
    pub content_type: &'static str,
    /// SQL expression for result title (use `t.` prefix — query aliases table as `t`)
    pub title_sql: Option<&'static str>,
    /// SQL expression for result preview (use `t.` prefix — query aliases table as `t`)
    pub preview_sql: &'static str,
    /// SQL expression for author/source (use `t.` prefix — query aliases table as `t`)
    pub author_sql: Option<&'static str>,
    /// SQL expression for timestamp (use `t.` prefix — query aliases table as `t`)
    pub timestamp_sql: &'static str,
    /// Optional extra predicate restricting WHICH ROWS of the table this
    /// ontology owns. Must begin with `AND` — it is spliced into the indexer's
    /// backlog query (see `extra_where_carries_its_own_and`).
    ///
    /// Exists because one table can hold two kinds of document. `app_pages`
    /// holds pages a person wrote and articles the record wrote about itself;
    /// they want different `content_type` labels, different day-source
    /// behaviour, and different retrieval treatment, so they are two
    /// ontologies over one table. Without a filter here they are not two
    /// ontologies at all — the indexer would embed every row twice, once under
    /// each name, because `embedding_id` is keyed `{ontology}:{record}:{chunk}`.
    pub embed_where: Option<&'static str>,
}

/// Whether an ontology produces discrete events or continuous measurement streams
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum TemporalType {
    /// Individual occurrences with timestamps (e.g., calendar events, messages, workouts)
    Discrete,
    /// Constant measurement stream needing aggregation (e.g., heart rate, HRV, steps)
    Continuous,
}

/// How a discrete ontology contributes to day sources.
/// SQL expressions use `t.` prefix (table aliased as `t`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaySourceConfig {
    /// Static source type label (e.g., "calendar", "email", "workout")
    pub source_type: &'static str,
    /// Optional SQL expression for dynamic source_type (overrides source_type when present)
    pub source_type_sql: Option<&'static str>,
    /// SQL expression for the event label
    pub label_sql: &'static str,
    /// SQL expression for the event preview text
    pub preview_sql: &'static str,
    /// SQL expression for the record ID
    pub id_sql: &'static str,
    /// Optional additional WHERE clause (e.g., confidence filters)
    pub extra_where: Option<&'static str>,
    /// If true, use `date(column) = $1` instead of timestamp range comparison
    pub use_date_filter: bool,
}

/// How a continuous ontology produces per-window summary stats.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContinuousAggConfig {
    /// Template string with placeholders: {avg}, {min}, {max}, {std}, {sum}, {count}
    pub summary_template: &'static str,
    /// SQL expression for the numeric value column
    pub value_sql: &'static str,
    /// Aggregation type: "stats" (avg/min/max/std) or "sum"
    pub agg_type: &'static str,
}

/// Ontology descriptor - metadata only
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OntologyDescriptor {
    /// Unique ontology name (e.g., "health_sleep", "calendar")
    pub name: &'static str,
    /// Human-readable display name
    pub display_name: &'static str,
    /// Description of what this ontology stores
    pub description: &'static str,
    /// Domain grouping (e.g., "health", "location", "social")
    pub domain: &'static str,
    /// Which lifeline lane this belongs to, if any.
    ///
    /// `None` means "not part of a life you would scan" — the record's own
    /// artifacts (pages, chats, articles, extracted chunks), intent rather than
    /// evidence (calendar), ambient conditions rather than conduct (weather),
    /// and anything derived from other lanes (`wiki_events`, which is a
    /// SUMMARY of the lanes and would double-count them).
    ///
    /// Declared per descriptor rather than derived from `domain`, because
    /// `domain` describes which collector family the data came from and this
    /// describes what it means to a person. They coincide today; a blocklist
    /// over `domain` would silently admit the next domain someone adds.
    pub lane: Option<&'static str>,
    /// Full database table name (e.g., "data_health_sleep", "chats")
    pub table_name: &'static str,
    /// Source streams that feed into this ontology
    pub source_streams: Vec<&'static str>,
    /// Primary timestamp column
    pub timestamp_column: &'static str,
    /// Optional end timestamp column for span/duration events
    pub end_timestamp_column: Option<&'static str>,
    /// Embedding configuration for semantic search (None if not searchable)
    pub embedding: Option<EmbeddingConfig>,
    /// Entity-extraction configuration (None = this ontology carries no prose,
    /// which is true of 18 of the 23 — every health, location, financial and
    /// activity table). See `ExtractionConfig`.
    pub extraction: Option<ExtractionConfig>,
    /// Whether this ontology produces discrete events or continuous measurements
    pub temporal_type: TemporalType,
    /// How discrete ontologies contribute to day sources (None for continuous/non-event ontologies)
    pub day_source: Option<DaySourceConfig>,
    /// How continuous ontologies produce aggregated summaries (None for discrete ontologies)
    pub continuous_agg: Option<ContinuousAggConfig>,
    /// Whether this ontology represents active behavioral signal (for action activation gates).
    /// True for: app_usage, location_visit, calendar, outbound messages, transcription, web_browsing, workout.
    /// False for passive data: heart_rate, hrv, steps, sleep, inbound email, financial records.
    pub is_activation_signal: bool,
}

/// What a lifeline lane can plot besides "how many rows landed here".
///
/// A row count is an artifact of COLLECTION, not of life: 300 heart-rate
/// samples on Tuesday against 280 on Wednesday tells you about the watch. The
/// financial lane counting transactions says nothing about money, and the
/// health lane counting samples says nothing about a body. A measure names the
/// number a person would actually ask for.
///
/// Kept as one flat table rather than a field on all 26 descriptors, because
/// measures are worth reading AS a table — every number the lifeline can draw,
/// in sixty lines you can scan — and because most ontologies have none.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct LaneMeasure {
    /// Stable id; appears in the URL, so renaming one breaks saved views.
    pub id: &'static str,
    pub lane: &'static str,
    /// The single table this reads. A measure never spans tables: averaging a
    /// heart rate together with a step count is not a number.
    pub table: &'static str,
    pub timestamp_column: &'static str,
    pub label: &'static str,
    /// Suffix for the readout. Empty when the label already says it.
    pub unit: &'static str,
    /// The bucket aggregate. References only `table`'s columns — it is
    /// interpolated into SQL, so it must stay a compile-time constant.
    pub agg: &'static str,
    /// Rows that carry this measure at all, ANDed onto the window.
    pub filter: Option<&'static str>,
    pub kind: MeasureKind,
}

/// Whether merging two buckets adds their values.
///
/// This decides how a lane may be drawn and rescaled. Zooming out merges
/// buckets: a `Total` legitimately grows (a month holds more steps than a day),
/// a `Rate` must not (a month of heart rate is not a bigger heart rate). Drawn
/// differently for the same reason — bars stand on zero and mean quantity, a
/// band floats between its own floor and ceiling and means level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MeasureKind {
    /// Steps, spend, messages, hours asleep.
    Total,
    /// Heart rate, HRV — an average, and only ever an average.
    Rate,
}

/// Every measure the lifeline can draw.
///
/// `is_from_me` lives in `metadata` rather than a column, so the sent/received
/// split reads JSONB; it is present on all 169k message rows as the text
/// 'true'/'false'. Financial amounts follow the Plaid sign convention —
/// POSITIVE is money leaving — which is why `spend` filters `> 0` and `income`
/// negates. Both were checked against a real box rather than assumed.
pub fn lane_measures() -> &'static [LaneMeasure] {
    use MeasureKind::{Rate, Total};
    const M: &[LaneMeasure] = &[
        // ── health ──────────────────────────────────────────────────────────
        LaneMeasure { id: "heart_rate", lane: "health", table: "data_health_heart_rate",
            timestamp_column: "occurred_at", label: "heart rate", unit: "bpm",
            agg: "avg(bpm)", filter: Some("bpm > 0"), kind: Rate },
        LaneMeasure { id: "hrv", lane: "health", table: "data_health_hrv",
            timestamp_column: "occurred_at", label: "HRV", unit: "ms",
            agg: "avg(hrv_ms)", filter: Some("hrv_ms > 0"), kind: Rate },
        LaneMeasure { id: "sleep", lane: "health", table: "data_health_sleep",
            timestamp_column: "started_at", label: "sleep", unit: "h",
            agg: "sum(duration_minutes) / 60.0", filter: None, kind: Total },
        LaneMeasure { id: "steps", lane: "health", table: "data_health_steps",
            timestamp_column: "occurred_at", label: "steps", unit: "",
            agg: "sum(step_count)", filter: None, kind: Total },
        LaneMeasure { id: "workouts", lane: "health", table: "data_health_workout",
            timestamp_column: "started_at", label: "workouts", unit: "",
            agg: "count(*)", filter: None, kind: Total },
        // ── location ────────────────────────────────────────────────────────
        LaneMeasure { id: "visits", lane: "location", table: "data_location_visit",
            timestamp_column: "started_at", label: "visits", unit: "",
            agg: "count(*)", filter: None, kind: Total },
        LaneMeasure { id: "time_out", lane: "location", table: "data_location_visit",
            timestamp_column: "started_at", label: "time out", unit: "h",
            agg: "sum(duration_minutes) / 60.0", filter: None, kind: Total },
        LaneMeasure { id: "places", lane: "location", table: "data_location_visit",
            timestamp_column: "started_at", label: "distinct places", unit: "",
            agg: "count(DISTINCT place_name)", filter: Some("place_name IS NOT NULL"),
            kind: Total },
        // ── communication ───────────────────────────────────────────────────
        LaneMeasure { id: "sent", lane: "communication", table: "data_communication_message",
            timestamp_column: "occurred_at", label: "messages sent", unit: "",
            agg: "count(*)", filter: Some("metadata->>'is_from_me' = 'true'"), kind: Total },
        LaneMeasure { id: "received", lane: "communication", table: "data_communication_message",
            timestamp_column: "occurred_at", label: "messages received", unit: "",
            agg: "count(*)", filter: Some("metadata->>'is_from_me' = 'false'"), kind: Total },
        LaneMeasure { id: "people", lane: "communication", table: "data_communication_message",
            timestamp_column: "occurred_at", label: "people spoken to", unit: "",
            agg: "count(DISTINCT from_handle)", filter: Some("from_handle IS NOT NULL"),
            kind: Total },
        LaneMeasure { id: "talk", lane: "communication", table: "data_communication_transcription",
            timestamp_column: "started_at", label: "speech heard", unit: "min",
            agg: "sum(duration_seconds) / 60.0", filter: None, kind: Total },
        // ── financial ───────────────────────────────────────────────────────
        LaneMeasure { id: "spend", lane: "financial", table: "data_financial_transaction",
            timestamp_column: "occurred_at", label: "spend", unit: "$",
            agg: "sum(amount) / 100.0", filter: Some("amount > 0"), kind: Total },
        LaneMeasure { id: "income", lane: "financial", table: "data_financial_transaction",
            timestamp_column: "occurred_at", label: "income", unit: "$",
            agg: "sum(-amount) / 100.0", filter: Some("amount < 0"), kind: Total },
        LaneMeasure { id: "transactions", lane: "financial", table: "data_financial_transaction",
            timestamp_column: "occurred_at", label: "transactions", unit: "",
            agg: "count(*)", filter: None, kind: Total },
        // ── activity ────────────────────────────────────────────────────────
        LaneMeasure { id: "screen", lane: "activity", table: "data_activity_app_session",
            timestamp_column: "started_at", label: "screen time", unit: "h",
            agg: "sum(EXTRACT(EPOCH FROM (ended_at - started_at))) / 3600.0",
            filter: Some("ended_at IS NOT NULL"), kind: Total },
        LaneMeasure { id: "apps", lane: "activity", table: "data_activity_app_session",
            timestamp_column: "started_at", label: "distinct apps", unit: "",
            agg: "count(DISTINCT app_name)", filter: Some("app_name IS NOT NULL"), kind: Total },
        LaneMeasure { id: "browsing", lane: "activity", table: "data_activity_web_browsing",
            timestamp_column: "occurred_at", label: "pages visited", unit: "",
            agg: "count(*)", filter: None, kind: Total },
    ];
    M
}

/// The measures offered for one lane, in declaration order.
pub fn measures_for_lane(lane: &str) -> Vec<LaneMeasure> {
    lane_measures().iter().copied().filter(|m| m.lane == lane).collect()
}

/// A stream that means "awake, and doing something".
#[derive(Debug, Clone, Copy)]
pub struct ActivitySource {
    pub table: &'static str,
    pub timestamp_column: &'static str,
    /// Narrowing beyond the ontology, where the table mixes signals.
    pub filter: Option<&'static str>,
}

/// What the day-clock raster is allowed to draw.
///
/// The raster's whole value is that SLEEP shows up as a dark band, and that
/// only works if every mark means a person was awake. `is_activation_signal`
/// already draws that line — it is documented as true for app usage, visits,
/// outbound messages, transcription, browsing and workouts, and
/// false for heart rate, HRV, steps and sleep. A watch samples a pulse all
/// night; including it would fill the exact rows the band is made of.
///
/// The one narrowing the descriptor cannot express: a message table holds both
/// directions, and a text ARRIVING at 3am says nothing about whether anyone was
/// awake to read it. The registry's own comment says the signal is *outbound*
/// messages, so that is what this asks for.
pub fn activity_sources() -> Vec<ActivitySource> {
    registered_ontologies()
        .into_iter()
        .filter(|o| o.lane.is_some() && o.is_activation_signal)
        .map(|o| ActivitySource {
            table: o.table_name,
            timestamp_column: o.timestamp_column,
            filter: match o.table_name {
                "data_communication_message" => Some("metadata->>'is_from_me' = 'true'"),
                _ => None,
            },
        })
        .collect()
}

/// Get all registered ontology descriptors
pub fn registered_ontologies() -> Vec<OntologyDescriptor> {
    vec![
        // ===== Health Ontologies =====
        OntologyDescriptor {
            name: "health_heart_rate",
            display_name: "Heart Rate",
            description: "Heart rate measurements from HealthKit",
            domain: "health",
            lane: Some("health"),
            table_name: "data_health_heart_rate",
            source_streams: vec!["stream_ios_healthkit"],
            timestamp_column: "occurred_at",
            end_timestamp_column: None,
            embedding: None,
            extraction: None,
            temporal_type: TemporalType::Continuous,
            // Continuous streams surface as individual day-source rows too, so the
            // day page can list every data point. They are high-frequency, so the
            // UI hides them behind a filter by default (keyed off `temporal_type`).
            day_source: Some(DaySourceConfig {
                source_type: "heart_rate",
                source_type_sql: None,
                label_sql: "'Heart rate'",
                preview_sql: "CAST(t.bpm AS TEXT) || ' bpm'",
                id_sql: "t.id",
                extra_where: None,
                use_date_filter: false,
            }),
            continuous_agg: Some(ContinuousAggConfig {
                summary_template: "Heart rate: avg {avg} bpm ({min}-{max})",
                value_sql: "t.bpm",
                agg_type: "stats",
            }),
            //                    who  whom what when where why  how
            is_activation_signal: false,
        },
        OntologyDescriptor {
            name: "health_hrv",
            display_name: "Heart Rate Variability",
            description: "HRV measurements indicating stress and recovery",
            domain: "health",
            lane: Some("health"),
            table_name: "data_health_hrv",
            source_streams: vec!["stream_ios_healthkit"],
            timestamp_column: "occurred_at",
            end_timestamp_column: None,
            embedding: None,
            extraction: None,
            temporal_type: TemporalType::Continuous,
            day_source: Some(DaySourceConfig {
                source_type: "hrv",
                source_type_sql: None,
                label_sql: "'HRV'",
                preview_sql: "CAST(ROUND(t.hrv_ms) AS INT) || ' ms'",
                id_sql: "t.id",
                extra_where: None,
                use_date_filter: false,
            }),
            continuous_agg: Some(ContinuousAggConfig {
                summary_template: "HRV: avg {avg}ms ({min}-{max})",
                value_sql: "t.hrv_ms",
                agg_type: "stats",
            }),
            //                    who  whom what when where why  how
            is_activation_signal: false,
        },
        OntologyDescriptor {
            name: "health_steps",
            display_name: "Steps",
            description: "Step count data from HealthKit",
            domain: "health",
            lane: Some("health"),
            table_name: "data_health_steps",
            source_streams: vec!["stream_ios_healthkit"],
            timestamp_column: "occurred_at",
            end_timestamp_column: None,
            embedding: None,
            extraction: None,
            temporal_type: TemporalType::Continuous,
            day_source: Some(DaySourceConfig {
                source_type: "steps",
                source_type_sql: None,
                label_sql: "'Steps'",
                preview_sql: "CAST(t.step_count AS TEXT) || ' steps'",
                id_sql: "t.id",
                extra_where: None,
                use_date_filter: false,
            }),
            continuous_agg: Some(ContinuousAggConfig {
                summary_template: "Steps: {sum} total",
                value_sql: "t.count",
                agg_type: "sum",
            }),
            //                    who  whom what when where why  how
            is_activation_signal: false,
        },
        OntologyDescriptor {
            name: "environment_weather",
            display_name: "Weather",
            description: "Weather where the owner was — observations, and forecasts kept separately",
            domain: "environment",
            // NOT a lane, and this is a decision already made and tested twice:
            // `lane`'s own doc excludes "ambient conditions rather than conduct
            // (weather)", and `the_exclusions_hold` asserts it with the reason
            // "weather is a condition you were in, not something you did".
            //
            // A descriptor without a lane is still worth having — it is what
            // makes this table visible to the registry at all, and stops it
            // being collected into silence. Whether ambient context should ALSO
            // plot on the lifeline is a product reversal, not a gap, and belongs
            // to whoever owns that doctrine.
            lane: None,
            table_name: "data_environment_weather",
            source_streams: vec!["stream_weather"],
            timestamp_column: "occurred_at",
            end_timestamp_column: None,
            // Numbers, not prose. Nothing here is worth embedding — "18°C" does
            // not answer a question anybody phrases in words, and the day page
            // renders the reading directly.
            embedding: None,
            // No entity extraction: weather mentions no one.
            extraction: None,
            temporal_type: TemporalType::Continuous,
            // Ambient context on the day page rather than a day SOURCE: a day is
            // not evidenced by the weather, and counting weather rows as sources
            // would inflate a quiet day into a busy one. The lane measures carry
            // it instead.
            day_source: None,
            continuous_agg: None,
            //                    who  whom what when where why  how
            // Not activation: rain is not the owner doing something.
            is_activation_signal: false,
        },
        OntologyDescriptor {
            name: "content_conversation",
            display_name: "AI Conversations",
            description: "Imported chat history from other AI providers — one row per turn",
            domain: "content",
            // No lane. This is content the owner produced and consumed, like a
            // bookmark or a document — not a rhythm of the day worth plotting.
            lane: None,
            table_name: "data_content_conversation",
            source_streams: vec!["stream_chat_import"],
            timestamp_column: "occurred_at",
            end_timestamp_column: None,
            // THE POINT OF GIVING THIS A CONTRACT AT ALL. It is prose, it is the
            // owner's own thinking, and it was unsearchable — "what was I working
            // on last spring" could not reach a year of it.
            embedding: Some(EmbeddingConfig {
                embed_text_sql: "COALESCE(t.content, '')",
                content_type: "conversation",
                // The provider is the closest thing to a counterpart's name.
                title_sql: Some("COALESCE(t.provider, 'AI') || ' conversation'"),
                preview_sql: "left(COALESCE(t.content, ''), 200)",
                // `role` is who said it — 'user' or 'assistant'. That is the
                // honest author for a turn, and it lets a result say which half
                // of the exchange matched.
                author_sql: Some("t.role"),
                timestamp_sql: "t.occurred_at",
                embed_where: None,
            }),
            // NO ENTITY EXTRACTION, deliberately, and this is the one judgement
            // call in the descriptor. Extraction resolves the people, places and
            // organizations named in prose into the owner's graph — but the names
            // in an AI conversation are usually DISCUSSED, not known: characters,
            // public figures, examples, a company being researched. Running it
            // here would populate `wiki_people` with people the owner has never
            // met, and there is no signal in the row to tell those apart from the
            // ones they have.
            extraction: None,
            temporal_type: TemporalType::Discrete,
            // Not a day source yet. A turn is too granular to list on a day page
            // — a single session can be two hundred of them — and the right unit
            // (a conversation, grouped by `conversation_id`) is a rollup nothing
            // builds today.
            day_source: None,
            continuous_agg: None,
            //                    who  whom what when where why  how
            // Talking to an AI is the owner doing something, and the activation
            // gate is about whether the box saw activity at all.
            is_activation_signal: true,
        },
        OntologyDescriptor {
            name: "health_sleep",
            display_name: "Sleep Sessions",
            description: "Sleep analysis from HealthKit with quality metrics",
            domain: "health",
            lane: Some("health"),
            table_name: "data_health_sleep",
            source_streams: vec!["stream_ios_healthkit"],
            timestamp_column: "started_at",
            end_timestamp_column: Some("ended_at"),
            embedding: None,
            extraction: None,
            temporal_type: TemporalType::Discrete,
            day_source: Some(DaySourceConfig {
                source_type: "sleep",
                source_type_sql: None,
                label_sql: "'Sleep'",
                preview_sql: "CASE WHEN t.duration_minutes IS NOT NULL THEN CAST(t.duration_minutes AS TEXT) || ' min' ELSE NULL END",
                id_sql: "t.id",
                extra_where: None,
                use_date_filter: false,
            }),
            continuous_agg: None,
            //                    who  whom what when where why  how
            is_activation_signal: false,
        },
        OntologyDescriptor {
            name: "health_workout",
            display_name: "Workouts",
            description: "Workout sessions from HealthKit and Strava",
            domain: "health",
            lane: Some("health"),
            table_name: "data_health_workout",
            source_streams: vec!["stream_ios_healthkit", "stream_strava_activities"],
            timestamp_column: "started_at",
            end_timestamp_column: Some("ended_at"),
            embedding: None,
            extraction: None,
            temporal_type: TemporalType::Discrete,
            day_source: Some(DaySourceConfig {
                source_type: "workout",
                source_type_sql: None,
                label_sql: "COALESCE(t.workout_type, 'Workout')",
                preview_sql: "CASE WHEN t.duration_minutes IS NOT NULL THEN CAST(t.duration_minutes AS TEXT) || ' min' ELSE NULL END",
                id_sql: "t.id",
                extra_where: None,
                use_date_filter: false,
            }),
            continuous_agg: None,
            //                    who  whom what when where why  how
            is_activation_signal: true,
        },
        // ===== Location Ontologies =====
        OntologyDescriptor {
            name: "location_point",
            display_name: "Location Points",
            description: "Raw GPS coordinates from device location services",
            domain: "location",
            lane: Some("location"),
            table_name: "data_location_point",
            source_streams: vec!["stream_ios_location"],
            timestamp_column: "occurred_at",
            end_timestamp_column: None,
            embedding: None,
            extraction: None,
            temporal_type: TemporalType::Continuous,
            day_source: None,
            continuous_agg: None, // Spatial data — not a numeric aggregate
            //                    who  whom what when where why  how
            is_activation_signal: false,
        },
        OntologyDescriptor {
            name: "location_visit",
            display_name: "Location Visits",
            description: "Clustered location visits with place resolution",
            domain: "location",
            lane: Some("location"),
            table_name: "data_location_visit",
            source_streams: vec![], // Derived from location_point via clustering
            timestamp_column: "started_at",
            end_timestamp_column: Some("ended_at"),
            embedding: None,
            extraction: None,
            temporal_type: TemporalType::Discrete,
            day_source: Some(DaySourceConfig {
                source_type: "location",
                source_type_sql: None,
                label_sql: "COALESCE(t.place_name, 'Unknown location')",
                preview_sql: "CASE WHEN t.duration_minutes IS NOT NULL THEN CAST(t.duration_minutes AS TEXT) || ' min' ELSE NULL END",
                // `t.id` is TEXT. `encode(t.id, 'hex')` — which lived here — takes
                // bytea, so this query raised `function encode(text, unknown) does
                // not exist` on EVERY day, for every user, since it was written.
                // It failed as a `warn!`, not an error, so the day simply had no
                // location sources and the cron reported success.
                id_sql: "t.id",
                extra_where: None,
                use_date_filter: false,
            }),
            continuous_agg: None,
            //                    who  whom what when where why  how
            is_activation_signal: true,
        },
        // ===== Communication Ontologies =====
        OntologyDescriptor {
            name: "communication_email",
            display_name: "Email",
            description: "Email messages from Gmail and other providers",
            domain: "communication",
            lane: Some("communication"),
            table_name: "data_communication_email",
            source_streams: vec!["stream_google_gmail"],
            timestamp_column: "occurred_at",
            end_timestamp_column: None,
            embedding: Some(EmbeddingConfig {
                embed_text_sql: "COALESCE(t.subject, '') || '\n\n' || COALESCE(t.body, '')",
                content_type: "email",
                title_sql: Some("t.subject"),
                preview_sql: "COALESCE(SUBSTR(t.body_preview, 1, 200), SUBSTR(t.body, 1, 200), '')",
                author_sql: Some("t.from_name"),
                timestamp_sql: "t.occurred_at",
                embed_where: None,
            }),
            // Bodies only. The From:/To: headers are ALREADY resolved
            // structurally (entity_resolution::people) — that is a join, not a
            // guess. Re-extracting the sender's name out of a signature block
            // would build a second, worse path to the same person.
            extraction: Some(ExtractionConfig {
                text_sql: "COALESCE(t.subject, '') || E'\n\n' || COALESCE(t.body, '')",
                // Google already sorted the junk. Its labelIds are stored
                // verbatim, so this is its classifier, not our heuristic.
                // CATEGORY_UPDATES is deliberately KEPT: flight confirmations,
                // bookings and receipts look like noise and are some of the
                // densest real places, orgs and future dates in the mailbox.
                filter_sql: Some(
                    "NOT (t.labels ?| array['CATEGORY_PROMOTIONS','CATEGORY_SOCIAL','CATEGORY_FORUMS','SPAM'])",
                ),
                max_chars: 6000,
            }),
            temporal_type: TemporalType::Discrete,
            day_source: Some(DaySourceConfig {
                source_type: "email",
                source_type_sql: Some("CASE WHEN t.direction = 'sent' THEN 'email_sent' ELSE 'email' END"),
                label_sql: "COALESCE(t.subject, '(no subject)')",
                preview_sql: "CASE WHEN t.direction = 'sent' THEN 'To: ' ELSE 'From: ' END || COALESCE(t.from_email, 'unknown')",
                id_sql: "t.id",
                extra_where: None,
                use_date_filter: false,
            }),
            continuous_agg: None,
            //                    who  whom what when where why  how
            is_activation_signal: false,
        },
        OntologyDescriptor {
            name: "communication_message",
            display_name: "Messages",
            description: "SMS and iMessage conversations",
            domain: "communication",
            lane: Some("communication"),
            table_name: "data_communication_message",
            source_streams: vec!["stream_mac_imessage"],
            timestamp_column: "occurred_at",
            end_timestamp_column: None,
            embedding: Some(EmbeddingConfig {
                embed_text_sql: "'From ' || COALESCE(t.from_name, 'Unknown') || ': ' || COALESCE(t.body, '')",
                content_type: "message",
                title_sql: None,
                preview_sql: "SUBSTR(t.body, 1, 200)",
                author_sql: Some("t.from_name"),
                timestamp_sql: "t.occurred_at",
                embed_where: None,
            }),
            extraction: Some(ExtractionConfig {
                text_sql: "COALESCE(t.body, '')",
                filter_sql: None,
                max_chars: 2000,
            }),
            temporal_type: TemporalType::Discrete,
            day_source: Some(DaySourceConfig {
                source_type: "message",
                source_type_sql: Some("'message:' || COALESCE(t.channel, 'unknown')"),
                label_sql: "COALESCE(t.from_name, 'Unknown')",
                preview_sql: "SUBSTR(COALESCE(t.body, ''), 1, 50)",
                id_sql: "t.id",
                extra_where: None,
                use_date_filter: false,
            }),
            continuous_agg: None,
            //                    who  whom what when where why  how
            is_activation_signal: true,
        },
        // ===== Calendar Ontology =====
        OntologyDescriptor {
            name: "calendar_event",
            display_name: "Calendar Events",
            description: "Scheduled events from Google Calendar and iOS EventKit",
            domain: "calendar",
            lane: None,
            table_name: "data_calendar_event",
            source_streams: vec!["stream_google_calendar", "stream_ios_eventkit"],
            timestamp_column: "started_at",
            end_timestamp_column: Some("ended_at"),
            embedding: Some(EmbeddingConfig {
                embed_text_sql: "COALESCE(t.title, '') || '\n\n' || COALESCE(t.description, '')",
                content_type: "calendar",
                title_sql: Some("t.title"),
                preview_sql: "COALESCE(SUBSTR(t.description, 1, 200), '')",
                author_sql: None,
                timestamp_sql: "t.started_at",
                embed_where: None,
            }),
            extraction: None,
            temporal_type: TemporalType::Discrete,
            day_source: Some(DaySourceConfig {
                source_type: "calendar",
                source_type_sql: None,
                label_sql: "COALESCE(t.title, '(no title)')",
                preview_sql: "NULL",
                id_sql: "t.id",
                extra_where: None,
                use_date_filter: false,
            }),
            continuous_agg: None,
            //                    who  whom what when where why  how
            is_activation_signal: true,
        },
        // ===== Activity Ontologies =====
        OntologyDescriptor {
            name: "activity_app_session",
            display_name: "App Usage",
            description: "Attended time in an app — bounded by idle, lock and machine suspend",
            domain: "activity",
            lane: Some("activity"),
            table_name: "data_activity_app_session",
            source_streams: vec!["stream_mac_apps"],
            timestamp_column: "started_at",
            end_timestamp_column: Some("ended_at"),
            embedding: None,
            extraction: None,
            temporal_type: TemporalType::Discrete,
            day_source: Some(DaySourceConfig {
                source_type: "app_usage",
                source_type_sql: None,
                label_sql: "COALESCE(t.app_name, 'Unknown app')",
                preview_sql: "t.window_title",
                id_sql: "t.id",
                // An open session's ended_at is provisional (it walks forward with
                // each heartbeat), so it would otherwise show up in a day's timeline
                // as a zero-length blip until it closes.
                // The `AND` is not decoration: `extra_where` is spliced raw after
                // the WHERE clause. Without it this read `... <= $2 t.is_open =
                // false`, so Postgres said `syntax error at or near "t"` and app
                // sessions vanished from every day — as a warning, not an error.
                extra_where: Some("AND t.is_open = false"),
                use_date_filter: false,
            }),
            continuous_agg: None,
            //                    who  whom what when where why  how
            is_activation_signal: true,
        },
        OntologyDescriptor {
            name: "activity_web_browsing",
            display_name: "Web Browsing",
            description: "Browser history from Safari and Chrome",
            domain: "activity",
            lane: Some("activity"),
            table_name: "data_activity_web_browsing",
            source_streams: vec!["stream_mac_browser"],
            timestamp_column: "occurred_at",
            end_timestamp_column: None,
            embedding: None,
            extraction: None,
            temporal_type: TemporalType::Discrete,
            day_source: Some(DaySourceConfig {
                source_type: "web_browsing",
                source_type_sql: None,
                label_sql: "COALESCE(t.page_title, t.url, 'Unknown page')",
                preview_sql: "t.domain",
                id_sql: "t.id",
                extra_where: None,
                use_date_filter: false,
            }),
            continuous_agg: None,
            //                    who  whom what when where why  how
            is_activation_signal: true,
        },
        OntologyDescriptor {
            name: "communication_transcription",
            display_name: "Voice Transcriptions",
            description: "Transcribed audio from microphone recordings",
            domain: "communication",
            lane: Some("communication"),
            table_name: "data_communication_transcription",
            source_streams: vec!["stream_ios_microphone"],
            timestamp_column: "started_at",
            end_timestamp_column: Some("ended_at"),
            // What you actually SAID — and until now, the one thing search could
            // not find. A row here is not an audio chunk: `transcription_resolution`
            // has already assembled the whole conversation (durations run to 6300s)
            // and given it a title, a summary and speaker-labelled text. It was a
            // finished document sitting outside the corpus.
            //
            // Title and summary lead, because they are the part a retriever can
            // match against a question; the verbatim follows as the evidence.
            embedding: Some(EmbeddingConfig {
                embed_text_sql: "COALESCE(t.title, '') || E'\n' || COALESCE(t.summary, '') \
                                 || E'\n\n' || COALESCE(t.text, '')",
                content_type: "transcription",
                title_sql: Some("t.title"),
                preview_sql: "COALESCE(SUBSTR(t.summary, 1, 200), SUBSTR(t.text, 1, 200), '')",
                author_sql: None,
                timestamp_sql: "t.started_at",
                // Silence is not a document.
                //
                // 68.6% of rows on a real box are silent chunks — the collector
                // records continuously and most of a day has nobody talking. They
                // were all being embedded anyway, producing 1,911 identical
                // "Silence / No speech detected" vectors: no recall value, real
                // cost, and enough repeated points to distort HNSW neighbourhoods
                // and skew the BM25 corpus statistics that every other query is
                // scored against.
                //
                // The row stays — it is an honest record that the microphone was
                // on and heard nothing, and the day pipeline reads coverage from
                // it. It just is not something a retriever should ever match.
                embed_where: Some("AND btrim(COALESCE(t.text, '')) <> ''"),
            }),
            // No ExtractionConfig, deliberately: the transcription action's own LLM
            // call already emits `entities`, and `entity_resolution::extract` drains
            // that column. Paying a second model to re-read the same words would be
            // absurd.
            extraction: None,
            temporal_type: TemporalType::Discrete,
            // NOT a day source any more. A 5-minute transcription chunk is a
            // recorder artifact — 271 a day drowned the detective. The day pipeline
            // reads `audio_session` instead (the changepoint rollup below); the
            // chunks stay here as the fine-grained citation + search layer.
            day_source: None,
            continuous_agg: None,
            //                    who  whom what when where why  how
            is_activation_signal: true,
        },
        // The audio SESSION — the coarse unit the day pipeline reads. One row per
        // coherent context (a conversation, a drive, a stretch of sleep), rolled up
        // from the 5-minute chunks by `sessionize::audio`. This is the "visit" of
        // audio: raw recording → chunk transcript → session, mirroring
        // point → visit. No embedding/extraction here yet — those stay on the chunk
        // ontology until search is repurposed (agents/record/event-timeline.md).
        OntologyDescriptor {
            name: "audio_session",
            display_name: "Audio Sessions",
            description: "Coherent audio context sessions (changepoint rollup of transcription chunks)",
            domain: "communication",
            lane: Some("communication"),
            table_name: "data_audio_session",
            source_streams: vec!["stream_ios_microphone"],
            timestamp_column: "started_at",
            end_timestamp_column: Some("ended_at"),
            embedding: None,
            extraction: None,
            temporal_type: TemporalType::Discrete,
            day_source: Some(DaySourceConfig {
                source_type: "audio",
                source_type_sql: None,
                // No title exists — the sessionizer is mechanical. Label from the
                // one classification the acoustic signal supports; the detective
                // reads `content` (the preview) for what was actually said.
                label_sql: "CASE t.speaker_mode \
                            WHEN 0 THEN 'Ambient audio' WHEN 1 THEN 'Solo audio' \
                            WHEN 2 THEN 'Conversation' ELSE 'Group conversation' END",
                preview_sql: "SUBSTR(COALESCE(t.content, ''), 1, 120)",
                id_sql: "t.id",
                extra_where: None,
                use_date_filter: false,
            }),
            continuous_agg: None,
            //                    who  whom what when where why  how
            is_activation_signal: true,
        },
        // ===== Content Ontologies =====
        OntologyDescriptor {
            name: "content_document",
            display_name: "Documents",
            description: "Pages from Notion and other document sources",
            domain: "content",
            lane: None,
            table_name: "data_content_document",
            source_streams: vec!["stream_notion_pages"],
            timestamp_column: "occurred_at",
            end_timestamp_column: None,
            embedding: Some(EmbeddingConfig {
                embed_text_sql: "COALESCE(t.title, '') || '\n\n' || COALESCE(t.content_summary, SUBSTR(t.content, 1, 8000), '')",
                content_type: "document",
                title_sql: Some("t.title"),
                preview_sql: "COALESCE(SUBSTR(t.content_summary, 1, 200), SUBSTR(t.content, 1, 200), '')",
                author_sql: Some("t.source_provider"),
                timestamp_sql: "COALESCE(t.last_modified_time, t.created_at)",
                embed_where: None,
            }),
            extraction: Some(ExtractionConfig {
                text_sql: "COALESCE(t.title, '') || E'\n\n' || COALESCE(t.content, '')",
                filter_sql: None,
                // Documents run long, and names cluster at the top. The tail is
                // mostly body text that names nobody new.
                max_chars: 8000,
            }),
            temporal_type: TemporalType::Discrete,
            day_source: Some(DaySourceConfig {
                source_type: "document",
                source_type_sql: None,
                label_sql: "COALESCE(t.title, 'Untitled')",
                preview_sql: "SUBSTR(COALESCE(t.content_summary, t.content, ''), 1, 80)",
                id_sql: "t.id",
                extra_where: None,
                use_date_filter: false,
            }),
            continuous_agg: None,
            //                    who  whom what when where why  how
            is_activation_signal: false,
        },
        // `content_conversation` lived here — zero rows, ever. Its own comment said
        // it was for "messages created directly by chat API", which is what
        // `app_chat` does, and its description called it a "search artifact" too.
        // A duplicate ontology over an empty table that would have double-indexed
        // every conversation the day anything wrote to it.
        //
        // ===== The narrative layer =====
        // An event is a document — and the best-written one the system produces.
        // 657 of them, and search could not return a single one.
        //
        // What is indexed is the SUMMARY, which is a paraphrase, so no words are
        // duplicated from the records beneath it. The event is the coarse handle
        // ("when did I talk to Rachel"); the records underneath remain the evidence
        // ("what did she say"). Small-to-big, and both are wanted.
        //
        // Not a `data_*` table: it is derived, re-derivable, and re-segmented
        // nightly. Event ids are content-addressed from their boundaries, so a
        // re-cut day mints new ids and strands the old chunks — the indexer prunes
        // records that no longer exist (see `search::indexer`).
        OntologyDescriptor {
            name: "wiki_event",
            display_name: "Events",
            description: "Segmented narrative events — the day as it was lived",
            domain: "narrative",
            lane: None,
            table_name: "wiki_events",
            source_streams: vec![],
            timestamp_column: "started_at",
            end_timestamp_column: Some("ended_at"),
            embedding: Some(EmbeddingConfig {
                // The user's own label wins over the machine's: autonomy over
                // tidiness, everywhere.
                //
                // NULL for gap-fill and hidden events — an "Unknown" event with no
                // summary is a placeholder for time we could not account for, and
                // embedding the word "Unknown" 84 times teaches the index nothing.
                // A NULL embed text makes the indexer skip the row rather than
                // reconsider it forever.
                embed_text_sql: "CASE WHEN t.event_summary IS NULL OR t.is_unknown OR t.user_hidden \
                                 THEN NULL ELSE \
                                 COALESCE(t.user_label, t.auto_label, '') || E'\n' \
                                 || t.event_summary || E'\n' \
                                 || COALESCE(t.user_notes, '') END",
                content_type: "event",
                title_sql: Some("COALESCE(t.user_label, t.auto_label)"),
                preview_sql: "SUBSTR(COALESCE(t.event_summary, ''), 1, 200)",
                author_sql: None,
                timestamp_sql: "t.started_at",
                embed_where: None,
            }),
            // Entities are already resolved onto events by the day pipeline
            // (`dayline::annotate`). Re-reading the summary for names would build a
            // second, worse path to the same people.
            extraction: None,
            temporal_type: TemporalType::Discrete,
            // It IS the day's narrative; it is not a source for it.
            day_source: None,
            continuous_agg: None,
            //                    who  whom what when where why  how
            is_activation_signal: false,
        },
        // ===== Financial Ontologies =====
        OntologyDescriptor {
            name: "financial_account",
            display_name: "Financial Accounts",
            description: "Bank accounts, credit cards, and other financial accounts from Plaid",
            domain: "financial",
            lane: Some("financial"),
            table_name: "data_financial_account",
            source_streams: vec!["stream_plaid_accounts", "stream_ios_financekit"],
            timestamp_column: "created_at",
            end_timestamp_column: None,
            embedding: None,
            extraction: None,
            temporal_type: TemporalType::Discrete,
            day_source: None, // Not events
            continuous_agg: None,
            //                    who  whom what when where why  how
            is_activation_signal: false,
        },
        OntologyDescriptor {
            name: "financial_transaction",
            display_name: "Financial Transactions",
            description: "Bank and credit card transactions from Plaid with merchant and category info",
            domain: "financial",
            lane: Some("financial"),
            table_name: "data_financial_transaction",
            source_streams: vec!["stream_plaid_transactions", "stream_ios_financekit"],
            timestamp_column: "occurred_at",
            end_timestamp_column: None,
            embedding: Some(EmbeddingConfig {
                // `t.category` is JSONB now (a string array). Cast to text and
                // strip the JSON quoting so embeddings see comma-separated
                // category labels rather than literal `["foo","bar"]`.
                embed_text_sql: "COALESCE(t.merchant_name, t.description, '') || ' ' || COALESCE((SELECT string_agg(value::text, ', ') FROM jsonb_array_elements_text(t.category) AS value), '')",
                content_type: "transaction",
                title_sql: Some("COALESCE(t.merchant_name, t.description)"),
                preview_sql: "COALESCE(t.merchant_name, t.description) || ' - $' || CAST(ABS(t.amount / 100.0) AS TEXT) || ' on ' || t.occurred_at::text",
                author_sql: None,
                timestamp_sql: "t.occurred_at",
                embed_where: None,
            }),
            extraction: None,
            temporal_type: TemporalType::Discrete,
            day_source: Some(DaySourceConfig {
                source_type: "transaction",
                source_type_sql: None,
                label_sql: "COALESCE(t.merchant_name, t.description, '(no description)')",
                preview_sql: "'$' || CAST(ABS(t.amount / 100.0) AS TEXT)",
                id_sql: "t.id",
                extra_where: None,
                use_date_filter: false,
            }),
            continuous_agg: None,
            //                    who  whom what when where why  how
            is_activation_signal: false,
        },
        OntologyDescriptor {
            name: "content_bookmark",
            display_name: "Bookmarks",
            description: "Saved/curated items: GitHub stars, browser bookmarks, saved links",
            domain: "content",
            lane: None,
            table_name: "data_content_bookmark",
            source_streams: vec!["stream_github_events"],
            timestamp_column: "occurred_at",
            end_timestamp_column: None,
            // THE USER'S OWN WORDS ARE THE BEST TEXT ON THE ROW.
            //
            // `note` is user-authored marginalia (migration 0073) and `tags` are
            // containers harvested at the source — browser folder paths, GitHub
            // topics. Both were absent here, which meant the one thing a person
            // deliberately wrote about a save was invisible to search, and a
            // bookmark filed under "Kitchen reno" could not be found by that
            // phrase. Intent language is also the closest match to future query
            // language, so it earns its place ahead of most machine text.
            //
            // `extraction_text` (0095) is the machine half: the enrichment
            // sweep's record rendered as prose, which is what makes a saved
            // Instagram post — no title, no description — findable at all. It
            // is written by `ExtractionRecord::to_embed_text` rather than
            // assembled from JSONB here, so the rendering has one definition.
            // Rows not yet enriched contribute an empty string and lose
            // nothing.
            //
            // The `jsonb_typeof` guard is load-bearing: `jsonb_array_elements_text`
            // raises on a non-array, and this expression is interpolated into the
            // indexer's scan for EVERY bookmark row — so one malformed `tags`
            // value from any producer would abort indexing for the whole
            // ontology, not just skip its own row.
            embedding: Some(EmbeddingConfig {
                embed_text_sql: "COALESCE(t.title, '') || '\n\n' || COALESCE(t.description, '') \
                     || '\n\n' || COALESCE(t.note, '') || '\n\n' || \
                     CASE WHEN jsonb_typeof(t.tags) = 'array' \
                          THEN COALESCE((SELECT string_agg(tag, ', ') \
                                           FROM jsonb_array_elements_text(t.tags) AS tag), '') \
                          ELSE '' END \
                     || '\n\n' || COALESCE(t.extraction_text, '')",
                content_type: "bookmark",
                title_sql: Some("t.title"),
                preview_sql: "COALESCE(SUBSTR(t.description, 1, 200), t.url)",
                author_sql: Some("t.author"),
                timestamp_sql: "t.occurred_at",
                // A saved URL with no title, no description, no note and no
                // extraction record yet has NOTHING to embed — the expression
                // above evaluates to a few newlines. Indexing that is not
                // merely wasteful: an empty document is a vector at an
                // arbitrary point in the space that can match any query, and it
                // skews the BM25 corpus statistics every other query is scored
                // against (the same reason silent transcriptions are excluded).
                //
                // The row stays and stays visible; it simply is not something a
                // retriever should match. Enrichment gives it text, and the
                // indexer picks it up on the next pass with no intervention.
                //
                // Tags are deliberately not part of this test: a bookmark whose
                // only text is a folder name is a bare link, and the folder is
                // already searchable as structure.
                embed_where: Some(
                    "AND btrim(COALESCE(t.title, '') || COALESCE(t.description, '') \
                     || COALESCE(t.note, '') || COALESCE(t.extraction_text, '')) <> ''",
                ),
            }),
            extraction: None,
            temporal_type: TemporalType::Discrete,
            day_source: Some(DaySourceConfig {
                source_type: "bookmark",
                source_type_sql: None,
                label_sql: "COALESCE(t.title, t.url, 'Bookmark')",
                preview_sql: "SUBSTR(COALESCE(t.description, t.url, ''), 1, 80)",
                id_sql: "t.id",
                extra_where: None,
                use_date_filter: false,
            }),
            continuous_agg: None,
            //                    who  whom what when where why  how
            is_activation_signal: false,
        },
        // ─────────────────────────────────────────────────────────────
        // App (intra-Virtues activity)
        // ─────────────────────────────────────────────────────────────
        OntologyDescriptor {
            name: "app_chat",
            display_name: "Chat Sessions",
            description: "Conversations with Virtues AI assistant",
            domain: "app",
            lane: None,
            table_name: "app_chats",
            source_streams: vec![],
            timestamp_column: "created_at",
            end_timestamp_column: Some("updated_at"),
            // THE CONVERSATION IS THE DOCUMENT. Not the turn.
            //
            // We used to index each message as its own document, and it is why
            // search was poor: a turn is a fragment, and a fragment cannot be
            // judged. Asked whether the message "theme" was about buying a house,
            // the cross-encoder had nothing to go on and hedged at +0.91 — 0.08
            // beneath an actual street address. Indexed as its conversation, the
            // same words become a two-message exchange about bold text formatting,
            // and the answer is obviously no.
            //
            // `string_agg` is safe here: `embed_text_sql` is interpolated raw
            // (indexer.rs), so a subquery needs no indexer change. The chunker
            // splits long conversations into 96-word windows on its own.
            //
            // Checkpoints and onboarding triggers are skipped — they are machinery,
            // not talk.
            embedding: Some(EmbeddingConfig {
                embed_text_sql: "COALESCE(t.title, '') || E'\n\n' || COALESCE((\
                     SELECT string_agg(m.role || ': ' || m.content, E'\n\n' ORDER BY m.sequence_num) \
                     FROM app_chat_messages m \
                     WHERE m.chat_id = t.id \
                       AND m.role <> 'checkpoint' \
                       AND COALESCE(m.subject, '') <> 'onboarding_synthetic' \
                       AND m.content IS NOT NULL), '')",
                content_type: "chat",
                title_sql: Some("t.title"),
                preview_sql: "COALESCE(SUBSTR(t.conversation_summary, 1, 200), SUBSTR(t.title, 1, 200), '')",
                author_sql: None,
                // The document changes as the conversation grows, so its time is
                // when it last did.
                timestamp_sql: "t.updated_at",
                embed_where: None,
            }),
            // Entity extraction on the same unit as retrieval. Chats carried NO
            // entity refs before, so a conversation about Rachel was unreachable
            // from Rachel — and if ER and IR disagreed about what a chat *is*, the
            // entity filter (query.rs joins on source_table + record_id) would
            // silently return nothing.
            //
            // User turns only: the assistant's replies are the model paraphrasing
            // the user back, and extracting names from them would count the same
            // mention twice.
            extraction: Some(ExtractionConfig {
                text_sql: "COALESCE((SELECT string_agg(m.content, E'\n' ORDER BY m.sequence_num) \
                           FROM app_chat_messages m \
                           WHERE m.chat_id = t.id AND m.role = 'user' AND m.content IS NOT NULL), '')",
                filter_sql: None,
                max_chars: 8000,
            }),
            temporal_type: TemporalType::Discrete,
            day_source: Some(DaySourceConfig {
                source_type: "chat",
                source_type_sql: None,
                label_sql: "COALESCE(t.title, 'Chat')",
                preview_sql: "CAST(t.message_count AS TEXT) || ' messages'",
                id_sql: "t.id",
                extra_where: None,
                use_date_filter: true,
            }),
            continuous_agg: None,
            //                    who  whom what when where why  how
            is_activation_signal: false,
        },
        // `app_chat_message` lived here, and its own description called it a
        // "search artifact" — an ontology that existed for no reason but to be
        // indexed. It indexed each TURN as a standalone document, which is what
        // made 98 of the corpus's 110 chunks chat fragments, and what put the
        // one-word message "theme" within 0.08 of a street address when the
        // reranker was asked which of them concerned a house.
        //
        // The conversation is the document. See `app_chat` above. The messages
        // remain, of course — they are facts, and facts are kept at source
        // granularity. Nothing semantic reads them one row at a time any more.
        OntologyDescriptor {
            name: "app_page",
            display_name: "Page Edits",
            description: "Wiki page creations and modifications",
            domain: "app",
            lane: None,
            table_name: "app_pages",
            source_streams: vec![],
            timestamp_column: "updated_at",
            end_timestamp_column: None,
            // Your own writing — and it was not searchable. A page is already a
            // document; there is nothing to assemble.
            //
            // Pages are LIVE (Yjs), so this is the ontology that most needs the
            // re-embed-on-change guard: without it an edit never reaches the index
            // and search answers with the version you wrote first.
            embedding: Some(EmbeddingConfig {
                embed_text_sql: "COALESCE(t.title, '') || E'\n\n' || COALESCE(t.content, '')",
                content_type: "page",
                title_sql: Some("t.title"),
                preview_sql: "SUBSTR(COALESCE(t.content, ''), 1, 200)",
                author_sql: None,
                timestamp_sql: "t.updated_at",
                // `app_pages` now holds two kinds of document (migration 0081).
                // This descriptor owns only the ones a person wrote.
                embed_where: Some("AND t.kind = 'page'"),
            }),
            extraction: None,
            temporal_type: TemporalType::Discrete,
            day_source: Some(DaySourceConfig {
                source_type: "page",
                source_type_sql: None,
                label_sql: "COALESCE(t.icon || ' ', '') || COALESCE(t.title, 'Untitled')",
                preview_sql: "NULL",
                id_sql: "t.id",
                extra_where: Some("AND t.kind = 'page'"),
                use_date_filter: true,
            }),
            continuous_agg: None,
            //                    who  whom what when where why  how
            is_activation_signal: false,
        },
        // ===== Wiki articles =====
        //
        // The same table as `app_page`, deliberately, and split from it for one
        // reason that is not cosmetic: `day_source`.
        //
        // A day source asks "what did you do that day", and `app_page` answers
        // it with `use_date_filter: true` over `updated_at` — every page edit
        // shows in the day as authorship. Articles are written by the record
        // about itself, on a schedule, and every CRDT flush bumps `updated_at`.
        // Left in the same ontology, a nightly maintenance pass over a few
        // dozen articles would fill the day view with "you wrote a page today"
        // for prose the user never touched. That is the same provenance failure
        // that once read calendar RSVPs as attendance, and it lands at applet
        // volume rather than one row at a time.
        //
        // So: `day_source: None`. An article is not something you did.
        //
        // It stays SEARCHABLE, because prose about your life is exactly what
        // you want to find — but under its own `content_type`, so a citation
        // chip can say what it is rather than labelling machine prose as your
        // own writing.
        OntologyDescriptor {
            name: "wiki_article",
            display_name: "Wiki Articles",
            description: "The record's own prose about a subject — people, places, days",
            domain: "wiki",
            lane: None,
            table_name: "app_pages",
            source_streams: vec![],
            timestamp_column: "updated_at",
            end_timestamp_column: None,
            embedding: Some(EmbeddingConfig {
                embed_text_sql: "COALESCE(t.title, '') || E'\n\n' || COALESCE(t.content, '')",
                content_type: "article",
                title_sql: Some("t.title"),
                preview_sql: "SUBSTR(COALESCE(t.content, ''), 1, 200)",
                author_sql: None,
                timestamp_sql: "t.updated_at",
                embed_where: Some("AND t.kind = 'article'"),
            }),
            extraction: None,
            temporal_type: TemporalType::Discrete,
            // Not a day source. See above — this is the whole reason for the split.
            day_source: None,
            continuous_agg: None,
            //                    who  whom what when where why  how
            is_activation_signal: false,
        },
        // ===== Documents (researcher-plan D1) =====
        // One row per retrieval chunk of an extracted drive file. Universal
        // extraction: every text-bearing upload lands here via the
        // document_extraction cron; the generic indexer embeds rows for free.
        OntologyDescriptor {
            name: "uploaded_document",
            display_name: "Documents",
            description: "Extracted text chunks from files in Drive (PDF, DOCX, text, HTML)",
            domain: "documents",
            lane: None,
            table_name: "extracted_document_chunks",
            source_streams: vec![],
            // Event time (the file's arrival in the record), not parse time —
            // `created_at` here is when the extractor ran, and using it made
            // every date-scoped document search chronologically wrong.
            timestamp_column: "occurred_at",
            end_timestamp_column: None,
            embedding: Some(EmbeddingConfig {
                embed_text_sql: "t.text",
                content_type: "document",
                // Title = the owning file's name (+ page when known), so search
                // results and model citations read "paper.pdf · p. 6".
                title_sql: Some(
                    "(SELECT f.filename FROM app_drive_files f WHERE f.id = t.file_id) || \
                     COALESCE(' · p. ' || t.page_num, '')",
                ),
                preview_sql: "SUBSTR(t.text, 1, 200)",
                author_sql: None,
                timestamp_sql: "t.occurred_at",
                embed_where: None,
            }),
            extraction: None,
            temporal_type: TemporalType::Discrete,
            // Chunks are not day-page material — they surface via search, the
            // Library, and citations, not the timeline.
            day_source: None,
            continuous_agg: None,
            //                    who  whom what when where why  how
            is_activation_signal: false,
        },
        // Highlights + margin notes on documents (researcher-plan D2). Your
        // own marks are dense signal — embedding quote + note makes "what did
        // I highlight about X" work, and annotations out-rank raw text.
        OntologyDescriptor {
            name: "document_marginalia",
            display_name: "Highlights",
            description: "Highlights and margin notes on documents in Drive",
            domain: "documents",
            lane: None,
            table_name: "app_marginalia",
            source_streams: vec![],
            timestamp_column: "created_at",
            end_timestamp_column: None,
            embedding: Some(EmbeddingConfig {
                embed_text_sql: "t.quote_text || CASE WHEN t.note_md <> '' \
                                 THEN E'\\n\\n' || t.note_md ELSE '' END",
                content_type: "annotation",
                title_sql: Some(
                    "(SELECT f.filename FROM app_drive_files f WHERE f.id = t.file_id) || \
                     COALESCE(' · p. ' || t.page_num, '')",
                ),
                preview_sql: "SUBSTR(t.quote_text, 1, 200)",
                author_sql: None,
                timestamp_sql: "t.created_at",
                embed_where: None,
            }),
            extraction: None,
            temporal_type: TemporalType::Discrete,
            day_source: None,
            continuous_agg: None,
            //                    who  whom what when where why  how
            is_activation_signal: false,
        },
    ]
}

/// Get ontology by name
pub fn get_ontology(name: &str) -> Option<OntologyDescriptor> {
    registered_ontologies().into_iter().find(|o| o.name == name)
}

/// Get ontologies by domain
pub fn get_ontologies_by_domain(domain: &str) -> Vec<OntologyDescriptor> {
    registered_ontologies()
        .into_iter()
        .filter(|o| o.domain == domain)
        .collect()
}

/// Get ontologies that have semantic search enabled
/// The ontologies that carry prose worth reading for names.
///
/// The extractor walks THIS, not a hardcoded list of tables. A new source
/// (Slack, Fastmail, an mbox import) normalizes into an existing ontology and
/// inherits extraction for free — there is nothing to add.
pub fn get_extractable_ontologies() -> Vec<OntologyDescriptor> {
    registered_ontologies()
        .into_iter()
        .filter(|o| o.extraction.is_some())
        .collect()
}

/// The `source_type`s that mean YOU DID SOMETHING — as opposed to a sensor
/// noticing that you exist.
///
/// This is `is_activation_signal`, which has been declared on all 22 ontologies
/// since the beginning, describes exactly this in its own doc comment, and was
/// read by nobody. A dormant field with no reader, like `avg_hr` and
/// `significance` before it.
///
/// It matters because the day summary asks an LLM to narrate a day, and a day
/// where the only record is your heart beating is not a day that happened — it is
/// a day you wore a watch. Narrating it invents a life. On a real box, **449 of
/// 533 days** hold nothing but passive data, and each one was an Opus call away
/// from a confident account of a day nobody lived.
pub fn activation_source_types() -> Vec<&'static str> {
    registered_ontologies()
        .into_iter()
        .filter(|o| o.is_activation_signal)
        .filter_map(|o| o.day_source.map(|d| d.source_type))
        .collect()
}

/// The source types that have a beginning and an END — and so can give a day its
/// SHAPE.
///
/// A `wiki_event` is a span. You cannot cut a day into 8–16 of them out of things
/// that have no duration: a text message is a *moment*, and a thousand moments
/// still do not tell you when anything started or stopped. A day of nothing but
/// messages is not a day the machine can segment; asked to try, it invents the
/// boundaries, and the boundaries are the one thing it must not invent.
///
/// This is why the gate is about shape rather than volume. On a real box, 678 days
/// carry some activity and only **91** carry a single span — because the location
/// and audio collectors are days old and the message history goes back to 2017.
/// Narrating the other 587 would be an LLM writing the day it assumes you had.
pub fn span_source_types() -> Vec<&'static str> {
    registered_ontologies()
        .into_iter()
        .filter(|o| o.is_activation_signal && o.end_timestamp_column.is_some())
        .filter_map(|o| o.day_source.map(|d| d.source_type))
        .collect()
}

pub fn get_searchable_ontologies() -> Vec<OntologyDescriptor> {
    registered_ontologies()
        .into_iter()
        .filter(|o| o.embedding.is_some())
        .collect()
}

/// Get ontologies that are fed by a specific stream
pub fn get_ontologies_for_stream(stream_table: &str) -> Vec<OntologyDescriptor> {
    registered_ontologies()
        .into_iter()
        .filter(|o| o.source_streams.contains(&stream_table))
        .collect()
}

/// List all domain names
pub fn list_domains() -> Vec<&'static str> {
    vec![
        "health",
        "location",
        "calendar",
        "communication",
        "content",
        "financial",
        "activity",
        "app",
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registered_ontologies() {
        let ontologies = registered_ontologies();
        assert!(!ontologies.is_empty());

        // Check we have all expected domains
        let domains: std::collections::HashSet<_> = ontologies.iter().map(|o| o.domain).collect();
        assert!(domains.contains(&"health"));
        assert!(domains.contains(&"location"));
        assert!(domains.contains(&"communication"));
        assert!(domains.contains(&"calendar"));
        assert!(domains.contains(&"content"));
        assert!(domains.contains(&"financial"));
        assert!(domains.contains(&"app"));
    }

    #[test]
    fn test_get_ontology() {
        let sleep = get_ontology("health_sleep");
        assert!(sleep.is_some());
        let s = sleep.unwrap();
        assert_eq!(s.domain, "health");
        assert_eq!(s.timestamp_column, "started_at");
        assert_eq!(s.end_timestamp_column, Some("ended_at"));
    }

    #[test]
    fn extractable_ontologies_are_exactly_the_prose_ones() {
        let names: Vec<&str> = get_extractable_ontologies()
            .iter()
            .map(|o| o.table_name)
            .collect();

        // Prose. These four get their own LLM call.
        assert!(names.contains(&"data_communication_email"));
        assert!(names.contains(&"data_communication_message"));
        assert!(names.contains(&"data_content_document"));

        // Chat extraction runs on the CONVERSATION, not the turn — the same unit
        // retrieval indexes. If ER wrote refs on messages while IR indexed chats,
        // the entity filter (source_table + record_id) would silently return
        // nothing, and "everything I discussed with Rachel" would come back empty.
        // One unit for meaning.
        assert!(names.contains(&"app_chats"));
        assert!(
            !names.contains(&"app_chat_messages"),
            "extraction must run on the conversation, not the turn"
        );

        // Transcriptions carry prose but are NOT here: the transcription action
        // already extracts their entities in a call we're paying for anyway, so
        // they're drained for free. Adding them here would double-bill the same
        // names.
        assert!(!names.contains(&"data_communication_transcription"));

        // Everything else has no free text at all. If this number grows, someone
        // pointed an LLM at heart-rate samples.
        assert_eq!(names.len(), 4, "unexpected extractable ontologies: {names:?}");
    }

    /// The two filters that are load-bearing, not cosmetic.
    #[test]
    fn prose_filters_hold_the_line() {
        let by = |t: &str| {
            registered_ontologies()
                .into_iter()
                .find(|o| o.table_name == t)
                .and_then(|o| o.extraction)
                .expect("has extraction config")
        };

        // Email: Google's own junk classification, not ours. UPDATES stays in —
        // receipts and confirmations are dense with real orgs and future dates.
        let email = by("data_communication_email");
        let f = email.filter_sql.expect("email must filter junk");
        assert!(f.contains("CATEGORY_PROMOTIONS") && f.contains("SPAM"));
        assert!(
            !f.contains("CATEGORY_UPDATES"),
            "UPDATES must NOT be filtered — it is the best structured signal in the mailbox"
        );

        // AI chats: the user's turns only. The assistant's replies are the model
        // paraphrasing the user back; extracting names from them manufactures
        // evidence for things the user never said.
        //
        // The unit is the conversation, so this is now a WHERE inside the
        // aggregate rather than a row filter — but the line it holds is the same,
        // and it must stay held.
        let chat = by("app_chats");
        assert!(
            chat.text_sql.contains("m.role = 'user'"),
            "chat extraction must read the user's turns only, not the assistant's echo"
        );
    }

    #[test]
    fn test_searchable_ontologies() {
        let searchable = get_searchable_ontologies();
        assert!(searchable.len() >= 7);
        for o in &searchable {
            assert!(o.embedding.is_some());
        }
    }

    /// The indexed unit is a DOCUMENT — the thing a human would call "a thing".
    /// Never a piece of one, and never a measurement.
    ///
    /// This is the guard for the bug that made search bad: we indexed each chat
    /// TURN as its own document, so 98 of the corpus's 110 chunks were fragments.
    /// Asked whether the one-word message "theme" concerned buying a house, the
    /// cross-encoder had nothing to judge and hedged at +0.91 — a hair under a real
    /// street address at +0.99. A fragment cannot be judged, and no amount of
    /// reranking rescues a corpus made of them.
    #[test]
    /// `extra_where` is spliced RAW after the WHERE clause, so it must carry its
    /// own `AND`. Forget it, and Postgres says `syntax error at or near "t"` — at
    /// runtime, on the box, where the caller logs a warning and carries on.
    ///
    /// That is not hypothetical. `activity_app_session` shipped without the `AND`,
    /// so app sessions vanished from every day the moment they were added, and the
    /// nightly cron reported success throughout. A day assembled from a hole is a
    /// day the LLM then writes a confident account of.
    ///
    /// The template lives in `api::wiki::get_day_sources`. This test is the only
    /// thing standing between that convention and the next person who writes a
    /// filter that reads perfectly well in isolation.
    /// `is_activation_signal` sat on all 22 ontologies, read by nobody, while the
    /// day summary asked an LLM to narrate 449 days that contained nothing but a
    /// heartbeat. A field with no reader is not a design; it is a comment that
    /// compiles.
    ///
    /// This pins the two halves of the distinction it exists to make, so the gate
    /// cannot quietly rot back into decoration.
    #[test]
    fn activation_signals_separate_doing_from_merely_existing() {
        let acts = activation_source_types();

        // Things you DID. A day made of these is a day that happened.
        for t in ["location", "calendar", "audio", "message", "app_usage"] {
            assert!(acts.contains(&t), "{t} is something you did — it makes a day");
        }

        // Things a sensor noticed while you existed. A day made only of these is
        // not a day; it is a day you wore a watch, and narrating it invents a life.
        for t in ["heart_rate", "hrv", "steps"] {
            assert!(
                !acts.contains(&t),
                "{t} is a sensor noticing you exist — it cannot make a day"
            );
        }
    }

    /// A day is cut into EVENTS, and an event is a span. So a day needs at least
    /// one thing with a beginning and an end before it can be segmented at all.
    #[test]
    fn only_spans_can_give_a_day_its_shape() {
        let spans = span_source_types();

        // These have duration. They can bound an event.
        for t in ["location", "calendar", "audio", "app_usage"] {
            assert!(spans.contains(&t), "{t} has a start and an end — it shapes a day");
        }

        // A message is a MOMENT. A thousand of them still never say when anything
        // started or stopped, and a model asked to segment a day of pure moments
        // invents the boundaries — the one thing it must not invent.
        assert!(
            !spans.contains(&"message"),
            "a message is a moment, not a span — it happens INSIDE an event, it cannot define one"
        );
    }

    #[test]
    fn extra_where_carries_its_own_and() {
        for o in registered_ontologies() {
            let Some(ds) = &o.day_source else { continue };
            let Some(w) = ds.extra_where else { continue };
            let t = w.trim_start();
            assert!(
                t.starts_with("AND ") || t.starts_with("AND("),
                "{}: extra_where is spliced straight after the WHERE clause, so it must \
                 begin with AND. Got: {w:?}",
                o.name
            );
        }
    }

    /// Same rule as `extra_where_carries_its_own_and`, for the embedding path.
    ///
    /// Split from that test rather than folded into it because the two splice
    /// into different queries and a failure means different things: a bad
    /// `day_source.extra_where` drops rows from a day view, a bad
    /// `embedding.embed_where` corrupts the search index under the wrong
    /// ontology name. Naming them separately makes the failure legible.
    #[test]
    fn embed_where_carries_its_own_and() {
        for o in registered_ontologies() {
            let Some(e) = &o.embedding else { continue };
            let Some(w) = e.embed_where else { continue };
            let t = w.trim_start();
            assert!(
                t.starts_with("AND ") || t.starts_with("AND("),
                "{}: embed_where is spliced after the staleness predicate, so it must \
                 begin with AND. Got: {w:?}",
                o.name
            );
        }
    }

    /// Two ontologies over one table must partition it, or the indexer embeds
    /// the overlap twice — once per ontology name, since `embedding_id` is
    /// `{ontology}:{record_id}:{chunk}`. An unscoped descriptor claims every
    /// row, so it can only be the sole owner of its table.
    #[test]
    fn tables_shared_by_two_ontologies_are_scoped() {
        use std::collections::HashMap;
        let all = registered_ontologies();
        let mut by_table: HashMap<&str, Vec<&OntologyDescriptor>> = HashMap::new();
        for o in &all {
            if o.embedding.is_some() {
                by_table.entry(o.table_name).or_default().push(o);
            }
        }
        for (table, owners) in by_table {
            if owners.len() < 2 {
                continue;
            }
            for o in &owners {
                assert!(
                    o.embedding.as_ref().unwrap().embed_where.is_some(),
                    "{table} is indexed by {} ontologies ({}), so each needs an \
                     embed_where that carves out its share — {} claims the whole table",
                    owners.len(),
                    owners.iter().map(|x| x.name).collect::<Vec<_>>().join(", "),
                    o.name
                );
            }
        }
    }

    #[test]
    fn the_indexed_unit_is_a_document() {
        let indexed: Vec<&str> = get_searchable_ontologies()
            .iter()
            .map(|o| o.table_name)
            .collect();

        // SIGNALS are samples in a stream. They are lossy by design and mean
        // nothing alone: a heart-rate reading is not a document, it is the
        // integrand. A signal becomes an ANNOTATION on the event it overlaps —
        // never a row in the corpus.
        for t in [
            "data_health_heart_rate",
            "data_health_hrv",
            "data_health_steps",
            "data_location_point",
        ] {
            assert!(
                !indexed.contains(&t),
                "{t} is a signal — it must never enter the search corpus"
            );
        }

        // FRAGMENTS are real facts that cannot stand alone. They stay in the
        // database as facts; the document is their container.
        for (fragment, container) in [("app_chat_messages", "app_chats")] {
            assert!(
                !indexed.contains(&fragment),
                "{fragment} is a fragment — index {container} instead"
            );
            assert!(
                indexed.contains(&container),
                "{container} is the document {fragment} belongs to"
            );
        }

        // The documents we own and, for a long time, did not index: what you said,
        // what you wrote, and what the day was.
        for t in [
            "data_communication_transcription", // whole conversations, LLM-titled
            "app_pages",                        // your own writing
            "wiki_events",                      // 657 event summaries
        ] {
            assert!(indexed.contains(&t), "{t} is a document and must be indexed");
        }
    }

    #[test]
    fn test_get_ontologies_for_stream() {
        let healthkit_ontologies = get_ontologies_for_stream("stream_ios_healthkit");
        assert!(healthkit_ontologies.len() >= 5); // heart_rate, hrv, steps, sleep, workout
    }

    #[test]
    fn test_ontology_table_names() {
        for ontology in registered_ontologies() {
            assert!(
                !ontology.table_name.is_empty(),
                "Ontology {} should have a table_name",
                ontology.name
            );
        }
    }

}
