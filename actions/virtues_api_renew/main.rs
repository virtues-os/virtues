//! virtues_api_renew — refresh this server's Virtues API access bearer.
//!
//! Runs the voucher dance: reads the billing token from the local vault,
//! asks Atlas for a one-time voucher, redeems it at virtues-api for a fresh
//! bearer, and stores the new bearer back in the vault. Triggered lazily by
//! the virtues-api client on a `bearer_expired` (402), or manually.
//!
//! Transparency by design: this action is visible in the action list, and
//! this source is the complete description of what it sends. Atlas sees a
//! billing token + a voucher (never the bearer). virtues-api sees a voucher
//! + a bearer (never the customer). Neither sees both halves.

use anyhow::Result;
use virtues::virtues_api::renew;
use virtues_helpers::{connect_from_env, output, read_input};

#[tokio::main]
async fn main() -> Result<()> {
    virtues_actions::init_tracing();

    let input = read_input()?;
    let pool = connect_from_env("virtues-action-virtues_api_renew").await?;

    let atlas_url = std::env::var("ATLAS_URL").unwrap_or_else(|_| "http://localhost:9100".into());
    let api_url =
        std::env::var("VIRTUES_API_URL").unwrap_or_else(|_| "http://localhost:9002".into());
    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()?;

    let result = renew::renew(&pool, &http, &atlas_url, &api_url).await?;

    let summary = format!(
        "renewed Virtues API access — bearer valid until {}",
        result.expires_at.to_rfc3339()
    );
    output(&summary, &input.config)
}
