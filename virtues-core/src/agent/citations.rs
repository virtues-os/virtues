//! Citations the model can copy, and the routes they stand for.
//!
//! A tool result's `ref` is a route such as `/record/communication_message/<uuid>`
//! or `/chat/<chat>/tool/<call>`, and the model cites a claim by linking to it.
//! Copying forty opaque characters exactly is the hard part of that for a model:
//! measured on 2026-10-22, deepseek-v4.1-flash-1017 broke 52 of 55 such links on
//! a real box (a dropped leading slash, a bare call id, `https://chat/…`), where
//! Sonnet 5.5 broke none.
//!
//! So the model is shown each route as a short handle (`r1`, `r2`, …), cites the
//! handle, and the reply is rewritten to the route before anyone sees it. Over the
//! same real tool outputs, 1017's links went from 75% to 100% valid as written.
//! Handles exist for one turn: replayed history shows the routes the turn stored,
//! and the rewriter accepts those as well.
//!
//! The rewriter also repairs a link that is nearly right (a route missing its
//! slash, one with a scheme in front, a bare row id) when exactly one route this
//! turn could see matches it, and turns a link it cannot place into plain text,
//! so a citation never renders as a link to nowhere.

use std::collections::{HashMap, HashSet};

use serde_json::Value;

/// The first segment of a route that only a tool result can produce. A link
/// under one of these that no tool returned is unlinked. Every other app route
/// (`/person/…`, `/page/…`) can also come from the system prompt, so it passes
/// through as written, as it did before handles.
const TOOL_ONLY_ROOTS: [&str; 2] = ["record", "chat"];

/// Every first segment a route on this box can have. A `https://` link whose
/// host is one of these is a route with a scheme in front of it, not a website.
const ROUTE_ROOTS: [&str; 13] = [
    "record", "chat", "page", "day", "year", "person", "place", "org", "drive", "project", "notebook", "source", "sources",
];

/// The handles of one turn and the routes the model could see.
#[derive(Debug, Default)]
pub struct Citations {
    by_handle: HashMap<String, String>,
    by_route: HashMap<String, String>,
    /// Routes the model saw as routes, in replayed history.
    seen: HashSet<String>,
}

impl Citations {
    /// Start a turn from the request as it will be sent: routes in replayed
    /// tool results, and links in earlier replies (stored with their routes),
    /// are citable as written. A replayed result is clipped, so a reply's own
    /// links are the only record of some of them.
    pub fn from_messages(messages: &[Value]) -> Self {
        let mut cites = Self::default();
        for message in messages {
            let Some(text) = message["content"].as_str() else { continue };
            match message["role"].as_str() {
                Some("tool") => cites.seen.extend(routes_in(text)),
                Some("assistant") => cites.seen.extend(link_targets(text)),
                _ => {}
            }
        }
        cites
    }

    /// `data` with every `ref` route replaced by its handle, as the model will
    /// read it. The client and the stored row keep the routes.
    pub fn shorten(&mut self, data: &Value) -> Value {
        match data {
            Value::Object(map) => Value::Object(
                map.iter()
                    .map(|(key, value)| match (key.as_str(), value) {
                        ("ref", Value::String(route)) => (key.clone(), Value::String(self.handle_for(route))),
                        _ => (key.clone(), self.shorten(value)),
                    })
                    .collect(),
            ),
            Value::Array(items) => Value::Array(items.iter().map(|v| self.shorten(v)).collect()),
            other => other.clone(),
        }
    }

    fn handle_for(&mut self, route: &str) -> String {
        if let Some(handle) = self.by_route.get(route) {
            return handle.clone();
        }
        let handle = format!("r{}", self.by_handle.len() + 1);
        self.by_handle.insert(handle.clone(), route.to_string());
        self.by_route.insert(route.to_string(), handle.clone());
        handle
    }

    /// What a link target the model wrote should point at: `Some` with the
    /// route (or the target itself, for a link that was never ours to check),
    /// or `None` to drop the link and keep its text.
    pub fn resolve(&self, target: &str) -> Option<String> {
        let t = target.trim().trim_matches(|c| matches!(c, '<' | '>' | '"' | '\'' | '`')).trim();
        if t.is_empty() {
            return None;
        }
        if let Some(route) = self.by_handle.get(t) {
            return Some(route.clone());
        }
        if self.is_known(t) {
            return Some(t.to_string());
        }
        // A website stays a website, unless its path is a route this turn
        // returned behind a host the model made up (`https://the-record/…`).
        if let Some(rest) = t.strip_prefix("https://").or_else(|| t.strip_prefix("http://")) {
            let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
            let path = format!("/{path}");
            if self.is_known(&path) {
                return Some(path);
            }
            if !ROUTE_ROOTS.contains(&host) {
                return Some(t.to_string());
            }
        }
        // A route the system prompt could have shown, written correctly.
        if let Some(root) = t.strip_prefix('/').and_then(|r| r.split('/').next()) {
            if ROUTE_ROOTS.contains(&root) && !TOOL_ONLY_ROOTS.contains(&root) {
                return Some(t.to_string());
            }
        }
        self.repair(t)
    }

