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
//! never from emoji or decoration — a contact typed as "Nick 🌷" names a person
//! "Nick". Everything else they answer to (the contact's nickname, a maiden
//! name, whatever the name used to be) becomes an alias, which is what matching
//! reads. The merge path used to leave `name` alone forever, so the first
//! spelling a contact ever had was the only one the box ever knew.
//!
//! THE PICTURE FOLLOWS THE CONTACT'S PHOTO until the owner chooses one. The
//! phone sends the card's thumbnail as base64 JPEG (`photo`) with its SHA-256
//! (`photoSha256`). [`externalize_photos`] keeps it in the media store before
//! the record is archived, leaving `photoUrl` and the hash in its place, and
//! [`contact_sets_picture`] decides whether it becomes the person's picture.
//! `metadata.picture_from` says whose the picture is (`contacts` or `owner`)
//! and `metadata.picture_sha256` which photo the contacts put there.
//!
//! Uses runtime `sqlx::query` (not compile-time `sqlx::query!`) because
//! the actions crate doesn't run a build-time DB connection.

use anyhow::{anyhow, Result};
use base64::Engine;
use chrono::{DateTime, NaiveDate};
use serde_json::{json, Value};
use sqlx::PgPool;
use sqlx::Row;
use virtues::api::media::upload_media;
use virtues::api::DriveConfig;
use virtues::bookmark_media::{sha256_hex, sniff_image_mime};
use virtues::storage::Storage;
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
    /// The card's photo, once [`externalize_photos`] has kept it.
    photo: Option<ContactPhoto>,
}

impl ContactRecord {
    /// A contact with no given or family name never becomes a person.
    fn has_name(&self) -> bool {
        !self.given_name.is_empty() || !self.family_name.is_empty()
    }
}

/// A contact's photo in the media store: the box URL that serves it, and the
/// SHA-256 of its bytes, which is the photo's identity.
#[derive(Debug, Clone, PartialEq)]
struct ContactPhoto {
    url: String,
    sha256: String,
}

/// The largest contact photo kept. The phone caps its thumbnails at 64 KiB;
/// this is four times that, so only a sender that ignores the contract meets it.
const MAX_PHOTO_BYTES: usize = 256 * 1024;

/// The photo types kept, with the extension each is stored under: the ones
/// every browser draws. The phone sends JPEG.
const PHOTO_TYPES: &[(&str, &str)] = &[("image/jpeg", "jpg"), ("image/png", "png")];

/// What a contact's `photo` turns into, decided without touching storage.
#[derive(Debug)]
enum IncomingPhoto {
    /// No photo on this record.
    Absent,
    Keep {
        bytes: Vec<u8>,
        mime: &'static str,
        ext: &'static str,
    },
    /// A photo that will be the same on every retry and can never be kept.
    /// The contact survives without it.
    Unusable(String),
}

/// Take the photo's base64 OUT of a contact record, whatever becomes of it: the
/// record is archived next, and the media store is the photo's one home.
///
/// A record replayed from the lake carries `photoUrl` and `photoSha256` but no
/// `photo`, and passes through untouched.
fn take_photo(rec: &mut Value) -> IncomingPhoto {
    let Some(obj) = rec.as_object_mut() else {
        return IncomingPhoto::Absent;
    };
    let Some(raw) = obj.remove("photo") else {
        return IncomingPhoto::Absent;
    };
    // The phone's hash describes the bytes just removed. It goes with them;
    // the hash of what is kept is written back beside the URL.
    let sent_sha = obj
        .remove("photoSha256")
        .and_then(|v| v.as_str().map(|s| s.trim().to_ascii_lowercase()));
    let Some(b64) = raw.as_str() else {
        return IncomingPhoto::Unusable("photo was not a string".into());
    };
    let bytes = match base64::engine::general_purpose::STANDARD.decode(b64.trim()) {
        Ok(b) if !b.is_empty() => b,
        Ok(_) => return IncomingPhoto::Unusable("photo was empty".into()),
        Err(e) => return IncomingPhoto::Unusable(format!("photo did not decode: {e}")),
    };
    if bytes.len() > MAX_PHOTO_BYTES {
        return IncomingPhoto::Unusable(format!(
            "photo is {} bytes, over the {MAX_PHOTO_BYTES}-byte cap",
            bytes.len()
        ));
    }
    if sent_sha.is_some_and(|sent| sent != sha256_hex(&bytes)) {
        return IncomingPhoto::Unusable("photo did not match the hash sent with it".into());
    }
    let kind = sniff_image_mime(&bytes)
        .and_then(|mime| PHOTO_TYPES.iter().find(|(m, _)| *m == mime));
    match kind {
        Some(&(mime, ext)) => IncomingPhoto::Keep { bytes, mime, ext },
        None => IncomingPhoto::Unusable("photo was not a JPEG or PNG".into()),
    }
}

