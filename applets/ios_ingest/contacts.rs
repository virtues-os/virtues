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
//! THE NAME FOLLOWS THE CONTACT until the owner renames the person. The
//! canonical name is built only from structured fields (given, middle, family),
//! never from emoji or decoration — a contact typed as "Caity 🌷" names a person
//! "Caity". Everything else she answers to (the contact's nickname, a maiden
//! name, whatever the name used to be) becomes an alias, which is what matching
//! reads. The merge path used to leave `name` alone forever, so the first
//! spelling a contact ever had was the only one the box ever knew.
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
    middle_name: String,
    family_name: String,
    /// Maiden or former family name (`CNContact.previousFamilyName`).
    previous_family_name: String,
    nickname: String,
    /// The contact card's related names, as `{label, name}` — "partner", "mother".
    /// Kept as source material for the person's article; the box never infers a
    /// relationship, it only records one the owner wrote down.
    relations: Vec<Value>,
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
    let text = |key: &str| {
        record
            .get(key)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string()
    };
    let middle_name = text("middleName");
    let previous_family_name = text("previousFamilyName");
    let nickname = text("nickname");
    let relations: Vec<Value> = record
        .get("relations")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
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
        middle_name,
        family_name,
        previous_family_name,
        nickname,
        relations,
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

/// Keep letters, digits and the punctuation names actually use; drop emoji and
/// other decoration, then collapse the whitespace that leaves behind.
fn clean_name_part(s: &str) -> String {
    let kept: String = s
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, '\'' | '’' | '-' | '.' | ' ') {
                c
            } else {
                ' '
            }
        })
        .collect();
    kept.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The canonical name: given + middle + family, from structured fields only.
/// `None` when the contact carries no usable name at all.
fn canonical_name(contact: &ContactRecord) -> Option<String> {
    let parts: Vec<String> = [&contact.given_name, &contact.middle_name, &contact.family_name]
        .iter()
        .map(|p| clean_name_part(p))
        .filter(|p| !p.is_empty())
        .collect();
    (!parts.is_empty()).then(|| parts.join(" "))
}

/// Every other name the contact says this person answers to.
fn contact_aliases(contact: &ContactRecord) -> Vec<String> {
    let mut out = Vec::new();
    let nick = clean_name_part(&contact.nickname);
    if !nick.is_empty() {
        out.push(nick);
    }
    let given = clean_name_part(&contact.given_name);
    let maiden = clean_name_part(&contact.previous_family_name);
    if !maiden.is_empty() {
        out.push(if given.is_empty() { maiden } else { format!("{given} {maiden}") });
    }
    out
}

/// Aliases are stored lowercased and unique (the same normal form
/// `api::wiki::normalize_aliases` writes), and never repeat the name itself.
fn merge_aliases(existing: &[String], add: &[String], name: &str) -> Vec<String> {
    let name_lc = name.to_lowercase();
    let mut out: Vec<String> = Vec::with_capacity(existing.len() + add.len());
    for a in existing.iter().chain(add) {
        let a = a.trim().to_lowercase();
        if !a.is_empty() && a != name_lc && !out.contains(&a) {
            out.push(a);
        }
    }
    out
}

/// Whether the contact still owns this person's name.
///
/// The owner renaming someone (`api::wiki::update_person` stamps
/// `name_edited_at`) ends it for good. Otherwise the contact owns the name if
/// the name is still the one the contact last wrote (`contact_name`). Rows from
/// before `contact_name` existed carry no such record, so for them the contact
/// owns the name only while it still starts with the contact's given name — a
/// "Caity 🌷" that has since become "Caity Ryan Richie" — and an owner edit that
/// changed it to anything else is left alone.
fn contact_owns_name(current: &str, metadata: &Value, contact: &ContactRecord) -> bool {
    if metadata.get("name_edited_at").is_some() {
        return false;
    }
    if let Some(last) = metadata.get("contact_name").and_then(|v| v.as_str()) {
        return current == last;
    }
    let from_contacts = metadata.get("source").and_then(|v| v.as_str()) == Some("ios_contacts");
    let given = clean_name_part(&contact.given_name).to_lowercase();
    let now = clean_name_part(current).to_lowercase();
    from_contacts
        && !given.is_empty()
        && (now == given || now.starts_with(&format!("{given} ")))
}

async fn merge_into_person(db: &PgPool, person_id: &str, contact: &ContactRecord) -> Result<()> {
    let row = sqlx::query(
        r#"SELECT name, emails, phones, birthday, metadata, aliases FROM wiki_people WHERE id = $1"#,
    )
    .bind(person_id)
    .fetch_one(db)
    .await?;
    let current_name: String = row.try_get("name")?;
    let existing_aliases: Vec<String> = row
        .try_get::<Option<Value>, _>("aliases")?
        .map(serde_json::from_value)
        .transpose()?
        .unwrap_or_default();

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
        if !contact.relations.is_empty() {
            obj.insert("ios_relations".to_string(), serde_json::json!(contact.relations));
        }
    }

    // The name, and what it used to be. A name the contact no longer owns is
    // left exactly as the owner wrote it; the contact's spellings still arrive
    // as aliases so a message or a transcript can find the person by them.
    let mut aliases_add = contact_aliases(contact);
    let mut name = current_name.clone();
    if let Some(canonical) = canonical_name(contact) {
        if contact_owns_name(&current_name, &metadata, contact) {
            if canonical != current_name {
                let old = clean_name_part(&current_name);
                if !old.is_empty() {
                    aliases_add.push(old);
                }
            }
            name = canonical.clone();
            if let Some(obj) = metadata.as_object_mut() {
                obj.insert("contact_name".to_string(), serde_json::json!(canonical));
            }
        } else if canonical != current_name {
            aliases_add.push(canonical);
        }
    }
    let aliases_json = serde_json::json!(merge_aliases(&existing_aliases, &aliases_add, &name));

    sqlx::query(
        r#"UPDATE wiki_people
           SET name = $1,
               emails = $2,
               phones = $3,
               birthday = COALESCE($4, birthday),
               metadata = $5,
               aliases = $6,
               updated_at = now()
           WHERE id = $7"#,
    )
    .bind(&name)
    .bind(emails_json)
    .bind(phones_json)
    .bind(birthday)
    .bind(metadata)
    .bind(aliases_json)
    .bind(person_id)
    .execute(db)
    .await?;

    Ok(())
}

