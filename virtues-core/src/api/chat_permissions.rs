//! Chat Edit Permissions API
//!
//! Manages which entities AI is allowed to edit within a specific chat session.
//! Permissions are chat-scoped and cleared when the chat is deleted.
//!
//! The permission flow:
//! 1. AI tries to edit an entity
//! 2. Backend checks if entity is in chat's permission list
//! 3. If not, returns `permission_needed: true` to frontend
//! 4. User sees permission prompt and clicks Allow/Deny
//! 5. If allowed, frontend POSTs to add permission, then retries edit

use crate::error::{Error, Result};
use crate::ids::generate_id;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

// ============================================================================
// Constants
// ============================================================================

pub const PERMISSION_PREFIX: &str = "perm";

// ============================================================================
// Types
// ============================================================================

/// A chat edit permission record
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ChatEditPermission {
    pub id: String,
    pub chat_id: String,
    pub entity_id: String,
    pub entity_type: String,
    pub entity_title: Option<String>,
    /// `TIMESTAMPTZ` in the table. This was a `String` from the SQLite days
    /// through the Postgres move, and sqlx refuses to decode a timestamptz into
    /// one — so every kept grant 500'd on the re-fetch after its INSERT, and
    /// every chat holding a grant 500'd on load. An empty list decodes nothing
    /// and succeeded, which is why the only test that ran (a ghost, which
    /// writes no row) stayed green.
    pub granted_at: DateTime<Utc>,
}

/// Request to add an edit permission
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddPermissionRequest {
    pub entity_id: String,
    pub entity_type: String,
    pub entity_title: Option<String>,
    /// The chat is a ghost: keep the grant in memory, write nothing.
    #[serde(default)]
    pub temporary: bool,
}

/// Grants made inside a ghost chat.
///
/// A temporary chat has no `app_chats` row — that is the promise the UI makes
/// about it — and `app_chat_edit_permissions` has a foreign key to one. So
/// granting used to CREATE the row "to ensure chat exists", which left an empty
/// "New conversation" in the list for a conversation that was never meant to
/// exist. A ghost's grant only has to outlive the ghost, and this is process
/// memory for exactly as long.
///
/// Dropping `chat_id` from the tool context instead would have been worse:
/// `check_tool_permission` reads a missing chat as "headless, not gated", so a
/// ghost would have skipped the gate rather than kept it.
#[derive(Debug, Clone, Default)]
pub struct GhostPermissions {
    granted: std::sync::Arc<
        std::sync::RwLock<std::collections::HashMap<String, std::collections::HashSet<String>>>,
    >,
}

impl GhostPermissions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn grant(&self, chat_id: &str, entity_id: &str) {
        let mut guard = self.granted.write().unwrap_or_else(|e| e.into_inner());
        guard
            .entry(chat_id.to_string())
            .or_default()
            .insert(entity_id.to_string());
    }

    pub fn has(&self, chat_id: &str, entity_id: &str) -> bool {
        let guard = self.granted.read().unwrap_or_else(|e| e.into_inner());
        guard.get(chat_id).is_some_and(|set| set.contains(entity_id))
    }

    /// Forget a ghost's grants. The tab is gone; so is what it allowed.
    pub fn forget(&self, chat_id: &str) {
        let mut guard = self.granted.write().unwrap_or_else(|e| e.into_inner());
        guard.remove(chat_id);
    }
}

/// Response for permission list
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionListResponse {
    pub permissions: Vec<ChatEditPermission>,
}

/// Response for single permission operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionResponse {
    pub permission: ChatEditPermission,
}

#[cfg(test)]
mod ghost_tests {
    use super::*;

    #[test]
    fn a_ghost_keeps_its_grants_and_leaves_nothing() {
        let ghosts = GhostPermissions::new();
        assert!(!ghosts.has("chat_ghost", "applet_digest"));

        ghosts.grant("chat_ghost", "applet_digest");
        assert!(ghosts.has("chat_ghost", "applet_digest"));
        // Scoped to the chat that granted it, like the table it replaces.
        assert!(!ghosts.has("chat_other", "applet_digest"));
        assert!(!ghosts.has("chat_ghost", "applet_else"));

        ghosts.forget("chat_ghost");
        assert!(!ghosts.has("chat_ghost", "applet_digest"));
    }

    /// A ghost grant writes nothing — not the permission, and above all not the
    /// `app_chats` row the foreign key used to demand.
    #[sqlx::test]
    async fn granting_in_a_ghost_creates_no_rows(pool: sqlx::PgPool) {
        let ghosts = GhostPermissions::new();
        let response = add_permission(
            &pool,
            &ghosts,
            "chat_ghost",
            AddPermissionRequest {
                entity_id: "applet_digest".to_string(),
                entity_type: "action".to_string(),
                entity_title: Some("Daily digest".to_string()),
                temporary: true,
            },
        )
        .await
        .expect("a ghost grant succeeds");
        assert_eq!(response.permission.entity_id, "applet_digest");
        assert!(ghosts.has("chat_ghost", "applet_digest"));

        let chats: i64 = sqlx::query_scalar("SELECT count(*) FROM app_chats")
            .fetch_one(&pool)
            .await
            .unwrap();
        let perms: i64 = sqlx::query_scalar("SELECT count(*) FROM app_chat_edit_permissions")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!((chats, perms), (0, 0), "a ghost leaves nothing behind");
    }

