//! Assemble release.json from components already in the origin, sign it, and
//! prove the signed file verifies before saying so.

use std::path::{Path, PathBuf};

use crate::content::TrustedRelease;
use crate::inventory::{ComponentId, SerializedComponent, UnsignedReleaseBuilder};
use crate::manifest::{Component, Release, Sha256Hex};
use crate::origin::{blob_path, RELEASE_FILE, SIGNATURE_FILE};
use crate::publish::keys::read_key;
use crate::publish::{io_err, manifest_path, manifest_url, out, refuse, thousands, Outcome, Sink};
use crate::signing::{self, publisher, Ed25519Verifier};
use crate::{Error, Result};

/// One component at one version, to pin in a release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pin {
    pub id: ComponentId,
    pub version: String,
}

#[derive(Debug)]
pub struct ReleaseRequest<'a> {
    pub origin: &'a Path,
    /// The label players see, for example 3.0.0.
    pub version: &'a str,
    /// Strictly higher than the release already in the origin.
    pub sequence: u64,
    /// The publisher.key file.
    pub key: &'a Path,
    pub pins: &'a [Pin],
    /// Write and sign. Without it everything is checked and nothing written.
    pub apply: bool,
}

/// A signed release, read back and verified.
#[derive(Debug, Clone)]
pub struct Signed {
    pub release_path: PathBuf,
    pub sig_path: PathBuf,
    pub version: String,
    pub sequence: u64,
    pub public_hex: String,
    /// Whether the signing key's public half is compiled into shipped launchers.
    pub shipped: bool,
    pub files: u64,
    pub bytes: u64,
}

/// Re-sign the release with one pin replaced or added and every other pin
/// carried forward at the version it is served at.
#[derive(Debug)]
pub struct CarryForward<'a> {
    pub origin: &'a Path,
    pub id: ComponentId,
    pub version: &'a str,
    /// Without a key the release step is only described.
    pub key: Option<&'a Path>,
    /// Defaults to the label already served.
    pub release_version: Option<&'a str>,
    /// Defaults to one above the release already served.
    pub release_sequence: Option<u64>,
    pub apply: bool,
}

