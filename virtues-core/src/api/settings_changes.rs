//! "Changed lately" on the Settings page: what the owner changed, the devices
//! they paired, and what needs them now, served at
//! `GET /api/settings/changes`, with `POST /api/settings/changes/:id/undo`.
//!
//! The settings endpoints write `app_settings_changes` after a save, one row
//! per setting whose stored value changed ([`record_diff`], [`record`]).
//! Pairings come from `app_auth_event` and what needs the owner from
//! `api::attention`; both are read where they live, not copied here.
//!
//! An undo writes the old value back through the same update the page uses,
//! so the undo is recorded as a change of its own, and marks the original
//! undone so it leaves the list.

use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{json, Value};
use sqlx::PgPool;

use crate::api::attention::AttentionItem;
use crate::database::Database;
use crate::error::{Error, Result};

/// The stored profile fields that are settings, and their setting ids in
/// apps/web/src/lib/settings.
pub const PROFILE_FIELDS: &[(&str, &str)] = &[
    ("full_name", "full-name"),
    ("preferred_name", "preferred-name"),
    ("birth_date", "birthday"),
    ("occupation", "occupation"),
    ("employer", "employer"),
    ("home_timezone", "home"),
    ("theme", "theme"),
];

/// The stored assistant-profile fields that are settings.
pub const ASSISTANT_FIELDS: &[(&str, &str)] = &[
    ("assistant_name", "assistant-name"),
    ("style_notes", "style"),
    ("lite_model_id", "models"),
    ("standard_model_id", "models"),
    ("deep_model_id", "models"),
    ("image_model_id", "models"),
    ("request_run_limit_micros", "limit-run"),
    ("request_day_limit_micros", "limit-day"),
];

/// Model slots store NULL for "the Virtues default", which an update can
/// write back; the other fields treat a missing value as "leave it", so a
/// change from nothing can't be undone to nothing.
const NULLABLE_FIELDS: &[&str] = &["lite_model_id", "standard_model_id", "deep_model_id", "image_model_id"];

/// How far back the list reaches, and how many changes it shows.
const LATELY_DAYS: i64 = 30;
const LATELY_LIMIT: i64 = 12;

/// One setting a save changed: (setting id, field, old, new).
#[derive(Debug, Clone, PartialEq)]
pub struct FieldChange {
    pub setting_id: &'static str,
    pub field: &'static str,
    pub old_value: Value,
    pub new_value: Value,
}

/// The fields in `fields` whose value differs between `before` and `after`,
/// two serialized copies of the same record. Pure, so it is tested here.
pub fn diff_fields(before: &Value, after: &Value, fields: &[(&'static str, &'static str)]) -> Vec<FieldChange> {
    fields
        .iter()
        .filter_map(|&(field, setting_id)| {
            let old_value = before.get(field).cloned().unwrap_or(Value::Null);
            let new_value = after.get(field).cloned().unwrap_or(Value::Null);
            (old_value != new_value).then_some(FieldChange { setting_id, field, old_value, new_value })
        })
        .collect()
}

/// Record one change. The save has already happened when this runs, so a
/// failed write is logged and never fails the caller's request.
pub async fn record(pool: &PgPool, setting_id: &str, field: &str, old_value: &Value, new_value: &Value) {
    if let Err(e) = sqlx::query(
        "INSERT INTO app_settings_changes (setting_id, field, old_value, new_value) VALUES ($1, $2, $3, $4)",
    )
    .bind(setting_id)
    .bind(field)
    .bind(old_value)
    .bind(new_value)
    .execute(pool)
    .await
    {
        tracing::warn!(setting_id, error = %e, "settings change not recorded");
    }
}

/// Record every tracked field a save changed, given the record before and
/// after it, serialized.
pub async fn record_diff(pool: &PgPool, before: &Value, after: &Value, fields: &[(&'static str, &'static str)]) {
    for c in diff_fields(before, after, fields) {
        record(pool, c.setting_id, c.field, &c.old_value, &c.new_value).await;
    }
}

#[derive(Debug, Serialize)]
pub struct ChangeItem {
    /// `setting:<row id>` or `paired:<event id>`.
    pub id: String,
    /// `setting` | `paired`.
    pub kind: &'static str,
    pub setting_id: Option<String>,
    pub old_value: Option<Value>,
    pub new_value: Option<Value>,
    /// The device's name, for a pairing.
    pub label: Option<String>,
    /// 'owner' or 'assistant', for a setting.
    pub actor: Option<String>,
    pub occurred_at: DateTime<Utc>,
    pub can_undo: bool,
}

#[derive(Debug, Serialize)]
pub struct Lately {
    /// What needs the owner now, most urgent first (`api::attention`).
    pub needs_you: Vec<AttentionItem>,
    /// Changes and pairings, newest first.
    pub changes: Vec<ChangeItem>,
}

/// Whether an undo can write `old_value` back to `field`.
fn undoable(field: &str, old_value: &Value) -> bool {
    let known = PROFILE_FIELDS.iter().chain(ASSISTANT_FIELDS).any(|&(f, _)| f == field);
    known && (!old_value.is_null() || NULLABLE_FIELDS.contains(&field))
}

