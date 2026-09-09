//! What the bucket serves, what sits in the local origin, and the gap
//! between them.
//!
//! [`read_status`] is one read that never fails as a whole. A local origin
//! that does not exist, a bucket that cannot be reached, a release whose
//! signature does not verify: each becomes one line in [`Status::problems`],
//! and every other part of the status still fills in. A publisher whose
//! bucket is unreachable still needs to see what is in the local origin, and
//! one whose local origin is missing still needs to see what players are
//! being served. This is meant to be the opening page of the publisher app:
//! the first thing an operator needs is not a command to run, it is an
//! honest picture of the three things that can drift apart.

use std::path::Path;

use crate::content::SignatureVerifier;
use crate::inventory::ComponentId;
use crate::manifest::{Component, Release};
use crate::net::Http;
use crate::origin::{Origin, RELEASE_FILE, SIGNATURE_FILE};
use crate::publish::COMPONENTS_DIR;
use crate::signing::decode_signature;
use crate::update::{self, ReleaseManifest};

/// The whole picture: the local origin, what the bucket serves, what the
/// launcher's own update channel serves, and the gap between the first two.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    pub local: Option<LocalOrigin>,
    pub hosted: Option<HostedRelease>,
    pub launcher: Option<HostedLauncher>,
    /// Components published locally that the bucket does not serve, or
    /// serves at a different version.
    pub pending: Vec<Pending>,
    pub problems: Vec<String>,
}

/// What `origin`, on this machine, holds: the signed release it has and
/// every component manifest sitting under `components/`, pinned or not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalOrigin {
    pub version: String,
    pub sequence: u64,
    pub pins: Vec<(ComponentId, String)>,
    /// Every component manifest found under `components/`, whether or not
    /// the local release pins it: `(component, version, sequence)`. A
    /// component published but never carried into a signed release shows up
    /// here and nowhere else, which is how a half-finished publish is
    /// noticed.
    pub components_on_disk: Vec<(ComponentId, String, u64)>,
}

/// What `release.json` at the bucket says, read over the network.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostedRelease {
    pub version: String,
    pub sequence: u64,
    pub pins: Vec<(ComponentId, String)>,
    /// Whether the signature over these exact bytes opened with a
    /// compiled-in key. `false` covers both "checked and failed" and
    /// "could not be checked" (unreachable signature file, unreadable
    /// text, no trust anchor compiled in): either way nothing here should
    /// be trusted without reading why in [`Status::problems`].
    pub verified: bool,
}

/// What the launcher's own update manifest serves for this platform.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostedLauncher {
    pub version: String,
    pub bytes: u64,
    pub sha256: String,
    pub file_name: String,
}

/// One component published locally that the bucket does not yet serve the
/// same way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pending {
    pub component: ComponentId,
    pub local: String,
    pub hosted: Option<String>,
    pub reason: PendingReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingReason {
    /// The bucket's release does not pin this component at all.
    NotServed,
    /// The bucket pins a different version of this component, and the local
    /// release's sequence does not exceed the bucket's, so a pending
    /// wholesale re-release does not explain the gap. Most often this is a
    /// component republished on disk without yet re-signing the release
    /// around it.
    OlderOnBucket,
    /// The local release carries a strictly higher sequence than the one
    /// the bucket serves, so every pin that differs is explained by that:
    /// a new release was signed locally and has not been uploaded yet.
    LocalReleaseNewer,
}

/// Read what the bucket serves, what the local origin holds, and what the
/// launcher's own update channel serves, and diff the first two against
/// each other. Never fails: every source that could not be read becomes a
/// line in `problems`, and the sources that could be read still fill in.
pub fn read_status(
    http: &dyn Http,
    origin: &Path,
    origin_base: &str,
    launcher_manifest: &str,
    verifier: &dyn SignatureVerifier,
) -> Status {
    let mut problems = Vec::new();
    let local = read_local_origin(origin, &mut problems);
    let hosted = read_hosted_release(http, origin_base, verifier, &mut problems);
    let launcher = read_hosted_launcher(http, launcher_manifest, &mut problems);
    let pending = local.as_ref().map(|l| compute_pending(l, hosted.as_ref())).unwrap_or_default();
    Status { local, hosted, launcher, pending, problems }
}

