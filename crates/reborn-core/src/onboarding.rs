//! First run, as a state machine with its progress on disk.
//!
//! # Why this is a state machine and not a counter
//!
//! The obvious shape for a setup flow is an integer: which screen are we on.
//! That shape has three failures this module exists to prevent.
//!
//! **It restarts.** A player who closes the launcher during the texture copy
//! comes back to screen one, answers everything again, and pays for the copy
//! twice. `WIREFRAMES.md` section 5 makes a promise in the interface text: "It
//! picks up where it left off if you close the launcher." A counter that lives
//! only in memory cannot keep that promise.
//!
//! **It redoes work that has already happened.** Two stages here write files
//! outside the journal: bringing a 2.1 install over, and preparing the install.
//! Re-running those is not merely slow. Going *back* to the migration offer
//! after the copy has run and choosing "do not bring over" would be recording a
//! decision that disk already contradicts.
//!
//! **It gets shown twice.** Whether a stage is needed depends on the machine: a
//! 2.1 install is present or it is not, a controller is attached or it is not.
//! Deriving "is onboarding done" from those live facts means unplugging a pad,
//! or reinstalling 2.1, drags a settled player back through first run. So
//! completion is *latched*: one terminal entry in the journal, written once,
//! never recomputed from the environment. See [`Onboarding::finish`].
//!
//! # The stages, and where they come from
//!
//! `WIREFRAMES.md` sections 1 to 5 specify five screens. Two more stages are
//! forced by [`crate::migration`]: the offer, which section 2 draws as a panel
//! inside game verification, and the copy itself, which section 5 draws as
//! nothing at all. They are separate stages here because they have different
//! properties: the offer is a reversible question, and the copy writes files.
//!
//! The order follows the step numbers printed on the wireframes (verification
//! is "Step 3 of 12", graphics is "Step 5 of 12", controller is "Step 7 of
//! 12", preparation is "Step 11 of 12"), which puts graphics before
//! controller. Note that this contradicts the order the same document lists its
//! own sections in, where controller setup is section 3 and graphics is section
//! 4. The printed step numbers were taken as authoritative because they are the
//! only statement of *order* in the document, while the section numbering is a
//! document ordering. This is a real ambiguity in the source and is worth
//! resolving there.
//!
//! **The migration copy sits before preparation, and that is the point.**
//! `migration.rs` orders its own work saves-first so that an interruption
//! cannot cost anything permanent, and its test says a 2 GB copy "must not sit
//! between the user and their saves". Preparation is the multi-gigabyte step of
//! this flow. Putting it first would put exactly that copy between a player and
//! the memory cards still sitting in the folder the 2.1 uninstaller deletes.
//!
//! # The step count is not twelve
//!
//! `INFORMATION_ARCHITECTURE.md` says onboarding is twelve steps and the
//! wireframes label four of them. Seven of the twelve are specified nowhere.
//! [`Onboarding::plan`] therefore counts the stages that actually exist and
//! apply to this machine. A hardcoded "of 12" would be an invented total, and
//! the label would drift the moment somebody wired up the real screens.
//!
//! # No user-facing strings for stages
//!
//! This module ships no screen titles. Five of the eight stages have no
//! wireframed heading, so any title here would be invented interface text that
//! looks specified. The front end owns every string a player reads. The
//! exceptions are [`Refusal::message`], which follows `session.rs` in owning
//! its own explanations, and [`PrepTask::label`], where the five task names are
//! quoted from a locked ruling rather than written here.
//!
//! # Ordering of writes
//!
//! Every mutation appends one line and flushes it before the in-memory view is
//! updated, so the view never claims more than disk holds. A stage that writes
//! files gets two entries, [`Event::Reached`] before the work and
//! [`Event::Satisfied`] after, following the same reasoning as
//! `supervisor.rs`: the before-marker is what makes "interrupted halfway"
//! distinguishable from "never started". Without it, the two look identical and
//! the only safe behavior would be to redo the work.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::controller::DeviceFamily;
use crate::devices::Enumeration;
use crate::mods::ModPackage;
use crate::migration::{LegacyInstall, MigrationPlan};
use crate::platform::{DataDir, Error, Platform, Result};
use crate::runtime::GraphicsPreset;
use crate::supervisor::{Clock, SystemClock};

/// The shape of the flow this binary knows how to run.
///
/// Recorded with the completion latch so a later launcher can see which flow a
/// player finished. It is deliberately *not* consulted by
/// [`Progress::is_complete`]: a finished onboarding stays finished even when
/// this constant moves. Re-running first run to collect one new answer would
/// re-offer migration and re-run preparation, so a future stage has to be asked
/// as its own targeted prompt, not by rewinding the whole flow.
pub const FLOW_VERSION: u32 = 1;

/// The journal filename, under [`DataDir::Config`]. Named here so nothing else
/// guesses it.
const JOURNAL: &str = "onboarding.jsonl";

// ---------------------------------------------------------------------------
// Stages
// ---------------------------------------------------------------------------

/// One stage of first run.
///
/// The serialized name is the on-disk key. [`Stage::slug`] returns the same
/// string and a test pins them together, so renaming a variant fails loudly
/// instead of silently orphaning every journal already written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    /// `WIREFRAMES.md` section 1.
    Welcome,
    /// `WIREFRAMES.md` section 2. Confirms the shipped image; there is no
    /// import pipeline (`RECONCILIATION_RULINGS.md` 105).
    GameVerification,
    /// The "Already have Reborn installed?" panel of section 2, as its own
    /// stage because it is a decision and the copy that follows is not.
    MigrationOffer,
    /// Doing the migration. Writes into the data root.
    MigrationCopy,
    /// `WIREFRAMES.md` section 4.
    SystemCheck,
    /// `WIREFRAMES.md` section 3.
    ControllerSetup,
    /// `WIREFRAMES.md` section 5, first screen. Writes into the data root.
    /// Choose which mods to turn on.
    ///
    /// In the launcher rather than the installer, and BEFORE
    /// [`Stage::Preparation`], which is what actually applies them.
    ///
    /// The 2.1 installer opened JSGME mid-wizard for this, which is the worst
    /// possible moment. JSGME moves the original files aside as it applies, so a
    /// wizard abandoned halfway leaves a half-composed game and a !BACKUP folder
    /// nobody looks at again. Choosing here only records a decision; the
    /// composition happens in Preparation where it can be verified and rebuilt.
    ModSelection,
    Preparation,
    /// `WIREFRAMES.md` section 5, second screen. Closed by
    /// [`Onboarding::finish`], not by [`Onboarding::satisfy`].
    Completion,
}

impl Stage {
    /// Every stage, in flow order.
    pub const ALL: &'static [Stage] = &[
        Stage::Welcome,
        Stage::GameVerification,
        Stage::MigrationOffer,
        Stage::MigrationCopy,
        Stage::SystemCheck,
        Stage::ControllerSetup,
        Stage::ModSelection,
        Stage::Preparation,
        Stage::Completion,
    ];

    /// The stable on-disk key.
    pub fn slug(self) -> &'static str {
        match self {
            Stage::Welcome => "welcome",
            Stage::GameVerification => "game_verification",
            Stage::MigrationOffer => "migration_offer",
            Stage::MigrationCopy => "migration_copy",
            Stage::SystemCheck => "system_check",
            Stage::ModSelection => "mod_selection",
            Stage::ControllerSetup => "controller_setup",
            Stage::Preparation => "preparation",
            Stage::Completion => "completion",
        }
    }

    /// Which `WIREFRAMES.md` section drew this stage, when one did.
    ///
    /// `None` is not a gap to be filled in with a guess: it means the screen is
    /// unspecified, and whoever builds it is designing rather than implementing.
    pub fn wireframe_section(self) -> Option<u8> {
        match self {
            Stage::Welcome => Some(1),
            Stage::GameVerification | Stage::MigrationOffer => Some(2),
            Stage::ControllerSetup => Some(3),
            Stage::SystemCheck => Some(4),
            Stage::Preparation | Stage::Completion => Some(5),
            // No wireframe drew either of these. Mod choice used to live in
            // the installer, so there is nothing to implement against and
            // whoever builds the screen is designing rather than following.
            Stage::MigrationCopy | Stage::ModSelection => None,
        }
    }

    /// Whether this stage's effects live outside the journal.
    ///
    /// Two do. The migration copy puts the player's memory cards in a new
    /// place, and preparation lays down the game, the emulator and two
    /// gigabytes of textures. Once either has run, re-entering it either
    /// repeats work the player already waited for, or records an answer that
    /// disk already contradicts. Both are refused rather than merely avoided.
    pub fn is_irreversible(self) -> bool {
        matches!(self, Stage::MigrationCopy | Stage::Preparation)
    }

    /// Whether B may leave this stage backwards.
    ///
    /// `INFORMATION_ARCHITECTURE.md` section 3: "Onboarding step | previous
    /// step, if the step is safely reversible."
    pub fn allows_back(self) -> bool {
        !matches!(
            self,
            // Nothing before it.
            Stage::Welcome
                // Work in progress, and work already done.
                | Stage::MigrationCopy
                | Stage::Preparation
                // Backing out of Ready would re-enter preparation.
                | Stage::Completion
        )
    }

    /// Whether the stage can be answered with a decline.
    pub fn skip_rule(self) -> SkipRule {
        match self {
            Stage::MigrationOffer => SkipRule::Declinable {
                because: "leaving a 2.1 install alone is a real answer, and the \
                          launcher never deletes it either way",
            },
            Stage::ControllerSetup => SkipRule::Declinable {
                because: "a player with nothing plugged in must not be trapped in \
                          first run. The refusal is recorded so the launcher can \
                          say there is no pad instead of starting the game into \
                          nothing",
            },
            Stage::ModSelection => SkipRule::Declinable {
                because: "no mods is the normal case and a perfectly good \
                          answer. Requiring a choice would also mean requiring \
                          one before the player has seen the game run once",
            },
            Stage::Welcome => SkipRule::Required {
                because: "it is the entry point, and it carries the time estimate",
            },
            Stage::GameVerification => SkipRule::Required {
                because: "the player is told what is about to be installed and \
                          where, before any of it is written",
            },
            Stage::MigrationCopy => SkipRule::Required {
                because: "it only exists once the player has asked for it; \
                          declining happens at the offer",
            },
            Stage::SystemCheck => SkipRule::Required {
                because: "the emulator configuration is regenerated from a \
                          graphics preset on every launch, so there has to be one",
            },
            Stage::Preparation => SkipRule::Required {
                because: "nothing is playable until it has run",
            },
            Stage::Completion => SkipRule::Required {
                because: "the player is shown what was decided before it is latched",
            },
        }
    }

    /// Whether this evidence belongs to this stage.
    ///
    /// Checked on every write. A [`Satisfaction::Graphics`] filed against
    /// [`Stage::ControllerSetup`] would make the summary claim a pad was tested
    /// when none was, and the Ready screen would print it.
    pub fn accepts(self, how: &Satisfaction) -> bool {
        match self {
            Stage::Welcome => matches!(how, Satisfaction::Acknowledged),
            Stage::GameVerification => matches!(how, Satisfaction::GameConfirmed { .. }),
            Stage::MigrationOffer => matches!(
                how,
                Satisfaction::Migrating { .. } | Satisfaction::MigrationDeclined
            ),
            Stage::MigrationCopy => matches!(how, Satisfaction::Migrated { .. }),
            Stage::SystemCheck => matches!(how, Satisfaction::Graphics { .. }),
            Stage::ControllerSetup => matches!(
                how,
                Satisfaction::Controller { .. } | Satisfaction::NoController { .. }
            ),
            Stage::ModSelection => matches!(
                how,
                Satisfaction::ModsChosen { .. } | Satisfaction::NoMods
            ),
            Stage::Preparation => matches!(how, Satisfaction::Prepared),
            // Closed by finish(), never by satisfy(). Two ways to end
            // onboarding is how one of them stops being maintained.
            Stage::Completion => false,
        }
    }

    fn index(self) -> usize {
        Stage::ALL
            .iter()
            .position(|s| *s == self)
            // Unreachable while ALL is complete, which a test enforces. Zero is
            // the safe fallback: it orders this stage first, which can only make
            // an ordering check stricter, never looser.
            .unwrap_or(0)
    }
}

