//! Smoke tests for vault helpers against an in-memory SQLite.
//!
//! Verifies:
//!   - mint_pending_credential writes a pending row
//!   - finalize_self_issued_bearer flips it to active and sets lookup_hash
//!   - finalize_credential idempotency (double-callback dedup)
//!   - finalize_apikey_credential writes an active row directly
//!   - mark_credential_status transitions cleanly
//!   - fanout_action_ids returns the right map
//!
//! Tests are `#[serial]` because they share the `VIRTUES_ENCRYPTION_KEY`
//! env var.

use base64::Engine;
use serde_json::json;
use serial_test::serial;
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::SqlitePool;
use virtues_helpers::auth::{
    fanout_action_ids, finalize_apikey_credential, finalize_credential,
    finalize_self_issued_bearer, mark_credential_status, mint_pending_credential,
    read_credential_secrets, update_credential_secrets, CredentialStatus,
};

async fn fresh_db() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("in-memory sqlite");

    sqlx::query(
        r#"CREATE TABLE credentials (
            id TEXT PRIMARY KEY,
            source_id TEXT NOT NULL,
            name TEXT NOT NULL,
            status TEXT NOT NULL,
            status_reason TEXT,
            secrets_ciphertext TEXT NOT NULL,
            secret_lookup_hash TEXT,
            scopes TEXT,
            expires_at TEXT,
            next_refresh_at TEXT,
            metadata TEXT NOT NULL DEFAULT '{}',
            last_seen_at TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        )"#,
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        r#"CREATE TABLE app_actions (
            id TEXT PRIMARY KEY,
            function_name TEXT,
            credential_id TEXT
        )"#,
    )
    .execute(&pool)
    .await
    .unwrap();

    pool
}

fn ensure_test_key() {
    if std::env::var("VIRTUES_ENCRYPTION_KEY").is_err() {
        let key = base64::engine::general_purpose::STANDARD.encode([0u8; 32]);
        std::env::set_var("VIRTUES_ENCRYPTION_KEY", key);
    }
}

#[tokio::test]
#[serial]
async fn mint_then_finalize_self_issued_bearer() {
    ensure_test_key();
    let db = fresh_db().await;

    let cred_id = mint_pending_credential(&db, "ios", "My iPhone").await.unwrap();

    let (status, lookup_hash): (String, Option<String>) =
        sqlx::query_as("SELECT status, secret_lookup_hash FROM credentials WHERE id = ?")
            .bind(&cred_id)
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(status, "pending");
    assert!(lookup_hash.is_none());

    finalize_self_issued_bearer(&db, &cred_id, "device-token-xyz", &json!({"device": "iPhone 15"}))
        .await
        .unwrap();

    let (status, lookup_hash): (String, Option<String>) =
        sqlx::query_as("SELECT status, secret_lookup_hash FROM credentials WHERE id = ?")
            .bind(&cred_id)
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(status, "active");
    assert!(lookup_hash.is_some());
    assert_eq!(lookup_hash.unwrap().len(), 64);
}

#[tokio::test]
#[serial]
async fn finalize_credential_dedups_double_callback() {
    ensure_test_key();
    let db = fresh_db().await;

    let cred_id = mint_pending_credential(&db, "example", "test@example.com").await.unwrap();

    finalize_credential(
        &db,
        &cred_id,
        &json!({"access_token": "tok-1", "refresh_token": "ref-1"}),
        &json!({"email": "test@example.com"}),
        Some(3600),
        Some(&["calendar.readonly".to_string(), "mail.readonly".to_string()]),
    )
    .await
    .unwrap();

    let status: String = sqlx::query_scalar("SELECT status FROM credentials WHERE id = ?")
        .bind(&cred_id)
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(status, "active");

    finalize_credential(
        &db,
        &cred_id,
        &json!({"access_token": "tok-2"}),
        &json!({}),
        Some(3600),
        None,
    )
    .await
    .expect("second finalize should dedup, not error");

    let expires_at: Option<String> =
        sqlx::query_scalar("SELECT expires_at FROM credentials WHERE id = ?")
            .bind(&cred_id)
            .fetch_one(&db)
            .await
            .unwrap();
    assert!(expires_at.is_some());
}

#[tokio::test]
#[serial]
async fn finalize_credential_rejects_unknown_id() {
    ensure_test_key();
    let db = fresh_db().await;

    let err = finalize_credential(
        &db,
        "cred_does_not_exist",
        &json!({}),
        &json!({}),
        None,
        None,
    )
    .await
    .unwrap_err();

    assert_eq!(err.http_status(), 404);
}

