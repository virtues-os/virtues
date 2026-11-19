//! `virtues export <folder>` — everything your server holds about you, as
//! readable files (Settings · Your data · Export everything).
//!
//! The folder gets:
//!
//!   README.txt              what each part is
//!   records/<table>.jsonl   one JSON object per line, one file per kind of
//!                           record: everything sources collected (`data_*`),
//!                           what the server wrote from it (`wiki_*`), and
//!                           your chats, pages, threads, tasks, the
//!                           assistant's notes, annotations, file records and
//!                           your profile
//!   pages/<title>-<id>.md   each page as Markdown
//!   files/                  the lake: recordings, photos, documents and the
//!                           raw streams sources sent (left out with
//!                           --skip-files)
//!
//! Left out on purpose: keys, passwords and sign-ins (the `credentials`
//! table and other secrets), pairing and auth records, and the server's own
//! bookkeeping. The export is not encrypted; a backup is (`virtues backup`).
//! Keep the folder somewhere only you can read.
//!
//! On a box, run it with sudo so it can read the database and the lake and
//! write where you ask.

use std::path::{Path, PathBuf};

use sqlx::PgPool;

use crate::error::{Error, Result};

/// App tables that hold the owner's own work, beside every `data_*` and
/// `wiki_*` table. Anything not named here stays out.
const APP_TABLES: &[&str] = &[
    "app_user_profile",
    "app_chats",
    "app_chat_messages",
    "app_pages",
    "app_page_versions",
    "app_threads",
    "app_thread_items",
    "app_tasks",
    "app_task_events",
    "app_task_links",
    "app_assistant_memories",
    "app_marginalia",
    "app_drive_files",
];

/// Columns that are binary editor state or derived vectors, not something a
/// person reads: dropped from every row that has them.
const DROPPED_COLUMNS: &[&str] = &["yjs_state", "embedding"];

/// Whether a table is part of the export.
fn exported(table: &str) -> bool {
    table.starts_with("data_") || table.starts_with("wiki_") || APP_TABLES.contains(&table)
}

