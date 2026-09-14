//! The GameCube controls profile plays the game the GameCube way.
//!
//! # The bug this exists to fix
//!
//! Reborn 2.1 swapped X and B for Xbox and PlayStation pads inside Dolphin's
//! pad profiles (`Profiles/GCPad/Xbox.ini` and `PS.ini`, both XInput), so a
//! GameCube controller, on an adapter or bound by hand, kept the game's own
//! buttons. The 3.0 disc carries that swap inside the game instead: a pad
//! cave hooked at the per-frame pad poll turns physical X into the game's B
//! and physical B into the game's X in every match, for every controller.
//! Xbox and PlayStation pads net out exactly as before. A GameCube pad now
//! has X and B flipped in every play, which is what players reported the
//! moment 3.0 reached them.
//!
//! # How it is undone without a disc release
//!
//! The swap is switched on by ONE word of the game: the branch at
//! [`HOOK_ADDRESS`] into the cave. Dolphin reads a per-game patch list from
//! the user's `GameSettings/<id>.ini` (`[OnFrame]`, enabled through
//! `[OnFrame_Enabled]`) and writes each entry into emulated memory every
//! frame, through the same path its Action Replay codes use. Restoring the
//! original `bl PADClamp` word there each frame means the cave never runs for
//! that player, and nobody else is touched. The launcher already owns that
//! file, so this is four lines written when the profile is GameCube and
//! removed when it is anything else.
//!
//! # Why the offline game only
//!
//! Netplay runs the same emulated code on every machine and Dolphin does not
//! synchronise `[OnFrame]` patches between them. A machine running the swap
//! next to one that does not would read the same pad differently and desync
//! on the first press. So the patch goes into the offline game's file alone,
//! and is taken OUT of any other game id the launch touches, whatever the
//! profile says. Netplay stays as the disc has it for everybody.

use std::path::{Path, PathBuf};

use crate::controller::ControlsProfile;
use crate::ini::IniDocument;

/// The name Dolphin shows for the patch, and the key it is enabled under.
pub const PATCH_NAME: &str = "Reborn GameCube controls (no X/B swap)";

/// The per-frame pad poll's call into the cave: `bl 0x80411400` on the 3.0
/// disc, `bl PADClamp` on every disc before it.
pub const HOOK_ADDRESS: u32 = 0x801E_2730;

/// `bl 0x802465BC` (PADClamp) as encoded at [`HOOK_ADDRESS`]: the word every
/// 2.1 disc carries there, read off three of them.
pub const ORIGINAL_WORD: u32 = 0x4806_3E8D;

/// The only game id that carries the patch. See the module notes on netplay.
pub const OFFLINE_GAME_ID: &str = "GN7E69";

/// Whether a recorded profile asks for the game's own buttons.
pub fn wanted_for(profile: Option<ControlsProfile>) -> bool {
    profile == Some(ControlsProfile::GameCube)
}

fn name_line() -> String {
    format!("${PATCH_NAME}")
}

/// The two `[OnFrame]` lines, exactly as Dolphin's patch parser reads them.
pub fn patch_lines() -> [String; 2] {
    [
        name_line(),
        format!("0x{HOOK_ADDRESS:08X}:dword:0x{ORIGINAL_WORD:08X}"),
    ]
}

/// The body of `[OnFrame]` with this patch's block taken out: its name line
/// and every entry line up to the next `$` name or the end.
fn without_block(body: &[String]) -> Vec<String> {
    let mine = name_line();
    let mut out = Vec::new();
    let mut inside = false;
    for line in body {
        let trimmed = line.trim();
        if trimmed.starts_with('$') {
            inside = trimmed == mine;
        }
        if !inside {
            out.push(line.clone());
        }
    }
    out
}

fn without_name(body: &[String]) -> Vec<String> {
    let mine = name_line();
    body.iter().filter(|l| l.trim() != mine).cloned().collect()
}

