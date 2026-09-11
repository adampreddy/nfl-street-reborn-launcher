//! Memory-card gear sync: the Builder's `card-sync` verb, run over every card
//! in the Dolphin user folder after a roster apply (full) and after every game
//! session (harvest only).
//!
//! The launcher never parses a card or a roster. The Builder is the authority
//! on both and is called as a program, exactly as `roster::apply` does, and
//! its report lines are read here. A line this module does not know is
//! ignored, so a newer Builder never breaks an older launcher.
//!
//! # What must hold
//!
//! - **A card the game holds open is refused, never written.**
//! - **A card is copied before it is written.** Under `recovery/cards/`,
//!   newest three kept. Sixteen megabytes, cheap enough to take every time.
//! - **Nothing here fails a launch or an update.** Every bad state is a
//!   variant in the result whose sentence names what remains as it was.
//! - **A full run costs nothing when nothing changed.** After a full run the
//!   card's and the roster's hashes are kept beside the Builder's state; a
//!   later full run that finds both unchanged does not start the Builder.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::library::hash_file;
use crate::platform::{DataDir, Error, Platform, Result};

/// Under `library/roster`: one JSON file per card, owned by the Builder.
pub const STATE_DIR: &str = "cards";
/// Under `recovery`: copies of cards taken before they were written.
pub const RECOVERY_DIR: &str = "cards";
/// How many recovery copies to keep per card.
pub const KEEP_RECOVERY: usize = 3;
/// Beside the Builder's state file: what the last full run saw.
pub const SEEN_SUFFIX: &str = ".seen.json";

/// The memory cards in a Dolphin user folder, sorted by name.
///
/// The convention is the one `session::user_dir_score` already relies on:
/// `GC/memorycard*.raw`, case-insensitive.
pub fn memory_cards(user_dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(user_dir.join("GC")) {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_lowercase();
            if name.starts_with("memorycard") && name.ends_with(".raw") && e.path().is_file() {
                out.push(e.path());
            }
        }
    }
    out.sort();
    out
}

fn file_name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

fn state_dir(data_root: &Path) -> PathBuf {
    data_root.join(DataDir::LibraryRoster.relative()).join(STATE_DIR)
}

/// Where the Builder keeps what it last wrote to this card.
pub fn state_path(data_root: &Path, card: &Path) -> PathBuf {
    state_dir(data_root).join(format!("{}.json", file_name(card)))
}

/// Where the launcher keeps what its last full run saw of this card.
pub fn seen_path(data_root: &Path, card: &Path) -> PathBuf {
    state_dir(data_root).join(format!("{}{SEEN_SUFFIX}", file_name(card)))
}

fn recovery_dir(data_root: &Path) -> PathBuf {
    data_root.join(DataDir::Recovery.relative()).join(RECOVERY_DIR)
}

/// Copy a card before it is written, and keep only the newest
/// [`KEEP_RECOVERY`] copies of it.
pub fn recovery_point(data_root: &Path, card: &Path, stamp: u64) -> Result<PathBuf> {
    let dir = recovery_dir(data_root);
    std::fs::create_dir_all(&dir).map_err(|source| Error::Io { path: dir.clone(), source })?;
    let name = file_name(card);
    let dest = dir.join(format!("{name}.{stamp}.bak"));
    std::fs::copy(card, &dest).map_err(|source| Error::Io { path: dest.clone(), source })?;

    let prefix = format!("{name}.");
    let mut copies: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map_err(|source| Error::Io { path: dir.clone(), source })?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            let n = file_name(p);
            n.starts_with(&prefix) && n.ends_with(".bak")
        })
        .collect();
    copies.sort();
    while copies.len() > KEEP_RECOVERY {
        let old = copies.remove(0);
        // Best effort: a copy that will not delete is not a reason to stop.
        let _ = std::fs::remove_file(&old);
    }
    Ok(dest)
}

/// What a full run last saw of one card. When a later full run finds the
/// same card bytes and the same roster file, the Builder has nothing new to
/// say and is not started.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Seen {
    pub card_sha256: String,
    pub roster_sha256: String,
    /// Unix seconds.
    pub at: u64,
}

fn read_seen(path: &Path) -> Option<Seen> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

fn write_seen(path: &Path, seen: &Seen) -> Result<()> {
    let text = serde_json::to_string_pretty(seen)
        .map_err(|e| Error::Io { path: path.to_path_buf(), source: std::io::Error::other(e) })?;
    std::fs::write(path, text).map_err(|source| Error::Io { path: path.to_path_buf(), source })
}

/// One `profile ...` line of the Builder's report.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProfileLine {
    pub profile: String,
    pub bootstrap: bool,
    pub updated: u64,
    pub harvested: u64,
    pub carried: u64,
    pub skipped: u64,
    /// Set when the Builder skipped the whole profile, with its reason.
    pub skipped_reason: Option<String>,
}

/// The Builder's last line: what it did to the card file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Written {
    Wrote { bytes: u64 },
    Unchanged,
    WouldWrite { bytes: u64 },
    /// No known last line. An older or newer Builder; the profiles still say what happened.
    Unknown,
}

/// What the Builder reported for one card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardReport {
    pub card: PathBuf,
    pub profiles: Vec<ProfileLine>,
    pub written: Written,
}

impl CardReport {
    pub fn harvested(&self) -> u64 {
        self.profiles.iter().map(|p| p.harvested).sum()
    }
    pub fn updated(&self) -> u64 {
        self.profiles.iter().map(|p| p.updated).sum()
    }
}

