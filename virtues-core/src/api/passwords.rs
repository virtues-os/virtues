//! Drive → Passwords: the owner's saved logins (agents/plan/secrets-plan.md,
//! slice 6).
//!
//! A login is a `credentials` row of kind `login`. `key` is the site's origin
//! (`https://example.com`), `name` is the label the owner sees, and the
//! ciphertext holds `{username, password, notes}`, so a database dump does
//! not list which accounts the owner has. A site can have several logins, one
//! per account, so a row's id is `login:<uuid>`.
//!
//! The list carries site, label and username. The password and notes leave
//! the box only through [`reveal`], which the Passwords page calls when the
//! owner opens a login. No tool calls any of this, and the chat's SQL role has
//! no grant on `credentials`, so a model never sees a password. The reveal
//! route also needs the app's owner proof (`api::owner_proof`), so a direct
//! request from another process on the owner's Mac, through the app's loopback
//! port, cannot read a password.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use virtues_helpers::crypto::TokenEncryptor;

use crate::error::{Error, Result};

/// The longest password or note the page accepts. Real ones are far shorter;
/// this only stops a pasted file from landing in the table.
const MAX_FIELD_BYTES: usize = 16 * 1024;

/// One saved login, without its password.
#[derive(Debug, Serialize, PartialEq)]
pub struct LoginItem {
    pub id: String,
    /// The site's origin, `https://example.com`.
    pub site: String,
    pub name: String,
    /// Empty when the row is `unreadable`.
    pub username: String,
    pub updated_at: DateTime<Utc>,
    /// The server's key does not open this row, for example a login sealed
    /// under another key before a restore. Site and label are plaintext
    /// columns, so the row is still listed and can be removed; it cannot be
    /// opened or edited.
    pub unreadable: bool,
}

/// What the owner sees after opening a login.
#[derive(Debug, Serialize, PartialEq)]
pub struct LoginSecret {
    pub password: String,
    pub notes: String,
}

/// A new login, or an edit. On an edit, a missing or empty `password` keeps
/// the saved one, and a missing `notes` keeps the saved notes.
#[derive(Debug, Deserialize)]
pub struct LoginInput {
    pub site: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
}

/// The sealed part of a row.
#[derive(Debug, Serialize, Deserialize)]
struct Sealed {
    username: String,
    password: String,
    #[serde(default)]
    notes: String,
}

pub async fn list(pool: &PgPool) -> Result<Vec<LoginItem>> {
    let rows: Vec<(String, String, String, String, DateTime<Utc>)> = sqlx::query_as(
        "SELECT id, key, name, secrets_ciphertext, updated_at FROM credentials WHERE kind = 'login'",
    )
    .fetch_all(pool)
    .await?;
    let enc = encryptor()?;
    // One row that does not open is listed as unreadable rather than failing
    // the page, so the owner still sees every other login and can remove it.
    let mut items: Vec<LoginItem> = rows
        .into_iter()
        .map(|(id, site, name, ciphertext, updated_at)| match open(&enc, &id, &ciphertext) {
            Ok(sealed) => LoginItem { id, site, name, username: sealed.username, updated_at, unreadable: false },
            Err(e) => {
                tracing::warn!(error = %e, "a saved login does not open with this server's key; listed as unreadable");
                LoginItem { id, site, name, username: String::new(), updated_at, unreadable: true }
            }
        })
        .collect();
    items.sort_by(|a, b| {
        (a.name.to_lowercase(), a.username.to_lowercase()).cmp(&(b.name.to_lowercase(), b.username.to_lowercase()))
    });
    Ok(items)
}

pub async fn create(pool: &PgPool, input: LoginInput) -> Result<LoginItem> {
    let (site, host) = normalize_site(&input.site)?;
    let password = input.password.unwrap_or_default();
    if password.is_empty() {
        return Err(Error::InvalidInput("A login needs a password. Type or paste it, then save.".into()));
    }
    let sealed = Sealed { username: input.username.trim().to_string(), password, notes: input.notes.unwrap_or_default() };
    check_sizes(&sealed)?;
    let name = label(input.name.as_deref(), &host);
    let id = format!("login:{}", uuid::Uuid::new_v4());
    sqlx::query(
        "INSERT INTO credentials (id, kind, key, name, status, secrets_ciphertext, metadata)
         VALUES ($1, 'login', $2, $3, 'active', $4, '{}'::jsonb)",
    )
    .bind(&id)
    .bind(&site)
    .bind(&name)
    .bind(seal(&sealed)?)
    .execute(pool)
    .await?;
    get(pool, &id).await
}

