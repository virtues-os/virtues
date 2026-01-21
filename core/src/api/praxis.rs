//! Praxis API - Managing temporal pursuits across different time horizons
//!
//! This module provides CRUD operations for:
//! - Tasks: Daily/weekly completable items (including recurring habits)
//! - Initiatives: Medium-term commitments (month-quarter)
//! - Aspirations: Long-term life goals (multi-year)
//! - Calendar: Time-bound events and time blocks
//!
//! These represent the "doing" layer - concrete actions across different time scales.

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::error::{Error, Result};

// ============================================================================
// Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Task {
    pub id: Uuid,
    pub title: String,
    pub description: Option<String>,
    pub tags: Option<Vec<String>>,
    pub thing_id: Option<Uuid>,
    pub status: String,
    pub progress_percent: Option<i32>,
    pub start_date: Option<chrono::DateTime<chrono::Utc>>,
    pub target_date: Option<chrono::DateTime<chrono::Utc>>,
    pub completed_date: Option<chrono::DateTime<chrono::Utc>>,
    // Habit fields
    pub recurrence_rule: Option<String>,
    pub is_habit: Option<bool>,
    pub current_streak: Option<i32>,
    pub best_streak: Option<i32>,
    pub last_completed_date: Option<chrono::NaiveDate>,
    // Hierarchical relations
    pub initiative_id: Option<Uuid>,
    pub parent_task_id: Option<Uuid>,
    // Source integration
    pub source_provider: Option<String>,
    pub external_id: Option<String>,
    pub external_url: Option<String>,
    // Context
    pub purpose: Option<String>,
    pub context_energy: Option<String>,
    pub context_location: Option<String>,
    pub estimated_minutes: Option<i32>,
    pub actual_minutes: Option<i32>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateTaskRequest {
    pub title: String,
    pub description: Option<String>,
    pub tags: Option<Vec<String>>,
    pub thing_id: Option<Uuid>,
    pub start_date: Option<chrono::DateTime<chrono::Utc>>,
    pub target_date: Option<chrono::DateTime<chrono::Utc>>,
    // Hierarchical
    pub initiative_id: Option<Uuid>,
    pub parent_task_id: Option<Uuid>,
    // Source integration
    pub source_provider: Option<String>,
    pub external_id: Option<String>,
    pub external_url: Option<String>,
    pub purpose: Option<String>,
    // Habit fields
    pub is_habit: Option<bool>,
    pub recurrence_rule: Option<String>,
    // Context
    pub context_energy: Option<String>,
    pub context_location: Option<String>,
    pub estimated_minutes: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateTaskRequest {
    pub title: Option<String>,
    pub description: Option<String>,
    pub tags: Option<Vec<String>>,
    pub thing_id: Option<Uuid>,
    pub status: Option<String>,
    pub progress_percent: Option<i32>,
    pub start_date: Option<chrono::DateTime<chrono::Utc>>,
    pub target_date: Option<chrono::DateTime<chrono::Utc>>,
    pub completed_date: Option<chrono::DateTime<chrono::Utc>>,
    // Hierarchical
    pub initiative_id: Option<Uuid>,
    pub parent_task_id: Option<Uuid>,
    pub purpose: Option<String>,
    // Context
    pub context_energy: Option<String>,
    pub context_location: Option<String>,
    pub estimated_minutes: Option<i32>,
    pub actual_minutes: Option<i32>,
}

// Initiative type - medium-term commitments (month-quarter)
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Initiative {
    pub id: Uuid,
    pub title: String,
    pub description: Option<String>,
    pub tags: Option<Vec<String>>,
    pub thing_id: Option<Uuid>,
    pub status: String,
    pub progress_percent: Option<i32>,
    pub start_date: Option<chrono::DateTime<chrono::Utc>>,
    pub target_date: Option<chrono::DateTime<chrono::Utc>>,
    pub completed_date: Option<chrono::DateTime<chrono::Utc>>,
    // Hierarchical
    pub parent_initiative_id: Option<Uuid>,
    // Source integration
    pub source_provider: Option<String>,
    pub external_id: Option<String>,
    pub external_url: Option<String>,
    pub purpose: Option<String>,
    // Commitment tracking
    pub is_commitment: Option<bool>,
    pub success_metrics: Option<serde_json::Value>,
    pub current_metrics: Option<serde_json::Value>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

// Aspiration type - long-term life goals (multi-year)
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Aspiration {
    pub id: Uuid,
    pub title: String,
    pub description: Option<String>,
    pub tags: Option<Vec<String>>,
    pub thing_id: Option<Uuid>,
    pub status: String,
    pub target_timeframe: Option<String>,
    pub target_date: Option<chrono::DateTime<chrono::Utc>>,
    pub achieved_date: Option<chrono::DateTime<chrono::Utc>>,
    // Activation tracking
    pub activated_date: Option<chrono::NaiveDate>,
    pub activated_as_initiative_id: Option<Uuid>,
    // Source integration
    pub source_provider: Option<String>,
    pub external_id: Option<String>,
    pub external_url: Option<String>,
    pub purpose: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateAspirationRequest {
    pub title: String,
    pub description: Option<String>,
    pub tags: Option<Vec<String>>,
    pub thing_id: Option<Uuid>,
    pub target_timeframe: Option<String>,
    pub target_date: Option<chrono::DateTime<chrono::Utc>>,
    // Source integration
    pub source_provider: Option<String>,
    pub external_id: Option<String>,
    pub external_url: Option<String>,
    pub purpose: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateAspirationRequest {
    pub title: Option<String>,
    pub description: Option<String>,
    pub tags: Option<Vec<String>>,
    pub thing_id: Option<Uuid>,
    pub status: Option<String>,
    pub target_timeframe: Option<String>,
    pub target_date: Option<chrono::DateTime<chrono::Utc>>,
    pub achieved_date: Option<chrono::DateTime<chrono::Utc>>,
    pub purpose: Option<String>,
}

// ============================================================================
// Task CRUD Operations
// ============================================================================

/// List all active tasks
pub async fn list_tasks(pool: &SqlitePool) -> Result<Vec<Task>> {
    // Use manual mapping because SQLite returns different types than the struct expects
    let rows = sqlx::query!(
        r#"
        SELECT id, title, description, tags, thing_id, status,
               progress_percent, start_date, target_date, completed_date,
               recurrence_rule, is_habit as "is_habit: bool", current_streak, best_streak, last_completed_date,
               initiative_id, parent_task_id,
               source_provider, external_id, external_url,
               purpose, context_energy, context_location, estimated_minutes, actual_minutes,
               created_at, updated_at
        FROM data_praxis_task
        WHERE status = 'active'
        ORDER BY created_at DESC
        "#
    )
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to list tasks: {}", e)))?;

    let tasks = rows
        .into_iter()
        .filter_map(|row| {
            // SQLite returns: id/thing_id/initiative_id/parent_task_id as Option<String> (UUID TEXT)
            // title/status/created_at/updated_at as String (NOT NULL)
            // dates as Option<String> (nullable, need parsing)
            Some(Task {
                id: row.id.as_ref().and_then(|s| Uuid::parse_str(s).ok())?,
                title: row.title.clone(),
                description: row.description.clone(),
                tags: row.tags.as_ref().and_then(|s| serde_json::from_str(s).ok()),
                thing_id: row.thing_id.as_ref().and_then(|s| Uuid::parse_str(s).ok()),
                status: row.status.clone(),
                progress_percent: row.progress_percent.map(|v| v as i32),
                start_date: row
                    .start_date
                    .as_ref()
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                    .map(|dt| dt.with_timezone(&chrono::Utc)),
                target_date: row
                    .target_date
                    .as_ref()
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                    .map(|dt| dt.with_timezone(&chrono::Utc)),
                completed_date: row
                    .completed_date
                    .as_ref()
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                    .map(|dt| dt.with_timezone(&chrono::Utc)),
                recurrence_rule: row.recurrence_rule.clone(),
                is_habit: row.is_habit,
                current_streak: row.current_streak.map(|v| v as i32),
                best_streak: row.best_streak.map(|v| v as i32),
                last_completed_date: row
                    .last_completed_date
                    .as_ref()
                    .and_then(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()),
                initiative_id: row
                    .initiative_id
                    .as_ref()
                    .and_then(|s| Uuid::parse_str(s).ok()),
                parent_task_id: row
                    .parent_task_id
                    .as_ref()
                    .and_then(|s| Uuid::parse_str(s).ok()),
                source_provider: row.source_provider.clone(),
                external_id: row.external_id.clone(),
                external_url: row.external_url.clone(),
                purpose: row.purpose.clone(),
                context_energy: row.context_energy.clone(),
                context_location: row.context_location.clone(),
                estimated_minutes: row.estimated_minutes.map(|v| v as i32),
                actual_minutes: row.actual_minutes.map(|v| v as i32),
                created_at: chrono::DateTime::parse_from_rfc3339(&row.created_at)
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                    .unwrap_or_else(|_| chrono::Utc::now()),
                updated_at: chrono::DateTime::parse_from_rfc3339(&row.updated_at)
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                    .unwrap_or_else(|_| chrono::Utc::now()),
            })
        })
        .collect();

    Ok(tasks)
}

/// Get a specific task by ID
pub async fn get_task(pool: &SqlitePool, task_id: Uuid) -> Result<Task> {
    let task_id_str = task_id.to_string();
    let row = sqlx::query!(
        r#"
        SELECT id, title, description, tags, thing_id, status,
               progress_percent, start_date, target_date, completed_date,
               recurrence_rule, is_habit as "is_habit: bool", current_streak, best_streak, last_completed_date,
               initiative_id, parent_task_id,
               source_provider, external_id, external_url,
               purpose, context_energy, context_location, estimated_minutes, actual_minutes,
               created_at, updated_at
        FROM data_praxis_task
        WHERE id = $1 AND status != 'archived'
        "#,
        task_id_str
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get task: {}", e)))?
    .ok_or_else(|| Error::NotFound(format!("Task not found: {}", task_id)))?;

    Ok(Task {
        id: row
            .id
            .as_ref()
            .and_then(|s| Uuid::parse_str(s).ok())
            .ok_or_else(|| Error::Database("Invalid task ID".to_string()))?,
        title: row.title.clone(),
        description: row.description.clone(),
        tags: row.tags.as_ref().and_then(|s| serde_json::from_str(s).ok()),
        thing_id: row.thing_id.as_ref().and_then(|s| Uuid::parse_str(s).ok()),
        status: row.status.clone(),
        progress_percent: row.progress_percent.map(|v| v as i32),
        start_date: row
            .start_date
            .as_ref()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc)),
        target_date: row
            .target_date
            .as_ref()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc)),
        completed_date: row
            .completed_date
            .as_ref()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc)),
        recurrence_rule: row.recurrence_rule.clone(),
        is_habit: row.is_habit,
        current_streak: row.current_streak.map(|v| v as i32),
        best_streak: row.best_streak.map(|v| v as i32),
        last_completed_date: row
            .last_completed_date
            .as_ref()
            .and_then(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()),
        initiative_id: row
            .initiative_id
            .as_ref()
            .and_then(|s| Uuid::parse_str(s).ok()),
        parent_task_id: row
            .parent_task_id
            .as_ref()
            .and_then(|s| Uuid::parse_str(s).ok()),
        source_provider: row.source_provider.clone(),
        external_id: row.external_id.clone(),
        external_url: row.external_url.clone(),
        purpose: row.purpose.clone(),
        context_energy: row.context_energy.clone(),
        context_location: row.context_location.clone(),
        estimated_minutes: row.estimated_minutes.map(|v| v as i32),
        actual_minutes: row.actual_minutes.map(|v| v as i32),
        created_at: chrono::DateTime::parse_from_rfc3339(&row.created_at)
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(|_| chrono::Utc::now()),
        updated_at: chrono::DateTime::parse_from_rfc3339(&row.updated_at)
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(|_| chrono::Utc::now()),
    })
}

