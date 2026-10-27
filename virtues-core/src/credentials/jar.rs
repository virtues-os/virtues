//! A source's browser session, as the Mac app hands it over after a sign-in.
//!
//! The connect form posts the source's named fields and, beside them, `jar`:
//! every cookie of the sign-in with its domain, path, expiry and flags
//! (`apps/web/src-tauri/src/browser.rs` `JarCookie`). A browser on the box
//! loads `jar` as it is. `domain` arrives without a leading dot, because the
//! Mac's cookie store does not keep whether a cookie was host-only: load a
//! `__Host-` cookie as host-only (no domain) and the rest as domain cookies.
//! Replay applets read their header or named cookies from it, and fall back
//! to the named fields a shell older than `jar` sent.

use chrono::{DateTime, Utc};
use serde_json::Value;

/// Cloudflare's cookies belong to the browser that earned them. The box makes
/// its own requests, from its own browser or client, and gets its own.
const CLOUDFLARE: [&str; 4] = ["cf_clearance", "__cf_bm", "__cflb", "_cfuvid"];

/// Drop Cloudflare's cookies from `fields.jar`, if there is one.
pub fn clean(fields: &mut Value) {
    if let Some(jar) = fields.get_mut("jar").and_then(Value::as_array_mut) {
        jar.retain(|c| {
            let name = c.get("name").and_then(Value::as_str).unwrap_or("");
            !CLOUDFLARE.contains(&name)
        });
    }
}

/// When the session ends: the earliest expiry among the cookies that mean
/// "logged in". `None` when there is no jar, a login cookie is missing from
/// it, or one of them lasts only as long as the browser session, because then
/// no date can be promised.
pub fn login_expiry(fields: &Value, login_cookies: &[String]) -> Option<DateTime<Utc>> {
    let jar = fields.get("jar")?.as_array()?;
    let mut earliest: Option<DateTime<Utc>> = None;
    for name in login_cookies {
        let cookie = jar.iter().find(|c| c.get("name").and_then(Value::as_str) == Some(name))?;
        let secs = cookie.get("expires").and_then(Value::as_i64)?;
        let at = DateTime::from_timestamp(secs, 0)?;
        earliest = Some(earliest.map_or(at, |e| e.min(at)));
    }
    earliest
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn names(fields: &Value) -> Vec<&str> {
        fields["jar"].as_array().unwrap().iter().map(|c| c["name"].as_str().unwrap()).collect()
    }

    #[test]
    fn cloudflare_cookies_are_dropped_and_the_rest_kept() {
        let mut fields = json!({
            "cookies": "sso=a; cf_clearance=b",
            "jar": [{"name": "sso", "value": "a"}, {"name": "cf_clearance", "value": "b"},
                    {"name": "__cf_bm", "value": "c"}, {"name": "device", "value": "d"}]
        });
        clean(&mut fields);
        assert_eq!(names(&fields), ["sso", "device"]);
        assert_eq!(fields["cookies"], "sso=a; cf_clearance=b", "named fields are left as sent");

        let mut no_jar = json!({"auth_token": "x"});
        clean(&mut no_jar);
        assert_eq!(no_jar, json!({"auth_token": "x"}));
    }

    #[test]
    fn the_session_ends_with_its_earliest_login_cookie() {
        let fields = json!({"jar": [
            {"name": "sso", "value": "a", "expires": 1_900_000_000},
            {"name": "sso-rw", "value": "b", "expires": 1_800_000_000},
            {"name": "theme", "value": "dark", "expires": 1_700_000_000}
        ]});
        let login = ["sso".to_string(), "sso-rw".to_string()];
        assert_eq!(login_expiry(&fields, &login), DateTime::from_timestamp(1_800_000_000, 0));
    }

    #[test]
    fn no_date_is_promised_without_one_for_every_login_cookie() {
        let login = ["sso".to_string(), "sso-rw".to_string()];
        let session_only = json!({"jar": [
            {"name": "sso", "value": "a", "expires": 1_900_000_000},
            {"name": "sso-rw", "value": "b"}
        ]});
        assert_eq!(login_expiry(&session_only, &login), None);
        let missing = json!({"jar": [{"name": "sso", "value": "a", "expires": 1_900_000_000}]});
        assert_eq!(login_expiry(&missing, &login), None);
        assert_eq!(login_expiry(&json!({"sso": "a"}), &login), None);
    }
}