#[tokio::test]
#[serial]
async fn apikey_credential_is_active_immediately() {
    ensure_test_key();
    let db = fresh_db().await;

    let cred_id = finalize_apikey_credential(
        &db,
        "mcp:test",
        "test PAT",
        &json!({"token": "test_xxxxxxxxxxxxxxxxxxxx"}),
    )
    .await
    .unwrap();

    let status: String = sqlx::query_scalar("SELECT status FROM credentials WHERE id = ?")
        .bind(&cred_id)
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(status, "active");
}

#[tokio::test]
#[serial]
async fn mark_status_transitions() {
    ensure_test_key();
    let db = fresh_db().await;

    let cred_id =
        finalize_apikey_credential(&db, "mcp:test", "test", &json!({"token": "x"})).await.unwrap();

    mark_credential_status(&db, &cred_id, CredentialStatus::ReauthRequired, Some("token_expired"))
        .await
        .unwrap();

    let (status, reason): (String, Option<String>) =
        sqlx::query_as("SELECT status, status_reason FROM credentials WHERE id = ?")
            .bind(&cred_id)
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(status, "reauth_required");
    assert_eq!(reason.as_deref(), Some("token_expired"));

    mark_credential_status(&db, &cred_id, CredentialStatus::Revoked, Some("user_revoked"))
        .await
        .unwrap();

    let status: String = sqlx::query_scalar("SELECT status FROM credentials WHERE id = ?")
        .bind(&cred_id)
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(status, "revoked");
}

#[tokio::test]
#[serial]
async fn refresh_path_round_trips_secrets() {
    ensure_test_key();
    let db = fresh_db().await;

    // Set up an active OAuth-shaped credential.
    let cred_id = mint_pending_credential(&db, "google", "adam@example.com")
        .await
        .unwrap();
    finalize_credential(
        &db,
        &cred_id,
        &json!({"access_token": "AT_v1", "refresh_token": "RT_v1"}),
        &json!({"email": "adam@example.com"}),
        Some(3600),
        Some(&["calendar.readonly".to_string()]),
    )
    .await
    .unwrap();

    // Read it back via the refresh-path helper.
    let secrets = read_credential_secrets(&db, &cred_id).await.unwrap();
    assert_eq!(secrets["access_token"], "AT_v1");
    assert_eq!(secrets["refresh_token"], "RT_v1");

    // Simulate a successful proxy_refresh: write new tokens.
    update_credential_secrets(
        &db,
        &cred_id,
        &json!({"access_token": "AT_v2", "refresh_token": "RT_v2"}),
        Some(7200),
    )
    .await
    .unwrap();

    // Status stays active; secrets are the new ones.
    let status: String = sqlx::query_scalar("SELECT status FROM credentials WHERE id = ?")
        .bind(&cred_id)
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(status, "active");

    let secrets = read_credential_secrets(&db, &cred_id).await.unwrap();
    assert_eq!(secrets["access_token"], "AT_v2");
    assert_eq!(secrets["refresh_token"], "RT_v2");
}

#[tokio::test]
#[serial]
async fn update_credential_secrets_rejects_unknown_id() {
    ensure_test_key();
    let db = fresh_db().await;

    let err = update_credential_secrets(&db, "cred_does_not_exist", &json!({}), None)
        .await
        .unwrap_err();

    assert_eq!(err.http_status(), 404);
}

#[tokio::test]
#[serial]
async fn fanout_action_ids_returns_map() {
    ensure_test_key();
    let db = fresh_db().await;

    let cred_id = mint_pending_credential(&db, "ios", "test").await.unwrap();

    for fname in ["ios_healthkit", "ios_location", "ios_eventkit"] {
        sqlx::query("INSERT INTO app_actions (id, function_name, credential_id) VALUES (?, ?, ?)")
            .bind(format!("action_{fname}_{cred_id}"))
            .bind(fname)
            .bind(&cred_id)
            .execute(&db)
            .await
            .unwrap();
    }

    let map = fanout_action_ids(&db, &cred_id).await.unwrap();
    assert_eq!(map.len(), 3);
    assert!(map.contains_key("ios_healthkit"));
    assert!(map.contains_key("ios_location"));
    assert!(map.contains_key("ios_eventkit"));
    assert_eq!(map["ios_healthkit"], format!("action_ios_healthkit_{cred_id}"));
}
