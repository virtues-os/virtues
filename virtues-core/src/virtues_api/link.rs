//! Box-side device-link client.
//!
//! Connects this box to a paid subscription via Atlas's device-authorization
//! flow ([RFC 8628] shape) — the box never holds a Stripe key and never sees a
//! checkout page. `start` kicks off a link and stashes the secret `device_code`
//! as a box secret, so a later `poll` (possibly a separate HTTP request from
//! the web UI) can resume. `poll` returns the status; on `ready` it stores the
//! device `api_key` — AI works immediately (atlas funds the wallet at link).
//!
//! [RFC 8628]: https://www.rfc-editor.org/rfc/rfc8628

use anyhow::{anyhow, Context, Result};
use serde::Serialize;
use sqlx::PgPool;

use crate::secrets::{self, Kind};

/// Box secret holding the in-flight link's secret device_code + metadata.
const INFLIGHT_KEY: &str = "billing_link_inflight";

/// Box secret naming the link whose key this box stored most recently. Its
/// sealed value is that link's device_code, which atlas no longer redeems
/// once it has handed the key over.
const STORED_FROM_KEY: &str = "billing_link_stored";

/// What the box shows the user to complete the link. The secret `device_code`
/// is intentionally absent — it stays box-side (sealed as a box secret) and is
/// never exposed to the browser.
#[derive(Debug, Clone, Serialize)]
pub struct LinkStart {
    pub user_code: String,
    pub verification_uri: String,
    pub verification_uri_complete: String,
    pub interval: u64,
    pub expires_in: i64,
}

/// Outcome of a poll.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkStatus {
    /// Checkout not completed yet — keep polling.
    Pending,
    /// Linked: api_key stored.
    Ready,
    /// The link expired or was denied — start over.
    Expired,
    /// No link is in flight on this box.
    None,
}

/// Start a device link: ask Atlas for a device/user code pair, stash the secret
/// device_code, and return the user-facing bits.
/// Send with bounded retry on transient *transport* failures (connection
/// resets, timeouts). The box is almost always set up on captive/flaky wifi,
/// where a single blip would otherwise hard-fail onboarding with a 502 mid-
/// wizard (the exact pain point this addresses). A real HTTP response — even
/// an error status — returns immediately and is the caller's to interpret;
/// only `send()` transport errors are retried. Three attempts, ~0.3s then
/// ~1.2s backoff between them.
async fn send_with_retry<F>(make_req: F) -> reqwest::Result<reqwest::Response>
where
    F: Fn() -> reqwest::RequestBuilder,
{
    const BACKOFFS_MS: [u64; 2] = [300, 1200];
    let mut attempt = 0usize;
    loop {
        match make_req().send().await {
            Ok(resp) => return Ok(resp),
            Err(e) => {
                if attempt >= BACKOFFS_MS.len() {
                    return Err(e);
                }
                tracing::warn!(attempt = attempt + 1, error = ?e, "atlas request failed; retrying");
                tokio::time::sleep(std::time::Duration::from_millis(BACKOFFS_MS[attempt])).await;
                attempt += 1;
            }
        }
    }
}