/// Whether a stage can be answered with a decline, and why that is the rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipRule {
    /// Onboarding cannot be latched complete without an answer.
    Required { because: &'static str },
    /// Declining is itself an answer, and it is recorded as one.
    Declinable { because: &'static str },
}

impl SkipRule {
    pub fn because(self) -> &'static str {
        match self {
            SkipRule::Required { because } | SkipRule::Declinable { because } => because,
        }
    }

    pub fn is_declinable(self) -> bool {
        matches!(self, SkipRule::Declinable { .. })
    }
}

// ---------------------------------------------------------------------------
// Preparation tasks
// ---------------------------------------------------------------------------

/// One named task in the preparation list.
///
/// Recorded individually so a resumed run does not repeat a copy the player
/// already waited through. That is a time saving rather than a safety property:
/// all five are safe to repeat. The safety property lives on
/// [`Stage::MigrationCopy`], where repeating means touching the player's saves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrepTask {
    CopyGameFiles,
    InstallRuntime,
    ApplyTextures,
    ApplySettings,
    CheckEverything,
}

impl PrepTask {
    /// The five tasks, in order.
    pub const ALL: &'static [PrepTask] = &[
        PrepTask::CopyGameFiles,
        PrepTask::InstallRuntime,
        PrepTask::ApplyTextures,
        PrepTask::ApplySettings,
        PrepTask::CheckEverything,
    ];

    /// The stable on-disk key.
    pub fn slug(self) -> &'static str {
        match self {
            PrepTask::CopyGameFiles => "copy_game_files",
            PrepTask::InstallRuntime => "install_runtime",
            PrepTask::ApplyTextures => "apply_textures",
            PrepTask::ApplySettings => "apply_settings",
            PrepTask::CheckEverything => "check_everything",
        }
    }

    /// The task name, quoted rather than written.
    ///
    /// `RECONCILIATION_RULINGS.md` 105 fixes this list as "the corrected
    /// preparation task list in WIREFRAMES section 5 (copy game files, install
    /// Reborn Dolphin, apply textures, apply your controller and graphics,
    /// check everything)". These five strings are the only user-facing text in
    /// this module that names a screen element, because they are a locked
    /// decision rather than a design choice made here.
    pub fn label(self) -> &'static str {
        match self {
            PrepTask::CopyGameFiles => "Copy game files",
            PrepTask::InstallRuntime => "Install Reborn Dolphin",
            PrepTask::ApplyTextures => "Apply textures",
            PrepTask::ApplySettings => "Apply your controller and graphics",
            PrepTask::CheckEverything => "Check everything",
        }
    }
}

// ---------------------------------------------------------------------------
// Evidence
// ---------------------------------------------------------------------------

/// Which graphics preset was chosen, as it is written to disk.
///
/// Mirrors [`GraphicsPreset`] for the reason `supervisor.rs` mirrors
/// `GameBuild`: the disk format is ours to keep readable, and it must not change
/// silently when an enum in the runtime module is refactored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RecordedPreset {
    Performance,
    Balanced,
    Quality,
    Ultra,
}

impl From<GraphicsPreset> for RecordedPreset {
    fn from(p: GraphicsPreset) -> Self {
        match p {
            GraphicsPreset::Performance => RecordedPreset::Performance,
            GraphicsPreset::Balanced => RecordedPreset::Balanced,
            GraphicsPreset::Quality => RecordedPreset::Quality,
            GraphicsPreset::Ultra => RecordedPreset::Ultra,
        }
    }
}

impl RecordedPreset {
    /// Back to the runtime's own type, for a launch.
    pub fn preset(self) -> GraphicsPreset {
        match self {
            RecordedPreset::Performance => GraphicsPreset::Performance,
            RecordedPreset::Balanced => GraphicsPreset::Balanced,
            RecordedPreset::Quality => GraphicsPreset::Quality,
            RecordedPreset::Ultra => GraphicsPreset::Ultra,
        }
    }
}

/// What satisfied a stage.
///
/// Every field that could be absent is [`Option`], and absent means nobody
/// measured it. The design documents are full of guillemet placeholders
/// («4.4 GB», «Ryzen 5 5600», «84%»), and `CONTENT_AND_UPDATE_MODEL.md` records
/// that "the install-size numbers in the design documents are wrong in both
/// directions". Carrying a default here would put one of those numbers on the
/// Ready screen looking measured.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "how", rename_all = "snake_case")]
pub enum Satisfaction {
    /// The player read the screen and moved on. Nothing was decided.
    Acknowledged,
    /// The shipped image was confirmed. `total_bytes` is what the caller
    /// actually measured, and `None` when nothing did.
    GameConfirmed { total_bytes: Option<u64> },
    /// Bring the 2.1 install over. The source is carried so
    /// [`Stage::MigrationCopy`] cannot be pointed at a folder the player was
    /// never shown.
    Migrating { source: PathBuf },
    /// Leave the 2.1 install where it is. Nothing is deleted either way.
    MigrationDeclined,
    /// The copy finished. `bytes` is what was copied, not an estimate.
    Migrated { bytes: u64 },
    /// A pad the player tested themselves. `device` is the emulator's own
    /// `source/id/name` string from [`crate::devices`], not a friendly name: a
    /// friendly name written into `GCPadNew.ini` parses fine and binds nothing.
    Controller {
        device: String,
        family: DeviceFamily,
        port: u8,
    },
    /// No pad, and why. Recorded rather than left blank, so the launcher can
    /// say so instead of starting the game into nothing.
    NoController { because: String },
    /// A preset was chosen. `recommended` is `None` when the machine was never
    /// measured, in which case the wireframe's "RECOMMENDED FOR YOUR PC" badge
    /// has nothing behind it and must not be drawn. `detected` is `None` when no
    /// hardware was read, in which case the detected-hardware line is omitted
    /// rather than filled with a plausible CPU name.
    Graphics {
        chosen: RecordedPreset,
        recommended: Option<RecordedPreset>,
        detected: Option<String>,
    },
    /// Mods the player turned on, by package reference.
    ///
    /// References rather than display names: two mods can share a name across
    /// versions, and a name that resolves to the wrong package composes the
    /// wrong files.
    ///
    /// `conflicts_shown` records that overlapping writes were put in front of
    /// the player. On a real install 20 paths are written by more than one mod,
    /// with 7 writing the same football texture, so keeping whichever landed
    /// last is a decision made FOR someone. False means the launcher has not
    /// asked, and must not present the selection as informed.
    ModsChosen {
        packages: Vec<String>,
        conflicts_shown: bool,
    },
    /// No mods, deliberately. Distinct from never having been asked.
    NoMods,
    /// Every preparation task finished.
    Prepared,
}

impl Satisfaction {
    /// The controller answer for a port, taking the pad's identity from an
    /// enumeration.
    ///
    /// A caller passes a PORT, never a device name. The string this returns ends
    /// up in `GCPadNew.ini`, and it has to be the emulator's own
    /// `source/id/name`: a friendly name parses fine there and binds nothing, so
    /// the game would start with a pad that does nothing and no error anywhere.
    ///
    /// The refusal distinguishes a complete search from a partial one. "No pad"
    /// and "no pad, but only one backend was searched" send someone to do
    /// different things, and on Windows the partial case is the common one,
    /// because the built in scan sees XInput and misses WGInput entirely.
    pub fn controller_on_port(scan: &Enumeration, port: u8) -> std::result::Result<Self, String> {
        let pads = scan.playable_pads();
        let pad = pads.get(usize::from(port)).ok_or_else(|| {
            if scan.is_complete() {
                format!(
                    "no pad on port {}, and {} were found. Plug one in, or \
                     wake it: a wireless pad that has gone to sleep is not \
                     listed.",
                    port + 1,
                    pads.len()
                )
            } else {
                format!(
                    "no pad on port {}, but only XInput was searched, so a \
                     pad on another backend would not appear here even \
                     though the game would see it. Point this at the \
                     emulator to search the rest.",
                    port + 1
                )
            }
        })?;
        Ok(Satisfaction::Controller {
            device: pad.device_string(),
            family: pad.identity().family,
            port,
        })
    }

    /// The mods answer for packages that have already been resolved on disk.
    ///
    /// Takes loaded packages rather than references, so a name that resolves to
    /// nothing cannot reach the journal. The recorded strings are the packages'
    /// own id and version, not whatever the caller typed.
    ///
    /// `conflicts_shown` is the caller's claim about what the player was shown,
    /// which nothing here can measure, so it is passed through rather than
    /// inferred. It is worth carrying because on a real library seven of the
    /// eleven mods write the same football texture, and keeping whichever landed
    /// last is a decision made for someone.
    pub fn mods_chosen(packages: &[ModPackage], conflicts_shown: bool) -> Self {
        Satisfaction::ModsChosen {
            packages: packages
                .iter()
                .map(|m| format!("{}@{}", m.id.as_str(), m.version.as_str()))
                .collect(),
            conflicts_shown,
        }
    }
}


// ---------------------------------------------------------------------------
// The journal
// ---------------------------------------------------------------------------

/// One line of the journal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub at_ms: u64,
    pub event: Event,
}

/// What happened, as it is written to disk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    /// The player is looking at this stage. For an irreversible stage this is
    /// the before-marker: written before any file is touched, so an interrupted
    /// run can tell "may have partly happened" from "never started".
    Reached { stage: Stage },
    /// The stage's requirement is met, and this is what met it.
    Satisfied { stage: Stage, how: Satisfaction },
    /// One preparation task finished.
    TaskDone { task: PrepTask },
    /// Onboarding is over. Terminal. Nothing is appended after this.
    Completed { flow_version: u32 },
}

/// The journal, and the view derived from it.
#[derive(Debug, Clone, Default)]
pub struct Progress {
    entries: Vec<Entry>,
    truncated: bool,
}

impl Progress {
    /// Read the journal, or an empty one if there is no file.
    ///
    /// Reading stops at the first line that will not parse, and
    /// [`Progress::is_truncated`] is set. That is a deliberate trade, and it
    /// differs from `supervisor.rs`, which skips bad lines and carries on.
    /// Skipping is right for a play-history log, where a lost line costs one row
    /// on a stats page. Here a skipped line could be the record of a copy that
    /// already happened, and carrying on past it would let the flow redo it.
    /// Stopping short instead makes the state provably incomplete, and
    /// [`Onboarding::next`] then routes every irreversible stage to
    /// [`Next::Reconcile`] rather than re-running it.
    ///
    /// The alternative, refusing to open at all, was rejected: a corrupt
    /// progress file would make the launcher unusable, and nothing here is worth
    /// that.
    pub fn read(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let raw = std::fs::read_to_string(path).map_err(|source| Error::Io {
            path: path.to_path_buf(),
            source,
        })?;

        let mut entries = Vec::new();
        let mut truncated = false;
        for line in raw.lines() {
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<Entry>(line) {
                Ok(e) => entries.push(e),
                Err(_) => {
                    truncated = true;
                    break;
                }
            }
        }
        Ok(Self { entries, truncated })
    }

    /// Every entry, oldest first.
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// Whether reading stopped early, which makes this view untrustworthy about
    /// anything it does not contain.
    pub fn is_truncated(&self) -> bool {
        self.truncated
    }

    /// What satisfied a stage, if anything. The most recent answer wins, so a
    /// player who goes back and changes a reversible answer gets the new one.
    pub fn satisfaction(&self, stage: Stage) -> Option<&Satisfaction> {
        self.entries.iter().rev().find_map(|e| match &e.event {
            Event::Satisfied { stage: s, how } if *s == stage => Some(how),
            _ => None,
        })
    }

    /// Whether the player has been shown this stage.
    pub fn was_reached(&self, stage: Stage) -> bool {
        self.entries
            .iter()
            .any(|e| matches!(&e.event, Event::Reached { stage: s } if *s == stage))
    }

    /// Whether onboarding is over.
    ///
    /// True for any recorded completion, whatever flow version wrote it,
    /// including one newer than this binary. A player who has finished has
    /// finished; see [`FLOW_VERSION`].
    pub fn is_complete(&self) -> bool {
        self.completed_flow_version().is_some()
    }

    /// The flow version that was completed, if one was.
    pub fn completed_flow_version(&self) -> Option<u32> {
        self.entries.iter().rev().find_map(|e| match &e.event {
            Event::Completed { flow_version } => Some(*flow_version),
            _ => None,
        })
    }

    /// Preparation tasks already finished, in the order they finished.
    pub fn tasks_done(&self) -> Vec<PrepTask> {
        let mut out: Vec<PrepTask> = Vec::new();
        for e in &self.entries {
            if let Event::TaskDone { task } = &e.event {
                if !out.contains(task) {
                    out.push(*task);
                }
            }
        }
        out
    }
}

