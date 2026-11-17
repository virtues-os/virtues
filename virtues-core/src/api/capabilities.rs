//! What the assistant can do, as the owner switches it in Settings ("What
//! {name} can do").
//!
//! Each capability covers one or more tools. The switches are stored in the
//! assistant profile's `enabled_tools`, keyed by tool id. A tool is off only
//! when its key is `false`, so a tool added later is on until someone turns
//! it off, and a profile written before these switches existed changes
//! nothing.
//!
//! Two places enforce a switch. The chat leaves an off tool out of the tools
//! it offers the model ([`without_off`]), so the model does not try it. The
//! tool executor refuses one a model names anyway ([`off_capability`]),
//! which also covers applet runs, subagents and `/api/mcp`.

use std::collections::HashSet;

use serde::Serialize;
use serde_json::{Map, Value};
use sqlx::PgPool;

use crate::error::{Error, Result};

/// Settings' switches, in the order the page lists them: (id, tools).
pub fn capabilities() -> Vec<(&'static str, Vec<&'static str>)> {
    let mut browser = vec![crate::browse::TOOL];
    browser.extend_from_slice(crate::browser::TOOLS);
    vec![
        ("web_search", vec!["web_search"]),
        ("browser", browser),
        ("applets", vec!["applet"]),
        ("pages", vec!["create_page", "edit_page"]),
    ]
}

/// One switch as Settings shows it.
#[derive(Debug, Serialize, PartialEq)]
pub struct CapabilityState {
    pub id: &'static str,
    pub on: bool,
}

async fn enabled_tools(pool: &PgPool) -> Result<Value> {
    let (value,): (Value,) =
        sqlx::query_as("SELECT enabled_tools FROM app_assistant_profile LIMIT 1")
            .fetch_one(pool)
            .await
            .map_err(|e| Error::Database(format!("reading what the assistant can do: {e}")))?;
    Ok(value)
}

/// Tool ids the owner turned off.
fn off_tools(enabled: &Value) -> HashSet<String> {
    enabled
        .as_object()
        .map(|m| {
            m.iter()
                .filter(|(_, v)| v.as_bool() == Some(false))
                .map(|(k, _)| k.clone())
                .collect()
        })
        .unwrap_or_default()
}

/// A capability is on unless any of its tools is off.
fn states(enabled: &Value) -> Vec<CapabilityState> {
    let off = off_tools(enabled);
    capabilities()
        .into_iter()
        .map(|(id, tools)| CapabilityState {
            id,
            on: !tools.iter().any(|t| off.contains(*t)),
        })
        .collect()
}

pub async fn list(pool: &PgPool) -> Result<Vec<CapabilityState>> {
    Ok(states(&enabled_tools(pool).await?))
}

/// Turns a capability on or off. Returns every switch.
pub async fn set(pool: &PgPool, id: &str, on: bool) -> Result<Vec<CapabilityState>> {
    let tools = capabilities()
        .into_iter()
        .find(|(c, _)| *c == id)
        .map(|(_, tools)| tools)
        .ok_or_else(|| Error::InvalidInput(format!("no capability named {id}")))?;
    let mut switches = Map::new();
    for tool in tools {
        switches.insert(tool.to_string(), Value::Bool(on));
    }
    let after = apply(pool, &Value::Object(switches)).await?;
    Ok(states(&after))
}

/// Writes these tool switches over the stored ones and records the change
/// for Changed lately with only those tools in it (field `enabled_tools`),
/// so undoing it later puts back these switches and leaves the others as
/// they are then. Settings' switches and Undo both come through here.
pub async fn apply(pool: &PgPool, switches: &Value) -> Result<Value> {
    let before = enabled_tools(pool).await?;
    let after = merged(&before, switches);
    let request: crate::api::assistant_profile::UpdateAssistantProfileRequest =
        serde_json::from_value(serde_json::json!({ "enabled_tools": after }))?;
    crate::api::assistant_profile::update_assistant_profile(pool, request).await?;
    let (old, new) = (subset(&before, switches), subset(&after, switches));
    if old != new {
        crate::api::settings_changes::record(pool, "capabilities", "enabled_tools", &old, &new)
            .await;
    }
    Ok(after)
}

/// `stored` with `switches`' keys written over it.
fn merged(stored: &Value, switches: &Value) -> Value {
    let mut map = stored.as_object().cloned().unwrap_or_default();
    if let Some(changes) = switches.as_object() {
        for (tool, on) in changes {
            map.insert(tool.clone(), on.clone());
        }
    }
    Value::Object(map)
}

