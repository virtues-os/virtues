//! Library-level API functions for programmatic access
//!
//! This module provides the SDK/API layer that can be used by:
//! - CLI applications
//! - HTTP servers
//! - Python wrappers and other bindings
//!
//! The API is organized into submodules for clarity:
//! - `types` - Shared types
//! - `sources` - Source CRUD operations
//! - `oauth` - OAuth flows and authentication
//! - `device_pairing` - Device registration and pairing
//! - `streams` - Stream management and configuration
//! - `jobs` - Async job tracking and management
//! - `registry` - Catalog/registry queries
//! - `ontologies` - Ontology table queries

//! - `rate_limit` - API usage tracking and rate limiting
//! - `models` - LLM model configurations
//! - `agents` - AI agent configurations
//! - `seed_testing` - Seed data pipeline validation and inspection
//! - `metrics` - Activity metrics and job statistics

pub mod agents;
pub mod assistant_profile;
pub mod auth;
pub mod bookmarks;
pub mod chat;
pub mod code;
pub mod compaction;
pub mod device_pairing;
pub mod drive;
pub mod entities;
pub mod exa;
pub mod explorer_nodes;
pub mod internal;
pub mod jobs;
pub mod metrics;
pub mod feedback;
pub mod models;
pub mod oauth;

pub mod ontologies;
pub mod pages;
pub mod places;
pub mod plaid;
pub mod workspaces;

pub mod profile;
pub mod rate_limit;
pub mod registry;
pub mod search;
pub mod seed_testing;
pub mod session_usage;
pub mod sessions;
pub mod sources;
pub mod storage;
pub mod streams;
pub mod token_estimation;
pub mod tools;
pub mod types;
pub mod usage;
pub mod validation;
pub mod wiki;
pub mod developer;
pub mod terminal;

// Re-export commonly used types
pub use streams::StreamConnection;
pub use types::{SourceConnection, SourceConnectionStatus};

// Re-export all functions for convenience
pub use agents::{get_agent, list_agents, AgentInfo};
pub use assistant_profile::{
    get_assistant_name, get_assistant_profile, update_assistant_profile,
    UpdateAssistantProfileRequest,
};
pub use auth::{
    callback_handler,
    cleanup_auth_data,
    // Boot seeding
    seed_owner_email,
    session_handler,
    // Handlers (for server routing)
    signin_handler,
    signout_handler,
    update_owner_email_handler,
    AuthErrorResponse,
    CallbackParams,
    SessionResponse,
    SessionUser,
    // Types
    SignInRequest,
    SignInResponse,
    UpdateOwnerEmailRequest,
    UpdateOwnerEmailResponse,
};
pub use bookmarks::{
    create_entity_bookmark, create_tab_bookmark, delete_bookmark, delete_bookmark_by_entity,
    delete_bookmark_by_route, is_entity_bookmarked, is_route_bookmarked, list_bookmarks,
    toggle_entity_bookmark, toggle_route_bookmark, Bookmark, BookmarkStatus,
    CreateEntityBookmarkRequest, CreateTabBookmarkRequest, ToggleBookmarkResponse,
};
pub use code::{execute_code, ExecuteCodeRequest, ExecuteCodeResponse};
pub use device_pairing::{
    check_pairing_status, complete_device_pairing, initiate_device_pairing, link_device_manually,
    list_pending_pairings, update_last_seen, validate_device_token, verify_device, DeviceInfo,
    DeviceVerified, PairingCompleted, PairingInitiated, PairingStatus, PendingPairing,
};
pub use drive::{
    check_quota as check_drive_quota,
    check_usage_warnings as check_drive_warnings,
    create_folder as create_drive_folder,
    delete_file as delete_drive_file,
    download_file as download_drive_file,
    download_file_stream as download_drive_file_stream,
    download_lake_object,
    empty_trash as empty_drive_trash,
    get_drive_usage,
    get_file_metadata as get_drive_file,
    // Functions
    init_drive_quota,
    is_lake_object_id,
    list_files as list_drive_files,
    list_trash as list_drive_trash,
    move_file as move_drive_file,
    purge_file as purge_drive_file,
    purge_old_trash as purge_old_drive_trash,
    // Quota constants
    quotas as drive_quotas,
    reconcile_usage as reconcile_drive_usage,
    restore_file as restore_drive_file,
    upload_file as upload_drive_file,
    validate_drive_path,
    CreateFolderRequest as DriveCreateFolderRequest,
    DriveConfig,
    // Types
    DriveFile,
    DriveTier,
    DriveUsage,
    MoveFileRequest as DriveMoveFileRequest,
    QuotaWarnings,
    UploadRequest as DriveUploadRequest,
};
pub use entities::{
    create_place, delete_place, get_place, list_places, set_home_place as set_home_place_entity,
    update_place, CreatePlaceRequest, CreatePlaceResponse, Place, UpdatePlaceRequest,
};
pub use exa::{
    search as exa_search, SearchRequest as ExaSearchRequest, SearchResponse as ExaSearchResponse,
};
pub use feedback::{submit_feedback, FeedbackRequest};
pub use jobs::{
    cancel_job, get_job_history, get_job_status, query_jobs, trigger_stream_sync,
    CreateJobResponse, QueryJobsRequest,
};
pub use metrics::{
    get_activity_metrics, ActivityMetrics, JobTypeStats, MetricsSummary, PeriodStats, RecentError,
    StreamStats, TimeWindowMetrics,
};
pub use models::{get_model, list_models, ModelInfo};
pub use oauth::{
    create_source, handle_oauth_callback, initiate_oauth_flow, register_device,
    CreateSourceRequest, OAuthAuthorizeRequest, OAuthAuthorizeResponse, OAuthCallbackParams,
    RegisterDeviceRequest,
};

