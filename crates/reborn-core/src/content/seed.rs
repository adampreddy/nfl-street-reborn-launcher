//! Seed the data root's content trees from what the installer put beside the
//! launcher, so the first content check finds them already right.
//!
//! The installer ships the runtime at `<install>\runtime` and the texture
//! pack at `<install>\content\textures\<game>`. The content system delivers
//! the same two components into the DATA root (`runtime`, `library/textures/
//! <game>`), and plans against what is there. Measured 2026-09-07 on an empty
//! data root: the first check planned 8,192 texture files (3.18 GB) and 2,569
//! runtime files (109 MB), all of which were sitting in the install root
//! already. A player with automatic updates on would have downloaded the pack
//! again minutes after installing it.
//!
//! So, before the first check, each data-root tree that is absent or empty is
//! filled from its install-root twin by hardlink (no bytes copied, no extra
//! disk; a copy on a filesystem without hardlinks or across volumes). A tree
//! that already holds anything is never touched: it is the player's, or a
//! migrated one, or delivered by an update. With the installer's trees built
//! from the same files the origin serves, the check that follows plans zero
//! fetches and the pipeline adopts the version by writing its pointer.

use std::path::{Path, PathBuf};

use crate::inventory::ComponentId;
use crate::platform::{DataDir, Error, Platform, Result};

/// The folder beside the launcher that holds shipped content.
pub const SHIPPED_CONTENT_DIR: &str = "content";