    fn is_known(&self, route: &str) -> bool {
        self.by_route.contains_key(route) || self.seen.contains(route)
    }

    /// The one route this turn could see that a nearly-right target means.
    fn repair(&self, target: &str) -> Option<String> {
        let mut t = target.trim_start_matches("https://").trim_start_matches("http://");
        // "the query ref, /chat/…": the route is the last thing said.
        if let Some((_, last)) = t.rsplit_once(',') {
            t = last.trim();
        }
        if let Some(route) = self.by_handle.get(t) {
            return Some(route.clone());
        }
        let slashed = format!("/{t}");
        if self.is_known(&slashed) {
            return Some(slashed);
        }
        if self.is_known(t) {
            return Some(t.to_string());
        }
        // A tail the model kept: a row id, a call id, `tool/call_…`.
        let tail = format!("/{}", t.trim_start_matches('/'));
        let mut matches = self
            .by_route
            .keys()
            .chain(self.seen.iter())
            .filter(|route| route.ends_with(&tail));
        match (matches.next(), matches.next()) {
            (Some(only), None) => Some(only.clone()),
            _ => None,
        }
    }
}

/// The `ref` values in a tool result as the model read it (JSON text, possibly
/// clipped).
fn routes_in(text: &str) -> impl Iterator<Item = String> + '_ {
    text.match_indices("\"ref\":").filter_map(move |(at, key)| {
        let rest = text[at + key.len()..].trim_start().strip_prefix('"')?;
        let end = rest.find('"')?;
        let route = &rest[..end];
        route.starts_with('/').then(|| route.to_string())
    })
}

/// The route targets of markdown links in a reply.
fn link_targets(text: &str) -> impl Iterator<Item = String> + '_ {
    text.match_indices("](/").filter_map(move |(at, _)| {
        let rest = &text[at + 2..];
        let end = rest.find(')')?;
        Some(rest[..end].to_string())
    })
}

/// Rewrites links in a reply as it streams: a link is held back from its `[`
/// until its `)`, then let through resolved, so a handle split across deltas is
/// never seen half-written. Code is passed through untouched.
#[derive(Debug, Default)]
pub struct LinkRewriter {
    held: String,
    state: State,
    /// Inside a backtick span or fence, where `[x](y)` is code, not a link.
    in_code: bool,
}

#[derive(Debug, Default, Clone, Copy, PartialEq)]
enum State {
    #[default]
    Text,
    Label,
    LabelClosed,
    Target,
}

/// Past these lengths a held run is not a link, and it goes out as written.
const MAX_LABEL: usize = 300;
const MAX_TARGET: usize = 600;

impl LinkRewriter {
    /// The part of `delta` that can go out now.
    pub fn push(&mut self, delta: &str, cites: &Citations) -> String {
        let mut out = String::new();
        for c in delta.chars() {
            self.step(c, cites, &mut out);
        }
        out
    }

    /// Whatever is still held when the text ends, as written.
    pub fn finish(&mut self) -> String {
        self.state = State::Text;
        std::mem::take(&mut self.held)
    }

    fn step(&mut self, c: char, cites: &Citations, out: &mut String) {
        if c == '`' && self.state == State::Text {
            self.in_code = !self.in_code;
            out.push(c);
            return;
        }
        if self.in_code {
            out.push(c);
            return;
        }
        match self.state {
            State::Text if c == '[' => {
                self.held.push(c);
                self.state = State::Label;
            }
            State::Text => out.push(c),
            State::Label if c == '[' => {
                // A second `[` before the first closed: the first was text.
                out.push_str(&std::mem::take(&mut self.held));
                self.held.push(c);
            }
            State::Label if c == ']' => {
                self.held.push(c);
                self.state = State::LabelClosed;
            }
            State::Label if c == '\n' || self.held.len() > MAX_LABEL => self.release(c, out),
            State::Label => self.held.push(c),
            State::LabelClosed if c == '(' => {
                self.held.push(c);
                self.state = State::Target;
            }
            State::LabelClosed => self.release(c, out),
            State::Target if c == ')' => {
                out.push_str(&self.resolved(cites));
                self.held.clear();
                self.state = State::Text;
            }
            State::Target if c == '\n' || self.held.len() > MAX_LABEL + MAX_TARGET => self.release(c, out),
            State::Target => self.held.push(c),
        }
    }

