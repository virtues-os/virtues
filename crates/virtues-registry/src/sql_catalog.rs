//! The SQL catalog: which tables the model may query, and what it is told
//! about each of them BEFORE it writes a line of SQL.
//!
//! This lived in `virtues-core::tools::sql_query`, where it served
//! `list_tables`, `get_schema` and the error explainer — every one of which
//! runs AFTER the model has already decided what to write. The paragraph the
//! model reads first, the `sql_query` tool description in `tools.rs`, was a
//! separate hand-written list of table NAMES with a prose gloss each, and no
//! columns at all. The columns were one `get_schema` call away, and the
//! description said to make that call "when uncertain".
//!
//! Measured on a live box over fourteen days (2026-09-20): 418 `sql_query`
//! calls, 4 of them `get_schema`, none `list_tables`. The model is never
//! uncertain. Of the 11 failures, 9 were an invented column — `day` for a
//! view whose column is `date`, `attendees` for `attendee_identifiers`,
//! `title` for `page_title`, `notes`, `is_from_me` — and the other two were
//! a bare `occurred_at`/`date` in a join. Every one recovered on the next
//! call, because the error hands back the real column list; every one also
//! cost a round trip of the whole context and a red block in the chat.
//!
//! The text-to-SQL literature settled this years ago: the schema goes in the
//! prompt, and the compact `table(col, col, …)` form scores within a point of
//! full DDL at a third of the tokens. So the description is now GENERATED
//! from this catalog by [`prompt_block`], the way `get_schema` always was —
//! one source, and the drift that put `entity_references` (a table that does
//! not exist) in the hand-written list cannot recur for columns.
//!
//! It lives in the registry rather than core because the tool description is
//! built here, and the registry may not depend on core.

use serde::Serialize;
use std::collections::HashMap;

/// Table metadata for `get_schema`, `list_tables` and the prompt block.
#[derive(Debug, Clone, Serialize)]
pub struct TableMetadata {
    /// One line on what the table holds. Also the visibility fence: a table
    /// with no entry here is not advertised to the model at all.
    pub description: &'static str,
    /// Groups the prompt block. Every value used here must appear in
    /// [`CATEGORY_ORDER`], or the table silently vanishes from the prompt —
    /// a test enforces it.
    pub category: &'static str,
    /// The columns worth querying. Not exhaustive (every table also has
    /// `id`, `created_at`, `updated_at`, and data tables their `source_*`
    /// and `metadata`) — `get_schema` gives the full set with types.
    pub key_columns: &'static [&'static str],
    /// How to reach the row's related entities. Returned by `get_schema`.
    pub join_hint: Option<&'static str>,
    /// One clause the model needs BEFORE it writes the query and that the
    /// column list cannot say: a sign convention, a jsonb shape, a view that
    /// looks like a table. Rendered in the prompt block after the columns.
    /// Kept to a clause on purpose — the whole block sits in every turn.
    pub note: Option<&'static str>,
}

/// The order the prompt block renders categories in, with their headings.
///
/// A category present in the catalog and absent here would drop its tables
/// from the prompt without any error, which is the same failure class as the
/// undescribed-table fence below (`get_table_metadata` docs) — so a test
/// checks the two against each other.
pub const CATEGORY_ORDER: &[(&str, &str)] = &[
    ("health", "HEALTH"),
    ("location", "LOCATION"),
    ("communication", "COMMUNICATION"),
    ("calendar", "CALENDAR"),
    ("financial", "FINANCIAL (amounts in cents — divide by 100 for dollars)"),
    ("activity", "ACTIVITY"),
    ("content", "CONTENT"),
    ("environment", "ENVIRONMENT"),
    ("wiki_entity", "WIKI ENTITIES (resolved nouns — one row per person, place, org)"),
    ("wiki_temporal", "WIKI TEMPORAL"),
    ("wiki_reference", "WIKI REFERENCES"),
    ("wiki", "WIKI NARRATIVE (the owner's own account of their life)"),
];

