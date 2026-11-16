//! The HTTP owner proof: a fresh signature, with the device's iroh key, on
//! one request.
//!
//! A few server routes (revealing a password or a key, adding a device,
//! changing the AI key, importing an applet package) need a fresh signature
//! from the app itself, because any process on a paired Mac can reach the
//! server as that Mac through the app's loopback port, while only the app
//! holds the device's key. The server issues a one-time nonce, the app signs
//! [`http_owner_proof_message`] with the key the server allowlisted at
//! pairing, and the request carries the result in [`HTTP_OWNER_PROOF_HEADER`].
//! The server and the apps' signer both build the message here.

/// Domain separation for the HTTP owner proof, so a signature made for it
/// can't be mistaken for any other use of the same key.
pub const HTTP_OWNER_PROOF_CONTEXT: &[u8] = b"virtues/http/owner-proof/v1\n";

/// The request header that carries an HTTP owner proof, as
/// [`http_owner_proof_header`] formats it.
pub const HTTP_OWNER_PROOF_HEADER: &str = "x-virtues-owner-proof";

/// The exact bytes a device signs to prove one HTTP request: the context, the
/// method, the path, and the nonce the server issued, one per line. The
/// method and path bind the signature to the one request it was made for, so
/// a proof for revealing one password cannot open another.
///
/// `path` is the request path as sent, without the query string.
pub fn http_owner_proof_message(method: &str, path: &str, nonce: &str) -> Vec<u8> {
    let mut m = Vec::with_capacity(HTTP_OWNER_PROOF_CONTEXT.len() + method.len() + path.len() + nonce.len() + 2);
    m.extend_from_slice(HTTP_OWNER_PROOF_CONTEXT);
    m.extend_from_slice(method.to_ascii_uppercase().as_bytes());
    m.push(b'\n');
    m.extend_from_slice(path.as_bytes());
    m.push(b'\n');
    m.extend_from_slice(nonce.as_bytes());
    m
}

/// The header value for a proof: the nonce and the signature, both hex,
/// joined by a dot.
pub fn http_owner_proof_header(nonce: &str, signature_hex: &str) -> String {
    format!("{nonce}.{signature_hex}")
}

/// Split a header value into `(nonce, signature_hex)`. `None` when either half
/// is missing.
pub fn parse_http_owner_proof_header(value: &str) -> Option<(&str, &str)> {
    let (nonce, signature) = value.trim().split_once('.')?;
    (!nonce.is_empty() && !signature.is_empty()).then_some((nonce, signature))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_message_layout_is_pinned() {
        // A Swift or JS signer would have to reproduce it byte for byte.
        let http = http_owner_proof_message("GET", "/api/keys/X/value", "0a0b0c");
        assert_eq!(http, b"virtues/http/owner-proof/v1\nGET\n/api/keys/X/value\n0a0b0c");
        // The method is compared as the server sees it, whatever case the app used.
        assert_eq!(http, http_owner_proof_message("get", "/api/keys/X/value", "0a0b0c"));
    }

    #[test]
    fn the_header_round_trips() {
        let h = http_owner_proof_header("ab12", "cd34");
        assert_eq!(parse_http_owner_proof_header(&h), Some(("ab12", "cd34")));
        assert_eq!(parse_http_owner_proof_header("ab12"), None);
        assert_eq!(parse_http_owner_proof_header(".cd34"), None);
        assert_eq!(parse_http_owner_proof_header("ab12."), None);
    }
}