// ---------------------------------------------------------------------------
// The machine
// ---------------------------------------------------------------------------

/// Facts about this machine that decide which stages exist.
///
/// This module does not go looking for a 2.1 install. Resolving where one would
/// live is a platform question that `reborn-cli` already answers, and a second
/// guess here would be a second place to be wrong. Worse, a guessed path that
/// happens to be empty reads as "no 2.1 install", which drops the migration
/// offer, and because onboarding runs once the offer would never come back. So
/// the caller passes what [`LegacyInstall::detect`] actually returned, and
/// [`Environment::nothing_found`] is an explicit statement rather than a default
/// nobody looked at.
#[derive(Debug, Clone, Default)]
pub struct Environment {
    legacy: Option<LegacyInstall>,
}

impl Environment {
    /// Detection ran and found no 2.1 install.
    pub fn nothing_found() -> Self {
        Self { legacy: None }
    }

    /// Detection ran and found this.
    pub fn with_legacy_install(legacy: LegacyInstall) -> Self {
        Self {
            legacy: Some(legacy),
        }
    }

    pub fn legacy(&self) -> Option<&LegacyInstall> {
        self.legacy.as_ref()
    }
}

/// A stage that applies to this machine, and where it sits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlannedStage {
    pub stage: Stage,
    /// One-based, over the stages that apply here. Not the wireframes' "of 12";
    /// see the module documentation.
    pub position: usize,
    pub total: usize,
}

/// What to show next.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Next {
    /// Show this stage. `resumed` is true when the player has been here before,
    /// which is the difference between "Continue" and "Get started".
    Show {
        stage: Stage,
        position: usize,
        total: usize,
        resumed: bool,
    },
    /// This stage writes files and the last run stopped inside it. Its work may
    /// have partly happened. Do not repeat it blindly.
    Reconcile {
        stage: Stage,
        position: usize,
        total: usize,
        what_to_do: &'static str,
    },
    /// Onboarding is over and must not be shown again.
    AlreadyComplete { flow_version: u32 },
}

/// Why nothing was recorded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// Onboarding is already latched complete.
    AlreadyComplete,
    /// [`Stage::Completion`] is closed by [`Onboarding::finish`].
    UseFinish,
    /// A later stage was answered before an earlier one.
    OutOfOrder { expected: Stage, got: Stage },
    /// The stage does not exist on this machine.
    NotOnThisMachine { stage: Stage, why: &'static str },
    /// An irreversible stage was answered without a before-marker.
    NotReached { stage: Stage },
    /// An irreversible stage was re-entered after it had already run.
    Irreversible { stage: Stage },
    /// The evidence belongs to a different stage.
    WrongEvidence { stage: Stage },
    /// The migration was pointed somewhere other than the folder that was
    /// found.
    WrongSource { given: PathBuf, found: PathBuf },
    /// The migration plan is blocked and cannot be accepted as a decision.
    BlockedMigration { why: String },
    /// Preparation was declared done with a task still outstanding.
    TasksOutstanding { first: PrepTask },
    /// Completion was requested with a stage still unanswered.
    Outstanding { stage: Stage },
    /// There is nothing safe to go back to.
    NoWayBack { stage: Stage },
}

impl Refusal {
    /// A sentence whoever hit this can act on.
    pub fn message(&self) -> String {
        match self {
            Refusal::AlreadyComplete => "First run is already finished, and nothing more is \
                 written to it. Change these settings in Settings instead."
                .to_string(),
            Refusal::UseFinish => "The completion stage is closed by finish(), which writes the \
                 latch that stops first run being shown again. Call that."
                .to_string(),
            Refusal::OutOfOrder { expected, got } => format!(
                "The {} stage cannot be answered yet, because {} has not been \
                 answered. Answer that one first.",
                got.slug(),
                expected.slug()
            ),
            Refusal::NotOnThisMachine { stage, why } => format!(
                "The {} stage does not apply here: {}. Skip it.",
                stage.slug(),
                why
            ),
            Refusal::NotReached { stage } => format!(
                "The {} stage writes files, so it needs a before-marker. Call \
                 reach() first, or an interrupted run cannot tell that it \
                 started.",
                stage.slug()
            ),
            Refusal::Irreversible { stage } => format!(
                "The {} stage has already run and written files. It cannot be \
                 entered again. Verify what is on disk instead of repeating it.",
                stage.slug()
            ),
            Refusal::WrongEvidence { stage } => format!(
                "That evidence does not belong to the {} stage. Pass the \
                 Satisfaction variant that stage accepts.",
                stage.slug()
            ),
            Refusal::WrongSource { given, found } => format!(
                "The migration was pointed at {}, but the install that was found \
                 and shown to the player is at {}. Migrate that one, or re-run \
                 detection.",
                given.display(),
                found.display()
            ),
            Refusal::BlockedMigration { why } => format!(
                "The migration cannot be accepted: {}. Offer only the decline, \
                 until the block is cleared.",
                why
            ),
            Refusal::TasksOutstanding { first } => format!(
                "Preparation is not done: \"{}\" has not finished. Record every \
                 task before declaring the stage complete, or the Ready screen \
                 claims a setup that never ran.",
                first.label()
            ),
            Refusal::Outstanding { stage } => format!(
                "First run cannot finish while the {} stage is unanswered. Answer \
                 it, or record a decline if it allows one.",
                stage.slug()
            ),
            Refusal::NoWayBack { stage } => format!(
                "There is nothing safe behind the {} stage. Hide Back on this \
                 screen rather than offering one that refuses.",
                stage.slug()
            ),
        }
    }
}

/// What a write did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Advance {
    /// Appended to the journal and flushed to disk.
    Recorded,
    /// Already in the journal. Nothing was written and nothing was lost.
    AlreadyRecorded,
    /// Nothing was written, and this is why.
    Refused(Refusal),
}

impl Advance {
    /// Whether the journal now holds this fact, whether or not this call put it
    /// there.
    pub fn is_held(&self) -> bool {
        matches!(self, Advance::Recorded | Advance::AlreadyRecorded)
    }
}

/// Where Back goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Back {
    To(Stage),
    Refused(Refusal),
}

/// The latched end of onboarding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Completion {
    pub at_ms: u64,
    pub flow_version: u32,
    pub summary: Summary,
}

/// What onboarding decided, for the Ready screen.
///
/// Every field is [`Option`] and `None` means nothing measured it. The Ready
/// screen must print absence, not a plausible default.
///
/// Two lines the wireframe draws are deliberately handled differently.
/// "Installed «4.4 GB»" is only present as `game_bytes` when a caller measured
/// it, and "Account: not connected" is absent entirely, because this module
/// cannot know it; ask [`crate::platform::CredentialVault`]. Inventing either is
/// the exact failure `CONTENT_AND_UPDATE_MODEL.md` records for the size figures.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Summary {
    /// The emulator's own device string, so button prompts and bindings agree.
    pub controller: Option<String>,
    pub controller_family: Option<DeviceFamily>,
    pub controller_port: Option<u8>,
    /// Present only when the player declined controller setup.
    pub no_controller_because: Option<String>,
    pub graphics: Option<RecordedPreset>,
    /// `None` when the machine was never measured, so there was no
    /// recommendation to agree or disagree with.
    pub graphics_was_recommended: Option<bool>,
    pub detected_hardware: Option<String>,
    pub migrated_from: Option<PathBuf>,
    pub migrated_bytes: Option<u64>,
    pub game_bytes: Option<u64>,
    /// Mods the player turned on. `Some(vec![])` is a deliberate "none",
    /// `None` means never asked, and a completion screen that renders both as
    /// "no mods" is claiming a decision that was never made.
    pub mods_chosen: Option<Vec<String>>,
    /// Whether overlapping writes were shown before the choice was taken.
    ///
    /// `Some(false)` is the case worth surfacing: mods were picked without the
    /// player being told that several of them write the same files.
    pub mod_conflicts_shown: Option<bool>,
    pub tasks_done: Vec<PrepTask>,
}

impl Summary {
    /// Fields with nothing behind them, named so the caller can render them as
    /// absent instead of guessing.
    pub fn absent(&self) -> Vec<&'static str> {
        let mut out = Vec::new();
        if self.controller.is_none() {
            out.push("controller");
        }
        if self.graphics.is_none() {
            out.push("graphics");
        }
        if self.graphics_was_recommended.is_none() {
            out.push("graphics recommendation");
        }
        if self.detected_hardware.is_none() {
            out.push("detected hardware");
        }
        if self.game_bytes.is_none() {
            out.push("installed size");
        }
        if self.mods_chosen.is_none() {
            out.push("mods");
        }
        out
    }
}

/// First run, with its progress on disk.
#[derive(Debug)]
pub struct Onboarding {
    path: PathBuf,
    progress: Progress,
    env: Environment,
    clock: Box<dyn Clock>,
}

impl Onboarding {
    /// Open the journal under the launcher's config directory.
    pub fn open(platform: &Platform, env: Environment) -> Result<Self> {
        let dir = platform.data_dir(DataDir::Config)?;
        Self::open_at(&dir, env, SystemClock)
    }

