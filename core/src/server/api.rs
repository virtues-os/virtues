//! REST API handlers for source management

use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};

use super::ingest::AppState;
use crate::error::Error;

/// Sanitize a filename for use in Content-Disposition headers.
/// Removes characters that could cause header injection or parsing issues.
fn sanitize_content_disposition(filename: &str) -> String {
    filename
        .replace('"', "'")
        .replace('\\', "_")
        .replace('\r', "")
        .replace('\n', "")
}

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
    Path(source_id): Path<String>,
) -> Response {
    api_response(crate::api::get_source(state.db.pool(), source_id).await)
}

/// Pause a source
pub async fn pause_source_handler(
    State(state): State<AppState>,
    Path(source_id): Path<String>,
) -> Response {
    api_response(crate::api::pause_source(state.db.pool(), source_id).await)
}

/// Resume a source
pub async fn resume_source_handler(
    State(state): State<AppState>,
    Path(source_id): Path<String>,
) -> Response {
    api_response(crate::api::resume_source(state.db.pool(), source_id).await)
}

/// Delete a source by ID
pub async fn delete_source_handler(
    State(state): State<AppState>,
    Path(source_id): Path<String>,
) -> Response {
    match crate::api::delete_source(state.db.pool(), source_id).await {
        Ok(_) => success_message("Source deleted successfully"),
        Err(e) => error_response(e),
    }
}

