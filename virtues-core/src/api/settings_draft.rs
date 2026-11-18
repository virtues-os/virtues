//! The Settings search field's agent: the owner types what they want and
//! presses Return (`POST /api/settings/draft`), and applies what they accept
//! (`POST /api/settings/draft/apply`).
//!
//! The Lite model reads every setting it may change ([`FIELDS`]) with what it
//! holds now, the pages it can only send the owner to ([`PAGES`]), and the
//! last changes, and replies in one of four ways: changes, two or three
//! options to pick from, a page to open, or an answer to a question about
//! their settings. Everything it returns is checked here ([`validate_reply`])
//! before the owner sees it, and nothing is saved until they apply it.
//!
//! Appearance settings are kept in the browser (the theme also in
//! `ui_preferences`), so the page sends their values with the request and
//! applies an accepted change itself; they never reach [`apply_draft`].
//!
//! A page is used instead of a change when the owner has to be there: a key
//! must go from them to their server without passing through the model, a
//! card payment is theirs to start, a device has to be in their hand.

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sqlx::PgPool;

use super::settings_changes::{record_diff_by, saving, ASSISTANT_FIELDS, PROFILE_FIELDS, PROFILE_TEXT_FIELDS};
use crate::error::{Error, Result};

/// What a drafted value must be.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    /// Text up to this many characters.
    Text(usize),
    /// An IANA time zone name.
    TimeZone,
    /// A date in the past, `YYYY-MM-DD`.
    Date,
    /// Dollars from the model, stored as micros-USD.
    Dollars,
    /// One of these stored values, each with the words a person uses for it.
    Choice(&'static [(&'static str, &'static str)]),
    /// Days to keep the audio of recordings: one of `recordings::CHOICES`,
    /// or null for forever.
    RecordingDays,
    /// On or off.
    Switch,
}

/// Where a field is saved.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Store {
    Profile,
    Assistant,
    /// `capabilities.<id>`: one switch of what the assistant can do.
    Capability,
    /// `reading.<id>`: one kind of record the assistant may read.
    Reading,
    /// Appearance, applied by the page.
    App,
}

/// One setting the agent may change.
#[derive(Debug, Clone, Copy)]
pub struct Field {
    pub field: &'static str,
    /// Its id in apps/web/src/lib/settings.
    pub setting_id: &'static str,
    pub kind: Kind,
    pub store: Store,
    /// What it is, for the prompt.
    pub about: &'static str,
}

const fn f(field: &'static str, setting_id: &'static str, kind: Kind, store: Store, about: &'static str) -> Field {
    Field { field, setting_id, kind, store, about }
}

const UNITS: &[(&str, &str)] = &[("us", "US units: miles, pounds, Fahrenheit"), ("metric", "metric units")];
const THEMES: &[(&str, &str)] = &[("oxford", "Light"), ("asgard", "Dark"), ("system", "match the device")];
const ACCENTS: &[(&str, &str)] = &[
    ("navy", "navy"),
    ("blue", "blue"),
    ("indigo", "indigo"),
    ("purple", "purple"),
    ("pink", "pink"),
    ("teal", "teal"),
    ("graphite", "graphite gray"),
];
const GLASS: &[(&str, &str)] = &[("none", "none, solid"), ("low", "low"), ("medium", "medium"), ("high", "high")];
const SIZES: &[(&str, &str)] = &[("s", "small"), ("m", "medium"), ("l", "large")];
const FONTS: &[(&str, &str)] = &[("sans", "sans serif"), ("serif", "serif"), ("mono", "monospace")];

