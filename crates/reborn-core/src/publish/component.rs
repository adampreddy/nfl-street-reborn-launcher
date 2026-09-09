//! One component tree into the origin: its manifest and its blobs.

use std::path::{Path, PathBuf};

use crate::inventory::{scan_component, ComponentSpec, ScanObserver, ScanOutcome, SerializedComponent};
use crate::library::hash_file;
use crate::manifest::{ManifestFile, Sha256Hex};
use crate::origin::blob_path;
use crate::publish::{err, io_err, manifest_path, out, refuse, thousands, Outcome, Sink};
use crate::{Error, Result};

/// What to publish, from where, into which origin.
#[derive(Debug)]
pub struct ComponentRequest<'a> {
    pub root: &'a Path,
    pub spec: ComponentSpec,
    pub origin: &'a Path,
    /// Write. Without it the tree is hashed and the work is described.
    pub apply: bool,
}

/// What a publish wrote, or would write.
#[derive(Debug, Clone)]
pub struct Published {
    pub manifest_path: PathBuf,
    pub sha256: Sha256Hex,
    pub files: u64,
    pub bytes: u64,
    pub new_blobs: u64,
    pub new_bytes: u64,
    pub shared_blobs: u64,
    pub shared_bytes: u64,
}

/// Hash `req.root`, then write its manifest and any new blobs into
/// `req.origin`. Without `req.apply` the tree is hashed and the plan is
/// described, and nothing is written.
pub fn publish_component(req: &ComponentRequest<'_>, observer: &dyn ScanObserver, sink: Sink<'_>) -> Result<Outcome<Published>> {
    let (root, origin, apply, spec) = (req.root, req.origin, req.apply, &req.spec);

    let manifest_path = manifest_path(origin, spec.id(), spec.version());
    out!(sink, "Tree          {}", root.display());
    out!(sink, "Component     {} version {}, sequence {}", spec.id().as_str(), spec.version(), spec.sequence());
    out!(sink, "Install root  {}", spec.install_root());
    out!(sink, "Origin        {}", origin.display());
    out!(sink, "Manifest      {}", manifest_path.display());
    out!(sink);
    out!(sink, "Hashing. Progress goes to standard error. The tree must be still.");

    let started = std::time::Instant::now();
    let scan = match scan_component(root, spec, observer)? {
        ScanOutcome::Cancelled { .. } => {
            return Ok(refuse(sink, &["Cancelled. Nothing was written.".to_string()]));
        }
        ScanOutcome::Complete(scan) => scan,
    };
    out!(
        sink,
        "Hashed {} file(s), {} bytes, in {:.1} s.",
        thousands(scan.progress.files_hashed),
        thousands(scan.progress.bytes_hashed),
        started.elapsed().as_secs_f64()
    );
    let serialized = SerializedComponent::new(scan.component)?;

    // A manifest already at this name must be THIS manifest. Replacing it
    // would leave every client holding the old release pinning a hash no
    // served file has.
    if manifest_path.is_file() {
        let existing = std::fs::read(&manifest_path).map_err(|e| io_err(&manifest_path, e))?;
        if existing != serialized.bytes() {
            return Ok(refuse(sink, &[
                String::new(),
                format!("{} already exists with DIFFERENT contents.", manifest_path.display()),
                "A published manifest is never rewritten: a client that has the release".to_string(),
                "pinning it would report the new file as tampering. Publish a new version".to_string(),
                "with a higher sequence instead. Nothing was written.".to_string(),
            ]));
        }
        out!(sink, "Manifest already published with identical bytes; it will not be rewritten.");
    }

    // The blob plan: which of these files the origin already holds.
    let mut new_files = 0u64;
    let mut new_bytes = 0u64;
    let mut shared = 0u64;
    let mut shared_bytes = 0u64;
    let mut to_copy: Vec<(&ManifestFile, PathBuf)> = Vec::new();
    for file in &serialized.component().files {
        let dst = origin.join(blob_path(&file.sha256));
        match std::fs::metadata(&dst) {
            Ok(m) if m.len() == file.bytes => {
                shared += 1;
                shared_bytes += file.bytes;
            }
            Ok(m) => {
                return Ok(refuse(sink, &[
                    String::new(),
                    format!("{} exists with {} bytes; the manifest says {}.", dst.display(), m.len(), file.bytes),
                    "A content-addressed blob cannot have two lengths. Something wrote a wrong".to_string(),
                    "or truncated file at that name. Delete it and run again. Nothing was written.".to_string(),
                ]));
            }
            Err(_) => {
                new_files += 1;
                new_bytes += file.bytes;
                to_copy.push((file, dst));
            }
        }
    }
    out!(sink);
    out!(
        sink,
        "Blobs         {} new ({} bytes), {} already in the origin ({} bytes)",
        thousands(new_files),
        thousands(new_bytes),
        thousands(shared),
        thousands(shared_bytes)
    );
    if shared > 0 {
        out!(sink, "              Shared blobs are uploaded once and fetched once, whichever");
        out!(sink, "              component or version references them.");
    }

    if !apply {
        out!(sink);
        out!(sink, "Nothing was written. Re-run with --apply to write the manifest and the");
        out!(sink, "{} new blob(s).", thousands(new_files));
        return Ok(Outcome::Described);
    }

    // Blobs first, manifest last. A manifest naming blobs that are not there
    // yet is a release that fails on every client; blobs with no manifest are
    // just bytes nobody asks for.
    let started = std::time::Instant::now();
    let mut copied_bytes = 0u64;
    for (i, (file, dst)) in to_copy.iter().enumerate() {
        let src = root.join(file.path.replace('/', std::path::MAIN_SEPARATOR_STR));
        copy_blob(&src, dst, &file.sha256, file.bytes)?;
        copied_bytes += file.bytes;
        if (i + 1) % 500 == 0 || i + 1 == to_copy.len() {
            err!(
                sink,
                "  copied {} of {} blob(s), {} bytes, {:.0} s",
                thousands((i + 1) as u64),
                thousands(to_copy.len() as u64),
                thousands(copied_bytes),
                started.elapsed().as_secs_f64()
            );
        }
    }
    if !manifest_path.is_file() {
        serialized.write_to(&manifest_path)?;
    }

    out!(sink);
    out!(sink, "Wrote {} blob(s) and {}", thousands(new_files), manifest_path.display());
    out!(sink, "Each new blob was hashed again after the copy; the copy is what is served.");
    out!(sink, "Pin it in a release with:");
    out!(sink, "  rebornctl publish release --origin {} --component {}={} ...", origin.display(), spec.id().as_str(), spec.version());

    Ok(Outcome::Done(Published {
        files: serialized.component().files.len() as u64,
        bytes: serialized.component().total_bytes(),
        sha256: serialized.sha256().clone(),
        manifest_path,
        new_blobs: new_files,
        new_bytes,
        shared_blobs: shared,
        shared_bytes,
    }))
}

