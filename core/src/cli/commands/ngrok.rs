//! ngrok command handler - start server with HTTPS tunnel for iOS/Mac testing

use crate::Virtues;
use console::style;
use regex::Regex;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};

/// Handle ngrok command - start server with HTTPS tunnel
pub async fn handle_ngrok_command(virtues: Virtues) -> Result<(), Box<dyn std::error::Error>> {
    // Check if ngrok is installed
    check_ngrok_installed()?;

    println!();
    println!("{}", style("🚀 Starting Virtues server with ngrok HTTPS tunnel...").bold());

    // Start ngrok in background
    let port = 8000;
    let mut ngrok_child = start_ngrok_process(port)?;
    println!("{} ngrok process started", style("✓").green());

    // Wait for ngrok to initialize and get HTTPS URL
    println!("{} Waiting for ngrok to initialize...", style("⏳").dim());
    let ngrok_url = wait_for_ngrok_url(&mut ngrok_child).await?;
    println!("{} ngrok tunnel ready", style("✓").green());

    // Display connection information
    let dashboard_url = "http://localhost:4040";
    display_connection_info(&ngrok_url, dashboard_url);

    // Start AXUM server (this will block until Ctrl+C)
    println!("{} Starting Virtues server...", style("⏳").dim());
    println!();

    crate::server::run(virtues, "0.0.0.0", port).await?;

    // Cleanup ngrok process when server stops
    println!();
    println!("{} Stopping ngrok...", style("⏳").dim());
    if let Err(e) = ngrok_child.kill().await {
        tracing::warn!("Failed to stop ngrok: {}", e);
    } else {
        println!("{} ngrok stopped", style("✓").green());
    }

    Ok(())
}

/// Check if ngrok is installed and available in PATH
fn check_ngrok_installed() -> Result<(), Box<dyn std::error::Error>> {
    let result = std::process::Command::new("ngrok")
        .arg("version")
        .output();

    match result {
        Ok(output) if output.status.success() => Ok(()),
        Ok(_) => Err(
            "ngrok command found but returned an error. Please verify your installation.".into(),
        ),
        Err(_) => Err(
            "ngrok is not installed. Please install it with:\n  brew install ngrok\n\nThen sign up and add your authtoken:\n  ngrok config add-authtoken YOUR_TOKEN".into(),
        ),
    }
}

/// Start ngrok as a child process
fn start_ngrok_process(port: u16) -> Result<Child, Box<dyn std::error::Error>> {
    // Start ngrok in background and capture stdout for URL extraction
    let child = Command::new("ngrok")
        .args(["http", &port.to_string(), "--log=stdout"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;

    Ok(child)
}

/// Wait for ngrok to start and extract the HTTPS URL from its output
async fn wait_for_ngrok_url(child: &mut Child) -> Result<String, Box<dyn std::error::Error>> {
    let mut attempts = 0;
    let max_attempts = 10; // Wait up to 10 seconds

    if let Some(stdout) = child.stdout.take() {
        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();

        while attempts < max_attempts {
            if let Ok(Some(line)) = lines.next_line().await {
                // Try to extract URL from current output
                if let Some(url) = extract_ngrok_url(&line) {
                    return Ok(url);
                }
            }

            // If no URL yet, wait a bit and try again
            tokio::time::sleep(Duration::from_secs(1)).await;
            attempts += 1;
        }
    }

    // Fallback: Try to query ngrok's API endpoint
    let ngrok_url = try_ngrok_api().await?;
    if ngrok_url.is_some() {
        return Ok(ngrok_url.unwrap());
    }

    Err(
        "Timeout: Could not extract ngrok URL after 10 seconds. \
         Please check:\n  1. ngrok is running (visit http://localhost:4040)\n  2. Network connectivity\n  3. ngrok logs for errors"
            .into(),
    )
}

/// Try to get ngrok URL from the inspect API
async fn try_ngrok_api() -> Result<Option<String>, Box<dyn std::error::Error>> {
    // ngrok's web inspection API typically runs on localhost:4040
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()?;

    match client.get("http://localhost:4040/api/tunnels").send().await {
        Ok(response) => {
            if let Ok(text) = response.text().await {
                if let Some(url) = extract_ngrok_url(&text) {
                    return Ok(Some(url));
                }
            }
        }
        Err(_) => {
            // API not available, return None
        }
    }

    Ok(None)
}

/// Extract HTTPS URL from ngrok output
fn extract_ngrok_url(output: &str) -> Option<String> {
    // ngrok shows URLs in various formats, look for https://*.ngrok-free.app or https://*.ngrok.io
    let re = Regex::new(r"https://[a-zA-Z0-9-]+\.ngrok-(free\.)?app").ok();

    if let Some(re) = re {
        if let Some(captures) = re.find(output) {
            return Some(captures.as_str().to_string());
        }
    }

    // Fallback pattern for older ngrok domains
    let re = Regex::new(r"https://[a-zA-Z0-9-]+\.ngrok\.io").ok();
    if let Some(re) = re {
        if let Some(captures) = re.find(output) {
            return Some(captures.as_str().to_string());
        }
    }

    None
}

/// Display connection information in a formatted way
fn display_connection_info(https_url: &str, ngrok_dashboard: &str) {
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
        style(https_url).yellow().bold()
    );
    println!();
    println!(
        "  {}  {}",
        style("ngrok Dashboard:").cyan().bold(),
        style(ngrok_dashboard).blue()
    );
    println!();
    println!("{}", style("━".repeat(70)).dim());
    println!();
    println!(
        "{}{}{}",
        style("Note: ").dim(),
        style("Free ngrok URLs change on restart. ").dim(),
        style("Press Ctrl+C to stop both services.").dim()
    );
    println!();
}