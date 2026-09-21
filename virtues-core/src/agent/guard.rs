//! The repeated-failure guard: what stops a turn from asking a tool the same
//! failing question until the step ceiling ends it.
//!
//! The agent loop has a step ceiling (`AgentConfig::max_steps`, 20), so a
//! turn always ends. What nothing did was notice that it was ending badly: a
//! model handed `permission denied for table app_pages` would, on a bad day,
//! ask again with the same SQL, and again, and every round trip re-sent the
//! whole context and was billed. Every other agent harness has a guard here —
//! OpenCode interrupts on the third identical call, Claude Code caps the
//! tool loop — and this one had only the ceiling.
//!
//! Two rules, both per turn, both scoped to one tool:
//!
//! 1. **An identical call that already failed is refused.** Same tool, same
//!    arguments, and the earlier one failed: the answer will not differ, so
//!    the tool is not run. The model is told what the earlier failure said
//!    and that it has to change something.
//! 2. **Consecutive failures close the tool.** After [`MAX_CONSECUTIVE`]
//!    failures of one tool with no success of that tool between them, the
//!    tool is closed for the rest of the turn, and the model is told to
//!    answer with what it has and say what it could not get.
//!
//! Consecutive, not total — that distinction was measured, not chosen. On a
//! live box (2026-09-18) one turn made sixteen `sql_query` calls of which
//! four failed and twelve succeeded, interleaved, and the answer was right. A
//! guard counting total failures would have closed the tool on that turn's
//! third miss with the answer still ahead of it. A success resets the count;
//! only a run of failures is a loop.
//!
//! A refusal is delivered as an ordinary failed tool result: the model reads
//! it in the transcript, the person sees it as a failed line in the thinking
//! block, and the row keeps it like any other failure. It also counts as a
//! failure for rule 2, so an identical retry walks toward the close rather
//! than around it.

use std::collections::{HashMap, HashSet};

use super::executor::ToolExecutionResult;
use super::stream::ToolCall;
use crate::tools::ToolResult;

/// Failures of one tool in a row, with no success between, before the tool
/// is closed for the turn. Four rather than three because tool calls in one
/// step run in parallel and fail together: two grant errors landing in the
/// same second plus one bad guess is three, and that turn went on to answer.
pub const MAX_CONSECUTIVE: u32 = 4;

#[derive(Default)]
struct ToolLedger {
    /// Failures since the last success of this tool.
    consecutive_failures: u32,
    /// Argument fingerprints of every failed call, with the first line of
    /// what the failure said — for telling the model what it already knows.
    failed_args: HashMap<String, String>,
}

/// Per-turn memory of which tools failed and how. Create one per agent run.
#[derive(Default)]
pub struct RepeatGuard {
    tools: HashMap<String, ToolLedger>,
}

impl RepeatGuard {
    pub fn new() -> Self {
        Self::default()
    }

    /// Split a step's tool calls into the ones to run and the ones refused,
    /// each refusal already shaped as a failed result the loop can emit and
    /// record like any other.
    pub fn admit(&self, calls: &[ToolCall]) -> (Vec<ToolCall>, Vec<ToolExecutionResult>) {
        let mut run = Vec::with_capacity(calls.len());
        let mut refused = Vec::new();
        // Within one step the same call can appear twice (parallel calls
        // are the model's to shape); the ledger has not seen the first yet,
        // so track fingerprints seen in this step as well.
        let mut seen_this_step: HashSet<(String, String)> = HashSet::new();
        for call in calls {
            match self.verdict(call, &seen_this_step) {
                None => {
                    seen_this_step.insert((call.name.clone(), fingerprint(&call.arguments)));
                    run.push(call.clone());
                }
                Some(reason) => {
                    tracing::warn!(
                        tool_call_id = %call.id,
                        tool_name = %call.name,
                        reason = %reason,
                        "tool call refused by the repeated-failure guard"
                    );
                    refused.push(ToolExecutionResult {
                        tool_call_id: call.id.clone(),
                        tool_name: call.name.clone(),
                        result: Ok(ToolResult::error(reason)),
                    });
                }
            }
        }
        (run, refused)
    }