/// Keep each contact's photo in the media store and point its record at it.
///
/// Runs BEFORE the records are archived (the bookmark arm's order), so the lake
/// holds the record and the photo's URL, never a second base64 copy. The media
/// store is content-addressed, so the same photo sent on every launch is one
/// file. A photo that can never be kept is dropped and the contact kept; a
/// failure to store one fails the batch, and the phone's outbox sends it again.
pub async fn externalize_photos(
    db: &PgPool,
    storage: &Storage,
    records: &[Value],
) -> Result<Vec<Value>> {
    let config = DriveConfig::new(std::sync::Arc::new(storage.clone()));
    let mut out = Vec::with_capacity(records.len());
    for record in records {
        let mut rec = record.clone();
        match take_photo(&mut rec) {
            IncomingPhoto::Absent => {}
            IncomingPhoto::Unusable(why) => {
                tracing::warn!(why, "contact photo dropped; keeping the contact without it");
            }
            IncomingPhoto::Keep { bytes, mime, ext } => {
                // A contact that will never be a person would leave its photo
                // in the store with nothing pointing at it.
                if parse_contact(&rec).is_some_and(|c| c.has_name()) {
                    let sha = sha256_hex(&bytes);
                    let stored = upload_media(
                        db,
                        &config,
                        &format!("contact-photo.{ext}"),
                        Some(mime.to_string()),
                        bytes.into(),
                    )
                    .await
                    .map_err(|e| anyhow!("could not store a contact photo: {e}"))?;
                    rec["photoUrl"] = json!(stored.url);
                    rec["photoSha256"] = json!(sha);
                }
            }
        }
        out.push(rec);
    }
    Ok(out)
}

pub async fn resolve_contacts(db: &PgPool, records: &[Value]) -> Result<(usize, usize)> {
    let mut resolved = 0;
    let mut failed = 0;

    for record in records {
        let Some(contact) = parse_contact(record) else {
            continue;
        };

        if !contact.has_name() {
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

    let photo = match (text("photoUrl"), text("photoSha256")) {
        (url, sha256) if !url.is_empty() && !sha256.is_empty() => {
            Some(ContactPhoto { url, sha256 })
        }
        _ => None,
    };

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
        photo,
    })
}

async fn resolve_or_create(db: &PgPool, contact: &ContactRecord) -> Result<String> {
    let person_id = match find_person(db, contact).await? {
        Some(person_id) => {
            merge_into_person(db, &person_id, contact).await?;
            person_id
        }
        None => create_person(db, contact).await?,
    };
    // A contact without a photo leaves the picture alone.
    if let Some(photo) = &contact.photo {
        apply_contact_photo(db, &person_id, photo).await?;
    }
    Ok(person_id)
}

/// The person this contact already is: whoever holds one of its addresses or
/// numbers, compared in normal form, addresses first.
async fn find_person(db: &PgPool, contact: &ContactRecord) -> Result<Option<String>> {
    for raw in contact.emails.iter().chain(contact.phones.iter()) {
        let Some(handle) = virtues_helpers::handles::normalize_handle(raw) else {
            continue;
        };
        if let Some(person_id) = find_by_handle(db, &handle).await? {
            return Ok(Some(person_id));
        }
    }
    Ok(None)
}

/// Whether the contact's photo becomes the person's picture.
///
/// A picture the contacts put there (`picture_from: "contacts"`) follows the
/// contact: a different photo replaces it, the same photo is left as it is.
/// Any other picture is the owner's, and so is the owner's choice of none
/// (`picture_from: "owner"` with no picture); the contact never touches
/// either. A person with no picture and no record of one takes the photo.
fn contact_sets_picture(current: Option<&str>, metadata: &Value, photo: &ContactPhoto) -> bool {
    let empty = current.is_none_or(|p| p.trim().is_empty());
    match metadata.get("picture_from").and_then(Value::as_str) {
        Some("contacts") => {
            empty
                || metadata.get("picture_sha256").and_then(Value::as_str)
                    != Some(photo.sha256.as_str())
        }
        None => empty,
        Some(_) => false,
    }
}

