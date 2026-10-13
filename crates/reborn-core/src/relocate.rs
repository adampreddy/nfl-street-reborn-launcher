//! "Move to another drive": the data root (game library, textures, Dolphin's
//! folders, saves) goes to a folder the player picks, and a folder link
//! (a junction on Windows) is left at the old path.
//!
//! Why a link and not a new setting. The data root is fixed under the
//! player's profile (`%APPDATA%\NFL Street Reborn`), and the launcher writes
//! that path into its own files: `settings.json` (`user_dir`, `game_path`),
//! `versions/*/prior.json`, the stats queue. A setting would have to rewrite
//! every one of them, and the 2026-09-12 adoption showed what is missed when
//! that is tried. With a link every one of those paths keeps working, and
//! nothing else in the launcher has to know the files moved.
//!
//! Two halves, because the launcher usually runs from INSIDE the data root
//! (`<root>\app`) and Windows will not rename a folder holding a running
//! program:
//!
//! 1. [`copy`], in the running launcher, with progress: the whole tree into
//!    `<target>.moving` beside the target. Hard links stay hard links (the
//!    textures are one file shared by the library, the payload and the
//!    composed tree, and copying each name would double the size).
//! 2. [`finish`], in a helper started after the launcher closes: copy what
//!    changed since, put the old folder aside, rename the copy into place,
//!    make the link, and delete the old folder. Every step up to the link is
//!    undone if a later one fails, and the old folder is deleted only once
//!    the link leads to the new one.

use crate::platform::dir_move::{self, is_dir_link, make_dir_link, plain, real, remove_dir_link};
use crate::platform::file_identity::{self, FileId};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// The copy's folder name, beside the target, until it is renamed into place.
pub const STAGED_SUFFIX: &str = ".moving";
/// Inside a copy in progress, so a leftover one is known to be ours.
const STAGED_MARKER: &str = ".reborn-moving";
/// Inside the old folder once it is put aside, so a leftover one is known to
/// be ours to delete.
const RETIRED_MARKER: &str = ".reborn-moved";
/// Left free on the new drive beyond the copy itself.
const MARGIN_BYTES: u64 = 1 << 30;
/// Where the outcome is written, inside the data root.
pub const RESULT_FILE: &str = "config/relocation.json";

/// One move, decided and measured.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plan {
    /// The data root as the launcher names it. Becomes the link.
    pub link: PathBuf,
    /// Where its files really are now (the same as `link` unless it was
    /// moved before).
    pub real: PathBuf,
    /// Where they will be.
    pub target: PathBuf,
    /// The copy, beside the target, until it is renamed into place.
    pub staged: PathBuf,
    pub bytes: u64,
    pub files: u64,
}

/// What the last move did, kept in [`RESULT_FILE`] for Settings to show.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Outcome {
    pub moved: bool,
    pub message: String,
    pub from: PathBuf,
    pub to: PathBuf,
    pub at_ms: u64,
}

/// Where the data root's files really are, and whether that is somewhere
/// else than its own path (it was moved).
pub fn location(data_root: &Path) -> (PathBuf, bool) {
    let at = real(data_root);
    let moved = is_dir_link(data_root);
    (at, moved)
}

fn same_path(a: &Path, b: &Path) -> bool {
    let n = |p: &Path| plain(p).to_string_lossy().replace('/', "\\").trim_end_matches('\\').to_lowercase();
    n(a) == n(b)
}

fn gib(bytes: u64) -> String {
    format!("{:.1} GB", bytes as f64 / 1_073_741_824.0)
}

