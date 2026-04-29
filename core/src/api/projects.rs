//! Projects API
//!
//! Projects are curated, reusable sets of references (pages, chats, people,
//! places, files) that a user can apply as a context lens in chat. A project
//! is NOT a workspace — it has no default chat thread, no scoped filtering,
//! no sub-pages. It is a named, icon'd table of URLs.
//!
//! Users @-mention a project in chat or pick one from the composer dropdown.
//! The chat send path fetches the project's items and inlines metadata (ids,
//! labels, icons, urls) into the system prompt. The agent fetches full content
//! on demand via the `get_project_item` tool.

use crate::error::{Error, Result};
use crate::ids::{generate_id, PROJECT_ITEM_PREFIX, PROJECT_PREFIX};
use crate::types::Timestamp;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

// ============================================================================
// Types
// ============================================================================

/// A project record (the container itself)
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub icon: Option<String>,
    pub description: Option<String>,
    pub sort_order: i64,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// A project list entry (includes item count for list-view badges)
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ProjectSummary {
    pub id: String,
    pub name: String,
    pub icon: Option<String>,
    pub description: Option<String>,
    pub sort_order: i64,
    pub item_count: i64,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// A single reference inside a project — an annotated bookmark.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ProjectItem {
    pub id: String,
    pub project_id: String,
    pub url: String,
    pub name: Option<String>,
    pub description: Option<String>,
    pub sort_order: i64,
    pub added_at: Timestamp,
}

/// Full project response: project + ordered items
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectDetail {
    #[serde(flatten)]
    pub project: Project,
    pub items: Vec<ProjectItem>,
}

/// List response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectListResponse {
    pub projects: Vec<ProjectSummary>,
}

/// Request to create a project
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateProjectRequest {
    pub name: String,
    pub icon: Option<String>,
    pub description: Option<String>,
}

/// Request to update a project. All fields optional — only provided fields change.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateProjectRequest {
    pub name: Option<String>,
    pub icon: Option<Option<String>>,
    pub description: Option<Option<String>>,
    pub sort_order: Option<i64>,
}

/// Request to add an item to a project
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddProjectItemRequest {
    pub url: String,
    pub name: Option<String>,
    pub description: Option<String>,
}

/// Request to reorder items in a project — provide the desired URL order
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReorderProjectItemsRequest {
    pub urls: Vec<String>,
}

// ============================================================================
// Project CRUD
// ============================================================================

