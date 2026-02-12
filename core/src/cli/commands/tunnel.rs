//! Cloudflare quick tunnel command handler - start server with HTTPS tunnel for iOS/Mac testing

use crate::Virtues;
use console::style;
use regex::Regex;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};

/// Handle tunnel command - start server with Cloudflare quick tunnel
pub async fn handle_tunnel_command(virtues: Virtues) -> Result<(), Box<dyn std::error::Error>> {
    check_cloudflared_installed()?;

    // Run migrations and seed data (same as auto-setup mode)
    println!("📊 Running migrations...");
    virtues.database.initialize().await?;
    println!("✅ Migrations complete");

    println!("🌱 Seeding defaults...");
    crate::seeding::prod_seed::seed_production_data(&virtues.database).await?;
    println!("✅ Seeding complete");

    println!();
    println!(
        "{}",
        style("🚀 Starting Virtues server with Cloudflare Tunnel...").bold()
    );

    // Start cloudflared quick tunnel in background
    let port = 8000;
    let mut tunnel_child = start_tunnel_process(port)?;
    println!("{} cloudflared process started", style("✓").green());

    // Wait for the tunnel URL from cloudflared stderr
    println!("{} Waiting for tunnel URL...", style("⏳").dim());
    let tunnel_url = wait_for_tunnel_url(&mut tunnel_child).await?;
    println!("{} tunnel ready", style("✓").green());

    // Set BACKEND_URL so the server uses the tunnel URL for OAuth callbacks etc.
    std::env::set_var("BACKEND_URL", &tunnel_url);

    display_connection_info(&tunnel_url);

    // Start AXUM server (this will block until Ctrl+C)
    println!("{} Starting Virtues server...", style("⏳").dim());
    println!();

    crate::server::run(virtues, "0.0.0.0", port).await?;

    // Cleanup cloudflared process when server stops
    println!();
    println!("{} Stopping cloudflared...", style("⏳").dim());
    if let Err(e) = tunnel_child.kill().await {
        tracing::warn!("Failed to stop cloudflared: {}", e);
    } else {
        println!("{} cloudflared stopped", style("✓").green());
    }

    Ok(())
}

/// Check if cloudflared is installed and available in PATH
fn check_cloudflared_installed() -> Result<(), Box<dyn std::error::Error>> {
    let result = std::process::Command::new("cloudflared")
        .arg("version")
        .output();

    match result {
        Ok(output) if output.status.success() => Ok(()),
        Ok(_) => Err(
            "cloudflared command found but returned an error. Please verify your installation."
                .into(),
        ),
        Err(_) => Err(
            "cloudflared is not installed. Install with:\n  brew install cloudflared".into(),
        ),
    }
}

/// Start cloudflared quick tunnel as a child process
fn start_tunnel_process(port: u16) -> Result<Child, Box<dyn std::error::Error>> {
    let child = Command::new("cloudflared")
        .args(["tunnel", "--url", &format!("http://localhost:{}", port)])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;

    Ok(child)
}

/// Wait for cloudflared to print the tunnel URL to stderr, then keep draining
/// stderr in the background so cloudflared doesn't die from SIGPIPE.
async fn wait_for_tunnel_url(child: &mut Child) -> Result<String, Box<dyn std::error::Error>> {
    let stderr = child
        .stderr
        .take()
        .ok_or("Failed to capture cloudflared stderr")?;

    // Also drain stdout so its pipe buffer doesn't fill up and block cloudflared
    if let Some(stdout) = child.stdout.take() {
        tokio::spawn(async move {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();
            while let Ok(Some(_)) = lines.next_line().await {}
        });
    }

    let reader = BufReader::new(stderr);
    let mut lines = reader.lines();
    let re = Regex::new(r"https://[a-zA-Z0-9-]+\.trycloudflare\.com")?;

    let timeout = Duration::from_secs(30);
    let start = std::time::Instant::now();

    while start.elapsed() < timeout {
        match tokio::time::timeout(Duration::from_secs(2), lines.next_line()).await {
            Ok(Ok(Some(line))) => {
                if let Some(m) = re.find(&line) {
                    let url = m.as_str().to_string();
                    // Keep draining stderr in background to prevent SIGPIPE killing cloudflared
                    tokio::spawn(async move {
                        while let Ok(Some(_)) = lines.next_line().await {}
                    });
                    return Ok(url);
                }
            }
            Ok(Ok(None)) => break, // EOF
            Ok(Err(e)) => return Err(format!("Error reading cloudflared output: {}", e).into()),
            Err(_) => continue, // timeout on this line, keep waiting
        }
    }

    Err(
        "Timeout: Could not get tunnel URL after 30 seconds.\n\
         Check that cloudflared is working: cloudflared tunnel --url http://localhost:8000"
            .into(),
    )
}

/// Display connection information in a formatted way
fn display_connection_info(tunnel_url: &str) {
    println!("{}", style("━".repeat(70)).dim());
    println!();
    println!(
        "{} {}",
        style("📱").bold(),
        style("Use this URL in your iOS/Mac app settings:").bold()
    );
    println!();
    println!(
        "  {}  {}",
        style("HTTPS URL:").cyan().bold(),
        style(tunnel_url).yellow().bold()
    );
    println!();
    println!("{}", style("━".repeat(70)).dim());
    println!();
    println!(
        "{}{}",
        style("Note: ").dim(),
        style("Press Ctrl+C to stop both services.").dim()
    );
    println!();
}