/// The block the model reads: every queryable table, its columns, and the
/// one-clause trap where there is one.
///
/// ```text
/// HEALTH
///   data_health_heart_rate(bpm, occurred_at)
///   data_health_sleep(started_at, ended_at, duration_minutes, …)
/// ```
///
/// One line per table, so a model scanning for a name finds its columns on
/// the same line. Descriptions are NOT rendered — they are for `list_tables`,
/// and forty sentences would double the block for prose the model does not
/// need once it can see the columns. Only `note` makes it in.
pub fn prompt_block() -> String {
    let catalog = get_table_metadata();
    let mut out = String::new();
    for (category, heading) in CATEGORY_ORDER {
        let mut tables: Vec<(&&str, &TableMetadata)> = catalog
            .iter()
            .filter(|(_, m)| m.category == *category)
            .collect();
        if tables.is_empty() {
            continue;
        }
        tables.sort_by_key(|(name, _)| **name);
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(heading);
        out.push('\n');
        for (name, meta) in tables {
            out.push_str("  ");
            out.push_str(name);
            out.push('(');
            out.push_str(&meta.key_columns.join(", "));
            out.push(')');
            if let Some(note) = meta.note {
                out.push_str(" — ");
                out.push_str(note);
            }
            out.push('\n');
        }
    }
    out
}

