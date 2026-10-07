//! The CLI's door into the server: `POST /api/console/tool/:tool`.
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
    Router::new().route("/api/console/tool/:tool", post(console_tool_handler))
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
    let outcome = exec.execute(&tool, arguments, &ToolContext::default()).await;

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

fn refuse(status: StatusCode, message: &str) -> Response {
    (status, Json(serde_json::json!({ "success": false, "error": message }))).into_response()
}
