//! The owner proof: a fresh signature from the Virtues app, asked for on the
//! few routes a local process on the owner's Mac must not reach.
//!
//! Every request from a paired device authenticates by the device's iroh key
//! (`middleware::auth`). On a Mac, the app serves that key's connection on
//! `127.0.0.1:7117`, so any process on the Mac that connects there reaches the
//! box as the Mac. For most routes that is an accepted gap
//! (agents/plan/secrets-plan.md, "Who can read what"). These routes are not
//! among them:
//!
//! - `GET /api/passwords/:id/secret` and `GET /api/keys/:name/value`, which
//!   reveal a secret.
//! - `POST /api/devices/enroll-peer`, `POST /api/pair/mint` and
//!   `POST /api/pair/mint-collector`, which add a device. The collector code
//!   redeems into a full device, so leaving it open would undo the other two.
//! - `POST` and `DELETE /api/settings/byo-key`, because whoever sets the AI key
//!   receives every message the assistant sends out.
//! - `POST /api/admin/applets/import-git`, which runs someone else's code on
//!   the box.
//!
//! On each, the request carries an `X-Virtues-Owner-Proof` header: a nonce
//! from `POST /api/owner-proof/nonce` and the device key's signature over
//! `virtues_improv::protocol::http_owner_proof_message` of the method, the
//! path and that nonce. Only the app holds the key, and it signs natively
//! (`virtues_reach_client::owner::sign_http_owner_proof`), so a process that
//! can only open the loopback port cannot answer. The message has its own
//! prefix, so a Bluetooth owner proof cannot be replayed here.
//!
//! The box's own console (a process on the box's loopback, and the dev
//! fallback) is exempt: it can already read `virtues.env`. An agent device is
//! never accepted. The web terminal (`/ws/terminal`) and a chat in sudo mode
//! do not ask for the proof yet, and both reach that console, so a process on
//! the Mac can still get to these actions through them.
//!
//! Nonces live in memory. The server is one process, a nonce lasts a minute,
//! and a restart only costs the app one retry.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use axum::{
    async_trait,
    extract::{FromRef, FromRequestParts, OriginalUri},
    http::{request::Parts, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use rand::RngCore;
use serde_json::json;
use sqlx::PgPool;
use virtues_improv::protocol::{
    http_owner_proof_message, parse_http_owner_proof_header, HTTP_OWNER_PROOF_HEADER,
};

use crate::middleware::auth::{AuthUser, AGENT_KIND, CONSOLE_DEVICE_ID};

/// How long a nonce can be used. The app asks for one and signs it right
/// away, so a minute is generous.
const NONCE_TTL: Duration = Duration::from_secs(60);

/// Most nonces one device can hold at once. The app asks for one per action
/// and uses it straight away, so a few covers actions that overlap. Past the
/// cap that device's oldest is dropped. A device that floods the route pushes
/// out only its own nonces, never another device's, and the table stays
/// bounded by the number of paired devices.
const MAX_PER_DEVICE: usize = 8;

/// What a refused request is told. Every refusal gets the same words, so the
/// answer says nothing about which part of a proof was wrong.
const REFUSED: &str = "Only the Virtues app on one of your paired devices can do this. Use the app, or update it if you already are.";

/// The nonces issued and not yet used.
#[derive(Default)]
struct Nonces(HashMap<String, Issued>);

struct Issued {
    device_id: String,
    at: Instant,
}

impl Nonces {
    /// A fresh random nonce for `device_id`, as hex.
    fn issue(&mut self, device_id: &str, now: Instant) -> String {
        self.0.retain(|_, i| now.saturating_duration_since(i.at) < NONCE_TTL);
        let mut mine: Vec<(Instant, String)> = self
            .0
            .iter()
            .filter(|(_, i)| i.device_id == device_id)
            .map(|(k, i)| (i.at, k.clone()))
            .collect();
        if mine.len() >= MAX_PER_DEVICE {
            mine.sort();
            for (_, oldest) in &mine[..=mine.len() - MAX_PER_DEVICE] {
                self.0.remove(oldest);
            }
        }
        let mut bytes = [0u8; 32];
        rand::rng().fill_bytes(&mut bytes);
        let nonce = hex::encode(bytes);
        self.0.insert(nonce.clone(), Issued { device_id: device_id.to_string(), at: now });
        nonce
    }

    /// Use `nonce`: true when it was issued to `device_id` less than
    /// [`NONCE_TTL`] ago. It is removed either way, so a nonce gets one try.
    fn take(&mut self, nonce: &str, device_id: &str, now: Instant) -> bool {
        self.0
            .remove(nonce)
            .is_some_and(|i| i.device_id == device_id && now.saturating_duration_since(i.at) < NONCE_TTL)
    }
}

fn nonces() -> &'static Mutex<Nonces> {
    static NONCES: OnceLock<Mutex<Nonces>> = OnceLock::new();
    NONCES.get_or_init(|| Mutex::new(Nonces::default()))
}