fn number_after(s: &str, key: &str) -> u64 {
    s.split(", ")
        .find_map(|part| part.strip_prefix(key).and_then(|n| n.trim().parse().ok()))
        .unwrap_or(0)
}

fn leading_number(s: &str) -> u64 {
    s.trim().split(' ').next().and_then(|n| n.parse().ok()).unwrap_or(0)
}

/// Read the Builder's report. Unknown lines are ignored, never an error.
pub fn parse_report(card: &Path, text: &str) -> CardReport {
    let mut report = CardReport { card: card.to_path_buf(), profiles: Vec::new(), written: Written::Unknown };
    for raw in text.lines() {
        let line = raw.trim_end();
        if let Some(rest) = line.strip_prefix("profile ") {
            let Some((name, tail)) = rest.split_once(": ") else { continue };
            let mut p = ProfileLine { profile: name.to_string(), ..Default::default() };
            if let Some(reason) = tail.strip_prefix("skipped (") {
                p.skipped_reason = Some(reason.trim_end_matches(')').to_string());
            } else {
                p.bootstrap = tail.starts_with("bootstrap");
                p.updated = number_after(tail, "updated ");
                p.harvested = number_after(tail, "harvested ");
                p.carried = number_after(tail, "carried ");
                p.skipped = number_after(tail, "skipped ");
            }
            report.profiles.push(p);
        } else if let Some(rest) = line.strip_prefix("wrote ") {
            let bytes = rest.rsplit_once(": ").map(|(_, n)| leading_number(n)).unwrap_or(0);
            report.written = Written::Wrote { bytes };
        } else if line.starts_with("unchanged ") {
            report.written = Written::Unchanged;
        } else if let Some(rest) = line.strip_prefix("would write ") {
            let bytes = rest.rsplit_once(": ").map(|(_, n)| leading_number(n)).unwrap_or(0);
            report.written = Written::WouldWrite { bytes };
        }
    }
    report
}

/// One run of the verb over every card in one user folder.
#[derive(Debug)]
pub struct SyncRequest<'a> {
    pub data_root: &'a Path,
    /// The `.rebornroster` the image was built from.
    pub roster: &'a Path,
    /// The Builder's console build.
    pub builder: &'a Path,
    /// The Dolphin user folder whose `GC` holds the cards.
    pub user_dir: &'a Path,
    /// Record in-game edits, write nothing to the card. After a session.
    pub harvest_only: bool,
    /// Report only, write nothing. For a person checking.
    pub dry_run: bool,
}

impl SyncRequest<'_> {
    /// A run that may write the card, and so is the only kind that leaves
    /// and honors a [`Seen`] marker.
    fn is_full(&self) -> bool {
        !self.harvest_only && !self.dry_run
    }
}

/// Why one card was left as it was. The other cards still run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CardRefusal {
    /// The game has the card open.
    InUse { path: PathBuf },
    /// The recovery copy could not be made, so the card was not written.
    RecoveryFailed { path: PathBuf, why: String },
    /// The Builder did not finish. `tail` is the end of what it printed.
    BuilderFailed { path: PathBuf, exit: String, tail: String },
}

impl CardRefusal {
    pub fn message(&self) -> String {
        match self {
            CardRefusal::InUse { path } => format!(
                "Memory card {} is in use, so its gear is as it was. Close the game and try again.",
                file_name(path)
            ),
            CardRefusal::RecoveryFailed { path, why } => format!(
                "A safety copy of memory card {} could not be made ({why}), so the card is as it was.",
                file_name(path)
            ),
            CardRefusal::BuilderFailed { path, exit, tail } => format!(
                "Reborn Builder could not update memory card {} (exit {exit}); the card is as it was. It said \"{}\".",
                file_name(path),
                tail.lines().last().unwrap_or("nothing")
            ),
        }
    }
}

/// What happened to one card.
#[derive(Debug, Clone)]
pub enum CardResult {
    Synced(CardReport),
    /// A full run found the card and the roster exactly as the last full run
    /// left them, so the Builder was not started.
    UpToDate { path: PathBuf },
    Refused(CardRefusal),
}

/// Why no card was touched at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncRefusal {
    NoRoster { path: PathBuf },
    NoBuilder { looked: Vec<PathBuf> },
    NoCards { dir: PathBuf },
}

impl SyncRefusal {
    pub fn message(&self) -> String {
        match self {
            SyncRefusal::NoRoster { path } => format!(
                "No roster update is installed at {}, so the memory cards were left as they were. Nothing was changed.",
                path.display()
            ),
            SyncRefusal::NoBuilder { looked } => format!(
                "No Reborn Builder console build was found (looked in {}), so the memory cards were left as they were. Nothing was changed.",
                looked.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", ")
            ),
            SyncRefusal::NoCards { dir } => format!(
                "No memory card was found in {}. Nothing was changed.",
                dir.display()
            ),
        }
    }
}

/// The result of a run.
#[derive(Debug, Clone)]
pub enum SyncOutcome {
    Done(Vec<CardResult>),
    Refused(SyncRefusal),
}

