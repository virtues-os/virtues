//! Per-turn tool budgets: a tool may be called at most N times in one turn.
//!
//! Plain chat caps `web_search` (see `CHAT_TOOL_CAPS` in api/chat.rs). A call
//! past the cap is not run; it comes back as a failed result telling the model
//! to answer with what it has, the same shape the repeat guard uses.
//!
//! Refused rather than dropped from the next request's tool list, as the AI
//! SDK's `prepareStep` examples do: a conversation holding tool calls must
//! still declare those tools, and the tool list is the front of the
//! provider's cache key, so changing it mid-turn re-bills the whole prefix.

use std::collections::HashMap;

use super::executor::ToolExecutionResult;
use super::stream::ToolCall;
use crate::tools::ToolResult;

/// Calls admitted so far this turn, against the turn's caps. One per run.
#[derive(Default)]
pub struct ToolCaps {
    caps: &'static [(&'static str, u32)],
    used: HashMap<&'static str, u32>,
}

impl ToolCaps {
    pub fn new(caps: &'static [(&'static str, u32)]) -> Self {
        Self { caps, used: HashMap::new() }
    }

    /// Split a step's calls into the ones within budget and the ones over it,
    /// each over-budget call already shaped as a failed result. Within one
    /// parallel batch, calls are admitted in the order the model sent them.
    pub fn admit(&mut self, calls: Vec<ToolCall>) -> (Vec<ToolCall>, Vec<ToolExecutionResult>) {
        let mut run = Vec::with_capacity(calls.len());
        let mut over = Vec::new();
        for call in calls {
            let Some(&(name, cap)) = self.caps.iter().find(|(n, _)| *n == call.name) else {
                run.push(call);
                continue;
            };
            let used = self.used.entry(name).or_default();
            if *used < cap {
                *used += 1;
                run.push(call);
                continue;
            }
            tracing::info!(tool_name = %call.name, cap, "tool call over the turn's cap");
            over.push(ToolExecutionResult {
                result: Ok(ToolResult::error(format!(
                    "The {name} budget for this turn is spent ({cap} calls). Answer now with \
                     what you have, and name anything you could not confirm."
                ))),
                tool_call_id: call.id,
                tool_name: call.name,
            });
        }
        (run, over)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(id: &str, name: &str) -> ToolCall {
        ToolCall { id: id.into(), name: name.into(), arguments: serde_json::json!({"query": id}) }
    }

    #[test]
    fn a_capped_tool_stops_at_its_cap_across_steps_and_within_one() {
        static CAPS: &[(&str, u32)] = &[("web_search", 3)];
        let mut caps = ToolCaps::new(CAPS);

        let (run, over) = caps.admit(vec![call("1", "web_search"), call("2", "web_search")]);
        assert_eq!(run.len(), 2);
        assert!(over.is_empty());

        // The next batch of two: one fits, one is over, in the order sent.
        let (run, over) = caps.admit(vec![call("3", "web_search"), call("4", "web_search")]);
        assert_eq!(run[0].id, "3");
        assert_eq!(over.len(), 1);
        assert_eq!(over[0].tool_call_id, "4");
        let said = over[0].to_llm_content();
        assert!(said.contains("budget for this turn is spent"), "{said}");

        // An uncapped tool is untouched.
        let (run, over) = caps.admit(vec![call("5", "sql_query")]);
        assert_eq!(run.len(), 1);
        assert!(over.is_empty());
    }

    #[test]
    fn no_caps_admits_everything() {
        let mut caps = ToolCaps::default();
        let (run, over) = caps.admit((0..10).map(|i| call(&i.to_string(), "web_search")).collect());
        assert_eq!(run.len(), 10);
        assert!(over.is_empty());
    }
}