/// Get source status with statistics
pub async fn get_source_status_handler(
    State(state): State<AppState>,
    Path(source_id): Path<String>,
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

/// Handle OAuth callback and return HTML redirect
///
/// The return URL comes from the state parameter that was set during OAuth initiation.
/// This allows any client (web, iOS, Mac) to specify where they want to be redirected
/// after OAuth completes, without the backend needing to know about specific frontend URLs.
pub async fn oauth_callback_handler(
    State(state): State<AppState>,
    Query(params): Query<crate::api::OAuthCallbackParams>,
) -> Response {
    // Try to extract return URL from state FIRST, before processing
    // This way we can redirect back to the client even if OAuth processing fails
    let return_url_from_state = params.state.as_ref().and_then(|s| {
        crate::sources::base::oauth::state::validate_and_extract_state(s)
            .ok()
            .flatten()
    });

    match crate::api::handle_oauth_callback(
        state.db.pool(),
        Some(&state.storage),
        Some(state.stream_writer.clone()),
        &params,
    )
    .await
    {
        Ok(response) => {
            // Use return_url from response (same as state extraction, but validated)
            let return_url = response
                .return_url
                .unwrap_or_else(|| "/data/sources/add".to_string());

            // Build final URL with source_id for configuration
            let final_url = if return_url.contains('?') {
                format!(
                    "{}&source_id={}&connected=true",
                    return_url, response.source.id
                )
            } else {
                format!(
                    "{}?source_id={}&connected=true",
                    return_url, response.source.id
                )
            };

            generate_redirect_html(&final_url, "Connection successful! Redirecting...")
        }
        Err(e) => {
            // Try to redirect back to the client's origin with error info
            // Fall back to BACKEND_URL only if we couldn't extract the return URL
            let error_base = return_url_from_state.unwrap_or_else(|| {
                let backend_url = std::env::var("BACKEND_URL")
                    .unwrap_or_else(|_| "http://localhost:8000".to_string());
                format!("{}/data/sources/add", backend_url)
            });

            let error_url = if error_base.contains('?') {
                format!(
                    "{}&error={}",
                    error_base,
                    urlencoding::encode(&e.to_string())
                )
            } else {
                format!(
                    "{}?error={}",
                    error_base,
                    urlencoding::encode(&e.to_string())
                )
            };

            generate_redirect_html(&error_url, "An error occurred. Redirecting...")
        }
    }
}

/// Generate an HTML page that redirects to the given URL
fn generate_redirect_html(url: &str, message: &str) -> Response {
    // HTML-escape the URL to prevent XSS
    let escaped_url = html_escape::encode_double_quoted_attribute(url);
    let escaped_url_text = html_escape::encode_text(url);

    let html = format!(
        r#"<!DOCTYPE html>
<html>
<head>
    <meta http-equiv="refresh" content="0;url={}">
    <title>Redirecting...</title>
</head>
<body>
    <p>{}</p>
    <p>If you are not redirected automatically, <a href="{}">click here</a>.</p>
</body>
</html>"#,
        escaped_url, message, escaped_url_text
    );

    (StatusCode::OK, [("content-type", "text/html")], html).into_response()
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
    Path(source_id): Path<String>,
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

/// Bulk update multiple streams for a source
///
/// POST /api/sources/:id/streams
///
/// This endpoint allows updating multiple streams in a single request.
/// It's more efficient than calling enable/disable for each stream individually.
pub async fn bulk_update_streams_handler(
    State(state): State<AppState>,
    Path(source_id): Path<String>,
    Json(request): Json<crate::api::BulkUpdateStreamsRequest>,
) -> Response {
    match crate::api::bulk_update_streams(
        state.db.pool(),
        &*state.storage,
        state.stream_writer.clone(),
        source_id,
        request.streams,
    )
    .await
    {
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

/// Get stream details
pub async fn get_stream_handler(
    State(state): State<AppState>,
    Path((source_id, stream_name)): Path<(String, String)>,
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
    Path((source_id, stream_name)): Path<(String, String)>,
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
    Path((source_id, stream_name)): Path<(String, String)>,
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
    Path((source_id, stream_name)): Path<(String, String)>,
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
    Path((source_id, stream_name)): Path<(String, String)>,
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
    Path((source_id, stream_name)): Path<(String, String)>,
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

/// Connection limits per tier
#[derive(Debug, Serialize)]
pub struct CatalogConnectionLimits {
    pub standard: u8,
    pub pro: u8,
}

/// Simplified catalog source for frontend display
#[derive(Debug, Serialize)]
pub struct CatalogSource {
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub auth_type: String,
    pub stream_count: usize,
    pub icon: Option<String>,
    /// Whether this source allows multiple connections
    pub is_multi_instance: bool,
    /// Connection limits per tier (only for multi-instance sources)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connection_limits: Option<CatalogConnectionLimits>,
    /// Current number of active connections (populated when db is available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_connections: Option<i64>,
}

/// Fetch user tier from Tollbooth (which hydrates from Atlas)
async fn fetch_user_tier() -> Result<String, String> {
    let tollbooth_url =
        std::env::var("TOLLBOOTH_URL").unwrap_or_else(|_| "http://localhost:9002".to_string());
    let secret = std::env::var("TOLLBOOTH_INTERNAL_SECRET")
        .map_err(|_| "TOLLBOOTH_INTERNAL_SECRET not set".to_string())?;

    let client = reqwest::Client::new();
    let resp = crate::tollbooth::with_system_auth(
        client.get(format!("{}/v1/limits/tier", tollbooth_url)),
        &secret,
    )
    .send()
    .await
    .map_err(|e| format!("Tollbooth request failed: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("Tollbooth returned {}", resp.status()));
    }

    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse Tollbooth response: {}", e))?;

    body.get("tier")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| "Tollbooth response missing 'tier' field".to_string())
}

/// List all available source types from the registry
pub async fn list_catalog_sources_handler(State(state): State<AppState>) -> Response {
    // Fetch tier from Tollbooth (fallback to TIER env var, then "standard")
    let user_tier = match fetch_user_tier().await {
        Ok(tier) => tier,
        Err(e) => {
            let fallback = std::env::var("TIER").unwrap_or_else(|_| "standard".to_string());
            tracing::warn!(
                "Failed to fetch tier from Tollbooth: {}, using fallback '{}'",
                e,
                fallback
            );
            fallback
        }
    };

    let sources = crate::registry::list_sources();

    // Get current connection counts per source type
    let counts: std::collections::HashMap<String, i64> = sqlx::query_as::<_, (String, i64)>(
        "SELECT source, COUNT(*) FROM elt_source_connections WHERE is_active = true GROUP BY source",
    )
    .fetch_all(state.db.pool())
    .await
    .unwrap_or_default()
    .into_iter()
    .collect();

    let catalog: Vec<CatalogSource> = sources
        .iter()
        .map(|s| {
            let is_multi = virtues_registry::is_multi_instance(s.descriptor.name);
            let limits = if is_multi {
                virtues_registry::get_connection_limit(s.descriptor.name, "standard").map(
                    |standard| CatalogConnectionLimits {
                        standard,
                        pro: virtues_registry::get_connection_limit(s.descriptor.name, "pro")
                            .unwrap_or(standard),
                    },
                )
            } else {
                None
            };

            CatalogSource {
                name: s.descriptor.name.to_string(),
                display_name: s.descriptor.display_name.to_string(),
                description: s.descriptor.description.to_string(),
                auth_type: format!("{:?}", s.descriptor.auth_type).to_lowercase(),
                stream_count: s.streams.len(),
                icon: s.descriptor.icon.map(|i| i.to_string()),
                is_multi_instance: is_multi,
                connection_limits: limits,
                current_connections: counts.get(s.descriptor.name).copied(),
            }
        })
        .collect();

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "tier": user_tier,
            "sources": catalog,
        })),
    )
        .into_response()
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
pub async fn get_job_handler(
    State(state): State<AppState>,
    Path(job_id): Path<String>,
) -> Response {
    match crate::api::get_job_status(state.db.pool(), &job_id).await {
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
    pub source_id: Option<String>,
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
        limit: Some(params.limit.unwrap_or(16)),
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
    Path(job_id): Path<String>,
) -> Response {
    match crate::api::cancel_job(state.db.pool(), &job_id).await {
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
    Path((source_id, stream_name)): Path<(String, String)>,
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
// Developer API
// ============================================================================

/// Execute a read-only SQL query
pub async fn execute_sql_handler(
    State(state): State<AppState>,
    Json(request): Json<crate::api::ExecuteSqlRequest>,
) -> Response {
    match crate::api::execute_sql(state.db.pool(), request).await {
        Ok(results) => (StatusCode::OK, Json(results)).into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": e.to_string()
            })),
        )
            .into_response(),
    }
}

/// List all tables
pub async fn list_tables_handler(State(state): State<AppState>) -> Response {
    match crate::api::list_tables(state.db.pool()).await {
        Ok(tables) => (StatusCode::OK, Json(tables)).into_response(),
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
    pub source_id: String,
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
    pub source_id: String,
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

/// Request to complete QR-based pairing (called by iOS app after scanning QR)
#[derive(Debug, Deserialize)]
pub struct CompleteQRPairingRequest {
    pub device_id: String,
    pub device_info: crate::DeviceInfo,
}

/// Complete QR-based device pairing by source ID
pub async fn complete_qr_pairing_handler(
    State(state): State<AppState>,
    Path(source_id): Path<String>,
    Json(request): Json<CompleteQRPairingRequest>,
) -> Response {
    match crate::api::complete_pairing_by_source_id(
        state.db.pool(),
        &source_id,
        &request.device_id,
        request.device_info,
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
        Err(e) => {
            let status = match &e {
                crate::Error::InvalidInput(_) => StatusCode::BAD_REQUEST,
                crate::Error::Other(msg) if msg.contains("not found") => StatusCode::NOT_FOUND,
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
    Path(source_id): Path<String>,
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
    pub source_id: String,
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

// Note: Built-in tools cannot be updated. They are read-only from the registry.
// MCP tools can be managed via separate endpoints (to be implemented).

// =============================================================================
// Models API
// =============================================================================

/// List all available models
pub async fn list_models_handler() -> Response {
    api_response(crate::api::list_models().await)
}

/// Get a specific model by ID
pub async fn get_model_handler(Path(model_id): Path<String>) -> Response {
    api_response(crate::api::get_model(&model_id).await)
}

/// List recommended models with slot assignments
pub async fn list_recommended_models_handler() -> Response {
    api_response(crate::api::list_recommended_models().await)
}

// =============================================================================
// Agents API
// =============================================================================

/// List all available agents
pub async fn list_agents_handler() -> Response {
    api_response(crate::api::list_agents().await)
}

/// Get a specific agent by ID
pub async fn get_agent_handler(Path(agent_id): Path<String>) -> Response {
    api_response(crate::api::get_agent(&agent_id).await)
}

// =============================================================================
// Personas API
// =============================================================================

/// List all personas (excluding hidden ones)
pub async fn list_personas_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::list_personas(state.db.pool()).await)
}

/// Get a specific persona by ID
pub async fn get_persona_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    api_response(crate::api::get_persona(state.db.pool(), &id).await)
}

/// Create a new custom persona
pub async fn create_persona_handler(
    State(state): State<AppState>,
    Json(request): Json<crate::api::CreatePersonaRequest>,
) -> Response {
    api_response(crate::api::create_persona(state.db.pool(), request).await)
}

/// Update an existing persona
pub async fn update_persona_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<crate::api::UpdatePersonaRequest>,
) -> Response {
    api_response(crate::api::update_persona(state.db.pool(), &id, request).await)
}

/// Hide a persona (soft delete for system, hard delete for custom)
pub async fn hide_persona_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    api_response(crate::api::hide_persona(state.db.pool(), &id).await)
}

/// Unhide a previously hidden persona
pub async fn unhide_persona_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    api_response(crate::api::unhide_persona(state.db.pool(), &id).await)
}

/// Reset personas to defaults (re-seed from registry)
pub async fn reset_personas_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::reset_personas(state.db.pool()).await)
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
/// Triggers initial sync for all enabled streams.
pub async fn exchange_plaid_token_handler(
    State(state): State<AppState>,
    Json(request): Json<crate::api::ExchangeTokenRequest>,
) -> Response {
    match crate::api::exchange_public_token(
        state.db.pool(),
        &state.storage,
        state.stream_writer.clone(),
        request,
    )
    .await
    {
        Ok(response) => (StatusCode::CREATED, Json(response)).into_response(),
        Err(e) => error_response(e),
    }
}

