//! AWS Signature Version 4 request signing, service `s3`, for the R2 (an
//! S3-compatible object store) upload. This file holds the signing section
//! only; a later task adds the transport and the upload itself on top of it.
//!
//! The reference is the AWS published test suite. `sign` reproduces its
//! canonical "GET Object" example exactly (see the tests below); everything
//! here follows from that one worked case: URI-encode the object key
//! segment by segment, sort the query by name, sign `host`,
//! `x-amz-content-sha256`, `x-amz-date` and whatever the caller adds, and
//! derive the signing key through the four-step HMAC chain the spec
//! describes.

use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

use percent_encoding::{utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};

type HmacSha256 = Hmac<Sha256>;

/// The bucket's address and credentials.
///
/// `endpoint` is host-only (scheme + authority, no path), for example
/// `"https://<account>.r2.cloudflarestorage.com"`. [`sign`] builds the
/// request path from the object key alone; nothing here inserts `bucket`
/// into it (a later task's caller decides whether the key it passes already
/// carries a bucket path segment).
pub struct Bucket {
    pub endpoint: String,
    pub bucket: String,
    pub region: String,
    pub access_key_id: String,
    pub secret_access_key: String,
}

/// Hand-written so the secret access key never prints: `<set>` or `<unset>`,
/// never the value. The access key id is not a secret and prints in full,
/// same as every other field. See `publish::roster::RosterRequest` for the
/// same pattern applied to a bearer token.
impl std::fmt::Debug for Bucket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Bucket")
            .field("endpoint", &self.endpoint)
            .field("bucket", &self.bucket)
            .field("region", &self.region)
            .field("access_key_id", &self.access_key_id)
            .field("secret_access_key", if self.secret_access_key.is_empty() { &"<unset>" } else { &"<set>" })
            .finish()
    }
}

/// One request to sign: the method, the full URL (host, path and sorted
/// query already assembled), and the headers to send, including
/// `Authorization`. Holds no secret, so the derived `Debug` is safe to log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedRequest {
    pub method: &'static str,
    pub url: String,
    pub headers: Vec<(String, String)>,
}

/// The payload hash to send when the body is not hashed up front, for
/// example a streamed multipart part. SigV4 still requires the header; this
/// is the value that tells S3 the signature does not cover the body.
pub const UNSIGNED_PAYLOAD: &str = "UNSIGNED-PAYLOAD";

/// Characters SigV4's URI-encoding step leaves alone: the unreserved set,
/// RFC 3986 section 2.3. Everything else, space included, percent-encodes
/// (space becomes `%20`, never `+`).
const UNRESERVED: &AsciiSet = &NON_ALPHANUMERIC.remove(b'-').remove(b'.').remove(b'_').remove(b'~');

fn uri_encode(s: &str) -> String {
    utf8_percent_encode(s, UNRESERVED).to_string()
}

/// The request path: `key` encoded segment by segment so `/` stays a
/// separator instead of becoming `%2F`.
fn canonical_uri(key: &str) -> String {
    let mut out = String::from("/");
    for (i, segment) in key.split('/').enumerate() {
        if i > 0 {
            out.push('/');
        }
        out.push_str(&uri_encode(segment));
    }
    out
}

/// The query string used both for signing and for the request URL: every
/// name and value URI-encoded, then sorted by name (ties broken by value,
/// which the spec does not require but never hurts).
fn canonical_query(query: &[(&str, &str)]) -> String {
    let mut pairs: Vec<(String, String)> = query.iter().map(|(k, v)| (uri_encode(k), uri_encode(v))).collect();
    pairs.sort();
    pairs.into_iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join("&")
}

