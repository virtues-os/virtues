//! In-process reach for the mobile app.
//!
//! The desktop ships a `virtues-client` sidecar that pairs and runs a `:7117`
//! loopback proxy over iroh. iOS can't spawn sidecars, so this plugin runs the
//! same `virtues-reach-client` core in-process: it pairs the device, persists
//! the record to the app container, and serves the box over iroh on a loopback
//! port the webview loads.

use std::net::TcpListener as StdTcpListener;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tauri::{
  plugin::{Builder, TauriPlugin},
  Manager, Runtime,
};
use virtues_reach_client::{
  outbox, BoxStore, PairedBox, SessionState, VirtuesIrohClient,
};

mod commands;
mod error;
mod ffi;
mod models;
mod stats;
mod upload;

pub use error::{Error, Result};
use models::ReachStatus;

/// The loopback port the webview loads (parity with the desktop `:7117` helper).
const LOOPBACK_PORT: u16 = 7117;

// ─── Process-global warm iroh client ─────────────────────────────────────────
//
// `ensure_serving` builds one warm client for the loopback + foreground drain.
// The FFI background drain (Swift-called on a sig-loc wake) can't reach Tauri
// state, so we also stash the client here — one endpoint, reused everywhere.
static WARM_CLIENT: std::sync::Mutex<Option<Arc<VirtuesIrohClient>>> =
  std::sync::Mutex::new(None);

pub(crate) fn set_warm_client(c: Arc<VirtuesIrohClient>) {
  if let Ok(mut g) = WARM_CLIENT.lock() {
    *g = Some(c);
  }
}

pub(crate) fn warm_client() -> Option<Arc<VirtuesIrohClient>> {
  WARM_CLIENT.lock().ok().and_then(|g| g.clone())
}

pub(crate) fn clear_warm_client() {
  if let Ok(mut g) = WARM_CLIENT.lock() {
    *g = None;
  }
}

// ─── Radio hygiene: endpoint parking + payload-aligned drains ────────────────
//
// iroh keeps a live endpoint chatty (QUIC path keepalives every 5s, relay ping
// every 15s, periodic re-STUN) and its transport config clamps the keepalive
// intervals, so the only way to let a phone's radio idle is to not have an
// endpoint at all between uploads. While the app is backgrounded (webview
// suspended, nobody browsing), drains tear the endpoint down afterwards and the
// next drain cold-builds one — a dial per 5-min window instead of pings every
// 5 seconds. Foreground keeps the warm client so the UI stays snappy.

/// True while the iOS app is backgrounded. Fed by Swift lifecycle observers via
/// `virtues_app_background` (ReachMonitor), seeded with the launch app state so
/// a cold background relaunch parks correctly.
static APP_BACKGROUNDED: AtomicBool = AtomicBool::new(false);

/// Rings when a collector enqueues the dominant payload (the ~5-min audio
/// chunk) so the drain fires immediately — the radio wakes when there is
/// something worth waking for, and queued location fixes ride along.
static DRAIN_NUDGE: tokio::sync::Notify = tokio::sync::Notify::const_new();

pub(crate) fn set_app_backgrounded(bg: bool) {
  APP_BACKGROUNDED.store(bg, Ordering::SeqCst);
}

pub(crate) fn app_backgrounded() -> bool {
  APP_BACKGROUNDED.load(Ordering::SeqCst)
}

pub(crate) fn nudge_drain() {
  DRAIN_NUDGE.notify_one();
}

/// Tear down the warm endpoint so nothing pings between background drains.
pub(crate) async fn park_endpoint(reason: &str) {
  let old = WARM_CLIENT.lock().ok().and_then(|mut g| g.take());
  if let Some(c) = old {
    // Bound the graceful close: a slow QUIC teardown on flaky cellular must
    // not outlive the caller's background-task budget. On timeout the client
    // drops abruptly — the box's side just sees a vanished peer, which is
    // exactly what an OS suspension would have produced anyway.
    let _ = tokio::time::timeout(std::time::Duration::from_secs(3), c.shutdown()).await;
    stats::bump(|s| s.parks += 1);
    tracing::info!(reason, "reach endpoint parked");
  }
}

/// The warm client, or a fresh cold-built one (stashed as the new warm client).
pub(crate) async fn ensure_client(rec: &PairedBox) -> Option<Arc<VirtuesIrohClient>> {
  if let Some(c) = warm_client() {
    return Some(c);
  }
  match virtues_reach_client::build_client(rec).await {
    Ok(c) => {
      set_warm_client(c.clone());
      stats::bump(|s| s.dials += 1);
      Some(c)
    }
    Err(e) => {
      tracing::warn!(error = %format!("{e:#}"), "reach client build failed");
      None
    }
  }
}