/// Copy one file into the blob store and prove the copy is the file.
///
/// Written to a temporary name and renamed, so a file at the final name is
/// always a verified one. Hashed after the copy because the copy is what is
/// served, and the scan's hash only proves what the source was.
pub(crate) fn copy_blob(src: &Path, dst: &Path, want: &Sha256Hex, want_bytes: u64) -> Result<()> {
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent).map_err(|e| io_err(parent, e))?;
    }
    let tmp = dst.with_extension("part");
    let copied = std::fs::copy(src, &tmp).map_err(|e| io_err(src, e))?;
    let got = hash_file(&tmp)?;
    if copied != want_bytes || &got != want {
        let _ = std::fs::remove_file(&tmp);
        return Err(Error::PlainIo(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!(
                "{} changed between the scan and the copy: scanned {want} at {want_bytes} bytes, \
                 copied {got} at {copied} bytes. The tree is not still. Nothing was published \
                 for it; run again on a quiet tree",
                src.display()
            ),
        )));
    }
    std::fs::rename(&tmp, dst).map_err(|e| io_err(dst, e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::{ComponentId, ComponentSpec, RunToCompletion};
    use crate::publish::{Event, Outcome};

    fn tree(dir: &Path, files: &[(&str, &[u8])]) {
        for (name, bytes) in files {
            let p = dir.join(name);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, bytes).unwrap();
        }
    }

    fn run(root: &Path, origin: &Path, version: &str, sequence: u64, apply: bool) -> (Outcome<Published>, Vec<String>) {
        let spec = ComponentSpec::new(ComponentId::Runtime, version, sequence).unwrap();
        let req = ComponentRequest { root, spec, origin, apply };
        let mut lines: Vec<String> = Vec::new();
        let outcome = publish_component(&req, &RunToCompletion, &mut |e: Event| lines.push(e.line)).unwrap();
        (outcome, lines)
    }

    #[test]
    fn a_dry_run_describes_and_writes_nothing() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("tree");
        tree(&root, &[("a.txt", b"alpha\n"), ("sub/b.txt", b"beta\n")]);
        let origin = d.path().join("origin");
        let (outcome, lines) = run(&root, &origin, "1.0.0", 1, false);
        assert!(matches!(outcome, Outcome::Described), "{outcome:?}");
        assert!(!origin.exists() || std::fs::read_dir(&origin).unwrap().next().is_none());
        assert!(lines.iter().any(|l| l.contains("2 new")), "{lines:?}");
        assert!(lines.iter().any(|l| l.starts_with("Nothing was written.")));
    }

    #[test]
    fn a_first_publish_writes_every_blob_and_the_manifest() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("tree");
        tree(&root, &[("a.txt", b"alpha\n"), ("sub/b.txt", b"beta\n"), ("c.bin", &[0u8; 1500])]);
        let origin = d.path().join("origin");
        let (outcome, lines) = run(&root, &origin, "1.0.0", 1, true);
        let p = match outcome { Outcome::Done(p) => p, other => panic!("{other:?}") };
        assert_eq!((p.files, p.new_blobs, p.shared_blobs), (3, 3, 0));
        assert!(p.manifest_path.is_file());
        assert_eq!(p.manifest_path, origin.join("components").join("runtime-1.0.0.json"));
        assert!(lines.iter().any(|l| l.contains("3 new")), "{lines:?}");
        assert!(lines.iter().any(|l| l.starts_with("Wrote 3 blob(s) and ")));
        // Every blob is at its content address.
        let manifest = crate::manifest::Component::parse(&std::fs::read(&p.manifest_path).unwrap()).unwrap();
        for f in &manifest.files {
            let blob = origin.join(crate::origin::blob_path(&f.sha256));
            assert_eq!(std::fs::metadata(&blob).unwrap().len(), f.bytes, "{}", f.path);
        }
    }

    #[test]
    fn a_second_version_shares_unchanged_blobs() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("tree");
        tree(&root, &[("a.txt", b"alpha\n"), ("b.txt", b"beta\n")]);
        let origin = d.path().join("origin");
        assert!(run(&root, &origin, "1.0.0", 1, true).0.is_done());
        std::fs::write(root.join("b.txt"), b"beta 2\n").unwrap();
        std::fs::write(root.join("new.txt"), b"new\n").unwrap();
        let (outcome, lines) = run(&root, &origin, "1.0.1", 2, true);
        let p = match outcome { Outcome::Done(p) => p, other => panic!("{other:?}") };
        assert_eq!((p.new_blobs, p.shared_blobs), (2, 1));
        assert!(lines.iter().any(|l| l.contains("2 new")), "{lines:?}");
    }

    #[test]
    fn the_same_bytes_twice_is_not_rewritten_and_different_bytes_are_refused() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("tree");
        tree(&root, &[("a.txt", b"alpha\n")]);
        let origin = d.path().join("origin");
        assert!(run(&root, &origin, "1.0.0", 1, true).0.is_done());
        let (again, lines) = run(&root, &origin, "1.0.0", 1, true);
        assert!(again.is_done());
        assert!(lines.iter().any(|l| l.starts_with("Manifest already published with identical bytes")));
        // Same name, different content: refused, manifest untouched.
        let before = std::fs::read(origin.join("components").join("runtime-1.0.0.json")).unwrap();
        std::fs::write(root.join("a.txt"), b"changed\n").unwrap();
        let (refused, lines) = run(&root, &origin, "1.0.0", 1, true);
        assert!(refused.refused().is_some(), "{refused:?}");
        assert!(lines.iter().any(|l| l.contains("already exists with DIFFERENT contents")), "{lines:?}");
        assert_eq!(std::fs::read(origin.join("components").join("runtime-1.0.0.json")).unwrap(), before);
    }

    #[test]
    fn a_blob_at_the_wrong_length_is_refused() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("tree");
        tree(&root, &[("a.txt", b"alpha\n")]);
        let origin = d.path().join("origin");
        let sha = crate::library::hash_file(&root.join("a.txt")).unwrap();
        let blob = origin.join(crate::origin::blob_path(&sha));
        std::fs::create_dir_all(blob.parent().unwrap()).unwrap();
        std::fs::write(&blob, b"truncated").unwrap();
        let (outcome, lines) = run(&root, &origin, "1.0.0", 1, true);
        assert!(outcome.refused().is_some());
        assert!(lines.iter().any(|l| l.contains("cannot have two lengths")), "{lines:?}");
        assert!(!origin.join("components").join("runtime-1.0.0.json").exists());
    }

    #[test]
    fn copy_blob_proves_the_copy_and_leaves_no_part_file_on_mismatch() {
        let d = tempfile::tempdir().unwrap();
        let src = d.path().join("src.bin");
        std::fs::write(&src, b"hello").unwrap();
        let want = crate::library::hash_file(&src).unwrap();
        let dst = d.path().join("blobs").join("x");
        copy_blob(&src, &dst, &want, 5).unwrap();
        assert_eq!(std::fs::read(&dst).unwrap(), b"hello");
        let wrong = d.path().join("blobs").join("y");
        let err = copy_blob(&src, &wrong, &want, 4).unwrap_err();
        assert!(err.to_string().contains("The tree is not still"), "{err}");
        assert!(!wrong.exists() && !wrong.with_extension("part").exists());
    }
}
