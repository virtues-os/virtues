//! Sidebar pins API.
//!
//! A pin is a single URL pointer the user has dragged to their sidebar's
//! "Pinned" section — a thing, page, day, person, project, or external URL.
//! Distinct from project membership (`app_project_items`): a pin is global to
//! the user's sidebar, not scoped to a project.
//!
//! Same URL convention as the rest of the app: `/person/per_xxx`,
//! `/page/page_xxx`, `/person/p_xxx`, or `https://...` for externals.
//!
//! A pin holds a URL, not a name. What the sidebar shows is resolved from the
//! thing each time pins are listed ([`PinView`]): a chat, page or project
//! shows its own title, icon and color, and a pin to one in the trash is not
//! listed at all. The row's `label`, `icon` and `color` are the pin's own
//! only where the thing has none to give: an external URL's label, and the
//! icon a pin carried before icons moved onto the thing.

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
    /// A `--cat-*` token key ('orange', 'emerald'…) or a custom hex, which
    /// the client keeps verbatim and fits to the theme (pin-colors.ts).
    pub color: Option<String>,
}

/// A pin as the sidebar draws it: the pointer, plus what it points at now.
#[derive(Debug, Clone, Serialize)]
pub struct PinView {
    pub id: String,
    pub url: String,
    /// `chat`, `page`, `project`, a wiki kind, `web` for an external URL, or
    /// `route` for an app screen with no record behind it.
    pub kind: String,
    /// The thing's name now. For an external URL, the pin's label or host.
    pub title: String,
    /// The thing's icon, else the pin's own.
    pub icon: Option<String>,
    /// The thing's color key, else the pin's own.
    pub color: Option<String>,
    /// The pin's stored label, which names only an external URL.
    pub label: Option<String>,
    pub sort_order: i32,
    pub pinned_at: Timestamp,
}

/// A chat, page or project behind a pin: `(title, icon, color, trashed)`.
type OwnedTarget = (String, Option<String>, Option<String>, bool);

/// The kinds whose name, icon and color live on an app table, and that can
/// be trashed. Anything else is named through `refs::resolve_refs`.
async fn owned_targets(db: &PgPool, kind: &str, ids: &[String]) -> Result<Vec<(String, OwnedTarget)>> {
    let sql = match kind {
        "chat" => {
            "SELECT id, title, icon, icon_color, deleted_at IS NOT NULL FROM app_chats WHERE id = ANY($1)"
        }
        "page" => {
            "SELECT id, COALESCE(NULLIF(title, ''), 'Untitled page'), icon, icon_color, deleted_at IS NOT NULL \
             FROM app_pages WHERE id = ANY($1)"
        }
        "project" => {
            "SELECT id, COALESCE(NULLIF(name, ''), 'Untitled project'), icon, accent_color, deleted_at IS NOT NULL \
             FROM app_projects WHERE id = ANY($1)"
        }
        _ => return Ok(Vec::new()),
    };
    let rows = sqlx::query_as::<_, (String, String, Option<String>, Option<String>, bool)>(sql)
        .bind(ids)
        .fetch_all(db)
        .await?;
    Ok(rows.into_iter().map(|(id, t, i, c, d)| (id, (t, i, c, d))).collect())
}

