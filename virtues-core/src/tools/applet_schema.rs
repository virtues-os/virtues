//! Applet-owned tables: versioned, append-only, written from the whole schema.
//!
//! Setup takes every `CREATE TABLE` an applet wants, every time (`tables`).
//! [`delta`] compares that with the tables the applet has and writes only
//! what is missing (new tables, and `ADD COLUMN` for new columns) as the next
//! numbered migration, `tables/NNNN_schema.sql`. Nothing is ever dropped.
//!
//! The migrations are what makes the folder portable: a fresh box, or one
//! restored from a backup, replays them in order (`replay_pending`) and gets
//! the same tables. Each runs once and is recorded in
//! `app_applet_schema_migrations`.
//!
//! The assistant used to send each change as its own migration, and to know
//! that `CREATE TABLE IF NOT EXISTS` on an existing table adds no column. A
//! modest model gets that wrong, so the box works it out instead.

use sha2::{Digest, Sha256};
use sqlx::PgPool;

/// The legacy single-file name. Present in folders authored before versioning;
/// read as version 1 so those applets keep working and can be migrated on top.
pub const LEGACY_SCHEMA_FILE: &str = "schema.sql";

/// Subdirectory holding the numbered versions, in a `manifest.toml` folder.
// TODO(2026-10-27): drop with the manifest.toml reader (slice 4 of
// agents/plan/applet-shape.md).
pub const SCHEMA_DIR: &str = "schema";

/// The same, in a `SKILL.md` folder.
pub const TABLES_DIR: &str = "tables";

/// The folder an applet's versions are in: `tables/` when it has one, else
/// `schema/`.
pub fn versions_dir(dir: &std::path::Path) -> std::path::PathBuf {
    let tables = dir.join(TABLES_DIR);
    if tables.is_dir() {
        tables
    } else {
        dir.join(SCHEMA_DIR)
    }
}

pub fn checksum(ddl: &str) -> String {
    let mut h = Sha256::new();
    h.update(ddl.as_bytes());
    format!("{:x}", h.finalize())
}

/// One recorded, applied version.
#[derive(Debug, Clone)]
pub struct AppliedMigration {
    pub version: i32,
    pub checksum: String,
}

/// Versions this box has already run for an applet, oldest first.
pub async fn applied(pool: &PgPool, applet_id: &str) -> Result<Vec<AppliedMigration>, String> {
    let rows: Vec<(i32, String)> = sqlx::query_as(
        "SELECT version, checksum FROM app_applet_schema_migrations \
         WHERE applet_id = $1 ORDER BY version",
    )
    .bind(applet_id)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(rows
        .into_iter()
        .map(|(version, checksum)| AppliedMigration { version, checksum })
        .collect())
}

/// Record a version as applied.
pub async fn record(
    pool: &PgPool,
    applet_id: &str,
    version: i32,
    name: &str,
    checksum: &str,
) -> Result<(), String> {
    sqlx::query(
        "INSERT INTO app_applet_schema_migrations (applet_id, version, name, checksum) \
         VALUES ($1, $2, $3, $4) ON CONFLICT (applet_id, version) DO NOTHING",
    )
    .bind(applet_id)
    .bind(version)
    .bind(name)
    .bind(checksum)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// What to do with a freshly submitted `schema_sql`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Submission {
    /// Byte-identical to a version already applied — the ordinary case when a
    /// re-setup changes the prompt and resubmits the same schema. Nothing to
    /// write, nothing to run.
    AlreadyApplied { version: i32 },
    /// New DDL. Write it as this version and apply it.
    NewVersion { version: i32 },
}