// -- the local origin -------------------------------------------------------

fn read_local_origin(origin: &Path, problems: &mut Vec<String>) -> Option<LocalOrigin> {
    let release_path = origin.join(RELEASE_FILE);
    let bytes = match std::fs::read(&release_path) {
        Ok(b) => b,
        Err(e) => {
            problems.push(format!(
                "{} could not be read ({e}). No release has been signed in this local \
                 origin yet, or the folder does not exist. Only the bucket and the \
                 launcher channel are shown.",
                release_path.display()
            ));
            return None;
        }
    };
    let release = match Release::parse(&bytes) {
        Ok(r) => r,
        Err(e) => {
            problems.push(format!(
                "{} does not parse as a release manifest ({e}). The local origin's \
                 pins could not be read.",
                release_path.display()
            ));
            return None;
        }
    };

    let mut pins = Vec::new();
    for (name, pinned) in &release.components {
        let Some(id) = ComponentId::parse(name) else {
            problems.push(format!(
                "{} pins a component named {name:?}, which this build does not \
                 recognize. That pin was skipped.",
                release_path.display()
            ));
            continue;
        };
        let manifest_path =
            origin.join(pinned.url.trim_start_matches('/').replace('/', std::path::MAIN_SEPARATOR_STR));
        match std::fs::read(&manifest_path) {
            Ok(manifest_bytes) => match Component::parse(&manifest_bytes) {
                Ok(component) => pins.push((id, component.version)),
                Err(e) => problems.push(format!(
                    "{} does not parse as a component manifest ({e}), but {} pins it \
                     as {name}.",
                    manifest_path.display(),
                    release_path.display()
                )),
            },
            Err(e) => problems.push(format!(
                "{} pins {name} at {}, but that file could not be read ({e}). The \
                 local origin is missing a manifest its own release names.",
                release_path.display(),
                manifest_path.display()
            )),
        }
    }
    pins.sort_by(|a, b| a.0.as_str().cmp(b.0.as_str()));

    let components_on_disk = scan_components_dir(origin, problems);

    Some(LocalOrigin { version: release.version, sequence: release.sequence, pins, components_on_disk })
}

/// Every manifest under `origin/components/`, whether or not any release
/// pins it. A missing directory is not a problem worth reporting: an origin
/// that has never published a component looks exactly like this.
fn scan_components_dir(origin: &Path, problems: &mut Vec<String>) -> Vec<(ComponentId, String, u64)> {
    let dir = origin.join(COMPONENTS_DIR);
    let mut out = Vec::new();
    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return out,
        Err(e) => {
            problems.push(format!(
                "{} could not be listed ({e}). Published components on disk are not shown.",
                dir.display()
            ));
            return out;
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(e) => {
                problems.push(format!("{} could not be read ({e}). Skipped.", path.display()));
                continue;
            }
        };
        let component = match Component::parse(&bytes) {
            Ok(c) => c,
            Err(e) => {
                problems.push(format!(
                    "{} does not parse as a component manifest ({e}). Skipped.",
                    path.display()
                ));
                continue;
            }
        };
        match ComponentId::parse(&component.component) {
            Some(id) => out.push((id, component.version.clone(), component.sequence)),
            None => problems.push(format!(
                "{} names the component {:?}, which this build does not recognize. Skipped.",
                path.display(),
                component.component
            )),
        }
    }
    out.sort_by(|a, b| (a.0.as_str(), a.1.as_str()).cmp(&(b.0.as_str(), b.1.as_str())));
    out
}

// -- the hosted release -------------------------------------------------------