/// Every setting the agent may change. The capability and reading rows must
/// name every switch `capabilities::capabilities` and `model_access::KINDS`
/// have; a test holds them together.
pub const FIELDS: &[Field] = &[
    f("full_name", "full-name", Kind::Text(200), Store::Profile, "their full name"),
    f("preferred_name", "preferred-name", Kind::Text(100), Store::Profile, "what the assistant calls them"),
    f("birth_date", "birthday", Kind::Date, Store::Profile, "their birthday"),
    f("occupation", "occupation", Kind::Text(200), Store::Profile, "their work"),
    f("employer", "employer", Kind::Text(200), Store::Profile, "where they work"),
    f("home_timezone", "home", Kind::TimeZone, Store::Profile, "the time zone where they live"),
    f("units", "units", Kind::Choice(UNITS), Store::Profile, "the units they read in"),
    f(
        "keep_recordings_days",
        "keep-recordings",
        Kind::RecordingDays,
        Store::Profile,
        "how long their server keeps the audio of recordings; transcripts always stay",
    ),
    f("assistant_name", "assistant-name", Kind::Text(100), Store::Assistant, "the assistant's name"),
    f(
        "style_notes",
        "style",
        Kind::Text(crate::api::assistant_profile::STYLE_NOTES_MAX_CHARS),
        Store::Assistant,
        "how the assistant talks to them, as short instructions in their voice",
    ),
    f("request_run_limit_micros", "limit-run", Kind::Dollars, Store::Assistant, "the most one request may spend"),
    f("request_day_limit_micros", "limit-day", Kind::Dollars, Store::Assistant, "the most requests may spend in a day"),
    f("capabilities.web_search", "capabilities", Kind::Switch, Store::Capability, "whether the assistant may search the web"),
    f("capabilities.browser", "capabilities", Kind::Switch, Store::Capability, "whether the assistant may use a web browser"),
    f("capabilities.applets", "capabilities", Kind::Switch, Store::Capability, "whether the assistant may make and change applets"),
    f("capabilities.pages", "capabilities", Kind::Switch, Store::Capability, "whether the assistant may write and edit pages"),
    f("reading.messages", "model-reading", Kind::Switch, Store::Reading, "whether the assistant may read their messages"),
    f("reading.email", "model-reading", Kind::Switch, Store::Reading, "whether the assistant may read their email"),
    f("reading.recordings", "model-reading", Kind::Switch, Store::Reading, "whether the assistant may read transcripts of their recordings"),
    f("reading.calendar", "model-reading", Kind::Switch, Store::Reading, "whether the assistant may read their calendar"),
    f("reading.health", "model-reading", Kind::Switch, Store::Reading, "whether the assistant may read their health records"),
    f("reading.cycle", "model-reading", Kind::Switch, Store::Reading, "whether the assistant may read their cycle tracking"),
    f("reading.location", "model-reading", Kind::Switch, Store::Reading, "whether the assistant may read their location history"),
    f("reading.money", "model-reading", Kind::Switch, Store::Reading, "whether the assistant may read their money records"),
    f("reading.browsing", "model-reading", Kind::Switch, Store::Reading, "whether the assistant may read their app and web history"),
    f("reading.documents", "model-reading", Kind::Switch, Store::Reading, "whether the assistant may read their saved documents and bookmarks"),
    f("theme", "theme", Kind::Choice(THEMES), Store::App, "the app's theme"),
    f("accent", "accent", Kind::Choice(ACCENTS), Store::App, "the app's accent color"),
    f("glass", "glass", Kind::Choice(GLASS), Store::App, "how much of the painting shows through the sidebar, tabs and buttons"),
    f("text_size", "text-size", Kind::Choice(SIZES), Store::App, "the size of text on pages"),
    f("reading_font", "reading-font", Kind::Choice(FONTS), Store::App, "the font pages are read in"),
    f("reduce_motion", "reduce-motion", Kind::Switch, Store::App, "whether the app keeps still instead of sliding and fading"),
];

/// Pages the agent sends the owner to, with why, as the card says it.
/// `{name}` is the assistant's name, filled in by the page.
pub const PAGES: &[(&str, &str)] = &[
    ("keys", "A key goes straight from you to your server, so {name} never reads it."),
    ("byo-ai", "A key goes straight from you to your server, so {name} never reads it."),
    ("subscription", "Your subscription and card are on Billing, where you see what you pay."),
    ("auto-top-up", "Auto top-up charges your card, so you set it on Billing."),
    ("models", "You pick models from the list, where you can compare what each costs."),
    ("subjects", "You add the subjects {name} never raises one at a time, on its page."),
    ("memory", "You choose what {name} forgets on Memory, one note at a time."),
    ("shared-links", "You turn off a link from its row on Shared links."),
    ("devices", "You unpair a device from its row on Devices."),
    ("pair-device", "Pairing needs the device in your hand."),
    ("backup", "Backup needs a drive and a recovery key that only you keep."),
    ("export", "You start the export on Data access, and it makes a file of everything."),
    ("updates", "Installing an update restarts your server, so you start it yourself."),
    ("auto-install", "This changes how your server updates, so you set it on Updates and network."),
    ("update-channel", "This changes which updates your server installs, so you pick it on Updates and network."),
    ("reach", "This changes how your devices reach your server, so you set it on Updates and network."),
    ("start-over", "Starting over unpairs everything, so you confirm it on its page."),
];

