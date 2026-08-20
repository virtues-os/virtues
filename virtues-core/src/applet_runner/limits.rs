//! Per-applet limits — declared in `config.limits`, enforced here.
//!
//! The manifest speaks in the units a person writes: **dollars** for money
//! (`max_llm_cost = 0.25`), **seconds** for time, plain counts for runs.
//! Everything is converted once, on the way in, so the rest of the runner
//! only ever sees micros-USD and `Duration`.
//!
//! Two rules the plan is explicit about, both encoded here:
//!
//! - **Enforce outside the model.** Nothing in this module asks the LLM
//!   whether it is over budget; the cap is checked against the gateway's
//!   authoritative `usage.cost`, recorded in `app_ai_calls`.
//! - **Protective defaults, never locks.** Every field is optional and every
//!   absent field means *no limit*. A cap exists because the owner wrote one.
//!
//! Spend is read two ways, because the two ceilings answer different
//! questions. The per-run ceiling ("this one run must not run away") is
//! tracked in memory as `Usage` events arrive, so it can stop a loop
//! mid-flight. The per-day ceiling ("this applet must not cost more than X a
//! day") is a pre-run question answered from `app_ai_calls` history.

use sqlx::PgPool;

use crate::error::Result;

/// Global subprocess ceiling when a manifest declares none. Generous enough
/// for the largest legitimate batch, short enough that a wedged process frees
/// the per-applet run lock instead of blocking it until the box restarts.
pub const DEFAULT_SUBPROCESS_TIMEOUT: std::time::Duration =
    std::time::Duration::from_secs(300);

/// The caps an applet declares. Every field optional; `None` means no limit.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Limits {
    /// Ceiling on LLM spend within a single run, in micros-USD.
    pub max_llm_cost_micros: Option<i64>,
    /// Ceiling on LLM spend by this applet across a rolling 24 hours.
    pub max_llm_cost_per_day_micros: Option<i64>,
    /// Ceiling on run attempts in a rolling hour.
    pub max_runs_per_hour: Option<i64>,
    /// Ceiling on run attempts in a rolling 24 hours.
    pub max_runs_per_day: Option<i64>,
    /// Wall-clock ceiling on the subprocess phase.
    pub timeout: Option<std::time::Duration>,
}

impl Limits {
    /// Read `config.limits`. Unknown keys are ignored rather than rejected —
    /// a manifest from a newer box must not fail to load on an older one.
    pub fn from_config(config: &serde_json::Value) -> Self {
        let Some(l) = config.get("limits") else {
            return Self::default();
        };

        // Money arrives as dollars (`0.25`), which is what a person writes and
        // what the preview gate displays. Integers are accepted too, so
        // `max_llm_cost = 1` means a dollar, not a micro.
        let dollars_to_micros = |key: &str| -> Option<i64> {
            l.get(key)
                .and_then(|v| v.as_f64())
                .filter(|d| d.is_finite() && *d >= 0.0)
                .map(|d| (d * 1_000_000.0).round() as i64)
        };
        let count = |key: &str| -> Option<i64> {
            l.get(key).and_then(|v| v.as_i64()).filter(|n| *n >= 0)
        };

        Self {
            max_llm_cost_micros: dollars_to_micros("max_llm_cost"),
            max_llm_cost_per_day_micros: dollars_to_micros("max_llm_cost_per_day"),
            max_runs_per_hour: count("max_runs_per_hour"),
            // A bare `max_runs` is the spelling AGENTS.md advertises and the
            // one a person reaches for. Read it as a daily cap — the window
            // someone means when they write "at most 50 runs".
            max_runs_per_day: count("max_runs_per_day").or_else(|| count("max_runs")),
            // `timeout_s` is what the shipped manifests carry and what this
            // has always read. `timeout` is what the tool schema has been
            // telling the model to write — so anything chat-authored used the
            // spelling nothing enforced. Both are accepted; `timeout_s` wins.
            timeout: l
                .get("timeout_s")
                .or_else(|| l.get("timeout"))
                .and_then(|v| v.as_u64())
                .map(std::time::Duration::from_secs),
        }
    }

