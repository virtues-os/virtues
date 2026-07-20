//! Applet faces — sandboxed-iframe HTML runtime.
//!
//! A face is `face/index.html` inside the applet's folder, rendered by the
//! web app in an `<iframe sandbox="allow-scripts">`. The sandbox (no
//! `allow-same-origin`) gives the document an **opaque origin**, so it holds
//! no cookies/storage and cannot read the parent. The box injects two
//! runtime files: `virtues.css` (theme variables) and `virtues.js` (a scoped
//! read-only `virtues.query(sql)` bridge).
//!
//! ## Trust model
//!
//! - Transport (proven iroh peer) is the outer wall, as for every route.
//! - The **inner wall is CORS**: only the routes in this module answer with
//!   `Access-Control-Allow-Origin: *`, so fetches from the opaque-origin
//!   iframe can read *these* responses and nothing else on the API.
//! - Data access needs a **face token** (minted by the authenticated app
//!   per iframe load, short-lived, bound to one applet) and executes as the
//!   `virtues_face_reader` PG role — default-deny, SELECT granted only on
//!   `data_*` / `wiki_*` tables and `applet_*` schemas — inside a READ ONLY
//!   transaction with a statement timeout.
//! - Face documents get a strict CSP: no external hosts, ever.

use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use super::webhook::AppState;

// ============================================================================
// Face tokens
// ============================================================================

const TOKEN_TTL: Duration = Duration::from_secs(60 * 60); // one iframe session
const MAX_ROWS: usize = 5000;

struct FaceToken {
    action_id: String,
    expires: Instant,
}

fn token_store() -> &'static Mutex<HashMap<String, FaceToken>> {
    static STORE: OnceLock<Mutex<HashMap<String, FaceToken>>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn mint_token(action_id: &str) -> String {
    let token = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    let mut store = token_store().lock().expect("face token lock poisoned");
    // Opportunistic GC so the map can't grow unboundedly.
    store.retain(|_, t| t.expires > Instant::now());
    store.insert(
        token.clone(),
        FaceToken {
            action_id: action_id.to_string(),
            expires: Instant::now() + TOKEN_TTL,
        },
    );
    token
}

fn validate_token(token: &str) -> Option<String> {
    let store = token_store().lock().expect("face token lock poisoned");
    store
        .get(token)
        .filter(|t| t.expires > Instant::now())
        .map(|t| t.action_id.clone())
}

/// `GET /api/actions/:id/face-token` — minted by the authenticated app per
/// iframe load. (Reaching this route at all required the proven transport.)
pub async fn mint_face_token_handler(Path(action_id): Path<String>) -> Response {
    if face_dir_for(&action_id).is_none() {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "applet has no face" })),
        )
            .into_response();
    }
    let token = mint_token(&action_id);
    Json(serde_json::json!({
        "token": token,
        "expires_in_seconds": TOKEN_TTL.as_secs(),
    }))
    .into_response()
}

// ============================================================================
// Face file serving
// ============================================================================

/// Resolve the on-disk `face/` directory for an applet id, if it has one.
/// Serving is rooted at `<applet folder>/face` — never the folder root, so
/// the manifest/prompt are not exposed to the iframe.
pub fn face_dir_for(action_id: &str) -> Option<std::path::PathBuf> {
    let dir = crate::action_templates::dir_for_action_id(action_id)?;
    let face = crate::action_templates::actions_root().join(dir).join("face");
    face.join("index.html").is_file().then_some(face)
}

fn cors_headers(headers: &mut HeaderMap) {
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_ORIGIN,
        HeaderValue::from_static("*"),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static("authorization, content-type"),
    );
}

/// Strict CSP for face documents: same-URL-origin subresources only, no
/// external hosts. `connect-src 'self'` lets the bridge reach the box; CORS
/// (absent everywhere but this module) blocks reading anything else.
const FACE_CSP: &str = "default-src 'none'; script-src 'self' 'unsafe-inline'; \
     style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; \
     font-src 'self' data:; connect-src 'self'";

/// `GET /face/:action_id/` and `GET /face/:action_id/*path` — static files
/// from the applet's `face/` directory. `virtues.js` / `virtues.css` resolve
/// from the injected runtime, shadowing any local file of the same name.
pub async fn face_file_handler(
    Path((action_id, path)): Path<(String, String)>,
) -> Response {
    serve_face_file(&action_id, &path).await
}

pub async fn face_index_handler(Path(action_id): Path<String>) -> Response {
    serve_face_file(&action_id, "index.html").await
}