/// The most a drafted spending limit may be, in dollars. A draft is checked
/// before the owner sees it; anything larger is more likely a misreading.
const DOLLARS_MAX: f64 = 1_000.0;

/// How many recent changes the model reads, to answer "why did … change".
const RECENT: i64 = 15;

pub fn field(name: &str) -> Option<&'static Field> {
    FIELDS.iter().find(|f| f.field == name)
}

/// One change the agent proposes, in stored form.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DraftChange {
    pub setting_id: &'static str,
    pub field: &'static str,
    pub old_value: Value,
    pub new_value: Value,
    /// Applied by the page (appearance), not by [`apply_draft`].
    pub in_app: bool,
    /// What happens that the owner should know before applying, in a sentence.
    pub consequence: Option<String>,
    /// False when what it does can't be put back.
    pub can_undo: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OpenPage {
    pub setting_id: &'static str,
    pub reason: &'static str,
}

/// The agent's reply, checked. `kind` says which card the page draws.
#[derive(Debug, Serialize)]
pub struct Draft {
    /// `change` | `choose` | `open` | `answer` | `none`.
    pub kind: &'static str,
    pub changes: Vec<DraftChange>,
    /// For `choose`: one change per option.
    pub options: Vec<DraftChange>,
    pub open: Option<OpenPage>,
    pub answer: Option<String>,
    /// For `answer`: the settings it is about, so the page can show their
    /// latest change.
    pub about: Vec<&'static str>,
    /// One sentence from the model, shown when nothing else came back.
    pub note: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ReplyChange {
    field: String,
    #[serde(default)]
    value: Value,
}

#[derive(Debug, Default, Deserialize)]
struct Reply {
    #[serde(default)]
    changes: Vec<ReplyChange>,
    #[serde(default)]
    options: Vec<ReplyChange>,
    #[serde(default)]
    open: Option<String>,
    #[serde(default)]
    answer: Option<String>,
    #[serde(default)]
    about: Vec<String>,
    #[serde(default)]
    note: Option<String>,
}

/// The value `name` would store, or `None` when it isn't one the field
/// accepts. `from_model` reads dollars and choice words as the model writes
/// them; otherwise the value is already in stored form, as an apply sends it.
pub fn stored_value(name: &str, value: &Value, from_model: bool) -> Option<Value> {
    let spec = field(name)?;
    match spec.kind {
        Kind::Text(max) => {
            let text = value.as_str()?.trim();
            (!text.is_empty() && text.chars().count() <= max).then(|| json!(text))
        }
        Kind::TimeZone => {
            let name = value.as_str()?.trim();
            name.parse::<chrono_tz::Tz>().ok().map(|_| json!(name))
        }
        Kind::Date => {
            let date = chrono::NaiveDate::parse_from_str(value.as_str()?.trim(), "%Y-%m-%d").ok()?;
            let today = chrono::Utc::now().date_naive();
            let earliest = chrono::NaiveDate::from_ymd_opt(1900, 1, 1)?;
            (date <= today && date >= earliest).then(|| json!(date.format("%Y-%m-%d").to_string()))
        }
        Kind::Dollars => {
            let micros = if from_model {
                let dollars = value.as_f64()?;
                if !(0.0..=DOLLARS_MAX).contains(&dollars) {
                    return None;
                }
                (dollars * 1_000_000.0).round() as i64
            } else {
                value.as_i64()?
            };
            (0..=(DOLLARS_MAX as i64) * 1_000_000).contains(&micros).then(|| json!(micros))
        }
        Kind::Choice(choices) => {
            let given = value.as_str()?.trim();
            choices
                .iter()
                .find(|(stored, words)| {
                    *stored == given || (from_model && words.eq_ignore_ascii_case(given))
                })
                .map(|(stored, _)| json!(stored))
        }
        Kind::RecordingDays => match value {
            Value::Null => Some(Value::Null),
            Value::String(s) if from_model && s.eq_ignore_ascii_case("forever") => Some(Value::Null),
            v => {
                let days = i32::try_from(v.as_i64()?).ok()?;
                crate::api::recordings::CHOICES.contains(&days).then(|| json!(days))
            }
        },
        Kind::Switch => value.as_bool().map(Value::Bool),
    }
}

/// What applying `name` = `new` does that can't be undone, if anything:
/// (the sentence, whether it can be put back).
fn consequence(name: &str, old: &Value, new: &Value) -> Option<(String, bool)> {
    if name == "keep_recordings_days" {
        let days = new.as_i64()?;
        let shorter = old.as_i64().is_none_or(|was| days < was);
        return shorter.then(|| {
            (
                format!(
                    "Your server deletes the audio of recordings older than {days} days, from now on. \
                     Transcripts stay. Deleted audio can't be brought back."
                ),
                false,
            )
        });
    }
    None
}

fn draft_change(spec: &'static Field, old_value: Value, new_value: Value) -> DraftChange {
    let consequence = consequence(spec.field, &old_value, &new_value);
    DraftChange {
        setting_id: spec.setting_id,
        field: spec.field,
        in_app: spec.store == Store::App,
        can_undo: consequence.as_ref().is_none_or(|(_, undoable)| *undoable),
        consequence: consequence.map(|(sentence, _)| sentence),
        old_value,
        new_value,
    }
}

/// A proposed change, checked against the field and what it holds now.
/// `None` when the field is unknown, the value isn't one it accepts, or it
/// holds that value already.
fn checked(c: &ReplyChange, current: &Value) -> Option<DraftChange> {
    let spec = field(c.field.trim())?;
    let new_value = stored_value(spec.field, &c.value, true)?;
    let old_value = current.get(spec.field).cloned().unwrap_or(Value::Null);
    (old_value != new_value).then(|| draft_change(spec, old_value, new_value))
}

/// A setting id for a field name or a setting id the model gave.
fn setting_id_for(name: &str) -> Option<&'static str> {
    let name = name.trim();
    field(name)
        .map(|f| f.setting_id)
        .or_else(|| FIELDS.iter().find(|f| f.setting_id == name).map(|f| f.setting_id))
        .or_else(|| PAGES.iter().find(|(id, _)| *id == name).map(|(id, _)| *id))
}

