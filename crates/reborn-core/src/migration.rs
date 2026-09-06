//! Migrating an existing 2.1 installation.
//!
//! # What is on disk today
//!
//! `%APPDATA%\NFL Street Reborn` holds both the program and the user's data,
//! which is the root cause of the two worst bugs in the shipped product:
//!
//! - The uninstaller runs `DelTree({app}, True, True, True)` over that folder,
//!   and the memory cards are inside it. **Uninstalling deletes saves.**
//! - `BackupUserData` passes a wildcard path to Inno Setup's `FileCopy`, which
//!   copies a single file and does not expand wildcards. The backup has never
//!   worked; it logs a failure and continues.
//!
//! Migration therefore has one job before anything else: get the irreplaceable
//! things out of that folder and somewhere the uninstaller cannot reach.
//!
//! # Order matters, and this order is deliberate
//!
//! 1. **Save data first.** Memory cards, then screenshots. If migration is
//!    interrupted at any point after step 1, nothing irreplaceable is at risk.
//! 2. **Un-apply JSGME mods**, before copying textures. A user with mods
//!    enabled has overlaid files plus a `!BACKUP` directory holding the
//!    originals. Copying the tree first captures the overlaid state and the
//!    pristine tree is polluted from birth.
//! 3. **Textures**, now genuinely pristine.
//! 4. **Custom soundtrack sources**, if any.
//! 5. Everything else is regenerable and is skipped.
//!
//! # What is deliberately NOT migrated
//!
//! Controller configuration. The old model is a single `GCPadNew.ini` bound to
//! `XInput/0/Gamepad`; the new one is per-device profiles keyed on hardware
//! identity. There is no honest mapping between them, and importing the old
//! file would produce a profile that claims to describe a device it was never
//! about. Re-running controller setup takes fifteen seconds and produces
//! something true.

use std::path::{Path, PathBuf};

use crate::platform::{DataDir, Error, Result};

/// A 2.1 installation found on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyInstall {
    pub root: PathBuf,
    pub has_memory_cards: bool,
    pub has_screenshots: bool,
    pub has_textures: bool,
    /// JSGME is present and at least one mod is applied.
    pub has_applied_mods: bool,
    /// `!BACKUP` exists and is not empty.
    ///
    /// True is NOT the same as "everything can be restored", and treating it
    /// that way was a real bug. Measured on a real install: `!BACKUP` was not
    /// empty, so this said yes, while `Xbox Controls` had 19 recorded paths and
    /// 0 backups. Un-applying it was impossible and migration planned to
    /// proceed anyway, which would have copied modded textures into the
    /// pristine tree and called them pristine.
    ///
    /// Use `unrecoverable_mods` for the question that actually matters.
    pub has_jsgme_backup: bool,
    /// Why the mod state makes migration unsafe, if it does.
    ///
    /// Computed by [`crate::jsgme::AppliedState::blocker`] rather than here, so
    /// there is one answer to "can these originals be restored" instead of two
    /// that can disagree.
    pub mod_blocker: Option<String>,
}

impl LegacyInstall {
    /// Look for an existing install at the standard location.
    pub fn detect(app_data_root: &Path) -> Option<Self> {
        if !app_data_root.is_dir() {
            return None;
        }
        let user = app_data_root.join("User");
        let mods = app_data_root.join("MODS");

        // A folder is only a 2.1 install if it has the shape of one. An empty
        // directory with the right name is not something to migrate from.
        let looks_legacy = user.is_dir() || app_data_root.join("Games").is_dir();
        if !looks_legacy {
            return None;
        }

        // JSGME records an install log per applied mod in !INSTLOGS, and puts
        // the files it displaced into !BACKUP. Those are two different facts
        // and conflating them hides the dangerous case: mods applied with no
        // way to restore the originals.
        //
        // Verified against the shipped MODS folder, where both directories
        // exist and are empty, which correctly reads as nothing applied.
        let backup = mods.join("!BACKUP");
        let instlogs = mods.join("!INSTLOGS");
        let non_empty = |p: &Path| {
            p.read_dir()
                .map(|mut d| d.next().is_some())
                .unwrap_or(false)
        };
        let has_applied_mods = non_empty(&instlogs);

        Some(Self {
            root: app_data_root.to_path_buf(),
            has_memory_cards: user.join("GC").is_dir(),
            has_screenshots: user.join("ScreenShots").is_dir(),
            has_textures: user.join("Load").join("Textures").is_dir(),
            has_applied_mods,
            has_jsgme_backup: non_empty(&backup),
            // Asked, not recomputed. jsgme owns the comparison between what an
            // install log recorded and what !BACKUP actually holds.
            mod_blocker: crate::jsgme::AppliedState::read(&mods)
                .ok()
                .and_then(|state| state.blocker()),
        })
    }