async fn serve_face_file(action_id: &str, raw_path: &str) -> Response {
    let rel = if raw_path.is_empty() { "index.html" } else { raw_path };

    // Injected runtime files shadow local ones.
    if rel == "virtues.js" {
        return static_lib(VIRTUES_JS, "application/javascript; charset=utf-8");
    }
    if rel == "virtues.css" {
        return static_lib(VIRTUES_CSS, "text/css; charset=utf-8");
    }

    let Some(face_dir) = face_dir_for(action_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    // Path-traversal guard: reject any segment that isn't a plain name.
    if rel
        .split('/')
        .any(|seg| seg.is_empty() || seg == "." || seg == ".." || seg.contains('\\'))
    {
        return StatusCode::BAD_REQUEST.into_response();
    }

    let full = face_dir.join(rel);
    let Ok(bytes) = tokio::fs::read(&full).await else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let mime = match full.extension().and_then(|e| e.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "application/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("woff2") => "font/woff2",
        _ => "application/octet-stream",
    };

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(mime).unwrap_or(HeaderValue::from_static("application/octet-stream")),
    );
    cors_headers(&mut headers);
    if mime.starts_with("text/html") {
        headers.insert(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static(FACE_CSP),
        );
    }
    (headers, bytes).into_response()
}

fn static_lib(body: &'static str, mime: &'static str) -> Response {
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(mime));
    cors_headers(&mut headers);
    (headers, body).into_response()
}

// ============================================================================
// The query bridge
// ============================================================================

#[derive(Deserialize)]
pub struct FaceQueryBody {
    pub sql: String,
}

#[derive(Deserialize)]
pub struct FaceQueryParams {
    pub vt: Option<String>,
}

/// `POST /api/face/query?vt=<token>` — the one data door for faces.
///
/// Executes the SQL as `virtues_face_reader` (default-deny grants) inside a
/// READ ONLY, rolled-back transaction with a statement timeout, the session
/// timezone set to home_timezone, and a hard row cap. The result is
/// aggregated to JSON in SQL (`json_agg(row_to_json(...))`) so no generic
/// row decoding is needed.
pub async fn face_query_handler(
    State(state): State<AppState>,
    Query(params): Query<FaceQueryParams>,
    headers: HeaderMap,
    Json(body): Json<FaceQueryBody>,
) -> Response {
    // Token from ?vt= or Authorization: Bearer.
    let token = params.vt.or_else(|| {
        headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .map(|s| s.to_string())
    });
    let Some(action_id) = token.as_deref().and_then(validate_token) else {
        return with_cors((
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "invalid or expired face token" })),
        ));
    };

    let pool = state.db.pool();
    match run_face_query(pool, &body.sql).await {
        Ok(rows) => with_cors((StatusCode::OK, Json(rows))),
        Err(e) => {
            tracing::debug!(action_id = %action_id, error = %e, "face query failed");
            with_cors((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": e })),
            ))
        }
    }
}

/// CORS preflight for the query endpoint (opaque-origin fetch sends one).
pub async fn face_query_preflight() -> Response {
    let mut headers = HeaderMap::new();
    cors_headers(&mut headers);
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static("POST, OPTIONS"),
    );
    (headers, StatusCode::NO_CONTENT).into_response()
}

fn with_cors(resp: impl IntoResponse) -> Response {
    let mut r = resp.into_response();
    cors_headers(r.headers_mut());
    r
}

async fn run_face_query(pool: &sqlx::PgPool, sql: &str) -> std::result::Result<serde_json::Value, String> {
    if sql.len() > 20_000 {
        return Err("query too long".into());
    }
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    sqlx::query("SET TRANSACTION READ ONLY")
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    sqlx::query("SET LOCAL statement_timeout = '5s'")
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    // Set the timezone BEFORE dropping to face_reader — the profile table is
    // app_*, which face_reader can't read. Reading it after the role switch
    // would error and poison the whole transaction ("current transaction is
    // aborted"), failing every face query. The pool role can read it here.
    sqlx::query(
        "SELECT set_config('timezone', COALESCE(\
             (SELECT home_timezone FROM app_user_profile LIMIT 1), \
             current_setting('timezone')), true)",
    )
    .execute(&mut *tx)
    .await
    .ok();
    sqlx::query("SET LOCAL ROLE virtues_face_reader")
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;

    let wrapped = format!(
        "SELECT COALESCE(json_agg(row_to_json(q)), '[]'::json) FROM \
         (SELECT * FROM ({sql}) qq LIMIT {MAX_ROWS}) q"
    );
    let value: serde_json::Value = sqlx::query_scalar(&wrapped)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    tx.rollback().await.ok();
    Ok(value)
}

