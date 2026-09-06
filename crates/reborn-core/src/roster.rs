//! Roster updates: the site's `.rebornroster`, applied to the live image.
//!
//! A roster change edits `template.dat` inside the disc image. Republishing
//! the image for that would cost every player 1.4 GB per roster; the
//! `.rebornroster` the site exports is half a megabyte, or thirty with
//! portraits. So the roster travels as its own small component and the
//! image is rebuilt on the player's machine by the Reborn Builder, which
//! copies the image and patches byte ranges in the copy. Measured on Adam's
//! machine: 3.0 seconds for the full apply.
//!
//! This module drives that tool. It does not parse a roster and does not
//! know the disc format; the Builder is the authority on both and is called
//! as a program, so a change to the roster format is a Builder release and
//! not a launcher one.
//!
//! # What must hold
//!
//! - **The live image is never written in place.** The Builder writes a new
//!   file into staging; it is hashed and measured; then one rename swaps it
//!   in. Dolphin holding the image open refuses the whole thing up front.
//! - **An unchanged image is not a success.** The build tool's own rule.
//!   Here it means "this roster is already in the image", which is reported
//!   as [`ApplyOutcome::Unchanged`] and changes nothing, rather than a swap
//!   of identical bytes that would look like an update.
//! - **The rebuilt image is recorded before it goes live.** The offline
//!   image is a customizable component: verification treats a file whose
//!   hash matches the recorded rebuild as the player's own, and anything
//!   else as damage to repair by re-download. Recording first, and undoing
//!   the record if the swap fails, means the image is never in a state the
//!   verifier would "repair" by downloading 1.4 GB over a roster.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::library::{hash_file, RebuildState};
use crate::manifest::Sha256Hex;
use crate::platform::{DataDir, Error, Platform, Result};

/// The one file the roster component ships.
pub const ROSTER_FILE: &str = "roster.rebornroster";
/// The Builder's console build, without the platform's executable suffix.
pub const BUILDER_NAME: &str = "reborn-builder-cli";
/// The offline image's name inside `library/offline`.
pub const IMAGE_FILE: &str = "NSR.iso";

/// What the Builder said about a build, read from its output.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BuilderReport {
    pub players: u64,
    pub teams: u64,
    pub portraits: u64,
    /// The base label the Builder recognized, for example `2026.08.28`.
    pub base: Option<String>,
    /// The site's warnings, one per line, exactly as printed.
    pub warnings: Vec<String>,
}

/// Read the Builder's report out of its output.
///
/// The lines it prints, as observed:
///
/// ```text
/// Done
/// 569 players, 51 teams, 0 portraits written.
/// out.iso - base 2026.08.28 - sha256 e97ca44c
/// 113 warning(s) from the site:
///   Odell Beckham Jr.: Jersey #3 is already worn by Abdul Carter on this team.
/// ```
///
/// Anything unrecognized is ignored, so a new line in a future Builder does
/// not break the launcher; the fields it did not find stay at their defaults
/// and the caller treats a zero player count as "no report", not as a roster
/// with nobody in it.
pub fn parse_report(text: &str) -> BuilderReport {
    let mut report = BuilderReport::default();
    let mut in_warnings = false;
    for raw in text.lines() {
        let line = raw.trim_end();
        if in_warnings {
            if let Some(w) = line.strip_prefix("  ") {
                report.warnings.push(w.trim().to_string());
                continue;
            }
            in_warnings = false;
        }
        if line.contains(" players, ") && line.contains(" teams, ") {
            let mut parts = line.split(", ");
            report.players = leading_number(parts.next());
            report.teams = leading_number(parts.next());
            report.portraits = leading_number(parts.next());
        } else if let Some(rest) = line.split(" - base ").nth(1) {
            report.base = rest.split(" - ").next().map(|s| s.trim().to_string());
        } else if line.contains("warning(s) from the site") {
            in_warnings = true;
        }
    }
    report
}

fn leading_number(s: Option<&str>) -> u64 {
    s.unwrap_or("")
        .trim()
        .split_whitespace()
        .next()
        .and_then(|n| n.parse().ok())
        .unwrap_or(0)
}

/// The first directory holding the Builder's console build.
pub fn find_builder(platform: &Platform, dirs: &[PathBuf]) -> Option<PathBuf> {
    let name = format!("{BUILDER_NAME}{}", platform.paths.exe_suffix());
    dirs.iter()
        .map(|d| d.join(&name))
        .find(|p| p.is_file())
}

