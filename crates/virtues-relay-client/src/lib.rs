//! Box-side client for the blind relay.
//!
//! The box **dials out** to the relay (so it needs no public inbound port and
//! works behind CGNAT), `Register`s its SNI, and holds a control connection. On
//! each `OpenConn` it dials a fresh **work connection** to the relay and splices
//! it to the box's own local TLS service — so the box terminates TLS with its own
//! cert and the relay only ever moves ciphertext.
//!
//! Reconnects with **Full-Jitter** backoff (and an initial splay) so a fleet
//! severed by a relay restart doesn't thunder back in lockstep.

mod wire;

use anyhow::{anyhow, Result};
use std::time::Duration;
use tokio::net::TcpStream;
use virtues_protocol::relay::{BoxHello, BoxMsg, RelayMsg};

use crate::wire::{read_msg, write_msg};

/// Full-Jitter backoff bounds (AWS "Exponential Backoff And Jitter").
const BACKOFF_BASE: Duration = Duration::from_secs(1);
const BACKOFF_CAP: Duration = Duration::from_secs(60);
/// Initial random splay before the *first* connect, so boxes that boot/restart
/// together don't all dial at the same instant.
const STARTUP_SPLAY: Duration = Duration::from_secs(30);

#[derive(Clone, Debug)]
pub struct RelayClientConfig {
    /// Relay control address the box dials out to (host:port).
    pub relay_addr: String,
    /// This box's SNI, e.g. `abc123.boxes.virtues.com`.
    pub sni: String,
    /// Shared bearer presented at `Register` (v1 auth; blinded tokens in P3).
    pub token: String,
    /// The box's own local TLS service to forward work connections to,
    /// e.g. `127.0.0.1:8443`. The box terminates TLS here with its own cert.
    pub local_addr: String,
}

/// Run the relay client forever: connect, serve, reconnect with jittered backoff.
/// Intended to be `tokio::spawn`ed alongside the box's HTTP/TLS server.
pub async fn run(cfg: RelayClientConfig) {
    // Initial splay so a fleet booting/restarting together doesn't dial in lockstep.
    tokio::time::sleep(splay()).await;

    let mut attempt: u32 = 0;
    loop {
        match serve_once(&cfg).await {
            Ok(()) => {
                tracing::info!(sni = %cfg.sni, "relay control connection closed");
                attempt = 0;
            }
            Err(e) => tracing::warn!(sni = %cfg.sni, error = %e, "relay control connection failed"),
        }
        let delay = backoff_delay(attempt);
        attempt = attempt.saturating_add(1);
        tracing::debug!(sni = %cfg.sni, ?delay, attempt, "reconnecting to relay");
        tokio::time::sleep(delay).await;
    }
}

/// One control-connection lifecycle: connect, register, then serve `OpenConn` /
/// `Ping` until the connection closes or errors. No splay or backoff (the caller
/// owns reconnection) — also the entry point integration tests drive directly.
pub async fn serve_once(cfg: &RelayClientConfig) -> Result<()> {
    let stream = TcpStream::connect(&cfg.relay_addr).await?;
    let (mut rd, mut wr) = stream.into_split();

    write_msg(
        &mut wr,
        &BoxHello::Register {
            sni: cfg.sni.clone(),
            token: cfg.token.clone(),
        },
    )
    .await?;

    match read_msg::<_, RelayMsg>(&mut rd).await? {
        RelayMsg::Registered => tracing::info!(sni = %cfg.sni, "registered with relay"),
        RelayMsg::Rejected { reason } => return Err(anyhow!("relay rejected register: {reason}")),
        other => return Err(anyhow!("unexpected pre-register message: {other:?}")),
    }

    // Control loop: respond to OpenConn (dial a work conn) and Ping (Pong).
    loop {
        match read_msg::<_, RelayMsg>(&mut rd).await? {
            RelayMsg::OpenConn { conn_id } => {
                let cfg = cfg.clone();
                tokio::spawn(async move {
                    if let Err(e) = serve_work(&cfg, conn_id).await {
                        tracing::debug!(error = %e, "work connection ended");
                    }
                });
            }
            RelayMsg::Ping => write_msg(&mut wr, &BoxMsg::Pong).await?,
            RelayMsg::Registered => {} // tolerate duplicates
            RelayMsg::Rejected { reason } => {
                return Err(anyhow!("relay rejected mid-session: {reason}"))
            }
        }
    }
}

/// Dial a work connection for `conn_id` and splice it to the box's local service.
async fn serve_work(cfg: &RelayClientConfig, conn_id: String) -> Result<()> {
    let mut work = TcpStream::connect(&cfg.relay_addr).await?;
    write_msg(&mut work, &BoxHello::Work { conn_id }).await?;

    // The relay will replay the client's ClientHello and then stream ciphertext;
    // our local service terminates TLS with the box's own cert.
    let mut local = TcpStream::connect(&cfg.local_addr).await?;
    tokio::io::copy_bidirectional(&mut work, &mut local).await?;
    Ok(())
}

/// Full-Jitter delay: `random(0, min(cap, base * 2^attempt))`.
fn backoff_delay(attempt: u32) -> Duration {
    use rand::Rng;
    let exp_ms = BACKOFF_BASE
        .as_millis()
        .saturating_mul(1u128 << attempt.min(20))
        .min(BACKOFF_CAP.as_millis()) as u64;
    let jittered = rand::thread_rng().gen_range(0..=exp_ms);
    Duration::from_millis(jittered)
}

fn splay() -> Duration {
    use rand::Rng;
    let ms = rand::thread_rng().gen_range(0..=STARTUP_SPLAY.as_millis() as u64);
    Duration::from_millis(ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_is_bounded_by_cap() {
        for attempt in 0..40 {
            let d = backoff_delay(attempt);
            assert!(d <= BACKOFF_CAP, "attempt {attempt} exceeded cap: {d:?}");
        }
    }

    #[test]
    fn backoff_grows_then_caps() {
        // The *upper bound* grows with attempt until the cap. We can't assert on
        // a single random draw, so check the cap math by sampling the max many
        // times at a low attempt vs a high attempt.
        let low_max = (0..1000).map(|_| backoff_delay(0)).max().unwrap();
        let high_max = (0..1000).map(|_| backoff_delay(10)).max().unwrap();
        assert!(low_max <= Duration::from_secs(1));
        assert!(high_max > Duration::from_secs(1));
        assert!(high_max <= BACKOFF_CAP);
    }

    #[test]
    fn splay_is_bounded() {
        for _ in 0..1000 {
            assert!(splay() <= STARTUP_SPLAY);
        }
    }
}