/// One recovery at a time — NWPathMonitor + foreground can fire nearly together.
static RECOVERING: AtomicBool = AtomicBool::new(false);

/// Recover the box connection after an iOS network change / foreground.
/// Two layers (docs/reach-reliability-plan.md):
///   • **L1 poke** — `Endpoint::network_change()` (rebind sockets / re-STUN /
///     relay reconnect). Heals the common case.
///   • **L2 rebuild** — if a bounded probe still fails, the iOS UDP socket is
///     wedged (iroh#4289) and a poke can't fix it, so rebuild the whole client
///     from the persisted seed (same EndpointId → pairing survives) and swap the
///     warm client. The loopback + drain read the warm client, so new pages /
///     chat / uploads immediately route through the fresh endpoint.
/// Returns: 0 healed by poke, 1 rebuilt, -1 not paired, -2 rebuild failed.
pub(crate) async fn recover_connection() -> i32 {
  if RECOVERING.swap(true, Ordering::SeqCst) {
    return 0; // already recovering
  }
  let rc = recover_inner().await;
  RECOVERING.store(false, Ordering::SeqCst);
  rc
}

async fn recover_inner() -> i32 {
  use std::time::Duration;
  // Parked by design: backgrounded with no warm client means the endpoint was
  // deliberately torn down so the radio can idle — background drains cold-build
  // their own. Rebuilding here (NWPathMonitor fires on every cellular↔Wi-Fi
  // hop in a pocket) would resurrect the keepalive chatter parking eliminated.
  if warm_client().is_none() {
    if app_backgrounded() {
      return 0;
    }
    // Parked, and the app just foregrounded: there is nothing to poke or
    // probe — skip L1's 600ms settle + 4s probe and build immediately, so the
    // resumed webview's first fetches find a client within dial time (the
    // loopback holds connections up to ~3s waiting for exactly this).
  } else {
    // L1: poke iroh to re-check the network.
    if let Some(c) = warm_client() {
      c.network_change().await;
    }
    // Let the rebind / relay reconnect settle, then probe for a live box.
    tokio::time::sleep(Duration::from_millis(600)).await;
    let alive = match warm_client() {
      Some(c) => matches!(
        tokio::time::timeout(Duration::from_secs(4), virtues_reach_client::probe_session(&c)).await,
        Ok(SessionState::Authed) | Ok(SessionState::Rejected)
      ),
      None => false,
    };
    if alive {
      return 0; // the poke healed it
    }
  }
  // L2: the socket is wedged — rebuild the whole endpoint.
  let store = FileStore::new();
  let Ok(Some(rec)) = store.load() else {
    return -1;
  };
  let old = warm_client();
  match virtues_reach_client::build_client(&rec).await {
    Ok(client) => {
      set_warm_client(client);
      stats::bump(|s| s.dials += 1);
      if let Some(old) = old {
        old.shutdown().await; // free the dead socket
      }
      tracing::info!("reach recovery: rebuilt endpoint after wedge");
      1
    }
    Err(e) => {
      tracing::warn!(error = %format!("{e:#}"), "reach recovery rebuild failed");
      -2
    }
  }
}

// ─── Credential storage: a 0600 file in the app container ────────────────────
//
// The 32-byte device seed is the credential. On iOS the app sandbox + data
// protection encrypts it at rest; keeping it a plain file (vs the Keychain)
// keeps the BoxStore in pure Rust. Hardening to the iOS Keychain is a follow-up.

/// Base dir injected from the plugin's `setup()` via Tauri's path API. Set on
/// Android, where `dirs::data_dir()` is unreliable: with no XDG vars and no
/// `HOME` it falls through to `"."`, and the process CWD on Android is `/` —
/// read-only, so both `box.json` and `outbox.sqlite` fail to write. Tauri's
/// `app_data_dir()` resolves the real app-private sandbox instead.
///
/// Left unset on desktop/iOS, which keep the `dirs`-derived path they already
/// use (changing it would strand existing pairings).
static BASE_DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// `<AppSupport>/virtues/` — the app container dir holding creds + the outbox.
fn virtues_dir() -> PathBuf {
  if let Some(base) = BASE_DIR.get() {
    return base.join("virtues");
  }
  let base = dirs::data_dir()
    .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support")))
    .unwrap_or_else(|| PathBuf::from("."));
  base.join("virtues")
}