impl CardResult {
    /// One sentence for a person, or nothing when the card was synced and
    /// nothing was written or harvested, which is most sessions.
    pub fn sentence(&self) -> Option<String> {
        match self {
            CardResult::Refused(r) => Some(r.message()),
            CardResult::UpToDate { path } => {
                Some(format!("Memory card {} is up to date with the installed roster.", file_name(path)))
            }
            CardResult::Synced(rep) => {
                let updated = rep.updated();
                let harvested = rep.harvested();
                let skipped: Vec<&str> = rep
                    .profiles
                    .iter()
                    .filter(|p| p.skipped_reason.is_some())
                    .map(|p| p.profile.as_str())
                    .collect();
                if updated == 0 && harvested == 0 && skipped.is_empty() {
                    return None;
                }
                let mut s = format!("Memory card {}: ", file_name(&rep.card));
                match &rep.written {
                    Written::Wrote { .. } => s.push_str(&format!("{updated} gear row(s) brought up to date, ")),
                    Written::WouldWrite { .. } => s.push_str(&format!("{updated} gear row(s) would change, ")),
                    Written::Unchanged | Written::Unknown => {}
                }
                s.push_str(&format!("{harvested} in-game edit(s) kept."));
                if !skipped.is_empty() {
                    s.push_str(&format!(" Profile(s) left as they were: {}.", skipped.join(", ")));
                }
                Some(s)
            }
        }
    }
}

impl SyncOutcome {
    /// One sentence per card for a person. Empty when every card was synced
    /// and nothing was written or harvested, which is most sessions. A card
    /// a full run found up to date says so.
    pub fn explain(&self) -> Vec<String> {
        match self {
            SyncOutcome::Refused(r) => vec![r.message()],
            SyncOutcome::Done(cards) => cards.iter().filter_map(CardResult::sentence).collect(),
        }
    }

    /// [`SyncOutcome::explain`] for a place that should stay quiet when there
    /// is nothing to act on: a launch, a page, the end of first run. A card
    /// that was simply up to date says nothing, and so does an install with
    /// no roster or a user folder with no card. A missing Builder still
    /// speaks: it means edits are not being kept, and someone should know.
    pub fn facts(&self) -> Vec<String> {
        match self {
            SyncOutcome::Refused(SyncRefusal::NoRoster { .. }) | SyncOutcome::Refused(SyncRefusal::NoCards { .. }) => {
                Vec::new()
            }
            SyncOutcome::Refused(r) => vec![r.message()],
            SyncOutcome::Done(cards) => cards
                .iter()
                .filter(|c| !matches!(c, CardResult::UpToDate { .. }))
                .filter_map(CardResult::sentence)
                .collect(),
        }
    }
}

fn now_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Whether a full run can skip this card: the Builder has a record of it,
/// and the card and the roster are exactly what the last full run saw.
fn already_seen(data_root: &Path, card: &Path, roster_sha: &str) -> bool {
    if !state_path(data_root, card).is_file() {
        return false;
    }
    let Some(seen) = read_seen(&seen_path(data_root, card)) else { return false };
    if seen.roster_sha256 != roster_sha {
        return false;
    }
    match hash_file(card) {
        Ok(h) => h.as_str() == seen.card_sha256,
        Err(_) => false,
    }
}

/// Run the Builder's verb over every card in the user folder.
pub fn sync(platform: &Platform, req: &SyncRequest<'_>) -> Result<SyncOutcome> {
    if !req.roster.is_file() {
        return Ok(SyncOutcome::Refused(SyncRefusal::NoRoster { path: req.roster.to_path_buf() }));
    }
    if !req.builder.is_file() {
        return Ok(SyncOutcome::Refused(SyncRefusal::NoBuilder { looked: vec![req.builder.to_path_buf()] }));
    }
    let cards = memory_cards(req.user_dir);
    if cards.is_empty() {
        return Ok(SyncOutcome::Refused(SyncRefusal::NoCards { dir: req.user_dir.join("GC") }));
    }
    // Hashed once per run, not once per card: the roster file is the same
    // thirty megabytes for every card.
    let roster_sha = if req.is_full() { Some(hash_file(req.roster)?.to_string()) } else { None };
    let stamp = now_seconds();
    let mut results = Vec::with_capacity(cards.len());
    for card in cards {
        if platform.files.is_locked(&card) {
            results.push(CardResult::Refused(CardRefusal::InUse { path: card }));
            continue;
        }
        if let Some(roster_sha) = &roster_sha {
            if already_seen(req.data_root, &card, roster_sha) {
                results.push(CardResult::UpToDate { path: card });
                continue;
            }
        }
        if req.is_full() {
            if let Err(e) = recovery_point(req.data_root, &card, stamp) {
                results.push(CardResult::Refused(CardRefusal::RecoveryFailed { path: card, why: e.to_string() }));
                continue;
            }
        }
        let state = state_path(req.data_root, &card);
        if let Some(parent) = state.parent() {
            std::fs::create_dir_all(parent).map_err(|source| Error::Io { path: parent.to_path_buf(), source })?;
        }
        let mut args: Vec<OsString> = vec![
            "card-sync".into(),
            "--card".into(),
            card.as_os_str().to_os_string(),
            "--update".into(),
            req.roster.as_os_str().to_os_string(),
            "--state".into(),
            state.as_os_str().to_os_string(),
        ];
        if req.harvest_only {
            args.push("--harvest-only".into());
        }
        if req.dry_run {
            args.push("--dry-run".into());
        }
        let run = platform.processes.run_capturing(req.builder, &args, &Default::default())?;
        if !run.succeeded() {
            let all = format!("{}\n{}", run.stdout, run.stderr);
            let tail: Vec<&str> = all.lines().filter(|l| !l.trim().is_empty()).collect();
            let tail = tail[tail.len().saturating_sub(6)..].join("\n");
            results.push(CardResult::Refused(CardRefusal::BuilderFailed {
                path: card,
                exit: format!("{:?}", run.exit),
                tail,
            }));
            continue;
        }
        if let Some(roster_sha) = &roster_sha {
            // The card as the Builder left it. A marker that cannot be
            // written only costs one more Builder run next time, so it is
            // not a reason to hide a sync that succeeded.
            if let Ok(card_sha) = hash_file(&card) {
                let seen = Seen { card_sha256: card_sha.to_string(), roster_sha256: roster_sha.clone(), at: stamp };
                let _ = write_seen(&seen_path(req.data_root, &card), &seen);
            }
        }
        results.push(CardResult::Synced(parse_report(&card, &run.stdout)));
    }
    Ok(SyncOutcome::Done(results))
}

