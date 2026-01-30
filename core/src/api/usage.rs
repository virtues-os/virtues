//! Usage metering and rate limiting for external API services
//!
//! Tracks monthly usage against configurable limits for:
//! - AI Gateway (tokens)
//! - Google Places (requests)
//! - Exa Search (requests)

use chrono::{Datelike, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use thiserror::Error;
use uuid::Uuid;

use crate::types::Timestamp;

/// Services that are tracked for usage
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Service {
    AiGateway,
    GooglePlaces,
    Exa,
    Unsplash,
}

impl Service {
    pub fn as_str(&self) -> &'static str {
        match self {
            Service::AiGateway => "ai_gateway",
            Service::GooglePlaces => "google_places",
            Service::Exa => "exa",
            Service::Unsplash => "unsplash",
        }
    }

    pub fn all() -> &'static [Service] {
        &[Service::AiGateway, Service::GooglePlaces, Service::Exa, Service::Unsplash]
    }
}

impl std::fmt::Display for Service {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Error returned when usage limit is exceeded
#[derive(Debug, Error)]
#[error("Usage limit exceeded for {service}: {used}/{limit} {unit}. Resets at {resets_at}")]
pub struct UsageLimitError {
    pub service: String,
    pub used: i64,
    pub limit: i64,
    pub unit: String,
    pub resets_at: Timestamp,
}

/// Usage information for a single service
#[derive(Debug, Clone, Serialize)]
pub struct ServiceUsage {
    pub used: i64,
    pub limit: i64,
    pub unit: String,
    pub limit_type: String,
}

/// Summary of all service usage
#[derive(Debug, Clone, Serialize)]
pub struct UsageSummary {
    pub period: String,
    pub tier: String,
    pub services: std::collections::HashMap<String, ServiceUsage>,
    pub resets_at: Timestamp,
}

/// Remaining usage after a limit check
#[derive(Debug, Clone, Serialize)]
pub struct RemainingUsage {
    pub allowed: bool,
    pub remaining: i64,
    /// True if usage exceeds the limit (only possible for soft limits)
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub over_limit: bool,
}

/// Type of limit enforcement
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LimitType {
    /// Hard limit - blocks requests when exceeded
    Hard,
    /// Soft limit - warns but allows requests when exceeded
    Soft,
}

impl LimitType {
    pub fn as_str(&self) -> &'static str {
        match self {
            LimitType::Hard => "hard",
            LimitType::Soft => "soft",
        }
    }
}

/// Tier configuration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    Starter,
    Pro,
}

impl Tier {
    pub fn from_env() -> Self {
        match std::env::var("TIER").as_deref() {
            Ok("pro") | Ok("Pro") | Ok("PRO") => Tier::Pro,
            _ => Tier::Starter,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Tier::Starter => "starter",
            Tier::Pro => "pro",
        }
    }

    /// Get the monthly limit for a service based on tier
    pub fn limit_for(&self, service: Service) -> i64 {
        match (self, service) {
            // Starter tier
            (Tier::Starter, Service::AiGateway) => 1_000_000,
            (Tier::Starter, Service::GooglePlaces) => 1_000,
            (Tier::Starter, Service::Exa) => 1_000,
            (Tier::Starter, Service::Unsplash) => 1_000,
            // Pro tier
            (Tier::Pro, Service::AiGateway) => 5_000_000,
            (Tier::Pro, Service::GooglePlaces) => 5_000,
            (Tier::Pro, Service::Exa) => 5_000,
            (Tier::Pro, Service::Unsplash) => 5_000,
        }
    }

    /// Get the limit type for a service
    /// AI Gateway is hard-limited (expensive), others are soft-limited
    pub fn limit_type_for(&self, service: Service) -> LimitType {
        match service {
            Service::AiGateway => LimitType::Hard,
            _ => LimitType::Soft,
        }
    }
}

/// Get the current month period string (e.g., "2025-01")
fn current_period() -> String {
    let now = Utc::now();
    format!("{}-{:02}", now.year(), now.month())
}

/// Get the first day of current month
fn first_of_month() -> NaiveDate {
    let now = Utc::now();
    NaiveDate::from_ymd_opt(now.year(), now.month(), 1).unwrap()
}