pub async fn update(pool: &PgPool, id: &str, input: LoginInput) -> Result<LoginItem> {
    let (site, host) = normalize_site(&input.site)?;
    let saved = load(pool, id).await?;
    let sealed = Sealed {
        username: input.username.trim().to_string(),
        password: input.password.filter(|p| !p.is_empty()).unwrap_or(saved.password),
        notes: input.notes.unwrap_or(saved.notes),
    };
    check_sizes(&sealed)?;
    sqlx::query(
        "UPDATE credentials SET key = $2, name = $3, secrets_ciphertext = $4
          WHERE id = $1 AND kind = 'login'",
    )
    .bind(id)
    .bind(&site)
    .bind(label(input.name.as_deref(), &host))
    .bind(seal(&sealed)?)
    .execute(pool)
    .await?;
    get(pool, id).await
}

/// Remove a login. Removing one that is already gone is not an error.
pub async fn delete(pool: &PgPool, id: &str) -> Result<()> {
    sqlx::query("DELETE FROM credentials WHERE id = $1 AND kind = 'login'")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// A login's password and notes, for the owner who opened it.
pub async fn reveal(pool: &PgPool, id: &str) -> Result<LoginSecret> {
    let sealed = load(pool, id).await?;
    Ok(LoginSecret { password: sealed.password, notes: sealed.notes })
}

async fn get(pool: &PgPool, id: &str) -> Result<LoginItem> {
    let row: Option<(String, String, String, DateTime<Utc>)> = sqlx::query_as(
        "SELECT key, name, secrets_ciphertext, updated_at FROM credentials WHERE id = $1 AND kind = 'login'",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    let (site, name, ciphertext, updated_at) = row.ok_or_else(not_found)?;
    let sealed = open(&encryptor()?, id, &ciphertext)?;
    Ok(LoginItem { id: id.to_string(), site, name, username: sealed.username, updated_at, unreadable: false })
}

async fn load(pool: &PgPool, id: &str) -> Result<Sealed> {
    let ciphertext: Option<String> =
        sqlx::query_scalar("SELECT secrets_ciphertext FROM credentials WHERE id = $1 AND kind = 'login'")
            .bind(id)
            .fetch_optional(pool)
            .await?;
    open(&encryptor()?, id, &ciphertext.ok_or_else(not_found)?)
}

fn not_found() -> Error {
    Error::NotFound("That login is not saved on your server any more.".into())
}

fn encryptor() -> Result<TokenEncryptor> {
    TokenEncryptor::from_env().map_err(|e| Error::Other(format!("vault encryptor: {e:#}")))
}

fn seal(sealed: &Sealed) -> Result<String> {
    let json = serde_json::to_string(sealed).map_err(|e| Error::Other(format!("encode login: {e}")))?;
    encryptor()?.encrypt(&json).map_err(|e| Error::Other(format!("seal login: {e:#}")))
}

fn open(enc: &TokenEncryptor, id: &str, ciphertext: &str) -> Result<Sealed> {
    let json = enc.decrypt(ciphertext).map_err(|e| Error::Other(format!("decrypt {id}: {e:#}")))?;
    serde_json::from_str(&json).map_err(|e| Error::Other(format!("decode {id}: {e}")))
}

fn check_sizes(sealed: &Sealed) -> Result<()> {
    if sealed.password.len() > MAX_FIELD_BYTES || sealed.notes.len() > MAX_FIELD_BYTES || sealed.username.len() > MAX_FIELD_BYTES {
        return Err(Error::InvalidInput("That is too long to save. Check what you pasted.".into()));
    }
    Ok(())
}

/// The owner's label, or the site's host without `www.`.
fn label(name: Option<&str>, host: &str) -> String {
    match name.map(str::trim).filter(|n| !n.is_empty()) {
        Some(n) => n.to_string(),
        None => host.strip_prefix("www.").unwrap_or(host).to_string(),
    }
}

/// A site as its origin and host. `example.com/login` becomes
/// `https://example.com`, because a saved login belongs to the site, not to
/// the page it was typed on.
fn normalize_site(input: &str) -> Result<(String, String)> {
    let trimmed = input.trim();
    let invalid =
        || Error::InvalidInput(format!("{trimmed} isn't a web address. Type the site, such as example.com."));
    if trimmed.is_empty() {
        return Err(Error::InvalidInput("A login needs a site, such as example.com.".into()));
    }
    let with_scheme = if trimmed.contains("://") { trimmed.to_string() } else { format!("https://{trimmed}") };
    let url = url::Url::parse(&with_scheme).map_err(|_| invalid())?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(invalid());
    }
    let host = url.host_str().filter(|h| !h.is_empty()).ok_or_else(invalid)?.to_string();
    Ok((url.origin().ascii_serialization(), host))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::credentials::ensure_test_key;

    fn input(site: &str, username: &str, password: Option<&str>) -> LoginInput {
        LoginInput {
            site: site.into(),
            name: None,
            username: username.into(),
            password: password.map(Into::into),
            notes: None,
        }
    }

    #[test]
    fn a_site_is_kept_as_its_origin() {
        assert_eq!(
            normalize_site("www.example.com/login?next=/").unwrap(),
            ("https://www.example.com".to_string(), "www.example.com".to_string())
        );
        assert_eq!(normalize_site("http://example.com:8080/a").unwrap().0, "http://example.com:8080");
        assert!(normalize_site("").is_err());
        assert!(normalize_site("ftp://example.com").is_err());
        assert_eq!(label(None, "www.example.com"), "example.com");
        assert_eq!(label(Some("  "), "example.com"), "example.com");
        assert_eq!(label(Some("Work mail"), "example.com"), "Work mail");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn a_login_is_saved_listed_edited_and_removed(pool: PgPool) {
        ensure_test_key();
        let made = create(&pool, input("example.com/sign-in", "nick@example.com", Some("first-pass")))
            .await
            .unwrap();
        assert_eq!((made.site.as_str(), made.name.as_str()), ("https://example.com", "example.com"));

        let listed = list(&pool).await.unwrap();
        assert_eq!(listed, vec![made]);
        let shown = serde_json::to_string(&listed).unwrap();
        assert!(!shown.contains("first-pass"), "the list must not carry passwords");

        // An edit without a password keeps the saved one.
        let id = listed[0].id.clone();
        let mut edit = input("example.com", "nick@example.com", None);
        edit.notes = Some("Recovery codes are in the safe.".into());
        update(&pool, &id, edit).await.unwrap();
        let secret = reveal(&pool, &id).await.unwrap();
        assert_eq!(secret.password, "first-pass");
        assert_eq!(secret.notes, "Recovery codes are in the safe.");

        update(&pool, &id, input("example.com", "nick@example.com", Some("second-pass"))).await.unwrap();
        let secret = reveal(&pool, &id).await.unwrap();
        assert_eq!(secret.password, "second-pass");
        assert_eq!(secret.notes, "Recovery codes are in the safe.", "notes left out are kept");

        delete(&pool, &id).await.unwrap();
        assert!(list(&pool).await.unwrap().is_empty());
        assert!(matches!(reveal(&pool, &id).await, Err(Error::NotFound(_))));
    }

    /// A login sealed under another key, as after a restore that kept the
    /// box's own key, is listed as unreadable beside the good ones, and the
    /// owner can still remove it.
    #[sqlx::test(migrations = "./migrations")]
    async fn one_unreadable_login_does_not_hide_the_rest(pool: PgPool) {
        use base64::Engine;
        ensure_test_key();
        let good = create(&pool, input("example.com", "nick@example.com", Some("first-pass"))).await.unwrap();

        let other_key = base64::engine::general_purpose::STANDARD.encode([7u8; 32]);
        let other = TokenEncryptor::from_base64_key(&other_key).unwrap();
        let foreign = other
            .encrypt(r#"{"username":"david@example.com","password":"old-pass","notes":""}"#)
            .unwrap();
        sqlx::query(
            "INSERT INTO credentials (id, kind, key, name, status, secrets_ciphertext, metadata)
             VALUES ('login:foreign', 'login', 'https://example.org', 'example.org', 'active', $1, '{}'::jsonb)",
        )
        .bind(&foreign)
        .execute(&pool)
        .await
        .unwrap();

        let listed = list(&pool).await.unwrap();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0], good);
        let bad = &listed[1];
        assert_eq!((bad.id.as_str(), bad.site.as_str(), bad.name.as_str()), ("login:foreign", "https://example.org", "example.org"));
        assert!(bad.unreadable);
        assert!(bad.username.is_empty(), "nothing from the sealed part is guessed");

        assert!(reveal(&pool, "login:foreign").await.is_err());
        delete(&pool, "login:foreign").await.unwrap();
        assert_eq!(list(&pool).await.unwrap(), vec![good]);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn logins_never_reach_other_kinds(pool: PgPool) {
        ensure_test_key();
        crate::secrets::put(&pool, crate::secrets::Kind::Env, "EXAMPLE_KEY", "v", &serde_json::json!({}))
            .await
            .unwrap();
        assert!(list(&pool).await.unwrap().is_empty());
        assert!(matches!(reveal(&pool, "env:EXAMPLE_KEY").await, Err(Error::NotFound(_))));
        delete(&pool, "env:EXAMPLE_KEY").await.unwrap();
        assert!(crate::secrets::get(&pool, crate::secrets::Kind::Env, "EXAMPLE_KEY").await.unwrap().is_some());
        assert!(create(&pool, input("example.com", "nick", Some(""))).await.is_err());
    }
}
