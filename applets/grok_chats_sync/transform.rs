//! grok.com responses → `data_content_conversation` rows, and the walk that
//! decides which chats a run fetches.
//!
//! Three calls make up a sync (agents/record/ai-chat-sync-spike-2026-10-23.md):
//! the chat list, each chat's response tree, and the responses themselves.
//! Every parser here errors on a body that lacks the field it reads. A changed
//! or empty response is how an expired session or a moved endpoint shows up,
//! and reading it as "no chats" would look like a quiet day.

use anyhow::{anyhow, bail, Result};
use chrono::{DateTime, Utc};
use serde_json::Value;
use virtues_helpers::conversations::{stream_id, ConversationMsg};

const PROVIDER: &str = "grok";
const SOURCE_TABLE: &str = "grok_chats";

/// One chat from the list.
#[derive(Debug, Clone, PartialEq)]
pub struct ChatSummary {
    pub id: String,
    pub modify_time: DateTime<Utc>,
}

/// One page of the chat list.
#[derive(Debug)]
pub struct ListPage {
    pub chats: Vec<ChatSummary>,
    pub next_page_token: Option<String>,
}

/// Parse a `GET /rest/app-chat/conversations` page.
///
/// Private chats (`temporary: true`) are left out: the owner asked Grok not to
/// keep them, so the box does not either.
pub fn parse_list_page(body: &Value) -> Result<ListPage> {
    let items = body
        .get("conversations")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("grok chat list has no `conversations` array (the endpoint changed, or the session is invalid)"))?;

    let mut chats = Vec::with_capacity(items.len());
    for item in items {
        let id = item
            .get("conversationId")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| anyhow!("grok chat list item has no conversationId"))?;
        let modify_time = item
            .get("modifyTime")
            .and_then(Value::as_str)
            .and_then(parse_time)
            .ok_or_else(|| anyhow!("grok chat {id} has no readable modifyTime"))?;
        if item.get("temporary").and_then(Value::as_bool) == Some(true) {
            continue;
        }
        chats.push(ChatSummary {
            id: id.to_string(),
            modify_time,
        });
    }

    let next_page_token = body
        .get("nextPageToken")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(String::from);

    Ok(ListPage { chats, next_page_token })
}

/// Parse `GET .../response-node`: the ids of every response in the chat,
/// branches from edits and retries included.
pub fn parse_response_ids(body: &Value) -> Result<Vec<String>> {
    let nodes = body
        .get("responseNodes")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("grok response-node body has no `responseNodes` array"))?;
    nodes
        .iter()
        .map(|n| {
            n.get("responseId")
                .and_then(Value::as_str)
                .map(String::from)
                .ok_or_else(|| anyhow!("grok response node has no responseId"))
        })
        .collect()
}

/// Parse `POST .../load-responses` into rows for one chat.
///
/// Keeps the owner's turns and the assistant's final text. Grok returns the
/// reasoning, search results and tool steps in other fields of the same
/// response; only `message` is read, so they are dropped. A response with no
/// text (an image-only turn, a cancelled reply) or no readable `createTime` is
/// skipped: a guessed time would put the message on the wrong day.
pub fn parse_responses(chat: &ChatSummary, body: &Value) -> Result<Vec<ConversationMsg>> {
    let responses = body
        .get("responses")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("grok load-responses body has no `responses` array"))?;

    let mut out = Vec::with_capacity(responses.len());
    for r in responses {
        let Some(response_id) = r.get("responseId").and_then(Value::as_str) else {
            continue;
        };
        let role = match r.get("sender").and_then(Value::as_str) {
            Some(s) if s.eq_ignore_ascii_case("human") => "user",
            Some(s) if s.eq_ignore_ascii_case("assistant") => "assistant",
            _ => continue,
        };
        let Some(content) = r
            .get("message")
            .and_then(Value::as_str)
            .filter(|m| !m.trim().is_empty())
        else {
            continue;
        };
        let Some(timestamp) = r.get("createTime").and_then(Value::as_str).and_then(parse_time) else {
            continue;
        };
        out.push(ConversationMsg {
            stream_id: stream_id(PROVIDER, &chat.id, response_id),
            conversation_id: chat.id.clone(),
            message_id: response_id.to_string(),
            role: role.to_string(),
            content: content.to_string(),
            provider: PROVIDER.to_string(),
            timestamp,
            source_table: SOURCE_TABLE.to_string(),
        });
    }
    Ok(out)
}

