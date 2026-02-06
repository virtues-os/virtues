//! Production seed - baseline data for new deployments
//!
//! Seeds user-specific defaults (assistant profile).
//!
//! Note: Models, agents, and built-in tools are no longer seeded to SQLite.
//! They are read directly from the virtues-registry crate at runtime.
//! See: packages/virtues-registry/

use crate::database::Database;
use crate::Result;
use tracing::info;

/// Seed assistant profile defaults
/// Uses static configuration from virtues-registry
/// Updates the singleton assistant profile row with defaults if not already set
pub async fn seed_assistant_profile(db: &Database) -> Result<()> {
    let defaults = virtues_registry::assistant::assistant_profile_defaults();

    // The assistant profile singleton UUID
    let profile_id =
        uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000001").expect("Valid UUID constant");
    let profile_id_str = profile_id.to_string();

    // SQLite requires storing values in variables before binding
    let assistant_name = defaults.assistant_name.clone();
    let default_agent_id = defaults.default_agent_id.clone();
    let default_model_id = defaults.default_model_id.clone();
    let enabled_tools = defaults.enabled_tools.clone();
    let ui_preferences = defaults.ui_preferences.clone();

    // Update assistant profile with defaults, but only for NULL fields
    // This preserves any user customizations while setting initial defaults
    sqlx::query!(
        r#"
        UPDATE app_assistant_profile
        SET
            assistant_name = COALESCE(assistant_name, $1),
            default_agent_id = COALESCE(default_agent_id, $2),
            default_model_id = COALESCE(default_model_id, $3),
            enabled_tools = COALESCE(enabled_tools, $4),
            ui_preferences = COALESCE(ui_preferences, $5),
            updated_at = datetime('now')
        WHERE id = $6
        "#,
        assistant_name,
        default_agent_id,
        default_model_id,
        enabled_tools,
        ui_preferences,
        profile_id_str
    )
    .execute(db.pool())
    .await?;

    info!("✅ Seeded assistant profile defaults from registry");
    Ok(())
}

pub async fn seed_production_data(db: &Database) -> Result<()> {
    info!("🌱 Seeding production defaults...");

    // Note: Models, agents, and built-in tools are no longer seeded to SQLite.
    // They are read directly from virtues_registry at runtime.

    seed_assistant_profile(db).await?;

    info!("✅ Production seeding completed successfully");
    Ok(())
}
