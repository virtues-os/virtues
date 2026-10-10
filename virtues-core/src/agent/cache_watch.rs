//! What a model call could have read from cache, and where it changed.
//!
//! A provider serves from cache only what an earlier request already sent, and
//! grok reuses a prefix only where an earlier request ENDED (measured
//! 2026-10-09). So the most a call can read is the longest earlier request in
//! its chat that it starts with, byte for byte. This keeps each chat's recent
//! requests as hashes and reports, beside each call's usage, how many tokens
//! that was, and where the call first left the request before it.
//!
//! The two numbers split a miss by owner. Reading far less than was reusable
//! is the provider's miss (routing, eviction): about a quarter of grok calls
//! even seconds apart. Little reusable at all means our own bytes changed, and
//! `diverged_at` names the place. Hashes and counts only, never content.

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::Value;

/// How long a chat's requests are remembered, and how many. Past an hour no
/// provider here still holds the cache, so nothing older can be reusable.
const REMEMBERED_FOR: Duration = Duration::from_secs(60 * 60);
const REMEMBERED_REQUESTS: usize = 64;

/// One request as hashes. `prefix[i]` covers the model, the tools and
/// messages `0..=i`, so two requests agree up to `i` exactly when their
/// `prefix[i]` match.
pub(super) struct Signature {
    head: u64,
    prefix: Vec<u64>,
    roles: Vec<String>,
}

/// Hash a request as it will be sent. The message JSON is serialized with
/// sorted keys, so equal bytes on the wire hash equal here.
pub(super) fn sign(model: &str, tools: &[Value], messages: &[Value]) -> Signature {
    let mut hasher = DefaultHasher::new();
    model.hash(&mut hasher);
    for tool in tools {
        tool.to_string().hash(&mut hasher);
    }
    let head = hasher.finish();
    let mut running = head;
    let mut prefix = Vec::with_capacity(messages.len());
    let mut roles = Vec::with_capacity(messages.len());
    for message in messages {
        let mut hasher = DefaultHasher::new();
        running.hash(&mut hasher);
        message.to_string().hash(&mut hasher);
        running = hasher.finish();
        prefix.push(running);
        roles.push(message["role"].as_str().unwrap_or("?").to_string());
    }
    Signature { head, prefix, roles }
}

struct Sent {
    signature: Signature,
    prompt_tokens: u32,
    at: Instant,
}

/// What one call could have read, and how it differed from the call before.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Reading {
    /// Prompt tokens of the longest earlier request this one starts with:
    /// the most any provider here could have served from cache.
    pub reusable_tokens: u32,
    /// Where this request first differs from the previous one in the chat,
    /// or empty when the previous one is a prefix of it (nothing of ours
    /// changed) or there was none.
    pub diverged_at: String,
}

fn sent() -> &'static Mutex<HashMap<String, Vec<Sent>>> {
    static SENT: OnceLock<Mutex<HashMap<String, Vec<Sent>>>> = OnceLock::new();
    SENT.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Compare a finished call with the chat's earlier ones, then remember it.
pub(super) fn record(chat_id: &str, signature: Signature, prompt_tokens: u32) -> Reading {
    let now = Instant::now();
    let mut chats = sent().lock().unwrap_or_else(|e| e.into_inner());
    chats.retain(|_, requests| {
        requests.retain(|r| now.duration_since(r.at) < REMEMBERED_FOR);
        !requests.is_empty()
    });
    let requests = chats.entry(chat_id.to_string()).or_default();

    let reusable_tokens = requests
        .iter()
        .filter(|r| is_prefix(&r.signature, &signature))
        .map(|r| r.prompt_tokens)
        .max()
        .unwrap_or(0);
    let diverged_at = requests
        .last()
        .map(|previous| divergence(&previous.signature, &signature))
        .unwrap_or_default();

    requests.push(Sent { signature, prompt_tokens, at: now });
    if requests.len() > REMEMBERED_REQUESTS {
        requests.remove(0);
    }
    Reading { reusable_tokens, diverged_at }
}

/// Whether `earlier` is the start of `later`, whole requests compared.
fn is_prefix(earlier: &Signature, later: &Signature) -> bool {
    let n = earlier.prefix.len();
    earlier.head == later.head && n > 0 && n <= later.prefix.len() && earlier.prefix[n - 1] == later.prefix[n - 1]
}

/// Where `later` first leaves `earlier`, in words a log reader can act on.
fn divergence(earlier: &Signature, later: &Signature) -> String {
    if is_prefix(earlier, later) {
        return String::new();
    }
    if earlier.head != later.head {
        return "tools".to_string();
    }
    let first = earlier
        .prefix
        .iter()
        .zip(&later.prefix)
        .position(|(a, b)| a != b);
    match first {
        Some(0) => "system prompt".to_string(),
        Some(i) => format!("message {i} ({})", later.roles[i]),
        // Agrees as far as it goes, and is shorter: history was dropped
        // (compaction, a regenerate).
        None => "history shortened".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn request(messages: &[(&str, &str)]) -> Vec<Value> {
        messages.iter().map(|(role, text)| json!({ "role": role, "content": text })).collect()
    }

    #[test]
    fn a_request_that_extends_the_last_reads_it_all_and_changed_nothing() {
        let chat = "chat_watch_extends";
        let first = request(&[("system", "S"), ("user", "hi")]);
        let mut second = first.clone();
        second.extend(request(&[("assistant", "hello"), ("user", "and?")]));

        assert_eq!(record(chat, sign("m", &[], &first), 1000).reusable_tokens, 0);
        let reading = record(chat, sign("m", &[], &second), 1200);
        assert_eq!(reading, Reading { reusable_tokens: 1000, diverged_at: String::new() });
    }

    #[test]
    fn a_changed_system_prompt_leaves_nothing_reusable_and_says_so() {
        let chat = "chat_watch_system";
        record(chat, sign("m", &[], &request(&[("system", "clock 6:30"), ("user", "hi")])), 1000);
        let reading = record(chat, sign("m", &[], &request(&[("system", "clock 6:45"), ("user", "hi")])), 1000);
        assert_eq!(reading.reusable_tokens, 0);
        assert_eq!(reading.diverged_at, "system prompt");
    }

    #[test]
    fn a_rewritten_turn_falls_back_to_the_last_request_it_still_starts_with() {
        let chat = "chat_watch_rewrite";
        let step1 = request(&[("system", "S"), ("user", "look")]);
        let mut step2 = step1.clone();
        step2.extend(request(&[("assistant", "calling"), ("tool", "32 KiB of rows")]));
        record(chat, sign("m", &[], &step1), 800);
        record(chat, sign("m", &[], &step2), 2000);

        let mut next = step1.clone();
        next.extend(request(&[("assistant", "calling"), ("tool", "2 KiB of rows"), ("user", "thanks")]));
        let reading = record(chat, sign("m", &[], &next), 1100);
        assert_eq!(reading.reusable_tokens, 800, "turn 1's first request is still a prefix");
        assert_eq!(reading.diverged_at, "message 3 (tool)");
    }

    #[test]
    fn a_new_tool_is_a_change_before_every_message() {
        let chat = "chat_watch_tools";
        let messages = request(&[("system", "S"), ("user", "hi")]);
        record(chat, sign("m", &[json!({"name": "a"})], &messages), 1000);
        let reading = record(chat, sign("m", &[json!({"name": "a"}), json!({"name": "b"})], &messages), 1000);
        assert_eq!(reading, Reading { reusable_tokens: 0, diverged_at: "tools".to_string() });
    }
}