/// A page's file name: its title made safe, then its id so two pages with one
/// title do not collide.
fn page_file(title: &str, id: &str) -> String {
    let mut slug: String = title
        .chars()
        .map(|c| if c.is_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect();
    while slug.contains("--") {
        slug = slug.replace("--", "-");
    }
    let slug = slug.trim_matches('-');
    let slug: String = slug.chars().take(60).collect();
    if slug.is_empty() { format!("{id}.md") } else { format!("{slug}-{id}.md") }
}

const README: &str = "Your Virtues export\n\
\n\
records/  One file per kind of record, one JSON object per line: what your\n\
          sources collected (data_*), what your server wrote from it (wiki_*),\n\
          and your chats, pages, threads, tasks, the assistant's notes,\n\
          annotations, file records and profile.\n\
pages/    Each page as Markdown.\n\
files/    Your recordings, photos, documents and the raw streams your\n\
          sources sent, as your server stored them.\n\
\n\
Keys, passwords and sign-ins are not included. This folder is not\n\
encrypted: keep it somewhere only you can read.\n";

/// Where an export goes when no folder is named: the box's exports folder,
/// which the `virtues` user can write, or the current folder off a box.
pub fn default_folder() -> PathBuf {
    let name = format!("virtues-export-{}", chrono::Utc::now().format("%Y%m%dT%H%M%SZ"));
    let on_a_box = Path::new("/var/lib/virtues").is_dir();
    if on_a_box { Path::new("/var/lib/virtues/exports").join(name) } else { PathBuf::from(name) }
}

pub async fn run(folder: &Path, skip_files: bool) -> Result<()> {
    if folder.exists() && std::fs::read_dir(folder)?.next().is_some() {
        return Err(Error::InvalidInput(format!(
            "{} isn't empty. Choose a new or empty folder.",
            folder.display()
        )));
    }
    let database_url = crate::database::normalize_database_url()?;
    let db = crate::database::Database::new(&database_url)?;
    db.connect().await?;
    let summary = export(db.pool(), folder, (!skip_files).then(crate::storage::lake::lake_root)).await?;
    println!("Exported to {}:", folder.display());
    println!("  {} kinds of record in records/", summary.tables);
    println!("  {} pages in pages/", summary.pages);
    if skip_files {
        println!("  files left out (--skip-files)");
    } else {
        println!("  {} files in files/", summary.files);
    }
    println!("The folder isn't encrypted. Keep it somewhere only you can read.");
    Ok(())
}

#[derive(Debug, Default)]
pub struct Summary {
    pub tables: usize,
    pub pages: usize,
    pub files: usize,
}

/// Writes the export into `folder`. `lake` is the lake to copy, or None to
/// leave files out.
pub async fn export(pool: &PgPool, folder: &Path, lake: Option<PathBuf>) -> Result<Summary> {
    let mut summary = Summary::default();
    std::fs::create_dir_all(folder.join("records"))?;
    std::fs::create_dir_all(folder.join("pages"))?;
    std::fs::write(folder.join("README.txt"), README)?;

    let tables: Vec<String> =
        sqlx::query_scalar("SELECT tablename::text FROM pg_tables WHERE schemaname = 'public' ORDER BY tablename")
            .fetch_all(pool)
            .await?;
    for table in tables.iter().filter(|t| exported(t)) {
        // Table names come from pg_tables and pass `exported`, so quoting
        // them is enough; the dropped columns are removed from each object.
        let drop = DROPPED_COLUMNS.iter().map(|c| format!(" - '{c}'")).collect::<String>();
        let rows: Vec<serde_json::Value> =
            sqlx::query_scalar(&format!("SELECT to_jsonb(t){drop} FROM public.\"{table}\" t"))
                .fetch_all(pool)
                .await
                .map_err(|e| Error::Database(format!("exporting {table}: {e}")))?;
        let mut out = String::new();
        for row in rows {
            out.push_str(&row.to_string());
            out.push('\n');
        }
        std::fs::write(folder.join("records").join(format!("{table}.jsonl")), out)?;
        summary.tables += 1;
    }

    let pages: Vec<(String, String, String)> =
        sqlx::query_as("SELECT id, title, content FROM app_pages WHERE deleted_at IS NULL ORDER BY created_at")
            .fetch_all(pool)
            .await
            .map_err(|e| Error::Database(format!("exporting pages: {e}")))?;
    for (id, title, content) in pages {
        let body = if title.trim().is_empty() { content } else { format!("# {title}\n\n{content}") };
        std::fs::write(folder.join("pages").join(page_file(&title, &id)), body)?;
        summary.pages += 1;
    }

    if let Some(lake) = lake {
        if lake.is_dir() {
            summary.files = copy_tree(&lake, &folder.join("files"))?;
        }
    }
    Ok(summary)
}

/// Copies a directory tree, following no symlinks. Returns the file count.
fn copy_tree(from: &Path, to: &Path) -> Result<usize> {
    std::fs::create_dir_all(to)?;
    let mut count = 0;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let target = to.join(entry.file_name());
        if kind.is_dir() {
            count += copy_tree(&entry.path(), &target)?;
        } else if kind.is_file() {
            std::fs::copy(entry.path(), &target)?;
            count += 1;
        }
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_owners_records_are_exported() {
        assert!(exported("data_health_heart_rate"));
        assert!(exported("wiki_days"));
        assert!(exported("app_chat_messages"));
        assert!(!exported("credentials"));
        assert!(!exported("app_auth_event"));
        assert!(!exported("app_pair_token"));
        assert!(!exported("app_settings_changes"));
    }

    #[test]
    fn page_files_are_safe_and_unique() {
        assert_eq!(page_file("Kitchen plans / v2", "p1"), "kitchen-plans-v2-p1.md");
        assert_eq!(page_file("   ", "p2"), "p2.md");
        assert_eq!(page_file("../../etc", "p3"), "etc-p3.md");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn an_export_writes_records_and_pages(pool: PgPool) {
        sqlx::query("INSERT INTO app_pages (id, title, content) VALUES ('p1', 'Garden plans', 'Invented text.')")
            .execute(&pool)
            .await
            .unwrap();
        let dir = std::env::temp_dir().join(format!("virtues-export-test-{}", uuid::Uuid::new_v4()));
        let summary = export(&pool, &dir, None).await.unwrap();
        assert!(summary.tables > 0);
        assert_eq!(summary.pages, 1);
        let page = std::fs::read_to_string(dir.join("pages").join("garden-plans-p1.md")).unwrap();
        assert_eq!(page, "# Garden plans\n\nInvented text.");
        let pages = std::fs::read_to_string(dir.join("records").join("app_pages.jsonl")).unwrap();
        assert!(pages.contains("\"Garden plans\"") && !pages.contains("yjs_state"));
        assert!(!dir.join("records").join("credentials.jsonl").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
