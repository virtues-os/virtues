//! `cargo run -p virtues-improv --features client --example scan [seconds]`
//!
//! What a machine can hear: adapter, service filter, and the state byte that
//! tells "needs wifi" from "already online".
//!
//! **On macOS this only runs from a context that already holds Bluetooth
//! permission**, because TCC attributes permission to an app BUNDLE and a
//! plain CLI has none. Embedding an Info.plist in the executable does not
//! satisfy it (tried, 2026-08-11 — the process still aborts with SIGABRT and
//! no dialog). Grant Bluetooth to your terminal in System Settings → Privacy
//! & Security → Bluetooth, or use the app bundle instead. Linux and Windows
//! have no such gate.
//!
//! Pass a box id as the second argument to also read that box's own wifi scan
//! over BLE (RPC 0x04), which is the next thing setup does:
//!
//! ```text
//! cargo run -p virtues-improv --features client --example scan 6 <id>
//! ```

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let seconds: f64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(5.0);
    let target = args.next();

    let client = virtues_improv::ImprovClient::shared();
    eprintln!("scanning {seconds}s for Improv boxes…");
    let boxes = client.discover(seconds).await?;
    if boxes.is_empty() {
        eprintln!("none heard. The box advertises only while UNCLAIMED; check it is powered,");
        eprintln!("and that this machine has Bluetooth permission.");
        return Ok(());
    }
    for b in &boxes {
        let state = match b.improv_state {
            0x02 => "needs wifi",
            0x04 => "online",
            other => &format!("state 0x{other:02x}"),
        };
        println!("{}  [{}]  rssi {}  id={}", b.name, state, b.rssi, b.id);
    }

    if let Some(id) = target {
        eprintln!("\nasking {id} for its own wifi scan…");
        for n in client.wifi_scan(&id).await? {
            let kind = if n.enterprise {
                "work network"
            } else if n.secured {
                "locked"
            } else {
                "open"
            };
            println!("  {:<28} {:>4}  {}", n.ssid, n.signal, kind);
        }
        client.disconnect().await;
    }
    Ok(())
}
