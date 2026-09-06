//! Where the game image lives: two layers.
//!
//! The **pristine** layer is `library/imported/<sha256>/<name>.iso`: the
//! image the import built from the player's own disc, or copied from an
//! existing Reborn install. Named by its own hash and never written to again.
//!
//! The **booted** layer is `library/offline/NSR.iso`: what Dolphin opens. It
//! starts as a hardlink of the pristine image and becomes a real file the
//! first time a roster is applied, because the Builder writes a new file and
//! that file is renamed over the link. The pristine bytes are untouched
//! either way.
//!
//! Two layers because a game update is a delta between two pristine builds
//! (see [`crate::game_patch`]), and a delta applies only to bytes that are
//! exactly what the publisher had. A roster applied in place makes every
//! player's image unique, and no delta can be published for that. Before
//! this, the roster was applied into the imported image itself, and a folder
//! named by a hash held bytes that no longer hashed to it.

use std::path::{Path, PathBuf};

use crate::library::RebuildState;
use crate::platform::{DataDir, Error, Platform, Result};

/// The name the booted image ships under.
pub const IMAGE_FILE: &str = "NSR.iso";

/// Where pristine images live, relative to the data root. One folder per
/// image, named by the image's SHA-256.
pub const IMPORTED_DIR: &str = "library/imported";

pub fn imported_root(data_root: &Path) -> PathBuf {
    data_root.join("library").join("imported")
}

/// The folder a pristine image with this hash lives in.
pub fn pristine_dir(data_root: &Path, sha256: &str) -> PathBuf {
    imported_root(data_root).join(sha256)
}

/// The image Dolphin boots.
pub fn booted_path(data_root: &Path) -> PathBuf {
    let mut p = data_root.to_path_buf();
    p.extend(DataDir::LibraryOffline.relative().split('/'));
    p.join(IMAGE_FILE)
}

/// A pristine image on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pristine {
    pub path: PathBuf,
    /// The folder name, which is the hash the importer verified. Fast to
    /// read; anything that patches the image hashes the bytes again first.
    pub folder_sha256: String,
}

/// Every pristine image, newest first.
///
/// Any `.iso` directly inside a folder under `library/imported`: the patch
/// import names its output `NSR.iso`, the existing-image import keeps the
/// player's file name. Before this, only `NSR.iso` was looked for, and a
/// migrated install had a game the roster could not find.
pub fn list_pristine(data_root: &Path) -> Vec<Pristine> {
    let mut found: Vec<(std::time::SystemTime, Pristine)> = Vec::new();
    let Ok(folders) = std::fs::read_dir(imported_root(data_root)) else {
        return Vec::new();
    };
    for folder in folders.flatten() {
        let dir = folder.path();
        if !dir.is_dir() {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        let mut images: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.is_file()
                    && p
                        .extension()
                        .and_then(|x| x.to_str())
                        .map(|x| x.eq_ignore_ascii_case("iso"))
                        .unwrap_or(false)
            })
            .collect();
        images.sort();
        // `NSR.iso` first when a folder somehow holds several.
        images.sort_by_key(|p| {
            !p.file_name()
                .map(|n| n.to_string_lossy().eq_ignore_ascii_case(IMAGE_FILE))
                .unwrap_or(false)
        });
        let Some(path) = images.into_iter().next() else { continue };
        let Ok(modified) = std::fs::metadata(&path).and_then(|m| m.modified()) else { continue };
        found.push((
            modified,
            Pristine {
                path,
                folder_sha256: folder.file_name().to_string_lossy().into_owned(),
            },
        ));
    }
    found.sort_by(|a, b| b.0.cmp(&a.0));
    found.into_iter().map(|(_, p)| p).collect()
}

/// The pristine image the launcher uses: the newest, which after a game
/// update is the one the update built.
pub fn find_pristine(data_root: &Path) -> Option<Pristine> {
    list_pristine(data_root).into_iter().next()
}

