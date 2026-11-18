//! "Changed lately" on the Settings page: what the owner changed and the
//! devices they paired in the last four weeks, served at
//! `GET /api/settings/changes`, with `POST /api/settings/changes/:id/undo`.
//!
//! The settings endpoints write `app_settings_changes` after a save, one row
//! per setting whose stored value changed ([`record_diff`], [`record`]).
//! Pairings are read from `app_auth_event` where they live, not copied here.
//!
//! An undo writes the old value back through the same update the page uses,
//! so the undo is recorded as a change of its own, and marks the original
//! undone so it leaves the list.
//!
//! Only the newest live change to a setting offers Undo: an older one would
//! write back over the newer one, which the undo refuses anyway.
//!
//! Drafting changes from what the owner types is in `settings_draft`; what
//! they apply from a draft is recorded here with the actor 'assistant'.

use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{json, Value};
use sqlx::PgPool;

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
    ("units", "units"),
    ("keep_recordings_days", "keep-recordings"),
];

/// The theme lives in the assistant profile's `ui_preferences` (what
/// `loadThemeFromDB` reads), so it is recorded under this path rather than as
/// a top-level field.
pub const THEME_FIELD: &str = "ui_preferences.theme";

/// Profile text fields: cleared with an empty string (`profile::update_profile`).
pub(crate) const PROFILE_TEXT_FIELDS: &[&str] = &["full_name", "preferred_name", "occupation", "employer", "home_timezone"];

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

/// What the assistant can do: rows hold only the tools one switch changed
/// (`api::capabilities::apply`), so this field is recorded there, not diffed
/// whole from a profile save.
const CAPABILITIES_FIELD: &str = "enabled_tools";

/// What the assistant may read: rows hold only the kind one switch changed
/// (`api::model_access::set`).
const READING_FIELD: &str = "reading";

/// Fields an undo may set back to nothing: model slots store NULL for "the
/// Virtues default", and the profile's text fields and birth date can be
/// cleared. The others treat a missing value as "leave it", so a change from
/// nothing can't be undone to nothing.
const NULLABLE_FIELDS: &[&str] = &[
    "lite_model_id",
    "standard_model_id",
    "deep_model_id",
    "image_model_id",
    "full_name",
    "preferred_name",
    "occupation",
    "employer",
    "home_timezone",
    "birth_date",
    "keep_recordings_days",
];

/// How far back Changed lately reaches: the four weeks its chart draws, one
/// dot per change. The limit only bounds a runaway table; four weeks of
/// changes made by hand stay far under it.
const LATELY_DAYS: i64 = 28;
const LATELY_LIMIT: i64 = 500;

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

/// One save of the two profiles at a time. Each save reads the record before
/// it, writes, and records the difference; holding this across all three
/// keeps a second save from landing in between and being recorded as part of
/// the first. Every profile save in this process takes it.
static SAVING: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

pub async fn saving() -> tokio::sync::MutexGuard<'static, ()> {
    SAVING.lock().await
}

/// Record one change the owner made. The save has already happened when
/// this runs, so a failed write is logged and never fails the caller's
/// request.
pub async fn record(pool: &PgPool, setting_id: &str, field: &str, old_value: &Value, new_value: &Value) {
    record_by(pool, "owner", setting_id, field, old_value, new_value).await;
}