/// Get accounts for an existing Plaid connection
pub async fn get_plaid_accounts_handler(
    State(state): State<AppState>,
    Path(source_id): Path<String>,
) -> Response {
    api_response(crate::api::get_plaid_accounts(state.db.pool(), source_id).await)
}

/// Remove a Plaid Item (disconnect bank account)
pub async fn remove_plaid_item_handler(
    State(state): State<AppState>,
    Path(source_id): Path<String>,
) -> Response {
    match crate::api::remove_plaid_item(state.db.pool(), source_id).await {
        Ok(_) => success_message("Plaid item removed successfully"),
        Err(e) => error_response(e),
    }
}

// ============================================================================
// Onboarding API
// ============================================================================

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
// Subscription & Billing API Handlers
// =============================================================================

/// GET /api/subscription - Get subscription status (proxied from Tollbooth)
///
/// Returns a safe fallback when Tollbooth is unreachable (e.g., local dev without Tollbooth).
/// This prevents 500 errors from spamming the browser console during development.
pub async fn get_subscription_handler(user: crate::middleware::auth::AuthUser) -> Response {
    match crate::api::subscription::get_subscription_status(&user.id).await {
        Ok(data) => (StatusCode::OK, Json(data)).into_response(),
        Err(e) => {
            tracing::debug!(
                "Subscription check failed (Tollbooth may be unavailable): {}",
                e
            );
            // Return a safe fallback: assume active so the app works without Tollbooth
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "status": "active",
                    "trial_expires_at": null,
                    "days_remaining": null,
                    "is_active": true
                })),
            )
                .into_response()
        }
    }
}

/// POST /api/billing/portal - Create Stripe billing portal session (proxied via Tollbooth → Atlas)
pub async fn create_billing_portal_handler(user: crate::middleware::auth::AuthUser) -> Response {
    match crate::api::subscription::create_billing_portal(&user.id).await {
        Ok(data) => (StatusCode::OK, Json(data)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

// =============================================================================
// System Update API Handlers
// =============================================================================

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
// Unsplash API Handler
// =============================================================================

/// Search Unsplash photos for cover images
pub async fn unsplash_search_handler(
    Json(request): Json<crate::api::UnsplashSearchRequest>,
) -> Response {
    match crate::api::unsplash_search(request).await {
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
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
    Path(object_id): Path<String>,
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
    Path(place_id): Path<String>,
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
    Path(place_id): Path<String>,
    Json(request): Json<crate::api::UpdatePlaceRequest>,
) -> Response {
    api_response(crate::api::update_place(state.db.pool(), place_id, request).await)
}

/// Delete a place
pub async fn delete_place_handler(
    State(state): State<AppState>,
    Path(place_id): Path<String>,
) -> Response {
    match crate::api::delete_place(state.db.pool(), place_id).await {
        Ok(_) => success_message("Place deleted successfully"),
        Err(e) => error_response(e),
    }
}

/// Set a place as the user's home
pub async fn set_place_as_home_handler(
    State(state): State<AppState>,
    Path(place_id): Path<String>,
) -> Response {
    match crate::api::set_home_place_entity(state.db.pool(), place_id).await {
        Ok(_) => success_message("Home place updated"),
        Err(e) => error_response(e),
    }
}

// ============================================================================
// Wiki API Handlers
// ============================================================================

/// Resolve an entity ID to its type
pub async fn wiki_resolve_id_handler(
    State(_state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    api_response(crate::api::resolve_id(&id))
}

// --- Person ---

/// Get a person by ID
pub async fn wiki_get_person_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    api_response(crate::api::get_person(state.db.pool(), id).await)
}

/// List all people
pub async fn wiki_list_people_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::list_people(state.db.pool()).await)
}

/// Update a person by ID
pub async fn wiki_update_person_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<crate::api::UpdateWikiPersonRequest>,
) -> Response {
    api_response(crate::api::update_person(state.db.pool(), id, request).await)
}

// --- Place ---

/// Get a place by ID
pub async fn wiki_get_place_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    api_response(crate::api::get_wiki_place(state.db.pool(), id).await)
}