/// The installed roster file, whether or not it exists yet.
fn installed_roster(data_root: &Path) -> PathBuf {
    data_root
        .join(DataDir::LibraryRoster.relative())
        .join(crate::roster::ROSTER_FILE)
}

/// After a roster reached the image, and before a launch: the full sync,
/// every card. Free when nothing changed since the last full run.
///
/// `extra_dirs` is where the caller also wants the Builder looked for (the
/// 2.1 tools folder); the core reads no environment variables.
pub fn after_apply(platform: &Platform, data_root: &Path, user_dir: &Path, extra_dirs: &[PathBuf]) -> Result<SyncOutcome> {
    let roster = installed_roster(data_root);
    let dirs = crate::roster::builder_dirs(platform, extra_dirs);
    let Some(builder) = crate::roster::find_builder(platform, &dirs) else {
        return Ok(SyncOutcome::Refused(SyncRefusal::NoBuilder { looked: dirs }));
    };
    sync(
        platform,
        &SyncRequest { data_root, roster: &roster, builder: &builder, user_dir, harvest_only: false, dry_run: false },
    )
}

/// What happened to the memory cards after the game closed.
///
/// Its own type for the same reason as [`crate::session::StatsAfterSession`]:
/// a failure here is reported, never turned into a failed launch of a game
/// that already ran.
#[derive(Debug, Clone)]
pub enum CardsAfterSession {
    Synced(SyncOutcome),
    Failed(String),
}

impl CardsAfterSession {
    /// One line for a player, or nothing when there was nothing to say.
    /// A missing roster is silent: a player with no roster installed has no
    /// cards to bring up to date. So is a user folder with no card. A
    /// missing Builder is not: it means edits are not being kept, and
    /// someone should know.
    pub fn explain(&self) -> Option<String> {
        match self {
            CardsAfterSession::Failed(why) => Some(format!("The memory cards could not be checked for gear edits: {why}")),
            CardsAfterSession::Synced(SyncOutcome::Refused(SyncRefusal::NoRoster { .. })) => None,
            CardsAfterSession::Synced(SyncOutcome::Refused(SyncRefusal::NoCards { .. })) => None,
            CardsAfterSession::Synced(outcome) => {
                let lines = outcome.explain();
                if lines.is_empty() { None } else { Some(lines.join(" ")) }
            }
        }
    }
}

/// After a session: harvest in-game edits off every card, write nothing.
pub fn after_session(platform: &Platform, user_dir: &Path) -> CardsAfterSession {
    let data_root = match platform.paths.data_root() {
        Ok(p) => p,
        Err(e) => return CardsAfterSession::Failed(e.to_string()),
    };
    let roster = installed_roster(&data_root);
    let dirs = crate::roster::builder_dirs(platform, &[]);
    let Some(builder) = crate::roster::find_builder(platform, &dirs) else {
        return CardsAfterSession::Synced(SyncOutcome::Refused(SyncRefusal::NoBuilder { looked: dirs }));
    };
    let req = SyncRequest { data_root: &data_root, roster: &roster, builder: &builder, user_dir, harvest_only: true, dry_run: false };
    match sync(platform, &req) {
        Ok(o) => CardsAfterSession::Synced(o),
        Err(e) => CardsAfterSession::Failed(e.to_string()),
    }
}

/// The sentence for a full sync that could not even be attempted (before a
/// game, after an update). A constant, never an error's text: the detail goes
/// to the log where it happens, for the same reason
/// `session::RESULTS_NOT_COLLECTED` is one.
pub const NOT_SYNCED: &str = "The memory cards could not be brought up to date, so their gear is as it was.";

/// Before a game: the full sync, so the cards the game is about to open
/// carry the installed roster's gear. Free when nothing changed since the
/// last full run. Never a reason not to start the game: what comes back is
/// the facts worth a line, and nothing when it was quiet.
pub fn before_session(platform: &Platform, user_dir: &Path) -> Vec<String> {
    let data_root = match platform.paths.data_root() {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!("the memory cards were not synced before the game: {e}");
            return vec![NOT_SYNCED.to_string()];
        }
    };
    match after_apply(platform, &data_root, user_dir, &[]) {
        Ok(outcome) => outcome.facts(),
        Err(e) => {
            tracing::warn!("the memory cards were not synced before the game: {e}");
            vec![NOT_SYNCED.to_string()]
        }
    }
}

/// What the memory cards said around one game: the full sync before it and
/// the harvest after it. One field on the launch result, one line for a
/// player, never a failed launch.
#[derive(Debug, Clone, Default)]
pub struct CardsAtSession {
    /// The facts of the full sync before the game; empty when it was quiet.
    pub before: Vec<String>,
    /// The harvest after the game. `None` when the game never ran.
    pub after: Option<CardsAfterSession>,
}

