//! Where this device keeps its pairing.
//!
//! The 32-byte device seed (`device_secret_hex`) is the device's whole
//! credential: the box treats a connection proven with its key as this paired
//! device, with the owner's authority. The rest of the record (the box's iroh
//! id, relay, addresses) says where the box is and is not secret.
//!
//! - iOS keeps the whole record in the Keychain (Swift, location-probe's
//!   `Keychain.swift`), so it survives the app being deleted.
//! - macOS keeps the seed in the login Keychain and the rest in `box.json`.
//!   A `box.json` written by an older build still holds the seed. Loading it
//!   copies the seed into the Keychain, reads it back, and only then rewrites
//!   the file without it. If the Keychain refuses, the file keeps the seed and
//!   the app goes on working from it.
//! - Windows, Linux and Android keep the whole record in `box.json`.
//!
//! On Unix `box.json` is created with mode 0600; on Windows it gets the
//! permissions of the directory it is in. It exists whenever the device is
//! paired, on every desktop. The shell's launch check (`is_paired` in the
//! app's main.rs) relies on that.
//!
//! The move into the macOS Keychain goes one way. A build that cannot read the
//! Keychain, an older release included, finds a migrated `box.json` with no
//! seed and asks the owner to pair the Mac again. Never remove the Keychain
//! read without first writing the seed back into the file.

use std::path::PathBuf;

use virtues_reach_client::{BoxStore, PairedBox};

use crate::virtues_dir;

/// iOS Keychain bridge (Swift `@_cdecl` in location-probe's Keychain.swift). The
/// pairing (seed + box info) lives in the Keychain so it SURVIVES app deletion —
/// a reinstalled app stays paired — and is cleared only by `forget`.
#[cfg(target_os = "ios")]
mod ios_keychain {
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

/// Somewhere safer than a file for the device seed alone.
#[cfg(not(target_os = "ios"))]
trait SeedVault: Send + Sync {
  /// The stored seed, or `None` when there is none.
  fn get(&self) -> anyhow::Result<Option<String>>;
  /// Store `seed`, replacing any other. Returns only once the seed can be
  /// read back.
  fn set(&self, seed: &str) -> anyhow::Result<()>;
  fn delete(&self) -> anyhow::Result<()>;
}

/// The macOS login Keychain, through the `keyring` crate.
#[cfg(target_os = "macos")]
mod mac_keychain {
  use std::sync::Mutex;

  use anyhow::{anyhow, Result};

  /// One item per app identity.
  const ACCOUNT: &str = "device-seed";

  /// The app's bundle identifier. A dev profile gets its own service, so
  /// `make mac-dev PROFILE=…` never reads or replaces the real pairing's seed,
  /// the same way it keeps its own `virtues-<name>/` directory.
  fn service() -> String {
    match crate::profile() {
      None => "com.virtues.desktop".to_string(),
      Some(p) => format!("com.virtues.desktop.{p}"),
    }
  }

  fn entry() -> Result<keyring::Entry> {
    keyring::Entry::new(&service(), ACCOUNT).map_err(|e| anyhow!("open the Keychain item: {e}"))
  }

  /// What this launch knows about the item. The first answer is kept for the
  /// life of the process: a store is loaded every few seconds, and asking the
  /// Keychain again after the owner pressed Deny would put the same macOS
  /// prompt in front of them over and over.
  enum Known {
    Unread,
    Holds(Option<String>),
    Failed(String),
  }

  pub(crate) struct Keychain {
    known: Mutex<Known>,
  }

  pub(crate) static KEYCHAIN: Keychain = Keychain { known: Mutex::new(Known::Unread) };

  impl super::SeedVault for Keychain {
    fn get(&self) -> Result<Option<String>> {
      // Held across the read, so two callers at launch share one prompt.
      let mut known = self.known.lock().unwrap_or_else(|e| e.into_inner());
      if let Known::Unread = *known {
        *known = match entry().and_then(|item| match item.get_password() {
          Ok(seed) => Ok(Some(seed)),
          Err(keyring::Error::NoEntry) => Ok(None),
          Err(e) => Err(anyhow!("{e}")),
        }) {
          Ok(seed) => Known::Holds(seed),
          Err(e) => Known::Failed(format!(
            "Virtues could not read this Mac's device key from the Keychain ({e:#}). \
             Quit and reopen Virtues to try again."
          )),
        };
      }
      match &*known {
        Known::Holds(seed) => Ok(seed.clone()),
        Known::Failed(message) => Err(anyhow!("{message}")),
        Known::Unread => unreachable!("read above"),
      }
    }

