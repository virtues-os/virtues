//! Configuration for virtues-api
//!
//! All secrets are injected via environment variables at runtime.
//! The source code contains no secrets.
//!
//! Budget state lives in Postgres (accounts + ledger); the pool is required at
//! boot. Atlas pushes credits + device registrations in via `/internal/*`;
//! virtues-api never calls back — there is no hydration mode.

use anyhow::{bail, Context, Result};

/// Minimum secret length for security (256 bits = 32 bytes)
/// Weak secrets enable brute-force attacks
pub const MIN_SECRET_LENGTH: usize = 32;

/// Where the map files are in S3-compatible object storage. A read is
/// path-style: `<endpoint>/<bucket>/latest.json`, `<endpoint>/<bucket>/<build>/<file>`.
/// It has no `Debug`, so the secret key cannot reach a log through `{:?}`.
#[derive(Clone)]
pub struct MapsBucket {
    /// `VIRTUES_MAPS_S3_ENDPOINT`, e.g. `https://s3.example-region.example.net`.
    /// Always `https://`: the path of a read names a square.
    pub endpoint: String,
    /// `VIRTUES_MAPS_S3_BUCKET`.
    pub bucket: String,
    /// `VIRTUES_MAPS_S3_REGION`, the region name the signature carries.
    pub region: String,
    /// `VIRTUES_MAPS_S3_ACCESS_KEY`. A key that can only read this bucket.
    pub access_key: String,
    /// `VIRTUES_MAPS_S3_SECRET_KEY`.
    pub secret_key: String,
}

const MAPS_S3_VARS: [&str; 5] = [
    "VIRTUES_MAPS_S3_ENDPOINT",
    "VIRTUES_MAPS_S3_BUCKET",
    "VIRTUES_MAPS_S3_REGION",
    "VIRTUES_MAPS_S3_ACCESS_KEY",
    "VIRTUES_MAPS_S3_SECRET_KEY",
];

/// All five variables, or none. Some but not all is a boot error: serving
/// maps from the directory because one name was misspelled would hide the
/// mistake until the directory is gone.
fn maps_bucket_from(var: impl Fn(&str) -> Option<String>) -> Result<Option<MapsBucket>> {
    let values: Vec<Option<String>> =
        MAPS_S3_VARS.iter().map(|name| var(name).map(|v| v.trim().to_string()).filter(|v| !v.is_empty())).collect();
    if values.iter().all(Option::is_none) {
        return Ok(None);
    }
    let missing: Vec<&str> = MAPS_S3_VARS.iter().zip(&values).filter(|(_, v)| v.is_none()).map(|(name, _)| *name).collect();
    if !missing.is_empty() {
        bail!("Map storage is partly configured. Set {} too, or unset every VIRTUES_MAPS_S3_* variable.", missing.join(", "));
    }
    // An env file passes quotes through as part of the value. A quoted or
    // spaced value would boot and then fail every read, so refuse it here.
    for (name, value) in MAPS_S3_VARS.iter().zip(&values) {
        if value.as_deref().is_some_and(|v| v.contains(|c: char| c.is_whitespace() || c == '"' || c == '\'')) {
            bail!("{name} holds a space or a quote. Write the value bare, with no quotes around it.");
        }
    }
    let mut values = values.into_iter().flatten();
    let mut next = || values.next().expect("five values checked above");
    let endpoint = next().trim_end_matches('/').to_string();
    if !endpoint.starts_with("https://") {
        bail!("VIRTUES_MAPS_S3_ENDPOINT must start with https://");
    }
    let bucket = MapsBucket { endpoint, bucket: next(), region: next(), access_key: next(), secret_key: next() };
    if bucket.bucket.is_empty() || !bucket.bucket.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'.') {
        bail!("VIRTUES_MAPS_S3_BUCKET must be a bare bucket name: lowercase letters, digits, hyphens and dots.");
    }
    if !reqwest::Url::parse(&bucket.endpoint).is_ok_and(|u| u.host_str().is_some() && u.path() == "/" && u.query().is_none()) {
        bail!("VIRTUES_MAPS_S3_ENDPOINT must be https://<host> with no path.");
    }
    Ok(Some(bucket))
}

#[derive(Clone)]
pub struct Config {
    /// Port to listen on (default: 9002).
    pub port: u16,

