//! `delete_data`: the assistant proposes deleting ingested records.
//!
//! The tool deletes nothing. It records what the owner asked to remove
//! (`api::deletions::propose`) and ends the turn; the chat draws a card from
//! the deletion's id, the owner allows it there with the app's owner proof,
//! and the server does the work. A model cannot allow its own proposal: no
//! tool argument does that, and the allow route is not a tool.
//!
//! The result carries each table's count and dates and never a row's
//! content, because a tool result is stored with the chat. The card reads
//! sample rows from the server while the deletion waits, and they go when
//! the rows do.

use serde_json::{json, Value};
use sqlx::PgPool;

use super::{ToolContext, ToolResult};
use crate::api::deletions::{self, Target};

pub const TOOL: &str = "delete_data";

pub async fn run(pool: &PgPool, args: &Value, context: &ToolContext) -> ToolResult {
    // Where an earlier proposal stands.
    if let Some(id) = args.get("deletion_id").and_then(|v| v.as_str()).map(str::trim).filter(|s| !s.is_empty()) {
        return match deletions::get(pool, id).await {
            Ok(d) => ToolResult::success(json!({
                "deletion_id": d.id,
                "summary": d.summary,
                "status": d.status,
                "receipt": d.receipt,
                "error": d.error,
            })),
            Err(e) => ToolResult::error(e.to_string()),
        };
    }

    // Only where the owner is present to see the card. An applet's run or a
    // background task has nobody to allow it.
    let Some(chat_id) = context.chat_id.as_deref() else {
        return ToolResult::error("Deleting data needs the owner in a chat to allow it. Nothing was proposed.");
    };
    if context.applet_id.is_some() || context.task_id.is_some() {
        return ToolResult::error(
            "Deleting data needs the owner in a chat to allow it. This run has nobody watching, so nothing was proposed.",
        );
    }

    let summary = args.get("summary").and_then(|v| v.as_str()).unwrap_or("");
    let targets: Vec<Target> = match args.get("targets").cloned().map(serde_json::from_value) {
        Some(Ok(targets)) => targets,
        Some(Err(e)) => return ToolResult::error(format!("`targets` is not a list of {{table, where}} or {{table, ids}}: {e}")),
        None => return ToolResult::error("Name what to delete in `targets`."),
    };

    // A ghost chat is never written down, and a chat's row may not exist
    // until its first turn is saved. Either way the deletion stands on its
    // own, without a chat to belong to.
    let stored_chat = match sqlx::query_scalar::<_, bool>("SELECT EXISTS (SELECT 1 FROM app_chats WHERE id = $1)")
        .bind(chat_id)
        .fetch_one(pool)
        .await
    {
        Ok(true) if !context.temporary => Some(chat_id),
        Ok(_) => None,
        Err(e) => return ToolResult::error(format!("Couldn't read the chat: {e}")),
    };

    match deletions::propose(pool, stored_chat, summary, &targets).await {
        Ok((id, previews)) => ToolResult::success(json!({
            "deletion_id": id,
            // The agent loop stops the turn on this (`agent::turn::needs_user`).
            "awaiting_owner": true,
            "summary": summary.trim(),
            "targets": previews.iter().map(|p| json!({
                "table": p.table,
                "label": p.label,
                "count": p.count,
                "earliest": p.earliest,
                "latest": p.latest,
            })).collect::<Vec<_>>(),
            "note": "Nothing is deleted yet. The owner sees a card with what this covers and allows it or not. \
                     Your turn ends here; do not call again for the same request.",
        })),
        Err(e) => ToolResult::error(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn seed(pool: &PgPool) {
        crate::server::faces::ensure_applet_db_grants(pool).await.unwrap();
        sqlx::query(
            "INSERT INTO data_health_heart_rate (id, bpm, occurred_at, source_stream_id, source_table, source_provider) \
             VALUES ('hr_a', 60, '2026-03-03T10:00:00Z', 'ios:hr:a', 'stream_ios_healthkit', 'ios')",
        )
        .execute(pool)
        .await
        .unwrap();
    }

    fn args() -> Value {
        json!({
            "summary": "heart rate from March 3",
            "targets": [{ "table": "data_health_heart_rate", "where": "occurred_at < '2026-03-04'" }],
        })
    }

    /// The call records a proposal, stops the turn for the owner, and leaves
    /// every row where it was. Its result has counts and no row content.
    #[sqlx::test]
    async fn the_call_proposes_and_deletes_nothing(pool: PgPool) {
        seed(&pool).await;
        let ctx = ToolContext { chat_id: Some("chat_not_saved_yet".into()), ..Default::default() };
        let result = run(&pool, &args(), &ctx).await;
        assert!(result.success, "{:?}", result.error);
        assert_eq!(result.data["awaiting_owner"], true);
        assert_eq!(result.data["targets"][0]["count"], 1);
        assert!(result.data["targets"][0].get("samples").is_none());

        let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM data_health_heart_rate").fetch_one(&pool).await.unwrap();
        assert_eq!(rows, 1);
        let (status, chat_id): (String, Option<String>) =
            sqlx::query_as("SELECT status, chat_id FROM app_deletions WHERE id = $1")
                .bind(result.data["deletion_id"].as_str().unwrap())
                .fetch_one(&pool)
                .await
                .unwrap();
        // The chat has no row yet, so the proposal stands without one.
        assert_eq!((status.as_str(), chat_id), ("proposed", None));

        // Asking where it stands does not stop the turn again.
        let check = run(&pool, &json!({ "deletion_id": result.data["deletion_id"] }), &ctx).await;
        assert_eq!(check.data["status"], "proposed");
        assert!(check.data.get("awaiting_owner").is_none());
        crate::database::restricted::close_pools(&pool).await;
    }

    /// Nobody is watching an applet's run or a background task, so neither
    /// may propose a deletion.
    #[sqlx::test]
    async fn a_run_with_nobody_watching_proposes_nothing(pool: PgPool) {
        seed(&pool).await;
        for ctx in [
            ToolContext::default(),
            ToolContext { chat_id: Some("chat_a".into()), applet_id: Some("applet_a".into()), ..Default::default() },
            ToolContext { chat_id: Some("chat_a".into()), task_id: Some("task_a".into()), ..Default::default() },
        ] {
            let result = run(&pool, &args(), &ctx).await;
            assert!(!result.success);
        }
        let proposals: i64 = sqlx::query_scalar("SELECT count(*) FROM app_deletions").fetch_one(&pool).await.unwrap();
        assert_eq!(proposals, 0);
    }
}