/// Decide where the data root goes when the player picks `chosen` (any
/// folder on the drive they want), and refuse, in words a player can act
/// on, anything that would not work or would not be safe.
///
/// The target is always `<drive root>\<data root's name>`, the same shape
/// as the folder under `%APPDATA%`, whatever folder was picked on that
/// drive. A folder deeper down is only as safe as every folder above it: on
/// a second drive a folder like `D:\Games` can be renamed by any account on
/// the PC, which could then put its own `NFL Street Reborn` in its place,
/// and the junction would lead there. The drive root cannot be renamed, and
/// it is checked to make sure no other account can delete what is in it or
/// take it over (security review, 2026-10-09).
pub fn plan(data_root: &Path, chosen: &Path) -> Result<Plan, String> {
    if !data_root.is_dir() {
        return Err("There is nothing to move yet. Set up the game first.".into());
    }
    if !chosen.is_absolute() || !chosen.is_dir() {
        return Err(format!("{} is not a folder on this PC.", chosen.display()));
    }
    let name = data_root.file_name().ok_or("The game folder has no name.")?;
    let real_root = real(data_root);
    let here = file_identity::of(&real_root).map_err(|e| format!("The game folder could not be read: {e}"))?;
    let there = file_identity::of_nearest(chosen).map_err(|e| format!("{} could not be read: {e}", chosen.display()))?;
    if here.volume == there.volume {
        return Err("The game is already on that drive. Choose a different drive.".into());
    }
    let facts = dir_move::volume_facts(chosen).ok_or("That drive could not be read.")?;
    #[cfg(not(windows))]
    let (target, staged) = unix_targets(data_root, chosen, name, &facts)?;
    #[cfg(windows)]
    let (target, staged) = windows_targets(name, &facts)?;
    // Never a folder that is already there: one this move did not make
    // could belong to another account. The one exception is this account's
    // own unfinished copy (the launcher closed during an earlier move),
    // which `copy` clears and starts again. Moving back to where the data
    // root started (Linux) puts the files where its link is now.
    if std::fs::symlink_metadata(&target).is_ok() && !(same_path(&target, data_root) && is_dir_link(data_root)) {
        return Err(format!("{} already exists. Rename or remove it, then try again.", target.display()));
    }
    if std::fs::symlink_metadata(&staged).is_ok() {
        let ours = !is_dir_link(&staged) && staged.join(STAGED_MARKER).is_file() && dir_move::owned_by_me(&staged).unwrap_or(false);
        if !ours {
            return Err(format!("{} already exists. Rename or remove it, then try again.", staged.display()));
        }
    }
    let (bytes, files) = measure(&real_root).map_err(|e| format!("The game folder could not be measured: {e}"))?;
    if facts.free_bytes < bytes + MARGIN_BYTES {
        return Err(format!(
            "{} has {} free. Moving the game needs {} (its size plus 1 GB to spare).",
            facts.mount.display(),
            gib(facts.free_bytes),
            gib(bytes + MARGIN_BYTES)
        ));
    }
    Ok(Plan { link: data_root.to_path_buf(), real: real_root, target, staged, bytes, files })
}

/// Windows: `<drive root>\<name>`, on a fixed NTFS drive with its own letter
/// whose top folder no other account can change.
#[cfg(windows)]
fn windows_targets(name: &std::ffi::OsStr, facts: &dir_move::VolumeFacts) -> Result<(PathBuf, PathBuf), String> {
    if facts.removable {
        return Err(format!("{} is a removable drive. The game has to stay on a drive that is always connected.", facts.mount.display()));
    }
    // The launcher keeps the textures as hard links (one file, several names),
    // and only NTFS and ReFS have them; exFAT and FAT32 drives do not.
    let fs = facts.filesystem.to_ascii_uppercase();
    if cfg!(windows) && fs != "NTFS" && fs != "REFS" {
        return Err(format!("{} is formatted as {}. The game needs an NTFS drive.", facts.mount.display(), facts.filesystem));
    }
    // A drive with its own letter only. A volume mounted onto a folder
    // (C:\Mounts\Games) has a "root" with folders above it, and any of
    // those that another account can rename reopens the swap this placement
    // exists to prevent (security review, 2026-10-09).
    if facts.mount.parent().is_some() {
        return Err(format!(
            "{} is a drive mounted inside a folder. Choose a drive with its own letter.",
            facts.mount.display()
        ));
    }
    match dir_move::others_can_replace_children(&facts.mount) {
        Ok(false) => {}
        Ok(true) => {
            return Err(format!(
                "Other accounts on this PC can change the top folder of {}, so the game's files would not be safe there. Choose another drive.",
                facts.mount.display()
            ))
        }
        Err(e) => return Err(format!("The permissions of {} could not be checked: {e}", facts.mount.display())),
    }
    Ok(targets_on(&facts.mount, name))
}