/// Grok sends RFC 3339 times with fractional seconds and a `Z`.
pub fn parse_time(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s).ok().map(|dt| dt.with_timezone(&Utc))
}

// ── The walk ─────────────────────────────────────────────────────────────

/// What the box has: every chat whose `modifyTime` falls in
/// `[oldest, newest]` is stored. `complete` means the range reaches the
/// bottom of the list, so nothing older is missing.
#[derive(Debug, Clone, PartialEq)]
pub struct Synced {
    pub newest: DateTime<Utc>,
    pub oldest: DateTime<Utc>,
    pub complete: bool,
}

/// Why a walk stopped.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Stop {
    /// It reached chats the box has, below which nothing is missing.
    CaughtUp,
    /// It used the run's chat budget; the next run continues.
    Budget,
}

/// Decides, page by page, which chats to fetch.
///
/// The list is sorted by `modifyTime`, newest first, so a chat that gets a new
/// message moves above the synced range. A run fetches everything above the
/// range, skips the range, and then fills in below it until the budget runs
/// out. The first run of a large account therefore spreads over several runs.
///
/// A run that fails part way stores nothing, and the next run starts again from
/// the last stored range. Messages already written are skipped by their id.
pub struct Walk {
    prior: Option<Synced>,
    budget: usize,
    taken: usize,
    last_seen: Option<DateTime<Utc>>,
    top_fetched: Option<DateTime<Utc>>,
    lowest_fetched_below: Option<DateTime<Utc>>,
    /// The budget ran out above the synced range, so the range cannot grow.
    starved_above: bool,
    stop: Option<Stop>,
}

impl Walk {
    pub fn new(prior: Option<Synced>, budget: usize) -> Self {
        Self {
            prior,
            budget,
            taken: 0,
            last_seen: None,
            top_fetched: None,
            lowest_fetched_below: None,
            starved_above: false,
            stop: None,
        }
    }

    pub fn stopped(&self) -> Option<Stop> {
        self.stop
    }

    /// Take one page, returning the chats on it to fetch. Errors when the list
    /// is out of order, because the stop rule would then skip chats.
    pub fn take(&mut self, chats: &[ChatSummary]) -> Result<Vec<ChatSummary>> {
        let mut fetch = Vec::new();
        for chat in chats {
            if self.stop.is_some() {
                break;
            }
            if let Some(prev) = self.last_seen {
                if chat.modify_time > prev {
                    bail!(
                        "grok's chat list is not sorted by last update, newest first \
                         (chat {} is newer than the one before it); the sync relies on that order",
                        chat.id
                    );
                }
            }
            self.last_seen = Some(chat.modify_time);

            let above = match &self.prior {
                None => false,
                Some(p) if chat.modify_time > p.newest => true,
                Some(p) if chat.modify_time >= p.oldest => {
                    if p.complete {
                        self.stop = Some(Stop::CaughtUp);
                    }
                    continue;
                }
                Some(_) => false,
            };

            if self.taken == self.budget {
                self.starved_above = above;
                self.stop = Some(Stop::Budget);
                break;
            }
            self.taken += 1;
            if self.top_fetched.is_none() {
                self.top_fetched = Some(chat.modify_time);
            }
            if !above {
                self.lowest_fetched_below = Some(chat.modify_time);
            }
            fetch.push(chat.clone());
        }
        Ok(fetch)
    }