fn read_hosted_release(
    http: &dyn Http,
    origin_base: &str,
    verifier: &dyn SignatureVerifier,
    problems: &mut Vec<String>,
) -> Option<HostedRelease> {
    let bucket = Origin::new(origin_base);
    let release_url = bucket.url(RELEASE_FILE);
    let sig_url = bucket.url(SIGNATURE_FILE);

    let reply = match http.get_text(&release_url) {
        Ok(r) => r,
        Err(e) => {
            problems.push(format!(
                "Could not reach {release_url} ({e}). The bucket's state is unknown; \
                 only the local origin and the launcher channel are shown."
            ));
            return None;
        }
    };
    if !reply.is_success() {
        problems.push(format!(
            "{release_url} answered {}. The bucket's release could not be read.",
            reply.status
        ));
        return None;
    }
    let release = match Release::parse(reply.body.as_bytes()) {
        Ok(r) => r,
        Err(e) => {
            problems.push(format!("{release_url} does not parse as a release manifest ({e})."));
            return None;
        }
    };

    let verified = check_signature(http, &release_url, &sig_url, reply.body.as_bytes(), verifier, problems);

    let mut pins = Vec::new();
    for (name, pinned) in &release.components {
        match ComponentId::parse(name) {
            Some(id) => match version_from_manifest_url(name, &pinned.url) {
                Some(version) => pins.push((id, version)),
                None => problems.push(format!(
                    "{release_url} pins {name:?} at {:?}, which does not follow the \
                     <component>-<version>.json layout every publish writes. That pin \
                     was skipped.",
                    pinned.url
                )),
            },
            None => problems.push(format!(
                "{release_url} pins a component named {name:?}, which this build does \
                 not recognize. That pin was skipped."
            )),
        }
    }
    pins.sort_by(|a, b| a.0.as_str().cmp(b.0.as_str()));

    Some(HostedRelease { version: release.version, sequence: release.sequence, pins, verified })
}

/// Fetch and check the detached signature over `signed_bytes`, exactly as
/// [`crate::origin::Origin::fetch_release`] would, but split from parsing so
/// a release that fails to verify still has its version and sequence read
/// and shown.
fn check_signature(
    http: &dyn Http,
    release_url: &str,
    sig_url: &str,
    signed_bytes: &[u8],
    verifier: &dyn SignatureVerifier,
    problems: &mut Vec<String>,
) -> bool {
    let reply = match http.get_text(sig_url) {
        Ok(r) => r,
        Err(e) => {
            problems.push(format!(
                "Could not reach {sig_url} ({e}). {release_url}'s signature could not \
                 be checked."
            ));
            return false;
        }
    };
    if !reply.is_success() {
        problems.push(format!(
            "{sig_url} answered {}. {release_url}'s signature could not be checked.",
            reply.status
        ));
        return false;
    }
    let Some(signature) = decode_signature(&reply.body) else {
        problems.push(format!(
            "{sig_url} is not a signature: expected 128 hex characters. \
             {release_url}'s signature could not be checked."
        ));
        return false;
    };
    match verifier.verify(signed_bytes, &signature) {
        Ok(true) => true,
        Ok(false) => {
            problems.push(format!(
                "{release_url}'s signature at {sig_url} did not verify. The release was \
                 altered in transit, or was signed by a key this build does not trust."
            ));
            false
        }
        Err(e) => {
            problems.push(format!("{release_url}'s signature could not be checked: {e}"));
            false
        }
    }
}

/// The version embedded in a manifest's file name, `<component>-<version>.json`,
/// the layout [`crate::publish::manifest_url`] always writes. Read from the
/// name rather than fetched, so reading what the bucket serves costs exactly
/// one request for the release and one for its signature, not one more per
/// pinned component.
fn version_from_manifest_url(component: &str, url: &str) -> Option<String> {
    let name = url.rsplit(['/', '\\']).next()?;
    let stem = name.strip_suffix(".json")?;
    let version = stem.strip_prefix(component)?.strip_prefix('-')?;
    (!version.is_empty()).then(|| version.to_string())
}

