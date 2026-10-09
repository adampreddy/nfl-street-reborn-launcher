//! The League edition: the league's own build of the game (its own ISO, texture tree and game ID), installed from the
//! League tab by members only (spec 2026-10-08 v2 §4).
//!
//! The on-disk layout is Plan 4's (the publisher writes it, this reads it) and is fixed here, never taken from
//! `league.json`: the file only confirms which build is installed. A manifest that put the disc anywhere else would be
//! a different component, and composing textures under a game ID this launcher does not boot would leave the league
//! game with none.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::platform::DataDir;

/// The disc ID the league build carries (its first six bytes). Dolphin files the league's textures and per-game
/// settings under it. Named by Adam 10-09 (contracts file). A stable installer build refuses if this is ever the
/// placeholder (`league_game_id_is_named_before_a_stable_release`).
pub const LEAGUE_GAME_ID: &str = "NSRS04";
/// The pre-naming value. Kept so the stable release gate can tell a named ID from an unnamed one.
pub const LEAGUE_GAME_ID_PLACEHOLDER: &str = "LGXXXX";
/// The retail Reborn disc, whose mod payloads the league build borrows (one shared mod list, spec v2).
pub const RETAIL_GAME_ID: &str = "GN7E69";
pub const LEAGUE_INFO_SCHEMA: &str = "nsr.league-game.v1";

const INFO_FILE: &str = "league.json";
const ISO_PATH: &str = "game/league.iso";
const SEED_PATH: &str = "seed/league.s01";

pub fn league_dir(data_root: &Path) -> PathBuf {
    data_root.join(DataDir::LibraryLeague.relative())
}

pub fn league_image_path(data_root: &Path) -> PathBuf {
    let mut p = league_dir(data_root);
    for seg in ISO_PATH.split('/') {
        p = p.join(seg);
    }
    p
}

/// The pristine league texture tree the composed `Load/Textures/<LEAGUE_GAME_ID>` is built from.
pub fn league_textures_root(data_root: &Path) -> PathBuf {
    league_dir(data_root).join("textures").join(LEAGUE_GAME_ID)
}

pub fn league_info_path(data_root: &Path) -> PathBuf {
    league_dir(data_root).join(INFO_FILE)
}

pub fn league_seed_path(data_root: &Path) -> PathBuf {
    let mut p = data_root.join(DataDir::LibraryLeagueContent.relative());
    for seg in SEED_PATH.split('/') {
        p = p.join(seg);
    }
    p
}

/// `league.json`, as Plan 4's publisher stages it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct LeagueInfo {
    pub schema: String,
    pub game_id: String,
    pub iso: String,
    pub textures: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LeagueInfoError {
    /// No league build installed.
    Missing,
    /// There, but not a `nsr.league-game.v1` file in Plan 4's layout.
    Unreadable,
    /// A league build for another game ID: this launcher would compose and boot the wrong thing.
    WrongGameId(String),
}

/// The installed build's `league.json`, accepted only when it is exactly the layout this launcher boots.
pub fn read_league_info(data_root: &Path) -> Result<LeagueInfo, LeagueInfoError> {
    let bytes = match std::fs::read(league_info_path(data_root)) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(LeagueInfoError::Missing),
        Err(_) => return Err(LeagueInfoError::Unreadable),
    };
    let info: LeagueInfo = serde_json::from_slice(&bytes).map_err(|_| LeagueInfoError::Unreadable)?;
    if info.schema != LEAGUE_INFO_SCHEMA
        || info.iso != ISO_PATH
        || info.textures != format!("textures/{}", info.game_id)
    {
        return Err(LeagueInfoError::Unreadable);
    }
    if info.game_id != LEAGUE_GAME_ID {
        return Err(LeagueInfoError::WrongGameId(info.game_id));
    }
    Ok(info)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_info(root: &std::path::Path, json: &str) {
        let p = league_info_path(root);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, json).unwrap();
    }

    #[test]
    fn the_layout_is_plan_4s() {
        let root = std::path::Path::new("/d");
        assert_eq!(league_dir(root), root.join("library/league"));
        assert_eq!(league_image_path(root), root.join("library/league").join("game").join("league.iso"));
        assert_eq!(league_textures_root(root), root.join("library/league").join("textures").join(LEAGUE_GAME_ID));
        assert_eq!(league_seed_path(root), root.join("library/league-content").join("seed").join("league.s01"));
    }

    #[test]
    fn league_info_must_name_this_launchers_game_id() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(read_league_info(dir.path()), Err(LeagueInfoError::Missing));
        write_info(dir.path(), "not json");
        assert_eq!(read_league_info(dir.path()), Err(LeagueInfoError::Unreadable));
        write_info(dir.path(), &format!(
            r#"{{"schema":"nsr.league-game.v1","game_id":"ZZZZ99","iso":"game/league.iso","textures":"textures/ZZZZ99"}}"#));
        assert_eq!(read_league_info(dir.path()), Err(LeagueInfoError::WrongGameId("ZZZZ99".into())));
        write_info(dir.path(), &format!(
            r#"{{"schema":"nsr.league-game.v2","game_id":"{LEAGUE_GAME_ID}","iso":"game/league.iso","textures":"textures/{LEAGUE_GAME_ID}"}}"#));
        assert_eq!(read_league_info(dir.path()), Err(LeagueInfoError::Unreadable));
        write_info(dir.path(), &format!(
            r#"{{"schema":"nsr.league-game.v1","game_id":"{LEAGUE_GAME_ID}","iso":"elsewhere.iso","textures":"textures/{LEAGUE_GAME_ID}"}}"#));
        assert_eq!(read_league_info(dir.path()), Err(LeagueInfoError::Unreadable), "the layout is fixed, never taken from the file");
        write_info(dir.path(), &format!(
            r#"{{"schema":"nsr.league-game.v1","game_id":"{LEAGUE_GAME_ID}","iso":"game/league.iso","textures":"textures/{LEAGUE_GAME_ID}"}}"#));
        assert_eq!(read_league_info(dir.path()).unwrap().game_id, LEAGUE_GAME_ID);
    }

    /// Release gate. `installer/windows/build.ps1 -Channel stable` runs this with `--ignored`; it fails if
    /// `LEAGUE_GAME_ID` is ever the placeholder again. A beta or a dev build is not held by it.
    #[test]
    #[ignore = "release gate: run by build.ps1 for the stable channel"]
    fn league_game_id_is_named_before_a_stable_release() {
        assert_ne!(LEAGUE_GAME_ID, LEAGUE_GAME_ID_PLACEHOLDER, "set LEAGUE_GAME_ID in league_edition.rs before a stable release");
        assert!(crate::mods::GameId::parse(LEAGUE_GAME_ID).is_ok());
    }

    #[test]
    fn the_league_game_id_is_a_valid_game_id() {
        assert!(crate::mods::GameId::parse(LEAGUE_GAME_ID).is_ok(), "the tree is filed under this name");
        assert_eq!(LEAGUE_GAME_ID.len(), 6);
        assert_ne!(LEAGUE_GAME_ID, RETAIL_GAME_ID);
    }
}
