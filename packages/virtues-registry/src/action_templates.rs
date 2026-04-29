//! Action template registry — blueprints for creating scheduled actions.
//!
//! Templates are compile-time constants. They define what an action does,
//! when it should run, and what context it needs. Users activate templates
//! to create personal action instances they can customize.
//!
//! Templates are NOT stored in SQLite — they live in code and are versioned
//! with the application. User-created actions (from templates) are stored
//! in app_actions.

use serde::Serialize;

/// Category for organizing templates in the UI
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TemplateCategory {
    /// Daily reflection, journaling, examen
    Reflection,
    /// Data analysis, pattern recognition
    Analysis,
    /// External data fetching, monitoring
    Monitoring,
    /// System maintenance
    System,
}

/// An action template — a blueprint for creating a user action.
#[derive(Debug, Clone, Serialize)]
pub struct ActionTemplate {
    /// Unique template ID (e.g., "morning_examen")
    pub id: &'static str,
    /// Human-readable name
    pub name: &'static str,
    /// One-line description
    pub description: &'static str,
    /// Longer explanation of what this template does and why
    pub long_description: &'static str,
    /// Category for UI grouping
    pub category: TemplateCategory,
    /// Icon (Iconify name)
    pub icon: &'static str,
    /// The action instruction text (may contain {placeholders} for user customization)
    pub instruction: &'static str,
    /// Suggested cron schedule (6-field)
    pub default_schedule: &'static str,
    /// Human-readable schedule description
    pub schedule_description: &'static str,
    /// Optional activation gate (Python code)
    pub activation_code: Option<&'static str>,
    /// What tools this action uses (informational, not enforced)
    pub tools_used: &'static [&'static str],
    /// Sort order for display
    pub sort_order: i32,
}

/// Get all registered action templates.
pub fn registered_action_templates() -> Vec<ActionTemplate> {
    vec![
        morning_examen(),
    ]
}

/// Get a template by ID.
pub fn get_action_template(id: &str) -> Option<ActionTemplate> {
    registered_action_templates().into_iter().find(|t| t.id == id)
}

// ============================================================================
// Template Definitions
// ============================================================================

fn morning_examen() -> ActionTemplate {
    ActionTemplate {
        id: "morning_examen",
        name: "Morning Examen",
        description: "Structured morning reflection on the day ahead",
        long_description: "A daily morning practice inspired by the Ignatian Examen. Reviews what happened yesterday, examines the day ahead (calendar, commitments), connects to your narrative identity and aspirations, and creates a structured reflection page for the day.",
        category: TemplateCategory::Reflection,
        icon: "ri:sun-line",
        instruction: MORNING_EXAMEN_INSTRUCTION,
        default_schedule: "0 0 7 * * *", // 7am UTC (user should adjust to local)
        schedule_description: "Daily at 7:00 AM",
        activation_code: Some(MORNING_EXAMEN_ACTIVATION),
        tools_used: &["sql_query", "create_page", "semantic_search"],
        sort_order: 1,
    }
}

const MORNING_EXAMEN_INSTRUCTION: &str = r#"You are running the Morning Examen — a structured daily reflection to prepare the user for their day.

## Your Task

Gather context about yesterday, the recent narrative arc, and the day ahead. Then create a reflection page for today using the create_page tool, linked to today's date.

## Step 1: Gather Context (sql_query)

Run these queries to build a picture:

a) **Yesterday's autobiography + recent arc** (last 14 days of day summaries for narrative context, full detail for last 3 days):
   - SELECT date, autobiography FROM wiki_days WHERE date >= date('now', '-14 days') AND date < date('now') ORDER BY date DESC
   - SELECT date, autobiography, epigraph, data_quality FROM wiki_days WHERE date >= date('now', '-3 days') AND date < date('now') ORDER BY date DESC

b) **Yesterday's events** (what actually happened, with emotional/novelty signal):
   - SELECT auto_label, event_summary, novelty_z, autonomic_z, start_time, end_time FROM wiki_events e JOIN wiki_days d ON e.day_id = d.id WHERE d.date = date('now', '-1 day') ORDER BY start_time

