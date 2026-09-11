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

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::platform::{DataDir, Error, Platform, Result};

/// Under `library/roster`: one JSON file per card, owned by the Builder.
pub const STATE_DIR: &str = "cards";
/// Under `recovery`: copies of cards taken before they were written.
pub const RECOVERY_DIR: &str = "cards";
/// How many recovery copies to keep per card.
pub const KEEP_RECOVERY: usize = 3;

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

/// Where the Builder keeps what it last wrote to this card.
pub fn state_path(data_root: &Path, card: &Path) -> PathBuf {
    data_root
        .join(DataDir::LibraryRoster.relative())
        .join(STATE_DIR)
        .join(format!("{}.json", file_name(card)))
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
