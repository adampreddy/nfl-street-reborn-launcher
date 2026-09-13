//! The data root, made ready before anything writes into it.
//!
//! Two things can be sitting where the launcher's data goes:
//!
//! - A 2.1 install. `%APPDATA%\NFL Street Reborn` was 2.1's own folder, a
//!   live Dolphin tree with a JSGME `MODS` folder; on Windows the launcher's
//!   `mods` IS that `MODS`. Everything that is not part of the 3.0 layout is
//!   moved into one subfolder, `Reborn 2.1`, and migration reads from there.
//! - The data of a launcher built before 2026-09-12, in `NFL Street Reborn
//!   Launcher[ Beta]` beside the new root. Its known entries are moved in.
//!
//! Both are renames on one volume: instant, and nothing is ever deleted or
//! copied. Both are idempotent, and both run BEFORE the supervisor lock and
//! before seeding, from `main` in the WINDOW only. Never from `rebornctl`:
//! that binary runs on developers' PCs under every end-to-end script, against
//! scratch roots, and a first version that prepared the real root from there
//! moved Adam's 2.1 folder aside in the middle of a test run (2026-09-12).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::platform::{DataDir, Error, Platform, Result};

/// Where a 2.1 install found at the data root is moved to.
pub const LEGACY_SUBFOLDER: &str = "Reborn 2.1";

/// Top-level names the layout owns that are not the first segment of a
/// `DataDir`.
const EXTRA_TOP_LEVEL: &[&str] = &[
    "app",
    "dolphin-users",
    "journal.json",
    "migrated-effects",
    "migrated-textures",
    LEGACY_SUBFOLDER,
];

/// Every top-level name of the 3.0 layout, lowercase.
pub fn top_level_names() -> BTreeSet<String> {
    let mut names: BTreeSet<String> = DataDir::ALL
        .iter()
        .filter_map(|d| d.relative().split('/').next())
        .map(str::to_lowercase)
        .collect();
    names.extend(EXTRA_TOP_LEVEL.iter().map(|s| s.to_lowercase()));
    names
}

/// What a move step did: the entries now in their new place, and the ones
/// that could not be moved with the reason.
pub type Moves = (Vec<PathBuf>, Vec<(PathBuf, String)>);

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Prepared {
    pub tucked: Vec<PathBuf>,
    pub adopted: Vec<PathBuf>,
    /// Entries that could not be moved, with the reason. A locked file (the
    /// old game still running) is the ordinary cause; the next start retries.
    pub blocked: Vec<(PathBuf, String)>,
    /// The tucked 2.1 install, when there is one.
    pub legacy_root: Option<PathBuf>,
    /// Files whose absolute paths named the previous root and now name this
    /// one (settings, retained-version records, the stats queue).
    pub rebased: Vec<PathBuf>,
}

impl Prepared {
    /// One line per thing done, for the log.
    pub fn log(&self) {
        for p in &self.tucked {
            tracing::warn!("moved a 2.1 entry aside: {}", p.display());
        }
        for p in &self.adopted {
            tracing::warn!("adopted from the previous data root: {}", p.display());
        }
        for (p, why) in &self.blocked {
            tracing::warn!("could not move {}: {why}", p.display());
        }
        for p in &self.rebased {
            tracing::warn!("rebased paths from the previous data root in {}", p.display());
        }
    }
}

/// Make the real data root ready.
pub fn prepare_data_root(platform: &Platform) -> Result<Prepared> {
    let data = platform.paths.data_root()?;
    let previous = platform.paths.previous_data_root()?;
    prepare_at(&data, previous.as_deref())
}

