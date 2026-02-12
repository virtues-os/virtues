//! iOS Contacts to wiki_people Entity Resolution
//!
//! Transforms iOS contacts into canonical wiki_people entities.
//!
//! ## Process
//!
//! 1. Fetch contacts from stream_ios_contacts in time window
//! 2. For each contact: match by email (primary) or phone (fallback)
//! 3. Create new person entities for unknowns
//! 4. Merge contact data into existing person entities

use async_trait::async_trait;

use crate::database::Database;
use crate::error::Result;
use crate::ids;
use crate::jobs::TransformContext;
use crate::sources::base::{OntologyTransform, TransformResult};

/// Transform iOS Contacts to wiki_people entities
pub struct IosContactsTransform;

#[async_trait]
impl OntologyTransform for IosContactsTransform {
    fn source_table(&self) -> &str {
        "stream_ios_contacts"
    }

    fn target_table(&self) -> &str {
        "wiki_people"
    }

    fn domain(&self) -> &str {
        "wiki_people"
    }

    #[tracing::instrument(skip(self, db, context), fields(source_table = %self.source_table(), target_table = %self.target_table()))]
    async fn transform(
        &self,
        db: &Database,
        context: &TransformContext,
        source_id: String,
    ) -> Result<TransformResult> {
        let mut records_read = 0;
        let mut records_written = 0;
        let mut last_processed_id: Option<String> = None;

        let data_source = context.get_data_source().ok_or_else(|| {
            crate::Error::Other("No data source available for transform".to_string())
        })?;

        let checkpoint_key = "ios_contacts_to_wiki_people";
        let batches = data_source
            .read_with_checkpoint(&source_id, "contacts", checkpoint_key)
            .await?;

        for batch in batches {
            for record in &batch.records {
                records_read += 1;

                // Parse contact record
                let contact = match parse_contact_record(record) {
                    Some(c) => c,
                    None => continue,
                };

                // Skip contacts with no name
                if contact.given_name.is_empty() && contact.family_name.is_empty() {
                    continue;
                }

                // Resolve or create person entity
                match resolve_or_create_person_from_contact(db, &contact).await {
                    Ok(person_id) => {
                        records_written += 1;
                        last_processed_id = Some(person_id);
                    }
                    Err(e) => {
                        tracing::warn!(
                            contact_id = %contact.identifier,
                            error = %e,
                            "Failed to resolve person from contact"
                        );
                    }
                }
            }

            // Update checkpoint after each batch
            if let Some(max_ts) = batch.max_timestamp {
                data_source
                    .update_checkpoint(&source_id, "contacts", checkpoint_key, max_ts)
                    .await?;
            }
        }

        tracing::info!(
            records_read,
            records_written,
            "Contacts to wiki_people transform completed"
        );

        Ok(TransformResult {
            records_read,
            records_written,
            records_failed: 0,
            last_processed_id,
            chained_transforms: vec![],
        })
    }
}

/// Parsed contact record from iOS
#[derive(Debug)]
struct ContactRecord {
    identifier: String,
    given_name: String,
    family_name: String,
    organization_name: Option<String>,
    phones: Vec<String>,
    emails: Vec<String>,
    birthday: Option<String>,
}

/// Parse a contact record from JSON
fn parse_contact_record(record: &serde_json::Value) -> Option<ContactRecord> {
    let identifier = record.get("identifier")?.as_str()?.to_string();
    let given_name = record
        .get("givenName")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let family_name = record
        .get("familyName")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let organization_name = record
        .get("organizationName")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // Parse phones array
    let phones: Vec<String> = record
        .get("phones")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|p| p.get("number").and_then(|n| n.as_str()).map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    // Parse emails array
    let emails: Vec<String> = record
        .get("emails")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|e| {
                    e.get("address")
                        .and_then(|a| a.as_str())
                        .map(|s| s.to_lowercase())
                })
                .collect()
        })
        .unwrap_or_default();

    // Parse birthday (ISO8601 string)
    let birthday = record
        .get("birthday")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    Some(ContactRecord {
        identifier,
        given_name,
        family_name,
        organization_name,
        phones,
        emails,
        birthday,
    })
}

