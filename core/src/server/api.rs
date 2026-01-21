//! REST API handlers for source management

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::ingest::AppState;
use crate::error::Error;

/// Helper to convert Result to Response with proper status code
fn api_response<T: Serialize>(result: crate::error::Result<T>) -> Response {
    match result {
        Ok(data) => (StatusCode::OK, Json(data)).into_response(),
        Err(e) => error_response(e),
    }
}

/// Helper to convert Error to Response with appropriate status code
fn error_response(error: Error) -> Response {
    let (status, message) = match &error {
        Error::NotFound(_) => (StatusCode::NOT_FOUND, error.to_string()),
        Error::Unauthorized(_) => (StatusCode::UNAUTHORIZED, error.to_string()),
        Error::InvalidInput(_) => (StatusCode::BAD_REQUEST, error.to_string()),
        Error::Database(msg) if msg.contains("already has an active") => {
            (StatusCode::CONFLICT, error.to_string())
        }
        _ => (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
    };

    (status, Json(serde_json::json!({ "error": message }))).into_response()
}

/// Helper to create a success message response
fn success_message(message: &str) -> Response {
    (
        StatusCode::OK,
        Json(serde_json::json!({ "message": message })),
    )
        .into_response()
}

/// List all sources
pub async fn list_sources_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::list_sources(state.db.pool()).await)
}

/// Get a specific source by ID
pub async fn get_source_handler(
    State(state): State<AppState>,
    Path(source_id): Path<Uuid>,
) -> Response {
    api_response(crate::api::get_source(state.db.pool(), source_id).await)
}

/// Pause a source
pub async fn pause_source_handler(
    State(state): State<AppState>,
    Path(source_id): Path<Uuid>,
) -> Response {
    api_response(crate::api::pause_source(state.db.pool(), source_id).await)
}

/// Resume a source
pub async fn resume_source_handler(
    State(state): State<AppState>,
    Path(source_id): Path<Uuid>,
) -> Response {
    api_response(crate::api::resume_source(state.db.pool(), source_id).await)
}

/// Delete a source by ID
pub async fn delete_source_handler(
    State(state): State<AppState>,
    Path(source_id): Path<Uuid>,
) -> Response {
    match crate::api::delete_source(state.db.pool(), source_id).await {
        Ok(_) => success_message("Source deleted successfully"),
        Err(e) => error_response(e),
    }
}

/// Get source status with statistics
pub async fn get_source_status_handler(
    State(state): State<AppState>,
    Path(source_id): Path<Uuid>,
) -> Response {
    match crate::api::get_source_status(state.db.pool(), source_id).await {
        Ok(status) => (StatusCode::OK, Json(status)).into_response(),
        Err(e) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": e.to_string()
            })),
        )
            .into_response(),
    }
}

/// Initiate OAuth authorization flow
pub async fn oauth_authorize_handler(
    Path(provider): Path<String>,
    Query(params): Query<crate::api::OAuthAuthorizeRequest>,
) -> Response {
    match crate::api::initiate_oauth_flow(&provider, params.redirect_uri, params.state).await {
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": e.to_string()
            })),
        )
            .into_response(),
    }
}

/// Handle OAuth callback
pub async fn oauth_callback_handler(
    State(state): State<AppState>,
    Query(params): Query<crate::api::OAuthCallbackParams>,
) -> Response {
    match crate::api::handle_oauth_callback(state.db.pool(), &params).await {
        Ok(source) => (StatusCode::CREATED, Json(source)).into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": e.to_string()
            })),
        )
            .into_response(),
    }
}

/// Create a source manually
pub async fn create_source_handler(
    State(state): State<AppState>,
    Json(request): Json<crate::api::CreateSourceRequest>,
) -> Response {
    match crate::api::create_source(state.db.pool(), request).await {
        Ok(source) => (StatusCode::CREATED, Json(source)).into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": e.to_string()
            })),
        )
            .into_response(),
    }
}

/// Register a device as a source
pub async fn register_device_handler(
    State(state): State<AppState>,
    Json(request): Json<crate::api::RegisterDeviceRequest>,
) -> Response {
    match crate::api::register_device(state.db.pool(), request).await {
        Ok(source) => (StatusCode::CREATED, Json(source)).into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": e.to_string()
            })),
        )
            .into_response(),
    }
}

/// List all streams for a source
pub async fn list_streams_handler(
    State(state): State<AppState>,
    Path(source_id): Path<Uuid>,
) -> Response {
    match crate::api::list_source_streams(state.db.pool(), source_id).await {
        Ok(streams) => (StatusCode::OK, Json(streams)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": e.to_string()
            })),
        )
            .into_response(),
    }
}

/// Get stream details
pub async fn get_stream_handler(
    State(state): State<AppState>,
    Path((source_id, stream_name)): Path<(Uuid, String)>,
) -> Response {
    match crate::api::get_stream_info(state.db.pool(), source_id, &stream_name).await {
        Ok(stream) => (StatusCode::OK, Json(stream)).into_response(),
        Err(e) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": e.to_string()
            })),
        )
            .into_response(),
    }
}

/// Enable a stream
pub async fn enable_stream_handler(
    State(state): State<AppState>,
    Path((source_id, stream_name)): Path<(Uuid, String)>,
    Json(request): Json<crate::api::EnableStreamRequest>,
) -> Response {
    match crate::api::enable_stream(
        state.db.pool(),
        &*state.storage,
        state.stream_writer.clone(),
        source_id,
        &stream_name,
        request.config,
    )
    .await
    {
        Ok(stream) => (StatusCode::OK, Json(stream)).into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": e.to_string()
            })),
        )
            .into_response(),
    }
}

/// Disable a stream
pub async fn disable_stream_handler(
    State(state): State<AppState>,
    Path((source_id, stream_name)): Path<(Uuid, String)>,
) -> Response {
    match crate::api::disable_stream(state.db.pool(), source_id, &stream_name).await {
        Ok(_) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "message": "Stream disabled successfully"
            })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": e.to_string()
            })),
        )
            .into_response(),
    }
}

/// Update stream configuration
pub async fn update_stream_config_handler(
    State(state): State<AppState>,
    Path((source_id, stream_name)): Path<(Uuid, String)>,
    Json(request): Json<crate::api::UpdateStreamConfigRequest>,
) -> Response {
    match crate::api::update_stream_config(state.db.pool(), source_id, &stream_name, request.config)
        .await
    {
        Ok(stream) => (StatusCode::OK, Json(stream)).into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": e.to_string()
            })),
        )
            .into_response(),
    }
}

/// Update stream schedule
pub async fn update_stream_schedule_handler(
    State(state): State<AppState>,
    Path((source_id, stream_name)): Path<(Uuid, String)>,
    Json(request): Json<crate::api::UpdateStreamScheduleRequest>,
) -> Response {
    match crate::api::update_stream_schedule(
        state.db.pool(),
        source_id,
        &stream_name,
        request.cron_schedule,
    )
    .await
    {
        Ok(stream) => (StatusCode::OK, Json(stream)).into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": e.to_string()
            })),
        )
            .into_response(),
    }
}