/// Decide how to treat submitted DDL against the applied set.
///
/// Only the *latest* version is compared, not every historical one. Matching
/// an older checksum would mean the model resubmitted a schema two edits stale,
/// which is a genuine change (a revert) and should be recorded as one rather
/// than silently treated as a no-op.
pub fn classify(ddl: &str, applied: &[AppliedMigration]) -> Submission {
    let sum = checksum(ddl);
    match applied.last() {
        Some(latest) if latest.checksum == sum => Submission::AlreadyApplied {
            version: latest.version,
        },
        Some(latest) => Submission::NewVersion {
            version: latest.version + 1,
        },
        None => Submission::NewVersion { version: 1 },
    }
}

/// Every version an applet folder carries, in order: the numbered files under
/// `schema/`, or a bare legacy `schema.sql` read as version 1.
///
/// The folder is the portable definition — this is what a box replays to reach
/// the shape the applet expects, and the reason versions are files rather than
/// only rows.
pub fn versions_on_disk(dir: &std::path::Path) -> Vec<(i32, String, String)> {
    let schema_dir = versions_dir(dir);
    if schema_dir.is_dir() {
        let mut out: Vec<(i32, String, String)> = std::fs::read_dir(&schema_dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                let path = e.path();
                if path.extension().and_then(|x| x.to_str()) != Some("sql") {
                    return None;
                }
                let name = path.file_name()?.to_str()?.to_string();
                // Leading digits are the ordinal, matching how `make migration`
                // numbers the box's own migrations.
                let digits: String = name.chars().take_while(|c| c.is_ascii_digit()).collect();
                let version: i32 = digits.parse().ok()?;
                let sql = std::fs::read_to_string(&path).ok()?;
                Some((version, name, sql))
            })
            .collect();
        out.sort_by_key(|(v, _, _)| *v);
        return out;
    }

    // Pre-versioning folder: one file, which is version 1 by definition.
    let legacy = dir.join(LEGACY_SCHEMA_FILE);
    match std::fs::read_to_string(&legacy) {
        Ok(sql) => vec![(1, LEGACY_SCHEMA_FILE.to_string(), sql)],
        Err(_) => Vec::new(),
    }
}

/// Apply any versions on disk that this box has not run yet, oldest first.
///
/// Idempotent and safe to call on every reconcile: a box that is current does
/// nothing. This is what makes a folder restored from git — or one that
/// travelled with a backup — actually produce its tables, rather than leaving
/// an applet whose prompt references a table nobody ever created.
pub async fn replay_pending(
    pool: &PgPool,
    applet_id: &str,
    slug: &str,
    dir: &std::path::Path,
) -> usize {
    let on_disk = versions_on_disk(dir);
    if on_disk.is_empty() {
        return 0;
    }
    let already = applied(pool, applet_id).await.unwrap_or_default();
    let mut ran = 0usize;

    for (version, name, sql) in on_disk {
        if let Some(prior) = already.iter().find(|a| a.version == version) {
            // A file whose contents no longer match what this box ran is a
            // divergence, not an update. Refusing loudly beats re-running DDL
            // against tables that were built from something else.
            if prior.checksum != checksum(&sql) {
                tracing::warn!(
                    applet_id,
                    version,
                    "applet schema version on disk differs from the one applied here; skipping"
                );
            }
            continue;
        }
        // The same guards the authoring path uses — a folder can arrive from a
        // git import, so its DDL is no more trusted than a model's.
        if let Err(e) = validate_text(&sql, slug) {
            tracing::warn!(applet_id, version, error = %e, "applet schema version rejected");
            continue;
        }
        if let Err(e) = apply(pool, &sql, slug).await {
            tracing::warn!(applet_id, version, error = %e, "applet schema version failed to apply");
            break; // ordered migrations: do not run N+1 after N failed
        }
        if let Err(e) = record(pool, applet_id, version, &name, &checksum(&sql)).await {
            tracing::warn!(applet_id, version, error = %e, "failed to record applet schema version");
        }
        ran += 1;
    }
    ran
}

// ============================================================================
// From the whole schema to the next migration
// ============================================================================

