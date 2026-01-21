//! Ontology domain primitives seeding
//!
//! Generates realistic test data for health, social, location, and activity domains

use crate::database::Database;
use crate::Result;
use chrono::{Duration, Utc};
use fake::faker::internet::en::SafeEmail;
use fake::faker::name::en::Name;
use fake::faker::phone_number::en::PhoneNumber;
use fake::Fake;
use rand::Rng;
use sqlx::SqlitePool;
use uuid::Uuid;

/// Seeds ~1000 ontology domain primitive records
pub async fn seed_ontologies(db: &Database) -> Result<usize> {
    let pool = db.pool();
    let mut total_count = 0;

    // Use a deterministic seed source UUID (virtues-app source from migrations)
    let virtues_source_id =
        Uuid::parse_str("00000000-0000-0000-0000-000000000001").expect("Valid UUID");

    // Get or create a test stream for seed data
    let seed_stream_id = get_or_create_seed_stream(pool, virtues_source_id).await?;

    // Seed health data (~400 records)
    total_count += seed_health_data(pool, seed_stream_id).await?;

    // Seed social data (~300 records)
    total_count += seed_social_data(pool, seed_stream_id).await?;

    // Seed location data (~200 records)
    total_count += seed_location_data(pool, seed_stream_id).await?;

    // Seed activity data (~100 records)
    total_count += seed_activity_data(pool, seed_stream_id).await?;

    Ok(total_count)
}

/// Get or create a stream for seed data
async fn get_or_create_seed_stream(pool: &SqlitePool, source_id: Uuid) -> Result<Uuid> {
    // SQLite requires string conversion for UUIDs
    let source_id_str = source_id.to_string();

    // Ensure source exists first
    sqlx::query!(
        r#"
        INSERT INTO data_source_connections (id, source, name, auth_type, is_active)
        VALUES ($1, 'seed_source', 'Seed Data Source', 'none', true)
        ON CONFLICT (id) DO NOTHING
        "#,
        source_id_str
    )
    .execute(pool)
    .await?;

    let stream_id = Uuid::new_v4();
    let stream_id_str = stream_id.to_string();
    sqlx::query!(
        r#"
        INSERT INTO data_stream_connections (id, source_connection_id, stream_name, table_name, is_enabled)
        VALUES ($1, $2, 'seed_data', 'stream_seed_data', true)
        ON CONFLICT (source_connection_id, stream_name)
        DO UPDATE SET updated_at = datetime('now')
        "#,
        stream_id_str,
        source_id_str
    )
    .execute(pool)
    .await?;

    Ok(stream_id)
}

/// Seeds health domain primitives
async fn seed_health_data(pool: &SqlitePool, stream_id: Uuid) -> Result<usize> {
    let mut count = 0;
    let now = Utc::now();
    let mut rng = rand::rng();
    // SQLite requires string conversion for UUIDs
    let stream_id_str = stream_id.to_string();

    // Generate 30 days of health data
    for day in 0..30 {
        let base_time = now - Duration::days(day);

        // Heart rate measurements (every 15 minutes during waking hours = ~64 per day, ~100 total for variety)
        for hour in 7..23 {
            for minute in (0..60).step_by(15) {
                let timestamp = base_time
                    .date_naive()
                    .and_hms_opt(hour, minute, 0)
                    .unwrap()
                    .and_utc();
                let timestamp_str = timestamp.to_rfc3339();

                let bpm: i32 = match hour {
                    7..=9 => rng.random_range(65..85),   // Morning
                    10..=12 => rng.random_range(70..90), // Mid-morning
                    13..=17 => rng.random_range(75..95), // Afternoon
                    18..=22 => rng.random_range(65..80), // Evening
                    _ => rng.random_range(60..75),
                };

                sqlx::query!(
                    r#"
                    INSERT INTO data_health_heart_rate
                    (bpm, timestamp, source_stream_id, source_table, source_provider)
                    VALUES ($1, $2, $3, 'stream_seed_data', 'seed')
                    ON CONFLICT DO NOTHING
                    "#,
                    bpm,
                    timestamp_str,
                    stream_id_str
                )
                .execute(pool)
                .await?;

                count += 1;
            }
        }

        // Sleep records (1 per day = 30 total)
        let sleep_start = base_time
            .date_naive()
            .and_hms_opt(23, 0, 0)
            .unwrap()
            .and_utc();
        let sleep_duration: i64 = rng.random_range(360..540); // 6-9 hours
        let sleep_end = sleep_start + Duration::minutes(sleep_duration);
        let sleep_start_str = sleep_start.to_rfc3339();
        let sleep_end_str = sleep_end.to_rfc3339();
        let sleep_duration_i32 = sleep_duration as i32;
        let sleep_quality: f64 = rng.random_range(0.6..1.0);

        sqlx::query!(
            r#"
            INSERT INTO data_health_sleep
            (duration_minutes, sleep_quality_score, start_time, end_time,
             source_stream_id, source_table, source_provider)
            VALUES ($1, $2, $3, $4, $5, 'stream_seed_data', 'seed')
            ON CONFLICT DO NOTHING
            "#,
            sleep_duration_i32,
            sleep_quality,
            sleep_start_str,
            sleep_end_str,
            stream_id_str
        )
        .execute(pool)
        .await?;

        count += 1;

        // Workout records (every 2-3 days = ~12 total)
        if day % 2 == 0 || day % 3 == 0 {
            let workout_start = base_time
                .date_naive()
                .and_hms_opt(18, 0, 0)
                .unwrap()
                .and_utc();
            let workout_duration: i64 = rng.random_range(30..90);
            let workout_end = workout_start + Duration::minutes(workout_duration);
            let workout_start_str = workout_start.to_rfc3339();
            let workout_end_str = workout_end.to_rfc3339();

            let workout_types = vec!["running", "cycling", "swimming", "weightlifting", "yoga"];
            let workout_type = workout_types[rng.random_range(0..workout_types.len())];
            let calories: i32 = rng.random_range(200..800);
            let avg_hr: i32 = rng.random_range(120..170);

            sqlx::query!(
                r#"
                INSERT INTO data_health_workout
                (workout_type, calories_burned, avg_heart_rate,
                 start_time, end_time, source_stream_id, source_table, source_provider)
                VALUES ($1, $2, $3, $4, $5, $6, 'stream_seed_data', 'seed')
                ON CONFLICT DO NOTHING
                "#,
                workout_type,
                calories,
                avg_hr,
                workout_start_str,
                workout_end_str,
                stream_id_str
            )
            .execute(pool)
            .await?;

            count += 1;
        }

        // Steps (1 per day = 30 total)
        let steps_time = base_time
            .date_naive()
            .and_hms_opt(23, 59, 0)
            .unwrap()
            .and_utc();
        let steps_time_str = steps_time.to_rfc3339();
        let step_count: i32 = rng.random_range(4000..15000);

        sqlx::query!(
            r#"
            INSERT INTO data_health_steps
            (step_count, timestamp, source_stream_id, source_table, source_provider)
            VALUES ($1, $2, $3, 'stream_seed_data', 'seed')
            ON CONFLICT DO NOTHING
            "#,
            step_count,
            steps_time_str,
            stream_id_str
        )
        .execute(pool)
        .await?;

        count += 1;
    }

    Ok(count)
}

