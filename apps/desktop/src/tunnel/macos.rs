//! macOS WireGuard tunnel.
//!
//! Mirrors `linux.rs` but uses macOS `utun` devices and BSD network commands
//! (`ifconfig`/`route`) instead of the Linux `ip` tool.
//!
//! ## Privileges
//!
//! Creating a utun device requires root. The LaunchDaemon runs as root;
//! for dev, `sudo virtues-client daemon`.
//!
//! ## Lifetime
//!
//! Same as the Linux side: [`start`] spawns a background task that owns the
//! typed `gotatun::device::Device`. The returned [`TunnelHandle`] carries a
//! shutdown-signal channel; calling [`TunnelHandle::stop`] drains the WG
//! state machine cleanly before returning.

use std::net::IpAddr;
use std::process::Stdio;

use anyhow::{anyhow, Context, Result};
use base64::Engine as _;
use gotatun::device::{DeviceBuilder, Peer};
use gotatun::x25519::{PublicKey, StaticSecret};
use ipnetwork::IpNetwork;
use tokio::process::Command;
use tokio::sync::oneshot;
use virtues_protocol::PairingBundle;

use crate::keychain;

const TUN_NAME: &str = "utun9";
const TUN_MTU: u32 = 1280;
const KEEPALIVE_SECS: u16 = 25;

/// Holds the shutdown signal for the background GotaTun task.
pub struct TunnelHandle {
    shutdown_tx: oneshot::Sender<()>,
    stopped_rx: oneshot::Receiver<()>,
}

impl TunnelHandle {
    pub async fn stop(self) {
        let _ = self.shutdown_tx.send(());
        let _ = self.stopped_rx.await;
        eprintln!("✓ tunnel stopped: {TUN_NAME}");
        tracing::info!(tun = TUN_NAME, "WG tunnel stopped");
    }
}

pub async fn start(bundle: &PairingBundle) -> Result<TunnelHandle> {
    sweep_stale_interface().await;

    let priv_b64 = keychain::load_wg_private()
        .context("load WG private key from keychain")?
        .ok_or_else(|| {
            anyhow!(
                "no WG private key in keychain — re-run `virtues-client pair`"
            )
        })?;
    let priv_bytes = b64_decode_32(&priv_b64).context("decode WG private key")?;
    let private_key = StaticSecret::from(priv_bytes);

    let server_pub_bytes = b64_decode_32(&bundle.wg.server_public_key)
        .context("decode box WG public key")?;
    let server_pub = PublicKey::from(server_pub_bytes);

    let server_endpoint: std::net::SocketAddr = bundle
        .wg
        .server_endpoint
        .parse()
        .with_context(|| format!("parse box endpoint `{}`", bundle.wg.server_endpoint))?;

    let preshared = b64_decode_32(&bundle.wg.preshared_key)
        .context("decode preshared key")?;

    let client_addr: IpAddr = bundle
        .wg
        .client_address
        .parse()
        .with_context(|| format!("parse client_address `{}`", bundle.wg.client_address))?;
    let server_addr: IpAddr = bundle
        .wg
        .server_address
        .parse()
        .with_context(|| format!("parse server_address `{}`", bundle.wg.server_address))?;

    let server_allowed = IpNetwork::new(server_addr, addr_prefix_full(server_addr))
        .context("compose server allowed-ip network")?;

    let builder = DeviceBuilder::new()
        .with_default_udp()
        .with_listen_port(0)
        .create_tun(TUN_NAME)
        .with_context(|| {
            format!(
                "create TUN device `{TUN_NAME}` — is this process running as root? \
                 (utun creation requires root on macOS; use the LaunchDaemon or \
                 `sudo virtues-client daemon --bundle-path ~/.virtues/bundle.json`)"
            )
        })?
        .with_private_key(private_key)
        .with_peer({
            let mut peer = Peer::new(server_pub)
                .with_endpoint(server_endpoint)
                .with_preshared_key(preshared)
                .with_allowed_ip(server_allowed);
            peer.keepalive = Some(KEEPALIVE_SECS);
            peer
        });

    configure_tun_link(client_addr, server_addr).await?;

    let device = builder.build().await.context("gotatun device build")?;

    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let (stopped_tx, stopped_rx) = oneshot::channel::<()>();

    tokio::spawn(async move {
        let _ = shutdown_rx.await;
        device.stop().await;
        let _ = stopped_tx.send(());
    });

    eprintln!("✓ tunnel up: {TUN_NAME} ({client_addr} ↔ box {server_addr})");
    tracing::info!(
        tun = TUN_NAME,
        client_addr = %client_addr,
        server_addr = %server_addr,
        endpoint = %server_endpoint,
        "WG tunnel established"
    );

    Ok(TunnelHandle {
        shutdown_tx,
        stopped_rx,
    })
}

