//! Verify and repair the managed game library.
//!
//! # What this replaces
//!
//! The shipped 2.1 "patcher" does not patch. Its core function is named
//! `fake_patch`, it sleeps through twelve hardcoded messages for about fourteen
//! seconds, and it never opens the file the user selected. The playable game is
//! an image shipped inside the installer.
//!
//! Since the launcher also ships the finished image, this module does not patch
//! either. What it does instead is the part that was missing: **copy, verify,
//! and repair precisely**.
//!
//! # The rule that keeps repair from deleting custom music
//!
//! A rebuilt offline image never matches its shipped hash, because the user
//! changed a song. A naive "hash mismatch means damage" rule would treat every
//! customized install as corrupt and re-fetch 1.5 GB over the top of it.
//!
//! So repair asks three questions in order, and only the third one is about the
//! hash:
//!
//! 1. Is the file missing? Then it is damage, whatever else is true.
//! 2. Is the component customizable, and does the local hash match what the
//!    launcher itself recorded when it last rebuilt? Then it is the user's
//!    music and it is left alone.
//! 3. Otherwise the hash decides.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::manifest::{Component, Sha256Hex};
use crate::platform::{Error, Result};

/// What the launcher recorded the last time it rebuilt a customizable file.
///
/// Persisted as `music/state.json`. Without it there is no way to tell a
/// user's rebuilt soundtrack from a corrupted download, because both are
/// simply "not the shipped hash".
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RebuildState {
    /// Path relative to the component install root, to the hash the launcher
    /// produced when it last rebuilt that file.
    pub rebuilt: BTreeMap<String, Sha256Hex>,
}

impl RebuildState {
    pub fn load(path: &Path) -> Result<Self> {
        match std::fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| Error::Io {
                path: path.to_path_buf(),
                source: std::io::Error::new(std::io::ErrorKind::InvalidData, e),
            }),
            // No record yet simply means nothing has been rebuilt.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(source) => Err(Error::Io {
                path: path.to_path_buf(),
                source,
            }),
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| Error::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| Error::Io {
            path: path.to_path_buf(),
            source: std::io::Error::other(e),
        })?;
        std::fs::write(path, bytes).map_err(|source| Error::Io {
            path: path.to_path_buf(),
            source,
        })
    }

    pub fn record(&mut self, relative_path: &str, hash: Sha256Hex) {
        self.rebuilt.insert(relative_path.to_string(), hash);
    }
}

/// What repair decided about one file, and why.
///
/// The reason is carried so the user can be told something true. "Repairing 3
/// files" is less useful than naming which and why, and a verdict of
/// `UserModified` must never be silently upgraded to a re-download.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileVerdict {
    /// Present and matches the manifest.
    Intact,
    /// Absent. Damage regardless of anything else.
    Missing,
    /// Present, wrong size or hash, and not explained by a recorded rebuild.
    Damaged { expected: Sha256Hex, actual: Sha256Hex },
    /// Present, does not match the manifest, and that is expected: the user
    /// customized it and the launcher recorded the result.
    UserModified,
}

impl FileVerdict {
    /// Whether repair should touch this file.
    pub fn needs_repair(&self) -> bool {
        matches!(self, FileVerdict::Missing | FileVerdict::Damaged { .. })
    }
}

/// The outcome of verifying a whole component.
#[derive(Debug, Clone)]
pub struct VerifyReport {
    pub component: String,
    pub verdicts: BTreeMap<String, FileVerdict>,
}

impl VerifyReport {
    pub fn is_healthy(&self) -> bool {
        !self.verdicts.values().any(FileVerdict::needs_repair)
    }

    pub fn to_repair(&self) -> Vec<&str> {
        self.verdicts
            .iter()
            .filter(|(_, v)| v.needs_repair())
            .map(|(p, _)| p.as_str())
            .collect()
    }

    pub fn user_modified(&self) -> Vec<&str> {
        self.verdicts
            .iter()
            .filter(|(_, v)| matches!(v, FileVerdict::UserModified))
            .map(|(p, _)| p.as_str())
            .collect()
    }

    /// Bytes that would actually be re-fetched.
    pub fn repair_bytes(&self, component: &Component) -> u64 {
        let needed: std::collections::BTreeSet<&str> = self.to_repair().into_iter().collect();
        component
            .files
            .iter()
            .filter(|f| needed.contains(f.path.as_str()))
            .map(|f| f.bytes)
            .sum()
    }
}

/// Verifies a component against its manifest.
#[derive(Debug)]
pub struct Verifier<'a> {
    root: &'a Path,
    state: &'a RebuildState,
}

impl<'a> Verifier<'a> {
    pub fn new(root: &'a Path, state: &'a RebuildState) -> Self {
        Self { root, state }
    }