    fn set(&self, seed: &str) -> Result<()> {
      let mut known = self.known.lock().unwrap_or_else(|e| e.into_inner());
      match &*known {
        Known::Holds(Some(held)) if held == seed => return Ok(()),
        // One try per launch, read or write. The caller keeps the seed in
        // box.json, and the next launch tries the Keychain again.
        Known::Failed(message) => return Err(anyhow!("{message}")),
        _ => {}
      }
      let written = entry().and_then(|item| item.set_password(seed).map_err(|e| anyhow!("{e}"))).and_then(|()| {
        // Read back through a new handle. A `keyring` built without its macOS
        // store (the `apple-native` feature) keeps the value in memory and
        // reports success, and a seed lost that way unpairs the Mac.
        match entry()?.get_password() {
          Ok(back) if back == seed => Ok(()),
          Ok(_) => Err(anyhow!("the Keychain returned a different value than was written")),
          Err(e) => Err(anyhow!("the Keychain did not return the value it was given: {e}")),
        }
      });
      match written {
        Ok(()) => {
          *known = Known::Holds(Some(seed.to_string()));
          Ok(())
        }
        Err(e) => {
          let message = format!("Virtues could not store this Mac's device key in the Keychain ({e:#}).");
          *known = Known::Failed(message.clone());
          Err(anyhow!("{message}"))
        }
      }
    }

    fn delete(&self) -> Result<()> {
      let mut known = self.known.lock().unwrap_or_else(|e| e.into_inner());
      match entry()?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => {
          *known = Known::Holds(None);
          Ok(())
        }
        Err(e) => Err(anyhow!("could not delete this Mac's device key from the Keychain: {e}")),
      }
    }
  }
}

/// Persists the `PairedBox` (see the module doc for where each part lives).
pub(crate) struct FileStore {
  path: PathBuf,
  #[cfg(not(target_os = "ios"))]
  vault: Option<&'static dyn SeedVault>,
}

impl FileStore {
  pub(crate) fn new() -> Self {
    FileStore {
      path: virtues_dir().join("box.json"),
      #[cfg(target_os = "macos")]
      vault: Some(&mac_keychain::KEYCHAIN),
      // TODO(2026-10-27): move the seed into Windows Credential Manager and
      // the Linux Secret Service the way macOS does. Until then it is in
      // box.json.
      #[cfg(not(any(target_os = "ios", target_os = "macos")))]
      vault: None,
    }
  }
}

/// Held while the record is read or written. Several tasks load it at once at
/// launch, and loading an old `box.json` rewrites it: without this, two
/// rewrites share one temporary file, and a pairing saved meanwhile can be
/// overwritten by the record it replaced.
#[cfg(not(target_os = "ios"))]
static RECORD: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(not(target_os = "ios"))]
fn hold_record() -> std::sync::MutexGuard<'static, ()> {
  RECORD.lock().unwrap_or_else(|e| e.into_inner())
}

#[cfg(not(target_os = "ios"))]
impl FileStore {
  fn read_file(&self) -> anyhow::Result<Option<PairedBox>> {
    match std::fs::read_to_string(&self.path) {
      Ok(json) => Ok(Some(serde_json::from_str(&json)?)),
      Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
      Err(e) => Err(e.into()),
    }
  }

