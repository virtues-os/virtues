//! iOS Contacts → `wiki_people` entity resolution.
//!
//! Ported from `core/src/sources/ios/contacts/transform.rs`.
//!
//! For each contact:
//! 1. Try to match an existing person by email (primary)
//! 2. Fall back to phone match (normalized)
//! 3. If no match, create a new `wiki_people` entity
//! 4. Merge the contact data into the matched/created person
//!
//! Uses runtime `sqlx::query` (not compile-time `sqlx::query!`) because
//! the actions crate doesn't run a build-time DB connection.

use anyhow::Result;
use chrono::{DateTime, NaiveDate};
use serde_json::Value;
use sqlx::PgPool;
use sqlx::Row;
use virtues_helpers::ids::{generate_id, WIKI_PERSON_PREFIX};

/// iOS sends a contact birthday as an ISO8601 datetime (or a bare `YYYY-MM-DD`);
/// `wiki_people.birthday` is a `DATE`, so reduce to a `NaiveDate`. Binding the raw
/// string failed (TEXT vs DATE) and silently dropped the whole contact row.
fn parse_birthday(s: &str) -> Option<NaiveDate> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.date_naive())
        .ok()
        .or_else(|| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
}

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

pub async fn resolve_contacts(db: &PgPool, records: &[Value]) -> Result<(usize, usize)> {
    let mut resolved = 0;
    let mut failed = 0;

    for record in records {
        let Some(contact) = parse_contact(record) else {
            continue;
        };

        if contact.given_name.is_empty() && contact.family_name.is_empty() {
            continue;
        }

        match resolve_or_create(db, &contact).await {
            Ok(_person_id) => resolved += 1,
            Err(e) => {
                tracing::warn!(
                    contact_id = %contact.identifier,
                    error = %e,
                    "failed to resolve contact"
                );
                failed += 1;
            }
        }
    }

    Ok((resolved, failed))
}

fn parse_contact(record: &Value) -> Option<ContactRecord> {
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
        .map(String::from);

    let phones: Vec<String> = record
        .get("phones")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|p| p.get("number").and_then(|n| n.as_str()).map(String::from))
                .collect()
        })
        .unwrap_or_default();

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

    let birthday = record
        .get("birthday")
        .and_then(|v| v.as_str())
        .map(String::from);

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

async fn resolve_or_create(db: &PgPool, contact: &ContactRecord) -> Result<String> {
    // Try to match by email (primary)
    for email in &contact.emails {
        if let Some(person_id) = find_by_email(db, email).await? {
            merge_into_person(db, &person_id, contact).await?;
            return Ok(person_id);
        }
    }

    // Fall back to phone match
    for phone in &contact.phones {
        let normalized = normalize_phone(phone);
        if let Some(person_id) = find_by_phone(db, &normalized).await? {
            merge_into_person(db, &person_id, contact).await?;
            return Ok(person_id);
        }
    }

    // No match — create a new person
    create_person(db, contact).await
}

async fn find_by_email(db: &PgPool, email: &str) -> Result<Option<String>> {
    let row = sqlx::query(
        r#"SELECT id FROM wiki_people
           WHERE emails @> to_jsonb($1::text)
           LIMIT 1"#,
    )
    .bind(email)
    .fetch_optional(db)
    .await?;
    Ok(row.and_then(|r| r.try_get::<Option<String>, _>("id").ok().flatten()))
}

async fn find_by_phone(db: &PgPool, phone: &str) -> Result<Option<String>> {
    // `phones` is JSONB (a string array), so the old `phones LIKE $1` errored
    // (`operator does not exist: jsonb ~~ text`) and counted EVERY phone-only
    // contact as a failure. Unnest the array and compare digit-only forms so
    // formatting differences ("(512) 555-1234" vs "+15125551234") still match —
    // mirrors `normalize_phone`, since stored numbers are raw.
    let digits: String = phone.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return Ok(None);
    }
    let row = sqlx::query(
        r#"SELECT id FROM wiki_people
           WHERE EXISTS (
               SELECT 1 FROM jsonb_array_elements_text(phones) AS p
               WHERE regexp_replace(p, '[^0-9]', '', 'g') LIKE '%' || $1 || '%'
           )
           LIMIT 1"#,
    )
    .bind(&digits)
    .fetch_optional(db)
    .await?;
    Ok(row.and_then(|r| r.try_get::<Option<String>, _>("id").ok().flatten()))
}

fn normalize_phone(phone: &str) -> String {
    let trimmed = phone.trim();
    if let Some(stripped) = trimmed.strip_prefix('+') {
        format!(
            "+{}",
            stripped
                .chars()
                .filter(|c| c.is_ascii_digit())
                .collect::<String>()
        )
    } else {
        trimmed.chars().filter(|c| c.is_ascii_digit()).collect()
    }
}

