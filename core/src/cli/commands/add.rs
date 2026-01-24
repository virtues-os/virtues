//! Add command - add new OAuth or device sources

use crate::client::Virtues;
use crate::DeviceInfo;
use console::style;
use std::env;

/// Handle adding a new source (OAuth or device)
pub async fn handle_add_source(
    virtues: Virtues,
    source_type: &str,
    _device_id: Option<String>,
    name: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("🔐 Adding {} source...", source_type);

    // Check if this is a device source
    let descriptor = crate::get_source_info(source_type);

    let is_device_source = descriptor
        .map(|d| matches!(d.descriptor.auth_type, crate::registry::AuthType::Device))
        .unwrap_or(false);

    if is_device_source {
        // Handle device pairing flow
        let name = name.ok_or_else(|| "name is required for device sources".to_string())?;
        return handle_device_pairing(virtues, source_type, &name).await;
    }

    // Handle OAuth flow
    let redirect_uri = "http://localhost:8080";
    let response = crate::initiate_oauth_flow(source_type, Some(redirect_uri.to_string()), None)
        .await
        .map_err(|e| format!("Failed to initiate OAuth flow: {e}"))?;

    println!("\n🌐 Please visit the following URL to authorize:");
    println!("{}", response.authorization_url);
    println!("\nPress Enter after you've authorized and been redirected...");

    // Open browser automatically
    #[cfg(not(target_os = "windows"))]
    std::process::Command::new("open")
        .arg(&response.authorization_url)
        .spawn()
        .ok();
    #[cfg(target_os = "windows")]
    std::process::Command::new("cmd")
        .args(&["/C", "start", &response.authorization_url])
        .spawn()
        .ok();

    // Wait for user input
    use std::io::{self, Write};
    io::stdout().flush()?;
    let mut _input = String::new();
    io::stdin().read_line(&mut _input)?;

    // Get the authorization code from the redirect URL
    println!("\n📋 Please paste the full redirect URL here:");
    io::stdout().flush()?;
    let mut redirect_url = String::new();
    io::stdin().read_line(&mut redirect_url)?;

    // Parse callback URL parameters
    let callback_params = parse_callback_url(&redirect_url, source_type)?;

    // Handle callback and create source
    let response = crate::handle_oauth_callback(virtues.database.pool(), &callback_params).await?;

    println!("\n✅ Source created successfully!");
    println!("   Name: {}", response.source.name);
    println!("   ID: {}", response.source.id);

    // List available streams
    let streams = crate::list_source_streams(virtues.database.pool(), response.source.id.clone()).await?;
    if !streams.is_empty() {
        println!("\n📊 Available streams (all disabled by default):");
        for stream in streams {
            println!(
                "   - {} ({})",
                stream.stream_name,
                if stream.is_enabled {
                    "enabled"
                } else {
                    "disabled"
                }
            );
        }
        println!(
            "\n💡 Enable streams with: virtues stream enable {} <stream_name>",
            response.source.id
        );
    }

    Ok(())
}

/// Parse OAuth callback URL into parameters
fn parse_callback_url(
    url: &str,
    provider: &str,
) -> Result<crate::OAuthCallbackParams, Box<dyn std::error::Error>> {
    use oauth2::url::Url;

    // Trim whitespace (including newline from stdin)
    let url = url.trim();

    let parsed_url = Url::parse(url)?;
    let params: std::collections::HashMap<_, _> = parsed_url.query_pairs().collect();

    // Extract common OAuth parameters
    let code = params.get("code").map(|s| s.to_string());
    let access_token = params.get("access_token").map(|s| s.to_string());
    let refresh_token = params.get("refresh_token").map(|s| s.to_string());
    let expires_in = params.get("expires_in").and_then(|s| s.parse::<i64>().ok());
    let state = params.get("state").map(|s| s.to_string());

    // Notion-specific fields
    let workspace_id = params.get("workspace_id").map(|s| s.to_string());
    let workspace_name = params.get("workspace_name").map(|s| s.to_string());
    let bot_id = params.get("bot_id").map(|s| s.to_string());

    // Validate that we have either code or access_token
    if code.is_none() && access_token.is_none() {
        return Err(
            "OAuth callback URL must contain either 'code' or 'access_token' parameter".into(),
        );
    }

    Ok(crate::OAuthCallbackParams {
        code,
        access_token,
        refresh_token,
        expires_in,
        provider: provider.to_string(),
        state,
        workspace_id,
        workspace_name,
        bot_id,
    })
}

/// Handle device pairing flow (Manual Link)
async fn handle_device_pairing(
    virtues: Virtues,
    device_type: &str,
    name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    use crate::cli::display::*;
    use std::io::{self, Write};

    let server_url =
        env::var("VIRTUES_SERVER_URL").unwrap_or_else(|_| "http://localhost:8000".to_string());

    println!(
        "\n📱 Adding {} device manually",
        style(device_type).cyan().bold()
    );
    println!("{}", style("━".repeat(50)).dim());
    println!("\n1. Open the {} app on your device", device_type);
    println!(
        "2. Go to {} and ensure the Server Endpoint is set to:",
        style("Settings").bold()
    );
    println!("   {}", style(&server_url).yellow());
    println!(
        "3. Copy the {} from the app settings",
        style("Device ID").bold()
    );

    print!("\n📝 Enter the Device ID here: ");
    io::stdout().flush()?;

    let mut device_id = String::new();
    io::stdin().read_line(&mut device_id)?;
    let device_id = device_id.trim();

    if device_id.is_empty() {
        println!("{}", style("❌ Device ID cannot be empty").red());
        return Ok(());
    }

    // Call the manual link endpoint logic directly
    // Note: We access the internal API function directly here since we are in the core crate
    match crate::api::link_device_manually(virtues.database.pool(), device_id, name, device_type)
        .await
    {
        Ok(completed) => {
            let device_info = DeviceInfo {
                device_id: device_id.to_string(),
                device_name: name.to_string(),
                device_model: "Unknown".to_string(),
                os_version: "Unknown".to_string(),
                app_version: None,
            };

            display_pairing_success(&device_info, &completed.source_id);

            // Fetch actual stream connections from DB to display correct status/types
            let streams =
                crate::list_source_streams(virtues.database.pool(), completed.source_id.clone()).await?;
            display_available_streams(&streams, &completed.source_id);
        }
        Err(e) => {
            println!(
                "\n{}",
                style(format!("❌ Failed to link device: {}", e)).red()
            );
        }
    }

    Ok(())
}
