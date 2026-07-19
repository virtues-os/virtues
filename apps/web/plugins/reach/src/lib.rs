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
  // L2: the socket is wedged — rebuild the whole endpoint.
  let store = FileStore::new();
  let Ok(Some(rec)) = store.load() else {
    return -1;
  };
  let old = warm_client();
  match virtues_reach_client::build_client(&rec).await {
    Ok(client) => {
      set_warm_client(client);
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

/// `<AppSupport>/virtues/` — the app container dir holding creds + the outbox.
fn virtues_dir() -> PathBuf {
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

    // Serve the loopback (webview → box). Reads the *current* warm client per
    // connection, so a rebuilt client (recovery from an iOS socket wedge) routes
    // new pages/chat/uploads through the fresh endpoint with no listener restart.
    let listener = tokio::net::TcpListener::from_std(std_listener)?;
    let loopback = tauri::async_runtime::spawn(async move {
      if let Err(e) = virtues_reach_client::serve_on_provider(listener, warm_client).await {
        tracing::warn!(error = %format!("{e:#}"), "reach loopback ended");
      }
    });

    // Foreground upload loop: drain the shared outbox to the box.
    // (Background sync via BGTaskScheduler + sig-loc wake is wired separately.)
    let store = self.store.clone();
    let drain = tauri::async_runtime::spawn(async move {
      loop {
        tokio::time::sleep(std::time::Duration::from_secs(20)).await;
        let Ok(Some(rec)) = store.load() else { continue };
        // Read the CURRENT warm client each tick, so a network-change rebuild OR a
        // re-pair to a different box is picked up without restarting this loop.
        let Some(client) = warm_client() else { continue };
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
    let client = match self.client().await {
      Some(c) => c,
      None => return Ok(0),
    };
    upload::drain(&client, &rec)
      .await
      .map_err(|e| Error::Reach(format!("{e:#}")))
  }

  pub async fn pair(&self, server: &str, code: &str) -> Result<ReachStatus> {
    let origin = normalize_server(server);
    let device_info = serde_json::json!({
      "device_name": "Virtues Mobile",
      "os": "ios",
      "client": "virtues-mobile",
      "version": env!("CARGO_PKG_VERSION"),
    });
    virtues_reach_client::pair::consume(self.store.as_ref(), &origin, code, "mobile_app", device_info)
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
fn normalize_server(input: &str) -> String {
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

pub fn init<R: Runtime>() -> TauriPlugin<R> {
  Builder::new("reach")
    .invoke_handler(tauri::generate_handler![
      commands::pair,
      commands::reach_status,
      commands::forget,
      commands::discover,
      commands::outbox_stats,
      commands::drain_now
    ])
    .setup(|app, _api| {
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
