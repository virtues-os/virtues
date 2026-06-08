//! Namespaces API
//!
//! This module provides read-only access to namespace configurations.
//! Namespaces define how URL paths map to storage backends.
//!
//! Entity namespaces (person, place, org, etc.) are backed by Postgres tables.
//! Storage namespaces (drive, lake) use the filesystem backend.
//! The 'virtues' namespace serves system pages (no backend).

use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

// ============================================================================
// Types
// ============================================================================

/// A namespace configuration record
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Namespace {
    pub name: String,
    pub backend: String,              // postgres, filesystem, s3, none
    pub backend_config: Option<String>, // JSON config
    pub is_entity: bool,              // TRUE = expects {name}_{id} pattern
    pub is_system: bool,              // TRUE = cannot be deleted by user
    pub icon: Option<String>,
    pub label: Option<String>,
    pub created_at: String,
}

/// Backend configuration for Postgres-backed namespaces
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostgresBackendConfig {
    pub table: String,
}

/// Backend configuration for filesystem-backed namespaces
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilesystemBackendConfig {
    pub mount: String,
}

/// List response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NamespaceListResponse {
    pub namespaces: Vec<Namespace>,
}

// ============================================================================
// Read Operations
// ============================================================================

/// List all namespaces
pub async fn list_namespaces(pool: &PgPool) -> Result<NamespaceListResponse> {
    let namespaces = sqlx::query_as::<_, Namespace>(
        r#"
        SELECT name, backend, backend_config, is_entity, is_system, icon, label, created_at
        FROM app_namespaces
        ORDER BY name ASC
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to list namespaces: {}", e)))?;

    Ok(NamespaceListResponse { namespaces })
}

/// Get a single namespace by name
pub async fn get_namespace(pool: &PgPool, name: &str) -> Result<Namespace> {
    let namespace = sqlx::query_as::<_, Namespace>(
        r#"
        SELECT name, backend, backend_config, is_entity, is_system, icon, label, created_at
        FROM app_namespaces
        WHERE name = $1
        "#,
    )
    .bind(name)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get namespace: {}", e)))?
    .ok_or_else(|| Error::NotFound(format!("Namespace not found: {}", name)))?;

    Ok(namespace)
}

/// Get entity namespaces only (for sidebar views)
pub async fn list_entity_namespaces(pool: &PgPool) -> Result<Vec<Namespace>> {
    let namespaces = sqlx::query_as::<_, Namespace>(
        r#"
        SELECT name, backend, backend_config, is_entity, is_system, icon, label, created_at
        FROM app_namespaces
        WHERE is_entity = TRUE
        ORDER BY name ASC
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to list entity namespaces: {}", e)))?;

    Ok(namespaces)
}

/// Parse the backend_config JSON for a Postgres namespace
pub fn parse_postgres_config(config_json: &str) -> Result<PostgresBackendConfig> {
    serde_json::from_str(config_json)
        .map_err(|e| Error::InvalidInput(format!("Invalid Postgres backend config: {}", e)))
}

/// Parse the backend_config JSON for a filesystem namespace
pub fn parse_filesystem_config(config_json: &str) -> Result<FilesystemBackendConfig> {
    serde_json::from_str(config_json)
        .map_err(|e| Error::InvalidInput(format!("Invalid filesystem backend config: {}", e)))
}

/// Extract namespace from an entity ID (e.g., "person_abc123" -> "person")
pub fn extract_namespace_from_entity_id(entity_id: &str) -> Option<&str> {
    entity_id.split('_').next()
}

/// Build route from entity ID using namespace pattern
/// e.g., "person_abc123" -> "/person/person_abc123"
pub fn entity_id_to_route(entity_id: &str) -> Option<String> {
    let namespace = extract_namespace_from_entity_id(entity_id)?;
    Some(format!("/{}/{}", namespace, entity_id))
}

/// Extract entity ID from route (e.g., "/person/person_abc123" -> "person_abc123")
pub fn route_to_entity_id(route: &str) -> Option<&str> {
    let parts: Vec<&str> = route.trim_start_matches('/').split('/').collect();
    if parts.len() == 2 {
        Some(parts[1])
    } else {
        None
    }
}