/// Tuck first, then adopt: adopting `mods` into a folder that still holds
/// JSGME's `MODS` would skip it as a collision.
pub fn prepare_at(data_root: &Path, previous_root: Option<&Path>) -> Result<Prepared> {
    let mut out = Prepared::default();
    if configured_launcher_root(data_root) {
        // A configured launcher may intentionally use top-level User/Games.
        // Dolphin can also recreate User on exit. Never relocate these again:
        // doing so invalidates saved paths and may hide the player's saves.
        // Only the case-insensitive MODS/mods namespace needs separation.
        if legacy_mod_entries(data_root)? {
            let (tucked, blocked) = tuck_jsgme_mods(&data_root.join("MODS"), &data_root.join(LEGACY_SUBFOLDER).join("MODS"))?;
            out.tucked = tucked;
            out.blocked = blocked;
        }
    } else if crate::migration::LegacyInstall::detect(data_root).is_some() {
        let (tucked, blocked) = tuck_legacy(data_root)?;
        out.tucked = tucked;
        out.blocked = blocked;
    }
    out.legacy_root = legacy_root(data_root);
    if let Some(previous) = previous_root {
        if previous.is_dir() && !same_named(previous, data_root) {
            let settings = |root: &Path| root.join(DataDir::Config.relative()).join("settings.json");
            if !settings(data_root).exists() && settings(previous).exists() {
                out.adopted = adopt_previous(previous, data_root)?;
            }
        }
        // Every start, not only the one that adopted: a root adopted by a
        // build before this step existed still carries the old paths.
        if !same_named(previous, data_root) {
            out.rebased = rebase_paths(previous, data_root)?;
        }
    }
    Ok(out)
}

/// An existing, readable launcher settings file distinguishes an established
/// launcher from a raw 2.1 install. Missing files must not count: Settings::load
/// intentionally returns defaults for those.
fn configured_launcher_root(root: &Path) -> bool {
    let settings = root.join(DataDir::Config.relative()).join("settings.json");
    settings.is_file() && crate::settings::Settings::load(&settings).is_ok()
}

fn legacy_mod_entries(root: &Path) -> Result<bool> {
    let mods = root.join("MODS");
    let entries = match std::fs::read_dir(&mods) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(source) => return Err(Error::Io { path: mods, source }),
    };
    for entry in entries {
        let entry = entry.map_err(|source| Error::Io { path: mods.clone(), source })?;
        if !LAUNCHER_MODS_ENTRIES.contains(&entry.file_name().to_string_lossy().to_lowercase().as_str()) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Import detection is deliberately broad (User or Games). A launch refusal
/// must instead distinguish a raw old installation from harmless retained data
/// in an established launcher. Real shared MODS conflicts are still refused.
pub fn legacy_blocks_launcher(root: &Path) -> Result<bool> {
    if legacy_mod_entries(root)? { return Ok(true); }
    Ok(!configured_launcher_root(root) && crate::migration::LegacyInstall::detect(root).is_some())
}

/// The files the launcher writes absolute paths into. Relative to the data
/// root; a `*` names every file of a directory.
const REBASED_FILES: &[&str] = &["config/settings.json", "versions/*/prior.json", "stats/queue/*"];

/// Rewrite every absolute path that names `previous` inside the files the
/// launcher writes paths into, so they name `data_root` instead.
///
/// Adoption moves the files but not what is written in them: after it,
/// `settings.json` still said `user_dir` and `game_path` were under the old
/// root, so a launch would have made a fresh, empty user folder at a path
/// that no longer holds anything and looked for a disc that had moved.
/// Both spellings are handled: the JSON-escaped backslash form the launcher
/// writes, and the forward-slash form. Idempotent; returns the files changed.
pub fn rebase_paths(previous: &Path, data_root: &Path) -> Result<Vec<PathBuf>> {
    let old = previous.to_string_lossy().to_string();
    let new = data_root.to_string_lossy().to_string();
    if old.is_empty() || old == new {
        return Ok(Vec::new());
    }
    let pairs = [
        (old.replace('\\', "\\\\"), new.replace('\\', "\\\\")),
        (old.replace('\\', "/"), new.replace('\\', "/")),
        (old.clone(), new.clone()),
    ];
    let mut changed = Vec::new();
    for pattern in REBASED_FILES {
        let mut files = Vec::new();
        let parts: Vec<&str> = pattern.split('/').collect();
        collect_matching(data_root, &parts, &mut files);
        for file in files {
            let Ok(text) = std::fs::read_to_string(&file) else { continue };
            let mut out = text.clone();
            for (from, to) in &pairs {
                if !from.is_empty() {
                    out = out.replace(from.as_str(), to.as_str());
                }
            }
            if out != text {
                std::fs::write(&file, out).map_err(|source| Error::Io { path: file.clone(), source })?;
                changed.push(file);
            }
        }
    }
    Ok(changed)
}

/// Expand a pattern like `versions/*/prior.json` under `root`, one `*` per
/// segment at most, into the files that exist.
fn collect_matching(dir: &Path, parts: &[&str], out: &mut Vec<PathBuf>) {
    let Some((head, rest)) = parts.split_first() else { return };
    if *head == "*" {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            if rest.is_empty() {
                if path.is_file() {
                    out.push(path);
                }
            } else if path.is_dir() {
                collect_matching(&path, rest, out);
            }
        }
    } else {
        let path = dir.join(head);
        if rest.is_empty() {
            if path.is_file() {
                out.push(path);
            }
        } else if path.is_dir() {
            collect_matching(&path, rest, out);
        }
    }
}