/// Get the first day of next month (reset time)
fn first_of_next_month() -> Timestamp {
    let now = Utc::now();
    let (year, month) = if now.month() == 12 {
        (now.year() + 1, 1)
    } else {
        (now.year(), now.month() + 1)
    };
    let date = NaiveDate::from_ymd_opt(year, month, 1).unwrap();
    let datetime = date.and_hms_opt(0, 0, 0).unwrap().and_utc();
    Timestamp::from(datetime)
}

/// Initialize usage limits from TIER environment variable
///
/// Updates the limits table with tier-appropriate values
pub async fn init_limits_from_tier(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let tier = Tier::from_env();
    tracing::info!("Initializing usage limits for tier: {}", tier.as_str());

    for service in Service::all() {
        let limit = tier.limit_for(*service);
        let limit_type = tier.limit_type_for(*service);
        let unit = match service {
            Service::AiGateway => "tokens",
            _ => "requests",
        };

        sqlx::query(
            r#"
            INSERT INTO app_usage_limits (service, monthly_limit, unit, limit_type, updated_at)
            VALUES ($1, $2, $3, $4, datetime('now'))
            ON CONFLICT (service) DO UPDATE SET
                monthly_limit = $2,
                unit = $3,
                limit_type = $4,
                updated_at = datetime('now')
            "#,
        )
        .bind(service.as_str())
        .bind(limit)
        .bind(unit)
        .bind(limit_type.as_str())
        .execute(pool)
        .await?;
    }

    Ok(())
}

/// Get current monthly usage for a service
pub async fn get_monthly_usage(pool: &SqlitePool, service: Service) -> Result<i64, sqlx::Error> {
    let first_day = first_of_month();

    let result = sqlx::query_scalar::<_, Option<i64>>(
        r#"
        SELECT COALESCE(SUM(
            CASE
                WHEN $1 = 'ai_gateway' THEN token_count
                ELSE request_count
            END
        ), 0)
        FROM app_api_usage
        WHERE endpoint = $1
          AND day_bucket >= $2
        "#,
    )
    .bind(service.as_str())
    .bind(first_day)
    .fetch_one(pool)
    .await?;

    Ok(result.unwrap_or(0))
}

/// Get the configured limit for a service (limit, unit, limit_type)
async fn get_limit(
    pool: &SqlitePool,
    service: Service,
) -> Result<(i64, String, LimitType), sqlx::Error> {
    let result = sqlx::query_as::<_, (i64, String, String)>(
        r#"
        SELECT monthly_limit, unit, limit_type
        FROM app_usage_limits
        WHERE service = $1 AND enabled = TRUE
        "#,
    )
    .bind(service.as_str())
    .fetch_optional(pool)
    .await?;

    // Fall back to tier defaults if not in database
    match result {
        Some((limit, unit, limit_type_str)) => {
            let limit_type = match limit_type_str.as_str() {
                "soft" => LimitType::Soft,
                _ => LimitType::Hard,
            };
            Ok((limit, unit, limit_type))
        }
        None => {
            let tier = Tier::from_env();
            let unit = match service {
                Service::AiGateway => "tokens".to_string(),
                _ => "requests".to_string(),
            };
            Ok((tier.limit_for(service), unit, tier.limit_type_for(service)))
        }
    }
}

/// Check if usage is within limits (read-only check)
///
/// Returns remaining usage or error if hard limit exceeded.
/// For soft limits, returns success with `over_limit: true` when exceeded.
/// NOTE: This is a read-only check. For atomic check-and-increment, use `check_and_record_usage`.
pub async fn check_limit(
    pool: &SqlitePool,
    service: Service,
) -> Result<RemainingUsage, UsageLimitError> {
    let used = get_monthly_usage(pool, service).await.map_err(|e| {
        tracing::error!("Failed to get usage for {}: {}", service, e);
        UsageLimitError {
            service: service.to_string(),
            used: 0,
            limit: 0,
            unit: "unknown".to_string(),
            resets_at: first_of_next_month(),
        }
    })?;

    let (limit, unit, limit_type) = get_limit(pool, service).await.map_err(|e| {
        tracing::error!("Failed to get limit for {}: {}", service, e);
        UsageLimitError {
            service: service.to_string(),
            used,
            limit: 0,
            unit: "unknown".to_string(),
            resets_at: first_of_next_month(),
        }
    })?;

    if used >= limit {
        match limit_type {
            LimitType::Hard => {
                // Hard limit - block the request
                return Err(UsageLimitError {
                    service: service.to_string(),
                    used,
                    limit,
                    unit,
                    resets_at: first_of_next_month(),
                });
            }
            LimitType::Soft => {
                // Soft limit - warn but allow
                tracing::warn!(
                    service = %service,
                    used = used,
                    limit = limit,
                    "Soft limit exceeded - allowing request but usage is over budget"
                );
                return Ok(RemainingUsage {
                    allowed: true,
                    remaining: limit - used, // Will be negative
                    over_limit: true,
                });
            }
        }
    }

    Ok(RemainingUsage {
        allowed: true,
        remaining: limit - used,
        over_limit: false,
    })
}

