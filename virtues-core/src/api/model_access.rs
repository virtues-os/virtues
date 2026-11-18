//! What the assistant may read (Settings · Your data): one switch per kind of
//! record. Off withholds that kind's tables from everything that reads the
//! record for a model: the SQL tool (the `virtues_face_reader` grants and its
//! catalog), search, the day pipeline, skills and applets. Your server keeps
//! collecting it; only the assistant stops seeing it.
//!
//! The switches live in `app_assistant_profile.reading`, keyed by kind id. A
//! missing key takes the kind's default: on, except cycle and symptoms, which
//! stay off until the owner turns them on (`ModelAccess::OwnerOptIn` in the
//! registry is that default).
//!
//! [`withheld_tables`] is the one answer every reader asks.

use std::collections::BTreeSet;

use serde::Serialize;
use serde_json::{json, Value};
use sqlx::PgPool;

use crate::error::{Error, Result};

/// One kind of record the owner can withhold.
pub struct Kind {
    pub id: &'static str,
    /// On unless the owner turns it off; false for kinds the owner must open.
    pub default_on: bool,
    pub tables: &'static [&'static str],
}

/// The kinds, in the order Settings lists them.
pub const KINDS: &[Kind] = &[
    Kind { id: "messages", default_on: true, tables: &["data_communication_message"] },
    Kind { id: "email", default_on: true, tables: &["data_communication_email"] },
    Kind {
        id: "recordings",
        default_on: true,
        tables: &["data_communication_transcription", "data_audio_session", "data_audio_recording"],
    },
    Kind { id: "calendar", default_on: true, tables: &["data_calendar_event"] },
    Kind {
        id: "health",
        default_on: true,
        tables: &["data_health_heart_rate", "data_health_hrv", "data_health_sleep", "data_health_steps", "data_health_workout"],
    },
    Kind {
        id: "cycle",
        default_on: false,
        tables: &["data_health_cycle_log", "data_health_symptom", "data_health_wrist_temperature"],
    },
    Kind { id: "location", default_on: true, tables: &["data_location_point", "data_location_visit"] },
    Kind {
        id: "money",
        default_on: true,
        tables: &["data_financial_account", "data_financial_transaction", "data_financial_asset", "data_financial_liability"],
    },
    Kind { id: "browsing", default_on: true, tables: &["data_activity_app_session", "data_activity_web_browsing"] },
    Kind {
        id: "documents",
        default_on: true,
        tables: &["data_content_bookmark", "data_content_conversation", "data_content_document"],
    },
];

/// One switch as Settings shows it.
#[derive(Debug, Serialize, PartialEq)]
pub struct KindState {
    pub id: &'static str,
    pub on: bool,
}

fn states(reading: &Value) -> Vec<KindState> {
    KINDS
        .iter()
        .map(|k| KindState { id: k.id, on: reading.get(k.id).and_then(Value::as_bool).unwrap_or(k.default_on) })
        .collect()
}

/// The tables withheld under these switches.
fn withheld_under(reading: &Value) -> BTreeSet<String> {
    states(reading)
        .into_iter()
        .filter(|s| !s.on)
        .flat_map(|s| KINDS.iter().find(|k| k.id == s.id).map(|k| k.tables).unwrap_or(&[]))
        .map(|t| t.to_string())
        .collect()
}

async fn reading(pool: &PgPool) -> Result<Value> {
    let (value,): (Value,) = sqlx::query_as("SELECT reading FROM app_assistant_profile LIMIT 1")
        .fetch_one(pool)
        .await
        .map_err(|e| Error::Database(format!("reading what the assistant may read: {e}")))?;
    Ok(value)
}

/// Every table the assistant may not read now. Readers fail closed: when this
/// errs, they must withhold rather than read.
pub async fn withheld_tables(pool: &PgPool) -> Result<BTreeSet<String>> {
    Ok(withheld_under(&reading(pool).await?))
}

/// For readers that cannot return an error: the withheld tables, or every
/// switchable table when the switches can't be read, so they fail closed.
pub async fn withheld_or_all(pool: &PgPool) -> BTreeSet<String> {
    match withheld_tables(pool).await {
        Ok(w) => w,
        Err(e) => {
            tracing::warn!(error = %e, "what the assistant may read is unreadable; withholding every kind");
            KINDS.iter().flat_map(|k| k.tables.iter().map(|t| t.to_string())).collect()
        }
    }
}

