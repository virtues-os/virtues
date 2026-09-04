//! Production seed - baseline data for new deployments
//!
//! Seeds user-specific defaults (assistant profile).
//!
//! Note: Models, agents, and built-in tools are no longer seeded to the database.
//! They are read directly from the virtues-registry crate at runtime.
//! See: crates/virtues-registry/

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

    // Store values in variables before binding (borrow lifetimes)
    let assistant_name = defaults.assistant_name.clone();
    let default_agent_id = defaults.default_agent_id.clone();
    let enabled_tools = defaults.enabled_tools.clone();
    let ui_preferences = defaults.ui_preferences.clone();

    // Update assistant profile with defaults, but only for NULL fields.
    // This preserves any user customizations while setting initial defaults.
    // No model is seeded: NULL slots mean "follow the Virtues default" at read
    // time. Seeding one froze the registry's then-current chat model into the
    // row, where it kept being served long after the default moved on.
    sqlx::query!(
        r#"
        UPDATE app_assistant_profile
        SET
            assistant_name = COALESCE(assistant_name, $1),
            default_agent_id = COALESCE(default_agent_id, $2),
            enabled_tools = COALESCE(enabled_tools, $3),
            ui_preferences = COALESCE(ui_preferences, $4),
            updated_at = now()
        WHERE id = $5
        "#,
        assistant_name,
        default_agent_id,
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

    // Note: Models, agents, and built-in tools are no longer seeded to the database.
    // They are read directly from virtues_registry at runtime.

    seed_assistant_profile(db).await?;

    // The narrative interview's conversation — one fixed chat, present from
    // first boot, so the product's first conversation is already waiting when
    // the person walks in from onboarding. The chat id forces interview mode
    // server-side (see chat_handler); the drafter reads this transcript.
    sqlx::query(
        "INSERT INTO app_chats (id, title, message_count) VALUES ($1, 'In your own words', 0) \
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(crate::api::narrative_draft::INTERVIEW_CHAT_ID)
    .execute(db.pool())
    .await?;

    info!("✅ Production seeding completed successfully");
    Ok(())
}