    /// Give up on the held run as a link and send it on, with `c`.
    fn release(&mut self, c: char, out: &mut String) {
        out.push_str(&std::mem::take(&mut self.held));
        self.state = State::Text;
        if c == '[' {
            self.held.push(c);
            self.state = State::Label;
        } else {
            out.push(c);
        }
    }

    /// The held `[label](target` as it should be sent, its `)` included.
    fn resolved(&self, cites: &Citations) -> String {
        let Some((label, target)) = self.held.strip_prefix('[').and_then(|h| h.split_once("](")) else {
            return format!("{})", self.held);
        };
        match cites.resolve(target) {
            Some(route) => format!("[{label}]({route})"),
            None => label.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn turn() -> Citations {
        let history = vec![
            json!({ "role": "tool", "content": r#"{"rows":[{"ref":"/record/sleep/sle_old"}],"ref":"/chat/c1/tool/call_old"}"# }),
            json!({ "role": "assistant", "content": "Earlier I said [this](/record/communication_email/em_7)." }),
        ];
        let mut cites = Citations::from_messages(&history);
        cites.shorten(&json!({
            "rows": [{ "id": "row_9", "ref": "/record/communication_message/row_9" }],
            "ref": "/chat/c1/tool/call_new"
        }));
        cites
    }

    #[test]
    fn the_model_reads_handles_and_the_routes_stay_behind() {
        let mut cites = Citations::default();
        let data = json!({ "rows": [{ "ref": "/record/a/1" }, { "ref": "/record/a/2" }, { "ref": "/record/a/1" }], "ref": "/chat/c/tool/t" });
        let shown = cites.shorten(&data);
        // Keys are visited in serde_json's order, so the query's own ref comes
        // before its rows'.
        assert_eq!(shown["ref"], "r1");
        assert_eq!(shown["rows"][0]["ref"], "r2");
        assert_eq!(shown["rows"][1]["ref"], "r3");
        assert_eq!(shown["rows"][2]["ref"], "r2", "one route, one handle");
        assert_eq!(data["rows"][0]["ref"], "/record/a/1", "the caller's data is untouched");
    }

    /// Every broken shape seen on a real box, and the ones that must not move.
    #[test]
    fn links_resolve_repair_or_drop() {
        let cites = turn();
        let cases = [
            ("r1", Some("/chat/c1/tool/call_new")),
            ("r2", Some("/record/communication_message/row_9")),
            (" r1", Some("/chat/c1/tool/call_new")),
            ("/chat/c1/tool/call_new", Some("/chat/c1/tool/call_new")),
            ("/record/sleep/sle_old", Some("/record/sleep/sle_old")),
            ("/record/communication_email/em_7", Some("/record/communication_email/em_7")),
            ("chat/c1/tool/call_new", Some("/chat/c1/tool/call_new")),
            ("https://chat/c1/tool/call_new", Some("/chat/c1/tool/call_new")),
            ("the query ref, /chat/c1/tool/call_new", Some("/chat/c1/tool/call_new")),
            ("tool/call_new", Some("/chat/c1/tool/call_new")),
            ("call_old", Some("/chat/c1/tool/call_old")),
            ("row_9", Some("/record/communication_message/row_9")),
            ("https://example.com/a", Some("https://example.com/a")),
            ("https://the-record/chat/c1/tool/call_new", Some("/chat/c1/tool/call_new")),
            ("/person/person_ab12", Some("/person/person_ab12")),
            ("r9", None),
            ("sql", None),
            ("/record/sleep/never_returned", None),
        ];
        for (target, want) in cases {
            assert_eq!(cites.resolve(target).as_deref(), want, "target {target:?}");
        }
    }

    #[test]
    fn a_reply_is_rewritten_however_it_is_split() {
        let cites = turn();
        let reply = "You slept badly [last night](r2), see [the rows](r1). [1] stays, `[x](r1)` is code, [gone](r9) is text.";
        let want = "You slept badly [last night](/record/communication_message/row_9), see [the rows](/chat/c1/tool/call_new). [1] stays, `[x](r1)` is code, gone is text.";
        for size in [1, 2, 3, 7, reply.len()] {
            let mut rewriter = LinkRewriter::default();
            let mut out = String::new();
            let chars: Vec<char> = reply.chars().collect();
            for chunk in chars.chunks(size) {
                out.push_str(&rewriter.push(&chunk.iter().collect::<String>(), &cites));
            }
            out.push_str(&rewriter.finish());
            assert_eq!(out, want, "split into pieces of {size}");
        }
    }

    #[test]
    fn text_that_only_looks_like_a_link_goes_out_whole() {
        let cites = turn();
        let mut rewriter = LinkRewriter::default();
        let mut out = rewriter.push("a [note] then (aside) and [open", &cites);
        out.push_str(&rewriter.finish());
        assert_eq!(out, "a [note] then (aside) and [open");
    }
}