pub async fn lately(db: &Database) -> Result<Lately> {
    let pool = db.pool();

    let rows: Vec<(i64, String, String, Option<Value>, Option<Value>, String, DateTime<Utc>)> = sqlx::query_as(
        "SELECT id, setting_id, field, old_value, new_value, actor, occurred_at \
         FROM app_settings_changes \
         WHERE undone_at IS NULL AND occurred_at > now() - make_interval(days => $1::int) \
         ORDER BY occurred_at DESC LIMIT $2",
    )
    .bind(LATELY_DAYS)
    .bind(LATELY_LIMIT)
    .fetch_all(pool)
    .await?;

    let pairings: Vec<(i64, Value, DateTime<Utc>)> = sqlx::query_as(
        "SELECT id, detail, occurred_at FROM app_auth_event \
         WHERE event_type IN ('paired', 'peer_enrolled') \
           AND occurred_at > now() - make_interval(days => $1::int) \
         ORDER BY occurred_at DESC LIMIT $2",
    )
    .bind(LATELY_DAYS)
    .bind(LATELY_LIMIT)
    .fetch_all(pool)
    .await?;

    let mut changes: Vec<ChangeItem> = rows
        .into_iter()
        .map(|(id, setting_id, field, old_value, new_value, actor, occurred_at)| {
            // absent-ok: a NULL old_value column means the setting had no value before.
            let can_undo = undoable(&field, old_value.as_ref().unwrap_or(&Value::Null));
            ChangeItem {
                id: format!("setting:{id}"),
                kind: "setting",
                setting_id: Some(setting_id),
                old_value,
                new_value,
                label: None,
                actor: Some(actor),
                occurred_at,
                can_undo,
            }
        })
        .chain(pairings.into_iter().map(|(id, detail, occurred_at)| ChangeItem {
            id: format!("paired:{id}"),
            kind: "paired",
            setting_id: None,
            old_value: None,
            new_value: None,
            label: detail.get("label").and_then(Value::as_str).map(str::to_owned),
            actor: None,
            occurred_at,
            can_undo: false,
        }))
        .collect();
    changes.sort_by(|a, b| b.occurred_at.cmp(&a.occurred_at));
    changes.truncate(LATELY_LIMIT as usize);

    // What needs the owner is read where it lives; a failure there must not
    // hide the changes.
    let needs_you = match crate::api::attention::attention(db).await {
        Ok(items) => items,
        Err(e) => {
            tracing::warn!("settings changes: attention unreadable: {e}");
            Vec::new()
        }
    };

    Ok(Lately { needs_you, changes })
}

/// Write a change's old value back and mark the change undone.
pub async fn undo(pool: &PgPool, id: i64) -> Result<()> {
    let row: Option<(String, Option<Value>, Option<DateTime<Utc>>)> =
        sqlx::query_as("SELECT field, old_value, undone_at FROM app_settings_changes WHERE id = $1")
            .bind(id)
            .fetch_optional(pool)
            .await?;
    let Some((field, old_value, undone_at)) = row else {
        return Err(Error::NotFound(format!("no settings change {id}")));
    };
    if undone_at.is_some() {
        return Err(Error::InvalidInput("that change was already undone".into()));
    }
    let old_value = old_value.unwrap_or(Value::Null);
    if !undoable(&field, &old_value) {
        return Err(Error::InvalidInput("that change can't be undone from here".into()));
    }

    let patch = json!({ field.as_str(): old_value });
    if PROFILE_FIELDS.iter().any(|&(f, _)| f == field) {
        let before = serde_json::to_value(crate::api::profile::get_profile(pool).await?)?;
        let after = crate::api::profile::update_profile(pool, serde_json::from_value(patch)?).await?;
        record_diff(pool, &before, &serde_json::to_value(after)?, PROFILE_FIELDS).await;
    } else {
        let before = serde_json::to_value(crate::api::assistant_profile::get_assistant_profile(pool).await?)?;
        let after = crate::api::assistant_profile::update_assistant_profile(pool, serde_json::from_value(patch)?).await?;
        record_diff(pool, &before, &serde_json::to_value(after)?, ASSISTANT_FIELDS).await;
    }

    sqlx::query("UPDATE app_settings_changes SET undone_at = now() WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_lists_only_tracked_fields_that_changed() {
        let before = json!({ "theme": "oxford", "full_name": "Nick", "sidebar_doors": null });
        let after = json!({ "theme": "pemberley", "full_name": "Nick", "sidebar_doors": { "order": [] } });
        let changes = diff_fields(&before, &after, PROFILE_FIELDS);
        assert_eq!(
            changes,
            vec![FieldChange {
                setting_id: "theme",
                field: "theme",
                old_value: json!("oxford"),
                new_value: json!("pemberley"),
            }]
        );
    }

    #[test]
    fn diff_treats_a_missing_field_as_null() {
        let changes = diff_fields(&json!({}), &json!({ "style_notes": "Short answers." }), ASSISTANT_FIELDS);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].setting_id, "style");
        assert_eq!(changes[0].old_value, Value::Null);
    }

    #[test]
    fn several_model_slots_share_one_setting() {
        let before = json!({ "lite_model_id": null, "standard_model_id": "a" });
        let after = json!({ "lite_model_id": "b", "standard_model_id": "c" });
        let ids: Vec<_> = diff_fields(&before, &after, ASSISTANT_FIELDS).iter().map(|c| c.setting_id).collect();
        assert_eq!(ids, vec!["models", "models"]);
    }

    #[test]
    fn undo_needs_a_value_to_write_back_except_for_model_slots() {
        assert!(undoable("theme", &json!("oxford")));
        assert!(!undoable("theme", &Value::Null));
        assert!(undoable("standard_model_id", &Value::Null));
        assert!(!undoable("sidebar_doors", &json!({})));
    }
}
