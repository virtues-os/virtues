//! The CLI's door into the server: `POST /api/console/tool/:tool`, and
//! `POST /api/console/pages/convert` for `virtues pages convert --apply`.
//!
//! The CLI's read verbs run their tools in the CLI process. Its write verbs
//! cannot: a page edit has to go through the live Yjs document this process
//! holds, `setup_applet` reloads this process's applet catalog, and
//! `run_applet` dispatches through this process's runner. A write made from
//! another process would land in the database behind all three, and the next
//! save of an open page would put it back.
//!
//! So the CLI sends the tool here and the server runs it through the same
//! executor chat uses. Only the local console may call it: a process on the
//! box itself, over loopback, which is what `virtues` on the box is. Each call
//! logs one audit line, which is the record of what an outside agent changed.
//! The plan is agents/plan/cli-data-verbs-plan.md.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use sha2::{Digest, Sha256};

use crate::middleware::auth::{AuthUser, CONSOLE_DEVICE_ID};
use crate::server::AppState;
use crate::tools::{ToolContext, ToolExecutor, CLI_TOOLS, CLI_WRITE_TOOLS};

/// The agent key a call arrived on (`cli/agent_key.rs`), for the audit line.
/// Trusted only as far as an audit label: only the console reaches this door.
pub const AGENT_KEY_HEADER: &str = "x-virtues-agent-key";

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/console/tool/:tool", post(console_tool_handler))
        .route("/api/console/pages/convert", post(convert_pages_handler))
}

async fn console_tool_handler(
    State(state): State<AppState>,
    user: AuthUser,
    headers: axum::http::HeaderMap,
    Path(tool): Path<String>,
    Json(arguments): Json<serde_json::Value>,
) -> Response {
    // A paired phone is the owner too, but it has the app; this door exists
    // for the process sitting on the box.
    if user.device_id != CONSOLE_DEVICE_ID {
        return refuse(StatusCode::FORBIDDEN, "the console tool door takes local callers only");
    }
    // Writes always come here; reads come here from an agent key, whose user
    // has no database of its own.
    if !CLI_WRITE_TOOLS.contains(&tool.as_str()) && !CLI_TOOLS.contains(&tool.as_str()) {
        return refuse(StatusCode::NOT_FOUND, &format!("{tool} is not a CLI verb"));
    }
    let key = headers.get(AGENT_KEY_HEADER).and_then(|v| v.to_str().ok()).unwrap_or("").to_string();

    let args_sha = {
        let mut h = Sha256::new();
        h.update(arguments.to_string().as_bytes());
        format!("{:x}", h.finalize())
    };
    let exec = ToolExecutor::new_with_yjs(state.db.pool().clone(), state.yjs_state.clone());
    let context = ToolContext { console: true, ..Default::default() };
    let outcome = exec.execute(&tool, arguments, &context).await;

    let (status, body) = match outcome {
        Ok(result) => {
            let ok = result.success;
            let code = if ok { StatusCode::OK } else { StatusCode::UNPROCESSABLE_ENTITY };
            (code, serde_json::json!(result))
        }
        Err(e) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            serde_json::json!({ "success": false, "error": e.to_string() }),
        ),
    };
    tracing::info!(
        audit = "console_tool",
        tool = %tool,
        agent_key = %key,
        args_sha = %args_sha,
        status = status.as_u16(),
        "console tool call"
    );
    (status, Json(body)).into_response()
}

/// What `virtues pages convert --apply` asks of the server.
#[derive(Debug, Default, serde::Deserialize)]
struct ConvertPages {
    /// The database the CLI was pointed at: the run is refused unless this
    /// server serves it, and when none is named (`page_convert::apply_on`).
    #[serde(default)]
    database: Option<crate::api::page_convert::DatabaseIdentity>,
    /// User pages or articles. A CLI from before articles moved names none,
    /// and moves user pages as it always did.
    #[serde(default)]
    which: crate::api::page_convert::Which,
}

/// `virtues pages convert --apply`: move every markdown user page, or every
/// opened article, to blocks in this process, which holds the pages' live
/// docs and the saves queued for them (`api::page_convert`). The answer is
/// the run's report.
///
/// The run is its own task, so a caller that hangs up (Ctrl-C on the CLI)
/// does not stop it between a page's flip and the retiring of its markdown
/// doc; it finishes, and keeps its report on the box.
async fn convert_pages_handler(
    State(state): State<AppState>,
    user: AuthUser,
    Json(ask): Json<ConvertPages>,
) -> Response {
    convert_pages(state.yjs_state.clone(), &user, ask).await
}