    /// The single most important thing in the folder.
    pub fn memory_card_dir(&self) -> PathBuf {
        self.root.join("User").join("GC")
    }
}

/// One unit of migration work.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationStep {
    pub order: u8,
    pub what: &'static str,
    pub from: PathBuf,
    pub to: DataDir,
    /// Whether losing this would cost the user something unrecoverable.
    pub irreplaceable: bool,
    /// Shown to the user, and the reason this step sits where it does.
    pub why: &'static str,
}

/// Why a thing present in the old install is being left behind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationSkip {
    pub what: &'static str,
    pub why: &'static str,
}

/// What migration will do, before it does any of it.
#[derive(Debug, Clone)]
pub struct MigrationPlan {
    pub source: LegacyInstall,
    pub steps: Vec<MigrationStep>,
    pub skipped: Vec<MigrationSkip>,
    /// JSGME mods are applied and must be un-applied before textures are
    /// copied, or the pristine tree is polluted.
    pub needs_mod_unapply: bool,
    /// Mods are applied but `!BACKUP` is missing or empty, so the originals
    /// cannot be restored and the textures cannot be trusted.
    /// Why migration must not run. Owned rather than static, because the
    /// useful message names which mods are affected.
    pub blocked: Option<String>,
}

impl MigrationPlan {
    /// Onboarding imports personal progress only. Mod state cannot make these
    /// files unsafe: no texture is imported as a pristine original.
    pub fn saves_only(install: &LegacyInstall) -> Self {
        let mut plan = Self::build(install);
        plan.steps.retain(|step| step.irreplaceable);
        plan.needs_mod_unapply = false;
        plan.blocked = None;
        plan
    }

    pub fn build(install: &LegacyInstall) -> Self {
        let mut steps = Vec::new();
        let mut skipped = Vec::new();

        if install.has_memory_cards {
            steps.push(MigrationStep {
                order: 1,
                what: "Memory cards",
                from: install.memory_card_dir(),
                to: DataDir::Saves,
                irreplaceable: true,
                why: "Franchise saves. The old uninstaller deletes these, so \
                      they move first and everything else can be interrupted \
                      safely",
            });
        }

        if install.has_screenshots {
            steps.push(MigrationStep {
                order: 2,
                what: "Screenshots",
                from: install.root.join("User").join("ScreenShots"),
                to: DataDir::Screenshots,
                irreplaceable: true,
                why: "Not regenerable, and small",
            });
        }

        if install.has_textures {
            steps.push(MigrationStep {
                order: 3,
                what: "Textures",
                from: install.root.join("User").join("Load").join("Textures"),
                to: DataDir::LibraryTextures,
                irreplaceable: false,
                why: "About 2 GB. Copied after mods are un-applied so the \
                      pristine tree is genuinely pristine",
            });
        }

        // Deliberate omissions, each with a reason the user can be shown.
        skipped.push(MigrationSkip {
            what: "Controller configuration",
            why: "The old setup binds one XInput device for everyone. The new \
                  one keeps a profile per physical controller, and there is no \
                  honest way to convert between them. Controller setup takes \
                  about fifteen seconds and produces something true",
        });
        skipped.push(MigrationSkip {
            what: "The game image",
            why: "The launcher ships its own verified copy, so there is nothing \
                  to gain by moving 1.5 GB",
        });
        skipped.push(MigrationSkip {
            what: "Dolphin graphics settings",
            why: "Regenerated from your graphics preset on every launch",
        });
        skipped.push(MigrationSkip {
            what: "Shader and texture caches",
            why: "Rebuilt automatically the first time you play",
        });

        let needs_mod_unapply = install.has_applied_mods;
        // Two ways this is unsafe. A completely absent !BACKUP is obvious.
        // Partial coverage is not, and it is the one that really happens: the
        // folder exists, looks populated, and hides that one mod's originals
        // were never kept. jsgme::AppliedState::blocker covers both.
        let blocked: Option<String> = if install.has_applied_mods && !install.has_jsgme_backup {
            Some(
                "Mods are applied and the JSGME !BACKUP folder is missing or \
                 empty, so any original file they displaced no longer exists \
                 anywhere. The live texture tree holds modded files as if they \
                 were pristine. Copying it now would import them as a pristine \
                 tree, and every later check would measure against the wrong \
                 thing. Un-apply the mods in JSGME first, or let the launcher \
                 fetch a clean copy of the textures component."
                    .to_string(),
            )
        } else {
            install.mod_blocker.clone()
        };

        steps.sort_by_key(|s| s.order);
        Self {
            source: install.clone(),
            steps,
            skipped,
            needs_mod_unapply,
            blocked,
        }
    }