/// Trigger a manual sync for a stream (async job-based)
pub async fn sync_stream_handler(
    State(state): State<AppState>,
    Path((source_id, stream_name)): Path<(Uuid, String)>,
    Json(request): Json<Option<SyncStreamRequest>>,
) -> Response {
    // Parse sync mode from request (default to incremental)
    let sync_mode = request.and_then(|r| {
        r.sync_mode.map(|m| match m.as_str() {
            "full_refresh" => crate::sources::base::SyncMode::FullRefresh,
            _ => crate::sources::base::SyncMode::incremental(None),
        })
    });

    // Use the new async job-based sync
    match crate::api::trigger_stream_sync(
        state.db.pool(),
        &*state.storage,
        state.stream_writer.clone(),
        source_id,
        &stream_name,
        sync_mode,
    )
    .await
    {
        Ok(response) => (StatusCode::CREATED, Json(response)).into_response(),
        Err(e) => {
            let status = if e.to_string().contains("already has an active sync") {
                StatusCode::CONFLICT
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            (
                status,
                Json(serde_json::json!({
                    "error": e.to_string()
                })),
            )
                .into_response()
        }
    }
}

/// Request for syncing a stream
#[derive(Debug, Deserialize)]
pub struct SyncStreamRequest {
    pub sync_mode: Option<String>,
}

// ============================================================================
// Catalog/Registry API
// ============================================================================

/// Simplified catalog source for frontend display
#[derive(Debug, Serialize)]
pub struct CatalogSource {
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub auth_type: String,
    pub stream_count: usize,
    pub icon: Option<String>,
}

/// List all available source types from the registry
pub async fn list_catalog_sources_handler() -> Response {
    let sources = crate::registry::list_sources();

    let catalog: Vec<CatalogSource> = sources
        .iter()
        .map(|s| CatalogSource {
            name: s.name.to_string(),
            display_name: s.display_name.to_string(),
            description: s.description.to_string(),
            auth_type: format!("{:?}", s.auth_type).to_lowercase(),
            stream_count: s.streams.len(),
            icon: s.icon.map(|i| i.to_string()),
        })
        .collect();

    (StatusCode::OK, Json(catalog)).into_response()
}

// ============================================================================
// Ontologies API

/// List available ontology tables
pub async fn list_available_ontologies_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::ontologies::list_available_ontologies(state.db.pool()).await)
}

/// Get ontologies overview with record counts and samples
pub async fn get_ontologies_overview_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::ontologies::get_ontologies_overview(state.db.pool()).await)
}

// ============================================================================
// Jobs API
// ============================================================================

/// Get job status by ID
pub async fn get_job_handler(State(state): State<AppState>, Path(job_id): Path<Uuid>) -> Response {
    match crate::api::get_job_status(state.db.pool(), job_id).await {
        Ok(job) => (StatusCode::OK, Json(job)).into_response(),
        Err(e) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": e.to_string()
            })),
        )
            .into_response(),
    }
}

/// Query jobs with filters
#[derive(Debug, Deserialize)]
pub struct QueryJobsParams {
    pub source_id: Option<Uuid>,
    pub status: Option<String>, // Comma-separated list
    pub limit: Option<i64>,
}

pub async fn query_jobs_handler(
    State(state): State<AppState>,
    Query(params): Query<QueryJobsParams>,
) -> Response {
    // Parse comma-separated status list
    let statuses = params.status.map(|s| {
        s.split(',')
            .map(|s| s.trim().to_string())
            .collect::<Vec<String>>()
    });

    let request = crate::api::QueryJobsRequest {
        source_id: params.source_id,
        status: statuses,
        limit: params.limit,
    };

    match crate::api::query_jobs(state.db.pool(), request).await {
        Ok(jobs) => (StatusCode::OK, Json(jobs)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": e.to_string()
            })),
        )
            .into_response(),
    }
}

/// Cancel a running job
pub async fn cancel_job_handler(
    State(state): State<AppState>,
    Path(job_id): Path<Uuid>,
) -> Response {
    match crate::api::cancel_job(state.db.pool(), job_id).await {
        Ok(_) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "message": "Job cancelled successfully"
            })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": e.to_string()
            })),
        )
            .into_response(),
    }
}

/// Get job history for a specific stream
#[derive(Debug, Deserialize)]
pub struct StreamJobsParams {
    pub limit: Option<i64>,
}

pub async fn get_stream_jobs_handler(
    State(state): State<AppState>,
    Path((source_id, stream_name)): Path<(Uuid, String)>,
    Query(params): Query<StreamJobsParams>,
) -> Response {
    let limit = params.limit.unwrap_or(10);

    match crate::api::get_job_history(state.db.pool(), source_id, &stream_name, limit).await {
        Ok(jobs) => (StatusCode::OK, Json(jobs)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": e.to_string()
            })),
        )
            .into_response(),
    }
}

// ============================================================================
// Device Pairing Endpoints
// ============================================================================

/// Request to initiate device pairing
#[derive(Debug, Deserialize)]
pub struct InitiatePairingRequest {
    pub device_type: String,
    pub name: String,
}

/// Response when pairing is initiated
#[derive(Debug, Serialize)]
pub struct InitiatePairingResponse {
    pub source_id: Uuid,
    pub code: String,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

/// Initiate device pairing by generating a pairing code
pub async fn initiate_device_pairing_handler(
    State(state): State<AppState>,
    Json(request): Json<InitiatePairingRequest>,
) -> Response {
    match crate::api::initiate_device_pairing(state.db.pool(), &request.device_type, &request.name)
        .await
    {
        Ok(pairing) => (
            StatusCode::OK,
            Json(InitiatePairingResponse {
                source_id: pairing.source_id,
                code: pairing.code,
                expires_at: pairing.expires_at,
            }),
        )
            .into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": e.to_string()
            })),
        )
            .into_response(),
    }
}

/// Request to complete device pairing
#[derive(Debug, Deserialize)]
pub struct CompletePairingRequest {
    pub code: String,
    pub device_info: crate::DeviceInfo,
}

/// Response when pairing is completed
#[derive(Debug, Serialize)]
pub struct CompletePairingResponse {
    pub source_id: Uuid,
    pub device_token: String,
    pub available_streams: Vec<crate::registry::RegisteredStream>,
}

/// Complete device pairing with a valid pairing code
pub async fn complete_device_pairing_handler(
    State(state): State<AppState>,
    Json(request): Json<CompletePairingRequest>,
) -> Response {
    match crate::api::complete_device_pairing(state.db.pool(), &request.code, request.device_info)
        .await
    {
        Ok(completed) => (
            StatusCode::OK,
            Json(CompletePairingResponse {
                source_id: completed.source_id,
                device_token: completed.device_token,
                available_streams: completed.available_streams,
            }),
        )
            .into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": e.to_string()
            })),
        )
            .into_response(),
    }
}

/// Request to manually link a device (UUID flow)
#[derive(Debug, Deserialize)]
pub struct LinkDeviceRequest {
    pub device_id: String,
    pub name: String,
    pub device_type: String,
}

/// Link a device manually using its UUID
pub async fn link_device_manual_handler(
    State(state): State<AppState>,
    Json(request): Json<LinkDeviceRequest>,
) -> Response {
    match crate::api::link_device_manually(
        state.db.pool(),
        &request.device_id,
        &request.name,
        &request.device_type,
    )
    .await
    {
        Ok(completed) => (
            StatusCode::OK,
            Json(CompletePairingResponse {
                source_id: completed.source_id,
                device_token: completed.device_token,
                available_streams: completed.available_streams,
            }),
        )
            .into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": e.to_string()
            })),
        )
            .into_response(),
    }
}

/// Response for pairing status
#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum PairingStatusResponse {
    Pending,
    Active { device_info: crate::DeviceInfo },
    Revoked,
}

/// Check the status of a device pairing
pub async fn check_pairing_status_handler(
    State(state): State<AppState>,
    Path(source_id): Path<Uuid>,
) -> Response {
    match crate::api::check_pairing_status(state.db.pool(), source_id).await {
        Ok(status) => {
            let response = match status {
                crate::PairingStatus::Pending => PairingStatusResponse::Pending,
                crate::PairingStatus::Active(info) => {
                    PairingStatusResponse::Active { device_info: info }
                }
                crate::PairingStatus::Revoked => PairingStatusResponse::Revoked,
            };
            (StatusCode::OK, Json(response)).into_response()
        }
        Err(e) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": e.to_string()
            })),
        )
            .into_response(),
    }
}

/// Response for pending pairings
#[derive(Debug, Serialize)]
pub struct PendingPairingsResponse {
    pub pairings: Vec<PendingPairingItem>,
}

