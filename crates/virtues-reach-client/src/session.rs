//! Reachability probe — is the box answering *and* does it recognize us?
//!
//! Sends `GET /auth/session` over the warm iroh client and classifies the reply
//! the same way the desktop connect screen does: an authenticated session, an
//! explicit rejection (re-pair), or indeterminate (box unreachable / no body).

use virtues_iroh::VirtuesIrohClient;

/// Outcome of a `/auth/session` probe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    /// Box answered and recognizes this device — load it.
    Authed,
    /// Box answered but rejected us (401 / `"user":null`) — creds stale, re-pair.
    Rejected,
    /// No usable answer (box unreachable, timeout, no body) — retry later.
    Unknown,
}

/// Probe `/auth/session` over the iroh client.
pub async fn probe_session(client: &VirtuesIrohClient) -> SessionState {
    let raw = b"GET /auth/session HTTP/1.1\r\nHost: box\r\nConnection: close\r\n\r\n";
    match client.request(raw).await {
        Ok(bytes) => classify(&String::from_utf8_lossy(&bytes)),
        Err(_) => SessionState::Unknown,
    }
}

/// Classify a raw `/auth/session` HTTP/1 response. Parses only the body (after
/// the header/body split) to avoid header false-matches; falls back to the
/// status line for a 401.
pub fn classify(raw: &str) -> SessionState {
    let (head, body) = match raw.split_once("\r\n\r\n") {
        Some((h, b)) => (h, b),
        None => (raw, ""),
    };
    if body.contains("\"user\":{") {
        return SessionState::Authed;
    }
    if body.contains("\"user\":null") {
        return SessionState::Rejected;
    }
    let status_line = head.lines().next().unwrap_or("");
    if status_line.contains(" 401") {
        return SessionState::Rejected;
    }
    SessionState::Unknown
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authed_body() {
        assert_eq!(
            classify("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n{\"user\":{\"id\":\"u_1\"}}"),
            SessionState::Authed
        );
    }

    #[test]
    fn rejected_null_user() {
        assert_eq!(
            classify("HTTP/1.1 200 OK\r\n\r\n{\"user\":null}"),
            SessionState::Rejected
        );
    }

    #[test]
    fn rejected_401() {
        assert_eq!(classify("HTTP/1.1 401 Unauthorized\r\n\r\n"), SessionState::Rejected);
    }

    #[test]
    fn unknown_empty() {
        assert_eq!(classify(""), SessionState::Unknown);
    }
}
