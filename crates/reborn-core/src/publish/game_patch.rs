//! A game update: the xdelta from the build players have to the new build,
//! appended to the chain, then a release carrying every other pin forward.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::game_patch::{find_xdelta, Chain, Patcher, Step, Xdelta3, BUNDLE_DIR, CHAIN_FILE};
use crate::inventory::{ComponentId, ComponentSpec, ScanObserver};
use crate::library::hash_file;
use crate::publish::component::{publish_component, ComponentRequest};
use crate::publish::release::{republish_pinning, CarryForward};
use crate::publish::roster::Push;
use crate::publish::{io_err, next_sequence, out, refuse, thousands, Outcome, Sink};
use crate::{Platform, Result};

#[derive(Debug)]
pub struct GamePatchRequest<'a> {
    /// The pristine image players have now: the head of the chain.
    pub from: &'a Path,
    /// The new pristine build.
    pub to: &'a Path,
    /// The build's label, for example 2026.09.07.
    pub version: &'a str,
    /// The tree the chain accumulates in. Defaults to `<origin>-stage\game-patch`.
    pub tree: Option<&'a Path>,
    /// xdelta3. Found in the import bundle when absent.
    pub xdelta: Option<&'a Path>,
    pub origin: &'a Path,
    pub sequence: Option<u64>,
    pub key: Option<&'a Path>,
    pub release_version: Option<&'a str>,
    pub release_sequence: Option<u64>,
    /// Encode, verify, write and sign. Without it both images are hashed and
    /// the work is described; no delta is made.
    pub apply: bool,
}

/// Removes the decode-and-verify check file when it drops. Covers every path
/// out of the decode-and-hash step that runs before the explicit `drop`
/// below: an early `?` from [`Patcher::decode`], a panic, or the ordinary
/// continuation. Dropped explicitly right after the hash comparison so the
/// file (a full decoded game image) does not sit on disk for the rest of the
/// call; kept as a guard rather than a plain `remove_file` so the early-exit
/// paths above it are covered too.
struct CheckGuard(PathBuf);