    fn verdict(&self, call: &ToolCall, seen_this_step: &HashSet<(String, String)>) -> Option<String> {
        let ledger = self.tools.get(&call.name);
        if let Some(l) = ledger {
            if l.consecutive_failures >= MAX_CONSECUTIVE {
                return Some(format!(
                    "{} has failed {} times in a row this turn and is closed for the rest of it. \
                     Answer with what you already have, and tell the person what you could not get.",
                    call.name, l.consecutive_failures
                ));
            }
        }
        let fp = fingerprint(&call.arguments);
        if let Some(said) = ledger.and_then(|l| l.failed_args.get(&fp)) {
            return Some(format!(
                "This exact {} call already failed this turn: {said} \
                 It will not answer differently. Change the call, or answer without it.",
                call.name
            ));
        }
        if seen_this_step.contains(&(call.name.clone(), fp)) {
            return Some(format!(
                "This exact {} call is already running in this step. One is enough.",
                call.name
            ));
        }
        None
    }

    /// Record every result of a step — the ones that ran and the ones refused.
    pub fn record(&mut self, calls: &[ToolCall], results: &[ToolExecutionResult]) {
        for r in results {
            let ledger = self.tools.entry(r.tool_name.clone()).or_default();
            if r.is_success() {
                ledger.consecutive_failures = 0;
                continue;
            }
            ledger.consecutive_failures += 1;
            if let Some(call) = calls.iter().find(|c| c.id == r.tool_call_id) {
                let said = first_line(&r.to_llm_content());
                ledger.failed_args.insert(fingerprint(&call.arguments), said);
            }
        }
    }
}

/// Canonical text of the arguments. `serde_json::Value` objects serialize in a
/// stable key order (sorted, or insertion order preserved — either way the
/// same call from the same model comes out the same), so equal text is the
/// same call.
fn fingerprint(arguments: &serde_json::Value) -> String {
    serde_json::to_string(arguments).unwrap_or_default()
}