/// [`convert_pages_handler`] on the docs `yjs` holds: the console alone
/// moves pages, and a run refused (another database, block pages off) is a
/// 422 saying why.
async fn convert_pages(yjs: crate::server::yjs::YjsState, user: &AuthUser, ask: ConvertPages) -> Response {
    if user.device_id != CONSOLE_DEVICE_ID {
        return refuse(StatusCode::FORBIDDEN, "the console door takes local callers only");
    }
    let run = tokio::spawn(async move {
        crate::api::page_convert::apply_on(&yjs, ask.database.as_ref(), ask.which).await
    });
    let ran = match run.await {
        Ok(ran) => ran,
        Err(e) => return refuse(StatusCode::INTERNAL_SERVER_ERROR, &format!("the conversion run stopped: {e}")),
    };
    match ran {
        Ok(report) => {
            tracing::info!(
                audit = "console_pages_convert",
                which = ?report.which,
                converted = report.converted,
                failed = report.failed,
                "console moved markdown pages to blocks"
            );
            (StatusCode::OK, Json(report)).into_response()
        }
        Err(crate::Error::InvalidInput(why)) => refuse(StatusCode::UNPROCESSABLE_ENTITY, &why),
        Err(e) => refuse(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

fn refuse(status: StatusCode, message: &str) -> Response {
    (status, Json(serde_json::json!({ "success": false, "error": message }))).into_response()
}

#[cfg(test)]
mod tests {
    //! The console door is one of the gates that keep the one-way move to
    //! blocks from running: only a process on the box opens it.

    use super::*;
    use crate::api::pages::{create_page_as, CreatePageRequest, PageFormat};
    use crate::server::yjs::YjsState;
    use sqlx::PgPool;

    async fn markdown_page(pool: &PgPool) -> String {
        let req = CreatePageRequest {
            title: "Notes".into(),
            content: "Lunch.\n".into(),
            project_id: None,
            icon: None,
            icon_color: None,
            cover_url: None,
            tags: None,
            format: None,
        };
        create_page_as(pool, req, PageFormat::Markdown).await.unwrap().page.id
    }

    async fn format(pool: &PgPool, id: &str) -> String {
        sqlx::query_scalar("SELECT format FROM app_pages WHERE id = $1").bind(id).fetch_one(pool).await.unwrap()
    }

    async fn answer(response: Response) -> (StatusCode, serde_json::Value) {
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        (status, serde_json::from_slice(&bytes).unwrap())
    }

    fn caller(device_id: &str) -> AuthUser {
        AuthUser { id: "owner".into(), device_id: device_id.into(), device_label: "test".into() }
    }

    /// A paired phone is the owner too, and still not the console: refused,
    /// and no page moves.
    #[sqlx::test]
    async fn the_move_is_refused_to_a_paired_device(pool: PgPool) {
        let id = markdown_page(&pool).await;
        let yjs = YjsState::new(pool.clone());
        let (status, body) = answer(convert_pages(yjs, &caller("phone-1"), ConvertPages::default()).await).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
        assert_eq!(format(&pool, &id).await, "markdown");
    }

    /// A run the server refuses (meant for another database, or with block
    /// pages off, as tests leave them) is a 422 saying why, and moves
    /// nothing.
    #[sqlx::test]
    async fn a_refused_run_says_why_and_moves_nothing(pool: PgPool) {
        let id = markdown_page(&pool).await;
        let yjs = YjsState::new(pool.clone());
        let ours = crate::api::page_convert::database_identity(&pool).await.unwrap();
        let elsewhere = ConvertPages {
            database: Some(crate::api::page_convert::DatabaseIdentity { name: "virtues_doc_dryrun".into(), ..ours.clone() }),
            which: Default::default(),
        };
        let (status, body) = answer(convert_pages(yjs.clone(), &caller(CONSOLE_DEVICE_ID), elsewhere).await).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
        assert!(body["error"].as_str().unwrap().contains("virtues_doc_dryrun"), "{body}");
        let (status, body) = answer(convert_pages(yjs.clone(), &caller(CONSOLE_DEVICE_ID), ConvertPages::default()).await).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
        assert_eq!(body["error"], crate::api::page_convert::NO_DATABASE_NAMED, "{body}");
        if !crate::api::pages::tree_pages_enabled() {
            let named = ConvertPages { database: Some(ours), which: Default::default() };
            let (status, body) = answer(convert_pages(yjs, &caller(CONSOLE_DEVICE_ID), named).await).await;
            assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
            assert_eq!(body["error"], crate::api::page_convert::BLOCK_PAGES_OFF, "{body}");
        }
        assert_eq!(format(&pool, &id).await, "markdown");
    }

    /// The door passes the kind of page the CLI asks to move: a CLI from
    /// before articles moved names none and moves user pages; one asking for
    /// articles is refused while block articles are off, as tests leave
    /// them, and moves nothing.
    #[sqlx::test]
    async fn console_passes_which(pool: PgPool) {
        use crate::api::page_convert::Which;
        let older: ConvertPages = serde_json::from_value(serde_json::json!({ "database": null })).unwrap();
        assert_eq!(older.which, Which::Pages);
        let ask: ConvertPages = serde_json::from_value(serde_json::json!({ "which": "articles" })).unwrap();
        assert_eq!(ask.which, Which::Articles);

        let article = crate::api::wiki_articles::create_article(&pool, "person", "p_1", "Nick", "Nick rows.\n")
            .await
            .unwrap();
        let yjs = YjsState::new(pool.clone());
        let ours = crate::api::page_convert::database_identity(&pool).await.unwrap();
        let ask = ConvertPages { database: Some(ours), which: Which::Articles };
        let (status, body) = answer(convert_pages(yjs, &caller(CONSOLE_DEVICE_ID), ask).await).await;
        if !crate::api::block_move::tree_articles_enabled() {
            assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
            let why = body["error"].as_str().unwrap();
            assert!(
                why == crate::api::page_convert::ARTICLES_OFF || why == crate::api::page_convert::BLOCK_PAGES_OFF,
                "{body}"
            );
        }
        assert_eq!(format(&pool, &article.page_id).await, "markdown");
    }
}
