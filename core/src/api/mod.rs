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
pub mod chat;
pub mod chats;
pub mod chat_usage;
pub mod code;
pub mod compaction;
pub mod device_pairing;
pub mod drive;
pub mod lake;
pub mod entities;
pub mod exa;
pub mod internal;
pub mod jobs;
pub mod metrics;
pub mod feedback;
pub mod models;
pub mod namespaces;
pub mod oauth;

pub mod ontologies;
pub mod pages;
pub mod places;
pub mod plaid;
pub mod spaces;

pub mod profile;
pub mod rate_limit;
pub mod registry;
pub mod search;
pub mod seed_testing;
pub mod sources;
pub mod storage;
pub mod streams;
pub mod token_estimation;
pub mod tools;
pub mod types;
pub mod unsplash;
pub mod usage;
pub mod validation;
pub mod views;
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
    reconcile_folder as reconcile_drive_folder,
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
pub use unsplash::{
    search as unsplash_search, SearchRequest as UnsplashSearchRequest,
    SearchResponse as UnsplashSearchResponse,
};
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
    // Version history
    create_version, list_versions, get_version,
    CreatePageRequest, EntitySearchResponse, EntitySearchResult, Page, PageListResponse,
    PageSummary, UpdatePageRequest,
    // Version types
    CreateVersionRequest, PageVersionSummary, PageVersionDetail, PageVersionsListResponse,
};
pub use spaces::{
    create_space, delete_space, get_space, list_spaces, save_tab_state,
    update_space, CreateSpaceRequest, SaveTabStateRequest, UpdateSpaceRequest,
    Space, SpaceListResponse, SpaceSummary,
};
pub use chats::{
    append_message, create_chat, create_chat_from_request, delete_chat, generate_title,
    get_chat, list_chats, update_chat_title, update_messages,
    Chat, ChatDetailResponse, ChatListItem, ChatListResponse, ChatMessage,
    ConversationMeta, CreateChatRequest, CreateChatResponse, DeleteChatResponse,
    GenerateTitleRequest, GenerateTitleResponse, IntentMetadata, MessageResponse,
    TimeRange, TitleMessage, ToolCall, UpdateChatResponse, UpdateTitleRequest,
};
pub use namespaces::{
    get_namespace, list_entity_namespaces, list_namespaces, entity_id_to_route,
    extract_namespace_from_entity_id, route_to_entity_id, Namespace, NamespaceListResponse,
};
pub use views::{
    add_item_to_view, create_view, delete_view, get_view, list_views, remove_item_from_view,
    resolve_view, update_view, CreateViewRequest, QueryConfig, UpdateViewRequest, View,
    ViewEntity, ViewListResponse, ViewResolutionResponse, ViewSummary,
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
pub use chat_usage::{
    calculate_cost as calculate_token_cost, check_compaction_needed, get_chat_usage,
    record_chat_usage, CompactionStatus, ChatUsageInfo, UsageData,
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
    // Place operations (wiki-specific)
    get_wiki_place,
    list_acts,
    list_chapters_for_act,
    list_days,
    list_organizations,
    list_people,
    list_wiki_places,
    resolve_slug,
    update_citation,
    update_day,
    update_organization,
    update_person,
    update_temporal_event,
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
};
