//! Fetch the roster the site published and push it: the component, then a
//! release that carries every other pinned component forward.
//!
//! This module is also the one place that reads a `.rebornroster`'s own
//! `manifest.json` for its player and team counts ([`read_counts`]), over a
//! real zip reader rather than anything hand rolled, so the desktop app's
//! Roster page can show Adam what he is about to publish without owning any
//! zip or DEFLATE code of its own. [`publish_roster`] itself never reads
//! that far: it only confirms a file *looks* like a `.rebornroster` (see
//! [`looks_like_roster`]), since the Builder, not the publisher, is the
//! authority on the disc-side format.

use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::inventory::{ComponentId, ComponentSpec, ScanObserver};
use crate::net::Http;
use crate::publish::component::{publish_component, ComponentRequest, Published};
use crate::publish::release::{republish_pinning, CarryForward, Signed};
use crate::publish::{io_err, next_sequence, out, refuse, thousands, Outcome, Sink};
use crate::roster::ROSTER_FILE;
use crate::{Error, Result};

pub struct RosterRequest<'a> {
    /// A URL, or a local .rebornroster path.
    pub from: &'a str,
    pub origin: &'a Path,
    /// The roster's label, for example 2026.09.06.
    pub version: &'a str,
    /// Defaults to one above the highest roster sequence in the origin.
    pub sequence: Option<u64>,
    /// Without it the component is published and the release only described.
    pub key: Option<&'a Path>,
    pub release_version: Option<&'a str>,
    pub release_sequence: Option<u64>,
    pub apply: bool,
    /// The site's bearer token, for a login-only export. Resolved by the caller.
    pub token: Option<&'a str>,
}

/// Hand-written so the token, which the vault holds, never appears in a log
/// or a journal: `<set>` or `<unset>`, never the value. Every other field
/// prints normally.
impl std::fmt::Debug for RosterRequest<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RosterRequest")
            .field("from", &self.from)
            .field("origin", &self.origin)
            .field("version", &self.version)
            .field("sequence", &self.sequence)
            .field("key", &self.key)
            .field("release_version", &self.release_version)
            .field("release_sequence", &self.release_sequence)
            .field("apply", &self.apply)
            .field("token", if self.token.is_some() { &"<set>" } else { &"<unset>" })
            .finish()
    }
}

/// A component pushed, and the release that pins it when one was signed.
#[derive(Debug, Clone)]
pub struct Push {
    pub component: Published,
    pub release: Option<Signed>,
}

/// Removes the staging directory when it drops, on every path out of
/// [`publish_roster`]: the early refusals, the `?` propagations, and the
/// final success. A directory unique to one call must not outlive that
/// call, or a long-lived host publishing roster after roster leaves one
/// orphan behind per call, forever. Errors removing it are not fatal: the
/// publish already succeeded or was refused on its own terms.
struct StageGuard(PathBuf);