    /// Internal secret for validating requests from Core backend
    /// Sent via X-Internal-Secret header
    pub internal_secret: String,

    // =========================================================================
    // Vercel AI Gateway (Single unified LLM provider)
    // =========================================================================
    /// Vercel AI Gateway API key
    /// Get from: https://vercel.com/ai-gateway
    pub ai_gateway_api_key: String,

    /// Vercel AI Gateway URL (default: https://ai-gateway.vercel.sh)
    pub ai_gateway_url: String,

    // =========================================================================
    // External Service API Keys (All billable services proxied through virtues-api)
    // =========================================================================
    /// Google API key (for Places autocomplete)
    pub google_api_key: Option<String>,

    /// Unsplash API key (for cover image search)
    pub unsplash_access_key: Option<String>,

    // =========================================================================
    // Plaid (bank data). The MASTER Plaid app credentials live ONLY here so the
    // home box never holds them — the box sends per-user `access_token`s and the
    // `/v1/services/plaid/*` proxy injects `client_id`+`secret` server-side.
    // =========================================================================
    /// Plaid app client id (master credential).
    pub plaid_client_id: Option<String>,

    /// Plaid app secret (master credential).
    pub plaid_secret: Option<String>,

    /// Truto API token: Google connections through Truto's verified app
    /// (routes/google.rs). It can reach every connected account, so it stays
    /// here and never goes to a box. Unset turns those routes off.
    pub truto_api_token: Option<String>,

    /// Truto API base URL.
    pub truto_api_url: String,

    /// Plaid API base URL, selected by `PLAID_ENV` (sandbox | development |
    /// production). Default: production. Used by both the `/v1/services/plaid/*`
    /// data proxy and the OAuth link/exchange calls in `routes/oauth.rs`.
    pub plaid_base_url: String,

    /// Where the monthly map cut lands: `<maps_dir>/latest.json` names the
    /// build, `<maps_dir>/<build>/` holds its files (`VIRTUES_MAPS_DIR`,
    /// default `/srv/maps`). See `routes/maps.rs`.
    pub maps_dir: std::path::PathBuf,

    /// Object storage with the same layout as `maps_dir`, set by the five
    /// `VIRTUES_MAPS_S3_*` variables. When it is set, `routes/maps.rs` reads
    /// from it and never from `maps_dir`.
    pub maps_bucket: Option<MapsBucket>,

    // Other OAuth provider credentials (google/notion/strava client_id/secret)
    // are read directly from the environment in `routes/oauth.rs`.
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            port: std::env::var("VIRTUES_API_PORT")
                .unwrap_or_else(|_| "9002".to_string())
                .parse()
                .context("Invalid VIRTUES_API_PORT")?,

            internal_secret: {
                let secret = std::env::var("VIRTUES_API_INTERNAL_SECRET")
                    .context("VIRTUES_API_INTERNAL_SECRET is required")?;
                if secret.len() < MIN_SECRET_LENGTH {
                    bail!(
                        "VIRTUES_API_INTERNAL_SECRET must be at least {} characters (got {})",
                        MIN_SECRET_LENGTH,
                        secret.len()
                    );
                }
                secret
            },

            // Atlas integration (optional)

            // Vercel AI Gateway
            ai_gateway_api_key: std::env::var("AI_GATEWAY_API_KEY")
                .context("AI_GATEWAY_API_KEY is required")?,
            ai_gateway_url: std::env::var("AI_GATEWAY_URL")
                .unwrap_or_else(|_| "https://ai-gateway.vercel.sh".to_string()),

            // External service API keys
            google_api_key: std::env::var("GOOGLE_API_KEY").ok(),
            unsplash_access_key: std::env::var("UNSPLASH_ACCESS_KEY").ok(),

            // Plaid master credentials + environment-selected base URL.
            plaid_client_id: std::env::var("PLAID_CLIENT_ID").ok(),
            plaid_secret: std::env::var("PLAID_SECRET").ok(),
            truto_api_token: std::env::var("TRUTO_API_TOKEN").ok().filter(|t| !t.is_empty()),
            truto_api_url: std::env::var("TRUTO_API_URL")
                .ok()
                .filter(|u| !u.is_empty())
                .unwrap_or_else(|| "https://api.truto.one".to_string()),
            plaid_base_url: plaid_base_url_from_env()?,

