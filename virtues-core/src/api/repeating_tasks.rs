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
/// page. Whether a shipped one should ever show in Tasks is an open question
/// in the plan (O7).
fn wants_row(a: &Applet) -> bool {
    let has_view = crate::server::faces::face_for(&a.id).is_some();
    applets::derived_kind(a, has_view) == "assistant_task" && a.archived_at.is_none() && a.owner != "system"
}

/// Open the applet's task row if it should have one, and return the id of
/// the row it has. `thread_id` is the row's home when it is first opened.
pub async fn ensure(pool: &PgPool, a: &Applet, thread_id: Option<&str>) -> Result<Option<String>> {
    if !wants_row(a) {
        return tasks::standing_task_id(pool, &a.id).await;
    }
    let task = tasks::open_standing_task(pool, &a.id, &a.name, a.description.as_deref(), thread_id).await?;
    Ok(Some(task.id))
}

/// Open the row of every applet that should have one. Run when the server
/// starts, after it has read the applet folders, so an applet made before
/// task rows existed, or put in place by hand, gets its row. Returns how many
/// applets have one.
pub async fn ensure_all(pool: &PgPool) -> Result<u64> {
    let mut with_row = 0;
    for a in applets::get_all_applets(pool).await? {
        if ensure(pool, &a, None).await?.is_some() {
            with_row += 1;
        }
    }
    Ok(with_row)
}

/// A run of the applet finished with something to read. The summary becomes
/// the row's result, with the page the run wrote when it wrote one, and the
/// row waits until the owner opens it. An applet with no row files nothing.
pub async fn file_result(pool: &PgPool, a: &Applet, run_id: &str, summary: &str) -> Result<()> {
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
    tasks::file_standing_result(pool, &a.id, summary, url.as_deref()).await?;
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
        file_result(&pool, &a, "run_1", "A full week, mostly outdoors.").await.unwrap();

        let ready = tasks::get_task(&pool, &task_id).await.unwrap();
        assert_eq!(ready.status, "ready");
        assert_eq!(ready.result.as_deref(), Some("A full week, mostly outdoors."));
        assert_eq!(ready.result_url.as_deref(), Some("/page/page_1"));

        // An applet that is not a task of the assistant's files nothing.
        applet(&pool, "applet_meeting_prep", "system", true, Some("Prepare the meeting."), None, None).await;
        let skill = applets::get_applet(&pool, "applet_meeting_prep").await.unwrap();
        file_result(&pool, &skill, "run_1", "Ready.").await.unwrap();
        assert_eq!(rows(&pool).await, ["applet_user__week_in_review"]);
    }
}