/// Make one document carry the patch exactly when `wanted`. Every other
/// line of the file, including other patches and the player's own Gecko and
/// Action Replay codes, is left as it was. Returns whether anything changed.
pub fn apply_to_document(doc: &mut IniDocument, wanted: bool) -> bool {
    let mut changed = false;

    let body = doc.raw_lines("OnFrame");
    let mut next = without_block(&body);
    if wanted {
        next.extend(patch_lines());
    }
    if next != body {
        changed |= doc.replace_raw_lines("OnFrame", &next);
    }

    let body = doc.raw_lines("OnFrame_Enabled");
    let mut next = without_name(&body);
    if wanted {
        next.push(name_line());
    }
    if next != body {
        changed |= doc.replace_raw_lines("OnFrame_Enabled", &next);
    }

    // A disabled entry would win over the enabled one, so it is never ours.
    let body = doc.raw_lines("OnFrame_Disabled");
    let next = without_name(&body);
    if next != body {
        changed |= doc.replace_raw_lines("OnFrame_Disabled", &next);
    }

    changed
}

/// What [`apply`] did.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Applied {
    /// Files whose contents changed.
    pub files_written: Vec<PathBuf>,
    /// Whether the offline game now carries the patch.
    pub carried: bool,
}

/// Bring every per-game file a launch touches into line: the offline game
/// carries the patch when `wanted`, every other game id never does.
///
/// A file that does not exist is created only to carry the patch. Nothing is
/// created to remove what was never there.
pub fn apply(user_dir: &Path, game_ids: &[&str], wanted: bool) -> std::io::Result<Applied> {
    let mut out = Applied::default();
    for game_id in game_ids {
        let carry = wanted && *game_id == OFFLINE_GAME_ID;
        let path = user_dir.join("GameSettings").join(format!("{game_id}.ini"));
        let existing = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && !carry => continue,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(e) => return Err(e),
        };
        let mut doc = IniDocument::parse(&existing);
        if apply_to_document(&mut doc, carry) {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&path, doc.render())?;
            out.files_written.push(path);
        }
        if carry {
            out.carried = true;
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shape of a real player's file: their own Gecko and Action Replay
    /// codes above the launcher's keys.
    const PLAYERS_OWN: &str = "[Gecko]\n$Remove GameBreaker 2 [Ludiyolo252]\nC20D411C 00000003\n2C1E2710 40810008\n3BC02710 93C90000\n60000000 00000000\n*Removes GameBreaker 2\n[ActionReplay]\n$Legends Text Code\n0428E9C8 4368616D\n[Core]\nCPUThread = False\nMEM1Size = 67108864\nRAMOverrideEnable = True\n[Video_Settings]\nCacheHiresTextures = False\n";

    #[test]
    fn the_patch_line_is_what_dolphin_parses() {
        assert_eq!(
            patch_lines(),
            [
                "$Reborn GameCube controls (no X/B swap)".to_string(),
                "0x801E2730:dword:0x48063E8D".to_string()
            ]
        );
    }

    #[test]
    fn gamecube_puts_the_patch_beside_the_players_own_codes() {
        let mut doc = IniDocument::parse(PLAYERS_OWN);
        assert!(apply_to_document(&mut doc, true));
        let text = doc.render();
        assert!(
            text.starts_with(PLAYERS_OWN),
            "everything the player had must still be there, first and unchanged:\n{text}"
        );
        assert_eq!(
            doc.raw_lines("OnFrame"),
            patch_lines().to_vec(),
            "the patch and only the patch under [OnFrame]"
        );
        assert_eq!(doc.raw_lines("OnFrame_Enabled"), vec![name_line()]);
        assert_eq!(doc.get("Core", "CPUThread"), Some("False"), "keys still readable");
    }

    #[test]
    fn xbox_takes_it_out_and_leaves_the_rest_byte_for_byte() {
        let mut doc = IniDocument::parse(PLAYERS_OWN);
        apply_to_document(&mut doc, true);
        let with = doc.render();
        let mut back = IniDocument::parse(&with);
        assert!(apply_to_document(&mut back, false));
        assert_eq!(back.render(), PLAYERS_OWN, "removing the patch restores the original file exactly");
    }

    #[test]
    fn a_second_apply_changes_nothing() {
        let mut doc = IniDocument::parse(PLAYERS_OWN);
        apply_to_document(&mut doc, true);
        let once = doc.render();
        assert!(!apply_to_document(&mut doc, true), "already there");
        assert_eq!(doc.render(), once);

        let mut doc = IniDocument::parse(PLAYERS_OWN);
        assert!(!apply_to_document(&mut doc, false), "nothing to remove");
        assert_eq!(doc.render(), PLAYERS_OWN);
    }

    #[test]
    fn somebody_elses_patch_survives_both_directions() {
        let theirs = "[OnFrame]\n$Their patch\n0x80000000:dword:0x00000001\n[OnFrame_Enabled]\n$Their patch\n";
        let mut doc = IniDocument::parse(theirs);
        assert!(apply_to_document(&mut doc, true));
        assert_eq!(
            doc.raw_lines("OnFrame"),
            vec![
                "$Their patch".to_string(),
                "0x80000000:dword:0x00000001".to_string(),
                name_line(),
                patch_lines()[1].clone()
            ]
        );
        assert_eq!(doc.raw_lines("OnFrame_Enabled"), vec!["$Their patch".to_string(), name_line()]);

        let with = doc.render();
        let mut back = IniDocument::parse(&with);
        assert!(apply_to_document(&mut back, false));
        assert_eq!(back.render(), theirs);
    }

    #[test]
    fn a_stale_disabled_entry_is_removed_so_it_cannot_win() {
        let stale = format!("[OnFrame]\n{}\n{}\n[OnFrame_Disabled]\n{}\n", patch_lines()[0], patch_lines()[1], name_line());
        let mut doc = IniDocument::parse(&stale);
        assert!(apply_to_document(&mut doc, true));
        assert!(doc.raw_lines("OnFrame_Disabled").is_empty(), "{}", doc.render());
        assert_eq!(doc.raw_lines("OnFrame_Enabled"), vec![name_line()]);
    }

    #[test]
    fn the_offline_game_carries_it_and_the_online_build_never_does() {
        let dir = tempfile::tempdir().unwrap();
        let user = dir.path();
        std::fs::create_dir_all(user.join("GameSettings")).unwrap();
        std::fs::write(user.join("GameSettings/GN7E69.ini"), PLAYERS_OWN).unwrap();
        // An online file that somehow carries the patch already.
        let mut online = IniDocument::parse("[Core]\nCPUThread = False\n");
        apply_to_document(&mut online, true);
        std::fs::write(user.join("GameSettings/FR4N31.ini"), online.render()).unwrap();

        let applied = apply(user, &["FR4N31", "GN7E69"], true).unwrap();
        assert!(applied.carried);
        assert_eq!(applied.files_written.len(), 2, "{applied:?}");

        let offline = IniDocument::parse(&std::fs::read_to_string(user.join("GameSettings/GN7E69.ini")).unwrap());
        assert_eq!(offline.raw_lines("OnFrame"), patch_lines().to_vec());
        let online = IniDocument::parse(&std::fs::read_to_string(user.join("GameSettings/FR4N31.ini")).unwrap());
        assert!(online.raw_lines("OnFrame").is_empty(), "netplay must run what the disc has");
        assert!(online.raw_lines("OnFrame_Enabled").is_empty());

        // Once more: nothing to write.
        let again = apply(user, &["FR4N31", "GN7E69"], true).unwrap();
        assert!(again.files_written.is_empty(), "{again:?}");
    }

    #[test]
    fn removing_never_creates_a_file_and_carrying_creates_the_offline_one() {
        let dir = tempfile::tempdir().unwrap();
        let user = dir.path();
        let applied = apply(user, &["GN7E69", "FR4N31"], false).unwrap();
        assert_eq!(applied, Applied::default());
        assert!(!user.join("GameSettings").exists(), "nothing to remove, nothing created");

        let applied = apply(user, &["GN7E69", "FR4N31"], true).unwrap();
        assert!(applied.carried);
        assert_eq!(applied.files_written, vec![user.join("GameSettings").join("GN7E69.ini")]);
        assert!(!user.join("GameSettings/FR4N31.ini").exists());
        let text = std::fs::read_to_string(user.join("GameSettings/GN7E69.ini")).unwrap();
        assert_eq!(
            text,
            "[OnFrame]\n$Reborn GameCube controls (no X/B swap)\n0x801E2730:dword:0x48063E8D\n\n[OnFrame_Enabled]\n$Reborn GameCube controls (no X/B swap)\n"
        );
    }

    #[test]
    fn only_the_gamecube_profile_wants_it() {
        assert!(wanted_for(Some(ControlsProfile::GameCube)));
        assert!(!wanted_for(Some(ControlsProfile::Xbox)));
        assert!(!wanted_for(Some(ControlsProfile::PlayStation)));
        assert!(!wanted_for(None), "an unanswered first run keeps the disc's behaviour");
    }
}
