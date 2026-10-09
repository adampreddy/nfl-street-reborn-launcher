//! The League edition's commands (Plan 3b; contracts file "Plan 3b → Plan 3a") and the two helpers the netplay
//! path shares with them. Every rejection is a fixed sentence; nothing here ever shows a status, URL or link.
//!
//! Task 7 stub: the netplay helpers only. Task 8 adds the commands and the membership check (it needs 3a Task 1's
//! `league_home` cache, which this branch does not have yet). Until then the server is the membership gate: it opens
//! a league room only for members, and the league files are a members-only download.

use std::path::PathBuf;

use reborn_core::league_edition::{self as le, LeagueInfoError};
use reborn_core::platform::Platform;

pub(crate) const NOT_INSTALLED: &str = "Install the league files from the League tab first.";
pub(crate) const WRONG_BUILD: &str = "These league files are for a different league build. Update the launcher, then update the league files.";
pub(crate) const DAMAGED: &str = "The league files look damaged. Update league files on the League tab to repair them.";

/// The installed league build's version (its content pointer), or `None` when it is not installed.
pub(crate) fn installed_league_version() -> Option<String> {
    let p = Platform::detect();
    let svc = reborn_core::content::ContentService::for_platform(&p).ok()?;
    svc.installed(reborn_core::inventory::ComponentId::LeagueGame.as_str()).ok().flatten().map(|ptr| ptr.current.version)
}

/// The league disc, ready to boot: the build installed, `league.json` naming this launcher's game ID, the image there.
pub(crate) fn ready_league_image(p: &Platform) -> Result<PathBuf, String> {
    let data = p.paths.data_root().map_err(|_| NOT_INSTALLED.to_string())?;
    ready_league_image_in(&data)
}

fn ready_league_image_in(data: &std::path::Path) -> Result<PathBuf, String> {
    match le::read_league_info(data) {
        Ok(_) => {}
        Err(LeagueInfoError::Missing) => return Err(NOT_INSTALLED.into()),
        Err(LeagueInfoError::WrongGameId(_)) => return Err(WRONG_BUILD.into()),
        Err(LeagueInfoError::Unreadable) => return Err(DAMAGED.into()),
    }
    let image = le::league_image_path(data);
    if !image.is_file() {
        return Err(DAMAGED.into());
    }
    Ok(image)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_league_disc_boots_only_when_its_build_is_installed_whole() {
        let dir = std::env::temp_dir().join(format!("reborn-ui-league-image-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(ready_league_image_in(&dir), Err(NOT_INSTALLED.to_string()));
        let info = le::league_info_path(&dir);
        std::fs::create_dir_all(info.parent().unwrap()).unwrap();
        let good = |id: &str| serde_json::json!({"schema":"nsr.league-game.v1","game_id":id,"iso":"game/league.iso","textures":format!("textures/{id}")}).to_string();
        std::fs::write(&info, good("ZZLG01")).unwrap();
        assert_eq!(ready_league_image_in(&dir), Err(WRONG_BUILD.to_string()));
        std::fs::write(&info, "not json").unwrap();
        assert_eq!(ready_league_image_in(&dir), Err(DAMAGED.to_string()));
        std::fs::write(&info, good(le::LEAGUE_GAME_ID)).unwrap();
        assert_eq!(ready_league_image_in(&dir), Err(DAMAGED.to_string()), "league.json without its disc");
        let image = le::league_image_path(&dir);
        std::fs::create_dir_all(image.parent().unwrap()).unwrap();
        std::fs::write(&image, b"disc").unwrap();
        assert_eq!(ready_league_image_in(&dir), Ok(image));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