c) **Recent journal entries / reflections** (the user's own writing from recent days):
   - SELECT title, content, date FROM app_pages WHERE date >= date('now', '-7 days') AND date IS NOT NULL ORDER BY date DESC LIMIT 10

d) **Today's calendar** (what's coming):
   - SELECT title, start_time, end_time, location FROM data_calendar_event WHERE date(start_time) = date('now') ORDER BY start_time

e) **Narrative identity** (who the user is becoming):
   - SELECT content FROM wiki_telos LIMIT 1

## Step 2: Find an Epigraph (semantic_search or web_search)

Find a short quote or passage to anchor the reflection. In order of preference:
1. A line from the user's OWN past journal entries or reflections that resonates with today's themes
2. A quote from a thinker the user resonates with (check your memory for preferences — e.g., CS Lewis, Chesterton, Austen, Seneca)
3. The daily gospel reading (web_search "USCCB daily reading [today's date]") if the user has indicated a Catholic/Christian practice in memory
4. A relevant passage found via semantic_search on the user's own data

The epigraph should be 1-2 sentences max. Attributed.

## Step 3: Create the Page (create_page)

Use create_page with date set to today (YYYY-MM-DD format).
Title: "Morning — [today's date formatted as Month Day, Year]"

Structure the content as markdown:

---

> [The epigraph quote, attributed]

## Looking Back
2-3 sentences on yesterday. What happened, what carried over, what was resolved. If yesterday's events include any with high novelty (z > 1.0) or notable autonomic signal, weave that in naturally — "the house showing was the most charged moment of the day" — without citing numbers.

If the user wrote their own reflections recently, acknowledge them: "You wrote about X on Tuesday..."

## The Day Ahead
Today's calendar events with times. For each, a brief note if it connects to ongoing themes or yesterday's unfinished business. If no events, note the open day.

End with a question — pick the event or theme that seems most significant and ask the user something specific about it. Not "how do you feel?" but "the meeting with David at 2pm — is this where you'll raise the hiring question from last week?"

## Intention
*What is your one intention for today?*

## Notes
*(space for your thoughts)*

---

## Step 4: Update Memory (update_action_memory)

After creating the page, update your memory with anything you learned:
- New preferences or patterns you noticed
- Themes that are emerging across days
- Which quote sources the user might prefer (note for next time)
- Anything from recent journal entries that reveals ongoing concerns or aspirations

Your memory is markdown. Read it at the start of each run (it's in the <memory> block above). Append new observations, prune stale ones. Keep it under 2000 characters.

## Guidelines

- Be concrete and specific. Name people, places, events from the data.
- The tone is a thoughtful friend reviewing the day with you — warm but grounded, never saccharine.
- The question at the end of "The Day Ahead" is important — it invites the user to engage, not just consume.
- If data is sparse (no autobiography, no events), keep it short. A sparse day gets a brief page, not a padded one.
- Keep the total page under 600 words (excluding the empty Intention/Notes sections).
- Never fabricate events or details not present in the data."#;

const MORNING_EXAMEN_ACTIVATION: &str = r#"import sqlite3, os, datetime, zoneinfo
db = sqlite3.connect(os.environ.get('DB_PATH', 'virtues.db'))
row = db.execute("SELECT timezone FROM app_user_profile LIMIT 1").fetchone()
if not row or not row[0]:
    print("")
else:
    try:
        tz = zoneinfo.ZoneInfo(row[0])
    except Exception:
        tz = datetime.timezone.utc
    local_now = datetime.datetime.now(tz)
    # Run between 6-9am local time
    if 6 <= local_now.hour <= 9:
        # Check we haven't already created a reflection for today
        today = local_now.strftime("%Y-%m-%d")
        existing = db.execute("SELECT COUNT(*) FROM app_pages WHERE date = ? AND title LIKE 'Morning%'", (today,)).fetchone()[0]
        if existing == 0:
            print(f"morning: {today}")
        else:
            print("")
    else:
        print("")
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registered_templates() {
        let templates = registered_action_templates();
        assert!(!templates.is_empty());

        for t in &templates {
            assert!(!t.id.is_empty());
            assert!(!t.name.is_empty());
            assert!(!t.instruction.is_empty());
            assert!(!t.default_schedule.is_empty());
        }
    }

    #[test]
    fn test_get_template() {
        assert!(get_action_template("morning_examen").is_some());
        assert!(get_action_template("nonexistent").is_none());
    }
}
