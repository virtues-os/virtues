//! SNI peek for the blind relay.
//!
//! The relay routes an inbound TLS connection to the right box by reading the
//! **Server Name Indication** — the one cleartext field in the TLS ClientHello.
//! It then forwards the *still-encrypted* bytes; it never terminates TLS.
//!
//! ## Why not `rustls::server::Acceptor`?
//! `Acceptor` consumes the ClientHello into its internal buffer and gives no way
//! to recover the raw bytes, so a passthrough backend would never receive that
//! first flight and the handshake would hang. We must **peek without consuming**:
//! read the ClientHello into our own buffer, parse the SNI, then **replay the
//! whole buffer to the box** before splicing the rest. See `peek_sni`.

use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt};

/// Hard cap on how many bytes we'll buffer looking for the SNI. A ClientHello is
/// normally <1 KB; 16 KB is the TLS record ceiling and a generous DoS guard.
pub const MAX_CLIENTHELLO: usize = 16 * 1024;

/// Max DNS hostname length (RFC 1035). We reject anything longer *before*
/// allocating a `String`, as a cheap heap-DoS guard.
pub const MAX_HOSTNAME: usize = 254;

/// How long we'll wait for a client to send a parseable ClientHello.
pub const PEEK_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, thiserror::Error)]
pub enum SniError {
    #[error("connection closed before a ClientHello arrived")]
    Eof,
    #[error("ClientHello exceeded {MAX_CLIENTHELLO} bytes")]
    TooLarge,
    #[error("ClientHello had no SNI extension")]
    NoSni,
    #[error("malformed TLS ClientHello")]
    Malformed,
    #[error("timed out waiting for ClientHello")]
    Timeout,
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Read from `stream` until the SNI can be parsed, **without consuming** the
/// bytes: returns `(sni, buffered)` so the caller replays `buffered` to the box
/// before splicing. Bounded by [`MAX_CLIENTHELLO`] and [`PEEK_TIMEOUT`].
pub async fn peek_sni<R>(stream: &mut R) -> Result<(String, Vec<u8>), SniError>
where
    R: AsyncRead + Unpin,
{
    tokio::time::timeout(PEEK_TIMEOUT, peek_sni_inner(stream))
        .await
        .map_err(|_| SniError::Timeout)?
}

async fn peek_sni_inner<R>(stream: &mut R) -> Result<(String, Vec<u8>), SniError>
where
    R: AsyncRead + Unpin,
{
    let mut buf: Vec<u8> = Vec::with_capacity(1024);
    let mut chunk = [0u8; 4096];
    loop {
        let n = stream.read(&mut chunk).await?;
        if n == 0 {
            return Err(SniError::Eof);
        }
        buf.extend_from_slice(&chunk[..n]);
        if buf.len() > MAX_CLIENTHELLO {
            return Err(SniError::TooLarge);
        }
        match extract_sni(&buf)? {
            Some(sni) => return Ok((sni, buf)),
            None => continue, // need more bytes (incomplete record)
        }
    }
}

/// Parse the SNI hostname from a (possibly partial) buffer of TLS record bytes.
///
/// - `Ok(Some(host))` — SNI found (lowercased).
/// - `Ok(None)` — not enough bytes yet; read more and retry.
/// - `Err(_)` — a complete ClientHello with no usable SNI, or malformed input.
///
/// NOTE: handles the common single-record ClientHello. A ClientHello fragmented
/// across multiple TLS *records* is not reassembled here (neither do sniton /
/// most SNI proxies); such clients fall through to `Malformed`/`NoSni`.
pub fn extract_sni(buf: &[u8]) -> Result<Option<String>, SniError> {
    use tls_parser::{
        parse_tls_extensions, parse_tls_plaintext, SNIType, TlsExtension, TlsMessage,
        TlsMessageHandshake,
    };

    let (_, record) = match parse_tls_plaintext(buf) {
        Ok(r) => r,
        Err(e) if e.is_incomplete() => return Ok(None),
        Err(_) => return Err(SniError::Malformed),
    };

    for msg in &record.msg {
        let TlsMessage::Handshake(TlsMessageHandshake::ClientHello(ch)) = msg else {
            continue;
        };
        let ext_bytes = ch.ext.unwrap_or(&[]);
        let (_, exts) = match parse_tls_extensions(ext_bytes) {
            Ok(e) => e,
            Err(e) if e.is_incomplete() => return Ok(None),
            Err(_) => return Err(SniError::Malformed),
        };
        for ext in exts {
            let TlsExtension::SNI(names) = ext else { continue };
            for (sni_type, host) in names {
                // Only host_name(0) is a routable DNS name (RFC 6066 deprecated
                // every other type). Skip anything else rather than treating its
                // bytes as a hostname and misrouting.
                if sni_type != SNIType(0) {
                    continue;
                }
                if host.len() > MAX_HOSTNAME {
                    return Err(SniError::Malformed);
                }
                let s = std::str::from_utf8(host).map_err(|_| SniError::Malformed)?;
                return Ok(Some(s.to_ascii_lowercase()));
            }
        }
        // A complete ClientHello that carries no SNI extension.
        return Err(SniError::NoSni);
    }

    // Parsed a record, but no ClientHello in it yet — need more bytes.
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal but valid TLS 1.2 ClientHello record carrying `sni`.
    fn client_hello_with_sni(sni: &str) -> Vec<u8> {
        // --- SNI extension ---
        let host = sni.as_bytes();
        let mut server_name = Vec::new();
        server_name.push(0u8); // name_type = host_name
        server_name.extend_from_slice(&(host.len() as u16).to_be_bytes());
        server_name.extend_from_slice(host);

        let mut sni_list = Vec::new();
        sni_list.extend_from_slice(&(server_name.len() as u16).to_be_bytes());
        sni_list.extend_from_slice(&server_name);

        let mut sni_ext = Vec::new();
        sni_ext.extend_from_slice(&0u16.to_be_bytes()); // ext type 0 = server_name
        sni_ext.extend_from_slice(&(sni_list.len() as u16).to_be_bytes());
        sni_ext.extend_from_slice(&sni_list);

        let mut extensions = Vec::new();
        extensions.extend_from_slice(&(sni_ext.len() as u16).to_be_bytes());
        extensions.extend_from_slice(&sni_ext);

        // --- ClientHello body ---
        let mut body = Vec::new();
        body.extend_from_slice(&[0x03, 0x03]); // client_version TLS 1.2
        body.extend_from_slice(&[0u8; 32]); // random
        body.push(0u8); // session_id length 0
        body.extend_from_slice(&2u16.to_be_bytes()); // cipher_suites length
        body.extend_from_slice(&[0x13, 0x01]); // one cipher suite
        body.push(1u8); // compression methods length
        body.push(0u8); // null compression
        body.extend_from_slice(&extensions);

        // --- Handshake header (type 1 = ClientHello, 24-bit length) ---
        let mut handshake = Vec::new();
        handshake.push(1u8);
        let len = body.len();
        handshake.extend_from_slice(&[(len >> 16) as u8, (len >> 8) as u8, len as u8]);
        handshake.extend_from_slice(&body);

        // --- TLS record (type 22 = handshake, version, length) ---
        let mut record = Vec::new();
        record.push(22u8);
        record.extend_from_slice(&[0x03, 0x01]); // record version TLS 1.0 (typical)
        record.extend_from_slice(&(handshake.len() as u16).to_be_bytes());
        record.extend_from_slice(&handshake);
        record
    }

    #[test]
    fn parses_sni_from_complete_hello() {
        let rec = client_hello_with_sni("box123.boxes.virtues.com");
        let got = extract_sni(&rec).expect("ok");
        assert_eq!(got.as_deref(), Some("box123.boxes.virtues.com"));
    }

    #[test]
    fn lowercases_sni() {
        let rec = client_hello_with_sni("Box123.Boxes.VIRTUES.com");
        assert_eq!(
            extract_sni(&rec).unwrap().as_deref(),
            Some("box123.boxes.virtues.com")
        );
    }

    #[test]
    fn incomplete_buffer_asks_for_more() {
        let rec = client_hello_with_sni("box123.boxes.virtues.com");
        // Feed only the first few bytes: not a full record yet.
        let partial = &rec[..5];
        assert!(matches!(extract_sni(partial), Ok(None)));
    }

    #[test]
    fn empty_buffer_asks_for_more() {
        assert!(matches!(extract_sni(&[]), Ok(None)));
    }

    #[test]
    fn non_tls_garbage_is_malformed() {
        let junk = [0xFFu8; 64];
        assert!(matches!(extract_sni(&junk), Err(SniError::Malformed)));
    }

    #[tokio::test]
    async fn peek_returns_full_buffer_for_replay() {
        let rec = client_hello_with_sni("box123.boxes.virtues.com");
        let mut cursor = std::io::Cursor::new(rec.clone());
        let (sni, buffered) = peek_sni(&mut cursor).await.expect("peek ok");
        assert_eq!(sni, "box123.boxes.virtues.com");
        // The buffer we hand back MUST contain the entire ClientHello so the
        // caller can replay it to the box — losing it would hang the handshake.
        assert!(
            buffered.starts_with(&rec[..]),
            "buffered bytes must include the full ClientHello for replay"
        );
    }

    #[tokio::test]
    async fn peek_handles_fragmented_reads() {
        // Simulate the ClientHello arriving split across two reads.
        let rec = client_hello_with_sni("split.boxes.virtues.com");
        let (head, tail) = rec.split_at(7);
        let chained = tokio::io::AsyncReadExt::chain(
            std::io::Cursor::new(head.to_vec()),
            std::io::Cursor::new(tail.to_vec()),
        );
        let mut chained = chained;
        let (sni, buffered) = peek_sni(&mut chained).await.expect("peek ok");
        assert_eq!(sni, "split.boxes.virtues.com");
        assert_eq!(buffered, rec);
    }

    #[tokio::test]
    async fn peek_eof_before_hello_errs() {
        let mut empty = std::io::Cursor::new(Vec::<u8>::new());
        assert!(matches!(peek_sni(&mut empty).await, Err(SniError::Eof)));
    }
}