#[derive(Debug, Serialize)]
pub struct PendingPairingItem {
    pub source_id: Uuid,
    pub name: String,
    pub device_type: String,
    pub code: String,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// List all pending device pairings
pub async fn list_pending_pairings_handler(State(state): State<AppState>) -> Response {
    match crate::api::list_pending_pairings(state.db.pool()).await {
        Ok(pairings) => {
            let items = pairings
                .into_iter()
                .map(|p| PendingPairingItem {
                    source_id: p.source_id,
                    name: p.name,
                    device_type: p.device_type,
                    code: p.code,
                    expires_at: p.expires_at,
                    created_at: p.created_at,
                })
                .collect();
            (
                StatusCode::OK,
                Json(PendingPairingsResponse { pairings: items }),
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": e.to_string()
            })),
        )
            .into_response(),
    }
}

/// Health check endpoint for devices to validate their authentication
///
/// This lightweight endpoint allows devices to verify their token is still valid
/// without creating any side effects. Used for startup validation and periodic health checks.
pub async fn device_health_check_handler(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
) -> Response {
    // Extract device token from Authorization header or X-Device-Token header
    let token = if let Some(value) = headers.get(axum::http::header::AUTHORIZATION) {
        let auth_str = value.to_str().unwrap_or("");
        if let Some(t) = auth_str.strip_prefix("Bearer ") {
            t.to_string()
        } else {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({
                    "error": "Invalid Authorization header format. Expected: Bearer <token>"
                })),
            )
                .into_response();
        }
    } else if let Some(value) = headers.get("X-Device-Token") {
        value.to_str().unwrap_or("").to_string()
    } else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({
                "error": "Missing authentication header. Provide Authorization: Bearer <token> or X-Device-Token: <token>"
            })),
        )
            .into_response();
    };

    // Validate the device token
    match crate::api::device_pairing::validate_device_token(state.db.pool(), &token).await {
        Ok(source_id) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "active",
                "source_id": source_id,
            })),
        )
            .into_response(),
        Err(_) => (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({
                "error": "Invalid or revoked device token"
            })),
        )
            .into_response(),
    }
}

/// Verify a device token and return configuration status
///
/// This is called by devices that already have a device_token
/// to check if streams have been configured
pub async fn verify_device_handler(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
) -> Response {
    // Extract Bearer token from Authorization header
    let token = match headers.get(axum::http::header::AUTHORIZATION) {
        Some(value) => {
            let auth_str = value.to_str().unwrap_or("");
            if let Some(token) = auth_str.strip_prefix("Bearer ") {
                token
            } else {
                return (
                    StatusCode::UNAUTHORIZED,
                    Json(serde_json::json!({
                        "error": "Missing or invalid Authorization header. Expected: Bearer <token>"
                    })),
                )
                    .into_response();
            }
        }
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({
                    "error": "Missing Authorization header"
                })),
            )
                .into_response();
        }
    };

    match crate::api::device_pairing::verify_device(state.db.pool(), token).await {
        Ok(verified) => {
            let response = serde_json::json!({
                "source_id": verified.source_id,
                "configuration_complete": verified.configuration_complete,
                "enabled_streams": verified.enabled_streams.iter().map(|s| {
                    serde_json::json!({
                        "stream_name": s.stream_name,
                        "display_name": s.display_name,
                        "description": s.description,
                        "is_enabled": s.is_enabled,
                        "config": s.config,
                        "supports_incremental": s.supports_incremental,
                        "default_cron_schedule": s.default_cron_schedule,
                    })
                }).collect::<Vec<_>>(),
            });
            (StatusCode::OK, Json(response)).into_response()
        }
        Err(e) => {
            let status = match e {
                crate::Error::Unauthorized(_) => StatusCode::UNAUTHORIZED,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            (
                status,
                Json(serde_json::json!({
                    "error": e.to_string()
                })),
            )
                .into_response()
        }
    }
}

// =============================================================================
// Profile API
// =============================================================================

/// Get user profile
pub async fn get_profile_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::get_profile(state.db.pool()).await)
}

/// Update user profile
pub async fn update_profile_handler(
    State(state): State<AppState>,
    Json(request): Json<crate::api::UpdateProfileRequest>,
) -> Response {
    api_response(crate::api::update_profile(state.db.pool(), request).await)
}

// =============================================================================
// Assistant Profile API
// =============================================================================

/// Get assistant profile
pub async fn get_assistant_profile_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::get_assistant_profile(state.db.pool()).await)
}

/// Update assistant profile
pub async fn update_assistant_profile_handler(
    State(state): State<AppState>,
    Json(request): Json<crate::api::UpdateAssistantProfileRequest>,
) -> Response {
    api_response(crate::api::update_assistant_profile(state.db.pool(), request).await)
}

// =============================================================================
// Axiology API - Tasks
// =============================================================================

/// List all active tasks
pub async fn list_tasks_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::list_tasks(state.db.pool()).await)
}

/// Get a specific task by ID
pub async fn get_task_handler(
    State(state): State<AppState>,
    Path(task_id): Path<Uuid>,
) -> Response {
    api_response(crate::api::get_task(state.db.pool(), task_id).await)
}

/// Create a new task
pub async fn create_task_handler(
    State(state): State<AppState>,
    Json(request): Json<crate::api::CreateTaskRequest>,
) -> Response {
    api_response(crate::api::create_task(state.db.pool(), request).await)
}

/// Update an existing task
pub async fn update_task_handler(
    State(state): State<AppState>,
    Path(task_id): Path<Uuid>,
    Json(request): Json<crate::api::UpdateTaskRequest>,
) -> Response {
    api_response(crate::api::update_task(state.db.pool(), task_id, request).await)
}

/// Delete a task (soft delete)
pub async fn delete_task_handler(
    State(state): State<AppState>,
    Path(task_id): Path<Uuid>,
) -> Response {
    match crate::api::delete_task(state.db.pool(), task_id).await {
        Ok(_) => success_message("Task deleted successfully"),
        Err(e) => error_response(e),
    }
}

/// List all distinct tags across temporal pursuits
pub async fn list_tags_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::list_tags(state.db.pool()).await)
}

// =============================================================================
// Actions API - Initiatives
// =============================================================================

/// List all active initiatives
pub async fn list_initiatives_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::list_initiatives(state.db.pool()).await)
}

/// Get a specific initiative by ID
pub async fn get_initiative_handler(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Response {
    api_response(crate::api::get_initiative(state.db.pool(), id).await)
}

/// Create a new initiative
pub async fn create_initiative_handler(
    State(state): State<AppState>,
    Json(request): Json<crate::api::CreateTaskRequest>,
) -> Response {
    api_response(crate::api::create_initiative(state.db.pool(), request).await)
}

/// Update an existing initiative
pub async fn update_initiative_handler(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(request): Json<crate::api::UpdateTaskRequest>,
) -> Response {
    api_response(crate::api::update_initiative(state.db.pool(), id, request).await)
}

/// Delete an initiative (soft delete)
pub async fn delete_initiative_handler(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Response {
    match crate::api::delete_initiative(state.db.pool(), id).await {
        Ok(_) => success_message("Initiative deleted successfully"),
        Err(e) => error_response(e),
    }
}

// =============================================================================
// Actions API - Aspirations
// =============================================================================

/// List all active aspirations
pub async fn list_aspirations_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::list_aspirations(state.db.pool()).await)
}

