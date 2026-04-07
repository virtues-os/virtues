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

Create a reflection page for today using the create_page tool. The page should be linked to today's date.

## Steps

1. **Gather context** using sql_query:
   - Yesterday's autobiography from wiki_days (SELECT autobiography FROM wiki_days WHERE date = date('now', '-1 day'))
   - Today's calendar events (SELECT title, start_time, end_time, location FROM data_calendar_event WHERE date(start_time) = date('now') ORDER BY start_time)
   - The user's narrative identity (SELECT content FROM wiki_telos LIMIT 1)
   - Recent themes from the last 3 days of autobiographies

2. **Find a reflection** using semantic_search:
   - Search for a quote or passage relevant to today's challenges or themes
   - Draw from the user's own past reflections if possible

3. **Create the reflection page** using create_page with date set to today (YYYY-MM-DD format):
   - Title: "Morning — [today's date formatted as Month Day, Year]"
   - Structure the content as markdown with these sections:

## Yesterday
A 2-3 sentence summary of what happened yesterday, drawn from the autobiography. What carried over? What was resolved?

## The Day Ahead
List today's calendar events with times. Note any that seem significant, challenging, or connected to ongoing themes.

## A Thought
The reflection quote or insight you found — attributed, with a sentence connecting it to today.

## Intention
Leave this section empty with a prompt: "What is your one intention for today?"

## Notes
Leave empty — space for the user to write freely.

---

Guidelines:
- Be concrete and specific, not generic or inspirational
- Reference actual events, people, and places from the data
- If yesterday's data is sparse, acknowledge it briefly and focus on today
- If no calendar events exist, note the open day and what that might mean
- The tone should be warm but grounded — a thoughtful friend reviewing the day with you, not a motivational poster
- Keep the total page under 500 words (excluding the empty sections)"#;

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
