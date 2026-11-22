//! The task row of an applet that is a task of the assistant's.
//!
//! An applet with a prompt that starts on its own ("write my week in review
//! every Sunday") is a task the assistant does, over and over. The applet
//! runner keeps running it. One row in `app_tasks` stands for it, so it sits
//! in the task list beside the owner's own tasks (`api::tasks`, "A task that
//! repeats"). This module decides which applets have that row, opens it, and
//! files each run's result on it.
//!
//! See agents/plan/task-skill-site-plan.md, slice D.

use sqlx::PgPool;

use crate::api::tasks;
use crate::error::Result;
use crate::scheduler::applets::{self, Applet};

/// Whether an applet has a task row: it is a task of the assistant's
/// (`applets::kind_of`), it has not finished, and someone set it up. One
/// that came with the server has no row, whether it is on or off: the
/// morning examen is on from the first day, and a row for it would put a
/// task nobody asked for in every task list. It stays on the Server tasks
/// page and never shows in the owner's own list.
fn wants_row(a: &Applet) -> bool {
    let has_view = crate::server::faces::face_for(&a.id).is_some();
    applets::derived_kind(a, has_view) == "assistant_task" && a.archived_at.is_none() && a.owner != "system"
}

/// Open the applet's task row if it should have one, and return the id of
/// the row it has. `thread_id` is the row's home when it is first opened.
///
/// An applet that should no longer have one keeps the row it had, closed as
/// done: a one-time task that ran, a task whose schedule was removed, or one
/// that gained a view and became a site. Without that the row would sit in
/// the task list for good, since the owner cannot end a task that repeats.
pub async fn ensure(pool: &PgPool, a: &Applet, thread_id: Option<&str>) -> Result<Option<String>> {
    if !wants_row(a) {
        tasks::close_standing_task(pool, &a.id).await?;
        return tasks::standing_task_id(pool, &a.id).await;
    }
    let task = tasks::open_standing_task(pool, &a.id, &a.name, a.description.as_deref(), thread_id).await?;
    Ok(Some(task.id))
}

/// Bring one applet's row in line with the applet as it is stored now. Used
/// where the caller has only the id, or holds an applet read before a change:
/// after a run that may have finished a one-time task, and when the owner
/// opens a row. An applet that is gone has no row left to settle.
pub async fn settle(pool: &PgPool, applet_id: &str) -> Result<()> {
    match applets::get_applet(pool, applet_id).await {
        Ok(a) => ensure(pool, &a, None).await.map(|_| ()),
        Err(crate::error::Error::NotFound(_)) => Ok(()),
        Err(e) => Err(e),
    }
}

/// Open the row of every applet that should have one. Run when the server
/// starts, after it has read the applet folders, so an applet made before
/// task rows existed, or put in place by hand, gets its row. Returns how many
/// applets have one. One applet that fails is logged and does not stop the
/// rest.
pub async fn ensure_all(pool: &PgPool) -> Result<u64> {
    let mut with_row = 0;
    for a in applets::get_all_applets(pool).await? {
        match ensure(pool, &a, None).await {
            Ok(Some(_)) => with_row += 1,
            Ok(None) => {}
            Err(e) => tracing::error!(applet_id = %a.id, error = %e, "could not open or close the applet's task row"),
        }
    }
    Ok(with_row)
}