/// How one tree was seeded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeedHow {
    Linked,
    Copied,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Seeded {
    pub component: ComponentId,
    pub from: PathBuf,
    pub to: PathBuf,
    pub files: usize,
    pub how: SeedHow,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkipReason {
    /// The installer shipped nothing for this component.
    NothingShipped,
    /// The data-root tree already holds files: the player's, migrated, or
    /// delivered by an update. Never written over.
    AlreadyPresent,
    /// A content update has installed this component (its pointer exists).
    Delivered,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skipped {
    pub component: ComponentId,
    pub reason: SkipReason,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SeedReport {
    pub seeded: Vec<Seeded>,
    pub skipped: Vec<Skipped>,
}

impl SeedReport {
    pub fn files(&self) -> usize {
        self.seeded.iter().map(|s| s.files).sum()
    }
}

/// The install-root twin of each seedable component.
fn pairs(install_root: &Path, data_root: &Path, game_id: &str) -> Vec<(ComponentId, PathBuf, PathBuf)> {
    let data = |dir: DataDir| {
        let mut p = data_root.to_path_buf();
        p.extend(dir.relative().split('/'));
        p
    };
    vec![
        (ComponentId::runtime_here(), install_root.join("runtime"), data(DataDir::Runtime)),
        (
            ComponentId::Textures,
            install_root.join(SHIPPED_CONTENT_DIR).join("textures").join(game_id),
            data(DataDir::LibraryTextures).join(game_id),
        ),
        // The mod packs the installer carries, so the library is full on first
        // run and the first check adopts them instead of fetching them.
        (
            ComponentId::Mods,
            install_root.join(SHIPPED_CONTENT_DIR).join("mods").join("packages"),
            data(DataDir::ModsPackages),
        ),
        // The game patch chain, so a disc imported during setup is moved to
        // the newest build offline and the first check has nothing to fetch.
        (
            ComponentId::GamePatch,
            install_root.join(SHIPPED_CONTENT_DIR).join("game-patch"),
            data(DataDir::LibraryGamePatch),
        ),
    ]
}

fn has_any_file(dir: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else { return false };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_file() || (p.is_dir() && has_any_file(&p)) {
            return true;
        }
    }
    false
}

fn io(path: &Path, source: std::io::Error) -> Error {
    Error::Io {
        path: path.to_path_buf(),
        source,
    }
}

/// Mirror `from` into `to`, hardlinking every file (copying when a link is
/// refused). `to` must not exist or must be empty.
fn mirror(platform: &Platform, from: &Path, to: &Path) -> Result<(usize, SeedHow)> {
    let mut files = 0usize;
    let mut how = SeedHow::Linked;
    let mut stack = vec![from.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let rel = dir.strip_prefix(from).expect("under the source");
        let dest_dir = to.join(rel);
        std::fs::create_dir_all(&dest_dir).map_err(|e| io(&dest_dir, e))?;
        for entry in std::fs::read_dir(&dir).map_err(|e| io(&dir, e))?.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if !path.is_file() {
                continue;
            }
            let dest = dest_dir.join(entry.file_name());
            // Config the player edits is COPIED, never hardlinked.
            //
            // A hardlink makes the folder the launcher runs from and the
            // content system's pristine tree the same bytes on disk, so a
            // launch-time write to ReShade.ini or a preset reaches inside the
            // managed tree and the next verify calls the managed tree damaged.
            // Everything else stays linked: that is what keeps seeding a
            // multi-gigabyte install close to free.
            let editable = dest
                .strip_prefix(to)
                .ok()
                .map(|r| r.to_string_lossy().replace('\\', "/"))
                .is_some_and(|r| crate::library::is_player_editable(&r));
            if editable {
                let _ = std::fs::remove_file(&dest);
                std::fs::copy(&path, &dest).map_err(|e| io(&dest, e))?;
            } else if platform.files.hardlink(&path, &dest).is_err() {
                let _ = std::fs::remove_file(&dest);
                std::fs::copy(&path, &dest).map_err(|e| io(&dest, e))?;
                how = SeedHow::Copied;
            }
            files += 1;
        }
    }
    Ok((files, how))
}

/// Fill each empty data-root tree from its install-root twin. Idempotent:
/// a tree with anything in it is left alone, and a component with a content
/// pointer is left alone.
pub fn seed_from_install(
    platform: &Platform,
    data_root: &Path,
    install_root: &Path,
    game_id: &str,
) -> Result<SeedReport> {
    let mut report = SeedReport::default();
    let layout = crate::content::ContentLayout::new(data_root);
    for (component, from, to) in pairs(install_root, data_root, game_id) {
        if !from.is_dir() || !has_any_file(&from) {
            report.skipped.push(Skipped {
                component,
                reason: SkipReason::NothingShipped,
            });
            continue;
        }
        if layout.pointer_path(component.as_str()).is_file() {
            report.skipped.push(Skipped {
                component,
                reason: SkipReason::Delivered,
            });
            continue;
        }
        if to.is_dir() && has_any_file(&to) {
            report.skipped.push(Skipped {
                component,
                reason: SkipReason::AlreadyPresent,
            });
            continue;
        }
        let (files, how) = mirror(platform, &from, &to)?;
        report.seeded.push(Seeded {
            component,
            from,
            to,
            files,
            how,
        });
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put(root: &Path, rel: &str, bytes: &[u8]) {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, bytes).unwrap();
    }

    fn install_with_both(install: &Path) {
        put(install, "runtime/Dolphin.exe", b"dolphin");
        put(install, "runtime/Sys/GC/font.bin", b"font");
        put(install, "content/textures/GN7E69/a.png", b"a");
        put(install, "content/textures/GN7E69/sub/b.png", b"b");
        put(install, "content/mods/packages/jsgme.aqua-football/1.0.0/package.json", b"{}");
        put(install, "content/game-patch/chain.json", b"{}");
    }

    #[test]
    fn empty_trees_are_filled_from_the_install_root_and_a_second_call_touches_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let platform = Platform::detect();
        let install = dir.path().join("install");
        let data = dir.path().join("data");
        install_with_both(&install);

        let r = seed_from_install(&platform, &data, &install, "GN7E69").unwrap();
        assert_eq!(r.seeded.len(), 4, "{r:?}");
        assert_eq!(r.files(), 6);
        assert_eq!(std::fs::read(data.join("library/game-patch/chain.json")).unwrap(), b"{}");
        assert_eq!(std::fs::read(data.join("runtime/Sys/GC/font.bin")).unwrap(), b"font");
        assert_eq!(std::fs::read(data.join("library/textures/GN7E69/sub/b.png")).unwrap(), b"b");
        assert_eq!(std::fs::read(data.join("mods/packages/jsgme.aqua-football/1.0.0/package.json")).unwrap(), b"{}");
        assert!(r.seeded.iter().all(|s| s.how == SeedHow::Linked), "same volume, so links: {r:?}");

        let again = seed_from_install(&platform, &data, &install, "GN7E69").unwrap();
        assert!(again.seeded.is_empty());
        assert!(again.skipped.iter().all(|s| s.reason == SkipReason::AlreadyPresent), "{again:?}");
    }

    #[test]
    fn a_tree_that_holds_anything_is_never_written_over() {
        let dir = tempfile::tempdir().unwrap();
        let platform = Platform::detect();
        let install = dir.path().join("install");
        let data = dir.path().join("data");
        install_with_both(&install);
        // A migrated texture tree, with one file the installer never had.
        put(&data, "library/textures/GN7E69/mine.png", b"mine");

        let r = seed_from_install(&platform, &data, &install, "GN7E69").unwrap();
        assert_eq!(r.seeded.iter().map(|s| s.component).collect::<Vec<_>>(), vec![ComponentId::runtime_here(), ComponentId::Mods, ComponentId::GamePatch]);
        assert!(!data.join("library/textures/GN7E69/a.png").exists(), "the player's tree was left alone");
        assert!(data.join("library/textures/GN7E69/mine.png").exists());
    }

    #[test]
    fn a_component_an_update_delivered_and_one_the_installer_did_not_ship_are_skipped_by_name() {
        let dir = tempfile::tempdir().unwrap();
        let platform = Platform::detect();
        let install = dir.path().join("install");
        let data = dir.path().join("data");
        put(&install, "runtime/Dolphin.exe", b"dolphin");
        // A pointer says an update installed the runtime already: this OS's
        // runtime, which is the one the seeding pairs name.
        let pointer = crate::content::ContentLayout::new(&data).pointer_path(ComponentId::runtime_here().as_str());
        std::fs::create_dir_all(pointer.parent().unwrap()).unwrap();
        std::fs::write(&pointer, b"{}").unwrap();

        let r = seed_from_install(&platform, &data, &install, "GN7E69").unwrap();
        assert!(r.seeded.is_empty(), "{r:?}");
        let reasons: Vec<(ComponentId, SkipReason)> = r.skipped.iter().map(|s| (s.component, s.reason.clone())).collect();
        assert!(reasons.contains(&(ComponentId::runtime_here(), SkipReason::Delivered)));
        assert!(reasons.contains(&(ComponentId::Textures, SkipReason::NothingShipped)));
        assert!(!data.join("runtime").exists());
    }

    #[test]
    fn a_seeded_tree_hashes_like_the_shipped_one_so_a_check_plans_nothing() {
        // The property the whole module exists for: after seeding, a plan
        // against the manifest the installer's tree was published from
        // fetches nothing.
        use crate::content::{ContentService, PlanOutcome, TrustedRelease};
        use crate::library::RebuildState;
        use crate::manifest::{Component, ManifestFile, Sha256Hex, COMPONENT_SCHEMA};

        #[derive(Debug)]
        struct Any;
        impl crate::content::SignatureVerifier for Any {
            fn verify(&self, _b: &[u8], _s: &[u8]) -> Result<bool> {
                Ok(true)
            }
        }

        let dir = tempfile::tempdir().unwrap();
        let platform = Platform::detect();
        let install = dir.path().join("install");
        let data = dir.path().join("data");
        put(&install, "runtime/Dolphin.exe", b"dolphin");
        put(&install, "runtime/Sys/GC/font.bin", b"font");
        seed_from_install(&platform, &data, &install, "GN7E69").unwrap();

        let files = vec![
            ManifestFile { path: "Dolphin.exe".into(), bytes: 7, sha256: Sha256Hex::from_bytes(b"dolphin") },
            ManifestFile { path: "Sys/GC/font.bin".into(), bytes: 4, sha256: Sha256Hex::from_bytes(b"font") },
        ];
        let component = Component {
            schema: COMPONENT_SCHEMA.to_string(),
            component: "runtime".into(),
            version: "1.0.0".into(),
            sequence: 1,
            install_root: DataDir::Runtime.relative().to_string(),
            customizable: false,
            files,
        };
        let bytes = serde_json::to_vec(&component).unwrap();
        let release = format!(
            r#"{{"schema":"nsr.release.v1","version":"3.0.0","sequence":1,"components":{{"runtime":{{"url":"mem://r","sha256":"{}"}}}}}}"#,
            Sha256Hex::from_bytes(&bytes)
        );
        let trusted = TrustedRelease::open(&Any, release.as_bytes(), b"sig").unwrap();
        let service = ContentService::new(&platform, &data);
        match service
            .plan(&trusted, "runtime", &bytes, DataDir::Runtime, &RebuildState::default())
            .unwrap()
        {
            PlanOutcome::Update(plan) => {
                assert!(plan.fetches.is_empty(), "nothing to fetch: {:?}", plan.fetches);
                assert_eq!(plan.already_present.len(), 2);
                assert!(plan.from.is_none(), "no pointer yet: this is what adoption writes");
            }
            other => panic!("{other:?}"),
        }
    }
}