/// `<data root>/Reborn 2.1`, when a 2.1 install sits there.
pub fn legacy_root(data_root: &Path) -> Option<PathBuf> {
    let candidate = data_root.join(LEGACY_SUBFOLDER);
    crate::migration::LegacyInstall::detect(&candidate).map(|_| candidate)
}

/// Move every top-level entry that is not part of the 3.0 layout into
/// `Reborn 2.1`. Entries that cannot be moved are reported, not fatal.
pub fn tuck_legacy(data_root: &Path) -> Result<Moves> {
    let keep = top_level_names();
    let into = data_root.join(LEGACY_SUBFOLDER);
    std::fs::create_dir_all(&into).map_err(|source| Error::Io { path: into.clone(), source })?;
    let mut moved = Vec::new();
    let mut blocked = Vec::new();
    let entries = std::fs::read_dir(data_root).map_err(|source| Error::Io {
        path: data_root.to_path_buf(),
        source,
    })?;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let lower = name.to_string_lossy().to_lowercase();
        if lower == "mods" {
            // JSGME's `MODS` is the launcher's `mods` on a case-insensitive
            // disk. Its own entries (`!BACKUP`, `!INSTLOGS`, one folder per
            // mod) go aside; the launcher's (`packages`, `staging`,
            // `activation.json`), if any are there already, stay.
            let (m, b) = tuck_jsgme_mods(&entry.path(), &into.join("MODS"))?;
            moved.extend(m);
            blocked.extend(b);
            continue;
        }
        if keep.contains(&lower) {
            continue;
        }
        let from = entry.path();
        let to = into.join(&name);
        if to.exists() {
            blocked.push((from, format!("{} already holds an entry of that name", into.display())));
            continue;
        }
        match std::fs::rename(&from, &to) {
            Ok(()) => moved.push(to),
            Err(e) => blocked.push((from, e.to_string())),
        }
    }
    Ok((moved, blocked))
}

/// What the launcher itself keeps directly under `mods`.
const LAUNCHER_MODS_ENTRIES: &[&str] = &["packages", "staging", "active", "activation.json"];

/// Move JSGME's entries out of a `MODS` folder into `into`, leaving the
/// launcher's own entries. A folder with nothing left is removed, so the
/// launcher recreates `mods` in its own spelling.
fn tuck_jsgme_mods(mods: &Path, into: &Path) -> Result<Moves> {
    let mut moved = Vec::new();
    let mut blocked = Vec::new();
    let entries = std::fs::read_dir(mods).map_err(|source| Error::Io { path: mods.to_path_buf(), source })?;
    let mut launcher_entries = false;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let lower = name.to_string_lossy().to_lowercase();
        if LAUNCHER_MODS_ENTRIES.contains(&lower.as_str()) {
            launcher_entries = true;
            continue;
        }
        std::fs::create_dir_all(into).map_err(|source| Error::Io { path: into.to_path_buf(), source })?;
        let from = entry.path();
        let to = into.join(&name);
        if to.exists() {
            blocked.push((from, format!("{} already holds an entry of that name", into.display())));
            continue;
        }
        match std::fs::rename(&from, &to) {
            Ok(()) => moved.push(to),
            Err(e) => blocked.push((from, e.to_string())),
        }
    }
    if !launcher_entries && std::fs::read_dir(mods).map(|mut d| d.next().is_none()).unwrap_or(false) {
        let _ = std::fs::remove_dir(mods);
    }
    Ok((moved, blocked))
}