/// The next migration that brings the applet's tables to `desired`: every
/// table and column it declares that the applet's schema lacks, and the
/// indexes of tables it creates. `None` when nothing is missing.
///
/// `desired` holds `CREATE TABLE`s, `CREATE [UNIQUE] INDEX`es, `COMMENT ON`s
/// and `ALTER TABLE … ADD COLUMN`s, with names qualified by the applet's
/// schema or not. A column whose definition changed is not altered, and
/// nothing is dropped: a table or column left out of `desired` stays.
pub async fn delta(pool: &PgPool, desired: &str, slug: &str) -> Result<Option<String>, String> {
    validate_statements(desired, slug)?;
    let schema = format!("applet_{slug}");
    let live: Vec<(String, String)> = sqlx::query_as(
        "SELECT table_name, column_name FROM information_schema.columns WHERE table_schema = $1",
    )
    .bind(&schema)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("could not read the applet's tables: {e}"))?;
    let mut tables: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
    for (table, column) in live {
        tables.entry(table.to_lowercase()).or_default().push(column.to_lowercase());
    }

    let unqualified = |name: &str| -> String {
        let name = name.trim().to_lowercase();
        name.strip_prefix(&format!("{schema}.")).unwrap_or(&name).to_string()
    };
    let mut out: Vec<String> = Vec::new();
    let mut created: Vec<String> = Vec::new();
    for stmt in split_statements(&strip_comments(desired)) {
        let stmt = stmt.trim();
        let lower = stmt.to_lowercase();
        let words: Vec<&str> = lower.split_whitespace().collect();
        match words.as_slice() {
            [] => {}
            ["create", "schema", ..] => {}
            ["drop", ..] => {
                return Err("`tables` never drops a table or index; leave out what you no longer need, and it stays".into())
            }
            ["create", "table", ..] => {
                let (name, columns) = create_table_parts(stmt)
                    .ok_or_else(|| format!("could not read the CREATE TABLE in '{}…'", &stmt.chars().take(60).collect::<String>()))?;
                let table = unqualified(&name);
                match tables.get_mut(&table) {
                    None => {
                        out.push(stmt.to_string());
                        tables.insert(table.clone(), columns.iter().map(|(c, _)| c.clone()).collect());
                        created.push(table);
                    }
                    Some(have) => {
                        for (column, def) in columns {
                            if !have.contains(&column) {
                                out.push(format!("ALTER TABLE {table} ADD COLUMN IF NOT EXISTS {def}"));
                                have.push(column);
                            }
                        }
                    }
                }
            }
            ["alter", "table", rest @ ..] => {
                let rest: Vec<&str> = rest.iter().copied().skip_while(|w| matches!(*w, "if" | "exists" | "only")).collect();
                let Some((name, after)) = rest.split_first() else { continue };
                let table = unqualified(name);
                let after: Vec<&str> = after.iter().copied().skip_while(|w| matches!(*w, "add" | "column" | "if" | "not" | "exists")).collect();
                let adds = lower.contains(" add ");
                match (adds, after.first(), tables.get_mut(&table)) {
                    (true, Some(column), Some(have)) if !lower.contains(" constraint ") => {
                        let column = column.trim_matches(',').to_string();
                        if !have.contains(&column) {
                            // The definition as written, after `ADD [COLUMN] [IF NOT EXISTS]`.
                            let at = stmt.to_lowercase().find(&format!(" {column}")).unwrap_or(0);
                            out.push(format!("ALTER TABLE {table} ADD COLUMN IF NOT EXISTS {}", stmt[at..].trim()));
                            have.push(column);
                        }
                    }
                    _ if created.contains(&table) => out.push(stmt.to_string()),
                    _ => {}
                }
            }
            _ => {
                // An index or a comment: for a table made here, or one that is
                // safe to run again.
                let names_new = created.iter().any(|t| {
                    lower.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '.')).any(|w| unqualified(w) == *t)
                });
                if names_new || lower.contains("if not exists") {
                    out.push(stmt.to_string());
                }
            }
        }
    }
    if out.is_empty() {
        return Ok(None);
    }
    Ok(Some(format!("CREATE SCHEMA IF NOT EXISTS {schema};\n\n{};\n", out.join(";\n\n"))))
}

