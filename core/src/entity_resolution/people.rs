//! People Resolution
//!
//! Resolves people from multiple sources to canonical wiki_people entities.
//!
//! ## Sources
//!
//! 1. **Calendar Attendees** - Event attendee emails → wiki_people
//! 2. **Email Senders** - From email addresses → wiki_people
//!
//! ## Process
//!
//! 1. Fetch records in time window (calendar events, emails)
//! 2. Extract emails from records
//! 3. Match against wiki_people by email
//! 4. Create new person entities for unknowns
//! 5. Update source records with resolved person IDs

use uuid::Uuid;

use super::TimeWindow;
use crate::database::Database;
use crate::error::{Error, Result};
use crate::ids;

/// Resolve people from all sources in time window
///
/// Returns the total number of people resolved.
pub async fn resolve_people(db: &Database, window: TimeWindow) -> Result<usize> {
    tracing::info!(
        start = %window.start,
        end = %window.end,
        "Resolving people from all sources"
    );

    let mut total_resolved = 0;

    // 1. Resolve from calendar attendees
    total_resolved += resolve_calendar_attendees(db, window).await?;

    // 2. Resolve from email senders
    total_resolved += resolve_email_senders(db, window).await?;

    tracing::info!(
        people_resolved = total_resolved,
        "People resolution completed"
    );

    Ok(total_resolved)
}

/// Resolve people from calendar attendees in time window
async fn resolve_calendar_attendees(db: &Database, window: TimeWindow) -> Result<usize> {
    let calendar_events = fetch_calendar_events(db, window).await?;

    if calendar_events.is_empty() {
        tracing::debug!("No calendar events to process");
        return Ok(0);
    }

    tracing::debug!(
        event_count = calendar_events.len(),
        "Fetched calendar events for people resolution"
    );

    let mut total_people_resolved = 0;
    for event in calendar_events {
        match resolve_and_link_event_attendees(db, &event).await {
            Ok(count) => total_people_resolved += count,
            Err(e) => {
                tracing::warn!(
                    event_id = %event.id,
                    error = %e,
                    "Failed to resolve attendees for event"
                );
            }
        }
    }

    tracing::debug!(
        people_resolved = total_people_resolved,
        "Calendar attendee resolution completed"
    );

    Ok(total_people_resolved)
}

/// Resolve people from email senders in time window
///
/// Links from_email to from_person_id in data_communication_email.
async fn resolve_email_senders(db: &Database, window: TimeWindow) -> Result<usize> {
    // Fetch emails without resolved from_person_id
    let emails = fetch_unresolved_emails(db, window).await?;

    if emails.is_empty() {
        tracing::debug!("No emails to process for sender resolution");
        return Ok(0);
    }

    tracing::debug!(
        email_count = emails.len(),
        "Fetched emails for sender resolution"
    );

    let mut total_resolved = 0;
    for email_record in emails {
        match resolve_and_link_email_sender(db, &email_record).await {
            Ok(true) => total_resolved += 1,
            Ok(false) => {}
            Err(e) => {
                tracing::warn!(
                    email_id = %email_record.id,
                    from_email = %email_record.from_email,
                    error = %e,
                    "Failed to resolve sender for email"
                );
            }
        }
    }

    tracing::debug!(
        people_resolved = total_resolved,
        "Email sender resolution completed"
    );

    Ok(total_resolved)
}

/// Email record for sender resolution
#[derive(Debug)]
struct EmailRecord {
    id: String,
    from_email: String,
    from_name: Option<String>,
}

/// Fetch emails without resolved from_person_id
async fn fetch_unresolved_emails(db: &Database, window: TimeWindow) -> Result<Vec<EmailRecord>> {
    let rows = sqlx::query!(
        r#"
        SELECT
            id,
            from_email,
            from_name
        FROM data_communication_email
        WHERE timestamp >= $1
          AND timestamp < $2
          AND from_person_id IS NULL
          AND from_email IS NOT NULL
          AND from_email != ''
        ORDER BY timestamp ASC
        LIMIT 1000
        "#,
        window.start,
        window.end
    )
    .fetch_all(db.pool())
    .await?;

    let emails = rows
        .into_iter()
        .filter_map(|row| {
            Some(EmailRecord {
                id: row.id?,
                from_email: row.from_email,
                from_name: row.from_name,
            })
        })
        .collect();

    Ok(emails)
}

