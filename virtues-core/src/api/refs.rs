//! Server-side resolution of ref URLs to human names.
//!
//! A ref URL (`/person/per_123`, `/drive/dr_abc`, `/page/pg_9`) is the one
//! identifier every collection in the system shares — notebook members, pins,
//! citations. Until now only the browser could turn one into a name, in
//! [`refSummary.ts`] and `NotebookDetailView`, one fetch per ref.
//!
//! The server needs the same thing for a different reason: text the model
//! reads. A bare `/drive/dr_abc` in a prompt is unreadable — the model cannot
//! tell a screenshot from a spreadsheet without spending a tool call to find
//! out, and it will spend several. So resolution here is BATCHED (one query
//! per type, not per ref) and returns the two facts a URL hides: what the
//! thing is called, and whether its text is actually readable.
//!
//! Unresolvable refs are simply absent from the returned map. A ref can point
//! at a deleted row, and a missing name is a cosmetic loss — never an error
//! worth failing a chat turn over.

use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::collections::HashMap;

/// A ref URL resolved to what a reader needs to recognize it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedRef {
    /// The URL this resolves, echoed back so callers can key on it.
    pub url: String,
    /// `person` | `place` | `org` | `thing` | `page` | `drive` | `day` | `year` | `web`
    pub kind: String,
    /// Display name. Never empty — falls back to the id, then the URL.
    pub title: String,
    /// Drive files only: the stored mime type, when known.
    pub mime: Option<String>,
    /// Drive files only: whether extracted text exists for retrieval.
    /// `indexed` | `pending` | `none`. See [`text_state`].
    pub text: Option<String>,
}

/// Map `app_drive_files.extraction_status` onto what a reader can act on.
///
/// The status column has six values that answer an operational question (did
/// the extraction job finish?). A reader is asking a different one: is there
/// text here to search? `failed`, `skipped` and `no_text` are distinct
/// histories with one identical consequence — nothing to retrieve — and
/// collapsing them keeps the model from theorizing about a distinction that
/// does not change what it should do next.
fn text_state(extraction_status: &str) -> &'static str {
    match extraction_status {
        "done" => "indexed",
        "pending" | "extracting" => "pending",
        _ => "none",
    }
}

/// Split `/type/id` into its parts. Returns None for anything that isn't a
/// two-segment app ref (external URLs are handled separately).
fn split_ref(url: &str) -> Option<(&str, &str)> {
    let rest = url.strip_prefix('/')?;
    let (kind, id) = rest.split_once('/')?;
    if kind.is_empty() || id.is_empty() {
        return None;
    }
    Some((kind, id))
}

/// Resolve many ref URLs at once.
///
/// One query per distinct type present, regardless of how many refs there are
/// — a 100-member notebook costs at most a handful of round trips, where the
/// browser's per-ref approach would cost 100.
pub async fn resolve_refs(pool: &PgPool, urls: &[String]) -> HashMap<String, ResolvedRef> {
    let mut out: HashMap<String, ResolvedRef> = HashMap::new();

    // Bucket by type so each type is one query. `ids_by_kind[kind]` holds the
    // ids; `urls_by_id` maps back, because two URLs can share an id.
    let mut ids_by_kind: HashMap<&str, Vec<String>> = HashMap::new();
    let mut url_of: HashMap<(&str, String), Vec<&String>> = HashMap::new();

    for url in urls {
        if url.starts_with("http://") || url.starts_with("https://") {
            out.insert(
                url.clone(),
                ResolvedRef {
                    url: url.clone(),
                    kind: "web".to_string(),
                    title: web_title(url),
                    mime: None,
                    text: None,
                },
            );
            continue;
        }

        let Some((kind, id)) = split_ref(url) else {
            continue;
        };

        // A day or year IS its id — `/day/2026-08-01` needs no lookup, and
        // hitting the database to learn that a date is called its own date
        // would be silly.
        if kind == "day" || kind == "year" {
            out.insert(
                url.clone(),
                ResolvedRef {
                    url: url.clone(),
                    kind: kind.to_string(),
                    title: id.to_string(),
                    mime: None,
                    text: None,
                },
            );
            continue;
        }

        ids_by_kind.entry(kind).or_default().push(id.to_string());
        url_of.entry((kind, id.to_string())).or_default().push(url);
    }

    for (kind, ids) in ids_by_kind {
        let rows = fetch_names(pool, kind, &ids).await;
        for (id, title, mime, status) in rows {
            let Some(urls_for_id) = url_of.get(&(kind, id.clone())) else {
                continue;
            };
            for url in urls_for_id {
                out.insert(
                    (*url).clone(),
                    ResolvedRef {
                        url: (*url).clone(),
                        kind: kind.to_string(),
                        title: title.clone(),
                        mime: mime.clone(),
                        text: status.as_deref().map(|s| text_state(s).to_string()),
                    },
                );
            }
        }
    }

    out
}

