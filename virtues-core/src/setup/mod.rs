//! Interactive setup wizard for Virtues

pub mod validation;

use console::style;
use dialoguer::{theme::ColorfulTheme, Confirm, Input};

use crate::error::Result;
use validation::{display_error, display_info, display_success};

/// Configuration collected from the setup wizard
#[derive(Debug, Clone)]
pub struct SetupConfig {
    pub database_url: String,
    pub server_url: String,
    pub storage_path: String,
    pub encryption_key: Option<String>,
    pub run_migrations: bool,
}

/// Run the interactive setup wizard
pub async fn run_init() -> Result<SetupConfig> {
    println!();
    println!("{}", style("🎯 Virtues Setup").bold().cyan());
    println!();

    // Step 1: Database configuration
    let database_url = setup_database().await?;

    // Step 2: Server URL for device pairing
    let server_url = setup_server_url()?;

    // Step 3: Run migrations?
    let run_migrations = Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt("Run migrations now?")
        .default(true)
        .interact()
        .unwrap_or(true);

    // Step 4: Encryption key
    let encryption_key = setup_encryption_key()?;

    // Step 5: Storage configuration
    let storage_path = setup_storage().await?;

    Ok(SetupConfig {
        database_url,
        server_url,
        storage_path,
        encryption_key,
        run_migrations,
    })
}

/// Build a SetupConfig from environment defaults — no prompts. Used by
/// `virtues init` Recommended mode and any other zero-question caller.
///
/// Precedence:
///   1. Environment variable (install.sh wrote `/var/lib/virtues/virtues.env`,
///      systemd's EnvironmentFile + this binary's env loader populate the
///      process env).
///   2. Production peer-auth default (`postgres:///virtues`) when env is
///      empty — matches what install.sh would have written.
pub fn recommended_config() -> Result<SetupConfig> {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres:///virtues".to_string());
    let server_url = std::env::var("VIRTUES_SERVER_URL")
        .unwrap_or_else(|_| "virtues.local".to_string());
    let storage_path = std::env::var("STORAGE_PATH")
        .unwrap_or_else(|_| "/var/lib/virtues/lake".to_string());
    // If VIRTUES_ENCRYPTION_KEY is already in env, the binary uses it
    // directly; we don't need to generate a new one here. Pass None so
    // save_config (if ever called) doesn't try to rotate it.
    let encryption_key = std::env::var("VIRTUES_ENCRYPTION_KEY").ok();
    Ok(SetupConfig {
        database_url,
        server_url,
        storage_path,
        encryption_key,
        run_migrations: true,
    })
}

/// Database setup step
async fn setup_database() -> Result<String> {
    println!("{}", style("📦 Database Configuration").bold());

    let database_url: String = Input::with_theme(&ColorfulTheme::default())
        .with_prompt("Postgres DATABASE_URL")
        .default("postgres://virtues:virtues@localhost:5432/virtues".to_string())
        .interact_text()
        .map_err(|e| crate::error::Error::Other(format!("Input error: {}", e)))?;

    // Test connection
    display_info("Testing connection...");
    match validation::test_database_connection(&database_url).await {
        Ok(_) => {
            display_success("Connected!");
        }
        Err(e) => {
            display_error(&format!("Connection failed: {}", e));
            return Err(e);
        }
    }

    println!();
    Ok(database_url)
}

/// Server URL setup step for device pairing
fn setup_server_url() -> Result<String> {
    println!("{}", style("🌐 Server Configuration").bold());

    let server_url: String = Input::with_theme(&ColorfulTheme::default())
        .with_prompt("Server URL (for device pairing)")
        .default("localhost:8000".to_string())
        .interact_text()
        .map_err(|e| crate::error::Error::Other(format!("Input error: {}", e)))?;

    display_info("This URL will be shown to users when pairing devices.");

    println!();
    Ok(server_url)
}

/// Encryption key setup step
fn setup_encryption_key() -> Result<Option<String>> {
    println!("{}", style("🔐 Security").bold());

    let generate = Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt("Generate encryption key for OAuth tokens?")
        .default(true)
        .interact()
        .unwrap_or(true);

    let key = if generate {
        let key = generate_encryption_key();
        display_success("Encryption key generated");
        Some(key)
    } else {
        None
    };

    println!();
    Ok(key)
}

