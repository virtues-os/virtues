//! Notebooks API — the "room" a chat lives in.
//!
//! A Notebook is a manual collection the user returns to: a project, pet, hobby,
//! goal, or topic. It gathers entities, chats, and pages as URL-native members
//! (`app_notebook_items`) and carries a single accent tint plus a catch-up memo
//! (`current_status`) shown when you re-enter the room.
//!
//! A chat lives in at most one Notebook (`app_chats.notebook_id`). Entering a Notebook
//! weights its members in retrieval; conversely the chat is folded into the
//! Notebook's corpus. Membership is manual in v1 — there is no smart/query view.
//!
//! This absorbs the old workspace-shell and the folder role that `wiki_things`
//! used to play (pins + memo); Things are now pure reference entities.

use crate::error::{Error, Result};
use crate::ids::{generate_id, NOTEBOOK_PREFIX};
use crate::types::Timestamp;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

// ============================================================================
// Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Notebook {
    pub id: String,
    pub name: String,
    pub icon: Option<String>,
    pub accent_color: Option<String>,
    /// Transient "state of the room" catch-up memo (what you read on re-entry).
    pub current_status: Option<String>,
    pub current_status_at: Option<Timestamp>,
    /// Persistent behavior for the assistant in this notebook (Claude-Projects-
    /// style custom instructions) — distinct from the transient memo above.
    pub instructions: Option<String>,
    pub sort_order: i32,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// List-view summary — adds member and chat counts.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct NotebookSummary {
    pub id: String,
    pub name: String,
    pub icon: Option<String>,
    pub accent_color: Option<String>,
    pub current_status: Option<String>,
    pub current_status_at: Option<Timestamp>,
    pub instructions: Option<String>,
    pub sort_order: i32,
    pub item_count: i64,
    pub chat_count: i64,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// A single URL-native member of a Notebook.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct NotebookItem {
    pub url: String,
    pub sort_order: i32,
    /// `library` (grounds chat) | `manuscript` (yours to write; excluded from
    /// retrieval so a draft is never cited back at you) | `pin` (nav-only).
    pub role: String,
    pub added_at: Timestamp,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotebookDetail {
    #[serde(flatten)]
    pub notebook: Notebook,
    pub items: Vec<NotebookItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotebookListResponse {
    pub notebooks: Vec<NotebookSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateNotebookRequest {
    pub name: String,
    pub icon: Option<String>,
    pub accent_color: Option<String>,
}

/// Update a Notebook. `Option<Option<T>>` fields are tri-state: absent = leave,
/// `Some(None)` = clear, `Some(Some(v))` = set.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateNotebookRequest {
    pub name: Option<String>,
    pub icon: Option<Option<String>>,
    pub accent_color: Option<Option<String>>,
    pub current_status: Option<Option<String>>,
    pub instructions: Option<Option<String>>,
    pub sort_order: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddNotebookItemRequest {
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReorderNotebookItemsRequest {
    pub urls: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetNotebookItemRoleRequest {
    pub url: String,
    /// `library` | `manuscript` | `pin`.
    pub role: String,
}

/// One entity referenced across a notebook's members.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotebookGraphNode {
    /// Ref URL for the entity, e.g. `/person/pe_abc` — also the node's identity.
    pub url: String,
    pub entity_type: String,
    pub name: String,
    /// Member urls that reference this entity. Drives click-to-filter.
    pub item_urls: Vec<String>,
}

/// Two entities that appear together in at least one member.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotebookGraphEdge {
    pub source: String,
    pub target: String,
    pub weight: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotebookGraph {
    pub nodes: Vec<NotebookGraphNode>,
    pub edges: Vec<NotebookGraphEdge>,
}

// ============================================================================
// Notebook CRUD
// ============================================================================

/// List all Notebooks, most-recently-active first, with member and chat counts.
pub async fn list_notebooks(pool: &PgPool) -> Result<NotebookListResponse> {
    let notebooks = sqlx::query_as::<_, NotebookSummary>(
        r#"
        SELECT
            s.id, s.name, s.icon, s.accent_color,
            s.current_status, s.current_status_at, s.instructions, s.sort_order,
            COALESCE((SELECT COUNT(*) FROM app_notebook_items WHERE notebook_id = s.id), 0) AS item_count,
            COALESCE((SELECT COUNT(*) FROM app_chats       WHERE notebook_id = s.id), 0) AS chat_count,
            s.created_at, s.updated_at
        FROM app_notebooks s
        ORDER BY s.sort_order ASC, s.updated_at DESC
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to list notebooks: {}", e)))?;

    Ok(NotebookListResponse { notebooks })
}

/// Get a single Notebook with its ordered members.
pub async fn get_notebook(pool: &PgPool, id: &str) -> Result<NotebookDetail> {
    let notebook = sqlx::query_as::<_, Notebook>(
        r#"
        SELECT id, name, icon, accent_color, current_status, current_status_at,
               instructions, sort_order, created_at, updated_at
        FROM app_notebooks
        WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get notebook: {}", e)))?
    .ok_or_else(|| Error::NotFound(format!("Notebook not found: {}", id)))?;

    let items = sqlx::query_as::<_, NotebookItem>(
        r#"
        SELECT url, sort_order, role, added_at
        FROM app_notebook_items
        WHERE notebook_id = $1
        ORDER BY sort_order ASC, added_at ASC
        "#,
    )
    .bind(id)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get notebook items: {}", e)))?;

    Ok(NotebookDetail { notebook, items })
}

/// Create a new Notebook.
pub async fn create_notebook(pool: &PgPool, req: CreateNotebookRequest) -> Result<Notebook> {
    let name = req.name.trim();
    if name.is_empty() {
        return Err(Error::InvalidInput("Notebook name cannot be empty".into()));
    }

    let timestamp = chrono::Utc::now().to_rfc3339();
    let id = generate_id(NOTEBOOK_PREFIX, &[name, &timestamp]);

    let notebook = sqlx::query_as::<_, Notebook>(
        r#"
        INSERT INTO app_notebooks (id, name, icon, accent_color)
        VALUES ($1, $2, $3, $4)
        RETURNING id, name, icon, accent_color, current_status, current_status_at,
                  instructions, sort_order, created_at, updated_at
        "#,
    )
    .bind(&id)
    .bind(name)
    .bind(&req.icon)
    .bind(&req.accent_color)
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to create notebook: {}", e)))?;

    Ok(notebook)
}

/// Update a Notebook. Only provided fields change. Touching `current_status`
/// stamps `current_status_at`.
pub async fn update_notebook(pool: &PgPool, id: &str, req: UpdateNotebookRequest) -> Result<Notebook> {
    let existing = sqlx::query_as::<_, Notebook>(
        r#"
        SELECT id, name, icon, accent_color, current_status, current_status_at,
               instructions, sort_order, created_at, updated_at
        FROM app_notebooks WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get notebook: {}", e)))?
    .ok_or_else(|| Error::NotFound(format!("Notebook not found: {}", id)))?;

    let name = req.name.as_deref().unwrap_or(&existing.name).trim().to_string();
    if name.is_empty() {
        return Err(Error::InvalidInput("Notebook name cannot be empty".into()));
    }

    let icon = match req.icon {
        Some(val) => val,
        None => existing.icon,
    };
    let accent_color = match req.accent_color {
        Some(val) => val,
        None => existing.accent_color,
    };
    let status_changed = req.current_status.is_some();
    let current_status = match req.current_status {
        Some(val) => val,
        None => existing.current_status,
    };
    let sort_order = req.sort_order.unwrap_or(existing.sort_order);
    let instructions = match req.instructions {
        Some(val) => val,
        None => existing.instructions,
    };

    let notebook = sqlx::query_as::<_, Notebook>(
        r#"
        UPDATE app_notebooks
        SET name = $2,
            icon = $3,
            accent_color = $4,
            current_status = $5,
            current_status_at = CASE WHEN $6 THEN now() ELSE current_status_at END,
            sort_order = $7,
            instructions = $8
        WHERE id = $1
        RETURNING id, name, icon, accent_color, current_status, current_status_at,
                  instructions, sort_order, created_at, updated_at
        "#,
    )
    .bind(id)
    .bind(&name)
    .bind(&icon)
    .bind(&accent_color)
    .bind(&current_status)
    .bind(status_changed)
    .bind(sort_order)
    .bind(&instructions)
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to update notebook: {}", e)))?;

    Ok(notebook)
}

/// Delete a Notebook. Members cascade; chats in it have `notebook_id` set to NULL.
pub async fn delete_notebook(pool: &PgPool, id: &str) -> Result<()> {
    let result = sqlx::query(r#"DELETE FROM app_notebooks WHERE id = $1"#)
        .bind(id)
        .execute(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to delete notebook: {}", e)))?;

    if result.rows_affected() == 0 {
        return Err(Error::NotFound(format!("Notebook not found: {}", id)));
    }
    Ok(())
}

/// Touch a Notebook's updated_at to reflect activity.
pub async fn touch_notebook(pool: &PgPool, id: &str) -> Result<()> {
    sqlx::query(r#"UPDATE app_notebooks SET updated_at = now() WHERE id = $1"#)
        .bind(id)
        .execute(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to touch notebook: {}", e)))?;
    Ok(())
}

// ============================================================================
// Membership
// ============================================================================

/// Add a member URL to a Notebook. Idempotent on (notebook_id, url).
pub async fn add_notebook_item(pool: &PgPool, notebook_id: &str, req: AddNotebookItemRequest) -> Result<NotebookItem> {
    let url = req.url.trim();
    if url.is_empty() {
        return Err(Error::InvalidInput("Member url cannot be empty".into()));
    }

    let exists: Option<String> = sqlx::query_scalar(r#"SELECT id FROM app_notebooks WHERE id = $1"#)
        .bind(notebook_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to verify notebook: {}", e)))?;
    if exists.is_none() {
        return Err(Error::NotFound(format!("Notebook not found: {}", notebook_id)));
    }

    // role='library' = grounds chat — the default and only v1 role ("Library"
    // as a noun is retired; membership itself means in-scope). 'pin' survives
    // schema-only for future nav-only edges.
    let item = sqlx::query_as::<_, NotebookItem>(
        r#"
        INSERT INTO app_notebook_items (notebook_id, url, sort_order, role)
        VALUES (
            $1, $2,
            (SELECT COALESCE(MAX(sort_order), -1) + 1 FROM app_notebook_items WHERE notebook_id = $1),
            'library'
        )
        -- Re-adding an existing member upgrades a legacy nav-only 'pin' to
        -- 'library', but must not demote a 'manuscript' back to source material.
        ON CONFLICT (notebook_id, url) DO UPDATE SET
            role = CASE WHEN app_notebook_items.role = 'pin'
                        THEN 'library' ELSE app_notebook_items.role END
        RETURNING url, sort_order, role, added_at
        "#,
    )
    .bind(notebook_id)
    .bind(url)
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to add notebook item: {}", e)))?;

    touch_notebook(pool, notebook_id).await.ok();
    Ok(item)
}

/// Remove a member URL from a Notebook.
pub async fn remove_notebook_item(pool: &PgPool, notebook_id: &str, url: &str) -> Result<()> {
    let result = sqlx::query(r#"DELETE FROM app_notebook_items WHERE notebook_id = $1 AND url = $2"#)
        .bind(notebook_id)
        .bind(url)
        .execute(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to remove notebook item: {}", e)))?;

    if result.rows_affected() == 0 {
        return Err(Error::NotFound(format!(
            "Member not found in notebook: {} / {}",
            notebook_id, url
        )));
    }

    touch_notebook(pool, notebook_id).await.ok();
    Ok(())
}

/// Remove all membership entries for a given URL across every Notebook.
/// Called when the underlying entity (chat/page/...) is deleted.
pub async fn remove_items_by_url(pool: &PgPool, url: &str) -> Result<i64> {
    let result = sqlx::query(r#"DELETE FROM app_notebook_items WHERE url = $1"#)
        .bind(url)
        .execute(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to remove items by URL: {}", e)))?;

    Ok(result.rows_affected() as i64)
}

/// Reorder a Notebook's members. Unknown URLs are ignored.
pub async fn reorder_notebook_items(
    pool: &PgPool,
    notebook_id: &str,
    req: ReorderNotebookItemsRequest,
) -> Result<()> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| Error::Database(format!("Failed to start transaction: {}", e)))?;

    for (idx, url) in req.urls.iter().enumerate() {
        sqlx::query(
            r#"UPDATE app_notebook_items SET sort_order = $1 WHERE notebook_id = $2 AND url = $3"#,
        )
        .bind(idx as i64)
        .bind(notebook_id)
        .bind(url)
        .execute(&mut *tx)
        .await
        .map_err(|e| Error::Database(format!("Failed to reorder notebook items: {}", e)))?;
    }

    sqlx::query(r#"UPDATE app_notebooks SET updated_at = now() WHERE id = $1"#)
        .bind(notebook_id)
        .execute(&mut *tx)
        .await
        .ok();

    tx.commit()
        .await
        .map_err(|e| Error::Database(format!("Failed to commit reorder: {}", e)))?;

    Ok(())
}

// ============================================================================
// Chat ↔ Notebook binding (one active Notebook per chat)
// ============================================================================

/// Set or clear a chat's Notebook. Passing `Some(notebook_id)` also folds the chat
/// into that Notebook's membership (idempotent); passing `None` detaches it. The
/// row update and the membership fold run in one transaction so the chat's
/// `notebook_id` and its `/chat/<id>` membership row can never diverge.
pub async fn set_chat_notebook(pool: &PgPool, chat_id: &str, notebook_id: Option<&str>) -> Result<()> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| Error::Database(format!("Failed to start transaction: {}", e)))?;

    sqlx::query(r#"UPDATE app_chats SET notebook_id = $2 WHERE id = $1"#)
        .bind(chat_id)
        .bind(notebook_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| Error::Database(format!("Failed to set chat notebook: {}", e)))?;

    if let Some(notebook_id) = notebook_id {
        sqlx::query(
            r#"
            INSERT INTO app_notebook_items (notebook_id, url, sort_order)
            VALUES (
                $1, $2,
                (SELECT COALESCE(MAX(sort_order), -1) + 1 FROM app_notebook_items WHERE notebook_id = $1)
            )
            ON CONFLICT (notebook_id, url) DO NOTHING
            "#,
        )
        .bind(notebook_id)
        .bind(format!("/chat/{}", chat_id))
        .execute(&mut *tx)
        .await
        .map_err(|e| Error::Database(format!("Failed to fold chat into notebook: {}", e)))?;

        sqlx::query(r#"UPDATE app_notebooks SET updated_at = now() WHERE id = $1"#)
            .bind(notebook_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| Error::Database(format!("Failed to touch notebook: {}", e)))?;
    }

    tx.commit()
        .await
        .map_err(|e| Error::Database(format!("Failed to commit chat notebook binding: {}", e)))?;

    Ok(())
}

/// Set a member's role. `library` grounds chat, `manuscript` is yours to write
/// (kept out of retrieval), `pin` is nav-only.
pub async fn set_notebook_item_role(
    pool: &PgPool,
    notebook_id: &str,
    req: SetNotebookItemRoleRequest,
) -> Result<NotebookItem> {
    if !matches!(req.role.as_str(), "library" | "manuscript" | "pin") {
        return Err(Error::InvalidInput(format!(
            "Unknown notebook item role: {}",
            req.role
        )));
    }

    let item = sqlx::query_as::<_, NotebookItem>(
        r#"
        UPDATE app_notebook_items SET role = $3
        WHERE notebook_id = $1 AND url = $2
        RETURNING url, sort_order, role, added_at
        "#,
    )
    .bind(notebook_id)
    .bind(&req.url)
    .bind(&req.role)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to set notebook item role: {}", e)))?
    .ok_or_else(|| Error::NotFound(format!("Notebook member not found: {}", req.url)))?;

    touch_notebook(pool, notebook_id).await.ok();
    Ok(item)
}

/// Entity ref-URL prefixes that can appear as a notebook member or inside a
/// page's markdown. Kept in sync with the frontend's ref routes.
const ENTITY_PREFIXES: [&str; 4] = ["/person/", "/place/", "/org/", "/thing/"];

/// Pull entity ref URLs out of markdown. Refs are stored inline as
/// `[@Label](/person/pe_x)` — there is no link table — so this scans for the
/// closing `](` of a link and reads the URL, mirroring `get_page_backlinks`.
fn extract_entity_urls(content: &str) -> std::collections::HashSet<String> {
    let mut out = std::collections::HashSet::new();
    for prefix in ENTITY_PREFIXES {
        let needle = format!("]({}", prefix);
        let mut from = 0usize;
        while let Some(hit) = content[from..].find(&needle) {
            let start = from + hit + 2; // skip "]("
            let rest = &content[start..];
            match rest.find(')') {
                Some(end) => {
                    let url = &rest[..end];
                    // Ignore anything with a fragment/query — a ref is a bare route.
                    if !url.is_empty() && !url.contains(['#', '?', ' ']) {
                        out.insert(url.to_string());
                    }
                    from = start + end;
                }
                None => break,
            }
        }
    }
    out
}

/// The entities referenced across a notebook's members, with co-occurrence
/// edges. Nodes come only from things the user explicitly wrote or filed —
/// entity members, and `[@ref]` links inside member pages. Nothing is inferred:
/// there is no NER over free text, so an entity that is merely *mentioned* in a
/// PDF does not appear here.
pub async fn notebook_graph(pool: &PgPool, notebook_id: &str) -> Result<NotebookGraph> {
    use std::collections::{HashMap, HashSet};

    // Nav-only pins are excluded: they are shortcuts, not content.
    let members: Vec<String> = sqlx::query_scalar(
        r#"
        SELECT url FROM app_notebook_items
        WHERE notebook_id = $1 AND role <> 'pin'
        ORDER BY sort_order ASC, added_at ASC
        "#,
    )
    .bind(notebook_id)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to load notebook members: {}", e)))?;

    if members.is_empty() {
        return Ok(NotebookGraph { nodes: Vec::new(), edges: Vec::new() });
    }

    // item url -> the entity urls it references.
    let mut per_item: Vec<(String, HashSet<String>)> = Vec::new();

    // Page members contribute whatever they link to; fetch their content in one go.
    let page_ids: Vec<String> = members
        .iter()
        .filter_map(|u| u.strip_prefix("/page/").map(str::to_string))
        .collect();
    let mut page_content: HashMap<String, String> = HashMap::new();
    if !page_ids.is_empty() {
        let rows: Vec<(String, String)> =
            sqlx::query_as(r#"SELECT id, content FROM app_pages WHERE id = ANY($1)"#)
                .bind(&page_ids)
                .fetch_all(pool)
                .await
                .map_err(|e| Error::Database(format!("Failed to load member pages: {}", e)))?;
        page_content.extend(rows);
    }

    for url in &members {
        let mut refs = HashSet::new();
        if ENTITY_PREFIXES.iter().any(|p| url.starts_with(p)) {
            // An entity filed directly in the notebook is a node in its own right.
            refs.insert(url.clone());
        } else if let Some(pid) = url.strip_prefix("/page/") {
            if let Some(content) = page_content.get(pid) {
                refs = extract_entity_urls(content);
            }
        }
        if !refs.is_empty() {
            per_item.push((url.clone(), refs));
        }
    }

    // Resolve display names. Ids are prefixed and unique across types, so one
    // id array filters every table.
    let entity_ids: Vec<String> = per_item
        .iter()
        .flat_map(|(_, refs)| refs.iter())
        .filter_map(|u| u.rsplit('/').next().map(str::to_string))
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();

    let mut names: HashMap<String, String> = HashMap::new();
    if !entity_ids.is_empty() {
        let rows: Vec<(String, String)> = sqlx::query_as(
            r#"
            SELECT id, canonical_name AS name FROM wiki_people WHERE id = ANY($1)
            UNION ALL SELECT id, name FROM wiki_places WHERE id = ANY($1)
            UNION ALL SELECT id, canonical_name AS name FROM wiki_orgs WHERE id = ANY($1)
            UNION ALL SELECT id, name FROM wiki_things WHERE id = ANY($1)
            "#,
        )
        .bind(&entity_ids)
        .fetch_all(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to resolve entity names: {}", e)))?;
        names.extend(rows);
    }

    // Nodes, in descending reference count so the caller can size them.
    let mut node_items: HashMap<String, Vec<String>> = HashMap::new();
    for (item_url, refs) in &per_item {
        for ent in refs {
            node_items.entry(ent.clone()).or_default().push(item_url.clone());
        }
    }

    let mut nodes: Vec<NotebookGraphNode> = node_items
        .into_iter()
        .filter_map(|(url, item_urls)| {
            let mut parts = url.trim_start_matches('/').splitn(2, '/');
            let entity_type = parts.next()?.to_string();
            let id = parts.next()?.to_string();
            // An unresolvable id is a dangling ref (entity deleted, stale link).
            // Drop it rather than render a node with no name.
            let name = names.get(&id)?.clone();
            Some(NotebookGraphNode { url, entity_type, name, item_urls })
        })
        .collect();
    nodes.sort_by(|a, b| {
        b.item_urls
            .len()
            .cmp(&a.item_urls.len())
            .then_with(|| a.name.cmp(&b.name))
    });

    let live: HashSet<&str> = nodes.iter().map(|n| n.url.as_str()).collect();

    // Edges: entities sharing a member. Undirected, deduped by sorted pair.
    let mut pair_weight: HashMap<(String, String), i64> = HashMap::new();
    for (_, refs) in &per_item {
        let mut present: Vec<&str> = refs
            .iter()
            .map(String::as_str)
            .filter(|u| live.contains(u))
            .collect();
        present.sort_unstable();
        for i in 0..present.len() {
            for j in (i + 1)..present.len() {
                *pair_weight
                    .entry((present[i].to_string(), present[j].to_string()))
                    .or_insert(0) += 1;
            }
        }
    }

    let mut edges: Vec<NotebookGraphEdge> = pair_weight
        .into_iter()
        .map(|((source, target), weight)| NotebookGraphEdge { source, target, weight })
        .collect();
    edges.sort_by(|a, b| b.weight.cmp(&a.weight).then_with(|| a.source.cmp(&b.source)));

    Ok(NotebookGraph { nodes, edges })
}