// -- the launcher's own update channel ---------------------------------------

fn read_hosted_launcher(
    http: &dyn Http,
    launcher_manifest: &str,
    problems: &mut Vec<String>,
) -> Option<HostedLauncher> {
    let reply = match http.get_text(launcher_manifest) {
        Ok(r) => r,
        Err(e) => {
            problems.push(format!(
                "Could not reach {launcher_manifest} ({e}). The launcher version being \
                 served is unknown."
            ));
            return None;
        }
    };
    if !reply.is_success() {
        problems.push(format!(
            "{launcher_manifest} answered {}. The launcher version being served is unknown.",
            reply.status
        ));
        return None;
    }
    let manifest: ReleaseManifest = match serde_json::from_str(&reply.body) {
        Ok(m) => m,
        Err(e) => {
            problems.push(format!(
                "{launcher_manifest} does not parse as a launcher release manifest ({e})."
            ));
            return None;
        }
    };
    if manifest.schema != update::MANIFEST_SCHEMA {
        problems.push(format!(
            "{launcher_manifest} has schema {:?}, this build reads {:?}. The launcher \
             version being served is unknown.",
            manifest.schema,
            update::MANIFEST_SCHEMA
        ));
        return None;
    }
    match manifest.asset_for_this_platform() {
        Some(asset) => Some(HostedLauncher {
            version: manifest.version.clone(),
            bytes: asset.bytes,
            sha256: asset.sha256.as_str().to_string(),
            file_name: asset.file_name.clone(),
        }),
        None => {
            problems.push(format!(
                "{launcher_manifest} publishes version {}, but names no asset for this \
                 platform.",
                manifest.version
            ));
            None
        }
    }
}

// -- the diff -----------------------------------------------------------

