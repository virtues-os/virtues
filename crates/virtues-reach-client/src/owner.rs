//! Prove to an offline box that this device is one of its own.
//!
//! A box that lost its network (carried to the office, wifi password changed)
//! cannot be reached by relay or LAN, so it opens its Bluetooth service again
//! and asks for its owner (`virtues-improv` RPC `0x88`/`0x89`). This device
//! answers by signing the box's one-time nonce with the SAME iroh key the box
//! allowlisted when it paired — so pairing is the enrollment, there is nothing
//! new to store, and revoking the device in Settings revokes this with it.
//!
//! Signing lives here, beside the key, rather than in the radio code: the seed
//! never leaves this crate, and the Bluetooth client only ever carries the
//! public id and a signature.

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
    let rec = store
        .load()
        .context("read this device's pairing")?
        .ok_or_else(|| anyhow!("This device isn't paired with a server."))?;
    let secret_hex = rec
        .device_secret_hex
        .ok_or_else(|| anyhow!("This device was paired before it had its own key. Pair it again."))?;
    let seed: [u8; 32] = hex::decode(secret_hex.trim())
        .context("decode this device's key")?
        .try_into()
        .map_err(|_| anyhow!("this device's stored key is the wrong length"))?;
    Ok(sign_with_seed(&seed, nonce))
}

fn sign_with_seed(seed: &[u8; 32], nonce: &[u8]) -> OwnerProof {
    let key = virtues_iroh::SecretKey::from_bytes(seed);
    let sig = key.sign(&virtues_improv::owner_proof_message(nonce));
    OwnerProof { endpoint_id: key.public().to_string(), signature_hex: hex::encode(sig.to_bytes()) }
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
        let legacy = paired(None);
        let err = sign_owner_proof(&legacy, &[0]).unwrap_err().to_string();
        assert!(err.contains("Pair it again"), "{err}");
    }
}