/// Initialize the shared outbox and clear any stale in-flight claims. Idempotent
/// — safe to call at launch and again after pairing (to refresh the device id).
fn init_outbox(store: &FileStore) {
  let device_id = store
    .load()
    .ok()
    .flatten()
    .and_then(|r| r.device_id)
    .unwrap_or_default();
  if let Err(e) = outbox::init(virtues_dir().join("outbox.sqlite"), &device_id, "ios_ingest") {
    tracing::warn!(error = %format!("{e:#}"), "outbox init failed");
    return;
  }
  let _ = outbox::reset_stale();
}

/// iOS Keychain bridge (Swift `@_cdecl` in location-probe's Keychain.swift). The
/// pairing (seed + box info) lives in the Keychain so it SURVIVES app deletion —
/// a reinstalled app stays paired — and is cleared only by `forget`.
#[cfg(target_os = "ios")]
mod keychain {
  use std::ffi::{CStr, CString};
  use std::os::raw::c_char;

  extern "C" {
    fn virtues_keychain_save(json: *const c_char) -> i32;
    fn virtues_keychain_load() -> *mut c_char;
    fn virtues_keychain_delete() -> i32;
    fn virtues_keychain_free(ptr: *mut c_char);
  }

  pub fn save(json: &str) -> anyhow::Result<()> {
    let c = CString::new(json)?;
    let rc = unsafe { virtues_keychain_save(c.as_ptr()) };
    if rc != 0 {
      anyhow::bail!("keychain save failed (OSStatus {rc})");
    }
    Ok(())
  }

  pub fn load() -> anyhow::Result<Option<String>> {
    let ptr = unsafe { virtues_keychain_load() };
    if ptr.is_null() {
      return Ok(None);
    }
    let s = unsafe { CStr::from_ptr(ptr) }.to_string_lossy().into_owned();
    unsafe { virtues_keychain_free(ptr) };
    Ok(Some(s))
  }

  pub fn delete() -> anyhow::Result<()> {
    let rc = unsafe { virtues_keychain_delete() };
    if rc != 0 {
      anyhow::bail!("keychain delete failed (OSStatus {rc})");
    }
    Ok(())
  }
}

/// Persists the `PairedBox`. On iOS this is Keychain-backed (survives app
/// deletion); elsewhere it's a `0600` JSON file. `path` is still used on iOS for
/// the one-time migration of a pre-Keychain `box.json` and as the desktop sink.
struct FileStore {
  path: PathBuf,
}

impl FileStore {
  fn new() -> Self {
    FileStore {
      path: virtues_dir().join("box.json"),
    }
  }
}

impl BoxStore for FileStore {
  fn load(&self) -> anyhow::Result<Option<PairedBox>> {
    #[cfg(target_os = "ios")]
    {
      if let Some(json) = keychain::load()? {
        return Ok(Some(serde_json::from_str(&json)?));
      }
      // One-time migration: older builds wrote a plaintext box.json. Move it into
      // the Keychain and delete the file so the seed no longer sits on disk.
      match std::fs::read_to_string(&self.path) {
        Ok(json) => {
          keychain::save(&json)?;
          let _ = std::fs::remove_file(&self.path);
          Ok(Some(serde_json::from_str(&json)?))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
      }
    }
    #[cfg(not(target_os = "ios"))]
    {
      match std::fs::read_to_string(&self.path) {
        Ok(json) => Ok(Some(serde_json::from_str(&json)?)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
      }
    }
  }

  fn save(&self, rec: &PairedBox) -> anyhow::Result<()> {
    let json = serde_json::to_string(rec)?;
    #[cfg(target_os = "ios")]
    {
      keychain::save(&json)?;
      return Ok(());
    }
    #[cfg(not(target_os = "ios"))]
    {
      if let Some(dir) = self.path.parent() {
        std::fs::create_dir_all(dir)?;
      }
      let tmp = self.path.with_extension("tmp");
      {
        use std::io::Write as _;
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
          use std::os::unix::fs::OpenOptionsExt;
          opts.mode(0o600);
        }
        let mut f = opts.open(&tmp)?;
        f.write_all(json.as_bytes())?;
        f.sync_all().ok();
      }
      std::fs::rename(&tmp, &self.path)?;
      Ok(())
    }
  }

  fn delete(&self) -> anyhow::Result<()> {
    #[cfg(target_os = "ios")]
    {
      keychain::delete()?;
    }
    // Also remove any on-disk copy (legacy iOS file, or the desktop sink).
    match std::fs::remove_file(&self.path) {
      Ok(()) => Ok(()),
      Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
      Err(e) => Err(e.into()),
    }
  }
}

// ─── Plugin state ────────────────────────────────────────────────────────────

pub struct ReachState {
  store: Arc<FileStore>,
  serving: AtomicBool,
  /// The loopback + drain tasks spawned by `ensure_serving`, so `forget` can abort
  /// them (freeing the loopback port + dropping the old client) and let a re-pair
  /// serve fresh without an app restart.
  tasks: std::sync::Mutex<Vec<tauri::async_runtime::JoinHandle<()>>>,
}

impl ReachState {
  fn new() -> Self {
    ReachState {
      store: Arc::new(FileStore::new()),
      serving: AtomicBool::new(false),
      tasks: std::sync::Mutex::new(Vec::new()),
    }
  }