/// Where the Builder is looked for: the install root's `tools` folder, the
/// install root itself, then whatever the caller adds (the 2.1 tools folder,
/// found by the caller because the core reads no environment variables).
/// An install root that cannot be resolved contributes nothing rather than
/// failing the search.
pub fn builder_dirs(platform: &Platform, extra: &[PathBuf]) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Ok(install) = platform.paths.install_root() {
        dirs.push(install.join("tools"));
        dirs.push(install);
    }
    dirs.extend(extra.iter().cloned());
    dirs
}

/// One request to apply a roster to the live image.
#[derive(Debug)]
pub struct ApplyRequest<'a> {
    pub data_root: &'a Path,
    /// The `.rebornroster` to apply.
    pub roster: &'a Path,
    /// The Builder's console build.
    pub builder: &'a Path,
    /// Where the Builder writes portraits. `None` sends them to a scratch
    /// directory under staging, so a run that only proves the image never
    /// touches a real texture folder.
    pub textures_root: Option<&'a Path>,
    /// Build even when the site flagged problems. The launcher passes true
    /// and shows the warnings; a build that stops on a warning nobody can
    /// see is a build nobody gets.
    pub accept_warnings: bool,
}

/// What an applied roster left behind.
#[derive(Debug, Clone)]
pub struct ApplyReceipt {
    pub image: PathBuf,
    pub before: Sha256Hex,
    pub after: Sha256Hex,
    pub report: BuilderReport,
}

/// Why nothing was applied. Nothing was changed in any of these.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplyRefusal {
    NoRoster { path: PathBuf },
    NoBuilder { path: PathBuf },
    NoImage { dir: PathBuf },
    SeveralImages { dir: PathBuf, found: Vec<String> },
    /// The game has the image open.
    ImageInUse { path: PathBuf },
    CrossVolume { staging: PathBuf, live: PathBuf },
    /// The Builder did not finish. `tail` is the end of what it printed.
    BuilderFailed { exit: String, tail: String },
    /// The Builder finished but the file it wrote is the wrong length. A
    /// range patch cannot change the length, so this is a broken write.
    Truncated { expected: u64, got: u64 },
}

impl ApplyRefusal {
    /// What failed, what remains safe, what to do. The live image is
    /// untouched in every case, and the message says so.
    pub fn message(&self) -> String {
        match self {
            Self::NoRoster { path } => format!(
                "no roster at {}. The game is as it was.",
                path.display()
            ),
            Self::NoBuilder { path } => format!(
                "the Reborn Builder is not at {}. It applies the roster to the image and \
                 nothing here can do that without it. The game is as it was.",
                path.display()
            ),
            Self::NoImage { dir } => format!(
                "no game image in {}. A roster is applied to the installed image, and there \
                 is none to apply it to. The game is as it was.",
                dir.display()
            ),
            Self::SeveralImages { dir, found } => format!(
                "{} holds {} images ({}) and none is named {IMAGE_FILE}, so which one is the \
                 game is not knowable. Nothing was changed.",
                dir.display(),
                found.len(),
                found.join(", ")
            ),
            Self::ImageInUse { path } => format!(
                "{} is open, which means the game is running. The roster installs the next \
                 time the game is closed. Nothing was changed.",
                path.display()
            ),
            Self::CrossVolume { staging, live } => format!(
                "staging ({}) and the image ({}) are on different volumes, so the swap could \
                 not be one rename. Nothing was changed.",
                staging.display(),
                live.display()
            ),
            Self::BuilderFailed { exit, tail } => format!(
                "the Reborn Builder stopped ({exit}) before writing an image. Nothing was \
                 changed; the game is as it was. It said:\n{tail}"
            ),
            Self::Truncated { expected, got } => format!(
                "the Builder wrote {got} bytes where the image is {expected}. That file was \
                 discarded and the game is as it was."
            ),
        }
    }
}

/// The result of an apply.
#[derive(Debug)]
pub enum ApplyOutcome {
    Applied(ApplyReceipt),
    /// The Builder ran and produced the same bytes: this roster is already
    /// in the image. Nothing was swapped.
    Unchanged { image: PathBuf, sha256: Sha256Hex, report: BuilderReport },
    Refused(ApplyRefusal),
}

