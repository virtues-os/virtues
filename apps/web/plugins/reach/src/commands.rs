use tauri::{command, AppHandle, Runtime};

use crate::models::{DiscoverResponse, FoundServer, PairRequest, ProvisionJoinResult, ReachStatus};
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
      claimed: b.claimed,
    })
    .collect::<Vec<_>>();
  let debug = if ips.is_empty() {
    "no LAN IP — turn on Local Network in Settings, or check Wi-Fi".to_string()
  } else {
    format!("scanned from {}", ips.join(", "))
  };
  Ok(DiscoverResponse { servers, debug })
}

// ─── Wifi provisioning over the box's setup AP ───────────────────────────────
//
// These three drive an APPLIANCE through wifi setup from the app, while the
// phone is joined to the box's own `Virtues-XXXX` network. See
// `virtues_reach_client::provision` for why this runs in the app at all rather
// than being left to the box's captive portal — short version: the owner's home
// wifi password deserves a native field, and the app can hold the setup session
// across the network handoff that follows.
//
// They go through Rust, not `fetch` in the webview, for the same reason `pair`
// does: plain HTTP to `10.42.0.1` from a `tauri://` origin is what App
// Transport Security exists to block.

/// Is this box in setup mode and reachable from where we are standing?
///
/// The box's own gates answer it: `/api/provision/*` exists only for a caller
/// on the AP subnet talking to an *unclaimed* box, so a 200 means both at once.
/// Cheap enough to run against every candidate the subnet scan turned up.
#[command]
pub(crate) async fn provision_open<R: Runtime>(
  _app: AppHandle<R>,
  server: String,
) -> Result<bool> {
  Ok(virtues_reach_client::provision::is_open(&crate::normalize_server(&server)).await)
}

/// Networks the BOX can see — not the phone's list. Different antenna, possibly
/// a different room; offering the phone's would let someone pick a network the
/// box cannot hear and produce a failure with no explanation.
#[command]
pub(crate) async fn provision_networks<R: Runtime>(
  _app: AppHandle<R>,
  server: String,
) -> Result<Vec<virtues_reach_client::provision::Network>> {
  virtues_reach_client::provision::networks(&crate::normalize_server(&server))
    .await
    .map_err(crate::Error::from)
}

/// Put the box on the owner's network.
///
/// Returns one of `"joined" | "failed" | "unknown"`, and **`"unknown"` is the
/// expected outcome, not an edge case**: the box drops its AP as the first step
/// of the join, so the phone issuing this request usually loses its socket
/// mid-flight — on the success path as often as the failure path. The caller
/// must treat it as "go and look", never as an error.
#[command]
pub(crate) async fn provision_join<R: Runtime>(
  _app: AppHandle<R>,
  server: String,
  ssid: String,
  psk: Option<String>,
  identity: Option<String>,
) -> Result<ProvisionJoinResult> {
  use virtues_reach_client::provision::JoinOutcome;
  let outcome = virtues_reach_client::provision::join_full(
    &crate::normalize_server(&server),
    &ssid,
    psk.as_deref(),
    identity.as_deref(),
  )
  .await
  .map_err(crate::Error::from)?;
  Ok(match outcome {
    JoinOutcome::Joined => ProvisionJoinResult { outcome: "joined".into(), detail: None },
    JoinOutcome::Failed(d) => {
      ProvisionJoinResult { outcome: "failed".into(), detail: Some(d) }
    }
    JoinOutcome::Unknown => ProvisionJoinResult { outcome: "unknown".into(), detail: None },
  })
}

// ─── Improv BLE setup — the primary path on EVERY platform ──────────────────
//
// The box serves the Improv Wi-Fi GATT service while unclaimed (virtues-core
// `maintenance::ble_provision`); this drives it. The client never leaves its
// own network, and the join is watched live over BLE instead of inferred from
// a dead socket.
//
// TWO implementations, ONE command surface:
//   * iOS      → Swift CoreBluetooth (`ios/Sources/ImprovClient.swift`)
//   * desktop  → `virtues_improv::client` (btleplug: CoreBluetooth, WinRT, BlueZ)
// Both speak the wire format in `virtues-improv::protocol`, and both answer to
// the same command names with the same JSON — which is what lets the connect
// shell be one file rather than one per platform. Android has neither yet and
// says so.
//
// Desktop is not a fallback here. A Mac is expected to be the FIRST device an
// appliance ever meets: it has the keyboard that 802.1X credentials and an
// email address want, and its dev loop is `tauri dev` rather than a device
// deploy — which is most of why this exists.