pub async fn start(db: &PgPool, http: &reqwest::Client, atlas_url: &str) -> Result<LinkStart> {
    // Who is asking. Atlas puts this on the verification page — "Link Virtues
    // 4812 · Dragon Q6A" — so the person signing in can check the name
    // against the one on the box's own screen. That check is the mitigation
    // for code-phishing (an attacker showing THEIR code to a victim), so the
    // identity must come from the box, not be typed by the person. Every
    // field is advisory: atlas tolerates its absence (older boxes send no
    // body), and `endpoint_id` is None in the rare pre-bind race.
    let identity = serde_json::json!({
        "box": {
            "name": format!("virtues-{}", crate::codename::box_number()),
            "label": crate::api::identity::box_label(db).await,
            "model": crate::maintenance::setup_ap::is_appliance().then_some("Dragon Q6A"),
            "endpoint_id": crate::relay::box_endpoint_id(),
            "version": crate::VERSION,
        }
    });
    let resp = send_with_retry(|| {
        http.post(format!("{}/init/start", atlas_url.trim_end_matches('/')))
            .json(&identity)
    })
    .await
    .context("POST /init/start")?;
    if !resp.status().is_success() {
        let s = resp.status();
        let b = resp.text().await.unwrap_or_default();
        return Err(anyhow!("link start failed: {s} — {b}"));
    }
    let v: serde_json::Value = resp.json().await?;
    let device_code = v["device_code"]
        .as_str()
        .ok_or_else(|| anyhow!("link start response missing device_code"))?
        .to_string();
    let user_code = v["user_code"].as_str().unwrap_or("").to_string();
    let verification_uri = v["verification_uri"].as_str().unwrap_or("").to_string();
    let verification_uri_complete =
        v["verification_uri_complete"].as_str().unwrap_or("").to_string();
    let interval = v["interval"].as_u64().unwrap_or(5);
    let expires_in = v["expires_in"].as_i64().unwrap_or(900);

    // Stash the secret device_code (sealed) + public bits for a later poll.
    let meta = serde_json::json!({
        "user_code": user_code,
        "verification_uri_complete": verification_uri_complete,
        "interval": interval,
    });
    secrets::put(db, Kind::Box, INFLIGHT_KEY, &device_code, &meta).await?;

    Ok(LinkStart {
        user_code,
        verification_uri,
        verification_uri_complete,
        interval,
        expires_in,
    })
}

/// Inject an app-supplied claim grant — a pre-approved `device_code` carried
/// over BLE (RPC 0x82) — as this box's in-flight link. The normal poll loop
/// redeems it the moment the box can reach atlas; nothing else about the flow
/// changes, which is the point: the grant path reuses every line of the
/// QR-path machinery below it (poll → api key → relay config → rebind).
///
/// The stored `user_code`/`verification_uri_complete` are empty on purpose:
/// this link never had a code meant for a screen, and the display renders its
/// pending state for the seconds it takes the redeem to land.
pub async fn inject_grant(db: &PgPool, device_code: &str) -> Result<()> {
    if device_code.is_empty() {
        return Err(anyhow!("empty claim grant"));
    }
    let meta = serde_json::json!({
        "user_code": "",
        "verification_uri_complete": "",
        "interval": 3,
        "source": "app_grant",
    });
    secrets::put(db, Kind::Box, INFLIGHT_KEY, device_code, &meta).await
}

/// The public bits of the in-flight link, if one exists. `user_code` is empty
/// for app-injected grants (they never had a screen code).
///
/// This exists so the display can RESUME a link across a service restart —
/// or notice a grant the app injected over BLE — instead of minting a fresh
/// session on top of it: `start` overwrites the stored `device_code`, which
/// would orphan a code someone may be mid-redeeming on their phone. The
/// remaining TTL is not stored, so `expires_in` is a fixed optimistic guess;
/// if the link actually expired, the next poll hears `expired` from atlas and
/// clears it, which self-corrects within one interval.
pub async fn inflight(db: &PgPool) -> Result<Option<LinkStart>> {
    let Some((_device_code, meta)) = secrets::get(db, Kind::Box, INFLIGHT_KEY).await? else {
        return Ok(None);
    };
    Ok(Some(LinkStart {
        user_code: meta["user_code"].as_str().unwrap_or("").to_string(),
        verification_uri: String::new(),
        verification_uri_complete: meta["verification_uri_complete"]
            .as_str()
            .unwrap_or("")
            .to_string(),
        interval: meta["interval"].as_u64().unwrap_or(5),
        expires_in: 900,
    }))
}

