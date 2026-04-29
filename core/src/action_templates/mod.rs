//! Action template loader, source catalog, and reconciler.
//!
//! Reads `actions/templates.toml` at startup and:
//!
//! 1. Parses `[[source]]` rows into a static catalog (lookup by `id`, used by
//!    auth handlers and the `/api/sources` endpoint).
//! 2. Reconciles `[[action]]` rows into `app_actions`. Template fields
//!    (function_name, triggers, agent, condition, owner, name) are overwritten
//!    on every startup. User-managed fields (cron_schedule, config, enabled,
//!    memory) are preserved.
//! 3. Per-credential templates fan out one row per matching `credentials` row
//!    (used for iOS streams where each paired device gets its own action row).

use std::sync::OnceLock;

use crate::error::{Error, Result};
use serde::Deserialize;
use sqlx::SqlitePool;

// ─────────────────────────────────────────────────────────────────────────────
// TOML schema
// ─────────────────────────────────────────────────────────────────────────────

/// One `[[source]]` entry in `actions/templates.toml`. Catalog tile.
#[derive(Debug, Deserialize, Clone)]
pub struct Source {
    /// Stable id used as `credentials.source_id` and as `[[action]].source.id`.
    pub id: String,
    pub display_name: String,
    pub icon: String,
    pub description: String,
    pub auth: SourceAuth,
}

/// How a source authenticates. Matches the three auth kinds in the charter.
#[derive(Debug, Deserialize, Clone, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SourceAuth {
    /// Server mints a bearer (iOS-style). Webhook router validates HMAC lookup.
    SelfIssuedBearer,
    /// Browser redirect through `apps/oauth-proxy`. Covers OAuth and Plaid Link.
    ViaProxy { start_path: String },
    /// User pastes one or more strings (MCP tokens, BYO API keys).
    ApiKey { fields: Vec<String> },
}

impl SourceAuth {
    /// Stable wire string for API responses + frontend dispatch.
    /// `self_issued_bearer` | `via_proxy` | `api_key`.
    pub fn kind_str(&self) -> &'static str {
        match self {
            Self::SelfIssuedBearer => "self_issued_bearer",
            Self::ViaProxy { .. } => "via_proxy",
            Self::ApiKey { .. } => "api_key",
        }
    }
}

/// One `[[action]]` entry in `actions/templates.toml`.
#[derive(Debug, Deserialize)]
struct Template {
    /// Stable id prefix; for non-per-credential entries the final id is this
    /// value. For per-credential entries: `{id_prefix}_{credential_id}`.
    id_prefix: String,
    name: String,
    owner: String,
    function_name: Option<String>,
    #[serde(default)]
    triggers: Vec<String>,
    #[serde(default)]
    default_cron: Option<String>,
    #[serde(default = "default_true")]
    default_enabled: bool,
    #[serde(default)]
    condition: Option<String>,
    #[serde(default)]
    agent: Option<String>,
    #[serde(default)]
    per_credential: bool,
    /// Reference to a `[[source]]` entry in this same file. Required when
    /// `per_credential = true` — fan-out matches credentials by `source_id`.
    /// Absent for credential-less templates (housekeeping).
    #[serde(default)]
    source: Option<SourceRef>,
}

#[derive(Debug, Deserialize)]
struct SourceRef {
    id: String,
}

#[derive(Debug, Deserialize)]
struct ParsedTemplates {
    #[serde(default)]
    source: Vec<Source>,
    #[serde(default)]
    action: Vec<Template>,
}

fn default_true() -> bool {
    true
}

// ─────────────────────────────────────────────────────────────────────────────
// Baked catalog
// ─────────────────────────────────────────────────────────────────────────────

/// The canonical action template catalog, baked in at compile time.
///
/// Baking via `include_str!` removes the deploy-time footgun of shipping a
/// binary without its `templates.toml`. The dev-loop source of truth is
/// still `actions/templates.toml`; `cargo build` picks it up on every
/// recompile.
const TEMPLATES_TOML: &str = include_str!("../../../actions/templates.toml");