/// Seeds social domain primitives
async fn seed_social_data(pool: &SqlitePool, stream_id: Uuid) -> Result<usize> {
    let mut count = 0;
    let now = Utc::now();
    let mut rng = rand::rng();
    // SQLite requires string conversion for UUIDs
    let stream_id_str = stream_id.to_string();

    // Generate 30 days of social data
    for day in 0..30 {
        let base_time = now - Duration::days(day);

        // Email messages (5-10 per day = ~225 total)
        let email_count = rng.random_range(5..11);
        for i in 0..email_count {
            let hour: u32 = rng.random_range(8..20);
            let minute: u32 = rng.random_range(0..60);
            let timestamp = base_time
                .date_naive()
                .and_hms_opt(hour, minute, 0)
                .unwrap()
                .and_utc();
            let timestamp_str = timestamp.to_rfc3339();

            let subjects = vec![
                "Weekly sync",
                "Project update",
                "Quick question",
                "Follow up",
                "Meeting notes",
                "FYI",
                "Action items",
            ];

            let directions = vec!["sent", "received"];
            let message_id = format!("seed-email-{}-{}", day, i);
            let subject = subjects[rng.random_range(0..subjects.len())];
            let from_email: String = SafeEmail().fake();
            let from_name: String = Name().fake();
            let to_email: String = SafeEmail().fake();
            let to_emails_json =
                serde_json::to_string(&vec![to_email]).unwrap_or_else(|_| "[]".to_string());
            let direction = directions[rng.random_range(0..directions.len())];

            sqlx::query!(
                r#"
                INSERT INTO data_social_email
                (message_id, subject, from_email, from_name, to_emails, direction, timestamp,
                 source_stream_id, source_table, source_provider)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'stream_seed_data', 'seed')
                ON CONFLICT DO NOTHING
                "#,
                message_id,
                subject,
                from_email,
                from_name,
                to_emails_json,
                direction,
                timestamp_str,
                stream_id_str
            )
            .execute(pool)
            .await?;

            count += 1;
        }

        // Messages (10-20 per day = ~450 total)
        let message_count = rng.random_range(10..21);
        for i in 0..message_count {
            let hour: u32 = rng.random_range(8..23);
            let minute: u32 = rng.random_range(0..60);
            let timestamp = base_time
                .date_naive()
                .and_hms_opt(hour, minute, 0)
                .unwrap()
                .and_utc();
            let timestamp_str = timestamp.to_rfc3339();

            let platforms = vec!["sms", "imessage", "whatsapp", "slack"];
            let message_id = format!("seed-msg-{}-{}", day, i);
            let platform = platforms[rng.random_range(0..platforms.len())];
            let from_identifier: String = PhoneNumber().fake();

            sqlx::query!(
                r#"
                INSERT INTO data_social_message
                (message_id, platform, content, from_identifier, timestamp,
                 source_stream_id, source_table, source_provider)
                VALUES ($1, $2, $3, $4, $5, $6, 'stream_seed_data', 'seed')
                ON CONFLICT DO NOTHING
                "#,
                message_id,
                platform,
                "Sample message content",
                from_identifier,
                timestamp_str,
                stream_id_str
            )
            .execute(pool)
            .await?;

            count += 1;
        }
    }

    Ok(count)
}