/// The model's reply, checked: only listed fields and pages, only values
/// those fields accept, only values that differ from `current`, one change
/// per field (the last one wins). Pure, so it is tested here.
pub fn validate_reply(reply_text: &str, current: &Value) -> Draft {
    let reply = reply_text
        .find('{')
        .zip(reply_text.rfind('}'))
        .filter(|(a, b)| a < b)
        .and_then(|(a, b)| serde_json::from_str::<Reply>(&reply_text[a..=b]).ok())
        .unwrap_or_default();

    let mut changes: Vec<DraftChange> = Vec::new();
    for c in &reply.changes {
        if let Some(change) = checked(c, current) {
            changes.retain(|x| x.field != change.field);
            changes.push(change);
        }
    }
    let mut options: Vec<DraftChange> = Vec::new();
    for c in &reply.options {
        if let Some(option) = checked(c, current) {
            if !options.iter().any(|o| o.field == option.field && o.new_value == option.new_value) {
                options.push(option);
            }
        }
    }
    options.truncate(3);
    // One option is a change.
    if changes.is_empty() && options.len() == 1 {
        changes = std::mem::take(&mut options);
    }
    let open = reply
        .open
        .as_deref()
        .and_then(|id| PAGES.iter().find(|(page, _)| *page == id.trim()))
        .map(|&(setting_id, reason)| OpenPage { setting_id, reason });
    let answer = reply
        .answer
        .map(|a| a.trim().to_string())
        .filter(|a| !a.is_empty() && a.chars().count() <= 600);
    let mut about: Vec<&'static str> = Vec::new();
    for id in reply.about.iter().filter_map(|name| setting_id_for(name)) {
        if !about.contains(&id) {
            about.push(id);
        }
    }
    let note = reply.note.map(|n| n.trim().to_string()).filter(|n| !n.is_empty());

    let kind = if !changes.is_empty() {
        "change"
    } else if options.len() >= 2 {
        "choose"
    } else if open.is_some() {
        "open"
    } else if answer.is_some() {
        "answer"
    } else {
        "none"
    };
    Draft {
        kind,
        options: if kind == "choose" { options } else { Vec::new() },
        open: if kind == "open" { open } else { None },
        answer: if kind == "answer" { answer } else { None },
        about: if kind == "answer" { about } else { Vec::new() },
        changes,
        note,
    }
}