/// Cached parse of `templates.toml`. Initialized once on first access.
static CATALOG: OnceLock<ParsedTemplates> = OnceLock::new();

fn catalog() -> &'static ParsedTemplates {
    CATALOG.get_or_init(|| {
        toml::from_str(TEMPLATES_TOML)
            .unwrap_or_else(|e| panic!("failed to parse baked templates.toml: {e}"))
    })
}

/// Look up a `[[source]]` entry by its id.
pub fn lookup_source(id: &str) -> Option<&'static Source> {
    catalog().source.iter().find(|s| s.id == id)
}

/// All `[[source]]` entries sorted by `display_name`. Used by the catalog API
/// for stable UI ordering regardless of insertion order in `templates.toml`.
pub fn list_sources_sorted() -> Vec<&'static Source> {
    let mut sorted: Vec<&'static Source> = catalog().source.iter().collect();
    sorted.sort_by(|a, b| a.display_name.cmp(&b.display_name));
    sorted
}

// ─────────────────────────────────────────────────────────────────────────────
// Reconciliation
// ─────────────────────────────────────────────────────────────────────────────

/// Parse the baked template catalog and reconcile rows in `app_actions`.
/// Returns the number of rows upserted.
pub async fn reconcile_templates(db: &SqlitePool) -> Result<usize> {
    let templates = catalog();

    // GC pass: delete fan-out action rows whose credential is no longer
    // active. The revoke_credential endpoint handles this inline, but any
    // state drift (direct SQL, import, bug) leaves orphans. We nullify
    // run FKs first so history is preserved under `action_id = NULL`.
    let pruned = sqlx::query(
        r#"UPDATE app_action_runs SET action_id = NULL
           WHERE action_id IN (
               SELECT id FROM app_actions
               WHERE credential_id IS NOT NULL
                 AND credential_id NOT IN (
                     SELECT id FROM credentials WHERE status = 'active'
                 )
           )"#,
    )
    .execute(db)
    .await?
    .rows_affected();

    let deleted = sqlx::query(
        r#"DELETE FROM app_actions
           WHERE credential_id IS NOT NULL
             AND credential_id NOT IN (
                 SELECT id FROM credentials WHERE status = 'active'
             )"#,
    )
    .execute(db)
    .await?
    .rows_affected();

    if deleted > 0 {
        tracing::info!(
            deleted,
            runs_nullified = pruned,
            "reconcile GC: removed fan-out actions for inactive credentials"
        );
    }

    let mut upserted = 0usize;

    for template in &templates.action {
        if template.triggers.is_empty() {
            tracing::warn!(
                id_prefix = %template.id_prefix,
                "template has empty triggers list; skipping"
            );
            continue;
        }

        // Webhook invariant: any action accepting webhook posts MUST have a
        // credential_id so bearer auth can resolve to an identity. The only
        // way a template gets a credential_id today is via per_credential
        // fan-out; a non-per-credential webhook template would be
        // unauthenticated.
        if template.triggers.iter().any(|t| t == "webhook") && !template.per_credential {
            return Err(Error::Other(format!(
                "template {} has 'webhook' trigger but per_credential=false — webhook actions must have a credential",
                template.id_prefix
            )));
        }

        if template.per_credential {
            let source_id = template
                .source
                .as_ref()
                .map(|s| s.id.as_str())
                .ok_or_else(|| {
                    Error::Other(format!(
                        "template {} has per_credential = true but no [action.source] block",
                        template.id_prefix
                    ))
                })?;

            // Validate the source reference resolves to a [[source]] entry.
            if lookup_source(source_id).is_none() {
                return Err(Error::Other(format!(
                    "template {} references unknown source '{}' — add a [[source]] block",
                    template.id_prefix, source_id
                )));
            }

            let credential_ids: Vec<(String,)> = sqlx::query_as(
                "SELECT id FROM credentials WHERE source_id = ? AND status = 'active'",
            )
            .bind(source_id)
            .fetch_all(db)
            .await?;

            for (cred_id,) in credential_ids {
                let action_id = format!("{}_{}", template.id_prefix, cred_id);
                upsert_row(db, template, &action_id, Some(&cred_id)).await?;
                upserted += 1;
            }
        } else {
            upsert_row(db, template, &template.id_prefix, None).await?;
            upserted += 1;
        }
    }

    tracing::info!(count = upserted, "reconciled action templates");
    Ok(upserted)
}