/// The live offline image: `NSR.iso`, or the only image in the folder.
pub fn find_image(offline_dir: &Path) -> std::result::Result<PathBuf, ApplyRefusal> {
    let named = offline_dir.join(IMAGE_FILE);
    if named.is_file() {
        return Ok(named);
    }
    let mut found: Vec<PathBuf> = match std::fs::read_dir(offline_dir) {
        Ok(entries) => entries
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.is_file()
                    && p
                        .extension()
                        .and_then(|x| x.to_str())
                        .map(|x| x.eq_ignore_ascii_case("iso"))
                        .unwrap_or(false)
            })
            .collect(),
        Err(_) => Vec::new(),
    };
    found.sort();
    match found.len() {
        0 => Err(ApplyRefusal::NoImage { dir: offline_dir.to_path_buf() }),
        1 => Ok(found.remove(0)),
        _ => Err(ApplyRefusal::SeveralImages {
            dir: offline_dir.to_path_buf(),
            found: found
                .iter()
                .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
                .collect(),
        }),
    }
}

/// The image the launcher's game import built from the player's own disc:
/// `library/imported/<output hash>/NSR.iso`. The newest when there are
/// several, which happens after a game update re-imports.
pub fn find_imported_image(data_root: &Path) -> Option<PathBuf> {
    let imported = data_root.join("library").join("imported");
    let mut found: Vec<(std::time::SystemTime, PathBuf)> = std::fs::read_dir(&imported)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path().join(IMAGE_FILE))
        .filter(|p| p.is_file())
        .filter_map(|p| {
            let t = std::fs::metadata(&p).and_then(|m| m.modified()).ok()?;
            Some((t, p))
        })
        .collect();
    found.sort_by(|a, b| b.0.cmp(&a.0));
    found.into_iter().next().map(|(_, p)| p)
}

/// Apply a roster to the live image.
pub fn apply(platform: &Platform, req: &ApplyRequest<'_>) -> Result<ApplyOutcome> {
    if !req.roster.is_file() {
        return Ok(ApplyOutcome::Refused(ApplyRefusal::NoRoster { path: req.roster.to_path_buf() }));
    }
    if !req.builder.is_file() {
        return Ok(ApplyOutcome::Refused(ApplyRefusal::NoBuilder { path: req.builder.to_path_buf() }));
    }
    // Two places an image can be. `library/offline` is where the content
    // system delivers one. `library/imported/<hash>/NSR.iso` is where the
    // launcher's game import puts the image it built from the player's own
    // disc, which is the shipping model since 2026-09-06. The imported one
    // wins when both exist: it is the one the player made.
    let offline_dir = req.data_root.join(DataDir::LibraryOffline.relative());
    let live = match find_imported_image(req.data_root).or_else(|| find_image(&offline_dir).ok()) {
        Some(p) => p,
        None => match find_image(&offline_dir) {
            Err(r) => return Ok(ApplyOutcome::Refused(r)),
            Ok(p) => p,
        },
    };
    let offline_dir = live.parent().map(Path::to_path_buf).unwrap_or(offline_dir);
    if platform.files.is_locked(&live) {
        return Ok(ApplyOutcome::Refused(ApplyRefusal::ImageInUse { path: live }));
    }

    // Staging keyed by the roster's own hash, so two different rosters never
    // share a scratch file and a re-run of the same one lands in the same place.
    let roster_sha = hash_file(req.roster)?;
    let staging = req
        .data_root
        .join(DataDir::Staging.relative())
        .join("roster")
        .join(&roster_sha.as_str()[..12]);
    std::fs::create_dir_all(&staging).map_err(|source| Error::Io { path: staging.clone(), source })?;
    if !platform.files.same_volume(&staging, &offline_dir)? {
        return Ok(ApplyOutcome::Refused(ApplyRefusal::CrossVolume {
            staging,
            live: offline_dir,
        }));
    }
    let image_name = live
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_else(|| IMAGE_FILE.into());
    let out = staging.join(&image_name);
    if out.exists() {
        std::fs::remove_file(&out).map_err(|source| Error::Io { path: out.clone(), source })?;
    }
    let scratch_textures;
    let textures = match req.textures_root {
        Some(t) => t.to_path_buf(),
        None => {
            scratch_textures = staging.join("textures");
            scratch_textures.clone()
        }
    };
    std::fs::create_dir_all(&textures).map_err(|source| Error::Io { path: textures.clone(), source })?;

    let mut args: Vec<OsString> = vec![
        "--iso".into(),
        live.as_os_str().to_os_string(),
        "--update".into(),
        req.roster.as_os_str().to_os_string(),
        "--out".into(),
        out.as_os_str().to_os_string(),
        "--textures".into(),
        textures.as_os_str().to_os_string(),
    ];
    if req.accept_warnings {
        args.push("--yes".into());
    }
    let run = platform.processes.run_capturing(req.builder, &args, &Default::default())?;
    if !run.succeeded() {
        let _ = std::fs::remove_file(&out);
        let all = format!("{}\n{}", run.stdout, run.stderr);
        let tail: Vec<&str> = all.lines().filter(|l| !l.trim().is_empty()).collect();
        let tail = tail[tail.len().saturating_sub(12)..].join("\n");
        return Ok(ApplyOutcome::Refused(ApplyRefusal::BuilderFailed {
            exit: format!("{:?}", run.exit),
            tail,
        }));
    }
    let report = parse_report(&run.stdout);

    let expected = std::fs::metadata(&live).map_err(|source| Error::Io { path: live.clone(), source })?.len();
    let got = std::fs::metadata(&out).map_err(|source| Error::Io { path: out.clone(), source })?.len();
    if got != expected {
        let _ = std::fs::remove_file(&out);
        return Ok(ApplyOutcome::Refused(ApplyRefusal::Truncated { expected, got }));
    }

    let before = hash_file(&live)?;
    let after = hash_file(&out)?;
    if after == before {
        let _ = std::fs::remove_file(&out);
        return Ok(ApplyOutcome::Unchanged { image: live, sha256: before, report });
    }

    // Record first, swap second; undo the record if the swap fails. See the
    // module doc for why this order and not the other.
    let state_path = crate::music::music_root(req.data_root).join("state.json");
    let mut state = RebuildState::load(&state_path)?;
    let key = image_name.to_string_lossy().into_owned();
    let previous = state.rebuilt.get(&key).cloned();
    state.rebuilt.insert(key.clone(), after.clone());
    state.save(&state_path)?;

    if let Err(e) = platform.files.atomic_replace(&out, &live) {
        match previous {
            Some(p) => {
                state.rebuilt.insert(key, p);
            }
            None => {
                state.rebuilt.remove(&key);
            }
        }
        if let Err(undo) = state.save(&state_path) {
            tracing::warn!("could not undo the rebuild record after a failed swap: {undo}");
        }
        return Err(e);
    }

    Ok(ApplyOutcome::Applied(ApplyReceipt { image: live, before, after, report }))
}