/// The part of a failure worth repeating back: the message, not the column
/// dump sql_query appends for the model's own retry.
fn first_line(text: &str) -> String {
    let line = text.lines().next().unwrap_or("").trim();
    let line = line
        .strip_prefix("Tool execution failed:")
        .map(str::trim)
        .unwrap_or(line);
    let mut line: String = line.chars().take(200).collect();
    if line.chars().count() == 200 && line.len() < text.lines().next().unwrap_or("").len() {
        line.push('…');
    }
    if !line.ends_with('.') {
        line.push('.');
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::executor::ToolExecutionError;

    fn call(id: &str, name: &str, args: serde_json::Value) -> ToolCall {
        ToolCall { id: id.into(), name: name.into(), arguments: args }
    }

    fn failed(call: &ToolCall, why: &str) -> ToolExecutionResult {
        ToolExecutionResult {
            tool_call_id: call.id.clone(),
            tool_name: call.name.clone(),
            result: Ok(ToolResult::error(why)),
        }
    }

    fn ok(call: &ToolCall) -> ToolExecutionResult {
        ToolExecutionResult {
            tool_call_id: call.id.clone(),
            tool_name: call.name.clone(),
            result: Ok(ToolResult::success(serde_json::json!({"rows": []}))),
        }
    }

    fn sql(id: &str, s: &str) -> ToolCall {
        call(id, "sql_query", serde_json::json!({"operation": "query", "sql": s}))
    }

    #[test]
    fn an_identical_call_that_failed_is_refused_and_told_why() {
        let mut g = RepeatGuard::new();
        let first = sql("1", "SELECT day FROM wiki_day_prose");
        let (run, refused) = g.admit(std::slice::from_ref(&first));
        assert_eq!(run.len(), 1);
        assert!(refused.is_empty());
        g.record(
            &run,
            &[failed(&first, "Execution failed: Query failed: column \"day\" does not exist\nwiki_day_prose has: day_id, date, prose")],
        );

        let again = sql("2", "SELECT day FROM wiki_day_prose");
        let (run, refused) = g.admit(std::slice::from_ref(&again));
        assert!(run.is_empty());
        assert_eq!(refused.len(), 1);
        let said = refused[0].to_llm_content();
        assert!(said.contains("already failed this turn"), "{said}");
        assert!(said.contains("column \"day\" does not exist"), "{said}");
        assert!(!said.contains("wiki_day_prose has:"), "column dump leaked: {said}");
    }

    #[test]
    fn a_changed_call_runs() {
        let mut g = RepeatGuard::new();
        let first = sql("1", "SELECT day FROM wiki_day_prose");
        g.record(std::slice::from_ref(&first), &[failed(&first, "nope")]);
        let fixed = sql("2", "SELECT date FROM wiki_day_prose");
        let (run, refused) = g.admit(std::slice::from_ref(&fixed));
        assert_eq!(run.len(), 1);
        assert!(refused.is_empty());
    }

    #[test]
    fn a_run_of_failures_closes_the_tool_and_a_success_resets_the_run() {
        let mut g = RepeatGuard::new();
        for i in 0..MAX_CONSECUTIVE {
            let c = sql(&i.to_string(), &format!("SELECT bad{i} FROM wiki_days"));
            let (run, _) = g.admit(std::slice::from_ref(&c));
            assert_eq!(run.len(), 1, "call {i} should still run");
            g.record(&run, &[failed(&c, "column does not exist")]);
        }
        let next = sql("x", "SELECT date FROM wiki_days");
        let (run, refused) = g.admit(std::slice::from_ref(&next));
        assert!(run.is_empty(), "the tool should be closed");
        assert!(refused[0].to_llm_content().contains("closed for the rest of it"));

        // Another tool is untouched.
        let other = call("y", "semantic_search", serde_json::json!({"queries": ["x"]}));
        let (run, _) = g.admit(std::slice::from_ref(&other));
        assert_eq!(run.len(), 1);

        // And a fresh guard where a success lands mid-run never closes: the
        // sixteen-call turn from the live box.
        let mut g = RepeatGuard::new();
        for i in 0..(MAX_CONSECUTIVE * 3) {
            let c = sql(&i.to_string(), &format!("SELECT q{i} FROM wiki_days"));
            let (run, refused) = g.admit(std::slice::from_ref(&c));
            assert!(refused.is_empty(), "call {i} was refused");
            let r = if i % 3 == 2 { ok(&c) } else { failed(&c, "miss") };
            g.record(&run, &[r]);
        }
    }

    #[test]
    fn a_refusal_counts_toward_the_close() {
        let mut g = RepeatGuard::new();
        let c = sql("1", "SELECT nope FROM wiki_days");
        g.record(std::slice::from_ref(&c), &[failed(&c, "miss")]);
        for i in 1..MAX_CONSECUTIVE {
            let again = sql(&(i + 1).to_string(), "SELECT nope FROM wiki_days");
            let (run, refused) = g.admit(std::slice::from_ref(&again));
            assert!(run.is_empty());
            g.record(std::slice::from_ref(&again), &refused);
        }
        let different = sql("z", "SELECT date FROM wiki_days");
        let (run, refused) = g.admit(std::slice::from_ref(&different));
        assert!(run.is_empty(), "identical retries walked the tool to its close");
        assert!(refused[0].to_llm_content().contains("closed"));
    }

    #[test]
    fn the_same_call_twice_in_one_step_runs_once() {
        let g = RepeatGuard::new();
        let a = sql("1", "SELECT date FROM wiki_days");
        let b = sql("2", "SELECT date FROM wiki_days");
        let (run, refused) = g.admit(&[a, b]);
        assert_eq!(run.len(), 1);
        assert_eq!(refused.len(), 1);
        assert_eq!(refused[0].tool_call_id, "2");
    }

    #[test]
    fn an_executor_error_counts_as_a_failure_too() {
        let mut g = RepeatGuard::new();
        let c = sql("1", "SELECT 1");
        let r = ToolExecutionResult {
            tool_call_id: c.id.clone(),
            tool_name: c.name.clone(),
            result: Err(ToolExecutionError::Timeout(std::time::Duration::from_secs(30))),
        };
        g.record(std::slice::from_ref(&c), &[r]);
        let (run, refused) = g.admit(std::slice::from_ref(&sql("2", "SELECT 1")));
        assert!(run.is_empty());
        assert!(refused[0].to_llm_content().contains("already failed"));
    }
}