/// Create a new task
pub async fn create_task(pool: &SqlitePool, req: CreateTaskRequest) -> Result<Task> {
    // Serialize tags to JSON for SQLite
    let tags_json = req
        .tags
        .as_ref()
        .map(|t| serde_json::to_string(t).unwrap_or_else(|_| "[]".to_string()));
    let thing_id_str = req.thing_id.map(|u| u.to_string());
    let initiative_id_str = req.initiative_id.map(|u| u.to_string());
    let parent_task_id_str = req.parent_task_id.map(|u| u.to_string());

    let task_id = Uuid::new_v4().to_string();
    let row = sqlx::query!(
        r#"
        INSERT INTO data_praxis_task
            (id, title, description, tags, thing_id, start_date, target_date,
             initiative_id, parent_task_id, source_provider, external_id, external_url,
             purpose, is_habit, recurrence_rule, context_energy, context_location, estimated_minutes)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18)
        RETURNING id, title, description, tags, thing_id, status,
                  progress_percent, start_date, target_date, completed_date,
                  recurrence_rule, is_habit as "is_habit: bool", current_streak, best_streak, last_completed_date,
                  initiative_id, parent_task_id,
                  source_provider, external_id, external_url,
                  purpose, context_energy, context_location, estimated_minutes, actual_minutes,
                  created_at, updated_at
        "#,
        task_id,
        req.title,
        req.description,
        tags_json,
        thing_id_str,
        req.start_date,
        req.target_date,
        initiative_id_str,
        parent_task_id_str,
        req.source_provider,
        req.external_id,
        req.external_url,
        req.purpose,
        req.is_habit,
        req.recurrence_rule,
        req.context_energy,
        req.context_location,
        req.estimated_minutes
    )
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to create task: {}", e)))?;

    Ok(Task {
        id: row
            .id
            .as_ref()
            .and_then(|s| Uuid::parse_str(s).ok())
            .ok_or_else(|| Error::Database("Invalid task ID".to_string()))?,
        title: row.title.clone(),
        description: row.description.clone(),
        tags: row.tags.as_ref().and_then(|s| serde_json::from_str(s).ok()),
        thing_id: row.thing_id.as_ref().and_then(|s| Uuid::parse_str(s).ok()),
        status: row.status.clone(),
        progress_percent: row.progress_percent.map(|v| v as i32),
        start_date: row
            .start_date
            .as_ref()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc)),
        target_date: row
            .target_date
            .as_ref()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc)),
        completed_date: row
            .completed_date
            .as_ref()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc)),
        recurrence_rule: row.recurrence_rule.clone(),
        is_habit: row.is_habit,
        current_streak: Some(row.current_streak as i32),
        best_streak: Some(row.best_streak as i32),
        last_completed_date: row
            .last_completed_date
            .as_ref()
            .and_then(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()),
        initiative_id: row
            .initiative_id
            .as_ref()
            .and_then(|s| Uuid::parse_str(s).ok()),
        parent_task_id: row
            .parent_task_id
            .as_ref()
            .and_then(|s| Uuid::parse_str(s).ok()),
        source_provider: row.source_provider.clone(),
        external_id: row.external_id.clone(),
        external_url: row.external_url.clone(),
        purpose: row.purpose.clone(),
        context_energy: row.context_energy.clone(),
        context_location: row.context_location.clone(),
        estimated_minutes: row.estimated_minutes.map(|v| v as i32),
        actual_minutes: row.actual_minutes.map(|v| v as i32),
        created_at: chrono::DateTime::parse_from_rfc3339(&row.created_at)
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(|_| chrono::Utc::now()),
        updated_at: chrono::DateTime::parse_from_rfc3339(&row.updated_at)
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(|_| chrono::Utc::now()),
    })
}

