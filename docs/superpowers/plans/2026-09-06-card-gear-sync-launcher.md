# Card Gear Sync, part 3 of 3: the launcher runs it

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** After every roster apply and after every game session, the launcher runs the Reborn Builder's `card-sync` verb over every memory card in the Dolphin user folder, takes a recovery copy of a card before writing it, and reports what happened as facts, never popups.

**Architecture:** One new core module, `cards.rs`, modeled on `roster.rs`: a request struct, refusal enums whose messages name what remains as it was, an outcome enum, and one `sync` function that finds the cards, refuses a locked one, copies it under `recovery/cards/`, runs the Builder with `run_capturing`, and parses its report lines. `Session::run` calls it in harvest-only mode after the stats ingest. The CLI's `roster apply` and a new `cards sync` verb call it in full mode; the UI's `content_update` calls it after a promote that applied a roster. The launcher never parses a card or a roster.

**Tech Stack:** Rust (workspace, `reborn-core` at rust-version 1.88, `#![forbid(unsafe_code)]`, `#![warn(missing_debug_implementations)]`), `tempfile` in dev-deps, `cargo test --workspace`, the `Tools\check-*.mjs` checkers, `Tools\test-all.ps1`.

**Spec:** `C:\Users\Adam\nflstreet2_clean\tools\reborn_builder\docs\specs\2026-09-06-card-gear-sync-design.md`, sections "Operations", "Data / Launcher state", "Components / Launcher". The Builder's report-line contract is in part 1's plan, `C:\Users\Adam\nflstreet2_clean\tools\reborn_builder\docs\plans\2026-09-06-card-gear-sync-builder.md`, Task 4.

## Global Constraints

- The launcher never parses a card or a roster (spec). It calls `reborn-builder-cli card-sync --card X --update Y --state S [--harvest-only] [--dry-run]` and reads these stdout lines, ignoring any it does not know:
  - `card <file name>: <n> profile(s)`
  - `profile <name>: bootstrap|synced, updated <n>, harvested <n>, carried <n>, skipped <n>`
  - `profile <name>: skipped (<reason>)`
  - `wrote <path>: <n> byte(s) changed` | `unchanged <path>` | `would write <path>: <n> byte(s)`
  - exit 0 success, 2 refusal (`Stopped: ...` on stderr), 4 bad arguments.
- Memory cards are the files in `<user dir>/GC` whose lowercased name starts with `memorycard` and ends with `.raw` (the convention `session.rs::user_dir_score` already uses).
- State file per card: `<data root>/library/roster/cards/<card file name>.json`. The Builder owns its contents; the launcher only names it.
- Recovery point before the first real write of a card in a run: `<data root>/recovery/cards/<card file name>.<unix seconds>.bak`, newest three kept per card. Not taken for `--harvest-only` or `--dry-run`, which write no card bytes.
- A card that `platform.files.is_locked` is refused, never written. Post-session runs inside the supervisor lock after `wait` returned, so the game is closed; post-apply runs only where the game is already known to be closed.
- Every bad state is a variant with a `message()` / `explain()` that is a sentence and contains `as it was` or `Nothing was changed`. Only I/O on the launcher's own files is an `Err`.
- Every new public type derives `Debug`. Every `std::fs` call is `.map_err(|source| Error::Io { path, source })`.
- No new Tauri command. No environment variable read in the core (the CLI supplies the 2.1 tools folder, as `legacy_tools_dirs()` does).
- Test names are full sentences in snake_case. `cargo test --workspace` and every `Tools\check-*.mjs` must pass before a commit. Kill a running `reborn-launcher` before `cargo build`.
- Another session is editing `reborn-ui`. Before each task: `git pull --rebase` is not the workflow here; instead run `git status` and re-read the insertion points named below, because line numbers move.
- Commits: one line, lowercase, what changed and why, as in the log.

---

## File structure

