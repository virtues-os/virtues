//! `virtues_helpers::conversations::write_messages` against the real schema.
//!
//! The writer's column list is an untyped string, so a column the schema no
//! longer has fails only at run time, on the first real import or sync. Unit
//! tests of the parsers never reach it. This writes through the migrated
//! schema and checks that a second write of the same message adds nothing.

use chrono::{TimeZone, Utc};
use sqlx::PgPool;
use virtues_helpers::conversations::{stream_id, write_messages, ConversationMsg};

fn msg(message_id: &str, role: &str, content: &str) -> ConversationMsg {
    ConversationMsg {
        stream_id: stream_id("grok", "c-test", message_id),
        conversation_id: "c-test".into(),
        message_id: message_id.into(),
        role: role.into(),
        content: content.into(),
        provider: "grok".into(),
        timestamp: Utc.with_ymd_and_hms(2031, 5, 1, 11, 58, 0).unwrap(),
        source_table: "grok_chats".into(),
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn messages_write_once_against_the_schema(pool: PgPool) {
    let batch = [
        msg("r-1", "user", "How long to paddle 20 km?"),
        msg("r-2", "assistant", "About five hours at a relaxed pace."),
    ];
    assert_eq!(write_messages(&pool, &batch).await.unwrap(), 2);
    assert_eq!(write_messages(&pool, &batch).await.unwrap(), 0, "a second write must add nothing");

    let (n, source_table): (i64, String) = sqlx::query_as(
        "SELECT count(*), max(source_table) FROM data_content_conversation WHERE conversation_id = 'c-test'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(n, 2);
    assert_eq!(source_table, "grok_chats");
}