  pub fn is_paired(&self) -> bool {
    matches!(self.store.load(), Ok(Some(_)))
  }

  pub fn loopback_url(&self) -> String {
    format!("http://127.0.0.1:{LOOPBACK_PORT}")
  }

  /// The warm iroh client, if serving. Reads the process-global source of truth
  /// so a network-change rebuild is reflected here too.
  pub async fn client(&self) -> Option<Arc<VirtuesIrohClient>> {
    warm_client()
  }

  /// Bind the loopback and start splicing to the box over iroh. Idempotent.
  /// Binds the port synchronously *first* so a webview pointed at it queues
  /// rather than gets refused, then builds the warm client and serves.
  pub async fn ensure_serving(&self) -> Result<()> {
    if self.serving.swap(true, Ordering::SeqCst) {
      return Ok(());
    }
    let rec = match self.store.load()? {
      Some(r) => r,
      None => {
        self.serving.store(false, Ordering::SeqCst);
        return Err(Error::Reach("not paired".into()));
      }
    };

    let std_listener = StdTcpListener::bind(("127.0.0.1", LOOPBACK_PORT))
      .map_err(|e| Error::Reach(format!("bind 127.0.0.1:{LOOPBACK_PORT}: {e}")))?;
    std_listener.set_nonblocking(true)?;

    // Refresh the outbox's device id now that we're definitely paired (setup()
    // may have run pre-pair with an empty id).
    init_outbox(&self.store);

    let client = virtues_reach_client::build_client(&rec).await?;
    // WARM_CLIENT is the single source of truth — the loopback, upload path, and
    // FFI background drain all read it, so a network-change rebuild (which swaps
    // it) is picked up everywhere without restarting anything.
    set_warm_client(client);
    stats::bump(|s| s.dials += 1);

    // Serve the loopback (webview → box). Reads the *current* warm client per
    // connection, so a rebuilt client (recovery from an iOS socket wedge) routes
    // new pages/chat/uploads through the fresh endpoint with no listener restart.
    let listener = tokio::net::TcpListener::from_std(std_listener)?;
    let loopback = tauri::async_runtime::spawn(async move {
      if let Err(e) = virtues_reach_client::serve_on_provider(listener, warm_client).await {
        tracing::warn!(error = %format!("{e:#}"), "reach loopback ended");
      }
    });

    // Upload loop: drain the shared outbox to the box. On mobile the tick is
    // slow and payload-aligned — `nudge_drain` (audio chunk enqueue) fires it
    // early — because every tick that touches the radio keeps it from idling.
    // Desktop stays at 20s (mains-powered).
    // (Background sync via BGTaskScheduler + sig-loc wake is wired separately.)
    let store = self.store.clone();
    let drain = tauri::async_runtime::spawn(async move {
      // iOS only, NOT cfg!(mobile): Android has no ReachMonitor feeding
      // APP_BACKGROUNDED and no audio plugin firing nudges, so a 300s tick
      // there would just make uploads 15x slower with zero parking benefit.
      let tick = std::time::Duration::from_secs(if cfg!(target_os = "ios") { 300 } else { 20 });
      loop {
        tokio::select! {
          _ = tokio::time::sleep(tick) => {}
          _ = DRAIN_NUDGE.notified() => {
            // Let the enqueue settle + coalesce near-simultaneous nudges.
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
          }
        }
        let Ok(Some(rec)) = store.load() else { continue };
        // Radio-quiet fast path: nothing due → no dial. Park a leftover warm
        // endpoint if we're backgrounded with nothing to send. An outbox ERROR
        // also takes this path (unwrap_or(true)): treating it as "something is
        // due" would dial + fail + park every tick forever — a perpetual radio
        // burn delivering nothing.
        if outbox::due_streams().map(|s| s.is_empty()).unwrap_or(true) {
          if cfg!(target_os = "ios") && app_backgrounded() {
            park_endpoint("idle").await;
          }
          continue;
        }
        // Warm client if present (foreground), else a cold build (parked). Reading
        // per tick also picks up a network-change rebuild or a re-pair to a
        // different box without restarting this loop.
        let Some(client) = ensure_client(&rec).await else { continue };
        match upload::drain(&client, &rec).await {
          Ok(n) if n > 0 => tracing::info!("uploaded {n} records"),
          Ok(_) => {}
          Err(e) => {
            tracing::warn!(error = %format!("{e:#}"), "outbox drain failed");
            // Drop the (possibly wedged, post-network-switch) connection so the
            // next tick re-dials fresh instead of reusing a dead one forever.
            client.drop_conn().await;
          }
        }
        // Backgrounded: the webview is suspended, nothing else needs the
        // endpoint — park it so the radio sleeps until the next wake.
        if cfg!(target_os = "ios") && app_backgrounded() {
          park_endpoint("post-drain").await;
        }
      }
    });

    if let Ok(mut t) = self.tasks.lock() {
      t.push(loopback);
      t.push(drain);
    }
    Ok(())
  }