/// List all places (wiki view)
pub async fn wiki_list_places_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::list_wiki_places(state.db.pool()).await)
}

/// Update a place by ID (wiki fields)
pub async fn wiki_update_place_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<crate::api::UpdateWikiPlaceRequest>,
) -> Response {
    api_response(crate::api::update_wiki_place(state.db.pool(), id, request).await)
}

// --- Organization ---

/// Get an organization by ID
pub async fn wiki_get_organization_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    api_response(crate::api::get_organization(state.db.pool(), id).await)
}

/// List all organizations
pub async fn wiki_list_organizations_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::list_organizations(state.db.pool()).await)
}

/// Update an organization by ID
pub async fn wiki_update_organization_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<crate::api::UpdateWikiOrganizationRequest>,
) -> Response {
    api_response(crate::api::update_organization(state.db.pool(), id, request).await)
}

// --- Telos ---

/// Get active telos
pub async fn wiki_get_active_telos_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::get_active_telos(state.db.pool()).await)
}

/// Get a telos by ID
pub async fn wiki_get_telos_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    api_response(crate::api::get_telos(state.db.pool(), &id).await)
}

// --- Act ---

/// Get an act by ID
pub async fn wiki_get_act_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    api_response(crate::api::get_act(state.db.pool(), id).await)
}

/// List all acts
pub async fn wiki_list_acts_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::list_acts(state.db.pool()).await)
}

// --- Chapter ---

/// Get a chapter by ID
pub async fn wiki_get_chapter_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    api_response(crate::api::get_chapter(state.db.pool(), id).await)
}