    /// The range to store once every fetched chat is written. `list_ended`
    /// says the walk read the last page. `None` means nothing to store yet.
    pub fn finish(&self, list_ended: bool) -> Option<Synced> {
        if self.starved_above {
            // The chats fetched above the range leave a gap below them, so the
            // stored range stays as it was; the next run fetches them again.
            return self.prior.clone();
        }
        let complete = match self.stop {
            Some(Stop::CaughtUp) => true,
            Some(Stop::Budget) => false,
            None => list_ended,
        };
        match &self.prior {
            Some(p) => Some(Synced {
                newest: self.top_fetched.map_or(p.newest, |t| t.max(p.newest)),
                oldest: self.lowest_fetched_below.unwrap_or(p.oldest),
                complete: p.complete || complete,
            }),
            None => Some(Synced {
                newest: self.top_fetched?,
                oldest: self.lowest_fetched_below?,
                complete,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn t(s: &str) -> DateTime<Utc> {
        parse_time(s).unwrap()
    }

    fn chat(id: &str, modify: &str) -> ChatSummary {
        ChatSummary { id: id.into(), modify_time: t(modify) }
    }

    fn ids(v: &[ChatSummary]) -> Vec<&str> {
        v.iter().map(|c| c.id.as_str()).collect()
    }

    #[test]
    fn list_page_parses_and_skips_private_chats() {
        let body = json!({
            "conversations": [
                {"conversationId": "c-3", "title": "Sourdough starter ratios",
                 "createTime": "2031-04-02T08:00:00.000000Z", "modifyTime": "2031-04-03T09:15:00.123456Z",
                 "starred": false, "temporary": false},
                {"conversationId": "c-2", "title": "", "modifyTime": "2031-04-01T10:00:00Z", "temporary": true},
                {"conversationId": "c-1", "modifyTime": "2031-03-30T07:30:00Z"}
            ],
            "nextPageToken": "tok-2",
            "textSearchMatches": []
        });
        let page = parse_list_page(&body).unwrap();
        assert_eq!(ids(&page.chats), ["c-3", "c-1"]);
        assert_eq!(page.next_page_token.as_deref(), Some("tok-2"));
    }

    #[test]
    fn list_page_without_conversations_is_an_error() {
        assert!(parse_list_page(&json!({"error": {"code": 16}})).is_err());
        assert!(parse_list_page(&json!({"conversations": null})).is_err());
    }

    #[test]
    fn list_page_item_without_a_time_is_an_error() {
        let body = json!({"conversations": [{"conversationId": "c-1", "modifyTime": "yesterday"}]});
        assert!(parse_list_page(&body).is_err());
    }

    #[test]
    fn empty_next_page_token_means_last_page() {
        let page = parse_list_page(&json!({"conversations": [], "nextPageToken": ""})).unwrap();
        assert_eq!(page.next_page_token, None);
    }

    #[test]
    fn response_ids_parse() {
        let body = json!({"responseNodes": [
            {"responseId": "r-1", "sender": "human"},
            {"responseId": "r-2", "parentResponseId": "r-1", "sender": "ASSISTANT"}
        ]});
        assert_eq!(parse_response_ids(&body).unwrap(), ["r-1", "r-2"]);
        assert!(parse_response_ids(&json!({})).is_err());
    }

    #[test]
    fn responses_keep_turns_and_final_text_only() {
        let c = ChatSummary {
            id: "c-9".into(),
            modify_time: t("2031-05-01T12:00:00Z"),
        };
        let body = json!({"responses": [
            {"responseId": "r-1", "sender": "human", "message": "How long to paddle 20 km?",
             "createTime": "2031-05-01T11:58:00.5Z", "thinkingTrace": ""},
            {"responseId": "r-2", "parentResponseId": "r-1", "sender": "ASSISTANT",
             "message": "About five hours at a relaxed pace.", "createTime": "2031-05-01T11:58:09Z",
             "model": "grok-test", "thinkingTrace": "Paddling speed is roughly 4 km/h...",
             "webSearchResults": [{"url": "https://example.com"}]},
            {"responseId": "r-3", "sender": "assistant", "message": "   ", "createTime": "2031-05-01T11:59:00Z"},
            {"responseId": "r-4", "sender": "human", "message": "No time on this one"},
            {"responseId": "r-5", "sender": "system", "message": "hidden", "createTime": "2031-05-01T11:59:00Z"}
        ]});
        let rows = parse_responses(&c, &body).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].role, "user");
        assert_eq!(rows[0].stream_id, "chatimport:grok:c-9:r-1");
        assert_eq!(rows[1].role, "assistant");
        assert_eq!(rows[1].content, "About five hours at a relaxed pace.");
        assert_eq!(rows[1].timestamp, t("2031-05-01T11:58:09Z"));
        assert!(parse_responses(&c, &json!({"responseNodes": []})).is_err());
    }

    #[test]
    fn first_walk_fetches_everything_and_completes_at_the_end() {
        let mut w = Walk::new(None, 10);
        let p1 = [chat("a", "2031-01-05T00:00:00Z"), chat("b", "2031-01-04T00:00:00Z")];
        let p2 = [chat("c", "2031-01-03T00:00:00Z")];
        assert_eq!(ids(&w.take(&p1).unwrap()), ["a", "b"]);
        assert_eq!(ids(&w.take(&p2).unwrap()), ["c"]);
        assert_eq!(w.stopped(), None);
        let s = w.finish(true).unwrap();
        assert_eq!(s.newest, t("2031-01-05T00:00:00Z"));
        assert_eq!(s.oldest, t("2031-01-03T00:00:00Z"));
        assert!(s.complete);
    }

    #[test]
    fn caught_up_walk_fetches_only_new_chats() {
        let prior = Synced {
            newest: t("2031-01-05T00:00:00Z"),
            oldest: t("2031-01-01T00:00:00Z"),
            complete: true,
        };
        let mut w = Walk::new(Some(prior), 10);
        let page = [
            chat("new", "2031-01-07T00:00:00Z"),
            chat("a", "2031-01-05T00:00:00Z"),
            chat("b", "2031-01-04T00:00:00Z"),
        ];
        assert_eq!(ids(&w.take(&page).unwrap()), ["new"]);
        assert_eq!(w.stopped(), Some(Stop::CaughtUp));
        let s = w.finish(false).unwrap();
        assert_eq!(s.newest, t("2031-01-07T00:00:00Z"));
        assert_eq!(s.oldest, t("2031-01-01T00:00:00Z"));
        assert!(s.complete);
    }

    #[test]
    fn budget_splits_a_backfill_across_runs() {
        let pages = [
            chat("a", "2031-01-05T00:00:00Z"),
            chat("b", "2031-01-04T00:00:00Z"),
            chat("c", "2031-01-03T00:00:00Z"),
            chat("d", "2031-01-02T00:00:00Z"),
        ];

        let mut w = Walk::new(None, 2);
        assert_eq!(ids(&w.take(&pages).unwrap()), ["a", "b"]);
        assert_eq!(w.stopped(), Some(Stop::Budget));
        let first = w.finish(false).unwrap();
        assert_eq!(first.oldest, t("2031-01-04T00:00:00Z"));
        assert!(!first.complete);

        // The next run skips what it has and continues below it.
        let mut w = Walk::new(Some(first), 2);
        assert_eq!(ids(&w.take(&pages).unwrap()), ["c", "d"]);
        let second = w.finish(true).unwrap();
        assert_eq!(second.newest, t("2031-01-05T00:00:00Z"));
        assert_eq!(second.oldest, t("2031-01-02T00:00:00Z"));
        assert!(second.complete);
    }

    #[test]
    fn budget_used_above_the_range_keeps_the_prior_range() {
        let prior = Synced {
            newest: t("2031-01-05T00:00:00Z"),
            oldest: t("2031-01-01T00:00:00Z"),
            complete: true,
        };
        let mut w = Walk::new(Some(prior.clone()), 1);
        let page = [chat("x", "2031-01-09T00:00:00Z"), chat("y", "2031-01-08T00:00:00Z")];
        assert_eq!(ids(&w.take(&page).unwrap()), ["x"]);
        assert_eq!(w.stopped(), Some(Stop::Budget));
        assert_eq!(w.finish(false), Some(prior));
    }

    #[test]
    fn out_of_order_list_is_an_error() {
        let mut w = Walk::new(None, 10);
        w.take(&[chat("a", "2031-01-02T00:00:00Z")]).unwrap();
        assert!(w.take(&[chat("b", "2031-01-03T00:00:00Z")]).is_err());
    }

    #[test]
    fn nothing_fetched_on_a_first_walk_stores_nothing() {
        let w = Walk::new(None, 10);
        assert_eq!(w.finish(true), None);
    }
}