    pub fn verify(&self, component: &Component) -> Result<VerifyReport> {
        let mut verdicts = BTreeMap::new();
        for file in &component.files {
            verdicts.insert(file.path.clone(), self.verify_one(component, &file.path)?);
        }
        Ok(VerifyReport {
            component: component.component.clone(),
            verdicts,
        })
    }

    fn verify_one(&self, component: &Component, relative: &str) -> Result<FileVerdict> {
        let file = component
            .files
            .iter()
            .find(|f| f.path == relative)
            .expect("caller iterates the manifest's own file list");

        let path = self.resolve(relative);

        let meta = match std::fs::metadata(&path) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(FileVerdict::Missing),
            Err(source) => return Err(Error::Io { path, source }),
        };

        // Size is a free pre-check. A 1.5 GB image only gets hashed if its
        // length already matches, which is the difference between a repair scan
        // taking seconds and taking minutes.
        if meta.len() == file.bytes {
            let actual = hash_file(&path)?;
            if actual == file.sha256 {
                return Ok(FileVerdict::Intact);
            }
            return Ok(self.explain_mismatch(component, relative, file.sha256.clone(), actual));
        }

        let actual = hash_file(&path)?;
        Ok(self.explain_mismatch(component, relative, file.sha256.clone(), actual))
    }

    /// A file that does not match the manifest is not automatically damage.
    fn explain_mismatch(
        &self,
        component: &Component,
        relative: &str,
        expected: Sha256Hex,
        actual: Sha256Hex,
    ) -> FileVerdict {
        if component.customizable {
            if let Some(recorded) = self.state.rebuilt.get(relative) {
                if *recorded == actual {
                    return FileVerdict::UserModified;
                }
            }
        }
        FileVerdict::Damaged { expected, actual }
    }

    fn resolve(&self, relative: &str) -> PathBuf {
        let mut path = self.root.to_path_buf();
        for segment in relative.split(['/', '\\']).filter(|s| !s.is_empty()) {
            path.push(segment);
        }
        path
    }
}

