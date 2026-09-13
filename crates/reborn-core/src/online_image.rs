//! The image netplay boots: the pristine game plus the roster, never music.
//!
//! # Why a second image exists
//!
//! Netplay is lockstep: both sides run the same game from the same bytes. The
//! offline image is deliberately not those bytes. A custom soundtrack rewrites
//! `pathfind.dat` and `template.dat` inside it (see [`crate::soundtrack`]), so
//! two players who each added their own songs hold two different discs, and
//! the disc is the one thing netplay cannot let differ.
//!
//! So music is offline only, which is a rule that needs somewhere to stand:
//! an image nothing customizes. That is this one, at `library/online`. It is
//! derived, never edited, and rebuilt whenever what it is derived from moves.
//!
//! # What it is derived from
//!
//! The pristine image (`library/imported/<sha>`), plus the installed roster if
//! there is one, and nothing else. `source.json` beside it records both, so
//! [`freshness`] can answer without hashing 1.4 GB: a game patch changes the
//! pristine hash, a roster push changes the roster hash, and either makes this
//! image stale. Textures are not part of it. They are Dolphin's own overlay,
//! read from a folder at run time and never inside the disc, so they cannot
//! make two players' images differ.
//!
//! # What it is not
//!
//! Not a different build of the game. `SYSTEM_ARCHITECTURE.md` reserves a
//! genuinely separate online build with its own disc id; that build does not
//! exist and this is not it. This is the same game with the same disc id,
//! assembled the standard way, which is all lockstep needs.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::library::hash_file;
use crate::platform::{DataDir, Error, Platform, Result};
use crate::roster::{self, ApplyOutcome, ApplyRefusal, ApplyRequest, BuilderReport, ROSTER_FILE};

/// The name the online image ships under, matching the offline one.
pub const IMAGE_FILE: &str = "NSR.iso";
/// What the image was built from, beside the image.
pub const SOURCE_FILE: &str = "source.json";

pub fn online_dir(data_root: &Path) -> PathBuf {
    data_root.join(DataDir::LibraryOnline.relative())
}

/// The image netplay boots.
pub fn online_path(data_root: &Path) -> PathBuf {
    online_dir(data_root).join(IMAGE_FILE)
}

pub fn source_path(data_root: &Path) -> PathBuf {
    online_dir(data_root).join(SOURCE_FILE)
}

/// The installed roster, when one is installed.
pub fn roster_file(data_root: &Path) -> Option<PathBuf> {
    let p = data_root.join(DataDir::LibraryRoster.relative()).join(ROSTER_FILE);
    p.is_file().then_some(p)
}

/// What the online image was built from.
///
/// Hashes rather than versions: a version label is what a publisher chose to
/// call something, and two builds can share one. These are the bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Source {
    /// The pristine image's own hash, which is its folder name.
    pub pristine_sha256: String,
    /// The roster applied, or absent when none was installed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub roster_sha256: Option<String>,
}

impl Source {
    fn load(data_root: &Path) -> Option<Source> {
        let bytes = std::fs::read(source_path(data_root)).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    fn save(&self, data_root: &Path) -> Result<()> {
        let dir = online_dir(data_root);
        std::fs::create_dir_all(&dir).map_err(|source| Error::Io { path: dir.clone(), source })?;
        let path = source_path(data_root);
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| Error::PlainIo(std::io::Error::other(e)))?;
        std::fs::write(&path, bytes).map_err(|source| Error::Io { path, source })
    }
}

/// What the current pristine image and roster say the online image should be
/// built from. `None` when there is no pristine image to build from.
pub fn wanted(data_root: &Path) -> Result<Option<Source>> {
    let Some(pristine) = crate::game_image::find_pristine(data_root) else {
        return Ok(None);
    };
    let roster_sha256 = match roster_file(data_root) {
        Some(p) => Some(hash_file(&p)?.as_str().to_string()),
        None => None,
    };
    Ok(Some(Source { pristine_sha256: pristine.folder_sha256, roster_sha256 }))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Freshness {
    /// No image, or no record of what built it.
    Missing,
    /// Built from something that has since moved.
    Stale { have: Source, want: Source },
    Current,
}

/// Whether the online image matches the pristine image and roster installed
/// now. Reads two small files; hashes the roster, not the disc.
pub fn freshness(data_root: &Path) -> Result<Freshness> {
    let Some(want) = wanted(data_root)? else {
        return Ok(Freshness::Missing);
    };
    if !online_path(data_root).is_file() {
        return Ok(Freshness::Missing);
    }
    match Source::load(data_root) {
        Some(have) if have == want => Ok(Freshness::Current),
        Some(have) => Ok(Freshness::Stale { have, want }),
        None => Ok(Freshness::Missing),
    }
}

/// Why the online image could not be made. Nothing was changed in any of these.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    NoPristine,
    /// A roster is installed and needs the Builder to apply it.
    NoBuilder { looked: Vec<PathBuf> },
    /// Something has the online image open.
    InUse { path: PathBuf },
    /// The Builder ran and refused, or failed.
    Roster(ApplyRefusal),
}