/// [`record`], naming who made the change: 'owner', or 'assistant' for a
/// change the owner applied from a draft.
pub async fn record_by(pool: &PgPool, actor: &str, setting_id: &str, field: &str, old_value: &Value, new_value: &Value) {
    if let Err(e) = sqlx::query(
        "INSERT INTO app_settings_changes (setting_id, field, old_value, new_value, actor) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(setting_id)
    .bind(field)
    .bind(old_value)
    .bind(new_value)
    .bind(actor)
    .execute(pool)
    .await
    {
        tracing::warn!(setting_id, error = %e, "settings change not recorded");
    }
}

/// Record every tracked field a save changed, given the record before and
/// after it, serialized.
pub async fn record_diff(pool: &PgPool, before: &Value, after: &Value, fields: &[(&'static str, &'static str)]) {
    record_diff_by(pool, "owner", before, after, fields).await;
}

/// [`record_diff`], naming who made the change.
pub async fn record_diff_by(pool: &PgPool, actor: &str, before: &Value, after: &Value, fields: &[(&'static str, &'static str)]) {
    for c in diff_fields(before, after, fields) {
        record_by(pool, actor, c.setting_id, c.field, &c.old_value, &c.new_value).await;
    }
}

#[derive(Debug, Serialize)]
pub struct ChangeItem {
    /// `setting:<row id>` or `paired:<event id>`.
    pub id: String,
    /// `setting` | `paired`.
    pub kind: &'static str,
    pub setting_id: Option<String>,
    /// The stored field a setting change wrote; a setting can have several
    /// (the model slots).
    pub field: Option<String>,
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
    /// Changes and pairings, newest first.
    pub changes: Vec<ChangeItem>,
}

/// Whether an undo can write `old_value` back to `field`.
fn undoable(field: &str, old_value: &Value) -> bool {
    if field == READING_FIELD {
        // A switch that was at its default holds null; putting it back means its default.
        return old_value.is_object();
    }
    if field == CAPABILITIES_FIELD || field == THEME_FIELD {
        return old_value.is_object() || old_value.is_string();
    }
    let known = PROFILE_FIELDS.iter().chain(ASSISTANT_FIELDS).any(|&(f, _)| f == field);
    known && (!old_value.is_null() || NULLABLE_FIELDS.contains(&field))
}

/// The theme a profile value holds, as recorded.
fn theme_of(profile: &Value) -> Value {
    profile.get("ui_preferences").and_then(|p| p.get("theme")).cloned().unwrap_or(Value::Null)
}

/// Records a theme change from an assistant-profile save (`ui_preferences`).
pub async fn record_theme(pool: &PgPool, before: &Value, after: &Value) {
    record_theme_by(pool, "owner", before, after).await;
}

async fn record_theme_by(pool: &PgPool, actor: &str, before: &Value, after: &Value) {
    let (old, new) = (theme_of(before), theme_of(after));
    if old != new && new.is_string() {
        record_by(pool, actor, "theme", THEME_FIELD, &old, &new).await;
    }
}

/// What `field` holds now, in the form a change row records it.
async fn current_value(pool: &PgPool, field: &str, recorded_new: &Value) -> Result<Value> {
    if field == CAPABILITIES_FIELD {
        let (stored,): (Value,) = sqlx::query_as("SELECT enabled_tools FROM app_assistant_profile LIMIT 1")
            .fetch_one(pool)
            .await?;
        // Only the tools the row names; a missing key reads as on.
        return Ok(crate::api::capabilities::subset(&stored, recorded_new));
    }
    if field == THEME_FIELD {
        let profile = serde_json::to_value(crate::api::assistant_profile::get_assistant_profile(pool).await?)?;
        return Ok(theme_of(&profile));
    }
    if field == READING_FIELD {
        let states = crate::api::model_access::list(pool).await?;
        let keys = recorded_new.as_object().map(|m| m.keys().cloned().collect::<Vec<_>>()).unwrap_or_default();
        return Ok(Value::Object(
            keys.into_iter()
                .map(|k| {
                    let on = states.iter().find(|s| s.id == k).map(|s| Value::Bool(s.on)).unwrap_or(Value::Null);
                    (k, on)
                })
                .collect(),
        ));
    }
    let profile = if PROFILE_FIELDS.iter().any(|&(f, _)| f == field) {
        serde_json::to_value(crate::api::profile::get_profile(pool).await?)?
    } else {
        serde_json::to_value(crate::api::assistant_profile::get_assistant_profile(pool).await?)?
    };
    Ok(profile.get(field).cloned().unwrap_or(Value::Null))
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
                field: Some(field),
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
            field: None,
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
    newest_only_undoes(&mut changes);

    Ok(Lately { changes })
}

/// What one change row writes: the field, and for a field that holds several
/// switches (what Ari can do, what it may read) the switches the row names,
/// so a change to web search doesn't hide Undo on a change to the browser.
fn undo_key(field: &str, new_value: Option<&Value>) -> String {
    match new_value.and_then(Value::as_object) {
        Some(switches) => {
            let mut keys: Vec<&str> = switches.keys().map(String::as_str).collect();
            keys.sort_unstable();
            format!("{field}:{}", keys.join(","))
        }
        None => field.to_string(),
    }
}

/// Takes Undo off every change but the newest to the same setting. `changes`
/// is newest first. Pure, so it is tested here.
fn newest_only_undoes(changes: &mut [ChangeItem]) {
    let mut seen = std::collections::HashSet::new();
    for c in changes.iter_mut().filter(|c| c.kind == "setting") {
        let key = undo_key(c.field.as_deref().unwrap_or_default(), c.new_value.as_ref());
        if !seen.insert(key) {
            c.can_undo = false;
        }
    }
}

/// Write a change's old value back and mark the change undone.
pub async fn undo(pool: &PgPool, id: i64) -> Result<()> {
    let _saving = saving().await;
    let row: Option<(String, Option<Value>, Option<Value>, Option<DateTime<Utc>>)> =
        sqlx::query_as("SELECT field, old_value, new_value, undone_at FROM app_settings_changes WHERE id = $1")
            .bind(id)
            .fetch_optional(pool)
            .await?;
    let Some((field, old_value, new_value, undone_at)) = row else {
        return Err(Error::NotFound(format!("no settings change {id}")));
    };
    if undone_at.is_some() {
        return Err(Error::InvalidInput("That change was already undone.".into()));
    }
    let old_value = old_value.unwrap_or(Value::Null);
    let new_value = new_value.unwrap_or(Value::Null);
    if !undoable(&field, &old_value) {
        return Err(Error::InvalidInput("That change can't be undone from here.".into()));
    }
    // Writing the old value back over a later change would lose it silently.
    if current_value(pool, &field, &new_value).await? != new_value {
        return Err(Error::InvalidInput(
            "That setting has changed again since. Change it on its page instead.".into(),
        ));
    }
    // Claim the row first, so two taps or two devices undo it once.
    let claimed = sqlx::query("UPDATE app_settings_changes SET undone_at = now() WHERE id = $1 AND undone_at IS NULL")
        .bind(id)
        .execute(pool)
        .await?
        .rows_affected();
    if claimed == 0 {
        return Err(Error::InvalidInput("That change was already undone.".into()));
    }

    let written = write_back(pool, &field, old_value).await;
    if written.is_err() {
        // Nothing was put back, so the change is still live and still undoable.
        sqlx::query("UPDATE app_settings_changes SET undone_at = NULL WHERE id = $1")
            .bind(id)
            .execute(pool)
            .await?;
    }
    written
}

/// Writes `old_value` back to `field` and records the write as a change of its own.
async fn write_back(pool: &PgPool, field: &str, old_value: Value) -> Result<()> {
    if field == CAPABILITIES_FIELD {
        // A capability change holds only its own tools; put those back.
        crate::api::capabilities::apply(pool, &old_value).await?;
        return Ok(());
    }
    if field == READING_FIELD {
        for (kind, was) in old_value.as_object().into_iter().flatten() {
            // `null` was the kind's default; the default is what it held.
            let default_on = crate::api::model_access::KINDS.iter().find(|k| k.id == kind).is_some_and(|k| k.default_on);
            crate::api::model_access::set(pool, kind, was.as_bool().unwrap_or(default_on)).await?;
        }
        return Ok(());
    }
    if field == THEME_FIELD {
        let before = serde_json::to_value(crate::api::assistant_profile::get_assistant_profile(pool).await?)?;
        let mut prefs = before.get("ui_preferences").cloned().filter(Value::is_object).unwrap_or_else(|| json!({}));
        prefs["theme"] = old_value;
        let request: crate::api::assistant_profile::UpdateAssistantProfileRequest =
            serde_json::from_value(json!({ "ui_preferences": prefs }))?;
        let after = crate::api::assistant_profile::update_assistant_profile(pool, request).await?;
        record_theme(pool, &before, &serde_json::to_value(after)?).await;
        return Ok(());
    }
    // A cleared text field goes back as "", which the profile update stores as NULL.
    let value = if old_value.is_null() && PROFILE_TEXT_FIELDS.contains(&field) { json!("") } else { old_value };
    let patch = json!({ field: value });
    if PROFILE_FIELDS.iter().any(|&(f, _)| f == field) {
        let before = serde_json::to_value(crate::api::profile::get_profile(pool).await?)?;
        let after = crate::api::profile::update_profile(pool, serde_json::from_value(patch)?).await?;
        record_diff(pool, &before, &serde_json::to_value(after)?, PROFILE_FIELDS).await;
    } else {
        let before = serde_json::to_value(crate::api::assistant_profile::get_assistant_profile(pool).await?)?;
        let after = crate::api::assistant_profile::update_assistant_profile(pool, serde_json::from_value(patch)?).await?;
        record_diff(pool, &before, &serde_json::to_value(after)?, ASSISTANT_FIELDS).await;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn change(id: &str, field: &str, new_value: Value) -> ChangeItem {
        ChangeItem {
            id: id.into(),
            kind: "setting",
            setting_id: Some("x".into()),
            field: Some(field.into()),
            old_value: Some(Value::Null),
            new_value: Some(new_value),
            label: None,
            actor: Some("owner".into()),
            occurred_at: Utc::now(),
            can_undo: true,
        }
    }

    #[test]
    fn only_the_newest_change_to_a_setting_undoes() {
        let mut changes = vec![
            change("a", THEME_FIELD, json!("oxford")),
            change("b", "enabled_tools", json!({ "web_search": false })),
            change("c", THEME_FIELD, json!("asgard")),
            change("d", "enabled_tools", json!({ "applet": true })),
            change("e", "enabled_tools", json!({ "web_search": true })),
        ];
        newest_only_undoes(&mut changes);
        let undoes: Vec<_> = changes.iter().map(|c| c.can_undo).collect();
        assert_eq!(undoes, vec![true, true, false, true, false]);
    }

    #[test]
    fn diff_lists_only_tracked_fields_that_changed() {
        let before = json!({ "employer": "Fieldwork", "full_name": "Nick", "sidebar_doors": null });
        let after = json!({ "employer": "Northwind", "full_name": "Nick", "sidebar_doors": { "order": [] } });
        let changes = diff_fields(&before, &after, PROFILE_FIELDS);
        assert_eq!(
            changes,
            vec![FieldChange {
                setting_id: "employer",
                field: "employer",
                old_value: json!("Fieldwork"),
                new_value: json!("Northwind"),
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
    fn undo_needs_a_value_to_write_back_except_for_clearable_fields() {
        assert!(undoable(THEME_FIELD, &json!("oxford")));
        assert!(!undoable(THEME_FIELD, &Value::Null));
        assert!(undoable("standard_model_id", &Value::Null));
        assert!(undoable("employer", &Value::Null), "a profile text field can go back to empty");
        assert!(!undoable("assistant_name", &Value::Null));
        assert!(!undoable("sidebar_doors", &json!({})));
    }

    #[test]
    fn the_theme_is_read_from_ui_preferences() {
        assert_eq!(theme_of(&json!({ "ui_preferences": { "theme": "asgard" } })), json!("asgard"));
        assert_eq!(theme_of(&json!({ "ui_preferences": null })), Value::Null);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn undo_refuses_when_the_setting_changed_again_and_runs_once(pool: PgPool) {
        // Two limit changes in a row; undoing the first would lose the second.
        record(&pool, "limit-day", "request_day_limit_micros", &json!(1_000_000), &json!(2_000_000)).await;
        record(&pool, "limit-day", "request_day_limit_micros", &json!(2_000_000), &json!(3_000_000)).await;
        sqlx::query("UPDATE app_assistant_profile SET request_day_limit_micros = 3000000").execute(&pool).await.unwrap();
        let ids: Vec<i64> = sqlx::query_scalar("SELECT id FROM app_settings_changes ORDER BY id").fetch_all(&pool).await.unwrap();

        assert!(undo(&pool, ids[0]).await.is_err(), "the older change is no longer what is stored");
        undo(&pool, ids[1]).await.expect("the newest change undoes");
        assert!(undo(&pool, ids[1]).await.is_err(), "a second undo of the same row is refused");
        let stored: Option<i64> =
            sqlx::query_scalar("SELECT request_day_limit_micros FROM app_assistant_profile").fetch_one(&pool).await.unwrap();
        assert_eq!(stored, Some(2_000_000));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn a_cleared_profile_field_undoes_back_to_its_value_and_clears_again(pool: PgPool) {
        sqlx::query("UPDATE app_user_profile SET employer = NULL").execute(&pool).await.unwrap();
        record(&pool, "employer", "employer", &json!("Northwind"), &Value::Null).await;
        let id: i64 = sqlx::query_scalar("SELECT id FROM app_settings_changes").fetch_one(&pool).await.unwrap();
        undo(&pool, id).await.expect("undo restores the value");
        let stored: Option<String> = sqlx::query_scalar("SELECT employer FROM app_user_profile").fetch_one(&pool).await.unwrap();
        assert_eq!(stored.as_deref(), Some("Northwind"));
    }
}