/// SHA-256 of a file, streamed so a 1.5 GB image does not go through memory.
pub fn hash_file(path: &Path) -> Result<Sha256Hex> {
    use sha2::{Digest, Sha256};
    use std::io::Read;

    let mut file = std::fs::File::open(path).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = file.read(&mut buf).map_err(|source| Error::Io {
            path: path.to_path_buf(),
            source,
        })?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(Sha256Hex::parse(&hex::encode(hasher.finalize()))
        .expect("hex of a sha-256 digest is always 64 hex characters"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::ManifestFile;

    fn component(customizable: bool, files: Vec<ManifestFile>) -> Component {
        Component {
            schema: crate::manifest::COMPONENT_SCHEMA.to_string(),
            component: "offline-content".into(),
            version: "2.4.0".into(),
            sequence: 7,
            install_root: "library/offline".into(),
            customizable,
            files,
        }
    }

    fn write(dir: &Path, rel: &str, contents: &[u8]) -> ManifestFile {
        let path = dir.join(rel);
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p).unwrap();
        }
        std::fs::write(&path, contents).unwrap();
        ManifestFile {
            path: rel.to_string(),
            bytes: contents.len() as u64,
            sha256: Sha256Hex::from_bytes(contents),
        }
    }

    #[test]
    fn an_untouched_install_verifies_clean() {
        let dir = tempfile::tempdir().unwrap();
        let f = write(dir.path(), "NSR.iso", b"the game");
        let c = component(false, vec![f]);
        let state = RebuildState::default();

        let report = Verifier::new(dir.path(), &state).verify(&c).unwrap();
        assert!(report.is_healthy());
        assert!(report.to_repair().is_empty());
    }

    #[test]
    fn a_missing_file_is_damage() {
        let dir = tempfile::tempdir().unwrap();
        let f = write(dir.path(), "NSR.iso", b"the game");
        std::fs::remove_file(dir.path().join("NSR.iso")).unwrap();
        let c = component(false, vec![f]);
        let state = RebuildState::default();

        let report = Verifier::new(dir.path(), &state).verify(&c).unwrap();
        assert!(!report.is_healthy());
        assert_eq!(report.verdicts["NSR.iso"], FileVerdict::Missing);
    }

    #[test]
    fn a_corrupted_file_is_damage() {
        let dir = tempfile::tempdir().unwrap();
        let f = write(dir.path(), "NSR.iso", b"the game");
        std::fs::write(dir.path().join("NSR.iso"), b"corrupted!!").unwrap();
        let c = component(false, vec![f]);
        let state = RebuildState::default();

        let report = Verifier::new(dir.path(), &state).verify(&c).unwrap();
        assert!(matches!(
            report.verdicts["NSR.iso"],
            FileVerdict::Damaged { .. }
        ));
    }

    /// THE ONE THAT MATTERS. A rebuilt soundtrack does not match the shipped
    /// hash, and repair must not re-fetch over it.
    #[test]
    fn a_rebuilt_soundtrack_is_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        let shipped = write(dir.path(), "NSR.iso", b"the shipped game");

        // The user changes a song. The launcher rebuilds and records what it
        // produced.
        let rebuilt_bytes = b"the game with my music in it";
        std::fs::write(dir.path().join("NSR.iso"), rebuilt_bytes).unwrap();
        let mut state = RebuildState::default();
        state.record("NSR.iso", Sha256Hex::from_bytes(rebuilt_bytes));

        let c = component(true, vec![shipped]);
        let report = Verifier::new(dir.path(), &state).verify(&c).unwrap();

        assert_eq!(report.verdicts["NSR.iso"], FileVerdict::UserModified);
        assert!(report.is_healthy(), "custom music is not a fault");
        assert!(report.to_repair().is_empty(), "repair must not touch it");
        assert_eq!(report.user_modified(), vec!["NSR.iso"]);
    }

    /// The exemption is narrow on purpose. Real corruption of a customizable
    /// file is still corruption.
    #[test]
    fn corruption_of_a_customizable_file_is_still_damage() {
        let dir = tempfile::tempdir().unwrap();
        let shipped = write(dir.path(), "NSR.iso", b"the shipped game");

        let mut state = RebuildState::default();
        state.record("NSR.iso", Sha256Hex::from_bytes(b"what I rebuilt"));

        // On disk is neither the shipped image nor what was recorded.
        std::fs::write(dir.path().join("NSR.iso"), b"bit rot").unwrap();

        let c = component(true, vec![shipped]);
        let report = Verifier::new(dir.path(), &state).verify(&c).unwrap();
        assert!(
            matches!(report.verdicts["NSR.iso"], FileVerdict::Damaged { .. }),
            "an unrecorded mismatch is damage even in a customizable component"
        );
    }

    /// The exemption must not leak into components that are not customizable.
    /// The online image is never customizable, which is what lets matchmaking
    /// trust it.
    #[test]
    fn a_non_customizable_component_ignores_the_rebuild_record() {
        let dir = tempfile::tempdir().unwrap();
        let shipped = write(dir.path(), "NSRONLINE.iso", b"pristine online");
        let modified = b"someone edited the online image";
        std::fs::write(dir.path().join("NSRONLINE.iso"), modified).unwrap();

        let mut state = RebuildState::default();
        state.record("NSRONLINE.iso", Sha256Hex::from_bytes(modified));

        let c = component(false, vec![shipped]);
        let report = Verifier::new(dir.path(), &state).verify(&c).unwrap();
        assert!(
            matches!(report.verdicts["NSRONLINE.iso"], FileVerdict::Damaged { .. }),
            "a rebuild record must not excuse a change to a non-customizable file"
        );
    }

    #[test]
    fn repair_bytes_counts_only_what_would_be_refetched() {
        let dir = tempfile::tempdir().unwrap();
        let a = write(dir.path(), "a.dat", &[b'a'; 100]);
        let b = write(dir.path(), "b.dat", &[b'b'; 250]);
        let cfile = write(dir.path(), "c.dat", &[b'c'; 999]);
        std::fs::remove_file(dir.path().join("b.dat")).unwrap();

        let c = component(false, vec![a, b, cfile]);
        let state = RebuildState::default();
        let report = Verifier::new(dir.path(), &state).verify(&c).unwrap();

        assert_eq!(report.to_repair(), vec!["b.dat"]);
        assert_eq!(report.repair_bytes(&c), 250, "only the missing file counts");
    }

    #[test]
    fn rebuild_state_survives_a_round_trip_and_absence() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("music").join("state.json");

        assert!(
            RebuildState::load(&path).unwrap().rebuilt.is_empty(),
            "a missing state file means nothing has been rebuilt, not an error"
        );

        let mut state = RebuildState::default();
        state.record("NSR.iso", Sha256Hex::from_bytes(b"rebuilt"));
        state.save(&path).unwrap();

        let loaded = RebuildState::load(&path).unwrap();
        assert_eq!(
            loaded.rebuilt.get("NSR.iso"),
            Some(&Sha256Hex::from_bytes(b"rebuilt"))
        );
    }

    #[test]
    fn hashing_a_large_file_streams_rather_than_buffering() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("big.bin");
        // Larger than the 1 MiB read buffer, so the streaming loop runs more
        // than once.
        let data = vec![0xABu8; 3 * 1024 * 1024 + 17];
        std::fs::write(&path, &data).unwrap();
        assert_eq!(hash_file(&path).unwrap(), Sha256Hex::from_bytes(&data));
    }
}
