//! The download pointer the league website reads to drive its download
//! button: what to fetch, how large it is, and its hash, so publishing a new
//! zip updates the button without a site deploy.
//!
//! [`pointer_for`] builds the pointer from the zip already sitting on disk.
//! The rule that matters most: the URL it writes is built exactly the way
//! `installer/windows/build.ps1` builds `latest.json`'s asset URL --
//! `$base.TrimEnd("/") + "/" + [uri]::EscapeDataString($name)` -- so the two
//! files can never disagree about how a space, or anything else outside
//! RFC 3986's unreserved set, comes out in a file name. See
//! `crate::update::Asset::url`, produced by that same script, for the
//! sibling this has to match.
//!
//! [`read_hosted`] is the site's side of the same file. `None` for a 404:
//! before the first publish there genuinely is no pointer, and that is a
//! state, not a fault. Anything that answers but does not parse as a
//! pointer is an error, because that means the bucket is serving garbage,
//! not "nothing yet".

use std::path::Path;
use std::time::SystemTime;

use percent_encoding::{utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};
use serde::{Deserialize, Serialize};

use crate::library::hash_file;
use crate::net::Http;
use crate::origin::Origin;
use crate::publish::io_err;
use crate::{Error, Result};

/// Where the pointer lives, relative to the bucket root: the same
/// `launcher/` folder that already holds `latest.json` and the installer it
/// names.
pub const DOWNLOAD_FILE: &str = "launcher/download.json";

/// Every pointer must carry exactly this.
pub const DOWNLOAD_SCHEMA: &str = "nsr.download.v1";

/// What the site's download button reads: where the zip is, how large it
/// is, its hash, and when it was published.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DownloadPointer {
    pub schema: String,
    pub version: String,
    pub url: String,
    pub bytes: u64,
    pub sha256: String,
    /// RFC 3339, stamped by the caller's clock. Same rule as
    /// `publish::journal::now`: UTC with an explicit `+00:00` offset, never
    /// a local reading.
    pub published: String,
}

/// Characters `[uri]::EscapeDataString` leaves alone: RFC 3986's unreserved
/// set. Everything else, space included, percent-encodes (space becomes
/// `%20`, never `+`).
///
/// This is the same set `publish::upload`'s SigV4 signer uses, kept as its
/// own copy here rather than shared: that one exists to match AWS's
/// URI-encoding rule, this one exists to match a PowerShell script's, and
/// the two happening to agree today is not a reason to make a change to one
/// silently reach the other.
const UNRESERVED: &AsciiSet = &NON_ALPHANUMERIC.remove(b'-').remove(b'.').remove(b'_').remove(b'~');

fn encode_file_name(name: &str) -> String {
    utf8_percent_encode(name, UNRESERVED).to_string()
}

/// Build the pointer from `zip` on disk: hash it, size it, and address it at
/// `public_base` + `launcher/` + its percent-encoded file name -- the exact
/// address `installer/windows/build.ps1` writes for the installer in
/// `latest.json`, so the two can never disagree about how the name encodes.
pub fn pointer_for(zip: &Path, version: &str, public_base: &str, now: SystemTime) -> Result<DownloadPointer> {
    let sha256 = hash_file(zip)?;
    let bytes = std::fs::metadata(zip).map_err(|e| io_err(zip, e))?.len();
    let name = zip.file_name().and_then(|n| n.to_str()).ok_or_else(|| {
        Error::PlainIo(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("{} has no file name to publish a download pointer for", zip.display()),
        ))
    })?;
    let url = Origin::new(public_base).url(&format!("launcher/{}", encode_file_name(name)));
    Ok(DownloadPointer {
        schema: DOWNLOAD_SCHEMA.to_string(),
        version: version.to_string(),
        url,
        bytes,
        sha256: sha256.to_string(),
        published: format_rfc3339(now),
    })
}

/// Fetch the pointer the bucket serves. `None` for a 404: before the first
/// publish there genuinely is no pointer, and that is not an error. Any
/// other unsuccessful status, or a body that does not parse as a pointer,
/// is an error: either means the bucket is serving something broken, not
/// simply "nothing yet".
pub fn read_hosted(http: &dyn Http, public_base: &str) -> Result<Option<DownloadPointer>> {
    let url = Origin::new(public_base).url(DOWNLOAD_FILE);
    let reply = http.get_text(&url)?;
    if reply.status == 404 {
        return Ok(None);
    }
    if !reply.is_success() {
        return Err(Error::PlainIo(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("{url} answered {}", reply.status),
        )));
    }
    serde_json::from_str(&reply.body).map(Some).map_err(|e| {
        Error::PlainIo(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("{url} did not answer with a download pointer: {e}"),
        ))
    })
}

