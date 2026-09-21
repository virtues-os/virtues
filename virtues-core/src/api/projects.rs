//! Projects API — the "room" a chat lives in.
//!
//! A Project is a manual collection the user returns to: an undertaking, a pet,
//! a hobby, a goal, or a topic. It gathers entities, chats, and pages as
//! URL-native members (`app_project_items`) and carries a single accent tint
//! plus a catch-up memo (`current_status`) shown when you re-enter the room.
//!
//! A chat lives in at most one Project (`app_chats.project_id`). Entering a Project
//! weights its members in retrieval; conversely the chat is folded into the
//! Project's corpus. Membership is manual in v1 — there is no smart/query view.
//! A project cannot be a member of a project — you cannot folder a folder.
//!
//! Lineage: this absorbs the old workspace-shell and the folder role that the
//! retired "Things" feature used to play (pins + memo). Things are gone
//! entirely as of migration 0060 — projects and hobbies became notebooks (now
//! projects) and stories. Notebooks were renamed to projects in migration 0029
//! because "project" is the clearer word for users; ids keep their `nb_`
//! prefix (see `ids::PROJECT_PREFIX`) and `/notebook/{id}` stays an accepted
//! legacy spelling of the ref URL (see `refs::split_ref`).

use crate::error::{Error, Result};
use crate::ids::{generate_id, PROJECT_PREFIX};
use crate::types::Timestamp;
use serde::{Deserialize, Deserializer, Serialize};
use sqlx::PgPool;

/// Distinguishes a missing field from an explicit `null` for `Option<Option<T>>`:
/// missing → `None` (leave alone), `null` → `Some(None)` (clear), value → set.
/// Without this, serde folds an explicit `null` into `None`, so a field can be
/// set but never cleared — which is why "Remove icon" silently did nothing.
fn deserialize_double_option<'de, D, T>(
    deserializer: D,
) -> std::result::Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Ok(Some(Option::deserialize(deserializer)?))
}

