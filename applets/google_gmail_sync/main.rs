//! Gmail sync.
//!
//! Cron-driven, per-credential. Two modes, chosen by what the saved state
//! holds:
//!
//! * **History** (the normal run). `history_id` is set, so one
//!   `users.history.list` call returns everything that changed since the last
//!   run: messages added, labels added or removed, messages deleted. A quiet
//!   run costs one request. New messages are fetched in full; label changes
//!   and deletions update rows already stored.
//! * **Baseline** (first run, or Gmail no longer has our `history_id`). Read
//!   the profile for the address and the current `historyId`, then list
//!   message ids with `after:<last_sync_unix>` (30 days on a first run) and
//!   fetch them. The `historyId` is read before the listing, so anything that
//!   arrives during it shows up again in the next history run, where the
//!   insert skips it as a duplicate.
//!
//! Drafts, spam and trash are skipped. Every save of a draft is a new message
//! id, so following drafts would fetch the same text over and over.

mod transform;

use std::collections::{HashMap, HashSet};

use anyhow::{Context, Result};
use serde_json::Value;
use virtues::storage::lake;
use virtues_helpers::{connect_from_env, output, read_input};

const ACTION: &str = "google_gmail_sync";
const PAGE_SIZE: u32 = 100;
const MAX_PAGES: u32 = 5;
const HISTORY_PAGE_SIZE: u32 = 500;
/// Full-message fetches per run. A history run that reaches it stops after the
/// current page and saves that page's position, so the next run resumes there.
const MAX_FETCHES: usize = 500;
const SKIPPED_LABELS: [&str; 3] = ["DRAFT", "SPAM", "TRASH"];
/// Full-message fetches in flight at once. One at a time, a first sync of a
/// busy mailbox could not finish inside the applet's time limit when each
/// fetch takes about a second, as it does for a sign-in made through
/// virtues-api, and it then started over on every run. Four stays under the
/// 50 requests per 10 seconds such a sign-in is allowed, and under Gmail's
/// own per-user quota when the box calls Google directly.
const FETCHES_AT_ONCE: usize = 4;
/// How many times one fetch waits out a "too many requests" answer.
const RATE_LIMIT_TRIES: u32 = 4;
/// The longest single wait, whatever `Retry-After` says.
const MAX_RATE_LIMIT_WAIT_SECS: u64 = 15;

#[tokio::main]
async fn main() -> Result<()> {
    virtues_applets::init_tracing();

    let mut input = read_input()?;
    let pool = connect_from_env("virtues-action-google_gmail_sync").await?;

    let access_token = virtues_applets::secret(&input, "access_token")?
        .to_string();

    let storage = lake::storage_from_env()?;
    // Must be the shared client, not `reqwest::Client::new()`: reqwest here is
    // built `rustls-tls-no-provider`, so a bare client panics "No provider set"
    // on the first HTTPS call. `http_client()` installs the ring provider (and
    // adds a timeout a bare client lacks entirely).
    let client = virtues_applets::http_client();
    // Google's own host, or virtues-api standing in for it.
    let hosts = virtues_applets::google_hosts(&input);
    let sync = Sync { client: &client, token: &access_token, gmail: &hosts.gmail, pool: &pool, storage: &storage };

    let history_id = input.config.get("history_id").and_then(|v| v.as_str()).map(String::from);
    let user_email = input.config.get("user_email").and_then(|v| v.as_str()).map(String::from);

    let summary = match (history_id, user_email) {
        (Some(start), Some(email)) if !email.is_empty() => {
            match sync.history(&start, &email).await {
                Ok(run) => {
                    input.config["history_id"] = Value::from(run.history_id);
                    if run.latest_ts > 0 {
                        input.config["last_sync_unix"] = Value::from(run.latest_ts);
                    }
                    run.summary
                }
                // Gmail keeps history for about a week. Past that, or after a
                // reset, it answers 404 and the only way back is a baseline.
                Err(e) if status_is(&e, reqwest::StatusCode::NOT_FOUND) => {
                    tracing::warn!("history_id is no longer known to Gmail; listing since the last sync");
                    sync.baseline(&mut input.config).await?
                }
                Err(e) => return Err(e),
            }
        }
        _ => sync.baseline(&mut input.config).await?,
    };

    output(&summary, &input.config)
}