/// List chapters for an act
pub async fn wiki_list_chapters_handler(
    State(state): State<AppState>,
    Path(act_id): Path<String>,
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

/// Generate a daily summary for a specific date
pub async fn wiki_generate_day_summary_handler(
    State(state): State<AppState>,
    Path(date): Path<String>,
) -> Response {
    match date.parse::<chrono::NaiveDate>() {
        Ok(parsed_date) => {
            api_response(
                crate::api::day_summary::generate_day_summary(state.db.pool(), parsed_date).await,
            )
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
    Path((source_type, source_id)): Path<(String, String)>,
) -> Response {
    api_response(crate::api::get_citations(state.db.pool(), &source_type, source_id).await)
}

/// Create a citation for a wiki page
pub async fn wiki_create_citation_handler(
    State(state): State<AppState>,
    Path((source_type, source_id)): Path<(String, String)>,
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
    Path(citation_id): Path<String>,
    Json(request): Json<crate::api::UpdateCitationRequest>,
) -> Response {
    api_response(crate::api::update_citation(state.db.pool(), citation_id, request).await)
}

/// Delete a citation
pub async fn wiki_delete_citation_handler(
    State(state): State<AppState>,
    Path(citation_id): Path<String>,
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
    Path(event_id): Path<String>,
    Json(request): Json<crate::api::UpdateTemporalEventRequest>,
) -> Response {
    api_response(crate::api::update_temporal_event(state.db.pool(), event_id, request).await)
}

/// Delete a temporal event
pub async fn wiki_delete_event_handler(
    State(state): State<AppState>,
    Path(event_id): Path<String>,
) -> Response {
    match crate::api::delete_temporal_event(state.db.pool(), event_id).await {
        Ok(_) => success_message("Event deleted"),
        Err(e) => error_response(e),
    }
}

/// Delete all auto-generated events for a day (regeneration support)
pub async fn wiki_delete_auto_events_handler(
    State(state): State<AppState>,
    Path(day_id): Path<String>,
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

/// Get timeline location chunks for a day (movement map)
pub async fn timeline_get_day_handler(
    State(state): State<AppState>,
    Path(date): Path<String>,
) -> Response {
    match date.parse::<chrono::NaiveDate>() {
        Ok(parsed_date) => {
            api_response(crate::api::get_timeline_day(state.db.pool(), parsed_date).await)
        }
        Err(_) => error_response(Error::InvalidInput(format!(
            "Invalid date format: {}",
            date
        ))),
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

/// Get 2D vector projection of W6H embeddings for a day
pub async fn wiki_get_day_vectors_handler(
    State(state): State<AppState>,
    Path(date): Path<String>,
) -> Response {
    api_response(
        crate::api::day_vectors::get_day_vector_projection(state.db.pool(), &date).await,
    )
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
// Chat Usage & Compaction API Handlers
// =============================================================================

/// Get token usage for a chat
pub async fn get_chat_usage_handler(
    State(state): State<AppState>,
    Path(chat_id): Path<String>,
) -> Response {
    api_response(crate::api::get_chat_usage(state.db.pool(), chat_id).await)
}

/// Compact a chat (summarize older messages)
pub async fn compact_chat_handler(
    State(state): State<AppState>,
    Path(chat_id): Path<String>,
    Json(request): Json<Option<CompactChatRequest>>,
) -> Response {
    let options = request.unwrap_or_default().into();
    api_response(crate::api::compaction::compact_chat(state.db.pool(), chat_id, options).await)
}

/// Request body for compaction
#[derive(Debug, Deserialize, Default)]
pub struct CompactChatRequest {
    /// Number of recent exchanges to keep verbatim (default: 4)
    pub keep_recent_exchanges: Option<usize>,
    /// Force compaction even if under threshold
    #[serde(default)]
    pub force: bool,
}

impl From<CompactChatRequest> for crate::api::compaction::CompactionOptions {
    fn from(req: CompactChatRequest) -> Self {
        let default_opts = crate::api::compaction::CompactionOptions::default();
        Self {
            keep_recent_exchanges: req
                .keep_recent_exchanges
                .unwrap_or(default_opts.keep_recent_exchanges),
            force: req.force,
            model_id: None, // API compaction uses default model context window
        }
    }
}

// =============================================================================
// Chats API Handlers
// =============================================================================

/// List chats
pub async fn list_chats_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::chats::list_chats(state.db.pool(), 25).await)
}

/// Create a new chat with initial messages
pub async fn create_chat_handler(
    State(state): State<AppState>,
    Json(request): Json<crate::api::chats::CreateChatRequest>,
) -> Response {
    api_response(crate::api::chats::create_chat_from_request(state.db.pool(), request).await)
}

/// Get a chat by ID
pub async fn get_chat_handler(
    State(state): State<AppState>,
    Path(chat_id): Path<String>,
) -> Response {
    api_response(crate::api::chats::get_chat(state.db.pool(), chat_id).await)
}

/// Update a chat (title and/or icon)
pub async fn update_chat_handler(
    State(state): State<AppState>,
    Path(chat_id): Path<String>,
    Json(request): Json<crate::api::chats::UpdateChatRequest>,
) -> Response {
    api_response(
        crate::api::chats::update_chat(state.db.pool(), chat_id, &request).await,
    )
}

/// Delete a chat
pub async fn delete_chat_handler(
    State(state): State<AppState>,
    Path(chat_id): Path<String>,
) -> Response {
    api_response(crate::api::chats::delete_chat(state.db.pool(), chat_id).await)
}

/// Generate a title for a chat
pub async fn generate_chat_title_handler(
    State(state): State<AppState>,
    Json(request): Json<crate::api::chats::GenerateTitleRequest>,
) -> Response {
    api_response(
        crate::api::chats::generate_title(state.db.pool(), request.chat_id, &request.messages)
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
        axum::extract::State(state.yjs_state.clone()),
        axum::extract::State(state.chat_cancel_state.clone()),
        user,
        Json(request),
    )
    .await
}

/// POST /api/chat/cancel - Cancel an in-progress chat request
pub async fn cancel_chat_handler(
    State(state): State<AppState>,
    user: crate::middleware::auth::AuthUser,
    Json(request): Json<crate::api::chat::CancelChatRequest>,
) -> impl IntoResponse {
    crate::api::chat::cancel_chat_handler(
        axum::extract::State(state.chat_cancel_state.clone()),
        user,
        Json(request),
    )
    .await
}

// =============================================================================
// Chat Edit Permissions API Handlers
// =============================================================================

/// GET /api/chats/:id/permissions - List edit permissions for a chat
pub async fn list_chat_permissions_handler(
    State(state): State<AppState>,
    Path(chat_id): Path<String>,
) -> Response {
    api_response(crate::api::chat_permissions::list_permissions(state.db.pool(), &chat_id).await)
}

/// POST /api/chats/:id/permissions - Add an edit permission
pub async fn add_chat_permission_handler(
    State(state): State<AppState>,
    Path(chat_id): Path<String>,
    Json(request): Json<crate::api::chat_permissions::AddPermissionRequest>,
) -> Response {
    api_response(
        crate::api::chat_permissions::add_permission(state.db.pool(), &chat_id, request).await,
    )
}

/// DELETE /api/chats/:id/permissions/:entity_id - Remove an edit permission
pub async fn remove_chat_permission_handler(
    State(state): State<AppState>,
    Path((chat_id, entity_id)): Path<(String, String)>,
) -> Response {
    match crate::api::chat_permissions::remove_permission(state.db.pool(), &chat_id, &entity_id)
        .await
    {
        Ok(_) => success_message("Permission removed"),
        Err(e) => error_response(e),
    }
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
    // Reconcile storage with database before listing
    if let Err(e) =
        crate::api::reconcile_drive_folder(state.db.pool(), &state.drive_config, &params.path).await
    {
        tracing::warn!("Folder reconciliation failed: {e}");
        // Non-fatal: continue with DB-only listing
    }
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
    // Lake objects use in-memory download (different storage layer)
    if crate::api::is_lake_object_id(&file_id) {
        let result =
            crate::api::download_lake_object(state.db.pool(), &state.storage, &file_id).await;
        return match result {
            Ok((file, content)) => {
                let content_type = file
                    .mime_type
                    .unwrap_or_else(|| "application/octet-stream".to_string());
                let filename = sanitize_content_disposition(&file.filename);
                (
                    [
                        (axum::http::header::CONTENT_TYPE, content_type),
                        (
                            axum::http::header::CONTENT_DISPOSITION,
                            format!("attachment; filename=\"{}\"", filename),
                        ),
                        (
                            axum::http::header::CONTENT_LENGTH,
                            content.len().to_string(),
                        ),
                    ],
                    content,
                )
                    .into_response()
            }
            Err(e) => error_response(e),
        };
    }

    // Regular drive files: stream from storage
    let result =
        crate::api::download_drive_file_stream(state.db.pool(), &state.drive_config, &file_id)
            .await;
    match result {
        Ok((file, stream)) => {
            let content_type = file
                .mime_type
                .unwrap_or_else(|| "application/octet-stream".to_string());
            let filename = sanitize_content_disposition(&file.filename);
            (
                [
                    (axum::http::header::CONTENT_TYPE, content_type),
                    (
                        axum::http::header::CONTENT_DISPOSITION,
                        format!("attachment; filename=\"{}\"", filename),
                    ),
                    (
                        axum::http::header::CONTENT_LENGTH,
                        file.size_bytes.to_string(),
                    ),
                ],
                Body::from_stream(stream),
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
    match crate::api::delete_drive_file(state.db.pool(), &state.drive_config, &file_id).await {
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
    api_response(
        crate::api::move_drive_file(
            state.db.pool(),
            &state.drive_config,
            &file_id,
            &request.new_path,
        )
        .await,
    )
}

/// POST /api/drive/upload - Upload a file (multipart form)
pub async fn upload_drive_file_handler(
    State(state): State<AppState>,
    mut multipart: axum::extract::Multipart,
) -> Response {
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
            match crate::api::upload_drive_file(
                state.db.pool(),
                &state.drive_config,
                request,
                bytes,
            )
            .await
            {
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
    match crate::api::create_drive_folder(state.db.pool(), &state.drive_config, request).await {
        Ok(folder) => (StatusCode::CREATED, Json(folder)).into_response(),
        Err(e) => error_response(e),
    }
}

/// POST /api/drive/reconcile - Reconcile usage with storage (admin)
pub async fn reconcile_drive_usage_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::reconcile_drive_usage(state.db.pool(), &state.drive_config).await)
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
    match crate::api::purge_drive_file(state.db.pool(), &state.drive_config, &file_id).await {
        Ok(_) => success_message("File permanently deleted"),
        Err(e) => error_response(e),
    }
}

/// POST /api/drive/trash/empty - Empty all files from trash
pub async fn empty_drive_trash_handler(State(state): State<AppState>) -> Response {
    match crate::api::empty_drive_trash(state.db.pool(), &state.drive_config).await {
        Ok(count) => (
            StatusCode::OK,
            Json(serde_json::json!({ "deleted_count": count })),
        )
            .into_response(),
        Err(e) => error_response(e),
    }
}

// =============================================================================
// Media Handlers
// =============================================================================

/// POST /api/media/upload - Upload media file with content-addressed dedup
///
/// Accepts multipart form with:
/// - `file`: The file data (required)
/// - `filename`: Override filename (optional, uses file's name by default)
///
/// Returns MediaFile with URL for embedding in pages.
/// If identical content already exists, returns existing file (dedup).
pub async fn upload_media_handler(
    State(state): State<AppState>,
    mut multipart: axum::extract::Multipart,
) -> Response {
    // Parse multipart form
    let mut filename: Option<String> = None;
    let mut mime_type: Option<String> = None;
    let mut data: Option<axum::body::Bytes> = None;

    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "filename" => {
                if let Ok(text) = field.text().await {
                    filename = Some(text);
                }
            }
            "file" => {
                // Use form field filename if no explicit filename provided
                if filename.is_none() {
                    filename = field.file_name().map(|s| s.to_string());
                }
                mime_type = field.content_type().map(|s| s.to_string());
                if let Ok(bytes) = field.bytes().await {
                    data = Some(bytes);
                }
            }
            _ => {}
        }
    }

    let filename = filename.unwrap_or_else(|| "unnamed".to_string());

    match data {
        Some(bytes) => {
            match crate::api::upload_media(
                state.db.pool(),
                &state.drive_config,
                &filename,
                mime_type,
                bytes,
            )
            .await
            {
                Ok(file) => (StatusCode::CREATED, Json(file)).into_response(),
                Err(e) => error_response(e),
            }
        }
        None => error_response(crate::error::Error::InvalidInput(
            "No file data provided".into(),
        )),
    }
}

/// GET /api/media/:id - Get media file metadata
pub async fn get_media_handler(
    State(state): State<AppState>,
    Path(file_id): Path<String>,
) -> Response {
    api_response(crate::api::get_media(state.db.pool(), &file_id).await)
}

// =============================================================================
// Internal API Handlers (Tollbooth Integration)
// =============================================================================

/// POST /internal/hydrate - Hydrate user profile from Tollbooth
///
/// This endpoint is called by Tollbooth on the first request to a newly
/// provisioned container. It seeds the profile with data from Atlas
/// provisioning and marks the server as ready.
pub async fn hydrate_profile_handler(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(request): Json<crate::api::HydrateRequest>,
) -> Response {
    // Validate Tollbooth secret
    let expected_secret = std::env::var("TOLLBOOTH_INTERNAL_SECRET").unwrap_or_default();
    let provided_secret = headers
        .get("X-Tollbooth-Secret")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    // In production, require the secret; in dev, allow any request
    let is_production = std::env::var("RUST_ENV")
        .map(|v| v == "production")
        .unwrap_or(false);

    if is_production && (expected_secret.is_empty() || provided_secret != expected_secret) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({
                "error": "Invalid or missing X-Tollbooth-Secret header"
            })),
        )
            .into_response();
    }

    api_response(crate::api::hydrate_profile(state.db.pool(), request).await)
}

/// GET /internal/server-status - Get current server status
pub async fn get_server_status_handler(State(state): State<AppState>) -> Response {
    match crate::api::get_server_status(state.db.pool()).await {
        Ok(status) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": status.as_str(),
                "is_ready": status == crate::api::ServerStatus::Ready
            })),
        )
            .into_response(),
        Err(e) => error_response(e),
    }
}