/// Get a specific aspiration by ID
pub async fn get_aspiration_handler(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Response {
    api_response(crate::api::get_aspiration(state.db.pool(), id).await)
}

/// Create a new aspiration
pub async fn create_aspiration_handler(
    State(state): State<AppState>,
    Json(request): Json<crate::api::CreateAspirationRequest>,
) -> Response {
    api_response(crate::api::create_aspiration(state.db.pool(), request).await)
}

/// Update an existing aspiration
pub async fn update_aspiration_handler(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(request): Json<crate::api::UpdateAspirationRequest>,
) -> Response {
    api_response(crate::api::update_aspiration(state.db.pool(), id, request).await)
}

/// Delete an aspiration (soft delete)
pub async fn delete_aspiration_handler(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Response {
    match crate::api::delete_aspiration(state.db.pool(), id).await {
        Ok(_) => success_message("Aspiration deleted successfully"),
        Err(e) => error_response(e),
    }
}

// =============================================================================
// Tools API
// =============================================================================

/// List all tools with optional filtering
pub async fn list_tools_handler(
    State(state): State<AppState>,
    Query(query): Query<crate::api::ListToolsQuery>,
) -> Response {
    api_response(crate::api::list_tools(state.db.pool(), query).await)
}

/// Get a specific tool by ID
pub async fn get_tool_handler(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    api_response(crate::api::get_tool(state.db.pool(), id).await)
}

/// Update a tool's metadata
pub async fn update_tool_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<crate::api::UpdateToolRequest>,
) -> Response {
    api_response(crate::api::update_tool(state.db.pool(), id, request).await)
}

// =============================================================================
// Models API
// =============================================================================

/// List all available models
pub async fn list_models_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::list_models(state.db.pool()).await)
}

/// Get a specific model by ID
pub async fn get_model_handler(
    State(state): State<AppState>,
    Path(model_id): Path<String>,
) -> Response {
    api_response(crate::api::get_model(state.db.pool(), &model_id).await)
}

// =============================================================================
// Agents API
// =============================================================================

/// List all available agents
pub async fn list_agents_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::list_agents(state.db.pool()).await)
}

/// Get a specific agent by ID
pub async fn get_agent_handler(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
) -> Response {
    api_response(crate::api::get_agent(state.db.pool(), &agent_id).await)
}

// =============================================================================
// Seed Testing API
// =============================================================================

/// Get pipeline status (archive, transform, clustering)
pub async fn seed_pipeline_status_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::get_pipeline_status(&state.db).await)
}

#[derive(Debug, serde::Deserialize)]
pub struct DataQualityQuery {
    pub start: String, // RFC3339 timestamp
    pub end: String,   // RFC3339 timestamp
}

/// Get data quality metrics for seed data
pub async fn seed_data_quality_handler(
    State(state): State<AppState>,
    Query(params): Query<DataQualityQuery>,
) -> Response {
    let start = match chrono::DateTime::parse_from_rfc3339(&params.start) {
        Ok(dt) => dt.with_timezone(&chrono::Utc),
        Err(_) => {
            return error_response(crate::error::Error::InvalidInput(
                "Invalid start timestamp. Use RFC3339 format".to_string(),
            ))
        }
    };

    let end = match chrono::DateTime::parse_from_rfc3339(&params.end) {
        Ok(dt) => dt.with_timezone(&chrono::Utc),
        Err(_) => {
            return error_response(crate::error::Error::InvalidInput(
                "Invalid end timestamp. Use RFC3339 format".to_string(),
            ))
        }
    };

    api_response(crate::api::get_data_quality_metrics(&state.db, start, end).await)
}

// ============================================================================
// Embedding handlers
// ============================================================================

/// Get embedding statistics
pub async fn get_embedding_stats_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::search::get_embedding_stats(state.db.pool()).await)
}

/// Trigger embedding job manually
pub async fn trigger_embedding_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::search::trigger_embedding_job(state.db.pool()).await)
}

// ============================================================================
// Metrics handlers
// ============================================================================

/// Get activity metrics (job statistics, time windows, recent errors)
pub async fn get_activity_metrics_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::get_activity_metrics(&state.db).await)
}

// ============================================================================
// Plaid Link API
// ============================================================================

/// Create a Plaid Link token for initializing Plaid Link
///
/// Returns a link_token that the frontend uses to initialize Plaid Link SDK.
pub async fn create_plaid_link_token_handler(
    State(state): State<AppState>,
    Json(request): Json<crate::api::CreateLinkTokenRequest>,
) -> Response {
    api_response(crate::api::create_link_token(state.db.pool(), request).await)
}

/// Exchange a public token for an access token
///
/// Called after the user completes the Plaid Link flow.
pub async fn exchange_plaid_token_handler(
    State(state): State<AppState>,
    Json(request): Json<crate::api::ExchangeTokenRequest>,
) -> Response {
    match crate::api::exchange_public_token(state.db.pool(), request).await {
        Ok(response) => (StatusCode::CREATED, Json(response)).into_response(),
        Err(e) => error_response(e),
    }
}

/// Get accounts for an existing Plaid connection
pub async fn get_plaid_accounts_handler(
    State(state): State<AppState>,
    Path(source_id): Path<Uuid>,
) -> Response {
    api_response(crate::api::get_plaid_accounts(state.db.pool(), source_id).await)
}

/// Remove a Plaid Item (disconnect bank account)
pub async fn remove_plaid_item_handler(
    State(state): State<AppState>,
    Path(source_id): Path<Uuid>,
) -> Response {
    match crate::api::remove_plaid_item(state.db.pool(), source_id).await {
        Ok(_) => success_message("Plaid item removed successfully"),
        Err(e) => error_response(e),
    }
}

// ============================================================================
// Onboarding API
// ============================================================================

/// Save axiology items from onboarding review
///
/// Bulk creates telos, virtues, vices, temperaments, and preferences
pub async fn save_onboarding_axiology_handler(
    State(state): State<AppState>,
    Json(request): Json<crate::api::SaveAxiologyRequest>,
) -> Response {
    match crate::api::save_onboarding_axiology(state.db.pool(), request).await {
        Ok(response) => (StatusCode::CREATED, Json(response)).into_response(),
        Err(e) => error_response(e),
    }
}

/// Save aspirations from onboarding
pub async fn save_onboarding_aspirations_handler(
    State(state): State<AppState>,
    Json(request): Json<crate::api::SaveAspirationsRequest>,
) -> Response {
    match crate::api::save_onboarding_aspirations(state.db.pool(), request).await {
        Ok(response) => (StatusCode::CREATED, Json(response)).into_response(),
        Err(e) => error_response(e),
    }
}

/// Mark onboarding as complete
pub async fn complete_onboarding_handler(State(state): State<AppState>) -> Response {
    match crate::api::complete_onboarding(state.db.pool()).await {
        Ok(_) => success_message("Onboarding completed"),
        Err(e) => error_response(e),
    }
}

/// Get onboarding status with granular completion tracking
pub async fn get_onboarding_status_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::get_onboarding_status(state.db.pool()).await)
}

/// Complete a specific onboarding step
pub async fn complete_onboarding_step_handler(
    State(state): State<AppState>,
    Path(step): Path<String>,
) -> Response {
    let step = match crate::api::OnboardingStep::from_str(&step) {
        Some(s) => s,
        None => {
            return error_response(crate::error::Error::InvalidInput(format!(
                "Invalid step: {}. Valid steps: profile, places, tools, axiology",
                step
            )))
        }
    };

    api_response(crate::api::complete_step(state.db.pool(), step).await)
}

/// Skip a specific onboarding step
pub async fn skip_onboarding_step_handler(
    State(state): State<AppState>,
    Path(step): Path<String>,
) -> Response {
    let step = match crate::api::OnboardingStep::from_str(&step) {
        Some(s) => s,
        None => {
            return error_response(crate::error::Error::InvalidInput(format!(
                "Invalid step: {}. Valid steps: profile, places, tools, axiology",
                step
            )))
        }
    };

    api_response(crate::api::skip_step(state.db.pool(), step).await)
}

// =============================================================================
// Places API Handlers (Google Places proxy)
// =============================================================================

