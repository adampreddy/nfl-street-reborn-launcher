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

/// Shown when Play is pressed on an NFL Street 2 disc that is not a Reborn
/// build. Worded as the fault it almost always is, never as a preference.
pub const UNVERIFIED_GAME: &str = "Your game file isn't NFL Street Reborn. This is NFL Street 2, but not a Reborn build, so rosters, faces and stats won't match. This usually means setup couldn't convert your disc. Repair it from the Settings page.";

/// What is known about the image that is about to boot, as far as Reborn's own
/// art is concerned.
///
/// Three states and not a `bool`, because "this is not a Reborn build" and "I
/// could not tell what this is" call for opposite actions and a `bool` had to
/// pick one. It picked withhold, which DELETES the composed pack (that is the
/// only thing that switches it off; Dolphin loads whatever is filed under the
/// booted disc's game ID whether or not a launch composed it). So a missing
/// DolphinTool or a disc held open for a moment cost the player their whole
/// texture pack, and "missing textures" is the report that came back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageStanding {
    /// Verified as a Reborn build. Reborn's art belongs on it.
    Reborn,
    /// Verified as something else: a disc Reborn could not convert and the
    /// player chose to play anyway, a retail disc, another game. Reborn's art
    /// has to come off, because it is art for a roster this disc does not have.
    NotReborn,
    /// Not determined. The image could not be read at all, which says nothing
    /// about the image and everything about the tool or the moment. Nothing is
    /// known, so nothing is done: whatever the player has is left alone.
    Unknown,
}

/// Whether this image may boot, and what Reborn then knows about it.
///
/// `Err` means do not boot it. Otherwise the standing says what to do with
/// Reborn's art: see [`ImageStanding`], and `session::texture_policy_for_launch`
/// for the one place that turns it into an action.
///
/// Lives here, beside the verdict, rather than in the window, because the
/// window is not the only thing that boots the game: `rebornctl play` ships in
/// the product and built a `LaunchRequest` with its textures claimed and no
/// verification at all. A second copy of these four lines over there is
/// exactly how the two would come to disagree.
pub fn may_play(verdict: &Verdict, acknowledged: bool) -> Result<ImageStanding, &'static str> {
    match verdict {
        Verdict::Unconverted if !acknowledged => Err(UNVERIFIED_GAME),
        // Asked and not answered. Refusing to boot over this would strand a
        // player whose disc is fine, and withholding would take their pack
        // for the same non-reason.
        Verdict::Unreadable { .. } => Ok(ImageStanding::Unknown),
        v if v.is_reborn() => Ok(ImageStanding::Reborn),
        _ => Ok(ImageStanding::NotReborn),
    }
}