/// Resolve pins against what they point at. A pin whose chat, page or project
/// is trashed or gone is dropped: the sidebar never shows a thing in the
/// trash, and restoring it brings the pin back untouched.
pub async fn resolve_pins(db: &PgPool, pins: Vec<Pin>) -> Result<Vec<PinView>> {
    use std::collections::HashMap;

    let mut ids_by_kind: HashMap<&'static str, Vec<String>> = HashMap::new();
    for pin in &pins {
        if let Some((kind, id)) = crate::api::refs::split_ref(&pin.url) {
            for owned in ["chat", "page", "project"] {
                if kind == owned {
                    ids_by_kind.entry(owned).or_default().push(id.to_string());
                }
            }
        }
    }
    let mut owned: HashMap<(&'static str, String), OwnedTarget> = HashMap::new();
    for (kind, ids) in &ids_by_kind {
        for (id, target) in owned_targets(db, kind, ids).await? {
            owned.insert((kind, id), target);
        }
    }
    let other: Vec<String> = pins
        .iter()
        .filter(|p| {
            !crate::api::refs::split_ref(&p.url).is_some_and(|(k, _)| ids_by_kind.contains_key(k))
        })
        .map(|p| p.url.clone())
        .collect();
    let names = crate::api::refs::resolve_refs(db, &other).await;

    let mut out = Vec::with_capacity(pins.len());
    for pin in pins {
        let parsed = crate::api::refs::split_ref(&pin.url);
        let owned_kind = parsed.and_then(|(k, _)| ids_by_kind.keys().find(|o| **o == k).copied());
        let (kind, title, icon, color) = if let (Some(kind), Some((_, id))) = (owned_kind, parsed) {
            match owned.get(&(kind, id.to_string())) {
                Some((_, _, _, true)) | None => continue,
                Some((title, icon, color, false)) => (
                    kind.to_string(),
                    title.clone(),
                    icon.clone().or_else(|| pin.icon.clone()),
                    color.clone().or_else(|| pin.color.clone()),
                ),
            }
        } else {
            let resolved = names.get(&pin.url);
            let label = pin.label.as_deref().map(str::trim).filter(|l| !l.is_empty());
            let kind = resolved
                .map(|r| r.kind.clone())
                .or_else(|| parsed.map(|(k, _)| k.to_string()))
                .unwrap_or_else(|| "route".to_string());
            // An external URL is named by the person (the pin IS the thing);
            // a record by its own name; an app screen by the label it was
            // pinned under, having no record to ask.
            let title = match (kind.as_str(), resolved, label) {
                ("web", _, Some(l)) => l.to_string(),
                (_, Some(r), _) if r.title != pin.url => r.title.clone(),
                (_, _, Some(l)) => l.to_string(),
                _ => pin.url.clone(),
            };
            (kind, title, pin.icon.clone(), pin.color.clone())
        };
        out.push(PinView {
            id: pin.id,
            url: pin.url,
            kind,
            title,
            icon,
            color,
            label: pin.label,
            sort_order: pin.sort_order,
            pinned_at: pin.pinned_at,
        });
    }
    Ok(out)
}

/// One pin, resolved. `None` when its target is trashed.
async fn view_of(db: &PgPool, pin: Pin) -> Result<Option<PinView>> {
    Ok(resolve_pins(db, vec![pin]).await?.into_iter().next())
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

pub async fn list_pins(db: &PgPool) -> Result<Vec<PinView>> {
    let pins = sqlx::query_as::<_, Pin>(
        r#"SELECT id, url, label, icon, sort_order, pinned_at, color
           FROM app_pins
           ORDER BY sort_order ASC, pinned_at DESC"#,
    )
    .fetch_all(db)
    .await?;
    resolve_pins(db, pins).await
}

/// Create a pin. If the URL is already pinned, return the existing row
/// (idempotent — pinning twice is a no-op).
pub async fn create_pin(db: &PgPool, req: CreatePinRequest) -> Result<PinView> {
    let pin = insert_pin(db, req).await?;
    view_of(db, pin)
        .await?
        .ok_or_else(|| Error::InvalidInput("You can't pin something in Recently deleted. Restore it first.".into()))
}

async fn insert_pin(db: &PgPool, req: CreatePinRequest) -> Result<Pin> {
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
        .await?;

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

pub async fn update_pin(db: &PgPool, id: &str, req: UpdatePinRequest) -> Result<PinView> {
    let pin = write_pin(db, id, req).await?;
    view_of(db, pin).await?.ok_or_else(|| Error::NotFound("Pinned item is in the trash".into()))
}

async fn write_pin(db: &PgPool, id: &str, req: UpdatePinRequest) -> Result<Pin> {
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

#[cfg(test)]
mod resolve_tests {
    use super::*;

    fn req(url: &str, label: Option<&str>, icon: Option<&str>) -> CreatePinRequest {
        CreatePinRequest { url: url.into(), label: label.map(Into::into), icon: icon.map(Into::into), color: None }
    }

    /// The name is the thing's, the icon falls back to the pin's own, a
    /// trashed target is not listed and comes back on restore, and an
    /// external URL keeps the label it was pinned under.
    #[sqlx::test(migrations = "./migrations")]
    async fn pins_show_the_thing_as_it_is_now(pool: PgPool) {
        sqlx::query("INSERT INTO app_pages (id, title, content) VALUES ('page_p', 'Old name', '')")
            .execute(&pool)
            .await
            .unwrap();
        create_pin(&pool, req("/page/page_p", Some("Stale label"), Some("📝"))).await.unwrap();
        create_pin(&pool, req("https://example.com/a", Some("Reading list"), None)).await.unwrap();

        sqlx::query("UPDATE app_pages SET title = 'New name' WHERE id = 'page_p'").execute(&pool).await.unwrap();
        let pins = list_pins(&pool).await.unwrap();
        let page = pins.iter().find(|p| p.url == "/page/page_p").unwrap();
        assert_eq!((page.kind.as_str(), page.title.as_str(), page.icon.as_deref()), ("page", "New name", Some("📝")));
        let web = pins.iter().find(|p| p.kind == "web").unwrap();
        assert_eq!(web.title, "Reading list");

        sqlx::query("UPDATE app_pages SET deleted_at = now() WHERE id = 'page_p'").execute(&pool).await.unwrap();
        assert!(list_pins(&pool).await.unwrap().iter().all(|p| p.url != "/page/page_p"));

        sqlx::query("UPDATE app_pages SET deleted_at = NULL WHERE id = 'page_p'").execute(&pool).await.unwrap();
        assert!(list_pins(&pool).await.unwrap().iter().any(|p| p.url == "/page/page_p"));
    }
}