fn compute_pending(local: &LocalOrigin, hosted: Option<&HostedRelease>) -> Vec<Pending> {
    let Some(hosted) = hosted else { return Vec::new() };
    let mut pending = Vec::new();
    for (id, local_version) in &local.pins {
        let hosted_version = hosted.pins.iter().find(|(hid, _)| hid == id).map(|(_, v)| v.clone());
        match hosted_version {
            None => pending.push(Pending {
                component: *id,
                local: local_version.clone(),
                hosted: None,
                reason: PendingReason::NotServed,
            }),
            Some(hv) if &hv != local_version => {
                let reason = if local.sequence > hosted.sequence {
                    PendingReason::LocalReleaseNewer
                } else {
                    PendingReason::OlderOnBucket
                };
                pending.push(Pending { component: *id, local: local_version.clone(), hosted: Some(hv), reason });
            }
            Some(_) => {}
        }
    }
    pending
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::fake::FakeHttp;
    use crate::signing::{encode_signature, Ed25519Verifier};
    use ed25519_dalek::{Signer as _, SigningKey};

    fn keypair(seed: u8) -> (SigningKey, [u8; 32]) {
        let sk = SigningKey::from_bytes(&[seed; 32]);
        let pk = sk.verifying_key().to_bytes();
        (sk, pk)
    }

    fn write_component(origin: &Path, id: ComponentId, version: &str, sequence: u64) {
        let dir = origin.join(COMPONENTS_DIR);
        std::fs::create_dir_all(&dir).unwrap();
        let name = id.as_str();
        let text = format!(
            r#"{{"schema":"nsr.component.v1","component":"{name}","version":"{version}","sequence":{sequence},"install_root":"x","customizable":false,"files":[]}}"#
        );
        std::fs::write(dir.join(format!("{name}-{version}.json")), text).unwrap();
    }

    /// `{"runtime":{"url":"components/runtime-1.0.0.json","sha256":"aaa..."},...}`
    fn pins_json(pins: &[(ComponentId, &str)]) -> String {
        let mut comps = String::new();
        for (i, (id, ver)) in pins.iter().enumerate() {
            if i > 0 {
                comps.push(',');
            }
            let name = id.as_str();
            comps.push_str(&format!(
                r#""{name}":{{"url":"components/{name}-{ver}.json","sha256":"{}"}}"#,
                "a".repeat(64)
            ));
        }
        format!("{{{comps}}}")
    }

    fn release_body(version: &str, sequence: u64, pins: &[(ComponentId, &str)]) -> String {
        let components_json = pins_json(pins);
        format!(
            r#"{{"schema":"nsr.release.v1","version":"{version}","sequence":{sequence},"components":{components_json}}}"#
        )
    }

    fn write_local_release(origin: &Path, version: &str, sequence: u64, pins: &[(ComponentId, &str)]) {
        std::fs::write(origin.join(RELEASE_FILE), release_body(version, sequence, pins)).unwrap();
    }

    /// Adds a signed `release.json` and `release.json.sig` to `http` at `base`.
    fn host_release(
        http: FakeHttp,
        base: &str,
        key: &SigningKey,
        version: &str,
        sequence: u64,
        pins: &[(ComponentId, &str)],
    ) -> FakeHttp {
        let body = release_body(version, sequence, pins);
        let sig = format!("{}\n", encode_signature(&key.sign(body.as_bytes()).to_bytes()));
        let bucket = Origin::new(base);
        http.reply(&bucket.url(RELEASE_FILE), 200, body).reply(&bucket.url(SIGNATURE_FILE), 200, sig)
    }

    fn host_launcher(http: FakeHttp, url: &str, version: &str) -> FakeHttp {
        let body = format!(
            r#"{{"schema":"{}","version":"{version}","windows":{{"url":"https://x/setup.exe","sha256":"{}","bytes":123,"file_name":"Setup.exe"}}}}"#,
            update::MANIFEST_SCHEMA,
            "c".repeat(64)
        );
        http.reply(url, 200, body)
    }

    #[test]
    fn a_newer_local_release_marks_every_differing_pin_and_leaves_unpinned_manifests_out_of_pending() {
        let dir = tempfile::tempdir().unwrap();
        let origin = dir.path().join("origin");
        std::fs::create_dir_all(&origin).unwrap();
        write_component(&origin, ComponentId::Runtime, "1.0.4", 4);
        write_component(&origin, ComponentId::Roster, "2026.09.06", 4);
        write_local_release(
            &origin,
            "3.1.0",
            4,
            &[(ComponentId::Runtime, "1.0.4"), (ComponentId::Roster, "2026.09.06")],
        );
        // Published, but not part of any signed local release: this is the
        // half-finished-publish case the tool exists to surface.
        write_component(&origin, ComponentId::Mods, "9.9.9", 1);

        let (sk, pk) = keypair(1);
        let verifier = Ed25519Verifier::with_keys(&[pk]).unwrap();
        let http = FakeHttp::default();
        let http = host_release(
            http,
            "http://bucket/",
            &sk,
            "3.0.0",
            3,
            &[(ComponentId::Runtime, "1.0.3"), (ComponentId::Roster, "2026.09.06")],
        );
        let http = host_launcher(http, "http://bucket/launcher/latest.json", "5.0.0");

        let status =
            read_status(&http, &origin, "http://bucket/", "http://bucket/launcher/latest.json", &verifier);

        assert!(status.problems.is_empty(), "{:?}", status.problems);

        let local = status.local.expect("local reads");
        assert_eq!(local.version, "3.1.0");
        assert_eq!(local.sequence, 4);
        assert!(
            local.components_on_disk.iter().any(|(id, v, seq)| *id == ComponentId::Mods && v == "9.9.9" && *seq == 1),
            "{:?}",
            local.components_on_disk
        );

        let hosted = status.hosted.expect("hosted reads");
        assert_eq!(hosted.sequence, 3);
        assert!(hosted.verified);

        assert_eq!(status.pending.len(), 1, "only the differing pin, not the identical roster one: {:?}", status.pending);
        let p = &status.pending[0];
        assert_eq!(p.component, ComponentId::Runtime);
        assert_eq!(p.local, "1.0.4");
        assert_eq!(p.hosted.as_deref(), Some("1.0.3"));
        assert_eq!(p.reason, PendingReason::LocalReleaseNewer);
        assert!(
            !status.pending.iter().any(|p| p.component == ComponentId::Mods),
            "an unpinned manifest on disk is never pending"
        );

        let launcher = status.launcher.expect("launcher channel reads");
        assert_eq!(launcher.version, "5.0.0");
        assert_eq!(launcher.file_name, "Setup.exe");
    }

    #[test]
    fn a_pin_that_differs_without_a_local_sequence_bump_is_older_on_bucket() {
        let dir = tempfile::tempdir().unwrap();
        let origin = dir.path().join("origin");
        std::fs::create_dir_all(&origin).unwrap();
        write_component(&origin, ComponentId::Runtime, "1.0.0", 2);
        write_local_release(&origin, "3.0.0", 2, &[(ComponentId::Runtime, "1.0.0")]);

        let (sk, pk) = keypair(2);
        let verifier = Ed25519Verifier::with_keys(&[pk]).unwrap();
        let http = FakeHttp::default();
        // The bucket's release carries a HIGHER sequence than the local one,
        // so a pending wholesale re-release cannot explain the difference.
        let http = host_release(http, "http://bucket/", &sk, "5.0.0", 9, &[(ComponentId::Runtime, "1.0.1")]);
        let http = host_launcher(http, "http://bucket/launcher/latest.json", "1.0.0");

        let status =
            read_status(&http, &origin, "http://bucket/", "http://bucket/launcher/latest.json", &verifier);

        assert_eq!(status.pending.len(), 1, "{:?}", status.pending);
        assert_eq!(status.pending[0].reason, PendingReason::OlderOnBucket);
    }

    #[test]
    fn a_component_the_bucket_does_not_pin_at_all_is_not_served() {
        let dir = tempfile::tempdir().unwrap();
        let origin = dir.path().join("origin");
        std::fs::create_dir_all(&origin).unwrap();
        write_component(&origin, ComponentId::Textures, "2.0.0", 1);
        write_local_release(&origin, "3.0.0", 1, &[(ComponentId::Textures, "2.0.0")]);

        let (sk, pk) = keypair(3);
        let verifier = Ed25519Verifier::with_keys(&[pk]).unwrap();
        let http = FakeHttp::default();
        let http = host_release(http, "http://bucket/", &sk, "3.0.0", 1, &[]);
        let http = host_launcher(http, "http://bucket/launcher/latest.json", "1.0.0");

        let status =
            read_status(&http, &origin, "http://bucket/", "http://bucket/launcher/latest.json", &verifier);

        assert_eq!(status.pending.len(), 1);
        assert_eq!(status.pending[0].component, ComponentId::Textures);
        assert_eq!(status.pending[0].hosted, None);
        assert_eq!(status.pending[0].reason, PendingReason::NotServed);
    }

    #[test]
    fn an_unreachable_bucket_names_the_url_and_leaves_the_local_origin_fully_populated() {
        let dir = tempfile::tempdir().unwrap();
        let origin = dir.path().join("origin");
        std::fs::create_dir_all(&origin).unwrap();
        write_component(&origin, ComponentId::Runtime, "1.0.0", 1);
        write_local_release(&origin, "3.0.0", 1, &[(ComponentId::Runtime, "1.0.0")]);

        let verifier = Ed25519Verifier::with_keys(&[keypair(4).1]).unwrap();
        let http = FakeHttp::default(); // no routes at all: everything is offline

        let status =
            read_status(&http, &origin, "http://bucket/", "http://bucket/launcher/latest.json", &verifier);

        let local = status.local.expect("local still reads with the bucket unreachable");
        assert_eq!(local.version, "3.0.0");
        assert_eq!(local.pins, vec![(ComponentId::Runtime, "1.0.0".to_string())]);
        assert!(status.hosted.is_none());
        assert!(status.launcher.is_none());

        let release_url = Origin::new("http://bucket/").url(RELEASE_FILE);
        assert!(
            status.problems.iter().any(|p| p.contains(&release_url)),
            "a problem must name the unreachable URL: {:?}",
            status.problems
        );
    }

    #[test]
    fn a_release_whose_signature_does_not_verify_still_shows_its_version_and_sequence() {
        let dir = tempfile::tempdir().unwrap();
        let origin = dir.path().join("origin"); // no local origin needed for this one

        let (signed_with, _) = keypair(5);
        let (_, trusted_key) = keypair(6); // a different key: the signature will not verify
        let verifier = Ed25519Verifier::with_keys(&[trusted_key]).unwrap();
        let http = FakeHttp::default();
        let http = host_release(http, "http://bucket/", &signed_with, "7.2.1", 42, &[(ComponentId::Runtime, "1.0.0")]);

        let status =
            read_status(&http, &origin, "http://bucket/", "http://bucket/launcher/latest.json", &verifier);

        let hosted = status.hosted.expect("version and sequence still parse");
        assert_eq!(hosted.version, "7.2.1");
        assert_eq!(hosted.sequence, 42);
        assert!(!hosted.verified);
        assert!(
            status.problems.iter().any(|p| p.contains("did not verify")),
            "{:?}",
            status.problems
        );
    }

    #[test]
    fn no_local_origin_at_all_is_exactly_one_problem_and_the_hosted_side_still_reads() {
        let dir = tempfile::tempdir().unwrap();
        let origin = dir.path().join("does-not-exist");

        let (sk, pk) = keypair(7);
        let verifier = Ed25519Verifier::with_keys(&[pk]).unwrap();
        let http = FakeHttp::default();
        let http = host_release(http, "http://bucket/", &sk, "1.0.0", 1, &[(ComponentId::Runtime, "1.0.0")]);
        let http = host_launcher(http, "http://bucket/launcher/latest.json", "1.0.0");

        let status =
            read_status(&http, &origin, "http://bucket/", "http://bucket/launcher/latest.json", &verifier);

        assert!(status.local.is_none());
        assert_eq!(status.problems.len(), 1, "{:?}", status.problems);
        assert!(status.problems[0].contains(&origin.join(RELEASE_FILE).display().to_string()));
        assert!(status.hosted.is_some());
        assert!(status.launcher.is_some());
    }

    #[test]
    fn an_unreachable_launcher_channel_names_the_url_without_blocking_the_rest() {
        let dir = tempfile::tempdir().unwrap();
        let origin = dir.path().join("origin");
        std::fs::create_dir_all(&origin).unwrap();
        write_component(&origin, ComponentId::Runtime, "1.0.0", 1);
        write_local_release(&origin, "1.0.0", 1, &[(ComponentId::Runtime, "1.0.0")]);

        let (sk, pk) = keypair(8);
        let verifier = Ed25519Verifier::with_keys(&[pk]).unwrap();
        let http = FakeHttp::default();
        let http = host_release(http, "http://bucket/", &sk, "1.0.0", 1, &[(ComponentId::Runtime, "1.0.0")]);
        // No reply registered for the launcher manifest: it is unreachable.

        let status =
            read_status(&http, &origin, "http://bucket/", "http://bucket/launcher/latest.json", &verifier);

        assert!(status.local.is_some());
        assert!(status.hosted.is_some());
        assert!(status.launcher.is_none());
        assert!(
            status.problems.iter().any(|p| p.contains("http://bucket/launcher/latest.json")),
            "{:?}",
            status.problems
        );
    }
}