/// Set the person's picture from the contact's photo when
/// [`contact_sets_picture`] says so. Returns whether it wrote.
async fn apply_contact_photo(db: &PgPool, person_id: &str, photo: &ContactPhoto) -> Result<bool> {
    let (picture, metadata): (Option<String>, Value) =
        sqlx::query_as("SELECT picture, metadata FROM wiki_people WHERE id = $1")
            .bind(person_id)
            .fetch_one(db)
            .await?;
    if !contact_sets_picture(picture.as_deref(), &metadata, photo) {
        return Ok(false);
    }
    // Written only if neither has moved since the read: an owner who chose a
    // picture in between keeps it.
    let written = sqlx::query(
        r#"UPDATE wiki_people
           SET picture = $2,
               metadata = metadata || jsonb_build_object('picture_from', 'contacts', 'picture_sha256', $3::text),
               updated_at = now()
           WHERE id = $1
             AND picture IS NOT DISTINCT FROM $4
             AND metadata->>'picture_from' IS NOT DISTINCT FROM $5"#,
    )
    .bind(person_id)
    .bind(&photo.url)
    .bind(&photo.sha256)
    .bind(&picture)
    .bind(metadata.get("picture_from").and_then(Value::as_str))
    .execute(db)
    .await?;
    Ok(written.rows_affected() > 0)
}

/// Who holds this normalized address or number. `handles` is complete for
/// everyone (the people resolver derives it from `emails` and `phones` on
/// every pass), so this is an exact, indexed match. It replaced a digits
/// `LIKE '%…%'`, which joined a contact to anyone whose number merely
/// contained its digits, and a case-sensitive match on raw `emails`.
async fn find_by_handle(db: &PgPool, handle: &str) -> Result<Option<String>> {
    Ok(sqlx::query_scalar("SELECT id FROM wiki_people WHERE handles ? $1 ORDER BY id LIMIT 1")
        .bind(handle)
        .fetch_optional(db)
        .await?)
}