/// Assemble `release.json` from components already published to `req.origin`,
/// sign it, and read the signed file back to prove it verifies before saying
/// so. Without `req.apply` everything is checked and described, nothing is
/// written.
pub fn release(req: &ReleaseRequest<'_>, sink: Sink<'_>) -> Result<Outcome<Signed>> {
    let (origin, version, sequence, key, apply) = (req.origin, req.version, req.sequence, req.key, req.apply);

    if req.pins.is_empty() {
        return Ok(refuse(sink, &["A release pinning nothing is refused: it is useless to a client.".to_string()]));
    }

    // The key first, because everything else is wasted without it.
    let Some(secret) = read_key(key)? else {
        return Ok(refuse(sink, &[format!("{} does not hold a key: expected 64 hex characters.", key.display())]));
    };
    let public = publisher::public_of(&secret);
    let public_hex = signing::encode_public_key(&public);
    let shipped = signing::TRUSTED_KEYS.contains(&public);

    // Replay protection at the publisher, mirroring the client's rule: a
    // release that is not strictly newer than the one already served would be
    // refused by every launcher that has seen the current one.
    let release_path = origin.join(RELEASE_FILE);
    if release_path.is_file() {
        let bytes = std::fs::read(&release_path).map_err(|e| io_err(&release_path, e))?;
        match Release::parse(&bytes) {
            Ok(current) if sequence <= current.sequence => {
                return Ok(refuse(sink, &[
                    format!(
                        "The origin already serves release {} at sequence {}. A new release must",
                        current.version, current.sequence
                    ),
                    "carry a strictly higher sequence, or every launcher that has seen the".to_string(),
                    "current one refuses it as a replay. Nothing was written.".to_string(),
                ]));
            }
            Ok(current) => out!(sink, "Replacing     release {} (sequence {})", current.version, current.sequence),
            Err(e) => {
                return Ok(refuse(sink, &[
                    format!("{} exists but does not parse: {e}", release_path.display()),
                    "Refusing to write over something this tool cannot read. Move it aside.".to_string(),
                ]));
            }
        }
    }

    out!(sink, "Origin        {}", origin.display());
    out!(sink, "Release       {version}, sequence {sequence}");
    out!(sink, "Signing key   {} (public {})", key.display(), public_hex);
    out!(sink);

    let mut builder = UnsignedReleaseBuilder::new(version, sequence)?;
    let mut total_files = 0u64;
    let mut total_bytes = 0u64;
    for pin in req.pins {
        let (id, name, ver) = (pin.id, pin.id.as_str(), pin.version.as_str());
        let path = manifest_path(origin, id, ver);
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(e) => {
                return Ok(refuse(sink, &[
                    format!("{}: {e}", path.display()),
                    format!("Publish the component first: rebornctl publish component --component {name} --version {ver} ..."),
                ]));
            }
        };
        let parsed = Component::parse(&bytes).map_err(|e| {
            Error::PlainIo(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("{} is not a component manifest: {e}", path.display()),
            ))
        })?;
        if parsed.component != name || parsed.version != ver {
            return Ok(refuse(sink, &[
                format!(
                    "{} says it is {} {}, not {name} {ver}. The file name and its contents disagree;",
                    path.display(),
                    parsed.component,
                    parsed.version
                ),
                "nothing was pinned.".to_string(),
            ]));
        }

        // The pin must be the hash of the FILE. SerializedComponent hashes its
        // own serialization, so prove that serialization is byte for byte the
        // file on disk before trusting its hash.
        let serialized = SerializedComponent::new(parsed)?;
        let on_disk = Sha256Hex::from_bytes(&bytes);
        if serialized.sha256() != &on_disk {
            return Ok(refuse(sink, &[
                format!("{} does not round-trip: the file hashes to {on_disk} but its", path.display()),
                format!("serialization hashes to {}. This tool did not write it, or something", serialized.sha256()),
                "reformatted it. Pinning either hash would break every client; nothing was pinned.".to_string(),
            ]));
        }

        // Every blob the manifest names, present at its published length.
        let mut missing = Vec::new();
        for file in &serialized.component().files {
            let blob = origin.join(blob_path(&file.sha256));
            match std::fs::metadata(&blob) {
                Ok(m) if m.len() == file.bytes => {}
                _ => missing.push(file.path.clone()),
            }
        }
        if !missing.is_empty() {
            return Ok(refuse(sink, &[
                format!("{name} {ver}: {} of {} blob(s) are not in the origin at their published", missing.len(), serialized.component().files.len()),
                format!("length, starting with {:?}. A release must never name content the origin", missing[0]),
                "does not hold. Re-run publish component for it. Nothing was pinned.".to_string(),
            ]));
        }

        let files = serialized.component().files.len() as u64;
        let bytes_total = serialized.component().total_bytes();
        total_files += files;
        total_bytes += bytes_total;
        out!(
            sink,
            "Pin           {name} {ver}: {} file(s), {} bytes, manifest sha256 {}",
            thousands(files),
            thousands(bytes_total),
            serialized.sha256()
        );
        builder = builder.pin(&manifest_url(id, ver), &serialized)?;
    }
    let unsigned = builder.build()?;
    let signature = publisher::sign(&secret, unsigned.bytes_to_sign());
    let sig_text = format!("{}\n", signing::encode_signature(&signature));

    out!(sink);
    out!(sink, "Total         {} file(s), {} bytes across {} component(s)", thousands(total_files), thousands(total_bytes), req.pins.len());
    out!(sink, "Would write   {}", release_path.display());
    out!(sink, "Would write   {}", origin.join(SIGNATURE_FILE).display());
    if !shipped {
        out!(sink);
        out!(sink, "WARNING: this key's public half is not in TRUSTED_KEYS in signing.rs, so no");
        out!(sink, "shipped launcher will accept this release. A launcher built after pasting");
        out!(sink, "  {public_hex}");
        out!(sink, "into TRUSTED_KEYS will. rebornctl update content --trust <hex> can test it now.");
    }

    if !apply {
        out!(sink);
        out!(sink, "Nothing was written. Re-run with --apply to write and sign.");
        return Ok(Outcome::Described);
    }

    // Signature first, then the release: a release.json with a stale or
    // absent .sig beside it is an outage for every client, and a .sig with no
    // matching release is a file nobody reads.
    let sig_path = origin.join(SIGNATURE_FILE);
    std::fs::write(&sig_path, &sig_text).map_err(|e| io_err(&sig_path, e))?;
    unsigned.write_to(&release_path)?;

    // Read both back and verify, the way a launcher will.
    let served = std::fs::read(&release_path).map_err(|e| io_err(&release_path, e))?;
    let served_sig = std::fs::read_to_string(&sig_path).map_err(|e| io_err(&sig_path, e))?;
    let decoded = signing::decode_signature(&served_sig).ok_or_else(|| {
        Error::PlainIo(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "the signature file this tool just wrote does not decode",
        ))
    })?;
    let verifier = Ed25519Verifier::with_keys(&[public])?;
    let trusted = TrustedRelease::open(&verifier, &served, &decoded)?;

    out!(sink);
    out!(sink, "Wrote {}", release_path.display());
    out!(sink, "Wrote {}", sig_path.display());
    out!(
        sink,
        "Read back and verified: release {} sequence {} opens with public key {}",
        trusted.release().version,
        trusted.release().sequence,
        public_hex
    );
    out!(sink, "That is the file a launcher fetches. Upload the origin directory as it is.");
    Ok(Outcome::Done(Signed {
        release_path,
        sig_path,
        version: trusted.release().version.clone(),
        sequence: trusted.release().sequence,
        public_hex,
        shipped,
        files: total_files,
        bytes: total_bytes,
    }))
}