/// Linux (the Steam Deck's SD card) and macOS: `<chosen>/<name>`, or the
/// data root's own path when the player picks the drive it started on.
///
/// Not the top of the drive, as on Windows: on Linux every drive is
/// mounted inside a folder (`/run/media/deck/<card>`), the internal one's
/// top is `/` or `/home` and not the player's to write, and an SD card is
/// removable by design. What the Windows rule protects against, another
/// account renaming a folder above the game, is checked on every folder
/// above it instead, on the real path: the picked folder is resolved first,
/// so no link in it can be pointed elsewhere after the check, and every
/// folder above the target must belong to this account or root and be
/// writable by no one else (security review, 2026-10-12). Nothing then can
/// change between this check and the copy but this account itself.
#[cfg(not(windows))]
fn unix_targets(data_root: &Path, chosen: &Path, name: &std::ffi::OsStr, facts: &dir_move::VolumeFacts) -> Result<(PathBuf, PathBuf), String> {
    // The textures are hard links and Dolphin is a program run from inside
    // the folder: FAT and exFAT have neither links nor permissions, and an
    // NTFS card through FUSE is unreliable for both.
    const FILESYSTEMS: &[&str] = &["ext4", "ext3", "ext2", "btrfs", "xfs", "f2fs", "bcachefs", "zfs", "apfs", "hfs"];
    let fs = facts.filesystem.to_ascii_lowercase();
    if !FILESYSTEMS.contains(&fs.as_str()) {
        return Err(format!(
            "{} is formatted as {}, which cannot hold the game. On a Steam Deck, Settings > System > Format SD Card formats it the way the game needs (this erases the card).",
            facts.mount.display(),
            facts.filesystem
        ));
    }
    let home_drive = data_root
        .parent()
        .and_then(|p| file_identity::of(p).ok())
        .zip(file_identity::of_nearest(chosen).ok())
        .is_some_and(|(home, there)| home.volume == there.volume);
    let chosen = std::fs::canonicalize(chosen).map_err(|e| format!("{} could not be read: {e}", chosen.display()))?;
    let target = if home_drive { data_root.to_path_buf() } else { chosen.join(name) };
    let mut staged_name = name.to_os_string();
    staged_name.push(STAGED_SUFFIX);
    let staged = target.with_file_name(staged_name);
    let mut above = target.parent();
    while let Some(dir) = above {
        match dir_move::only_me_or_root_can_change(dir) {
            Ok(true) => {}
            Ok(false) => {
                return Err(format!(
                    "Other accounts can change {}, so the game's files would not be safe there. Choose another folder.",
                    dir.display()
                ))
            }
            Err(e) => return Err(format!("The permissions of {} could not be checked: {e}", dir.display())),
        }
        above = dir.parent();
    }
    Ok((target, staged))
}

/// The new folder and its copy in progress, at the top of the drive
/// mounted at `mount`.
pub fn targets_on(mount: &Path, name: &std::ffi::OsStr) -> (PathBuf, PathBuf) {
    let mut staged_name = name.to_os_string();
    staged_name.push(STAGED_SUFFIX);
    (mount.join(name), mount.join(staged_name))
}

enum Item {
    Dir(PathBuf),
    File(PathBuf, u64),
    Link(PathBuf),
}

/// Every entry under `root`, relative, folders before what is in them.
/// Folder links are listed, never followed.
fn walk(root: &Path) -> std::io::Result<Vec<Item>> {
    let mut out = Vec::new();
    let mut pending = vec![PathBuf::new()];
    while let Some(rel) = pending.pop() {
        for entry in std::fs::read_dir(root.join(&rel))? {
            let entry = entry?;
            let child = rel.join(entry.file_name());
            let path = entry.path();
            if is_dir_link(&path) {
                out.push(Item::Link(child));
                continue;
            }
            let meta = std::fs::metadata(&path)?;
            if meta.is_dir() {
                out.push(Item::Dir(child.clone()));
                pending.push(child);
            } else {
                out.push(Item::File(child, meta.len()));
            }
        }
    }
    Ok(out)
}

/// Bytes and files under `root`, each hard-linked file counted once.
pub fn measure(root: &Path) -> std::io::Result<(u64, u64)> {
    let mut seen = BTreeSet::new();
    let (mut bytes, mut files) = (0u64, 0u64);
    for item in walk(root)? {
        if let Item::File(rel, len) = item {
            files += 1;
            match file_identity::of(&root.join(&rel)) {
                Ok(id) if id.links > 1 => {
                    if seen.insert((id.volume, id.index)) {
                        bytes += len;
                    }
                }
                _ => bytes += len,
            }
        }
    }
    Ok((bytes, files))
}

/// Copy the data root into `plan.staged`. `progress` hears bytes done and
/// the total; `cancel` stops it between files. On any failure, and on a
/// cancel, the partial copy is removed.
pub fn copy(plan: &Plan, progress: &dyn Fn(u64, u64), cancel: &AtomicBool) -> Result<(), String> {
    let result = copy_inner(plan, progress, cancel);
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&plan.staged);
    }
    result
}