    /// Whether migration can proceed without asking the user something first.
    pub fn is_safe(&self) -> bool {
        self.blocked.is_none()
    }

    /// Steps whose loss would cost the user something unrecoverable.
    pub fn irreplaceable_steps(&self) -> Vec<&MigrationStep> {
        self.steps.iter().filter(|s| s.irreplaceable).collect()
    }

    /// The point after which an interruption cannot cost anything permanent.
    ///
    /// Used by the progress screen to tell the user when it is safe to walk
    /// away, which is a genuinely useful thing to know during a long copy.
    pub fn safe_after_step(&self) -> Option<u8> {
        self.steps
            .iter()
            .filter(|s| s.irreplaceable)
            .map(|s| s.order)
            .max()
    }
}

/// Copies a directory tree, refusing to overwrite anything already present.
///
/// Refusing rather than overwriting is the whole point. Migration runs against
/// a destination that may already hold a previous attempt's output, and silent
/// overwriting is how a partially migrated save gets replaced by an older one.
pub fn copy_tree_no_clobber(from: &Path, to: &Path) -> Result<u64> {
    copy_tree_with_progress(from,to,&mut |_|{})
}
pub fn copy_tree_with_progress(from:&Path,to:&Path,progress:&mut dyn FnMut(u64))->Result<u64>{
    copy_tree_checked(from,to,&mut Vec::new(),progress)
}
fn copy_tree_checked(from: &Path, to: &Path, ancestors: &mut Vec<PathBuf>, progress:&mut dyn FnMut(u64)) -> Result<u64> {
    let mut copied = 0u64;
    if !from.is_dir() {
        return Ok(0);
    }
    std::fs::create_dir_all(to).map_err(|source| Error::Io {
        path: to.to_path_buf(),
        source,
    })?;

    let canonical = std::fs::canonicalize(from)?;
    let target = std::fs::canonicalize(to)?;
    if target.starts_with(&canonical) || ancestors.contains(&canonical) {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "The import contains a directory loop or its destination is inside its source.").into());
    }
    ancestors.push(canonical);
    let entries = std::fs::read_dir(from).map_err(|source| Error::Io {
        path: from.to_path_buf(),
        source,
    })?;

    for entry in entries {
        let entry = entry.map_err(|source| Error::Io {
            path: from.to_path_buf(),
            source,
        })?;
        let src = entry.path();
        let dest = to.join(entry.file_name());

        if src.is_dir() {
            copied += copy_tree_checked(&src, &dest, ancestors, progress)?;
        } else if dest.exists() {
            // Already there. Leave it alone and say nothing, because the
            // destination is authoritative once it exists.
            continue;
        } else {
            let bytes = std::fs::copy(&src, &dest).map_err(|source| Error::Io {
                path: dest.clone(),
                source,
            })?;
            copied += bytes;
            progress(bytes);
        }
    }
    ancestors.pop();
    Ok(copied)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copying_into_own_tree_is_rejected() {
        let dir=tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("save.raw"),b"keep").unwrap();
        assert!(copy_tree_no_clobber(dir.path(),&dir.path().join("nested")).is_err());
        assert_eq!(std::fs::read(dir.path().join("save.raw")).unwrap(),b"keep");
    }

    /// Build a plausible 2.1 install on disk.
    fn legacy(dir: &Path, with_mods: bool, with_backup: bool) -> LegacyInstall {
        let user = dir.join("User");
        std::fs::create_dir_all(user.join("GC")).unwrap();
        std::fs::write(user.join("GC").join("MemoryCardA.USA.raw"), b"saves").unwrap();
        std::fs::create_dir_all(user.join("ScreenShots")).unwrap();
        std::fs::create_dir_all(user.join("Load").join("Textures").join("GN7E69")).unwrap();
        std::fs::create_dir_all(dir.join("Games")).unwrap();

        // JSGME applies a mod by writing an install log and stashing the files
        // it displaced. with_mods controls the first, with_backup the second,
        // so the dangerous combination can be built: applied, unrestorable.
        let mods = dir.join("MODS");
        let backup = mods.join("!BACKUP");
        let instlogs = mods.join("!INSTLOGS");
        std::fs::create_dir_all(&backup).unwrap();
        std::fs::create_dir_all(&instlogs).unwrap();
        if with_mods {
            std::fs::write(instlogs.join("PS Controls.txt"), b"applied").unwrap();
        }
        if with_backup && with_mods {
            std::fs::create_dir_all(backup.join("PS Controls")).unwrap();
            std::fs::write(backup.join("PS Controls").join("original.dds"), b"pristine").unwrap();
        }

        LegacyInstall::detect(dir).expect("should detect a legacy install")
    }

    #[test]
    fn an_empty_directory_is_not_a_legacy_install() {
        let dir = tempfile::tempdir().unwrap();
        assert!(
            LegacyInstall::detect(dir.path()).is_none(),
            "a folder with the right name but no shape must not be migrated from"
        );
    }

    #[test]
    fn a_missing_directory_is_not_a_legacy_install() {
        assert!(LegacyInstall::detect(Path::new("/nope/does/not/exist")).is_none());
    }

    #[test]
    fn detection_finds_what_is_actually_there() {
        let dir = tempfile::tempdir().unwrap();
        let install = legacy(dir.path(), false, true);
        assert!(install.has_memory_cards);
        assert!(install.has_screenshots);
        assert!(install.has_textures);
        assert!(
            !install.has_applied_mods,
            "an empty !INSTLOGS means nothing is applied"
        );
        assert!(
            !install.has_jsgme_backup,
            "an empty !BACKUP is not a usable backup, which is how the shipped folder looks"
        );
    }

    /// Saves move first so that an interruption after step 1 costs nothing
    /// permanent.
    #[test]
    fn memory_cards_are_the_first_step() {
        let dir = tempfile::tempdir().unwrap();
        let plan = MigrationPlan::build(&legacy(dir.path(), false, true));
        assert_eq!(plan.steps[0].what, "Memory cards");
        assert_eq!(plan.steps[0].order, 1);
        assert!(plan.steps[0].irreplaceable);
        assert_eq!(plan.steps[0].to, DataDir::Saves);
    }

    #[test]
    fn textures_are_copied_after_everything_irreplaceable() {
        let dir = tempfile::tempdir().unwrap();
        let plan = MigrationPlan::build(&legacy(dir.path(), false, true));
        let textures = plan.steps.iter().find(|s| s.what == "Textures").unwrap();
        let last_irreplaceable = plan.safe_after_step().unwrap();
        assert!(
            textures.order > last_irreplaceable,
            "a 2 GB copy must not sit between the user and their saves"
        );
        assert!(!textures.irreplaceable, "textures can be refetched");
    }

    #[test]
    fn the_plan_says_when_it_is_safe_to_walk_away() {
        let dir = tempfile::tempdir().unwrap();
        let plan = MigrationPlan::build(&legacy(dir.path(), false, true));
        assert_eq!(plan.safe_after_step(), Some(2), "cards then screenshots");
    }

    /// Copying a modded texture tree captures the overlaid state, and the
    /// pristine tree is polluted from birth.
    #[test]
    fn applied_mods_require_un_applying_first() {
        let dir = tempfile::tempdir().unwrap();
        let plan = MigrationPlan::build(&legacy(dir.path(), true, true));
        assert!(plan.needs_mod_unapply);
        assert!(plan.is_safe(), "with !BACKUP present it can be un-applied");
    }

    /// The case that must stop and ask rather than guess.
    #[test]
    fn applied_mods_with_no_backup_block_migration() {
        let dir = tempfile::tempdir().unwrap();
        let plan = MigrationPlan::build(&legacy(dir.path(), true, false));
        assert!(!plan.is_safe());
        let reason = plan.blocked.unwrap();
        assert!(reason.contains("!BACKUP"), "{reason}");
        assert!(
            reason.contains("as if they were pristine"),
            "the reason must say what goes wrong, not just that something did"
        );
    }

    #[test]
    fn controller_config_is_skipped_with_a_reason_worth_reading() {
        let dir = tempfile::tempdir().unwrap();
        let plan = MigrationPlan::build(&legacy(dir.path(), false, true));
        let skip = plan
            .skipped
            .iter()
            .find(|s| s.what == "Controller configuration")
            .expect("controller config must be explicitly skipped");
        assert!(skip.why.contains("XInput"), "{}", skip.why);
        assert!(skip.why.contains("fifteen seconds"), "{}", skip.why);
    }

    #[test]
    fn the_game_image_is_not_dragged_across() {
        let dir = tempfile::tempdir().unwrap();
        let plan = MigrationPlan::build(&legacy(dir.path(), false, true));
        assert!(plan.skipped.iter().any(|s| s.what == "The game image"));
        assert!(
            !plan.steps.iter().any(|s| s.what.contains("game image")),
            "1.5 GB the launcher already ships"
        );
    }

    #[test]
    fn copying_moves_the_tree_and_reports_the_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let from = dir.path().join("from");
        let to = dir.path().join("to");
        std::fs::create_dir_all(from.join("nested")).unwrap();
        std::fs::write(from.join("a.raw"), b"12345").unwrap();
        std::fs::write(from.join("nested").join("b.raw"), b"678").unwrap();

        let bytes = copy_tree_no_clobber(&from, &to).unwrap();
        assert_eq!(bytes, 8);
        assert_eq!(std::fs::read(to.join("a.raw")).unwrap(), b"12345");
        assert_eq!(
            std::fs::read(to.join("nested").join("b.raw")).unwrap(),
            b"678"
        );
    }

    /// Migration may run twice. The second run must not replace a newer save
    /// with the older one it came from.
    #[test]
    fn copying_never_overwrites_what_is_already_there() {
        let dir = tempfile::tempdir().unwrap();
        let from = dir.path().join("from");
        let to = dir.path().join("to");
        std::fs::create_dir_all(&from).unwrap();
        std::fs::create_dir_all(&to).unwrap();
        std::fs::write(from.join("card.raw"), b"the old card").unwrap();
        std::fs::write(to.join("card.raw"), b"the card I have been playing").unwrap();

        let bytes = copy_tree_no_clobber(&from, &to).unwrap();
        assert_eq!(bytes, 0, "nothing should have been copied");
        assert_eq!(
            std::fs::read(to.join("card.raw")).unwrap(),
            b"the card I have been playing",
            "a second migration must never clobber the live save"
        );
    }

    #[test]
    fn copying_a_missing_source_is_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let bytes =
            copy_tree_no_clobber(&dir.path().join("absent"), &dir.path().join("to")).unwrap();
        assert_eq!(bytes, 0);
    }
    /// Migration exists to rescue things the player cannot get back. Putting
    /// one of those in a directory classed derived undoes the whole point,
    /// because derived directories are what a cache purge is allowed to delete.
    ///
    /// This was real: screenshots were marked irreplaceable and copied into
    /// `cache`. Found by printing a real plan with the CLI, not by a test,
    /// which is why the rule is now a test.
    #[test]
    fn nothing_irreplaceable_lands_in_a_purgeable_directory() {
        for install in [
            LegacyInstall {
                root: PathBuf::from("legacy"),
                has_memory_cards: true,
                has_screenshots: true,
                has_textures: true,
                has_applied_mods: false,
                has_jsgme_backup: true,
                mod_blocker: None,
            },
            LegacyInstall {
                root: PathBuf::from("legacy"),
                has_memory_cards: true,
                has_screenshots: true,
                has_textures: false,
                has_applied_mods: true,
                has_jsgme_backup: true,
                mod_blocker: None,
            },
        ] {
            let plan = MigrationPlan::build(&install);
            for step in plan.irreplaceable_steps() {
                assert!(
                    !step.to.is_derived(),
                    "{} is irreplaceable but is copied into {}, which a cache \
                     purge may delete",
                    step.what,
                    step.to.relative()
                );
            }
        }
    }
}