impl Drop for StageGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Fetch a roster and push it as the `roster` component, then re-sign a
/// release that pins it beside everything already served. Without
/// `req.apply` the component is described and the release step is described
/// too, so a dry run reports the whole plan.
pub fn publish_roster(http: &dyn Http, req: &RosterRequest<'_>, observer: &dyn ScanObserver, sink: Sink<'_>) -> Result<Outcome<Push>> {
    let origin = req.origin;

    // A folder holding exactly the one file, because the component scan
    // hashes everything under its root. Named for this process and this call,
    // not just the version: two publishes of the same version must never
    // race over one directory, remove_dir_all-ing out from under each other.
    static CALLS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let call = CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let stage = std::env::temp_dir()
        .join("reborn-publish")
        .join("roster")
        .join(format!("{}-{}-{call}", req.version, std::process::id()));
    // A per-call unique path removes the common collision, but a process
    // killed rather than unwound (Ctrl+C on Windows) skips the `Drop` guard
    // below, and Windows recycles process ids, so a leftover directory from
    // an earlier, killed call can still land on this exact path.
    if stage.exists() {
        std::fs::remove_dir_all(&stage).map_err(|e| io_err(&stage, e))?;
    }
    std::fs::create_dir_all(&stage).map_err(|e| io_err(&stage, e))?;
    // Cleared on every return below, including a `?` propagation. Made only
    // after the directory exists, so nothing above ever has to clean up a
    // directory that was never created; kept alive until publish_component
    // (which reads from `stage`) has already run to completion on every
    // path that reaches it.
    let _stage_guard = StageGuard(stage.clone());
    let staged = stage.join(ROSTER_FILE);

    if req.from.starts_with("http://") || req.from.starts_with("https://") {
        // The token: resolved by the caller. Its value is never printed;
        // only whether one was sent.
        let token = req.token;
        out!(
            sink,
            "Fetching      {}{}",
            req.from,
            if token.is_some() { "  (with the site token)" } else { "" }
        );
        let bytes = match http.get_to_file_as(req.from, &staged, token) {
            Ok(b) => b,
            Err(e) if e.to_string().contains("answered 401") => {
                return Ok(refuse(sink, &[
                    String::new(),
                    "The site answered 401: that export needs a login. Store the site's API token".to_string(),
                    "with `rebornctl publish token --from-file <file>` or pass --token-file. Nothing".to_string(),
                    "was published.".to_string(),
                ]));
            }
            Err(e) => return Err(e),
        };
        out!(sink, "Fetched       {} bytes", thousands(bytes));
    } else {
        let src = Path::new(req.from);
        if !src.is_file() {
            return Ok(refuse(sink, &[format!("{} is neither a URL nor a file. Nothing was published.", req.from)]));
        }
        std::fs::copy(src, &staged).map_err(|e| io_err(src, e))?;
        out!(sink, "Roster        {}", src.display());
    }

    // The Builder is the authority on the format; this only refuses what
    // is plainly not a roster, so a 404 page saved as a file cannot be
    // published as one. A .rebornroster is a zip and its entry names sit in
    // the clear inside it.
    let head = std::fs::read(&staged).map_err(|e| io_err(&staged, e))?;
    if !looks_like_roster(&head) {
        return Ok(refuse(sink, &[String::new(), not_a_roster_message(&head), "Nothing was published.".to_string()]));
    }

    let sequence = match req.sequence {
        Some(s) => s,
        None => {
            let next = next_sequence(origin, ComponentId::Roster)?;
            out!(sink, "Sequence      {next} (one above the highest roster already in the origin)");
            next
        }
    };

    let spec = match ComponentSpec::new(ComponentId::Roster, req.version, sequence) {
        Ok(s) => s,
        Err(e) => return Ok(refuse(sink, &[e.to_string()])),
    };
    let component = match publish_component(&ComponentRequest { root: &stage, spec, origin, apply: req.apply }, observer, sink)? {
        Outcome::Refused(r) => return Ok(Outcome::Refused(r)),
        Outcome::Described => {
            let carry = CarryForward {
                origin,
                id: ComponentId::Roster,
                version: req.version,
                key: req.key,
                release_version: req.release_version,
                release_sequence: req.release_sequence,
                apply: req.apply,
            };
            return Ok(match republish_pinning(&carry, sink)? {
                Outcome::Refused(r) => Outcome::Refused(r),
                _ => Outcome::Described,
            });
        }
        Outcome::Done(component) => component,
    };

    let carry = CarryForward {
        origin,
        id: ComponentId::Roster,
        version: req.version,
        key: req.key,
        release_version: req.release_version,
        release_sequence: req.release_sequence,
        apply: req.apply,
    };
    Ok(match republish_pinning(&carry, sink)? {
        Outcome::Done(s) => Outcome::Done(Push { component, release: Some(s) }),
        Outcome::Described => Outcome::Done(Push { component, release: None }),
        Outcome::Refused(r) => Outcome::Refused(r),
    })
}

// ---------------------------------------------------------------------------
// Reading a `.rebornroster`'s own counts
// ---------------------------------------------------------------------------

/// What a `.rebornroster`'s own `manifest.json` says about itself: how many
/// players and teams it holds, how many portraits, and which base image it
/// was exported against. Read by the desktop app's Roster page before Run
/// is ever clicked, so Adam sees what he is about to publish; see
/// [`read_counts`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RosterCounts {
    pub players: u64,
    pub teams: u64,
    pub portraits: u64,
    pub base_label: Option<String>,
}

/// Whether `bytes` looks like a `.rebornroster`: a zip, by its local file
/// header magic, holding `manifest.json`, `players.json` and `teams.json`
/// somewhere in the clear. A zip's entry names sit unencrypted in its local
/// file headers even when an entry's own data is compressed, so checking
/// this needs no zip parsing and no decompression -- which is exactly what
/// keeps a saved 404 page or a truncated download from ever reaching the
/// zip reader in [`read_counts`] and surfacing as a decompression error
/// instead of this plain one. Shared by [`publish_roster`] and
/// [`read_counts`] so a junk file is refused in the same words wherever it
/// is caught.
fn looks_like_roster(bytes: &[u8]) -> bool {
    bytes.starts_with(b"PK\x03\x04")
        && [&b"manifest.json"[..], &b"players.json"[..], &b"teams.json"[..]]
            .iter()
            .all(|n| bytes.windows(n.len()).any(|w| w == *n))
}