/// The canonical headers block (`name:value\n` per line, lowercase name,
/// sorted) and the matching `SignedHeaders` list. `host`,
/// `x-amz-content-sha256` and `x-amz-date` are always included; whatever
/// the caller passed in `extra_headers` joins them.
fn canonical_headers_and_signed(host: &str, amz_date: &str, payload_sha256: &str, extra_headers: &[(String, String)]) -> (String, String) {
    let mut headers: Vec<(String, String)> = vec![
        ("host".to_string(), host.to_string()),
        ("x-amz-content-sha256".to_string(), payload_sha256.to_string()),
        ("x-amz-date".to_string(), amz_date.to_string()),
    ];
    for (name, value) in extra_headers {
        headers.push((name.to_lowercase(), value.trim().to_string()));
    }
    headers.sort();
    headers.dedup_by(|a, b| a.0 == b.0);

    let mut canonical = String::new();
    for (name, value) in &headers {
        canonical.push_str(name);
        canonical.push(':');
        canonical.push_str(value);
        canonical.push('\n');
    }
    let signed = headers.iter().map(|(name, _)| name.as_str()).collect::<Vec<_>>().join(";");
    (canonical, signed)
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    // A key of any length is valid input to HMAC (RFC 2104 pads or hashes it
    // as needed), so `new_from_slice` cannot fail here.
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC-SHA256 accepts a key of any length");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

/// `HMAC(HMAC(HMAC(HMAC("AWS4"+secret, date), region), "s3"), "aws4_request")`.
fn signing_key(secret: &str, date_stamp: &str, region: &str) -> Vec<u8> {
    let k_date = hmac_sha256(format!("AWS4{secret}").as_bytes(), date_stamp.as_bytes());
    let k_region = hmac_sha256(&k_date, region.as_bytes());
    let k_service = hmac_sha256(&k_region, b"s3");
    hmac_sha256(&k_service, b"aws4_request")
}

/// `now` as `yyyymmddTHHMMSSZ`, UTC. Hand-rolled instead of pulling in a
/// date/time crate for one timestamp format: seconds since the epoch split
/// into a day count and a time of day, then the day count turned into a
/// civil date with Howard Hinnant's `civil_from_days`
/// (<https://howardhinnant.github.io/date_algorithms.html#civil_from_days>).
fn format_amz_date(now: std::time::SystemTime) -> String {
    let secs = now.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
    let days = (secs / 86_400) as i64;
    let time_of_day = secs % 86_400;
    let (year, month, day) = civil_from_days(days);
    let hour = time_of_day / 3600;
    let minute = (time_of_day % 3600) / 60;
    let second = time_of_day % 60;
    format!("{year:04}{month:02}{day:02}T{hour:02}{minute:02}{second:02}Z")
}

/// Days since 1970-01-01 to a proleptic Gregorian (year, month, day).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    let year = if month <= 2 { y + 1 } else { y };
    (year, month, day)
}

