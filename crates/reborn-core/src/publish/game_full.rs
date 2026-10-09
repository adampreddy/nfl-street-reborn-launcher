//! The `game-full` recovery component: the head build's complete disc image,
//! staged and pinned so a player whose own copy cannot be converted can
//! download a working game instead of being stranded.
//!
//! # Why being in the release costs a player nothing
//!
//! This component is pinned like any other, and
//! [`crate::publish::release::republish_pinning`] carries it forward into
//! every later release like any other. That is deliberate, and it was not
//! always so: the pin was once skipped in the carry-forward loop, on the
//! theory that it kept 1.36 GB away from a normal install. It did not. A pin
//! is a line of JSON; what decides who downloads the full disc is
//! [`crate::content::pipeline::updatable`], which does not list `game-full`,
//! so the launcher's automatic check/fetch/promote loop never asks for it
//! however many releases pin it. Only the recovery command names it, and
//! only when a player presses the button. See
//! `crate::content::pipeline::tests::
//! game_full_is_recovery_the_automatic_loop_never_asks_for`.
//!
//! What the exclusion did buy was silent expiry: the pin vanished from the
//! next ordinary release, so recovery quietly stopped being offered to the
//! player it exists for. `crate::publish::release::tests::
//! the_full_disc_carries_forward_so_recovery_does_not_expire` guards against
//! that returning.
//!
//! # Why the stage is a fresh tree of exactly one file
//!
//! [`ComponentRequest`] scans every file under its `root`. A build's own
//! output directory is not self-contained the way the game-patch chain's
//! tree is (that tree accumulates only `chain.json` and patches, written by
//! nothing else); publishing straight out of wherever the ISO happens to sit
//! would publish whatever else is in that directory as part of this
//! component. So [`stage`] hard-links (falling back to a copy) the one file
//! this component ever has into a tree of its own.
//!
//! # Where it lands
//!
//! `ComponentId::GameFull`'s `install_root()` is `library/game-full`, not
//! `library/offline`. `library/offline` holds the BOOTED image
//! (`crate::game_image::booted_path`), which a roster apply rewrites in
//! place, and pristine images live at `library/imported/<sha256>`
//! (`crate::game_image` module doc). A downloaded disc is a pristine
//! image, not a booted one, so its landing spot must collide with
//! neither; turning a landed download into a pristine image is a later,
//! separate step.

use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::game_patch::{Chain, CHAIN_FILE};
use crate::inventory::{ComponentId, ComponentSpec, ScanObserver};
use crate::library::hash_file;
use crate::publish::component::{publish_component, ComponentRequest};
use crate::publish::release::{republish_pinning, CarryForward};
use crate::publish::roster::Push;
use crate::publish::{default_stage, io_err, next_sequence, out, refuse, thousands, Outcome, Sink};
use crate::Result;

/// The file name the disc is published under. Matches the name the booted
/// image ships under (`crate::game_image::IMAGE_FILE`) so a later promotion
/// step needs no rename, but this component's own install root is
/// `library/game-full`, not the booted or pristine layer: see
/// [`crate::inventory::ComponentId::install_root`]'s note on `GameFull` for
/// why landing here is not the same as becoming the player's game.
pub const ISO_FILE_NAME: &str = "NSR.iso";

#[derive(Debug)]
pub struct GameFullRequest<'a> {
    /// The complete built disc to publish. Must hash to the game-patch
    /// chain's current head.
    pub iso: &'a Path,
    /// The build's label, matching the game-patch step that produced it.
    pub version: &'a str,
    /// The tree the game-patch chain accumulates in, read here only for its
    /// head. Defaults to `<origin>-stage\game-patch`, matching
    /// `publish_game_patch`'s own default.
    pub chain_tree: Option<&'a Path>,
    /// Where the one-file component tree is staged before publishing.
    /// Defaults to `<origin>-stage\game-full`.
    pub stage: Option<&'a Path>,
    pub origin: &'a Path,
    pub sequence: Option<u64>,
    pub key: Option<&'a Path>,
    pub release_version: Option<&'a str>,
    pub release_sequence: Option<u64>,
    /// Stage, publish and sign. Without it the disc is hashed and checked
    /// against the chain head, and nothing is written.
    pub apply: bool,
}

/// The published full disc and the chain head must be the same bytes, or a
/// player who recovers by download lands on a build no patch can move.
///
/// Returns a plain `String` rather than [`crate::platform::Error`]: this is
/// a pure check with one caller, [`stage`], which folds the message into its
/// own refusal lines rather than propagating an error type.
pub fn refuse_if_not_chain_head(iso_sha: &str, chain_head: &str) -> std::result::Result<(), String> {
    if iso_sha != chain_head {
        return Err(format!(
            "the full disc is {iso_sha} but the chain head is {chain_head}; publish the chain first"
        ));
    }
    Ok(())
}