/// Update an existing task
pub async fn update_task(pool: &SqlitePool, task_id: Uuid, req: UpdateTaskRequest) -> Result<Task> {
    // Serialize tags and UUIDs as strings for SQLite
    let task_id_str = task_id.to_string();
    let tags_json = req
        .tags
        .as_ref()
        .map(|t| serde_json::to_string(t).unwrap_or_else(|_| "[]".to_string()));
    let thing_id_str = req.thing_id.map(|u| u.to_string());
    let initiative_id_str = req.initiative_id.map(|u| u.to_string());
    let parent_task_id_str = req.parent_task_id.map(|u| u.to_string());

    let row = sqlx::query!(
        r#"
        UPDATE data_praxis_task
        SET title = COALESCE($2, title),
            description = COALESCE($3, description),
            tags = COALESCE($4, tags),
            thing_id = COALESCE($5, thing_id),
            status = COALESCE($6, status),
            progress_percent = COALESCE($7, progress_percent),
            start_date = COALESCE($8, start_date),
            target_date = COALESCE($9, target_date),
            completed_date = COALESCE($10, completed_date),
            initiative_id = COALESCE($11, initiative_id),
            parent_task_id = COALESCE($12, parent_task_id),
            purpose = COALESCE($13, purpose),
            context_energy = COALESCE($14, context_energy),
            context_location = COALESCE($15, context_location),
            estimated_minutes = COALESCE($16, estimated_minutes),
            actual_minutes = COALESCE($17, actual_minutes),
            updated_at = datetime('now')
        WHERE id = $1 AND status != 'archived'
        RETURNING id, title, description, tags, thing_id, status,
                  progress_percent, start_date, target_date, completed_date,
                  recurrence_rule, is_habit as "is_habit: bool", current_streak, best_streak, last_completed_date,
                  initiative_id, parent_task_id,
                  source_provider, external_id, external_url,
                  purpose, context_energy, context_location, estimated_minutes, actual_minutes,
                  created_at, updated_at
        "#,
        task_id_str,
        req.title,
        req.description,
        tags_json,
        thing_id_str,
        req.status,
        req.progress_percent,
        req.start_date,
        req.target_date,
        req.completed_date,
        initiative_id_str,
        parent_task_id_str,
        req.purpose,
        req.context_energy,
        req.context_location,
        req.estimated_minutes,
        req.actual_minutes
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to update task: {}", e)))?
    .ok_or_else(|| Error::NotFound(format!("Task not found: {}", task_id)))?;

    Ok(Task {
        id: row
            .id
            .as_ref()
            .and_then(|s| Uuid::parse_str(s).ok())
            .ok_or_else(|| Error::Database("Invalid task ID".to_string()))?,
        title: row.title.clone(),
        description: row.description.clone(),
        tags: row.tags.as_ref().and_then(|s| serde_json::from_str(s).ok()),
        thing_id: row.thing_id.as_ref().and_then(|s| Uuid::parse_str(s).ok()),
        status: row.status.clone(),
        progress_percent: row.progress_percent.map(|v| v as i32),
        start_date: row
            .start_date
            .as_ref()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc)),
        target_date: row
            .target_date
            .as_ref()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc)),
        completed_date: row
            .completed_date
            .as_ref()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc)),
        recurrence_rule: row.recurrence_rule.clone(),
        is_habit: row.is_habit,
        current_streak: row.current_streak.map(|v| v as i32),
        best_streak: row.best_streak.map(|v| v as i32),
        last_completed_date: row
            .last_completed_date
            .as_ref()
            .and_then(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()),
        initiative_id: row
            .initiative_id
            .as_ref()
            .and_then(|s| Uuid::parse_str(s).ok()),
        parent_task_id: row
            .parent_task_id
            .as_ref()
            .and_then(|s| Uuid::parse_str(s).ok()),
        source_provider: row.source_provider.clone(),
        external_id: row.external_id.clone(),
        external_url: row.external_url.clone(),
        purpose: row.purpose.clone(),
        context_energy: row.context_energy.clone(),
        context_location: row.context_location.clone(),
        estimated_minutes: row.estimated_minutes.map(|v| v as i32),
        actual_minutes: row.actual_minutes.map(|v| v as i32),
        created_at: chrono::DateTime::parse_from_rfc3339(&row.created_at)
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(|_| chrono::Utc::now()),
        updated_at: chrono::DateTime::parse_from_rfc3339(&row.updated_at)
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(|_| chrono::Utc::now()),
    })
}