/// Who is asking, as far as the proof is concerned.
enum Requester<'a> {
    /// The box's own console.
    Console,
    /// A paired device: its id, the key it paired with, and its kind.
    Device { id: &'a str, endpoint_id: Option<&'a str>, kind: &'a str },
}

/// Whether `header` proves this request for `who`. Uses up the nonce it
/// names, valid or not.
fn accepts(
    nonces: &mut Nonces,
    now: Instant,
    who: Requester<'_>,
    method: &str,
    path: &str,
    header: Option<&str>,
) -> bool {
    let (device_id, endpoint_id) = match who {
        Requester::Console => return true,
        Requester::Device { kind, .. } if kind == AGENT_KIND => return false,
        Requester::Device { id, endpoint_id, .. } => (id, endpoint_id),
    };
    let Some((nonce, signature_hex)) = header.and_then(parse_http_owner_proof_header) else {
        return false;
    };
    if !nonces.take(nonce, device_id, now) {
        return false;
    }
    let Some(key) = endpoint_id.and_then(|e| e.trim().parse::<virtues_iroh::EndpointId>().ok()) else {
        return false;
    };
    let Some(bytes) = hex::decode(signature_hex.trim()).ok().and_then(|b| <[u8; 64]>::try_from(b).ok()) else {
        return false;
    };
    key.verify(
        &http_owner_proof_message(method, path, nonce),
        &virtues_iroh::Signature::from_bytes(&bytes),
    )
    .is_ok()
}

/// Check the owner proof on a request from `user`. `Err` is the response to
/// send instead of doing the action.
pub async fn check(
    pool: &PgPool,
    user: &AuthUser,
    method: &str,
    path: &str,
    headers: &HeaderMap,
) -> Result<(), Response> {
    let console = user.device_id == CONSOLE_DEVICE_ID;
    let device: Option<(Option<String>, String)> = if console {
        None
    } else {
        sqlx::query_as("SELECT endpoint_id, kind FROM app_device WHERE id = $1 AND revoked_at IS NULL")
            .bind(&user.device_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "owner proof: device lookup failed");
                (
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(json!({ "error": "Your server couldn't check this device. Try again." })),
                )
                    .into_response()
            })?
    };
    let who = if console {
        Requester::Console
    } else if let Some((endpoint_id, kind)) = &device {
        Requester::Device { id: &user.device_id, endpoint_id: endpoint_id.as_deref(), kind }
    } else {
        // Revoked between authentication and here.
        return Err(refused());
    };
    let header = headers.get(HTTP_OWNER_PROOF_HEADER).and_then(|v| v.to_str().ok());
    let ok = {
        let mut nonces = nonces().lock().expect("owner proof nonces lock poisoned");
        accepts(&mut nonces, Instant::now(), who, method, path, header)
    };
    if ok {
        Ok(())
    } else {
        tracing::warn!(device = %user.device_id, method, path, has_proof = header.is_some(), "owner proof refused");
        Err(refused())
    }
}

