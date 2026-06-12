//! Linux WG tunnel implementation.
//!
//! Stack:
//! - **GotaTun** drives the WG state machine (handshake, key rotation,
//!   keepalive, rate limiting).
//! - **`tun` crate** (bundled inside GotaTun's `tun` feature) owns
//!   `/dev/net/tun`.
//! - **`ip` shell command** configures the TUN's L3 address + link state.
//!   We shell out instead of using netlink to avoid pulling in `rtnetlink`'s
//!   transitive dep tree for what amounts to three commands per startup.
//!
//! ## Privileges
//!
//! Creating a TUN device on Linux requires `CAP_NET_ADMIN`. Install modes:
//!   - **dev**: run `virtues-client up` as root via `sudo`
//!   - **production**:
//!     ```
//!     sudo setcap cap_net_admin+ep /usr/local/bin/virtues-client
//!     ```
//!     after installation. The systemd user unit relies on the binary having
//!     this capability set — `AmbientCapabilities=` is silently ignored by
//!     systemd in `--user` mode, so we can't grant it from the unit.
//!
//! ## Lifetime
//!
//! [`start`] builds the [`gotatun::device::Device`] inside a background task
//! that owns the typed handle. The returned [`TunnelHandle`] is a
//! shutdown-signal endpoint; calling [`TunnelHandle::stop`] sends the signal,
//! the background task calls `Device::stop().await` cleanly, then confirms
//! back. Dropping the handle without calling `stop` aborts the task and
//! leaves cleanup to the runtime — works, but logs less clearly.

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

/// Name of the TUN device GotaTun creates. Stable per-box so `ip link show
/// virtues0` is a meaningful diagnostic.
const TUN_NAME: &str = "virtues0";

/// MTU for the inside-tunnel link. 1280 is the IPv6 minimum and stays under
/// every cellular carrier's effective MTU; we'd rather accept a slightly
/// smaller MSS than hit silent fragmentation drops.
const TUN_MTU: u32 = 1280;

/// Persistent keepalive interval (seconds). 25 s keeps NAT mappings warm on
/// every consumer router we've measured.
const KEEPALIVE_SECS: u16 = 25;

/// Holds the shutdown signal endpoints for the background task that owns
/// the typed `gotatun::device::Device`. Call [`stop`] to shut down
/// cleanly. Dropping the handle aborts the task (the runtime owns its
/// lifetime via the `JoinHandle` we don't keep) — `Device::stop` then
/// doesn't run, which is acceptable as a fail-safe but logged less
/// clearly than the cooperative path.
pub struct TunnelHandle {
    shutdown_tx: oneshot::Sender<()>,
    stopped_rx: oneshot::Receiver<()>,
}

impl TunnelHandle {
    /// Cooperative shutdown. Signals the owning task to call
    /// `Device::stop().await`, then waits for confirmation. Returns once
    /// the WG state machine has fully drained and the TUN file descriptor
    /// is closed.
    pub async fn stop(self) {
        let _ = self.shutdown_tx.send(());
        let _ = self.stopped_rx.await;
        eprintln!("✓ tunnel stopped: {TUN_NAME}");
        tracing::info!(tun = TUN_NAME, "WG tunnel stopped");
    }
}