#[cfg(test)]
mod tests {
    use super::*;

    const REPORT: &str = "\
Reading the roster
Done
569 players, 51 teams, 12 portraits written.
out.iso - base 2026.08.28 - sha256 e97ca44c
2 warning(s) from the site:
  Odell Beckham Jr.: Jersey #3 is already worn by Abdul Carter on this team.
  Rams: PGID 342 is vacant and will export as a blank row.
log: C:\\somewhere\\build-log.txt
";

    #[test]
    fn the_builders_report_is_read_line_by_line() {
        let r = parse_report(REPORT);
        assert_eq!((r.players, r.teams, r.portraits), (569, 51, 12));
        assert_eq!(r.base.as_deref(), Some("2026.08.28"));
        assert_eq!(r.warnings.len(), 2);
        assert!(r.warnings[0].starts_with("Odell Beckham Jr.:"));
        assert!(r.warnings[1].starts_with("Rams:"), "the warning block ends at the first unindented line");
    }

    #[test]
    fn an_unfamiliar_report_yields_defaults_rather_than_a_panic() {
        let r = parse_report("something new\nentirely\n");
        assert_eq!(r, BuilderReport::default());
        let r = parse_report("x players, y teams, z portraits written.");
        assert_eq!((r.players, r.teams, r.portraits), (0, 0, 0));
    }

    #[test]
    fn the_image_is_nsr_iso_or_the_only_iso_and_never_a_guess_between_two() {
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(find_image(dir.path()), Err(ApplyRefusal::NoImage { .. })));

        std::fs::write(dir.path().join("Whatever.iso"), b"x").unwrap();
        assert_eq!(find_image(dir.path()).unwrap(), dir.path().join("Whatever.iso"));

        std::fs::write(dir.path().join("Other.iso"), b"y").unwrap();
        match find_image(dir.path()) {
            Err(ApplyRefusal::SeveralImages { found, .. }) => assert_eq!(found.len(), 2),
            other => panic!("two unnamed images must not pick one: {other:?}"),
        }