async fn create_person(db: &PgPool, contact: &ContactRecord) -> Result<String> {
    // A contact whose only name is decoration ("🌷") still gets a person — named
    // by the raw text, since there is nothing structured to prefer.
    let name = canonical_name(contact).unwrap_or_else(|| {
        format!("{} {}", contact.given_name, contact.family_name)
            .trim()
            .to_string()
    });
    let aliases_json = serde_json::json!(merge_aliases(&[], &contact_aliases(contact), &name));

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

    let mut metadata = serde_json::json!({
        "ios_contact_id": contact.identifier,
        "source": "ios_contacts",
        "organization": contact.organization_name,
        "contact_name": name,
    });
    if !contact.relations.is_empty() {
        metadata["ios_relations"] = serde_json::json!(contact.relations);
    }

    // ON CONFLICT keeps the existing name and aliases: a conflict means this id
    // already exists, and the name on an existing row is `merge_into_person`'s
    // decision, not a blind overwrite.
    sqlx::query(
        r#"INSERT INTO wiki_people (id, name, emails, phones, handles, birthday, metadata, aliases)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
           ON CONFLICT (id) DO UPDATE SET
               emails = EXCLUDED.emails,
               phones = EXCLUDED.phones,
               handles = EXCLUDED.handles,
               birthday = COALESCE(EXCLUDED.birthday, wiki_people.birthday),
               metadata = wiki_people.metadata || EXCLUDED.metadata - 'contact_name',
               updated_at = now()"#,
    )
    .bind(&person_id)
    .bind(&name)
    .bind(&emails_json)
    .bind(&phones_json)
    .bind(&handles_json)
    .bind(&birthday)
    .bind(&metadata)
    .bind(&aliases_json)
    .execute(db)
    .await?;

    Ok(person_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contact(given: &str, middle: &str, family: &str) -> ContactRecord {
        ContactRecord {
            identifier: "c1".into(),
            given_name: given.into(),
            middle_name: middle.into(),
            family_name: family.into(),
            previous_family_name: String::new(),
            nickname: String::new(),
            relations: vec![],
            organization_name: None,
            phones: vec![],
            emails: vec![],
            birthday: None,
        }
    }

    #[test]
    fn canonical_name_drops_decoration() {
        assert_eq!(canonical_name(&contact("Nick 🌷", "", "")).as_deref(), Some("Nick"));
        assert_eq!(
            canonical_name(&contact("David", "Ade", "Okafor")).as_deref(),
            Some("David Ade Okafor")
        );
        assert_eq!(canonical_name(&contact("Mary-Jo", "", "O’Neil")).as_deref(), Some("Mary-Jo O’Neil"));
        assert_eq!(canonical_name(&contact("🌷", "", "")), None);
    }

    #[test]
    fn nickname_and_maiden_name_become_aliases() {
        let mut c = contact("David", "", "Okafor");
        c.nickname = "Dave ⚽".into();
        c.previous_family_name = "Adeyemi".into();
        assert_eq!(contact_aliases(&c), vec!["Dave".to_string(), "David Adeyemi".to_string()]);
        let merged = merge_aliases(&["dave".into()], &contact_aliases(&c), "David Okafor");
        assert_eq!(merged, vec!["dave".to_string(), "david adeyemi".to_string()]);
    }

    #[test]
    fn the_contact_owns_a_name_until_the_owner_renames() {
        let c = contact("Nick", "", "Example");
        let legacy = serde_json::json!({"source": "ios_contacts"});
        assert!(contact_owns_name("Nick 🌷", &legacy, &c));
        assert!(contact_owns_name("Nick", &legacy, &c));
        // Renamed by hand to something else: leave it alone.
        assert!(!contact_owns_name("Nicholas", &legacy, &c));
        // Not created from contacts (e.g. minted from an email sender).
        assert!(!contact_owns_name("Nick", &serde_json::json!({}), &c));
        // Tracked: owned while the name is still the one contacts wrote.
        let tracked = serde_json::json!({"source": "ios_contacts", "contact_name": "Nick Old"});
        assert!(contact_owns_name("Nick Old", &tracked, &c));
        assert!(!contact_owns_name("Nick O.", &tracked, &c));
        // An owner edit ends it for good.
        let edited = serde_json::json!({"contact_name": "Nick", "name_edited_at": "2026-09-24"});
        assert!(!contact_owns_name("Nick", &edited, &c));
    }
}