fn copy_inner(plan: &Plan, progress: &dyn Fn(u64, u64), cancel: &AtomicBool) -> Result<(), String> {
    if plan.staged.exists() {
        if plan.staged.join(STAGED_MARKER).is_file() {
            std::fs::remove_dir_all(&plan.staged).map_err(|e| format!("An earlier unfinished copy at {} could not be removed: {e}", plan.staged.display()))?;
        } else {
            return Err(format!("{} is in the way. Move or rename it and try again.", plan.staged.display()));
        }
    }
    if let Some(parent) = plan.staged.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{} could not be made: {e}", parent.display()))?;
    }
    std::fs::create_dir(&plan.staged).map_err(|e| format!("{} could not be made: {e}", plan.staged.display()))?;
    // Before a single file goes in: the launcher and Dolphin are part of
    // what moves, and a second drive's folders are writable by every account
    // on the PC. Refused rather than moved into a folder others can change.
    dir_move::make_private(&plan.staged).map_err(|e| format!("The new folder could not be made private to your account, so nothing was moved: {e}"))?;
    std::fs::write(plan.staged.join(STAGED_MARKER), b"").map_err(|e| format!("The new folder could not be written: {e}"))?;
    let items = walk(&plan.real).map_err(|e| format!("The game folder could not be read: {e}"))?;
    let mut linked: HashMap<(u64, u64), PathBuf> = HashMap::new();
    let mut done = 0u64;
    progress(0, plan.bytes);
    for item in items {
        if cancel.load(Ordering::SeqCst) {
            return Err("The move was cancelled. Nothing was changed.".into());
        }
        match item {
            Item::Dir(rel) => {
                let to = plan.staged.join(&rel);
                std::fs::create_dir_all(&to).map_err(|e| format!("{} could not be made: {e}", to.display()))?;
            }
            Item::File(rel, len) => {
                let (from, to) = (plan.real.join(&rel), plan.staged.join(&rel));
                if let Some(parent) = to.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| format!("{} could not be made: {e}", parent.display()))?;
                }
                let id: Option<FileId> = file_identity::of(&from).ok().filter(|id| id.links > 1);
                if let Some(first) = id.and_then(|id| linked.get(&(id.volume, id.index))) {
                    std::fs::hard_link(first, &to).map_err(|e| format!("{} could not be linked: {e}", to.display()))?;
                    continue;
                }
                std::fs::copy(&from, &to).map_err(|e| format!("{} could not be copied: {e}", from.display()))?;
                if let Some(id) = id {
                    linked.insert((id.volume, id.index), to);
                }
                done += len;
                progress(done.min(plan.bytes), plan.bytes);
            }
            Item::Link(rel) => {
                // A link inside the data root (an old build's texture
                // junction) is made again with the same target. A target
                // inside the old root still resolves once the root is a link.
                let (from, to) = (plan.real.join(&rel), plan.staged.join(&rel));
                match std::fs::read_link(&from) {
                    Ok(target) if target.is_dir() => {
                        make_dir_link(&to, &target).map_err(|e| format!("{} could not be linked: {e}", to.display()))?
                    }
                    _ => tracing::warn!(link = %from.display(), "A link in the game folder was not copied: it leads nowhere usable"),
                }
            }
        }
    }
    progress(plan.bytes, plan.bytes);
    Ok(())
}

/// Make `staged` match `real` again: what changed or appeared since the
/// copy is copied, what went away is removed. Small by design: it runs once
/// the launcher has closed, seconds after the copy.
fn resync(real_root: &Path, staged: &Path) -> std::io::Result<()> {
    let mut present = BTreeSet::new();
    for item in walk(real_root)? {
        match item {
            Item::Dir(rel) => {
                std::fs::create_dir_all(staged.join(&rel))?;
                present.insert(rel);
            }
            Item::File(rel, len) => {
                let (from, to) = (real_root.join(&rel), staged.join(&rel));
                let same = std::fs::metadata(&to).ok().is_some_and(|m| {
                    m.len() == len && m.modified().ok() == std::fs::metadata(&from).ok().and_then(|f| f.modified().ok())
                });
                if !same {
                    if let Some(parent) = to.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    std::fs::copy(&from, &to)?;
                }
                present.insert(rel);
            }
            Item::Link(rel) => {
                present.insert(rel);
            }
        }
    }
    // Deepest first, so a folder is emptied before it is removed.
    let mut extra: Vec<(PathBuf, bool)> = walk(staged)?
        .into_iter()
        .filter_map(|item| match item {
            Item::Dir(rel) => Some((rel, true)),
            Item::File(rel, _) => Some((rel, false)),
            Item::Link(_) => None,
        })
        .filter(|(rel, _)| !present.contains(rel) && rel.as_os_str() != STAGED_MARKER)
        .collect();
    extra.sort_by_key(|(rel, _)| std::cmp::Reverse(rel.components().count()));
    for (rel, is_dir) in extra {
        let path = staged.join(rel);
        let _ = if is_dir { std::fs::remove_dir_all(&path) } else { std::fs::remove_file(&path) };
    }
    Ok(())
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// Rename with a few retries: a virus scanner or the search indexer can
/// hold a file in a folder for a moment after the launcher closes.
fn rename_patiently(from: &Path, to: &Path, tries: u32) -> std::io::Result<()> {
    let mut last = None;
    for attempt in 0..tries {
        match std::fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(e) => last = Some(e),
        }
        if attempt + 1 < tries {
            std::thread::sleep(Duration::from_millis(1000));
        }
    }
    Err(last.unwrap_or_else(|| std::io::Error::other("rename failed")))
}