/// Delete a task (soft delete - sets status = 'archived')
pub async fn delete_task(pool: &SqlitePool, task_id: Uuid) -> Result<()> {
    let task_id_str = task_id.to_string();
    let result = sqlx::query!(
        r#"
        UPDATE data_praxis_task
        SET status = 'archived', updated_at = datetime('now')
        WHERE id = $1 AND status != 'archived'
        "#,
        task_id_str
    )
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to delete task: {}", e)))?;

    if result.rows_affected() == 0 {
        return Err(Error::NotFound(format!("Task not found: {}", task_id)));
    }

    Ok(())
}

// ============================================================================
// Initiative CRUD Operations
// ============================================================================

pub async fn list_initiatives(pool: &SqlitePool) -> Result<Vec<Initiative>> {
    let rows = sqlx::query!(
        r#"
        SELECT id, title, description, tags, thing_id, status,
               progress_percent, start_date, target_date, completed_date,
               parent_initiative_id,
               source_provider, external_id, external_url, purpose,
               is_commitment as "is_commitment: bool", success_metrics, current_metrics,
               created_at, updated_at
        FROM data_praxis_initiative
        WHERE status = 'active'
        ORDER BY created_at DESC
        "#
    )
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to list initiatives: {}", e)))?;

    let items = rows
        .into_iter()
        .filter_map(|row| {
            Some(Initiative {
                id: row.id.as_ref().and_then(|s| Uuid::parse_str(s).ok())?,
                title: row.title.clone(),
                description: row.description.clone(),
                tags: row.tags.as_ref().and_then(|s| serde_json::from_str(s).ok()),
                thing_id: row.thing_id.as_ref().and_then(|s| Uuid::parse_str(s).ok()),
                status: row.status.clone(),
                progress_percent: row.progress_percent.map(|v| v as i32),
                start_date: row
                    .start_date
                    .as_ref()
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                    .map(|dt| dt.with_timezone(&chrono::Utc)),
                target_date: row
                    .target_date
                    .as_ref()
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                    .map(|dt| dt.with_timezone(&chrono::Utc)),
                completed_date: row
                    .completed_date
                    .as_ref()
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                    .map(|dt| dt.with_timezone(&chrono::Utc)),
                parent_initiative_id: row
                    .parent_initiative_id
                    .as_ref()
                    .and_then(|s| Uuid::parse_str(s).ok()),
                source_provider: row.source_provider.clone(),
                external_id: row.external_id.clone(),
                external_url: row.external_url.clone(),
                purpose: row.purpose.clone(),
                is_commitment: row.is_commitment,
                success_metrics: row
                    .success_metrics
                    .as_ref()
                    .and_then(|s| serde_json::from_str(s).ok()),
                current_metrics: row
                    .current_metrics
                    .as_ref()
                    .and_then(|s| serde_json::from_str(s).ok()),
                created_at: chrono::DateTime::parse_from_rfc3339(&row.created_at)
                    .ok()?
                    .with_timezone(&chrono::Utc),
                updated_at: chrono::DateTime::parse_from_rfc3339(&row.updated_at)
                    .ok()?
                    .with_timezone(&chrono::Utc),
            })
        })
        .collect();

    Ok(items)
}