/// The desktop client, and the small amount of glue that makes its results
/// look exactly like the Swift plugin's.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod desktop {
  use super::*;

  pub(super) fn client() -> &'static virtues_improv::ImprovClient {
    virtues_improv::ImprovClient::shared()
  }

  /// Mirror the Swift plugin's `trigger("improv-progress", …)`. Tauri's
  /// `addPluginListener('reach', 'improv-progress', …)` listens on this exact
  /// event name, so one JS listener serves both platforms.
  pub(super) fn progress<R: Runtime>(app: &AppHandle<R>, stage: &str) {
    use tauri::Emitter;
    let _ = app.emit("plugin:reach|improv-progress", serde_json::json!({ "stage": stage }));
  }
}

/// Scan for unclaimed boxes advertising Improv. Returns `{boxes: [...]}` with
/// `id` (opaque, for the calls below), `name`, `improvState` (0x02 = needs
/// wifi, 0x04 = already online), `rssi`.
#[command]
pub(crate) async fn improv_discover<R: Runtime>(
  app: AppHandle<R>,
  seconds: Option<f64>,
) -> Result<serde_json::Value> {
  #[cfg(target_os = "ios")]
  {
    use tauri::Manager;
    let handle = app.state::<crate::IosPluginHandle<R>>();
    return handle
      .0
      .run_mobile_plugin("improv_discover", serde_json::json!({ "seconds": seconds }))
      .map_err(|e| crate::Error::Reach(e.to_string()));
  }
  #[cfg(not(any(target_os = "ios", target_os = "android")))]
  {
    let _ = &app;
    // A scan that FAILS (no adapter, Bluetooth off, permission refused) must
    // not look like a scan that found nothing. They are the same picture on a
    // screen that keeps searching, and that ambiguity is precisely what makes
    // Bluetooth setup unfalsifiable in the field — you cannot tell "no box
    // here" from "this machine cannot see any box, ever". The reason rides
    // back in the payload so the shell can say which.
    let (boxes, error) = match desktop::client().discover(seconds.unwrap_or(4.0)).await {
      Ok(b) => (b, None),
      Err(e) => {
        let msg = format!("{e:#}");
        tracing::info!(error = %msg, "improv: bluetooth discovery unavailable");
        (Vec::new(), Some(msg))
      }
    };
    // TEMPORARY (2026-08-13): the app has been rendering "[Virtues Honest
    // Kestrel" while the box advertises "Virtues-Honest-Kestrel", and two
    // rounds of guessing at the JS end fixed nothing. Print the exact bytes
    // CoreBluetooth handed us, so the next change is based on the value rather
    // than a theory about it. Debug-formatted: a stray control character or a
    // real bracket look identical in a plain print.
    return Ok(serde_json::json!({
      "boxes": boxes.into_iter().map(|b| serde_json::json!({
        "id": b.id,
        "name": b.name,
        "improvState": b.improv_state,
        "rssi": b.rssi,
      })).collect::<Vec<_>>(),
      "error": error,
    }));
  }
  #[cfg(target_os = "android")]
  {
    let _ = (app, seconds);
    Ok(serde_json::json!({ "boxes": [] }))
  }
}

/// This machine's name, for the box's panel — "Adam's Mac", not a hostname if
/// we can help it.
///
/// It travels with the setup claim and REPLACES the phrase on the box's screen,
/// so it is read by someone standing at the box deciding whether the session
/// that just started is theirs. That makes a recognisable name worth a
/// subprocess; `scutil` gives the name the owner chose in System Settings,
/// while `hostname` gives its mangled DNS form.
fn this_device_label() -> String {
  // Mobile sandboxes forbid spawning anything, and setup is a desktop job now
  // — the phone joins later as a second device, by which point the box has a
  // name for it from pairing.
  if cfg!(mobile) {
    return String::new();
  }
  #[cfg(target_os = "macos")]
  {
    if let Ok(out) = std::process::Command::new("scutil").args(["--get", "ComputerName"]).output() {
      let name = String::from_utf8_lossy(&out.stdout).trim().to_string();
      if !name.is_empty() {
        return name;
      }
    }
  }
  if let Ok(out) = std::process::Command::new("hostname").output() {
    let name = String::from_utf8_lossy(&out.stdout).trim().trim_end_matches(".local").to_string();
    if !name.is_empty() {
      return name;
    }
  }
  // The panel copes with an empty label — it just says a device is setting up.
  String::new()
}