/// The second half, run by the helper once the launcher has closed: wait for
/// `launcher_pid` to exit, then copy what changed, swap the folders, make the
/// link. Returns the outcome (also written to [`RESULT_FILE`]) and, when the
/// move went through, the old folder to delete. The caller relaunches the
/// launcher before deleting it, so the player is not kept waiting.
pub fn finish(plan: &Plan, launcher_pid: Option<u32>) -> (Outcome, Option<PathBuf>) {
    // A plan with an empty or relative path is not one `plan` made, and
    // nothing is touched or written for it (a relative path would land in
    // whatever folder the helper happened to start in).
    if ![&plan.link, &plan.real, &plan.target, &plan.staged].iter().all(|p| p.is_absolute() && p.file_name().is_some()) {
        let outcome = Outcome {
            moved: false,
            message: "The move could not be finished: its plan was not readable. Nothing was changed.".into(),
            from: plan.real.clone(),
            to: plan.target.clone(),
            at_ms: now_ms(),
        };
        return (outcome, None);
    }
    let started = Instant::now();
    if let Some(pid) = launcher_pid {
        while dir_move::pid_alive(pid) && started.elapsed() < Duration::from_secs(120) {
            std::thread::sleep(Duration::from_millis(250));
        }
    }
    let result = swap(plan);
    let outcome = Outcome {
        moved: result.is_ok(),
        message: match &result {
            Ok(_) => format!("Moved to {}.", plan.target.display()),
            Err(why) => why.clone(),
        },
        from: plan.real.clone(),
        to: plan.target.clone(),
        at_ms: now_ms(),
    };
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&plan.staged);
    }
    write_outcome(&plan.link, &outcome);
    (outcome, result.ok())
}

fn swap(plan: &Plan) -> Result<PathBuf, String> {
    let busy = "Something on this PC is still using the game folder (the game, a File Explorer window, or another program), so nothing was moved. Close it and try again.";
    resync(&plan.real, &plan.staged).map_err(|e| format!("The last changes could not be copied, so nothing was moved: {e}"))?;
    let mut retired_name = plan.real.file_name().unwrap_or_default().to_os_string();
    retired_name.push(format!(".moved-{}", now_ms()));
    let retired = plan.real.with_file_name(retired_name);
    // 1. The old folder aside. Fails while anything holds a file in it, and
    //    then nothing has changed yet.
    rename_patiently(&plan.real, &retired, 10).map_err(|e| {
        tracing::warn!(%e, "The old game folder could not be put aside");
        busy.to_string()
    })?;
    let undo_1 = || {
        let _ = std::fs::rename(&retired, &plan.real);
    };
    // 2. A link from an earlier move is removed (the folder it led to was
    //    just put aside). Removing a link removes no files.
    let was_link = !same_path(&plan.link, &plan.real);
    if was_link {
        if let Err(e) = remove_dir_link(&plan.link) {
            undo_1();
            return Err(format!("The old folder link at {} could not be removed, so nothing was moved: {e}", plan.link.display()));
        }
    }
    let undo_2 = || {
        if was_link {
            let _ = make_dir_link(&plan.link, &plan.real);
        }
    };
    // 3. The copy into place.
    if let Err(e) = rename_patiently(&plan.staged, &plan.target, 5) {
        undo_2();
        undo_1();
        return Err(format!("The copy could not be named {}, so nothing was moved: {e}", plan.target.display()));
    }
    // 4. The link at the old path, unless the files went back to it.
    let back_home = same_path(&plan.link, &plan.target);
    if let Err(e) = if back_home { Ok(()) } else { make_dir_link(&plan.link, &plan.target) } {
        let _ = remove_dir_link(&plan.link);
        let _ = std::fs::rename(&plan.target, &plan.staged);
        undo_2();
        undo_1();
        return Err(format!("The folder link could not be made, so nothing was moved: {e}"));
    }
    let _ = std::fs::remove_file(plan.target.join(STAGED_MARKER));
    let _ = std::fs::write(retired.join(RETIRED_MARKER), b"");
    Ok(retired)
}

fn write_outcome(data_root: &Path, outcome: &Outcome) {
    let path = data_root.join(RESULT_FILE);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(text) = serde_json::to_string_pretty(outcome) {
        let _ = std::fs::write(path, text);
    }
}

/// The last move's outcome, if there was one.
pub fn last_outcome(data_root: &Path) -> Option<Outcome> {
    serde_json::from_slice(&std::fs::read(data_root.join(RESULT_FILE)).ok()?).ok()
}