/// Publish the `game-full` recovery component: hash the disc, refuse unless
/// it is the game-patch chain's current head, stage it alone, publish it,
/// and re-sign the release with every other pin carried forward and this one
/// pinned by name.
///
/// The release step goes through [`republish_pinning`] like every other
/// component publisher in this module family, and this pin then carries
/// forward into later releases like every other, so recovery stays available
/// to a player long after the release that published it. Keeping the full
/// disc away from a normal install is
/// [`crate::content::pipeline::updatable`]'s job, not this one's.
pub fn stage(req: &GameFullRequest<'_>, observer: &dyn ScanObserver, sink: Sink<'_>) -> Result<Outcome<Push>> {
    let (iso, version, origin, apply) = (req.iso, req.version, req.origin, req.apply);

    if !iso.is_file() {
        return Ok(refuse(sink, &[format!("--iso {} is not a file. Nothing was published.", iso.display())]));
    }

    let chain_tree: PathBuf = match req.chain_tree {
        Some(t) => t.to_path_buf(),
        None => default_stage(origin, "game-patch"),
    };
    let chain_path = chain_tree.join(CHAIN_FILE);
    let chain = Chain::load(&chain_path)?.unwrap_or_default();
    let Some(head) = chain.head() else {
        return Ok(refuse(sink, &[
            format!("{} names no chain, or the chain has no steps yet.", chain_path.display()),
            "Publish at least one game-patch first, or pass --chain-tree. Nothing was published.".to_string(),
        ]));
    };

    out!(sink, "Disc          {}", iso.display());
    out!(sink, "Chain         {} (head {}…)", chain_path.display(), &head[..12]);
    out!(sink, "Origin        {}", origin.display());
    out!(sink);
    out!(sink, "Hashing the disc.");
    let started = Instant::now();
    let iso_sha = hash_file(iso)?;
    let iso_bytes = std::fs::metadata(iso).map_err(|e| io_err(iso, e))?.len();
    out!(sink, "Disc is       {iso_sha}  ({} bytes), hashed in {:.1} s", thousands(iso_bytes), started.elapsed().as_secs_f64());

    if let Err(e) = refuse_if_not_chain_head(iso_sha.as_str(), head) {
        return Ok(refuse(sink, &[String::new(), e, "Nothing was published.".to_string()]));
    }

    if !apply {
        out!(sink);
        out!(sink, "Nothing was staged or written. Re-run with --apply to stage the disc as");
        out!(sink, "{ISO_FILE_NAME}, publish the component and re-sign the release.");
        return Ok(Outcome::Described);
    }

    let stage_dir: PathBuf = match req.stage {
        Some(t) => t.to_path_buf(),
        None => default_stage(origin, "game-full"),
    };
    std::fs::create_dir_all(&stage_dir).map_err(|e| io_err(&stage_dir, e))?;
    let staged = stage_dir.join(ISO_FILE_NAME);
    // Dropped first rather than compared: the disc was just hashed above, so
    // a stale file left by an earlier version at this same path is replaced
    // unconditionally instead of paying for a second 1.36 GB hash to decide
    // whether it already matches.
    let _ = std::fs::remove_file(&staged);
    if std::fs::hard_link(iso, &staged).is_err() {
        std::fs::copy(iso, &staged).map_err(|e| io_err(&staged, e))?;
    }

    let sequence = match req.sequence {
        Some(s) => s,
        None => {
            let next = next_sequence(origin, ComponentId::GameFull)?;
            out!(sink, "Sequence      {next} (one above the highest game-full already in the origin)");
            next
        }
    };
    out!(sink);
    let spec = match ComponentSpec::new(ComponentId::GameFull, version, sequence) {
        Ok(s) => s,
        Err(e) => return Ok(refuse(sink, &[e.to_string()])),
    };
    let component = match publish_component(&ComponentRequest { root: &stage_dir, spec, origin, apply: true }, observer, sink)? {
        Outcome::Refused(r) => return Ok(Outcome::Refused(r)),
        Outcome::Described => {
            // Unreachable with `apply: true` above; kept so this match stays
            // exhaustive over `Outcome` the way `game_patch.rs` keeps it,
            // rather than assuming a sibling function's internals can never
            // change shape under it.
            let carry = CarryForward {
                origin, id: ComponentId::GameFull, version,
                key: req.key, release_version: req.release_version, release_sequence: req.release_sequence,
                notes: None,
                apply: true,
            };
            return Ok(match republish_pinning(&carry, sink)? {
                Outcome::Refused(r) => Outcome::Refused(r),
                _ => Outcome::Described,
            });
        }
        Outcome::Done(component) => component,
    };

    let carry = CarryForward {
        origin, id: ComponentId::GameFull, version,
        key: req.key, release_version: req.release_version, release_sequence: req.release_sequence,
        notes: None,
        apply: true,
    };
    Ok(match republish_pinning(&carry, sink)? {
        Outcome::Done(s) => Outcome::Done(Push { component, release: Some(s) }),
        Outcome::Described => Outcome::Done(Push { component, release: None }),
        Outcome::Refused(r) => Outcome::Refused(r),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_patch::Step;
    use crate::inventory::RunToCompletion;
    use crate::publish::{Event, Outcome};

    fn write_chain(tree: &Path, head_sha: &str) {
        let mut chain = Chain::new();
        chain.push(Step {
            from_sha256: "0".repeat(64),
            to_sha256: head_sha.to_string(),
            to_bytes: 1,
            patch: Chain::patch_name(&"0".repeat(64), head_sha),
            label: "2026.09.13".into(),
        }).unwrap();
        chain.save(&tree.join(CHAIN_FILE)).unwrap();
    }

    fn req<'a>(iso: &'a Path, chain_tree: &'a Path, stage: &'a Path, origin: &'a Path) -> GameFullRequest<'a> {
        GameFullRequest {
            iso, version: "2026.09.13.3", chain_tree: Some(chain_tree), stage: Some(stage), origin,
            sequence: None, key: None, release_version: None, release_sequence: None, apply: true,
        }
    }

    fn run(r: &GameFullRequest<'_>) -> (Outcome<Push>, Vec<String>) {
        let mut lines = Vec::new();
        (stage(r, &RunToCompletion, &mut |e: Event| lines.push(e.line)).unwrap(), lines)
    }

    #[test]
    fn refuse_if_not_chain_head_names_both_hashes() {
        assert!(refuse_if_not_chain_head("a", "a").is_ok());
        let err = refuse_if_not_chain_head("a", "b").unwrap_err();
        assert!(err.contains('a') && err.contains('b'), "{err}");
    }

    #[test]
    fn a_missing_iso_is_refused_before_anything_else() {
        let d = tempfile::tempdir().unwrap();
        let missing = d.path().join("missing.iso");
        let chain_tree = d.path().join("chain");
        let stage_dir = d.path().join("stage");
        let origin = d.path().join("origin");
        let r = req(&missing, &chain_tree, &stage_dir, &origin);
        let (outcome, lines) = run(&r);
        assert!(outcome.refused().is_some());
        assert!(lines.iter().any(|l| l.contains("--iso") && l.contains("is not a file")), "{lines:?}");
    }

    #[test]
    fn a_chain_with_no_head_is_refused() {
        let d = tempfile::tempdir().unwrap();
        let iso = d.path().join("disc.iso");
        std::fs::write(&iso, b"disc bytes").unwrap();
        let chain_tree = d.path().join("chain"); // never written
        let stage_dir = d.path().join("stage");
        let origin = d.path().join("origin");
        let r = req(&iso, &chain_tree, &stage_dir, &origin);
        let (outcome, lines) = run(&r);
        assert!(outcome.refused().is_some());
        assert!(lines.iter().any(|l| l.contains("names no chain")), "{lines:?}");
    }

    #[test]
    fn a_disc_that_is_not_the_chain_head_is_refused() {
        let d = tempfile::tempdir().unwrap();
        let iso = d.path().join("disc.iso");
        std::fs::write(&iso, b"disc bytes").unwrap();
        let chain_tree = d.path().join("chain");
        // The chain's head is some hash that is not this disc's.
        write_chain(&chain_tree, &"f".repeat(64));
        let stage_dir = d.path().join("stage");
        let origin = d.path().join("origin");
        let r = req(&iso, &chain_tree, &stage_dir, &origin);
        let (outcome, lines) = run(&r);
        assert!(outcome.refused().is_some());
        assert!(lines.iter().any(|l| l.contains("is not the chain head") || l.contains("but the chain head is")), "{lines:?}");
        assert!(!stage_dir.join(ISO_FILE_NAME).exists(), "a refused disc must not be staged");
    }

    #[test]
    fn a_dry_run_checks_and_writes_nothing() {
        let d = tempfile::tempdir().unwrap();
        let iso = d.path().join("disc.iso");
        std::fs::write(&iso, b"disc bytes").unwrap();
        let head = hash_file(&iso).unwrap();
        let chain_tree = d.path().join("chain");
        write_chain(&chain_tree, head.as_str());
        let stage_dir = d.path().join("stage");
        let origin = d.path().join("origin");
        let mut r = req(&iso, &chain_tree, &stage_dir, &origin);
        r.apply = false;
        let (outcome, lines) = run(&r);
        assert!(matches!(outcome, Outcome::Described));
        assert!(lines.iter().any(|l| l.contains("Nothing was staged or written")), "{lines:?}");
        assert!(!stage_dir.exists(), "a dry run must not create the stage directory");
        assert!(!origin.join("components").exists());
    }

    #[test]
    fn staging_publishes_the_component_and_signs_a_release_carrying_the_rest() {
        use crate::publish::keys::keygen;
        use crate::publish::release::{release, Pin, ReleaseRequest};

        let d = tempfile::tempdir().unwrap();
        let iso = d.path().join("disc.iso");
        std::fs::write(&iso, b"the complete disc").unwrap();
        let head = hash_file(&iso).unwrap();
        let chain_tree = d.path().join("chain");
        write_chain(&chain_tree, head.as_str());
        let stage_dir = d.path().join("stage");
        let origin = d.path().join("origin");
        let keys = d.path().join("keys");
        let written = match keygen(&keys, &mut |_| {}).unwrap() { Outcome::Done(w) => w, o => panic!("{o:?}") };

        // A release already serving another component, so the carry-forward
        // this call performs has something real to carry.
        let root = d.path().join("tree-roster");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("roster.rebornroster"), b"PK").unwrap();
        let spec = ComponentSpec::new(ComponentId::Roster, "2026.09.13", 1).unwrap();
        assert!(publish_component(&ComponentRequest { root: &root, spec, origin: &origin, apply: true }, &RunToCompletion, &mut |_| {}).unwrap().is_done());
        let sign_req = ReleaseRequest {
            origin: &origin, version: "3.0.0", sequence: 1, key: &written.publisher_path,
            pins: &[Pin { id: ComponentId::Roster, version: "2026.09.13".to_string() }], notes: None, apply: true,
        };
        assert!(release(&sign_req, &mut |_| {}).unwrap().is_done());

        let r = GameFullRequest {
            iso: &iso, version: "2026.09.13.3", chain_tree: Some(&chain_tree), stage: Some(&stage_dir),
            origin: &origin, sequence: None, key: Some(&written.publisher_path),
            release_version: None, release_sequence: None, apply: true,
        };
        let (outcome, lines) = run(&r);
        let push = match outcome { Outcome::Done(p) => p, o => panic!("{o:?}") };
        assert_eq!(push.component.files, 1);

        // Staged under its own name, hard-linked or copied rather than left
        // where the caller's ISO sits.
        let staged = stage_dir.join(ISO_FILE_NAME);
        assert!(staged.is_file());
        assert_eq!(hash_file(&staged).unwrap(), head);

        // The manifest is published under the component's own name.
        assert!(origin.join("components").join("game-full-2026.09.13.3.json").is_file());

        // The release now pins both: the new game-full AND the roster
        // carried forward from before.
        let release_signed = push.release.expect("a key was given, so the release step ran");
        assert_eq!(release_signed.sequence, 2);
        assert!(
            lines.iter().any(|l| l.contains("pinning game-full=2026.09.13.3, roster=2026.09.13")),
            "{lines:?}"
        );
    }

    #[test]
    fn a_second_publish_reuses_the_stage_directory_without_a_stale_file_surviving() {
        let d = tempfile::tempdir().unwrap();
        let chain_tree = d.path().join("chain");
        let stage_dir = d.path().join("stage");
        let origin = d.path().join("origin");

        let old_iso = d.path().join("old.iso");
        std::fs::write(&old_iso, b"old disc bytes").unwrap();
        write_chain(&chain_tree, hash_file(&old_iso).unwrap().as_str());
        let r1 = req(&old_iso, &chain_tree, &stage_dir, &origin);
        assert!(run(&r1).0.is_done());

        let new_iso = d.path().join("new.iso");
        std::fs::write(&new_iso, b"a different, newer disc").unwrap();
        write_chain(&chain_tree, hash_file(&new_iso).unwrap().as_str());
        let mut r2 = req(&new_iso, &chain_tree, &stage_dir, &origin);
        r2.version = "2026.09.14";
        let (outcome, lines) = run(&r2);
        assert!(outcome.is_done(), "{lines:?}");

        let staged = stage_dir.join(ISO_FILE_NAME);
        assert_eq!(hash_file(&staged).unwrap(), hash_file(&new_iso).unwrap());
    }
}
