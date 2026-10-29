//! Data triggers: an applet that starts on rows in a data table.
//!
//! `trigger: new data_x`, `30 minutes before data_x` and
//! `15 minutes after data_x ends` (agents/plan/applet-shape.md, slice 11).
//! Every row has a moment: when the box wrote it for `new`, its start minus
//! the minutes for `before`, its end plus the minutes for `after`. Once a
//! minute [`poll`] looks for rows whose moment came after the applet's
//! `watched_through` and starts a run. The run takes them by moving
//! `watched_through` forward ([`claim`]), so each row goes to one run, and
//! its SQL and prompt read them as `new_rows`.
//!
//! - Rows that were there when the trigger first appeared start no run: the
//!   first poll only sets `watched_through`.
//! - `new` counts only rows whose own time is within [`RECENT_DAYS`] of now,
//!   so a source's first backfill of years of history starts no run. That
//!   bound also keeps the poll on the time column's index.
//! - A row's `created_at` is when its transaction began, which can be some
//!   seconds before it commits. The cutoff trails the clock by [`SETTLE`] so
//!   such a row is not passed over.
//! - An applet does not wake on rows it writes itself: the reader refuses
//!   `new` on a table in the applet's own `writes`.
//! - While a run is going, or the applet is at its hourly limit, rows wait,
//!   and the next run takes all of them. A data trigger gets
//!   [`RUNS_PER_HOUR`] when its file sets no hourly limit.

use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::applet_runner::{limits, RunnerDeps};
use crate::error::{Error, Result};
use crate::scheduler::applets::{self, Applet};

/// How far the cutoff trails the clock, as SQL.
const SETTLE: &str = "interval '1 minute'";

/// How recent a row's own time must be for `new` to count it.
pub const RECENT_DAYS: i64 = 7;

/// The hourly limit a data trigger gets when its file sets none.
pub const RUNS_PER_HOUR: i64 = 6;

/// One data trigger, as the file says it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Watch {
    /// A `data_*` table the registry knows.
    pub table: String,
    pub kind: WatchKind,
    /// The offset for `before` and `after_end`; 0 for `new`.
    #[serde(default)]
    pub minutes: i64,
    /// The table as a person says it, "calendar events", for the applet's
    /// page.
    #[serde(default)]
    pub label: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WatchKind {
    New,
    Before,
    AfterEnd,
}

/// The registry's entry for a `data_*` table.
pub fn data_table(table: &str) -> Option<virtues_registry::OntologyDescriptor> {
    virtues_registry::registered_ontologies()
        .into_iter()
        .find(|o| o.table_name == table && o.table_name.starts_with("data_"))
}

impl Watch {
    /// The table, and the time column its moment is read from: the end for
    /// `after_end`, the table's own time otherwise.
    fn columns(&self) -> Result<(&'static str, &'static str)> {
        let o = data_table(&self.table).ok_or_else(|| Error::Other(format!("`{}` is not a data table", self.table)))?;
        let time = match self.kind {
            WatchKind::AfterEnd => o
                .end_timestamp_column
                .ok_or_else(|| Error::Other(format!("`{}` has no end time", self.table)))?,
            _ => o.timestamp_column,
        };
        Ok((o.table_name, time))
    }

    /// The rows whose moment is after `after` and at or before `through`,
    /// both SQL expressions. Each range is written on the table's own time
    /// column where it can be, which is what that column's index serves.
    fn rows_sql(&self, after: &str, through: &str) -> Result<String> {
        let (table, time) = self.columns()?;
        let m = self.minutes;
        let range = match self.kind {
            WatchKind::New => format!(
                "created_at > {after} AND created_at <= {through} \
                 AND {time} >= {through} - interval '{RECENT_DAYS} days'"
            ),
            WatchKind::Before => format!(
                "{time} > {after} + interval '{m} minutes' AND {time} <= {through} + interval '{m} minutes'"
            ),
            WatchKind::AfterEnd => format!(
                "{time} > {after} - interval '{m} minutes' AND {time} <= {through} - interval '{m} minutes'"
            ),
        };
        Ok(format!("SELECT * FROM {table} WHERE {range}"))
    }
}

