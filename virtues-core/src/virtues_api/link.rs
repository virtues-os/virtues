//! Box-side device-link client.
//!
//! Connects this box to a paid subscription via Atlas's device-authorization
//! flow ([RFC 8628] shape) — the box never holds a Stripe key and never sees a
//! checkout page. `start` kicks off a link and stashes the secret `device_code`
//! in `box_secrets`, so a later `poll` (possibly a separate HTTP request from
//! the web UI) can resume. `poll` returns the status; on `ready` it stores the
//! device `api_key` — AI works immediately (atlas funds the wallet at link).
//!
//! [RFC 8628]: https://www.rfc-editor.org/rfc/rfc8628

use anyhow::{anyhow, Context, Result};
use serde::Serialize;
use sqlx::PgPool;

use crate::box_secrets;

/// box_secrets key holding the in-flight link's secret device_code + metadata.
const INFLIGHT_KEY: &str = "billing_link_inflight";

/// What the box shows the user to complete the link. The secret `device_code`
/// is intentionally absent — it stays box-side (sealed in `box_secrets`) and is
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
    // Who is asking. Atlas puts this on the verification page — "Link Honest
    // Kestrel · Dragon Q6A" — so the person signing in can check the name
    // against the one on the box's own screen. That check is the mitigation
    // for code-phishing (an attacker showing THEIR code to a victim), so the
    // identity must come from the box, not be typed by the person. Every
    // field is advisory: atlas tolerates its absence (older boxes send no
    // body), and `endpoint_id` is None in the rare pre-bind race.
    let name = crate::codename::box_codename();
    let identity = serde_json::json!({
        "box": {
            "name": name,
            "label": crate::codename::pretty(&name),
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
    box_secrets::put(db, INFLIGHT_KEY, &device_code, &meta).await?;

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
    box_secrets::put(db, INFLIGHT_KEY, device_code, &meta).await
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
    let Some((_device_code, meta)) = box_secrets::get(db, INFLIGHT_KEY).await? else {
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

/// Poll the in-flight link. On `ready`, store the api_key (atlas already
/// registered the device + funded the wallet, so AI works immediately).
/// Clears the in-flight state on any terminal outcome.
pub async fn poll(
    db: &PgPool,
    http: &reqwest::Client,
    atlas_url: &str,
) -> Result<LinkStatus> {
    let Some((device_code, _meta)) = box_secrets::get(db, INFLIGHT_KEY).await? else {
        return Ok(LinkStatus::None);
    };

    let resp = send_with_retry(|| {
        http.post(format!("{}/init/poll", atlas_url.trim_end_matches('/')))
            .json(&serde_json::json!({ "device_code": device_code }))
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
            super::renew::store_api_key(db, api_key).await?;
            // Provision relay reachability (best-effort): atlas mints this box's
            // per-SNI token; the box stores it for the relay subsystem. A failure
            // (e.g. relay disabled → 503) just leaves the box reachable on LAN.
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
            clear_inflight(db).await;
            Ok(LinkStatus::Ready)
        }
        // Already retrieved on a prior poll (e.g. crash before clearing) — the
        // api_key is stored; just converge.
        "claimed" => {
            clear_inflight(db).await;
            Ok(LinkStatus::Ready)
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
    let Some((device_code, _meta)) = box_secrets::get(db, INFLIGHT_KEY).await? else {
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

async fn clear_inflight(db: &PgPool) {
    if let Err(e) = sqlx::query("DELETE FROM box_secrets WHERE key = $1")
        .bind(INFLIGHT_KEY)
        .execute(db)
        .await
    {
        tracing::warn!("failed to clear in-flight billing link: {e}");
    }
}
