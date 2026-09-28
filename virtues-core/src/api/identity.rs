//! `GET /api/box/identity` — who is this box, publicly.
//!
//! Exists for the two-boxes-on-one-LAN problem, seen live 2026-08-10: a prod
//! and a test box in one house rendered as two identical "Virtues box" chips
//! in the app, distinguishable only by IP. The subnet scan probes an endpoint
//! that proves *a* box exists without learning anything about it; this is the
//! endpoint it asks next.
//!
//! Deliberately public (LAN-readable, pre-auth) and deliberately tiny. The
//! name is a label the box already broadcasts in its AP SSID and BLE
//! advertisement — a stranger on the LAN learns nothing the airwaves don't
//! already say. `claimed` is already public via `/api/setup/state`. Version
//! and everything else stay off this surface; discovery needs a name and a
//! state, not a fingerprint.
//!
//! `linked` is the same kind of fact and earns its place the same way: the
//! app's setup flow has to know whether the box still needs step 2, and the
//! alternative is asking the person to read their own hardware and report
//! back. It is one bit — "does an account key exist" — never the key, never
//! the account, never the code. A stranger learns that a box they can already
//! see is or isn't paying for relay reach, which changes nothing they could
//! do about it.

use axum::{extract::State, response::IntoResponse, Json};

use crate::server::AppState;

/// The owner's name and the assistant's, each if set.
async fn names(pool: &sqlx::PgPool) -> (Option<String>, Option<String>) {
    let person = match sqlx::query_scalar::<_, Option<String>>(
        "SELECT preferred_name FROM app_user_profile LIMIT 1",
    )
    .fetch_optional(pool)
    .await
    {
        Ok(v) => v.flatten(),
        Err(e) => {
            tracing::warn!(error = %e, "box label: couldn't read the owner's name");
            None
        }
    };
    let assistant = crate::api::assistant_profile::get_assistant_name(pool).await.ok();
    let set = |s: Option<String>| s.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    (set(person), set(assistant))
}

/// What people call this box in a list: the app's chooser, Settings, the
/// atlas link page. "Virtues 4812" until it has an owner; after, the
/// owner's: "Adam's server". The owner, not the assistant, because most
/// people keep the default assistant name, and "Ari's server" in every
/// house tells no two servers apart (2026-09-28). "Ari's server" only until
/// the owner has said what to call them.
///
/// The radio name stays machine-shaped (`Virtues-4812`, `setup_ap::ap_ssid`):
/// a claimed box only advertises while it is offline, and an apostrophe has
/// no business in an SSID.
pub async fn box_label(pool: &sqlx::PgPool) -> String {
    if crate::api::pair::is_unclaimed(pool).await {
        return format!("Virtues {}", crate::codename::box_number());
    }
    let (person, assistant) = names(pool).await;
    list_name(person.as_deref(), assistant.as_deref())
}

fn list_name(person: Option<&str>, assistant: Option<&str>) -> String {
    match (person, assistant) {
        (Some(p), _) => format!("{p}'s server"),
        (None, Some(a)) => format!("{a}'s server"),
        (None, None) => "Your server".into(),
    }
}

fn face_name(person: Option<&str>, assistant: Option<&str>) -> String {
    match (person, assistant) {
        (Some(p), Some(a)) => format!("{p} · {a}"),
        (Some(one), None) | (None, Some(one)) => one.to_string(),
        (None, None) => "Your server".into(),
    }
}

/// What the box's own screen says: "Virtues 4812" until it has an owner,
/// then the person and the assistant side by side, "Adam · Ari". The glass
/// is where the pairing is personal rather than administrative; lists use
/// `box_label`.
pub async fn face_label(pool: &sqlx::PgPool) -> String {
    if crate::api::pair::is_unclaimed(pool).await {
        return format!("Virtues {}", crate::codename::box_number());
    }
    let (person, assistant) = names(pool).await;
    face_name(person.as_deref(), assistant.as_deref())
}

pub async fn identity_handler(State(state): State<AppState>) -> impl IntoResponse {
    let pool = state.db.pool();
    Json(serde_json::json!({
        // Machine-shaped ("virtues-4812") and for people ("Virtues 4812",
        // then "Ari's server"). Older apps read `label` as it is.
        "name": format!("virtues-{}", crate::codename::box_number()),
        "label": box_label(pool).await,
        // Fails CLOSED — a DB blip must not tell the LAN this box is unclaimed.
        "claimed": !crate::api::pair::is_unclaimed(pool).await,
        // Setup's step 2, as one bit — see the module docs.
        "linked": crate::virtues_api::renew::read_api_key(pool)
            .await
            .ok()
            .flatten()
            .is_some(),
        "online": crate::cli::link::has_internet(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_list_names_the_owner_first() {
        assert_eq!(list_name(Some("Nick"), Some("Ari")), "Nick's server");
        assert_eq!(list_name(None, Some("Ari")), "Ari's server", "until the owner has a name");
        assert_eq!(list_name(None, None), "Your server");
    }

    #[test]
    fn the_screen_sets_person_and_assistant_side_by_side() {
        assert_eq!(face_name(Some("Nick"), Some("Juno")), "Nick · Juno");
        assert_eq!(face_name(None, Some("Ari")), "Ari");
        assert_eq!(face_name(None, None), "Your server");
    }
}
