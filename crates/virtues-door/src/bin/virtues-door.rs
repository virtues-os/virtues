//! `virtues-door --root <bundles> --key-dir <dir> [--relay <url>] [--core-socket <path>]`
//!
//! Serves published bundles as its own iroh endpoint. On first start it makes
//! a key in `--key-dir` and writes its EndpointId beside it (`endpoint-id`)
//! and to stdout (`endpoint-id <id>`), which is what the core reads to build
//! links. The key is the door's, never the box's.

use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::{bail, Context, Result};
use virtues_iroh::{RelayUrl, SecretKey};

const DEFAULT_RELAY: &str = "https://relay.virtues.ch";

fn arg(name: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1).cloned())
}

fn load_or_create_key(dir: &Path) -> Result<SecretKey> {
    let path = dir.join("door.key");
    if let Ok(bytes) = std::fs::read(&path) {
        let Ok(seed) = <[u8; 32]>::try_from(bytes.as_slice()) else {
            bail!("{} is not a 32-byte key", path.display());
        };
        return Ok(SecretKey::from_bytes(&seed));
    }
    std::fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
    let mut seed = [0u8; 32];
    getrandom::fill(&mut seed).map_err(|e| anyhow::anyhow!("os random source: {e}"))?;
    let key = SecretKey::from_bytes(&seed);
    write_private(&path, &seed)?;
    Ok(key)
}

#[cfg(unix)]
fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .with_context(|| format!("create {}", path.display()))?;
    f.write_all(bytes)?;
    Ok(())
}

#[cfg(not(unix))]
fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    std::fs::write(path, bytes).with_context(|| format!("write {}", path.display()))
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "virtues_door=info,iroh=warn".into()),
        )
        .init();

    let root = PathBuf::from(arg("--root").context("--root <bundle directory> is required")?);
    let key_dir = PathBuf::from(arg("--key-dir").context("--key-dir <directory> is required")?);
    let relay = arg("--relay").unwrap_or_else(|| DEFAULT_RELAY.to_string());
    let relay = RelayUrl::from_str(&relay).with_context(|| format!("not a relay URL: {relay}"))?;

    let key = load_or_create_key(&key_dir)?;
    let endpoint = virtues_iroh::build_endpoint(key, Some(relay), None).await?;
    let id = endpoint.id();
    std::fs::write(key_dir.join("endpoint-id"), id.to_string()).context("write endpoint-id")?;
    // The core reads this line: on a box the key directory is the door's own,
    // and the core cannot read it.
    println!("endpoint-id {id}");

    let core = virtues_door::core::Core::at(arg("--core-socket").map(PathBuf::from));
    let router = virtues_door::serve(endpoint, virtues_door::bundle::Store::new(&root), core);
    tracing::info!(%id, root = %root.display(), "door open");

    let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = term.recv() => {}
    }
    router.shutdown().await?;
    Ok(())
}