/// Bring the tunnel up using the bundle stored at pair time.
pub async fn start(bundle: &PairingBundle) -> Result<TunnelHandle> {
    // Sweep any orphaned interface left behind by a prior unclean shutdown
    // (panic, kill -9, OS-level network reset). `ip link del` returns
    // "Cannot find device" if there's no leftover, which is the expected
    // case; we ignore that. Doing this BEFORE create_tun avoids EBUSY /
    // EEXIST surprises in the gotatun stack.
    sweep_stale_interface().await;

    // 1. Load this device's WG private key (saved at pair time).
    let priv_b64 = keychain::load_wg_private()
        .context("load WG private key from keychain")?
        .ok_or_else(|| {
            anyhow!(
                "no WG private key in keychain — re-run `virtues-client pair` \
                 (this can happen if the bundle was paired before keypair \
                 persistence was wired)"
            )
        })?;
    let priv_bytes = b64_decode_32(&priv_b64).context("decode WG private key")?;
    let private_key = StaticSecret::from(priv_bytes);

    // 2. Parse the box's WG identity + endpoint from the bundle.
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

    // 3. Parse the addresses we route inside the tunnel.
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

    // 4. Compose the builder. `create_tun` is the syscall that needs
    //    CAP_NET_ADMIN; everything else is in-process.
    let server_allowed = IpNetwork::new(server_addr, addr_prefix_full(server_addr))
        .context("compose server allowed-ip network")?;

    let builder = DeviceBuilder::new()
        .with_default_udp()
        .with_listen_port(0) // ephemeral; OS picks a port
        .create_tun(TUN_NAME)
        .with_context(|| {
            format!(
                "create TUN device `{TUN_NAME}` — does this process have \
                 CAP_NET_ADMIN? (run with sudo for dev, or \
                 `sudo setcap cap_net_admin+ep /usr/local/bin/virtues-client`)"
            )
        })?
        .with_private_key(private_key)
        .with_peer({
            // gotatun 0.7 has no `with_keepalive` builder — `keepalive` is a
            // plain pub field on `Peer`.
            let mut peer = Peer::new(server_pub)
                .with_endpoint(server_endpoint)
                .with_preshared_key(preshared)
                .with_allowed_ip(server_allowed);
            peer.keepalive = Some(KEEPALIVE_SECS);
            peer
        });

    // 5. Configure the L3 addressing on the new TUN before build() so the
    //    state machine doesn't drop packets while we're still adding the
    //    address.
    configure_tun_link(client_addr, server_addr).await?;

    // 6. Build the device, then move it into a background task that owns
    //    the typed handle. The task awaits a oneshot shutdown signal and
    //    confirms back when Device::stop completes — that's the only path
    //    that can call typed methods on `Device`, since we don't spell
    //    `Device<(UdpSocketFactory, TunDevice, TunDevice)>` at this crate's
    //    public boundary.
    let device = builder.build().await.context("gotatun device build")?;

    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let (stopped_tx, stopped_rx) = oneshot::channel::<()>();

    tokio::spawn(async move {
        // Block until `TunnelHandle::stop()` (or the runtime walking the
        // value graph at shutdown) drops the sender; either way we proceed
        // to graceful shutdown.
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

/// Delete any leftover `virtues0` interface from a prior crashed run.
/// "No such device" is the expected case on a clean machine; we treat any
/// other error as a soft warning since the subsequent `create_tun` would
/// surface a real problem more clearly.
async fn sweep_stale_interface() {
    let result = Command::new("ip")
        .args(["link", "del", TUN_NAME])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .await;
    match result {
        Ok(out) if out.status.success() => {
            tracing::info!(tun = TUN_NAME, "swept orphaned tunnel interface from prior run");
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            let lower = stderr.to_ascii_lowercase();
            // Both wordings depending on iproute2 version.
            if lower.contains("cannot find device") || lower.contains("does not exist") {
                tracing::debug!(tun = TUN_NAME, "no stale interface to sweep (clean state)");
            } else {
                tracing::warn!(
                    tun = TUN_NAME,
                    status = %out.status,
                    stderr = %stderr.trim(),
                    "ip link del returned non-zero (continuing)"
                );
            }
        }
        Err(e) => {
            tracing::warn!("ip link del failed to spawn: {e}; continuing");
        }
    }
}

/// Bring the TUN device's L3 up. Three `ip` commands:
///
///     ip link set dev virtues0 mtu 1280 up
///     ip addr add <client_addr>/<prefix> dev virtues0
///     ip route add <server_addr>/<prefix> dev virtues0
async fn configure_tun_link(client: IpAddr, server: IpAddr) -> Result<()> {
    run_idempotent(&[
        "ip", "link", "set", "dev", TUN_NAME, "mtu", &TUN_MTU.to_string(), "up",
    ])
    .await
    .context("ip link set up")?;

    let client_cidr = format!("{client}/{}", addr_prefix_full(client));
    run_idempotent(&["ip", "addr", "add", &client_cidr, "dev", TUN_NAME])
        .await
        .context("ip addr add")?;

    let server_cidr = format!("{server}/{}", addr_prefix_full(server));
    run_idempotent(&["ip", "route", "add", &server_cidr, "dev", TUN_NAME])
        .await
        .context("ip route add")?;

    Ok(())
}

/// Run an `ip` command. Tolerates the "already configured" error returned by
/// iproute2 when the resource exists — every other non-zero exit is a real
/// failure and surfaces with full stderr context.
///
/// Without the discrimination we'd silently start with a half-configured
/// tunnel after a typo, a missing device, or a kernel rejection — the worst
/// kind of bug to debug. EEXIST patterns from iproute2 (matched
/// case-insensitively to ride out wording shifts between distro versions):
///
///   - `"RTNETLINK answers: File exists"` — `ip addr add`, `ip route add`
///   - `"already assigned"` / `"already attached"` — newer wording
///   - explicit `"EEXIST"` — older versions
async fn run_idempotent(argv: &[&str]) -> Result<()> {
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
    if lower.contains("file exists")
        || lower.contains("already assigned")
        || lower.contains("already attached")
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
        // 32 bytes of zeros base64-encoded → 44 chars padded
        let s = base64::engine::general_purpose::STANDARD.encode([0u8; 32]);
        assert!(b64_decode_32(&s).is_ok());
    }
}