/// Get autocomplete predictions for an address query
pub async fn places_autocomplete_handler(
    State(state): State<AppState>,
    Query(request): Query<crate::api::AutocompleteRequest>,
) -> Response {
    // Check usage limit first
    if let Err(e) =
        crate::api::check_limit(state.db.pool(), crate::api::Service::GooglePlaces).await
    {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(serde_json::json!({
                "error": "usage_limit_exceeded",
                "service": e.service,
                "used": e.used,
                "limit": e.limit,
                "unit": e.unit,
                "resets_at": e.resets_at,
                "message": format!("Monthly Google Places limit reached. Resets at {}", e.resets_at)
            })),
        )
            .into_response();
    }

    match crate::api::autocomplete(request).await {
        Ok(response) => {
            // Record usage on success - warn but don't fail if recording fails
            // The user already received their response, so this is a billing/tracking issue only
            if let Err(e) = crate::api::record_service_usage(
                state.db.pool(),
                crate::api::Service::GooglePlaces,
                1,
            )
            .await
            {
                tracing::warn!(
                    service = "google_places",
                    error = %e,
                    "Usage recording failed - request succeeded but usage may be undercounted"
                );
            }
            (StatusCode::OK, Json(response)).into_response()
        }
        Err(e) => error_response(e),
    }
}

/// Get details for a specific place by ID
pub async fn places_details_handler(
    State(state): State<AppState>,
    Query(request): Query<crate::api::PlaceDetailsRequest>,
) -> Response {
    // Check usage limit first
    if let Err(e) =
        crate::api::check_limit(state.db.pool(), crate::api::Service::GooglePlaces).await
    {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(serde_json::json!({
                "error": "usage_limit_exceeded",
                "service": e.service,
                "used": e.used,
                "limit": e.limit,
                "unit": e.unit,
                "resets_at": e.resets_at,
                "message": format!("Monthly Google Places limit reached. Resets at {}", e.resets_at)
            })),
        )
            .into_response();
    }

    match crate::api::get_place_details(request).await {
        Ok(response) => {
            // Record usage on success - warn but don't fail if recording fails
            if let Err(e) = crate::api::record_service_usage(
                state.db.pool(),
                crate::api::Service::GooglePlaces,
                1,
            )
            .await
            {
                tracing::warn!(
                    service = "google_places",
                    error = %e,
                    "Usage recording failed - request succeeded but usage may be undercounted"
                );
            }
            (StatusCode::OK, Json(response)).into_response()
        }
        Err(e) => error_response(e),
    }
}

// =============================================================================
// Usage API Handlers
// =============================================================================

/// Get usage summary for all services
pub async fn usage_handler(State(state): State<AppState>) -> Response {
    match crate::api::get_all_usage(state.db.pool()).await {
        Ok(summary) => (StatusCode::OK, Json(summary)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": format!("Failed to get usage: {}", e) })),
        )
            .into_response(),
    }
}

/// Check remaining usage for a service
#[derive(Debug, Deserialize)]
pub struct UsageCheckQuery {
    pub service: String,
}

pub async fn usage_check_handler(
    State(state): State<AppState>,
    Query(query): Query<UsageCheckQuery>,
) -> Response {
    let service = match query.service.as_str() {
        "ai_gateway" => crate::api::Service::AiGateway,
        "google_places" => crate::api::Service::GooglePlaces,
        "exa" => crate::api::Service::Exa,
        _ => {
            return error_response(crate::error::Error::InvalidInput(format!(
                "Invalid service: {}. Valid services: ai_gateway, google_places, exa",
                query.service
            )))
        }
    };

    match crate::api::check_limit(state.db.pool(), service).await {
        Ok(remaining) => (StatusCode::OK, Json(remaining)).into_response(),
        Err(e) => (
            StatusCode::TOO_MANY_REQUESTS,
            Json(serde_json::json!({
                "error": "usage_limit_exceeded",
                "service": e.service,
                "used": e.used,
                "limit": e.limit,
                "unit": e.unit,
                "resets_at": e.resets_at,
                "message": format!("Monthly {} limit reached. Resets at {}", e.service, e.resets_at)
            })),
        )
            .into_response(),
    }
}

// =============================================================================
// Exa Search API Handlers
// =============================================================================

/// Perform a web search using Exa AI
pub async fn exa_search_handler(
    State(state): State<AppState>,
    Json(request): Json<crate::api::ExaSearchRequest>,
) -> Response {
    // Check usage limit first
    if let Err(e) = crate::api::check_limit(state.db.pool(), crate::api::Service::Exa).await {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(serde_json::json!({
                "error": "usage_limit_exceeded",
                "service": e.service,
                "used": e.used,
                "limit": e.limit,
                "unit": e.unit,
                "resets_at": e.resets_at,
                "message": format!("Monthly Exa search limit reached. Resets at {}", e.resets_at)
            })),
        )
            .into_response();
    }

    // Perform the search
    match crate::api::exa_search(request).await {
        Ok(response) => {
            // Record usage on success - warn but don't fail if recording fails
            if let Err(e) =
                crate::api::record_service_usage(state.db.pool(), crate::api::Service::Exa, 1).await
            {
                tracing::warn!(
                    service = "exa",
                    error = %e,
                    "Usage recording failed - request succeeded but usage may be undercounted"
                );
            }
            (StatusCode::OK, Json(response)).into_response()
        }
        Err(e) => error_response(e),
    }
}

// =============================================================================
// Storage API Handlers
// =============================================================================

/// Query parameters for listing storage objects
#[derive(Debug, Deserialize)]
pub struct ListStorageObjectsParams {
    pub limit: Option<i64>,
}

/// List recent storage objects
pub async fn list_storage_objects_handler(
    State(state): State<AppState>,
    Query(params): Query<ListStorageObjectsParams>,
) -> Response {
    let limit = params.limit.unwrap_or(10);
    api_response(crate::api::list_recent_objects(state.db.pool(), limit).await)
}

/// Get decrypted content of a storage object
pub async fn get_storage_object_content_handler(
    State(state): State<AppState>,
    Path(object_id): Path<Uuid>,
) -> Response {
    api_response(crate::api::get_object_content(state.db.pool(), &*state.storage, object_id).await)
}

// =============================================================================
// Entities API - Places
// =============================================================================

/// List all known places
pub async fn list_places_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::list_places(state.db.pool()).await)
}

/// Get a specific place by ID
pub async fn get_place_handler(
    State(state): State<AppState>,
    Path(place_id): Path<Uuid>,
) -> Response {
    api_response(crate::api::get_place(state.db.pool(), place_id).await)
}

/// Create a new place
pub async fn create_place_handler(
    State(state): State<AppState>,
    Json(request): Json<crate::api::CreatePlaceRequest>,
) -> Response {
    match crate::api::create_place(state.db.pool(), request).await {
        Ok(response) => (StatusCode::CREATED, Json(response)).into_response(),
        Err(e) => error_response(e),
    }
}

/// Update an existing place
pub async fn update_place_handler(
    State(state): State<AppState>,
    Path(place_id): Path<Uuid>,
    Json(request): Json<crate::api::UpdatePlaceRequest>,
) -> Response {
    api_response(crate::api::update_place(state.db.pool(), place_id, request).await)
}

/// Delete a place
pub async fn delete_place_handler(
    State(state): State<AppState>,
    Path(place_id): Path<Uuid>,
) -> Response {
    match crate::api::delete_place(state.db.pool(), place_id).await {
        Ok(_) => success_message("Place deleted successfully"),
        Err(e) => error_response(e),
    }
}

/// Set a place as the user's home
pub async fn set_place_as_home_handler(
    State(state): State<AppState>,
    Path(place_id): Path<Uuid>,
) -> Response {
    match crate::api::set_home_place_entity(state.db.pool(), place_id).await {
        Ok(_) => success_message("Home place updated"),
        Err(e) => error_response(e),
    }
}

// ============================================================================
// Wiki API Handlers
// ============================================================================

/// Resolve a slug to its entity type
pub async fn wiki_resolve_slug_handler(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Response {
    api_response(crate::api::resolve_slug(state.db.pool(), &slug).await)
}

// --- Person ---

/// Get a person by slug
pub async fn wiki_get_person_handler(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Response {
    api_response(crate::api::get_person_by_slug(state.db.pool(), &slug).await)
}

/// List all people
pub async fn wiki_list_people_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::list_people(state.db.pool()).await)
}