struct Sync<'a> {
    client: &'a reqwest::Client,
    token: &'a str,
    /// Gmail's API for the signed-in user, up to and including `users/me`.
    gmail: &'a str,
    pool: &'a sqlx::PgPool,
    storage: &'a virtues::storage::Storage,
}

struct HistoryRun {
    history_id: String,
    latest_ts: i64,
    summary: String,
}

impl Sync<'_> {
    /// Apply everything Gmail recorded after `start`.
    async fn history(&self, start: &str, user_email: &str) -> Result<HistoryRun> {
        let mut cursor = start.to_string();
        let mut page_token: Option<String> = None;
        let mut latest_ts = 0i64;
        let (mut added, mut relabeled, mut deleted) = (0usize, 0u64, 0u64);
        let mut fetched = 0usize;

        loop {
            let mut req = self.client.get(format!("{}/history", self.gmail)).bearer_auth(self.token).query(&[
                ("startHistoryId", start),
                ("maxResults", &HISTORY_PAGE_SIZE.to_string()),
            ]);
            for t in ["messageAdded", "labelAdded", "labelRemoved", "messageDeleted"] {
                req = req.query(&[("historyTypes", t)]);
            }
            if let Some(p) = &page_token {
                req = req.query(&[("pageToken", p.as_str())]);
            }
            let page: Value = req
                .send()
                .await
                .context("gmail history failed")?
                .error_for_status()
                .context("gmail history non-2xx")?
                .json()
                .await
                .context("gmail history non-JSON")?;

            let records = page.get("history").and_then(|v| v.as_array()).cloned().unwrap_or_default();
            if !records.is_empty() {
                lake::archive_cloud(self.pool, self.storage, "google", ACTION, "gmail_history", std::slice::from_ref(&page))
                    .await?;
            }
            let changes = Changes::from_records(&records);

            // A message deleted between the change and this run is not in
            // what comes back.
            let messages = self.fetch_messages(&changes.added).await?;
            for m in &messages {
                if let Some(ts) = internal_secs(m) {
                    latest_ts = latest_ts.max(ts);
                }
            }
            fetched += messages.len();
            if !messages.is_empty() {
                lake::archive_cloud(self.pool, self.storage, "google", ACTION, "gmail", &messages).await?;
                added += transform::write_messages(self.pool, user_email, &messages).await?;
            }
            for (id, labels) in &changes.labels {
                relabeled += transform::update_labels(self.pool, id, labels).await?;
            }
            if !changes.deleted.is_empty() {
                deleted += transform::mark_deleted(self.pool, &changes.deleted).await?;
            }

            page_token = page.get("nextPageToken").and_then(|v| v.as_str()).map(String::from);
            match &page_token {
                None => {
                    // The last page carries the mailbox's current position.
                    if let Some(h) = page.get("historyId").and_then(|v| v.as_str()) {
                        cursor = h.to_string();
                    }
                    break;
                }
                Some(_) => {
                    // A later run can start after the last record handled here.
                    if let Some(id) = records.last().and_then(|r| r.get("id")).and_then(|v| v.as_str()) {
                        cursor = id.to_string();
                    }
                    if fetched >= MAX_FETCHES {
                        break;
                    }
                }
            }
        }

        let summary = if added + relabeled as usize + deleted as usize == 0 {
            "no new Gmail messages".to_string()
        } else {
            format!("synced {added} new Gmail messages, {relabeled} label changes, {deleted} deletions")
        };
        Ok(HistoryRun { history_id: cursor, latest_ts, summary })
    }

    /// First run, or recovery after Gmail forgot our `history_id`.
    async fn baseline(&self, config: &mut Value) -> Result<String> {
        let profile = self.profile().await?;
        let user_email = profile.get("emailAddress").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let history_id = profile
            .get("historyId")
            .and_then(|v| v.as_str())
            .context("gmail profile has no historyId")?
            .to_string();

        // Cursor: Unix seconds. First run: the last 30 days.
        let after = config
            .get("last_sync_unix")
            .and_then(|v| v.as_i64())
            .unwrap_or_else(|| chrono::Utc::now().timestamp() - 30 * 86400);

        let mut page_token: Option<String> = None;
        let mut total_written = 0usize;
        let mut latest_ts = after;
        let query = format!("after:{after}");

        for _ in 0..MAX_PAGES {
            let mut req = self.client.get(format!("{}/messages", self.gmail)).bearer_auth(self.token).query(&[
                ("q", query.as_str()),
                ("maxResults", &PAGE_SIZE.to_string()),
            ]);
            if let Some(p) = &page_token {
                req = req.query(&[("pageToken", p.as_str())]);
            }
            let list_resp: Value = req
                .send()
                .await
                .context("gmail list failed")?
                .error_for_status()
                .context("gmail list non-2xx")?
                .json()
                .await
                .context("gmail list non-JSON")?;

            let ids: Vec<String> = list_resp
                .get("messages")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|m| m.get("id").and_then(|i| i.as_str()).map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            if ids.is_empty() {
                break;
            }

            let mut messages = self.fetch_messages(&ids).await?;
            messages.retain(|m| !has_skipped_label(m.get("labelIds")));
            for m in &messages {
                if let Some(ts) = internal_secs(m) {
                    latest_ts = latest_ts.max(ts);
                }
            }

            // Raw Gmail message objects, exactly as the API returned them —
            // headers, labels and MIME parts included. The transform reads a
            // handful of those fields; the rest is the part that would be gone
            // forever.
            lake::archive_cloud(self.pool, self.storage, "google", ACTION, "gmail", &messages).await?;
            total_written += transform::write_messages(self.pool, &user_email, &messages).await?;

            page_token = list_resp.get("nextPageToken").and_then(|v| v.as_str()).map(String::from);
            if page_token.is_none() {
                break;
            }
        }

        config["user_email"] = Value::from(user_email);
        config["history_id"] = Value::from(history_id);
        config["last_sync_unix"] = Value::from(latest_ts);

        Ok(if total_written == 0 {
            "no new Gmail messages".to_string()
        } else {
            format!("synced {total_written} Gmail messages")
        })
    }

    async fn profile(&self) -> Result<Value> {
        self.client
            .get(format!("{}/profile", self.gmail))
            .bearer_auth(self.token)
            .send()
            .await
            .context("gmail profile failed")?
            .error_for_status()
            .context("gmail profile non-2xx")?
            .json()
            .await
            .context("gmail profile non-JSON")
    }

    /// These messages in full, in the order asked for. One Gmail no longer
    /// has is left out.
    async fn fetch_messages(&self, ids: &[String]) -> Result<Vec<Value>> {
        fetch_messages(self.client, self.token, self.gmail, ids).await
    }
}