| File | Responsibility |
|---|---|
| `crates/reborn-core/src/cards.rs` (new) | Everything above: card discovery, state and recovery paths, `SyncRequest`, refusals, `parse_report`, `sync`, `after_session`, `after_apply`. |
| `crates/reborn-core/src/lib.rs` (modify: module list lines 41-69) | `pub mod cards;` |
| `crates/reborn-core/src/session.rs` (modify: `Launched::Finished` lines 100-126, `Session::run` after the stats block at lines 703-711) | `cards: CardsAfterSession` field; the post-session harvest. |
| `crates/reborn-cli/src/main.rs` (modify: `Command` enum near line 105, `RosterCommand` dispatch near 662, `roster_apply` near 4433, `Launched::Finished` printing near 1263) | `cards sync` verb; sync after `roster apply`; print the post-session line. |
| `crates/reborn-ui/src/main.rs` (modify: `content_update` command; the session-finished view near line 712) | Sync after a promote that applied a roster; show the post-session line. |

---

### Task 1: Card discovery, paths, recovery point, report parsing

**Files:**
- Create: `crates/reborn-core/src/cards.rs`
- Modify: `crates/reborn-core/src/lib.rs`

**Interfaces:**
- Produces: `cards::memory_cards(user_dir: &Path) -> Vec<PathBuf>`; `cards::state_path(data_root: &Path, card: &Path) -> PathBuf`; `cards::recovery_point(data_root: &Path, card: &Path, stamp: u64) -> Result<PathBuf>`; `cards::KEEP_RECOVERY: usize = 3`; `cards::ProfileLine { profile: String, bootstrap: bool, updated: u64, harvested: u64, carried: u64, skipped: u64, skipped_reason: Option<String> }`; `cards::Written { Wrote { bytes: u64 }, Unchanged, WouldWrite { bytes: u64 }, Unknown }`; `cards::CardReport { card: PathBuf, profiles: Vec<ProfileLine>, written: Written }`; `cards::parse_report(card: &Path, text: &str) -> CardReport`.

- [ ] **Step 1: Write the module with its tests, tests first in your head, code second**

`crates/reborn-core/src/cards.rs` (the whole file for this task; Task 2 appends to it):

```rust
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

/// Where the Builder keeps what it last wrote to this card.
pub fn state_path(data_root: &Path, card: &Path) -> PathBuf {
    let name = card.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    data_root
        .join(DataDir::LibraryRoster.relative())
        .join(STATE_DIR)
        .join(format!("{name}.json"))
}

fn recovery_dir(data_root: &Path) -> PathBuf {
    data_root.join(DataDir::Recovery.relative()).join(RECOVERY_DIR)
}

/// Copy a card before it is written, and keep only the newest
/// [`KEEP_RECOVERY`] copies of it.
pub fn recovery_point(data_root: &Path, card: &Path, stamp: u64) -> Result<PathBuf> {
    let dir = recovery_dir(data_root);
    std::fs::create_dir_all(&dir).map_err(|source| Error::Io { path: dir.clone(), source })?;
    let name = card.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let dest = dir.join(format!("{name}.{stamp}.bak"));
    std::fs::copy(card, &dest).map_err(|source| Error::Io { path: dest.clone(), source })?;

    let prefix = format!("{name}.");
    let mut copies: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map_err(|source| Error::Io { path: dir.clone(), source })?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            let n = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
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
```

- [ ] **Step 2: Declare the module**

In `crates/reborn-core/src/lib.rs`, in the module list (alphabetical, after `pub mod content;`... it is not strictly alphabetical; put it after `pub mod controller_profiles;`):

```rust
pub mod cards;
```

- [ ] **Step 3: Run the tests**

Run: `cargo test -p reborn-core cards::`
Expected: 4 passed.

- [ ] **Step 4: Commit**

```bash
git add crates/reborn-core/src/cards.rs crates/reborn-core/src/lib.rs
git commit -m "cards: find memory cards, name their state, copy before writing, read the builder's report"
```

---

### Task 2: `sync`, the refusals, and the two entry points

**Files:**
- Modify: `crates/reborn-core/src/cards.rs` (append)