/// The bundled DolphinTool, looked for the way `find_xdelta` looks for
/// xdelta3: beside the installed program first, then in whatever extra
/// directories the caller knows about (its own program folder, usually).
pub fn find_tool(platform: &crate::platform::Platform, extra: &[std::path::PathBuf]) -> Option<std::path::PathBuf> {
    let name = format!("DolphinTool{}", platform.paths.exe_suffix());
    let mut dirs = Vec::new();
    if let Ok(install) = platform.paths.install_root() {
        dirs.push(install.join(crate::game_patch::BUNDLE_DIR));
        dirs.push(install);
    }
    dirs.extend(extra.iter().cloned());
    dirs.iter().map(|d| d.join(&name)).find(|p| p.is_file())
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

/// The verdict for the image the launcher boots.
///
/// The booted image is NOT hashed against the chain on its own, because a
/// roster apply deliberately rewrites it (see `game_image`'s module header):
/// the Builder writes a new file over the link, so a rostered disc is not a
/// chain node and never will be. Hashing it called Reborn's own output
/// `Unconverted` and refused every league player: told "Your game file
/// isn't NFL Street Reborn" at Play, and had their HD texture pack deleted,
/// for the crime of having a roster installed.
///
/// Its own hash is still tried first, because an install with no roster yet
/// really is the pristine and that is the strongest answer available. Only
/// when that fails do we fall back to the provenance: which pristine it was
/// established from (`game_image::find_pristine`), and whether THAT is a
/// build the chain knows.
///
/// This is weaker than hashing: someone who replaces `library/offline/NSR.iso`
/// in place, by hand, keeps a "Reborn" answer, because the fallback never
/// looks at the booted file's own bytes again once the own-hash check has
/// failed. That hole is accepted deliberately, the same way the analogous
/// one is accepted in `online_image::built_from`/`wanted`: the alternative
/// is refusing every league player's own, legitimately-produced disc, which
/// is a worse failure than a hole nobody has been shown to exploit.
pub fn identify_booted(data_root: &Path, tool: &Path, booted: &Path) -> Verdict {
    let game_id = match read_game_id(tool, booted) {
        Ok(id) => id,
        Err(reason) => return Verdict::Unreadable { reason },
    };
    booted_verdict(data_root, &game_id, booted)
}

/// The provenance-aware classification `identify_booted` applies once the
/// disc's game ID is known. Split out from `identify_booted` so the
/// fallback logic is testable without a working DolphinTool to read a game
/// ID with, the same way `classify` is tested apart from `identify`'s
/// DolphinTool call.
fn booted_verdict(data_root: &Path, game_id: &str, booted: &Path) -> Verdict {
    let chain_path = crate::game_patch::component_root(data_root).join(crate::game_patch::CHAIN_FILE);
    let chain = Chain::load(&chain_path).ok().flatten();

    // The booted image's own hash first: an install with no roster yet
    // really is the pristine, and that is the strongest answer there is.
    if let Ok(sha256) = crate::library::hash_file(booted) {
        let verdict = classify(chain.as_ref(), game_id, sha256.as_str());
        if !matches!(verdict, Verdict::Unconverted) {
            return verdict;
        }
    }

    // Fall back to provenance: which pristine this booted image was
    // established from, and whether the chain recognizes THAT build. Not
    // `Convertible`: a pristine that is merely the chain's unmodified root
    // is not itself a claim that the booted bytes in front of us are
    // Reborn's own, so only a genuine build earns `is_reborn()` here.
    if let Some(pristine) = crate::game_image::find_pristine(data_root) {
        let verdict = classify(chain.as_ref(), game_id, &pristine.folder_sha256);
        if verdict.is_reborn() {
            return verdict;
        }
    }

    Verdict::Unconverted
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

    /// The tool is absent here, so this pins the spawn-failure path:
    /// `identify` must not turn "I could not ask" into a verdict about the
    /// disc. It does NOT prove anything about the image's bytes: `tool`
    /// never runs, so `image` is never read by anything and this test would
    /// pass just as well over a byte-perfect real GameCube ISO. Covering a
    /// working tool that rejects genuinely bad disc data would need a
    /// runnable stub DolphinTool, which is beyond this test.
    #[test]
    fn a_missing_dolphin_tool_is_unreadable_not_a_different_game() {
        let dir = tempfile::tempdir().expect("temp dir");
        let image = dir.path().join("not-a-disc.iso");
        std::fs::write(&image, b"this is not a gamecube disc").expect("write");
        let missing_tool = dir.path().join("DolphinTool.exe");
        match identify(dir.path(), &missing_tool, &image) {
            Verdict::Unreadable { .. } => {}
            other => panic!("expected Unreadable, got {other:?}"),
        }
    }

    // `booted_verdict` tests below build a chain around a REAL SHA-256, not
    // a mnemonic stand-in like `chain()` above uses: they need a genuine
    // file whose bytes really hash to a chain node, because unlike
    // `classify`, `booted_verdict` hashes actual files on disk.

    fn write(path: &Path, bytes: &[u8]) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    /// A one-step chain whose head is the real SHA-256 of whatever bytes the
    /// test wrote at the hash it is given.
    fn real_chain(head_sha256: &str) -> Chain {
        let json = r#"{"schema":"nsr.game-patch-chain.v1","disc_id":"DISC_ID","steps":[
            {"from_sha256":"ROOT","to_sha256":"HEAD","to_bytes":1,
             "patch":"patches/root-head.xdelta","label":"2026.09.17"}]}"#
            .replace("DISC_ID", DISC_ID)
            .replace("ROOT", ROOT)
            .replace("HEAD", head_sha256);
        Chain::parse(json.as_bytes()).expect("the fixture chain parses")
    }

    fn save_chain(data_root: &Path, chain: &Chain) {
        let path = crate::game_patch::component_root(data_root).join(crate::game_patch::CHAIN_FILE);
        chain.save(&path).unwrap();
    }

    /// A pristine folder named by a hash the test chose, holding a
    /// placeholder file. `find_pristine` trusts the folder name (that is the
    /// whole point of it, per its own doc comment), so the placeholder's
    /// actual bytes never have to hash to it.
    fn put_pristine(data_root: &Path, sha256: &str) {
        let dir = crate::game_image::pristine_dir(data_root, sha256);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(crate::game_image::IMAGE_FILE), b"pristine placeholder").unwrap();
    }

    /// The bug this module exists to fix, reproduced directly: a booted
    /// image a roster apply has rewritten (so its own bytes are deliberately
    /// not any chain node) must still read as Reborn's own build when the
    /// pristine it came from is one. Before the fix this returned
    /// `Unconverted`, which is exactly "Your game file isn't NFL Street
    /// Reborn" for every league player.
    #[test]
    fn a_rostered_booted_image_is_a_reborn_build_when_its_pristine_is_a_chain_node() {
        let dir = tempfile::tempdir().expect("temp dir");
        let data_root = dir.path();
        let head_sha256 = {
            let scratch = data_root.join("scratch-head.iso");
            write(&scratch, b"the pristine bytes the chain heads at");
            crate::library::hash_file(&scratch).unwrap().as_str().to_string()
        };
        save_chain(data_root, &real_chain(&head_sha256));
        put_pristine(data_root, &head_sha256);
        let booted = data_root.join("library/offline/NSR.iso");
        // NOT the pristine's bytes: a roster apply deliberately rewrites the
        // booted image (see this module's header), so its own hash must
        // never land on the chain again.
        write(&booted, b"the pristine bytes the chain heads at, plus a league roster");

        let verdict = booted_verdict(data_root, DISC_ID, &booted);
        assert!(
            verdict.is_reborn(),
            "a rostered booted image whose pristine is a Reborn build must read as Reborn's \
             own, not {verdict:?}"
        );
    }

    /// The no-roster case must not regress: when the booted image's own
    /// bytes ARE the chain head, that is the strongest answer and must win
    /// without ever consulting the pristine folder.
    #[test]
    fn a_booted_image_whose_own_hash_is_the_chain_head_stays_a_reborn_build() {
        let dir = tempfile::tempdir().expect("temp dir");
        let data_root = dir.path();
        let booted = data_root.join("library/offline/NSR.iso");
        write(&booted, b"the pristine build, never touched by a roster");
        let head_sha256 = crate::library::hash_file(&booted).unwrap().as_str().to_string();
        save_chain(data_root, &real_chain(&head_sha256));
        // Deliberately no pristine folder at all: the own-hash match alone
        // must be enough, so this cannot pass by accident through the
        // fallback the test above exercises.

        let verdict = booted_verdict(data_root, DISC_ID, &booted);
        assert_eq!(verdict, Verdict::RebornBuild { label: "2026.09.17".into(), at_head: true });
    }

    /// Neither the booted bytes nor their provenance are chain nodes: the
    /// original protection must still hold.
    #[test]
    fn a_booted_image_with_no_reborn_provenance_is_unconverted() {
        let dir = tempfile::tempdir().expect("temp dir");
        let data_root = dir.path();
        let head_sha256 = {
            let scratch = data_root.join("scratch-head.iso");
            write(&scratch, b"the real reborn build");
            crate::library::hash_file(&scratch).unwrap().as_str().to_string()
        };
        save_chain(data_root, &real_chain(&head_sha256));
        put_pristine(data_root, "not-a-chain-node");
        let booted = data_root.join("library/offline/NSR.iso");
        write(&booted, b"somebody else's modified disc");

        let verdict = booted_verdict(data_root, DISC_ID, &booted);
        assert_eq!(verdict, Verdict::Unconverted);
        assert!(!verdict.is_reborn());
    }

    /// No pristine at all must never read as a Reborn build either.
    #[test]
    fn a_booted_image_with_no_pristine_at_all_is_unconverted() {
        let dir = tempfile::tempdir().expect("temp dir");
        let data_root = dir.path();
        let head_sha256 = {
            let scratch = data_root.join("scratch-head.iso");
            write(&scratch, b"the real reborn build");
            crate::library::hash_file(&scratch).unwrap().as_str().to_string()
        };
        save_chain(data_root, &real_chain(&head_sha256));
        // No `library/imported` directory at all: `find_pristine` returns
        // `None`.
        let booted = data_root.join("library/offline/NSR.iso");
        write(&booted, b"anything at all");

        let verdict = booted_verdict(data_root, DISC_ID, &booted);
        assert_eq!(verdict, Verdict::Unconverted);
        assert!(!verdict.is_reborn());
    }

    /// The wrapper's own tool-spawn failure path, mirroring `identify`'s: a
    /// missing DolphinTool must read as `Unreadable`, never as a verdict
    /// about bytes nothing actually asked about.
    #[test]
    fn identify_booted_with_a_missing_tool_is_unreadable() {
        let dir = tempfile::tempdir().expect("temp dir");
        let booted = dir.path().join("NSR.iso");
        std::fs::write(&booted, b"anything").expect("write");
        let missing_tool = dir.path().join("DolphinTool.exe");
        match identify_booted(dir.path(), &missing_tool, &booted) {
            Verdict::Unreadable { .. } => {}
            other => panic!("expected Unreadable, got {other:?}"),
        }
    }
}