/// A run of the applet finished with something to read. The summary becomes
/// the row's result, with the page the run wrote when it wrote one. An applet
/// with no open row files nothing.
///
/// `trigger` is what started the run. A run the applet started itself
/// (`cron`, `webhook`, `data`) is new to the owner, and the row waits until
/// they open it. A run they started by hand shows its result on the row
/// without marking it new. A run from a chat (`message`, `tool`) answered in
/// that chat and files nothing.
pub async fn file_result(pool: &PgPool, a: &Applet, run_id: &str, summary: &str, trigger: &str) -> Result<()> {
    if matches!(trigger, "message" | "tool") {
        return Ok(());
    }
    if ensure(pool, a, None).await?.is_none() {
        return Ok(());
    }
    let page_id: Option<String> = sqlx::query_scalar(
        "SELECT ref_id FROM app_applet_run_outputs WHERE run_id = $1 AND kind = 'page' ORDER BY created_at DESC LIMIT 1",
    )
    .bind(run_id)
    .fetch_optional(pool)
    .await?;
    let url = page_id.map(|id| format!("/page/{id}"));
    let new = matches!(trigger, "cron" | "webhook" | "data");
    tasks::file_standing_result(pool, &a.id, summary, url.as_deref(), new).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn applet(pool: &PgPool, id: &str, owner: &str, enabled: bool, prompt: Option<&str>, schedule: Option<&str>, command: Option<&str>) {
        sqlx::query(
            "INSERT INTO app_applets (id, name, owner, enabled, agent, schedule, command, triggers)
             VALUES ($1, $2, $3, $4, $5, $6, $7, '[\"manual\"]')",
        )
        .bind(id)
        .bind(id.trim_start_matches("applet_").trim_start_matches("user__").replace('_', " "))
        .bind(owner)
        .bind(enabled)
        .bind(prompt)
        .bind(schedule)
        .bind(command)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn rows(pool: &PgPool) -> Vec<String> {
        sqlx::query_scalar("SELECT applet_id FROM app_tasks WHERE applet_id IS NOT NULL ORDER BY applet_id")
            .fetch_all(pool)
            .await
            .unwrap()
    }

    /// Only a prompt that starts on its own, set up by someone, gets a task
    /// row. One that came with the server gets none, on or off.
    #[sqlx::test(migrations = "./migrations")]
    async fn a_prompt_that_starts_itself_gets_one_task_row(pool: PgPool) {
        let sunday = Some("0 0 18 * * 0");
        applet(&pool, "applet_user__week_in_review", "ai", true, Some("Write the week."), sunday, None).await;
        applet(&pool, "applet_meeting_prep", "system", true, Some("Prepare the meeting."), None, None).await;
        applet(&pool, "applet_morning_examen", "system", true, Some("Look back."), Some("0 0 7 * * *"), None).await;
        applet(&pool, "applet_user__reading_nudge", "user", false, Some("Suggest a book."), Some("0 0 9 * * 6"), None).await;
        applet(&pool, "applet_embedding_index", "system", true, None, Some("0 */15 * * * *"), Some("[\"embedding_index\"]")).await;

        // Yours get a row, on or off. The shipped examen gets none though it is on.
        let yours = ["applet_user__reading_nudge", "applet_user__week_in_review"];
        assert_eq!(ensure_all(&pool).await.unwrap(), 2);
        assert_eq!(rows(&pool).await, yours);
        // Asking again opens nothing new.
        assert_eq!(ensure_all(&pool).await.unwrap(), 2);
        assert_eq!(rows(&pool).await, yours);

        let examen = applets::get_applet(&pool, "applet_morning_examen").await.unwrap();
        assert!(ensure(&pool, &examen, None).await.unwrap().is_none());
    }

    /// A run's summary lands on the row with the page the run wrote. A
    /// thread that does not exist is no home.
    #[sqlx::test(migrations = "./migrations")]
    async fn a_runs_result_lands_on_the_row_with_its_page(pool: PgPool) {
        applet(&pool, "applet_user__week_in_review", "ai", true, Some("Write the week."), Some("0 0 18 * * 0"), None).await;
        let a = applets::get_applet(&pool, "applet_user__week_in_review").await.unwrap();
        let task_id = ensure(&pool, &a, Some("thread_gone")).await.unwrap().expect("a row");
        let task = tasks::get_task(&pool, &task_id).await.unwrap();
        assert!(task.thread_id.is_none());

        sqlx::query("INSERT INTO app_applet_runs (id, applet_id, status, trigger) VALUES ('run_1', $1, 'success', 'cron')")
            .bind(&a.id)
            .execute(&pool)
            .await
            .unwrap();
        applets::record_page_output(&pool, "run_1", "page_1").await.unwrap();
        file_result(&pool, &a, "run_1", "A full week, mostly outdoors.", "cron").await.unwrap();

        let ready = tasks::get_task(&pool, &task_id).await.unwrap();
        assert_eq!(ready.status, "ready");
        assert_eq!(ready.result.as_deref(), Some("A full week, mostly outdoors."));
        assert_eq!(ready.result_url.as_deref(), Some("/page/page_1"));

        // An applet that is not a task of the assistant's files nothing.
        applet(&pool, "applet_meeting_prep", "system", true, Some("Prepare the meeting."), None, None).await;
        let skill = applets::get_applet(&pool, "applet_meeting_prep").await.unwrap();
        file_result(&pool, &skill, "run_1", "Ready.", "cron").await.unwrap();
        assert_eq!(rows(&pool).await, ["applet_user__week_in_review"]);
    }

    /// A row closes when its applet stops being a task that repeats: a
    /// one-time task that ran, or one that lost its schedule. A result the
    /// owner has not opened keeps the row waiting until they do. A schedule
    /// put back opens the row again.
    #[sqlx::test(migrations = "./migrations")]
    async fn a_row_closes_when_its_applet_has_no_more_runs(pool: PgPool) {
        applet(&pool, "applet_user__dentist_reminder", "ai", true, Some("Remind me."), Some("0 0 9 2 11 *"), None).await;
        let a = applets::get_applet(&pool, "applet_user__dentist_reminder").await.unwrap();
        let task_id = ensure(&pool, &a, None).await.unwrap().expect("a row");
        sqlx::query("INSERT INTO app_applet_runs (id, applet_id, status, trigger) VALUES ('run_1', $1, 'success', 'cron')")
            .bind(&a.id)
            .execute(&pool)
            .await
            .unwrap();
        file_result(&pool, &a, "run_1", "Your appointment is at nine.", "cron").await.unwrap();

        // It ran once and was archived. The unread result still waits.
        sqlx::query("UPDATE app_applets SET archived_at = now() WHERE id = $1").bind(&a.id).execute(&pool).await.unwrap();
        settle(&pool, &a.id).await.unwrap();
        assert_eq!(tasks::get_task(&pool, &task_id).await.unwrap().status, "ready");

        // Opened, it closes as done and keeps the result.
        tasks::mark_standing_seen(&pool, &task_id).await.unwrap();
        settle(&pool, &a.id).await.unwrap();
        let done = tasks::get_task(&pool, &task_id).await.unwrap();
        assert_eq!((done.status.as_str(), done.result.as_deref()), ("done", Some("Your appointment is at nine.")));
        assert!(done.closed_at.is_some());

        // A task whose schedule was removed is a skill, and its row closes.
        applet(&pool, "applet_user__week_in_review", "ai", true, Some("Write the week."), Some("0 0 18 * * 0"), None).await;
        let weekly = applets::get_applet(&pool, "applet_user__week_in_review").await.unwrap();
        let weekly_row = ensure(&pool, &weekly, None).await.unwrap().expect("a row");
        sqlx::query("UPDATE app_applets SET schedule = NULL WHERE id = $1").bind(&weekly.id).execute(&pool).await.unwrap();
        settle(&pool, &weekly.id).await.unwrap();
        assert_eq!(tasks::get_task(&pool, &weekly_row).await.unwrap().status, "done");

        // The schedule put back opens the same row again.
        sqlx::query("UPDATE app_applets SET schedule = '0 0 18 * * 0' WHERE id = $1").bind(&weekly.id).execute(&pool).await.unwrap();
        settle(&pool, &weekly.id).await.unwrap();
        let again = tasks::get_task(&pool, &weekly_row).await.unwrap();
        assert_eq!(again.status, "open");
        assert!(again.closed_at.is_none());

        // An applet that is gone has nothing to settle.
        settle(&pool, "applet_user__nothing").await.unwrap();
    }

    /// Only a run the applet started itself is new to the owner. One they
    /// started by hand shows its result without waiting, and one from a chat
    /// files nothing.
    #[sqlx::test(migrations = "./migrations")]
    async fn a_result_is_new_only_when_the_applet_started_the_run(pool: PgPool) {
        applet(&pool, "applet_user__week_in_review", "ai", true, Some("Write the week."), Some("0 0 18 * * 0"), None).await;
        let a = applets::get_applet(&pool, "applet_user__week_in_review").await.unwrap();
        let task_id = ensure(&pool, &a, None).await.unwrap().expect("a row");

        file_result(&pool, &a, "run_none", "Asked in a chat.", "message").await.unwrap();
        file_result(&pool, &a, "run_none", "Used as a tool.", "tool").await.unwrap();
        let untouched = tasks::get_task(&pool, &task_id).await.unwrap();
        assert_eq!((untouched.status.as_str(), untouched.result.as_deref()), ("open", None));

        file_result(&pool, &a, "run_none", "Run by hand.", "manual").await.unwrap();
        let by_hand = tasks::get_task(&pool, &task_id).await.unwrap();
        assert_eq!((by_hand.status.as_str(), by_hand.result.as_deref()), ("open", Some("Run by hand.")));

        file_result(&pool, &a, "run_none", "Sunday's run.", "cron").await.unwrap();
        let scheduled = tasks::get_task(&pool, &task_id).await.unwrap();
        assert_eq!((scheduled.status.as_str(), scheduled.result.as_deref()), ("ready", Some("Sunday's run.")));
    }
}