**Interfaces:**
- Consumes: `roster::builder_dirs`, `roster::find_builder`, `roster::ROSTER_FILE`, `platform.files.is_locked`, `platform.processes.run_capturing`.
- Produces: `cards::SyncRequest<'a> { data_root: &Path, roster: &Path, builder: &Path, user_dir: &Path, harvest_only: bool, dry_run: bool }`; `cards::CardRefusal { InUse { path }, RecoveryFailed { path, why }, BuilderFailed { path, exit, tail } }` with `message()`; `cards::CardResult { Synced(CardReport), Refused(CardRefusal) }`; `cards::SyncRefusal { NoRoster { path }, NoBuilder { looked: Vec<PathBuf> }, NoCards { dir } }` with `message()`; `cards::SyncOutcome { Done(Vec<CardResult>), Refused(SyncRefusal) }` with `explain(&self) -> Vec<String>`; `cards::sync(platform, &SyncRequest) -> Result<SyncOutcome>`; `cards::after_apply(platform, data_root, user_dir, extra_dirs) -> Result<SyncOutcome>`; `cards::after_session(platform, user_dir) -> CardsAfterSession`; `cards::CardsAfterSession { Synced(SyncOutcome), Failed(String) }` with `explain(&self) -> Option<String>`.

- [ ] **Step 1: Append the code**

Append to `crates/reborn-core/src/cards.rs` before `#[cfg(test)]`:

```rust
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
        let name = |p: &Path| p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        match self {
            CardRefusal::InUse { path } => format!(
                "Memory card {} is in use, so its gear is as it was. Close the game and try again.",
                name(path)
            ),
            CardRefusal::RecoveryFailed { path, why } => format!(
                "A safety copy of memory card {} could not be made ({why}), so the card is as it was.",
                name(path)
            ),
            CardRefusal::BuilderFailed { path, exit, tail } => format!(
                "Reborn Builder could not update memory card {} (exit {exit}); the card is as it was. It said: {}",
                name(path),
                tail.lines().last().unwrap_or("nothing")
            ),
        }
    }
}

/// What happened to one card.
#[derive(Debug, Clone)]
pub enum CardResult {
    Synced(CardReport),
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

impl SyncOutcome {
    /// One sentence per card for a person. Empty when every card was synced
    /// and nothing was written or harvested, which is most sessions.
    pub fn explain(&self) -> Vec<String> {
        let name = |p: &Path| p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        match self {
            SyncOutcome::Refused(r) => vec![r.message()],
            SyncOutcome::Done(cards) => cards
                .iter()
                .filter_map(|c| match c {
                    CardResult::Refused(r) => Some(r.message()),
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
                        let mut s = format!("Memory card {}: ", name(&rep.card));
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
                })
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
    let stamp = now_seconds();
    let mut results = Vec::with_capacity(cards.len());
    for card in cards {
        if platform.files.is_locked(&card) {
            results.push(CardResult::Refused(CardRefusal::InUse { path: card }));
            continue;
        }
        if !req.harvest_only && !req.dry_run {
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
        results.push(CardResult::Synced(parse_report(&card, &run.stdout)));
    }
    Ok(SyncOutcome::Done(results))
}

/// The installed roster file, or None.
fn installed_roster(data_root: &Path) -> PathBuf {
    data_root
        .join(DataDir::LibraryRoster.relative())
        .join(crate::roster::ROSTER_FILE)
}

/// After a roster reached the image: the full sync, every card.
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
    /// cards to bring up to date. A missing Builder is not: it means edits
    /// are not being kept, and someone should know.
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
```

- [ ] **Step 2: Append the tests**