/// Every field's value now, keyed by field name: the profile and assistant
/// profile, one key per capability and reading switch, and the appearance
/// values `app` sent (only the ones a field accepts).
async fn current_values(pool: &PgPool, app: &Value) -> Result<Value> {
    let mut merged = serde_json::to_value(crate::api::profile::get_profile(pool).await?)?;
    let assistant = serde_json::to_value(crate::api::assistant_profile::get_assistant_profile(pool).await?)?;
    let Some(m) = merged.as_object_mut() else {
        return Err(Error::Other("the profile didn't read as an object".into()));
    };
    if let Value::Object(a) = assistant {
        m.extend(a);
    }
    for s in crate::api::capabilities::list(pool).await? {
        m.insert(format!("capabilities.{}", s.id), Value::Bool(s.on));
    }
    for s in crate::api::model_access::list(pool).await? {
        m.insert(format!("reading.{}", s.id), Value::Bool(s.on));
    }
    for spec in FIELDS.iter().filter(|f| f.store == Store::App) {
        if let Some(v) = app.get(spec.field).and_then(|v| stored_value(spec.field, v, false)) {
            m.insert(spec.field.to_string(), v);
        }
    }
    Ok(merged)
}

/// A value as the prompt shows it.
fn shown(kind: Kind, value: &Value) -> String {
    match (kind, value) {
        (Kind::Dollars, Value::Number(n)) => format!("{:.2} dollars", n.as_f64().unwrap_or(0.0) / 1_000_000.0),
        (Kind::RecordingDays, Value::Null) => "forever".to_string(),
        (_, Value::Null) => "(not set)".to_string(),
        (_, Value::String(s)) if s.chars().count() > 160 => format!("{:?}…", s.chars().take(160).collect::<String>()),
        (_, v) => v.to_string(),
    }
}

fn kind_words(kind: Kind) -> String {
    match kind {
        Kind::Text(max) => format!("text, up to {max} characters"),
        Kind::TimeZone => "an IANA time zone name, like Europe/Lisbon".into(),
        Kind::Date => "a date, YYYY-MM-DD".into(),
        Kind::Dollars => "a number of dollars".into(),
        Kind::Choice(choices) => {
            let listed: Vec<String> = choices.iter().map(|(v, words)| format!("\"{v}\" ({words})")).collect();
            format!("one of {}", listed.join(", "))
        }
        Kind::RecordingDays => "30, 90 or 365 days, or null for forever".into(),
        Kind::Switch => "true or false".into(),
    }
}

fn draft_prompt(current: &Value, recent: &[String]) -> String {
    let settings: Vec<String> = FIELDS
        .iter()
        .map(|f| {
            let value = current.get(f.field).cloned().unwrap_or(Value::Null);
            format!("- {} ({}; {}): {}", f.field, f.about, kind_words(f.kind), shown(f.kind, &value))
        })
        .collect();
    let pages: Vec<String> = PAGES.iter().map(|(id, why)| format!("- {id}: {}", why.replace("{name}", "the assistant"))).collect();
    let recent = if recent.is_empty() { "(none in the last four weeks)".to_string() } else { recent.join("\n") };
    format!(
        "Settings you can change, with what each holds now:\n{}\n\nPages you can only open for them, because they have to do it there:\n{}\n\nTheir last changes, newest first:\n{}",
        settings.join("\n"),
        pages.join("\n"),
        recent
    )
}

/// The last few changes, one line each, for answering "why did … change",
/// with times in the owner's time zone.
async fn recent_changes(pool: &PgPool, tz: chrono_tz::Tz) -> Result<Vec<String>> {
    let rows: Vec<(String, Option<Value>, Option<Value>, String, chrono::DateTime<chrono::Utc>)> = sqlx::query_as(
        "SELECT field, old_value, new_value, actor, occurred_at FROM app_settings_changes \
         WHERE undone_at IS NULL AND occurred_at > now() - interval '28 days' \
         ORDER BY occurred_at DESC LIMIT $1",
    )
    .bind(RECENT)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(field, old, new, actor, at)| {
            let by = if actor == "assistant" { "drafted by the assistant, applied by them" } else { "by them" };
            let short = |v: Option<Value>| {
                let s = v.unwrap_or(Value::Null).to_string();
                if s.chars().count() > 120 { format!("{}…", s.chars().take(120).collect::<String>()) } else { s }
            };
            format!("- {} {field}: {} → {} ({by})", at.with_timezone(&tz).format("%A %Y-%m-%d %H:%M"), short(old), short(new))
        })
        .collect())
}