/// Hostname for an external link — `www.` stripped, since it is noise in a
/// list and never the distinguishing part.
fn web_title(url: &str) -> String {
    let after_scheme = url.split_once("://").map(|(_, r)| r).unwrap_or(url);
    let host = after_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or(after_scheme);
    host.strip_prefix("www.").unwrap_or(host).to_string()
}

/// One batched lookup for a single ref type.
///
/// Returns `(id, title, mime, extraction_status)`. Only drive files populate
/// the last two. A database error resolves to no rows rather than propagating:
/// see the module note on why a missing name must not fail the caller.
async fn fetch_names(
    pool: &PgPool,
    kind: &str,
    ids: &[String],
) -> Vec<(String, String, Option<String>, Option<String>)> {
    // Drive files carry two extra columns, so they get their own query rather
    // than a lowest-common-denominator shape shared with the entity tables.
    if kind == "drive" {
        let rows = sqlx::query_as::<_, (String, String, Option<String>, String, bool)>(
            r#"SELECT id, filename, mime_type, extraction_status, is_folder
               FROM app_drive_files
               WHERE id = ANY($1) AND deleted_at IS NULL"#,
        )
        .bind(ids)
        .fetch_all(pool)
        .await;

        return match rows {
            Ok(rows) => rows
                .into_iter()
                .map(|(id, filename, mime, status, is_folder)| {
                    // A folder carries no mime and lands on extraction_status
                    // 'skipped' like an unreadable file does — reporting one as
                    // `text="none"` would invite the model to announce that a
                    // folder's contents could not be extracted. It has none.
                    if is_folder {
                        (id, filename, None, None)
                    } else {
                        (id, filename, mime, Some(status))
                    }
                })
                .collect(),
            Err(e) => {
                tracing::warn!("[refs] drive lookup failed: {}", e);
                Vec::new()
            }
        };
    }

    // `COALESCE(NULLIF(...))` rather than a plain column: an entity row can
    // carry an empty-string name, and an empty title in a list is worse than
    // the id — it looks like a rendering bug.
    let sql = match kind {
        "person" => {
            "SELECT id, COALESCE(NULLIF(canonical_name, ''), NULLIF(nickname, ''), id) \
             FROM wiki_people WHERE id = ANY($1)"
        }
        "place" => "SELECT id, COALESCE(NULLIF(name, ''), id) FROM wiki_places WHERE id = ANY($1)",
        "org" => {
            "SELECT id, COALESCE(NULLIF(canonical_name, ''), id) FROM wiki_orgs WHERE id = ANY($1)"
        }
        "thing" => "SELECT id, COALESCE(NULLIF(name, ''), id) FROM wiki_things WHERE id = ANY($1)",
        "page" => {
            "SELECT id, COALESCE(NULLIF(title, ''), 'Untitled page') \
             FROM app_pages WHERE id = ANY($1)"
        }
        // Chats and notebooks are ordinary members — a notebook routinely
        // collects the conversations that happened inside it, and pins point
        // at sibling notebooks. Both were reaching the prompt as bare ids.
        "chat" => {
            "SELECT id, COALESCE(NULLIF(title, ''), 'Untitled chat') \
             FROM app_chats WHERE id = ANY($1)"
        }
        "notebook" => {
            "SELECT id, COALESCE(NULLIF(name, ''), 'Untitled notebook') \
             FROM app_notebooks WHERE id = ANY($1)"
        }
        _ => return Vec::new(),
    };

    match sqlx::query_as::<_, (String, String)>(sql)
        .bind(ids)
        .fetch_all(pool)
        .await
    {
        Ok(rows) => rows
            .into_iter()
            .map(|(id, title)| (id, title, None, None))
            .collect(),
        Err(e) => {
            tracing::warn!("[refs] {} lookup failed: {}", kind, e);
            Vec::new()
        }
    }
}