/// Claim the setup session with the box's four-word phrase (Improv RPC 0x86).
///
/// Must succeed before wifi, the account grant, or pairing: an unclaimed box
/// advertises to everyone in radio range, and radio range passes through walls.
/// The phrase is on the box's own panel, so having it proves line of sight.
#[command]
pub(crate) async fn improv_claim<R: Runtime>(
  app: AppHandle<R>,
  id: String,
  phrase: String,
) -> Result<serde_json::Value> {
  let label = this_device_label();
  #[cfg(target_os = "ios")]
  {
    use tauri::Manager;
    let handle = app.state::<crate::IosPluginHandle<R>>();
    return handle
      .0
      .run_mobile_plugin(
        "improv_claim",
        serde_json::json!({ "id": id, "phrase": phrase, "label": label }),
      )
      .map_err(|e| crate::Error::Reach(e.to_string()));
  }
  #[cfg(not(any(target_os = "ios", target_os = "android")))]
  {
    let _ = &app;
    // `gated: false` — the box is older than the phrase gate and asked for
    // nothing. The UI skips the "save these words" step, because on that box
    // there are no words to save.
    return Ok(match desktop::client().claim_setup(&id, &phrase, &label).await {
      Ok(gated) => serde_json::json!({ "ok": true, "gated": gated }),
      Err(e) => serde_json::json!({ "ok": false, "error": format!("{e:#}") }),
    });
  }
  #[cfg(target_os = "android")]
  {
    let _ = (app, id, phrase, label);
    Err(crate::Error::Reach("Bluetooth setup isn't available on Android yet".into()))
  }
}

/// Ask the box for its account-link code (Improv RPC 0x84).
///
/// Replaces printing that code on the box's own panel. The app has already
/// proved line of sight with the phrase and holds the setup session, so handing
/// it over that session is both tighter — the glass proves only that you are in
/// the room — and better: the app opens the link page with the code already in
/// the URL, so the owner types nothing.
///
/// `{ ok: true, code: null }` when the box has no link in flight: already
/// linked, or no internet yet to start one.
#[command]
pub(crate) async fn improv_link_code<R: Runtime>(
  app: AppHandle<R>,
  id: String,
) -> Result<serde_json::Value> {
  #[cfg(target_os = "ios")]
  {
    use tauri::Manager;
    let handle = app.state::<crate::IosPluginHandle<R>>();
    return handle
      .0
      .run_mobile_plugin("improv_link_code", serde_json::json!({ "id": id }))
      .map_err(|e| crate::Error::Reach(e.to_string()));
  }
  #[cfg(not(any(target_os = "ios", target_os = "android")))]
  {
    let _ = &app;
    return Ok(match desktop::client().link_code(&id).await {
      Ok(Some((code, url))) => serde_json::json!({ "ok": true, "code": code, "url": url }),
      Ok(None) => serde_json::json!({ "ok": true, "code": null }),
      Err(e) => serde_json::json!({ "ok": false, "error": format!("{e:#}") }),
    });
  }
  #[cfg(target_os = "android")]
  {
    let _ = (app, id);
    Err(crate::Error::Reach("Bluetooth setup isn't available on Android yet".into()))
  }
}

/// Ask the box for its standing pair code, over BLE (Improv RPC 0x85).
///
/// `code: null` = the box has none to give, or predates the RPC. Both mean the
/// same thing to the caller: fall back to the six digits on the panel.
#[command]
pub(crate) async fn improv_pair_code<R: Runtime>(
  app: AppHandle<R>,
  id: String,
) -> Result<serde_json::Value> {
  #[cfg(target_os = "ios")]
  {
    use tauri::Manager;
    let handle = app.state::<crate::IosPluginHandle<R>>();
    return handle
      .0
      .run_mobile_plugin("improv_pair_code", serde_json::json!({ "id": id }))
      .map_err(|e| crate::Error::Reach(e.to_string()));
  }
  #[cfg(not(any(target_os = "ios", target_os = "android")))]
  {
    let _ = &app;
    return Ok(match desktop::client().pair_code(&id).await {
      Ok(Some(code)) => serde_json::json!({ "ok": true, "code": code }),
      Ok(None) => serde_json::json!({ "ok": true, "code": null }),
      Err(e) => serde_json::json!({ "ok": false, "error": format!("{e:#}") }),
    });
  }
  #[cfg(target_os = "android")]
  {
    let _ = (app, id);
    Err(crate::Error::Reach("Bluetooth setup isn't available on Android yet".into()))
  }
}