impl CardsAtSession {
    /// One line for a player, or nothing when there was nothing to say.
    pub fn explain(&self) -> Option<String> {
        let mut parts = self.before.clone();
        if let Some(line) = self.after.as_ref().and_then(CardsAfterSession::explain) {
            parts.push(line);
        }
        if parts.is_empty() {
            None
        } else {
            Some(parts.join(" "))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex};

    use crate::platform::{CapturedRun, ExitKind, FileOps, LockGuard, ProcessHandle, ProcessHost};

    #[test]
    fn memory_cards_are_the_gc_folders_memorycard_raw_files_and_nothing_else() {
        let dir = tempfile::tempdir().unwrap();
        let gc = dir.path().join("GC");
        std::fs::create_dir_all(&gc).unwrap();
        std::fs::write(gc.join("MemoryCardB.USA.raw"), [0u8; 16]).unwrap();
        std::fs::write(gc.join("MemoryCardA.USA.raw"), [0u8; 16]).unwrap();
        std::fs::write(gc.join("MemoryCardA.USA.raw.bak_pre_probe"), [0u8; 16]).unwrap();
        std::fs::write(gc.join("SRAM.raw"), [0u8; 68]).unwrap();
        std::fs::write(gc.join("NetPlayTempA.USA.raw"), [0u8; 16]).unwrap();
        let names: Vec<String> = memory_cards(dir.path())
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["MemoryCardA.USA.raw", "MemoryCardB.USA.raw"]);
        assert!(memory_cards(&dir.path().join("nowhere")).is_empty());
    }

    #[test]
    fn the_state_file_is_named_after_the_card_under_library_roster_cards() {
        let p = state_path(Path::new("root"), Path::new("C:/u/GC/MemoryCardB.USA.raw"));
        assert_eq!(p, Path::new("root").join("library").join("roster").join("cards").join("MemoryCardB.USA.raw.json"));
        let s = seen_path(Path::new("root"), Path::new("C:/u/GC/MemoryCardB.USA.raw"));
        assert_eq!(s, Path::new("root").join("library").join("roster").join("cards").join("MemoryCardB.USA.raw.seen.json"));
    }

    #[test]
    fn a_recovery_point_copies_the_card_and_keeps_only_the_newest_three() {
        let dir = tempfile::tempdir().unwrap();
        let card = dir.path().join("MemoryCardB.USA.raw");
        std::fs::write(&card, b"card bytes").unwrap();
        let mut made = Vec::new();
        for stamp in [10u64, 20, 30, 40] {
            made.push(recovery_point(dir.path(), &card, stamp).unwrap());
        }
        assert_eq!(std::fs::read(&made[3]).unwrap(), b"card bytes");
        assert!(!made[0].exists(), "the oldest copy is pruned");
        assert!(made[1].exists() && made[2].exists() && made[3].exists());
        assert!(made[3].ends_with("MemoryCardB.USA.raw.40.bak"));
    }

    #[test]
    fn parse_report_reads_the_builders_lines_and_ignores_what_it_does_not_know() {
        let text = "card MemoryCardB.USA.raw: 3 profile(s)\n\
                    profile Aaaa: bootstrap, updated 12, harvested 1, carried 0, skipped 2\n\
                    profile A1: synced, updated 0, harvested 0, carried 1, skipped 0\n\
                    profile Old: skipped (no record of this profile and the roster update carries no history)\n\
                    some future line\n\
                    wrote C:\\u\\GC\\MemoryCardB.USA.raw: 240 byte(s) changed\n";
        let r = parse_report(Path::new("C:/u/GC/MemoryCardB.USA.raw"), text);
        assert_eq!(r.profiles.len(), 3);
        assert_eq!(r.profiles[0], ProfileLine { profile: "Aaaa".into(), bootstrap: true, updated: 12, harvested: 1, carried: 0, skipped: 2, skipped_reason: None });
        assert_eq!(r.profiles[1].carried, 1);
        assert_eq!(r.profiles[2].skipped_reason.as_deref(), Some("no record of this profile and the roster update carries no history"));
        assert_eq!(r.written, Written::Wrote { bytes: 240 });
        assert_eq!(r.harvested(), 1);
        assert_eq!(r.updated(), 12);
        assert_eq!(parse_report(Path::new("x"), "unchanged x\n").written, Written::Unchanged);
        assert_eq!(parse_report(Path::new("x"), "would write x: 7 byte(s)\n").written, Written::WouldWrite { bytes: 7 });
        assert_eq!(parse_report(Path::new("x"), "").written, Written::Unknown);
    }

    /// Records every run, answers with one canned report, and writes the
    /// state file the real Builder writes.
    struct FakeBuilder {
        calls: Arc<Mutex<Vec<Vec<OsString>>>>,
        stdout: String,
        stderr: String,
        exit: ExitKind,
    }

