//! `virtues pages convert --dry-run | --apply [--articles]`: move every
//! markdown user page, or with `--articles` every article, to blocks
//! (`api::page_convert`, which says what each step keeps).
//!
//! `--dry-run` runs in this process and only reads the database, so it runs
//! against a copy of a box as well as the box itself. `--apply` runs in the
//! server, through the console door (`POST /api/console/pages/convert`),
//! because a page has to be moved by the process that holds its live doc and
//! the save queued for it: moved from here, a tab's queued keystrokes would
//! be lost and its socket left bound to markdown. So `--apply` needs the
//! server running on this machine, as every write verb does
//! (`cli/data.rs`). The server is found by its port alone, so `--apply`
//! opens the database `DATABASE_URL` points at and names it, by its name
//! and the Postgres server holding it (`page_convert::DatabaseIdentity`),
//! and the server refuses unless it serves that one: a run meant for a copy
//! of a box never moves the pages of another server that happens to answer,
//! a copy restored under the box's own name included.
//!
//! Both print the run's report: a line of counts, then every page that
//! converting changed or left in markdown. `--json` prints the whole report,
//! every page included. Exits 1 when a page stays markdown.
//!
//! `--dry-run --articles --texts <dir>` also writes each day page's markdown,
//! its export and its notes' anchors into `<dir>`, for the day page's
//! readers to be checked against both (`apps/web/src/lib/wiki/
//! dryrun.readers.test.ts`). Those files hold the pages' words; the report
//! holds only what converting found.

use serde_json::{json, Value};
use sqlx::PgPool;

use super::data::{array, heading, note, print_json, table, Console};
use super::types::PagesCmd;
use super::ui;

/// `pool`: the database this process was pointed at, which a dry run reads
/// and `--apply` asks the server to be serving.
pub async fn run(pool: PgPool, cmd: PagesCmd) -> Result<(), String> {
    use crate::api::page_convert::{self, Which};
    let PagesCmd::Convert { dry_run, apply, articles, texts, out } = cmd;
    let which = if articles { Which::Articles } else { Which::Pages };
    let report = if apply {
        let database = page_convert::database_identity(&pool).await.map_err(|e| e.to_string())?;
        Console::new(None).convert_pages(&database, which).await?
    } else {
        debug_assert!(dry_run);
        let report = page_convert::dry_run_keeping_texts(&pool, which, texts.as_deref())
            .await
            .map_err(|e| e.to_string())?;
        serde_json::to_value(report).map_err(|e| e.to_string())?
    };

    if out.json {
        print_json(&report)?;
    } else {
        print(&report);
    }
    let failed = report["failed"].as_u64().unwrap_or(0);
    if failed > 0 {
        return Err(format!(
            "{failed} page(s) stay markdown; the report says why for each"
        ));
    }
    Ok(())
}

/// The report as a summary and a table of the pages worth a look.
fn print(report: &Value) {
    let applied = report["run"] == "apply";
    let articles = report["which"] == "articles";
    heading(match (applied, articles) {
        (true, false) => "Moved pages to blocks",
        (false, false) => "Converting pages to blocks (dry run)",
        (true, true) => "Moved articles to blocks",
        (false, true) => "Moving articles to blocks (dry run)",
    });

    let rows: Vec<Value> = array(report, "pages").iter().filter(|p| worth_a_look(p)).map(row).collect();
    table(&["id", "title", "outcome", "suggestions", "notes", "reason"], &rows);
    let summary = summary(report);
    if ui::tty() {
        println!();
        ui::ok(&summary);
    } else {
        note(&summary);
    }
    match (report["kept_at"].as_str(), report["not_kept"].as_str()) {
        (Some(path), _) => note(&format!("report kept at {path}")),
        (None, Some(why)) => note(&format!("the report file couldn't be written ({why}); `--json` prints it")),
        (None, None) => {}
    }
}