/// Serializes polls inside this process. The display loop, the BLE grant
/// loop and the web UI can poll the same link at once. Without this, a poll
/// that hears `claimed` while another is still storing the key would find no
/// [`STORED_FROM_KEY`] record yet and report `Expired`. A poll from another
/// process (`virtues subscribe`) is not covered, and can still do that if
/// its answer lands in the moment between the store and the record.
static POLL_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Poll the in-flight link. On `ready`, store the api_key (atlas already
/// registered the device + funded the wallet, so AI works immediately).
/// Clears the in-flight state on any terminal outcome.
///
/// Atlas hands the key over once: the poll that sees `ready` gets it, and
/// every later poll hears `claimed`. So `Ready` means the box stored the key
/// and read it back: in this poll on `ready`, or, on `claimed`, in an earlier
/// poll that recorded it under [`STORED_FROM_KEY`]. If the key was lost on
/// the way (a dropped response, a failed write), `claimed` finds no record
/// for this link and returns `Expired`, which every caller shows as "start
/// the link again".
pub async fn poll(
    db: &PgPool,
    http: &reqwest::Client,
    atlas_url: &str,
) -> Result<LinkStatus> {
    let _serial = POLL_LOCK.lock().await;
    let Some((device_code, _meta)) = secrets::get(db, Kind::Box, INFLIGHT_KEY).await? else {
        return Ok(LinkStatus::None);
    };

    let resp = send_with_retry(|| {
        // `endpoint_id` labels the per-box key atlas mints when this poll
        // redeems a GRANTED link (attach-at-redemption; one-wire-plan). A
        // label, never authorization — atlas treats it accordingly. None in
        // the rare pre-bind race, which degrades to an unlabeled key.
        http.post(format!("{}/init/poll", atlas_url.trim_end_matches('/')))
            .json(&serde_json::json!({
                "device_code": device_code,
                "endpoint_id": crate::relay::box_endpoint_id(),
            }))
    })
    .await
    .context("POST /init/poll")?;
    if !resp.status().is_success() {
        let s = resp.status();
        let b = resp.text().await.unwrap_or_default();
        return Err(anyhow!("link poll failed: {s} — {b}"));
    }

    let v: serde_json::Value = resp.json().await?;
    match v["status"].as_str().unwrap_or("pending") {
        "ready" => {
            let api_key = v["api_key"]
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| anyhow!("link ready but no api_key"))?;
            super::renew::store_api_key(db, api_key)
                .await
                .context("store the subscription key")?;
            match super::renew::read_api_key(db).await {
                Ok(Some(stored)) if stored == api_key => {}
                Ok(_) => return Err(anyhow!("the subscription key did not read back after it was stored")),
                Err(e) => return Err(e.context("read back the subscription key")),
            }
            // Record which link the key came from, then clear the in-flight
            // row, both before the relay call below. This link gets polled
            // again when the clear fails, when the server stops between the
            // two writes, or when another process polls it at the same time.
            // That poll hears `claimed` and reads the record to tell a
            // stored key from a lost one. The key is already stored, so a
            // failed record is logged and the link still reports Ready.
            if let Err(e) =
                secrets::put(db, Kind::Box, STORED_FROM_KEY, &device_code, &serde_json::json!({})).await
            {
                tracing::warn!("failed to record which link the subscription key came from: {e:#}");
            }
            clear_inflight(db).await;
            // Provision relay reachability (best-effort): atlas returns the relay
            // URL; the box stores it for the relay subsystem. A failure (e.g.
            // relay disabled → 503) leaves `crate::relay::resolve_relay_url` on
            // its later steps: the env override, then the baked default.
            match super::relay::fetch_and_store(db, http, atlas_url, api_key).await {
                // The running endpoint keeps whatever relay it bound with (none,
                // pre-link), so ask the reach loop to rebind with the new config
                // now — the display advances to the pair code within seconds and
                // the relay must be real by then, not after the next restart.
                Ok(()) => crate::relay::request_rebind(),
                Err(e) => {
                    tracing::warn!(error = %e, "relay config provisioning skipped (LAN-only reach)")
                }
            }
            Ok(LinkStatus::Ready)
        }
        // Atlas gave the key to an earlier poll of this link. Ready only if
        // that poll stored it here. Otherwise the key never reached the vault
        // (the response was lost, or the write failed), atlas will not send
        // it again, and a key already on the box may be one this link
        // rotated out, so the link starts over. A failed read returns the
        // error and keeps the in-flight row for the next poll.
        "claimed" => {
            let stored = key_stored_from(db, &device_code).await?;
            clear_inflight(db).await;
            if stored {
                return Ok(LinkStatus::Ready);
            }
            tracing::warn!(
                "atlas reports this link claimed, but its key was never stored on this server; the link must be started again"
            );
            Ok(LinkStatus::Expired)
        }
        "expired" | "denied" => {
            clear_inflight(db).await;
            Ok(LinkStatus::Expired)
        }
        _ => Ok(LinkStatus::Pending),
    }
}