    impl std::fmt::Debug for FakeBuilder {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("FakeBuilder").finish()
        }
    }

    impl ProcessHost for FakeBuilder {
        fn spawn(&self, _: &Path, _: &[OsString], _: Option<&Path>, _: &BTreeMap<String, OsString>) -> Result<ProcessHandle> {
            Err(Error::NotImplemented { operation: "spawn", platform: "fake" })
        }
        fn wait(&self, _: ProcessHandle) -> Result<ExitKind> {
            Err(Error::NotImplemented { operation: "wait", platform: "fake" })
        }
        fn is_running(&self, _: &ProcessHandle) -> bool {
            false
        }
        fn run_capturing(&self, _program: &Path, args: &[OsString], _env: &BTreeMap<String, OsString>) -> Result<CapturedRun> {
            self.calls.lock().expect("calls").push(args.to_vec());
            if self.exit == ExitKind::Normal {
                let dry = args.iter().any(|a| a == "--dry-run");
                if let Some(i) = args.iter().position(|a| a == "--state") {
                    if !dry {
                        let state = PathBuf::from(&args[i + 1]);
                        std::fs::create_dir_all(state.parent().unwrap()).unwrap();
                        std::fs::write(&state, "{}").unwrap();
                    }
                }
            }
            Ok(CapturedRun { stdout: self.stdout.clone(), stderr: self.stderr.clone(), exit: self.exit.clone() })
        }
    }

    /// Says one path is locked; everything else goes to the real file ops.
    struct LockedFiles {
        inner: Box<dyn FileOps>,
        locked: PathBuf,
    }

    impl std::fmt::Debug for LockedFiles {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("LockedFiles").field("locked", &self.locked).finish()
        }
    }

    impl FileOps for LockedFiles {
        fn atomic_replace(&self, src: &Path, dest: &Path) -> Result<()> {
            self.inner.atomic_replace(src, dest)
        }
        fn same_volume(&self, a: &Path, b: &Path) -> Result<bool> {
            self.inner.same_volume(a, b)
        }
        fn hardlink(&self, src: &Path, dest: &Path) -> Result<()> {
            self.inner.hardlink(src, dest)
        }
        fn is_locked(&self, path: &Path) -> bool {
            path == self.locked || self.inner.is_locked(path)
        }
        fn lock_exclusive(&self, path: &Path) -> Result<Option<LockGuard>> {
            self.inner.lock_exclusive(path)
        }
    }

    /// Only exists so the real [`FileOps`] can be moved out of a `Platform`.
    #[derive(Debug)]
    struct NoFiles;
    impl FileOps for NoFiles {
        fn atomic_replace(&self, _: &Path, _: &Path) -> Result<()> { Ok(()) }
        fn same_volume(&self, _: &Path, _: &Path) -> Result<bool> { Ok(true) }
        fn hardlink(&self, _: &Path, _: &Path) -> Result<()> { Ok(()) }
        fn is_locked(&self, _: &Path) -> bool { false }
        fn lock_exclusive(&self, _: &Path) -> Result<Option<LockGuard>> { Ok(None) }
    }

    const REPORT: &str = "card MemoryCardB.USA.raw: 1 profile(s)\n\
                          profile Aaaa: synced, updated 3, harvested 1, carried 0, skipped 0\n\
                          wrote x: 60 byte(s) changed\n";

    fn platform_with(builder: FakeBuilder, locked: Option<&Path>) -> Platform {
        let mut platform = Platform::detect();
        platform.processes = Box::new(builder);
        if let Some(l) = locked {
            let real = std::mem::replace(&mut platform.files, Box::new(NoFiles));
            platform.files = Box::new(LockedFiles { inner: real, locked: l.to_path_buf() });
        }
        platform
    }

    fn scene() -> (tempfile::TempDir, PathBuf, PathBuf, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let user = dir.path().join("User");
        std::fs::create_dir_all(user.join("GC")).unwrap();
        std::fs::write(user.join("GC").join("MemoryCardA.USA.raw"), b"card A").unwrap();
        std::fs::write(user.join("GC").join("MemoryCardB.USA.raw"), b"card B").unwrap();
        let roster = dir.path().join("roster.rebornroster");
        std::fs::write(&roster, b"zip").unwrap();
        let builder = dir.path().join("reborn-builder-cli.exe");
        std::fs::write(&builder, b"exe").unwrap();
        (dir, user, roster, builder)
    }

    fn fake(exit: ExitKind) -> FakeBuilder {
        FakeBuilder { calls: Arc::new(Mutex::new(Vec::new())), stdout: REPORT.into(), stderr: String::new(), exit }
    }

    fn full<'a>(dir: &'a Path, roster: &'a Path, builder: &'a Path, user: &'a Path) -> SyncRequest<'a> {
        SyncRequest { data_root: dir, roster, builder, user_dir: user, harvest_only: false, dry_run: false }
    }

    fn names_in(dir: &Path) -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(dir)
            .map(|rd| rd.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect())
            .unwrap_or_default();
        v.sort();
        v
    }

    #[test]
    fn sync_refuses_before_running_anything_when_the_inputs_are_missing() {
        let (dir, user, roster, builder) = scene();
        let fb = fake(ExitKind::Normal);
        let calls = fb.calls.clone();
        let platform = platform_with(fb, None);
        let missing = dir.path().join("none.rebornroster");
        assert!(matches!(sync(&platform, &full(dir.path(), &missing, &builder, &user)).unwrap(), SyncOutcome::Refused(SyncRefusal::NoRoster { .. })));
        assert!(matches!(sync(&platform, &full(dir.path(), &roster, &missing, &user)).unwrap(), SyncOutcome::Refused(SyncRefusal::NoBuilder { .. })));
        assert!(matches!(sync(&platform, &full(dir.path(), &roster, &builder, &dir.path().join("empty"))).unwrap(), SyncOutcome::Refused(SyncRefusal::NoCards { .. })));
        assert!(calls.lock().unwrap().is_empty(), "nothing ran");
    }

    #[test]
    fn sync_runs_the_builder_once_per_card_and_copies_each_card_first() {
        let (dir, user, roster, builder) = scene();
        let fb = fake(ExitKind::Normal);
        let calls = fb.calls.clone();
        let platform = platform_with(fb, None);
        let SyncOutcome::Done(results) = sync(&platform, &full(dir.path(), &roster, &builder, &user)).unwrap() else { panic!("done") };
        assert_eq!(results.len(), 2);
        assert_eq!(calls.lock().unwrap().len(), 2);
        assert!(results.iter().all(|r| matches!(r, CardResult::Synced(rep) if rep.updated() == 3)));
        // Recovery copies exist for both cards.
        let copies = names_in(&recovery_dir(dir.path()));
        assert!(copies.iter().any(|n| n.starts_with("MemoryCardA.USA.raw.")) && copies.iter().any(|n| n.starts_with("MemoryCardB.USA.raw.")), "{copies:?}");
        // The state directory was made.
        assert!(state_path(dir.path(), &user.join("GC").join("MemoryCardA.USA.raw")).parent().unwrap().is_dir());
    }

    #[test]
    fn the_arguments_are_the_verb_the_card_the_update_the_state_and_the_flags() {
        let (dir, user, roster, builder) = scene();
        let fb = fake(ExitKind::Normal);
        let calls = fb.calls.clone();
        let platform = platform_with(fb, None);
        let req = SyncRequest { data_root: dir.path(), roster: &roster, builder: &builder, user_dir: &user, harvest_only: true, dry_run: true };
        sync(&platform, &req).unwrap();
        let got = calls.lock().unwrap();
        assert_eq!(got.len(), 2);
        let a: Vec<String> = got[0].iter().map(|s| s.to_string_lossy().into_owned()).collect();
        assert_eq!(a[0], "card-sync");
        assert_eq!(&a[1..3], &["--card".to_string(), user.join("GC").join("MemoryCardA.USA.raw").to_string_lossy().into_owned()]);
        assert_eq!(&a[3..5], &["--update".to_string(), roster.to_string_lossy().into_owned()]);
        assert_eq!(a[5], "--state");
        assert!(a[6].ends_with("MemoryCardA.USA.raw.json"));
        assert_eq!(&a[7..], &["--harvest-only".to_string(), "--dry-run".to_string()]);
        assert!(!recovery_dir(dir.path()).exists(), "harvest-only takes no recovery copy");
    }

    #[test]
    fn a_builder_failure_refuses_that_card_and_the_others_still_run() {
        let (dir, user, roster, builder) = scene();
        let platform = platform_with(FakeBuilder { calls: Arc::new(Mutex::new(Vec::new())), stdout: String::new(), stderr: "Stopped: the memory card is in use\n".into(), exit: ExitKind::Error { code: 2 } }, None);
        let SyncOutcome::Done(results) = sync(&platform, &full(dir.path(), &roster, &builder, &user)).unwrap() else { panic!("done") };
        assert_eq!(results.len(), 2);
        for r in &results {
            let CardResult::Refused(CardRefusal::BuilderFailed { exit, tail, .. }) = r else { panic!("{r:?}") };
            assert!(exit.contains("2") && tail.contains("in use"));
        }
        assert!(names_in(&state_dir(dir.path())).is_empty(), "no marker after a failed run");
    }

    #[test]
    fn a_locked_card_is_refused_and_never_copied_while_the_other_card_runs() {
        let (dir, user, roster, builder) = scene();
        let locked = user.join("GC").join("MemoryCardA.USA.raw");
        let platform = platform_with(fake(ExitKind::Normal), Some(&locked));
        let SyncOutcome::Done(results) = sync(&platform, &full(dir.path(), &roster, &builder, &user)).unwrap() else { panic!("done") };
        assert!(matches!(&results[0], CardResult::Refused(CardRefusal::InUse { path }) if *path == locked));
        assert!(matches!(&results[1], CardResult::Synced(_)));
        let copies = names_in(&recovery_dir(dir.path()));
        assert!(copies.iter().all(|n| n.starts_with("MemoryCardB.USA.raw.")), "{copies:?}");
    }

    #[test]
    fn a_full_run_leaves_a_marker_and_the_next_full_run_skips_the_builder_until_the_card_or_roster_changes() {
        let (dir, user, roster, builder) = scene();
        let fb = fake(ExitKind::Normal);
        let calls = fb.calls.clone();
        let platform = platform_with(fb, None);
        let card_a = user.join("GC").join("MemoryCardA.USA.raw");
        let runs = || calls.lock().unwrap().len();

        // First full run: both cards through the Builder, a marker each.
        sync(&platform, &full(dir.path(), &roster, &builder, &user)).unwrap();
        assert_eq!(runs(), 2);
        let seen_a = read_seen(&seen_path(dir.path(), &card_a)).expect("marker for A");
        assert_eq!(seen_a.card_sha256, hash_file(&card_a).unwrap().to_string());
        assert_eq!(seen_a.roster_sha256, hash_file(&roster).unwrap().to_string());

        // Nothing changed: no Builder, both up to date, and it says so.
        let SyncOutcome::Done(results) = sync(&platform, &full(dir.path(), &roster, &builder, &user)).unwrap() else { panic!("done") };
        assert_eq!(runs(), 2);
        assert!(results.iter().all(|r| matches!(r, CardResult::UpToDate { .. })), "{results:?}");
        let lines = SyncOutcome::Done(results).explain();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("MemoryCardA.USA.raw is up to date with the installed roster"), "{}", lines[0]);

        // The game saved onto card A: only A runs again.
        std::fs::write(&card_a, b"card A after a save").unwrap();
        let SyncOutcome::Done(results) = sync(&platform, &full(dir.path(), &roster, &builder, &user)).unwrap() else { panic!("done") };
        assert_eq!(runs(), 3);
        assert!(matches!(&results[0], CardResult::Synced(_)));
        assert!(matches!(&results[1], CardResult::UpToDate { .. }));

        // A new roster: every card runs again.
        std::fs::write(&roster, b"zip 2").unwrap();
        sync(&platform, &full(dir.path(), &roster, &builder, &user)).unwrap();
        assert_eq!(runs(), 5);

        // Harvest-only and dry-run never skip and never touch the marker.
        let before = read_seen(&seen_path(dir.path(), &card_a)).unwrap();
        let harvest = SyncRequest { data_root: dir.path(), roster: &roster, builder: &builder, user_dir: &user, harvest_only: true, dry_run: false };
        sync(&platform, &harvest).unwrap();
        let dry = SyncRequest { data_root: dir.path(), roster: &roster, builder: &builder, user_dir: &user, harvest_only: false, dry_run: true };
        sync(&platform, &dry).unwrap();
        assert_eq!(runs(), 9);
        assert_eq!(read_seen(&seen_path(dir.path(), &card_a)).unwrap(), before);

        // Without the Builder's own record the marker alone does not skip.
        std::fs::remove_file(state_path(dir.path(), &card_a)).unwrap();
        let SyncOutcome::Done(results) = sync(&platform, &full(dir.path(), &roster, &builder, &user)).unwrap() else { panic!("done") };
        assert_eq!(runs(), 10);
        assert!(matches!(&results[0], CardResult::Synced(_)));
        assert!(matches!(&results[1], CardResult::UpToDate { .. }));
    }

    #[test]
    fn every_refusal_names_what_remains_as_it_was() {
        let card = [
            CardRefusal::InUse { path: "c".into() },
            CardRefusal::RecoveryFailed { path: "c".into(), why: "disk full".into() },
            CardRefusal::BuilderFailed { path: "c".into(), exit: "Error { code: 2 }".into(), tail: "Stopped: x".into() },
        ];
        for r in card {
            let m = r.message();
            assert!(m.contains("as it was"), "{r:?}: {m}");
            assert!(m.ends_with('.'), "not a sentence: {m}");
        }
        let all = [
            SyncRefusal::NoRoster { path: "r".into() },
            SyncRefusal::NoBuilder { looked: vec!["a".into(), "b".into()] },
            SyncRefusal::NoCards { dir: "d".into() },
        ];
        for r in all {
            let m = r.message();
            assert!(m.contains("Nothing was changed"), "{r:?}: {m}");
        }
    }

    #[test]
    fn explain_is_silent_for_a_quiet_sync_and_speaks_for_edits_writes_and_refusals() {
        let quiet = CardReport { card: "c".into(), profiles: vec![ProfileLine { profile: "A".into(), ..Default::default() }], written: Written::Unchanged };
        assert!(SyncOutcome::Done(vec![CardResult::Synced(quiet)]).explain().is_empty());
        let busy = parse_report(Path::new("MemoryCardB.USA.raw"), REPORT);
        let lines = SyncOutcome::Done(vec![CardResult::Synced(busy), CardResult::Refused(CardRefusal::InUse { path: "MemoryCardA.USA.raw".into() })]).explain();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("3 gear row(s) brought up to date") && lines[0].contains("1 in-game edit(s) kept"), "{}", lines[0]);
        assert!(lines[1].contains("in use"));
        assert!(CardsAfterSession::Synced(SyncOutcome::Refused(SyncRefusal::NoRoster { path: "r".into() })).explain().is_none());
        assert!(CardsAfterSession::Synced(SyncOutcome::Refused(SyncRefusal::NoCards { dir: "d".into() })).explain().is_none());
        assert!(CardsAfterSession::Synced(SyncOutcome::Refused(SyncRefusal::NoBuilder { looked: vec![] })).explain().is_some());
        assert!(CardsAfterSession::Failed("boom".into()).explain().unwrap().contains("boom"));
    }

    #[test]
    fn facts_stay_quiet_for_an_up_to_date_card_a_missing_roster_or_no_card_but_not_a_missing_builder() {
        let up = SyncOutcome::Done(vec![CardResult::UpToDate { path: PathBuf::from("C:/u/GC/MemoryCardB.USA.raw") }]);
        assert_eq!(up.explain().len(), 1, "the verb still says so");
        assert!(up.facts().is_empty(), "a page or a launch stays quiet");
        assert!(SyncOutcome::Refused(SyncRefusal::NoRoster { path: PathBuf::from("r") }).facts().is_empty());
        assert!(SyncOutcome::Refused(SyncRefusal::NoCards { dir: PathBuf::from("d") }).facts().is_empty());
        assert_eq!(SyncOutcome::Refused(SyncRefusal::NoBuilder { looked: vec![] }).facts().len(), 1);
        let wrote = SyncOutcome::Done(vec![CardResult::Synced(parse_report(
            Path::new("C:/u/GC/MemoryCardB.USA.raw"),
            "profile A: synced, updated 3, harvested 0, carried 0, skipped 0\nwrote x: 60 byte(s) changed\n",
        ))]);
        assert_eq!(
            wrote.facts(),
            vec!["Memory card MemoryCardB.USA.raw: 3 gear row(s) brought up to date, 0 in-game edit(s) kept.".to_string()]
        );
        assert_eq!(wrote.facts(), wrote.explain());
    }

    #[test]
    fn cards_at_session_says_one_line_for_before_and_after_and_nothing_when_both_were_quiet() {
        assert_eq!(CardsAtSession::default().explain(), None);
        let quiet_after = CardsAfterSession::Synced(SyncOutcome::Done(vec![]));
        let s = CardsAtSession { before: vec!["Before.".into()], after: Some(quiet_after) };
        assert_eq!(s.explain().as_deref(), Some("Before."));
        let no_builder = || CardsAfterSession::Synced(SyncOutcome::Refused(SyncRefusal::NoBuilder { looked: vec![] }));
        let s = CardsAtSession { before: vec![], after: Some(no_builder()) };
        assert!(s.explain().unwrap().starts_with("No Reborn Builder console build was found"));
        let s = CardsAtSession { before: vec!["Before.".into()], after: Some(no_builder()) };
        assert!(s.explain().unwrap().starts_with("Before. No Reborn Builder"));
        crate::player_sentence::assert_player_safe(NOT_SYNCED);
    }
}