pub async fn get_initiative(pool: &SqlitePool, id: Uuid) -> Result<Initiative> {
    let id_str = id.to_string();
    let row = sqlx::query!(
        r#"
        SELECT id, title, description, tags, thing_id, status,
               progress_percent, start_date, target_date, completed_date,
               parent_initiative_id,
               source_provider, external_id, external_url, purpose,
               is_commitment as "is_commitment: bool", success_metrics, current_metrics,
               created_at, updated_at
        FROM data_praxis_initiative
        WHERE id = $1 AND status != 'archived'
        "#,
        id_str
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get initiative: {}", e)))?
    .ok_or_else(|| Error::NotFound(format!("Initiative not found: {}", id)))?;

    Ok(Initiative {
        id: row
            .id
            .as_ref()
            .and_then(|s| Uuid::parse_str(s).ok())
            .ok_or_else(|| Error::Database("Invalid initiative ID".to_string()))?,
        title: row.title.clone(),
        description: row.description.clone(),
        tags: row.tags.as_ref().and_then(|s| serde_json::from_str(s).ok()),
        thing_id: row.thing_id.as_ref().and_then(|s| Uuid::parse_str(s).ok()),
        status: row.status.clone(),
        progress_percent: row.progress_percent.map(|v| v as i32),
        start_date: row
            .start_date
            .as_ref()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc)),
        target_date: row
            .target_date
            .as_ref()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc)),
        completed_date: row
            .completed_date
            .as_ref()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc)),
        parent_initiative_id: row
            .parent_initiative_id
            .as_ref()
            .and_then(|s| Uuid::parse_str(s).ok()),
        source_provider: row.source_provider.clone(),
        external_id: row.external_id.clone(),
        external_url: row.external_url.clone(),
        purpose: row.purpose.clone(),
        is_commitment: row.is_commitment,
        success_metrics: row
            .success_metrics
            .as_ref()
            .and_then(|s| serde_json::from_str(s).ok()),
        current_metrics: row
            .current_metrics
            .as_ref()
            .and_then(|s| serde_json::from_str(s).ok()),
        created_at: chrono::DateTime::parse_from_rfc3339(&row.created_at)
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(|_| chrono::Utc::now()),
        updated_at: chrono::DateTime::parse_from_rfc3339(&row.updated_at)
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(|_| chrono::Utc::now()),
    })
}

pub async fn create_initiative(pool: &SqlitePool, req: CreateTaskRequest) -> Result<Initiative> {
    let tags_json = req
        .tags
        .as_ref()
        .map(|t| serde_json::to_string(t).unwrap_or_else(|_| "[]".to_string()));
    let thing_id_str = req.thing_id.map(|u| u.to_string());

    let initiative_id = Uuid::new_v4().to_string();
    let row = sqlx::query!(
        r#"
        INSERT INTO data_praxis_initiative
            (id, title, description, tags, thing_id, start_date, target_date,
             source_provider, external_id, external_url, purpose)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
        RETURNING id, title, description, tags, thing_id, status,
                  progress_percent, start_date, target_date, completed_date,
                  parent_initiative_id,
                  source_provider, external_id, external_url, purpose,
                  is_commitment as "is_commitment: bool", success_metrics, current_metrics,
                  created_at, updated_at
        "#,
        initiative_id,
        req.title,
        req.description,
        tags_json,
        thing_id_str,
        req.start_date,
        req.target_date,
        req.source_provider,
        req.external_id,
        req.external_url,
        req.purpose
    )
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to create initiative: {}", e)))?;

    Ok(Initiative {
        id: row
            .id
            .as_ref()
            .and_then(|s| Uuid::parse_str(s).ok())
            .ok_or_else(|| Error::Database("Invalid initiative ID".to_string()))?,
        title: row.title.clone(),
        description: row.description.clone(),
        tags: row.tags.as_ref().and_then(|s| serde_json::from_str(s).ok()),
        thing_id: row.thing_id.as_ref().and_then(|s| Uuid::parse_str(s).ok()),
        status: row.status.clone(),
        progress_percent: row.progress_percent.map(|v| v as i32),
        start_date: row
            .start_date
            .as_ref()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc)),
        target_date: row
            .target_date
            .as_ref()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc)),
        completed_date: row
            .completed_date
            .as_ref()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc)),
        parent_initiative_id: row
            .parent_initiative_id
            .as_ref()
            .and_then(|s| Uuid::parse_str(s).ok()),
        source_provider: row.source_provider.clone(),
        external_id: row.external_id.clone(),
        external_url: row.external_url.clone(),
        purpose: row.purpose.clone(),
        is_commitment: Some(row.is_commitment),
        success_metrics: row
            .success_metrics
            .as_ref()
            .and_then(|s| serde_json::from_str(s).ok()),
        current_metrics: row
            .current_metrics
            .as_ref()
            .and_then(|s| serde_json::from_str(s).ok()),
        created_at: chrono::DateTime::parse_from_rfc3339(&row.created_at)
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(|_| chrono::Utc::now()),
        updated_at: chrono::DateTime::parse_from_rfc3339(&row.updated_at)
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(|_| chrono::Utc::now()),
    })
}