/// Outcome of a `login_start` attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LoginStart {
    /// Magic link was emailed; the box should keep polling /init/poll
    /// (same loop as Stripe Checkout flow — the device_link flips to
    /// `ready` once the user clicks the link).
    Sent,
    /// No Virtues subscription found for this email. Box should surface
    /// "no account — subscribe?" CTA to the user.
    NoAccount,
    /// Too many recent attempts from this email — wait an hour.
    RateLimited,
}

/// Initiate the magic-link login for an existing Virtues subscription.
///
/// Flow (mirrors `start` for the create-new case, just hits a different
/// atlas endpoint):
///   1. Read the in-flight device_code (must have called `start` first).
///   2. POST {device_code, email} to atlas /init/login.
///   3. Atlas resolves the customer + sends a magic link via Resend.
///   4. Box waits for the user to click; `poll` picks up status='ready'.
pub async fn login(
    db: &PgPool,
    http: &reqwest::Client,
    atlas_url: &str,
    email: &str,
) -> Result<LoginStart> {
    let Some((device_code, _meta)) = secrets::get(db, Kind::Box, INFLIGHT_KEY).await? else {
        return Err(anyhow!(
            "no in-flight link — call `link::start` first to mint a device_code"
        ));
    };

    let resp = send_with_retry(|| {
        http.post(format!("{}/init/login", atlas_url.trim_end_matches('/')))
            .json(&serde_json::json!({ "device_code": device_code, "email": email }))
    })
    .await
    .context("POST /init/login")?;

    let status = resp.status();
    let v: serde_json::Value = resp.json().await.context("/init/login non-JSON response")?;
    if !status.is_success() {
        // Atlas-side error codes we care about:
        //   rate_limited       — RateLimited
        //   no_device_link     — caller restarted state; surface as error
        //   email_send_failed  — error
        if v["error"]["code"].as_str() == Some("rate_limited") {
            return Ok(LoginStart::RateLimited);
        }
        return Err(anyhow!(
            "login start failed: {status} — {}",
            v["error"]["message"].as_str().unwrap_or("unknown")
        ));
    }

    match v["status"].as_str().unwrap_or("") {
        "sent" => Ok(LoginStart::Sent),
        "no_account" => Ok(LoginStart::NoAccount),
        other => Err(anyhow!("unexpected login status: {other}")),
    }
}

/// Whether the key this box holds came from the link `device_code`.
async fn key_stored_from(db: &PgPool, device_code: &str) -> Result<bool> {
    let Some((stored_from, _meta)) = secrets::get(db, Kind::Box, STORED_FROM_KEY).await? else {
        return Ok(false);
    };
    Ok(stored_from == device_code && super::renew::has_api_key(db).await?)
}