  /// Probe `/auth/session` over the warm client, bounded so a dead connection
  /// reports quickly (Unknown) instead of hanging the status call.
  async fn session(&self) -> SessionState {
    let Some(c) = self.client().await else {
      return SessionState::Unknown;
    };
    match tokio::time::timeout(
      std::time::Duration::from_secs(6),
      virtues_reach_client::probe_session(&c),
    )
    .await
    {
      Ok(s) => s,
      Err(_) => SessionState::Unknown, // timed out → treat as unreachable
    }
  }

  pub async fn status(&self) -> ReachStatus {
    let paired = self.is_paired();
    // The probe both diagnoses auth AND (re)connects, so read the live path
    // AFTER it so `path` reflects the current route rather than a cold cache.
    let session_state = if paired {
      self.session().await
    } else {
      SessionState::Unknown
    };
    let session = if !paired {
      "unpaired"
    } else {
      match session_state {
        SessionState::Authed => "authed",
        SessionState::Rejected => "rejected",
        SessionState::Unknown => "unknown",
      }
    };
    // Reachable = the box actually answered (authed or rejected are both "we
    // reached it"); Unknown/timeout = offline/unreachable.
    let reachable = matches!(session_state, SessionState::Authed | SessionState::Rejected);
    let path = if reachable {
      match self.client().await {
        Some(c) => c.path_kind().await.as_str().to_string(),
        None => "offline".into(),
      }
    } else {
      "offline".into()
    };
    ReachStatus {
      paired,
      session: session.into(),
      loopback_url: self.loopback_url(),
      reachable,
      path,
    }
  }

  /// Drain the outbox to the box right now (the "Sync now" button). Returns the
  /// number of records delivered.
  pub async fn drain_now(&self) -> Result<usize> {
    let rec = match self.store.load()? {
      Some(r) => r,
      None => return Ok(0),
    };
    // Explicit user action bypasses retry backoff AND the silent-chunk
    // deferral grid — "Sync now" means everything, now. (Box dedups, so an
    // early resend is harmless.)
    if let Err(e) = outbox::clear_backoff() {
      tracing::warn!(error = %format!("{e:#}"), "clear_backoff failed");
    }
    // Cold-build if parked — "Sync now" must work even before a foreground
    // recovery has rebuilt the warm client.
    let client = match ensure_client(&rec).await {
      Some(c) => c,
      None => return Ok(0),
    };
    upload::drain(&client, &rec)
      .await
      .map_err(|e| Error::Reach(format!("{e:#}")))
  }