/// The one refusal sentence for something that does not look like a
/// `.rebornroster`. See [`looks_like_roster`].
fn not_a_roster_message(bytes: &[u8]) -> String {
    format!(
        "That is not a .rebornroster: expected a zip holding manifest.json, players.json and teams.json. \
         Got {} bytes starting {:?}.",
        bytes.len(),
        String::from_utf8_lossy(&bytes[..bytes.len().min(16)])
    )
}

fn invalid_roster(why: impl Into<String>) -> Error {
    Error::PlainIo(std::io::Error::new(std::io::ErrorKind::InvalidData, why.into()))
}

/// Read `path`, confirm it looks like a `.rebornroster`, and pull the
/// counts out of its `manifest.json` entry, through a real zip reader (the
/// `zip` crate, over flate2's pure-Rust `miniz_oxide` backend) rather than
/// anything hand rolled. Every failure is a plain sentence, never a panic
/// and never a raw decompression error: a saved 404 page, a truncated
/// download, or an export from a future format this reads all refuse this
/// way instead.
pub fn read_counts(path: &Path) -> Result<RosterCounts> {
    let bytes = std::fs::read(path).map_err(|e| io_err(path, e))?;
    if !looks_like_roster(&bytes) {
        return Err(invalid_roster(not_a_roster_message(&bytes)));
    }

    let mut archive = zip::ZipArchive::new(Cursor::new(bytes.as_slice())).map_err(|e| Error::PlainIo(e.into()))?;
    let mut entry = archive.by_name("manifest.json").map_err(|e| Error::PlainIo(e.into()))?;
    let mut manifest_bytes = Vec::new();
    entry.read_to_end(&mut manifest_bytes).map_err(|e| io_err(path, e))?;
    drop(entry);

    let manifest: Value = serde_json::from_slice(&manifest_bytes)
        .map_err(|e| invalid_roster(format!("manifest.json does not parse as JSON: {e}")))?;
    let counts = manifest.get("counts").ok_or_else(|| invalid_roster("manifest.json has no \"counts\" object."))?;
    let players = counts
        .get("players")
        .and_then(Value::as_u64)
        .ok_or_else(|| invalid_roster("manifest.json's counts.players is missing or not a whole number."))?;
    let teams = counts
        .get("teams")
        .and_then(Value::as_u64)
        .ok_or_else(|| invalid_roster("manifest.json's counts.teams is missing or not a whole number."))?;
    let portraits = counts.get("portraits").and_then(Value::as_u64).unwrap_or(0);
    let base_label = manifest.get("base_label").and_then(Value::as_str).map(str::to_string);

    Ok(RosterCounts { players, teams, portraits, base_label })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::RunToCompletion;
    use crate::net::HttpResponse;
    use crate::publish::keys::keygen;
    use crate::publish::{Event, Outcome};
    use crate::Error;
    use std::io::Write;

    /// A site that serves one file at one URL, and remembers the bearer it saw.
    #[derive(Debug)]
    struct FakeSite {
        url: String,
        body: Vec<u8>,
        seen_bearer: std::sync::Mutex<Option<Option<String>>>,
    }

    impl Http for FakeSite {
        fn get_text(&self, _url: &str) -> Result<HttpResponse> {
            unimplemented!()
        }

        fn get_to_file(&self, _url: &str, _to: &Path) -> Result<u64> {
            unimplemented!()
        }

        fn post_json(&self, _url: &str, _body: &[u8], _bearer: Option<&str>) -> Result<HttpResponse> {
            unimplemented!()
        }

        fn get_to_file_as(&self, url: &str, to: &Path, bearer: Option<&str>) -> Result<u64> {
            *self.seen_bearer.lock().unwrap() = Some(bearer.map(str::to_string));
            if url != self.url {
                return Err(Error::PlainIo(std::io::Error::new(std::io::ErrorKind::NotFound, format!("{url} answered 404"))));
            }
            std::fs::File::create(to)?.write_all(&self.body)?;
            Ok(self.body.len() as u64)
        }
    }

    fn roster_zip() -> Vec<u8> {
        // A zip with the three entry names in the clear, which is all the
        // publisher checks. Local file headers only; the Builder never reads it.
        let mut z = Vec::new();
        for name in ["manifest.json", "players.json", "teams.json"] {
            z.extend_from_slice(b"PK\x03\x04");
            z.extend_from_slice(&[0u8; 22]);
            z.extend_from_slice(&(name.len() as u16).to_le_bytes());
            z.extend_from_slice(&0u16.to_le_bytes());
            z.extend_from_slice(name.as_bytes());
        }
        z
    }

    fn origin_with_runtime_and_keys() -> (tempfile::TempDir, PathBuf, PathBuf) {
        use crate::inventory::{ComponentId, ComponentSpec};
        use crate::publish::component::{publish_component, ComponentRequest};
        use crate::publish::release::{release, Pin, ReleaseRequest};
        let dir = tempfile::tempdir().unwrap();
        let origin = dir.path().join("origin");
        let written = match keygen(&dir.path().join("keys"), &mut |_| {}).unwrap() { Outcome::Done(w) => w, o => panic!("{o:?}") };
        let tree = dir.path().join("runtime");
        std::fs::create_dir_all(&tree).unwrap();
        std::fs::write(tree.join("a.txt"), b"alpha\n").unwrap();
        let spec = ComponentSpec::new(ComponentId::Runtime, "1.0.2", 1).unwrap();
        assert!(publish_component(&ComponentRequest { root: &tree, spec, origin: &origin, apply: true }, &RunToCompletion, &mut |_| {}).unwrap().is_done());
        let pins = [Pin { id: ComponentId::Runtime, version: "1.0.2".into() }];
        assert!(release(&ReleaseRequest { origin: &origin, version: "3.0.0", sequence: 3, key: &written.publisher_path, pins: &pins, apply: true }, &mut |_| {}).unwrap().is_done());
        (dir, origin, written.publisher_path)
    }

    #[test]
    fn a_roster_from_a_url_is_fetched_published_and_pinned_beside_the_runtime() {
        let (_dir, origin, key) = origin_with_runtime_and_keys();
        let site = FakeSite { url: "https://site.test/export".into(), body: roster_zip(), seen_bearer: Default::default() };
        let mut lines = Vec::new();
        // Distinctive rather than a short word like "tok", which could
        // appear inside an unrelated word and false-positive the check below.
        let token = "s3cr3t-token-value";
        let req = RosterRequest {
            from: "https://site.test/export", origin: &origin, version: "2026.09.06", sequence: None,
            key: Some(&key), release_version: None, release_sequence: None, apply: true, token: Some(token),
        };
        let outcome = publish_roster(&site, &req, &RunToCompletion, &mut |e: Event| lines.push(e.line)).unwrap();
        let push = match outcome { Outcome::Done(p) => p, o => panic!("{o:?}") };
        assert_eq!(push.release.as_ref().map(|s| s.sequence), Some(4));
        assert_eq!(*site.seen_bearer.lock().unwrap(), Some(Some(token.to_string())));
        assert!(lines.iter().any(|l| l.starts_with("Fetched")), "{lines:?}");
        assert!(lines.iter().any(|l| l == "Sequence      1 (one above the highest roster already in the origin)"), "{lines:?}");
        assert!(lines.iter().any(|l| l.contains("pinning roster=2026.09.06, runtime=1.0.2")), "{lines:?}");
        assert!(origin.join("components").join("roster-2026.09.06.json").is_file());
        // The token itself never appears in any emitted line: only whether
        // one was sent (the "with the site token" suffix on "Fetching").
        assert!(!lines.iter().any(|l| l.contains(token)), "{lines:?}");
    }

    #[test]
    fn junk_is_refused_before_anything_is_published() {
        let (_dir, origin, key) = origin_with_runtime_and_keys();
        let site = FakeSite { url: "https://site.test/export".into(), body: b"<html>404</html>".to_vec(), seen_bearer: Default::default() };
        let mut lines = Vec::new();
        let req = RosterRequest {
            from: "https://site.test/export", origin: &origin, version: "2026.09.07", sequence: None,
            key: Some(&key), release_version: None, release_sequence: None, apply: true, token: None,
        };
        let outcome = publish_roster(&site, &req, &RunToCompletion, &mut |e: Event| lines.push(e.line)).unwrap();
        assert!(outcome.refused().is_some());
        assert!(lines.iter().any(|l| l.contains("not a .rebornroster")), "{lines:?}");
        assert!(!origin.join("components").join("roster-2026.09.07.json").exists());
    }

    #[test]
    fn a_local_file_needs_no_site_and_no_key_leaves_the_release_described() {
        let (dir, origin, _key) = origin_with_runtime_and_keys();
        let file = dir.path().join("local.rebornroster");
        std::fs::write(&file, roster_zip()).unwrap();
        let site = FakeSite { url: String::new(), body: Vec::new(), seen_bearer: Default::default() };
        let mut lines = Vec::new();
        let from = file.to_string_lossy().into_owned();
        let req = RosterRequest {
            from: &from, origin: &origin, version: "2026.09.06", sequence: Some(5),
            key: None, release_version: None, release_sequence: None, apply: true, token: None,
        };
        let outcome = publish_roster(&site, &req, &RunToCompletion, &mut |e: Event| lines.push(e.line)).unwrap();
        let push = match outcome { Outcome::Done(p) => p, o => panic!("{o:?}") };
        assert!(push.release.is_none());
        assert!(site.seen_bearer.lock().unwrap().is_none(), "no fetch for a local file");
        assert!(lines.iter().any(|l| l.contains("No --key")), "{lines:?}");
    }

    #[test]
    fn a_401_is_explained_and_refused() {
        #[derive(Debug)]
        struct Locked;
        impl Http for Locked {
            fn get_text(&self, _url: &str) -> Result<HttpResponse> {
                unimplemented!()
            }

            fn get_to_file(&self, _url: &str, _to: &Path) -> Result<u64> {
                unimplemented!()
            }

            fn post_json(&self, _url: &str, _body: &[u8], _bearer: Option<&str>) -> Result<HttpResponse> {
                unimplemented!()
            }

            fn get_to_file_as(&self, url: &str, _to: &Path, _bearer: Option<&str>) -> Result<u64> {
                Err(Error::PlainIo(std::io::Error::new(std::io::ErrorKind::PermissionDenied, format!("{url} answered 401"))))
            }
        }
        let (_dir, origin, key) = origin_with_runtime_and_keys();
        let mut lines = Vec::new();
        let req = RosterRequest {
            from: "https://site.test/export", origin: &origin, version: "2026.09.06", sequence: None,
            key: Some(&key), release_version: None, release_sequence: None, apply: true, token: None,
        };
        let outcome = publish_roster(&Locked, &req, &RunToCompletion, &mut |e: Event| lines.push(e.line)).unwrap();
        assert!(outcome.refused().is_some());
        assert!(lines.iter().any(|l| l.contains("answered 401")), "{lines:?}");
    }

    /// The names directly under `temp_dir()/reborn-publish/roster`, so a test
    /// can check whether one call's staging directory is still there without
    /// being thrown off by other tests' entries under the same shared root.
    fn stage_entries() -> Vec<String> {
        let root = std::env::temp_dir().join("reborn-publish").join("roster");
        std::fs::read_dir(&root)
            .map(|rd| rd.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect())
            .unwrap_or_default()
    }

    #[test]
    fn the_staging_directory_does_not_survive_a_successful_or_a_refused_call() {
        let (_dir, origin, key) = origin_with_runtime_and_keys();

        // A version used by no other test in this module, so a leftover
        // directory can only be this call's.
        let ok_version = "2026.10.01-stageguard-ok";
        let site = FakeSite { url: "https://site.test/export".into(), body: roster_zip(), seen_bearer: Default::default() };
        let req = RosterRequest {
            from: "https://site.test/export", origin: &origin, version: ok_version, sequence: Some(1),
            key: Some(&key), release_version: None, release_sequence: None, apply: true, token: None,
        };
        let outcome = publish_roster(&site, &req, &RunToCompletion, &mut |_| {}).unwrap();
        assert!(outcome.is_done(), "{outcome:?}");
        let leftover = stage_entries();
        assert!(
            leftover.iter().all(|n| !n.starts_with(&format!("{ok_version}-"))),
            "a staging directory for {ok_version} outlived a successful call: {leftover:?}"
        );

        let refused_version = "2026.10.02-stageguard-refused";
        let junk_site = FakeSite { url: "https://site.test/export".into(), body: b"<html>404</html>".to_vec(), seen_bearer: Default::default() };
        let req = RosterRequest {
            from: "https://site.test/export", origin: &origin, version: refused_version, sequence: Some(2),
            key: Some(&key), release_version: None, release_sequence: None, apply: true, token: None,
        };
        let outcome = publish_roster(&junk_site, &req, &RunToCompletion, &mut |_| {}).unwrap();
        assert!(outcome.refused().is_some());
        let leftover = stage_entries();
        assert!(
            leftover.iter().all(|n| !n.starts_with(&format!("{refused_version}-"))),
            "a staging directory for {refused_version} outlived a refused call: {leftover:?}"
        );
    }

    // -- read_counts: a real zip, through the real zip crate -----------------

    /// A real zip, with a proper central directory, unlike [`roster_zip`]
    /// above (which only ever needs to fool the magic-and-names check
    /// `publish_roster` itself runs, and is never opened by an actual zip
    /// reader). `read_counts` opens what it is given with the `zip` crate,
    /// which requires the central directory a genuine export (PHP's
    /// `ZipArchive`, or this helper) always writes.
    fn write_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut writer = zip::ZipWriter::new(Cursor::new(&mut buf));
            let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
            for (name, data) in entries {
                writer.start_file(*name, options).unwrap();
                writer.write_all(data).unwrap();
            }
            writer.finish().unwrap();
        }
        buf
    }

    #[test]
    fn read_counts_reads_a_hand_built_zip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.rebornroster");
        let manifest = br#"{"counts":{"players":569,"teams":51,"portraits":12},"base_label":"2026.08.28"}"#;
        let bytes = write_zip(&[("manifest.json", manifest), ("players.json", b"[]"), ("teams.json", b"[]")]);
        std::fs::write(&path, bytes).unwrap();

        let counts = read_counts(&path).unwrap();
        assert_eq!(counts.players, 569);
        assert_eq!(counts.teams, 51);
        assert_eq!(counts.portraits, 12);
        assert_eq!(counts.base_label.as_deref(), Some("2026.08.28"));
    }

    #[test]
    fn read_counts_reads_a_stored_entry_too() {
        // method 0 (stored), not just method 8 (deflated): a manifest.json
        // small enough that a real exporter might not bother compressing it.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.rebornroster");
        let mut buf = Vec::new();
        {
            let mut writer = zip::ZipWriter::new(Cursor::new(&mut buf));
            let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
            writer.start_file("manifest.json", options).unwrap();
            writer.write_all(br#"{"counts":{"players":3,"teams":1}}"#).unwrap();
            writer.start_file("players.json", options).unwrap();
            writer.write_all(b"[]").unwrap();
            writer.start_file("teams.json", options).unwrap();
            writer.write_all(b"[]").unwrap();
            writer.finish().unwrap();
        }
        std::fs::write(&path, buf).unwrap();

        let counts = read_counts(&path).unwrap();
        assert_eq!(counts.players, 3);
        assert_eq!(counts.teams, 1);
        assert_eq!(counts.portraits, 0);
        assert_eq!(counts.base_label, None);
    }

    #[test]
    fn read_counts_refuses_junk_before_the_zip_reader_ever_opens_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("404.html");
        std::fs::write(&path, b"<html>not found</html>").unwrap();

        let err = read_counts(&path).unwrap_err().to_string();
        assert!(err.contains("not a .rebornroster"), "{err}");
    }

    #[test]
    fn read_counts_refuses_a_zip_that_is_not_a_roster() {
        // A real, valid zip -- opens fine -- but missing the three entries
        // that make something a .rebornroster. Must be caught by
        // looks_like_roster before the zip reader ever opens it, so this
        // reads exactly like the junk case above, not like a missing-entry
        // error from inside the zip crate.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("other.zip");
        std::fs::write(&path, write_zip(&[("readme.txt", b"hello")])).unwrap();

        let err = read_counts(&path).unwrap_err().to_string();
        assert!(err.contains("not a .rebornroster"), "{err}");
    }

    #[test]
    fn read_counts_refuses_a_manifest_with_no_counts() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.rebornroster");
        std::fs::write(&path, write_zip(&[("manifest.json", b"{}"), ("players.json", b"[]"), ("teams.json", b"[]")])).unwrap();

        let err = read_counts(&path).unwrap_err().to_string();
        assert!(err.contains("\"counts\""), "{err}");
    }

    #[test]
    fn read_counts_refuses_a_file_that_does_not_exist() {
        let err = read_counts(Path::new(r"C:\nowhere\at\all.rebornroster")).unwrap_err();
        assert!(matches!(err, Error::Io { .. }), "{err:?}");
    }
}