    /// The declared caps, in the words the preview gate and the detail page
    /// show them. Empty when the applet declares none.
    pub fn describe(&self) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(c) = self.max_llm_cost_micros {
            out.push(format!("at most {} of model spend per run", format_usd(c)));
        }
        if let Some(c) = self.max_llm_cost_per_day_micros {
            out.push(format!("at most {} of model spend per day", format_usd(c)));
        }
        if let Some(n) = self.max_runs_per_hour {
            out.push(format!("at most {n} runs an hour"));
        }
        if let Some(n) = self.max_runs_per_day {
            out.push(format!("at most {n} runs a day"));
        }
        if let Some(t) = self.timeout {
            out.push(format!("stops after {}s", t.as_secs()));
        }
        out
    }

    /// The subprocess ceiling, or the global default.
    pub fn subprocess_timeout(&self) -> std::time::Duration {
        self.timeout.unwrap_or(DEFAULT_SUBPROCESS_TIMEOUT)
    }

    /// True when this applet declares any LLM spend ceiling at all. Lets the
    /// agent phase skip the bookkeeping entirely for the common uncapped case.
    pub fn caps_llm_spend(&self) -> bool {
        self.max_llm_cost_micros.is_some() || self.max_llm_cost_per_day_micros.is_some()
    }
}

/// Format micros-USD the way both the run log and the gate show it.
pub fn format_usd(micros: i64) -> String {
    format!("${:.2}", micros as f64 / 1_000_000.0)
}

/// LLM spend by this applet over the last 24 hours, in micros-USD.
///
/// Joins `app_ai_calls` to the applet through the run that spent it. Runs are
/// never pruned, so this window is always complete.
pub async fn spend_micros_last_day(db: &PgPool, applet_id: &str) -> Result<i64> {
    let total: Option<i64> = sqlx::query_scalar(
        r#"SELECT COALESCE(SUM(c.cost_micros), 0)
             FROM app_ai_calls c
             JOIN app_applet_runs r ON r.id = c.applet_run_id
            WHERE r.applet_id = $1
              AND c.created_at > now() - interval '24 hours'"#,
    )
    .bind(applet_id)
    .fetch_one(db)
    .await?;
    Ok(total.unwrap_or(0))
}

/// LLM spend recorded against one run, in micros-USD. Used by the detail page
/// to show what a run actually cost; the live cap is tracked in memory.
pub async fn spend_micros_for_run(db: &PgPool, run_id: &str) -> Result<i64> {
    let total: Option<i64> = sqlx::query_scalar(
        "SELECT COALESCE(SUM(cost_micros), 0) FROM app_ai_calls WHERE applet_run_id = $1",
    )
    .bind(run_id)
    .fetch_one(db)
    .await?;
    Ok(total.unwrap_or(0))
}

/// Run attempts by this applet inside a rolling window.
///
/// Counts every row that represents an *attempt the applet made* — success,
/// error, and budget_exceeded. `skipped` is excluded deliberately: a run the
/// gate turned away never consumed anything, and counting it would let a
/// falsy condition on a two-minute poll exhaust a daily cap by lunchtime.
pub async fn run_count_within(db: &PgPool, applet_id: &str, hours: i32) -> Result<i64> {
    let n: Option<i64> = sqlx::query_scalar(
        r#"SELECT COUNT(*)
             FROM app_applet_runs
            WHERE applet_id = $1
              AND status <> 'skipped'
              AND started_at > now() - make_interval(hours => $2)"#,
    )
    .bind(applet_id)
    .bind(hours)
    .fetch_one(db)
    .await?;
    Ok(n.unwrap_or(0))
}

/// Why a run was refused before it started. Carries its own sentence so the
/// run row explains itself in the log without the caller reassembling it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// A rate cap was already met. Recorded `skipped` — nothing was spent and
    /// nothing is broken; the next window lets it through.
    RateLimited(String),
    /// The daily spend ceiling was already met. Recorded `budget_exceeded`.
    OverDailyBudget(String),
}