const DRAFT_SYSTEM: &str = "You are the settings agent in Virtues, a personal assistant that runs on the owner's own server. They type in the Settings search field, and you turn what they typed into one card. Nothing is saved until they choose to.\n\nReply with JSON only, in this shape, leaving out what you don't use: {\"changes\": [{\"field\": \"<field>\", \"value\": <value>}], \"options\": [{\"field\": \"<field>\", \"value\": <value>}], \"open\": \"<page id>\", \"answer\": \"<one or two sentences>\", \"about\": [\"<field or page id>\"], \"note\": \"<one short sentence>\"}.\n\n- When they ask for a change and it is clear which settings, put the changes in changes.\n- When it could mean two or three different settings, put one change for each in options and leave changes empty.\n- When what they want is done on one of the pages, put its id in open.\n- When they ask a question about their settings or about what changed, answer it in answer from the values and changes above, in plain words, speaking to them as \"you\", and list the fields it is about in about. Say when in words, like this morning or on Monday, not as a clock time.\n- When nothing fits, return nothing else and say in note, in plain words, what they could do instead.\n\nFor style_notes return the whole new note: keep what is already there unless they ask to replace it, and add or change only what they asked for, in their own voice, as a short instruction. For a choice give one of the listed values exactly. Spending limits are a number of dollars. A switch is true or false. Use only the fields and pages listed.";

/// Ask the Lite model for a card. `app` holds the appearance values the
/// page has now. Saves nothing.
pub async fn draft(pool: &PgPool, text: &str, app: &Value) -> Result<Draft> {
    let text = text.trim();
    if text.is_empty() {
        return Err(Error::InvalidInput("Type what you want to change first.".into()));
    }
    if text.chars().count() > 500 {
        return Err(Error::InvalidInput("That's longer than a settings change. Try asking in a chat.".into()));
    }
    let current = current_values(pool, app).await?;
    // absent-ok: with no home time zone set, times read in UTC.
    let tz = current.get("home_timezone").and_then(Value::as_str).and_then(|z| z.parse().ok()).unwrap_or(chrono_tz::UTC);
    let recent = recent_changes(pool, tz).await?;
    let now = chrono::Utc::now().with_timezone(&tz).format("%A %Y-%m-%d %H:%M");
    let user = format!("{}\n\nIt is now {now} for them.\n\nWhat they typed: {}", draft_prompt(&current, &recent), json!(text));
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

#[derive(Debug, Deserialize)]
pub struct ApplyChange {
    pub field: String,
    pub new_value: Value,
    /// What the draft said the setting held. When it no longer does, the
    /// draft is stale and applying it would overwrite a newer change.
    #[serde(default)]
    pub old_value: Option<Value>,
}

/// Save drafted changes the owner accepted, checked again, and record them
/// as the assistant's. Appearance changes are the page's to apply.
pub async fn apply_draft(pool: &PgPool, changes: Vec<ApplyChange>) -> Result<()> {
    let _saving = saving().await;
    let current = current_values(pool, &Value::Null).await?;
    let mut profile = Map::new();
    let mut assistant = Map::new();
    let mut switches: Vec<(&'static Field, bool)> = Vec::new();
    for c in changes {
        let Some(spec) = field(&c.field) else {
            return Err(Error::InvalidInput(format!("{} can't be changed from a draft.", c.field)));
        };
        if spec.store == Store::App {
            return Err(Error::InvalidInput(format!("{} is set in the app, not on your server.", c.field)));
        }
        if let Some(old) = &c.old_value {
            if current.get(spec.field).unwrap_or(&Value::Null) != old {
                return Err(Error::InvalidInput(
                    "A setting changed after this was drafted. Nothing was changed. Ask again.".into(),
                ));
            }
        }
        let Some(value) = stored_value(spec.field, &c.new_value, false) else {
            return Err(Error::InvalidInput(format!("{} can't take that value.", spec.field)));
        };
        match spec.store {
            Store::Profile => {
                profile.insert(spec.field.into(), value);
            }
            Store::Assistant => {
                assistant.insert(spec.field.into(), value);
            }
            Store::Capability | Store::Reading => switches.push((spec, value.as_bool().unwrap_or(false))),
            Store::App => unreachable!("refused above"),
        }
    }
    // The two profiles live in two tables. The profile goes first; if the
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
                    put_profile_back(pool, profile_before, fields).await;
                }
                return Err(e);
            }
        }
    }
    if let Some((before, after, _)) = &profile_saved {
        record_diff_by(pool, "assistant", before, after, PROFILE_FIELDS).await;
    }
    // Each switch is its own small write that records itself.
    for (spec, on) in switches {
        let id = spec.field.split_once('.').map(|(_, id)| id).unwrap_or(spec.field);
        let written = match spec.store {
            Store::Capability => crate::api::capabilities::set_by(pool, id, on, "assistant").await.map(|_| ()),
            _ => crate::api::model_access::set_by(pool, id, on, "assistant").await.map(|_| ()),
        };
        if let Err(e) = written {
            tracing::warn!(field = spec.field, error = %e, "apply draft: a switch failed after other changes saved");
            return Err(Error::Other(
                "Your server saved part of this but not every switch. Changed lately shows what saved.".into(),
            ));
        }
    }
    Ok(())
}