    /// A kept grant writes a row and reads it back — both the re-fetch inside
    /// `add_permission` and `list_permissions` decode `granted_at`. This is the
    /// test the ghost one could not be: it needs a row in the table.
    #[sqlx::test]
    async fn a_kept_grant_reads_back(pool: sqlx::PgPool) {
        let ghosts = GhostPermissions::new();
        let granted = add_permission(
            &pool,
            &ghosts,
            "chat_kept",
            AddPermissionRequest {
                entity_id: "page_notes".to_string(),
                entity_type: "page".to_string(),
                entity_title: Some("Notes".to_string()),
                temporary: false,
            },
        )
        .await
        .expect("a kept grant inserts and re-reads its row");
        assert_eq!(granted.permission.entity_id, "page_notes");

        let listed = list_permissions(&pool, "chat_kept")
            .await
            .expect("a chat with a grant lists it");
        assert_eq!(listed.permissions.len(), 1);
        assert_eq!(listed.permissions[0].granted_at, granted.permission.granted_at);
        assert!(has_permission(&pool, "chat_kept", "page_notes").await.unwrap());
    }
}

// ============================================================================
// CRUD Operations
// ============================================================================

/// List all edit permissions for a chat
pub async fn list_permissions(pool: &PgPool, chat_id: &str) -> Result<PermissionListResponse> {
    let permissions = sqlx::query_as::<_, ChatEditPermission>(
        r#"
        SELECT id, chat_id, entity_id, entity_type, entity_title, granted_at
        FROM app_chat_edit_permissions
        WHERE chat_id = $1
        ORDER BY granted_at ASC
        "#,
    )
    .bind(chat_id)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to list permissions: {}", e)))?;

    Ok(PermissionListResponse { permissions })
}

/// Check if a specific entity has edit permission in a chat
pub async fn has_permission(pool: &PgPool, chat_id: &str, entity_id: &str) -> Result<bool> {
    let result = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(*) FROM app_chat_edit_permissions
        WHERE chat_id = $1 AND entity_id = $2
        "#,
    )
    .bind(chat_id)
    .bind(entity_id)
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to check permission: {}", e)))?;

    Ok(result > 0)
}

/// Add an edit permission for an entity in a chat
/// Returns the permission if created, or existing if already present
pub async fn add_permission(
    pool: &PgPool,
    ghosts: &GhostPermissions,
    chat_id: &str,
    request: AddPermissionRequest,
) -> Result<PermissionResponse> {
    let id = generate_id(PERMISSION_PREFIX, &[chat_id, &request.entity_id]);

    // A ghost's grant lives in memory and nothing is written. The row creation
    // below exists so the permission's foreign key resolves, and for a chat
    // that is never persisted it was creating the very thing the ghost promises
    // not to leave behind.
    if request.temporary {
        ghosts.grant(chat_id, &request.entity_id);
        return Ok(PermissionResponse {
            permission: ChatEditPermission {
                id,
                chat_id: chat_id.to_string(),
                entity_id: request.entity_id,
                entity_type: request.entity_type,
                entity_title: request.entity_title,
                granted_at: Utc::now(),
            },
        });
    }

    // Ensure chat exists first (for new chats that haven't sent a message yet)
    // Use ON CONFLICT DO NOTHING so we don't conflict if chat already exists
    sqlx::query(
        r#"
        INSERT INTO app_chats (id, title, message_count)
        VALUES ($1, 'New conversation', 0)
        ON CONFLICT (id) DO NOTHING
        "#,
    )
    .bind(chat_id)
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to ensure chat exists: {}", e)))?;

    // ON CONFLICT DO NOTHING to handle duplicate (chat_id, entity_id) pairs gracefully.
    sqlx::query(
        r#"
        INSERT INTO app_chat_edit_permissions (id, chat_id, entity_id, entity_type, entity_title)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (chat_id, entity_id) DO NOTHING
        "#,
    )
    .bind(&id)
    .bind(chat_id)
    .bind(&request.entity_id)
    .bind(&request.entity_type)
    .bind(&request.entity_title)
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to add permission: {}", e)))?;

    // Fetch the permission (either just created or existing)
    let permission = sqlx::query_as::<_, ChatEditPermission>(
        r#"
        SELECT id, chat_id, entity_id, entity_type, entity_title, granted_at
        FROM app_chat_edit_permissions
        WHERE chat_id = $1 AND entity_id = $2
        "#,
    )
    .bind(chat_id)
    .bind(&request.entity_id)
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to fetch permission: {}", e)))?;

    Ok(PermissionResponse { permission })
}

/// Remove an edit permission by entity ID
pub async fn remove_permission(pool: &PgPool, chat_id: &str, entity_id: &str) -> Result<()> {
    let result = sqlx::query(
        r#"
        DELETE FROM app_chat_edit_permissions
        WHERE chat_id = $1 AND entity_id = $2
        "#,
    )
    .bind(chat_id)
    .bind(entity_id)
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to remove permission: {}", e)))?;

    if result.rows_affected() == 0 {
        return Err(Error::NotFound(format!(
            "Permission not found for entity {} in chat {}",
            entity_id, chat_id
        )));
    }

    Ok(())
}

/// Remove all edit permissions for a chat
/// (Usually not needed since CASCADE handles this, but useful for explicit clearing)
pub async fn clear_permissions(pool: &PgPool, chat_id: &str) -> Result<u64> {
    let result = sqlx::query(
        r#"
        DELETE FROM app_chat_edit_permissions
        WHERE chat_id = $1
        "#,
    )
    .bind(chat_id)
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to clear permissions: {}", e)))?;

    Ok(result.rows_affected())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_permission_prefix() {
        let id = generate_id(PERMISSION_PREFIX, &["chat_123", "page_456"]);
        assert!(id.starts_with("perm_"));
    }
}
