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
//! Drafting (`POST /api/settings/draft`): the owner types what they want in
//! the Settings search field and presses Return. The Lite model proposes new
//! values for the settings in [`DRAFT_FIELDS`] only, each proposal is checked
//! here ([`validate_reply`]), and nothing is saved. `POST
//! /api/settings/draft/apply` saves what the owner accepted, checked again,
//! and records it with the actor 'assistant'.

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
const PROFILE_TEXT_FIELDS: &[&str] = &["full_name", "preferred_name", "occupation", "employer", "home_timezone"];

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

    Ok(Lately { changes })
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

// =============================================================================
// Drafting
// =============================================================================

/// What a drafted value must be.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DraftKind {
    /// Text up to this many characters.
    Text(usize),
    /// An IANA time zone name.
    TimeZone,
    /// Dollars from the model, stored as micros-USD.
    Dollars,
}

/// The settings the assistant may draft: (field, setting id, kind, whether
/// it lives on the profile rather than the assistant profile). Only text and
/// money that the two profile updates write; switches that move money or
/// network settings are not drafted.
pub const DRAFT_FIELDS: &[(&str, &str, DraftKind, bool)] = &[
    ("full_name", "full-name", DraftKind::Text(200), true),
    ("preferred_name", "preferred-name", DraftKind::Text(100), true),
    ("occupation", "occupation", DraftKind::Text(200), true),
    ("employer", "employer", DraftKind::Text(200), true),
    ("home_timezone", "home", DraftKind::TimeZone, true),
    ("assistant_name", "assistant-name", DraftKind::Text(100), false),
    ("style_notes", "style", DraftKind::Text(crate::api::assistant_profile::STYLE_NOTES_MAX_CHARS), false),
    ("request_run_limit_micros", "limit-run", DraftKind::Dollars, false),
    ("request_day_limit_micros", "limit-day", DraftKind::Dollars, false),
];

/// The most a drafted spending limit may be, in dollars. A draft is checked
/// before the owner sees it; anything larger is more likely a misreading.
const DRAFT_DOLLARS_MAX: f64 = 1_000.0;

fn draft_spec(field: &str) -> Option<(&'static str, &'static str, DraftKind, bool)> {
    DRAFT_FIELDS.iter().copied().find(|&(f, ..)| f == field)
}

/// One change the assistant proposes, in stored form.
#[derive(Debug, Clone, PartialEq, Serialize, serde::Deserialize)]
pub struct DraftChange {
    pub setting_id: String,
    pub field: String,
    pub old_value: Value,
    pub new_value: Value,
}

