//! Outlook mail sync, from Microsoft Graph through virtues-api.
//!
//! Cron-driven, per-credential. Reads three well-known folders (`inbox`,
//! `sentitems`, `archive`), each through its own Graph delta query:
//!
//! * **First run** for a folder (no saved link): ask for the messages received
//!   in the last 30 days, then follow `@odata.nextLink` pages until Graph
//!   hands back an `@odata.deltaLink`.
//! * **Later runs**: open the saved `@odata.deltaLink`, which returns only
//!   what changed since, and save the new one.
//!
//! A run that reaches the page limit saves the next page's link instead, so
//! the next run resumes there. Links are saved as Graph wrote them and
//! rebuilt under `api_base` when used, so a changed `api_base` still works.
//!
//! When Graph no longer knows a folder's sync state (410 Gone, or the codes
//! `syncStateNotFound`/`resyncRequired`), that folder starts its first run
//! again. A folder the mailbox does not have (404) is skipped.
//!
//! Folders are independent: one failing is reported on stderr and the run
//! still exits clean, so the links of the folders that worked are saved. The
//! runner discards a failed run's state. Only when every folder fails, or the
//! sign-in ended, does the run fail.
//!
//! The applet only reads. Its scopes are read-only, and virtues-api forwards
//! only the GET calls listed in `SKILL.md`.

mod transform;

use std::collections::HashMap;

use anyhow::{anyhow, bail, Result};
use serde_json::Value;
use virtues::storage::lake;
use virtues_applets::{graph_error, graph_get, graph_link};
use virtues_helpers::{connect_from_env, output_with_records, read_input};

const ACTION: &str = "outlook_mail_sync";
const FOLDERS: [&str; 3] = ["inbox", transform::SENT_FOLDER, "archive"];
/// Pages per folder per run. At 50 messages a page this is 1,000 messages,
/// which fits the applet's time limit with room for all three folders.
const MAX_PAGES_PER_FOLDER: usize = 20;
const FIRST_RUN_LOOKBACK_DAYS: i64 = 30;
/// Text bodies and 50 messages a page. Graph may ignore either, so the
/// transform still handles HTML bodies.
const PREFER: &str = r#"outlook.body-content-type="text", odata.maxpagesize=50"#;
const SELECT: &str = "id,internetMessageId,conversationId,subject,bodyPreview,body,from,toRecipients,\
     ccRecipients,bccRecipients,isRead,flag,hasAttachments,categories,receivedDateTime,sentDateTime,isDraft";

#[tokio::main]
async fn main() -> Result<()> {
    virtues_applets::init_tracing();

    let mut input = read_input()?;
    let pool = connect_from_env("virtues-action-outlook_mail_sync").await?;
    let token = virtues_applets::secret(&input, "access_token")?.to_string();
    let graph = virtues_applets::outlook_graph(&input)?;
    let storage = lake::storage_from_env()?;
    let client = virtues_applets::http_client();

    let user_email = match virtues_applets::config_str(&input, "user_email").filter(|e| !e.is_empty()) {
        Some(e) => e.to_string(),
        None => {
            let me = graph_get(&client, &token, &format!("{graph}/me"), None).await?;
            let email = own_address(&me).ok_or_else(|| anyhow!("Graph /me has no mail or userPrincipalName"))?;
            input.config["user_email"] = Value::from(email.clone());
            email
        }
    };

    let mut links: HashMap<String, String> = input
        .config
        .get("delta_links")
        .and_then(Value::as_object)
        .map(|o| o.iter().filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string()))).collect())
        .unwrap_or_default();

    let sync = Sync { client: &client, token: &token, graph: &graph, pool: &pool, storage: &storage, user_email: &user_email };
    let mut totals = transform::Written::default();
    let mut failures: Vec<String> = Vec::new();
    let mut attempted = 0usize;

    for folder in FOLDERS {
        attempted += 1;
        let saved = links.get(folder).cloned();
        let result = match sync.folder(folder, saved.as_deref()).await {
            Err(e) if saved.is_some() && needs_resync(&e) => {
                tracing::warn!(folder, "Graph no longer knows this folder's sync state; starting it again");
                links.remove(folder);
                sync.folder(folder, None).await
            }
            other => other,
        };
        match result {
            Ok(run) => {
                totals.added += run.written.added;
                totals.changed += run.written.changed;
                links.insert(folder.to_string(), run.link);
            }
            Err(e) if graph_error(&e).is_some_and(|g| g.is_reauth()) => return Err(e),
            Err(e) if graph_error(&e).is_some_and(|g| g.status == 404) => {
                tracing::info!(folder, "this mailbox has no such folder; skipping it");
                attempted -= 1;
                links.remove(folder);
            }
            Err(e) => {
                tracing::warn!(folder, error = %e, "folder sync failed");
                failures.push(format!("{folder}: {e:#}"));
            }
        }
    }

    input.config["delta_links"] = serde_json::json!(links);

    if !failures.is_empty() {
        if failures.len() == attempted {
            bail!("every Outlook folder failed: {}", failures.join("; "));
        }
        eprintln!("{} of {attempted} Outlook folders failed: {}", failures.len(), failures.join("; "));
    }

    let summary = if totals.added == 0 && totals.changed == 0 {
        "no new Outlook messages".to_string()
    } else {
        format!("synced {} new Outlook messages, {} changes", totals.added, totals.changed)
    };
    output_with_records(&summary, &input.config, totals.added as i64 + totals.changed as i64)
}

