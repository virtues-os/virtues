//! `virtues-door --root <bundles> [--key-file <path>] [--relay <url>] [--core-socket <path>]`
//!
//! Serves published bundles as its own iroh endpoint. The key is the core's
//! (it lives in the box's database, so it survives a restore): `--key-file`
//! names it on a dev machine, and on a box systemd hands it over as the
//! credential `door.key`. The door never makes a key, and never holds the
//! box's.

use std::path::PathBuf;
use std::str::FromStr;

use anyhow::{bail, Context, Result};
use virtues_iroh::{RelayUrl, SecretKey};

const DEFAULT_RELAY: &str = "https://relay.virtues.ch";

fn arg(name: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1).cloned())
}

/// The key from `--key-file`, else from systemd's credential directory.
fn load_key() -> Result<SecretKey> {
    let path = match arg("--key-file") {
        Some(p) => PathBuf::from(p),
        None => PathBuf::from(
            std::env::var("CREDENTIALS_DIRECTORY")
                .context("no --key-file and no systemd credential directory")?,
        )
        .join("door.key"),
    };
    let bytes = std::fs::read(&path).with_context(|| format!("read {}", path.display()))?;
    let Ok(seed) = <[u8; 32]>::try_from(bytes.as_slice()) else {
        bail!("{} is not a 32-byte key", path.display());
    };
    Ok(SecretKey::from_bytes(&seed))
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "virtues_door=info,iroh=warn".into()),
        )
        .init();

    let root = PathBuf::from(arg("--root").context("--root <bundle directory> is required")?);
    let relay = arg("--relay").unwrap_or_else(|| DEFAULT_RELAY.to_string());
    let relay = RelayUrl::from_str(&relay).with_context(|| format!("not a relay URL: {relay}"))?;

    let key = load_key()?;
    let endpoint = virtues_iroh::build_endpoint(key, Some(relay), None).await?;
    let id = endpoint.id();

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
