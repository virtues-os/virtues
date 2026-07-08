//! The `:7117` localhost helper — CLI wrapper over `virtues_reach_client::proxy`.
//!
//! `virtues-client up` loads the paired box, builds a warm iroh client, and
//! splices browser connections on `127.0.0.1:7117` to the box over iroh. All the
//! reach logic lives in the shared crate; this just wires it to the keychain
//! store and the fixed desktop bind address.

use anyhow::{anyhow, Context, Result};

use crate::keychain;

const BIND: &str = "127.0.0.1:7117";

pub async fn run() -> Result<()> {
    let rec = keychain::load_box()
        .context("load paired box")?
        .ok_or_else(|| anyhow!("not paired — run `virtues-client pair <url>` first"))?;

    let client = virtues_reach_client::build_client(&rec).await?;
    let bind = BIND.parse().expect("valid bind addr");
    eprintln!("virtues helper: serving your box at http://{BIND}  (Ctrl+C to stop)");
    virtues_reach_client::serve_loopback(client, bind).await
}