/// Atomically check limit and record usage in a single transaction
///
/// This prevents race conditions where multiple concurrent requests could all pass
/// the limit check before any records usage. Returns the remaining usage after recording,
/// or an error if a hard limit would be exceeded.
/// For soft limits, always records usage and returns success with `over_limit: true`.
pub async fn check_and_record_usage(
    pool: &SqlitePool,
    service: Service,
    units: i64,
) -> Result<RemainingUsage, UsageLimitError> {
    let today = Utc::now().date_naive();
    let first_day = first_of_month();
    let (limit, unit, limit_type) = get_limit(pool, service).await.map_err(|e| {
        tracing::error!("Failed to get limit for {}: {}", service, e);
        UsageLimitError {
            service: service.to_string(),
            used: 0,
            limit: 0,
            unit: "unknown".to_string(),
            resets_at: first_of_next_month(),
        }
    })?;

    // For AI gateway, units are tokens; for others, units are requests
    let (request_delta, token_delta) = match service {
        Service::AiGateway => (1i64, units),
        _ => (units, 0i64),
    };

    // For soft limits, always record usage regardless of limit
    // For hard limits, only record if within limit
    let should_enforce = limit_type == LimitType::Hard;

    // Atomic check-and-increment using a single query with RETURNING
    // This query:
    // 1. Calculates current monthly usage
    // 2. Checks if adding new usage would exceed limit
    // 3. If within limit OR soft limit, inserts/updates the usage record
    // 4. Returns the new total and whether the operation succeeded
    let result = sqlx::query_as::<_, (i64, bool)>(
        r#"
        WITH current_usage AS (
            SELECT COALESCE(SUM(
                CASE
                    WHEN $1 = 'ai_gateway' THEN token_count
                    ELSE request_count
                END
            ), 0) as total
            FROM app_api_usage
            WHERE endpoint = $1
              AND day_bucket >= $6
        ),
        new_usage AS (
            SELECT
                total + $5 as projected_total,
                total + $5 <= $7 as within_limit
            FROM current_usage
        ),
        upsert AS (
            INSERT INTO app_api_usage (id, endpoint, day_bucket, request_count, token_count)
            SELECT lower(hex(randomblob(4)) || '-' || hex(randomblob(2)) || '-4' || substr(hex(randomblob(2)),2) || '-' || substr('89ab',abs(random()) % 4 + 1, 1) || substr(hex(randomblob(2)),2) || '-' || hex(randomblob(6))), $1, $2, $3, $4
            FROM new_usage
            WHERE within_limit = true OR $8 = false
            ON CONFLICT (endpoint, day_bucket) DO UPDATE SET
                request_count = app_api_usage.request_count + $3,
                token_count = app_api_usage.token_count + $4,
                updated_at = datetime('now')
            RETURNING 1
        )
        SELECT
            projected_total,
            within_limit
        FROM new_usage
        "#,
    )
    .bind(service.as_str()) // $1: endpoint
    .bind(today) // $2: day_bucket
    .bind(request_delta) // $3: request_count delta
    .bind(token_delta) // $4: token_count delta
    .bind(if service == Service::AiGateway {
        units
    } else {
        units
    }) // $5: units to check
    .bind(first_day) // $6: first day of month
    .bind(limit) // $7: limit
    .bind(should_enforce) // $8: whether to enforce limit
    .fetch_one(pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to check and record usage for {}: {}", service, e);
        UsageLimitError {
            service: service.to_string(),
            used: 0,
            limit,
            unit: unit.clone(),
            resets_at: first_of_next_month(),
        }
    })?;

    let (projected_total, within_limit) = result;

    if !within_limit {
        match limit_type {
            LimitType::Hard => {
                return Err(UsageLimitError {
                    service: service.to_string(),
                    used: projected_total - units, // Current usage before this request
                    limit,
                    unit,
                    resets_at: first_of_next_month(),
                });
            }
            LimitType::Soft => {
                tracing::warn!(
                    service = %service,
                    used = projected_total,
                    limit = limit,
                    "Soft limit exceeded - recorded usage but over budget"
                );
                return Ok(RemainingUsage {
                    allowed: true,
                    remaining: limit - projected_total, // Will be negative
                    over_limit: true,
                });
            }
        }
    }

    Ok(RemainingUsage {
        allowed: true,
        remaining: limit - projected_total,
        over_limit: false,
    })
}