/// POST /internal/mark-ready - Mark server as ready (dev/admin use)
pub async fn mark_server_ready_handler(State(state): State<AppState>) -> Response {
    match crate::api::mark_server_ready(state.db.pool()).await {
        Ok(_) => success_message("Server marked as ready"),
        Err(e) => error_response(e),
    }
}

// ============================================================================
// Pages Handlers
// ============================================================================

/// Query params for pages list
#[derive(Debug, Deserialize)]
pub struct ListPagesQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub workspace_id: Option<String>,
}

/// GET /api/pages - List all pages
pub async fn list_pages_handler(
    State(state): State<AppState>,
    Query(query): Query<ListPagesQuery>,
) -> Response {
    // Note: workspace_id filter removed - views handle filtering now
    api_response(crate::api::list_pages(state.db.pool(), query.limit, query.offset).await)
}

/// GET /api/pages/:id - Get a single page
pub async fn get_page_handler(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    api_response(crate::api::get_page(state.db.pool(), &id).await)
}

/// POST /api/pages - Create a new page
pub async fn create_page_handler(
    State(state): State<AppState>,
    Json(request): Json<crate::api::CreatePageRequest>,
) -> Response {
    match crate::api::create_page(state.db.pool(), request).await {
        Ok(page) => (StatusCode::CREATED, Json(page)).into_response(),
        Err(e) => error_response(e),
    }
}

/// PUT /api/pages/:id - Update a page
pub async fn update_page_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<crate::api::UpdatePageRequest>,
) -> Response {
    api_response(crate::api::update_page(state.db.pool(), &id, request).await)
}