/// Resolve email sender and link to person entity
///
/// Returns true if a new person was created or linked.
async fn resolve_and_link_email_sender(db: &Database, email_record: &EmailRecord) -> Result<bool> {
    let email_lower = email_record.from_email.to_lowercase();

    // Resolve or create person
    let person_id = resolve_or_create_person_with_name(
        db,
        &email_lower,
        email_record.from_name.as_deref(),
    )
    .await?;

    // Update email with resolved person ID
    sqlx::query!(
        r#"
        UPDATE data_communication_email
        SET from_person_id = $1,
            updated_at = datetime('now')
        WHERE id = $2
        "#,
        person_id,
        email_record.id
    )
    .execute(db.pool())
    .await?;

    tracing::debug!(
        email_id = %email_record.id,
        from_email = %email_record.from_email,
        person_id = %person_id,
        "Linked email sender to person"
    );

    Ok(true)
}

/// Resolve email to person entity (or create if new), with optional display name
///
/// If the person already exists, updates the canonical name if a better name is provided.
async fn resolve_or_create_person_with_name(
    db: &Database,
    email: &str,
    display_name: Option<&str>,
) -> Result<String> {
    // Check if person exists with this email
    let existing = sqlx::query!(
        r#"
        SELECT id, canonical_name
        FROM wiki_people
        WHERE EXISTS (
            SELECT 1 FROM json_each(emails) WHERE value = $1
        )
        LIMIT 1
        "#,
        email
    )
    .fetch_optional(db.pool())
    .await?;

    if let Some(row) = existing {
        let person_id = row
            .id
            .ok_or_else(|| Error::Database("Missing person ID".to_string()))?;

        // Update canonical name if we have a better one (from email header vs extracted from email)
        if let Some(name) = display_name {
            let current_name = row.canonical_name;
            // Only update if current name looks like it was extracted from email (no spaces, or matches email pattern)
            let name_trimmed = name.trim();
            if !name_trimmed.is_empty()
                && !current_name.contains(' ')
                && name_trimmed.contains(' ')
            {
                sqlx::query!(
                    r#"
                    UPDATE wiki_people
                    SET canonical_name = $1,
                        updated_at = datetime('now')
                    WHERE id = $2
                    "#,
                    name_trimmed,
                    person_id
                )
                .execute(db.pool())
                .await?;

                tracing::debug!(
                    person_id = %person_id,
                    old_name = %current_name,
                    new_name = %name_trimmed,
                    "Updated person canonical name from email header"
                );
            }
        }

        return Ok(person_id);
    }

    // Create new person entity
    let canonical_name = display_name
        .filter(|n| !n.trim().is_empty())
        .map(|n| n.trim().to_string())
        .unwrap_or_else(|| extract_name_from_email(email));

    let emails_json =
        serde_json::to_string(&vec![email.to_string()]).unwrap_or_else(|_| "[]".to_string());

    let person_id = ids::generate_id(ids::WIKI_PERSON_PREFIX, &[email]);

    sqlx::query!(
        r#"
        INSERT INTO wiki_people (
            id,
            canonical_name,
            emails
        ) VALUES ($1, $2, $3)
        ON CONFLICT (id) DO NOTHING
        RETURNING id
        "#,
        person_id,
        canonical_name,
        emails_json
    )
    .fetch_optional(db.pool())
    .await?;

    tracing::info!(
        email = %email,
        person_id = %person_id,
        canonical_name = %canonical_name,
        source = "email_sender",
        "Created new person entity"
    );

    Ok(person_id)
}

/// Calendar event with attendees
#[derive(Debug)]
struct CalendarEvent {
    id: Uuid,
    attendee_identifiers: Vec<String>,
}

/// Fetch calendar events in time window
async fn fetch_calendar_events(db: &Database, window: TimeWindow) -> Result<Vec<CalendarEvent>> {
    // SQLite uses json_array_length() instead of PostgreSQL's array_length()
    let rows = sqlx::query!(
        r#"
        SELECT
            id,
            attendee_identifiers
        FROM data_calendar_event
        WHERE start_time >= $1
          AND start_time < $2
          AND json_array_length(attendee_identifiers) > 0
        "#,
        window.start,
        window.end
    )
    .fetch_all(db.pool())
    .await?;

    let events = rows
        .into_iter()
        .filter_map(|row| {
            // Parse JSON array into Vec<String>
            let identifiers: Vec<String> = row
                .attendee_identifiers
                .as_ref()
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or_default();
            let id = row.id.as_ref().and_then(|s| Uuid::parse_str(s).ok())?;
            Some(CalendarEvent {
                id,
                attendee_identifiers: identifiers,
            })
        })
        .collect();

    Ok(events)
}