/// The rows one run takes.
#[derive(Debug, Clone)]
pub struct Batch {
    pub watch: Watch,
    /// The moments it covers: after the first, through the second. None
    /// when nothing had come since the last run.
    pub range: Option<(DateTime<Utc>, DateTime<Utc>)>,
}

fn literal(t: DateTime<Utc>) -> String {
    format!("timestamptz '{}'", t.to_rfc3339_opts(SecondsFormat::Micros, true))
}

impl Batch {
    /// `WITH new_rows AS (…)`, put in front of the run's SQL.
    pub fn cte(&self) -> Result<String> {
        let rows = match self.range {
            Some((after, through)) => self.watch.rows_sql(&literal(after), &literal(through))?,
            None => format!("SELECT * FROM {} WHERE false", self.watch.columns()?.0),
        };
        Ok(format!("WITH new_rows AS ({rows}) "))
    }

    /// Whether it holds no rows.
    pub async fn is_empty(&self, db: &PgPool) -> Result<bool> {
        if self.range.is_none() {
            return Ok(true);
        }
        let sql = format!("{}SELECT NOT EXISTS (SELECT 1 FROM new_rows)", self.cte()?);
        Ok(sqlx::query_scalar(&sql).fetch_one(db).await?)
    }

    /// The rows in the order the prompt reads them.
    pub fn listing_sql(&self) -> Result<String> {
        Ok(format!("SELECT * FROM new_rows ORDER BY {}", self.watch.columns()?.1))
    }

    /// The line above the rows in the prompt's context.
    pub fn heading(&self) -> String {
        let Watch { table, minutes, .. } = &self.watch;
        match self.watch.kind {
            WatchKind::New => format!("new_rows, the rows new in {table} since this applet's last run:"),
            WatchKind::Before => format!("new_rows, the rows in {table} that start in {minutes} minutes:"),
            WatchKind::AfterEnd => format!("new_rows, the rows in {table} that ended {minutes} minutes ago:"),
        }
    }

    /// What a run with no rows to take says to its prompt.
    pub fn nothing(&self) -> String {
        format!("new_rows is empty: no rows in {} have come up since this applet's last run.", self.watch.table)
    }

    /// The payload code receives on stdin.
    pub fn payload(&self) -> serde_json::Value {
        serde_json::json!({
            "table": self.watch.table,
            "after": self.range.map(|r| r.0),
            "through": self.range.map(|r| r.1),
        })
    }
}

/// Take the rows that have come since the applet's last run, by moving its
/// `watched_through` to the cutoff. Only one caller can move it from the
/// value it read, so two runs never take the same rows. A trigger that has
/// never been polled starts here with nothing to take.
pub async fn claim(db: &PgPool, applet: &Applet) -> Result<Option<Batch>> {
    let Some(watch) = applet.watch.clone() else {
        return Ok(None);
    };
    let moved: Option<DateTime<Utc>> = sqlx::query_scalar(&format!(
        "UPDATE app_applets SET watched_through = now() - {SETTLE} \
         WHERE id = $1 AND watched_through IS NOT DISTINCT FROM $2 \
           AND (watched_through IS NULL OR watched_through < now() - {SETTLE}) \
         RETURNING watched_through"
    ))
    .bind(&applet.id)
    .bind(applet.watched_through)
    .fetch_optional(db)
    .await?;
    let range = match (applet.watched_through, moved) {
        (Some(after), Some(through)) => Some((after.into(), through)),
        _ => None,
    };
    Ok(Some(Batch { watch, range }))
}