/// Fetch messages [`FETCHES_AT_ONCE`] at a time and return them in the order
/// asked for.
async fn fetch_messages(client: &reqwest::Client, token: &str, gmail: &str, ids: &[String]) -> Result<Vec<Value>> {
    let mut out = Vec::with_capacity(ids.len());
    for wave in ids.chunks(FETCHES_AT_ONCE) {
        let mut set = tokio::task::JoinSet::new();
        for (i, id) in wave.iter().enumerate() {
            set.spawn(fetch_message(client.clone(), token.to_string(), format!("{gmail}/messages/{id}"), i));
        }
        let mut got: Vec<(usize, Value)> = Vec::with_capacity(wave.len());
        while let Some(joined) = set.join_next().await {
            if let (i, Some(m)) = joined.context("a gmail fetch stopped")?? {
                got.push((i, m));
            }
        }
        got.sort_by_key(|(i, _)| *i);
        out.extend(got.into_iter().map(|(_, m)| m));
    }
    Ok(out)
}

/// One message in full, with the place it was asked for. `None` when Gmail no
/// longer has it. A "too many requests" answer is waited out and tried again.
async fn fetch_message(client: reqwest::Client, token: String, url: String, at: usize) -> Result<(usize, Option<Value>)> {
    let mut tries = 0;
    loop {
        let resp = client
            .get(&url)
            .bearer_auth(&token)
            .query(&[("format", "full")])
            .send()
            .await
            .context("gmail get failed")?;
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok((at, None));
        }
        tries += 1;
        if resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS && tries < RATE_LIMIT_TRIES {
            tokio::time::sleep(rate_limit_wait(resp.headers().get(reqwest::header::RETRY_AFTER))).await;
            continue;
        }
        let m = resp
            .error_for_status()
            .context("gmail get non-2xx")?
            .json()
            .await
            .context("gmail get non-JSON")?;
        return Ok((at, Some(m)));
    }
}