/// Ask THAT BOX what wifi it can see, over BLE (Improv RPC 0x04).
#[command]
pub(crate) async fn improv_wifi_scan<R: Runtime>(
  app: AppHandle<R>,
  id: String,
) -> Result<serde_json::Value> {
  #[cfg(target_os = "ios")]
  {
    use tauri::Manager;
    let handle = app.state::<crate::IosPluginHandle<R>>();
    return handle
      .0
      .run_mobile_plugin("improv_wifi_scan", serde_json::json!({ "id": id }))
      .map_err(|e| crate::Error::Reach(e.to_string()));
  }
  #[cfg(not(any(target_os = "ios", target_os = "android")))]
  {
    let _ = &app;
    // Same shape the Swift side returns: an error rides IN the payload, so
    // the shell renders the list-with-a-reason rather than a thrown promise.
    return Ok(match desktop::client().wifi_scan(&id).await {
      Ok(nets) => serde_json::json!({
        "networks": nets.into_iter().map(|n| serde_json::json!({
          "ssid": n.ssid,
          "signal": n.signal,
          "secured": n.secured,
          "enterprise": n.enterprise,
        })).collect::<Vec<_>>()
      }),
      Err(e) => serde_json::json!({ "networks": [], "error": format!("{e:#}") }),
    });
  }
  #[cfg(target_os = "android")]
  {
    let _ = (app, id);
    Err(crate::Error::Reach("Bluetooth setup isn't available on Android yet".into()))
  }
}

/// Send credentials and watch the join (Improv RPC 0x01, or 0x81 when
/// `identity` is present — 802.1X). Progress arrives as `improv-progress`
/// plugin events; the returned value is the outcome.
#[command]
pub(crate) async fn improv_provision<R: Runtime>(
  app: AppHandle<R>,
  id: String,
  ssid: String,
  password: String,
  identity: Option<String>,
) -> Result<serde_json::Value> {
  #[cfg(target_os = "ios")]
  {
    use tauri::Manager;
    let handle = app.state::<crate::IosPluginHandle<R>>();
    return handle
      .0
      .run_mobile_plugin(
        "improv_provision",
        serde_json::json!({ "id": id, "ssid": ssid, "password": password, "identity": identity }),
      )
      .map_err(|e| crate::Error::Reach(e.to_string()));
  }
  #[cfg(not(any(target_os = "ios", target_os = "android")))]
  {
    let handle = app.clone();
    let identity = identity.filter(|i| !i.is_empty());
    return Ok(
      match desktop::client()
        .provision(&id, &ssid, &password, identity.as_deref(), move |stage| {
          desktop::progress(&handle, stage)
        })
        .await
      {
        Ok(url) => serde_json::json!({ "ok": true, "url": url }),
        Err(e) => serde_json::json!({ "ok": false, "error": format!("{e:#}") }),
      },
    );
  }
  #[cfg(target_os = "android")]
  {
    let _ = (app, id, ssid, password, identity);
    Err(crate::Error::Reach("Bluetooth setup isn't available on Android yet".into()))
  }
}

/// Pair THROUGH the box's Bluetooth (our Improv RPC 0x83): the box redeems
/// the pair code against its own consume endpoint and streams the response
/// back; this command then persists the pairing exactly as the LAN path
/// does. Exists because the LAN leg dies on client-isolated networks (an
/// office blocked phone→box HTTP on the same wifi, live, 2026-08-11).
/// Returns `{ok: true, status}` (a `ReachStatus`) or `{ok: false, error}`.
#[command]
pub(crate) async fn improv_pair<R: Runtime>(
  app: AppHandle<R>,
  id: String,
  code: String,
) -> Result<serde_json::Value> {
  // Key custody stays in Rust on BOTH platforms: mint here, hand only the
  // public EndpointId to the radio layer, persist the box's relayed consume
  // response through the same code the LAN path uses.
  let identity = virtues_reach_client::pair::mint_identity();
  let (kind, label) = if cfg!(mobile) {
    ("mobile_app", "Virtues Mobile")
  } else {
    ("desktop_app", "Virtues Desktop")
  };

  #[cfg(target_os = "ios")]
  let body: String = {
    use tauri::Manager;
    let handle = app.state::<crate::IosPluginHandle<R>>();
    let resp: serde_json::Value = handle
      .0
      .run_mobile_plugin(
        "improv_pair",
        serde_json::json!({
          "id": id,
          "code": code,
          "label": label,
          "endpointId": identity.node_id,
        }),
      )
      .map_err(|e| crate::Error::Reach(e.to_string()))?;
    if resp.get("ok").and_then(|v| v.as_bool()) != Some(true) {
      // The Swift layer's error is already user-facing words — pass through.
      return Ok(resp);
    }
    resp.get("response").and_then(|v| v.as_str()).unwrap_or("").to_string()
  };

  #[cfg(not(any(target_os = "ios", target_os = "android")))]
  let body: String = {
    // `source` distinguishes a collector from a plain device on the box: a
    // Mac collects, so it declares "mac" and earns its ingest fan-out.
    match desktop::client().pair(&id, &code, kind, "mac", label, &identity.node_id).await {
      Ok(b) => b,
      Err(e) => return Ok(serde_json::json!({ "ok": false, "error": format!("{e:#}") })),
    }
  };

  #[cfg(target_os = "android")]
  {
    let _ = (&app, &id, &code, kind, label, &identity);
    return Err(crate::Error::Reach("Bluetooth setup isn't available on Android yet".into()));
  }

  #[cfg(not(target_os = "android"))]
  {
    use crate::ReachExt;
    let status = app.reach().pair_finish_ble(&body, identity).await?;
    Ok(serde_json::json!({ "ok": true, "status": status }))
  }
}

