//! Keys in Settings: the owner's sign-ins and named keys
//! (agents/plan/secrets-plan.md, slice 5).
//!
//! The overview carries names only. A key's value leaves the box only through
//! [`reveal_key`], which Settings calls when the owner asks to see it; no
//! tool calls it, so a model learns that a key exists and never its value.
//! Its route also needs the app's owner proof (`api::owner_proof`), so a
//! direct request from another process on the owner's Mac, through the app's
//! loopback port, cannot read a value.
//! Named keys are the `env` secrets (`crate::secrets`) that applets declare.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;

use crate::api::credentials::CredentialListItem;
use crate::error::{Error, Result};
use crate::secrets::{self, Kind};

/// The longest value Settings accepts. A key or token is far shorter; this
/// only stops a pasted file from landing in the table.
const MAX_VALUE_BYTES: usize = 16 * 1024;

#[derive(Debug, Serialize)]
pub struct KeysOverview {
    /// Connected sources' sign-ins. Settings links each to Sources, where it
    /// is reconnected.
    pub sign_ins: Vec<CredentialListItem>,
    /// Every key the owner has set or an applet declares, by name.
    pub keys: Vec<KeyItem>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct KeyItem {
    pub name: String,
    pub is_set: bool,
    /// When the value last changed. `None` for a declared key not yet set.
    pub updated_at: Option<DateTime<Utc>>,
    /// The applets that declare it, by name.
    pub used_by: Vec<String>,
}

pub async fn overview(pool: &PgPool) -> Result<KeysOverview> {
    let sign_ins = crate::api::credentials::list_credentials(pool).await?;
    let set = secrets::list(pool, Kind::Env)
        .await
        .map_err(|e| Error::Database(format!("list keys: {e:#}")))?;
    let set: BTreeMap<String, DateTime<Utc>> = set.into_iter().map(|n| (n.key, n.updated_at)).collect();
    Ok(KeysOverview { sign_ins, keys: merge_keys(&set, crate::applet_templates::secret_declarers()) })
}

/// The set keys and the declared ones as one list, sorted by name.
fn merge_keys(set: &BTreeMap<String, DateTime<Utc>>, mut declarers: BTreeMap<String, Vec<String>>) -> Vec<KeyItem> {
    let mut names: Vec<&String> = set.keys().chain(declarers.keys()).collect();
    names.sort();
    names.dedup();
    let names: Vec<String> = names.into_iter().cloned().collect();
    names
        .into_iter()
        .map(|name| KeyItem {
            is_set: set.contains_key(&name),
            updated_at: set.get(&name).copied(),
            used_by: declarers.remove(&name).unwrap_or_default(),
            name,
        })
        .collect()
}

/// Add or replace a key's value.
pub async fn set_key(pool: &PgPool, name: &str, value: &str) -> Result<()> {
    check_name(name)?;
    // A pasted key often carries a trailing newline, which the provider then
    // rejects with an auth error that does not say why.
    let value = value.trim();
    if value.is_empty() {
        return Err(Error::InvalidInput("A key needs a value. Paste it, then save.".into()));
    }
    if value.len() > MAX_VALUE_BYTES {
        return Err(Error::InvalidInput("That value is too long for a key. Check you pasted the key itself.".into()));
    }
    secrets::put(pool, Kind::Env, name, value, &serde_json::json!({}))
        .await
        .map_err(|e| Error::Database(format!("save {name}: {e:#}")))
}

/// Remove a key. Removing one that is not set is not an error.
pub async fn delete_key(pool: &PgPool, name: &str) -> Result<()> {
    check_name(name)?;
    secrets::delete(pool, Kind::Env, name)
        .await
        .map_err(|e| Error::Database(format!("remove {name}: {e:#}")))
}

/// A key's value, for the owner who asked to see it.
pub async fn reveal_key(pool: &PgPool, name: &str) -> Result<String> {
    check_name(name)?;
    match secrets::get(pool, Kind::Env, name)
        .await
        .map_err(|e| Error::Database(format!("read {name}: {e:#}")))?
    {
        Some((value, _)) => Ok(value),
        None => Err(Error::NotFound(format!("{name} is not set."))),
    }
}

fn check_name(name: &str) -> Result<()> {
    if crate::applet_templates::valid_secret_name(name) {
        Ok(())
    } else {
        Err(Error::InvalidInput(format!(
            "{name} can't be a key name. Use capitals, digits and underscores, starting with a letter, \
             and not a name your server sets itself."
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ensure_test_key() {
        // Sealing needs a vault key; CI has none. Thirty-two zero bytes, as
        // the crypto crate's own tests use, and only when nothing is set.
        if std::env::var("VIRTUES_ENCRYPTION_KEY").is_err() {
            use base64::Engine;
            std::env::set_var(
                "VIRTUES_ENCRYPTION_KEY",
                base64::engine::general_purpose::STANDARD.encode([0u8; 32]),
            );
        }
    }

    #[test]
    fn declared_and_set_keys_are_one_list() {
        let at = Utc::now();
        let set = BTreeMap::from([("GITHUB_TOKEN".to_string(), at), ("OPENAI_API_KEY".to_string(), at)]);
        let declarers = BTreeMap::from([
            ("OPENAI_API_KEY".to_string(), vec!["Morning brief".to_string()]),
            ("WEATHER_KEY".to_string(), vec!["Forecast".to_string()]),
        ]);
        let keys = merge_keys(&set, declarers);
        let summary: Vec<(&str, bool, usize)> =
            keys.iter().map(|k| (k.name.as_str(), k.is_set, k.used_by.len())).collect();
        assert_eq!(summary, [("GITHUB_TOKEN", true, 0), ("OPENAI_API_KEY", true, 1), ("WEATHER_KEY", false, 1)]);
        assert_eq!(keys[2].updated_at, None);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn a_key_is_set_revealed_and_removed(pool: PgPool) {
        ensure_test_key();
        set_key(&pool, "GITHUB_TOKEN", "ghp_test").await.unwrap();
        assert_eq!(reveal_key(&pool, "GITHUB_TOKEN").await.unwrap(), "ghp_test");

        let o = overview(&pool).await.unwrap();
        let k = o.keys.iter().find(|k| k.name == "GITHUB_TOKEN").unwrap();
        assert!(k.is_set && k.updated_at.is_some());
        let shown = serde_json::to_string(&o).unwrap();
        assert!(!shown.contains("ghp_test"), "the overview must carry names only");

        delete_key(&pool, "GITHUB_TOKEN").await.unwrap();
        assert!(matches!(reveal_key(&pool, "GITHUB_TOKEN").await, Err(Error::NotFound(_))));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn names_the_box_uses_and_empty_values_are_refused(pool: PgPool) {
        ensure_test_key();
        for bad in ["DATABASE_URL", "VIRTUES_ENCRYPTION_KEY", "lower", ""] {
            assert!(matches!(set_key(&pool, bad, "x").await, Err(Error::InvalidInput(_))), "{bad}");
        }
        assert!(matches!(set_key(&pool, "GITHUB_TOKEN", "  ").await, Err(Error::InvalidInput(_))));
    }
}