/// Old folders a finished move put aside and could not delete at the time
/// (a file was still held). Ours only: each carries [`RETIRED_MARKER`].
pub fn leftovers(data_root: &Path) -> Vec<PathBuf> {
    let Some(name) = data_root.file_name().map(|n| n.to_string_lossy().to_lowercase()) else { return Vec::new() };
    let prefix = format!("{name}.moved-");
    let mut found = Vec::new();
    let mut parents: Vec<PathBuf> = data_root.parent().map(Path::to_path_buf).into_iter().collect();
    if let Some(p) = real(data_root).parent() {
        if !parents.iter().any(|q| same_path(q, p)) {
            parents.push(p.to_path_buf());
        }
    }
    for parent in parents {
        let Ok(entries) = std::fs::read_dir(&parent) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            let n = entry.file_name().to_string_lossy().to_lowercase();
            if n.starts_with(&prefix) && !is_dir_link(&path) && path.join(RETIRED_MARKER).is_file() {
                found.push(path);
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put(root: &Path, rel: &str, body: &[u8]) {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
    }

    /// A plan between two folders of one temp volume: `plan` itself refuses
    /// that (same drive), and everything after it does not care.
    fn same_volume_plan(base: &Path) -> Plan {
        let link = base.join("Roaming").join("NFL Street Reborn");
        let target = base.join("D").join("NFL Street Reborn");
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        let (bytes, files) = measure(&link).unwrap();
        Plan { real: real(&link), staged: target.with_file_name("NFL Street Reborn.moving"), link, target, bytes, files }
    }

    fn sample(base: &Path) -> PathBuf {
        let root = base.join("Roaming").join("NFL Street Reborn");
        put(&root, "library/textures/a.png", &[7u8; 4000]);
        std::fs::create_dir_all(root.join("app/content/textures")).unwrap();
        std::fs::hard_link(root.join("library/textures/a.png"), root.join("app/content/textures/a.png")).unwrap();
        put(&root, "config/settings.json", br#"{"user_dir":"x"}"#);
        put(&root, "dolphin-users/User/GC/MemoryCardA.USA.raw", &[1u8; 100]);
        std::fs::create_dir_all(root.join("empty/folder")).unwrap();
        root
    }

    #[test]
    fn a_hard_linked_file_is_measured_once() {
        let dir = tempfile::tempdir().unwrap();
        let root = sample(dir.path());
        let (bytes, files) = measure(&root).unwrap();
        assert_eq!(files, 4);
        assert_eq!(bytes, 4000 + 16 + 100);
    }

    #[test]
    fn the_same_drive_and_a_folder_inside_the_game_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let root = sample(dir.path());
        let elsewhere = dir.path().join("elsewhere");
        std::fs::create_dir_all(&elsewhere).unwrap();
        assert!(plan(&root, &elsewhere).unwrap_err().contains("already on that drive"));
        assert!(plan(&root, &root.join("library")).unwrap_err().contains("already on that drive"));
        assert!(plan(&root, Path::new("relative")).is_err());
    }

    /// A real move to another drive, run by hand (the Windows Sandbox walk,
    /// with a virtual drive attached): plans and copies the data root at
    /// REBORN_MOVE_ROOT onto REBORN_MOVE_DRIVE and writes the plan to
    /// REBORN_MOVE_PLAN_OUT for the launcher's --finish-move to complete.
    /// With REBORN_MOVE_EXPECT_REFUSAL set, asserts the plan is refused with
    /// those words instead.
    #[test]
    #[ignore]
    fn real_drive_move() {
        let var = |k: &str| std::env::var(k).unwrap_or_else(|_| panic!("{k} not set"));
        let root = PathBuf::from(var("REBORN_MOVE_ROOT"));
        let drive = PathBuf::from(var("REBORN_MOVE_DRIVE"));
        if let Ok(words) = std::env::var("REBORN_MOVE_EXPECT_REFUSAL") {
            let why = plan(&root, &drive).expect_err("the plan should have been refused");
            println!("refused: {why}");
            assert!(why.contains(&words), "{why}");
            return;
        }
        let plan = plan(&root, &drive).unwrap_or_else(|why| panic!("plan refused: {why}"));
        println!("plan: {plan:?}");
        copy(&plan, &|_, _| {}, &AtomicBool::new(false)).unwrap_or_else(|why| panic!("copy: {why}"));
        std::fs::write(var("REBORN_MOVE_PLAN_OUT"), serde_json::to_vec_pretty(&plan).unwrap()).unwrap();
    }

    #[test]
    #[cfg(windows)]
    fn the_new_folder_is_always_at_the_top_of_the_drive() {
        let (target, staged) = targets_on(Path::new("D:\\"), std::ffi::OsStr::new("NFL Street Reborn"));
        assert_eq!(target, PathBuf::from("D:\\NFL Street Reborn"));
        assert_eq!(staged, PathBuf::from("D:\\NFL Street Reborn.moving"));
    }

    #[test]
    fn the_copy_keeps_hard_links_as_one_file() {
        let dir = tempfile::tempdir().unwrap();
        sample(dir.path());
        let plan = same_volume_plan(dir.path());
        let seen = std::sync::Mutex::new(Vec::new());
        copy(&plan, &|done, total| seen.lock().unwrap().push((done, total)), &AtomicBool::new(false)).unwrap();
        let a = file_identity::of(&plan.staged.join("library/textures/a.png")).unwrap();
        let b = file_identity::of(&plan.staged.join("app/content/textures/a.png")).unwrap();
        assert_eq!(a.index, b.index, "the two names are still one file");
        assert!(plan.staged.join("empty/folder").is_dir());
        assert_eq!(seen.lock().unwrap().last().copied(), Some((plan.bytes, plan.bytes)));
    }

    #[test]
    fn a_cancelled_copy_leaves_nothing_behind() {
        let dir = tempfile::tempdir().unwrap();
        sample(dir.path());
        let plan = same_volume_plan(dir.path());
        assert!(copy(&plan, &|_, _| {}, &AtomicBool::new(true)).unwrap_err().contains("cancelled"));
        assert!(!plan.staged.exists());
    }

    #[test]
    fn finishing_swaps_the_folders_leaves_a_link_and_catches_late_changes() {
        let dir = tempfile::tempdir().unwrap();
        let root = sample(dir.path());
        let plan = same_volume_plan(dir.path());
        copy(&plan, &|_, _| {}, &AtomicBool::new(false)).unwrap();
        // The launcher kept running for a moment after the copy.
        put(&root, "config/settings.json", br#"{"user_dir":"changed"}"#);
        put(&root, "stats/new.json", b"{}");
        std::fs::remove_file(root.join("dolphin-users/User/GC/MemoryCardA.USA.raw")).unwrap();

        let (outcome, retired) = finish(&plan, None);
        assert!(outcome.moved, "{}", outcome.message);
        let retired = retired.unwrap();
        assert!(retired.join(RETIRED_MARKER).is_file());
        assert!(is_dir_link(&root), "the old path is now a link");
        assert_eq!(real(&root), real(&plan.target));
        assert_eq!(std::fs::read(root.join("config/settings.json")).unwrap(), br#"{"user_dir":"changed"}"#);
        assert!(plan.target.join("stats/new.json").is_file());
        assert!(!plan.target.join("dolphin-users/User/GC/MemoryCardA.USA.raw").exists());
        assert!(!plan.target.join(STAGED_MARKER).exists());
        assert!(!plan.staged.exists());
        // The new folder is private to this account: nothing inherited from
        // the drive (security review, 2026-10-09).
        #[cfg(windows)]
        {
            let out = std::process::Command::new("icacls").arg(&plan.target).output().unwrap();
            let text = String::from_utf8_lossy(&out.stdout).to_string();
            let entries: Vec<&str> = text.lines().filter(|l| l.contains(":(")).collect();
            assert_eq!(entries.len(), 3, "{text}");
            assert!(entries.iter().all(|l| !l.contains("(I)")), "{text}");
        }
        assert_eq!(last_outcome(&root).unwrap(), outcome);
        assert_eq!(location(&root), (real(&plan.target), true));
        assert_eq!(leftovers(&root), vec![retired.clone()]);
        std::fs::remove_dir_all(&retired).unwrap();
        assert!(leftovers(&root).is_empty());
    }

    #[test]
    fn moving_again_replaces_the_link_and_puts_the_last_folder_aside() {
        let dir = tempfile::tempdir().unwrap();
        let root = sample(dir.path());
        let first = same_volume_plan(dir.path());
        copy(&first, &|_, _| {}, &AtomicBool::new(false)).unwrap();
        let (_, retired) = finish(&first, None);
        std::fs::remove_dir_all(retired.unwrap()).unwrap();

        let target = dir.path().join("E").join("NFL Street Reborn");
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        let (bytes, files) = measure(&root).unwrap();
        let second = Plan { link: root.clone(), real: real(&root), staged: target.with_file_name("NFL Street Reborn.moving"), target: target.clone(), bytes, files };
        assert!(same_path(&second.real, &real(&first.target)));
        copy(&second, &|_, _| {}, &AtomicBool::new(false)).unwrap();
        let (outcome, retired) = finish(&second, None);
        assert!(outcome.moved, "{}", outcome.message);
        assert_eq!(real(&root), real(&target));
        assert!(!first.target.exists(), "the first new folder was put aside");
        assert!(retired.unwrap().join(RETIRED_MARKER).is_file());
    }

    #[test]
    fn a_plan_with_empty_paths_touches_and_writes_nothing() {
        let empty = Plan { link: PathBuf::new(), real: PathBuf::new(), target: PathBuf::new(), staged: PathBuf::new(), bytes: 0, files: 0 };
        let before = std::env::current_dir().unwrap().join(RESULT_FILE).exists();
        let (outcome, retired) = finish(&empty, None);
        assert!(!outcome.moved && retired.is_none());
        assert_eq!(std::env::current_dir().unwrap().join(RESULT_FILE).exists(), before, "no result file written relative to the working folder");
    }

    #[test]
    fn a_held_file_stops_the_swap_and_changes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let root = sample(dir.path());
        let plan = same_volume_plan(dir.path());
        copy(&plan, &|_, _| {}, &AtomicBool::new(false)).unwrap();
        // A process with its working folder inside the root holds it on
        // Windows the way a running launcher or Dolphin would.
        let held = std::fs::File::options().read(true).write(true).open(root.join("config/settings.json")).unwrap();
        let blocked = cfg!(windows) && std::fs::rename(&root, root.with_file_name("probe")).is_err();
        if !blocked {
            // Not reproducible on this filesystem; the rename succeeded, undo it.
            let _ = std::fs::rename(root.with_file_name("probe"), &root);
            return;
        }
        let (outcome, retired) = finish(&plan, None);
        drop(held);
        assert!(!outcome.moved);
        assert!(retired.is_none());
        assert!(!is_dir_link(&root) && root.join("config/settings.json").is_file());
        assert!(!plan.staged.exists() && !plan.target.exists());
    }

    /// Steam Deck: the game went to the SD card, then the player picks
    /// internal storage again. The files go back to the data root's own path
    /// and the link there is replaced by the real folder.
    #[test]
    #[cfg(unix)]
    fn moving_back_to_where_it_started_leaves_a_real_folder_and_no_link() {
        let dir = tempfile::tempdir().unwrap();
        let root = sample(dir.path());
        let out = same_volume_plan(dir.path());
        copy(&out, &|_, _| {}, &AtomicBool::new(false)).unwrap();
        let (outcome, retired) = finish(&out, None);
        assert!(outcome.moved, "{}", outcome.message);
        std::fs::remove_dir_all(retired.unwrap()).unwrap();
        assert!(is_dir_link(&root));

        let (bytes, files) = measure(&root).unwrap();
        let back = Plan { link: root.clone(), real: real(&root), target: root.clone(), staged: root.with_file_name("NFL Street Reborn.moving"), bytes, files };
        copy(&back, &|_, _| {}, &AtomicBool::new(false)).unwrap();
        let (outcome, retired) = finish(&back, None);
        assert!(outcome.moved, "{}", outcome.message);
        assert!(!is_dir_link(&root) && root.is_dir(), "a real folder again");
        assert_eq!(std::fs::read(root.join("config/settings.json")).unwrap(), br#"{"user_dir":"x"}"#);
        assert!(!out.target.exists(), "the SD card copy was put aside");
        assert!(retired.unwrap().join(RETIRED_MARKER).is_file());
        assert_eq!(location(&root), (real(&root), false));
    }

    #[test]
    #[cfg(not(windows))]
    fn a_card_that_cannot_hold_links_is_refused_in_words_a_deck_player_can_act_on() {
        let dir = tempfile::tempdir().unwrap();
        let root = sample(dir.path());
        let facts = |fs: &str| dir_move::VolumeFacts { mount: PathBuf::from("/run/media/deck/card"), free_bytes: u64::MAX, filesystem: fs.into(), removable: true };
        let why = unix_targets(&root, dir.path(), std::ffi::OsStr::new("NFL Street Reborn"), &facts("exfat")).unwrap_err();
        assert!(why.contains("Format SD Card"), "{why}");
        let (target, staged) = unix_targets(&root, dir.path(), std::ffi::OsStr::new("NFL Street Reborn"), &facts("ext4")).unwrap();
        assert_eq!(target, root, "the same drive as the data root means back to the data root's own path");
        assert_eq!(staged, root.with_file_name("NFL Street Reborn.moving"));
    }

    /// A folder others may write, or reached through a link, is never where
    /// the game goes (security review, 2026-10-12).
    #[test]
    #[cfg(unix)]
    fn a_folder_others_can_change_is_refused_even_through_a_link() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let root = sample(dir.path());
        let facts = dir_move::VolumeFacts { mount: PathBuf::from("/run/media/deck/card"), free_bytes: u64::MAX, filesystem: "ext4".into(), removable: true };
        let open = dir.path().join("open");
        std::fs::create_dir(&open).unwrap();
        std::fs::set_permissions(&open, std::fs::Permissions::from_mode(0o777)).unwrap();
        let card = open.join("card");
        std::fs::create_dir(&card).unwrap();
        // The data root's own drive would mean "move back"; pretend the card is elsewhere by asking
        // for the folders above `card/<name>` directly.
        let name = std::ffi::OsStr::new("NFL Street Reborn");
        let other_root = PathBuf::from("/nonexistent/NFL Street Reborn");
        let why = unix_targets(&other_root, &card, name, &facts).unwrap_err();
        assert!(why.contains("Other accounts can change"), "{why}");
        let via_link = dir.path().join("link");
        std::os::unix::fs::symlink(&card, &via_link).unwrap();
        let why = unix_targets(&other_root, &via_link, name, &facts).unwrap_err();
        assert!(why.contains(&open.display().to_string()), "checked on the real path: {why}");
        let _ = root;
    }
}