/// How long to wait after "too many requests": what `Retry-After` asks for in
/// seconds, within [`MAX_RATE_LIMIT_WAIT_SECS`], or two seconds when it says
/// nothing usable.
fn rate_limit_wait(retry_after: Option<&reqwest::header::HeaderValue>) -> std::time::Duration {
    let secs = retry_after
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse::<u64>().ok())
        .unwrap_or(2)
        .clamp(1, MAX_RATE_LIMIT_WAIT_SECS);
    std::time::Duration::from_secs(secs)
}

/// What one page of history asks us to do, after folding repeats: a message
/// relabeled three times needs only its final labels, and a message added
/// and deleted in the same window needs nothing.
#[derive(Debug, Default, PartialEq)]
struct Changes {
    added: Vec<String>,
    labels: Vec<(String, Vec<String>)>,
    deleted: Vec<String>,
}

impl Changes {
    fn from_records(records: &[Value]) -> Changes {
        let mut added: Vec<String> = Vec::new();
        let mut added_set: HashSet<String> = HashSet::new();
        let mut labels: HashMap<String, Vec<String>> = HashMap::new();
        let mut label_order: Vec<String> = Vec::new();
        let mut deleted: Vec<String> = Vec::new();
        let mut deleted_set: HashSet<String> = HashSet::new();

        let entries = |r: &Value, key: &str| -> Vec<Value> {
            r.get(key).and_then(|v| v.as_array()).cloned().unwrap_or_default()
        };
        for r in records {
            for e in entries(r, "messagesAdded") {
                let m = &e["message"];
                let Some(id) = m.get("id").and_then(|v| v.as_str()) else { continue };
                if !has_skipped_label(m.get("labelIds")) && added_set.insert(id.to_string()) {
                    added.push(id.to_string());
                }
            }
            for key in ["labelsAdded", "labelsRemoved"] {
                for e in entries(r, key) {
                    let m = &e["message"];
                    let Some(id) = m.get("id").and_then(|v| v.as_str()) else { continue };
                    // Each entry carries the message's full label set after the
                    // change, so the last one seen is the current state.
                    if labels.insert(id.to_string(), label_list(m.get("labelIds"))).is_none() {
                        label_order.push(id.to_string());
                    }
                }
            }
            for e in entries(r, "messagesDeleted") {
                if let Some(id) = e["message"].get("id").and_then(|v| v.as_str()) {
                    if deleted_set.insert(id.to_string()) {
                        deleted.push(id.to_string());
                    }
                }
            }
        }

        added.retain(|id| !deleted_set.contains(id));
        // A new message is fetched in full, labels included.
        let labels = label_order
            .into_iter()
            .filter(|id| !deleted_set.contains(id) && !added_set.contains(id))
            .map(|id| {
                let l = labels.remove(&id).unwrap_or_default();
                (id, l)
            })
            .collect();
        Changes { added, labels, deleted }
    }
}

fn label_list(v: Option<&Value>) -> Vec<String> {
    v.and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
        .unwrap_or_default()
}

fn has_skipped_label(v: Option<&Value>) -> bool {
    label_list(v).iter().any(|l| SKIPPED_LABELS.contains(&l.as_str()))
}

fn internal_secs(m: &Value) -> Option<i64> {
    m.get("internalDate")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<i64>().ok())
        .map(|ms| ms / 1000)
}