/// A `CREATE TABLE` statement's table name, and each column with its whole
/// definition (`kcal INTEGER NOT NULL`). Table-level constraints are left out.
fn create_table_parts(stmt: &str) -> Option<(String, Vec<(String, String)>)> {
    let lower = stmt.to_lowercase();
    let after = lower.find("create table")? + "create table".len();
    let paren = after + stmt[after..].find('(')?;
    let head = lower[after..paren].replace("if not exists", "");
    let name = head.trim().to_string();
    if name.is_empty() {
        return None;
    }
    let body_start = paren + 1;
    let mut depth = 1i32;
    let mut end = None;
    for (i, c) in stmt[body_start..].char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    end = Some(body_start + i);
                    break;
                }
            }
            _ => {}
        }
    }
    let body = &stmt[body_start..end?];
    let columns = split_top_level(body)
        .into_iter()
        .filter_map(|item| {
            let item = item.trim().to_string();
            let first = item.split_whitespace().next()?.to_lowercase();
            if matches!(first.as_str(), "primary" | "foreign" | "unique" | "check" | "constraint" | "exclude" | "like") {
                return None;
            }
            Some((first, item))
        })
        .collect();
    Some((name, columns))
}

/// Split DDL into statements on semicolons that actually terminate one.
///
/// `ddl.split(';')` was wrong in a way that only bites real input: a semicolon
/// inside a `-- comment` or a string literal cut the statement in half and fed
/// the database a fragment. Applet DDL is written by a model and commented in
/// prose, and prose has semicolons in it — this was going to happen to the
/// first tracker anyone wrote, and did.
fn split_statements(ddl: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut chars = ddl.chars().peekable();
    let (mut in_line, mut in_block, mut in_str) = (false, false, false);

    while let Some(c) = chars.next() {
        if in_line {
            cur.push(c);
            if c == '\n' {
                in_line = false;
            }
            continue;
        }
        if in_block {
            cur.push(c);
            if c == '*' && chars.peek() == Some(&'/') {
                cur.push(chars.next().unwrap());
                in_block = false;
            }
            continue;
        }
        if in_str {
            cur.push(c);
            if c == '\'' {
                // '' is an escaped quote, not the end of the literal.
                if chars.peek() == Some(&'\'') {
                    cur.push(chars.next().unwrap());
                } else {
                    in_str = false;
                }
            }
            continue;
        }
        match c {
            '-' if chars.peek() == Some(&'-') => {
                cur.push(c);
                cur.push(chars.next().unwrap());
                in_line = true;
            }
            '/' if chars.peek() == Some(&'*') => {
                cur.push(c);
                cur.push(chars.next().unwrap());
                in_block = true;
            }
            '\'' => {
                cur.push(c);
                in_str = true;
            }
            ';' => out.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur);
    }
    out
}

fn split_top_level(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut in_quote = false;
    let mut cur = String::new();
    for c in body.chars() {
        match c {
            '\'' => {
                in_quote = !in_quote;
                cur.push(c);
            }
            '(' if !in_quote => {
                depth += 1;
                cur.push(c);
            }
            ')' if !in_quote => {
                depth -= 1;
                cur.push(c);
            }
            ',' if depth == 0 && !in_quote => {
                out.push(std::mem::take(&mut cur));
            }
            _ => cur.push(c),
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur);
    }
    out
}

// ============================================================================
// Applying DDL — the security guards, in one place
// ============================================================================

pub async fn check(pool: &PgPool, ddl: &str, slug: &str) -> Result<(), String> {
    validate_text(ddl, slug)?;
    run_schema_statements(pool, ddl, slug, false).await
}