impl Refusal {
    /// The `app_applet_runs.status` this refusal is recorded as.
    pub fn status(&self) -> &'static str {
        match self {
            Refusal::RateLimited(_) => "skipped",
            Refusal::OverDailyBudget(_) => "budget_exceeded",
        }
    }

    pub fn message(&self) -> &str {
        match self {
            Refusal::RateLimited(m) | Refusal::OverDailyBudget(m) => m,
        }
    }
}

/// Check the pre-run ceilings. `Ok(None)` means the run may proceed.
///
/// A **manual** trigger is exempt from rate caps and only from rate caps: the
/// caps exist to bound automation, and refusing a person who just pressed "Run
/// now" because a cron had a busy hour is the limit behaving as a lock. Spend
/// ceilings are not exempt — those bound real money, and the wallet does not
/// care who pressed the button.
pub async fn check_pre_run(
    db: &PgPool,
    applet_id: &str,
    limits: &Limits,
    trigger: &str,
) -> Result<Option<Refusal>> {
    if trigger != "manual" {
        if let Some(cap) = limits.max_runs_per_hour {
            let used = run_count_within(db, applet_id, 1).await?;
            if used >= cap {
                return Ok(Some(Refusal::RateLimited(format!(
                    "rate limit reached — {used} of {cap} runs allowed per hour"
                ))));
            }
        }
        if let Some(cap) = limits.max_runs_per_day {
            let used = run_count_within(db, applet_id, 24).await?;
            if used >= cap {
                return Ok(Some(Refusal::RateLimited(format!(
                    "rate limit reached — {used} of {cap} runs allowed per day"
                ))));
            }
        }
    }

    if let Some(cap) = limits.max_llm_cost_per_day_micros {
        let spent = spend_micros_last_day(db, applet_id).await?;
        if spent >= cap {
            return Ok(Some(Refusal::OverDailyBudget(format!(
                "daily budget reached — {} of {} spent in the last 24 hours",
                format_usd(spent),
                format_usd(cap)
            ))));
        }
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn absent_limits_block_means_no_limits() {
        let l = Limits::from_config(&json!({}));
        assert_eq!(l, Limits::default());
        assert!(!l.caps_llm_spend());
        assert_eq!(l.subprocess_timeout(), DEFAULT_SUBPROCESS_TIMEOUT);
    }

    #[test]
    fn money_is_dollars_on_the_way_in() {
        let l = Limits::from_config(&json!({ "limits": { "max_llm_cost": 0.25 } }));
        assert_eq!(l.max_llm_cost_micros, Some(250_000));
        // An integer dollar is a dollar, not a micro — the trap a bare
        // `max_llm_cost = 1` would otherwise fall into.
        let l = Limits::from_config(&json!({ "limits": { "max_llm_cost": 1 } }));
        assert_eq!(l.max_llm_cost_micros, Some(1_000_000));
    }

    #[test]
    fn bare_max_runs_reads_as_a_daily_cap() {
        let l = Limits::from_config(&json!({ "limits": { "max_runs": 50 } }));
        assert_eq!(l.max_runs_per_day, Some(50));
        assert_eq!(l.max_runs_per_hour, None);
        // An explicit per-day spelling wins over the bare alias.
        let l = Limits::from_config(
            &json!({ "limits": { "max_runs": 50, "max_runs_per_day": 10 } }),
        );
        assert_eq!(l.max_runs_per_day, Some(10));
    }

    #[test]
    fn timeout_still_reads_from_the_existing_key() {
        let l = Limits::from_config(&json!({ "limits": { "timeout_s": 7500 } }));
        assert_eq!(l.subprocess_timeout(), std::time::Duration::from_secs(7500));
    }

    #[test]
    fn nonsense_values_are_ignored_not_fatal() {
        let l = Limits::from_config(&json!({
            "limits": { "max_llm_cost": -1, "max_runs": -5, "unknown_future_key": 3 }
        }));
        assert_eq!(l.max_llm_cost_micros, None);
        assert_eq!(l.max_runs_per_day, None);
    }

    #[test]
    fn usd_formatting_is_what_the_gate_shows() {
        assert_eq!(format_usd(250_000), "$0.25");
        assert_eq!(format_usd(0), "$0.00");
        assert_eq!(format_usd(1_234_567), "$1.23");
    }
}