/// Whether one table is withheld now.
pub async fn is_withheld(pool: &PgPool, table: &str) -> Result<bool> {
    Ok(withheld_tables(pool).await?.contains(table))
}

pub async fn list(pool: &PgPool) -> Result<Vec<KindState>> {
    Ok(states(&reading(pool).await?))
}

/// Turns a kind on or off, records it for Changed lately, and re-applies the
/// database grants so the SQL tool's role follows at once.
pub async fn set(pool: &PgPool, id: &str, on: bool) -> Result<Vec<KindState>> {
    set_by(pool, id, on, "owner").await
}

/// [`set`], naming who made the change: 'owner', or 'assistant' for a switch
/// the owner applied from a Settings draft.
pub async fn set_by(pool: &PgPool, id: &str, on: bool, actor: &str) -> Result<Vec<KindState>> {
    if !KINDS.iter().any(|k| k.id == id) {
        return Err(Error::InvalidInput(format!("No kind of record is called {id}.")));
    }
    let mut tx = pool.begin().await?;
    let (before,): (Value,) = sqlx::query_as("SELECT reading FROM app_assistant_profile LIMIT 1 FOR UPDATE")
        .fetch_one(&mut *tx)
        .await?;
    let was = states(&before).into_iter().find(|s| s.id == id).map(|s| s.on);
    // absent-ok: the column is a jsonb object by default; anything else means
    // no switch was ever set, which is the defaults.
    let mut after = before.as_object().cloned().unwrap_or_default();
    after.insert(id.to_string(), Value::Bool(on));
    let after = Value::Object(after);
    sqlx::query("UPDATE app_assistant_profile SET reading = $1, updated_at = now()")
        .bind(&after)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    crate::server::faces::ensure_applet_db_grants(pool).await?;
    if was != Some(on) {
        crate::api::settings_changes::record_by(pool, actor, "model-reading", "reading", &json!({ id: was }), &json!({ id: on }))
            .await;
    }
    Ok(states(&after))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_withhold_only_cycle() {
        let withheld = withheld_under(&json!({}));
        assert!(withheld.contains("data_health_cycle_log"));
        assert!(!withheld.contains("data_communication_message"));
    }

    #[test]
    fn a_switch_overrides_its_default() {
        let withheld = withheld_under(&json!({ "messages": false, "cycle": true }));
        assert!(withheld.contains("data_communication_message"));
        assert!(!withheld.contains("data_health_cycle_log"));
    }

    #[test]
    fn the_registry_opt_in_tables_are_off_by_default_here() {
        let defaults = withheld_under(&json!({}));
        for table in virtues_registry::ontologies::owner_opt_in_tables() {
            assert!(defaults.contains(table), "{table} is opt-in in the registry but not withheld by default");
        }
    }

    #[test]
    fn every_kind_names_tables_the_record_has() {
        let known: BTreeSet<&str> = virtues_registry::registered_ontologies().iter().map(|o| o.table_name).collect();
        for kind in KINDS {
            for table in kind.tables {
                let catalog_only = matches!(*table, "data_audio_recording" | "data_financial_asset" | "data_financial_liability");
                assert!(known.contains(table) || catalog_only, "{} names {table}", kind.id);
            }
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn turning_a_kind_off_revokes_the_sql_role(pool: PgPool) {
        crate::server::faces::ensure_applet_db_grants(&pool).await.unwrap();
        let can_read = |pool: PgPool| async move {
            sqlx::query_scalar::<_, bool>("SELECT has_table_privilege('virtues_face_reader', 'data_communication_message', 'SELECT')")
                .fetch_one(&pool)
                .await
                .unwrap()
        };
        assert!(can_read(pool.clone()).await);
        set(&pool, "messages", false).await.unwrap();
        assert!(!can_read(pool.clone()).await);
        set(&pool, "messages", true).await.unwrap();
        assert!(can_read(pool.clone()).await);
    }
}
