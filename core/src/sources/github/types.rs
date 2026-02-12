//! GitHub API types
//!
//! Deserialization types for the GitHub Events API responses.
//! Based on https://docs.github.com/en/rest/activity/events

use serde::{Deserialize, Serialize};

/// A GitHub event from the Events API
///
/// Represents a single activity event (star, fork, push, PR, etc.)
/// See: https://docs.github.com/en/rest/activity/events#list-events-for-the-authenticated-user
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubEvent {
    pub id: String,
    #[serde(rename = "type")]
    pub event_type: String,
    pub actor: Actor,
    pub repo: Repo,
    pub payload: serde_json::Value,
    pub public: bool,
    pub created_at: String,
    pub org: Option<Org>,
}

/// The actor who performed the event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Actor {
    pub id: i64,
    pub login: String,
    pub avatar_url: Option<String>,
}

/// The repository associated with the event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Repo {
    pub id: i64,
    pub name: String,
    pub url: String,
}

/// The organization associated with the event (if any)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Org {
    pub id: i64,
    pub login: String,
    pub avatar_url: Option<String>,
}

/// Authenticated user response from GET /user
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubUser {
    pub login: String,
    pub id: i64,
    pub avatar_url: Option<String>,
    pub name: Option<String>,
    pub email: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_event() {
        let json = serde_json::json!({
            "id": "12345",
            "type": "WatchEvent",
            "actor": {
                "id": 1,
                "login": "testuser",
                "avatar_url": "https://avatars.githubusercontent.com/u/1"
            },
            "repo": {
                "id": 100,
                "name": "owner/repo",
                "url": "https://api.github.com/repos/owner/repo"
            },
            "payload": {
                "action": "started"
            },
            "public": true,
            "created_at": "2024-01-15T10:30:00Z",
            "org": null
        });

        let event: GitHubEvent = serde_json::from_value(json).unwrap();
        assert_eq!(event.id, "12345");
        assert_eq!(event.event_type, "WatchEvent");
        assert_eq!(event.actor.login, "testuser");
        assert_eq!(event.repo.name, "owner/repo");
    }

    #[test]
    fn test_deserialize_user() {
        let json = serde_json::json!({
            "login": "testuser",
            "id": 1,
            "avatar_url": "https://avatars.githubusercontent.com/u/1",
            "name": "Test User",
            "email": "test@example.com"
        });

        let user: GitHubUser = serde_json::from_value(json).unwrap();
        assert_eq!(user.login, "testuser");
        assert_eq!(user.name, Some("Test User".to_string()));
    }
}