/// Update a person by ID
pub async fn wiki_update_person_handler(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(request): Json<crate::api::UpdateWikiPersonRequest>,
) -> Response {
    api_response(crate::api::update_person(state.db.pool(), id, request).await)
}

// --- Place ---

/// Get a place by slug
pub async fn wiki_get_place_handler(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Response {
    api_response(crate::api::get_place_by_slug(state.db.pool(), &slug).await)
}

/// List all places (wiki view)
pub async fn wiki_list_places_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::list_wiki_places(state.db.pool()).await)
}

/// Update a place by ID (wiki fields)
pub async fn wiki_update_place_handler(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(request): Json<crate::api::UpdateWikiPlaceRequest>,
) -> Response {
    api_response(crate::api::update_wiki_place(state.db.pool(), id, request).await)
}

// --- Organization ---

/// Get an organization by slug
pub async fn wiki_get_organization_handler(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Response {
    api_response(crate::api::get_organization_by_slug(state.db.pool(), &slug).await)
}

/// List all organizations
pub async fn wiki_list_organizations_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::list_organizations(state.db.pool()).await)
}

/// Update an organization by ID
pub async fn wiki_update_organization_handler(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(request): Json<crate::api::UpdateWikiOrganizationRequest>,
) -> Response {
    api_response(crate::api::update_organization(state.db.pool(), id, request).await)
}

// --- Thing ---

/// Get a thing by slug
pub async fn wiki_get_thing_handler(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Response {
    api_response(crate::api::get_thing_by_slug(state.db.pool(), &slug).await)
}

/// List all things
pub async fn wiki_list_things_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::list_things(state.db.pool()).await)
}

/// Update a thing by ID
pub async fn wiki_update_thing_handler(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(request): Json<crate::api::UpdateWikiThingRequest>,
) -> Response {
    api_response(crate::api::update_thing(state.db.pool(), id, request).await)
}

// --- Telos ---

/// Get active telos
pub async fn wiki_get_active_telos_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::get_active_telos(state.db.pool()).await)
}

/// Get a telos by slug
pub async fn wiki_get_telos_handler(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Response {
    api_response(crate::api::get_telos_by_slug(state.db.pool(), &slug).await)
}

// --- Act ---

/// Get an act by slug
pub async fn wiki_get_act_handler(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Response {
    api_response(crate::api::get_act_by_slug(state.db.pool(), &slug).await)
}

/// List all acts
pub async fn wiki_list_acts_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::list_acts(state.db.pool()).await)
}

// --- Chapter ---

/// Get a chapter by slug
pub async fn wiki_get_chapter_handler(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Response {
    api_response(crate::api::get_chapter_by_slug(state.db.pool(), &slug).await)
}

/// List chapters for an act
pub async fn wiki_list_chapters_handler(
    State(state): State<AppState>,
    Path(act_id): Path<Uuid>,
) -> Response {
    api_response(crate::api::list_chapters_for_act(state.db.pool(), act_id).await)
}

// --- Day ---

#[derive(Deserialize)]
pub struct WikiDayQuery {
    pub start_date: Option<chrono::NaiveDate>,
    pub end_date: Option<chrono::NaiveDate>,
}

/// Get a day by date
pub async fn wiki_get_day_handler(
    State(state): State<AppState>,
    Path(date): Path<String>,
) -> Response {
    match date.parse::<chrono::NaiveDate>() {
        Ok(parsed_date) => {
            api_response(crate::api::get_or_create_day(state.db.pool(), parsed_date).await)
        }
        Err(_) => error_response(Error::InvalidInput(format!(
            "Invalid date format: {}",
            date
        ))),
    }
}

/// Update a day by date
pub async fn wiki_update_day_handler(
    State(state): State<AppState>,
    Path(date): Path<String>,
    Json(request): Json<crate::api::UpdateWikiDayRequest>,
) -> Response {
    match date.parse::<chrono::NaiveDate>() {
        Ok(parsed_date) => {
            api_response(crate::api::update_day(state.db.pool(), parsed_date, request).await)
        }
        Err(_) => error_response(Error::InvalidInput(format!(
            "Invalid date format: {}",
            date
        ))),
    }
}

/// List days in a date range
pub async fn wiki_list_days_handler(
    State(state): State<AppState>,
    Query(query): Query<WikiDayQuery>,
) -> Response {
    let today = chrono::Utc::now().date_naive();
    let start_date = query
        .start_date
        .unwrap_or(today - chrono::Duration::days(30));
    let end_date = query.end_date.unwrap_or(today);
    api_response(crate::api::list_days(state.db.pool(), start_date, end_date).await)
}

// =============================================================================
// Wiki Citations API
// =============================================================================

/// Get citations for a wiki page
pub async fn wiki_get_citations_handler(
    State(state): State<AppState>,
    Path((source_type, source_id)): Path<(String, Uuid)>,
) -> Response {
    api_response(crate::api::get_citations(state.db.pool(), &source_type, source_id).await)
}

/// Create a citation for a wiki page
pub async fn wiki_create_citation_handler(
    State(state): State<AppState>,
    Path((source_type, source_id)): Path<(String, Uuid)>,
    Json(mut request): Json<crate::api::CreateCitationRequest>,
) -> Response {
    // Set source type/id from path
    request.source_type = source_type;
    request.source_id = source_id;
    match crate::api::create_citation(state.db.pool(), request).await {
        Ok(citation) => (StatusCode::CREATED, Json(citation)).into_response(),
        Err(e) => error_response(e),
    }
}

/// Update a citation
pub async fn wiki_update_citation_handler(
    State(state): State<AppState>,
    Path(citation_id): Path<Uuid>,
    Json(request): Json<crate::api::UpdateCitationRequest>,
) -> Response {
    api_response(crate::api::update_citation(state.db.pool(), citation_id, request).await)
}

/// Delete a citation
pub async fn wiki_delete_citation_handler(
    State(state): State<AppState>,
    Path(citation_id): Path<Uuid>,
) -> Response {
    match crate::api::delete_citation(state.db.pool(), citation_id).await {
        Ok(_) => success_message("Citation deleted"),
        Err(e) => error_response(e),
    }
}

// =============================================================================
// Wiki Temporal Events API
// =============================================================================

/// Get events for a day by date
pub async fn wiki_get_day_events_handler(
    State(state): State<AppState>,
    Path(date): Path<String>,
) -> Response {
    match date.parse::<chrono::NaiveDate>() {
        Ok(parsed_date) => {
            api_response(crate::api::get_events_by_date(state.db.pool(), parsed_date).await)
        }
        Err(_) => error_response(Error::InvalidInput(format!(
            "Invalid date format: {}",
            date
        ))),
    }
}

/// Create a temporal event
pub async fn wiki_create_event_handler(
    State(state): State<AppState>,
    Json(request): Json<crate::api::CreateTemporalEventRequest>,
) -> Response {
    match crate::api::create_temporal_event(state.db.pool(), request).await {
        Ok(event) => (StatusCode::CREATED, Json(event)).into_response(),
        Err(e) => error_response(e),
    }
}

/// Update a temporal event
pub async fn wiki_update_event_handler(
    State(state): State<AppState>,
    Path(event_id): Path<Uuid>,
    Json(request): Json<crate::api::UpdateTemporalEventRequest>,
) -> Response {
    api_response(crate::api::update_temporal_event(state.db.pool(), event_id, request).await)
}

/// Delete a temporal event
pub async fn wiki_delete_event_handler(
    State(state): State<AppState>,
    Path(event_id): Path<Uuid>,
) -> Response {
    match crate::api::delete_temporal_event(state.db.pool(), event_id).await {
        Ok(_) => success_message("Event deleted"),
        Err(e) => error_response(e),
    }
}

/// Delete all auto-generated events for a day (regeneration support)
pub async fn wiki_delete_auto_events_handler(
    State(state): State<AppState>,
    Path(day_id): Path<Uuid>,
) -> Response {
    match crate::api::delete_auto_events_for_day(state.db.pool(), day_id).await {
        Ok(count) => (
            StatusCode::OK,
            Json(serde_json::json!({ "deleted": count })),
        )
            .into_response(),
        Err(e) => error_response(e),
    }
}

/// Get data sources (ontology records) for a day
pub async fn wiki_get_day_sources_handler(
    State(state): State<AppState>,
    Path(date): Path<String>,
) -> Response {
    match date.parse::<chrono::NaiveDate>() {
        Ok(parsed_date) => {
            api_response(crate::api::get_day_sources(state.db.pool(), parsed_date).await)
        }
        Err(_) => error_response(Error::InvalidInput(format!(
            "Invalid date format: {}",
            date
        ))),
    }
}

/// Get all ontology data streams for a day (dynamic query across all ontologies)
pub async fn wiki_get_day_streams_handler(
    State(state): State<AppState>,
    Path(date): Path<String>,
) -> Response {
    match date.parse::<chrono::NaiveDate>() {
        Ok(parsed_date) => {
            api_response(crate::api::get_day_streams(state.db.pool(), parsed_date).await)
        }
        Err(_) => error_response(Error::InvalidInput(format!(
            "Invalid date format: {}",
            date
        ))),
    }
}

// =============================================================================
// Code Execution API (AI Sandbox)
// =============================================================================

/// Execute Python code in a sandboxed environment
///
/// Used by the AI agent's code_interpreter tool.
/// On Linux, uses nsjail for process isolation.
/// On dev machines (macOS/Windows), runs Python directly.
pub async fn execute_code_handler(Json(request): Json<crate::api::ExecuteCodeRequest>) -> Response {
    let response = crate::api::execute_code(request).await;
    (StatusCode::OK, Json(response)).into_response()
}

// =============================================================================
// Bookmarks API Handlers
// =============================================================================

/// List all bookmarks
pub async fn list_bookmarks_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::list_bookmarks(state.db.pool()).await)
}