/// Resolve all attendees for an event and update the calendar record
///
/// Returns the number of unique people resolved.
async fn resolve_and_link_event_attendees(db: &Database, event: &CalendarEvent) -> Result<usize> {
    if event.attendee_identifiers.is_empty() {
        return Ok(0);
    }

    // Resolve each attendee email to person entity
    let mut person_ids: Vec<String> = Vec::new();
    let mut unique_people = std::collections::HashSet::new();

    for email in &event.attendee_identifiers {
        let email_lower = email.to_lowercase();

        match resolve_or_create_person(db, &email_lower).await {
            Ok(person_id) => {
                unique_people.insert(person_id.clone());
                person_ids.push(person_id);
            }
            Err(e) => {
                tracing::warn!(
                    email = %email,
                    event_id = %event.id,
                    error = %e,
                    "Failed to resolve person for attendee"
                );
            }
        }
    }

    if person_ids.is_empty() {
        return Ok(0);
    }

    // Update calendar event with resolved person IDs
    // SQLite doesn't support arrays - serialize to JSON
    let person_ids_json =
        serde_json::to_string(&person_ids).unwrap_or_else(|_| "[]".to_string());
    let event_id_str = event.id.to_string();

    sqlx::query!(
        r#"
        UPDATE data_calendar_event
        SET attendee_person_ids = $1,
            updated_at = datetime('now')
        WHERE id = $2
        "#,
        person_ids_json,
        event_id_str
    )
    .execute(db.pool())
    .await?;

    tracing::debug!(
        event_id = %event.id,
        people_count = unique_people.len(),
        "Linked attendees to calendar event"
    );

    Ok(unique_people.len())
}

/// Resolve email to person entity (or create if new)
///
/// Returns the person entity ID (format: person_{hash16}).
async fn resolve_or_create_person(db: &Database, email: &str) -> Result<String> {
    // Check if person exists with this email
    // SQLite uses json_each() to search within JSON arrays instead of PostgreSQL's ANY()
    let existing = sqlx::query!(
        r#"
        SELECT id
        FROM wiki_people
        WHERE EXISTS (
            SELECT 1 FROM json_each(emails) WHERE value = $1
        )
        LIMIT 1
        "#,
        email
    )
    .fetch_optional(db.pool())
    .await?;

    if let Some(row) = existing {
        let id_str = row
            .id
            .ok_or_else(|| Error::Database("Missing person ID".to_string()))?;
        tracing::debug!(
            email = %email,
            person_id = %id_str,
            "Found existing person entity"
        );
        return Ok(id_str);
    }

    // Create new person entity
    let canonical_name = extract_name_from_email(email);

    // SQLite doesn't support arrays - serialize emails to JSON
    let emails_json =
        serde_json::to_string(&vec![email.to_string()]).unwrap_or_else(|_| "[]".to_string());

    // Generate ID with proper prefix (person_{hash16})
    let person_id = ids::generate_id(ids::WIKI_PERSON_PREFIX, &[email]);
    let row = sqlx::query!(
        r#"
        INSERT INTO wiki_people (
            id,
            canonical_name,
            emails
        ) VALUES (
            $1, $2, $3
        )
        RETURNING id
        "#,
        person_id,
        canonical_name,
        emails_json
    )
    .fetch_one(db.pool())
    .await?;

    let person_id_str = row
        .id
        .ok_or_else(|| Error::Database("Missing returned ID".to_string()))?;

    tracing::info!(
        email = %email,
        person_id = %person_id_str,
        canonical_name = %canonical_name,
        "Created new person entity"
    );

    Ok(person_id_str)
}

/// Extract name from email (simple heuristic)
///
/// Examples:
/// - adam.jace@example.com → "Adam Jace"
/// - john.doe@company.co → "John Doe"
/// - user123@domain.com → "user123"
fn extract_name_from_email(email: &str) -> String {
    let local_part = email.split('@').next().unwrap_or(email);

    // Split by dot or underscore
    let parts: Vec<&str> = local_part.split(&['.', '_'][..]).collect();

    if parts.len() > 1 {
        // Capitalize each part
        parts
            .iter()
            .map(|part| {
                let mut chars = part.chars();
                match chars.next() {
                    None => String::new(),
                    Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    } else {
        // Just return the local part as-is
        local_part.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_name_from_email() {
        assert_eq!(
            extract_name_from_email("adam.jace@example.com"),
            "Adam Jace"
        );
        assert_eq!(extract_name_from_email("john_doe@company.co"), "John Doe");
        assert_eq!(extract_name_from_email("user123@domain.com"), "user123");
        assert_eq!(extract_name_from_email("single@test.com"), "single");
    }
}
