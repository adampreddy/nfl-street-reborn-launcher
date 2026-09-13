//! One-time notices the launcher owes the player, kept in the data root
//! (`config/notices.json`) until dismissed. Code that changes something on
//! the player's behalf adds one; the Play and Controllers pages show them
//! with a "Got it" button. Each notice is an id here, and its words live
//! here too, so a page never invents what happened.
use crate::platform::{DataDir, Platform, Result};
use std::path::{Path, PathBuf};

/// A 2.1 mapping still swapped B and X in Dolphin; the launcher put them
/// back because the game does the swap itself now.
pub const BX_SWAP_UNDONE: &str = "bx-swap-undone";

/// Older builds forced graphics and controller settings back on every launch,
/// and pinned some of them in the per-game file where Dolphin's own windows
/// could not beat them. The launcher has taken those back out.
pub const SETTINGS_ARE_YOURS: &str = "settings-are-yours";

fn file(platform: &Platform) -> Result<PathBuf> {
    Ok(platform.paths.data_root()?.join(DataDir::Config.relative()).join("notices.json"))
}

pub fn pending(platform: &Platform) -> Result<Vec<String>> {
    read(&file(platform)?)
}

/// Queue `id` unless it is already pending. True when it was added.
pub fn add(platform: &Platform, id: &str) -> Result<bool> {
    let path = file(platform)?;
    let mut list = read(&path)?;
    if list.iter().any(|n| n == id) {
        return Ok(false);
    }
    list.push(id.to_string());
    write(&path, &list)?;
    Ok(true)
}

pub fn dismiss(platform: &Platform, id: &str) -> Result<()> {
    let path = file(platform)?;
    let mut list = read(&path)?;
    list.retain(|n| n != id);
    write(&path, &list)
}

/// The words for a notice: a title and one short paragraph. `None` for an
/// id this build does not know, which a page then skips rather than shows.
pub fn text(id: &str) -> Option<(&'static str, &'static str)> {
    match id {
        SETTINGS_ARE_YOURS => Some((
            "Your settings are yours",
            "NFL Street Reborn used to reset some emulator settings every time you pressed Play. \
             It now keeps only the few the game needs to run correctly. Resolution, anti-aliasing, \
             graphics backend, controller mapping and ReShade effects are yours to change, in the \
             launcher or in Dolphin, and they stay changed.",
        )),
        BX_SWAP_UNDONE => Some((
            "X and B are back to normal",
            "NFL Street Reborn swaps X and B inside the game now, so the 2.1 controller swap is no \
             longer needed and has been removed from your mapping: B is B and X is X.",
        )),
        _ => None,
    }
}

fn read(path: &Path) -> Result<Vec<String>> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes).unwrap_or_default()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(e.into()),
    }
}

fn write(path: &Path, list: &[String]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let bytes = serde_json::to_vec_pretty(list).map_err(std::io::Error::other)?;
    let temporary = path.with_extension("json.tmp");
    std::fs::write(&temporary, bytes)?;
    std::fs::rename(temporary, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_notice_is_queued_once_and_dismissed_for_good() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config").join("notices.json");
        assert_eq!(read(&path).unwrap(), Vec::<String>::new());
        let mut list = read(&path).unwrap();
        list.push(BX_SWAP_UNDONE.to_string());
        write(&path, &list).unwrap();
        assert_eq!(read(&path).unwrap(), vec![BX_SWAP_UNDONE.to_string()]);
        let mut again = read(&path).unwrap();
        if !again.iter().any(|n| n == BX_SWAP_UNDONE) {
            again.push(BX_SWAP_UNDONE.to_string());
        }
        assert_eq!(again.len(), 1, "queued once");
        again.retain(|n| n != BX_SWAP_UNDONE);
        write(&path, &again).unwrap();
        assert_eq!(read(&path).unwrap(), Vec::<String>::new());
        assert!(text(BX_SWAP_UNDONE).is_some());
        assert!(text("something-this-build-never-heard-of").is_none());
    }
}