        std::fs::write(dir.path().join(IMAGE_FILE), b"z").unwrap();
        assert_eq!(find_image(dir.path()).unwrap(), dir.path().join(IMAGE_FILE), "the named one wins");
    }

    #[test]
    fn the_imported_image_is_found_and_the_newest_wins() {
        // The game import puts the player's patched disc at
        // library/imported/<hash>/NSR.iso, not library/offline. Before this,
        // roster apply on a real install said "no game image".
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(find_imported_image(dir.path()), None, "no imported folder at all");

        let older = dir.path().join("library/imported/aaaa");
        let newer = dir.path().join("library/imported/bbbb");
        std::fs::create_dir_all(&older).unwrap();
        std::fs::create_dir_all(&newer).unwrap();
        std::fs::write(older.join(IMAGE_FILE), b"old build").unwrap();
        std::fs::write(newer.join(IMAGE_FILE), b"new build").unwrap();
        // Make the timestamps unambiguous whatever the filesystem resolution.
        let then = std::time::SystemTime::now() - std::time::Duration::from_secs(600);
        std::fs::File::options().write(true).open(older.join(IMAGE_FILE)).unwrap().set_modified(then).unwrap();

        assert_eq!(find_imported_image(dir.path()), Some(newer.join(IMAGE_FILE)), "the newest import wins");

        // A folder without the image inside does not count.
        std::fs::create_dir_all(dir.path().join("library/imported/cccc")).unwrap();
        assert_eq!(find_imported_image(dir.path()), Some(newer.join(IMAGE_FILE)));
    }

    #[test]
    fn find_builder_takes_the_first_directory_that_has_it() {
        let platform = Platform::detect();
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a");
        let b = dir.path().join("b");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        let name = format!("{BUILDER_NAME}{}", platform.paths.exe_suffix());
        assert_eq!(find_builder(&platform, &[a.clone(), b.clone()]), None);
        std::fs::write(b.join(&name), b"").unwrap();
        assert_eq!(find_builder(&platform, &[a.clone(), b.clone()]), Some(b.join(&name)));
    }

    #[test]
    fn every_refusal_names_what_remains_safe() {
        let refusals = [
            ApplyRefusal::NoRoster { path: "r".into() },
            ApplyRefusal::NoBuilder { path: "b".into() },
            ApplyRefusal::NoImage { dir: "d".into() },
            ApplyRefusal::SeveralImages { dir: "d".into(), found: vec!["a.iso".into(), "b.iso".into()] },
            ApplyRefusal::ImageInUse { path: "i".into() },
            ApplyRefusal::CrossVolume { staging: "s".into(), live: "l".into() },
            ApplyRefusal::BuilderFailed { exit: "Error { code: 4 }".into(), tail: "boom".into() },
            ApplyRefusal::Truncated { expected: 10, got: 9 },
        ];
        for r in refusals {
            let m = r.message();
            assert!(
                m.contains("as it was") || m.contains("Nothing was changed"),
                "{r:?}: {m}"
            );
        }
    }

    #[test]
    fn apply_refuses_before_running_anything_when_the_inputs_are_missing() {
        let platform = Platform::detect();
        let dir = tempfile::tempdir().unwrap();
        let roster = dir.path().join("r.rebornroster");
        let builder = dir.path().join("builder.exe");
        fn req<'a>(root: &'a Path, roster: &'a Path, builder: &'a Path) -> ApplyRequest<'a> {
            ApplyRequest {
                data_root: root,
                roster,
                builder,
                textures_root: None,
                accept_warnings: true,
            }
        }
        assert!(matches!(
            apply(&platform, &req(dir.path(), &roster, &builder)).unwrap(),
            ApplyOutcome::Refused(ApplyRefusal::NoRoster { .. })
        ));
        std::fs::write(&roster, b"roster").unwrap();
        assert!(matches!(
            apply(&platform, &req(dir.path(), &roster, &builder)).unwrap(),
            ApplyOutcome::Refused(ApplyRefusal::NoBuilder { .. })
        ));
        std::fs::write(&builder, b"not really").unwrap();
        assert!(matches!(
            apply(&platform, &req(dir.path(), &roster, &builder)).unwrap(),
            ApplyOutcome::Refused(ApplyRefusal::NoImage { .. })
        ));
    }
}