pub async fn update_initiative(
    pool: &SqlitePool,
    id: Uuid,
    req: UpdateTaskRequest,
) -> Result<Initiative> {
    let id_str = id.to_string();
    let tags_json = req
        .tags
        .as_ref()
        .map(|t| serde_json::to_string(t).unwrap_or_else(|_| "[]".to_string()));
    let thing_id_str = req.thing_id.map(|u| u.to_string());

    let row = sqlx::query!(
        r#"
        UPDATE data_praxis_initiative
        SET title = COALESCE($2, title),
            description = COALESCE($3, description),
            tags = COALESCE($4, tags),
            thing_id = COALESCE($5, thing_id),
            status = COALESCE($6, status),
            progress_percent = COALESCE($7, progress_percent),
            start_date = COALESCE($8, start_date),
            target_date = COALESCE($9, target_date),
            completed_date = COALESCE($10, completed_date),
            purpose = COALESCE($11, purpose),
            updated_at = datetime('now')
        WHERE id = $1 AND status != 'archived'
        RETURNING id, title, description, tags, thing_id, status,
                  progress_percent, start_date, target_date, completed_date,
                  parent_initiative_id,
                  source_provider, external_id, external_url, purpose,
                  is_commitment as "is_commitment: bool", success_metrics, current_metrics,
                  created_at, updated_at
        "#,
        id_str,
        req.title,
        req.description,
        tags_json,
        thing_id_str,
        req.status,
        req.progress_percent,
        req.start_date,
        req.target_date,
        req.completed_date,
        req.purpose
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to update initiative: {}", e)))?
    .ok_or_else(|| Error::NotFound(format!("Initiative not found: {}", id)))?;

    Ok(Initiative {
        id: row
            .id
            .as_ref()
            .and_then(|s| Uuid::parse_str(s).ok())
            .ok_or_else(|| Error::Database("Invalid initiative ID".to_string()))?,
        title: row.title.clone(),
        description: row.description.clone(),
        tags: row.tags.as_ref().and_then(|s| serde_json::from_str(s).ok()),
        thing_id: row.thing_id.as_ref().and_then(|s| Uuid::parse_str(s).ok()),
        status: row.status.clone(),
        progress_percent: row.progress_percent.map(|v| v as i32),
        start_date: row
            .start_date
            .as_ref()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc)),
        target_date: row
            .target_date
            .as_ref()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc)),
        completed_date: row
            .completed_date
            .as_ref()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc)),
        parent_initiative_id: row
            .parent_initiative_id
            .as_ref()
            .and_then(|s| Uuid::parse_str(s).ok()),
        source_provider: row.source_provider.clone(),
        external_id: row.external_id.clone(),
        external_url: row.external_url.clone(),
        purpose: row.purpose.clone(),
        is_commitment: row.is_commitment,
        success_metrics: row
            .success_metrics
            .as_ref()
            .and_then(|s| serde_json::from_str(s).ok()),
        current_metrics: row
            .current_metrics
            .as_ref()
            .and_then(|s| serde_json::from_str(s).ok()),
        created_at: chrono::DateTime::parse_from_rfc3339(&row.created_at)
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(|_| chrono::Utc::now()),
        updated_at: chrono::DateTime::parse_from_rfc3339(&row.updated_at)
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(|_| chrono::Utc::now()),
    })
}

pub async fn delete_initiative(pool: &SqlitePool, id: Uuid) -> Result<()> {
    let id_str = id.to_string();
    let result = sqlx::query!(
        r#"
        UPDATE data_praxis_initiative
        SET status = 'archived', updated_at = datetime('now')
        WHERE id = $1 AND status != 'archived'
        "#,
        id_str
    )
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to delete initiative: {}", e)))?;

    if result.rows_affected() == 0 {
        return Err(Error::NotFound(format!("Initiative not found: {}", id)));
    }

    Ok(())
}

// ============================================================================
// Aspiration CRUD Operations
// ============================================================================