/// Start a run for every applet whose data trigger has rows waiting. Called
/// once a minute by the scheduler.
pub async fn poll(deps: &RunnerDeps) -> Result<()> {
    let rows = sqlx::query(
        "SELECT * FROM app_applets \
         WHERE enabled AND archived_at IS NULL AND watch IS NOT NULL AND triggers @> '[\"data\"]'::jsonb",
    )
    .fetch_all(&deps.db)
    .await?;
    for row in rows {
        let applet = match applets::applet_from_row(&row) {
            Ok(a) => a,
            Err(e) => {
                tracing::warn!(error = %e, "could not read an applet with a data trigger");
                continue;
            }
        };
        match due(&deps.db, &applet).await {
            Ok(false) => {}
            Ok(true) => {
                let deps = deps.clone();
                tokio::spawn(async move {
                    if let Err(e) = crate::applet_runner::run_applet(&deps, &applet.id, "data", None).await {
                        tracing::error!(applet_id = %applet.id, error = %e, "data-triggered run failed");
                    }
                });
            }
            Err(e) => tracing::warn!(applet_id = %applet.id, error = %e, "data trigger check failed"),
        }
    }
    Ok(())
}

/// Whether to start a run now: rows are waiting, no run is going, and its
/// limits let one start. A trigger polled for the first time only starts
/// watching.
async fn due(db: &PgPool, applet: &Applet) -> Result<bool> {
    let Some(watch) = &applet.watch else {
        return Ok(false);
    };
    let Some(after) = applet.watched_through else {
        sqlx::query(&format!(
            "UPDATE app_applets SET watched_through = now() - {SETTLE} WHERE id = $1 AND watched_through IS NULL"
        ))
        .bind(&applet.id)
        .execute(db)
        .await?;
        return Ok(false);
    };
    let rows = watch.rows_sql(&literal(after.into()), &format!("now() - {SETTLE}"))?;
    let waiting: bool = sqlx::query_scalar(&format!("SELECT EXISTS ({rows})")).fetch_one(db).await?;
    if !waiting || applets::has_active_run(db, &applet.id).await? {
        return Ok(false);
    }
    let caps = limits::Limits::from_config(&applet.config);
    Ok(limits::check_pre_run(db, &applet.id, &caps, "data").await?.is_none())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn watch(kind: WatchKind, minutes: i64) -> Watch {
        Watch { table: "data_calendar_event".into(), kind, minutes, label: "calendar events".into() }
    }

    #[test]
    fn each_kind_reads_its_own_moment() {
        let new = watch(WatchKind::New, 0).rows_sql("A", "B").unwrap();
        assert!(new.contains("created_at > A AND created_at <= B"), "{new}");
        assert!(new.contains("started_at >= B - interval '7 days'"), "{new}");
        let before = watch(WatchKind::Before, 30).rows_sql("A", "B").unwrap();
        assert!(before.contains("started_at > A + interval '30 minutes'"), "{before}");
        let after = watch(WatchKind::AfterEnd, 15).rows_sql("A", "B").unwrap();
        assert!(after.contains("ended_at <= B - interval '15 minutes'"), "{after}");
    }

    #[test]
    fn an_empty_batch_selects_no_rows_of_the_right_shape() {
        let b = Batch { watch: watch(WatchKind::New, 0), range: None };
        assert_eq!(b.cte().unwrap(), "WITH new_rows AS (SELECT * FROM data_calendar_event WHERE false) ");
        assert!(b.nothing().contains("data_calendar_event"));
    }

    async fn applet(pool: &PgPool, kind: &str) {
        sqlx::query(
            "INSERT INTO app_applets (id, name, owner, run_sql, triggers, watch) \
             VALUES ('applet_w', 'Watcher', 'user', 'select * from new_rows', '[\"manual\",\"data\"]', $1::jsonb)",
        )
        .bind(format!(r#"{{"table":"data_calendar_event","kind":"{kind}","minutes":30,"label":"calendar events"}}"#))
        .execute(pool)
        .await
        .unwrap();
    }

    async fn event(pool: &PgPool, id: &str, starts_in_minutes: i64, written_minutes_ago: i64) {
        sqlx::query(
            "INSERT INTO data_calendar_event (id, title, started_at, ended_at, source_stream_id, source_table, source_provider, created_at) \
             VALUES ($1, 'Planning', now() + make_interval(mins => $2::int), now() + make_interval(mins => $2::int + 60), \
                     $1, 'stream_calendar', 'google', now() - make_interval(mins => $3::int))",
        )
        .bind(id)
        .bind(starts_in_minutes)
        .bind(written_minutes_ago)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn set_watched_through(pool: &PgPool, minutes_ago: i64) {
        sqlx::query("UPDATE app_applets SET watched_through = now() - make_interval(mins => $1::int) WHERE id = 'applet_w'")
            .bind(minutes_ago)
            .execute(pool)
            .await
            .unwrap();
    }

    #[sqlx::test]
    async fn new_rows_wait_until_a_run_takes_them_once(pool: PgPool) {
        applet(&pool, "new").await;
        event(&pool, "ev_old", 600, 60).await;
        let get = || applets::get_applet(&pool, "applet_w");

        // The first poll only starts watching: the row already there counts
        // for nothing.
        assert!(!due(&pool, &get().await.unwrap()).await.unwrap());
        assert!(get().await.unwrap().watched_through.is_some());
        assert!(!due(&pool, &get().await.unwrap()).await.unwrap());

        // A row written since is waiting, once it has settled.
        set_watched_through(&pool, 10).await;
        event(&pool, "ev_new", 600, 5).await;
        event(&pool, "ev_unsettled", 600, 0).await;
        assert!(due(&pool, &get().await.unwrap()).await.unwrap());

        // A run takes it, and a second claim from the same reading takes
        // nothing.
        let read = get().await.unwrap();
        let batch = claim(&pool, &read).await.unwrap().unwrap();
        assert!(batch.range.is_some());
        let ids: Vec<String> = sqlx::query_scalar(&format!("{}SELECT id FROM new_rows ORDER BY id", batch.cte().unwrap()))
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(ids, ["ev_new"]);
        assert!(claim(&pool, &read).await.unwrap().unwrap().range.is_none());
        assert!(!due(&pool, &get().await.unwrap()).await.unwrap());
    }

    #[sqlx::test]
    async fn new_passes_over_a_backfill_of_old_history(pool: PgPool) {
        applet(&pool, "new").await;
        set_watched_through(&pool, 10).await;
        // Written just now, about a meeting a month ago.
        event(&pool, "ev_backfill", -30 * 24 * 60, 5).await;
        assert!(!due(&pool, &applets::get_applet(&pool, "applet_w").await.unwrap()).await.unwrap());
    }

    #[sqlx::test]
    async fn before_wakes_when_the_start_comes_into_range(pool: PgPool) {
        applet(&pool, "before").await;
        set_watched_through(&pool, 10).await;
        // Starts in 25 minutes: 30 minutes before it was 5 minutes ago.
        event(&pool, "ev_soon", 25, 600).await;
        // Starts in two hours: not yet.
        event(&pool, "ev_later", 120, 600).await;
        let read = applets::get_applet(&pool, "applet_w").await.unwrap();
        assert!(due(&pool, &read).await.unwrap());
        let batch = claim(&pool, &read).await.unwrap().unwrap();
        let ids: Vec<String> = sqlx::query_scalar(&format!("{}SELECT id FROM new_rows", batch.cte().unwrap()))
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(ids, ["ev_soon"]);
    }

    #[sqlx::test]
    async fn a_running_applet_or_one_at_its_limit_lets_rows_wait(pool: PgPool) {
        applet(&pool, "new").await;
        set_watched_through(&pool, 10).await;
        event(&pool, "ev_new", 600, 5).await;
        sqlx::query("UPDATE app_applets SET config = '{\"limits\": {\"max_runs_per_hour\": 1}}' WHERE id = 'applet_w'")
            .execute(&pool)
            .await
            .unwrap();
        let run = applets::create_run(&pool, Some("applet_w"), "data").await.unwrap();
        let read = applets::get_applet(&pool, "applet_w").await.unwrap();
        assert!(!due(&pool, &read).await.unwrap(), "a run is going");
        applets::complete_run(&pool, &run.id, "success", 1, None, Some("1 row.")).await.unwrap();
        assert!(!due(&pool, &read).await.unwrap(), "one run this hour is its limit");
    }
}
