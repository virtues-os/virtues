//! Box-side rendezvous publisher.
//!
//! When the box's public endpoint changes (ISP prefix rotation), it re-encrypts
//! the endpoint under its per-box key K and PUTs the opaque blob to the blind
//! rendezvous, so paired phones can relearn it on their next handshake failure.
//! The box is otherwise dark — it publishes *on change*, holds no connection.
//!
//! [`publish`] (the encrypt-and-PUT) is cross-platform and composes the pieces
//! already built (the rendezvous blob crypto + `BearerClient`). The change
//! *detector* that triggers it — a netlink `RTM_NEWADDR`/`RTM_DELADDR` watch on
//! the WAN interface, debounced — is Linux-only and lives with the staging WG
//! integration (it needs real netlink + a real prefix change to validate).

use anyhow::{anyhow, Context, Result};
use base64::Engine;
use sqlx::PgPool;

use super::pairing::ensure_rendezvous_identity;
use crate::virtues_api::client::BearerClient;
use crate::virtues_api::rendezvous::{encrypt_endpoint, EndpointBlob};

/// 1b publish loop (spawned by the server): periodically read the box's current
/// endpoint — recorded by the `virtues-wireguard` daemon in `box_secrets` — and
/// publish it to the rendezvous **when it changes**. The app owns this because
/// it holds the bearer; the daemon only *detects* the endpoint. No-op on a
/// core-only box (no endpoint recorded → nothing to publish, no bearer call).
pub async fn run_publish_loop(db: PgPool) {
    let mut last: Option<crate::wireguard::endpoint::Endpoint> = None;
    loop {
        match crate::wireguard::endpoint::read_current(&db).await {
            Ok(Some(ep)) if last.as_ref() != Some(&ep) => {
                match publish(&db, &ep.ip, ep.port, &ep.wg_pub).await {
                    Ok(()) => {
                        tracing::info!("rendezvous: published endpoint update");
                        last = Some(ep);
                    }
                    Err(e) => tracing::warn!("rendezvous publish failed: {e:#}"),
                }
            }
            Ok(_) => {}
            Err(e) => tracing::warn!("rendezvous endpoint read failed: {e:#}"),
        }
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
    }
}

/// Encrypt the box's current endpoint under K and publish it to the rendezvous.
/// Cheap and idempotent; callers debounce and publish only on an actual change.
/// `ip`/`port` are the box's current public WG endpoint; `wg_pub` is the box's
/// WG public key (lets the phone repin if the server key ever rotates).
pub async fn publish(db: &PgPool, ip: &str, port: u16, wg_pub: &str) -> Result<()> {
    let identity = ensure_rendezvous_identity(db).await?;

    let key_bytes = base64::engine::general_purpose::STANDARD
        .decode(&identity.key_b64)
        .context("decode rendezvous K")?;
    let key: [u8; 32] = key_bytes
        .as_slice()
        .try_into()
        .map_err(|_| anyhow!("rendezvous K must be 32 bytes"))?;

    let blob = EndpointBlob {
        v: 1,
        ip: ip.to_string(),
        port,
        wg_pub: wg_pub.to_string(),
        ts: chrono::Utc::now().timestamp(),
    };
    let ciphertext = encrypt_endpoint(&key, &blob)?;

    let path = format!("/v1/rendezvous/{}", identity.publish_id);
    let status = BearerClient::from_env(db.clone())
        .put_bytes(&path, ciphertext)
        .await?;
    if !(200..300).contains(&status) {
        return Err(anyhow!("rendezvous publish failed: HTTP {status}"));
    }
    tracing::info!("published endpoint to rendezvous");
    Ok(())
}