pub async fn list_aspirations(pool: &SqlitePool) -> Result<Vec<Aspiration>> {
    let rows = sqlx::query!(
        r#"
        SELECT id, title, description, tags, thing_id, status,
               target_timeframe, target_date, achieved_date,
               activated_date, activated_as_initiative_id,
               source_provider, external_id, external_url, purpose,
               created_at, updated_at
        FROM data_praxis_aspiration
        WHERE status IN ('dreaming', 'activated')
        ORDER BY created_at DESC
        "#
    )
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to list aspirations: {}", e)))?;

    let items = rows
        .into_iter()
        .filter_map(|row| {
            Some(Aspiration {
                id: row.id.as_ref().and_then(|s| Uuid::parse_str(s).ok())?,
                title: row.title.clone(),
                description: row.description.clone(),
                tags: row.tags.as_ref().and_then(|s| serde_json::from_str(s).ok()),
                thing_id: row.thing_id.as_ref().and_then(|s| Uuid::parse_str(s).ok()),
                status: row.status.clone(),
                target_timeframe: row.target_timeframe.clone(),
                target_date: row
                    .target_date
                    .as_ref()
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                    .map(|dt| dt.with_timezone(&chrono::Utc)),
                achieved_date: row
                    .achieved_date
                    .as_ref()
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                    .map(|dt| dt.with_timezone(&chrono::Utc)),
                activated_date: row
                    .activated_date
                    .as_ref()
                    .and_then(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()),
                activated_as_initiative_id: row
                    .activated_as_initiative_id
                    .as_ref()
                    .and_then(|s| Uuid::parse_str(s).ok()),
                source_provider: row.source_provider.clone(),
                external_id: row.external_id.clone(),
                external_url: row.external_url.clone(),
                purpose: row.purpose.clone(),
                created_at: chrono::DateTime::parse_from_rfc3339(&row.created_at)
                    .ok()?
                    .with_timezone(&chrono::Utc),
                updated_at: chrono::DateTime::parse_from_rfc3339(&row.updated_at)
                    .ok()?
                    .with_timezone(&chrono::Utc),
            })
        })
        .collect();

    Ok(items)
}

pub async fn get_aspiration(pool: &SqlitePool, id: Uuid) -> Result<Aspiration> {
    let id_str = id.to_string();
    let row = sqlx::query!(
        r#"
        SELECT id, title, description, tags, thing_id, status,
               target_timeframe, target_date, achieved_date,
               activated_date, activated_as_initiative_id,
               source_provider, external_id, external_url, purpose,
               created_at, updated_at
        FROM data_praxis_aspiration
        WHERE id = $1 AND status != 'archived'
        "#,
        id_str
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to get aspiration: {}", e)))?
    .ok_or_else(|| Error::NotFound(format!("Aspiration not found: {}", id)))?;

    Ok(Aspiration {
        id: row
            .id
            .as_ref()
            .and_then(|s| Uuid::parse_str(s).ok())
            .ok_or_else(|| Error::Database("Invalid aspiration ID".to_string()))?,
        title: row.title.clone(),
        description: row.description.clone(),
        tags: row.tags.as_ref().and_then(|s| serde_json::from_str(s).ok()),
        thing_id: row.thing_id.as_ref().and_then(|s| Uuid::parse_str(s).ok()),
        status: row.status.clone(),
        target_timeframe: row.target_timeframe.clone(),
        target_date: row
            .target_date
            .as_ref()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc)),
        achieved_date: row
            .achieved_date
            .as_ref()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc)),
        activated_date: row
            .activated_date
            .as_ref()
            .and_then(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()),
        activated_as_initiative_id: row
            .activated_as_initiative_id
            .as_ref()
            .and_then(|s| Uuid::parse_str(s).ok()),
        source_provider: row.source_provider.clone(),
        external_id: row.external_id.clone(),
        external_url: row.external_url.clone(),
        purpose: row.purpose.clone(),
        created_at: chrono::DateTime::parse_from_rfc3339(&row.created_at)
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(|_| chrono::Utc::now()),
        updated_at: chrono::DateTime::parse_from_rfc3339(&row.updated_at)
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(|_| chrono::Utc::now()),
    })
}

pub async fn create_aspiration(
    pool: &SqlitePool,
    req: CreateAspirationRequest,
) -> Result<Aspiration> {
    let tags_json = req
        .tags
        .as_ref()
        .map(|t| serde_json::to_string(t).unwrap_or_else(|_| "[]".to_string()));
    let thing_id_str = req.thing_id.map(|u| u.to_string());

    let aspiration_id = Uuid::new_v4().to_string();
    let row = sqlx::query!(
        r#"
        INSERT INTO data_praxis_aspiration
            (id, title, description, tags, thing_id, target_timeframe, target_date,
             source_provider, external_id, external_url, purpose)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
        RETURNING id, title, description, tags, thing_id, status,
                  target_timeframe, target_date, achieved_date,
                  activated_date, activated_as_initiative_id,
                  source_provider, external_id, external_url, purpose,
                  created_at, updated_at
        "#,
        aspiration_id,
        req.title,
        req.description,
        tags_json,
        thing_id_str,
        req.target_timeframe,
        req.target_date,
        req.source_provider,
        req.external_id,
        req.external_url,
        req.purpose
    )
    .fetch_one(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to create aspiration: {}", e)))?;

    Ok(Aspiration {
        id: row
            .id
            .as_ref()
            .and_then(|s| Uuid::parse_str(s).ok())
            .ok_or_else(|| Error::Database("Invalid aspiration ID".to_string()))?,
        title: row.title.clone(),
        description: row.description.clone(),
        tags: row.tags.as_ref().and_then(|s| serde_json::from_str(s).ok()),
        thing_id: row.thing_id.as_ref().and_then(|s| Uuid::parse_str(s).ok()),
        status: row.status.clone(),
        target_timeframe: row.target_timeframe.clone(),
        target_date: row
            .target_date
            .as_ref()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc)),
        achieved_date: row
            .achieved_date
            .as_ref()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc)),
        activated_date: row
            .activated_date
            .as_ref()
            .and_then(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()),
        activated_as_initiative_id: row
            .activated_as_initiative_id
            .as_ref()
            .and_then(|s| Uuid::parse_str(s).ok()),
        source_provider: row.source_provider.clone(),
        external_id: row.external_id.clone(),
        external_url: row.external_url.clone(),
        purpose: row.purpose.clone(),
        created_at: chrono::DateTime::parse_from_rfc3339(&row.created_at)
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(|_| chrono::Utc::now()),
        updated_at: chrono::DateTime::parse_from_rfc3339(&row.updated_at)
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(|_| chrono::Utc::now()),
    })
}

