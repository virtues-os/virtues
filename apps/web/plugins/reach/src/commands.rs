use tauri::{command, AppHandle, Runtime};

use crate::models::{DiscoverResponse, FoundServer, PairRequest, ReachStatus};
use crate::ReachExt;
use crate::Result;
use virtues_reach_client::outbox;

#[command]
pub(crate) async fn pair<R: Runtime>(
  app: AppHandle<R>,
  payload: PairRequest,
) -> Result<ReachStatus> {
  app.reach().pair(&payload.server, &payload.code).await
}

#[command]
pub(crate) async fn reach_status<R: Runtime>(app: AppHandle<R>) -> Result<ReachStatus> {
  Ok(app.reach().status().await)
}

#[command]
pub(crate) async fn forget<R: Runtime>(app: AppHandle<R>) -> Result<()> {
  app.reach().forget()
}

#[command]
pub(crate) async fn discover<R: Runtime>(_app: AppHandle<R>) -> Result<DiscoverResponse> {
  // Subnet scan (mDNS-free) — reliable across iOS's flaky Bonjour + APs that
  // filter multicast. Returns boxes as IP origins.
  let ips = virtues_reach_client::local_private_ipv4s();
  let servers = virtues_reach_client::scan_subnet()
    .await
    .into_iter()
    .map(|b| FoundServer {
      name: b.name,
      origin: b.origin,
    })
    .collect::<Vec<_>>();
  let debug = if ips.is_empty() {
    "no LAN IP — turn on Local Network in Settings, or check Wi-Fi".to_string()
  } else {
    format!("scanned from {}", ips.join(", "))
  };
  Ok(DiscoverResponse { servers, debug })
}

/// Sync-queue health for a stream (device screen's Sync section).
#[command]
pub(crate) async fn outbox_stats<R: Runtime>(
  _app: AppHandle<R>,
  stream: String,
) -> Result<outbox::OutboxStats> {
  Ok(outbox::stats(&stream).unwrap_or_default())
}

/// Drain the outbox to the box immediately ("Sync now"). Returns records sent.
#[command]
pub(crate) async fn drain_now<R: Runtime>(app: AppHandle<R>) -> Result<usize> {
  app.reach().drain_now().await
}