fn refused() -> Response {
    (
        StatusCode::FORBIDDEN,
        Json(json!({ "error": REFUSED, "code": "owner_proof_required" })),
    )
        .into_response()
}

/// An authenticated owner whose request carried a valid owner proof. Take it
/// in place of [`AuthUser`] on a route that needs the proof.
pub struct OwnerProven(pub AuthUser);

#[async_trait]
impl<S> FromRequestParts<S> for OwnerProven
where
    PgPool: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let user = AuthUser::from_request_parts(parts, state)
            .await
            .map_err(IntoResponse::into_response)?;
        // The path the client signed is the one it sent, before any nesting
        // stripped a prefix.
        let path = parts
            .extensions
            .get::<OriginalUri>()
            .map(|u| u.0.path().to_string())
            .unwrap_or_else(|| parts.uri.path().to_string());
        check(&PgPool::from_ref(state), &user, parts.method.as_str(), &path, &parts.headers).await?;
        Ok(OwnerProven(user))
    }
}

/// `POST /api/owner-proof/nonce` — a one-time nonce for the next proof from
/// this device.
pub async fn nonce_handler(user: AuthUser) -> Response {
    let nonce = nonces()
        .lock()
        .expect("owner proof nonces lock poisoned")
        .issue(&user.device_id, Instant::now());
    Json(json!({ "nonce": nonce, "expires_in_seconds": NONCE_TTL.as_secs() })).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use virtues_iroh::SecretKey;

    const PATH: &str = "/api/passwords/login_1/secret";

    fn key(seed: u8) -> SecretKey {
        SecretKey::from_bytes(&[seed; 32])
    }

    fn proof(key: &SecretKey, method: &str, path: &str, nonce: &str) -> String {
        let sig = key.sign(&http_owner_proof_message(method, path, nonce));
        virtues_improv::protocol::http_owner_proof_header(nonce, &hex::encode(sig.to_bytes()))
    }

    fn mac(endpoint_id: &str) -> Requester<'_> {
        Requester::Device { id: "dev_mac", endpoint_id: Some(endpoint_id), kind: "desktop_app" }
    }

    #[test]
    fn a_good_proof_is_accepted() {
        let mut n = Nonces::default();
        let now = Instant::now();
        let k = key(1);
        let id = k.public().to_string();
        let nonce = n.issue("dev_mac", now);
        let h = proof(&k, "GET", PATH, &nonce);
        assert!(accepts(&mut n, now, mac(&id), "GET", PATH, Some(&h)));
    }

    #[test]
    fn a_nonce_works_once() {
        let mut n = Nonces::default();
        let now = Instant::now();
        let k = key(1);
        let id = k.public().to_string();
        let nonce = n.issue("dev_mac", now);
        let h = proof(&k, "GET", PATH, &nonce);
        assert!(accepts(&mut n, now, mac(&id), "GET", PATH, Some(&h)));
        assert!(!accepts(&mut n, now, mac(&id), "GET", PATH, Some(&h)), "a replay is refused");

        // A failed try uses the nonce up too, so a guess gets one attempt.
        let nonce = n.issue("dev_mac", now);
        let wrong = proof(&key(2), "GET", PATH, &nonce);
        assert!(!accepts(&mut n, now, mac(&id), "GET", PATH, Some(&wrong)));
        let right = proof(&k, "GET", PATH, &nonce);
        assert!(!accepts(&mut n, now, mac(&id), "GET", PATH, Some(&right)));
    }

    #[test]
    fn an_expired_nonce_is_refused() {
        let mut n = Nonces::default();
        let then = Instant::now();
        let k = key(1);
        let id = k.public().to_string();
        let nonce = n.issue("dev_mac", then);
        let h = proof(&k, "GET", PATH, &nonce);
        let later = then + NONCE_TTL + Duration::from_secs(1);
        assert!(!accepts(&mut n, later, mac(&id), "GET", PATH, Some(&h)));
    }

    #[test]
    fn another_devices_key_or_nonce_is_refused() {
        let mut n = Nonces::default();
        let now = Instant::now();
        let (mine, theirs) = (key(1), key(2));
        let id = mine.public().to_string();

        // Signed by a key this device did not pair with.
        let nonce = n.issue("dev_mac", now);
        let h = proof(&theirs, "GET", PATH, &nonce);
        assert!(!accepts(&mut n, now, mac(&id), "GET", PATH, Some(&h)));

        // A nonce issued to another device, even signed with the right key.
        let nonce = n.issue("dev_phone", now);
        let h = proof(&mine, "GET", PATH, &nonce);
        assert!(!accepts(&mut n, now, mac(&id), "GET", PATH, Some(&h)));
    }

    #[test]
    fn a_proof_opens_only_the_request_it_was_made_for() {
        let mut n = Nonces::default();
        let now = Instant::now();
        let k = key(1);
        let id = k.public().to_string();

        let nonce = n.issue("dev_mac", now);
        let h = proof(&k, "GET", PATH, &nonce);
        assert!(!accepts(&mut n, now, mac(&id), "GET", "/api/passwords/login_2/secret", Some(&h)));

        let nonce = n.issue("dev_mac", now);
        let h = proof(&k, "GET", "/api/settings/byo-key", &nonce);
        assert!(!accepts(&mut n, now, mac(&id), "DELETE", "/api/settings/byo-key", Some(&h)));

        // The Bluetooth proof signs other bytes and is no use here.
        let nonce = n.issue("dev_mac", now);
        let ble = k.sign(&virtues_improv::owner_proof_message(nonce.as_bytes()));
        let h = virtues_improv::protocol::http_owner_proof_header(&nonce, &hex::encode(ble.to_bytes()));
        assert!(!accepts(&mut n, now, mac(&id), "GET", PATH, Some(&h)));
    }

    #[test]
    fn a_missing_or_malformed_proof_is_refused() {
        let mut n = Nonces::default();
        let now = Instant::now();
        let id = key(1).public().to_string();
        assert!(!accepts(&mut n, now, mac(&id), "GET", PATH, None));
        assert!(!accepts(&mut n, now, mac(&id), "GET", PATH, Some("not-a-proof")));
        let nonce = n.issue("dev_mac", now);
        assert!(!accepts(&mut n, now, mac(&id), "GET", PATH, Some(&format!("{nonce}.zz"))));
        // A device row without a key cannot prove anything.
        let nonce = n.issue("dev_mac", now);
        let h = proof(&key(1), "GET", PATH, &nonce);
        let keyless = Requester::Device { id: "dev_mac", endpoint_id: None, kind: "desktop_app" };
        assert!(!accepts(&mut n, now, keyless, "GET", PATH, Some(&h)));
    }

    #[test]
    fn the_console_is_exempt() {
        let mut n = Nonces::default();
        assert!(accepts(&mut n, Instant::now(), Requester::Console, "GET", PATH, None));
    }

    #[test]
    fn an_agent_is_refused_even_with_a_valid_signature() {
        let mut n = Nonces::default();
        let now = Instant::now();
        let k = key(1);
        let id = k.public().to_string();
        let nonce = n.issue("dev_agent", now);
        let h = proof(&k, "GET", PATH, &nonce);
        let agent = Requester::Device { id: "dev_agent", endpoint_id: Some(&id), kind: AGENT_KIND };
        assert!(!accepts(&mut n, now, agent, "GET", PATH, Some(&h)));
    }

    /// A device that floods the nonce route, such as a process on the Mac
    /// speaking as the Mac, pushes out its own nonces and nobody else's.
    #[test]
    fn a_flood_from_one_device_leaves_the_others_alone() {
        let mut n = Nonces::default();
        let now = Instant::now();
        let phone_key = key(3);
        let phone_id = phone_key.public().to_string();
        let phone_nonce = n.issue("dev_phone", now);
        for i in 0..(MAX_PER_DEVICE * 4) {
            n.issue("dev_mac", now + Duration::from_millis(i as u64));
        }
        let macs = n.0.values().filter(|i| i.device_id == "dev_mac").count();
        assert_eq!(macs, MAX_PER_DEVICE);

        let h = proof(&phone_key, "GET", PATH, &phone_nonce);
        let phone = Requester::Device { id: "dev_phone", endpoint_id: Some(&phone_id), kind: "mobile_app" };
        assert!(accepts(&mut n, now, phone, "GET", PATH, Some(&h)));

        // The newest of the flooding device's own nonces still works.
        let mac_key = key(1);
        let mac_id = mac_key.public().to_string();
        let latest = n.issue("dev_mac", now + Duration::from_secs(1));
        let h = proof(&mac_key, "GET", PATH, &latest);
        assert!(accepts(&mut n, now + Duration::from_secs(1), mac(&mac_id), "GET", PATH, Some(&h)));
    }

    fn headers(h: &str) -> HeaderMap {
        let mut m = HeaderMap::new();
        m.insert(HTTP_OWNER_PROOF_HEADER, h.parse().unwrap());
        m
    }

    fn user(device_id: &str) -> AuthUser {
        AuthUser {
            id: crate::middleware::http::OWNER_USER_ID.to_string(),
            device_id: device_id.to_string(),
            device_label: "test".to_string(),
        }
    }

    /// The box looks up the requesting device's own key and kind, and the
    /// console needs no proof and no row.
    #[sqlx::test(migrations = "./migrations")]
    async fn the_box_checks_against_the_devices_stored_key(pool: PgPool) {
        for (id, kind, seed) in [("dev_mac", "desktop_app", 11u8), ("dev_agent", AGENT_KIND, 12u8)] {
            sqlx::query(
                "INSERT INTO app_device (id, user_id, kind, label, endpoint_id) VALUES ($1, $2, $3, 'test', $4)",
            )
            .bind(id)
            .bind(crate::middleware::http::OWNER_USER_ID)
            .bind(kind)
            .bind(key(seed).public().to_string())
            .execute(&pool)
            .await
            .unwrap();
        }
        let issue = |device: &str| nonces().lock().unwrap().issue(device, Instant::now());

        let nonce = issue("dev_mac");
        let h = headers(&proof(&key(11), "GET", PATH, &nonce));
        assert!(check(&pool, &user("dev_mac"), "GET", PATH, &h).await.is_ok());

        let nonce = issue("dev_mac");
        let h = headers(&proof(&key(12), "GET", PATH, &nonce));
        let refused = check(&pool, &user("dev_mac"), "GET", PATH, &h).await.unwrap_err();
        assert_eq!(refused.status(), StatusCode::FORBIDDEN);

        let nonce = issue("dev_agent");
        let h = headers(&proof(&key(12), "GET", PATH, &nonce));
        assert!(check(&pool, &user("dev_agent"), "GET", PATH, &h).await.is_err());

        // Revoked: the row no longer answers.
        sqlx::query("UPDATE app_device SET revoked_at = now() WHERE id = 'dev_mac'")
            .execute(&pool)
            .await
            .unwrap();
        let nonce = issue("dev_mac");
        let h = headers(&proof(&key(11), "GET", PATH, &nonce));
        assert!(check(&pool, &user("dev_mac"), "GET", PATH, &h).await.is_err());

        assert!(check(&pool, &user(CONSOLE_DEVICE_ID), "GET", PATH, &HeaderMap::new()).await.is_ok());
    }
}