pub async fn update_aspiration(
    pool: &SqlitePool,
    id: Uuid,
    req: UpdateAspirationRequest,
) -> Result<Aspiration> {
    let id_str = id.to_string();
    let tags_json = req
        .tags
        .as_ref()
        .map(|t| serde_json::to_string(t).unwrap_or_else(|_| "[]".to_string()));
    let thing_id_str = req.thing_id.map(|u| u.to_string());

    let row = sqlx::query!(
        r#"
        UPDATE data_praxis_aspiration
        SET title = COALESCE($2, title),
            description = COALESCE($3, description),
            tags = COALESCE($4, tags),
            thing_id = COALESCE($5, thing_id),
            status = COALESCE($6, status),
            target_timeframe = COALESCE($7, target_timeframe),
            target_date = COALESCE($8, target_date),
            achieved_date = COALESCE($9, achieved_date),
            purpose = COALESCE($10, purpose),
            updated_at = datetime('now')
        WHERE id = $1 AND status != 'archived'
        RETURNING id, title, description, tags, thing_id, status,
                  target_timeframe, target_date, achieved_date,
                  activated_date, activated_as_initiative_id,
                  source_provider, external_id, external_url, purpose,
                  created_at, updated_at
        "#,
        id_str,
        req.title,
        req.description,
        tags_json,
        thing_id_str,
        req.status,
        req.target_timeframe,
        req.target_date,
        req.achieved_date,
        req.purpose
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to update aspiration: {}", e)))?
    .ok_or_else(|| Error::NotFound(format!("Aspiration not found: {}", id)))?;

    Ok(Aspiration {
        id: row
            .id
            .as_ref()
            .and_then(|s| Uuid::parse_str(s).ok())
            .ok_or_else(|| Error::Database("Invalid aspiration ID".to_string()))?,
        title: row.title.clone(),
        description: row.description.clone(),
        tags: row.tags.as_ref().and_then(|s| serde_json::from_str(s).ok()),
        thing_id: row.thing_id.as_ref().and_then(|s| Uuid::parse_str(s).ok()),
        status: row.status.clone(),
        target_timeframe: row.target_timeframe.clone(),
        target_date: row
            .target_date
            .as_ref()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc)),
        achieved_date: row
            .achieved_date
            .as_ref()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc)),
        activated_date: row
            .activated_date
            .as_ref()
            .and_then(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()),
        activated_as_initiative_id: row
            .activated_as_initiative_id
            .as_ref()
            .and_then(|s| Uuid::parse_str(s).ok()),
        source_provider: row.source_provider.clone(),
        external_id: row.external_id.clone(),
        external_url: row.external_url.clone(),
        purpose: row.purpose.clone(),
        created_at: chrono::DateTime::parse_from_rfc3339(&row.created_at)
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(|_| chrono::Utc::now()),
        updated_at: chrono::DateTime::parse_from_rfc3339(&row.updated_at)
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(|_| chrono::Utc::now()),
    })
}

pub async fn delete_aspiration(pool: &SqlitePool, id: Uuid) -> Result<()> {
    let id_str = id.to_string();
    let result = sqlx::query!(
        r#"
        UPDATE data_praxis_aspiration
        SET status = 'archived', updated_at = datetime('now')
        WHERE id = $1 AND status != 'archived'
        "#,
        id_str
    )
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to delete aspiration: {}", e)))?;

    if result.rows_affected() == 0 {
        return Err(Error::NotFound(format!("Aspiration not found: {}", id)));
    }

    Ok(())
}

/// List all distinct tags used across all active temporal pursuits (tasks, initiatives, aspirations)
pub async fn list_tags(pool: &SqlitePool) -> Result<Vec<String>> {
    // SQLite uses json_each() to expand JSON arrays instead of PostgreSQL's unnest()
    // Use query_scalar for simpler single-column results
    let rows = sqlx::query_scalar::<_, String>(
        r#"
        SELECT DISTINCT j.value
        FROM data_praxis_task, json_each(tags) j
        WHERE status = 'active' AND tags IS NOT NULL AND tags != '[]'
        UNION
        SELECT DISTINCT j.value
        FROM data_praxis_initiative, json_each(tags) j
        WHERE status = 'active' AND tags IS NOT NULL AND tags != '[]'
        UNION
        SELECT DISTINCT j.value
        FROM data_praxis_aspiration, json_each(tags) j
        WHERE status IN ('dreaming', 'activated') AND tags IS NOT NULL AND tags != '[]'
        ORDER BY 1
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to list tags: {}", e)))?;

    Ok(rows)
}
