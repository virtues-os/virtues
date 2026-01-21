//! Production seed - baseline data for new deployments
//!
//! Seeds system defaults for models, agents, and sample axiology tags

use super::config_loader;
use crate::database::Database;
use crate::Result;
use chrono::Utc;
use tracing::info;
use uuid::Uuid;

/// Seed system default source connections
/// Loads source connection configurations from config/seeds/_generated_source_connections.json
pub async fn seed_default_sources(db: &Database) -> Result<usize> {
    let source_connections = config_loader::load_source_connections()?;

    let count = source_connections.len();
    for conn in &source_connections {
        // SQLite requires string conversion for timestamps and storing values in variables
        let now = Utc::now().to_rfc3339();
        let id = conn.id.to_string();
        let source = conn.source.clone();
        let name = conn.name.clone();
        let auth_type = conn.auth_type.clone();

        sqlx::query!(
            r#"
            INSERT INTO data_source_connections (
                id, source, name, auth_type, is_active, is_internal, created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (id) DO NOTHING
            "#,
            id,
            source,
            name,
            auth_type,
            conn.is_active,
            conn.is_internal,
            now,
            now
        )
        .execute(db.pool())
        .await?;
    }

    info!(
        "✅ Seeded {} system default source connections from config",
        count
    );
    Ok(count)
}

/// Seed system default stream connections
/// Loads stream connection configurations from config/seeds/_generated_stream_connections.json
/// Applies registry default cron schedules if not explicitly set in JSON
pub async fn seed_default_streams(db: &Database) -> Result<usize> {
    let stream_connections = config_loader::load_stream_connections()?;
    let source_connections = config_loader::load_source_connections()?;

    let count = stream_connections.len();
    for conn in &stream_connections {
        // SQLite requires string conversion for timestamps and UUIDs
        let now = Utc::now().to_rfc3339();
        let id = conn.id.to_string();
        let source_connection_id = conn.source_connection_id.to_string();
        let stream_name = conn.stream_name.clone();
        let table_name = conn.table_name.clone();

        // Look up the source connection to get the source name
        let source_conn = source_connections
            .iter()
            .find(|s| s.id == conn.source_connection_id)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Stream '{}' references unknown source_connection_id {}",
                    conn.stream_name,
                    conn.source_connection_id
                )
            })?;

        // Determine cron_schedule: use JSON value if provided, otherwise look up registry default
        let cron_schedule: Option<String> = match &conn.cron_schedule {
            Some(schedule) => Some(schedule.clone()),
            None => {
                // Look up registry default
                if let Some(registered_stream) =
                    crate::registry::get_stream(&source_conn.source, &conn.stream_name)
                {
                    registered_stream
                        .default_cron_schedule
                        .map(|s| s.to_string())
                } else {
                    None
                }
            }
        };

        sqlx::query!(
            r#"
            INSERT INTO data_stream_connections (
                id, source_connection_id, stream_name, table_name, is_enabled, cron_schedule, created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (id) DO NOTHING
            "#,
            id,
            source_connection_id,
            stream_name,
            table_name,
            conn.is_enabled,
            cron_schedule,
            now,
            now
        )
        .execute(db.pool())
        .await?;
    }

    info!(
        "✅ Seeded {} system default stream connections from config",
        count
    );
    Ok(count)
}