/// A phone's identity for "do we already have this number": its E.164 form
/// when it has one, else the digits.
fn phone_key(phone: &str) -> String {
    virtues_helpers::handles::normalize_phone(phone)
        .unwrap_or_else(|| phone.chars().filter(|c| c.is_ascii_digit()).collect())
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
/// "Nick 🌷" that has since become "Nick Moreau" — and an owner edit that
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
        let key = phone_key(phone);
        if !phones.iter().any(|p| phone_key(p) == key) {
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

    /// Two spellings of one number are one number, so a card synced again
    /// does not add "+1 512 555 0101" beside "(512) 555-0101".
    #[test]
    fn one_number_in_two_spellings_is_one_number() {
        assert_eq!(phone_key("(512) 555-0101"), phone_key("+1 512 555 0101"));
        assert_ne!(phone_key("512-555-0101"), phone_key("+44 20 7946 0101"));
    }

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
            photo: None,
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

    // ── the picture ─────────────────────────────────────────────────────

    /// Enough of a JPEG for the sniffer: the start-of-image marker, then filler.
    fn jpeg(seed: u8) -> Vec<u8> {
        let mut b = vec![0xFF, 0xD8, 0xFF, 0xE0];
        b.extend(std::iter::repeat_n(seed, 32));
        b
    }

    fn b64(bytes: &[u8]) -> String {
        base64::engine::general_purpose::STANDARD.encode(bytes)
    }

    fn photo(url: &str, sha256: &str) -> ContactPhoto {
        ContactPhoto { url: url.into(), sha256: sha256.into() }
    }

    #[test]
    fn a_photo_is_taken_out_of_the_record_and_checked_against_its_hash() {
        let bytes = jpeg(1);
        let mut rec = json!({
            "identifier": "c1", "givenName": "Nick",
            "photo": b64(&bytes), "photoSha256": sha256_hex(&bytes).to_uppercase(),
        });
        match take_photo(&mut rec) {
            IncomingPhoto::Keep { bytes: kept, mime, ext } => {
                assert_eq!(kept, bytes);
                assert_eq!((mime, ext), ("image/jpeg", "jpg"));
            }
            other => panic!("expected Keep, got {other:?}"),
        }
        assert!(rec.get("photo").is_none(), "the base64 must not reach the archive");
        assert!(rec.get("photoSha256").is_none(), "the phone's hash goes with its bytes");
        assert_eq!(rec["givenName"], "Nick", "the rest of the contact is untouched");
    }

    #[test]
    fn a_photo_that_can_never_be_kept_is_dropped_but_still_removed() {
        let bytes = jpeg(1);
        let png_ok = sha256_hex(&bytes);
        let cases = [
            json!({"photo": b64(&bytes), "photoSha256": sha256_hex(&jpeg(2))}),
            json!({"photo": b64(b"just some text")}),
            json!({"photo": "not base64 !!"}),
            json!({"photo": ""}),
            json!({"photo": 42}),
            json!({"photo": b64(&vec![0xFF; MAX_PHOTO_BYTES + 1])}),
        ];
        for mut rec in cases {
            rec["givenName"] = json!("Nick");
            assert!(matches!(take_photo(&mut rec), IncomingPhoto::Unusable(_)), "{rec}");
            assert!(rec.get("photo").is_none(), "removed even when unusable");
            assert_eq!(rec["givenName"], "Nick", "the contact survives");
        }
        // A hash with no bytes is a record replayed from the lake: left alone.
        let mut replayed = json!({"photoUrl": "/api/drive/files/f1/download", "photoSha256": png_ok});
        let before = replayed.clone();
        assert!(matches!(take_photo(&mut replayed), IncomingPhoto::Absent));
        assert_eq!(replayed, before);
    }

    #[test]
    fn the_picture_follows_the_contact_until_the_owner_chooses() {
        let p = photo("/api/drive/files/f2/download", "bbb");
        let none = json!({});
        // Nobody has chosen a picture: the contact's photo is it.
        assert!(contact_sets_picture(None, &none, &p));
        assert!(contact_sets_picture(Some("  "), &none, &p));
        // A picture with no record of whose it is is not the contact's to replace.
        assert!(!contact_sets_picture(Some("https://example.com/a.jpg"), &none, &p));
        // Put there by the contacts: the same photo is a no-op, a new one replaces it.
        let from_contacts = json!({"picture_from": "contacts", "picture_sha256": "bbb"});
        assert!(!contact_sets_picture(Some("/api/drive/files/f2/download"), &from_contacts, &p));
        let older = json!({"picture_from": "contacts", "picture_sha256": "aaa"});
        assert!(contact_sets_picture(Some("/api/drive/files/f1/download"), &older, &p));
        // The owner's picture, and the owner's choice of none, are never touched.
        let owner = json!({"picture_from": "owner"});
        assert!(!contact_sets_picture(Some("/api/drive/files/mine/download"), &owner, &p));
        assert!(!contact_sets_picture(None, &owner, &p));
    }

    async fn person_picture(pool: &PgPool, id: &str) -> (Option<String>, Value) {
        sqlx::query_as("SELECT picture, metadata FROM wiki_people WHERE id = $1")
            .bind(id)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    fn card(photo: Option<(&str, &str)>) -> Value {
        let mut rec = json!({
            "identifier": "c-nick", "givenName": "Nick", "familyName": "Example",
            "phones": [{"number": "+15125550142"}], "emails": [],
        });
        if let Some((url, sha)) = photo {
            rec["photoUrl"] = json!(url);
            rec["photoSha256"] = json!(sha);
        }
        rec
    }

    /// The whole life of a contact photo against a real table: set, re-sent,
    /// changed, sent with no photo, then the owner's choice and the owner's
    /// removal, neither of which a contact undoes.
    #[sqlx::test(migrations = "../virtues-core/migrations")]
    async fn a_contact_photo_is_the_picture_until_the_owner_chooses(pool: PgPool) {
        let first = ("/api/drive/files/f1/download", "aaa");
        let (n, failed) = resolve_contacts(&pool, &[card(Some(first))]).await.unwrap();
        assert_eq!((n, failed), (1, 0));
        let id: String = sqlx::query_scalar("SELECT id FROM wiki_people WHERE name = 'Nick Example'")
            .fetch_one(&pool)
            .await
            .unwrap();
        let (picture, meta) = person_picture(&pool, &id).await;
        assert_eq!(picture.as_deref(), Some(first.0), "set from the contact");
        assert_eq!(meta["picture_from"], "contacts");
        assert_eq!(meta["picture_sha256"], "aaa");

        // The same photo again writes nothing.
        let p = photo(first.0, first.1);
        assert!(!apply_contact_photo(&pool, &id, &p).await.unwrap(), "same hash is a no-op");
        resolve_contacts(&pool, &[card(Some(first))]).await.unwrap();
        assert_eq!(person_picture(&pool, &id).await.0.as_deref(), Some(first.0));

        // A new photo on the card replaces the one the contact put there.
        let second = ("/api/drive/files/f2/download", "bbb");
        resolve_contacts(&pool, &[card(Some(second))]).await.unwrap();
        let (picture, meta) = person_picture(&pool, &id).await;
        assert_eq!(picture.as_deref(), Some(second.0), "a changed photo replaces it");
        assert_eq!(meta["picture_sha256"], "bbb");

        // A card with no photo leaves the picture alone.
        resolve_contacts(&pool, &[card(None)]).await.unwrap();
        assert_eq!(person_picture(&pool, &id).await.0.as_deref(), Some(second.0));

        // The owner chooses a picture: the next photo from the card doesn't replace it.
        let mine = "/api/drive/files/mine/download";
        let req = serde_json::from_value(json!({"picture": mine})).unwrap();
        virtues::api::wiki::update_person(&pool, id.clone(), req).await.unwrap();
        resolve_contacts(&pool, &[card(Some(("/api/drive/files/f3/download", "ccc")))])
            .await
            .unwrap();
        let (picture, meta) = person_picture(&pool, &id).await;
        assert_eq!(picture.as_deref(), Some(mine), "an owner-set picture is kept");
        assert_eq!(meta["picture_from"], "owner");

        // The owner removes it: no picture, and the card doesn't put one back.
        let req = serde_json::from_value(json!({"picture": ""})).unwrap();
        virtues::api::wiki::update_person(&pool, id.clone(), req).await.unwrap();
        resolve_contacts(&pool, &[card(Some(first))]).await.unwrap();
        assert_eq!(person_picture(&pool, &id).await.0, None, "removal is the owner's choice too");
    }

    /// The photo lands in the media store under its own hash and the record
    /// keeps only its address; a contact that will never be a person stores
    /// nothing.
    #[sqlx::test(migrations = "../virtues-core/migrations")]
    async fn a_photo_is_kept_in_the_media_store_before_the_record_is_archived(pool: PgPool) {
        let dir = std::env::temp_dir().join(format!("contact-photos-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let storage = Storage::file(dir.to_string_lossy().into_owned()).unwrap();

        let bytes = jpeg(7);
        let sha = sha256_hex(&bytes);
        let mut named = card(None);
        named["photo"] = json!(b64(&bytes));
        named["photoSha256"] = json!(sha);
        let nameless = json!({"identifier": "c-blank", "givenName": "", "photo": b64(&jpeg(8))});

        let out = externalize_photos(&pool, &storage, &[named, nameless]).await.unwrap();
        assert!(out.iter().all(|r| r.get("photo").is_none()), "no base64 reaches the archive");
        let url = out[0]["photoUrl"].as_str().expect("a url for the named contact");
        assert!(url.starts_with("/api/drive/files/") && url.ends_with("/download"), "{url}");
        assert_eq!(out[0]["photoSha256"], json!(sha));
        assert!(out[1].get("photoUrl").is_none(), "nothing stored for a nameless contact");

        let path: String = sqlx::query_scalar(
            "SELECT path FROM app_drive_files WHERE id = split_part($1, '/', 5)",
        )
        .bind(url)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(path, format!(".media/{}/{sha}.jpg", &sha[..2]));
        assert!(dir.join(&path).is_file(), "the bytes are on disk");

        // Parsed back, the record names the stored photo.
        let parsed = parse_contact(&out[0]).unwrap();
        assert_eq!(parsed.photo, Some(photo(url, &sha)));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
