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
//! - `praxis` - Temporal pursuits management (tasks/initiatives/aspirations)
//! - `axiology` - Value system management (values/telos/virtues/vices/habits/temperaments/preferences)
//! - `rate_limit` - API usage tracking and rate limiting
//! - `models` - LLM model configurations
//! - `agents` - AI agent configurations
//! - `seed_testing` - Seed data pipeline validation and inspection
//! - `metrics` - Activity metrics and job statistics

pub mod agents;
pub mod assistant_profile;
pub mod axiology;
pub mod device_pairing;
pub mod entities;
pub mod exa;
pub mod jobs;
pub mod metrics;
pub mod models;
pub mod oauth;
pub mod onboarding;
pub mod ontologies;
pub mod places;
pub mod plaid;
pub mod praxis;
pub mod profile;
pub mod rate_limit;
pub mod registry;
pub mod search;
pub mod seed_testing;
pub mod sources;
pub mod storage;
pub mod streams;
pub mod timeline;
pub mod tools;
pub mod types;
pub mod usage;
pub mod validation;

// Re-export commonly used types
pub use streams::StreamConnection;
pub use types::{SourceConnection, SourceConnectionStatus};

// Re-export all functions for convenience
pub use agents::{get_agent, list_agents, AgentInfo};
pub use assistant_profile::{
    get_assistant_name, get_assistant_profile, update_assistant_profile,
    UpdateAssistantProfileRequest,
};
pub use axiology::{
    create_preference, create_telos, create_temperament, create_vice, create_virtue,
    delete_preference, delete_telos, delete_temperament, delete_vice, delete_virtue,
    get_preference, get_telos, get_temperament, get_vice, get_virtue, list_preferences,
    list_telos, list_temperaments, list_vices, list_virtues, update_preference,
    update_telos, update_temperament, update_vice, update_virtue,
    CreatePreferenceRequest, CreateSimpleRequest, Preference, Telos, Temperament,
    UpdatePreferenceRequest, UpdateSimpleRequest, Vice, Virtue,
};
pub use device_pairing::{
    check_pairing_status, complete_device_pairing, initiate_device_pairing, list_pending_pairings,
    update_last_seen, validate_device_token, verify_device, DeviceInfo, DeviceVerified,
    PairingCompleted, PairingInitiated, PairingStatus, PendingPairing,
};
pub use jobs::{
    cancel_job, get_job_history, get_job_status, query_jobs, trigger_stream_sync,
    CreateJobResponse, QueryJobsRequest,
};
pub use models::{get_model, list_models, ModelInfo};
pub use oauth::{
    create_source, handle_oauth_callback, initiate_oauth_flow, register_device,
    CreateSourceRequest, OAuthAuthorizeRequest, OAuthAuthorizeResponse, OAuthCallbackParams,
    RegisterDeviceRequest,
};
pub use praxis::{
    create_aspiration, create_initiative, create_task, delete_aspiration, delete_initiative,
    delete_task, get_aspiration, get_initiative, get_task, list_aspirations, list_initiatives,
    list_tags, list_tasks, update_aspiration, update_initiative, update_task, Aspiration,
    CreateAspirationRequest, CreateTaskRequest, Initiative, Task, UpdateAspirationRequest,
    UpdateTaskRequest,
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
    get_chunks_summary, get_data_quality_metrics, get_pipeline_status, ChunksSummary,
    DataQualityMetrics, PipelineStatus,
};
pub use metrics::{
    get_activity_metrics, ActivityMetrics, JobTypeStats, MetricsSummary, PeriodStats,
    RecentError, StreamStats, TimeWindowMetrics,
};
pub use sources::{
    delete_source, get_source, get_source_status, list_sources, pause_source, resume_source,
};
pub use streams::{
    disable_stream, enable_stream, get_stream_info, list_source_streams, update_stream_config,
    update_stream_schedule, EnableStreamRequest, UpdateStreamConfigRequest,
    UpdateStreamScheduleRequest,
};
pub use timeline::{
    get_day_view, AttachedCalendarEvent, AttachedEmail, AttachedHealthEvent, AttachedMessage,
    AttachedTranscript, Chunk, DayView, LocationChunk, MissingDataChunk, TransitChunk,
};
pub use tools::{get_tool, list_tools, update_tool, ListToolsQuery, Tool, UpdateToolRequest};
pub use plaid::{
    create_link_token, exchange_public_token, get_plaid_accounts, remove_plaid_item,
    CreateLinkTokenRequest, CreateLinkTokenResponse, ExchangeTokenRequest, ExchangeTokenResponse,
    PlaidAccount,
};
pub use onboarding::{
    complete_onboarding, complete_step, get_onboarding_status, save_onboarding_aspirations,
    save_onboarding_axiology, skip_step, ExtractedAxiologyItem, OnboardingAspiration,
    OnboardingStatus, OnboardingStep, SaveAspirationsRequest, SaveAspirationsResponse,
    SaveAxiologyRequest, SaveAxiologyResponse,
};
pub use places::{
    autocomplete, get_place_details, AutocompleteRequest, AutocompletePrediction,
    AutocompleteResponse, PlaceDetailsRequest, PlaceDetailsResponse,
};
pub use exa::{search as exa_search, SearchRequest as ExaSearchRequest, SearchResponse as ExaSearchResponse};
pub use usage::{
    check_and_record_usage, check_limit, get_all_usage, init_limits_from_tier,
    record_usage as record_service_usage, LimitType, RemainingUsage, Service, ServiceUsage,
    UsageLimitError, UsageSummary,
};
pub use storage::{
    get_object_content, list_recent_objects, ObjectContent, StreamObjectSummary,
};
pub use entities::{
    create_place, delete_place, get_place, list_places, set_home_place as set_home_place_entity,
    update_place, CreatePlaceRequest, CreatePlaceResponse, Place, UpdatePlaceRequest,
};
