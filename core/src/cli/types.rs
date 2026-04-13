//! CLI argument types and command structures

use clap::{Parser, Subcommand};

/// Default port: reads NOMAD_PORT_http env var (Nomad host networking),
/// falling back to 8000 for local development.
fn default_port() -> u16 {
    std::env::var("NOMAD_PORT_http")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(8000)
}

#[derive(Parser)]
#[command(name = "virtues")]
#[command(version, about = "Virtues personal data platform CLI", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Interactive setup wizard
    Init,

    /// Run database migrations
    Migrate,

    /// Start the HTTP server
    Server {
        /// Host to bind to
        #[arg(long, default_value = "0.0.0.0")]
        host: String,

        /// Port to bind to (defaults to NOMAD_PORT_http env var, or 8000)
        #[arg(long, default_value_t = default_port())]
        port: u16,
    },

    /// Seed the database with demo data (people, places, events, etc.)
    Seed,

    /// Start server with Cloudflare Tunnel (for iOS/Mac development)
    Tunnel,

    /// Pre-download ML models (embedding, etc.) for offline/Docker use
    WarmModels,

    /// Compute novelty scores for all days with events
    ComputeNovelty,

    /// Compute autonomic z-scores for all days with avg_hr data
    ComputeAutonomic,

    /// Pair an iOS device manually (dev shortcut — bypasses web UI auth)
    ///
    /// Calls `link_device_manually()` directly, which writes to both
    /// `elt_source_connections` and `action_credentials`, then seeds the
    /// six iOS `app_actions` rows. Idempotent — re-running with the same
    /// device id is safe.
    PairIos {
        /// Device UUID from the iOS app's Settings → Device Identity
        device_id: String,

        /// Friendly name for the device
        #[arg(long, default_value = "iPhone")]
        name: String,
    },

    /// Diagnose token encryption: pull stored device tokens, try to decrypt
    /// them with the current `VIRTUES_ENCRYPTION_KEY`, and report what happens
    /// for each. Pass an optional bearer token to compare against the
    /// decrypted plaintext.
    VerifyTokens {
        /// Optional bearer token (raw, no "Bearer " prefix) to match against
        bearer: Option<String>,
    },

    /// Generate the day summary (autobiography + 24h event timeline) for a date.
    ///
    /// Calls `api::day_summary::generate_day_summary`, which gathers the day's
    /// ontology data, prompts the user's chat model via Tollbooth, and writes
    /// the results to `wiki_days` (autobiography/epigraph/data_quality) and
    /// `wiki_events` (clearing existing auto events first; manual events are
    /// preserved). Gaps in the LLM-emitted timeline are backfilled as "Unknown"
    /// to guarantee 00:00–24:00 coverage.
    DaySummary {
        /// Date to summarize (YYYY-MM-DD). Defaults to today in the user's
        /// profile timezone (or local time if no timezone is set).
        #[arg(long)]
        date: Option<String>,
    },

    /// Run entity resolution (places + people) over the last N hours.
    ///
    /// This is the bridge while the legacy transform-chaining pipeline still
    /// owns clustering. The new actions path (ios_location, etc.) writes
    /// `data_location_point` rows but doesn't chain into place resolution,
    /// so visits don't get created. Use this to manually backfill.
    ResolveEntities {
        /// Lookback window in hours (default: 24)
        #[arg(long, default_value_t = 24)]
        hours: i64,
    },
}