pub use internal::{
    get_server_status, hydrate_profile, mark_server_ready, seed_dev_server_status,
    HydrateRequest, HydrateResponse, ServerStatus,
};
pub use places::{
    autocomplete, get_place_details, AutocompletePrediction, AutocompleteRequest,
    AutocompleteResponse, PlaceDetailsRequest, PlaceDetailsResponse,
};
pub use plaid::{
    create_link_token, exchange_public_token, get_plaid_accounts, remove_plaid_item,
    CreateLinkTokenRequest, CreateLinkTokenResponse, ExchangeTokenRequest, ExchangeTokenResponse,
    PlaidAccount,
};
pub use pages::{
    create_page, delete_page, get_page, list_pages, search_entities, update_page,
    CreatePageRequest, EntitySearchResponse, EntitySearchResult, Page, PageListResponse,
    PageSummary, UpdatePageRequest,
};
pub use workspaces::{
    create_workspace, delete_workspace, get_workspace, list_workspaces, save_tab_state,
    update_workspace, CreateWorkspaceRequest, SaveTabStateRequest, UpdateWorkspaceRequest,
    Workspace, WorkspaceListResponse, WorkspaceSummary,
};
pub use explorer_nodes::{
    create_node, delete_node, get_node, get_workspace_tree, move_nodes, resolve_view,
    update_node, CreateNodeRequest, ExplorerNode, MoveNodesRequest, ResolveViewRequest,
    TreeNode, UpdateNodeRequest, ViewConfig, ViewEntity, ViewResolutionResponse,
    WorkspaceTreeResponse,
};

pub use profile::{get_display_name, get_profile, update_profile, UpdateProfileRequest};
pub use rate_limit::{
    check_rate_limit, get_usage_stats, record_usage, RateLimitError, RateLimits, TokenUsage,
    UsageStats,
};
pub use registry::{
    get_source_info, get_stream_descriptor, list_all_streams, list_available_sources,
};
pub use seed_testing::{
    get_data_quality_metrics, get_pipeline_status, DataQualityMetrics, PipelineStatus,
};
pub use sources::{
    delete_source, get_source, get_source_status, list_sources, pause_source, resume_source,
};
pub use storage::{get_object_content, list_recent_objects, ObjectContent, StreamObjectSummary};
pub use streams::{
    bulk_update_streams, disable_stream, enable_stream, get_stream_info, list_source_streams,
    update_stream_config, update_stream_schedule, BulkUpdateStreamsRequest,
    BulkUpdateStreamsResponse, EnableStreamRequest, StreamUpdate, UpdateStreamConfigRequest,
    UpdateStreamScheduleRequest,
};
pub use tools::{get_tool, list_tools, ListToolsQuery, Tool};
pub use developer::{execute_sql, list_tables, ExecuteSqlRequest};
pub use session_usage::{
    calculate_cost as calculate_token_cost, check_compaction_needed, get_session_usage,
    record_session_usage, CompactionStatus, SessionUsage, UsageData,
};
pub use token_estimation::{
    estimate_message_tokens, estimate_session_context, estimate_tokens, ContextEstimate,
    ContextStatus,
};
pub use usage::{
    check_and_record_usage, check_limit, get_all_usage, init_limits_from_tier,
    record_usage as record_service_usage, LimitType, RemainingUsage, Service, ServiceUsage,
    UsageLimitError, UsageSummary,
};
pub use wiki::{
    create_citation,
    create_temporal_event,
    delete_auto_events_for_day,
    delete_citation,
    delete_temporal_event,
    // Act operations
    get_act,
    get_act_by_slug,
    // Telos operations
    get_active_telos,
    // Chapter operations
    get_chapter,
    get_chapter_by_slug,
    get_citations,
    get_day_events,
    get_day_sources,
    get_events_by_date,
    // Day operations
    get_or_create_day,
    // Organization operations
    get_organization,
    get_organization_by_slug,
    // Person operations
    get_person,
    get_person_by_slug,
    get_place_by_slug,
    get_telos_by_slug,
    // Thing operations
    get_thing,
    get_thing_by_slug,
    // Place operations (wiki-specific)
    get_wiki_place,
    list_acts,
    list_chapters_for_act,
    list_days,
    list_organizations,
    list_people,
    list_things,
    list_wiki_places,
    resolve_slug,
    update_citation,
    update_day,
    update_organization,
    update_person,
    update_temporal_event,
    update_thing,
    update_wiki_place,
    // Citation types and operations
    Citation,
    CreateCitationRequest,
    CreateTemporalEventRequest,
    // Day sources (ontology records for a day)
    DaySource,
    // Day streams (dynamic ontology queries)
    get_day_streams,
    DayStream,
    DayStreamsResponse,
    StreamRecord,
    // Slug resolution
    SlugResolution,
    // Temporal event types and operations
    TemporalEvent,
    UpdateCitationRequest,
    UpdateTemporalEventRequest,
    UpdateWikiDayRequest,
    UpdateWikiOrganizationRequest,
    // Update requests
    UpdateWikiPersonRequest,
    UpdateWikiPlaceRequest,
    UpdateWikiThingRequest,
    WikiAct,
    WikiChapter,
    WikiDay,
    WikiOrganization,
    WikiOrganizationListItem,
    // Entity types
    WikiPerson,
    WikiPersonListItem,
    WikiPlace,
    WikiPlaceListItem,
    // Narrative types
    WikiTelos,
    WikiThing,
    WikiThingListItem,
};