/// Create a tab bookmark
pub async fn create_tab_bookmark_handler(
    State(state): State<AppState>,
    Json(request): Json<crate::api::CreateTabBookmarkRequest>,
) -> Response {
    match crate::api::create_tab_bookmark(state.db.pool(), request).await {
        Ok(bookmark) => (StatusCode::CREATED, Json(bookmark)).into_response(),
        Err(e) => error_response(e),
    }
}

/// Create an entity bookmark
pub async fn create_entity_bookmark_handler(
    State(state): State<AppState>,
    Json(request): Json<crate::api::CreateEntityBookmarkRequest>,
) -> Response {
    match crate::api::create_entity_bookmark(state.db.pool(), request).await {
        Ok(bookmark) => (StatusCode::CREATED, Json(bookmark)).into_response(),
        Err(e) => error_response(e),
    }
}

/// Delete a bookmark by ID
pub async fn delete_bookmark_handler(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Response {
    match crate::api::delete_bookmark(state.db.pool(), id).await {
        Ok(_) => success_message("Bookmark deleted"),
        Err(e) => error_response(e),
    }
}

/// Toggle bookmark for a route (create or delete)
pub async fn toggle_route_bookmark_handler(
    State(state): State<AppState>,
    Json(request): Json<crate::api::CreateTabBookmarkRequest>,
) -> Response {
    api_response(crate::api::toggle_route_bookmark(state.db.pool(), request).await)
}

/// Toggle bookmark for an entity (create or delete)
pub async fn toggle_entity_bookmark_handler(
    State(state): State<AppState>,
    Json(request): Json<crate::api::CreateEntityBookmarkRequest>,
) -> Response {
    api_response(crate::api::toggle_entity_bookmark(state.db.pool(), request).await)
}

/// Query params for checking route bookmark status
#[derive(Debug, Deserialize)]
pub struct RouteBookmarkQuery {
    pub route: String,
}

/// Check if a route is bookmarked
pub async fn check_route_bookmark_handler(
    State(state): State<AppState>,
    Query(params): Query<RouteBookmarkQuery>,
) -> Response {
    api_response(crate::api::is_route_bookmarked(state.db.pool(), &params.route).await)
}

/// Check if an entity is bookmarked
pub async fn check_entity_bookmark_handler(
    State(state): State<AppState>,
    Path(entity_id): Path<Uuid>,
) -> Response {
    api_response(crate::api::is_entity_bookmarked(state.db.pool(), entity_id).await)
}

// =============================================================================
// Session Usage & Compaction API Handlers
// =============================================================================

/// Get token usage for a session
pub async fn get_session_usage_handler(
    State(state): State<AppState>,
    Path(session_id): Path<Uuid>,
) -> Response {
    api_response(crate::api::get_session_usage(state.db.pool(), session_id).await)
}

/// Compact a session (summarize older messages)
pub async fn compact_session_handler(
    State(state): State<AppState>,
    Path(session_id): Path<Uuid>,
    Json(request): Json<Option<CompactSessionRequest>>,
) -> Response {
    let options = request.unwrap_or_default().into();
    api_response(crate::api::compaction::compact_session(state.db.pool(), session_id, options).await)
}

/// Request body for compaction
#[derive(Debug, Deserialize, Default)]
pub struct CompactSessionRequest {
    /// Number of recent exchanges to keep verbatim (default: 8)
    pub keep_recent_exchanges: Option<usize>,
    /// Force compaction even if under threshold
    #[serde(default)]
    pub force: bool,
}

impl From<CompactSessionRequest> for crate::api::compaction::CompactionOptions {
    fn from(req: CompactSessionRequest) -> Self {
        Self {
            keep_recent_exchanges: req.keep_recent_exchanges.unwrap_or(8),
            force: req.force,
        }
    }
}

// =============================================================================
// Sessions API Handlers
// =============================================================================

/// List chat sessions
pub async fn list_sessions_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::sessions::list_sessions(state.db.pool(), 25).await)
}

/// Create a new chat session with initial messages
pub async fn create_session_handler(
    State(state): State<AppState>,
    Json(request): Json<crate::api::sessions::CreateSessionRequest>,
) -> Response {
    api_response(crate::api::sessions::create_session_from_request(state.db.pool(), request).await)
}

/// Get a chat session by ID
pub async fn get_session_handler(
    State(state): State<AppState>,
    Path(session_id): Path<Uuid>,
) -> Response {
    api_response(crate::api::sessions::get_session(state.db.pool(), session_id).await)
}

/// Update a chat session title
pub async fn update_session_handler(
    State(state): State<AppState>,
    Path(session_id): Path<Uuid>,
    Json(request): Json<crate::api::sessions::UpdateTitleRequest>,
) -> Response {
    api_response(
        crate::api::sessions::update_session_title(state.db.pool(), session_id, &request.title)
            .await,
    )
}

/// Delete a chat session
pub async fn delete_session_handler(
    State(state): State<AppState>,
    Path(session_id): Path<Uuid>,
) -> Response {
    api_response(crate::api::sessions::delete_session(state.db.pool(), session_id).await)
}

/// Generate a title for a chat session
pub async fn generate_session_title_handler(
    State(state): State<AppState>,
    Json(request): Json<crate::api::sessions::GenerateTitleRequest>,
) -> Response {
    api_response(
        crate::api::sessions::generate_title(
            state.db.pool(),
            request.session_id,
            &request.messages,
        )
        .await,
    )
}

// =============================================================================
// Chat API Handler
// =============================================================================

/// POST /api/chat - Stream chat completion (requires authentication)
pub async fn chat_handler(
    State(state): State<AppState>,
    user: crate::middleware::auth::AuthUser,
    Json(request): Json<crate::api::chat::ChatRequest>,
) -> Response {
    crate::api::chat::chat_handler(
        axum::extract::State(state.db.pool().clone()),
        user,
        Json(request),
    )
    .await
}

// =============================================================================
// Auth API Handlers
// =============================================================================

/// POST /auth/signin - Send magic link email
pub async fn auth_signin_handler(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(request): Json<crate::api::auth::SignInRequest>,
) -> Response {
    crate::api::auth::signin_handler(
        axum::extract::State(state.db.pool().clone()),
        headers,
        Json(request),
    )
    .await
    .into_response()
}

