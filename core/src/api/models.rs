// ! API functions for model management

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::error::Result;

/// Model information returned by API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub model_id: String,
    pub display_name: String,
    pub provider: String,
    pub enabled: bool,
    pub sort_order: i32,
    pub context_window: Option<i32>,
    pub max_output_tokens: Option<i32>,
    pub supports_tools: Option<bool>,
    pub is_default: Option<bool>,
}

/// List all system default models (user_id IS NULL)
pub async fn list_models(db: &SqlitePool) -> Result<Vec<ModelInfo>> {
    // SQLite returns INTEGER as i64, need explicit type casts
    let models = sqlx::query_as!(
        ModelInfo,
        r#"
        SELECT
            model_id,
            display_name,
            provider,
            enabled as "enabled: bool",
            sort_order as "sort_order: i32",
            context_window as "context_window: i32",
            max_output_tokens as "max_output_tokens: i32",
            supports_tools as "supports_tools: bool",
            is_default as "is_default: bool"
        FROM app_models
        WHERE user_id IS NULL AND enabled = true
        ORDER BY sort_order ASC
        "#
    )
    .fetch_all(db)
    .await?;

    Ok(models)
}

/// Get the default model (is_default = true)
pub async fn get_default_model(db: &SqlitePool) -> Result<ModelInfo> {
    let model = sqlx::query_as!(
        ModelInfo,
        r#"
        SELECT
            model_id,
            display_name,
            provider,
            enabled as "enabled: bool",
            sort_order as "sort_order: i32",
            context_window as "context_window: i32",
            max_output_tokens as "max_output_tokens: i32",
            supports_tools as "supports_tools: bool",
            is_default as "is_default: bool"
        FROM app_models
        WHERE user_id IS NULL AND enabled = true AND is_default = true
        LIMIT 1
        "#
    )
    .fetch_one(db)
    .await?;

    Ok(model)
}

/// Get a specific model by ID
pub async fn get_model(db: &SqlitePool, model_id: &str) -> Result<ModelInfo> {
    // SQLite returns INTEGER as i64, need explicit type casts
    let model = sqlx::query_as!(
        ModelInfo,
        r#"
        SELECT
            model_id,
            display_name,
            provider,
            enabled as "enabled: bool",
            sort_order as "sort_order: i32",
            context_window as "context_window: i32",
            max_output_tokens as "max_output_tokens: i32",
            supports_tools as "supports_tools: bool",
            is_default as "is_default: bool"
        FROM app_models
        WHERE user_id IS NULL AND model_id = $1
        "#,
        model_id
    )
    .fetch_one(db)
    .await?;

    Ok(model)
}