/// Seed system default models (user_id = NULL)
/// Loads model configurations from config/seeds/models.json
pub async fn seed_default_models(db: &Database) -> Result<usize> {
    let models = config_loader::load_models()?;

    let count = models.len();
    for model in &models {
        // SQLite requires storing string values in variables
        let model_id = model.model_id.clone();
        let display_name = model.display_name.clone();
        let provider = model.provider.clone();

        let id = Uuid::new_v4().to_string();
        sqlx::query!(
            r#"
            INSERT INTO app_models (
                id, user_id, model_id, display_name, provider, enabled, sort_order,
                context_window, max_output_tokens, supports_tools, is_default
            )
            VALUES ($1, NULL, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            ON CONFLICT (model_id) WHERE user_id IS NULL DO UPDATE SET
                display_name = EXCLUDED.display_name,
                provider = EXCLUDED.provider,
                enabled = EXCLUDED.enabled,
                sort_order = EXCLUDED.sort_order,
                context_window = EXCLUDED.context_window,
                max_output_tokens = EXCLUDED.max_output_tokens,
                supports_tools = EXCLUDED.supports_tools,
                is_default = EXCLUDED.is_default,
                updated_at = datetime('now')
            "#,
            id,
            model_id,
            display_name,
            provider,
            model.enabled,
            model.sort_order,
            model.context_window,
            model.max_output_tokens,
            model.supports_tools,
            model.is_default
        )
        .execute(db.pool())
        .await?;
    }

    info!("✅ Seeded {} system default models from config", count);
    Ok(count)
}

/// Seed system default agents (user_id = NULL)
/// Loads agent configurations from config/seeds/agents.json
pub async fn seed_default_agents(db: &Database) -> Result<usize> {
    let agents = config_loader::load_agents()?;

    let count = agents.len();
    for agent in &agents {
        // SQLite requires storing string values in variables
        let agent_id = agent.agent_id.clone();
        let name = agent.name.clone();
        let description = agent.description.clone();
        let color = agent.color.clone();
        let icon = agent.icon.clone();

        let id = Uuid::new_v4().to_string();
        sqlx::query!(
            r#"
            INSERT INTO app_agents (id, user_id, agent_id, name, description, color, icon, enabled, sort_order)
            VALUES ($1, NULL, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (agent_id) WHERE user_id IS NULL DO UPDATE SET
                name = EXCLUDED.name,
                description = EXCLUDED.description,
                color = EXCLUDED.color,
                icon = EXCLUDED.icon,
                enabled = EXCLUDED.enabled,
                sort_order = EXCLUDED.sort_order,
                updated_at = datetime('now')
            "#,
            id,
            agent_id,
            name,
            description,
            color,
            icon,
            agent.enabled,
            agent.sort_order
        )
        .execute(db.pool())
        .await?;
    }

    info!("✅ Seeded {} system default agents from config", count);
    Ok(count)
}

/// Seed sample axiology tags via tasks
/// Creates sample tasks with common tags to populate autocomplete
pub async fn seed_axiology_tags(db: &Database) -> Result<usize> {
    let sample_tasks = vec![
        (
            "Work projects",
            Some("Professional and career-related tasks"),
            vec!["work".to_string()],
        ),
        (
            "Family time",
            Some("Spending quality time with loved ones"),
            vec!["family".to_string(), "relational".to_string()],
        ),
        (
            "Exercise routine",
            Some("Physical fitness and wellbeing"),
            vec!["health".to_string()],
        ),
        (
            "Personal development",
            Some("Self-improvement and growth"),
            vec!["personal".to_string()],
        ),
        (
            "Spiritual practice",
            Some("Meditation, reflection, and inner work"),
            vec!["spiritual".to_string()],
        ),
        (
            "New experiences",
            Some("Trying new things and adventures"),
            vec!["experiential".to_string()],
        ),
        (
            "Building relationships",
            Some("Deepening connections with others"),
            vec!["relational".to_string()],
        ),
    ];

    let count = sample_tasks.len();
    for (title, description, tags) in &sample_tasks {
        // SQLite requires JSON serialization for arrays
        let tags_json = serde_json::to_string(tags).unwrap_or_else(|_| "[]".to_string());
        let desc = description.as_deref().map(|s| s.to_string());
        let id = Uuid::new_v4().to_string();

        sqlx::query!(
            r#"
            INSERT INTO data_praxis_task (id, title, description, tags)
            VALUES ($1, $2, $3, $4)
            "#,
            id,
            title,
            desc,
            tags_json
        )
        .execute(db.pool())
        .await?;
    }

    info!("✅ Seeded {} sample action tasks with tags", count);
    Ok(count)
}

/// Seed default tools metadata
/// Loads tool configurations from config/seeds/tools.json
pub async fn seed_default_tools(db: &Database) -> Result<usize> {
    let (tools, _defaults) = config_loader::load_tools()?;

    let count = tools.len();
    for tool in &tools {
        // SQLite requires storing string values in variables
        let id = tool.id.clone();
        let name = tool.name.clone();
        let description = tool.description.clone();
        let tool_type = tool.tool_type.clone();
        let category = tool.category.clone();
        let icon = tool.icon.clone();

        sqlx::query!(
            r#"
            INSERT INTO app_tools (id, name, description, tool_type, category, icon, display_order)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (id) DO UPDATE SET
                name = EXCLUDED.name,
                description = EXCLUDED.description,
                tool_type = EXCLUDED.tool_type,
                category = EXCLUDED.category,
                icon = EXCLUDED.icon,
                display_order = EXCLUDED.display_order,
                updated_at = datetime('now')
            "#,
            id,
            name,
            description,
            tool_type,
            category,
            icon,
            tool.display_order
        )
        .execute(db.pool())
        .await?;
    }

    info!("✅ Seeded {} default tools from config", count);
    Ok(count)
}

/// Initialize enabled_tools with explicit default values
/// DEPRECATED: Use seed_assistant_profile instead
// Commented out - function is deprecated and never called, causes SQLX cache issues
/*
pub async fn seed_enabled_tools(db: &Database) -> Result<()> {
    sqlx::query!(
        r#"
        UPDATE app_assistant_profile
        SET enabled_tools = '{"query_location_map": true, "web_search": true}'::jsonb
        WHERE enabled_tools = '{}'::jsonb OR enabled_tools IS NULL
        "#
    )
    .execute(db.pool())
    .await?;

    info!("✅ Initialized enabled_tools with explicit defaults");
    Ok(())
}
*/

/// Seed assistant profile defaults
/// Loads configuration from config/seeds/assistant_profile.json
/// Updates the singleton assistant profile row with defaults if not already set
pub async fn seed_assistant_profile(db: &Database) -> Result<()> {
    let defaults = config_loader::load_assistant_profile_defaults()?;

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

    info!("✅ Seeded assistant profile defaults from config");
    Ok(())
}

/// Seed all production defaults
pub async fn seed_production_data(db: &Database) -> Result<()> {
    info!("🌱 Seeding production defaults...");

    seed_default_sources(db).await?;
    seed_default_streams(db).await?;
    seed_default_models(db).await?;
    seed_default_agents(db).await?;
    seed_default_tools(db).await?;
    seed_assistant_profile(db).await?;
    seed_axiology_tags(db).await?;

    info!("✅ Production seeding completed successfully");
    Ok(())
}