/// Storage setup step
async fn setup_storage() -> Result<String> {
    println!("{}", style("💾 Storage Configuration").bold());

    let path: String = Input::with_theme(&ColorfulTheme::default())
        .with_prompt("Storage path for stream archives")
        .default("./data/lake".to_string())
        .interact_text()
        .map_err(|e| crate::error::Error::Other(format!("Input error: {}", e)))?;

    // Test local storage
    display_info("Testing storage...");
    match validation::test_local_storage(&path).await {
        Ok(_) => {
            display_success(&format!("Using: {}", path));
        }
        Err(e) => {
            display_error(&format!("Storage test failed: {}", e));
            return Err(e);
        }
    }

    println!();
    Ok(path)
}

/// Generate a random 32-byte encryption key
fn generate_encryption_key() -> String {
    use rand::Rng;
    let mut rng = rand::rng();
    let key: [u8; 32] = rng.random();
    base64::Engine::encode(&base64::engine::general_purpose::STANDARD, key)
}

/// Save configuration to .env file
pub fn save_config(config: &SetupConfig) -> Result<()> {
    // Check if .env already exists
    if std::path::Path::new(".env").exists() {
        println!();
        println!(
            "{} {}",
            style("⚠️").yellow().bold(),
            style(".env already exists in this directory")
                .yellow()
                .bold()
        );

        let overwrite = Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt("Overwrite existing .env file?")
            .default(false)
            .interact()
            .unwrap_or(false);

        if !overwrite {
            println!();
            println!(
                "{} Configuration cancelled. Your existing .env was not modified.",
                style("✓").green().bold()
            );
            println!();
            return Ok(());
        }

        // Defense-in-depth: even though the user just said yes, snapshot the
        // existing .env to .env.bak.<unix-ts> so a mis-keyed init can't lose
        // real secrets. Cheap insurance for a footgun.
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let backup = format!(".env.bak.{ts}");
        if let Err(e) = std::fs::copy(".env", &backup) {
            // Non-fatal — but we refuse to proceed silently. The user wanted
            // an overwrite; if we can't snapshot first, bail.
            return Err(crate::error::Error::Other(format!(
                "could not back up existing .env to {backup}: {e}"
            )));
        }
        println!(
            "{} backed up existing .env → {}",
            style("✓").green().bold(),
            style(&backup).dim()
        );
    }

    let mut content = String::new();

    content.push_str("# Generated by virtues init\n\n");

    // Database
    content.push_str("# Database (required)\n");
    content.push_str(&format!("DATABASE_URL={}\n\n", config.database_url));

    // Server URL
    content.push_str("# Server URL for device pairing (required for device sources)\n");
    content.push_str(&format!("VIRTUES_SERVER_URL={}\n\n", config.server_url));

    // Storage
    content.push_str("# Storage path for stream archives\n");
    content.push_str(&format!("STORAGE_PATH={}\n\n", config.storage_path));

    // Encryption key
    if let Some(key) = &config.encryption_key {
        content.push_str("# Security (recommended)\n");
        content.push_str(&format!("VIRTUES_ENCRYPTION_KEY={}\n\n", key));
    }

    // OAuth Proxy (informational)
    content.push_str("# OAuth Proxy — defaults to https://auth.virtues.com\n");
    content.push_str("# Self-hosting your own proxy? Uncomment and point at it:\n");
    content.push_str("# VIRTUES_OAUTH_PROXY_URL=https://auth.example.com\n");

    // Write to .env file
    std::fs::write(".env", content)
        .map_err(|e| crate::error::Error::Other(format!("Failed to write .env: {}", e)))?;

    display_success("Configuration saved to .env");
    Ok(())
}

/// Display completion message with next steps
pub fn display_completion() {
    println!();
    println!("{}", style("Done! Try these commands:").bold().green());
    println!(
        "  {} - Browse available integrations",
        style("virtues catalog sources").cyan()
    );
    println!(
        "  {} - Connect your Notion workspace",
        style("virtues add notion").cyan()
    );
    println!(
        "  {} - List connected sources",
        style("virtues source list").cyan()
    );
    println!();
}