/// How the booted image was made.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Establish {
    /// A hardlink: no bytes copied, no extra disk. The common case.
    Linked,
    /// A copy, on a filesystem without hardlinks (exFAT, FAT32).
    Copied,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Established {
    pub path: PathBuf,
    pub how: Establish,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EstablishRefusal {
    NoPristine { path: PathBuf },
    /// Dolphin has it open. Nothing was changed; try again with the game
    /// closed.
    BootedInUse { path: PathBuf },
}

impl EstablishRefusal {
    pub fn message(&self) -> String {
        match self {
            EstablishRefusal::NoPristine { path } => format!(
                "There is no game image at {}, so nothing could be set up to play. Nothing was changed.",
                path.display()
            ),
            EstablishRefusal::BootedInUse { path } => format!(
                "{} is open in another program, so it was not replaced. Close the game and try again. \
                 Nothing was changed.",
                path.display()
            ),
        }
    }
}

#[derive(Debug)]
pub enum EstablishOutcome {
    Done(Established),
    Refused(EstablishRefusal),
}

/// Make `library/offline/NSR.iso` the given pristine image, as a hardlink
/// when the filesystem allows and a copy when it does not.
///
/// Called after an import, and after a game update builds a new pristine
/// image. Whatever was booted before is replaced in one rename. The rebuild
/// record for the booted image is cleared, because it is pristine again and
/// a record saying otherwise would make verification keep a stale file.
pub fn establish(platform: &Platform, data_root: &Path, pristine: &Path) -> Result<EstablishOutcome> {
    if !pristine.is_file() {
        return Ok(EstablishOutcome::Refused(EstablishRefusal::NoPristine {
            path: pristine.to_path_buf(),
        }));
    }
    let booted = booted_path(data_root);
    if booted.exists() && platform.files.is_locked(&booted) {
        return Ok(EstablishOutcome::Refused(EstablishRefusal::BootedInUse { path: booted }));
    }
    let parent = booted.parent().expect("the booted path has a parent");
    std::fs::create_dir_all(parent).map_err(|source| Error::Io {
        path: parent.to_path_buf(),
        source,
    })?;
    let temp = parent.join(format!("{IMAGE_FILE}.new"));
    if temp.exists() {
        std::fs::remove_file(&temp).map_err(|source| Error::Io {
            path: temp.clone(),
            source,
        })?;
    }
    let how = match platform.files.hardlink(pristine, &temp) {
        Ok(()) => Establish::Linked,
        Err(e) => {
            tracing::info!("hardlink refused ({e}); copying the image instead");
            let _ = std::fs::remove_file(&temp);
            std::fs::copy(pristine, &temp).map_err(|source| Error::Io {
                path: temp.clone(),
                source,
            })?;
            Establish::Copied
        }
    };
    if let Err(e) = platform.files.atomic_replace(&temp, &booted) {
        let _ = std::fs::remove_file(&temp);
        return Err(e);
    }

    let state_path = crate::music::music_root(data_root).join("state.json");
    let mut state = RebuildState::load(&state_path)?;
    if state.rebuilt.remove(IMAGE_FILE).is_some() {
        state.save(&state_path)?;
    }
    Ok(EstablishOutcome::Done(Established { path: booted, how }))
}

/// Remove every pristine folder except the one holding `keep`.
///
/// Best effort, after a game update has put the new image in place and the
/// booted copy has been re-established from it. Returns what was removed.
pub fn retire_other_pristine(data_root: &Path, keep: &Path) -> Vec<PathBuf> {
    let mut removed = Vec::new();
    for p in list_pristine(data_root) {
        if p.path == keep {
            continue;
        }
        let Some(folder) = p.path.parent() else { continue };
        match std::fs::remove_dir_all(folder) {
            Ok(()) => removed.push(folder.to_path_buf()),
            Err(e) => tracing::warn!("could not remove the old image at {}: {e}", folder.display()),
        }
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put(data_root: &Path, sha: &str, name: &str, bytes: &[u8]) -> PathBuf {
        let dir = pristine_dir(data_root, sha);
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join(name);
        std::fs::write(&p, bytes).unwrap();
        p
    }

    #[test]
    fn the_newest_pristine_wins_and_any_iso_name_counts() {
        let dir = tempfile::tempdir().unwrap();
        assert!(find_pristine(dir.path()).is_none());

        let older = put(dir.path(), "aaaa", "NFL Street 2 FIELDS 12-19.iso", b"old");
        let newer = put(dir.path(), "bbbb", IMAGE_FILE, b"new");
        let then = std::time::SystemTime::now() - std::time::Duration::from_secs(600);
        std::fs::File::options().write(true).open(&older).unwrap().set_modified(then).unwrap();

        let found = find_pristine(dir.path()).unwrap();
        assert_eq!(found.path, newer);
        assert_eq!(found.folder_sha256, "bbbb");
        let all = list_pristine(dir.path());
        assert_eq!(all.len(), 2);
        assert_eq!(all[1].path, older, "the migrated file name is found too");

        std::fs::create_dir_all(pristine_dir(dir.path(), "cccc")).unwrap();
        assert_eq!(list_pristine(dir.path()).len(), 2, "an empty folder is not an image");
    }

    #[test]
    fn establishing_links_the_pristine_into_place_and_clears_the_rebuild_record() {
        let dir = tempfile::tempdir().unwrap();
        let platform = Platform::detect();
        let pristine = put(dir.path(), "aaaa", IMAGE_FILE, b"pristine bytes");

        let state_path = crate::music::music_root(dir.path()).join("state.json");
        let mut state = RebuildState::default();
        state
            .rebuilt
            .insert(IMAGE_FILE.to_string(), crate::manifest::Sha256Hex::from_bytes(b"stale"));
        state.save(&state_path).unwrap();

        let done = match establish(&platform, dir.path(), &pristine).unwrap() {
            EstablishOutcome::Done(d) => d,
            other => panic!("{other:?}"),
        };
        assert_eq!(done.path, booted_path(dir.path()));
        assert_eq!(std::fs::read(&done.path).unwrap(), b"pristine bytes");
        assert!(
            !RebuildState::load(&state_path).unwrap().rebuilt.contains_key(IMAGE_FILE),
            "the booted image is pristine again"
        );

        // Replacing the booted file must not touch the pristine one. This is
        // the property the whole layering rests on: a rename over a hardlink
        // unlinks that name only.
        let replacement = dir.path().join("with-roster.iso");
        std::fs::write(&replacement, b"pristine bytes plus roster").unwrap();
        platform.files.atomic_replace(&replacement, &done.path).unwrap();
        assert_eq!(std::fs::read(&pristine).unwrap(), b"pristine bytes");
        assert_eq!(std::fs::read(&done.path).unwrap(), b"pristine bytes plus roster");

        // Establishing again puts the pristine back.
        match establish(&platform, dir.path(), &pristine).unwrap() {
            EstablishOutcome::Done(d) => assert_eq!(std::fs::read(&d.path).unwrap(), b"pristine bytes"),
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            establish(&platform, dir.path(), &dir.path().join("missing.iso")).unwrap(),
            EstablishOutcome::Refused(EstablishRefusal::NoPristine { .. })
        ));
    }

    #[test]
    fn retiring_keeps_only_the_named_image() {
        let dir = tempfile::tempdir().unwrap();
        let old = put(dir.path(), "aaaa", IMAGE_FILE, b"old");
        let keep = put(dir.path(), "bbbb", IMAGE_FILE, b"new");
        let removed = retire_other_pristine(dir.path(), &keep);
        assert_eq!(removed, vec![old.parent().unwrap().to_path_buf()]);
        assert!(!old.exists());
        assert!(keep.exists());
        assert_eq!(list_pristine(dir.path()).len(), 1);
    }
}
