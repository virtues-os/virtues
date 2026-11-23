//! Sending the account service the one answer it keeps about why the owner
//! came: what excites them most about Virtues
//! (`app_user_profile.start_with`, `api::profile::StartWith`).
//!
//! The answer is sent when this server links to an account (`link::poll`)
//! and whenever the owner changes or clears it (`api::profile::update_profile`).
//! A server with no account key sends nothing. Each send is best-effort and
//! carries the current answer, so a send that fails is corrected by the next
//! one; nothing retries on a timer.

use anyhow::{anyhow, Context, Result};
use sqlx::PgPool;

/// Send the current answer, or `null` when there is none, for the account
/// that owns `api_key`.
pub async fn send(db: &PgPool, http: &reqwest::Client, atlas_url: &str, api_key: &str) -> Result<()> {
    let start_with = crate::api::profile::get_profile(db).await?.start_with;
    let resp = http
        .put(format!("{}/account/start-with", atlas_url.trim_end_matches('/')))
        .json(&serde_json::json!({ "api_key": api_key, "start_with": start_with }))
        .send()
        .await
        .context("atlas /account/start-with request")?;
    let status = resp.status();
    if !status.is_success() {
        return Err(anyhow!("atlas /account/start-with returned {status}"));
    }
    Ok(())
}

/// Send the current answer if this server is linked to an account. Logs a
/// failure and returns: the owner's own save has already succeeded.
pub async fn send_if_linked(db: PgPool) {
    let api_key = match super::renew::read_api_key(&db).await {
        Ok(Some(key)) => key,
        // Not linked: there is no account to keep the answer with.
        Ok(None) => return,
        Err(e) => {
            tracing::warn!("start_with not sent, the account key is unreadable: {e}");
            return;
        }
    };
    let http = crate::http_client::virtues_api_client();
    if let Err(e) = send(&db, &http, &super::atlas_url(), &api_key).await {
        tracing::warn!("start_with not sent: {e:#}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};
    use std::sync::{Arc, Mutex};

    /// A stand-in for atlas that records the one body it is sent.
    async fn atlas_recording(status: u16) -> (String, Arc<Mutex<Option<Value>>>) {
        use axum::{extract::State, http::StatusCode, routing::put, Json, Router};
        let seen = Arc::new(Mutex::new(None));
        let app = Router::new()
            .route(
                "/account/start-with",
                put(move |State(seen): State<Arc<Mutex<Option<Value>>>>, Json(body): Json<Value>| async move {
                    *seen.lock().unwrap() = Some(body);
                    StatusCode::from_u16(status).unwrap()
                }),
            )
            .with_state(seen.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (url, seen)
    }

    /// No proxy, so a machine-wide proxy setting cannot catch the loopback call.
    fn http() -> reqwest::Client {
        reqwest::Client::builder().no_proxy().build().unwrap()
    }

    #[sqlx::test]
    async fn it_sends_the_answer_then_null_once_cleared(pool: PgPool) {
        let http = http();
        let (url, seen) = atlas_recording(200).await;

        sqlx::query("UPDATE app_user_profile SET start_with = 'memory'").execute(&pool).await.unwrap();
        send(&pool, &http, &url, "vk-example").await.expect("send the answer");
        assert_eq!(seen.lock().unwrap().take(), Some(json!({ "api_key": "vk-example", "start_with": "memory" })));

        sqlx::query("UPDATE app_user_profile SET start_with = NULL").execute(&pool).await.unwrap();
        send(&pool, &http, &url, "vk-example").await.expect("send the clear");
        assert_eq!(seen.lock().unwrap().take(), Some(json!({ "api_key": "vk-example", "start_with": null })));
    }

    #[sqlx::test]
    async fn a_refusal_from_atlas_is_an_error(pool: PgPool) {
        let (url, _seen) = atlas_recording(401).await;
        let sent = send(&pool, &http(), &url, "vk-example").await;
        assert!(sent.is_err(), "{sent:?}");
    }
}