/// Idempotent grants for the two applet DB roles. Called at server boot and
/// after each setup_applet schema apply.
///
/// - `virtues_face_reader`: SELECT on `data_*` / `wiki_*` tables and every
///   `applet_*` schema (the faces' read-only bridge).
/// - `virtues_applet_writer`: DML strictly inside `applet_*` schemas (the
///   `sql_write` tool) — the write scope is PG grants, not SQL parsing.
pub async fn ensure_applet_db_grants(pool: &sqlx::PgPool) -> crate::error::Result<()> {
    // Role MEMBERSHIP must hold for the *connected* login role, or SET LOCAL
    // ROLE fails ("permission denied to set role") and every face query and
    // applet write breaks. Migrations 0052/0054 grant membership to whoever
    // ran them — which on a cloud/externally-managed DB may differ from the
    // app's pool role. Re-establish it here, at boot, as the pool role itself.
    sqlx::query(
        "GRANT virtues_face_reader, virtues_applet_writer TO current_user",
    )
    .execute(pool)
    .await
    .map_err(|e| crate::error::Error::Database(format!("applet role membership grant failed: {e}")))?;

    sqlx::query(
        r#"
        DO $$
        DECLARE t record;
        BEGIN
            FOR t IN
                SELECT schemaname, tablename FROM pg_tables
                WHERE (schemaname = 'public'
                       AND (tablename LIKE 'data\_%' OR tablename LIKE 'wiki\_%'))
                   OR schemaname LIKE 'applet\_%'
            LOOP
                EXECUTE format('GRANT SELECT ON %I.%I TO virtues_face_reader',
                               t.schemaname, t.tablename);
            END LOOP;
            FOR t IN
                SELECT schemaname, tablename FROM pg_tables
                WHERE schemaname LIKE 'applet\_%'
            LOOP
                EXECUTE format(
                    'GRANT SELECT, INSERT, UPDATE, DELETE ON %I.%I TO virtues_applet_writer',
                    t.schemaname, t.tablename);
            END LOOP;
            FOR t IN
                SELECT nspname AS schemaname, NULL::text AS tablename
                FROM pg_namespace WHERE nspname LIKE 'applet\_%'
            LOOP
                EXECUTE format('GRANT USAGE ON SCHEMA %I TO virtues_face_reader',
                               t.schemaname);
                EXECUTE format('GRANT USAGE ON SCHEMA %I TO virtues_applet_writer',
                               t.schemaname);
                EXECUTE format(
                    'GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA %I TO virtues_applet_writer',
                    t.schemaname);
            END LOOP;
        END $$
        "#,
    )
    .execute(pool)
    .await
    .map_err(|e| crate::error::Error::Database(format!("applet db grants failed: {e}")))?;
    Ok(())
}

// ============================================================================
// Injected runtime
// ============================================================================

const VIRTUES_JS: &str = r#"// virtues.js — the face runtime bridge (read-only).
(function () {
  const params = new URL(location.href).searchParams;
  const token = params.get('vt') || '';
  const theme = params.get('theme') || 'light';
  document.documentElement.dataset.theme = theme;

  async function query(sql) {
    const res = await fetch(`/api/face/query?vt=${encodeURIComponent(token)}`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ sql }),
    });
    const body = await res.json();
    if (!res.ok) throw new Error(body.error || `query failed (${res.status})`);
    return body;
  }

  window.virtues = { query, theme };
})();
"#;

const VIRTUES_CSS: &str = r#"/* virtues.css — face theme variables + minimal base. */
:root {
  --color-surface: #ffffff;
  --color-surface-elevated: #f3f4f6;
  --color-foreground: #111827;
  --color-foreground-subtle: #6b7280;
  --color-border: #e5e7eb;
  --color-accent: #1d4ed8;
  --color-success: #047857;
  --color-error: #b91c1c;
  color-scheme: light;
}
:root[data-theme="dark"] {
  --color-surface: #0b0f1a;
  --color-surface-elevated: #161b28;
  --color-foreground: #e5e7eb;
  --color-foreground-subtle: #9ca3af;
  --color-border: #253044;
  --color-accent: #60a5fa;
  --color-success: #34d399;
  --color-error: #f87171;
  color-scheme: dark;
}
html, body {
  margin: 0;
  padding: 0;
  background: var(--color-surface);
  color: var(--color-foreground);
  font: 14px/1.5 -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
}
"#;