impl Refusal {
    pub fn message(&self) -> String {
        match self {
            Refusal::NoPristine => {
                "There is no imported game to build the netplay copy from. Import the game first.".to_string()
            }
            Refusal::NoBuilder { looked } => format!(
                "The netplay copy needs the Reborn Builder to apply your roster, and it was not found. Looked in: {}",
                looked.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", ")
            ),
            Refusal::InUse { path } => format!(
                "{} is open in another program, so it was not rebuilt. Close the game and try again.",
                path.display()
            ),
            Refusal::Roster(r) => r.message(),
        }
    }
}

#[derive(Debug)]
pub enum Outcome {
    /// Already built from what is installed now.
    Current { path: PathBuf },
    /// Rebuilt. `report` is present when a roster was applied.
    Built { path: PathBuf, report: Option<BuilderReport> },
    Refused(Refusal),
}

impl Outcome {
    /// The image to boot, when there is one.
    pub fn path(&self) -> Option<&Path> {
        match self {
            Outcome::Current { path } | Outcome::Built { path, .. } => Some(path),
            Outcome::Refused(_) => None,
        }
    }

    /// A sentence for a page or a console.
    pub fn note(&self) -> String {
        match self {
            Outcome::Current { .. } => "The netplay copy of the game is up to date.".to_string(),
            Outcome::Built { report: Some(r), .. } => format!(
                "Built the netplay copy of the game: {} players, {} teams, no custom music.",
                r.players, r.teams
            ),
            Outcome::Built { report: None, .. } => {
                "Built the netplay copy of the game from your imported disc.".to_string()
            }
            Outcome::Refused(r) => r.message(),
        }
    }
}

/// Make sure the online image matches the pristine image and roster installed
/// now, building it when it does not.
///
/// `builder` is the Reborn Builder's console build, needed only when a roster
/// is installed. With no roster the online image is the pristine image, so it
/// costs a hardlink and no disk.
pub fn ensure(platform: &Platform, data_root: &Path, builder: Option<&Path>, builder_dirs: &[PathBuf]) -> Result<Outcome> {
    let Some(want) = wanted(data_root)? else {
        return Ok(Outcome::Refused(Refusal::NoPristine));
    };
    let live = online_path(data_root);
    if freshness(data_root)? == Freshness::Current {
        return Ok(Outcome::Current { path: live });
    }
    if live.exists() && platform.files.is_locked(&live) {
        return Ok(Outcome::Refused(Refusal::InUse { path: live }));
    }
    let pristine = crate::game_image::find_pristine(data_root).ok_or_else(|| {
        Error::PlainIo(std::io::Error::new(std::io::ErrorKind::NotFound, "the pristine image went away mid-build"))
    })?;
    std::fs::create_dir_all(online_dir(data_root))
        .map_err(|source| Error::Io { path: online_dir(data_root), source })?;

    // No roster: the standard game is the pristine game, so link it and stop.
    let Some(roster_path) = roster_file(data_root) else {
        link_or_copy(platform, &pristine.path, &live)?;
        want.save(data_root)?;
        return Ok(Outcome::Built { path: live, report: None });
    };

    let Some(builder) = builder.map(Path::to_path_buf).or_else(|| roster::find_builder(platform, builder_dirs)) else {
        return Ok(Outcome::Refused(Refusal::NoBuilder { looked: builder_dirs.to_vec() }));
    };
    // The Builder writes a whole new file, so it is given the online path as
    // its output and never sees the booted image.
    let outcome = roster::apply(
        platform,
        &ApplyRequest {
            data_root,
            roster: &roster_path,
            builder: &builder,
            // No portraits into any live texture tree: this image is only ever
            // booted, never composed against.
            textures_root: None,
            accept_warnings: true,
            output: Some(&live),
        },
    )?;
    match outcome {
        ApplyOutcome::Applied(receipt) => {
            want.save(data_root)?;
            Ok(Outcome::Built { path: live, report: Some(receipt.report) })
        }
        // The Builder produced the bytes already there. Still record what they
        // were built from, so the next check is a file read rather than a run.
        ApplyOutcome::Unchanged { image, report, .. } => {
            want.save(data_root)?;
            Ok(Outcome::Built { path: image, report: Some(report) })
        }
        ApplyOutcome::Refused(r) => Ok(Outcome::Refused(Refusal::Roster(r))),
    }
}