  /// Written whole to a temporary file and renamed over, so a crash mid-write
  /// leaves the old record rather than half of one.
  fn write_file(&self, rec: &PairedBox) -> anyhow::Result<()> {
    let json = serde_json::to_string(rec)?;
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

  /// The record as written to the file once the vault holds its seed.
  fn without_seed(rec: &PairedBox) -> PairedBox {
    PairedBox { device_secret_hex: None, ..rec.clone() }
  }
}

impl BoxStore for FileStore {
  fn load(&self) -> anyhow::Result<Option<PairedBox>> {
    #[cfg(target_os = "ios")]
    {
      if let Some(json) = ios_keychain::load()? {
        return Ok(Some(serde_json::from_str(&json)?));
      }
      // One-time migration: older builds wrote a plaintext box.json. Move it into
      // the Keychain and delete the file so the seed no longer sits on disk.
      match std::fs::read_to_string(&self.path) {
        Ok(json) => {
          ios_keychain::save(&json)?;
          let _ = std::fs::remove_file(&self.path);
          Ok(Some(serde_json::from_str(&json)?))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
      }
    }
    #[cfg(not(target_os = "ios"))]
    {
      let _held = hold_record();
      // No file means not paired, whatever a vault still holds.
      let Some(mut rec) = self.read_file()? else { return Ok(None) };
      let Some(vault) = self.vault else { return Ok(Some(rec)) };
      match rec.device_secret_hex.as_deref() {
        // Written by a build that kept the seed in the file, or by one whose
        // Keychain write failed. The file is rewritten only once the vault
        // gives the seed back.
        Some(seed) => match vault.set(seed) {
          Ok(()) => match self.write_file(&Self::without_seed(&rec)) {
            Ok(()) => tracing::info!("reach: moved the device key from box.json into the Keychain"),
            // The seed is in both places, and the next load tries again.
            Err(e) => tracing::warn!(error = %format!("{e:#}"), "reach: device key is in the Keychain, but box.json could not be rewritten"),
          },
          // Once per launch: the record is loaded every few seconds, and the
          // vault answers a refused write the same way until the next launch.
          Err(e) => {
            static SAID: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
            if !SAID.swap(true, std::sync::atomic::Ordering::Relaxed) {
              tracing::warn!(error = %format!("{e:#}"), "reach: box.json keeps the device key");
            }
          }
        },
        None => rec.device_secret_hex = vault.get()?,
      }
      Ok(Some(rec))
    }
  }

  fn save(&self, rec: &PairedBox) -> anyhow::Result<()> {
    #[cfg(target_os = "ios")]
    {
      ios_keychain::save(&serde_json::to_string(rec)?)?;
      return Ok(());
    }
    #[cfg(not(target_os = "ios"))]
    {
      let _held = hold_record();
      let (Some(vault), Some(seed)) = (self.vault, rec.device_secret_hex.as_deref()) else {
        return self.write_file(rec);
      };
      match vault.set(seed) {
        Ok(()) => self.write_file(&Self::without_seed(rec)),
        // A pairing that only half lands is worse than a seed in a 0600 file:
        // keep it in the file, and the next launch moves it.
        Err(e) => {
          tracing::warn!(error = %format!("{e:#}"), "reach: box.json keeps the device key");
          self.write_file(rec)
        }
      }
    }
  }

  fn delete(&self) -> anyhow::Result<()> {
    #[cfg(target_os = "ios")]
    {
      ios_keychain::delete()?;
    }
    #[cfg(not(target_os = "ios"))]
    let _held = hold_record();
    // Also remove the file: the desktop record, or a legacy iOS one.
    let file = match std::fs::remove_file(&self.path) {
      Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
      _ => Ok(()),
    };
    // The vault is cleared even when the file could not be, and the other way
    // round, so a forget never leaves the seed behind in either.
    #[cfg(not(target_os = "ios"))]
    if let Some(vault) = self.vault {
      return file.and(vault.delete());
    }
    file
  }
}

#[cfg(all(test, not(target_os = "ios")))]
mod tests {
  use std::sync::atomic::{AtomicUsize, Ordering};
  use std::sync::Mutex;

  use super::*;

  /// A vault in memory. `refuse` makes every write fail; `broken` makes every
  /// read fail.
  #[derive(Default)]
  struct Mem {
    seed: Mutex<Option<String>>,
    refuse: bool,
    broken: bool,
  }

  impl SeedVault for Mem {
    fn get(&self) -> anyhow::Result<Option<String>> {
      if self.broken {
        anyhow::bail!("the vault is locked");
      }
      Ok(self.seed.lock().unwrap().clone())
    }
    fn set(&self, seed: &str) -> anyhow::Result<()> {
      if self.refuse {
        anyhow::bail!("the vault refused the write");
      }
      *self.seed.lock().unwrap() = Some(seed.to_string());
      Ok(())
    }
    fn delete(&self) -> anyhow::Result<()> {
      *self.seed.lock().unwrap() = None;
      Ok(())
    }
  }

  const SEED: &str = "0101010101010101010101010101010101010101010101010101010101010101";

  fn record(seed: Option<&str>) -> PairedBox {
    PairedBox {
      box_url: "http://192.168.1.20:8000".into(),
      device_id: Some("dev_example".into()),
      box_node_id: Some("ab".repeat(32)),
      relay_url: Some("https://relay.example.com/".into()),
      box_direct_addrs: vec!["192.168.1.20:7300".into()],
      device_secret_hex: seed.map(str::to_string),
      applet_ids: Default::default(),
    }
  }