/// Whether a page is a row of the table: one that stays markdown or was
/// skipped, and one that moves (or would, or will when opened) changed on
/// the way in, by a converter's note, an export that reads back otherwise,
/// or, for an article, an export that says it otherwise
/// (`PageResult::changed_on_the_way_in`).
fn worth_a_look(page: &Value) -> bool {
    !matches!(page["outcome"].as_str(), Some("converted" | "would_convert" | "converts_when_opened"))
        || !array(page, "notes").is_empty()
        || !page["export_differs"].is_null()
        || page["article"].get("changed_by").and_then(Value::as_array).is_some_and(|by| !by.is_empty())
        || page["article"].get("content_changed").is_some_and(|c| !c.is_null())
}

/// The run's counts in a line: what moved or would, what moves when first
/// opened, what stays markdown and what changed on the way in.
fn summary(report: &Value) -> String {
    let n = |key: &str| report[key].as_u64().unwrap_or(0);
    let applied = report["run"] == "apply";
    let when_opened = match (n("converts_when_opened"), applied) {
        (0, _) => String::new(),
        (k, true) => format!(", {k} move when first opened"),
        (k, false) => format!(", {k} would move when first opened"),
    };
    if applied {
        format!(
            "{} moved to blocks{when_opened}, {} stay markdown, {} skipped; {} changed on the way in",
            n("converted"),
            n("failed"),
            n("skipped"),
            n("with_notes")
        )
    } else {
        format!(
            "{} would move to blocks{when_opened}, {} would stay markdown, {} skipped; {} would change on the way in. No page changed.",
            n("would_convert"),
            n("failed"),
            n("skipped"),
            n("with_notes")
        )
    }
}

/// One page as a table row: its notes by kind, each kind once.
fn row(page: &Value) -> Value {
    let mut kinds: Vec<String> = vec![];
    for n in array(page, "notes") {
        let kind = virtues_document::migrate::category(n["message"].as_str().unwrap_or(""));
        if !kinds.contains(&kind) {
            kinds.push(kind);
        }
    }
    if page.get("text_lost").is_some_and(|t| !t.is_null()) {
        kinds.push("text lost".into());
    }
    if page.get("export_differs").is_some_and(|t| !t.is_null()) {
        kinds.push("export reads back differently".into());
    }
    for by in page["article"].get("changed_by").and_then(Value::as_array).into_iter().flatten() {
        if let Some(by) = by.as_str().map(|b| format!("export: {b}")) {
            if !kinds.contains(&by) {
                kinds.push(by);
            }
        }
    }
    json!({
        "id": page["id"],
        "title": page["title"],
        "outcome": page["outcome"],
        "suggestions": page.get("suggestions").cloned().unwrap_or(json!(0)),
        "notes": kinds.join("; "),
        "reason": page.get("reason").cloned().unwrap_or(Value::Null),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A dry run over articles nobody has opened: each would move when first
    /// opened, and its export changes it (a final newline, an escape) with
    /// no converter's note. The line of counts says both, and each is a row
    /// of the table: the human output left out every such article.
    #[test]
    fn a_dry_run_says_what_moves_when_opened_and_every_export_change() {
        let report = json!({
            "run": "dry_run",
            "which": "articles",
            "would_convert": 0,
            "converts_when_opened": 2,
            "failed": 0,
            "skipped": 0,
            "with_notes": 2,
            "pages": [
                {"id": "page_1", "title": "Nick", "outcome": "converts_when_opened",
                 "article": {"changed_by": ["final newline"], "content_changed": "line 1: Met Nick."}},
                {"id": "page_2", "title": "A day", "outcome": "converts_when_opened",
                 "article": {"changed_by": ["escapes"], "content_changed": "line 1: R\\&B night."}},
                {"id": "page_3", "title": "Untouched", "outcome": "converts_when_opened", "article": {}},
            ],
        });
        assert_eq!(
            summary(&report),
            "0 would move to blocks, 2 would move when first opened, 0 would stay markdown, 0 skipped; \
             2 would change on the way in. No page changed."
        );
        let shown: Vec<&str> = array(&report, "pages")
            .iter()
            .filter(|p| worth_a_look(p))
            .filter_map(|p| p["id"].as_str())
            .collect();
        assert_eq!(shown, ["page_1", "page_2"]);
        assert_eq!(row(&array(&report, "pages")[1])["notes"], "export: escapes");
    }
}