async fn merge_into_person(db: &PgPool, person_id: &str, contact: &ContactRecord) -> Result<()> {
    let row =
        sqlx::query(r#"SELECT emails, phones, birthday, metadata FROM wiki_people WHERE id = $1"#)
            .bind(person_id)
            .fetch_one(db)
            .await?;

    // These columns are JSONB / DATE — read them as native types, not String.
    // (try_get::<String> on a JSONB/DATE column fails, so the prior code silently
    //  lost existing data and then bound strings back, failing the UPDATE entirely.)
    let existing_emails: Option<Value> = row.try_get("emails").ok();
    let existing_phones: Option<Value> = row.try_get("phones").ok();
    let existing_birthday: Option<NaiveDate> = row.try_get("birthday").ok();
    let existing_metadata: Option<Value> = row.try_get("metadata").ok();

    // Merge emails
    let mut emails: Vec<String> = existing_emails
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default();
    for email in &contact.emails {
        if !emails.contains(email) {
            emails.push(email.clone());
        }
    }
    let emails_json = serde_json::json!(emails);

    // Merge phones
    let mut phones: Vec<String> = existing_phones
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default();
    for phone in &contact.phones {
        let normalized = normalize_phone(phone);
        if !phones.iter().any(|p| normalize_phone(p) == normalized) {
            phones.push(phone.clone());
        }
    }
    let phones_json = serde_json::json!(phones);

    // Birthday — only set if not already set
    let birthday =
        existing_birthday.or_else(|| contact.birthday.as_deref().and_then(parse_birthday));

    // Metadata — add ios_contact_id and organization
    let mut metadata: Value = existing_metadata.unwrap_or_else(|| serde_json::json!({}));
    if let Some(obj) = metadata.as_object_mut() {
        obj.insert(
            "ios_contact_id".to_string(),
            serde_json::json!(contact.identifier),
        );
        if let Some(org) = &contact.organization_name {
            obj.insert("organization".to_string(), serde_json::json!(org));
        }
    }
    sqlx::query(
        r#"UPDATE wiki_people
           SET emails = $1,
               phones = $2,
               birthday = COALESCE($3, birthday),
               metadata = $4,
               updated_at = now()
           WHERE id = $5"#,
    )
    .bind(emails_json)
    .bind(phones_json)
    .bind(birthday)
    .bind(metadata)
    .bind(person_id)
    .execute(db)
    .await?;

    Ok(())
}

async fn create_person(db: &PgPool, contact: &ContactRecord) -> Result<String> {
    let name = if !contact.given_name.is_empty() && !contact.family_name.is_empty() {
        format!("{} {}", contact.given_name, contact.family_name)
    } else if !contact.given_name.is_empty() {
        contact.given_name.clone()
    } else {
        contact.family_name.clone()
    };

    let id_seed = contact
        .emails
        .first()
        .map(String::as_str)
        .unwrap_or(&contact.identifier);
    let person_id = generate_id(WIKI_PERSON_PREFIX, &[id_seed]);

    let emails_json = serde_json::json!(contact.emails);
    let phones_json = serde_json::json!(contact.phones);

    // The normal form of everything this person answers to — E.164 phones, lowercased
    // emails — indexed so a message from "+15125550142" can find the contact you
    // typed as "(512) 555-0142". `emails`/`phones` keep the raw strings: what the
    // human wrote is worth keeping, and a normal form is not a replacement for it.
    //
    // Without this, resolution is impossible: 525 contacts, thousands of messages, and
    // not one connection, because the two sides spell the same person differently.
    let handles_json = serde_json::json!(virtues_helpers::handles::normalized_handles(
        contact.emails.iter().map(String::as_str),
        contact.phones.iter().map(String::as_str),
    ));

    let birthday = contact.birthday.as_deref().and_then(parse_birthday);

    let metadata = serde_json::json!({
        "ios_contact_id": contact.identifier,
        "source": "ios_contacts",
        "organization": contact.organization_name,
    });

    sqlx::query(
        r#"INSERT INTO wiki_people (id, name, emails, phones, handles, birthday, metadata)
           VALUES ($1, $2, $3, $4, $5, $6, $7)
           ON CONFLICT (id) DO UPDATE SET
               emails = EXCLUDED.emails,
               phones = EXCLUDED.phones,
               handles = EXCLUDED.handles,
               birthday = COALESCE(EXCLUDED.birthday, wiki_people.birthday),
               metadata = EXCLUDED.metadata,
               updated_at = now()"#,
    )
    .bind(&person_id)
    .bind(&name)
    .bind(&emails_json)
    .bind(&phones_json)
    .bind(&handles_json)
    .bind(&birthday)
    .bind(&metadata)
    .execute(db)
    .await?;

    Ok(person_id)
}