            maps_dir: std::env::var("VIRTUES_MAPS_DIR")
                .ok()
                .filter(|d| !d.is_empty())
                .unwrap_or_else(|| "/srv/maps".to_string())
                .into(),
            maps_bucket: maps_bucket_from(|name| std::env::var(name).ok())?,
        })
    }

    /// Check if LLM provider (AI Gateway) is configured
    pub fn has_llm_provider(&self) -> bool {
        !self.ai_gateway_api_key.is_empty()
    }
}

/// Resolve the Plaid API base URL from `PLAID_ENV`. Unset → production. An
/// unrecognized non-empty value is a hard error rather than a silent fallback:
/// this var gates real-money bank calls, so a typo (`sandbox\n`, `Sandbox`,
/// `dev`) must fail boot instead of quietly routing to production.
fn plaid_base_url_from_env() -> Result<String> {
    match std::env::var("PLAID_ENV").ok().as_deref().map(str::trim) {
        None | Some("") | Some("production") => Ok("https://production.plaid.com".to_string()),
        Some("sandbox") => Ok("https://sandbox.plaid.com".to_string()),
        Some("development") => Ok("https://development.plaid.com".to_string()),
        Some(other) => bail!("PLAID_ENV must be sandbox|development|production, got '{other}'"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(set: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let set: Vec<(String, String)> = set.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        move |name| set.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone())
    }

    const ALL: [(&str, &str); 5] = [
        ("VIRTUES_MAPS_S3_ENDPOINT", "https://s3.example.net/"),
        ("VIRTUES_MAPS_S3_BUCKET", "maps"),
        ("VIRTUES_MAPS_S3_REGION", "example-1"),
        ("VIRTUES_MAPS_S3_ACCESS_KEY", "access"),
        ("VIRTUES_MAPS_S3_SECRET_KEY", "secret"),
    ];

    #[test]
    fn no_map_storage_variables_means_the_directory() {
        assert!(maps_bucket_from(vars(&[])).unwrap().is_none());
    }

    #[test]
    fn all_five_variables_make_a_bucket() {
        let b = maps_bucket_from(vars(&ALL)).unwrap().expect("a bucket");
        assert_eq!(b.endpoint, "https://s3.example.net");
        assert_eq!((b.bucket.as_str(), b.region.as_str()), ("maps", "example-1"));
        assert_eq!((b.access_key.as_str(), b.secret_key.as_str()), ("access", "secret"));
    }

    #[test]
    fn a_missing_variable_fails_and_is_named() {
        let err = maps_bucket_from(vars(&ALL[..4])).err().expect("an error").to_string();
        assert!(err.contains("VIRTUES_MAPS_S3_SECRET_KEY"), "{err}");
        let err = maps_bucket_from(vars(&[ALL[0], ("VIRTUES_MAPS_S3_BUCKET", ""), ALL[2], ALL[3], ALL[4]])).err().expect("an error").to_string();
        assert!(err.contains("VIRTUES_MAPS_S3_BUCKET"), "{err}");
    }

    #[test]
    fn values_are_trimmed_and_cannot_hold_quotes_or_spaces() {
        let mut set = ALL;
        set[4].1 = " secret\n";
        assert_eq!(maps_bucket_from(vars(&set)).unwrap().expect("a bucket").secret_key, "secret");
        for (i, bad) in [(4, "\"secret\""), (3, "'access'"), (1, "my maps")] {
            let mut set = ALL;
            set[i].1 = bad;
            let err = maps_bucket_from(vars(&set)).err().expect("an error").to_string();
            assert!(err.contains(ALL[i].0), "{err}");
        }
    }

    #[test]
    fn the_endpoint_and_bucket_must_make_a_url() {
        for (i, bad) in [(0, "https://"), (0, "https://s3.example.net/some/path"), (0, "https://s3.example.net?x=1"), (1, "maps/other"), (1, "maps?x=1"), (1, "Maps")] {
            let mut set = ALL;
            set[i].1 = bad;
            assert!(maps_bucket_from(vars(&set)).is_err(), "{bad}");
        }
    }

    #[test]
    fn the_endpoint_must_be_https() {
        for bad in ["s3.example.net", "http://s3.example.net"] {
            let mut set = ALL;
            set[0].1 = bad;
            assert!(maps_bucket_from(vars(&set)).is_err(), "{bad}");
        }
    }
}