async fn upsert_row(
    db: &SqlitePool,
    template: &Template,
    action_id: &str,
    credential_id: Option<&str>,
) -> Result<()> {
    let triggers_json = serde_json::to_string(&template.triggers)
        .map_err(|e| Error::Other(format!("failed to serialize triggers: {e}")))?;

    // Owner determines reconcile semantics:
    //
    //   system: UPSERT with overwrite of template-managed fields (name, owner,
    //           agent, condition, triggers, function_name, credential_id).
    //           Preserves user-managed fields (cron_schedule, enabled, config,
    //           memory). Every restart re-asserts the canonical definition.
    //
    //   user:   INSERT OR IGNORE. Factory defaults are seeded the first time
    //           the template is added; after that the row is fully owned by
    //           the user and reconcile is a no-op. Edits to `agent`, `name`,
    //           `triggers`, etc. survive restarts.
    let sql = if template.owner == "user" {
        r#"
        INSERT OR IGNORE INTO app_actions (
            id, name, owner, agent, cron_schedule, enabled, config, condition,
            triggers, function_name, credential_id
        )
        VALUES (?, ?, ?, ?, ?, ?, '{}', ?, ?, ?, ?)
        "#
    } else {
        r#"
        INSERT INTO app_actions (
            id, name, owner, agent, cron_schedule, enabled, config, condition,
            triggers, function_name, credential_id
        )
        VALUES (?, ?, ?, ?, ?, ?, '{}', ?, ?, ?, ?)
        ON CONFLICT(id) DO UPDATE SET
            name           = excluded.name,
            owner          = excluded.owner,
            agent          = excluded.agent,
            condition      = excluded.condition,
            triggers       = excluded.triggers,
            function_name  = excluded.function_name,
            credential_id  = excluded.credential_id,
            updated_at     = datetime('now')
        "#
    };

    sqlx::query(sql)
        .bind(action_id)
        .bind(&template.name)
        .bind(&template.owner)
        .bind(&template.agent)
        .bind(&template.default_cron)
        .bind(template.default_enabled as i64)
        .bind(&template.condition)
        .bind(&triggers_json)
        .bind(&template.function_name)
        .bind(credential_id)
        .execute(db)
        .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Golden test: the baked templates.toml must parse cleanly with the
    /// current struct shape. If this fails, a TOML edit broke schema compat.
    #[test]
    fn baked_templates_parse() {
        let _ = catalog();
    }

    #[test]
    fn ios_source_present() {
        let ios = lookup_source("ios").expect("ios source must exist in templates.toml");
        assert_eq!(ios.display_name, "iPhone");
        assert!(matches!(ios.auth, SourceAuth::SelfIssuedBearer));
        assert_eq!(ios.auth.kind_str(), "self_issued_bearer");
    }

    #[test]
    fn source_ids_unique() {
        let mut seen = std::collections::HashSet::new();
        for s in &catalog().source {
            assert!(
                seen.insert(s.id.clone()),
                "duplicate source id in templates.toml: {}",
                s.id
            );
        }
    }

    #[test]
    fn list_sorted_is_stable() {
        let names: Vec<&str> = list_sources_sorted()
            .iter()
            .map(|s| s.display_name.as_str())
            .collect();
        let mut expected = names.clone();
        expected.sort();
        assert_eq!(names, expected);
    }

    #[test]
    fn every_per_credential_action_references_known_source() {
        for tmpl in &catalog().action {
            if tmpl.per_credential {
                let src = tmpl
                    .source
                    .as_ref()
                    .unwrap_or_else(|| panic!("{} per_credential needs source", tmpl.id_prefix));
                assert!(
                    lookup_source(&src.id).is_some(),
                    "{} references unknown source '{}'",
                    tmpl.id_prefix,
                    src.id
                );
            }
        }
    }

    #[test]
    fn no_action_uses_legacy_connector_field() {
        // The legacy `connector = { id = "..." }` field is not in the Template
        // struct anymore. If a TOML row still uses it, deserialization with
        // serde's default `deny_unknown_fields = false` will silently ignore
        // it and `per_credential` validation will fail. This test verifies
        // *parse* succeeds (i.e. nobody added `connector =` back).
        let raw_toml = TEMPLATES_TOML;
        assert!(
            !raw_toml.contains("connector = {"),
            "templates.toml still references legacy `connector = {{ id = ... }}` field; rename to `source = {{ id = ... }}`"
        );
    }

    /// Reconcile must be idempotent: a second back-to-back call against the
    /// same DB and templates produces zero `app_actions` row diffs.
    ///
    /// This is the precondition for triggering reconcile from auth handlers
    /// (Phase 3 + Phase 4). If reconcile churns rows, every double-callback
    /// or refresh sweep would mutate state needlessly and break the
    /// dual-path verification window in Phase 6.
    #[tokio::test]
    async fn reconcile_is_idempotent() {
        use sqlx::sqlite::SqlitePoolOptions;

        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("in-memory sqlite");

        // Minimal schema covering the columns reconcile reads/writes.
        sqlx::query(
            r#"CREATE TABLE credentials (
                id TEXT PRIMARY KEY,
                source_id TEXT NOT NULL,
                status TEXT NOT NULL
            )"#,
        )
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            r#"CREATE TABLE app_actions (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                owner TEXT NOT NULL DEFAULT 'user',
                function_name TEXT,
                agent TEXT,
                triggers TEXT NOT NULL DEFAULT '["cron"]',
                cron_schedule TEXT,
                condition TEXT,
                enabled INTEGER NOT NULL DEFAULT 1,
                config TEXT NOT NULL DEFAULT '{}',
                memory TEXT,
                credential_id TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            )"#,
        )
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            r#"CREATE TABLE app_action_runs (
                id TEXT PRIMARY KEY,
                action_id TEXT
            )"#,
        )
        .execute(&pool)
        .await
        .unwrap();

        // Seed an active iOS credential so per_credential templates fan out.
        sqlx::query("INSERT INTO credentials (id, source_id, status) VALUES (?, 'ios', 'active')")
            .bind("cred_test_ios")
            .execute(&pool)
            .await
            .unwrap();

        // First reconcile: populates rows.
        let first = reconcile_templates(&pool).await.expect("first reconcile");
        assert!(first > 0, "first reconcile should populate some rows");

        let snapshot_before: Vec<(String, String, String, String, Option<String>)> = sqlx::query_as(
            "SELECT id, name, owner, triggers, credential_id FROM app_actions ORDER BY id",
        )
        .fetch_all(&pool)
        .await
        .unwrap();

        // Second reconcile: must produce identical row set.
        let second = reconcile_templates(&pool).await.expect("second reconcile");
        assert_eq!(
            second, first,
            "second reconcile upsert count must match first (idempotent)"
        );

        let snapshot_after: Vec<(String, String, String, String, Option<String>)> = sqlx::query_as(
            "SELECT id, name, owner, triggers, credential_id FROM app_actions ORDER BY id",
        )
        .fetch_all(&pool)
        .await
        .unwrap();

        assert_eq!(
            snapshot_before, snapshot_after,
            "row set must be byte-identical across back-to-back reconciles"
        );
    }
}