    /// As [`Onboarding::open`], rooted at an explicit directory with an injected
    /// clock.
    ///
    /// Exists so tests exercise real files and real interruption in a temporary
    /// directory rather than the developer's own install.
    pub fn open_at<C: Clock + 'static>(dir: &Path, env: Environment, clock: C) -> Result<Self> {
        std::fs::create_dir_all(dir).map_err(|source| Error::Io {
            path: dir.to_path_buf(),
            source,
        })?;
        let path = dir.join(JOURNAL);
        let progress = Progress::read(&path)?;
        Ok(Self {
            path,
            progress,
            env,
            clock: Box::new(clock),
        })
    }

    pub fn progress(&self) -> &Progress {
        &self.progress
    }

    pub fn environment(&self) -> &Environment {
        &self.env
    }

    /// Whether first run is over. The one question a launcher asks at startup.
    pub fn is_complete(&self) -> bool {
        self.progress.is_complete()
    }

    /// The latched completion, if there is one.
    pub fn completion(&self) -> Option<Completion> {
        let flow_version = self.progress.completed_flow_version()?;
        let at_ms = self
            .progress
            .entries
            .iter()
            .rev()
            .find_map(|e| matches!(e.event, Event::Completed { .. }).then_some(e.at_ms))?;
        Some(Completion {
            at_ms,
            flow_version,
            summary: self.summary(),
        })
    }

    /// Why a stage does not exist on this machine, if it does not.
    ///
    /// Decided from the environment alone, never from an answer the player has
    /// given. That is what keeps [`PlannedStage::total`] still: a total that
    /// shrank when the player declined migration would relabel every screen
    /// after it.
    pub fn excluded(&self, stage: Stage) -> Option<&'static str> {
        match stage {
            Stage::MigrationOffer | Stage::MigrationCopy if self.env.legacy.is_none() => Some(
                "no NFL Street Reborn 2.1 install was found, so there is nothing \
                 to bring over",
            ),
            _ => None,
        }
    }

    /// Why a stage that exists here has nothing left to do, if it has nothing.
    ///
    /// Distinct from [`Onboarding::excluded`]: this depends on an answer.
    /// Declining the migration offer makes the copy moot without removing it
    /// from the plan, so the stepped list can show it as skipped and the step
    /// numbers do not move.
    pub fn moot(&self, stage: Stage) -> Option<&'static str> {
        match stage {
            Stage::MigrationCopy => match self.progress.satisfaction(Stage::MigrationOffer) {
                Some(Satisfaction::MigrationDeclined) => {
                    Some("the 2.1 install was left where it is")
                }
                _ => None,
            },
            _ => None,
        }
    }

    /// The stages that apply to this machine, in order.
    pub fn plan(&self) -> Vec<PlannedStage> {
        let applicable: Vec<Stage> = Stage::ALL
            .iter()
            .copied()
            .filter(|s| self.excluded(*s).is_none())
            .collect();
        let total = applicable.len();
        applicable
            .iter()
            .enumerate()
            .map(|(i, s)| PlannedStage {
                stage: *s,
                position: i + 1,
                total,
            })
            .collect()
    }

    /// Stages this machine does not have, and why.
    ///
    /// Reported rather than silently dropped, following `devices.rs`: a flow
    /// that quietly omits a step is a flow nobody can check.
    pub fn set_aside(&self) -> Vec<(Stage, &'static str)> {
        Stage::ALL
            .iter()
            .filter_map(|s| self.excluded(*s).map(|why| (*s, why)))
            .collect()
    }

    /// The migration plan for the install that was found, built by
    /// [`crate::migration`] so its saves-first ordering cannot be restated
    /// differently here.
    pub fn migration_plan(&self) -> Option<MigrationPlan> {
        self.env.legacy.as_ref().map(MigrationPlan::build)
    }

    /// The first stage that still needs an answer.
    fn outstanding(&self) -> Option<Stage> {
        Stage::ALL.iter().copied().find(|s| {
            self.excluded(*s).is_none()
                && self.moot(*s).is_none()
                && self.progress.satisfaction(*s).is_none()
        })
    }

    /// Preparation tasks still to run.
    pub fn preparation_remaining(&self) -> Vec<PrepTask> {
        let done = self.progress.tasks_done();
        PrepTask::ALL
            .iter()
            .copied()
            .filter(|t| !done.contains(t))
            .collect()
    }

    fn placement(&self, stage: Stage) -> (usize, usize) {
        let plan = self.plan();
        let total = plan.len();
        let position = plan
            .iter()
            .find(|p| p.stage == stage)
            .map(|p| p.position)
            // Zero means "not in the plan", which only happens for a stage this
            // machine does not have. Callers checking `excluded` first never see
            // it.
            .unwrap_or(0);
        (position, total)
    }

    /// What to show next.
    ///
    /// The whole resumption rule is here. An irreversible stage that was reached
    /// and never satisfied, or any irreversible stage at all when the journal is
    /// truncated, routes to [`Next::Reconcile`] rather than [`Next::Show`]. The
    /// truncated case is the subtle one: a journal that stopped short may be
    /// missing the before-marker of work that really did run, so its silence
    /// about a stage proves nothing.
    pub fn next(&self) -> Next {
        if let Some(flow_version) = self.progress.completed_flow_version() {
            return Next::AlreadyComplete { flow_version };
        }
        let Some(stage) = self.outstanding() else {
            // Completion is never satisfied by satisfy(), so it stays
            // outstanding until finish() latches. Reaching this arm would mean
            // the stage list is empty, which ALL prevents.
            let (position, total) = self.placement(Stage::Completion);
            return Next::Show {
                stage: Stage::Completion,
                position,
                total,
                resumed: self.progress.was_reached(Stage::Completion),
            };
        };
        let (position, total) = self.placement(stage);

        if stage.is_irreversible()
            && (self.progress.was_reached(stage) || self.progress.is_truncated())
        {
            return Next::Reconcile {
                stage,
                position,
                total,
                what_to_do: reconcile_advice(stage),
            };
        }

        Next::Show {
            stage,
            position,
            total,
            resumed: self.progress.was_reached(stage),
        }
    }

    /// Where Back goes from a stage.
    ///
    /// Back stops at the nearest previous stage that applies here, and is
    /// refused when that stage is irreversible. It deliberately does not skip
    /// over an irreversible stage to reach a reversible one behind it: stepping
    /// back past the migration copy to the offer would let a player un-choose a
    /// migration that has already copied their memory cards.
    pub fn back_from(&self, stage: Stage) -> Back {
        if !stage.allows_back() {
            return Back::Refused(Refusal::NoWayBack { stage });
        }
        let previous = Stage::ALL
            .iter()
            .copied()
            .rfind(|s| s.index() < stage.index() && self.excluded(*s).is_none());
        match previous {
            None => Back::Refused(Refusal::NoWayBack { stage }),
            Some(p) if p.is_irreversible() => Back::Refused(Refusal::Irreversible { stage: p }),
            Some(p) => Back::To(p),
        }
    }

    /// Record that the player has been shown a stage.
    ///
    /// Required before an irreversible stage can be satisfied. Harmless and
    /// useful elsewhere: it is what makes a resumed run land on the screen the
    /// player was actually on.
    pub fn reach(&mut self, stage: Stage) -> Result<Advance> {
        if self.progress.is_complete() {
            return Ok(Advance::Refused(Refusal::AlreadyComplete));
        }
        if let Some(why) = self.excluded(stage) {
            return Ok(Advance::Refused(Refusal::NotOnThisMachine { stage, why }));
        }
        if stage.is_irreversible() && self.progress.satisfaction(stage).is_some() {
            return Ok(Advance::Refused(Refusal::Irreversible { stage }));
        }
        if self.progress.was_reached(stage) {
            return Ok(Advance::AlreadyRecorded);
        }
        self.append(Event::Reached { stage })?;
        Ok(Advance::Recorded)
    }

    /// Record a preparation task as finished.
    pub fn record_task(&mut self, task: PrepTask) -> Result<Advance> {
        if self.progress.is_complete() {
            return Ok(Advance::Refused(Refusal::AlreadyComplete));
        }
        if !self.progress.was_reached(Stage::Preparation) {
            return Ok(Advance::Refused(Refusal::NotReached {
                stage: Stage::Preparation,
            }));
        }
        // An unanswered earlier stage means this task would apply something the
        // player was never asked about, mods being the obvious one. The check
        // has to be here and not only in satisfy(): satisfy() is where the
        // stage reports being finished, which is after every file has been
        // written, and reach() deliberately stays permissive so a resumed run
        // can re-open a screen.
        if let Some(expected) = self.outstanding() {
            if expected != Stage::Preparation {
                return Ok(Advance::Refused(Refusal::OutOfOrder {
                    expected,
                    got: Stage::Preparation,
                }));
            }
        }
        if self.progress.tasks_done().contains(&task) {
            return Ok(Advance::AlreadyRecorded);
        }
        self.append(Event::TaskDone { task })?;
        Ok(Advance::Recorded)
    }

    /// Answer a stage.
    ///
    /// An already-answered reversible stage may be answered again, and the new
    /// answer wins. That is safe even after preparation has run, because the
    /// emulator configuration is regenerated from the graphics preset on every
    /// launch (see `session.rs`), so a late change cannot leave a stale file
    /// behind. An already-answered irreversible stage is refused.
    pub fn satisfy(&mut self, stage: Stage, how: Satisfaction) -> Result<Advance> {
        if self.progress.is_complete() {
            return Ok(Advance::Refused(Refusal::AlreadyComplete));
        }
        if stage == Stage::Completion {
            return Ok(Advance::Refused(Refusal::UseFinish));
        }
        if let Some(why) = self.excluded(stage) {
            return Ok(Advance::Refused(Refusal::NotOnThisMachine { stage, why }));
        }
        if !stage.accepts(&how) {
            return Ok(Advance::Refused(Refusal::WrongEvidence { stage }));
        }

        // The migration decision is checked against the install that was
        // actually found, and against migration.rs's own blocking rule.
        if let Satisfaction::Migrating { source } = &how {
            let Some(found) = self.env.legacy.as_ref() else {
                return Ok(Advance::Refused(Refusal::NotOnThisMachine {
                    stage,
                    why: "no NFL Street Reborn 2.1 install was found, so there is \
                          nothing to bring over",
                }));
            };
            if source != &found.root {
                return Ok(Advance::Refused(Refusal::WrongSource {
                    given: source.clone(),
                    found: found.root.clone(),
                }));
            }
            if let Some(plan) = self.migration_plan() {
                if let Some(why) = plan.blocked {
                    return Ok(Advance::Refused(Refusal::BlockedMigration { why }));
                }
            }
        }

        // Deliberately after the ordering check below in effect, by being
        // expressed as part of it: an unanswered earlier stage is the more
        // fundamental problem, and naming outstanding tasks first would point
        // the caller at work that should not start yet.
        if stage == Stage::Preparation && self.outstanding() == Some(Stage::Preparation) {
            if let Some(first) = self.preparation_remaining().first().copied() {
                return Ok(Advance::Refused(Refusal::TasksOutstanding { first }));
            }
        }

        if self.progress.satisfaction(stage).is_some() {
            if stage.is_irreversible() {
                return Ok(Advance::Refused(Refusal::Irreversible { stage }));
            }
        } else {
            if let Some(expected) = self.outstanding() {
                if expected != stage {
                    return Ok(Advance::Refused(Refusal::OutOfOrder {
                        expected,
                        got: stage,
                    }));
                }
            }
            if stage.is_irreversible() && !self.progress.was_reached(stage) {
                return Ok(Advance::Refused(Refusal::NotReached { stage }));
            }
        }

        self.append(Event::Satisfied { stage, how })?;
        Ok(Advance::Recorded)
    }

    /// Latch onboarding as complete.
    ///
    /// This is the single write that stops first run being shown again, and it
    /// is deliberately the only one: [`Stage::Completion`] cannot be answered
    /// through [`Onboarding::satisfy`], because two ways to end the flow is how
    /// one of them stops being maintained.
    ///
    /// It requires that the player was shown the Ready screen. Latching without
    /// that would hide what was decided behind a flow the player can never
    /// reopen.
    pub fn finish(&mut self) -> Result<Advance> {
        if self.progress.is_complete() {
            return Ok(Advance::Refused(Refusal::AlreadyComplete));
        }
        if let Some(stage) = self.outstanding() {
            if stage != Stage::Completion {
                return Ok(Advance::Refused(Refusal::Outstanding { stage }));
            }
        }
        if !self.progress.was_reached(Stage::Completion) {
            return Ok(Advance::Refused(Refusal::NotReached {
                stage: Stage::Completion,
            }));
        }
        self.append(Event::Completed {
            flow_version: FLOW_VERSION,
        })?;
        Ok(Advance::Recorded)
    }

    /// What onboarding decided so far.
    pub fn summary(&self) -> Summary {
        let mut s = Summary::default();

        if let Some(Satisfaction::GameConfirmed { total_bytes }) =
            self.progress.satisfaction(Stage::GameVerification)
        {
            s.game_bytes = *total_bytes;
        }
        if let Some(Satisfaction::Migrating { source }) =
            self.progress.satisfaction(Stage::MigrationOffer)
        {
            s.migrated_from = Some(source.clone());
        }
        if let Some(Satisfaction::Migrated { bytes }) =
            self.progress.satisfaction(Stage::MigrationCopy)
        {
            s.migrated_bytes = Some(*bytes);
        }
        match self.progress.satisfaction(Stage::ControllerSetup) {
            Some(Satisfaction::Controller {
                device,
                family,
                port,
            }) => {
                s.controller = Some(device.clone());
                s.controller_family = Some(*family);
                s.controller_port = Some(*port);
            }
            Some(Satisfaction::NoController { because }) => {
                s.no_controller_because = Some(because.clone());
            }
            _ => {}
        }
        if let Some(Satisfaction::Graphics {
            chosen,
            recommended,
            detected,
        }) = self.progress.satisfaction(Stage::SystemCheck)
        {
            s.graphics = Some(*chosen);
            // None stays None. "The recommendation was not taken" and "there was
            // no recommendation" are different facts, and the badge depends on
            // which one it is.
            s.graphics_was_recommended = recommended.map(|r| r == *chosen);
            s.detected_hardware = detected.clone();
        }
        match self.progress.satisfaction(Stage::ModSelection) {
            Some(Satisfaction::ModsChosen {
                packages,
                conflicts_shown,
            }) => {
                s.mods_chosen = Some(packages.clone());
                s.mod_conflicts_shown = Some(*conflicts_shown);
            }
            // Declining is a decision, so it reports an empty list rather than
            // absence. There were no conflicts to show, which is not the same
            // as having withheld them.
            Some(Satisfaction::NoMods) => {
                s.mods_chosen = Some(Vec::new());
                s.mod_conflicts_shown = Some(true);
            }
            _ => {}
        }
        s.tasks_done = self.progress.tasks_done();
        s
    }

    /// Append one line, flush it, and only then update the in-memory view.
    ///
    /// That order matters: a view that has moved on from disk would let the
    /// caller believe an irreversible step was recorded when a full disk or a
    /// power cut dropped it.
    fn append(&mut self, event: Event) -> Result<()> {
        use std::io::Write;

        let entry = Entry {
            at_ms: self.clock.now_ms(),
            event,
        };
        let line = serde_json::to_string(&entry).map_err(|e| {
            Error::PlainIo(std::io::Error::other(format!(
                "could not encode an onboarding entry: {e}"
            )))
        })?;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|source| Error::Io {
                path: self.path.clone(),
                source,
            })?;
        writeln!(file, "{line}").map_err(|source| Error::Io {
            path: self.path.clone(),
            source,
        })?;
        // This journal is the only thing standing between an interrupted run and
        // repeating a copy of the player's memory cards. Worth the flush.
        file.sync_all().map_err(|source| Error::Io {
            path: self.path.clone(),
            source,
        })?;

        self.progress.entries.push(entry);
        Ok(())
    }
}