/// DELETE /api/pages/:id - Delete a page
pub async fn delete_page_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    match crate::api::delete_page(state.db.pool(), &id).await {
        Ok(_) => success_message("Page deleted successfully"),
        Err(e) => error_response(e),
    }
}

/// Query params for entity search
#[derive(Debug, Deserialize)]
pub struct EntitySearchQuery {
    pub q: String,
}

/// GET /api/pages/search/entities - Search entities for autocomplete
pub async fn search_entities_handler(
    State(state): State<AppState>,
    Query(query): Query<EntitySearchQuery>,
) -> Response {
    api_response(crate::api::search_entities(state.db.pool(), &query.q).await)
}

// ============================================================================
// Page Sharing Handlers
// ============================================================================

/// POST /api/pages/:id/share - Create or replace a share link for a page
pub async fn create_page_share_handler(
    State(state): State<AppState>,
    Path(page_id): Path<String>,
) -> Response {
    match crate::api::create_page_share(state.db.pool(), &page_id).await {
        Ok(share) => (StatusCode::CREATED, Json(share)).into_response(),
        Err(e) => error_response(e),
    }
}

/// GET /api/pages/:id/share - Get the active share for a page
pub async fn get_page_share_handler(
    State(state): State<AppState>,
    Path(page_id): Path<String>,
) -> Response {
    api_response(crate::api::get_page_share(state.db.pool(), &page_id).await)
}

/// DELETE /api/pages/:id/share - Revoke the share for a page
pub async fn delete_page_share_handler(
    State(state): State<AppState>,
    Path(page_id): Path<String>,
) -> Response {
    match crate::api::delete_page_share(state.db.pool(), &page_id).await {
        Ok(_) => success_message("Share revoked"),
        Err(e) => error_response(e),
    }
}

/// GET /api/s/:token - Get a shared page (public, no auth)
pub async fn get_shared_page_handler(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Response {
    api_response(crate::api::get_shared_page(state.db.pool(), &token).await)
}

/// GET /api/s/:token/files/:file_id - Download a file from a shared page (public, no auth)
/// Validates that the file is referenced by the shared page's content
pub async fn shared_file_download_handler(
    State(state): State<AppState>,
    Path((token, file_id)): Path<(String, String)>,
) -> Response {
    // Validate the share token and that this file belongs to the shared page
    if let Err(e) = crate::api::validate_shared_file(state.db.pool(), &token, &file_id).await {
        return error_response(e);
    }

    // Lake objects use in-memory download
    if crate::api::is_lake_object_id(&file_id) {
        let result =
            crate::api::download_lake_object(state.db.pool(), &state.storage, &file_id).await;
        return match result {
            Ok((file, content)) => {
                let content_type = file
                    .mime_type
                    .unwrap_or_else(|| "application/octet-stream".to_string());
                let filename = sanitize_content_disposition(&file.filename);
                (
                    [
                        (axum::http::header::CONTENT_TYPE, content_type),
                        (
                            axum::http::header::CONTENT_DISPOSITION,
                            format!("inline; filename=\"{}\"", filename),
                        ),
                        (
                            axum::http::header::CONTENT_LENGTH,
                            content.len().to_string(),
                        ),
                    ],
                    content,
                )
                    .into_response()
            }
            Err(e) => error_response(e),
        };
    }

    // Regular drive files: stream from storage
    let result =
        crate::api::download_drive_file_stream(state.db.pool(), &state.drive_config, &file_id)
            .await;
    match result {
        Ok((file, stream)) => {
            let content_type = file
                .mime_type
                .unwrap_or_else(|| "application/octet-stream".to_string());
            let filename = sanitize_content_disposition(&file.filename);
            (
                [
                    (axum::http::header::CONTENT_TYPE, content_type),
                    (
                        axum::http::header::CONTENT_DISPOSITION,
                        format!("inline; filename=\"{}\"", filename),
                    ),
                ],
                axum::body::Body::from_stream(stream),
            )
                .into_response()
        }
        Err(e) => error_response(e),
    }
}

// ============================================================================
// Page Versions Handlers
// ============================================================================

/// Query params for versions list
#[derive(Debug, Deserialize)]
pub struct ListVersionsQuery {
    pub limit: Option<i64>,
}

/// GET /api/pages/:id/versions - List versions for a page
pub async fn list_page_versions_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<ListVersionsQuery>,
) -> Response {
    api_response(crate::api::list_versions(state.db.pool(), &id, query.limit).await)
}

/// POST /api/pages/:id/versions - Create a new version snapshot
pub async fn create_page_version_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<crate::api::CreateVersionRequest>,
) -> Response {
    match crate::api::create_version(state.db.pool(), &id, request).await {
        Ok(version) => (StatusCode::CREATED, Json(version)).into_response(),
        Err(e) => error_response(e),
    }
}

/// GET /api/pages/versions/:version_id - Get a single version (with snapshot for restore)
pub async fn get_page_version_handler(
    State(state): State<AppState>,
    Path(version_id): Path<String>,
) -> Response {
    api_response(crate::api::get_version(state.db.pool(), &version_id).await)
}

// ============================================================================
// Spaces Handlers
// ============================================================================

/// GET /api/spaces - List all spaces
pub async fn list_spaces_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::spaces::list_spaces(state.db.pool()).await)
}

/// GET /api/spaces/:id - Get a single space
pub async fn get_space_handler(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    api_response(crate::api::spaces::get_space(state.db.pool(), &id).await)
}

/// POST /api/spaces - Create a new space
pub async fn create_space_handler(
    State(state): State<AppState>,
    Json(request): Json<crate::api::spaces::CreateSpaceRequest>,
) -> Response {
    match crate::api::spaces::create_space(state.db.pool(), request).await {
        Ok(space) => (StatusCode::CREATED, Json(space)).into_response(),
        Err(e) => error_response(e),
    }
}