  fn store(vault: Option<Mem>) -> FileStore {
    static N: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
      "virtues-reach-store-{}-{}",
      std::process::id(),
      N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    FileStore {
      path: dir.join("box.json"),
      vault: vault.map(|v| &*Box::leak(Box::new(v)) as &'static dyn SeedVault),
    }
  }

  fn file_text(store: &FileStore) -> String {
    std::fs::read_to_string(&store.path).unwrap()
  }

  fn vault_seed(store: &FileStore) -> Option<String> {
    store.vault.unwrap().get().unwrap()
  }

  #[test]
  fn an_old_box_json_moves_its_seed_into_the_vault() {
    let store = store(Some(Mem::default()));
    store.write_file(&record(Some(SEED))).unwrap();

    let loaded = store.load().unwrap().unwrap();
    assert_eq!(loaded.device_secret_hex.as_deref(), Some(SEED));
    assert_eq!(vault_seed(&store).as_deref(), Some(SEED));
    assert!(!file_text(&store).contains(SEED), "box.json still holds the seed");

    // The rewritten file still carries everything else, and loads whole.
    let again = store.load().unwrap().unwrap();
    assert_eq!(again.device_secret_hex.as_deref(), Some(SEED));
    assert_eq!(again.box_node_id, loaded.box_node_id);
    assert_eq!(again.box_direct_addrs, loaded.box_direct_addrs);
  }

  /// Loads at launch run at once, and each takes the record lock and then the
  /// vault's. This checks they all finish and leave one clean file. The
  /// shared-temporary-file race the lock closes is too narrow to hit here.
  #[test]
  fn loads_at_the_same_time_migrate_an_old_box_json_cleanly() {
    let store: &'static FileStore = Box::leak(Box::new(store(Some(Mem::default()))));
    for _ in 0..20 {
      store.write_file(&record(Some(SEED))).unwrap();
      let start = std::sync::Arc::new(std::sync::Barrier::new(16));
      let loads: Vec<_> = (0..16)
        .map(|_| {
          let start = start.clone();
          std::thread::spawn(move || {
            start.wait();
            store.load()
          })
        })
        .collect();
      for load in loads {
        let rec = load.join().unwrap().unwrap().unwrap();
        assert_eq!(rec.device_secret_hex.as_deref(), Some(SEED));
      }
      assert!(!file_text(store).contains(SEED));
      assert!(!store.path.with_extension("tmp").exists(), "a rewrite left its temporary file");
    }
  }

  #[test]
  fn a_vault_that_refuses_leaves_the_seed_in_the_file() {
    let store = store(Some(Mem { refuse: true, ..Default::default() }));
    store.write_file(&record(Some(SEED))).unwrap();

    let loaded = store.load().unwrap().unwrap();
    assert_eq!(loaded.device_secret_hex.as_deref(), Some(SEED), "the Mac is still paired");
    assert!(file_text(&store).contains(SEED), "the only copy of the seed was dropped");
  }

  #[test]
  fn pairing_puts_the_seed_in_the_vault_and_the_rest_in_the_file() {
    let store = store(Some(Mem::default()));
    store.save(&record(Some(SEED))).unwrap();

    assert!(!file_text(&store).contains(SEED));
    assert!(store.path.exists(), "the launch check looks for box.json");
    assert_eq!(store.load().unwrap().unwrap().device_secret_hex.as_deref(), Some(SEED));
  }

  #[test]
  fn pairing_with_a_vault_that_refuses_keeps_the_seed_in_the_file() {
    let store = store(Some(Mem { refuse: true, ..Default::default() }));
    store.save(&record(Some(SEED))).unwrap();

    assert!(file_text(&store).contains(SEED));
    assert_eq!(store.load().unwrap().unwrap().device_secret_hex.as_deref(), Some(SEED));
  }

  #[test]
  fn a_vault_that_cannot_be_read_is_an_error_not_a_missing_key() {
    let store = store(Some(Mem { broken: true, ..Default::default() }));
    store.write_file(&record(None)).unwrap();
    assert!(store.load().is_err());
  }

  #[test]
  fn forgetting_clears_the_file_and_the_vault() {
    let store = store(Some(Mem::default()));
    store.save(&record(Some(SEED))).unwrap();
    store.delete().unwrap();

    assert!(!store.path.exists());
    assert_eq!(vault_seed(&store), None);
    assert!(store.load().unwrap().is_none());
    store.delete().unwrap();
  }

  #[test]
  fn no_file_means_not_paired_even_with_a_seed_left_in_the_vault() {
    let store = store(Some(Mem::default()));
    store.vault.unwrap().set(SEED).unwrap();
    assert!(store.load().unwrap().is_none());
  }

  #[test]
  fn a_pairing_without_a_seed_loads_without_one() {
    let store = store(Some(Mem::default()));
    store.write_file(&record(None)).unwrap();
    assert_eq!(store.load().unwrap().unwrap().device_secret_hex, None);
  }

  #[test]
  fn without_a_vault_the_file_keeps_the_seed() {
    let store = store(None);
    store.save(&record(Some(SEED))).unwrap();
    assert!(file_text(&store).contains(SEED));
    assert_eq!(store.load().unwrap().unwrap().device_secret_hex.as_deref(), Some(SEED));
  }

  #[cfg(unix)]
  #[test]
  fn box_json_is_readable_by_its_owner_only() {
    use std::os::unix::fs::PermissionsExt;
    let store = store(Some(Mem::default()));
    store.save(&record(Some(SEED))).unwrap();
    let mode = std::fs::metadata(&store.path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
  }
}
