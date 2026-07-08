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
use tokio::sync::Mutex;
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
    match std::fs::read_to_string(&self.path) {
      Ok(json) => Ok(Some(serde_json::from_str(&json)?)),
      Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
      Err(e) => Err(e.into()),
    }
  }

  fn save(&self, rec: &PairedBox) -> anyhow::Result<()> {
    if let Some(dir) = self.path.parent() {
      std::fs::create_dir_all(dir)?;
    }
    let json = serde_json::to_string(rec)?;
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

  fn delete(&self) -> anyhow::Result<()> {
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
  /// The warm iroh client — shared between the loopback and (later) the upload
  /// coordinator so there's exactly one endpoint/identity.
  client: Mutex<Option<Arc<VirtuesIrohClient>>>,
  serving: AtomicBool,
}

impl ReachState {
  fn new() -> Self {
    ReachState {
      store: Arc::new(FileStore::new()),
      client: Mutex::new(None),
      serving: AtomicBool::new(false),
    }
  }

  pub fn is_paired(&self) -> bool {
    matches!(self.store.load(), Ok(Some(_)))
  }

  pub fn loopback_url(&self) -> String {
    format!("http://127.0.0.1:{LOOPBACK_PORT}")
  }

  /// The warm iroh client, if serving. Used by the upload coordinator.
  pub async fn client(&self) -> Option<Arc<VirtuesIrohClient>> {
    self.client.lock().await.clone()
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
    *self.client.lock().await = Some(client.clone());

    // Serve the loopback (webview → box).
    let listener = tokio::net::TcpListener::from_std(std_listener)?;
    let serve_client = client.clone();
    tauri::async_runtime::spawn(async move {
      if let Err(e) = virtues_reach_client::serve_on(listener, serve_client).await {
        tracing::warn!(error = %format!("{e:#}"), "reach loopback ended");
      }
    });

    // Foreground upload loop: drain the shared outbox to the box.
    // (Background sync via BGTaskScheduler + sig-loc wake is wired separately.)
    let store = self.store.clone();
    tauri::async_runtime::spawn(async move {
      loop {
        tokio::time::sleep(std::time::Duration::from_secs(20)).await;
        let Ok(Some(rec)) = store.load() else { continue };
        match upload::drain(&client, &rec).await {
          Ok(n) if n > 0 => tracing::info!("uploaded {n} records"),
          Ok(_) => {}
          Err(e) => tracing::warn!(error = %format!("{e:#}"), "outbox drain failed"),
        }
      }
    });
    Ok(())
  }

  /// Probe `/auth/session` over the warm client.
  async fn session(&self) -> SessionState {
    match self.client().await {
      Some(c) => virtues_reach_client::probe_session(&c).await,
      None => SessionState::Unknown,
    }
  }

  pub async fn status(&self) -> ReachStatus {
    let paired = self.is_paired();
    let session = if !paired {
      "unpaired"
    } else {
      match self.session().await {
        SessionState::Authed => "authed",
        SessionState::Rejected => "rejected",
        SessionState::Unknown => "unknown",
      }
    };
    ReachStatus {
      paired,
      session: session.into(),
      loopback_url: self.loopback_url(),
    }
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
    // Clears creds. A loopback task already running lingers harmlessly until the
    // next launch (it holds the old client; nothing new dials it). Stopping it
    // mid-flight is a follow-up.
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
      commands::outbox_stats
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