#[derive(Debug, Serialize)]
pub struct Draft {
    pub changes: Vec<DraftChange>,
    /// One sentence from the model, shown when nothing could be drafted.
    pub note: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct ReplyChange {
    field: String,
    value: Value,
}

#[derive(Debug, serde::Deserialize)]
struct Reply {
    #[serde(default)]
    changes: Vec<ReplyChange>,
    #[serde(default)]
    note: Option<String>,
}

/// The value a draft would store for `field`, or `None` when it isn't one
/// the field accepts. `from_model` reads dollars as the model writes them;
/// otherwise the value is already in stored form (micros), as an apply sends.
pub fn stored_value(field: &str, value: &Value, from_model: bool) -> Option<Value> {
    let (_, _, kind, _) = draft_spec(field)?;
    match kind {
        DraftKind::Text(max) => {
            let text = value.as_str()?.trim();
            (!text.is_empty() && text.chars().count() <= max).then(|| json!(text))
        }
        DraftKind::TimeZone => {
            let name = value.as_str()?.trim();
            name.parse::<chrono_tz::Tz>().ok().map(|_| json!(name))
        }
        DraftKind::Dollars => {
            let micros = if from_model {
                let dollars = value.as_f64()?;
                if !(0.0..=DRAFT_DOLLARS_MAX).contains(&dollars) {
                    return None;
                }
                (dollars * 1_000_000.0).round() as i64
            } else {
                value.as_i64()?
            };
            (0..=(DRAFT_DOLLARS_MAX as i64) * 1_000_000).contains(&micros).then(|| json!(micros))
        }
    }
}

/// The model's reply, checked: only fields in [`DRAFT_FIELDS`], only values
/// those fields accept, only values that differ from `current`, one change
/// per field (the last one wins). Pure, so it is tested here.
pub fn validate_reply(reply_text: &str, current: &Value) -> Draft {
    let parsed = reply_text
        .find('{')
        .zip(reply_text.rfind('}'))
        .filter(|(a, b)| a < b)
        .and_then(|(a, b)| serde_json::from_str::<Reply>(&reply_text[a..=b]).ok());
    let Some(reply) = parsed else {
        return Draft { changes: Vec::new(), note: None };
    };

    let mut changes: Vec<DraftChange> = Vec::new();
    for c in reply.changes {
        let Some((field, setting_id, ..)) = draft_spec(&c.field) else { continue };
        let Some(new_value) = stored_value(field, &c.value, true) else { continue };
        let old_value = current.get(field).cloned().unwrap_or(Value::Null);
        if old_value == new_value {
            continue;
        }
        changes.retain(|x| x.field != field);
        changes.push(DraftChange { setting_id: setting_id.into(), field: field.into(), old_value, new_value });
    }
    let note = reply.note.map(|n| n.trim().to_string()).filter(|n| !n.is_empty());
    Draft { changes, note }
}

/// The profile and assistant profile as one object, for the prompt and for
/// comparing a draft against what is stored.
async fn current_values(pool: &PgPool) -> Result<Value> {
    let mut merged = serde_json::to_value(crate::api::profile::get_profile(pool).await?)?;
    let assistant = serde_json::to_value(crate::api::assistant_profile::get_assistant_profile(pool).await?)?;
    if let (Some(m), Value::Object(a)) = (merged.as_object_mut(), assistant) {
        m.extend(a);
    }
    Ok(merged)
}

fn draft_prompt(current: &Value) -> String {
    let mut lines = Vec::new();
    for &(field, _, kind, _) in DRAFT_FIELDS {
        let value = current.get(field).cloned().unwrap_or(Value::Null);
        let shown = match (kind, &value) {
            (DraftKind::Dollars, Value::Number(n)) => format!("{:.2} dollars", n.as_f64().unwrap_or(0.0) / 1_000_000.0),
            (_, Value::Null) => "(not set)".to_string(),
            (_, v) => v.to_string(),
        };
        let what = match kind {
            DraftKind::Text(max) => format!("text, up to {max} characters"),
            DraftKind::TimeZone => "an IANA time zone name, like Europe/Lisbon".to_string(),
            DraftKind::Dollars => "a number of dollars".to_string(),
        };
        lines.push(format!("- {field} ({what}): {shown}"));
    }
    format!("The settings you can change, with their current values:\n{}", lines.join("\n"))
}

const DRAFT_SYSTEM: &str = "You turn what a person types into changes to their settings in Virtues, a personal assistant that runs on their own server. They see your changes as a draft and choose whether to apply them.\n\nChange only the settings listed. Reply with JSON only, in this shape: {\"changes\": [{\"field\": \"<field name>\", \"value\": <new value>}], \"note\": \"<one short sentence>\"}.\n\nFor style_notes, which says how the assistant should talk to them, return the whole new note: keep what is already there unless they ask to replace it, and add or change only what they asked for, in their own voice, as a short instruction. For request_run_limit_micros and request_day_limit_micros give the value in dollars, as a number. When what they typed is not about these settings, return an empty changes list and say in the note what they could do instead, in plain words.";

/// Ask the Lite model for a draft. Saves nothing.
pub async fn draft(pool: &PgPool, text: &str) -> Result<Draft> {
    let text = text.trim();
    if text.is_empty() {
        return Err(Error::InvalidInput("type what you want to change first".into()));
    }
    if text.chars().count() > 500 {
        return Err(Error::InvalidInput("that's longer than a settings change; try asking in a chat".into()));
    }
    let current = current_values(pool).await?;
    let user = format!("{}\n\nWhat they typed: {}", draft_prompt(&current), json!(text));
    let reply = crate::virtues_api::completion::system_completion(
        pool,
        virtues_registry::models::ModelSlot::Lite,
        "settings_draft",
        DRAFT_SYSTEM,
        &user,
        crate::virtues_api::request::Thinking::Low,
        0.2,
    )
    .await?;
    Ok(validate_reply(&reply, &current))
}

#[derive(Debug, serde::Deserialize)]
pub struct ApplyChange {
    pub field: String,
    pub new_value: Value,
    /// What the draft said the setting held. When it no longer does, the
    /// draft is stale and applying it would overwrite a newer change.
    #[serde(default)]
    pub old_value: Option<Value>,
}

/// Save drafted changes the owner accepted, checked again, and record them
/// as the assistant's.
pub async fn apply_draft(pool: &PgPool, changes: Vec<ApplyChange>) -> Result<()> {
    let _saving = saving().await;
    let current = current_values(pool).await?;
    let mut profile = serde_json::Map::new();
    let mut assistant = serde_json::Map::new();
    for c in changes {
        if let Some(old) = &c.old_value {
            if current.get(&c.field).unwrap_or(&Value::Null) != old {
                return Err(Error::InvalidInput(
                    "A setting changed after this was drafted. Nothing was changed; ask again.".into(),
                ));
            }
        }
        let Some((field, _, _, on_profile)) = draft_spec(&c.field) else {
            return Err(Error::InvalidInput(format!("{} can't be changed from a draft", c.field)));
        };
        let Some(value) = stored_value(field, &c.new_value, false) else {
            return Err(Error::InvalidInput(format!("{field} can't take that value")));
        };
        if on_profile {
            profile.insert(field.into(), value);
        } else {
            assistant.insert(field.into(), value);
        }
    }
    // The two halves live in two tables. The profile goes first; if the
    // assistant half then fails, the profile half is put back, so a failed
    // Apply changes nothing, as the card says.
    let profile_saved = if profile.is_empty() {
        None
    } else {
        let before = serde_json::to_value(crate::api::profile::get_profile(pool).await?)?;
        let fields: Vec<String> = profile.keys().cloned().collect();
        let after = crate::api::profile::update_profile(pool, serde_json::from_value(Value::Object(profile))?).await?;
        Some((before, serde_json::to_value(after)?, fields))
    };
    if !assistant.is_empty() {
        let before = serde_json::to_value(crate::api::assistant_profile::get_assistant_profile(pool).await?)?;
        let saved = async {
            let request = serde_json::from_value(Value::Object(assistant))?;
            crate::api::assistant_profile::update_assistant_profile(pool, request).await
        }
        .await;
        match saved {
            Ok(after) => record_diff_by(pool, "assistant", &before, &serde_json::to_value(after)?, ASSISTANT_FIELDS).await,
            Err(e) => {
                if let Some((profile_before, _, fields)) = &profile_saved {
                    let restore: serde_json::Map<String, Value> = fields
                        .iter()
                        .map(|f| {
                            let v = profile_before.get(f).cloned().unwrap_or(Value::Null);
                            let v = if v.is_null() && PROFILE_TEXT_FIELDS.contains(&f.as_str()) { json!("") } else { v };
                            (f.clone(), v)
                        })
                        .collect();
                    if let Err(undo) = async {
                        crate::api::profile::update_profile(pool, serde_json::from_value(Value::Object(restore))?).await
                    }
                    .await
                    {
                        tracing::warn!("apply draft: couldn't put the profile half back: {undo}");
                    }
                }
                return Err(e);
            }
        }
    }
    if let Some((before, after, _)) = &profile_saved {
        record_diff_by(pool, "assistant", before, after, PROFILE_FIELDS).await;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_draft_keeps_only_listed_fields_and_valid_values() {
        let current = json!({ "style_notes": "No lists.", "preferred_name": "Nick", "request_day_limit_micros": 2_000_000 });
        let reply = r#"Here you go: {"changes": [
            {"field": "style_notes", "value": "No lists. Keep answers short."},
            {"field": "theme", "value": "dark"},
            {"field": "home_timezone", "value": "Mars/Olympus"},
            {"field": "request_day_limit_micros", "value": 5},
            {"field": "preferred_name", "value": "Nick"}
        ], "note": "Done."}"#;
        let d = validate_reply(reply, &current);
        let fields: Vec<_> = d.changes.iter().map(|c| c.field.as_str()).collect();
        assert_eq!(fields, vec!["style_notes", "request_day_limit_micros"]);
        assert_eq!(d.changes[1].new_value, json!(5_000_000));
        assert_eq!(d.changes[1].old_value, json!(2_000_000));
        assert_eq!(d.note.as_deref(), Some("Done."));
    }