/// What to do about an irreversible stage whose last run was interrupted.
fn reconcile_advice(stage: Stage) -> &'static str {
    match stage {
        Stage::MigrationCopy => {
            "Re-run the migration with migration::copy_tree_no_clobber, which \
             refuses to overwrite anything already at the destination, so files \
             brought over by the interrupted run are left alone and a newer save \
             is never replaced by the older one it came from."
        }
        Stage::Preparation => {
            "Verify each preparation task before running it again. Repeating them \
             is safe but costs the player the copy time a second time, and the \
             interface has already promised it picks up where it left off."
        }
        _ => {
            "This stage does not write files, so there is nothing to reconcile. \
             Show it again."
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::supervisor::FixedClock;

    /// Build a plausible 2.1 install on disk, the same shape migration.rs tests
    /// build.
    fn legacy(dir: &Path, with_mods: bool, with_backup: bool) -> LegacyInstall {
        let user = dir.join("User");
        std::fs::create_dir_all(user.join("GC")).unwrap();
        std::fs::write(user.join("GC").join("MemoryCardA.USA.raw"), b"saves").unwrap();
        std::fs::create_dir_all(user.join("ScreenShots")).unwrap();
        std::fs::create_dir_all(user.join("Load").join("Textures")).unwrap();
        let mods = dir.join("MODS");
        std::fs::create_dir_all(mods.join("!BACKUP")).unwrap();
        std::fs::create_dir_all(mods.join("!INSTLOGS")).unwrap();
        if with_mods {
            std::fs::write(mods.join("!INSTLOGS").join("PS Controls.txt"), b"applied").unwrap();
        }
        if with_backup && with_mods {
            std::fs::create_dir_all(mods.join("!BACKUP").join("PS Controls")).unwrap();
            std::fs::write(
                mods.join("!BACKUP").join("PS Controls").join("o.dds"),
                b"pristine",
            )
            .unwrap();
        }
        LegacyInstall::detect(dir).expect("a legacy install")
    }

    fn fresh(dir: &Path) -> Onboarding {
        Onboarding::open_at(dir, Environment::nothing_found(), FixedClock::new(1_000))
            .expect("open")
    }

    fn recorded(a: Advance) {
        assert_eq!(a, Advance::Recorded, "expected a write");
    }

    fn graphics() -> Satisfaction {
        Satisfaction::Graphics {
            chosen: RecordedPreset::Balanced,
            recommended: Some(RecordedPreset::Balanced),
            detected: Some("Ryzen 5 5600, RTX 3060, 16 GB".into()),
        }
    }

    fn pad() -> Satisfaction {
        Satisfaction::Controller {
            device: "XInput/0/Gamepad".into(),
            family: DeviceFamily::Xbox,
            port: 1,
        }
    }

    fn confirm_game() -> Satisfaction {
        Satisfaction::GameConfirmed { total_bytes: None }
    }

    /// Answer everything up to, but not through, preparation.
    fn walk_to_preparation(ob: &mut Onboarding) {
        recorded(ob.reach(Stage::Welcome).unwrap());
        recorded(
            ob.satisfy(Stage::Welcome, Satisfaction::Acknowledged)
                .unwrap(),
        );
        recorded(ob.satisfy(Stage::GameVerification, confirm_game()).unwrap());
        recorded(ob.satisfy(Stage::SystemCheck, graphics()).unwrap());
        recorded(ob.satisfy(Stage::ControllerSetup, pad()).unwrap());
        // Mods are chosen before Preparation, which is what applies them,
        // so walking 'to preparation' now passes through here.
        recorded(ob.reach(Stage::ModSelection).unwrap());
        recorded(
            ob.satisfy(Stage::ModSelection, Satisfaction::NoMods)
                .unwrap(),
        );
    }

    /// Walk a machine with no 2.1 install up to, but not through, the latch.
    fn walk_to_completion_screen(ob: &mut Onboarding) {
        walk_to_preparation(ob);
        recorded(ob.reach(Stage::Preparation).unwrap());
        for t in PrepTask::ALL {
            recorded(ob.record_task(*t).unwrap());
        }
        recorded(
            ob.satisfy(Stage::Preparation, Satisfaction::Prepared)
                .unwrap(),
        );
        recorded(ob.reach(Stage::Completion).unwrap());
    }

    // -----------------------------------------------------------------------
    // Never shown twice
    // -----------------------------------------------------------------------

    #[test]
    fn a_finished_first_run_is_never_shown_again() {
        let dir = tempfile::tempdir().unwrap();
        {
            let mut ob = fresh(dir.path());
            walk_to_completion_screen(&mut ob);
            recorded(ob.finish().unwrap());
        }
        let ob = fresh(dir.path());
        assert!(ob.is_complete());
        assert_eq!(
            ob.next(),
            Next::AlreadyComplete {
                flow_version: FLOW_VERSION
            },
            "a completed first run must not be offered again"
        );
    }

    /// The reason completion is a latch and not a derived fact. Onboarding was
    /// finished on a machine with no 2.1 install. Later the player reinstalls
    /// 2.1, or unplugs the pad. Recomputing "is every stage satisfied" from the
    /// live machine would drag a settled player back through first run, re-offer
    /// the migration, and re-run preparation.
    #[test]
    fn completion_is_latched_and_never_recomputed_from_the_machine() {
        let dir = tempfile::tempdir().unwrap();
        let legacy_dir = dir.path().join("NFL Street Reborn");
        std::fs::create_dir_all(&legacy_dir).unwrap();

        {
            let mut ob = fresh(dir.path());
            walk_to_completion_screen(&mut ob);
            recorded(ob.finish().unwrap());
        }

        // The same journal, a different machine.
        let changed = Environment::with_legacy_install(legacy(&legacy_dir, false, false));
        let ob = Onboarding::open_at(dir.path(), changed, FixedClock::new(9_000)).unwrap();
        assert!(
            ob.is_complete(),
            "a 2.1 install appearing later must not restart first run"
        );
        assert!(matches!(ob.next(), Next::AlreadyComplete { .. }));
    }

    #[test]
    fn nothing_is_written_after_the_completion_latch() {
        let dir = tempfile::tempdir().unwrap();
        let mut ob = fresh(dir.path());
        walk_to_completion_screen(&mut ob);
        recorded(ob.finish().unwrap());
        let before = std::fs::read_to_string(dir.path().join(JOURNAL)).unwrap();

        assert_eq!(
            ob.satisfy(Stage::SystemCheck, graphics()).unwrap(),
            Advance::Refused(Refusal::AlreadyComplete)
        );
        assert_eq!(
            ob.reach(Stage::Welcome).unwrap(),
            Advance::Refused(Refusal::AlreadyComplete)
        );
        assert_eq!(
            ob.record_task(PrepTask::CheckEverything).unwrap(),
            Advance::Refused(Refusal::AlreadyComplete)
        );
        assert_eq!(
            ob.finish().unwrap(),
            Advance::Refused(Refusal::AlreadyComplete),
            "a second finish must not append a second latch"
        );
        assert_eq!(
            std::fs::read_to_string(dir.path().join(JOURNAL)).unwrap(),
            before,
            "the journal must be byte-identical after every refused write"
        );
    }

    /// A future launcher that adds a stage must not rewind a player who already
    /// finished, because rewinding would re-offer migration and re-run
    /// preparation.
    #[test]
    fn a_completion_written_by_another_flow_version_still_counts() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(JOURNAL),
            "{\"at_ms\":5,\"event\":{\"event\":\"completed\",\"flow_version\":99}}\n",
        )
        .unwrap();
        let ob = fresh(dir.path());
        assert!(ob.is_complete());
        assert_eq!(ob.next(), Next::AlreadyComplete { flow_version: 99 });
    }

    // -----------------------------------------------------------------------
    // Resumption
    // -----------------------------------------------------------------------

    #[test]
    fn an_interrupted_run_resumes_where_it_stopped_rather_than_restarting() {
        let dir = tempfile::tempdir().unwrap();
        {
            let mut ob = fresh(dir.path());
            recorded(ob.reach(Stage::Welcome).unwrap());
            recorded(
                ob.satisfy(Stage::Welcome, Satisfaction::Acknowledged)
                    .unwrap(),
            );
            recorded(ob.satisfy(Stage::GameVerification, confirm_game()).unwrap());
            recorded(ob.reach(Stage::SystemCheck).unwrap());
            // The process dies here.
        }
        let ob = fresh(dir.path());
        match ob.next() {
            Next::Show { stage, resumed, .. } => {
                assert_eq!(stage, Stage::SystemCheck, "must not restart at welcome");
                assert!(resumed, "the player has been on this screen before");
            }
            other => panic!("expected the graphics screen again, got {other:?}"),
        }
    }

    /// The case this module exists for. Preparation writes gigabytes, so a run
    /// that stopped inside it must be reconciled, never repeated blindly.
    #[test]
    fn an_interrupted_irreversible_stage_is_reconciled_not_redone() {
        let dir = tempfile::tempdir().unwrap();
        {
            let mut ob = fresh(dir.path());
            walk_to_preparation(&mut ob);
            recorded(ob.reach(Stage::Preparation).unwrap());
            recorded(ob.record_task(PrepTask::CopyGameFiles).unwrap());
            recorded(ob.record_task(PrepTask::InstallRuntime).unwrap());
            // Power cut during the texture copy.
        }
        let ob = fresh(dir.path());
        match ob.next() {
            Next::Reconcile {
                stage, what_to_do, ..
            } => {
                assert_eq!(stage, Stage::Preparation);
                assert!(
                    what_to_do.contains("Verify"),
                    "the advice must say what to do: {what_to_do}"
                );
            }
            other => panic!("interrupted preparation must be reconciled, got {other:?}"),
        }
        assert_eq!(
            ob.preparation_remaining(),
            vec![
                PrepTask::ApplyTextures,
                PrepTask::ApplySettings,
                PrepTask::CheckEverything
            ],
            "the two finished copies must not be paid for twice"
        );
    }

    /// A journal that stopped short may be missing the before-marker of work that
    /// really did run, so its silence proves nothing. Every irreversible stage
    /// must be reconciled rather than shown.
    #[test]
    fn a_truncated_journal_forces_reconciliation_of_irreversible_work() {
        let dir = tempfile::tempdir().unwrap();
        {
            let mut ob = fresh(dir.path());
            walk_to_preparation(&mut ob);
        }
        // A power cut mid-append leaves half a line where the Reached marker for
        // preparation should be.
        let mut raw = std::fs::read_to_string(dir.path().join(JOURNAL)).unwrap();
        raw.push_str("{\"at_ms\":9,\"event\":{\"event\":\"rea");
        std::fs::write(dir.path().join(JOURNAL), raw).unwrap();

        let ob = fresh(dir.path());
        assert!(ob.progress().is_truncated());
        match ob.next() {
            Next::Reconcile { stage, .. } => assert_eq!(stage, Stage::Preparation),
            other => panic!(
                "a truncated journal must not send the flow straight back into an \
                 irreversible stage, got {other:?}"
            ),
        }
    }

    /// Stopping at a bad line must not lose the answers before it. Those are the
    /// record of what the player already decided.
    #[test]
    fn a_torn_last_line_does_not_lose_the_answers_before_it() {
        let dir = tempfile::tempdir().unwrap();
        {
            let mut ob = fresh(dir.path());
            recorded(ob.reach(Stage::Welcome).unwrap());
            recorded(
                ob.satisfy(Stage::Welcome, Satisfaction::Acknowledged)
                    .unwrap(),
            );
            recorded(ob.satisfy(Stage::GameVerification, confirm_game()).unwrap());
            recorded(ob.satisfy(Stage::SystemCheck, graphics()).unwrap());
        }
        let mut raw = std::fs::read_to_string(dir.path().join(JOURNAL)).unwrap();
        raw.push_str("{\"at_ms\":9,\"eve");
        std::fs::write(dir.path().join(JOURNAL), raw).unwrap();

        let ob = fresh(dir.path());
        assert!(ob.progress().is_truncated());
        assert_eq!(
            ob.progress().satisfaction(Stage::SystemCheck),
            Some(&graphics()),
            "an answer written before the torn line must survive"
        );
        assert_eq!(ob.summary().graphics, Some(RecordedPreset::Balanced));
    }

    #[test]
    fn a_missing_journal_is_a_first_run_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let ob = fresh(dir.path());
        assert!(!ob.is_complete());
        assert!(!ob.progress().is_truncated());
        match ob.next() {
            Next::Show {
                stage,
                position,
                resumed,
                ..
            } => {
                assert_eq!(stage, Stage::Welcome);
                assert_eq!(position, 1);
                assert!(!resumed);
            }
            other => panic!("expected the welcome screen, got {other:?}"),
        }
    }

    // -----------------------------------------------------------------------
    // Migration
    // -----------------------------------------------------------------------

    /// migration.rs orders its own work saves-first so an interruption costs
    /// nothing permanent. Preparation is this flow's multi-gigabyte step, so it
    /// must not sit between the player and the memory cards still in the folder
    /// the 2.1 uninstaller deletes.
    #[test]
    fn the_migration_copy_runs_before_the_multi_gigabyte_preparation() {
        assert!(
            Stage::MigrationCopy.index() < Stage::Preparation.index(),
            "a multi-gigabyte install must not sit between the player and their saves"
        );
        assert!(Stage::MigrationOffer.index() < Stage::MigrationCopy.index());
    }

    #[test]
    fn the_migration_plan_comes_from_the_migration_module_saves_first() {
        let dir = tempfile::tempdir().unwrap();
        let legacy_dir = dir.path().join("legacy");
        std::fs::create_dir_all(&legacy_dir).unwrap();
        let env = Environment::with_legacy_install(legacy(&legacy_dir, false, false));
        let ob = Onboarding::open_at(dir.path(), env, FixedClock::new(1)).unwrap();

        let plan = ob.migration_plan().expect("a plan for a found install");
        assert_eq!(
            plan.steps[0].what, "Memory cards",
            "onboarding must not restate the migration order differently"
        );
        assert_eq!(plan.steps[0].to, DataDir::Saves);
    }

    /// migration.rs blocks when mods are applied with no !BACKUP, because
    /// copying then captures modded files as if they were pristine. Onboarding
    /// must not let the player accept that.
    #[test]
    fn a_blocked_migration_cannot_be_accepted_as_a_decision() {
        let dir = tempfile::tempdir().unwrap();
        let legacy_dir = dir.path().join("legacy");
        std::fs::create_dir_all(&legacy_dir).unwrap();
        let install = legacy(&legacy_dir, true, false);
        let env = Environment::with_legacy_install(install.clone());
        let mut ob = Onboarding::open_at(dir.path(), env, FixedClock::new(1)).unwrap();

        recorded(ob.reach(Stage::Welcome).unwrap());
        recorded(
            ob.satisfy(Stage::Welcome, Satisfaction::Acknowledged)
                .unwrap(),
        );
        recorded(ob.satisfy(Stage::GameVerification, confirm_game()).unwrap());

        match ob
            .satisfy(
                Stage::MigrationOffer,
                Satisfaction::Migrating {
                    source: install.root.clone(),
                },
            )
            .unwrap()
        {
            Advance::Refused(Refusal::BlockedMigration { why }) => {
                assert!(why.contains("!BACKUP"), "{why}");
            }
            other => panic!("a blocked plan must not be accepted, got {other:?}"),
        }

        // Declining is still available, so the player is not stuck.
        recorded(
            ob.satisfy(Stage::MigrationOffer, Satisfaction::MigrationDeclined)
                .unwrap(),
        );
    }

    #[test]
    fn migration_cannot_be_pointed_at_a_folder_the_player_was_never_shown() {
        let dir = tempfile::tempdir().unwrap();
        let legacy_dir = dir.path().join("legacy");
        std::fs::create_dir_all(&legacy_dir).unwrap();
        let env = Environment::with_legacy_install(legacy(&legacy_dir, false, false));
        let mut ob = Onboarding::open_at(dir.path(), env, FixedClock::new(1)).unwrap();

        recorded(ob.reach(Stage::Welcome).unwrap());
        recorded(
            ob.satisfy(Stage::Welcome, Satisfaction::Acknowledged)
                .unwrap(),
        );
        recorded(ob.satisfy(Stage::GameVerification, confirm_game()).unwrap());
        match ob
            .satisfy(
                Stage::MigrationOffer,
                Satisfaction::Migrating {
                    source: PathBuf::from("somewhere/else"),
                },
            )
            .unwrap()
        {
            Advance::Refused(Refusal::WrongSource { .. }) => {}
            other => panic!("a different folder must be refused, got {other:?}"),
        }
    }

    /// Declining skips the copy, and the step count does not move. A total that
    /// shrank mid-flow would relabel every screen after it.
    #[test]
    fn declining_migration_skips_the_copy_without_moving_the_step_count() {
        let dir = tempfile::tempdir().unwrap();
        let legacy_dir = dir.path().join("legacy");
        std::fs::create_dir_all(&legacy_dir).unwrap();
        let env = Environment::with_legacy_install(legacy(&legacy_dir, false, false));
        let mut ob = Onboarding::open_at(dir.path(), env, FixedClock::new(1)).unwrap();

        let before = ob.plan().len();
        recorded(ob.reach(Stage::Welcome).unwrap());
        recorded(
            ob.satisfy(Stage::Welcome, Satisfaction::Acknowledged)
                .unwrap(),
        );
        recorded(ob.satisfy(Stage::GameVerification, confirm_game()).unwrap());
        recorded(
            ob.satisfy(Stage::MigrationOffer, Satisfaction::MigrationDeclined)
                .unwrap(),
        );

        assert_eq!(ob.plan().len(), before, "the step total must not move");
        assert!(ob.moot(Stage::MigrationCopy).is_some());
        match ob.next() {
            Next::Show { stage, .. } => assert_eq!(
                stage,
                Stage::SystemCheck,
                "a declined migration must not stop on the copy screen"
            ),
            other => panic!("expected the graphics screen, got {other:?}"),
        }
    }

    #[test]
    fn a_machine_with_no_legacy_install_has_no_migration_stages() {
        let dir = tempfile::tempdir().unwrap();
        let ob = fresh(dir.path());
        let stages: Vec<Stage> = ob.plan().iter().map(|p| p.stage).collect();
        assert!(!stages.contains(&Stage::MigrationOffer));
        assert!(!stages.contains(&Stage::MigrationCopy));
        assert_eq!(ob.set_aside().len(), 2);
        for (_, why) in ob.set_aside() {
            assert!(!why.is_empty(), "a skipped stage must say why");
        }
        assert!(ob.migration_plan().is_none());
    }

    /// The step count is the stages that apply here, not the wireframes' 12,
    /// because seven of those twelve are specified nowhere.
    #[test]
    fn the_step_total_counts_only_the_stages_that_apply_here() {
        let dir = tempfile::tempdir().unwrap();
        let clean = fresh(dir.path());
        assert_eq!(clean.plan().len(), Stage::ALL.len() - 2);

        let legacy_dir = dir.path().join("legacy");
        std::fs::create_dir_all(&legacy_dir).unwrap();
        let ob = Onboarding::open_at(
            &dir.path().join("other"),
            Environment::with_legacy_install(legacy(&legacy_dir, false, false)),
            FixedClock::new(1),
        )
        .unwrap();
        assert_eq!(ob.plan().len(), Stage::ALL.len());
        for (i, p) in ob.plan().iter().enumerate() {
            assert_eq!(p.position, i + 1);
            assert_eq!(p.total, Stage::ALL.len());
        }
    }

    // -----------------------------------------------------------------------
    // Ordering and reversibility
    // -----------------------------------------------------------------------

    #[test]
    fn stages_cannot_be_answered_out_of_order() {
        let dir = tempfile::tempdir().unwrap();
        let mut ob = fresh(dir.path());
        match ob.satisfy(Stage::ControllerSetup, pad()).unwrap() {
            Advance::Refused(Refusal::OutOfOrder { expected, got }) => {
                assert_eq!(expected, Stage::Welcome);
                assert_eq!(got, Stage::ControllerSetup);
            }
            other => panic!("expected an ordering refusal, got {other:?}"),
        }
        assert!(
            !dir.path().join(JOURNAL).exists(),
            "a refused write must not create the journal"
        );
    }

    #[test]
    fn an_irreversible_stage_cannot_be_re_entered_once_it_has_run() {
        let dir = tempfile::tempdir().unwrap();
        let mut ob = fresh(dir.path());
        walk_to_completion_screen(&mut ob);

        assert_eq!(
            ob.reach(Stage::Preparation).unwrap(),
            Advance::Refused(Refusal::Irreversible {
                stage: Stage::Preparation
            }),
            "re-entering preparation repeats a copy the player already paid for"
        );
        assert_eq!(
            ob.satisfy(Stage::Preparation, Satisfaction::Prepared)
                .unwrap(),
            Advance::Refused(Refusal::Irreversible {
                stage: Stage::Preparation
            })
        );
    }

    /// An irreversible stage needs a before-marker, or an interrupted run cannot
    /// tell that it started.
    #[test]
    fn an_irreversible_stage_refuses_to_be_answered_without_a_before_marker() {
        let dir = tempfile::tempdir().unwrap();
        let mut ob = fresh(dir.path());
        walk_to_preparation(&mut ob);

        assert_eq!(
            ob.record_task(PrepTask::CopyGameFiles).unwrap(),
            Advance::Refused(Refusal::NotReached {
                stage: Stage::Preparation
            })
        );
        assert_eq!(
            ob.satisfy(Stage::Preparation, Satisfaction::Prepared)
                .unwrap(),
            Advance::Refused(Refusal::TasksOutstanding {
                first: PrepTask::CopyGameFiles
            })
        );
    }

    /// Back must not step past the migration copy to the offer behind it, or a
    /// player un-chooses a migration that already moved their memory cards.
    #[test]
    fn back_is_refused_out_of_a_stage_that_has_already_written_files() {
        let dir = tempfile::tempdir().unwrap();
        let legacy_dir = dir.path().join("legacy");
        std::fs::create_dir_all(&legacy_dir).unwrap();
        let env = Environment::with_legacy_install(legacy(&legacy_dir, false, false));
        let ob = Onboarding::open_at(dir.path(), env, FixedClock::new(1)).unwrap();

        assert_eq!(
            ob.back_from(Stage::SystemCheck),
            Back::Refused(Refusal::Irreversible {
                stage: Stage::MigrationCopy
            }),
            "Back must not reach the offer across a copy that has run"
        );
        assert_eq!(
            ob.back_from(Stage::Preparation),
            Back::Refused(Refusal::NoWayBack {
                stage: Stage::Preparation
            })
        );
        assert_eq!(
            ob.back_from(Stage::Welcome),
            Back::Refused(Refusal::NoWayBack {
                stage: Stage::Welcome
            })
        );
        assert_eq!(
            ob.back_from(Stage::MigrationOffer),
            Back::To(Stage::GameVerification)
        );
    }

    /// On a machine with no 2.1 install there is no copy stage in the way, so
    /// Back works normally and skips the stages that do not exist.
    #[test]
    fn back_skips_stages_this_machine_does_not_have() {
        let dir = tempfile::tempdir().unwrap();
        let ob = fresh(dir.path());
        assert_eq!(
            ob.back_from(Stage::SystemCheck),
            Back::To(Stage::GameVerification),
            "Back must not land on a migration screen that was never shown"
        );
        assert_eq!(
            ob.back_from(Stage::ControllerSetup),
            Back::To(Stage::SystemCheck)
        );
    }

    /// Changing the graphics answer after preparation is safe, because the
    /// emulator configuration is regenerated from the preset on every launch.
    #[test]
    fn a_reversible_answer_can_be_changed_and_the_new_one_wins() {
        let dir = tempfile::tempdir().unwrap();
        let mut ob = fresh(dir.path());
        walk_to_completion_screen(&mut ob);

        recorded(
            ob.satisfy(
                Stage::SystemCheck,
                Satisfaction::Graphics {
                    chosen: RecordedPreset::Performance,
                    recommended: Some(RecordedPreset::Balanced),
                    detected: None,
                },
            )
            .unwrap(),
        );
        assert_eq!(ob.summary().graphics, Some(RecordedPreset::Performance));
        assert_eq!(
            ob.summary().graphics_was_recommended,
            Some(false),
            "the player overrode the recommendation, and that is a fact worth keeping"
        );
    }

    #[test]
    fn evidence_from_the_wrong_stage_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let mut ob = fresh(dir.path());
        recorded(ob.reach(Stage::Welcome).unwrap());
        assert_eq!(
            ob.satisfy(Stage::Welcome, graphics()).unwrap(),
            Advance::Refused(Refusal::WrongEvidence {
                stage: Stage::Welcome
            }),
            "a graphics answer filed against welcome would make the Ready screen \
             claim a preset nobody chose"
        );
        for stage in Stage::ALL {
            assert!(
                !stage.accepts(&Satisfaction::Acknowledged) || *stage == Stage::Welcome,
                "{:?} must not accept a bare acknowledgement",
                stage
            );
        }
    }

    // -----------------------------------------------------------------------
    // Completion rules
    // -----------------------------------------------------------------------

    #[test]
    fn finish_is_refused_while_a_stage_is_unanswered() {
        let dir = tempfile::tempdir().unwrap();
        let mut ob = fresh(dir.path());
        recorded(ob.reach(Stage::Welcome).unwrap());
        recorded(
            ob.satisfy(Stage::Welcome, Satisfaction::Acknowledged)
                .unwrap(),
        );
        match ob.finish().unwrap() {
            Advance::Refused(Refusal::Outstanding { stage }) => {
                assert_eq!(stage, Stage::GameVerification);
            }
            other => panic!("expected an outstanding refusal, got {other:?}"),
        }
        assert!(!ob.is_complete());
    }

    /// Latching without showing the Ready screen hides what was decided behind a
    /// flow the player can never reopen.
    #[test]
    fn finish_is_refused_until_the_player_has_seen_what_was_decided() {
        let dir = tempfile::tempdir().unwrap();
        let mut ob = fresh(dir.path());
        walk_to_preparation(&mut ob);
        recorded(ob.reach(Stage::Preparation).unwrap());
        for t in PrepTask::ALL {
            recorded(ob.record_task(*t).unwrap());
        }
        recorded(
            ob.satisfy(Stage::Preparation, Satisfaction::Prepared)
                .unwrap(),
        );

        assert_eq!(
            ob.finish().unwrap(),
            Advance::Refused(Refusal::NotReached {
                stage: Stage::Completion
            })
        );
        recorded(ob.reach(Stage::Completion).unwrap());
        recorded(ob.finish().unwrap());
    }

    /// Two ways to end onboarding is how one of them stops being maintained.
    #[test]
    fn the_completion_stage_cannot_be_answered_through_satisfy() {
        let dir = tempfile::tempdir().unwrap();
        let mut ob = fresh(dir.path());
        walk_to_completion_screen(&mut ob);
        assert_eq!(
            ob.satisfy(Stage::Completion, Satisfaction::Acknowledged)
                .unwrap(),
            Advance::Refused(Refusal::UseFinish)
        );
        assert!(!ob.is_complete());
    }

    /// A player with nothing plugged in must be able to finish, and the reason
    /// must be recorded so the launcher can say there is no pad rather than
    /// starting the game into nothing.
    #[test]
    fn a_player_with_no_controller_can_still_finish_first_run() {
        let dir = tempfile::tempdir().unwrap();
        let mut ob = fresh(dir.path());
        recorded(ob.reach(Stage::Welcome).unwrap());
        recorded(
            ob.satisfy(Stage::Welcome, Satisfaction::Acknowledged)
                .unwrap(),
        );
        recorded(ob.satisfy(Stage::GameVerification, confirm_game()).unwrap());
        recorded(ob.satisfy(Stage::SystemCheck, graphics()).unwrap());
        recorded(
            ob.satisfy(
                Stage::ControllerSetup,
                Satisfaction::NoController {
                    because: "nothing was plugged in, and SDL was not searched".into(),
                },
            )
            .unwrap(),
        );
        // Mods come before Preparation, which is what applies them.
        recorded(ob.reach(Stage::ModSelection).unwrap());
        recorded(
            ob.satisfy(Stage::ModSelection, Satisfaction::NoMods)
                .unwrap(),
        );
        recorded(ob.reach(Stage::Preparation).unwrap());
        for t in PrepTask::ALL {
            recorded(ob.record_task(*t).unwrap());
        }
        recorded(
            ob.satisfy(Stage::Preparation, Satisfaction::Prepared)
                .unwrap(),
        );
        recorded(ob.reach(Stage::Completion).unwrap());
        recorded(ob.finish().unwrap());

        let summary = ob.completion().expect("a latched completion").summary;
        assert!(summary.controller.is_none());
        assert!(
            summary.no_controller_because.is_some(),
            "the reason must survive so the launcher can say it"
        );
        assert!(summary.absent().contains(&"controller"));
        assert!(Stage::ControllerSetup.skip_rule().is_declinable());
    }

    // -----------------------------------------------------------------------
    // Never invent data
    // -----------------------------------------------------------------------

    /// The Ready screen prints Controller, Graphics and Installed. Anything not
    /// measured must be reported absent, never defaulted, because the design
    /// documents' own size and hardware figures are placeholders and
    /// CONTENT_AND_UPDATE_MODEL.md records them as wrong in both directions.
    #[test]
    fn the_summary_reports_absence_rather_than_a_plausible_default() {
        let dir = tempfile::tempdir().unwrap();
        let mut ob = fresh(dir.path());
        recorded(ob.reach(Stage::Welcome).unwrap());
        recorded(
            ob.satisfy(Stage::Welcome, Satisfaction::Acknowledged)
                .unwrap(),
        );
        recorded(ob.satisfy(Stage::GameVerification, confirm_game()).unwrap());
        let s = ob.summary();
        assert_eq!(s.game_bytes, None);
        assert_eq!(s.graphics, None);
        assert_eq!(s.controller, None);
        assert_eq!(s.detected_hardware, None);
        let absent = s.absent();
        for field in [
            "controller",
            "graphics",
            "graphics recommendation",
            "detected hardware",
            "installed size",
        ] {
            assert!(absent.contains(&field), "{field} must be reported absent");
        }
    }

    /// "The recommendation was not taken" and "there was no recommendation" are
    /// different facts, and the wireframe's RECOMMENDED badge depends on which.
    #[test]
    fn no_measurement_means_no_recommended_badge() {
        let dir = tempfile::tempdir().unwrap();
        let mut ob = fresh(dir.path());
        recorded(ob.reach(Stage::Welcome).unwrap());
        recorded(
            ob.satisfy(Stage::Welcome, Satisfaction::Acknowledged)
                .unwrap(),
        );
        recorded(ob.satisfy(Stage::GameVerification, confirm_game()).unwrap());
        recorded(
            ob.satisfy(
                Stage::SystemCheck,
                Satisfaction::Graphics {
                    chosen: RecordedPreset::Balanced,
                    recommended: None,
                    detected: None,
                },
            )
            .unwrap(),
        );
        let s = ob.summary();
        assert_eq!(s.graphics, Some(RecordedPreset::Balanced));
        assert_eq!(
            s.graphics_was_recommended, None,
            "an unmeasured machine must not be shown as agreeing with a \
             recommendation that never existed"
        );
        assert!(s.absent().contains(&"graphics recommendation"));
    }

    /// A stage with no wireframe must say so rather than carry an invented
    /// heading that reads as specified.
    #[test]
    fn an_unspecified_screen_is_reported_as_unspecified() {
        assert_eq!(Stage::MigrationCopy.wireframe_section(), None);
        assert_eq!(Stage::Welcome.wireframe_section(), Some(1));
        assert_eq!(Stage::ControllerSetup.wireframe_section(), Some(3));
        assert_eq!(Stage::SystemCheck.wireframe_section(), Some(4));
    }

    // -----------------------------------------------------------------------
    // The disk format
    // -----------------------------------------------------------------------

    /// The journal is read by every future launcher. If a key changes, this fails
    /// and whoever changed it has to think about installs already on disk, where
    /// losing an entry means redoing a copy.
    #[test]
    fn the_on_disk_shape_is_pinned() {
        let entry = Entry {
            at_ms: 42,
            event: Event::Satisfied {
                stage: Stage::MigrationOffer,
                how: Satisfaction::Migrating {
                    source: PathBuf::from("legacy"),
                },
            },
        };
        let v = serde_json::to_value(&entry).unwrap();
        assert_eq!(v["at_ms"], 42);
        assert_eq!(v["event"]["event"], "satisfied");
        assert_eq!(v["event"]["stage"], "migration_offer");
        assert_eq!(v["event"]["how"]["how"], "migrating");

        let round: Entry = serde_json::from_value(v).unwrap();
        assert_eq!(round, entry, "the format must survive a round trip");

        let latch = Entry {
            at_ms: 1,
            event: Event::Completed { flow_version: 1 },
        };
        let v = serde_json::to_value(&latch).unwrap();
        assert_eq!(v["event"]["event"], "completed");
        assert_eq!(v["event"]["flow_version"], 1);
    }

    /// slug() is what humans read in logs and refusal messages; the serde name is
    /// what is on disk. They must be the same string, or a rename silently
    /// orphans every journal already written.
    #[test]
    fn every_slug_matches_its_serialized_name() {
        for stage in Stage::ALL {
            let json = serde_json::to_value(stage).unwrap();
            assert_eq!(
                json.as_str(),
                Some(stage.slug()),
                "{:?} serializes as {json} but its slug is {}",
                stage,
                stage.slug()
            );
        }
        for task in PrepTask::ALL {
            let json = serde_json::to_value(task).unwrap();
            assert_eq!(json.as_str(), Some(task.slug()));
        }
        for (preset, name) in [
            (RecordedPreset::Performance, "performance"),
            (RecordedPreset::Balanced, "balanced"),
            (RecordedPreset::Quality, "quality"),
            (RecordedPreset::Ultra, "ultra"),
        ] {
            assert_eq!(serde_json::to_value(preset).unwrap(), name);
        }
    }

    /// index() falls back to zero if a variant is missing from ALL, which would
    /// silently reorder the flow. It must never be missing.
    #[test]
    fn every_stage_appears_exactly_once_in_the_flow_order() {
        let mut seen = std::collections::HashSet::new();
        for stage in Stage::ALL {
            assert!(seen.insert(*stage), "{:?} listed twice", stage);
        }
        for (i, stage) in Stage::ALL.iter().enumerate() {
            assert_eq!(stage.index(), i);
        }
        for stage in Stage::ALL {
            assert!(!stage.slug().is_empty());
            assert!(!stage.skip_rule().because().is_empty());
        }
    }

    /// The five tasks and their names are a locked ruling, not a choice made
    /// here. RECONCILIATION_RULINGS.md 105.
    #[test]
    fn the_preparation_task_list_is_the_ruled_five_in_order() {
        let labels: Vec<&str> = PrepTask::ALL.iter().map(|t| t.label()).collect();
        assert_eq!(
            labels,
            vec![
                "Copy game files",
                "Install Reborn Dolphin",
                "Apply textures",
                "Apply your controller and graphics",
                "Check everything",
            ]
        );
    }

    #[test]
    fn a_recorded_preset_survives_a_round_trip_through_the_runtime_type() {
        for p in [
            GraphicsPreset::Performance,
            GraphicsPreset::Balanced,
            GraphicsPreset::Quality,
            GraphicsPreset::Ultra,
        ] {
            assert_eq!(RecordedPreset::from(p).preset(), p);
        }
    }

    #[test]
    fn every_refusal_says_something_actionable() {
        for r in [
            Refusal::AlreadyComplete,
            Refusal::UseFinish,
            Refusal::OutOfOrder {
                expected: Stage::Welcome,
                got: Stage::Preparation,
            },
            Refusal::NotOnThisMachine {
                stage: Stage::MigrationOffer,
                why: "nothing was found",
            },
            Refusal::NotReached {
                stage: Stage::Preparation,
            },
            Refusal::Irreversible {
                stage: Stage::MigrationCopy,
            },
            Refusal::WrongEvidence {
                stage: Stage::Welcome,
            },
            Refusal::WrongSource {
                given: PathBuf::from("a"),
                found: PathBuf::from("b"),
            },
            Refusal::BlockedMigration {
                why: "no !BACKUP".to_string(),
            },
            Refusal::TasksOutstanding {
                first: PrepTask::ApplyTextures,
            },
            Refusal::Outstanding {
                stage: Stage::SystemCheck,
            },
            Refusal::NoWayBack {
                stage: Stage::Welcome,
            },
        ] {
            let m = r.message();
            assert!(m.len() > 20, "too terse to act on: {m}");
            assert!(m.ends_with('.'), "not a sentence: {m}");
            assert!(!m.contains('\u{2014}'), "no em dashes: {m}");
        }
    }

    /// Preparation records its tasks so a resume does not repeat them, and
    /// recording one twice must not double the journal.
    #[test]
    fn recording_a_finished_task_twice_writes_nothing_the_second_time() {
        let dir = tempfile::tempdir().unwrap();
        let mut ob = fresh(dir.path());
        walk_to_preparation(&mut ob);
        recorded(ob.reach(Stage::Preparation).unwrap());
        recorded(ob.record_task(PrepTask::ApplyTextures).unwrap());

        let before = std::fs::read_to_string(dir.path().join(JOURNAL)).unwrap();
        assert_eq!(
            ob.record_task(PrepTask::ApplyTextures).unwrap(),
            Advance::AlreadyRecorded
        );
        assert_eq!(
            std::fs::read_to_string(dir.path().join(JOURNAL)).unwrap(),
            before
        );
        assert_eq!(ob.preparation_remaining().len(), 4);
    }

    /// A full run, end to end, on a machine that has a 2.1 install.
    #[test]
    fn a_machine_with_a_legacy_install_walks_every_stage_and_latches_once() {
        let dir = tempfile::tempdir().unwrap();
        let legacy_dir = dir.path().join("legacy");
        std::fs::create_dir_all(&legacy_dir).unwrap();
        let install = legacy(&legacy_dir, false, false);
        let env = Environment::with_legacy_install(install.clone());
        let mut ob = Onboarding::open_at(dir.path(), env, FixedClock::new(1)).unwrap();

        recorded(ob.reach(Stage::Welcome).unwrap());
        recorded(
            ob.satisfy(Stage::Welcome, Satisfaction::Acknowledged)
                .unwrap(),
        );
        recorded(
            ob.satisfy(
                Stage::GameVerification,
                Satisfaction::GameConfirmed {
                    total_bytes: Some(4_400_000_000),
                },
            )
            .unwrap(),
        );
        recorded(
            ob.satisfy(
                Stage::MigrationOffer,
                Satisfaction::Migrating {
                    source: install.root.clone(),
                },
            )
            .unwrap(),
        );
        recorded(ob.reach(Stage::MigrationCopy).unwrap());
        recorded(
            ob.satisfy(Stage::MigrationCopy, Satisfaction::Migrated { bytes: 5 })
                .unwrap(),
        );
        recorded(ob.satisfy(Stage::SystemCheck, graphics()).unwrap());
        recorded(ob.satisfy(Stage::ControllerSetup, pad()).unwrap());
        // Mods come before Preparation, which is what applies them.
        recorded(ob.reach(Stage::ModSelection).unwrap());
        recorded(
            ob.satisfy(Stage::ModSelection, Satisfaction::NoMods)
                .unwrap(),
        );
        recorded(ob.reach(Stage::Preparation).unwrap());
        for t in PrepTask::ALL {
            recorded(ob.record_task(*t).unwrap());
        }
        recorded(
            ob.satisfy(Stage::Preparation, Satisfaction::Prepared)
                .unwrap(),
        );
        recorded(ob.reach(Stage::Completion).unwrap());
        recorded(ob.finish().unwrap());

        let done = ob.completion().expect("a latched completion");
        assert_eq!(done.flow_version, FLOW_VERSION);
        assert_eq!(done.summary.migrated_from, Some(install.root.clone()));
        assert_eq!(done.summary.migrated_bytes, Some(5));
        assert_eq!(done.summary.game_bytes, Some(4_400_000_000));
        assert_eq!(done.summary.controller.as_deref(), Some("XInput/0/Gamepad"));
        assert_eq!(done.summary.tasks_done.len(), PrepTask::ALL.len());

        let latches = std::fs::read_to_string(dir.path().join(JOURNAL))
            .unwrap()
            .lines()
            .filter(|l| l.contains("\"completed\""))
            .count();
        assert_eq!(latches, 1, "exactly one latch, ever");
    }
    // ---- mod selection -----------------------------------------------------
    //
    // The stage exists because the 2.1 installer asked this question by opening
    // JSGME mid-wizard, which applies as it goes. These tests pin the two
    // properties that made moving it here worth doing: the choice lands BEFORE
    // anything is applied, and declining is recorded as a decision rather than
    // read back as silence.

    #[test]
    fn mods_are_chosen_before_anything_is_applied() {
        let dir = tempfile::tempdir().unwrap();
        let mut ob = fresh(dir.path());
        recorded(ob.reach(Stage::Welcome).unwrap());
        recorded(
            ob.satisfy(Stage::Welcome, Satisfaction::Acknowledged)
                .unwrap(),
        );
        recorded(ob.satisfy(Stage::GameVerification, confirm_game()).unwrap());
        recorded(ob.satisfy(Stage::SystemCheck, graphics()).unwrap());
        recorded(ob.satisfy(Stage::ControllerSetup, pad()).unwrap());

        // Opening the screen is allowed. reach() stays permissive on purpose so
        // a resumed run can re-show a stage.
        recorded(ob.reach(Stage::Preparation).unwrap());

        // Applying is not. record_task is the call that copies files and
        // composes mods, so it is the one that has to refuse, and it has to
        // refuse BEFORE doing the work rather than when the stage reports being
        // finished. This assertion is why the guard was moved there: it used to
        // pass, and the first task ran with the mod question unanswered.
        assert_eq!(
            ob.record_task(PrepTask::CopyGameFiles).unwrap(),
            Advance::Refused(Refusal::OutOfOrder {
                expected: Stage::ModSelection,
                got: Stage::Preparation,
            }),
            "nothing may be applied before the mod choice is recorded"
        );
        assert!(
            ob.preparation_remaining().contains(&PrepTask::CopyGameFiles),
            "a refused task must not be marked done"
        );
    }

    #[test]
    fn declining_mods_is_a_decision_and_not_an_absence() {
        let dir = tempfile::tempdir().unwrap();

        // Never asked.
        let ob = fresh(dir.path());
        assert_eq!(
            ob.summary().mods_chosen, None,
            "an unanswered question must not read as an answer"
        );
        assert!(ob.summary().absent().contains(&"mods"));

        // Asked and declined.
        let mut ob = fresh(dir.path());
        walk_to_preparation(&mut ob);
        let s = ob.summary();
        assert_eq!(
            s.mods_chosen,
            Some(Vec::new()),
            "declining is an empty list, not a missing one"
        );
        assert!(
            !s.absent().contains(&"mods"),
            "a recorded decline is not an absent field"
        );
    }

    #[test]
    fn a_choice_taken_without_showing_conflicts_says_so() {
        let dir = tempfile::tempdir().unwrap();
        let mut ob = fresh(dir.path());
        recorded(ob.reach(Stage::Welcome).unwrap());
        recorded(
            ob.satisfy(Stage::Welcome, Satisfaction::Acknowledged)
                .unwrap(),
        );
        recorded(ob.satisfy(Stage::GameVerification, confirm_game()).unwrap());
        recorded(ob.satisfy(Stage::SystemCheck, graphics()).unwrap());
        recorded(ob.satisfy(Stage::ControllerSetup, pad()).unwrap());
        recorded(ob.reach(Stage::ModSelection).unwrap());

        // Seven mods on this machine write the same football texture. Picking
        // two of them without being told that is a real state, and the summary
        // has to carry it so the completion screen cannot claim the selection
        // was informed.
        recorded(
            ob.satisfy(
                Stage::ModSelection,
                Satisfaction::ModsChosen {
                    packages: vec!["Aqua Football".into(), "Blue Football".into()],
                    conflicts_shown: false,
                },
            )
            .unwrap(),
        );

        let s = ob.summary();
        assert_eq!(
            s.mods_chosen.as_deref(),
            Some(&["Aqua Football".to_string(), "Blue Football".to_string()][..])
        );
        assert_eq!(
            s.mod_conflicts_shown,
            Some(false),
            "an uninformed selection must not be reportable as an informed one"
        );
    }

    #[test]
    fn mod_selection_is_reversible_and_declinable() {
        // Both matter for the same reason: this is the one stage a player is
        // most likely to want to change their mind about, and it is safe to,
        // because nothing has been written yet.
        assert!(
            !Stage::ModSelection.is_irreversible(),
            "nothing is applied here, so going back cannot cost anything"
        );
        assert!(
            Stage::ModSelection.skip_rule().is_declinable(),
            "no mods is the normal case"
        );
        assert!(Stage::ModSelection.allows_back());
    }

    #[test]
    fn a_controller_answer_carries_the_emulators_own_device_string() {
        // The whole point of deriving this rather than accepting it: a friendly
        // name written into GCPadNew.ini parses and binds nothing.
        let scan = crate::devices::DeviceScanner::scan();
        let pads = scan.playable_pads();
        if pads.is_empty() {
            // A real and recurring state on this machine, since a wireless pad
            // goes to sleep. The refusal has to say which search it was.
            let refused = Satisfaction::controller_on_port(&scan, 0).unwrap_err();
            assert!(
                refused.contains("port 1"),
                "the refusal must name the port a player would recognize: {refused}"
            );
            if !scan.is_complete() {
                assert!(
                    refused.contains("XInput"),
                    "a partial search must say so rather than claim there is no pad: \
                     {refused}"
                );
            }
            return;
        }

        let answer = Satisfaction::controller_on_port(&scan, 0).unwrap();
        let Satisfaction::Controller { device, port, .. } = answer else {
            panic!("a pad on a port must produce a controller answer");
        };
        assert_eq!(port, 0);
        assert!(
            device.matches('/').count() >= 2,
            "the device string must be the emulator's source/id/name, got {device:?}"
        );
        assert!(
            Stage::ControllerSetup.accepts(&Satisfaction::controller_on_port(&scan, 0).unwrap()),
            "the derived answer must be one the stage accepts"
        );
    }

    #[test]
    fn a_port_with_no_pad_is_refused_rather_than_guessed() {
        let scan = crate::devices::DeviceScanner::scan();
        // Port 200 cannot exist. The alternative to refusing is falling back to
        // whatever pad is first, which would bind a player's port 4 selection to
        // someone else's pad.
        let refused = Satisfaction::controller_on_port(&scan, 200).unwrap_err();
        assert!(
            refused.contains("port 201"),
            "the refusal must name the port: {refused}"
        );
    }

    #[test]
    fn the_mods_answer_records_resolved_ids_not_typed_names() {
        // An empty selection is still a ModsChosen rather than a NoMods: the
        // caller asked for a list, and it happened to be empty. NoMods is the
        // player declining, which is a different sentence on the Ready screen.
        let answer = Satisfaction::mods_chosen(&[], true);
        assert_eq!(
            answer,
            Satisfaction::ModsChosen {
                packages: vec![],
                conflicts_shown: true
            }
        );
        assert!(Stage::ModSelection.accepts(&answer));
    }

    #[test]
    fn no_message_shown_to_a_player_has_a_collapsed_line_continuation() {
        // A wrapped string literal whose backslash gets lost keeps the source
        // indentation inside the message, so it reads with a 20 space gap in the
        // middle of a sentence. It compiles, every contains() assertion still
        // passes, and it is invisible in the source. This caught a real one in
        // controller_on_port.
        let mut messages: Vec<String> = Vec::new();

        for stage in Stage::ALL {
            messages.push(stage.skip_rule().because().to_string());
            messages.push(Refusal::NotReached { stage: *stage }.message());
            messages.push(Refusal::Irreversible { stage: *stage }.message());
            messages.push(Refusal::NoWayBack { stage: *stage }.message());
            messages.push(
                Refusal::OutOfOrder {
                    expected: *stage,
                    got: Stage::Completion,
                }
                .message(),
            );
        }
        for task in PrepTask::ALL {
            messages.push(Refusal::TasksOutstanding { first: *task }.message());
            messages.push(task.label().to_string());
        }
        // The derived answers, which is where the real one was. Both branches:
        // the wording differs between a complete search and a partial one, and
        // only the partial one was broken.
        let complete = Enumeration {
            devices: Vec::new(),
            not_searched: Vec::new(),
        };
        let partial = Enumeration {
            devices: Vec::new(),
            not_searched: vec![(
                crate::devices::Backend::Sdl,
                "no emulator path was given",
            )],
        };
        for scan in [&complete, &partial] {
            for port in [0u8, 3, 200] {
                if let Err(why) = Satisfaction::controller_on_port(scan, port) {
                    messages.push(why);
                }
            }
        }

        for m in messages {
            assert!(
                !m.contains("  "),
                "a doubled space means a line continuation was lost: {m:?}"
            );
        }
    }

}