/// Static table metadata — descriptions, key queryable columns, and the fence
/// that decides which tables the model can see at all.
pub fn get_table_metadata() -> HashMap<&'static str, TableMetadata> {
    let mut m = HashMap::new();

    // ============================================================================
    // DATA TABLES - Health
    // ============================================================================
    m.insert("data_health_heart_rate", TableMetadata {
        description: "Heart rate BPM measurements from wearables",
        category: "health",
        key_columns: &["bpm", "occurred_at"],
        join_hint: None,
        note: None,
    });
    m.insert("data_health_hrv", TableMetadata {
        description: "Heart rate variability measurements in milliseconds",
        category: "health",
        key_columns: &["hrv_ms", "occurred_at"],
        join_hint: None,
        note: None,
    });
    m.insert("data_health_steps", TableMetadata {
        description: "Step count records (may have multiple per day)",
        category: "health",
        key_columns: &["step_count", "occurred_at"],
        join_hint: None,
        note: None,
    });
    m.insert("data_health_sleep", TableMetadata {
        description: "Sleep sessions with duration and quality metrics",
        category: "health",
        key_columns: &["started_at", "ended_at", "duration_minutes", "sleep_quality_score", "sleep_stages"],
        join_hint: None,
        note: None,
    });
    m.insert("data_health_workout", TableMetadata {
        description: "Exercise and workout sessions",
        category: "health",
        key_columns: &["workout_type", "started_at", "ended_at", "duration_minutes", "calories_burned", "distance_km", "avg_heart_rate", "max_heart_rate"],
        join_hint: Some("JOIN wiki_refs er ON er.source_table = 'data_health_workout' AND er.source_id = data_health_workout.id JOIN wiki_places ON er.entity_id = wiki_places.id AND er.entity_type = 'place'"),
        note: None,
    });
    m.insert("data_health_active_energy", TableMetadata {
        description: "Active energy burned, in kilocalories",
        category: "health",
        key_columns: &["kcal", "occurred_at"],
        join_hint: None,
        note: None,
    });
    m.insert("data_health_distance", TableMetadata {
        description: "Distance moved, in meters",
        category: "health",
        key_columns: &["meters", "occurred_at"],
        join_hint: None,
        note: None,
    });

    // ============================================================================
    // DATA TABLES - Location
    // ============================================================================
    m.insert("data_location_point", TableMetadata {
        description: "Raw GPS coordinates (high volume, use sparingly)",
        category: "location",
        key_columns: &["latitude", "longitude", "altitude", "horizontal_accuracy", "occurred_at"],
        join_hint: None,
        note: Some("high volume; prefer data_location_visit"),
    });
    m.insert("data_location_visit", TableMetadata {
        description: "Place visits with arrival/departure times",
        category: "location",
        key_columns: &["place_name", "latitude", "longitude", "started_at", "ended_at", "duration_minutes"],
        join_hint: Some("JOIN wiki_refs er ON er.source_table = 'data_location_visit' AND er.source_id = data_location_visit.id JOIN wiki_places ON er.entity_id = wiki_places.id AND er.entity_type = 'place'"),
        note: None,
    });

    // ============================================================================
    // DATA TABLES - Communication
    // ============================================================================
    m.insert("data_communication_email", TableMetadata {
        description: "Email messages from Gmail, etc.",
        category: "communication",
        key_columns: &["subject", "body", "body_preview", "from_email", "from_name", "to_emails", "direction", "is_read", "is_starred", "has_attachments", "labels", "thread_id", "occurred_at"],
        join_hint: Some("JOIN wiki_refs er ON er.source_table = 'data_communication_email' AND er.source_id = data_communication_email.id JOIN wiki_people ON er.entity_id = wiki_people.id AND er.entity_type = 'person'"),
        note: None,
    });
    m.insert("data_communication_message", TableMetadata {
        description: "Chat messages (iMessage, SMS, etc.)",
        category: "communication",
        key_columns: &["body", "channel", "from_identifier", "from_name", "to_identifiers", "is_read", "is_group_message", "has_attachments", "thread_id", "occurred_at"],
        // A message links to the person on the other end via wiki_refs:
        // role='sender' for messages you received, role='recipient' for messages you
        // sent. Filter both to get a full thread with someone; the message's own
        // direction is in metadata->>'is_from_me'.
        join_hint: Some("JOIN wiki_refs er ON er.source_table = 'data_communication_message' AND er.source_id = data_communication_message.id AND er.entity_type = 'person' AND er.role IN ('sender','recipient') JOIN wiki_people ON er.entity_id = wiki_people.id"),
        // `is_from_me` was a guessed column on a live box. It is a metadata
        // key, and the person on the other end is only reachable via wiki_refs.
        note: Some("the other person is via wiki_refs (role sender = they wrote it, recipient = you did); direction is metadata->>'is_from_me'"),
    });
    m.insert("data_communication_transcription", TableMetadata {
        description: "Voice/audio transcriptions",
        category: "communication",
        key_columns: &["text", "language", "duration_seconds", "started_at", "ended_at", "speaker_count"],
        join_hint: None,
        note: None,
    });
    // Tables that hold real data and were never described, so the agent could
    // see them only because the old catalog listed everything matching
    // `data_%`/`wiki_%`. Now that an undescribed table is a hidden table, the
    // omission would have silently taken the owner's own recordings, weather and
    // notes out of reach — so they are described here deliberately.
    m.insert("data_audio_recording", TableMetadata {
        description: "Microphone recordings captured by the phone — one row per chunk. Join to transcriptions on source_stream_id for the words.",
        category: "communication",
        key_columns: &["started_at", "ended_at", "duration_seconds", "is_silent", "average_db_level"],
        join_hint: Some("JOIN data_communication_transcription t ON t.source_stream_id = data_audio_recording.source_stream_id"),
        note: Some("chunks without words; the text is data_communication_transcription via source_stream_id"),
    });
    m.insert("data_audio_session", TableMetadata {
        description: "Conversations, derived by grouping adjacent transcription chunks into one sitting",
        category: "communication",
        key_columns: &["started_at", "ended_at", "speaker_mode", "chunk_count", "content"],
        join_hint: None,
        note: None,
    });

    // ============================================================================
    // DATA TABLES - Calendar
    // ============================================================================
    m.insert("data_calendar_event", TableMetadata {
        // "with attendees" is how a model comes to write `attendees` — the
        // description is the only prose it sees, and a word that is not a
        // column reads as one. Name the column instead: on a live box the
        // query `attendees::text ILIKE '%name%'` failed exactly this way.
        description: "Calendar events; attendees are in attendee_identifiers (an array of handles), location in location_name",
        category: "calendar",
        key_columns: &["title", "description", "calendar_name", "status", "response_status", "organizer_identifier", "attendee_identifiers", "location_name", "started_at", "ended_at", "is_all_day"],
        join_hint: Some("JOIN wiki_refs er ON er.source_table = 'data_calendar_event' AND er.source_id = data_calendar_event.id"),
        note: Some("attendee_identifiers = raw handles; the people are via wiki_refs role attendee"),
    });

    // ============================================================================
    // DATA TABLES - Financial (amounts in cents)
    // ============================================================================
    m.insert("data_financial_account", TableMetadata {
        description: "Bank, credit, and investment accounts",
        category: "financial",
        key_columns: &["account_name", "account_type", "institution_name", "mask", "currency", "current_balance", "available_balance"],
        join_hint: None,
        note: None,
    });
    m.insert("data_financial_transaction", TableMetadata {
        // positive=expense, negative=credit — Plaid's convention, which is what
        // the collector writes. This said the opposite, so a model looking for
        // spending wrote `WHERE amount < 0` and saw under one percent of the
        // record.
        description: "Transactions (amounts in cents, positive=money out, negative=refund or credit)",
        category: "financial",
        key_columns: &["account_id", "amount", "currency", "merchant_name", "merchant_category", "description", "category", "is_pending", "transaction_type", "payment_channel", "occurred_at"],
        join_hint: Some("JOIN data_financial_account ON account_id = data_financial_account.id"),
        note: Some("positive = money out, negative = refund/credit; category is a jsonb array — group by merchant_category"),
    });
    m.insert("data_financial_asset", TableMetadata {
        description: "Investment holdings (stocks, crypto, etc.)",
        category: "financial",
        key_columns: &["account_id", "asset_type", "symbol", "name", "quantity", "cost_basis", "current_value", "currency", "occurred_at"],
        join_hint: Some("JOIN data_financial_account ON account_id = data_financial_account.id"),
        note: None,
    });
    m.insert("data_financial_liability", TableMetadata {
        description: "Loans, mortgages, and debt",
        category: "financial",
        key_columns: &["account_id", "liability_type", "principal", "interest_rate", "minimum_payment", "next_payment_due_date", "currency", "occurred_at"],
        join_hint: Some("JOIN data_financial_account ON account_id = data_financial_account.id"),
        note: None,
    });

    // ============================================================================
    // DATA TABLES - Activity
    // ============================================================================
    m.insert("data_activity_app_session", TableMetadata {
        description: "Desktop/mobile app usage sessions",
        category: "activity",
        key_columns: &["app_name", "app_bundle_id", "started_at", "ended_at", "window_title"],
        join_hint: None,
        note: None,
    });
    m.insert("data_activity_web_browsing", TableMetadata {
        description: "Web browsing history",
        category: "activity",
        // `title` was a guessed column on a live box; it is `page_title`.
        key_columns: &["url", "domain", "page_title", "occurred_at"],
        join_hint: None,
        note: None,
    });

    // ============================================================================
    // DATA TABLES - Content
    // ============================================================================
    m.insert("data_content_document", TableMetadata {
        description: "Saved documents and notes",
        category: "content",
        key_columns: &["title", "content", "document_type", "tags", "is_authored", "occurred_at", "last_modified_time"],
        join_hint: None,
        note: None,
    });
    m.insert("data_content_conversation", TableMetadata {
        description: "Past AI chat conversation history",
        category: "content",
        key_columns: &["conversation_id", "message_id", "role", "content", "provider", "occurred_at"],
        join_hint: None,
        note: None,
    });
    m.insert("data_content_bookmark", TableMetadata {
        description: "Saved/starred content (GitHub stars, browser bookmarks, etc.)",
        category: "content",
        key_columns: &["url", "title", "description", "source_platform", "bookmark_type", "author", "tags", "occurred_at"],
        join_hint: None,
        note: None,
    });

    // ============================================================================
    // DATA TABLES - Environment
    // ============================================================================
    m.insert("data_environment_weather", TableMetadata {
        description: "Weather where the owner was. Holds BOTH observations and forecasts — filter is_forecast = false for what actually happened.",
        category: "environment",
        key_columns: &["occurred_at", "is_forecast", "temperature_c", "apparent_c", "latitude", "longitude"],
        join_hint: None,
        note: Some("observations AND forecasts; is_forecast = false for what happened"),
    });

    // ============================================================================
    // WIKI TABLES - Narrative
    // ============================================================================
    m.insert("wiki_articles", TableMetadata {
        description: "Links a wiki subject (person/place/organization/day) to the page holding its written article",
        category: "wiki",
        key_columns: &["subject_type", "subject_id", "page_id"],
        join_hint: Some("JOIN app_pages p ON p.id = wiki_articles.page_id"),
        note: Some("link row; the text is the page at page_id"),
    });
    m.insert("wiki_notes", TableMetadata {
        description: "Notes and open questions attached to a wiki subject, written by the owner or by the assistant",
        category: "wiki",
        key_columns: &["subject_type", "subject_id", "kind", "body", "author", "resolved_at"],
        join_hint: None,
        note: None,
    });
    m.insert("wiki_chapters", TableMetadata {
        description: "The chapters of the owner's life — their own gapless partition of it into named eras, authored in the narrative interview and never inferred. A day's chapter is a range lookup on started_at/ended_at; each chapter also has a wiki article (subject_type 'chapter')",
        category: "wiki",
        key_columns: &["title", "kind", "started_at", "ended_at", "is_current", "changepoint", "summary"],
        join_hint: None,
        note: Some("the owner's own eras, never inferred; a day's chapter is the row spanning it"),
    });
    m.insert("wiki_rules", TableMetadata {
        description: "Standing instructions the owner has given about how their record is written — 'avoid' subjects to leave alone, 'defend' ones to state carefully",
        category: "wiki",
        key_columns: &["rule", "kind", "active"],
        join_hint: None,
        note: None,
    });
    // Advertised because they are real subjects now. The fence keys on having
    // a description, so a table stays invisible to the agent until someone
    // writes one — which is why these two were dark while they were empty, and
    // why they belong here the moment they are not.
    m.insert("wiki_years", TableMetadata {
        description: "A year of the owner's life as a subject: their own title and summary for it",
        category: "wiki",
        key_columns: &["year", "title", "summary"],
        join_hint: Some("id is 'year_YYYY'; the days of a year are wiki_days filtered by EXTRACT(YEAR FROM date)"),
        note: Some("id is 'year_YYYY'"),
    });
    m.insert("wiki_stories", TableMetadata {
        description: "Subjects the owner named themselves — a theme or thread that mattered, not a span",
        category: "wiki",
        key_columns: &["title", "summary", "started_at", "ended_at"],
        join_hint: Some("dates are optional and often absent; a story is not a time range"),
        note: Some("dates optional, often absent"),
    });

    // ============================================================================
    // WIKI TABLES - Entities (resolved nouns)
    // ============================================================================
    m.insert("wiki_people", TableMetadata {
        description: "The people in the owner's life, resolved to one row each",
        category: "wiki_entity",
        key_columns: &["name", "emails", "phones", "relationship_category", "nickname", "bond", "birthday", "died_on"],
        join_hint: None,
        note: None,
    });
    m.insert("wiki_places", TableMetadata {
        description: "The places of the owner's life, resolved to one row each",
        category: "wiki_entity",
        key_columns: &["name", "category", "address", "latitude", "longitude", "radius_m"],
        join_hint: None,
        note: None,
    });
    m.insert("wiki_orgs", TableMetadata {
        description: "The organizations in the owner's life, resolved to one row each",
        category: "wiki_entity",
        key_columns: &["name", "organization_type", "relationship_type", "role_title", "started_at", "ended_at"],
        join_hint: None,
        note: None,
    });

    // ============================================================================
    // WIKI TABLES - Temporal
    // ============================================================================
    m.insert("wiki_days", TableMetadata {
        // Both relations carry `date`, and the join hint below sends the
        // model straight from one to the other — so a bare `SELECT date`
        // across that join is ambiguous, and a live box answered exactly
        // that. wiki_day_prose already has the date; the join is only
        // needed for wiki_days' own columns.
        description: "Day records; a day's prose lives in the wiki_day_prose view (day_id, date, prose), which already carries date — query it alone for prose, and qualify `date` if you join the two",
        category: "wiki_temporal",
        // `last_edited_by` was here until 0025 dropped it, and this catalog is
        // serialized straight to the model — so the agent was being handed a
        // column whose every mention it wrote came back as an error. `narrated_at`
        // is the live column that answers what the model actually wants to know
        // about a day.
        key_columns: &["date", "start_timezone", "narrated_at"],
        join_hint: Some("JOIN wiki_day_prose ON wiki_day_prose.day_id = wiki_days.id"),
        note: Some("prose is in wiki_day_prose, not here"),
    });
    // A VIEW, not a table — and cataloged on purpose. The wiki_days entry
    // above instructs a JOIN on it, and the fence in get_schema refuses
    // anything outside this catalog, so omitting it meant the model was told
    // to join a relation it was then refused a schema for. list_tables
    // includes views for the same reason.
    //
    // `day` was the single most-guessed column on a live box — three of nine
    // invented names in two weeks. The column is `date`.
    m.insert("wiki_day_prose", TableMetadata {
        description: "VIEW: each day's narrated prose (day_id, date, prose). The text of a day page.",
        category: "wiki_temporal",
        key_columns: &["day_id", "date", "prose"],
        join_hint: Some("JOIN wiki_days ON wiki_days.id = wiki_day_prose.day_id"),
        note: Some("a VIEW; the column is date, not day; if joined to wiki_days, qualify date"),
    });
    m.insert("wiki_events", TableMetadata {
        description: "Timeline events within a day",
        category: "wiki_temporal",
        key_columns: &["day_id", "started_at", "ended_at", "auto_label", "auto_location", "user_label", "user_location", "user_notes", "is_unknown", "is_transit"],
        join_hint: Some("JOIN wiki_days ON day_id = wiki_days.id"),
        note: None,
    });

    // ============================================================================
    // WIKI TABLES - References
    // ============================================================================
    m.insert("wiki_refs", TableMetadata {
        description: "Junction table linking entities (people, places, orgs) to ontology records. Use for 'everything about entity X' queries.",
        category: "wiki_reference",
        key_columns: &["entity_type", "entity_id", "source_table", "source_id", "role", "occurred_at"],
        join_hint: None,
        note: Some("the ONLY path from a data_* row to a person/place/org; role is one of sender, recipient, attendee, location, merchant"),
    });

    m
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A category the renderer does not know is a table the model never
    /// sees, with no error anywhere — the undescribed-table fence, one level
    /// up. Both directions: an entry in the order with no tables is dead
    /// text that will mislead the next reader.
    #[test]
    fn every_category_is_rendered_and_every_rendered_category_exists() {
        let catalog = get_table_metadata();
        let ordered: Vec<&str> = CATEGORY_ORDER.iter().map(|(c, _)| *c).collect();
        for (name, meta) in &catalog {
            assert!(
                ordered.contains(&meta.category),
                "{name} has category {:?}, which CATEGORY_ORDER does not render — \
                 the table would be missing from the prompt",
                meta.category
            );
        }
        for c in ordered {
            assert!(
                catalog.values().any(|m| m.category == c),
                "CATEGORY_ORDER lists {c:?} but no table has it"
            );
        }
    }

    /// The point of generating the block: every table, every key column,
    /// on the table's own line.
    #[test]
    fn prompt_block_names_every_table_with_its_columns() {
        let block = prompt_block();
        for (name, meta) in get_table_metadata() {
            let line = block
                .lines()
                .find(|l| l.trim_start().starts_with(&format!("{name}(")))
                .unwrap_or_else(|| panic!("{name} has no line in the prompt block"));
            for col in meta.key_columns {
                assert!(
                    line.contains(col),
                    "{name}'s line lacks its key column {col}: {line}"
                );
            }
            if let Some(note) = meta.note {
                assert!(line.contains(note), "{name}'s note is not on its line");
            }
        }
    }

    /// The block sits in every turn. The budget is the size of the prose
    /// list it replaced plus the columns — if it grows past this, something
    /// is being rendered that belongs in `get_schema`.
    #[test]
    fn prompt_block_stays_within_budget() {
        let block = prompt_block();
        assert!(
            block.len() < 6_000,
            "prompt block is {} bytes; descriptions or join hints leaking in?",
            block.len()
        );
    }
}