/// PUT /api/spaces/:id - Update a space
pub async fn update_space_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<crate::api::spaces::UpdateSpaceRequest>,
) -> Response {
    api_response(crate::api::spaces::update_space(state.db.pool(), &id, request).await)
}

/// DELETE /api/spaces/:id - Delete a space
pub async fn delete_space_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    match crate::api::spaces::delete_space(state.db.pool(), &id).await {
        Ok(_) => success_message("Space deleted successfully"),
        Err(e) => error_response(e),
    }
}

/// PUT /api/spaces/:id/tabs - Save tab state for a space
pub async fn save_space_tabs_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<crate::api::spaces::SaveTabStateRequest>,
) -> Response {
    match crate::api::spaces::save_tab_state(state.db.pool(), &id, request).await {
        Ok(_) => success_message("Tab state saved"),
        Err(e) => error_response(e),
    }
}

/// GET /api/spaces/:id/views - Get views for a space
pub async fn list_space_views_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    api_response(crate::api::views::list_views(state.db.pool(), &id).await)
}

/// GET /api/spaces/:id/items - Get root-level items for a space
pub async fn list_space_items_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    api_response(crate::api::views::resolve_space_items(state.db.pool(), &id).await)
}

/// POST /api/spaces/:id/items - Add item to space root level
pub async fn add_space_item_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<ViewItemRequest>,
) -> Response {
    match crate::api::views::add_space_item(state.db.pool(), &id, &request.url).await {
        Ok(item) => (StatusCode::CREATED, Json(item)).into_response(),
        Err(e) => error_response(e),
    }
}

/// DELETE /api/spaces/:id/items - Remove item from space root level
pub async fn remove_space_item_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<ViewItemRequest>,
) -> Response {
    match crate::api::views::remove_space_item(state.db.pool(), &id, &request.url).await {
        Ok(_) => success_message("Item removed from space"),
        Err(e) => error_response(e),
    }
}

/// Request to reorder space items with explicit sort_order values
#[derive(serde::Deserialize)]
pub struct ReorderSpaceItemsRequest {
    pub items: Vec<crate::api::views::ItemSortOrder>,
}

/// PUT /api/spaces/:id/items/reorder - Reorder space root items
pub async fn reorder_space_items_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<ReorderSpaceItemsRequest>,
) -> Response {
    match crate::api::views::reorder_space_items(state.db.pool(), &id, request.items).await {
        Ok(_) => success_message("Space items reordered"),
        Err(e) => error_response(e),
    }
}

// ============================================================================
// Namespaces Handlers
// ============================================================================

/// GET /api/namespaces - List all namespaces
pub async fn list_namespaces_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::namespaces::list_namespaces(state.db.pool()).await)
}

/// GET /api/namespaces/:name - Get a specific namespace
pub async fn get_namespace_handler(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Response {
    api_response(crate::api::namespaces::get_namespace(state.db.pool(), &name).await)
}

// ============================================================================
// Views Handlers
// ============================================================================

/// POST /api/views - Create a new view
pub async fn create_view_handler(
    State(state): State<AppState>,
    Json(request): Json<crate::api::views::CreateViewRequest>,
) -> Response {
    match crate::api::views::create_view(state.db.pool(), request).await {
        Ok(view) => (StatusCode::CREATED, Json(view)).into_response(),
        Err(e) => error_response(e),
    }
}

/// GET /api/views/:id - Get a view
pub async fn get_view_handler(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    api_response(crate::api::views::get_view(state.db.pool(), &id).await)
}

/// PUT /api/views/:id - Update a view
pub async fn update_view_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<crate::api::views::UpdateViewRequest>,
) -> Response {
    api_response(crate::api::views::update_view(state.db.pool(), &id, request).await)
}

/// DELETE /api/views/:id - Delete a view
pub async fn delete_view_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    match crate::api::views::delete_view(state.db.pool(), &id).await {
        Ok(_) => success_message("View deleted successfully"),
        Err(e) => error_response(e),
    }
}

/// Request for resolve view with optional pagination
#[derive(serde::Deserialize)]
pub struct ResolveViewQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// POST /api/views/:id/resolve - Resolve a view to its entities
pub async fn resolve_view_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<ResolveViewQuery>,
) -> Response {
    api_response(
        crate::api::views::resolve_view(state.db.pool(), &id, query.limit, query.offset).await,
    )
}

/// Request to add/remove item from view
#[derive(serde::Deserialize)]
pub struct ViewItemRequest {
    pub url: String,
}

/// POST /api/views/:id/items - Add an item to a manual view
pub async fn add_view_item_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<ViewItemRequest>,
) -> Response {
    api_response(crate::api::views::add_item_to_view(state.db.pool(), &id, &request.url).await)
}

/// DELETE /api/views/:id/items - Remove an item from a manual view
pub async fn remove_view_item_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<ViewItemRequest>,
) -> Response {
    api_response(crate::api::views::remove_item_from_view(state.db.pool(), &id, &request.url).await)
}

/// GET /api/views/:id/items - List items in a manual view
pub async fn list_view_items_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    api_response(crate::api::views::list_view_items(state.db.pool(), &id).await)
}

/// Request to reorder view items
#[derive(serde::Deserialize)]
pub struct ReorderViewItemsRequest {
    pub url_order: Vec<String>,
}

/// PUT /api/views/:id/items/reorder - Reorder items in a manual view
pub async fn reorder_view_items_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<ReorderViewItemsRequest>,
) -> Response {
    api_response(
        crate::api::views::reorder_view_items(state.db.pool(), &id, request.url_order).await,
    )
}

// ============================================================================
// Lake API handlers
// ============================================================================

/// GET /api/lake/summary - Get lake summary statistics
pub async fn get_lake_summary_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::lake::get_lake_summary(state.db.pool()).await)
}

/// GET /api/lake/streams - List all streams in the lake
pub async fn list_lake_streams_handler(State(state): State<AppState>) -> Response {
    api_response(crate::api::lake::list_lake_streams(state.db.pool()).await)
}