/// Seeds location domain primitives
async fn seed_location_data(pool: &SqlitePool, stream_id: Uuid) -> Result<usize> {
    let mut count = 0;
    let now = Utc::now();
    let mut rng = rand::rng();
    // SQLite requires string conversion for UUIDs
    let stream_id_str = stream_id.to_string();

    // Base coordinates (San Francisco area)
    let base_lat = 37.7749;
    let base_lon = -122.4194;

    // Generate 30 days of location data
    for day in 0..30 {
        let base_time = now - Duration::days(day);

        // Location points (every 30 minutes = ~48 per day, sample 5 to keep it reasonable = 150 total)
        for hour in [9, 12, 15, 18, 21].iter() {
            let timestamp = base_time
                .date_naive()
                .and_hms_opt(*hour, 0, 0)
                .unwrap()
                .and_utc();
            let timestamp_str = timestamp.to_rfc3339();

            // Add some random variation
            let lat: f64 = base_lat + rng.random_range(-0.05..0.05);
            let lon: f64 = base_lon + rng.random_range(-0.05..0.05);
            let accuracy: f64 = rng.random_range(5.0..50.0);

            sqlx::query!(
                r#"
                INSERT INTO data_location_point
                (latitude, longitude, horizontal_accuracy, timestamp,
                 source_stream_id, source_table, source_provider)
                VALUES ($1, $2, $3, $4, $5, 'stream_seed_data', 'seed')
                ON CONFLICT DO NOTHING
                "#,
                lat,
                lon,
                accuracy,
                timestamp_str,
                stream_id_str
            )
            .execute(pool)
            .await?;

            count += 1;
        }
    }

    Ok(count)
}

/// Seeds activity domain primitives
async fn seed_activity_data(pool: &SqlitePool, stream_id: Uuid) -> Result<usize> {
    let mut count = 0;
    let now = Utc::now();
    let mut rng = rand::rng();
    // SQLite requires string conversion for UUIDs
    let stream_id_str = stream_id.to_string();

    // Generate 30 days of activity data
    for day in 0..30 {
        let base_time = now - Duration::days(day);

        // Calendar entries (2-4 per day = ~90 total)
        let cal_count = rng.random_range(2..5);
        for _ in 0..cal_count {
            let hour: u32 = rng.random_range(9..17);
            let start_time = base_time
                .date_naive()
                .and_hms_opt(hour, 0, 0)
                .unwrap()
                .and_utc();
            let duration: i64 = rng.random_range(30..120); // 30 mins to 2 hours
            let end_time = start_time + Duration::minutes(duration);
            let start_time_str = start_time.to_rfc3339();
            let end_time_str = end_time.to_rfc3339();

            let titles = vec![
                "Team standup",
                "1:1 meeting",
                "Project review",
                "Client call",
                "Planning session",
            ];
            let title = titles[rng.random_range(0..titles.len())];

            sqlx::query!(
                r#"
                INSERT INTO data_praxis_calendar
                (title, start_time, end_time, is_all_day,
                 source_stream_id, source_table, source_provider)
                VALUES ($1, $2, $3, $4, $5, 'stream_seed_data', 'seed')
                ON CONFLICT DO NOTHING
                "#,
                title,
                start_time_str,
                end_time_str,
                false,
                stream_id_str
            )
            .execute(pool)
            .await?;

            count += 1;
        }

        // App usage (5-10 per day = ~225 total)
        let app_count = rng.random_range(5..11);
        for _ in 0..app_count {
            let hour: u32 = rng.random_range(8..22);
            let minute: u32 = rng.random_range(0..60);
            let start_time = base_time
                .date_naive()
                .and_hms_opt(hour, minute, 0)
                .unwrap()
                .and_utc();
            let duration: i64 = rng.random_range(5..120); // 5 mins to 2 hours
            let end_time = start_time + Duration::minutes(duration);
            let start_time_str = start_time.to_rfc3339();
            let end_time_str = end_time.to_rfc3339();

            let apps = vec![
                ("VS Code", "code"),
                ("Chrome", "browsing"),
                ("Slack", "communication"),
                ("Terminal", "development"),
                ("Notion", "productivity"),
            ];
            let (app_name, app_category) = apps[rng.random_range(0..apps.len())];

            sqlx::query!(
                r#"
                INSERT INTO data_activity_app_usage
                (app_name, app_category, start_time, end_time,
                 source_stream_id, source_table, source_provider)
                VALUES ($1, $2, $3, $4, $5, 'stream_seed_data', 'seed')
                ON CONFLICT DO NOTHING
                "#,
                app_name,
                app_category,
                start_time_str,
                end_time_str,
                stream_id_str
            )
            .execute(pool)
            .await?;

            count += 1;
        }
    }

    Ok(count)
}