    #[test]
    fn a_draft_with_no_json_or_nothing_to_change_is_empty() {
        assert!(validate_reply("I can't help with that.", &json!({})).changes.is_empty());
        let d = validate_reply(r#"{"changes": [], "note": "Ask in a chat instead."}"#, &json!({}));
        assert!(d.changes.is_empty());
        assert_eq!(d.note.as_deref(), Some("Ask in a chat instead."));
    }

    #[test]
    fn limits_must_be_sane_dollars_and_text_must_fit() {
        assert_eq!(stored_value("request_run_limit_micros", &json!(1.5), true), Some(json!(1_500_000)));
        assert_eq!(stored_value("request_run_limit_micros", &json!(-1), true), None);
        assert_eq!(stored_value("request_run_limit_micros", &json!(5000), true), None);
        assert_eq!(stored_value("request_run_limit_micros", &json!(1_500_000), false), Some(json!(1_500_000)));
        assert_eq!(stored_value("assistant_name", &json!("  "), true), None);
        assert_eq!(stored_value("assistant_name", &json!("x".repeat(101)), true), None);
        assert_eq!(stored_value("home_timezone", &json!("Europe/Lisbon"), true), Some(json!("Europe/Lisbon")));
        assert_eq!(stored_value("sidebar_doors", &json!("x"), true), None);
    }

    #[test]
    fn the_last_change_to_a_field_wins() {
        let reply = r#"{"changes": [{"field": "assistant_name", "value": "Ida"}, {"field": "assistant_name", "value": "Juno"}]}"#;
        let d = validate_reply(reply, &json!({ "assistant_name": "Ari" }));
        assert_eq!(d.changes.len(), 1);
        assert_eq!(d.changes[0].new_value, json!("Juno"));
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