/// Apply DDL for real (post-check), committing it.
pub async fn apply(pool: &PgPool, ddl: &str, slug: &str) -> Result<(), String> {
    run_schema_statements(pool, ddl, slug, true).await
}

/// What a statement in `schema_sql` may start with: tables, their indexes
/// and the applet's own schema, nothing that runs code.
const STATEMENTS: &[&str] = &[
    "create schema",
    "create table",
    "create index",
    "create unique index",
    "alter table",
    "drop table",
    "drop index",
    "comment on",
];

/// Words that read or run SQL from inside a statement the list allows: a
/// `CREATE TABLE … AS SELECT`, a default, or a function that runs a query
/// given as text (`query_to_xml`, `database_to_xml`, `ts_stat`).
const FORBIDDEN_WORDS: &[&str] = &["select", "set_config", "ts_stat"];

/// Textual guards. The DDL runs as the server's role, so this is the
/// boundary until the applet has its own login (slice 9 of
/// agents/plan/applet-shape.md): every statement is one [`STATEMENTS`]
/// allows, nothing is quoted (a quoted `"public"."x"` hides its schema) or
/// dollar-quoted, nothing reads data, and every schema-qualified identifier
/// is the applet's own schema.
pub fn validate_text(ddl: &str, slug: &str) -> Result<(), String> {
    validate_statements(ddl, slug)?;
    let expected = format!("applet_{slug}");
    if !ddl.to_lowercase().contains(&expected) {
        return Err(format!(
            "schema_sql must create tables in schema {expected} (start with CREATE SCHEMA IF NOT EXISTS {expected};)"
        ));
    }
    Ok(())
}

/// [`validate_text`]'s rules for each statement, without requiring the
/// `CREATE SCHEMA` line, which [`delta`] writes itself.
fn validate_statements(ddl: &str, slug: &str) -> Result<(), String> {
    let lowered = ddl.to_lowercase();
    for kw in ["commit", "rollback", "savepoint", "grant", "revoke", "create role", "drop role"] {
        if lowered.contains(kw) {
            return Err(format!("tables may not contain '{kw}'"));
        }
    }
    // Comments never run, and the shipped files' prose quotes things.
    let code = strip_comments(&lowered);
    for (c, what) in [('"', "quoted identifiers"), ('$', "dollar quoting")] {
        if code.contains(c) {
            return Err(format!("tables may not use {what} ('{c}'). Use plain lowercase names."));
        }
    }
    if code.contains("to_xml") || code.contains("xmlschema") {
        return Err("tables may not call the XML export functions, which run queries".into());
    }
    for word in code.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')) {
        if FORBIDDEN_WORDS.contains(&word) {
            return Err(format!("tables may not contain '{word}': it creates and changes tables, and reads nothing"));
        }
    }
    for stmt in split_statements(&code) {
        let words: Vec<&str> = stmt.split_whitespace().collect();
        if words.is_empty() {
            continue;
        }
        let head = words.iter().take(3).copied().collect::<Vec<_>>().join(" ");
        if !STATEMENTS.iter().any(|s| head == *s || head.starts_with(&format!("{s} "))) {
            let shown = words.iter().take(3).copied().collect::<Vec<_>>().join(" ");
            return Err(format!(
                "tables may only create, alter or drop tables and indexes in its own schema; '{shown}…' is not one of those"
            ));
        }
    }
    let expected = format!("applet_{slug}");
    for token in lowered.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '.')) {
        if let Some((schema, _)) = token.split_once('.') {
            if (schema.starts_with("applet_") && schema != expected)
                || schema == "public"
                || schema.starts_with("data_")
                || schema.starts_with("app_")
                || schema.starts_with("wiki_")
            {
                return Err(format!(
                    "tables must only target schema {expected} (found '{token}')"
                ));
            }
        }
    }
    Ok(())
}