/// Resolve a contact to an existing person entity or create a new one
async fn resolve_or_create_person_from_contact(
    db: &Database,
    contact: &ContactRecord,
) -> Result<String> {
    // Try to find existing person by email (primary match)
    for email in &contact.emails {
        if let Some(person_id) = find_person_by_email(db, email).await? {
            // Found existing person - merge contact data
            merge_contact_into_person(db, &person_id, contact).await?;
            return Ok(person_id);
        }
    }

    // Try to find by phone (fallback match)
    for phone in &contact.phones {
        let normalized = normalize_phone(phone);
        if let Some(person_id) = find_person_by_phone(db, &normalized).await? {
            // Found existing person - merge contact data
            merge_contact_into_person(db, &person_id, contact).await?;
            return Ok(person_id);
        }
    }

    // No match found - create new person entity
    create_person_from_contact(db, contact).await
}

/// Find person by email in wiki_people
async fn find_person_by_email(db: &Database, email: &str) -> Result<Option<String>> {
    let row = sqlx::query!(
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

    Ok(row.and_then(|r| r.id))
}

/// Find person by phone in wiki_people
async fn find_person_by_phone(db: &Database, phone: &str) -> Result<Option<String>> {
    // SQLite doesn't support complex JSON queries easily, so we search for substring match
    // This isn't ideal but works for most cases
    let pattern = format!("%{}%", phone);
    let row = sqlx::query!(
        r#"
        SELECT id
        FROM wiki_people
        WHERE phones LIKE $1
        LIMIT 1
        "#,
        pattern
    )
    .fetch_optional(db.pool())
    .await?;

    Ok(row.and_then(|r| r.id))
}

/// Normalize phone number (remove non-digits except leading +)
fn normalize_phone(phone: &str) -> String {
    let trimmed = phone.trim();
    if trimmed.starts_with('+') {
        format!(
            "+{}",
            trimmed[1..].chars().filter(|c| c.is_ascii_digit()).collect::<String>()
        )
    } else {
        trimmed.chars().filter(|c| c.is_ascii_digit()).collect()
    }
}

/// Merge contact data into existing person entity
async fn merge_contact_into_person(
    db: &Database,
    person_id: &str,
    contact: &ContactRecord,
) -> Result<()> {
    // Fetch existing person data
    let row = sqlx::query!(
        r#"
        SELECT emails, phones, birthday, metadata
        FROM wiki_people
        WHERE id = $1
        "#,
        person_id
    )
    .fetch_one(db.pool())
    .await?;

    // Merge emails
    let mut existing_emails: Vec<String> = row
        .emails
        .as_ref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default();

    for email in &contact.emails {
        if !existing_emails.contains(email) {
            existing_emails.push(email.clone());
        }
    }
    let emails_json = serde_json::to_string(&existing_emails)?;

    // Merge phones
    let mut existing_phones: Vec<String> = row
        .phones
        .as_ref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default();

    for phone in &contact.phones {
        let normalized = normalize_phone(phone);
        if !existing_phones.iter().any(|p| normalize_phone(p) == normalized) {
            existing_phones.push(phone.clone());
        }
    }
    let phones_json = serde_json::to_string(&existing_phones)?;

    // Update birthday if not set
    let birthday = if row.birthday.is_none() {
        contact.birthday.clone()
    } else {
        row.birthday.clone()
    };

    // Update metadata to track iOS contact source
    let mut metadata: serde_json::Value = row
        .metadata
        .as_ref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_else(|| serde_json::json!({}));

    if let Some(obj) = metadata.as_object_mut() {
        obj.insert(
            "ios_contact_id".to_string(),
            serde_json::json!(contact.identifier),
        );
        if let Some(org) = &contact.organization_name {
            obj.insert("organization".to_string(), serde_json::json!(org));
        }
    }
    let metadata_json = serde_json::to_string(&metadata)?;

    // Update person record
    sqlx::query!(
        r#"
        UPDATE wiki_people
        SET emails = $1,
            phones = $2,
            birthday = COALESCE($3, birthday),
            metadata = $4,
            updated_at = datetime('now')
        WHERE id = $5
        "#,
        emails_json,
        phones_json,
        birthday,
        metadata_json,
        person_id
    )
    .execute(db.pool())
    .await?;

    tracing::debug!(
        person_id = %person_id,
        contact_id = %contact.identifier,
        "Merged contact data into existing person"
    );

    Ok(())
}

/// Create a new person entity from contact
async fn create_person_from_contact(db: &Database, contact: &ContactRecord) -> Result<String> {
    // Build canonical name
    let canonical_name = if !contact.given_name.is_empty() && !contact.family_name.is_empty() {
        format!("{} {}", contact.given_name, contact.family_name)
    } else if !contact.given_name.is_empty() {
        contact.given_name.clone()
    } else {
        contact.family_name.clone()
    };

    // Generate ID - use first email if available, otherwise use contact identifier
    let id_seed = contact
        .emails
        .first()
        .map(|e| e.as_str())
        .unwrap_or(&contact.identifier);
    let person_id = ids::generate_id(ids::WIKI_PERSON_PREFIX, &[id_seed]);

    // Serialize arrays
    let emails_json = serde_json::to_string(&contact.emails)?;
    let phones_json = serde_json::to_string(&contact.phones)?;

    // Build metadata
    let metadata = serde_json::json!({
        "ios_contact_id": contact.identifier,
        "source": "ios_contacts",
        "organization": contact.organization_name,
    });
    let metadata_json = serde_json::to_string(&metadata)?;

    // Insert new person
    sqlx::query!(
        r#"
        INSERT INTO wiki_people (
            id,
            canonical_name,
            emails,
            phones,
            birthday,
            metadata
        ) VALUES ($1, $2, $3, $4, $5, $6)
        ON CONFLICT (id) DO UPDATE SET
            emails = EXCLUDED.emails,
            phones = EXCLUDED.phones,
            birthday = COALESCE(EXCLUDED.birthday, wiki_people.birthday),
            metadata = EXCLUDED.metadata,
            updated_at = datetime('now')
        "#,
        person_id,
        canonical_name,
        emails_json,
        phones_json,
        contact.birthday,
        metadata_json
    )
    .execute(db.pool())
    .await?;

    tracing::info!(
        person_id = %person_id,
        canonical_name = %canonical_name,
        contact_id = %contact.identifier,
        "Created new person from iOS contact"
    );

    Ok(person_id)
}

// Self-registration
struct IosContactsRegistration;
impl crate::sources::base::TransformRegistration for IosContactsRegistration {
    fn source_table(&self) -> &'static str {
        "stream_ios_contacts"
    }
    fn target_table(&self) -> &'static str {
        "wiki_people"
    }
    fn create(&self, _context: &TransformContext) -> Result<Box<dyn OntologyTransform>> {
        Ok(Box::new(IosContactsTransform))
    }
}
inventory::submit! { &IosContactsRegistration as &dyn crate::sources::base::TransformRegistration }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_phone() {
        assert_eq!(normalize_phone("+1 (555) 123-4567"), "+15551234567");
        assert_eq!(normalize_phone("555-123-4567"), "5551234567");
        assert_eq!(normalize_phone("  +44 20 7946 0958  "), "+442079460958");
    }

    #[test]
    fn test_parse_contact_record() {
        let record = serde_json::json!({
            "identifier": "abc123",
            "givenName": "John",
            "familyName": "Doe",
            "organizationName": "Acme Corp",
            "phones": [
                {"label": "mobile", "number": "+1-555-123-4567"},
                {"label": "home", "number": "555-987-6543"}
            ],
            "emails": [
                {"label": "work", "address": "John.Doe@acme.com"},
                {"label": "personal", "address": "johnd@gmail.com"}
            ],
            "birthday": "1990-05-15"
        });

        let contact = parse_contact_record(&record).unwrap();
        assert_eq!(contact.identifier, "abc123");
        assert_eq!(contact.given_name, "John");
        assert_eq!(contact.family_name, "Doe");
        assert_eq!(contact.organization_name, Some("Acme Corp".to_string()));
        assert_eq!(contact.phones.len(), 2);
        assert_eq!(contact.emails.len(), 2);
        assert_eq!(contact.emails[0], "john.doe@acme.com"); // lowercase
        assert_eq!(contact.birthday, Some("1990-05-15".to_string()));
    }
}