Inside `mod tests`, add (this also introduces the workspace's first `ProcessHost` fake; keep it in this module):

```rust
    use std::collections::BTreeMap;
    use std::sync::Mutex;

    use crate::platform::{CapturedRun, ExitKind, FileOps, LockGuard, ProcessHandle, ProcessHost};

    /// Records every run and answers with one canned report.
    struct FakeBuilder {
        calls: Mutex<Vec<Vec<OsString>>>,
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
        FakeBuilder { calls: Mutex::new(Vec::new()), stdout: REPORT.into(), stderr: String::new(), exit }
    }

    #[test]
    fn sync_refuses_before_running_anything_when_the_inputs_are_missing() {
        let (dir, user, roster, builder) = scene();
        let platform = platform_with(fake(ExitKind::Normal), None);
        let missing = dir.path().join("none.rebornroster");
        let req = |r: &Path, b: &Path, u: &Path| SyncRequest { data_root: dir.path(), roster: r, builder: b, user_dir: u, harvest_only: false, dry_run: false };
        assert!(matches!(sync(&platform, &req(&missing, &builder, &user)).unwrap(), SyncOutcome::Refused(SyncRefusal::NoRoster { .. })));
        assert!(matches!(sync(&platform, &req(&roster, &missing, &user)).unwrap(), SyncOutcome::Refused(SyncRefusal::NoBuilder { .. })));
        assert!(matches!(sync(&platform, &req(&roster, &builder, &dir.path().join("empty"))).unwrap(), SyncOutcome::Refused(SyncRefusal::NoCards { .. })));
    }

    #[test]
    fn sync_runs_the_builder_once_per_card_and_copies_each_card_first() {
        let (dir, user, roster, builder) = scene();
        let platform = platform_with(fake(ExitKind::Normal), None);
        let req = SyncRequest { data_root: dir.path(), roster: &roster, builder: &builder, user_dir: &user, harvest_only: false, dry_run: false };
        let SyncOutcome::Done(results) = sync(&platform, &req).unwrap() else { panic!("done") };
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| matches!(r, CardResult::Synced(rep) if rep.updated() == 3)));
        // Recovery copies exist for both cards.
        let rec = recovery_dir(dir.path());
        let copies: Vec<String> = std::fs::read_dir(&rec).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        assert!(copies.iter().any(|n| n.starts_with("MemoryCardA.USA.raw.")) && copies.iter().any(|n| n.starts_with("MemoryCardB.USA.raw.")), "{copies:?}");
        // The state directory was made.
        assert!(state_path(dir.path(), &user.join("GC").join("MemoryCardA.USA.raw")).parent().unwrap().is_dir());
    }

    #[test]
    fn the_arguments_are_the_verb_the_card_the_update_the_state_and_the_flags() {
        let (dir, user, roster, builder) = scene();
        let calls: std::sync::Arc<Mutex<Vec<Vec<OsString>>>> = Default::default();
        struct Recording(std::sync::Arc<Mutex<Vec<Vec<OsString>>>>);
        impl std::fmt::Debug for Recording {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str("Recording") }
        }
        impl ProcessHost for Recording {
            fn spawn(&self, _: &Path, _: &[OsString], _: Option<&Path>, _: &BTreeMap<String, OsString>) -> Result<ProcessHandle> {
                Err(Error::NotImplemented { operation: "spawn", platform: "fake" })
            }
            fn wait(&self, _: ProcessHandle) -> Result<ExitKind> { Err(Error::NotImplemented { operation: "wait", platform: "fake" }) }
            fn is_running(&self, _: &ProcessHandle) -> bool { false }
            fn run_capturing(&self, _: &Path, args: &[OsString], _: &BTreeMap<String, OsString>) -> Result<CapturedRun> {
                self.0.lock().unwrap().push(args.to_vec());
                Ok(CapturedRun { stdout: REPORT.into(), stderr: String::new(), exit: ExitKind::Normal })
            }
        }
        let mut platform = Platform::detect();
        platform.processes = Box::new(Recording(calls.clone()));
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
        let platform = platform_with(FakeBuilder { calls: Mutex::new(Vec::new()), stdout: String::new(), stderr: "Stopped: the memory card is in use\n".into(), exit: ExitKind::Error { code: 2 } }, None);
        let req = SyncRequest { data_root: dir.path(), roster: &roster, builder: &builder, user_dir: &user, harvest_only: false, dry_run: false };
        let SyncOutcome::Done(results) = sync(&platform, &req).unwrap() else { panic!("done") };
        assert_eq!(results.len(), 2);
        for r in &results {
            let CardResult::Refused(CardRefusal::BuilderFailed { exit, tail, .. }) = r else { panic!("{r:?}") };
            assert!(exit.contains("2") && tail.contains("in use"));
        }
    }

    #[test]
    fn a_locked_card_is_refused_and_never_copied_while_the_other_card_runs() {
        let (dir, user, roster, builder) = scene();
        let locked = user.join("GC").join("MemoryCardA.USA.raw");
        let platform = platform_with(fake(ExitKind::Normal), Some(&locked));
        let req = SyncRequest { data_root: dir.path(), roster: &roster, builder: &builder, user_dir: &user, harvest_only: false, dry_run: false };
        let SyncOutcome::Done(results) = sync(&platform, &req).unwrap() else { panic!("done") };
        assert!(matches!(&results[0], CardResult::Refused(CardRefusal::InUse { path }) if *path == locked));
        assert!(matches!(&results[1], CardResult::Synced(_)));
        let copies: Vec<String> = std::fs::read_dir(recovery_dir(dir.path())).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        assert!(copies.iter().all(|n| n.starts_with("MemoryCardB.USA.raw.")), "{copies:?}");
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
        assert!(CardsAfterSession::Synced(SyncOutcome::Refused(SyncRefusal::NoBuilder { looked: vec![] })).explain().is_some());
    }
```

Note on `sync_runs_the_builder_once_per_card...`: delete the two lines that call `platform.processes.run_capturing` directly and the `drop(calls)` line; they are scaffolding left from drafting and the assertions below them do the work. The argument check lives in `the_arguments_are_the_verb...`.

- [ ] **Step 3: Run the tests**

Run: `cargo test -p reborn-core cards::`
Expected: 11 passed. If `ProcessHandle` or `LockGuard` is not re-exported under `crate::platform`, import them from where `platform/mod.rs` defines them (`grep -n "pub struct ProcessHandle\|pub struct LockGuard" crates/reborn-core/src/platform/mod.rs`).

- [ ] **Step 4: Commit**

```bash
git add crates/reborn-core/src/cards.rs
git commit -m "cards: sync every card through the builder, refuse a locked one, copy first"
```

---

### Task 3: After every session

**Files:**
- Modify: `crates/reborn-core/src/session.rs` (`Launched::Finished` at lines 100-126; `Session::run` after the `let stats = { ... };` block at lines 703-711)
- Modify: `crates/reborn-cli/src/main.rs` (the `Launched::Finished { .. }` arm near line 1263)
- Modify: `crates/reborn-ui/src/main.rs` (the `Launched::Finished { .. }` arm near line 712)

- [ ] **Step 1: Carry the outcome in the launch result**

In `session.rs`, add to `Launched::Finished` after the `stats` field:

```rust
        /// What happened to the memory cards' gear edits after the game
        /// closed. Never fails a launch that already happened.
        cards: crate::cards::CardsAfterSession,
```

In `Session::run`, after the `let stats = { ... };` block and before `let end = ...`:

```rust
        // The member's in-game gear edits, harvested off the memory cards
        // while the lock is still held and the game is known to be closed.
        // Nothing is written to a card here; the roster apply does that.
        let cards = crate::cards::after_session(platform, written.user_dir.path());
```

and add `cards,` to the `Launched::Finished { ... }` constructor at the end of `run`.

- [ ] **Step 2: Show the line where the stats line is shown**

In `crates/reborn-cli/src/main.rs`, in the `Launched::Finished { .. }` arm (it destructures with `..`), add `cards,` to the pattern and, right after the `if let Some(line) = stats.explain() { ... }` block:

```rust
            if let Some(line) = cards.explain() {
                println!("{line}");
            }
```

In `crates/reborn-ui/src/main.rs`, in the `Launched::Finished { .. }` arm near line 712, add `cards,` to the pattern and append `cards.explain()` to whatever list of lines that view returns beside the stats line (grep `stats.explain()` in that file; the same `if let Some(line)` shape, pushing onto the same vector).

- [ ] **Step 3: Build, test, check**

Run: `cargo build --workspace` then `cargo test --workspace`
Expected: green. The `session.rs` tests that construct `Launched::Finished` by hand, if any, need the new field; grep `Launched::Finished {` in tests and add `cards: crate::cards::CardsAfterSession::Synced(crate::cards::SyncOutcome::Done(Vec::new()))`.

Run: `node Tools/check-shell-pages.mjs`
Expected: PASS (no page calls a new command; the Play page still draws under every answer).

- [ ] **Step 4: Commit**

```bash
git add crates/reborn-core/src/session.rs crates/reborn-cli/src/main.rs crates/reborn-ui/src/main.rs
git commit -m "session: harvest gear edits off the memory cards after every game"
```

---

### Task 4: After a roster apply, and the `cards sync` verb

**Files:**
- Modify: `crates/reborn-cli/src/main.rs` (`Command` enum near line 105; dispatch near line 662; `roster_apply` near line 4433)
- Modify: `crates/reborn-ui/src/main.rs` (`content_update` command)

- [ ] **Step 1: Sync after `roster apply`**

In `roster_apply`, after the match on `roster::apply(...)` where `ApplyOutcome::Applied` and `ApplyOutcome::Unchanged` are printed and before the function returns `OK`, add:

```rust
    // The memory cards, brought up to date with the roster that is now in the
    // image. Harmless when nothing changed: the Builder reports "unchanged".
    let user_dir = reborn_core::session::resolve_user_dir(platform, None)?;
    match reborn_core::cards::after_apply(platform, &root, user_dir.path(), &legacy_tools_dirs())? {
        reborn_core::cards::SyncOutcome::Refused(r) => println!("{}", r.message()),
        outcome => {
            for line in outcome.explain() {
                println!("{line}");
            }
        }
    }
```

(`root` is the data root already resolved at the top of `roster_apply`; `legacy_tools_dirs()` is the existing helper.) Do not run it on `ApplyOutcome::Refused`.

- [ ] **Step 2: The verb**

In the `Command` enum, after `Roster { .. }`:

```rust
    /// Bring the memory cards' gear up to date with the installed roster, keeping in-game edits.
    Cards {
        #[command(subcommand)]
        what: CardsCommand,
    },
```

A new subcommand enum beside `RosterCommand`:

```rust
#[derive(Subcommand, Debug)]
enum CardsCommand {
    /// Run the Reborn Builder's card-sync over every memory card in the Dolphin user folder.
    Sync {
        /// The .rebornroster. Defaults to the one the roster component installed.
        #[arg(long)]
        roster: Option<PathBuf>,
        /// The Builder's console build. Found automatically when absent.
        #[arg(long)]
        builder: Option<PathBuf>,
        /// Use this data root instead of the real one. For tests.
        #[arg(long)]
        data_root: Option<PathBuf>,
        /// The Dolphin user folder. Defaults to the one a launch would use.
        #[arg(long)]
        user_dir: Option<PathBuf>,
        /// Record in-game edits only; write nothing to any card.
        #[arg(long)]
        harvest_only: bool,
        /// Report what would change; write nothing.
        #[arg(long)]
        dry_run: bool,
    },
}
```

Dispatch, beside the `Command::Roster` arm:

```rust
        Command::Cards { what } => match what {
            CardsCommand::Sync { roster, builder, data_root, user_dir, harvest_only, dry_run } => cards_sync(
                &platform,
                roster.as_deref(),
                builder.as_deref(),
                data_root.as_deref(),
                user_dir.as_deref(),
                harvest_only,
                dry_run,
            ),
        },
```

The handler, beside `roster_apply`:

```rust
/// Sync every memory card with the Builder's card-sync verb.
fn cards_sync(
    platform: &Platform,
    roster: Option<&Path>,
    builder: Option<&Path>,
    data_root: Option<&Path>,
    user_dir: Option<&Path>,
    harvest_only: bool,
    dry_run: bool,
) -> reborn_core::Result<std::process::ExitCode> {
    use reborn_core::cards::{self, SyncOutcome, SyncRequest};
    use reborn_core::roster::{self, ROSTER_FILE};

    let root = match data_root {
        Some(p) => p.to_path_buf(),
        None => platform.paths.data_root()?,
    };
    let roster_path = match roster {
        Some(p) => p.to_path_buf(),
        None => root.join(DataDir::LibraryRoster.relative()).join(ROSTER_FILE),
    };
    let builder_path = match builder {
        Some(p) => p.to_path_buf(),
        None => {
            let dirs = roster::builder_dirs(platform, &legacy_tools_dirs());
            match roster::find_builder(platform, &dirs) {
                Some(p) => p,
                None => {
                    println!("{}", cards::SyncRefusal::NoBuilder { looked: dirs }.message());
                    return Ok(PROBLEM);
                }
            }
        }
    };
    let user = match user_dir {
        Some(p) => p.to_path_buf(),
        None => reborn_core::session::resolve_user_dir(platform, None)?.path().to_path_buf(),
    };
    let req = SyncRequest { data_root: &root, roster: &roster_path, builder: &builder_path, user_dir: &user, harvest_only, dry_run };
    match cards::sync(platform, &req)? {
        SyncOutcome::Refused(r) => {
            println!("{}", r.message());
            Ok(PROBLEM)
        }
        outcome => {
            let lines = outcome.explain();
            if lines.is_empty() {
                println!("Every memory card is up to date, and no new gear edits were found.");
            }
            for line in lines {
                println!("{line}");
            }
            Ok(OK)
        }
    }
}
```

- [ ] **Step 3: After a promote in the window**

In `crates/reborn-ui/src/main.rs`, find the `content_update` command (grep `fn content_update`). After the promote returns and where its `roster: Option<RosterResult>` is turned into text for the view, add, only when the roster result is `Applied` or `Unchanged`:

```rust
                let user = reborn_core::session::resolve_user_dir(&p, None).map_err(|e| e.to_string())?;
                match reborn_core::cards::after_apply(&p, layout.data_root(), user.path(), &[]).map_err(|e| e.to_string())? {
                    reborn_core::cards::SyncOutcome::Refused(r) => lines.push(r.message()),
                    outcome => lines.extend(outcome.explain()),
                }
```

where `p` is the `Platform` the command already detected, `layout.data_root()` is the data root it already uses, and `lines` is the `Vec<String>` the view already returns for the roster step. If the view has no such vector, add a `cards: Vec<String>` field to the view struct, fill it here, and render it in `settings.js` directly under the roster line (one `<p>` per entry; the shell pages checker must still pass with the field absent, null, and rejected).

- [ ] **Step 4: Build, test, check**

Run: `cargo build --workspace`, `cargo test --workspace`, then every checker: `Get-ChildItem Tools\check-*.mjs | ForEach-Object { node $_.FullName }`.
Expected: green. `check-harness-args.mjs` reports no new command (none was added). `check-page-call-args.mjs` passes because `settings.js` invokes no new command.

- [ ] **Step 5: Try the verb for real, dry run**

Run: `.\target\debug\rebornctl.exe cards sync --dry-run --data-root C:\Users\Adam\AppData\Roaming\NFL Street Reborn Launcher Beta --builder "C:\Users\Adam\AppData\Roaming\NFL Street Reborn\Tools\reborn-builder-cli.exe" --roster <a .rebornroster downloaded after part 2 deployed> --user-dir "C:\Users\Adam\AppData\Roaming\NFL Street Reborn\User"`
Expected: exit 0 and one line per card, for example `Memory card MemoryCardB.USA.raw: <n> gear row(s) would change, <n> in-game edit(s) kept.`, or the quiet sentence. With a roster file that has no `history.json` (any export before part 2 deployed) each profile is left as it was and the line names them.

- [ ] **Step 6: Commit**

```bash
git add crates/reborn-cli/src/main.rs crates/reborn-ui/src/main.rs
git commit -m "cards sync: after a roster apply, and as a verb"
```

---

### Task 5: End to end on this machine, and the guide

**Files:**
- Create: `Tools/e2e-cards.py`
- Modify: `docs/RUNNING_UPDATES.md` (the "what can be updated" table and a short section)

- [ ] **Step 1: The e2e script, modeled on `Tools/e2e-roster.py`**

`Tools/e2e-cards.py`:

```python
"""End to end: a real card copy, the real Builder, a real roster with history, the real verb.

Skips (exit 0, says why) when any input is absent, like e2e-roster.py. Never touches the
real card: it copies the 2026-09-06 backup into a scratch user folder.

    py -3 Tools/e2e-cards.py <path to a .rebornroster that carries history.json>
"""
import os, shutil, subprocess, sys, tempfile, zipfile
from pathlib import Path

APPDATA = Path(os.environ["APPDATA"])
BUILDER = APPDATA / "NFL Street Reborn" / "Tools" / "reborn-builder-cli.exe"
CARD = APPDATA / "NFL Street Reborn" / "User" / "GC" / "MemoryCardB.USA.raw.bak_pre_gearsync_probe_20260906_141745"
CLI = Path(__file__).resolve().parent.parent / "target" / "debug" / "rebornctl.exe"


def skip(why):
    print("SKIP:", why)
    sys.exit(0)


def main():
    if len(sys.argv) != 2:
        skip("no roster file given")
    roster = Path(sys.argv[1])
    for p, what in [(BUILDER, "builder"), (CARD, "card backup"), (CLI, "rebornctl (cargo build -p reborn-cli)"), (roster, "roster")]:
        if not p.is_file():
            skip("%s missing: %s" % (what, p))
    if "history.json" not in zipfile.ZipFile(roster).namelist():
        skip("that roster carries no history.json; export one after the site change is deployed")
    with tempfile.TemporaryDirectory() as td:
        root = Path(td) / "data"
        user = Path(td) / "User"
        (user / "GC").mkdir(parents=True)
        shutil.copy2(CARD, user / "GC" / "MemoryCardB.USA.raw")
        before = (user / "GC" / "MemoryCardB.USA.raw").read_bytes()
        common = ["cards", "sync", "--data-root", str(root), "--roster", str(roster), "--builder", str(BUILDER), "--user-dir", str(user)]
        r = subprocess.run([str(CLI), *common, "--dry-run"], capture_output=True, text=True)
        print(r.stdout)
        assert r.returncode == 0, r.stderr
        assert (user / "GC" / "MemoryCardB.USA.raw").read_bytes() == before, "dry run wrote the card"
        assert not (root / "recovery").exists(), "dry run took a recovery copy"
        r = subprocess.run([str(CLI), *common], capture_output=True, text=True)
        print(r.stdout)
        assert r.returncode == 0, r.stderr
        state = root / "library" / "roster" / "cards" / "MemoryCardB.USA.raw.json"
        assert state.is_file(), "no state file"
        copies = list((root / "recovery" / "cards").glob("MemoryCardB.USA.raw.*.bak"))
        assert len(copies) == 1 and copies[0].read_bytes() == before, "no recovery copy of the untouched card"
        r2 = subprocess.run([str(CLI), *common], capture_output=True, text=True)
        assert r2.returncode == 0 and "up to date" in r2.stdout or r2.stdout.strip() == "", r2.stdout
        print("OK")


if __name__ == "__main__":
    main()
```

- [ ] **Step 2: Run it**

Run: `cargo build -p reborn-cli` then `py -3 Tools/e2e-cards.py <roster with history>`
Expected: two reports and `OK`. The first full run bootstraps all four profiles from the history; the second run says up to date.

- [ ] **Step 3: The guide**

In `docs/RUNNING_UPDATES.md`, add a row to the "what can be updated" table: `memory card gear | after every roster apply and every session, by the Builder's card-sync; recovery copies under recovery/cards | rebornctl cards sync --dry-run`. Add a short section "Memory card gear" saying: the launcher never writes a card while the game runs; it copies a card before writing it and keeps three copies; a member's in-game gear edits are recorded after every session and follow the person across roster changes; a card the launcher has never written is reconciled against the roster's own history, floor 2.1; the Builder console build must be installed for any of this to happen, and the line "No Reborn Builder console build was found" after a session means it is not.

```bash
git add Tools/e2e-cards.py docs/RUNNING_UPDATES.md
git commit -m "cards: end to end on a real card copy, and the guide"
```

---

## Self-review against the spec

- "Run after `roster apply` and after each session; skip when unchanged": Task 3 (harvest after session), Task 4 (full after apply; the Builder's own `unchanged` makes the skip). Covered.
- Quiescence rule: `is_locked` refusal, post-session inside the lock after `wait`, post-apply only where the game is closed (`roster apply` refuses `ImageInUse` first; the window's update path requires the game closed). Covered.
- Recovery point: `recovery_point`, newest three. Covered.
- Refusals as facts, no popups: every bad state is a variant with a sentence; the window renders lines. Covered.
- Unparseable card or unknown layout: the Builder skips the profile and says so; the launcher shows the profile names. Covered by `skipped_reason`.
- State file path `library/roster/cards/<card file name>.json`: `state_path`. Covered.
- Prerequisite outside this plan: the installer must ship `reborn-builder-cli.exe` where `roster::builder_dirs` looks (install root `tools`). That is item 3 of the update-system notes and blocks this feature at launch if missed.
