//! What a disc image actually is, in one verdict.
//!
//! Three commands write `settings.game_path` and a fourth boots it. Before
//! this module each answered the question differently or not at all, and a
//! disc nothing recognized was accepted in silence: the player then booted
//! their own NFL Street 2 with Reborn's texture pack over it, which is where
//! "old names with new faces" came from.

use std::path::Path;

use crate::game_patch::{Chain, Route};

/// The game ID every NFL Street 2 (USA) disc reports, Reborn's included.
///
/// Re-exported rather than redeclared: `Chain::parse` already refuses a
/// chain whose `disc_id` is not this, so a second literal here could drift
/// out of step with the one that gates the chain itself.
pub use crate::game_patch::DISC_ID;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// A known node in the chain. `at_head` means no update is pending.
    RebornBuild { label: String, at_head: bool },
    /// The chain's root: an unmodified USA disc, patchable to head.
    Convertible,
    /// NFL Street 2, but bytes nothing in the chain knows.
    Unconverted,
    /// Not NFL Street 2 at all.
    DifferentGame { game_id: String },
    /// Not a readable disc image.
    Unreadable { reason: String },
}

impl Verdict {
    /// Whether Reborn may treat this image as its own: boot it without a
    /// notice, apply its texture pack, and offer it updates.
    pub fn is_reborn(&self) -> bool {
        matches!(self, Verdict::RebornBuild { .. })
    }
}

pub fn classify(chain: Option<&Chain>, game_id: &str, sha256: &str) -> Verdict {
    if game_id != DISC_ID {
        return Verdict::DifferentGame { game_id: game_id.to_string() };
    }
    // No chain means no way to tell Reborn from a mod. Say so rather than
    // assume; assuming is the bug.
    let Some(chain) = chain else { return Verdict::Unconverted };
    match chain.route(sha256) {
        Route::Current => Verdict::RebornBuild {
            label: chain.label_for(sha256).unwrap_or_default(),
            at_head: true,
        },
        Route::Steps(_) if Some(sha256) == chain.base() => Verdict::Convertible,
        Route::Steps(_) => Verdict::RebornBuild {
            label: chain.label_for(sha256).unwrap_or_default(),
            at_head: false,
        },
        Route::Unknown => Verdict::Unconverted,
    }
}

/// The disc's game ID, read with DolphinTool. Moved here from
/// `setup_tools::check_reborn_id` so one place knows how to ask.
pub fn read_game_id(tool: &Path, image: &Path) -> Result<String, String> {
    let mut cmd = std::process::Command::new(tool);
    cmd.arg("header").arg("-i").arg(image).arg("-j");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    let out = cmd.output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err("The game file could not be read.".to_string());
    }
    let header: serde_json::Value =
        serde_json::from_slice(&out.stdout).map_err(|_| "The game file could not be read.".to_string())?;
    header["game_id"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| "The game file has no game ID.".to_string())
}

/// What this image is, end to end: its game ID, its hash, and the installed
/// chain's opinion of it.
pub fn identify(data_root: &Path, tool: &Path, image: &Path) -> Verdict {
    let game_id = match read_game_id(tool, image) {
        Ok(id) => id,
        Err(reason) => return Verdict::Unreadable { reason },
    };
    let sha256 = match crate::library::hash_file(image) {
        Ok(h) => h,
        Err(e) => return Verdict::Unreadable { reason: e.to_string() },
    };
    let chain_path = crate::game_patch::component_root(data_root).join(crate::game_patch::CHAIN_FILE);
    let chain = Chain::load(&chain_path).ok().flatten();
    classify(chain.as_ref(), &game_id, sha256.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Chain::parse enforces real 64-character lowercase hex hashes, so the
    // fixture cannot use short mnemonic stand-ins; each node repeats one
    // letter so the three builds stay visually distinct at a glance.
    const ROOT: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const MID: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    const HEAD: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";

    /// Two steps: root -> mid -> head.
    fn chain() -> Chain {
        let json = r#"{"schema":"nsr.game-patch-chain.v1","disc_id":"DISC_ID","steps":[
            {"from_sha256":"ROOT","to_sha256":"MID","to_bytes":1459978240,
             "patch":"patches/aa-bb.xdelta","label":"NSRV3"},
            {"from_sha256":"MID","to_sha256":"HEAD","to_bytes":1459978240,
             "patch":"patches/bb-cc.xdelta","label":"2026.09.13"}]}"#
            .replace("DISC_ID", DISC_ID)
            .replace("ROOT", ROOT)
            .replace("MID", MID)
            .replace("HEAD", HEAD);
        Chain::parse(json.as_bytes()).expect("the fixture chain parses")
    }

    #[test]
    fn the_chain_head_is_a_reborn_build_with_nothing_pending() {
        assert_eq!(
            classify(Some(&chain()), DISC_ID, HEAD),
            Verdict::RebornBuild { label: "2026.09.13".into(), at_head: true }
        );
    }

    #[test]
    fn a_mid_chain_node_is_a_reborn_build_with_an_update_pending() {
        assert_eq!(
            classify(Some(&chain()), DISC_ID, MID),
            Verdict::RebornBuild { label: "NSRV3".into(), at_head: false }
        );
    }

    #[test]
    fn the_chain_root_is_an_unmodified_disc_we_can_convert() {
        assert_eq!(classify(Some(&chain()), DISC_ID, ROOT), Verdict::Convertible);
    }

    /// The Discord bug. A modified NFL Street 2 passes every game-ID check
    /// ever written and is the one image that can impersonate Reborn.
    #[test]
    fn a_modified_nfl_street_2_is_unconverted_not_a_reborn_build() {
        let v = classify(Some(&chain()), DISC_ID, "deadbeef");
        assert_eq!(v, Verdict::Unconverted);
        assert!(!v.is_reborn(), "an unconverted disc must never pass as Reborn");
    }

    #[test]
    fn another_game_is_named_rather_than_called_unconverted() {
        assert_eq!(
            classify(Some(&chain()), "GALE01", "deadbeef"),
            Verdict::DifferentGame { game_id: "GALE01".into() }
        );
    }

    /// Without a chain we cannot tell a Reborn build from a modified disc,
    /// and guessing "Reborn" is the failure this module exists to prevent.
    #[test]
    fn no_installed_chain_is_unconverted_never_a_reborn_build() {
        let v = classify(None, DISC_ID, HEAD);
        assert_eq!(v, Verdict::Unconverted);
        assert!(!v.is_reborn());
    }

    #[test]
    fn an_image_that_is_not_a_disc_is_unreadable_not_a_different_game() {
        let dir = tempfile::tempdir().expect("temp dir");
        let image = dir.path().join("not-a-disc.iso");
        std::fs::write(&image, b"this is not a gamecube disc").expect("write");
        let missing_tool = dir.path().join("DolphinTool.exe");
        match identify(dir.path(), &missing_tool, &image) {
            Verdict::Unreadable { .. } => {}
            other => panic!("expected Unreadable, got {other:?}"),
        }
    }
}