/// Bring down any leftover utun9 from a prior crash. utun devices are
/// transient — `ifconfig utun9 down` is enough to clean them up. Errors
/// are logged as warnings only; the subsequent `create_tun` surfaces real
/// failures more clearly.
async fn sweep_stale_interface() {
    let result = Command::new("ifconfig")
        .args([TUN_NAME, "down"])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .await;
    match result {
        Ok(out) if out.status.success() => {
            tracing::info!(tun = TUN_NAME, "swept orphaned tunnel interface");
        }
        Ok(_) => {
            tracing::debug!(tun = TUN_NAME, "no stale interface to sweep");
        }
        Err(e) => {
            tracing::warn!("ifconfig down failed to spawn: {e}");
        }
    }
}

/// Configure the utun device's L3 addressing. Three BSD network commands:
///
///     ifconfig utun9 mtu 1280 up
///     ifconfig utun9 inet6 <client> <server> prefixlen 128
///     route add -inet6 -host <server> -interface utun9
async fn configure_tun_link(client: IpAddr, server: IpAddr) -> Result<()> {
    // Bring up with MTU.
    run_or_ignore(&["ifconfig", TUN_NAME, "mtu", &TUN_MTU.to_string(), "up"])
        .await
        .context("ifconfig mtu up")?;

    // Assign the point-to-point IPv6 addresses.
    // BSD ifconfig inet6: <interface> inet6 <local> <remote> prefixlen <n>
    run_or_ignore(&[
        "ifconfig", TUN_NAME, "inet6",
        &client.to_string(),
        &server.to_string(),
        "prefixlen", "128",
    ])
    .await
    .context("ifconfig inet6")?;

    // Add a host route so the server address is reachable through the tunnel.
    run_or_ignore(&[
        "route", "add", "-inet6", "-host",
        &server.to_string(),
        "-interface", TUN_NAME,
    ])
    .await
    .context("route add")?;

    Ok(())
}

/// Run a network command; tolerate already-configured errors. Every other
/// non-zero exit surfaces the full stderr.
async fn run_or_ignore(argv: &[&str]) -> Result<()> {
    let output = Command::new(argv[0])
        .args(&argv[1..])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .await
        .with_context(|| format!("spawn {argv:?}"))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let lower = stderr.to_ascii_lowercase();
    // ifconfig/route "already configured" variants on BSD/macOS
    if lower.contains("file exists")
        || lower.contains("already")
        || lower.contains("can't assign requested address")
        || lower.contains("eexist")
    {
        tracing::debug!(argv = ?argv, "already configured (tolerated)");
        return Ok(());
    }
    Err(anyhow!(
        "{argv:?} failed ({}): {}",
        output.status,
        stderr.trim()
    ))
}

fn addr_prefix_full(addr: IpAddr) -> u8 {
    match addr {
        IpAddr::V4(_) => 32,
        IpAddr::V6(_) => 128,
    }
}

fn b64_decode_32(s: &str) -> Result<[u8; 32]> {
    let v = base64::engine::general_purpose::STANDARD
        .decode(s)
        .context("base64 decode")?;
    v.try_into()
        .map_err(|v: Vec<u8>| anyhow!("expected 32 bytes, got {}", v.len()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefix_full_ipv4_is_32() {
        let v4: IpAddr = "10.0.0.1".parse().unwrap();
        assert_eq!(addr_prefix_full(v4), 32);
    }

    #[test]
    fn prefix_full_ipv6_is_128() {
        let v6: IpAddr = "fd00::1".parse().unwrap();
        assert_eq!(addr_prefix_full(v6), 128);
    }

    #[test]
    fn b64_decode_32_rejects_short_input() {
        assert!(b64_decode_32("YQ==").is_err());
    }

    #[test]
    fn b64_decode_32_accepts_exact_length() {
        let s = base64::engine::general_purpose::STANDARD.encode([0u8; 32]);
        assert!(b64_decode_32(&s).is_ok());
    }
}