/// The same RFC 3339 rule as `publish::journal::now`, built from
/// [`crate::publish::civil_from_days`] rather than a timezone dependency.
/// Not the same function: this task's brief scopes the change to this file
/// and one `pub mod` line in `publish::mod`, so this keeps its own thin
/// formatting wrapper around the shared date algorithm rather than making
/// `journal`'s private copy reachable from here.
fn format_rfc3339(t: SystemTime) -> String {
    let secs = t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
    let days = (secs / 86_400) as i64;
    let time_of_day = secs % 86_400;
    let (year, month, day) = crate::publish::civil_from_days(days);
    let hour = time_of_day / 3600;
    let minute = (time_of_day % 3600) / 60;
    let second = time_of_day % 60;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}+00:00")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::fake::FakeHttp;

    const POINTER_URL: &str = "https://files.nflstreetreborn.com/launcher/download.json";

    fn write_zip(dir: &Path, name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, bytes).unwrap();
        path
    }

    fn sample_pointer() -> DownloadPointer {
        DownloadPointer {
            schema: DOWNLOAD_SCHEMA.to_string(),
            version: "3.0.0".to_string(),
            url: "https://files.nflstreetreborn.com/launcher/NFL%20Street%20Reborn%203.0.0.zip".to_string(),
            bytes: 123,
            sha256: "a".repeat(64),
            published: "2025-09-09T15:12:07+00:00".to_string(),
        }
    }

    #[test]
    fn pointer_for_carries_the_zips_real_size_and_hash() {
        let dir = tempfile::tempdir().unwrap();
        let content = b"a stand-in for a 2 GB zip, just some bytes for the test";
        let zip = write_zip(dir.path(), "NFL Street Reborn 3.0.0.zip", content);
        let now = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_757_430_727);

        let pointer = pointer_for(&zip, "3.0.0", "https://files.nflstreetreborn.com/", now).unwrap();

        assert_eq!(pointer.schema, DOWNLOAD_SCHEMA);
        assert_eq!(pointer.version, "3.0.0");
        assert_eq!(pointer.bytes, content.len() as u64);
        assert_eq!(pointer.sha256, hash_file(&zip).unwrap().to_string());
        assert_eq!(pointer.published, "2025-09-09T15:12:07+00:00", "matches journal.rs's own known epoch");
    }

    /// The detail that matters most: a space in the file name must come out
    /// as `%20`, exactly what `[uri]::EscapeDataString` produces in
    /// `installer/windows/build.ps1`, so the site and the launcher never
    /// disagree about the same file's address.
    #[test]
    fn the_url_percent_encodes_a_space_like_uri_escape_data_string() {
        let dir = tempfile::tempdir().unwrap();
        let zip = write_zip(dir.path(), "NFL Street Reborn 3.0.0.zip", b"x");

        let pointer = pointer_for(&zip, "3.0.0", "https://files.nflstreetreborn.com/", SystemTime::now()).unwrap();

        assert_eq!(
            pointer.url,
            "https://files.nflstreetreborn.com/launcher/NFL%20Street%20Reborn%203.0.0.zip"
        );
    }

    #[test]
    fn public_base_without_a_trailing_slash_still_joins_cleanly() {
        let dir = tempfile::tempdir().unwrap();
        let zip = write_zip(dir.path(), "Setup 3.0.0.zip", b"x");

        let pointer = pointer_for(&zip, "3.0.0", "https://files.nflstreetreborn.com", SystemTime::now()).unwrap();

        assert_eq!(pointer.url, "https://files.nflstreetreborn.com/launcher/Setup%203.0.0.zip");
    }

    #[test]
    fn a_round_trip_through_serde_json_is_identical() {
        let pointer = sample_pointer();
        let text = serde_json::to_string(&pointer).unwrap();
        let back: DownloadPointer = serde_json::from_str(&text).unwrap();
        assert_eq!(back, pointer);
    }

    #[test]
    fn read_hosted_is_none_before_the_first_publish() {
        let http = FakeHttp::default().reply(POINTER_URL, 404, "");
        assert_eq!(read_hosted(&http, "https://files.nflstreetreborn.com/").unwrap(), None);
    }

    #[test]
    fn read_hosted_errors_on_a_body_that_is_not_a_pointer() {
        let http = FakeHttp::default().reply(POINTER_URL, 200, "not json");
        let err = read_hosted(&http, "https://files.nflstreetreborn.com/").unwrap_err();
        assert!(err.to_string().contains("did not answer with a download pointer"), "{err}");
    }

    #[test]
    fn read_hosted_errors_on_a_server_error_status() {
        let http = FakeHttp::default().reply(POINTER_URL, 500, "");
        let err = read_hosted(&http, "https://files.nflstreetreborn.com/").unwrap_err();
        assert!(err.to_string().contains("answered 500"), "{err}");
    }

    #[test]
    fn read_hosted_returns_exactly_what_was_published() {
        let pointer = sample_pointer();
        let http = FakeHttp::default().reply(POINTER_URL, 200, serde_json::to_string(&pointer).unwrap());
        assert_eq!(read_hosted(&http, "https://files.nflstreetreborn.com/").unwrap(), Some(pointer));
    }
}