  pub async fn pair(&self, server: &str, code: &str) -> Result<ReachStatus> {
    let origin = normalize_server(server);
    // Label the device by the platform it's actually pairing from, so the box's
    // device list shows a Mac as desktop and a phone as mobile (was hardcoded to
    // ios/mobile). `std::env::consts::OS` = "macos"/"windows"/"linux"/"ios"/
    // "android"; `client_kind` picks the box's device class.
    let (client_kind, device_name, client_id) = if cfg!(mobile) {
      ("mobile_app", "Virtues Mobile", "virtues-mobile")
    } else {
      ("desktop_app", "Virtues Desktop", "virtues-desktop")
    };
    let device_info = serde_json::json!({
      "device_name": device_name,
      "os": std::env::consts::OS,
      "client": client_id,
      "version": env!("CARGO_PKG_VERSION"),
    });
    virtues_reach_client::pair::consume(self.store.as_ref(), &origin, code, client_kind, device_info)
      .await?;
    self.ensure_serving().await?;
    Ok(self.status().await)
  }

  pub fn forget(&self) -> Result<()> {
    // Full teardown so a re-pair (even to a different box) serves fresh WITHOUT an
    // app restart: abort the loopback + drain tasks (frees the loopback port),
    // clear the warm client, reset the serving flag, then delete the creds. The
    // pairing lives in the Keychain (survives app deletion), so this is the only
    // way to truly forget a box.
    if let Ok(mut t) = self.tasks.lock() {
      for h in t.drain(..) {
        h.abort();
      }
    }
    clear_warm_client();
    self.serving.store(false, Ordering::SeqCst);
    self.store.delete()?;
    Ok(())
  }
}

/// Normalize a user-typed box address to an `http://host:port` origin
/// (default port 8000), mirroring the desktop connect UI.
pub(crate) fn normalize_server(input: &str) -> String {
  let s = input.trim().trim_end_matches('/');
  if s.starts_with("http://") || s.starts_with("https://") {
    return s.to_string();
  }
  if s.contains(':') {
    format!("http://{s}")
  } else {
    format!("http://{s}:8000")
  }
}

/// Access the reach state from `App`/`AppHandle`/`Window`.
pub trait ReachExt<R: Runtime> {
  fn reach(&self) -> &ReachState;
}

impl<R: Runtime, T: Manager<R>> ReachExt<R> for T {
  fn reach(&self) -> &ReachState {
    self.state::<ReachState>().inner()
  }
}

// The Swift half (`ios/Sources/ReachPlugin.swift`): programmatic wifi join via
// NEHotspotConfiguration, for driving an appliance's setup-AP flow from inside
// the app instead of sending the user to Settings and a captive sheet.
#[cfg(target_os = "ios")]
tauri::ios_plugin_binding!(init_plugin_reach);

/// Handle to the registered iOS plugin, for `run_mobile_plugin` calls.
/// Managed state so commands can reach it; iOS-only by construction.
#[cfg(target_os = "ios")]
pub(crate) struct IosPluginHandle<R: Runtime>(pub tauri::plugin::PluginHandle<R>);

pub fn init<R: Runtime>() -> TauriPlugin<R> {
  Builder::new("reach")
    .invoke_handler(tauri::generate_handler![
      commands::pair,
      commands::reach_status,
      commands::forget,
      commands::discover,
      commands::provision_open,
      commands::provision_networks,
      commands::provision_join,
      commands::wifi_join,
      commands::wifi_forget,
      commands::outbox_stats,
      commands::drain_now,
      commands::radio_stats
    ])
    .setup(|app, _api| {
      #[cfg(target_os = "ios")]
      {
        match _api.register_ios_plugin(init_plugin_reach) {
          Ok(handle) => {
            app.manage(IosPluginHandle(handle));
          }
          Err(e) => tracing::error!(error = %e, "reach: iOS plugin registration failed — wifi join unavailable"),
        }
      }
      // Android: pin the storage base to the app-private sandbox BEFORE anything
      // resolves virtues_dir() — both ReachState::new() (box.json) and
      // init_outbox() (outbox.sqlite) read it below. See BASE_DIR.
      #[cfg(target_os = "android")]
      {
        match app.path().app_data_dir() {
          Ok(dir) => {
            let _ = BASE_DIR.set(dir);
          }
          Err(e) => tracing::error!(error = %e, "app_data_dir unavailable — reach storage will fail"),
        }
      }
      // Keep the Swift-called C ABI (virtues_enqueue) in the linked static lib.
      ffi::keep_symbols();
      let state = ReachState::new();
      // Bring the outbox up before any collector enqueues (incl. a cold
      // background relaunch, where setup() runs first).
      init_outbox(&state.store);
      app.manage(state);
      Ok(())
    })
    .build()
}
