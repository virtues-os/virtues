//! Prove to a box that this device is one of its own.
//!
//! Two places ask. A box that lost its network (carried to the office, wifi
//! password changed) cannot be reached by relay or LAN, so it opens its
//! Bluetooth service again and asks for its owner (`virtues-improv` RPC
//! `0x88`/`0x89`). And a box that is reachable asks for a proof on the few
//! routes a local process on this machine must not reach through the app's
//! loopback port: revealing a password or a key, adding a device, changing the
//! AI key, importing an applet package ([`sign_http_owner_proof`]).
//!
//! Both answers sign the box's one-time nonce with the SAME iroh key the box
//! allowlisted when it paired, so pairing is the enrollment, there is nothing
//! new to store, and revoking the device in Settings revokes this with it.
//! Each signs a differently prefixed message, so one proof cannot stand in for
//! the other.
//!
//! Signing lives here, beside the key, rather than in the radio or HTTP code:
//! the seed never leaves this crate, and callers only ever carry the public id
//! and a signature.

use anyhow::{anyhow, Context, Result};

use crate::store::BoxStore;

/// What goes over the air in `0x89`: both hex.
#[derive(Debug, Clone)]
pub struct OwnerProof {
    pub endpoint_id: String,
    pub signature_hex: String,
}

/// Sign `nonce` (the raw bytes from `0x88`) with this device's paired key.
///
/// Fails when this device is not paired, or was paired before devices had
/// their own keys (`device_secret_hex` is `None` on those records) — neither
/// can prove anything, and re-pairing is the only fix for the second.
pub fn sign_owner_proof(store: &dyn BoxStore, nonce: &[u8]) -> Result<OwnerProof> {
    let seed = paired_seed(store)?;
    Ok(sign_with_seed(&seed, &virtues_improv::owner_proof_message(nonce)))
}

/// Sign one HTTP request for the box: `method` and `path` exactly as the
/// request will carry them (path without the query string), and the `nonce`
/// the box issued at `POST /api/owner-proof/nonce`. Returns the value for the
/// [`virtues_improv::protocol::HTTP_OWNER_PROOF_HEADER`] header.
///
/// Fails for the same reasons as [`sign_owner_proof`].
pub fn sign_http_owner_proof(store: &dyn BoxStore, method: &str, path: &str, nonce: &str) -> Result<String> {
    use virtues_improv::protocol::{http_owner_proof_header, http_owner_proof_message};
    let seed = paired_seed(store)?;
    let proof = sign_with_seed(&seed, &http_owner_proof_message(method, path, nonce));
    Ok(http_owner_proof_header(nonce, &proof.signature_hex))
}

/// This device's iroh seed, from its pairing record.
fn paired_seed(store: &dyn BoxStore) -> Result<[u8; 32]> {
    let rec = store
        .load()
        .context("read this device's pairing")?
        .ok_or_else(|| anyhow!("This device isn't paired with a server."))?;
    let secret_hex = rec
        .device_secret_hex
        .ok_or_else(|| anyhow!("This device was paired before it had its own key. Pair it again."))?;
    hex::decode(secret_hex.trim())
        .context("decode this device's key")?
        .try_into()
        .map_err(|_| anyhow!("this device's stored key is the wrong length"))
}

fn sign_with_seed(seed: &[u8; 32], message: &[u8]) -> OwnerProof {
    let key = virtues_iroh::SecretKey::from_bytes(seed);
    let sig = key.sign(message);
    OwnerProof { endpoint_id: key.public().to_string(), signature_hex: hex::encode(sig.to_bytes()) }
}

/// The raw HTTP request that asks the box for a proof nonce. Raw bytes because
/// the Rust callers reach the box through the warm iroh client's `request`,
/// which speaks HTTP/1 over a bi-stream.
pub fn nonce_request() -> Vec<u8> {
    b"POST /api/owner-proof/nonce HTTP/1.1\r\nHost: virtues\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        .to_vec()
}