/// Move the known 3.0 entries of `previous` into `data_root`. The program
/// folder `app` and anything the layout does not know stay behind. An
/// entry already present in `data_root` is skipped. An emptied previous
/// root is removed.
pub fn adopt_previous(previous: &Path, data_root: &Path) -> Result<Vec<PathBuf>> {
    let known = top_level_names();
    let mut adopted = Vec::new();
    std::fs::create_dir_all(data_root).map_err(|source| Error::Io {
        path: data_root.to_path_buf(),
        source,
    })?;
    let entries = std::fs::read_dir(previous).map_err(|source| Error::Io {
        path: previous.to_path_buf(),
        source,
    })?;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let lower = name.to_string_lossy().to_lowercase();
        if lower == "app" || lower == LEGACY_SUBFOLDER.to_lowercase() || !known.contains(&lower) {
            continue;
        }
        let to = data_root.join(&name);
        if to.exists() {
            tracing::warn!("not adopting {}: {} already exists", entry.path().display(), to.display());
            continue;
        }
        match std::fs::rename(entry.path(), &to) {
            Ok(()) => adopted.push(to),
            Err(e) => tracing::warn!("could not adopt {}: {e}", entry.path().display()),
        }
    }
    if std::fs::read_dir(previous).map(|mut d| d.next().is_none()).unwrap_or(false) {
        let _ = std::fs::remove_dir(previous);
    }
    Ok(adopted)
}