/// List all projects, ordered by sort_order then most-recently-updated.
pub async fn list_projects(pool: &SqlitePool) -> Result<ProjectListResponse> {
    let projects = sqlx::query_as::<_, ProjectSummary>(
        r#"
        SELECT
            p.id,
            p.name,
            p.icon,
            p.description,
            p.sort_order,
            COALESCE((SELECT COUNT(*) FROM app_project_items WHERE project_id = p.id), 0) AS item_count,
            p.created_at,
            p.updated_at
        FROM app_projects p
        ORDER BY p.sort_order ASC, p.updated_at DESC
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to list projects: {}", e)))?;

    Ok(ProjectListResponse { projects })
}

/// Get a single project with its items (ordered).
pub async fn get_project(pool: &SqlitePool, id: &str) -> Result<ProjectDetail> {
    let project = sqlx::query_as::<_, Project>(
        r#"
        SELECT id, name, icon, description, sort_order, created_at, updated_at
        FROM app_projects
        WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get project: {}", e)))?
    .ok_or_else(|| Error::NotFound(format!("Project not found: {}", id)))?;

    let items = sqlx::query_as::<_, ProjectItem>(
        r#"
        SELECT id, project_id, url, name, description, sort_order, added_at
        FROM app_project_items
        WHERE project_id = $1
        ORDER BY sort_order ASC, added_at ASC
        "#,
    )
    .bind(id)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get project items: {}", e)))?;

    Ok(ProjectDetail { project, items })
}

/// Create a new project.
pub async fn create_project(pool: &SqlitePool, req: CreateProjectRequest) -> Result<Project> {
    let name = req.name.trim();
    if name.is_empty() {
        return Err(Error::InvalidInput("Project name cannot be empty".into()));
    }

    let timestamp = chrono::Utc::now().to_rfc3339();
    let id = generate_id(PROJECT_PREFIX, &[name, &timestamp]);

    // New projects go to the end: max(sort_order) + 1
    let next_sort: i64 = sqlx::query_scalar(
        r#"SELECT COALESCE(MAX(sort_order), -1) + 1 FROM app_projects"#,
    )
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to compute sort_order: {}", e)))?;

    let project = sqlx::query_as::<_, Project>(
        r#"
        INSERT INTO app_projects (id, name, icon, description, sort_order)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING id, name, icon, description, sort_order, created_at, updated_at
        "#,
    )
    .bind(&id)
    .bind(name)
    .bind(&req.icon)
    .bind(&req.description)
    .bind(next_sort)
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to create project: {}", e)))?;

    Ok(project)
}

/// Update a project (only provided fields change).
pub async fn update_project(
    pool: &SqlitePool,
    id: &str,
    req: UpdateProjectRequest,
) -> Result<Project> {
    let existing = sqlx::query_as::<_, Project>(
        r#"
        SELECT id, name, icon, description, sort_order, created_at, updated_at
        FROM app_projects
        WHERE id = $1
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
    let description = match req.description {
        Some(val) => val,
        None => existing.description,
    };
    let sort_order = req.sort_order.unwrap_or(existing.sort_order);

    let project = sqlx::query_as::<_, Project>(
        r#"
        UPDATE app_projects
        SET name = $2, icon = $3, description = $4, sort_order = $5
        WHERE id = $1
        RETURNING id, name, icon, description, sort_order, created_at, updated_at
        "#,
    )
    .bind(id)
    .bind(&name)
    .bind(&icon)
    .bind(&description)
    .bind(sort_order)
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to update project: {}", e)))?;

    Ok(project)
}

/// Delete a project. Cascades to items via FK.
pub async fn delete_project(pool: &SqlitePool, id: &str) -> Result<()> {
    let result = sqlx::query(r#"DELETE FROM app_projects WHERE id = $1"#)
        .bind(id)
        .execute(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to delete project: {}", e)))?;

    if result.rows_affected() == 0 {
        return Err(Error::NotFound(format!("Project not found: {}", id)));
    }

    Ok(())
}

// ============================================================================
// Project Items
// ============================================================================

/// Add an item to a project. Dedupes on (project_id, url): if the URL is
/// already present, returns the existing row instead of erroring.
pub async fn add_project_item(
    pool: &SqlitePool,
    project_id: &str,
    req: AddProjectItemRequest,
) -> Result<ProjectItem> {
    let url = req.url.trim();
    if url.is_empty() {
        return Err(Error::InvalidInput("Project item url cannot be empty".into()));
    }

    // Verify project exists
    let project_exists: Option<String> =
        sqlx::query_scalar(r#"SELECT id FROM app_projects WHERE id = $1"#)
            .bind(project_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to verify project: {}", e)))?;

    if project_exists.is_none() {
        return Err(Error::NotFound(format!("Project not found: {}", project_id)));
    }

    // Dedupe: return existing row if already present
    if let Some(existing) = sqlx::query_as::<_, ProjectItem>(
        r#"
        SELECT id, project_id, url, name, description, sort_order, added_at
        FROM app_project_items
        WHERE project_id = $1 AND url = $2
        "#,
    )
    .bind(project_id)
    .bind(url)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to check existing item: {}", e)))?
    {
        return Ok(existing);
    }

    let timestamp = chrono::Utc::now().to_rfc3339();
    let id = generate_id(PROJECT_ITEM_PREFIX, &[project_id, url, &timestamp]);

    // New items go to the end of the project
    let next_sort: i64 = sqlx::query_scalar(
        r#"SELECT COALESCE(MAX(sort_order), -1) + 1 FROM app_project_items WHERE project_id = $1"#,
    )
    .bind(project_id)
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to compute sort_order: {}", e)))?;

    let item = sqlx::query_as::<_, ProjectItem>(
        r#"
        INSERT INTO app_project_items (id, project_id, url, name, description, sort_order)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING id, project_id, url, name, description, sort_order, added_at
        "#,
    )
    .bind(&id)
    .bind(project_id)
    .bind(url)
    .bind(&req.name)
    .bind(&req.description)
    .bind(next_sort)
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to add project item: {}", e)))?;

    // Touch project so updated_at reflects activity
    sqlx::query(r#"UPDATE app_projects SET updated_at = datetime('now') WHERE id = $1"#)
        .bind(project_id)
        .execute(pool)
        .await
        .ok();

    Ok(item)
}

/// Remove an item from a project (by URL).
pub async fn remove_project_item(pool: &SqlitePool, project_id: &str, url: &str) -> Result<()> {
    let result = sqlx::query(
        r#"DELETE FROM app_project_items WHERE project_id = $1 AND url = $2"#,
    )
    .bind(project_id)
    .bind(url)
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to remove project item: {}", e)))?;

    if result.rows_affected() == 0 {
        return Err(Error::NotFound(format!(
            "Item not found in project: {} / {}",
            project_id, url
        )));
    }

    sqlx::query(r#"UPDATE app_projects SET updated_at = datetime('now') WHERE id = $1"#)
        .bind(project_id)
        .execute(pool)
        .await
        .ok();

    Ok(())
}

/// Reorder items in a project. Unknown URLs are ignored; missing URLs keep
/// their relative order at the end.
pub async fn reorder_project_items(
    pool: &SqlitePool,
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

    sqlx::query(r#"UPDATE app_projects SET updated_at = datetime('now') WHERE id = $1"#)
        .bind(project_id)
        .execute(&mut *tx)
        .await
        .ok();

    tx.commit()
        .await
        .map_err(|e| Error::Database(format!("Failed to commit reorder: {}", e)))?;

    Ok(())
}
