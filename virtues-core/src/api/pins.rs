//! Sidebar pins API.
//!
//! A pin is a single URL pointer the user has dragged to their sidebar's
//! "Pinned" section — a thing, page, day, person, project, or external URL.
//! Distinct from project membership (`app_project_items`): a pin is global to
//! the user's sidebar, not scoped to a project.
//!
//! Same URL convention as the rest of the app: `/person/per_xxx`,
//! `/page/page_xxx`, `/person/p_xxx`, or `https://...` for externals.

use crate::error::{Error, Result};
use crate::ids::{generate_id, PIN_PREFIX};
use crate::types::Timestamp;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Pin {
    pub id: String,
    pub url: String,
    pub label: Option<String>,
    pub icon: Option<String>,
    pub sort_order: i32,
    pub pinned_at: Timestamp,
    /// A `--cat-*` token key ('orange', 'emerald'…), never a hex — see
    /// migration 0070. The sidebar renders it as the row's ribbon.
    pub color: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatePinRequest {
    pub url: String,
    pub label: Option<String>,
    pub icon: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdatePinRequest {
    pub label: Option<Option<String>>,
    pub icon: Option<Option<String>>,
    pub sort_order: Option<i32>,
    pub color: Option<Option<String>>,
}

pub async fn list_pins(db: &PgPool) -> Result<Vec<Pin>> {
    let pins = sqlx::query_as::<_, Pin>(
        r#"SELECT id, url, label, icon, sort_order, pinned_at, color
           FROM app_pins
           ORDER BY sort_order ASC, pinned_at DESC"#,
    )
    .fetch_all(db)
    .await?;
    Ok(pins)
}

/// Create a pin. If the URL is already pinned, return the existing row
/// (idempotent — pinning twice is a no-op).
pub async fn create_pin(db: &PgPool, req: CreatePinRequest) -> Result<Pin> {
    if let Some(existing) = sqlx::query_as::<_, Pin>(
        r#"SELECT id, url, label, icon, sort_order, pinned_at, color
           FROM app_pins WHERE url = $1"#,
    )
    .bind(&req.url)
    .fetch_optional(db)
    .await?
    {
        return Ok(existing);
    }

    let id = generate_id(PIN_PREFIX, &[&req.url]);
    let next_sort: i32 = sqlx::query_scalar("SELECT COALESCE(MAX(sort_order), -1) + 1 FROM app_pins")
        .fetch_one(db)
        .await
        .unwrap_or(0);

    sqlx::query(
        r#"INSERT INTO app_pins (id, url, label, icon, sort_order, color)
           VALUES ($1, $2, $3, $4, $5, $6)"#,
    )
    .bind(&id)
    .bind(&req.url)
    .bind(&req.label)
    .bind(&req.icon)
    .bind(next_sort)
    .bind(&req.color)
    .execute(db)
    .await?;

    sqlx::query_as::<_, Pin>(
        r#"SELECT id, url, label, icon, sort_order, pinned_at, color FROM app_pins WHERE id = $1"#,
    )
    .bind(&id)
    .fetch_one(db)
    .await
    .map_err(Error::from)
}

pub async fn update_pin(db: &PgPool, id: &str, req: UpdatePinRequest) -> Result<Pin> {
    if let Some(label) = req.label {
        sqlx::query("UPDATE app_pins SET label = $1 WHERE id = $2")
            .bind(label)
            .bind(id)
            .execute(db)
            .await?;
    }
    if let Some(icon) = req.icon {
        sqlx::query("UPDATE app_pins SET icon = $1 WHERE id = $2")
            .bind(icon)
            .bind(id)
            .execute(db)
            .await?;
    }
    if let Some(color) = req.color {
        sqlx::query("UPDATE app_pins SET color = $1 WHERE id = $2")
            .bind(color)
            .bind(id)
            .execute(db)
            .await?;
    }
    if let Some(sort) = req.sort_order {
        sqlx::query("UPDATE app_pins SET sort_order = $1 WHERE id = $2")
            .bind(sort)
            .bind(id)
            .execute(db)
            .await?;
    }

    sqlx::query_as::<_, Pin>(
        r#"SELECT id, url, label, icon, sort_order, pinned_at, color FROM app_pins WHERE id = $1"#,
    )
    .bind(id)
    .fetch_one(db)
    .await
    .map_err(Error::from)
}

pub async fn delete_pin(db: &PgPool, id: &str) -> Result<()> {
    sqlx::query("DELETE FROM app_pins WHERE id = $1")
        .bind(id)
        .execute(db)
        .await?;
    Ok(())
}

/// Reorder all pins to match the supplied URL list. Items not present are
/// left untouched at the end of the order.
pub async fn reorder_pins(db: &PgPool, urls: &[String]) -> Result<()> {
    let mut tx = db.begin().await?;
    for (i, url) in urls.iter().enumerate() {
        sqlx::query("UPDATE app_pins SET sort_order = $1 WHERE url = $2")
            .bind(i as i64)
            .bind(url)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}
