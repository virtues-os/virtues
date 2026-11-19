//! AWS Signature Version 4 for one S3 `GET`, the request virtues-api makes
//! to read a map file from object storage (`services/virtues-api/src/routes/maps.rs`).
//!
//! It signs a read with an empty body and no query string, and nothing else.
//! Add a method or a query here when a caller needs one. It lives in `crypto/`
//! because the signing key is an HMAC chain.

use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

/// sha256 of the empty body every `GET` carries.
const EMPTY_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

/// One S3 read to sign.
pub struct S3Get<'a> {
    pub access_key: &'a str,
    pub secret_key: &'a str,
    pub region: &'a str,
    /// The `Host` header the request will carry, with its port when the URL has one.
    pub host: &'a str,
    /// The URL path, starting with `/`. The caller passes it already
    /// URI-encoded; a path of ASCII letters, digits, `/`, `.`, `-` and `_`
    /// needs no encoding.
    pub path: &'a str,
    /// The `Range` header, when the request carries one. A signed range
    /// cannot be changed on the way.
    pub range: Option<&'a str>,
    pub now: DateTime<Utc>,
}

/// The headers that authorize the read, besides `Host` and `Range`, which the
/// caller already sends: `authorization`, `x-amz-date` and
/// `x-amz-content-sha256`, as lowercase name and value.
pub fn sign_s3_get(req: &S3Get<'_>) -> [(&'static str, String); 3] {
    let amz_date = req.now.format("%Y%m%dT%H%M%SZ").to_string();
    let date = &amz_date[..8];

    // Canonical headers are sorted by name; `range` falls between `host` and `x-amz-*`.
    let mut headers = format!("host:{}\n", req.host);
    let mut signed = String::from("host");
    if let Some(range) = req.range {
        // The canonical value is trimmed, with each run of spaces as one space.
        let range = range.split_ascii_whitespace().collect::<Vec<_>>().join(" ");
        headers.push_str(&format!("range:{range}\n"));
        signed.push_str(";range");
    }
    headers.push_str(&format!("x-amz-content-sha256:{EMPTY_SHA256}\nx-amz-date:{amz_date}\n"));
    signed.push_str(";x-amz-content-sha256;x-amz-date");

    let canonical = format!("GET\n{}\n\n{headers}\n{signed}\n{EMPTY_SHA256}", req.path);
    let scope = format!("{date}/{}/s3/aws4_request", req.region);
    let to_sign = format!("AWS4-HMAC-SHA256\n{amz_date}\n{scope}\n{}", hex::encode(Sha256::digest(canonical.as_bytes())));

    let mut key = hmac(format!("AWS4{}", req.secret_key).as_bytes(), date.as_bytes());
    for part in [req.region, "s3", "aws4_request"] {
        key = hmac(&key, part.as_bytes());
    }
    let signature = hex::encode(hmac(&key, to_sign.as_bytes()));

    [
        (
            "authorization",
            format!("AWS4-HMAC-SHA256 Credential={}/{scope},SignedHeaders={signed},Signature={signature}", req.access_key),
        ),
        ("x-amz-date", amz_date),
        ("x-amz-content-sha256", EMPTY_SHA256.to_string()),
    ]
}

fn hmac(key: &[u8], msg: &[u8]) -> Vec<u8> {
    let mut mac = <Hmac<Sha256>>::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(msg);
    mac.finalize().into_bytes().to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    /// The "GET Object" example from the Amazon S3 API reference
    /// (Signature Version 4, authorization header): published keys, a
    /// ten-byte range, and the signature the reference arrives at.
    #[test]
    fn matches_the_published_s3_example() {
        let headers = sign_s3_get(&S3Get {
            access_key: "AKIAIOSFODNN7EXAMPLE",
            secret_key: "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
            region: "us-east-1",
            host: "examplebucket.s3.amazonaws.com",
            path: "/test.txt",
            range: Some("bytes=0-9"),
            now: Utc.with_ymd_and_hms(2013, 5, 24, 0, 0, 0).unwrap(),
        });
        assert_eq!(
            headers[0].1,
            "AWS4-HMAC-SHA256 Credential=AKIAIOSFODNN7EXAMPLE/20130524/us-east-1/s3/aws4_request,\
             SignedHeaders=host;range;x-amz-content-sha256;x-amz-date,\
             Signature=f0e8bdb87c964420e857bd35b5d6ed310bd44f0170aba48dd91039c6036bdb41"
        );
        assert_eq!(headers[1], ("x-amz-date", "20130524T000000Z".to_string()));
    }

    #[test]
    fn spaces_in_a_range_are_signed_as_the_service_reads_them() {
        let sign = |range| {
            sign_s3_get(&S3Get {
                access_key: "AKIAIOSFODNN7EXAMPLE",
                secret_key: "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
                region: "us-east-1",
                host: "examplebucket.s3.amazonaws.com",
                path: "/test.txt",
                range: Some(range),
                now: Utc.with_ymd_and_hms(2013, 5, 24, 0, 0, 0).unwrap(),
            })
        };
        assert_eq!(sign("  bytes=0-9 "), sign("bytes=0-9"));
        assert_eq!(sign("bytes=0-9,   20-29"), sign("bytes=0-9, 20-29"));
    }

    #[test]
    fn a_read_without_a_range_does_not_sign_one() {
        let headers = sign_s3_get(&S3Get {
            access_key: "AKIAIOSFODNN7EXAMPLE",
            secret_key: "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
            region: "us-east-1",
            host: "examplebucket.s3.amazonaws.com",
            path: "/test.txt",
            range: None,
            now: Utc.with_ymd_and_hms(2013, 5, 24, 0, 0, 0).unwrap(),
        });
        assert!(headers[0].1.contains("SignedHeaders=host;x-amz-content-sha256;x-amz-date,"));
    }
}
