//! Userspace WireGuard tunnel + loopback forwarder.
//!
//! The one client tunnel engine is `virtues-tunnel` (boringtun + smoltcp, no
//! root, no `utun` — shared with iOS). The box is reached entirely in userspace:
//! this module brings the tunnel up and runs a tiny loopback forwarder that
//! bridges each `127.0.0.1` connection from the reverse proxy to a
//! `tunnel.dial()` TCP stream *inside* the tunnel. The proxy stays a plain
//! localhost HTTP client (unchanged); the sync↔async seam lives here.
//!
//! This replaced the old gotatun/`utun` path (which needed root and a system
//! network interface). The WG Noise handshake — which pins the box's static
//! public key — *is* the SPKI trust check; see `virtues-protocol::spki`.

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use tokio::net::{TcpListener, TcpStream};
use virtues_protocol::PairingBundle;
use virtues_tunnel::{Tunnel, TunnelStatus};

use crate::keychain;

/// How long to wait for the WG handshake before giving up. Bounded so an
/// unreachable box (behind hostile wifi / no IPv6) falls back to the BYO
/// upstream quickly rather than hanging the proxy start.
const HANDSHAKE_WAIT: Duration = Duration::from_secs(6);

/// A running userspace tunnel + its loopback forwarder. Holds the tunnel alive;
/// [`stop`](TunnelHandle::stop) tears the WG state machine down and stops the
/// forwarder. The reverse proxy forwards to [`forwarder_addr`](Self::forwarder_addr).
pub struct TunnelHandle {
    _tunnel: Arc<Tunnel>,
    pub forwarder_addr: SocketAddr,
    shutdown: tokio::sync::watch::Sender<bool>,
}

impl TunnelHandle {
    pub async fn stop(self) {
        let _ = self.shutdown.send(true);
        // Dropping the last Arc<Tunnel> tears down the WG event loop.
        eprintln!("✓ tunnel stopped");
    }
}

/// Bring the userspace tunnel up and start the loopback forwarder. Returns once
/// the WG handshake reaches `Connected` (or errors if it can't within
/// [`HANDSHAKE_WAIT`]). The returned handle MUST be held for the proxy's
/// lifetime — dropping it tears the tunnel down.
pub async fn start(bundle: &PairingBundle) -> Result<TunnelHandle> {
    let priv_b64 = keychain::load_wg_private()
        .context("read WG private key from keychain")?
        .ok_or_else(|| anyhow::anyhow!("no WG private key for this device — re-pair"))?;

    let tunnel = Tunnel::connect(bundle, &priv_b64)
        .map_err(|e| anyhow::anyhow!("tunnel connect: {e}"))?;

    // Wait for the handshake so the proxy doesn't 502 on the first request.
    let deadline = Instant::now() + HANDSHAKE_WAIT;
    loop {
        match tunnel.status() {
            TunnelStatus::Connected => break,
            TunnelStatus::Failed(m) => anyhow::bail!("tunnel handshake failed: {m}"),
            TunnelStatus::Closed => anyhow::bail!("tunnel closed unexpectedly"),
            TunnelStatus::Connecting => {
                if Instant::now() >= deadline {
                    anyhow::bail!(
                        "tunnel handshake timed out — the box's WireGuard endpoint isn't \
                         reachable from here (no IPv6 / NAT / hostile network?)"
                    );
                }
                tokio::time::sleep(Duration::from_millis(150)).await;
            }
        }
    }

    let tunnel = Arc::new(tunnel);

    // The box's in-tunnel HTTP address (ULA + http_port from the bundle).
    let box_ip: IpAddr = bundle
        .internal_ip
        .parse()
        .with_context(|| format!("parse internal_ip `{}`", bundle.internal_ip))?;
    let box_port = bundle.http_port;

    // Loopback forwarder: proxy → 127.0.0.1:<ephemeral> → tunnel.dial → box.
    let listener = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .context("bind loopback forwarder")?;
    let forwarder_addr = listener.local_addr()?;

    let (shutdown, mut shutdown_rx) = tokio::sync::watch::channel(false);
    let fwd_tunnel = tunnel.clone();
    tokio::spawn(async move {
        loop {
            let accepted = tokio::select! {
                a = listener.accept() => a,
                _ = shutdown_rx.changed() => break,
            };
            let (inbound, _peer) = match accepted {
                Ok(p) => p,
                Err(e) => {
                    tracing::warn!("forwarder accept failed: {e}");
                    continue;
                }
            };
            let t = fwd_tunnel.clone();
            tokio::spawn(async move {
                if let Err(e) = bridge_conn(inbound, t, box_ip, box_port).await {
                    tracing::debug!("forwarder bridge ended: {e:#}");
                }
            });
        }
    });

    eprintln!("✓ userspace WireGuard tunnel up (forwarder {forwarder_addr})");
    Ok(TunnelHandle {
        _tunnel: tunnel,
        forwarder_addr,
        shutdown,
    })
}

/// Bridge one inbound loopback TCP connection to a tunnel TCP stream,
/// full-duplex. `virtues-tunnel`'s stream is synchronous, so each direction runs
/// on a blocking thread; the inbound socket is split via `try_clone` and the
/// tunnel stream via [`into_split`](virtues_tunnel::TunnelStream::into_split).
async fn bridge_conn(
    inbound: TcpStream,
    tunnel: Arc<Tunnel>,
    ip: IpAddr,
    port: u16,
) -> Result<()> {
    // dial() blocks until the in-tunnel TCP socket is Established.
    let stream = tokio::task::spawn_blocking(move || tunnel.dial(ip, port))
        .await
        .context("tunnel dial task panicked")?
        .map_err(|e| anyhow::anyhow!("tunnel dial {ip}:{port}: {e}"))?;
    let (tun_rd, tun_wr) = stream.into_split();

    // Take the inbound tokio socket back to a blocking std socket and split it
    // into independent read/write handles (they share the fd).
    let inbound_std = inbound.into_std().context("inbound into_std")?;
    inbound_std
        .set_nonblocking(false)
        .context("inbound set_nonblocking(false)")?;
    let in_rd = inbound_std.try_clone().context("clone inbound socket")?;
    let in_wr = inbound_std;

    // up: browser→box ; down: box→browser. Each is a blocking copy loop.
    //
    // Terminating BOTH directions when EITHER peer goes away is the subtle part
    // (a blocking `read()` can't be cancelled, so a half-open connection would
    // otherwise hang a thread forever and leak it from the bounded blocking
    // pool):
    //   • browser closes first → `up` ends → dropping `tun_wr` sends the tunnel
    //     Close, which EOFs `tun_rd`, ending `down`.
    //   • box closes first → `down` ends → we explicitly `shutdown(Both)` the
    //     inbound socket, which unblocks `up`'s `in_rd.read()` (same fd) instead
    //     of leaving it parked forever.
    let up = tokio::task::spawn_blocking(move || {
        let mut r = in_rd;
        let mut w = tun_wr;
        let _ = std::io::copy(&mut r, &mut w);
        // `tun_wr` drops here → tunnel Close → `tun_rd` EOFs.
    });
    let down = tokio::task::spawn_blocking(move || {
        let mut r = tun_rd;
        let mut w = in_wr;
        let _ = std::io::copy(&mut r, &mut w);
        // Unblock a still-parked `up` on a half-open connection.
        let _ = w.shutdown(std::net::Shutdown::Both);
    });
    let _ = tokio::join!(up, down);
    Ok(())
}