/// Drop the BLE connection. Safe to always call on leaving the setup flow.
#[command]
pub(crate) async fn improv_disconnect<R: Runtime>(app: AppHandle<R>) -> Result<()> {
  #[cfg(target_os = "ios")]
  {
    use tauri::Manager;
    let handle = app.state::<crate::IosPluginHandle<R>>();
    handle
      .0
      .run_mobile_plugin::<serde_json::Value>("improv_disconnect", serde_json::json!({}))
      .map_err(|e| crate::Error::Reach(e.to_string()))?;
    return Ok(());
  }
  #[cfg(not(any(target_os = "ios", target_os = "android")))]
  {
    let _ = &app;
    desktop::client().disconnect().await;
    return Ok(());
  }
  #[cfg(target_os = "android")]
  {
    let _ = app;
    Ok(())
  }
}

/// Join a wifi network whose SSID starts with `ssid_prefix`, natively.
///
/// iOS only — `NEHotspotConfiguration` (NOT Personal Hotspot, NOT the
/// gated `NEHotspotHelper`; see `ios/Sources/ReachPlugin.swift`). Used by the
/// connect screen to put the phone on a box's `Virtues-XXXX` setup network
/// without a trip to Settings, a camera banner, or a captive sheet — the three
/// OS surfaces that each failed on hardware 2026-08-10. The user types only
/// the passphrase off the box's display; the prefix join finds the SSID.
///
/// Raises one system dialog ("Wants to Join…"). `joinOnce` on the Swift side
/// scopes the config to this app session, so nothing lingers in the phone's
/// network list after setup.
#[command]
pub(crate) async fn wifi_join<R: Runtime>(
  app: AppHandle<R>,
  ssid_prefix: String,
  passphrase: String,
) -> Result<serde_json::Value> {
  #[cfg(target_os = "ios")]
  {
    use tauri::Manager;
    let handle = app.state::<crate::IosPluginHandle<R>>();
    return handle
      .0
      .run_mobile_plugin(
        "wifi_join",
        serde_json::json!({ "ssidPrefix": ssid_prefix, "passphrase": passphrase }),
      )
      .map_err(|e| crate::Error::Reach(e.to_string()));
  }
  #[cfg(not(target_os = "ios"))]
  {
    let _ = (app, ssid_prefix, passphrase);
    // Android's equivalent is WifiNetworkSpecifier — not built yet. The
    // connect screen treats this error as "fall back to manual join".
    Err(crate::Error::Reach("programmatic wifi join is iOS-only for now".into()))
  }
}

/// Drop any setup-network config this app added (prefix-matched).
#[command]
pub(crate) async fn wifi_forget<R: Runtime>(
  app: AppHandle<R>,
  ssid_prefix: String,
) -> Result<serde_json::Value> {
  #[cfg(target_os = "ios")]
  {
    use tauri::Manager;
    let handle = app.state::<crate::IosPluginHandle<R>>();
    return handle
      .0
      .run_mobile_plugin("wifi_forget", serde_json::json!({ "ssidPrefix": ssid_prefix }))
      .map_err(|e| crate::Error::Reach(e.to_string()));
  }
  #[cfg(not(target_os = "ios"))]
  {
    let _ = (app, ssid_prefix);
    Ok(serde_json::json!({ "removed": false }))
  }
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

/// Radio-hygiene counters (device screen's Sync section) — the on-device
/// battery A/B harness. `parked` = no warm endpoint right now (radio idle).
#[command]
pub(crate) async fn radio_stats<R: Runtime>(_app: AppHandle<R>) -> Result<serde_json::Value> {
  let s = crate::stats::snapshot();
  let mut v = serde_json::to_value(&s).unwrap_or_else(|_| serde_json::json!({}));
  if let Some(obj) = v.as_object_mut() {
    obj.insert("parked".into(), serde_json::Value::Bool(crate::warm_client().is_none()));
  }
  Ok(v)
}