impl Drop for CheckGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Publish a game update: encode the delta, verify it decodes to the new
/// build, append it to the chain, publish the chain as the `game-patch`
/// component, and re-sign the release with every other pin carried forward.
pub fn publish_game_patch(platform: &Platform, req: &GamePatchRequest<'_>, observer: &dyn ScanObserver, sink: Sink<'_>) -> Result<Outcome<Push>> {
    let (from, to, version, origin, apply) = (req.from, req.to, req.version, req.origin, req.apply);

    for (what, p) in [("--from", from), ("--to", to)] {
        if !p.is_file() {
            return Ok(refuse(sink, &[format!("{what} {} is not a file. Nothing was published.", p.display())]));
        }
    }
    let Some(xdelta) = find_xdelta_for_publishing(platform, req.xdelta) else {
        return Ok(refuse(sink, &[
            "xdelta3 was not found. Pass --xdelta <path to xdelta3.exe>; the import bundle".to_string(),
            "under target\\installer-payload\\game-import holds one. Nothing was published.".to_string(),
        ]));
    };
    let tree: PathBuf = match req.tree {
        Some(t) => t.to_path_buf(),
        None => {
            let name = origin
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "origin".to_string());
            origin.with_file_name(format!("{name}-stage")).join("game-patch")
        }
    };
    let chain_path = tree.join(CHAIN_FILE);
    let mut chain = Chain::load(&chain_path)?.unwrap_or_default();

    out!(sink, "From          {}", from.display());
    out!(sink, "To            {}", to.display());
    out!(sink, "Tool          {}", xdelta.display());
    out!(sink, "Chain         {} ({} step(s) so far)", chain_path.display(), chain.steps.len());
    out!(sink, "Origin        {}", origin.display());
    out!(sink);
    out!(sink, "Hashing both images.");
    let started = Instant::now();
    let from_sha = hash_file(from)?;
    let to_sha = hash_file(to)?;
    let to_bytes = std::fs::metadata(to).map_err(|e| io_err(to, e))?.len();
    out!(sink, "From is       {from_sha}");
    out!(sink, "To is         {to_sha}  ({} bytes), hashed in {:.1} s", thousands(to_bytes), started.elapsed().as_secs_f64());
    if from_sha == to_sha {
        return Ok(refuse(sink, &[String::new(), "Those are the same build. Nothing was published.".to_string()]));
    }
    if let Some(head) = chain.head() {
        if head != from_sha.as_str() {
            return Ok(refuse(sink, &[
                String::new(),
                format!("The chain ends at {}… and --from is {}…. A game patch is always made from the", &head[..12], &from_sha.as_str()[..12]),
                "build players have, which is the head of the chain. Nothing was published.".to_string(),
            ]));
        }
    }
    if chain.steps.iter().any(|s| s.to_sha256 == to_sha.as_str() || s.from_sha256 == to_sha.as_str()) {
        return Ok(refuse(sink, &[String::new(), "That build is already in the chain. Nothing was published.".to_string()]));
    }
    let patch_rel = Chain::patch_name(from_sha.as_str(), to_sha.as_str());
    let patch_path = tree.join(patch_rel.replace('/', std::path::MAIN_SEPARATOR_STR));
    let step = Step {
        from_sha256: from_sha.as_str().to_string(),
        to_sha256: to_sha.as_str().to_string(),
        to_bytes,
        patch: patch_rel.clone(),
        label: version.to_string(),
    };
    out!(sink);
    out!(sink, "Step          {} -> {} as {version}", &from_sha.as_str()[..12], &to_sha.as_str()[..12]);
    out!(sink, "Patch         {}", patch_path.display());
    if !apply {
        out!(sink);
        out!(sink, "Nothing was encoded or written. Re-run with --apply to make the delta, verify it,");
        out!(sink, "append it to the chain, publish the component and re-sign the release.");
        return Ok(Outcome::Described);
    }

    if let Some(parent) = patch_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| io_err(parent, e))?;
    }
    out!(sink);
    out!(sink, "Encoding the delta. This reads both images once.");
    let started = Instant::now();
    let args: Vec<OsString> = vec![
        "-e".into(),
        "-f".into(),
        "-B".into(),
        "268435456".into(),
        "-s".into(),
        from.as_os_str().to_os_string(),
        to.as_os_str().to_os_string(),
        patch_path.as_os_str().to_os_string(),
    ];
    let run = platform.processes.run_capturing(&xdelta, &args, &Default::default())?;
    if !run.succeeded() {
        let _ = std::fs::remove_file(&patch_path);
        // One entry per line: `Event::line` is one human line (see its doc
        // comment), and a GUI renders one event per row.
        return Ok(refuse(sink, &[
            format!("xdelta3 failed ({:?}):", run.exit),
            run.stdout,
            run.stderr,
            "Nothing was published.".to_string(),
        ]));
    }
    let delta_bytes = std::fs::metadata(&patch_path).map_err(|e| io_err(&patch_path, e))?.len();
    out!(sink, "Delta         {} bytes in {:.1} s", thousands(delta_bytes), started.elapsed().as_secs_f64());

    // Decode it back before anyone else does. A delta that does not
    // reproduce the new build byte for byte would be served to every player.
    out!(sink, "Verifying     decoding the delta over the old build and hashing the result.");
    let started = Instant::now();
    let check = std::env::temp_dir().join("reborn-publish").join(format!("game-patch-check-{}.iso", &to_sha.as_str()[..12]));
    if let Some(parent) = check.parent() {
        std::fs::create_dir_all(parent).map_err(|e| io_err(parent, e))?;
    }
    // Covers every return between here and the explicit drop below,
    // including a `?` propagation from `Patcher::decode`.
    let check_guard = CheckGuard(check.clone());
    let patcher = Xdelta3::new(platform, xdelta.clone());
    let decoded = Patcher::decode(&patcher, from, &patch_path, &check)?;
    let verified = decoded.succeeded && hash_file(&check).map(|h| h == to_sha).unwrap_or(false);
    // Dropped here rather than left for the function to return: `check` can
    // be a full decoded game image, and nothing from here on (the chain
    // save, the sequence lookup, the component publish, the release
    // re-sign) needs it on disk. Do not remove this drop to "simplify" the
    // guard away: without it the file sits for the rest of the call.
    drop(check_guard);
    if !verified {
        let _ = std::fs::remove_file(&patch_path);
        return Ok(refuse(sink, &[format!(
            "The delta did not reproduce the new build ({}). It was deleted. Nothing was published.",
            decoded.tail
        )]));
    }
    out!(sink, "Verified      in {:.1} s", started.elapsed().as_secs_f64());

    chain.push(step)?;
    chain.save(&chain_path)?;
    out!(sink, "Chain         now {} step(s), head {}…", chain.steps.len(), &to_sha.as_str()[..12]);

    let sequence = match req.sequence {
        Some(s) => s,
        None => {
            let next = next_sequence(origin, ComponentId::GamePatch)?;
            out!(sink, "Sequence      {next} (one above the highest game-patch already in the origin)");
            next
        }
    };
    out!(sink);
    let spec = match ComponentSpec::new(ComponentId::GamePatch, version, sequence) {
        Ok(s) => s,
        Err(e) => return Ok(refuse(sink, &[e.to_string()])),
    };
    let component = match publish_component(&ComponentRequest { root: &tree, spec, origin, apply: true }, observer, sink)? {
        Outcome::Refused(r) => return Ok(Outcome::Refused(r)),
        Outcome::Described => {
            let carry = CarryForward {
                origin,
                id: ComponentId::GamePatch,
                version,
                key: req.key,
                release_version: req.release_version,
                release_sequence: req.release_sequence,
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
        origin,
        id: ComponentId::GamePatch,
        version,
        key: req.key,
        release_version: req.release_version,
        release_sequence: req.release_sequence,
        apply: true,
    };
    Ok(match republish_pinning(&carry, sink)? {
        Outcome::Done(s) => Outcome::Done(Push { component, release: Some(s) }),
        Outcome::Described => Outcome::Done(Push { component, release: None }),
        Outcome::Refused(r) => Outcome::Refused(r),
    })
}

/// xdelta3 for the publisher: as given, the repository's import bundle when
/// run from the repository, or the bundle beside rebornctl.
pub fn find_xdelta_for_publishing(platform: &Platform, given: Option<&Path>) -> Option<PathBuf> {
    if let Some(p) = given {
        return p.is_file().then(|| p.to_path_buf());
    }
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        // The staged bundle under target: the usual folder first, then any
        // timestamped sibling build.ps1 staged in when that one was in use.
        let target = cwd.join("target");
        dirs.push(target.join("installer-payload").join(BUNDLE_DIR));
        if let Ok(entries) = std::fs::read_dir(&target) {
            let mut siblings: Vec<PathBuf> = entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.is_dir()
                        && p.file_name()
                            .and_then(|n| n.to_str())
                            .map(|n| n.starts_with("installer-payload-") && !n.ends_with("launcher"))
                            .unwrap_or(false)
                })
                .collect();
            siblings.sort();
            siblings.reverse();
            dirs.extend(siblings.into_iter().map(|p| p.join(BUNDLE_DIR)));
        }
    }
    if let Some(exe_dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)) {
        dirs.push(exe_dir.join(BUNDLE_DIR));
        dirs.push(exe_dir);
    }
    find_xdelta(platform, &dirs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::RunToCompletion;
    use crate::publish::{Event, Outcome};

    fn req<'a>(from: &'a Path, to: &'a Path, origin: &'a Path, tree: &'a Path, xdelta: &'a Path) -> GamePatchRequest<'a> {
        GamePatchRequest {
            from, to, version: "2026.09.07", tree: Some(tree), xdelta: Some(xdelta), origin,
            sequence: None, key: None, release_version: None, release_sequence: None, apply: true,
        }
    }

    fn run(r: &GamePatchRequest<'_>) -> (Outcome<Push>, Vec<String>) {
        let platform = crate::Platform::detect();
        let mut lines = Vec::new();
        (publish_game_patch(&platform, r, &RunToCompletion, &mut |e: Event| lines.push(e.line)).unwrap(), lines)
    }

    #[test]
    fn a_missing_image_is_refused_before_anything_else() {
        let d = tempfile::tempdir().unwrap();
        let to = d.path().join("b.iso");
        std::fs::write(&to, b"B").unwrap();
        let fake_xdelta = d.path().join("xdelta3.exe");
        std::fs::write(&fake_xdelta, b"").unwrap();
        let missing = d.path().join("missing.iso");
        let origin = d.path().join("origin");
        let stage = d.path().join("stage");
        let r = req(&missing, &to, &origin, &stage, &fake_xdelta);
        let (outcome, lines) = run(&r);
        assert!(outcome.refused().is_some());
        assert!(lines.iter().any(|l| l.contains("--from") && l.contains("is not a file")), "{lines:?}");
    }

    #[test]
    fn the_same_build_twice_is_refused() {
        let d = tempfile::tempdir().unwrap();
        let a = d.path().join("a.iso");
        std::fs::write(&a, b"same").unwrap();
        let fake_xdelta = d.path().join("xdelta3.exe");
        std::fs::write(&fake_xdelta, b"").unwrap();
        let origin = d.path().join("origin");
        let stage = d.path().join("stage");
        let r = req(&a, &a, &origin, &stage, &fake_xdelta);
        let (outcome, lines) = run(&r);
        assert!(outcome.refused().is_some());
        assert!(lines.iter().any(|l| l.contains("Those are the same build")), "{lines:?}");
    }

    #[test]
    fn xdelta3_not_found_is_refused_before_a_process_runs() {
        let d = tempfile::tempdir().unwrap();
        let a = d.path().join("a.iso");
        let b = d.path().join("b.iso");
        std::fs::write(&a, b"A").unwrap();
        std::fs::write(&b, b"B").unwrap();
        let origin = d.path().join("origin");
        let stage = d.path().join("stage");
        // A path that is not a file: find_xdelta_for_publishing refuses to
        // fall back to a search when one was given explicitly.
        let missing_xdelta = d.path().join("nope.exe");
        let r = req(&a, &b, &origin, &stage, &missing_xdelta);
        let (outcome, lines) = run(&r);
        assert!(outcome.refused().is_some());
        assert!(lines.iter().any(|l| l.contains("xdelta3 was not found")), "{lines:?}");
    }

    #[test]
    fn a_build_already_in_the_chain_is_refused() {
        let d = tempfile::tempdir().unwrap();
        let a = d.path().join("a.iso");
        let b = d.path().join("b.iso");
        std::fs::write(&a, b"A").unwrap();
        std::fs::write(&b, b"B").unwrap();
        let from_sha = hash_file(&a).unwrap();
        let to_sha = hash_file(&b).unwrap();
        let stage = d.path().join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        // A chain whose head is `a` (so the head-of-chain check passes) but
        // which already carries `b`'s hash earlier in it (so the
        // already-in-the-chain check is the one that fires). Two steps,
        // linear: to_sha -> mid -> from_sha.
        let mid = "2".repeat(64);
        let mut chain = Chain::new();
        chain.push(Step {
            from_sha256: to_sha.as_str().to_string(), to_sha256: mid.clone(), to_bytes: 1,
            patch: Chain::patch_name(to_sha.as_str(), &mid), label: "old1".into(),
        }).unwrap();
        chain.push(Step {
            from_sha256: mid.clone(), to_sha256: from_sha.as_str().to_string(), to_bytes: 1,
            patch: Chain::patch_name(&mid, from_sha.as_str()), label: "old2".into(),
        }).unwrap();
        chain.save(&stage.join(CHAIN_FILE)).unwrap();
        let fake_xdelta = d.path().join("xdelta3.exe");
        std::fs::write(&fake_xdelta, b"").unwrap();
        let origin = d.path().join("origin");
        let r = req(&a, &b, &origin, &stage, &fake_xdelta);
        let (outcome, lines) = run(&r);
        assert!(outcome.refused().is_some());
        assert!(lines.iter().any(|l| l.contains("already in the chain")), "{lines:?}");
    }

    #[test]
    fn a_from_that_is_not_the_chain_head_is_refused() {
        let d = tempfile::tempdir().unwrap();
        let a = d.path().join("a.iso");
        let b = d.path().join("b.iso");
        std::fs::write(&a, b"A").unwrap();
        std::fs::write(&b, b"B").unwrap();
        let stage = d.path().join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        // A chain whose head is some other build.
        let mut chain = Chain::new();
        chain.push(Step {
            from_sha256: "0".repeat(64), to_sha256: "1".repeat(64), to_bytes: 1,
            patch: Chain::patch_name(&"0".repeat(64), &"1".repeat(64)), label: "old".into(),
        }).unwrap();
        chain.save(&stage.join(CHAIN_FILE)).unwrap();
        let fake_xdelta = d.path().join("xdelta3.exe");
        std::fs::write(&fake_xdelta, b"").unwrap();
        let origin = d.path().join("origin");
        let r = req(&a, &b, &origin, &stage, &fake_xdelta);
        let (outcome, lines) = run(&r);
        assert!(outcome.refused().is_some());
        assert!(lines.iter().any(|l| l.contains("A game patch is always made from the")), "{lines:?}");
    }

    #[test]
    fn a_dry_run_describes_the_step_and_encodes_nothing() {
        let d = tempfile::tempdir().unwrap();
        let a = d.path().join("a.iso");
        let b = d.path().join("b.iso");
        std::fs::write(&a, b"A").unwrap();
        std::fs::write(&b, b"B").unwrap();
        let fake_xdelta = d.path().join("xdelta3.exe");
        std::fs::write(&fake_xdelta, b"").unwrap();
        let stage = d.path().join("stage");
        let origin = d.path().join("origin");
        let mut r = req(&a, &b, &origin, &stage, &fake_xdelta);
        r.apply = false;
        let (outcome, lines) = run(&r);
        assert!(matches!(outcome, Outcome::Described), "{outcome:?}");
        assert!(lines.iter().any(|l| l.starts_with("Step          ")), "{lines:?}");
        assert!(!stage.join(CHAIN_FILE).exists());
    }

    #[test]
    fn xdelta_is_found_where_given_and_only_where_a_file_is() {
        let d = tempfile::tempdir().unwrap();
        let platform = crate::Platform::detect();
        let there = d.path().join("xdelta3.exe");
        std::fs::write(&there, b"").unwrap();
        assert_eq!(find_xdelta_for_publishing(&platform, Some(&there)), Some(there.clone()));
        assert_eq!(find_xdelta_for_publishing(&platform, Some(&d.path().join("nope.exe"))), None);
    }
}