fn status_is(err: &anyhow::Error, status: reqwest::StatusCode) -> bool {
    err.chain().any(|c| {
        c.downcast_ref::<reqwest::Error>()
            .and_then(|e| e.status())
            .is_some_and(|s| s == status)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A stand-in for Gmail: every message takes `delay` to answer, `gone`
    /// is a 404, and `busy` answers "too many requests" the first time.
    async fn slow_gmail(delay: std::time::Duration, gone: &'static str, busy: &'static str) -> String {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let refused = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else { return };
                let refused = refused.clone();
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 4096];
                    let n = socket.read(&mut buf).await.unwrap_or(0);
                    let request = String::from_utf8_lossy(&buf[..n]).to_string();
                    let path = request.split_whitespace().nth(1).unwrap_or("").to_string();
                    let id = path.rsplit('/').next().unwrap_or("").split('?').next().unwrap_or("").to_string();
                    tokio::time::sleep(delay).await;
                    let (status, extra, body) = if id == gone {
                        ("404 Not Found", String::new(), "{}".to_string())
                    } else if id == busy && !refused.swap(true, std::sync::atomic::Ordering::SeqCst) {
                        ("429 Too Many Requests", "Retry-After: 1\r\n".to_string(), "{}".to_string())
                    } else {
                        ("200 OK", String::new(), format!("{{\"id\":\"{id}\"}}"))
                    };
                    let response = format!(
                        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = socket.write_all(response.as_bytes()).await;
                });
            }
        });
        base
    }

    #[tokio::test]
    async fn messages_are_fetched_several_at_once_and_come_back_in_order() {
        let delay = std::time::Duration::from_millis(250);
        let base = slow_gmail(delay, "gone", "none").await;
        let client = reqwest::Client::new();
        let ids: Vec<String> = ["a", "b", "gone", "c", "d", "e", "f", "g"].iter().map(|s| s.to_string()).collect();

        let started = std::time::Instant::now();
        let got = fetch_messages(&client, "token", &base, &ids).await.unwrap();
        let took = started.elapsed();

        let order: Vec<&str> = got.iter().filter_map(|m| m["id"].as_str()).collect();
        assert_eq!(order, ["a", "b", "c", "d", "e", "f", "g"], "in the order asked for, without the one that is gone");
        // Eight fetches one at a time would take eight delays; two waves take two.
        assert!(took < delay * 5, "took {took:?}, which is not several at once");
    }

    #[tokio::test]
    async fn a_fetch_waits_out_too_many_requests_and_tries_again() {
        let base = slow_gmail(std::time::Duration::from_millis(10), "none", "b").await;
        let client = reqwest::Client::new();
        let ids: Vec<String> = ["a", "b", "c"].iter().map(|s| s.to_string()).collect();
        let got = fetch_messages(&client, "token", &base, &ids).await.unwrap();
        let order: Vec<&str> = got.iter().filter_map(|m| m["id"].as_str()).collect();
        assert_eq!(order, ["a", "b", "c"]);
    }

    #[test]
    fn a_rate_limit_wait_follows_retry_after_within_bounds() {
        let header = |v: &'static str| reqwest::header::HeaderValue::from_static(v);
        assert_eq!(rate_limit_wait(Some(&header("5"))).as_secs(), 5);
        assert_eq!(rate_limit_wait(Some(&header("600"))).as_secs(), MAX_RATE_LIMIT_WAIT_SECS);
        assert_eq!(rate_limit_wait(Some(&header("0"))).as_secs(), 1);
        assert_eq!(rate_limit_wait(Some(&header("Wed, 21 Oct 2026 07:28:00 GMT"))).as_secs(), 2);
        assert_eq!(rate_limit_wait(None).as_secs(), 2);
    }

    #[test]
    fn history_folds_repeats_and_drops_what_was_deleted() {
        let records = vec![
            json!({ "id": "10", "messagesAdded": [
                { "message": { "id": "a", "labelIds": ["INBOX", "UNREAD"] } },
                { "message": { "id": "draft", "labelIds": ["DRAFT"] } },
                { "message": { "id": "gone", "labelIds": ["INBOX"] } }
            ]}),
            json!({ "id": "11", "labelsRemoved": [
                { "message": { "id": "old", "labelIds": ["INBOX"] }, "labelIds": ["UNREAD"] },
                { "message": { "id": "a", "labelIds": ["INBOX"] }, "labelIds": ["UNREAD"] }
            ]}),
            json!({ "id": "12", "labelsRemoved": [
                { "message": { "id": "old", "labelIds": [] }, "labelIds": ["INBOX"] }
            ]}),
            json!({ "id": "13", "messagesDeleted": [ { "message": { "id": "gone" } } ] }),
        ];
        assert_eq!(
            Changes::from_records(&records),
            Changes {
                added: vec!["a".into()],
                labels: vec![("old".into(), vec![])],
                deleted: vec!["gone".into()],
            }
        );
    }
}