/// The nonce from the box's answer to [`nonce_request`]. `Ok(None)` when the
/// box has no such route (404), which a server older than the proof answers:
/// it does not ask for one either, so the caller sends its request without.
pub fn parse_nonce_response(status: u16, body: &str) -> Result<Option<String>> {
    if status == 404 {
        return Ok(None);
    }
    if status != 200 {
        tracing::warn!(status, body, "owner proof: the box refused to issue a nonce");
        // The box's own `error` sentence when it sent one; the raw body only
        // goes to the log.
        let said = serde_json::from_str::<serde_json::Value>(body)
            .ok()
            .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(str::to_string));
        return Err(match said {
            Some(said) => anyhow!("{said}"),
            None => anyhow!("Your server couldn't start the check. Try again."),
        });
    }
    let v: serde_json::Value = serde_json::from_str(body).context("decode the proof nonce")?;
    let nonce = v
        .get("nonce")
        .and_then(|n| n.as_str())
        .filter(|n| !n.is_empty())
        .ok_or_else(|| anyhow!("Your server sent an answer this app couldn't read. Update the app."))?;
    Ok(Some(nonce.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::PairedBox;
    use std::sync::Mutex;

    struct Mem(Mutex<Option<PairedBox>>);
    impl BoxStore for Mem {
        fn load(&self) -> Result<Option<PairedBox>> {
            Ok(self.0.lock().unwrap().clone())
        }
        fn save(&self, rec: &PairedBox) -> Result<()> {
            *self.0.lock().unwrap() = Some(rec.clone());
            Ok(())
        }
        fn delete(&self) -> Result<()> {
            *self.0.lock().unwrap() = None;
            Ok(())
        }
    }

    fn paired(secret: Option<String>) -> Mem {
        Mem(Mutex::new(Some(PairedBox {
            box_url: "http://192.168.1.50:8000".into(),
            device_id: None,
            box_node_id: None,
            relay_url: None,
            box_direct_addrs: vec![],
            device_secret_hex: secret,
            applet_ids: Default::default(),
        })))
    }

    #[test]
    fn the_proof_is_made_with_the_key_the_box_allowlisted() {
        // The box admits the proof by checking `endpoint_id` against the ids it
        // stored at pairing — so the id here must be the one pairing sent,
        // which is derived from this same seed.
        let seed = [5u8; 32];
        let store = paired(Some(hex::encode(seed)));
        let proof = sign_owner_proof(&store, &[1, 2, 3]).unwrap();
        let expected = virtues_iroh::SecretKey::from_bytes(&seed).public();
        assert_eq!(proof.endpoint_id, expected.to_string());

        let sig_bytes: [u8; 64] = hex::decode(&proof.signature_hex).unwrap().try_into().unwrap();
        let sig = virtues_iroh::Signature::from_bytes(&sig_bytes);
        expected
            .verify(&virtues_improv::owner_proof_message(&[1, 2, 3]), &sig)
            .expect("the box's verification must accept it");
    }

    #[test]
    fn an_unpaired_or_keyless_device_cannot_prove_anything() {
        let none = Mem(Mutex::new(None));
        assert!(sign_owner_proof(&none, &[0]).is_err());
        assert!(sign_http_owner_proof(&none, "GET", "/api/keys/X/value", "ab").is_err());
        let legacy = paired(None);
        let err = sign_owner_proof(&legacy, &[0]).unwrap_err().to_string();
        assert!(err.contains("Pair it again"), "{err}");
        let err = sign_http_owner_proof(&legacy, "GET", "/api/keys/X/value", "ab").unwrap_err().to_string();
        assert!(err.contains("Pair it again"), "{err}");
    }

    #[test]
    fn the_http_proof_verifies_against_the_allowlisted_key_for_its_request_only() {
        use virtues_improv::protocol::{http_owner_proof_message, parse_http_owner_proof_header};
        let seed = [6u8; 32];
        let store = paired(Some(hex::encode(seed)));
        let header = sign_http_owner_proof(&store, "GET", "/api/passwords/login_1/secret", "00ff").unwrap();
        let (nonce, sig_hex) = parse_http_owner_proof_header(&header).unwrap();
        assert_eq!(nonce, "00ff");

        let sig_bytes: [u8; 64] = hex::decode(sig_hex).unwrap().try_into().unwrap();
        let sig = virtues_iroh::Signature::from_bytes(&sig_bytes);
        let public = virtues_iroh::SecretKey::from_bytes(&seed).public();
        public
            .verify(&http_owner_proof_message("GET", "/api/passwords/login_1/secret", "00ff"), &sig)
            .expect("the box's verification must accept it");
        assert!(public
            .verify(&http_owner_proof_message("GET", "/api/passwords/login_2/secret", "00ff"), &sig)
            .is_err());
        assert!(public.verify(&virtues_improv::owner_proof_message(b"00ff"), &sig).is_err());
    }

    #[test]
    fn an_old_server_without_the_nonce_route_asks_for_no_proof() {
        assert_eq!(parse_nonce_response(404, "").unwrap(), None);
        assert_eq!(parse_nonce_response(200, r#"{"nonce":"ab12"}"#).unwrap().as_deref(), Some("ab12"));
        assert!(parse_nonce_response(403, "{}").is_err());
        // The owner sees the box's sentence or a plain one, never the raw body.
        let said = parse_nonce_response(503, r#"{"error":"Your server couldn't check this device. Try again."}"#);
        assert_eq!(said.unwrap_err().to_string(), "Your server couldn't check this device. Try again.");
        let raw = parse_nonce_response(502, "<html>bad gateway</html>").unwrap_err().to_string();
        assert!(!raw.contains("html"), "{raw}");
        assert!(parse_nonce_response(200, r#"{"nonce":""}"#).is_err());
        let raw = String::from_utf8(nonce_request()).unwrap();
        assert!(raw.starts_with("POST /api/owner-proof/nonce HTTP/1.1\r\n"));
        assert!(raw.ends_with("\r\n\r\n"));
    }
}