/// `stored`'s value for each of `switches`' tools, a missing key read as on.
fn subset(stored: &Value, switches: &Value) -> Value {
    let keys = switches
        .as_object()
        .map(|m| m.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    Value::Object(
        keys.into_iter()
            .map(|k| {
                let v = stored.get(&k).cloned().unwrap_or(Value::Bool(true));
                (k, v)
            })
            .collect(),
    )
}

/// The capability that turns `tool` off, when the owner has turned it off.
/// Reads the profile only for tools a switch covers.
pub async fn off_capability(pool: &PgPool, tool: &str) -> Result<Option<&'static str>> {
    let Some((id, _)) = capabilities()
        .into_iter()
        .find(|(_, tools)| tools.contains(&tool))
    else {
        return Ok(None);
    };
    let off = off_tools(&enabled_tools(pool).await?);
    Ok(off.contains(tool).then_some(id))
}

/// `tools` (LLM function definitions) without the ones the owner turned off.
pub async fn without_off(pool: &PgPool, tools: Vec<Value>) -> Result<Vec<Value>> {
    let off = off_tools(&enabled_tools(pool).await?);
    if off.is_empty() {
        return Ok(tools);
    }
    Ok(tools
        .into_iter()
        .filter(|t| {
            t["function"]["name"]
                .as_str()
                .is_none_or(|name| !off.contains(name))
        })
        .collect())
}

/// What the executor says to a model that named an off tool.
pub fn refusal(capability: &str) -> String {
    let what = match capability {
        "web_search" => "searching the web",
        "browser" => "using the browser",
        "applets" => "making and changing applets",
        "pages" => "writing pages",
        other => other,
    };
    format!("The owner turned off {what} in Settings. Say so and answer without it.")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_missing_key_is_on_and_only_false_is_off() {
        let s = states(&json!({ "web_search": true, "create_page": false }));
        assert!(s.iter().find(|c| c.id == "web_search").unwrap().on);
        assert!(
            s.iter().find(|c| c.id == "browser").unwrap().on,
            "browser tools have no keys yet"
        );
        assert!(
            !s.iter().find(|c| c.id == "pages").unwrap().on,
            "one off tool turns the capability off"
        );
        assert!(states(&Value::Null).iter().all(|c| c.on));
    }

    #[test]
    fn a_change_writes_only_its_tools_and_records_only_them() {
        let stored = json!({ "think": true, "web_search": false });
        let switches = json!({ "create_page": false, "edit_page": false });
        assert_eq!(
            merged(&stored, &switches),
            json!({ "think": true, "web_search": false, "create_page": false, "edit_page": false })
        );
        assert_eq!(
            subset(&stored, &switches),
            json!({ "create_page": true, "edit_page": true }),
            "a missing key reads as on"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn an_off_capability_is_refused_left_out_and_recorded(pool: PgPool) {
        assert_eq!(off_capability(&pool, "web_search").await.unwrap(), None);

        let states = set(&pool, "web_search", false).await.unwrap();
        assert!(!states.iter().find(|c| c.id == "web_search").unwrap().on);
        assert_eq!(
            off_capability(&pool, "web_search").await.unwrap(),
            Some("web_search")
        );
        assert_eq!(
            off_capability(&pool, "sql_query").await.unwrap(),
            None,
            "a tool no switch covers is never off"
        );

        let offered = without_off(
            &pool,
            vec![
                json!({ "function": { "name": "web_search" } }),
                json!({ "function": { "name": "sql_query" } }),
            ],
        )
        .await
        .unwrap();
        assert_eq!(
            offered,
            vec![json!({ "function": { "name": "sql_query" } })]
        );

        let recorded: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM app_settings_changes WHERE setting_id = 'capabilities'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(recorded, 1);

        // Pages goes off after web search; undoing the web search change
        // turns web search back on and leaves pages off.
        set(&pool, "pages", false).await.unwrap();
        let first: i64 = sqlx::query_scalar(
            "SELECT id FROM app_settings_changes WHERE setting_id = 'capabilities' ORDER BY id LIMIT 1",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        crate::api::settings_changes::undo(&pool, first)
            .await
            .unwrap();
        assert_eq!(off_capability(&pool, "web_search").await.unwrap(), None);
        assert_eq!(
            off_capability(&pool, "edit_page").await.unwrap(),
            Some("pages")
        );
    }

    #[test]
    fn every_capability_names_registry_tools() {
        let mut known: HashSet<String> = virtues_registry::tools::default_tools()
            .into_iter()
            .map(|t| t.id)
            .collect();
        known.insert(crate::browse::TOOL.to_string());
        for (id, tools) in capabilities() {
            for tool in tools {
                assert!(
                    known.contains(tool),
                    "{id} names {tool}, which no tool list has"
                );
            }
        }
    }
}