/// Best-effort name for a single ref. Falls back to the URL itself.
pub async fn resolve_one(pool: &PgPool, url: &str) -> ResolvedRef {
    let urls = vec![url.to_string()];
    resolve_refs(pool, &urls)
        .await
        .remove(url)
        .unwrap_or_else(|| ResolvedRef {
            url: url.to_string(),
            kind: split_ref(url)
                .map(|(k, _)| k.to_string())
                .unwrap_or_else(|| "unknown".to_string()),
            title: url.to_string(),
            mime: None,
            text: None,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_app_refs() {
        assert_eq!(split_ref("/person/per_1"), Some(("person", "per_1")));
        assert_eq!(split_ref("/drive/dr_abc"), Some(("drive", "dr_abc")));
        // Ids containing slashes stay whole — the first segment is the type.
        assert_eq!(split_ref("/page/a/b"), Some(("page", "a/b")));
        assert_eq!(split_ref("/person"), None);
        assert_eq!(split_ref("/person/"), None);
        assert_eq!(split_ref("person/per_1"), None);
    }

    #[test]
    fn text_state_collapses_every_dead_end_to_none() {
        assert_eq!(text_state("done"), "indexed");
        assert_eq!(text_state("pending"), "pending");
        assert_eq!(text_state("extracting"), "pending");
        // The three distinct histories a reader cannot act on differently.
        assert_eq!(text_state("skipped"), "none");
        assert_eq!(text_state("no_text"), "none");
        assert_eq!(text_state("failed"), "none");
    }

    #[test]
    fn web_titles_are_bare_hostnames() {
        assert_eq!(web_title("https://www.example.com/a/b?c=1"), "example.com");
        assert_eq!(web_title("http://example.com"), "example.com");
    }
}

/// Live-database checks. Ignored by default — they need a real box database,
/// which CI does not have. Run against a dev checkout with:
///   cargo test -p virtues --lib api::refs::live -- --ignored --nocapture
#[cfg(test)]
mod live {
    use super::*;

    async fn pool() -> PgPool {
        let url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://virtues:virtues@localhost:5432/virtues".to_string());
        PgPool::connect(&url).await.expect("dev database")
    }

    #[tokio::test]
    #[ignore]
    async fn resolves_every_kind_a_real_notebook_actually_contains() {
        let pool = pool().await;

        let urls: Vec<String> = sqlx::query_scalar("SELECT DISTINCT url FROM app_notebook_items")
            .fetch_all(&pool)
            .await
            .expect("notebook items");
        assert!(!urls.is_empty(), "no notebook members to resolve");

        let resolved = resolve_refs(&pool, &urls).await;

        // Every kind that appears in real data must resolve to something a
        // reader can use. The point of this test is that the KINDS are
        // discovered from the data rather than from my imagination — /chat and
        // /notebook members were both missed on the first pass.
        let mut unresolved: Vec<&String> = Vec::new();
        for url in &urls {
            // A single-segment route like /home names no record; nothing to look up.
            if split_ref(url).is_none() {
                continue;
            }
            match resolved.get(url) {
                Some(r) => assert!(!r.title.trim().is_empty(), "empty title for {url}"),
                None => unresolved.push(url),
            }
        }

        // A ref can point at a deleted row, so absence is legal — but it must
        // be rare. A whole unhandled kind shows up as a cluster.
        let mut by_kind: std::collections::HashMap<&str, usize> = Default::default();
        for url in &unresolved {
            *by_kind.entry(split_ref(url).unwrap().0).or_default() += 1;
        }
        for (kind, missing) in &by_kind {
            let total = urls
                .iter()
                .filter(|u| split_ref(u).map(|(k, _)| k) == Some(kind))
                .count();
            assert!(
                *missing < total,
                "kind `{kind}` never resolves ({missing}/{total}) — it is unhandled, not deleted"
            );
        }
        println!("resolved {}/{} refs", resolved.len(), urls.len());
    }

    #[tokio::test]
    #[ignore]
    async fn folders_do_not_masquerade_as_unreadable_files() {
        let pool = pool().await;
        let folder: Option<String> =
            sqlx::query_scalar("SELECT id FROM app_drive_files WHERE is_folder = TRUE LIMIT 1")
                .fetch_optional(&pool)
                .await
                .expect("query");
        let Some(id) = folder else {
            println!("no folders in this database; nothing to check");
            return;
        };

        let url = format!("/drive/{id}");
        let r = resolve_one(&pool, &url).await;
        assert!(
            r.text.is_none(),
            "folder {id} reported text={:?} — it has no text to extract",
            r.text
        );
    }
}
