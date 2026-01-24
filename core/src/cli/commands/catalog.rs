//! Catalog command handlers - browse available sources and streams

use crate::cli::types::CatalogCommands;

/// Handle catalog/registry browsing commands
pub fn handle_catalog_command(
    action: Option<CatalogCommands>,
) -> Result<(), Box<dyn std::error::Error>> {
    match action {
        None | Some(CatalogCommands::Sources) => {
            // List all available sources from registry
            let sources = crate::list_available_sources();

            println!("Available Data Sources:");
            println!("{:<15} {:<30} {}", "Type", "Name", "Auth");
            println!("{}", "-".repeat(60));

            for source in sources {
                let auth_type = match source.descriptor.auth_type {
                    crate::registry::AuthType::OAuth2 => "OAuth2",
                    crate::registry::AuthType::Device => "Device",
                    crate::registry::AuthType::ApiKey => "API Key",
                    crate::registry::AuthType::None => "None",
                };
                println!(
                    "{:<15} {:<30} {}",
                    source.descriptor.name, source.descriptor.display_name, auth_type
                );
            }

            println!();
            println!("Use 'virtues catalog source <type>' for details about streams");
        }

        Some(CatalogCommands::Source { name }) => {
            let info = crate::get_source_info(&name)
                .ok_or_else(|| format!("Source '{}' not found", name))?;

            println!("Source: {}", info.descriptor.display_name);
            println!("Type: {}", info.descriptor.name);
            println!("Description: {}", info.descriptor.description);
            println!();
            println!("Authentication: {:?}", info.descriptor.auth_type);

            if let Some(oauth_config) = &info.descriptor.oauth_config {
                println!("OAuth Scopes: {}", oauth_config.scopes.join(", "));
            }

            println!();
            println!("Available Streams:");
            println!("{:<20} {:<40} {}", "Stream", "Description", "Sync Modes");
            println!("{}", "-".repeat(80));

            for stream in &info.streams {
                let mut modes = Vec::new();
                if stream.descriptor.supports_incremental {
                    modes.push("incremental");
                }
                if stream.descriptor.supports_full_refresh {
                    modes.push("full");
                }
                let modes_str = modes.join(", ");

                println!(
                    "{:<20} {:<40} {}",
                    stream.descriptor.name, stream.descriptor.description, modes_str
                );
            }
        }

        Some(CatalogCommands::Streams) => {
            // List all streams across all sources
            let streams = crate::list_all_streams();

            println!("Available Streams:\n");
            println!(
                "{:<15} {:<15} {:<45} {}",
                "Source", "Stream", "Description", "Sync Modes"
            );
            println!("{}", "─".repeat(90));

            for (source_name, stream) in streams {
                let mut modes = Vec::new();
                if stream.descriptor.supports_incremental {
                    modes.push("incremental");
                }
                if stream.descriptor.supports_full_refresh {
                    modes.push("full");
                }
                let modes_str = modes.join(", ");

                println!(
                    "{:<15} {:<15} {:<45} {}",
                    source_name, stream.descriptor.name, stream.descriptor.description, modes_str
                );
            }

            println!();
            println!("Use 'virtues catalog source <type>' for details about a specific source");
            println!("Use 'virtues add <type>' to connect a source");
        }
    }

    Ok(())
}