async fn clear_inflight(db: &PgPool) {
    if let Err(e) = secrets::delete(db, Kind::Box, INFLIGHT_KEY).await {
        tracing::warn!("failed to clear in-flight billing link: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::credentials::ensure_test_key;
    use serde_json::json;

    /// A stand-in for atlas whose `/init/poll` always answers `body`. Every
    /// other path is a 404, so the best-effort relay step is skipped.
    async fn atlas_answering(body: serde_json::Value) -> String {
        let app = axum::Router::new().route(
            "/init/poll",
            axum::routing::post(move || {
                let body = body.clone();
                async move { axum::Json(body) }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("http://{addr}")
    }

    fn http() -> reqwest::Client {
        reqwest::Client::builder().no_proxy().build().unwrap()
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn ready_stores_the_key_before_it_says_ready(pool: PgPool) {
        ensure_test_key();
        inject_grant(&pool, "device-code-example").await.unwrap();
        let atlas = atlas_answering(json!({ "status": "ready", "api_key": "vk-example" })).await;

        assert_eq!(poll(&pool, &http(), &atlas).await.unwrap(), LinkStatus::Ready);
        assert_eq!(super::super::renew::read_api_key(&pool).await.unwrap().as_deref(), Some("vk-example"));
        assert!(inflight(&pool).await.unwrap().is_none());
    }

    /// The in-flight row can outlive a stored key: the clear fails, the
    /// server stops before it, or another process polls the same link. The
    /// next poll hears `claimed` and must still say Ready.
    #[sqlx::test(migrations = "./migrations")]
    async fn claimed_after_the_key_was_stored_is_ready(pool: PgPool) {
        ensure_test_key();
        inject_grant(&pool, "device-code-example").await.unwrap();
        let ready = atlas_answering(json!({ "status": "ready", "api_key": "vk-example" })).await;
        assert_eq!(poll(&pool, &http(), &ready).await.unwrap(), LinkStatus::Ready);

        // The same link's row, back as if the clear had not happened.
        inject_grant(&pool, "device-code-example").await.unwrap();
        let claimed = atlas_answering(json!({ "status": "claimed" })).await;
        assert_eq!(poll(&pool, &http(), &claimed).await.unwrap(), LinkStatus::Ready);
        assert!(inflight(&pool).await.unwrap().is_none());
    }

    /// Atlas hands the key out once. A poll that hears `claimed` with no
    /// record that this link's key was stored means the key never reached
    /// the vault, so the box must not say it is linked.
    #[sqlx::test(migrations = "./migrations")]
    async fn claimed_without_a_stored_key_starts_over(pool: PgPool) {
        ensure_test_key();
        inject_grant(&pool, "device-code-example").await.unwrap();
        let atlas = atlas_answering(json!({ "status": "claimed" })).await;

        assert_eq!(poll(&pool, &http(), &atlas).await.unwrap(), LinkStatus::Expired);
        assert!(!super::super::renew::has_api_key(&pool).await.unwrap());
        assert_eq!(poll(&pool, &http(), &atlas).await.unwrap(), LinkStatus::None, "the in-flight row is gone");
    }

    /// Re-linking a box that already holds a key: if the new key is lost, the
    /// old one is still on the box, and atlas may have rotated it out. Holding
    /// some key, or a record for an earlier link, is not proof that this link
    /// worked.
    #[sqlx::test(migrations = "./migrations")]
    async fn an_older_key_does_not_make_a_lost_link_ready(pool: PgPool) {
        ensure_test_key();
        inject_grant(&pool, "device-code-older").await.unwrap();
        let ready = atlas_answering(json!({ "status": "ready", "api_key": "vk-older" })).await;
        assert_eq!(poll(&pool, &http(), &ready).await.unwrap(), LinkStatus::Ready);

        inject_grant(&pool, "device-code-example").await.unwrap();
        let atlas = atlas_answering(json!({ "status": "claimed" })).await;

        assert_eq!(poll(&pool, &http(), &atlas).await.unwrap(), LinkStatus::Expired);
    }
}