/// A statement with its `--` and `/* */` comments removed.
fn strip_comments(stmt: &str) -> String {
    let mut out = String::with_capacity(stmt.len());
    let mut rest = stmt;
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix("--") {
            rest = after.split_once('\n').map_or("", |(_, r)| r);
            out.push(' ');
        } else if let Some(after) = rest.strip_prefix("/*") {
            rest = after.split_once("*/").map_or("", |(_, r)| r);
            out.push(' ');
        } else {
            let c = rest.chars().next().expect("not empty");
            out.push(c);
            rest = &rest[c.len_utf8()..];
        }
    }
    out
}

/// Run the DDL statement-by-statement over the extended protocol inside one
/// transaction (ROLLBACK for the dry-run check, COMMIT for apply). One
/// statement per query means a smuggled multi-statement string is a protocol
/// error, not an escape — and it avoids `raw_sql`, whose future trips
/// rustc's Send-generality inference inside the agent stream.
async fn run_schema_statements(pool: &PgPool, ddl: &str, slug: &str, commit: bool) -> Result<(), String> {
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    sqlx::query("SET LOCAL statement_timeout = '5s'")
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    sqlx::query("SET LOCAL lock_timeout = '2s'")
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    // The search path is the applet's own schema alone, so an unqualified
    // name (`CREATE TABLE drinks`) lands there and `public` is never on it:
    // an unqualified `data_location_points` resolves to nothing and errors.
    // Combined with validate_text rejecting any qualified name outside
    // applet_<slug>, this DDL cannot touch data_*/wiki_*/app_*/public. The
    // slug is lowercase letters, digits and underscores, from the folder.
    if !slug.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_') {
        return Err(format!("`{slug}` is not an applet slug"));
    }
    sqlx::query(&format!("SET LOCAL search_path = applet_{slug}"))
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    for stmt in split_statements(ddl) {
        let stmt = stmt.trim();
        if stmt.is_empty() {
            continue;
        }
        if let Err(e) = sqlx::query(stmt).execute(&mut *tx).await {
            tx.rollback().await.ok();
            // Truncate on a CHAR boundary. `&stmt[..60]` panics the moment a
            // multibyte character straddles the cut, and DDL comments are
            // prose — em-dashes and quotes land there constantly.
            let head: String = stmt.chars().take(60).collect();
            return Err(format!("in statement '{head}…': {e}"));
        }
    }
    if commit {
        tx.commit().await.map_err(|e| e.to_string())
    } else {
        tx.rollback().await.map_err(|e| e.to_string())
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    /// Each way past the old text check is refused, and the shape the
    /// authoring loop writes still passes.
    #[test]
    fn schema_sql_creates_tables_and_nothing_else() {
        let ok = "-- the \"log\", which nobody selects from\nCREATE SCHEMA IF NOT EXISTS applet_water;\n\
                  CREATE TABLE IF NOT EXISTS applet_water.drinks (id TEXT PRIMARY KEY, ml INTEGER NOT NULL DEFAULT 250, at TIMESTAMPTZ DEFAULT now());\n\
                  CREATE INDEX ON applet_water.drinks (at);\nALTER TABLE applet_water.drinks ADD COLUMN note TEXT;";
        assert_eq!(validate_text(ok, "water"), Ok(()));
        let shipped = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../applets/daily-readings/tables/0001_schema.sql");
        assert_eq!(validate_text(&std::fs::read_to_string(shipped).unwrap(), "daily_readings"), Ok(()));
        for bad in [
            "CREATE SCHEMA IF NOT EXISTS applet_water; CREATE TABLE applet_water.t AS SELECT * FROM \"public\".\"credentials\";",
            "CREATE SCHEMA IF NOT EXISTS applet_water; CREATE TABLE applet_water.t AS SELECT 1;",
            "CREATE SCHEMA IF NOT EXISTS applet_water; DO $$ BEGIN END $$;",
            "CREATE SCHEMA IF NOT EXISTS applet_water; CREATE FUNCTION applet_water.f() RETURNS int AS 'select 1' LANGUAGE sql;",
            "CREATE SCHEMA IF NOT EXISTS applet_water; ALTER TABLE applet_water.t ADD COLUMN x xml DEFAULT database_to_xml(true, true, '');",
            "CREATE SCHEMA IF NOT EXISTS applet_water; CREATE VIEW applet_water.v AS (VALUES (1));",
        ] {
            assert!(validate_text(bad, "water").is_err(), "{bad}");
        }
    }

    #[test]
    fn classify_recognizes_an_unchanged_resubmit() {
        let ddl = "CREATE TABLE applet_x.a (id int);";
        let applied = vec![AppliedMigration {
            version: 1,
            checksum: checksum(ddl),
        }];
        assert_eq!(
            classify(ddl, &applied),
            Submission::AlreadyApplied { version: 1 },
            "re-setup that did not touch the schema must not append a version"
        );
    }

    #[test]
    fn classify_numbers_the_first_and_the_next() {
        assert_eq!(
            classify("CREATE TABLE applet_x.a (id int);", &[]),
            Submission::NewVersion { version: 1 }
        );
        let applied = vec![AppliedMigration {
            version: 1,
            checksum: "other".into(),
        }];
        assert_eq!(
            classify("ALTER TABLE applet_x.a ADD COLUMN b int;", &applied),
            Submission::NewVersion { version: 2 }
        );
    }

    async fn insert_applet(pool: &PgPool, id: &str) {
        sqlx::query(
            "INSERT INTO app_applets (id, name, owner, agent) VALUES ($1, $1, 'ai', 'x')",
        )
        .bind(id)
        .execute(pool)
        .await
        .expect("insert applet");
    }

    /// A `SKILL.md` folder keeps its versions in `tables/`.
    #[test]
    fn versions_are_read_from_tables_when_the_folder_has_it() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(TABLES_DIR)).unwrap();
        std::fs::write(dir.path().join(TABLES_DIR).join("0001_log.sql"), "create table applet_x.log (id int);").unwrap();
        let v = versions_on_disk(dir.path());
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].0, 1);
    }

    fn write_version(dir: &std::path::Path, name: &str, sql: &str) {
        let d = dir.join(SCHEMA_DIR);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join(name), sql).unwrap();
    }

    /// The whole point: a version runs once. Replaying is what reconcile does
    /// on every pass, so a second call must be a no-op rather than re-running
    /// DDL against tables that already exist.
    #[sqlx::test]
    async fn replay_applies_each_version_exactly_once(pool: PgPool) {
        let id = "applet_user__t1";
        insert_applet(&pool, id).await;
        let tmp = tempfile::tempdir().unwrap();
        write_version(
            tmp.path(),
            "0001_schema.sql",
            "CREATE SCHEMA IF NOT EXISTS applet_t1; \
             CREATE TABLE IF NOT EXISTS applet_t1.entries (id TEXT PRIMARY KEY);",
        );

        assert_eq!(replay_pending(&pool, id, "t1", tmp.path()).await, 1);
        assert_eq!(
            replay_pending(&pool, id, "t1", tmp.path()).await,
            0,
            "a box already current must do nothing"
        );
        assert_eq!(applied(&pool, id).await.unwrap().len(), 1);
    }

    /// An edit adds a version, and only the new one runs.
    #[sqlx::test]
    async fn replay_runs_only_the_new_version(pool: PgPool) {
        let id = "applet_user__t2";
        insert_applet(&pool, id).await;
        let tmp = tempfile::tempdir().unwrap();
        write_version(
            tmp.path(),
            "0001_schema.sql",
            "CREATE SCHEMA IF NOT EXISTS applet_t2; \
             CREATE TABLE IF NOT EXISTS applet_t2.entries (id TEXT PRIMARY KEY);",
        );
        assert_eq!(replay_pending(&pool, id, "t2", tmp.path()).await, 1);

        // The shape an edit now produces: only the change.
        write_version(
            tmp.path(),
            "0002_schema.sql",
            "ALTER TABLE applet_t2.entries ADD COLUMN kcal INTEGER;",
        );
        assert_eq!(replay_pending(&pool, id, "t2", tmp.path()).await, 1);

        let cols: Vec<String> = sqlx::query_scalar(
            "SELECT column_name FROM information_schema.columns WHERE table_schema = 'applet_t2' AND table_name = 'entries'",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert!(cols.contains(&"kcal".to_string()), "the ALTER actually ran");
    }

    #[test]
    fn a_semicolon_in_a_comment_does_not_split_a_statement() {
        let ddl = "-- it is honest; a made-up number is not\n\
                   CREATE TABLE applet_x.t (id TEXT);\n\
                   -- and another; here\n\
                   CREATE TABLE applet_x.u (id TEXT);";
        let stmts: Vec<String> = split_statements(ddl)
            .into_iter()
            .filter(|s| !s.trim().is_empty())
            .collect();
        assert_eq!(stmts.len(), 2, "comments must not terminate statements");
        assert!(stmts[0].contains("applet_x.t"));
        assert!(stmts[1].contains("applet_x.u"));
    }

    #[test]
    fn a_semicolon_in_a_string_literal_does_not_split_either() {
        let ddl = "CREATE TABLE applet_x.t (k TEXT DEFAULT 'a;b', j TEXT DEFAULT \'it\'\'s\');";
        let stmts: Vec<String> = split_statements(ddl)
            .into_iter()
            .filter(|s| !s.trim().is_empty())
            .collect();
        assert_eq!(stmts.len(), 1);
    }

    /// The whole schema, sent again with a column and a table added, becomes
    /// a migration that adds only those, and the same schema again adds
    /// nothing. Unqualified names land in the applet's own schema.
    #[sqlx::test]
    async fn the_whole_schema_becomes_only_what_is_missing(pool: PgPool) {
        let first = "CREATE TABLE drinks (id TEXT PRIMARY KEY, ml INTEGER NOT NULL);\n\
                     CREATE INDEX ON drinks (ml);";
        let ddl = delta(&pool, first, "water").await.unwrap().expect("a first migration");
        assert!(ddl.starts_with("CREATE SCHEMA IF NOT EXISTS applet_water;"), "{ddl}");
        assert!(ddl.contains("CREATE INDEX ON drinks"), "{ddl}");
        apply(&pool, &ddl, "water").await.unwrap();
        assert_eq!(delta(&pool, first, "water").await.unwrap(), None, "nothing is missing");

        let more = "CREATE TABLE IF NOT EXISTS applet_water.drinks (id TEXT PRIMARY KEY, ml INTEGER NOT NULL, \
                    note TEXT DEFAULT 'none');\nCREATE INDEX ON drinks (ml);\nCREATE TABLE goals (day DATE PRIMARY KEY, ml INTEGER);";
        let ddl = delta(&pool, more, "water").await.unwrap().expect("a second migration");
        assert!(ddl.contains("ALTER TABLE drinks ADD COLUMN IF NOT EXISTS note TEXT DEFAULT 'none'"), "{ddl}");
        assert!(ddl.contains("CREATE TABLE goals"), "{ddl}");
        assert!(!ddl.contains("CREATE INDEX"), "the old table's index is not made again: {ddl}");
        apply(&pool, &ddl, "water").await.unwrap();
        let columns: Vec<String> = sqlx::query_scalar(
            "SELECT column_name FROM information_schema.columns WHERE table_schema = 'applet_water' AND table_name = 'drinks' ORDER BY column_name",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(columns, ["id", "ml", "note"]);

        assert!(delta(&pool, "DROP TABLE goals;", "water").await.unwrap_err().contains("never drops"));
        assert!(delta(&pool, "CREATE TABLE public.x (id int);", "water").await.is_err());
    }

}