// ============================================================================
// Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub icon: Option<String>,
    pub accent_color: Option<String>,
    /// Transient "state of the room" catch-up memo (what you read on re-entry).
    pub current_status: Option<String>,
    pub current_status_at: Option<Timestamp>,
    /// Persistent behavior for the assistant in this project (Claude-Projects-
    /// style custom instructions) — distinct from the transient memo above.
    pub instructions: Option<String>,
    pub sort_order: i32,
    /// Finished, kept, out of the working view. Distinct from `deleted_at`
    /// (the trash): an archived project is one the owner closed, a trashed one
    /// is one they removed. Archived projects leave the Home panel, the
    /// projects list and the "Add to project" submenu; their chats and pages
    /// stay filed in them, and the project reopens from the Archived fold.
    pub archived_at: Option<Timestamp>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// List-view summary — adds member and chat counts.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ProjectSummary {
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
    pub archived_at: Option<Timestamp>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// A single URL-native member of a Project.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ProjectItem {
    pub url: String,
    pub sort_order: i32,
    /// `library` (grounds chat) | `manuscript` (yours to write; excluded from
    /// retrieval so a draft is never cited back at you) | `pin` (nav-only).
    pub role: String,
    pub added_at: Timestamp,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectDetail {
    #[serde(flatten)]
    pub project: Project,
    pub items: Vec<ProjectItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectListResponse {
    pub projects: Vec<ProjectSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateProjectRequest {
    pub name: String,
    pub icon: Option<String>,
    pub accent_color: Option<String>,
}

/// Update a Project. `Option<Option<T>>` fields are tri-state: absent = leave,
/// `Some(None)` = clear, `Some(Some(v))` = set.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateProjectRequest {
    pub name: Option<String>,
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub icon: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub accent_color: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub current_status: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub instructions: Option<Option<String>>,
    pub sort_order: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddProjectItemRequest {
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReorderProjectItemsRequest {
    pub urls: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetProjectItemRoleRequest {
    pub url: String,
    /// `library` | `manuscript` | `pin`.
    pub role: String,
}

/// One entity referenced across a project's members.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectGraphNode {
    /// Ref URL for the entity, e.g. `/person/pe_abc` — also the node's identity.
    pub url: String,
    pub entity_type: String,
    pub name: String,
    /// Member urls that reference this entity. Drives click-to-filter.
    pub item_urls: Vec<String>,
}

/// Two entities that appear together in at least one member.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectGraphEdge {
    pub source: String,
    pub target: String,
    pub weight: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectGraph {
    pub nodes: Vec<ProjectGraphNode>,
    pub edges: Vec<ProjectGraphEdge>,
}

// ============================================================================
// Project CRUD
// ============================================================================

/// List Projects, most-recently-active first, with member and chat counts.
/// Live ones only unless `include_archived` — the projects page asks for both
/// and folds the archived ones; every other reader (Home panel, ⌘K, the
/// "Add to project" submenu) wants the working set.
///
/// A trashed member keeps its membership row so a restore is whole (see
/// `api::trash`), which is why the counts and `get_project`'s member list
/// filter members through their own table's `deleted_at` rather than trusting
/// the row's existence.
pub async fn list_projects(pool: &PgPool, include_archived: bool) -> Result<ProjectListResponse> {
    let projects = sqlx::query_as::<_, ProjectSummary>(
        r#"
        SELECT
            s.id, s.name, s.icon, s.accent_color,
            s.current_status, s.current_status_at, s.instructions, s.sort_order, s.archived_at,
            COALESCE((SELECT COUNT(*) FROM app_project_items
                      WHERE project_id = s.id
              AND NOT EXISTS (SELECT 1 FROM app_pages p WHERE url = '/page/' || p.id AND p.deleted_at IS NOT NULL)
              AND NOT EXISTS (SELECT 1 FROM app_chats c WHERE url = '/chat/' || c.id AND c.deleted_at IS NOT NULL)), 0) AS item_count,
            COALESCE((SELECT COUNT(*) FROM app_chats
                      WHERE project_id = s.id AND deleted_at IS NULL), 0) AS chat_count,
            s.created_at, s.updated_at
        FROM app_projects s
        WHERE s.deleted_at IS NULL AND ($1 OR s.archived_at IS NULL)
        ORDER BY s.sort_order ASC, s.updated_at DESC
        "#,
    )
    .bind(include_archived)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to list projects: {}", e)))?;

    Ok(ProjectListResponse { projects })
}

/// Get a single Project with its ordered members.
pub async fn get_project(pool: &PgPool, id: &str) -> Result<ProjectDetail> {
    let project = sqlx::query_as::<_, Project>(
        r#"
        SELECT id, name, icon, accent_color, current_status, current_status_at,
               instructions, sort_order, archived_at, created_at, updated_at
        FROM app_projects
        WHERE id = $1 AND deleted_at IS NULL
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get project: {}", e)))?
    .ok_or_else(|| Error::NotFound(format!("Project not found: {}", id)))?;

    let items = sqlx::query_as::<_, ProjectItem>(
        r#"
        SELECT url, sort_order, role, added_at
        FROM app_project_items
        WHERE project_id = $1
              AND NOT EXISTS (SELECT 1 FROM app_pages p WHERE url = '/page/' || p.id AND p.deleted_at IS NOT NULL)
              AND NOT EXISTS (SELECT 1 FROM app_chats c WHERE url = '/chat/' || c.id AND c.deleted_at IS NOT NULL)
        ORDER BY sort_order ASC, added_at ASC
        "#,
    )
    .bind(id)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get project items: {}", e)))?;

    Ok(ProjectDetail { project, items })
}

/// Create a new Project.
pub async fn create_project(pool: &PgPool, req: CreateProjectRequest) -> Result<Project> {
    let name = req.name.trim();
    if name.is_empty() {
        return Err(Error::InvalidInput("Project name cannot be empty".into()));
    }

    let timestamp = chrono::Utc::now().to_rfc3339();
    let id = generate_id(PROJECT_PREFIX, &[name, &timestamp]);

    let project = sqlx::query_as::<_, Project>(
        r#"
        INSERT INTO app_projects (id, name, icon, accent_color)
        VALUES ($1, $2, $3, $4)
        RETURNING id, name, icon, accent_color, current_status, current_status_at,
                  instructions, sort_order, archived_at, created_at, updated_at
        "#,
    )
    .bind(&id)
    .bind(name)
    .bind(&req.icon)
    .bind(&req.accent_color)
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to create project: {}", e)))?;

    Ok(project)
}

/// Update a Project. Only provided fields change. Touching `current_status`
/// stamps `current_status_at`.
pub async fn update_project(pool: &PgPool, id: &str, req: UpdateProjectRequest) -> Result<Project> {
    let existing = sqlx::query_as::<_, Project>(
        r#"
        SELECT id, name, icon, accent_color, current_status, current_status_at,
               instructions, sort_order, archived_at, created_at, updated_at
        FROM app_projects WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get project: {}", e)))?
    .ok_or_else(|| Error::NotFound(format!("Project not found: {}", id)))?;

    let name = req.name.as_deref().unwrap_or(&existing.name).trim().to_string();
    if name.is_empty() {
        return Err(Error::InvalidInput("Project name cannot be empty".into()));
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

    let project = sqlx::query_as::<_, Project>(
        r#"
        UPDATE app_projects
        SET name = $2,
            icon = $3,
            accent_color = $4,
            current_status = $5,
            current_status_at = CASE WHEN $6 THEN now() ELSE current_status_at END,
            sort_order = $7,
            instructions = $8
        WHERE id = $1
        RETURNING id, name, icon, accent_color, current_status, current_status_at,
                  instructions, sort_order, archived_at, created_at, updated_at
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
    .map_err(|e| Error::Database(format!("Failed to update project: {}", e)))?;

    Ok(project)
}

/// Close a project: it leaves the working surfaces and keeps everything.
/// Idempotent — archiving an archived project is not an error.
pub async fn archive_project(pool: &PgPool, id: &str) -> Result<()> {
    set_archived(pool, id, true).await
}

/// Reopen an archived project.
pub async fn unarchive_project(pool: &PgPool, id: &str) -> Result<()> {
    set_archived(pool, id, false).await
}

async fn set_archived(pool: &PgPool, id: &str, archived: bool) -> Result<()> {
    let sql = if archived {
        "UPDATE app_projects SET archived_at = COALESCE(archived_at, now()) \
         WHERE id = $1 AND deleted_at IS NULL RETURNING id"
    } else {
        "UPDATE app_projects SET archived_at = NULL \
         WHERE id = $1 AND deleted_at IS NULL RETURNING id"
    };
    let hit: Option<String> = sqlx::query_scalar(sql)
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to set archived on project {id}: {e}")))?;
    if hit.is_none() {
        return Err(Error::NotFound(format!("Project not found: {id}")));
    }
    Ok(())
}

/// Delete a Project — into the trash. Members and the chats' `project_id`
/// stay on the row so a restore brings the project back whole; the FK
/// cascade and SET NULL only fire on `trash::purge`, the hard delete the
/// trash and the sweeper reach after `TRASH_RETENTION_DAYS`.
pub async fn delete_project(pool: &PgPool, id: &str) -> Result<()> {
    crate::api::trash::trash(pool, crate::api::trash::TrashKind::Project, id).await
}

/// Touch a Project's updated_at to reflect activity.
pub async fn touch_project(pool: &PgPool, id: &str) -> Result<()> {
    sqlx::query(r#"UPDATE app_projects SET updated_at = now() WHERE id = $1"#)
        .bind(id)
        .execute(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to touch project: {}", e)))?;
    Ok(())
}

// ============================================================================
// Membership
// ============================================================================

/// Add a member URL to a Project. Idempotent on (project_id, url).
pub async fn add_project_item(pool: &PgPool, project_id: &str, req: AddProjectItemRequest) -> Result<ProjectItem> {
    let url = req.url.trim();
    if url.is_empty() {
        return Err(Error::InvalidInput("Member url cannot be empty".into()));
    }

    // A project cannot be a member of a project — you cannot folder a folder.
    // Before the rename a nested notebook was a nav-only 'pin' edge and a
    // self-reference was a cycle in scope resolution; migration 0029 deleted
    // the rows that existed, and this is what keeps them from coming back.
    // `/notebook/` is the legacy spelling of the same route (`refs::split_ref`).
    if url.starts_with("/project/") || url.starts_with("/notebook/") {
        return Err(Error::InvalidInput(
            "You can't put a project inside another project.".into(),
        ));
    }

    let found: Option<Option<Timestamp>> = sqlx::query_scalar(
        r#"SELECT archived_at FROM app_projects WHERE id = $1 AND deleted_at IS NULL"#,
    )
    .bind(project_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to verify project: {}", e)))?;
    let Some(archived_at) = found else {
        return Err(Error::NotFound(format!("Project not found: {}", project_id)));
    };
    // A closed project does not take new members; reopen it first.
    if archived_at.is_some() {
        return Err(Error::InvalidInput("this project is archived".into()));
    }

    // A chat is filed by its own `project_id`, not by a member row: the
    // project view lists chats from the session list and ignores `/chat/`
    // member rows, so a bare row here was an add that did nothing (VIR-359).
    // Bind the chat; the insert below then keeps the membership row in step.
    if let Some(chat_id) = url.strip_prefix("/chat/") {
        set_chat_project(pool, chat_id, Some(project_id)).await?;
    }

    // role='library' = grounds chat, which is what membership means. (The old
    // nav-only 'pin' role for a nested notebook is gone with nesting itself;
    // 'pin' survives only as a role the user can set explicitly.)
    let item = sqlx::query_as::<_, ProjectItem>(
        r#"
        INSERT INTO app_project_items (project_id, url, sort_order, role)
        VALUES (
            $1, $2,
            (SELECT COALESCE(MAX(sort_order), -1) + 1 FROM app_project_items WHERE project_id = $1),
            'library'
        )
        -- Re-adding an existing member upgrades a legacy nav-only 'pin' to
        -- 'library', but must not demote a 'manuscript' back to source material.
        ON CONFLICT (project_id, url) DO UPDATE SET
            role = CASE
                WHEN app_project_items.role = 'pin' THEN 'library'
                ELSE app_project_items.role
            END
        RETURNING url, sort_order, role, added_at
        "#,
    )
    .bind(project_id)
    .bind(url)
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to add project item: {}", e)))?;

    touch_project(pool, project_id).await.ok();
    Ok(item)
}

/// Remove a member URL from a Project.
pub async fn remove_project_item(pool: &PgPool, project_id: &str, url: &str) -> Result<()> {
    let result = sqlx::query(r#"DELETE FROM app_project_items WHERE project_id = $1 AND url = $2"#)
        .bind(project_id)
        .bind(url)
        .execute(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to remove project item: {}", e)))?;

    if result.rows_affected() == 0 {
        return Err(Error::NotFound(format!(
            "Member not found in project: {} / {}",
            project_id, url
        )));
    }

    touch_project(pool, project_id).await.ok();
    Ok(())
}

/// Remove all membership entries for a given URL across every Project.
/// Called when the underlying entity (chat/page/...) is deleted.
pub async fn remove_items_by_url(pool: &PgPool, url: &str) -> Result<i64> {
    let result = sqlx::query(r#"DELETE FROM app_project_items WHERE url = $1"#)
        .bind(url)
        .execute(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to remove items by URL: {}", e)))?;

    Ok(result.rows_affected() as i64)
}

/// Reorder a Project's members. Unknown URLs are ignored.
pub async fn reorder_project_items(
    pool: &PgPool,
    project_id: &str,
    req: ReorderProjectItemsRequest,
) -> Result<()> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| Error::Database(format!("Failed to start transaction: {}", e)))?;

    for (idx, url) in req.urls.iter().enumerate() {
        sqlx::query(
            r#"UPDATE app_project_items SET sort_order = $1 WHERE project_id = $2 AND url = $3"#,
        )
        .bind(idx as i64)
        .bind(project_id)
        .bind(url)
        .execute(&mut *tx)
        .await
        .map_err(|e| Error::Database(format!("Failed to reorder project items: {}", e)))?;
    }

    sqlx::query(r#"UPDATE app_projects SET updated_at = now() WHERE id = $1"#)
        .bind(project_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| Error::Database(format!("Failed to touch project: {}", e)))?;

    tx.commit()
        .await
        .map_err(|e| Error::Database(format!("Failed to commit reorder: {}", e)))?;

    Ok(())
}

// ============================================================================
// Chat ↔ Project binding (one active Project per chat)
// ============================================================================

/// Set or clear a chat's Project. Passing `Some(project_id)` also folds the chat
/// into that Project's membership (idempotent); passing `None` detaches it. The
/// row update and the membership fold run in one transaction so the chat's
/// `project_id` and its `/chat/<id>` membership row can never diverge.
pub async fn set_chat_project(pool: &PgPool, chat_id: &str, project_id: Option<&str>) -> Result<()> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| Error::Database(format!("Failed to start transaction: {}", e)))?;

    sqlx::query(r#"UPDATE app_chats SET project_id = $2 WHERE id = $1"#)
        .bind(chat_id)
        .bind(project_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| Error::Database(format!("Failed to set chat project: {}", e)))?;

    if let Some(project_id) = project_id {
        sqlx::query(
            r#"
            INSERT INTO app_project_items (project_id, url, sort_order)
            VALUES (
                $1, $2,
                (SELECT COALESCE(MAX(sort_order), -1) + 1 FROM app_project_items WHERE project_id = $1)
            )
            ON CONFLICT (project_id, url) DO NOTHING
            "#,
        )
        .bind(project_id)
        .bind(format!("/chat/{}", chat_id))
        .execute(&mut *tx)
        .await
        .map_err(|e| Error::Database(format!("Failed to fold chat into project: {}", e)))?;

        sqlx::query(r#"UPDATE app_projects SET updated_at = now() WHERE id = $1"#)
            .bind(project_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| Error::Database(format!("Failed to touch project: {}", e)))?;
    }

    tx.commit()
        .await
        .map_err(|e| Error::Database(format!("Failed to commit chat project binding: {}", e)))?;

    Ok(())
}

/// Set a member's role. `library` grounds chat, `manuscript` is yours to write
/// (kept out of retrieval), `pin` is nav-only.
pub async fn set_project_item_role(
    pool: &PgPool,
    project_id: &str,
    req: SetProjectItemRoleRequest,
) -> Result<ProjectItem> {
    if !matches!(req.role.as_str(), "library" | "manuscript" | "pin") {
        return Err(Error::InvalidInput(format!(
            "Unknown project item role: {}",
            req.role
        )));
    }

    let item = sqlx::query_as::<_, ProjectItem>(
        r#"
        UPDATE app_project_items SET role = $3
        WHERE project_id = $1 AND url = $2
        RETURNING url, sort_order, role, added_at
        "#,
    )
    .bind(project_id)
    .bind(&req.url)
    .bind(&req.role)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to set project item role: {}", e)))?
    .ok_or_else(|| Error::NotFound(format!("Project member not found: {}", req.url)))?;

    touch_project(pool, project_id).await.ok();
    Ok(item)
}

/// Entity ref-URL prefixes that can appear as a project member or inside a
/// page's markdown. Kept in sync with the frontend's ref routes. (`/thing/`
/// is gone: migration 0071 dropped wiki_things and swept every stored
/// `/thing/` url — the stragglers here were what kept its ghost walking.)
const ENTITY_PREFIXES: [&str; 3] = ["/person/", "/place/", "/org/"];

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

/// The entities referenced across a project's members, with co-occurrence
/// edges. Nodes come only from things the user explicitly wrote or filed —
/// entity members, and `[@ref]` links inside member pages. Nothing is inferred:
/// there is no NER over free text, so an entity that is merely *mentioned* in a
/// PDF does not appear here.
pub async fn project_graph(pool: &PgPool, project_id: &str) -> Result<ProjectGraph> {
    use std::collections::{HashMap, HashSet};

    // Nav-only pins are excluded: they are shortcuts, not content.
    let members: Vec<String> = sqlx::query_scalar(
        r#"
        SELECT url FROM app_project_items
        WHERE project_id = $1 AND role <> 'pin'
        ORDER BY sort_order ASC, added_at ASC
        "#,
    )
    .bind(project_id)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to load project members: {}", e)))?;

    if members.is_empty() {
        return Ok(ProjectGraph { nodes: Vec::new(), edges: Vec::new() });
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
            sqlx::query_as(r#"SELECT id, content FROM app_pages WHERE id = ANY($1) AND deleted_at IS NULL"#)
                .bind(&page_ids)
                .fetch_all(pool)
                .await
                .map_err(|e| Error::Database(format!("Failed to load member pages: {}", e)))?;
        page_content.extend(rows);
    }

    for url in &members {
        let mut refs = HashSet::new();
        if ENTITY_PREFIXES.iter().any(|p| url.starts_with(p)) {
            // An entity filed directly in the project is a node in its own right.
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
            SELECT id, name AS name FROM wiki_people WHERE id = ANY($1)
            UNION ALL SELECT id, name FROM wiki_places WHERE id = ANY($1)
            UNION ALL SELECT id, name AS name FROM wiki_orgs WHERE id = ANY($1)
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

    let mut nodes: Vec<ProjectGraphNode> = node_items
        .into_iter()
        .filter_map(|(url, item_urls)| {
            let mut parts = url.trim_start_matches('/').splitn(2, '/');
            let entity_type = parts.next()?.to_string();
            let id = parts.next()?.to_string();
            // An unresolvable id is a dangling ref (entity deleted, stale link).
            // Drop it rather than render a node with no name.
            let name = names.get(&id)?.clone();
            Some(ProjectGraphNode { url, entity_type, name, item_urls })
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

    let mut edges: Vec<ProjectGraphEdge> = pair_weight
        .into_iter()
        .map(|((source, target), weight)| ProjectGraphEdge { source, target, weight })
        .collect();
    edges.sort_by(|a, b| b.weight.cmp(&a.weight).then_with(|| a.source.cmp(&b.source)));

    Ok(ProjectGraph { nodes, edges })
}