/// Put `src` at `dest`: a hardlink where the filesystem allows, a copy where
/// it does not. Written to a temporary name and renamed, so `dest` is never
/// half a disc.
fn link_or_copy(platform: &Platform, src: &Path, dest: &Path) -> Result<()> {
    let parent = dest.parent().expect("the online image has a parent");
    let temp = parent.join(format!("{IMAGE_FILE}.new"));
    if temp.exists() {
        std::fs::remove_file(&temp).map_err(|source| Error::Io { path: temp.clone(), source })?;
    }
    if let Err(e) = platform.files.hardlink(src, &temp) {
        tracing::info!("hardlink refused ({e}); copying the netplay image instead");
        let _ = std::fs::remove_file(&temp);
        std::fs::copy(src, &temp).map_err(|source| Error::Io { path: temp.clone(), source })?;
    }
    if let Err(e) = platform.files.atomic_replace(&temp, dest) {
        let _ = std::fs::remove_file(&temp);
        return Err(e);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let bytes = b"pristine game bytes";
        let sha = crate::manifest::Sha256Hex::from_bytes(bytes);
        let pdir = crate::game_image::pristine_dir(&root, sha.as_str());
        std::fs::create_dir_all(&pdir).unwrap();
        std::fs::write(pdir.join(crate::game_image::IMAGE_FILE), bytes).unwrap();
        (dir, root)
    }

    #[test]
    fn with_no_roster_the_online_image_is_the_pristine_image() {
        let (_dir, root) = scratch();
        let platform = Platform::detect();
        assert_eq!(freshness(&root).unwrap(), Freshness::Missing);

        let out = ensure(&platform, &root, None, &[]).unwrap();
        let path = out.path().unwrap().to_path_buf();
        assert_eq!(path, online_path(&root));
        assert_eq!(std::fs::read(&path).unwrap(), b"pristine game bytes");
        assert!(matches!(out, Outcome::Built { report: None, .. }));
        assert_eq!(freshness(&root).unwrap(), Freshness::Current);

        // A second call does nothing.
        assert!(matches!(ensure(&platform, &root, None, &[]).unwrap(), Outcome::Current { .. }));
    }

    #[test]
    fn a_new_pristine_image_makes_it_stale() {
        let (_dir, root) = scratch();
        let platform = Platform::detect();
        ensure(&platform, &root, None, &[]).unwrap();
        assert_eq!(freshness(&root).unwrap(), Freshness::Current);

        // A game patch lands a second pristine image.
        let bytes = b"patched game bytes";
        let sha = crate::manifest::Sha256Hex::from_bytes(bytes);
        let pdir = crate::game_image::pristine_dir(&root, sha.as_str());
        std::fs::create_dir_all(&pdir).unwrap();
        std::fs::write(pdir.join(crate::game_image::IMAGE_FILE), bytes).unwrap();

        match freshness(&root).unwrap() {
            Freshness::Stale { have, want } => assert_ne!(have.pristine_sha256, want.pristine_sha256),
            other => panic!("{other:?}"),
        }
        ensure(&platform, &root, None, &[]).unwrap();
        assert_eq!(std::fs::read(online_path(&root)).unwrap(), b"patched game bytes");
        assert_eq!(freshness(&root).unwrap(), Freshness::Current);
    }

    #[test]
    fn a_roster_needs_the_builder_and_nothing_is_written_without_one() {
        let (_dir, root) = scratch();
        let platform = Platform::detect();
        let rdir = root.join(DataDir::LibraryRoster.relative());
        std::fs::create_dir_all(&rdir).unwrap();
        std::fs::write(rdir.join(ROSTER_FILE), b"a roster export").unwrap();

        match ensure(&platform, &root, None, &[]).unwrap() {
            Outcome::Refused(r @ Refusal::NoBuilder { .. }) => {
                assert!(r.message().contains("Reborn Builder"), "{}", r.message())
            }
            other => panic!("{other:?}"),
        }
        assert!(!online_path(&root).exists(), "nothing was written");
        assert_eq!(freshness(&root).unwrap(), Freshness::Missing);
    }

    #[test]
    fn a_roster_push_makes_it_stale() {
        let (_dir, root) = scratch();
        let platform = Platform::detect();
        ensure(&platform, &root, None, &[]).unwrap();
        let rdir = root.join(DataDir::LibraryRoster.relative());
        std::fs::create_dir_all(&rdir).unwrap();
        std::fs::write(rdir.join(ROSTER_FILE), b"a roster export").unwrap();
        assert!(matches!(freshness(&root).unwrap(), Freshness::Stale { .. }));
    }

    #[test]
    fn with_no_pristine_image_there_is_nothing_to_build() {
        let dir = tempfile::tempdir().unwrap();
        let platform = Platform::detect();
        assert!(wanted(dir.path()).unwrap().is_none());
        match ensure(&platform, dir.path(), None, &[]).unwrap() {
            Outcome::Refused(r @ Refusal::NoPristine) => assert!(r.message().contains("Import the game")),
            other => panic!("{other:?}"),
        }
    }
}
