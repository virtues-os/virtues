//! One request per bi-stream.
//!
//! In: a single JSON line, at most [`MAX_REQUEST_BYTES`], read within
//! [`REQUEST_TIMEOUT`]. Out: a single JSON status line, then the body.
//!
//! ```text
//! → {"op":"page","token":"<22 chars>"}
//! ← {"status":"ok","content_type":"text/html; charset=utf-8","bytes":1310}
//! ← <1310 bytes>
//! ```
//!
//! Every failure a visitor could provoke answers `not_found` or
//! `bad_request` and nothing more specific: no paths, no errors, no hint
//! whether a token was ever valid.

use std::time::Duration;

use iroh::endpoint::{RecvStream, SendStream};
use serde::{Deserialize, Serialize};
use tokio::io::AsyncReadExt;

use crate::bundle::Store;

pub const MAX_REQUEST_BYTES: usize = 1024;
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Deserialize, PartialEq)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Request {
    Page { token: String },
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Status {
    Ok { content_type: &'static str, bytes: usize },
    NotFound,
    BadRequest,
}

/// Parse a request line. Anything that is not exactly one known request is
/// `None`.
pub fn parse(raw: &[u8]) -> Option<Request> {
    if raw.len() > MAX_REQUEST_BYTES {
        return None;
    }
    let line = std::str::from_utf8(raw).ok()?.trim_end_matches(['\r', '\n']);
    if line.contains('\n') {
        return None;
    }
    serde_json::from_str(line).ok()
}

/// What to send back for `request`: a status line and a body.
pub fn respond(store: &Store, request: Option<Request>) -> (Status, Vec<u8>) {
    match request {
        None => (Status::BadRequest, Vec::new()),
        Some(Request::Page { token }) => match store.load(&token) {
            Some(page) => (
                Status::Ok { content_type: "text/html; charset=utf-8", bytes: page.len() },
                page,
            ),
            None => (Status::NotFound, Vec::new()),
        },
    }
}

/// Serve one request on one stream.
pub async fn handle(store: &Store, mut send: SendStream, recv: RecvStream) -> anyhow::Result<()> {
    // One byte past the limit, so an oversized request is seen as one.
    let mut raw = Vec::new();
    let read = tokio::time::timeout(
        REQUEST_TIMEOUT,
        recv.take(MAX_REQUEST_BYTES as u64 + 1).read_to_end(&mut raw),
    )
    .await;
    let request = match read {
        Ok(Ok(_)) => parse(&raw),
        _ => None,
    };
    let (status, body) = respond(store, request);
    let mut head = serde_json::to_vec(&status)?;
    head.push(b'\n');
    send.write_all(&head).await?;
    send.write_all(&body).await?;
    send.finish()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{bundle, token};

    fn head(status: &Status) -> String {
        serde_json::to_string(status).unwrap()
    }

    #[test]
    fn a_page_request_parses() {
        let t = token::generate();
        let line = format!("{{\"op\":\"page\",\"token\":\"{t}\"}}\n");
        assert_eq!(parse(line.as_bytes()), Some(Request::Page { token: t }));
    }

    #[test]
    fn hostile_requests_are_bad_requests() {
        let big = format!("{{\"op\":\"page\",\"token\":\"{}\"}}", "a".repeat(2000));
        for raw in [
            &b""[..],
            b"GET / HTTP/1.1\r\n\r\n",
            b"{\"op\":\"shell\",\"cmd\":\"id\"}",
            b"{\"op\":\"page\"}",
            b"{\"op\":\"page\",\"token\":\"x\",\"extra\":1}",
            b"{\"op\":\"page\",\"token\":\"x\"}\n{\"op\":\"page\",\"token\":\"y\"}",
            b"\xff\xfe",
            big.as_bytes(),
        ] {
            assert_eq!(parse(raw), None, "{:?}", String::from_utf8_lossy(raw));
        }
    }

    #[test]
    fn every_miss_reads_the_same() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path());
        let expired = token::generate();
        bundle::write(dir.path(), &expired, b"x", &bundle::Meta { expires_at_unix: Some(1) })
            .unwrap();
        let answers: Vec<String> = [
            token::generate(),
            expired,
            "../../etc/passwd".to_string(),
            String::new(),
        ]
        .into_iter()
        .map(|token| head(&respond(&store, Some(Request::Page { token })).0))
        .collect();
        assert!(answers.iter().all(|a| a == "{\"status\":\"not_found\"}"), "{answers:?}");
    }

    #[test]
    fn a_found_page_says_its_size() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path());
        let t = token::generate();
        bundle::write(dir.path(), &t, b"<p>hi</p>", &bundle::Meta::default()).unwrap();
        let (status, body) = respond(&store, Some(Request::Page { token: t }));
        assert_eq!(
            head(&status),
            "{\"status\":\"ok\",\"content_type\":\"text/html; charset=utf-8\",\"bytes\":9}"
        );
        assert_eq!(body, b"<p>hi</p>");
    }
}