/// GET /auth/callback - Verify magic link token
pub async fn auth_callback_handler(
    State(state): State<AppState>,
    Query(params): Query<crate::api::auth::CallbackParams>,
    jar: axum_extra::extract::cookie::CookieJar,
) -> Response {
    crate::api::auth::callback_handler(
        axum::extract::State(state.db.pool().clone()),
        Query(params),
        jar,
    )
    .await
    .into_response()
}

/// POST /auth/signout - Sign out and clear session
pub async fn auth_signout_handler(
    State(state): State<AppState>,
    jar: axum_extra::extract::cookie::CookieJar,
) -> Response {
    crate::api::auth::signout_handler(axum::extract::State(state.db.pool().clone()), jar)
        .await
        .into_response()
}

/// GET /auth/session - Get current session
pub async fn auth_session_handler(
    State(state): State<AppState>,
    jar: axum_extra::extract::cookie::CookieJar,
) -> Response {
    crate::api::auth::session_handler(axum::extract::State(state.db.pool().clone()), jar)
        .await
        .into_response()
}

/// POST /api/profile/owner-email - Atlas webhook to update owner email
pub async fn auth_owner_email_handler(
    State(state): State<AppState>,
    Json(request): Json<crate::api::auth::UpdateOwnerEmailRequest>,
) -> Response {
    crate::api::auth::update_owner_email_handler(
        axum::extract::State(state.db.pool().clone()),
        Json(request),
    )
    .await
    .into_response()
}

// =============================================================================
// Drive API Handlers (User File Storage)
// =============================================================================

/// GET /api/drive/usage - Get drive usage statistics
pub async fn get_drive_usage_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::get_drive_usage(state.db.pool()).await)
}

/// GET /api/drive/warnings - Get quota warnings
pub async fn get_drive_warnings_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::check_drive_warnings(state.db.pool()).await)
}

/// Query params for listing drive files
#[derive(Debug, Deserialize)]
pub struct ListDriveFilesQuery {
    #[serde(default = "default_drive_path")]
    pub path: String,
}

fn default_drive_path() -> String {
    String::new() // Empty string = root directory
}

/// GET /api/drive/files - List files in a directory
pub async fn list_drive_files_handler(
    State(state): State<AppState>,
    Query(params): Query<ListDriveFilesQuery>,
) -> Response {
    api_response(crate::api::list_drive_files(state.db.pool(), &params.path).await)
}

/// GET /api/drive/files/:id - Get file metadata
pub async fn get_drive_file_handler(
    State(state): State<AppState>,
    Path(file_id): Path<String>,
) -> Response {
    api_response(crate::api::get_drive_file(state.db.pool(), &file_id).await)
}

/// GET /api/drive/files/:id/download - Download file content
pub async fn download_drive_file_handler(
    State(state): State<AppState>,
    Path(file_id): Path<String>,
) -> Response {
    let config = crate::api::DriveConfig::from_env();
    match crate::api::download_drive_file(state.db.pool(), &config, &file_id).await {
        Ok((file, content)) => {
            let content_type = file
                .mime_type
                .unwrap_or_else(|| "application/octet-stream".to_string());
            (
                [
                    (axum::http::header::CONTENT_TYPE, content_type),
                    (
                        axum::http::header::CONTENT_DISPOSITION,
                        format!("attachment; filename=\"{}\"", file.filename),
                    ),
                ],
                content,
            )
                .into_response()
        }
        Err(e) => error_response(e),
    }
}

/// DELETE /api/drive/files/:id - Delete a file or folder
pub async fn delete_drive_file_handler(
    State(state): State<AppState>,
    Path(file_id): Path<String>,
) -> Response {
    let config = crate::api::DriveConfig::from_env();
    match crate::api::delete_drive_file(state.db.pool(), &config, &file_id).await {
        Ok(_) => success_message("File deleted"),
        Err(e) => error_response(e),
    }
}

/// PUT /api/drive/files/:id/move - Move or rename a file
pub async fn move_drive_file_handler(
    State(state): State<AppState>,
    Path(file_id): Path<String>,
    Json(request): Json<crate::api::DriveMoveFileRequest>,
) -> Response {
    let config = crate::api::DriveConfig::from_env();
    api_response(
        crate::api::move_drive_file(state.db.pool(), &config, &file_id, &request.new_path).await,
    )
}

/// POST /api/drive/upload - Upload a file (multipart form)
pub async fn upload_drive_file_handler(
    State(state): State<AppState>,
    mut multipart: axum::extract::Multipart,
) -> Response {
    let config = crate::api::DriveConfig::from_env();

    // Parse multipart form
    let mut path: Option<String> = None;
    let mut filename: Option<String> = None;
    let mut mime_type: Option<String> = None;
    let mut data: Option<axum::body::Bytes> = None;

    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "path" => {
                if let Ok(text) = field.text().await {
                    path = Some(text);
                }
            }
            "file" => {
                filename = field.file_name().map(|s| s.to_string());
                mime_type = field.content_type().map(|s| s.to_string());
                if let Ok(bytes) = field.bytes().await {
                    data = Some(bytes);
                }
            }
            _ => {}
        }
    }

    let request = crate::api::DriveUploadRequest {
        path: path.unwrap_or_else(|| "uploads".to_string()),
        filename: filename.unwrap_or_else(|| "unnamed".to_string()),
        mime_type,
    };

    match data {
        Some(bytes) => {
            match crate::api::upload_drive_file(state.db.pool(), &config, request, bytes).await {
                Ok(file) => (StatusCode::CREATED, Json(file)).into_response(),
                Err(e) => error_response(e),
            }
        }
        None => error_response(crate::error::Error::InvalidInput(
            "No file data provided".into(),
        )),
    }
}

/// POST /api/drive/folders - Create a folder
pub async fn create_drive_folder_handler(
    State(state): State<AppState>,
    Json(request): Json<crate::api::DriveCreateFolderRequest>,
) -> Response {
    let config = crate::api::DriveConfig::from_env();
    match crate::api::create_drive_folder(state.db.pool(), &config, request).await {
        Ok(folder) => (StatusCode::CREATED, Json(folder)).into_response(),
        Err(e) => error_response(e),
    }
}

/// POST /api/drive/reconcile - Reconcile usage with filesystem (admin)
pub async fn reconcile_drive_usage_handler(State(state): State<AppState>) -> Response {
    let config = crate::api::DriveConfig::from_env();
    api_response(crate::api::reconcile_drive_usage(state.db.pool(), &config).await)
}

// =============================================================================
// Drive Trash Handlers
// =============================================================================

/// GET /api/drive/trash - List files in trash
pub async fn list_drive_trash_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::list_drive_trash(state.db.pool()).await)
}

/// POST /api/drive/files/:id/restore - Restore a file from trash
pub async fn restore_drive_file_handler(
    State(state): State<AppState>,
    Path(file_id): Path<String>,
) -> Response {
    api_response(crate::api::restore_drive_file(state.db.pool(), &file_id).await)
}

/// DELETE /api/drive/files/:id/purge - Permanently delete a file (skip trash)
pub async fn purge_drive_file_handler(
    State(state): State<AppState>,
    Path(file_id): Path<String>,
) -> Response {
    let config = crate::api::DriveConfig::from_env();
    match crate::api::purge_drive_file(state.db.pool(), &config, &file_id).await {
        Ok(_) => success_message("File permanently deleted"),
        Err(e) => error_response(e),
    }
}

/// POST /api/drive/trash/empty - Empty all files from trash
pub async fn empty_drive_trash_handler(State(state): State<AppState>) -> Response {
    let config = crate::api::DriveConfig::from_env();
    match crate::api::empty_drive_trash(state.db.pool(), &config).await {
        Ok(count) => {
            (StatusCode::OK, Json(serde_json::json!({ "deleted_count": count }))).into_response()
        }
        Err(e) => error_response(e),
    }
}