/// Sign one request. `now` is a parameter, never read from the clock here,
/// so a caller's test is deterministic.
pub fn sign(
    bucket: &Bucket,
    method: &'static str,
    key: &str,
    query: &[(&str, &str)],
    extra_headers: &[(String, String)],
    payload_sha256: &str,
    now: std::time::SystemTime,
) -> SignedRequest {
    let amz_date = format_amz_date(now);
    let date_stamp = amz_date[..8].to_string();

    let host = bucket.endpoint.trim_start_matches("https://").trim_start_matches("http://").trim_end_matches('/').to_string();

    let uri = canonical_uri(key);
    let query_string = canonical_query(query);
    let (canonical_headers, signed_headers) = canonical_headers_and_signed(&host, &amz_date, payload_sha256, extra_headers);

    let canonical_request = format!("{method}\n{uri}\n{query_string}\n{canonical_headers}\n{signed_headers}\n{payload_sha256}");
    let hashed_canonical_request = hex::encode(Sha256::digest(canonical_request.as_bytes()));

    let credential_scope = format!("{date_stamp}/{}/s3/aws4_request", bucket.region);
    let string_to_sign = format!("AWS4-HMAC-SHA256\n{amz_date}\n{credential_scope}\n{hashed_canonical_request}");

    let key_bytes = signing_key(&bucket.secret_access_key, &date_stamp, &bucket.region);
    let signature = hex::encode(hmac_sha256(&key_bytes, string_to_sign.as_bytes()));

    let authorization = format!(
        "AWS4-HMAC-SHA256 Credential={}/{credential_scope},SignedHeaders={signed_headers},Signature={signature}",
        bucket.access_key_id
    );

    let mut headers = vec![
        ("host".to_string(), host),
        ("x-amz-content-sha256".to_string(), payload_sha256.to_string()),
        ("x-amz-date".to_string(), amz_date),
    ];
    headers.extend(extra_headers.iter().cloned());
    headers.push(("Authorization".to_string(), authorization));

    let mut url = format!("{}{}", bucket.endpoint.trim_end_matches('/'), uri);
    if !query_string.is_empty() {
        url.push('?');
        url.push_str(&query_string);
    }

    SignedRequest { method, url, headers }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The AWS test suite's bucket and credentials, used by every test
    /// below unless a test needs its own.
    fn test_bucket() -> Bucket {
        Bucket {
            endpoint: "https://examplebucket.s3.amazonaws.com".to_string(),
            bucket: "examplebucket".to_string(),
            region: "us-east-1".to_string(),
            access_key_id: "AKIAIOSFODNN7EXAMPLE".to_string(),
            secret_access_key: "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".to_string(),
        }
    }

    /// 2013-05-24T00:00:00Z, the AWS test suite's fixed timestamp.
    fn fixed_now() -> std::time::SystemTime {
        std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_369_353_600)
    }

    const EMPTY_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

    fn header<'a>(signed: &'a SignedRequest, name: &str) -> Option<&'a str> {
        signed.headers.iter().find(|(n, _)| n == name).map(|(_, v)| v.as_str())
    }

    /// The AWS published SigV4 test suite's "GET Object" example, byte for
    /// byte. This is the proof the algorithm is right: everything else
    /// below is a property test around it.
    #[test]
    fn aws_published_vector_get_object_with_range() {
        let bucket = test_bucket();
        let extra = vec![("range".to_string(), "bytes=0-9".to_string())];

        let signed = sign(&bucket, "GET", "test.txt", &[], &extra, EMPTY_SHA256, fixed_now());

        assert_eq!(signed.url, "https://examplebucket.s3.amazonaws.com/test.txt");
        assert_eq!(
            header(&signed, "Authorization"),
            Some(
                "AWS4-HMAC-SHA256 Credential=AKIAIOSFODNN7EXAMPLE/20130524/us-east-1/s3/aws4_request,\
                 SignedHeaders=host;range;x-amz-content-sha256;x-amz-date,\
                 Signature=f0e8bdb87c964420e857bd35b5d6ed310bd44f0170aba48dd91039c6036bdb41"
            )
        );
    }

    #[test]
    fn key_with_a_space_encodes_as_percent_20_not_plus() {
        let signed = sign(&test_bucket(), "GET", "a b.txt", &[], &[], UNSIGNED_PAYLOAD, fixed_now());

        assert_eq!(signed.url, "https://examplebucket.s3.amazonaws.com/a%20b.txt");
        assert!(!signed.url.contains('+'));

        // The encoded form is what actually got signed, not a display-only
        // detail: changing the key changes the signature.
        let unspaced = sign(&test_bucket(), "GET", "a_b.txt", &[], &[], UNSIGNED_PAYLOAD, fixed_now());
        assert_ne!(header(&signed, "Authorization"), header(&unspaced, "Authorization"));
    }

    #[test]
    fn key_with_a_slash_keeps_it_as_a_path_separator() {
        let signed = sign(&test_bucket(), "GET", "a/b.txt", &[], &[], UNSIGNED_PAYLOAD, fixed_now());

        assert_eq!(signed.url, "https://examplebucket.s3.amazonaws.com/a/b.txt");
        assert!(!signed.url.contains("%2F") && !signed.url.contains("%2f"));
    }

    #[test]
    fn query_parameters_sort_by_name_no_matter_the_call_order() {
        let bucket = test_bucket();
        let forward = sign(&bucket, "GET", "test.txt", &[("uploadId", "abc"), ("partNumber", "2")], &[], UNSIGNED_PAYLOAD, fixed_now());
        let reversed = sign(&bucket, "GET", "test.txt", &[("partNumber", "2"), ("uploadId", "abc")], &[], UNSIGNED_PAYLOAD, fixed_now());

        assert_eq!(forward.url, reversed.url);
        assert!(forward.url.ends_with("?partNumber=2&uploadId=abc"));
        // Same signature too: sorting happened before signing, not after.
        assert_eq!(header(&forward, "Authorization"), header(&reversed, "Authorization"));
    }

    #[test]
    fn secret_never_appears_in_signed_request_or_its_headers() {
        let bucket = test_bucket();
        let extra = vec![("range".to_string(), "bytes=0-9".to_string())];
        let signed = sign(&bucket, "GET", "test.txt", &[], &extra, EMPTY_SHA256, fixed_now());

        let debug = format!("{signed:?}");
        assert!(!debug.contains(bucket.secret_access_key.as_str()));
        for (_, value) in &signed.headers {
            assert!(!value.contains(bucket.secret_access_key.as_str()));
        }
    }

    #[test]
    fn bucket_debug_hides_the_secret_but_shows_the_access_key_id() {
        let bucket = test_bucket();
        let debug = format!("{bucket:?}");

        assert!(debug.contains("AKIAIOSFODNN7EXAMPLE"));
        assert!(debug.contains("<set>"));
        assert!(!debug.contains(bucket.secret_access_key.as_str()));
    }

    #[test]
    fn bucket_debug_shows_unset_for_an_empty_secret() {
        let bucket = Bucket { secret_access_key: String::new(), ..test_bucket() };
        let debug = format!("{bucket:?}");

        assert!(debug.contains("<unset>"));
        assert!(!debug.contains("<set>"));
    }
}