/// Re-sign the release with one component's pin replaced (or added) and
/// every other pinned component carried forward at the version it is
/// served at.
pub fn republish_pinning(req: &CarryForward<'_>, sink: Sink<'_>) -> Result<Outcome<Signed>> {
    let (origin, version, apply) = (req.origin, req.version, req.apply);
    let name = req.id.as_str();

    let release_path = origin.join(RELEASE_FILE);
    let existing = if release_path.is_file() {
        let bytes = std::fs::read(&release_path).map_err(|e| io_err(&release_path, e))?;
        Some(Release::parse(&bytes).map_err(|e| {
            Error::PlainIo(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("{} does not parse: {e}", release_path.display()),
            ))
        })?)
    } else {
        None
    };
    let mut pin_labels: Vec<String> = Vec::new();
    let mut pins: Vec<Pin> = Vec::new();
    if let Some(current) = &existing {
        for (pinned_name, pinned) in &current.components {
            if pinned_name == name {
                continue;
            }
            let manifest = origin.join(pinned.url.trim_start_matches('/').replace('/', std::path::MAIN_SEPARATOR_STR));
            let bytes = std::fs::read(&manifest).map_err(|e| io_err(&manifest, e))?;
            let component = Component::parse(&bytes).map_err(|e| {
                Error::PlainIo(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("{} does not parse: {e}", manifest.display()),
                ))
            })?;
            let Some(id) = ComponentId::parse(pinned_name) else {
                return Ok(refuse(sink, &[format!("The served release pins {pinned_name:?}, which this build does not know. Nothing was signed.")]));
            };
            pin_labels.push(format!("{pinned_name}={}", component.version));
            pins.push(Pin { id, version: component.version.clone() });
        }
    }
    pin_labels.push(format!("{name}={version}"));
    pins.push(Pin { id: req.id, version: version.to_string() });
    pin_labels.sort();
    pins.sort_by(|a, b| format!("{}={}", a.id.as_str(), a.version).cmp(&format!("{}={}", b.id.as_str(), b.version)));

    let (rv, rs) = match (&existing, req.release_version, req.release_sequence) {
        (Some(c), v, s) => (
            v.map(str::to_string).unwrap_or_else(|| c.version.clone()),
            s.unwrap_or(c.sequence + 1),
        ),
        (None, Some(v), Some(s)) => (v.to_string(), s),
        (None, _, _) => {
            out!(sink);
            out!(sink, "The origin serves no release yet, so there is nothing to carry forward.");
            out!(sink, "Pass --release-version and --release-sequence, or run publish release");
            out!(
                sink,
                "with every component. The {name} component itself {}.",
                if apply { "was published" } else { "would be published with --apply" }
            );
            return Ok(Outcome::Described);
        }
    };

    let Some(key) = req.key else {
        out!(sink);
        out!(sink, "No --key, so the release was not signed. To pin it, run:");
        out!(
            sink,
            "  rebornctl publish release --origin {} --version {rv} --sequence {rs} --key <publisher.key> {} --apply",
            origin.display(),
            pin_labels.iter().map(|p| format!("--component {p}")).collect::<Vec<_>>().join(" ")
        );
        return Ok(Outcome::Described);
    };

    out!(sink);
    out!(sink, "Release       {rv}, sequence {rs}, pinning {}", pin_labels.join(", "));
    let inner = ReleaseRequest { origin, version: &rv, sequence: rs, key, pins: &pins, apply };
    release(&inner, sink)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::{ComponentId, ComponentSpec, RunToCompletion};
    use crate::publish::component::{publish_component, ComponentRequest};
    use crate::publish::keys::keygen;
    use crate::publish::{Event, Outcome};

    struct World {
        _dir: tempfile::TempDir,
        origin: PathBuf,
        key: PathBuf,
        public: [u8; signing::PUBLIC_KEY_LEN],
    }

    fn world() -> World {
        let dir = tempfile::tempdir().unwrap();
        let origin = dir.path().join("origin");
        let keys = dir.path().join("keys");
        let written = match keygen(&keys, &mut |_| {}).unwrap() { Outcome::Done(w) => w, o => panic!("{o:?}") };
        World { _dir: dir, origin, key: written.publisher_path, public: written.everyday_public }
    }

    fn publish(w: &World, id: ComponentId, version: &str, sequence: u64, files: &[(&str, &[u8])]) {
        let root = w._dir.path().join(format!("tree-{}-{version}", id.as_str()));
        for (name, bytes) in files {
            std::fs::create_dir_all(root.join(name).parent().unwrap()).unwrap();
            std::fs::write(root.join(name), bytes).unwrap();
        }
        let spec = ComponentSpec::new(id, version, sequence).unwrap();
        let req = ComponentRequest { root: &root, spec, origin: &w.origin, apply: true };
        assert!(publish_component(&req, &RunToCompletion, &mut |_| {}).unwrap().is_done());
    }

    fn sign(w: &World, version: &str, sequence: u64, pins: &[Pin], apply: bool) -> (Outcome<Signed>, Vec<String>) {
        let mut lines = Vec::new();
        let req = ReleaseRequest { origin: &w.origin, version, sequence, key: &w.key, pins, apply };
        (release(&req, &mut |e: Event| lines.push(e.line)).unwrap(), lines)
    }

    fn pin(id: ComponentId, version: &str) -> Pin {
        Pin { id, version: version.to_string() }
    }

    #[test]
    fn a_release_signs_writes_and_reads_back() {
        let w = world();
        publish(&w, ComponentId::Runtime, "1.0.0", 1, &[("a.txt", b"alpha\n")]);
        let (outcome, lines) = sign(&w, "3.0.0", 1, &[pin(ComponentId::Runtime, "1.0.0")], true);
        let s = match outcome { Outcome::Done(s) => s, o => panic!("{o:?}") };
        assert_eq!(s.sequence, 1);
        assert!(s.release_path.is_file() && s.sig_path.is_file());
        assert!(lines.iter().any(|l| l.starts_with("Read back and verified")), "{lines:?}");
        // The file verifies with the public key alone, the way a launcher does.
        let verifier = Ed25519Verifier::with_keys(&[w.public]).unwrap();
        let sig = signing::decode_signature(&std::fs::read_to_string(&s.sig_path).unwrap()).unwrap();
        let trusted = TrustedRelease::open(&verifier, &std::fs::read(&s.release_path).unwrap(), &sig).unwrap();
        assert_eq!(trusted.release().sequence, 1);
    }

    #[test]
    fn a_dry_run_signs_nothing() {
        let w = world();
        publish(&w, ComponentId::Runtime, "1.0.0", 1, &[("a.txt", b"alpha\n")]);
        let (outcome, lines) = sign(&w, "3.0.0", 1, &[pin(ComponentId::Runtime, "1.0.0")], false);
        assert!(matches!(outcome, Outcome::Described));
        assert!(!w.origin.join(RELEASE_FILE).exists());
        assert!(lines.iter().any(|l| l.starts_with("Would write")));
    }

    #[test]
    fn a_replay_is_refused_with_the_words_the_scripts_look_for() {
        let w = world();
        publish(&w, ComponentId::Runtime, "1.0.0", 1, &[("a.txt", b"alpha\n")]);
        assert!(sign(&w, "3.0.0", 2, &[pin(ComponentId::Runtime, "1.0.0")], true).0.is_done());
        let (outcome, lines) = sign(&w, "3.0.0", 2, &[pin(ComponentId::Runtime, "1.0.0")], true);
        assert!(outcome.refused().is_some());
        assert!(lines.iter().any(|l| l.contains("strictly higher sequence")), "{lines:?}");
    }

    #[test]
    fn a_missing_manifest_and_a_missing_blob_are_refused() {
        let w = world();
        let (outcome, lines) = sign(&w, "3.0.0", 1, &[pin(ComponentId::Runtime, "9.9.9")], true);
        assert!(outcome.refused().is_some());
        assert!(lines.iter().any(|l| l.contains("Publish the component first")), "{lines:?}");
        publish(&w, ComponentId::Runtime, "1.0.0", 1, &[("a.txt", b"alpha\n")]);
        let sha = crate::library::hash_file(&w._dir.path().join("tree-runtime-1.0.0").join("a.txt")).unwrap();
        std::fs::remove_file(w.origin.join(blob_path(&sha))).unwrap();
        let (outcome, lines) = sign(&w, "3.0.0", 1, &[pin(ComponentId::Runtime, "1.0.0")], true);
        assert!(outcome.refused().is_some());
        assert!(lines.iter().any(|l| l.contains("not in the origin at their published")), "{lines:?}");
    }

    #[test]
    fn pinning_nothing_is_refused() {
        let w = world();
        let (outcome, _) = sign(&w, "3.0.0", 1, &[], true);
        assert!(outcome.refused().is_some());
    }

    #[test]
    fn carry_forward_replaces_one_pin_and_keeps_the_others_sorted() {
        let w = world();
        publish(&w, ComponentId::Runtime, "1.0.2", 1, &[("a.txt", b"alpha\n")]);
        publish(&w, ComponentId::Roster, "2026.09.06", 1, &[("roster.rebornroster", b"PK")]);
        assert!(sign(&w, "3.0.0", 3, &[pin(ComponentId::Runtime, "1.0.2")], true).0.is_done());
        let mut lines = Vec::new();
        let req = CarryForward {
            origin: &w.origin, id: ComponentId::Roster, version: "2026.09.06",
            key: Some(&w.key), release_version: None, release_sequence: None, apply: true,
        };
        let outcome = republish_pinning(&req, &mut |e: Event| lines.push(e.line)).unwrap();
        let s = match outcome { Outcome::Done(s) => s, o => panic!("{o:?}") };
        assert_eq!((s.version.as_str(), s.sequence), ("3.0.0", 4));
        assert!(lines.iter().any(|l| l.contains("pinning roster=2026.09.06, runtime=1.0.2")), "{lines:?}");
    }

    #[test]
    fn carry_forward_without_a_key_or_a_release_only_describes() {
        let w = world();
        publish(&w, ComponentId::Roster, "2026.09.06", 1, &[("roster.rebornroster", b"PK")]);
        let mut lines = Vec::new();
        let no_release = CarryForward {
            origin: &w.origin, id: ComponentId::Roster, version: "2026.09.06",
            key: Some(&w.key), release_version: None, release_sequence: None, apply: true,
        };
        assert!(matches!(republish_pinning(&no_release, &mut |e: Event| lines.push(e.line)).unwrap(), Outcome::Described));
        assert!(lines.iter().any(|l| l.contains("serves no release yet")), "{lines:?}");
        assert!(sign(&w, "3.0.0", 1, &[pin(ComponentId::Roster, "2026.09.06")], true).0.is_done());
        let no_key = CarryForward { key: None, ..no_release };
        lines.clear();
        assert!(matches!(republish_pinning(&no_key, &mut |e: Event| lines.push(e.line)).unwrap(), Outcome::Described));
        assert!(lines.iter().any(|l| l.contains("No --key")), "{lines:?}");
    }
}