/// Whether two paths name the same directory, after canonicalizing when
/// both exist. Public so a caller guessing at a sibling folder can refuse
/// to treat the data root itself as that sibling.
pub fn same_named(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn put(root: &Path, rel: &str) {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, b"x").unwrap();
    }
    fn names(dir: &Path) -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        v.sort();
        v
    }

    #[test]
    fn a_2_1_install_at_the_data_root_is_tucked_into_its_own_subfolder_and_nothing_is_deleted() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("NFL Street Reborn");
        for rel in [
            "User/GC/MemoryCardA.USA.raw",
            "Games/NSR.iso",
            "MODS/!INSTLOGS/x.txt",
            "Tools/reborn-builder-cli.exe",
            "NFL Street Reborn Dolphin.exe",
            "unins000.exe",
            "JSGME.ini",
            "logs/launcher.log",
            "app/reborn-launcher.exe",
        ] {
            put(&root, rel);
        }
        let p = prepare_at(&root, None).unwrap();
        assert!(p.blocked.is_empty(), "{:?}", p.blocked);
        assert_eq!(names(&root), ["Reborn 2.1", "app", "logs"]);
        assert_eq!(
            names(&root.join(LEGACY_SUBFOLDER)),
            ["Games", "JSGME.ini", "MODS", "NFL Street Reborn Dolphin.exe", "Tools", "User", "unins000.exe"]
        );
        assert!(root.join(LEGACY_SUBFOLDER).join("User/GC/MemoryCardA.USA.raw").is_file());
        assert_eq!(p.legacy_root.as_deref(), Some(root.join(LEGACY_SUBFOLDER).as_path()));
        assert!(
            crate::migration::LegacyInstall::detect(&root).is_none(),
            "the root no longer looks like a 2.1 install"
        );
        // idempotent
        let again = prepare_at(&root, None).unwrap();
        assert!(again.tucked.is_empty() && again.adopted.is_empty());
        assert_eq!(names(&root), ["Reborn 2.1", "app", "logs"]);
    }

    #[test]
    fn a_data_root_that_is_not_a_2_1_install_is_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("NFL Street Reborn");
        put(&root, "config/settings.json");
        put(&root, "somebody-elses-folder/a.txt");
        let p = prepare_at(&root, None).unwrap();
        assert!(p.tucked.is_empty());
        assert_eq!(names(&root), ["config", "somebody-elses-folder"]);
    }

    #[test]
    fn configured_launcher_keeps_active_user_and_games_even_when_old_archive_has_the_same_names() {
        let dir=tempfile::tempdir().unwrap(); let root=dir.path();
        put(root,"User/GC/card.raw"); put(root,"Games/NSR.iso");
        put(root,"Reborn 2.1/User/GC/card.raw"); put(root,"Reborn 2.1/Games/NSR.iso");
        let mut settings=crate::settings::Settings::default();
        settings.user_dir=Some(root.join("User"));
        settings.game_path=Some(root.join("Games/NSR.iso"));
        settings.save(&root.join("config/settings.json")).unwrap();
        let before=std::fs::read(root.join("config/settings.json")).unwrap();
        for _ in 0..2 {
            let prepared=prepare_at(root,None).unwrap();
            assert!(prepared.tucked.is_empty() && prepared.blocked.is_empty());
            assert!(!legacy_blocks_launcher(root).unwrap());
            assert!(root.join("User/GC/card.raw").is_file());
            assert!(root.join("Games/NSR.iso").is_file());
            assert!(root.join("Reborn 2.1/User/GC/card.raw").is_file());
            assert_eq!(std::fs::read(root.join("config/settings.json")).unwrap(),before);
        }
    }

    #[test]
    fn actual_mod_namespace_conflicts_still_require_separation_in_a_configured_launcher() {
        let dir=tempfile::tempdir().unwrap();let root=dir.path();
        crate::settings::Settings::default().save(&root.join("config/settings.json")).unwrap();
        put(root,"User/GC/card.raw"); put(root,"MODS/!BACKUP/original.dds");
        put(root,"MODS/packages/current/package.json");
        assert!(legacy_blocks_launcher(root).unwrap());
        let prepared=prepare_at(root,None).unwrap();
        assert!(prepared.blocked.is_empty());
        assert!(!legacy_blocks_launcher(root).unwrap());
        assert!(root.join("MODS/packages/current/package.json").is_file());
        assert!(root.join("Reborn 2.1/MODS/!BACKUP/original.dds").is_file());
        assert!(root.join("User/GC/card.raw").is_file());
    }

    #[test]
    fn a_mod_backup_collision_preserves_both_copies_and_remains_protected() {
        let dir=tempfile::tempdir().unwrap();let root=dir.path();
        crate::settings::Settings::default().save(&root.join("config/settings.json")).unwrap();
        put(root,"MODS/!BACKUP/new-original.dds");
        put(root,"Reborn 2.1/MODS/!BACKUP/old-original.dds");
        assert!(!prepare_at(root,None).unwrap().blocked.is_empty());
        assert!(legacy_blocks_launcher(root).unwrap());
        assert!(root.join("MODS/!BACKUP/new-original.dds").is_file());
        assert!(root.join("Reborn 2.1/MODS/!BACKUP/old-original.dds").is_file());
    }

    #[test]
    fn a_launcher_era_root_is_adopted_by_rename_and_unknown_entries_stay() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("NFL Street Reborn");
        let previous = dir.path().join("NFL Street Reborn Launcher");
        put(&root, "app/reborn-launcher.exe");
        for rel in [
            "config/settings.json",
            "library/textures/GN7E69/a.dds",
            "dolphin-users/fresh-1/GC/MemoryCardA.USA.raw",
            "mods/packages/p/1.0.0/package.json",
            "journal.json",
            "publisher/app/x.exe",
            "app/old.exe",
        ] {
            put(&previous, rel);
        }
        let p = prepare_at(&root, Some(&previous)).unwrap();
        assert_eq!(names(&root), ["app", "config", "dolphin-users", "journal.json", "library", "mods"]);
        assert!(root.join("dolphin-users/fresh-1/GC/MemoryCardA.USA.raw").is_file());
        assert!(root.join("app/reborn-launcher.exe").is_file(), "the new program folder is never touched");
        assert_eq!(names(&previous), ["app", "publisher"], "unknown entries and the old program folder stay behind");
        assert_eq!(p.adopted.len(), 5);
    }

    #[test]
    fn adoption_never_overwrites_and_only_happens_into_a_root_with_no_settings() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("NFL Street Reborn");
        let previous = dir.path().join("NFL Street Reborn Launcher");
        put(&previous, "config/settings.json");
        put(&previous, "library/a");
        put(&root, "library/b");
        let p = prepare_at(&root, Some(&previous)).unwrap();
        assert_eq!(names(&root), ["config", "library"]);
        assert!(
            root.join("library/b").is_file() && !root.join("library/a").is_file(),
            "an existing entry is skipped, not merged"
        );
        assert!(previous.join("library/a").is_file());
        assert_eq!(p.adopted.len(), 1);
        // a root that already has settings adopts nothing
        put(&previous, "stats/queue/x");
        let again = prepare_at(&root, Some(&previous)).unwrap();
        assert!(again.adopted.is_empty());
        assert!(previous.join("stats/queue/x").is_file());
    }

    #[test]
    fn tuck_runs_before_adopt_so_mods_never_lands_inside_jsgme_mods() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("NFL Street Reborn");
        let previous = dir.path().join("NFL Street Reborn Launcher");
        put(&root, "User/GC/MemoryCardA.USA.raw");
        put(&root, "MODS/!BACKUP/x");
        put(&previous, "config/settings.json");
        put(&previous, "mods/packages/p/1.0.0/package.json");
        prepare_at(&root, Some(&previous)).unwrap();
        assert!(root.join("mods/packages/p/1.0.0/package.json").is_file());
        assert!(root.join(LEGACY_SUBFOLDER).join("MODS/!BACKUP/x").is_file());
    }

    #[test]
    fn a_mods_folder_holding_both_jsgme_and_launcher_entries_keeps_the_launchers() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("NFL Street Reborn");
        put(&root, "User/GC/MemoryCardA.USA.raw");
        put(&root, "MODS/!INSTLOGS/x.txt");
        put(&root, "MODS/Aqua Football/GN7E69/ball.dds");
        put(&root, "MODS/packages/p/1.0.0/package.json");
        put(&root, "MODS/activation.json");
        let p = prepare_at(&root, None).unwrap();
        assert!(p.blocked.is_empty(), "{:?}", p.blocked);
        assert_eq!(names(&root.join(LEGACY_SUBFOLDER).join("MODS")), ["!INSTLOGS", "Aqua Football"]);
        assert_eq!(names(&root.join("MODS")), ["activation.json", "packages"]);
        assert!(root.join("MODS/packages/p/1.0.0/package.json").is_file());
    }

    /// Adoption moves the files; this moves what is written in them. A root
    /// adopted before this step existed is fixed at any later start too.
    #[test]
    fn paths_written_under_the_previous_root_are_rebased_in_both_spellings_every_start() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("NFL Street Reborn Beta");
        let previous = dir.path().join("NFL Street Reborn Launcher Beta");
        let old_json = previous.to_string_lossy().replace('\\', "\\\\");
        let old_fwd = previous.to_string_lossy().replace('\\', "/");
        std::fs::create_dir_all(root.join("config")).unwrap();
        std::fs::create_dir_all(root.join("versions").join("runtime")).unwrap();
        std::fs::create_dir_all(root.join("stats").join("queue")).unwrap();
        std::fs::write(
            root.join("config").join("settings.json"),
            format!(r#"{{"schema":"nsr.settings.v1","user_dir":"{old_json}\\dolphin-users\\fresh-1","game_path":"{old_fwd}/library/offline/NSR.iso","legacy_root":"D:\\Elsewhere\\NFL Street Reborn"}}"#),
        )
        .unwrap();
        std::fs::write(root.join("versions").join("runtime").join("prior.json"), format!(r#"{{"prior":"{old_json}\\versions\\runtime\\prior"}}"#)).unwrap();
        std::fs::write(root.join("stats").join("queue").join("a.entry.json"), format!(r#"{{"payload":"{old_json}\\stats\\games\\a.json"}}"#)).unwrap();
        // no previous root on disk any more: the rebase still runs
        let p = prepare_at(&root, Some(&previous)).unwrap();
        assert_eq!(p.rebased.len(), 3, "{:?}", p.rebased);
        let text = std::fs::read_to_string(root.join("config").join("settings.json")).unwrap();
        let new_json = root.to_string_lossy().replace('\\', "\\\\");
        let new_fwd = root.to_string_lossy().replace('\\', "/");
        assert!(text.contains(&format!("{new_json}\\\\dolphin-users")), "{text}");
        assert!(text.contains(&format!("{new_fwd}/library/offline/NSR.iso")), "{text}");
        assert!(text.contains("D:\\\\Elsewhere"), "a path elsewhere is untouched: {text}");
        assert!(!text.contains("Launcher Beta"), "{text}");
        assert!(std::fs::read_to_string(root.join("stats").join("queue").join("a.entry.json")).unwrap().contains(&new_json));
        // idempotent
        let again = prepare_at(&root, Some(&previous)).unwrap();
        assert!(again.rebased.is_empty());
    }

    #[test]
    fn the_top_level_names_come_from_the_layout_and_include_the_program_folder() {
        let names = top_level_names();
        for n in [
            "library",
            "runtime",
            "mods",
            "saves",
            "config",
            "cache",
            "logs",
            "dolphin-users",
            "app",
            "journal.json",
            "reborn 2.1",
        ] {
            assert!(names.contains(n), "{n}");
        }
        assert!(!names.contains("user") && !names.contains("games"));
    }
}
