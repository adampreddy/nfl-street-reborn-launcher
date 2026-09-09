//! Fetch the roster the site published and push it: the component, then a
//! release that carries every other pinned component forward.

use std::path::{Path, PathBuf};

use crate::inventory::{ComponentId, ComponentSpec, ScanObserver};
use crate::net::Http;
use crate::publish::component::{publish_component, ComponentRequest, Published};
use crate::publish::release::{republish_pinning, CarryForward, Signed};
use crate::publish::{io_err, next_sequence, out, refuse, thousands, Outcome, Sink};
use crate::roster::ROSTER_FILE;
use crate::Result;

#[derive(Debug)]
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
    let looks_like_zip = head.starts_with(b"PK\x03\x04");
    let names_present = [&b"manifest.json"[..], &b"players.json"[..], &b"teams.json"[..]]
        .iter()
        .all(|n| head.windows(n.len()).any(|w| w == *n));
    if !looks_like_zip || !names_present {
        return Ok(refuse(sink, &[
            String::new(),
            "That is not a .rebornroster: expected a zip holding manifest.json, players.json".to_string(),
            format!(
                "and teams.json. Got {} bytes starting {:?}. Nothing was published.",
                head.len(),
                String::from_utf8_lossy(&head[..head.len().min(16)])
            ),
        ]));
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
        let req = RosterRequest {
            from: "https://site.test/export", origin: &origin, version: "2026.09.06", sequence: None,
            key: Some(&key), release_version: None, release_sequence: None, apply: true, token: Some("tok"),
        };
        let outcome = publish_roster(&site, &req, &RunToCompletion, &mut |e: Event| lines.push(e.line)).unwrap();
        let push = match outcome { Outcome::Done(p) => p, o => panic!("{o:?}") };
        assert_eq!(push.release.as_ref().map(|s| s.sequence), Some(4));
        assert_eq!(*site.seen_bearer.lock().unwrap(), Some(Some("tok".to_string())));
        assert!(lines.iter().any(|l| l.starts_with("Fetched")), "{lines:?}");
        assert!(lines.iter().any(|l| l == "Sequence      1 (one above the highest roster already in the origin)"), "{lines:?}");
        assert!(lines.iter().any(|l| l.contains("pinning roster=2026.09.06, runtime=1.0.2")), "{lines:?}");
        assert!(origin.join("components").join("roster-2026.09.06.json").is_file());
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
}