struct Sync<'a> {
    client: &'a reqwest::Client,
    token: &'a str,
    /// `{api_base}/v1.0`.
    graph: &'a str,
    pool: &'a sqlx::PgPool,
    storage: &'a virtues::storage::Storage,
    user_email: &'a str,
}

struct FolderRun {
    /// The link the next run opens: a deltaLink when the folder was read to
    /// the end, else the next page's nextLink.
    link: String,
    written: transform::Written,
}

impl Sync<'_> {
    /// Read one folder's delta from `saved` (a link from an earlier run), or
    /// from the start when there is none.
    async fn folder(&self, folder: &str, saved: Option<&str>) -> Result<FolderRun> {
        let mut url = match saved {
            Some(link) => graph_link(self.graph, link)?,
            None => first_url(self.graph, folder, chrono::Utc::now()),
        };
        let mut resume = saved.map(String::from);
        let mut written = transform::Written::default();

        for _ in 0..MAX_PAGES_PER_FOLDER {
            let page = graph_get(self.client, self.token, &url, Some(PREFER)).await?;
            let items = page.get("value").and_then(Value::as_array).cloned().unwrap_or_default();
            if !items.is_empty() {
                // The page as Graph sent it, removals and all. The transform
                // reads a few fields; the rest is kept here.
                lake::archive_cloud(self.pool, self.storage, "outlook", ACTION, "outlook_mail", std::slice::from_ref(&page))
                    .await?;
                let w = transform::write_messages(self.pool, self.user_email, folder, &items).await?;
                written.added += w.added;
                written.changed += w.changed;
            }
            if let Some(next) = page.get("@odata.nextLink").and_then(Value::as_str) {
                url = graph_link(self.graph, next)?;
                resume = Some(next.to_string());
                continue;
            }
            if let Some(delta) = page.get("@odata.deltaLink").and_then(Value::as_str) {
                return Ok(FolderRun { link: delta.to_string(), written });
            }
            bail!("Graph returned a {folder} page with neither a nextLink nor a deltaLink");
        }

        tracing::info!(folder, pages = MAX_PAGES_PER_FOLDER, "page limit reached; the next run resumes here");
        let link = resume.ok_or_else(|| anyhow!("no link to resume {folder} from"))?;
        Ok(FolderRun { link, written })
    }
}

/// The first request of a folder's delta: the last 30 days of messages, with
/// only the fields the transform reads. Graph carries both into the links it
/// returns.
fn first_url(graph: &str, folder: &str, now: chrono::DateTime<chrono::Utc>) -> String {
    let since = (now - chrono::Duration::days(FIRST_RUN_LOOKBACK_DAYS)).format("%Y-%m-%dT%H:%M:%SZ");
    let filter = format!("receivedDateTime ge {since}");
    format!(
        "{graph}/me/mailFolders/{folder}/messages/delta?$select={}&$filter={}",
        urlencoding::encode(SELECT),
        urlencoding::encode(&filter)
    )
}

/// The mailbox's own address, from Graph's `/me`.
fn own_address(me: &Value) -> Option<String> {
    ["mail", "userPrincipalName"]
        .iter()
        .find_map(|k| me.get(*k).and_then(Value::as_str).filter(|s| !s.is_empty()))
        .map(String::from)
}

/// Graph no longer has the sync state a saved link points at, and the only
/// way back is to start the folder again.
fn needs_resync(err: &anyhow::Error) -> bool {
    graph_error(err).is_some_and(|g| {
        g.status == 410 || matches!(g.code.as_deref(), Some("syncStateNotFound" | "resyncRequired"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use virtues_applets::GraphError;

    #[test]
    fn a_first_run_asks_for_thirty_days_with_the_fields_it_reads() {
        let now = "2026-11-21T08:30:00Z".parse().unwrap();
        let url = first_url("https://api.example.com/v1/outlook/c/conn_1/v1.0", "inbox", now);
        assert!(url.starts_with("https://api.example.com/v1/outlook/c/conn_1/v1.0/me/mailFolders/inbox/messages/delta?$select="));
        assert!(url.contains("$filter=receivedDateTime%20ge%202026-10-22T08%3A30%3A00Z"));
        assert!(url.contains("internetMessageId%2CconversationId"));
    }

    #[test]
    fn the_own_address_is_mail_then_the_principal_name() {
        let both = serde_json::json!({ "mail": "david@example.com", "userPrincipalName": "dokafor@example.com" });
        assert_eq!(own_address(&both).as_deref(), Some("david@example.com"));
        let upn = serde_json::json!({ "mail": null, "userPrincipalName": "dokafor@example.com" });
        assert_eq!(own_address(&upn).as_deref(), Some("dokafor@example.com"));
        assert_eq!(own_address(&serde_json::json!({})), None);
    }

    #[test]
    fn a_lost_sync_state_starts_the_folder_again() {
        let err = |status, body: &str| anyhow::Error::new(GraphError::from_body(status, body));
        assert!(needs_resync(&err(410, "{}")));
        assert!(needs_resync(&err(400, r#"{"error":{"code":"syncStateNotFound","message":"x"}}"#)));
        assert!(needs_resync(&err(400, r#"{"error":{"code":"resyncRequired","message":"x"}}"#)));
        assert!(!needs_resync(&err(400, r#"{"error":{"code":"BadRequest","message":"x"}}"#)));
        assert!(!needs_resync(&anyhow!("network down")));
    }
}