async fn put_profile_back(pool: &PgPool, before: &Value, fields: &[String]) {
    let restore: Map<String, Value> = fields
        .iter()
        .map(|f| {
            let v = before.get(f).cloned().unwrap_or(Value::Null);
            let v = if v.is_null() && PROFILE_TEXT_FIELDS.contains(&f.as_str()) { json!("") } else { v };
            (f.clone(), v)
        })
        .collect();
    if let Err(e) = async { crate::api::profile::update_profile(pool, serde_json::from_value(Value::Object(restore))?).await }.await {
        tracing::warn!("apply draft: couldn't put the profile half back: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_draft_keeps_only_listed_fields_and_valid_values() {
        let current = json!({ "style_notes": "No lists.", "preferred_name": "Nick", "request_day_limit_micros": 2_000_000 });
        let reply = r#"Here you go: {"changes": [
            {"field": "style_notes", "value": "No lists. Keep answers short."},
            {"field": "sidebar_doors", "value": "x"},
            {"field": "home_timezone", "value": "Mars/Olympus"},
            {"field": "request_day_limit_micros", "value": 5},
            {"field": "preferred_name", "value": "Nick"}
        ], "note": "Done."}"#;
        let d = validate_reply(reply, &current);
        assert_eq!(d.kind, "change");
        let fields: Vec<_> = d.changes.iter().map(|c| c.field).collect();
        assert_eq!(fields, vec!["style_notes", "request_day_limit_micros"]);
        assert_eq!(d.changes[1].new_value, json!(5_000_000));
        assert_eq!(d.changes[1].old_value, json!(2_000_000));
        assert_eq!(d.note.as_deref(), Some("Done."));
    }

    #[test]
    fn a_reply_with_no_json_or_nothing_is_none() {
        let d = validate_reply("I can't help with that.", &json!({}));
        assert_eq!(d.kind, "none");
        let d = validate_reply(r#"{"changes": [], "note": "Ask in a chat instead."}"#, &json!({}));
        assert_eq!(d.kind, "none");
        assert_eq!(d.note.as_deref(), Some("Ask in a chat instead."));
    }

    #[test]
    fn values_must_fit_their_field() {
        assert_eq!(stored_value("request_run_limit_micros", &json!(1.5), true), Some(json!(1_500_000)));
        assert_eq!(stored_value("request_run_limit_micros", &json!(-1), true), None);
        assert_eq!(stored_value("request_run_limit_micros", &json!(5000), true), None);
        assert_eq!(stored_value("request_run_limit_micros", &json!(1_500_000), false), Some(json!(1_500_000)));
        assert_eq!(stored_value("assistant_name", &json!("  "), true), None);
        assert_eq!(stored_value("assistant_name", &json!("x".repeat(101)), true), None);
        assert_eq!(stored_value("home_timezone", &json!("Europe/Lisbon"), true), Some(json!("Europe/Lisbon")));
        assert_eq!(stored_value("sidebar_doors", &json!("x"), true), None);
        assert_eq!(stored_value("theme", &json!("Dark"), true), Some(json!("asgard")), "the model may use the word");
        assert_eq!(stored_value("theme", &json!("Dark"), false), None, "an apply sends the stored value");
        assert_eq!(stored_value("theme", &json!("sepia"), true), None);
        assert_eq!(stored_value("keep_recordings_days", &json!(30), true), Some(json!(30)));
        assert_eq!(stored_value("keep_recordings_days", &json!(45), true), None);
        assert_eq!(stored_value("keep_recordings_days", &json!("forever"), true), Some(Value::Null));
        assert_eq!(stored_value("birth_date", &json!("1990-04-02"), true), Some(json!("1990-04-02")));
        assert_eq!(stored_value("birth_date", &json!("2990-04-02"), true), None);
        assert_eq!(stored_value("reading.health", &json!(false), true), Some(json!(false)));
        assert_eq!(stored_value("reading.health", &json!("off"), true), None);
    }

    #[test]
    fn the_last_change_to_a_field_wins() {
        let reply = r#"{"changes": [{"field": "assistant_name", "value": "Ida"}, {"field": "assistant_name", "value": "Juno"}]}"#;
        let d = validate_reply(reply, &json!({ "assistant_name": "Ari" }));
        assert_eq!(d.changes.len(), 1);
        assert_eq!(d.changes[0].new_value, json!("Juno"));
    }

    #[test]
    fn shorter_recordings_say_what_is_deleted_and_cannot_undo() {
        let reply = r#"{"changes": [{"field": "keep_recordings_days", "value": 30}]}"#;
        let d = validate_reply(reply, &json!({ "keep_recordings_days": null }));
        assert!(!d.changes[0].can_undo);
        assert!(d.changes[0].consequence.as_deref().is_some_and(|c| c.contains("30 days")));
        let longer = validate_reply(r#"{"changes": [{"field": "keep_recordings_days", "value": 365}]}"#, &json!({ "keep_recordings_days": 30 }));
        assert!(longer.changes[0].can_undo);
        assert_eq!(longer.changes[0].consequence, None);
    }

    #[test]
    fn appearance_changes_are_the_pages_to_apply() {
        let d = validate_reply(r#"{"changes": [{"field": "theme", "value": "asgard"}]}"#, &json!({ "theme": "oxford" }));
        assert!(d.changes[0].in_app);
        assert_eq!(d.changes[0].setting_id, "theme");
    }

    #[test]
    fn options_open_and_answer_each_make_their_own_card() {
        let current = json!({ "text_size": "m", "glass": "medium" });
        let d = validate_reply(
            r#"{"options": [{"field": "text_size", "value": "l"}, {"field": "glass", "value": "none"}, {"field": "glass", "value": "none"}]}"#,
            &current,
        );
        assert_eq!(d.kind, "choose");
        assert_eq!(d.options.len(), 2, "a repeated option is dropped");

        let one = validate_reply(r#"{"options": [{"field": "text_size", "value": "l"}]}"#, &current);
        assert_eq!(one.kind, "change", "a single option is a change");

        let open = validate_reply(r#"{"open": "keys"}"#, &current);
        assert_eq!(open.kind, "open");
        assert_eq!(open.open.as_ref().map(|o| o.setting_id), Some("keys"));
        assert_eq!(validate_reply(r#"{"open": "the-moon"}"#, &current).kind, "none");

        let answer = validate_reply(
            r#"{"answer": "You asked for short answers this morning.", "about": ["style_notes", "style_notes", "nonsense"]}"#,
            &current,
        );
        assert_eq!(answer.kind, "answer");
        assert_eq!(answer.about, vec!["style"]);
    }

    #[test]
    fn the_switch_fields_name_every_switch() {
        let capabilities: Vec<String> =
            crate::api::capabilities::capabilities().iter().map(|(id, _)| format!("capabilities.{id}")).collect();
        let reading: Vec<String> = crate::api::model_access::KINDS.iter().map(|k| format!("reading.{}", k.id)).collect();
        let listed = |store: Store| -> Vec<String> {
            FIELDS.iter().filter(|f| f.store == store).map(|f| f.field.to_string()).collect()
        };
        assert_eq!(listed(Store::Capability), capabilities);
        assert_eq!(listed(Store::Reading), reading);
    }
}