/// Record usage for a service (without limit check)
///
/// Increments the usage counter in daily buckets.
/// NOTE: Prefer `check_and_record_usage` for atomic limit checking.
/// This function is useful when you want to record usage after an operation
/// completes successfully, without pre-checking limits.
pub async fn record_usage(
    pool: &SqlitePool,
    service: Service,
    units: i64,
) -> Result<(), sqlx::Error> {
    let today = Utc::now().date_naive();

    // For AI gateway, units are tokens; for others, units are requests
    let (request_delta, token_delta): (i64, i64) = match service {
        Service::AiGateway => (1, units), // 1 request, N tokens
        _ => (units, 0),                  // N requests, 0 tokens
    };

    let usage_id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        INSERT INTO app_api_usage (id, endpoint, day_bucket, request_count, token_count)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (endpoint, day_bucket) DO UPDATE SET
            request_count = app_api_usage.request_count + $4,
            token_count = app_api_usage.token_count + $5,
            updated_at = datetime('now')
        "#,
    )
    .bind(&usage_id)
    .bind(service.as_str())
    .bind(today)
    .bind(request_delta)
    .bind(token_delta)
    .execute(pool)
    .await?;

    Ok(())
}

/// Get usage summary for all services
pub async fn get_all_usage(pool: &SqlitePool) -> Result<UsageSummary, sqlx::Error> {
    let tier = Tier::from_env();
    let mut services = std::collections::HashMap::new();

    for service in Service::all() {
        let used = get_monthly_usage(pool, *service).await?;
        let (limit, unit, limit_type) = get_limit(pool, *service).await?;

        services.insert(
            service.as_str().to_string(),
            ServiceUsage {
                used,
                limit,
                unit,
                limit_type: limit_type.as_str().to_string(),
            },
        );
    }

    Ok(UsageSummary {
        period: current_period(),
        tier: tier.as_str().to_string(),
        services,
        resets_at: first_of_next_month(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tier_from_env() {
        // Default is starter
        std::env::remove_var("TIER");
        assert_eq!(Tier::from_env(), Tier::Starter);

        std::env::set_var("TIER", "pro");
        assert_eq!(Tier::from_env(), Tier::Pro);

        std::env::set_var("TIER", "starter");
        assert_eq!(Tier::from_env(), Tier::Starter);
    }

    #[test]
    fn test_tier_limits() {
        assert_eq!(Tier::Starter.limit_for(Service::AiGateway), 1_000_000);
        assert_eq!(Tier::Pro.limit_for(Service::AiGateway), 5_000_000);
        assert_eq!(Tier::Starter.limit_for(Service::Exa), 1_000);
        assert_eq!(Tier::Pro.limit_for(Service::Exa), 5_000);
    }

    #[test]
    fn test_service_as_str() {
        assert_eq!(Service::AiGateway.as_str(), "ai_gateway");
        assert_eq!(Service::GooglePlaces.as_str(), "google_places");
        assert_eq!(Service::Exa.as_str(), "exa");
    }

    #[test]
    fn test_current_period() {
        let period = current_period();
        assert!(period.len() == 7); // "YYYY-MM"
        assert!(period.contains('-'));
    }
}
