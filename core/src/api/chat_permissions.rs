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
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

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
    pub granted_at: String,
}

/// Request to add an edit permission
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddPermissionRequest {
    pub entity_id: String,
    pub entity_type: String,
    pub entity_title: Option<String>,
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

// ============================================================================
// CRUD Operations
// ============================================================================

/// List all edit permissions for a chat
pub async fn list_permissions(pool: &SqlitePool, chat_id: &str) -> Result<PermissionListResponse> {
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
pub async fn has_permission(pool: &SqlitePool, chat_id: &str, entity_id: &str) -> Result<bool> {
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
    pool: &SqlitePool,
    chat_id: &str,
    request: AddPermissionRequest,
) -> Result<PermissionResponse> {
    let id = generate_id(PERMISSION_PREFIX, &[chat_id, &request.entity_id]);

    // Ensure chat exists first (for new chats that haven't sent a message yet)
    // Use INSERT OR IGNORE so we don't conflict if chat already exists
    sqlx::query(
        r#"
        INSERT OR IGNORE INTO app_chats (id, title, message_count)
        VALUES ($1, 'New conversation', 0)
        "#,
    )
    .bind(chat_id)
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to ensure chat exists: {}", e)))?;

    // Use INSERT OR IGNORE to handle duplicates gracefully
    sqlx::query(
        r#"
        INSERT OR IGNORE INTO app_chat_edit_permissions (id, chat_id, entity_id, entity_type, entity_title)
        VALUES ($1, $2, $3, $4, $5)
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
pub async fn remove_permission(pool: &SqlitePool, chat_id: &str, entity_id: &str) -> Result<()> {
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
pub async fn clear_permissions(pool: &SqlitePool, chat_id: &str) -> Result<u64> {
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
