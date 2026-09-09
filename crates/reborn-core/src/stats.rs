//! Match results: captured by the emulator, queued by the launcher, sent later.
//!
//! # The twelve fields, and the screen that had to be thrown away
//!
//! Dolphin's `NflsStatTracker` writes one `nsr.game.v1` JSON file per finished
//! game. It records **twelve** per-player counters, and **none of them is
//! yardage**:
//!
//! ```text
//! pass_td  rush_td  rec_td   pick_six    fum_td         interceptions
//! tackles  sacks    fumbles  thrown_int  forced_fumbles fumble_rec
//! ```
//!
//! An earlier draft of the box score showed Total yards, Passing and Rushing.
//! Those statistics do not exist anywhere in the capture, in the DOL hook that
//! feeds it, or in the game. The whole screen was invalid and had to be redrawn
//! the moment a real capture was exported. That is why the twelve are named
//! fields on [`PlayerStats`] rather than a free map: a thirteenth cannot be
//! added by accident, and [`StatField`] is the single list of their on-disk
//! names.
//!
//! # What this module does and does not decide
//!
//! It owns the queue, the idempotency key, and the validation pass. It does
//! **not** own the network and it does **not** own routing.
//!
//! - The network is [`Submitter`], which the caller implements. Nothing here
//!   opens a socket, holds a credential, or knows a URL.
//! - Routing is the caller's, decided at launch and recorded, never derived
//!   from the file. The identical `nsr.game.v1` file is produced by an offline
//!   CPU game, a league game and a ranked game, and `source` is the literal
//!   string `"dolphin"` in every capture on disk. A capture cannot say where it
//!   should go. What this module enforces is the other half of that rule: a
//!   capture that no session accounts for is quarantined rather than guessed
//!   at, see [`QuarantineReason::NoSession`].
//!
//! # The two orderings that keep a game from being lost or sent twice
//!
//! **Enqueue writes the payload before the record.** If the process dies
//! between the two, the bytes are still on disk and the source file has not
//! been removed yet, so the next ingest re-enqueues it and the content key
//! makes that a no-op. The other order would leave a record pointing at a game
//! whose data is gone.
//!
//! **Submission appends to the ledger before updating the entry.** If the
//! process dies between the two, the ledger says sent and the entry still says
//! queued. Every reader resolves that in the ledger's favour, so the game is
//! never sent a second time. The other order would resend it, which on this
//! project's own history means a duplicate result on a real fixture.
//!
//! # Why the ledger outlives the payload
//!
//! [`StatsQueue::prune_terminal`] deletes payloads for games that have been
//! sent, and never deletes anything that has not. The ledger row stays
//! forever, because it is the only thing that stops a capture the player still
//! has a copy of from being submitted again after its queue payload is gone.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::manifest::Sha256Hex;
use crate::platform::{DataDir, Error, Platform, Result};
use crate::supervisor::{Clock, SystemClock};

/// The only capture schema this launcher accepts.
///
/// The deployed league endpoint compares this string exactly, so it must not be
/// bumped when the capture gains a field. The one sanctioned change to the
/// format is additive and top level, which is why [`Capture::extra`] exists.
pub const GAME_SCHEMA: &str = "nsr.game.v1";

/// NFL Street 2 is first to 36.
///
/// Used as a floor and nothing else. A final score below it means the game did
/// not finish normally, because the winning play overshoots the remaining gap
/// rather than stopping at it. It never decides who won.
pub const TARGET_SCORE: i64 = 36;

/// The wire payload cap inherited from the deployed endpoint.
///
/// Checked here so an oversized capture is refused before a request is built
/// rather than after it is rejected.
pub const MAX_PAYLOAD_BYTES: usize = 512 * 1024;

/// The folder Dolphin writes captures into, inside the per-launch user
/// directory. Named here so no caller has to guess it.
pub const DOLPHIN_CAPTURE_DIR: &str = "LeagueGameStats";

/// Where Dolphin will have written this launch's captures.
pub fn dolphin_capture_dir(user_dir: &Path) -> PathBuf {
    user_dir.join(DOLPHIN_CAPTURE_DIR)
}

// ---------------------------------------------------------------------------
// The twelve fields
// ---------------------------------------------------------------------------

/// One of the twelve per-player counters, and nothing else.
///
/// The discriminants are the array positions used by [`StatTotals`], so the
/// order of [`StatField::ALL`] is load bearing and is pinned by a test.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum StatField {
    PassTd = 0,
    RushTd = 1,
    RecTd = 2,
    PickSix = 3,
    FumTd = 4,
    Interceptions = 5,
    ForcedFumbles = 6,
    FumbleRec = 7,
    Tackles = 8,
    Sacks = 9,
    ThrownInt = 10,
    Fumbles = 11,
}

impl StatField {
    /// Every field, in capture order. Exactly twelve, and none is yardage.
    pub const ALL: [StatField; 12] = [
        StatField::PassTd,
        StatField::RushTd,
        StatField::RecTd,
        StatField::PickSix,
        StatField::FumTd,
        StatField::Interceptions,
        StatField::ForcedFumbles,
        StatField::FumbleRec,
        StatField::Tackles,
        StatField::Sacks,
        StatField::ThrownInt,
        StatField::Fumbles,
    ];

    /// The key as it appears in the capture JSON. Not ours to rename.
    pub fn key(self) -> &'static str {
        match self {
            StatField::PassTd => "pass_td",
            StatField::RushTd => "rush_td",
            StatField::RecTd => "rec_td",
            StatField::PickSix => "pick_six",
            StatField::FumTd => "fum_td",
            StatField::Interceptions => "interceptions",
            StatField::ForcedFumbles => "forced_fumbles",
            StatField::FumbleRec => "fumble_rec",
            StatField::Tackles => "tackles",
            StatField::Sacks => "sacks",
            StatField::ThrownInt => "thrown_int",
            StatField::Fumbles => "fumbles",
        }
    }

    /// A short column heading, defined once so two front ends cannot disagree
    /// about what to call a statistic the player will compare between screens.
    pub fn label(self) -> &'static str {
        match self {
            StatField::PassTd => "Pass TD",
            StatField::RushTd => "Rush TD",
            StatField::RecTd => "Rec TD",
            StatField::PickSix => "Pick six",
            StatField::FumTd => "Fum TD",
            StatField::Interceptions => "INT",
            StatField::ForcedFumbles => "FF",
            StatField::FumbleRec => "Fum rec",
            StatField::Tackles => "Tkl",
            StatField::Sacks => "Sack",
            StatField::ThrownInt => "INT thrown",
            StatField::Fumbles => "FUM",
        }
    }

    fn index(self) -> usize {
        self as usize
    }
}

/// The twelve counters for one player, exactly as captured.
///
/// The counters are `i64` rather than `u32` on purpose. A negative value is a
/// bug worth reporting, and parsing it into an unsigned field would fail the
/// whole file instead, which loses the only copy of a game. It is read, then
/// flagged by [`codes::NEGATIVE_COUNTER`].
///
/// There is deliberately no `Default`. An all-zero `PlayerStats` is
/// indistinguishable from a real line where nothing happened, and handing one
/// out is how an unmeasured statistic becomes a published zero. Use
/// [`StatTotals`] when a zero start value is what you actually mean.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayerStats {
    pub pass_td: i64,
    pub rush_td: i64,
    pub rec_td: i64,
    pub pick_six: i64,
    pub fum_td: i64,
    pub interceptions: i64,
    pub forced_fumbles: i64,
    pub fumble_rec: i64,
    pub tackles: i64,
    pub sacks: i64,
    pub thrown_int: i64,
    pub fumbles: i64,

    /// Anything the tracker emitted that this launcher does not model.
    ///
    /// Kept rather than dropped so a capture survives a round trip through a
    /// launcher older than the fork that wrote it, and flagged by
    /// [`codes::UNKNOWN_STAT_FIELD`] so the gap is visible instead of silent.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl PlayerStats {
    pub fn get(&self, field: StatField) -> i64 {
        match field {
            StatField::PassTd => self.pass_td,
            StatField::RushTd => self.rush_td,
            StatField::RecTd => self.rec_td,
            StatField::PickSix => self.pick_six,
            StatField::FumTd => self.fum_td,
            StatField::Interceptions => self.interceptions,
            StatField::ForcedFumbles => self.forced_fumbles,
            StatField::FumbleRec => self.fumble_rec,
            StatField::Tackles => self.tackles,
            StatField::Sacks => self.sacks,
            StatField::ThrownInt => self.thrown_int,
            StatField::Fumbles => self.fumbles,
        }
    }

    /// Whether this line looks like the passer's.
    ///
    /// There is no position anywhere in the capture, so the passer is
    /// identified by what only a passer produces: a passing touchdown or a
    /// thrown interception.
    fn is_a_passer(&self) -> bool {
        self.pass_td > 0 || self.thrown_int > 0
    }
}

/// A sum of the twelve counters over some set of players.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StatTotals([i64; 12]);

impl StatTotals {
    pub fn get(&self, field: StatField) -> i64 {
        self.0[field.index()]
    }

    fn add(&mut self, stats: &PlayerStats) {
        for field in StatField::ALL {
            let slot = &mut self.0[field.index()];
            *slot = slot.saturating_add(stats.get(field));
        }
    }
}

// ---------------------------------------------------------------------------
// The capture
// ---------------------------------------------------------------------------

/// One team, as the capture names it.
///
/// Names arrive truncated at 14 characters, because the in-game text descriptor
/// has `maxlen 0x0E`. Resolving a truncated name to a real team is the server's
/// job, and an ambiguous prefix must resolve to nothing rather than to a coin
/// flip. Nothing here tries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Team {
    pub name: String,
    pub score: i64,
}

/// The optional per-team block. Present in five of the six captures on disk.
///
/// Its `style` field is the one to trust: the top level `style` array is in the
/// inverted side order, not `teams` order. See [`Capture::style_for_team`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeamTotals {
    pub score: i64,
    pub style: i64,
    pub total_td: i64,
    pub rush_td: i64,
    pub pass_td: i64,
    pub def_td: i64,
    pub sacks: i64,
    pub safeties: i64,
    pub int: i64,
    pub ff: i64,
    pub first_downs: i64,
    pub gamebreakers: i64,
    pub gb_2pt: i64,
    pub hot_spots: i64,
    pub credits: i64,
}

/// One player line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Player {
    pub name: String,
    /// The team name this line was attributed to. Which side that team was on
    /// is a separate question, and the one `sides_verified` answers.
    pub team: String,
    /// 0 or 1. Partitions the two rosters reliably even when the side to team
    /// attribution is wrong, which is why every cross check here groups by side
    /// and not by name.
    pub side: i64,
    /// Roster slot within the side.
    pub slot: i64,
    /// The game's own player identifier. The dedupe hash sorts on it, so a
    /// duplicate makes the sort order, and therefore the key, unknowable.
    pub digp: i64,
    pub stats: PlayerStats,
}

/// A whole capture file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Capture {
    /// Must be exactly [`GAME_SCHEMA`].
    pub schema: String,
    /// `"dolphin"` in every capture on disk. It does not distinguish an offline
    /// game from a ranked one, so it must never be used to route.
    pub source: String,
    /// UTC, stamped when the game ended, not when it was uploaded.
    pub captured_at: String,
    pub teams: Vec<Team>,
    /// Two entries, in the **inverted** side order. Read style through
    /// [`Capture::style_for_team`] instead of indexing this.
    pub style: Vec<i64>,
    /// Emitted conditionally. Absent in the one real capture that fails
    /// verification, which is not a coincidence: with no team totals there is
    /// nothing to verify the sides against.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub team_totals: Option<Vec<TeamTotals>>,
    /// Absent in a capture written before the fork emitted it unconditionally.
    /// Read it through [`Capture::sides_are_verified`], which treats absent as
    /// false. Never as true.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sides_verified: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub per_player_matched: Option<i64>,
    pub players: Vec<Player>,

    /// Additive top level keys, `reborn_match` among them.
    ///
    /// Preserved so that parsing and re-serializing a capture cannot silently
    /// drop the identity block that says which seat played it. That block sits
    /// outside the hashed set on purpose, because the two clients of one
    /// lockstep game each write a different one and both copies still have to
    /// hash the same.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// Why a capture could not be read as `nsr.game.v1`.
#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    #[error("the capture is not valid JSON: {0}. Keep the file and report it, it cannot be repaired here")]
    NotJson(String),

    #[error(
        "the capture is missing or mistypes something the schema requires: {0}. \
         A counter that was not measured is never filled in with a zero, so the \
         file is kept unparsed rather than published with invented numbers"
    )]
    Incomplete(String),
}

impl Capture {
    /// Parse a capture file.
    ///
    /// A missing field is an error rather than a default. A capture with no
    /// `tackles` key is not a game in which nobody made a tackle, and the
    /// difference matters the moment the number reaches a public profile.
    pub fn parse(bytes: &[u8]) -> std::result::Result<Self, CaptureError> {
        serde_json::from_slice(bytes).map_err(|e| match e.classify() {
            serde_json::error::Category::Data => CaptureError::Incomplete(e.to_string()),
            _ => CaptureError::NotJson(e.to_string()),
        })
    }

    /// Whether the tracker proved which side was which.
    ///
    /// A missing key is false. One real capture predates the unconditional
    /// emission and has no such key, and reading absence as agreement would
    /// publish exactly the results nobody checked.
    pub fn sides_are_verified(&self) -> bool {
        self.sides_verified.unwrap_or(false)
    }

    /// The style score for `teams[index]`.
    ///
    /// Read from the per-team block, never from the top level `style` array,
    /// which is in the inverted side order. Returns `None` when there is no
    /// per-team block: the inversion could be applied to the top level array
    /// instead, but the captures with no block are exactly the captures where
    /// side attribution is unproven, so inverting there would be a guess
    /// dressed up as a number.
    pub fn style_for_team(&self, index: usize) -> Option<i64> {
        self.team_totals.as_ref()?.get(index).map(|t| t.style)
    }

    /// Totals for one side of the ball, 0 or 1.
    pub fn totals_for_side(&self, side: i64) -> StatTotals {
        let mut totals = StatTotals::default();
        for p in self.players.iter().filter(|p| p.side == side) {
            totals.add(&p.stats);
        }
        totals
    }

    /// Totals for every player line attributed to a team name.
    pub fn totals_for_team(&self, team: &str) -> StatTotals {
        let mut totals = StatTotals::default();
        for p in self.players.iter().filter(|p| p.team == team) {
            totals.add(&p.stats);
        }
        totals
    }

    /// The single team name every player on this side carries, if there is one.
    fn side_label(&self, side: i64) -> Option<&str> {
        let mut names = self
            .players
            .iter()
            .filter(|p| p.side == side)
            .map(|p| p.team.as_str());
        let first = names.next()?;
        if names.all(|n| n == first) {
            Some(first)
        } else {
            None
        }
    }

    /// Index into `teams` of the winner, or `None` for a tie or a capture that
    /// does not have exactly two teams.
    ///
    /// The 36 point floor is not consulted. A short final score means the game
    /// did not finish, which is a reason to hold the result, never a reason to
    /// change who won it.
    pub fn winner(&self) -> Option<usize> {
        if self.teams.len() != 2 {
            return None;
        }
        match self.teams[0].score.cmp(&self.teams[1].score) {
            std::cmp::Ordering::Greater => Some(0),
            std::cmp::Ordering::Less => Some(1),
            std::cmp::Ordering::Equal => None,
        }
    }

    /// The bytes the local content key is taken over.
    ///
    /// Only `teams`, `style`, `team_totals` and `players` go in, with players
    /// sorted by `digp` and every stat map key sorted. `captured_at`, `source`,
    /// `schema`, the file path and everything in [`Capture::extra`] are left
    /// out, and that exclusion is the whole point: the two clients of one
    /// lockstep online game write different timestamps and different identity
    /// blocks for the same game, and both copies have to reduce to one key or
    /// the game is recorded twice.
    ///
    /// This is the launcher's **local** key. It is modeled on the deployed
    /// endpoint's canonicalization but has not been checked byte for byte
    /// against it, so it must not be sent as the wire `capture_hash` field
    /// until somebody has compared the two implementations on a real capture.
    /// Nothing in this module sends it anywhere.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut players: Vec<&Player> = self.players.iter().collect();
        // digp is the server's sort key. The rest are tiebreakers so that a
        // capture with a duplicate digp still produces a stable key here rather
        // than one that depends on read order. Such a capture is also flagged,
        // because the server's own tiebreak is unknown.
        players.sort_by(|a, b| {
            (a.digp, a.side, a.slot, a.name.as_str()).cmp(&(
                b.digp,
                b.side,
                b.slot,
                b.name.as_str(),
            ))
        });

        let mut root: BTreeMap<&str, Value> = BTreeMap::new();
        root.insert(
            "players",
            Value::Array(players.iter().map(|p| canonical_player(p)).collect()),
        );
        root.insert(
            "style",
            Value::Array(self.style.iter().map(|s| Value::from(*s)).collect()),
        );
        if let Some(totals) = &self.team_totals {
            root.insert(
                "team_totals",
                Value::Array(totals.iter().map(canonical_team_totals).collect()),
            );
        }
        root.insert(
            "teams",
            Value::Array(
                self.teams
                    .iter()
                    .map(|t| {
                        let mut m: BTreeMap<&str, Value> = BTreeMap::new();
                        m.insert("name", Value::from(t.name.as_str()));
                        m.insert("score", Value::from(t.score));
                        sorted_object(m)
                    })
                    .collect(),
            ),
        );

        // to_vec cannot fail for a Value built only out of strings, arrays and
        // integers. There is no float anywhere in the canonical set, which is
        // what makes the bytes reproducible on another machine.
        serde_json::to_vec(&sorted_object(root)).unwrap_or_default()
    }

    /// The local idempotency key for this game.
    pub fn content_key(&self) -> CaptureKey {
        CaptureKey(Sha256Hex::from_bytes(&self.canonical_bytes()))
    }

    /// Check shape and plausibility.
    ///
    /// `payload_bytes` is the length of the file as captured, used only for the
    /// wire cap. Pass `None` when the original byte count is not to hand:
    /// measuring a re-serialization instead would check a number the server is
    /// never going to see.
    ///
    /// **This validates shape and plausibility, not correctness.** A capture
    /// with no flags is not a verified result. It is a capture that showed
    /// nothing this module knows how to doubt. Every plausibility rule here was
    /// derived from six real captures, which is enough to catch a whole class
    /// of attribution bug and nowhere near enough to certify a box score.
    pub fn validate(&self, payload_bytes: Option<usize>) -> Validation {
        let mut flags = Vec::new();
        self.check_shape(payload_bytes, &mut flags);
        // The plausibility rules read per-side aggregates, which are meaningless
        // if the sides are not two clean rosters. Running them anyway would
        // report a pile of consequences of one structural problem.
        if !flags.iter().any(|f| f.severity == Severity::Shape) {
            self.check_plausibility(&mut flags);
        }
        Validation { flags }
    }

    fn check_shape(&self, payload_bytes: Option<usize>, flags: &mut Vec<Flag>) {
        if self.schema != GAME_SCHEMA {
            flags.push(Flag::shape(
                codes::WRONG_SCHEMA,
                format!(
                    "this file declares schema {:?}, not {GAME_SCHEMA}. The ingest \
                     endpoint compares the string exactly, so it would be refused. \
                     Keep the file and report which build wrote it.",
                    self.schema
                ),
            ));
        }
        if self.teams.len() != 2 {
            flags.push(Flag::shape(
                codes::WRONG_TEAM_COUNT,
                format!(
                    "a game has two teams and this capture has {}. It cannot be sent.",
                    self.teams.len()
                ),
            ));
        }
        if self.style.len() != 2 {
            flags.push(Flag::shape(
                codes::WRONG_STYLE_COUNT,
                format!(
                    "the style array must have two entries and this one has {}.",
                    self.style.len()
                ),
            ));
        }
        if let Some(totals) = &self.team_totals {
            if totals.len() != 2 {
                flags.push(Flag::shape(
                    codes::WRONG_TEAM_TOTALS_COUNT,
                    format!(
                        "the team_totals block must have two entries and this one has {}.",
                        totals.len()
                    ),
                ));
            }
        }
        if self.players.is_empty() {
            flags.push(Flag::shape(
                codes::NO_PLAYERS,
                "this capture has no player lines. The statread1 DOL hook is missing \
                 from the disc image that was booted, so there are no per-player \
                 statistics to send at all. Rebuild the image with the hook."
                    .to_string(),
            ));
        }
        if let Some(len) = payload_bytes {
            if len > MAX_PAYLOAD_BYTES {
                flags.push(Flag::shape(
                    codes::PAYLOAD_TOO_LARGE,
                    format!(
                        "the capture is {len} bytes and the endpoint accepts at most \
                         {MAX_PAYLOAD_BYTES}, so it will not be sent."
                    ),
                ));
            }
        }

        let team_names: BTreeSet<&str> = self.teams.iter().map(|t| t.name.as_str()).collect();
        let mut slots: BTreeSet<(i64, i64)> = BTreeSet::new();
        let mut digps: BTreeSet<i64> = BTreeSet::new();
        for p in &self.players {
            if !team_names.contains(p.team.as_str()) {
                flags.push(Flag::shape(
                    codes::UNKNOWN_TEAM_FOR_PLAYER,
                    format!(
                        "{} is attributed to team {:?}, which is not one of the two \
                         teams in this game.",
                        p.name, p.team
                    ),
                ));
            }
            if p.side != 0 && p.side != 1 {
                flags.push(Flag::shape(
                    codes::SIDE_OUT_OF_RANGE,
                    format!(
                        "{} is on side {}, and there are only sides 0 and 1.",
                        p.name, p.side
                    ),
                ));
            }
            if !slots.insert((p.side, p.slot)) {
                flags.push(Flag::shape(
                    codes::DUPLICATE_ROSTER_SLOT,
                    format!(
                        "two players share side {} slot {}. Statistics are attributed \
                         by slot, so nothing on this capture can be trusted to the \
                         right player.",
                        p.side, p.slot
                    ),
                ));
            }
            if !digps.insert(p.digp) {
                flags.push(Flag::shape(
                    codes::DUPLICATE_DIGP,
                    format!(
                        "two players share digp {}. The dedupe hash sorts players by \
                         digp, so with a duplicate the two clients of one game can \
                         hash it differently and record it twice.",
                        p.digp
                    ),
                ));
            }
            for field in StatField::ALL {
                let value = p.stats.get(field);
                if value < 0 {
                    flags.push(Flag::shape(
                        codes::NEGATIVE_COUNTER,
                        format!(
                            "{} has {} of {}, which is not a possible count. The file \
                             is kept for inspection.",
                            p.name,
                            value,
                            field.key()
                        ),
                    ));
                }
            }
        }

        for side in [0i64, 1] {
            let on_side: Vec<&Player> = self.players.iter().filter(|p| p.side == side).collect();
            if on_side.is_empty() {
                flags.push(Flag::shape(
                    codes::SIDE_HAS_NO_PLAYERS,
                    format!(
                        "side {side} has no players. A capture with one roster cannot be \
                         a completed two team game."
                    ),
                ));
                continue;
            }
            let names: BTreeSet<&str> = on_side.iter().map(|p| p.team.as_str()).collect();
            if names.len() > 1 {
                flags.push(Flag::shape(
                    codes::SIDE_SPANS_TWO_TEAMS,
                    format!(
                        "side {side} carries more than one team name ({}). The side to \
                         team attribution is broken, not merely unverified.",
                        names.into_iter().collect::<Vec<_>>().join(", ")
                    ),
                ));
            }
        }
    }

    fn check_plausibility(&self, flags: &mut Vec<Flag>) {
        match self.sides_verified {
            Some(true) => {}
            other => flags.push(Flag::plausibility(
                codes::SIDES_UNVERIFIED,
                format!(
                    "the tracker could not confirm which player controlled which team{}. \
                     Hold this result rather than counting it.",
                    if other.is_none() {
                        ", and this capture predates the key that would say so, which is \
                         read as unverified and never as verified"
                    } else {
                        ""
                    }
                ),
            )),
        }

        if self.team_totals.is_none() {
            flags.push(Flag::plausibility(
                codes::NO_TEAM_TOTALS,
                "this capture has no team_totals block, so there was nothing for the \
                 tracker to verify the sides against and no style score can be \
                 attributed to a team."
                    .to_string(),
            ));
        }

        if let Some(declared) = self.per_player_matched {
            if declared != self.players.len() as i64 {
                flags.push(Flag::plausibility(
                    codes::PLAYER_COUNT_MISMATCH,
                    format!(
                        "the tracker says it matched {declared} players and wrote {}. \
                         Some lines are missing or duplicated.",
                        self.players.len()
                    ),
                ));
            }
        }

        if let Some(totals) = &self.team_totals {
            for (i, team) in self.teams.iter().enumerate() {
                if let Some(t) = totals.get(i) {
                    if t.score != team.score {
                        flags.push(Flag::plausibility(
                            codes::TEAM_TOTAL_SCORE_MISMATCH,
                            format!(
                                "{} is scored {} in teams and {} in team_totals. The \
                                 capture contradicts itself about the final score.",
                                team.name, team.score, t.score
                            ),
                        ));
                    }
                }
            }
        }

        let high = self.teams.iter().map(|t| t.score).max().unwrap_or(0);
        if high < TARGET_SCORE {
            flags.push(Flag::plausibility(
                codes::BELOW_TARGET_SCORE,
                format!(
                    "the winning score is {high} and this game is first to {TARGET_SCORE}, \
                     so it did not finish normally. This does not change who won."
                ),
            ));
        }
        if self.teams.len() == 2 && self.winner().is_none() {
            flags.push(Flag::plausibility(
                codes::TIED_FINAL_SCORE,
                format!(
                    "the game is recorded as a {high} to {high} tie. First to \
                     {TARGET_SCORE} does not end level, so there is no winner to report."
                ),
            ));
        }

        for p in &self.players {
            // The finding this check exists for: in a real capture a
            // quarterback is credited with 2 sacks and 5 thrown interceptions,
            // and a second offensive player with 1 sack. That is what a tracker
            // attributing defensive plays by roster slot rather than by player
            // would look like.
            //
            // It is a flag and not a verdict. NFL Street 2 fields the same seven
            // players on both sides of the ball, so a passer genuinely can make
            // a defensive play. The narrow predicate is deliberate: widening it
            // to any offensive player with a sack would flag most of a normal
            // box score and the signal would be worth nothing.
            if p.stats.is_a_passer() && p.stats.sacks > 0 {
                flags.push(Flag::plausibility(
                    codes::PASSER_CREDITED_WITH_SACKS,
                    format!(
                        "{} threw for {} and was intercepted {} times, and is also \
                         credited with {} sacks. Defensive plays may be attributed by \
                         roster slot rather than by player. Check this against the \
                         tracker before it reaches a profile.",
                        p.name, p.stats.pass_td, p.stats.thrown_int, p.stats.sacks
                    ),
                ));
            }
            if p.stats.pick_six > p.stats.interceptions {
                flags.push(Flag::plausibility(
                    codes::PICK_SIX_EXCEEDS_INTERCEPTIONS,
                    format!(
                        "{} has {} pick sixes and {} interceptions. A pick six is an \
                         interception returned for a score, so it cannot exceed them.",
                        p.name, p.stats.pick_six, p.stats.interceptions
                    ),
                ));
            }
            if p.stats.fum_td > p.stats.fumble_rec {
                flags.push(Flag::plausibility(
                    codes::FUM_TD_EXCEEDS_RECOVERIES,
                    format!(
                        "{} has {} fumble return scores and {} fumble recoveries. A \
                         returned fumble has to be recovered first.",
                        p.name, p.stats.fum_td, p.stats.fumble_rec
                    ),
                ));
            }
            for extra in p.stats.extra.keys() {
                flags.push(Flag::plausibility(
                    codes::UNKNOWN_STAT_FIELD,
                    format!(
                        "{} carries a statistic named {:?}, which this launcher does not \
                         model. The value is preserved in the file. Add the field here \
                         before showing it anywhere.",
                        p.name, extra
                    ),
                ));
            }
        }

        // Aggregates that have to agree between the two sides. All three hold in
        // every one of the six real captures on disk, and all three break in
        // exactly the way a side or slot misattribution would break them.
        for side in [0i64, 1] {
            let mine = self.totals_for_side(side);
            let theirs = self.totals_for_side(1 - side);
            let label = self
                .side_label(side)
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("side {side}"));

            let passing = mine.get(StatField::PassTd);
            let receiving = mine.get(StatField::RecTd);
            if passing != receiving {
                flags.push(Flag::plausibility(
                    codes::PASSING_AND_RECEIVING_DISAGREE,
                    format!(
                        "{label} has {passing} passing scores and {receiving} receiving \
                         scores. Every touchdown pass is caught by a teammate, so these \
                         have to match."
                    ),
                ));
            }

            let thrown = mine.get(StatField::ThrownInt);
            let caught = theirs.get(StatField::Interceptions);
            if thrown != caught {
                flags.push(Flag::plausibility(
                    codes::INTERCEPTIONS_DO_NOT_CROSS_OVER,
                    format!(
                        "{label} threw {thrown} interceptions and the other side is \
                         credited with {caught}. Every thrown interception is caught by \
                         an opponent, so a mismatch means plays are landing on the wrong \
                         side."
                    ),
                ));
            }

            let forced = mine.get(StatField::ForcedFumbles);
            let opponent_fumbles = theirs.get(StatField::Fumbles);
            if forced > opponent_fumbles {
                flags.push(Flag::plausibility(
                    codes::FORCED_FUMBLES_EXCEED_OPPONENT_FUMBLES,
                    format!(
                        "{label} forced {forced} fumbles and the other side only fumbled \
                         {opponent_fumbles} times. A forced fumble is one of the \
                         opponent's fumbles, so it cannot exceed them."
                    ),
                ));
            }
        }
    }
}

fn canonical_player(p: &Player) -> Value {
    let mut stats: BTreeMap<&str, Value> = BTreeMap::new();
    for field in StatField::ALL {
        stats.insert(field.key(), Value::from(p.stats.get(field)));
    }
    let mut m: BTreeMap<&str, Value> = BTreeMap::new();
    m.insert("digp", Value::from(p.digp));
    m.insert("name", Value::from(p.name.as_str()));
    m.insert("side", Value::from(p.side));
    m.insert("slot", Value::from(p.slot));
    m.insert("stats", sorted_object(stats));
    m.insert("team", Value::from(p.team.as_str()));
    sorted_object(m)
}

fn canonical_team_totals(t: &TeamTotals) -> Value {
    let mut m: BTreeMap<&str, Value> = BTreeMap::new();
    m.insert("credits", Value::from(t.credits));
    m.insert("def_td", Value::from(t.def_td));
    m.insert("ff", Value::from(t.ff));
    m.insert("first_downs", Value::from(t.first_downs));
    m.insert("gamebreakers", Value::from(t.gamebreakers));
    m.insert("gb_2pt", Value::from(t.gb_2pt));
    m.insert("hot_spots", Value::from(t.hot_spots));
    m.insert("int", Value::from(t.int));
    m.insert("pass_td", Value::from(t.pass_td));
    m.insert("rush_td", Value::from(t.rush_td));
    m.insert("sacks", Value::from(t.sacks));
    m.insert("safeties", Value::from(t.safeties));
    m.insert("score", Value::from(t.score));
    m.insert("style", Value::from(t.style));
    m.insert("total_td", Value::from(t.total_td));
    sorted_object(m)
}

/// Build a JSON object whose keys are in sorted order whichever `Map` backing
/// serde_json was compiled with.
///
/// With the default backing this is automatic. With the `preserve_order` feature
/// it is not, and the canonical bytes would then depend on insertion order,
/// which would silently change every content key in the queue and orphan every
/// ledger row. Feeding an already sorted iterator makes both cases identical.
fn sorted_object(fields: BTreeMap<&str, Value>) -> Value {
    Value::Object(
        fields
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect(),
    )
}

/// PHP `trim()`: only " \t\n\r\0\x0B". Rust's `trim()` strips more, so it is not used here.
fn php_trim(s: &str) -> &str {
    s.trim_matches(|c| matches!(c, ' ' | '\t' | '\n' | '\r' | '\0' | '\x0B'))
}

/// PHP `(int)` for every shape a real capture carries: a JSON number, a bool,
/// or null. For a non-numeric string this does not reproduce true PHP
/// `(int)` (which would read a leading numeric prefix, so `"12abc"` becomes
/// `12`); it yields `0` instead, because that is what the backend's own
/// `Number()`-based cast also yields for that string. The backend
/// recomputes this hash and refuses a mismatch, so agreement with the
/// backend, not with PHP's parsing quirks, is the property the wire
/// actually depends on.
fn php_int(v: &Value) -> i64 {
    match v {
        Value::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|f| f.trunc() as i64)).unwrap_or(0),
        Value::String(s) => s.trim().parse::<f64>().map(|f| f.trunc() as i64).unwrap_or(0),
        Value::Bool(b) => i64::from(*b),
        _ => 0,
    }
}

/// A PHP assoc array after ksort(): keys in byte order, values cast to int, and an EMPTY one is `[]`.
fn php_sorted_int_map(v: Option<&Value>) -> String {
    let Some(Value::Object(m)) = v else { return "[]".to_string() };
    if m.is_empty() {
        return "[]".to_string();
    }
    let sorted: BTreeMap<&String, i64> = m.iter().map(|(k, v)| (k, php_int(v))).collect();
    let body: Vec<String> = sorted
        .iter()
        .map(|(k, v)| format!("{}:{}", serde_json::to_string(k).unwrap_or_default(), v))
        .collect();
    format!("{{{}}}", body.join(","))
}

fn php_str(v: Option<&Value>) -> String {
    let owned;
    let s: &str = match v {
        Some(Value::String(s)) => s.as_str(),
        // A numeric name is real: the backend does `phpTrim(String(s ?? ""))`,
        // so a JSON number here must stringify before trimming or the
        // backend recomputes a different name and refuses the hash.
        Some(Value::Number(n)) => {
            owned = n.to_string();
            owned.as_str()
        }
        Some(Value::Null) | None => "",
        // A bool, array or object is not a shape either the game or the site
        // ever produces for a name. Very large or exponential numbers can
        // also render differently between Rust's and JavaScript's number
        // formatting, but neither side emits one here.
        _ => "",
    };
    serde_json::to_string(php_trim(s)).unwrap_or_else(|_| "\"\"".to_string())
}

/// The wire hash's input: `si_capture_hash()` from the league site, byte for byte.
///
/// Key order is the PHP array's insertion order (teams, style, team_totals, players),
/// players are sorted by digp, every map is ksorted and int-cast, an absent
/// team_totals is `[]`, and an empty stats map is `[]` because PHP encodes an empty
/// array that way. `serde_json` and PHP's json_encode with UNESCAPED_SLASHES and
/// UNESCAPED_UNICODE escape strings identically for everything a team or player
/// name can hold, except U+2028 and U+2029, which PHP escapes and serde does not.
///
/// This is deliberately separate from [`Capture::canonical_bytes`]. That recipe
/// sorts all keys alphabetically and exists to name queue files locally; this one
/// reproduces the site's insertion order and int-casting rules so the resulting
/// hash matches what the PHP backend and the site itself compute over the same
/// bytes. The two must never be merged into one function.
pub fn wire_canonical(cap: &Value) -> String {
    let arr = |k: &str| -> Vec<&Value> {
        match cap.get(k) {
            Some(Value::Array(a)) => a.iter().collect(),
            _ => Vec::new(),
        }
    };
    let teams: Vec<String> = arr("teams")
        .iter()
        .map(|t| format!("{{\"name\":{},\"score\":{}}}", php_str(t.get("name")), php_int(t.get("score").unwrap_or(&Value::Null))))
        .collect();
    let style: Vec<String> = arr("style").iter().map(|s| php_int(s).to_string()).collect();
    let totals: Vec<String> = arr("team_totals").iter().map(|row| php_sorted_int_map(Some(row))).collect();
    let mut players: Vec<(i64, String)> = arr("players")
        .iter()
        .map(|p| {
            let digp = php_int(p.get("digp").unwrap_or(&Value::Null));
            let line = format!(
                "{{\"digp\":{},\"name\":{},\"side\":{},\"stats\":{}}}",
                digp,
                php_str(p.get("name")),
                php_int(p.get("side").unwrap_or(&Value::Null)),
                php_sorted_int_map(p.get("stats"))
            );
            (digp, line)
        })
        .collect();
    players.sort_by_key(|(d, _)| *d); // stable, like PHP 8's usort
    format!(
        "{{\"teams\":[{}],\"style\":[{}],\"team_totals\":[{}],\"players\":[{}]}}",
        teams.join(","),
        style.join(","),
        totals.join(","),
        players.into_iter().map(|(_, l)| l).collect::<Vec<_>>().join(",")
    )
}

/// The wire `capture_hash`: sha256 hex of [`wire_canonical`]. None when the bytes are not a JSON object.
pub fn wire_hash_of(bytes: &[u8]) -> Option<String> {
    let v: Value = serde_json::from_slice(bytes).ok()?;
    if !v.is_object() {
        return None;
    }
    use sha2::Digest;
    Some(hex::encode(sha2::Sha256::digest(wire_canonical(&v).as_bytes())))
}

// ---------------------------------------------------------------------------
// Validation results
// ---------------------------------------------------------------------------

/// How much a flag matters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// The capture is not a well formed `nsr.game.v1`. It must not be sent, and
    /// it is not deleted either.
    Shape,
    /// Well formed, but something in it does not look right. Publishing one of
    /// these without a person looking is the failure this severity exists for.
    Plausibility,
}

/// Stable flag codes.
///
/// Constants rather than inline strings so a typo is a compile error, and so a
/// front end can react to a specific finding without matching on prose that is
/// going to be reworded.
pub mod codes {
    pub const WRONG_SCHEMA: &str = "wrong-schema";
    pub const WRONG_TEAM_COUNT: &str = "wrong-team-count";
    pub const WRONG_STYLE_COUNT: &str = "wrong-style-count";
    pub const WRONG_TEAM_TOTALS_COUNT: &str = "wrong-team-totals-count";
    pub const NO_PLAYERS: &str = "no-players";
    pub const PAYLOAD_TOO_LARGE: &str = "payload-too-large";
    pub const UNKNOWN_TEAM_FOR_PLAYER: &str = "unknown-team-for-player";
    pub const SIDE_OUT_OF_RANGE: &str = "side-out-of-range";
    pub const SIDE_HAS_NO_PLAYERS: &str = "side-has-no-players";
    pub const SIDE_SPANS_TWO_TEAMS: &str = "side-spans-two-teams";
    pub const DUPLICATE_ROSTER_SLOT: &str = "duplicate-roster-slot";
    pub const DUPLICATE_DIGP: &str = "duplicate-digp";
    pub const NEGATIVE_COUNTER: &str = "negative-counter";

    pub const SIDES_UNVERIFIED: &str = "sides-unverified";
    pub const NO_TEAM_TOTALS: &str = "no-team-totals";
    pub const PLAYER_COUNT_MISMATCH: &str = "player-count-mismatch";
    pub const TEAM_TOTAL_SCORE_MISMATCH: &str = "team-total-score-mismatch";
    pub const BELOW_TARGET_SCORE: &str = "below-target-score";
    pub const TIED_FINAL_SCORE: &str = "tied-final-score";
    pub const PASSER_CREDITED_WITH_SACKS: &str = "passer-credited-with-sacks";
    pub const PICK_SIX_EXCEEDS_INTERCEPTIONS: &str = "pick-six-exceeds-interceptions";
    pub const FUM_TD_EXCEEDS_RECOVERIES: &str = "fum-td-exceeds-recoveries";
    pub const UNKNOWN_STAT_FIELD: &str = "unknown-stat-field";
    pub const PASSING_AND_RECEIVING_DISAGREE: &str = "passing-and-receiving-disagree";
    pub const INTERCEPTIONS_DO_NOT_CROSS_OVER: &str = "interceptions-do-not-cross-over";
    pub const FORCED_FUMBLES_EXCEED_OPPONENT_FUMBLES: &str =
        "forced-fumbles-exceed-opponent-fumbles";

    /// Every code, so a test can prove nothing is emitted that is not listed.
    pub const ALL: &[&str] = &[
        WRONG_SCHEMA,
        WRONG_TEAM_COUNT,
        WRONG_STYLE_COUNT,
        WRONG_TEAM_TOTALS_COUNT,
        NO_PLAYERS,
        PAYLOAD_TOO_LARGE,
        UNKNOWN_TEAM_FOR_PLAYER,
        SIDE_OUT_OF_RANGE,
        SIDE_HAS_NO_PLAYERS,
        SIDE_SPANS_TWO_TEAMS,
        DUPLICATE_ROSTER_SLOT,
        DUPLICATE_DIGP,
        NEGATIVE_COUNTER,
        SIDES_UNVERIFIED,
        NO_TEAM_TOTALS,
        PLAYER_COUNT_MISMATCH,
        TEAM_TOTAL_SCORE_MISMATCH,
        BELOW_TARGET_SCORE,
        TIED_FINAL_SCORE,
        PASSER_CREDITED_WITH_SACKS,
        PICK_SIX_EXCEEDS_INTERCEPTIONS,
        FUM_TD_EXCEEDS_RECOVERIES,
        UNKNOWN_STAT_FIELD,
        PASSING_AND_RECEIVING_DISAGREE,
        INTERCEPTIONS_DO_NOT_CROSS_OVER,
        FORCED_FUMBLES_EXCEED_OPPONENT_FUMBLES,
    ];
}

/// One finding about one capture.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Flag {
    /// One of [`codes::ALL`].
    pub code: &'static str,
    pub severity: Severity,
    /// A sentence naming what was seen and what to do about it.
    pub message: String,
}

impl Flag {
    fn shape(code: &'static str, message: String) -> Self {
        Flag {
            code,
            severity: Severity::Shape,
            message,
        }
    }
    fn plausibility(code: &'static str, message: String) -> Self {
        Flag {
            code,
            severity: Severity::Plausibility,
            message,
        }
    }
}

/// What [`Capture::validate`] found.
#[derive(Debug, Clone, Default)]
pub struct Validation {
    pub flags: Vec<Flag>,
}

impl Validation {
    /// Whether the capture is a well formed `nsr.game.v1`.
    ///
    /// True does not mean the numbers are right. See
    /// [`Validation::needs_review`].
    pub fn is_well_formed(&self) -> bool {
        !self.flags.iter().any(|f| f.severity == Severity::Shape)
    }

    /// Whether something in the capture should be looked at by a person before
    /// the result is counted.
    pub fn needs_review(&self) -> bool {
        self.flags
            .iter()
            .any(|f| f.severity == Severity::Plausibility)
    }

    /// The flags that stop the capture being sent at all.
    pub fn blocking(&self) -> Vec<&Flag> {
        self.flags
            .iter()
            .filter(|f| f.severity == Severity::Shape)
            .collect()
    }

    pub fn has(&self, code: &str) -> bool {
        self.flags.iter().any(|f| f.code == code)
    }

    pub fn codes(&self) -> Vec<&'static str> {
        self.flags.iter().map(|f| f.code).collect()
    }
}

// ---------------------------------------------------------------------------
// The queue
// ---------------------------------------------------------------------------

/// The identity of one game, derived from its contents.
///
/// Local to this install. See [`Capture::canonical_bytes`] for what is and is
/// not hashed, and for why it must not be sent as the wire hash yet.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CaptureKey(Sha256Hex);

impl CaptureKey {
    /// Key a capture that could not be parsed.
    ///
    /// Taken over the raw bytes, because the canonical set cannot be computed
    /// without reading the file. Two different encodings of the same unreadable
    /// game therefore key differently, which is acceptable only because neither
    /// of them can be submitted.
    pub fn of_bytes(bytes: &[u8]) -> Self {
        CaptureKey(Sha256Hex::from_bytes(bytes))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl std::fmt::Display for CaptureKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0.as_str())
    }
}

/// Why a capture is being held back rather than queued for sending.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum QuarantineReason {
    /// The bytes are not a capture this launcher can read. The file is kept.
    Unreadable { detail: String },
    /// Readable, but not a well formed `nsr.game.v1`. Carries the shape flag
    /// codes so the reason survives even after the payload is gone.
    FailedValidation { flags: Vec<String> },
    /// No launch this launcher started accounts for this file.
    ///
    /// Where a result should go is decided at launch and written down, never
    /// worked out from the file afterwards, because an offline game, a league
    /// game and a ranked game produce byte-identical captures. With no session
    /// there is no destination, and guessing one is how an offline game lands on
    /// a real league fixture.
    NoSession,
}

impl QuarantineReason {
    /// A sentence for the player.
    pub fn message(&self) -> String {
        match self {
            QuarantineReason::Unreadable { detail } => {
                format!("This result could not be read: {detail} It is being kept, not deleted.")
            }
            QuarantineReason::FailedValidation { flags } => format!(
                "This result is incomplete ({}) and cannot be sent. It is being kept so \
                 it can be reported.",
                flags.join(", ")
            ),
            QuarantineReason::NoSession => "This result was found on disk but no game this \
                 launcher started explains it, so there is nowhere it can safely be sent. \
                 It is being kept."
                .to_string(),
        }
    }
}

/// Where a queued capture has got to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EntryState {
    /// Waiting to be sent.
    Queued,
    /// Held back and never sent. Not an error, and not deleted.
    Quarantined { reason: QuarantineReason },
    /// The server stored it and counted it.
    Accepted,
    /// The server stored it and a person will decide.
    ///
    /// This is a **success**. Resending it is the mistake that produced a real
    /// outage on this project: the uploader read "review" as a failure and
    /// retried forever.
    Held { reason: String },
    /// The server refused it for good. Not retried, and not deleted.
    Rejected { reason: String },
    /// Too many transient failures. Still on disk, and
    /// [`StatsQueue::retry_stalled`] puts it back in the queue.
    Stalled { reason: String },
}

impl EntryState {
    /// Whether the result has reached the server and must never be sent again.
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            EntryState::Accepted | EntryState::Held { .. } | EntryState::Rejected { .. }
        )
    }

    /// Whether the next submission pass should try this one.
    pub fn is_pending(&self) -> bool {
        matches!(self, EntryState::Queued)
    }
}

/// The terminal states, as recorded in the ledger.
///
/// A narrower type than [`EntryState`] on purpose: only these three may ever be
/// written to the ledger, because the ledger's single job is to answer "has this
/// game already reached the server".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalState {
    Accepted,
    Held,
    Rejected,
}

impl TerminalState {
    fn to_entry_state(self, reason: Option<String>) -> EntryState {
        let reason = reason.unwrap_or_else(|| "recorded in the ledger".to_string());
        match self {
            TerminalState::Accepted => EntryState::Accepted,
            TerminalState::Held => EntryState::Held { reason },
            TerminalState::Rejected => EntryState::Rejected { reason },
        }
    }
}

/// Enough of the capture to show a row without reading the payload back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaptureSummary {
    pub captured_at: String,
    pub teams: Vec<Team>,
    pub players: usize,
    /// Already resolved: a capture with no `sides_verified` key is false here.
    pub sides_verified: bool,
    /// Flag codes from validation at enqueue time. Codes and not messages,
    /// because the wording is regenerated from the payload while it exists and
    /// the codes are what have to outlive it.
    pub flags: Vec<String>,
    pub needs_review: bool,
}

/// One capture in the queue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueueEntry {
    pub key: CaptureKey,
    pub state: EntryState,
    /// Failed sending attempts so far. Never reset by a failure, only by
    /// [`StatsQueue::retry_stalled`].
    pub attempts: u32,
    pub first_seen_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_attempt_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    /// Where Dolphin wrote it. Absent when the bytes were handed over directly.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_path: Option<PathBuf>,
    /// The launcher session that accounts for this capture. Absent means nothing
    /// does, and the entry is quarantined.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Absent when the capture could not be parsed. Not filled in with
    /// placeholder team names: a row that cannot be described is shown as one
    /// that cannot be described.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<CaptureSummary>,
}

/// What happened to one capture handed to the queue.
#[derive(Debug, Clone)]
pub enum Enqueued {
    /// Now on disk and waiting to be sent.
    Queued(QueueEntry),
    /// On disk and deliberately not going to be sent.
    Quarantined(QueueEntry),
    /// This exact game was already in the queue. Nothing was written.
    AlreadyQueued(QueueEntry),
    /// This exact game already reached the server. Nothing was written, and this
    /// is the refusal that makes a double submission impossible even after the
    /// payload has been pruned.
    AlreadySubmitted {
        key: CaptureKey,
        state: TerminalState,
    },
    /// The file could not be read off disk at all, so nothing was queued and the
    /// file was left where it was.
    CouldNotRead { path: PathBuf, detail: String },
}

impl Enqueued {
    pub fn key(&self) -> Option<&CaptureKey> {
        match self {
            Enqueued::Queued(e) | Enqueued::Quarantined(e) | Enqueued::AlreadyQueued(e) => {
                Some(&e.key)
            }
            Enqueued::AlreadySubmitted { key, .. } => Some(key),
            Enqueued::CouldNotRead { .. } => None,
        }
    }

    /// A sentence a player can act on.
    pub fn message(&self) -> String {
        match self {
            Enqueued::Queued(_) => {
                "Saved this result. It will be sent when there is a connection.".to_string()
            }
            Enqueued::Quarantined(e) => match &e.state {
                EntryState::Quarantined { reason } => reason.message(),
                _ => "This result is being kept back.".to_string(),
            },
            Enqueued::AlreadyQueued(_) => {
                "This result is already saved and waiting to be sent.".to_string()
            }
            Enqueued::AlreadySubmitted { .. } => {
                "This result has already been sent, so it was not saved again.".to_string()
            }
            Enqueued::CouldNotRead { path, detail } => format!(
                "Could not read {}: {detail} The file was left alone.",
                path.display()
            ),
        }
    }
}

/// What the server said about one capture.
///
/// There is no `Err` variant, and that is the point. An unclassified failure is
/// what made a duplicate-key exception look like a network problem and sent the
/// uploader into a retry loop. Whoever implements [`Submitter`] has to decide
/// which of these four a failure is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubmitOutcome {
    /// Stored and counted. Never send again.
    Accepted,
    /// Stored, and a person will decide. **A success.** Never send again.
    Held { reason: String },
    /// Refused for good, for example a schema the server will not take. Never
    /// send again, and do not delete it either.
    Rejected { reason: String },
    /// Could not be delivered this time. Try later.
    Deferred { reason: String },
}

/// Sending a capture somewhere. Implemented by the caller.
///
/// No network code, no URL and no credential lives in this crate, so this trait
/// is the whole seam. The bytes handed over are the bytes as captured, never a
/// re-serialization, because the server hashes what it is given.
pub trait Submitter {
    fn submit(&self, entry: &QueueEntry, capture: &[u8]) -> SubmitOutcome;
}

/// The result of one submission pass.
#[derive(Debug, Clone, Default)]
pub struct SubmitReport {
    pub accepted: Vec<CaptureKey>,
    pub held: Vec<CaptureKey>,
    pub rejected: Vec<CaptureKey>,
    /// Tried and deferred. Still queued.
    pub deferred: Vec<CaptureKey>,
    /// Out of attempts. Still on disk.
    pub stalled: Vec<CaptureKey>,
    /// Entries whose record said queued while the ledger said sent. Repaired,
    /// and not sent again. A non-empty list here means the launcher died between
    /// the ledger append and the record update, which is the window the ordering
    /// exists to survive.
    pub repaired_from_ledger: Vec<CaptureKey>,
    /// Queued entries whose payload has gone missing. Quarantined, not silently
    /// dropped.
    pub lost_payload: Vec<CaptureKey>,
}

impl SubmitReport {
    /// How many captures reached the server in this pass.
    pub fn sent(&self) -> usize {
        self.accepted.len() + self.held.len() + self.rejected.len()
    }

    /// How many are still waiting.
    pub fn still_waiting(&self) -> usize {
        self.deferred.len() + self.stalled.len()
    }
}

const ENTRY_SUFFIX: &str = ".entry.json";
const PAYLOAD_SUFFIX: &str = ".capture.json";
const LEDGER_FILE: &str = "ledger.jsonl";

#[derive(Debug, Serialize, Deserialize)]
struct LedgerRow {
    key: CaptureKey,
    state: TerminalState,
    at_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
}

/// The durable queue of captured results.
///
/// Lives under [`DataDir::StatsQueue`], which is classed user data and is never
/// purged with the cache. A result that has not been sent is not regenerable:
/// the game is over.
#[derive(Debug)]
pub struct StatsQueue {
    dir: PathBuf,
    clock: Box<dyn Clock>,
}

impl StatsQueue {
    /// Open the queue in its real place.
    pub fn open(platform: &Platform) -> Result<Self> {
        let dir = platform.data_dir(DataDir::StatsQueue)?;
        Ok(Self {
            dir,
            clock: Box::new(SystemClock),
        })
    }

    /// Open a queue rooted anywhere, with an injected clock.
    ///
    /// Exists so tests exercise real files in a temporary directory and can
    /// assert on retention windows without waiting for them.
    pub fn open_at<C: Clock + 'static>(dir: &Path, clock: C) -> Result<Self> {
        std::fs::create_dir_all(dir).map_err(|source| Error::Io {
            path: dir.to_path_buf(),
            source,
        })?;
        Ok(Self {
            dir: dir.to_path_buf(),
            clock: Box::new(clock),
        })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn entry_path(&self, key: &CaptureKey) -> PathBuf {
        self.dir.join(format!("{key}{ENTRY_SUFFIX}"))
    }

    fn payload_path(&self, key: &CaptureKey) -> PathBuf {
        self.dir.join(format!("{key}{PAYLOAD_SUFFIX}"))
    }

    fn ledger_path(&self) -> PathBuf {
        self.dir.join(LEDGER_FILE)
    }

    /// Take a capture into the queue.
    ///
    /// `session_id` is the launcher session that accounts for this result.
    /// `None` quarantines it: see [`QuarantineReason::NoSession`].
    ///
    /// The payload is written before the record, so an interruption leaves bytes
    /// without a record rather than a record without bytes.
    pub fn enqueue(
        &self,
        bytes: &[u8],
        source_path: Option<&Path>,
        session_id: Option<&str>,
    ) -> Result<Enqueued> {
        let parsed = Capture::parse(bytes);
        let key = match &parsed {
            Ok(capture) => capture.content_key(),
            Err(_) => CaptureKey::of_bytes(bytes),
        };

        if let Some((state, _)) = self.terminal_states()?.get(&key) {
            return Ok(Enqueued::AlreadySubmitted { key, state: *state });
        }
        if let Some(existing) = self.read_entry(&key)? {
            return Ok(Enqueued::AlreadyQueued(existing));
        }

        let payload = self.payload_path(&key);
        std::fs::write(&payload, bytes).map_err(|source| Error::Io {
            path: payload,
            source,
        })?;

        let (summary, state) = match &parsed {
            Err(e) => (
                None,
                EntryState::Quarantined {
                    reason: QuarantineReason::Unreadable {
                        detail: e.to_string(),
                    },
                },
            ),
            Ok(capture) => {
                let validation = capture.validate(Some(bytes.len()));
                let summary = CaptureSummary {
                    captured_at: capture.captured_at.clone(),
                    teams: capture.teams.clone(),
                    players: capture.players.len(),
                    sides_verified: capture.sides_are_verified(),
                    flags: validation.codes().iter().map(|c| c.to_string()).collect(),
                    needs_review: validation.needs_review(),
                };
                let state = if !validation.is_well_formed() {
                    EntryState::Quarantined {
                        reason: QuarantineReason::FailedValidation {
                            flags: validation
                                .blocking()
                                .iter()
                                .map(|f| f.code.to_string())
                                .collect(),
                        },
                    }
                } else if session_id.is_none() {
                    EntryState::Quarantined {
                        reason: QuarantineReason::NoSession,
                    }
                } else {
                    EntryState::Queued
                };
                (Some(summary), state)
            }
        };

        let entry = QueueEntry {
            key,
            state,
            attempts: 0,
            first_seen_ms: self.clock.now_ms(),
            last_attempt_ms: None,
            finished_ms: None,
            last_error: None,
            source_path: source_path.map(|p| p.to_path_buf()),
            session_id: session_id.map(|s| s.to_string()),
            summary,
        };
        self.write_entry(&entry)?;

        if matches!(entry.state, EntryState::Quarantined { .. }) {
            Ok(Enqueued::Quarantined(entry))
        } else {
            Ok(Enqueued::Queued(entry))
        }
    }

    /// Move everything Dolphin wrote into the queue.
    ///
    /// Only `*.json` directly in `dir` is touched, so `uploaded.txt` and the
    /// `_previous` folder the manual test procedure uses are left alone.
    ///
    /// A source file is removed only after its capture is safely in the queue,
    /// so an interruption re-reads it and the content key makes that a no-op.
    /// Running this twice on the same folder is therefore safe, which matters
    /// because it runs after every game.
    pub fn ingest_from(&self, dir: &Path, session_id: Option<&str>) -> Result<Vec<Enqueued>> {
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let listing = std::fs::read_dir(dir).map_err(|source| Error::Io {
            path: dir.to_path_buf(),
            source,
        })?;
        let mut paths: Vec<PathBuf> = Vec::new();
        for item in listing {
            let item = item.map_err(|source| Error::Io {
                path: dir.to_path_buf(),
                source,
            })?;
            let path = item.path();
            if !path.is_file() {
                continue;
            }
            let is_json = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.eq_ignore_ascii_case("json"))
                .unwrap_or(false);
            if !is_json {
                continue;
            }
            paths.push(path);
        }
        paths.sort();

        let mut out = Vec::new();
        for path in paths {
            let bytes = match std::fs::read(&path) {
                Ok(b) => b,
                Err(e) => {
                    out.push(Enqueued::CouldNotRead {
                        path: path.clone(),
                        detail: e.to_string(),
                    });
                    continue;
                }
            };
            let result = self.enqueue(&bytes, Some(&path), session_id)?;
            // Every one of these means the capture is accounted for, so the
            // source can go. Leaving it would mean the folder never empties and
            // the same games are re-examined after every launch.
            if !matches!(result, Enqueued::CouldNotRead { .. }) {
                std::fs::remove_file(&path).map_err(|source| Error::Io {
                    path: path.clone(),
                    source,
                })?;
            }
            out.push(result);
        }
        Ok(out)
    }

    /// Every entry, with the ledger applied over it.
    ///
    /// The ledger wins. It is written before the entry, so an entry that says
    /// queued while the ledger says sent is the crash window and not the truth.
    /// Applying it here means no caller can see, or act on, a state that would
    /// send a game twice.
    ///
    /// Unreadable records are skipped rather than failing the whole listing. A
    /// half written file from a power cut should not make the results screen
    /// refuse to open.
    pub fn entries(&self) -> Result<Vec<QueueEntry>> {
        let ledger = self.terminal_states()?;
        let mut entries = self.read_all_raw()?;
        for entry in &mut entries {
            if let Some((state, reason)) = ledger.get(&entry.key) {
                if !entry.state.is_terminal() {
                    entry.state = state.to_entry_state(reason.clone());
                }
            }
        }
        entries.sort_by(|a, b| {
            (a.first_seen_ms, a.key.as_str()).cmp(&(b.first_seen_ms, b.key.as_str()))
        });
        Ok(entries)
    }

    /// Entries the next submission pass will try, oldest first.
    pub fn pending(&self) -> Result<Vec<QueueEntry>> {
        Ok(self
            .entries()?
            .into_iter()
            .filter(|e| e.state.is_pending())
            .collect())
    }

    /// Entries held back and never sent, so a caller can surface them.
    pub fn quarantined(&self) -> Result<Vec<QueueEntry>> {
        Ok(self
            .entries()?
            .into_iter()
            .filter(|e| matches!(e.state, EntryState::Quarantined { .. }))
            .collect())
    }

    /// Entries that ran out of attempts and are waiting for a person or a
    /// working connection.
    pub fn stalled(&self) -> Result<Vec<QueueEntry>> {
        Ok(self
            .entries()?
            .into_iter()
            .filter(|e| matches!(e.state, EntryState::Stalled { .. }))
            .collect())
    }

    /// The capture bytes exactly as captured, or `None` once pruned.
    pub fn capture_bytes(&self, key: &CaptureKey) -> Result<Option<Vec<u8>>> {
        let path = self.payload_path(key);
        match std::fs::read(&path) {
            Ok(b) => Ok(Some(b)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(source) => Err(Error::Io { path, source }),
        }
    }

    /// Whether this game already reached the server.
    pub fn terminal_state(&self, key: &CaptureKey) -> Result<Option<TerminalState>> {
        Ok(self.terminal_states()?.get(key).map(|(s, _)| *s))
    }

    /// Payload files with no record beside them.
    ///
    /// The window between writing the payload and writing the record. Surfaced
    /// rather than left invisible, because the bytes are a real game.
    pub fn orphaned_payloads(&self) -> Result<Vec<CaptureKey>> {
        let recorded: BTreeSet<CaptureKey> =
            self.read_all_raw()?.into_iter().map(|e| e.key).collect();
        let mut out = Vec::new();
        for key in self.keys_with_suffix(PAYLOAD_SUFFIX)? {
            if !recorded.contains(&key) {
                out.push(key);
            }
        }
        out.sort();
        Ok(out)
    }

    /// Try to send everything queued.
    ///
    /// `max_attempts` is how many failed attempts an entry tolerates before it
    /// is stalled. Nothing is deleted at any point in here.
    pub fn submit_pending(
        &self,
        submitter: &dyn Submitter,
        max_attempts: u32,
    ) -> Result<SubmitReport> {
        let ledger = self.terminal_states()?;
        let mut raw = self.read_all_raw()?;
        raw.sort_by(|a, b| {
            (a.first_seen_ms, a.key.as_str()).cmp(&(b.first_seen_ms, b.key.as_str()))
        });

        let mut report = SubmitReport::default();
        for mut entry in raw {
            if let Some((state, reason)) = ledger.get(&entry.key) {
                if !entry.state.is_terminal() {
                    entry.state = state.to_entry_state(reason.clone());
                    entry.finished_ms = entry.finished_ms.or(Some(self.clock.now_ms()));
                    self.write_entry(&entry)?;
                    report.repaired_from_ledger.push(entry.key.clone());
                }
                continue;
            }
            if !entry.state.is_pending() {
                continue;
            }

            let Some(bytes) = self.capture_bytes(&entry.key)? else {
                entry.state = EntryState::Quarantined {
                    reason: QuarantineReason::Unreadable {
                        detail: "the capture payload is no longer in the queue folder, so \
                                 there is nothing left to send."
                            .to_string(),
                    },
                };
                self.write_entry(&entry)?;
                report.lost_payload.push(entry.key.clone());
                continue;
            };

            let outcome = submitter.submit(&entry, &bytes);
            let now = self.clock.now_ms();
            entry.last_attempt_ms = Some(now);

            match outcome {
                SubmitOutcome::Accepted => {
                    self.record_terminal(&entry.key, TerminalState::Accepted, None)?;
                    entry.state = EntryState::Accepted;
                    entry.finished_ms = Some(now);
                    entry.last_error = None;
                    self.write_entry(&entry)?;
                    report.accepted.push(entry.key.clone());
                }
                SubmitOutcome::Held { reason } => {
                    self.record_terminal(&entry.key, TerminalState::Held, Some(&reason))?;
                    entry.state = EntryState::Held {
                        reason: reason.clone(),
                    };
                    entry.finished_ms = Some(now);
                    entry.last_error = None;
                    self.write_entry(&entry)?;
                    report.held.push(entry.key.clone());
                }
                SubmitOutcome::Rejected { reason } => {
                    self.record_terminal(&entry.key, TerminalState::Rejected, Some(&reason))?;
                    entry.state = EntryState::Rejected {
                        reason: reason.clone(),
                    };
                    entry.finished_ms = Some(now);
                    entry.last_error = Some(reason);
                    self.write_entry(&entry)?;
                    report.rejected.push(entry.key.clone());
                }
                SubmitOutcome::Deferred { reason } => {
                    entry.attempts = entry.attempts.saturating_add(1);
                    entry.last_error = Some(reason.clone());
                    if entry.attempts >= max_attempts {
                        entry.state = EntryState::Stalled { reason };
                        self.write_entry(&entry)?;
                        report.stalled.push(entry.key.clone());
                    } else {
                        self.write_entry(&entry)?;
                        report.deferred.push(entry.key.clone());
                    }
                }
            }
        }
        Ok(report)
    }

    /// Put stalled entries back in the queue.
    ///
    /// Nothing expires a stalled capture on its own. The game happened, so the
    /// only two ways out are sending it or the player deleting it.
    pub fn retry_stalled(&self) -> Result<Vec<CaptureKey>> {
        let mut moved = Vec::new();
        for mut entry in self.entries()? {
            if matches!(entry.state, EntryState::Stalled { .. }) {
                entry.state = EntryState::Queued;
                entry.attempts = 0;
                self.write_entry(&entry)?;
                moved.push(entry.key);
            }
        }
        Ok(moved)
    }

    /// Delete payloads for results that have reached the server.
    ///
    /// `keep_for_ms` is how long a finished result is kept so the player can
    /// still look at the box score. Only entries in a terminal state are
    /// touched. Queued, stalled and quarantined captures are never deleted by
    /// this, at any age, because they are the ones nobody else has a copy of.
    ///
    /// The ledger row is not removed, so a capture the player still has cannot
    /// be re-queued and sent a second time once its payload is gone.
    pub fn prune_terminal(&self, keep_for_ms: u64) -> Result<Vec<CaptureKey>> {
        let now = self.clock.now_ms();
        let mut removed = Vec::new();
        for entry in self.entries()? {
            if !entry.state.is_terminal() {
                continue;
            }
            let since = entry.finished_ms.unwrap_or(entry.first_seen_ms);
            if now.saturating_sub(since) < keep_for_ms {
                continue;
            }
            let payload = self.payload_path(&entry.key);
            if payload.exists() {
                std::fs::remove_file(&payload).map_err(|source| Error::Io {
                    path: payload,
                    source,
                })?;
            }
            let record = self.entry_path(&entry.key);
            if record.exists() {
                std::fs::remove_file(&record).map_err(|source| Error::Io {
                    path: record,
                    source,
                })?;
            }
            removed.push(entry.key);
        }
        Ok(removed)
    }

    fn read_entry(&self, key: &CaptureKey) -> Result<Option<QueueEntry>> {
        let path = self.entry_path(key);
        match std::fs::read(&path) {
            Ok(bytes) => Ok(serde_json::from_slice::<QueueEntry>(&bytes).ok()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(source) => Err(Error::Io { path, source }),
        }
    }

    fn write_entry(&self, entry: &QueueEntry) -> Result<()> {
        let path = self.entry_path(&entry.key);
        let json = serde_json::to_vec_pretty(entry).map_err(|e| {
            Error::PlainIo(std::io::Error::other(format!(
                "could not encode queue entry: {e}"
            )))
        })?;
        std::fs::write(&path, json).map_err(|source| Error::Io { path, source })
    }

    /// Filenames in the queue folder that carry a valid key and the suffix.
    fn keys_with_suffix(&self, suffix: &str) -> Result<Vec<CaptureKey>> {
        let listing = match std::fs::read_dir(&self.dir) {
            Ok(l) => l,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(source) => {
                return Err(Error::Io {
                    path: self.dir.clone(),
                    source,
                })
            }
        };
        let mut out = Vec::new();
        for item in listing {
            let item = item.map_err(|source| Error::Io {
                path: self.dir.clone(),
                source,
            })?;
            let name = item.file_name();
            let Some(name) = name.to_str() else { continue };
            let Some(stem) = name.strip_suffix(suffix) else {
                continue;
            };
            // A hand-dropped or renamed file is ignored rather than guessed at.
            let Ok(hex) = Sha256Hex::parse(stem) else {
                continue;
            };
            out.push(CaptureKey(hex));
        }
        out.sort();
        Ok(out)
    }

    fn read_all_raw(&self) -> Result<Vec<QueueEntry>> {
        let mut out = Vec::new();
        for key in self.keys_with_suffix(ENTRY_SUFFIX)? {
            let Some(entry) = self.read_entry(&key)? else {
                continue;
            };
            // A record whose filename and contents disagree has been moved or
            // copied. Acting on it would mean sending one game under another
            // game's identity, so it is ignored and the files are left alone.
            if entry.key != key {
                continue;
            }
            out.push(entry);
        }
        Ok(out)
    }

    fn terminal_states(&self) -> Result<BTreeMap<CaptureKey, (TerminalState, Option<String>)>> {
        let path = self.ledger_path();
        let raw = match std::fs::read_to_string(&path) {
            Ok(r) => r,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
            Err(source) => return Err(Error::Io { path, source }),
        };
        let mut out = BTreeMap::new();
        for line in raw.lines().filter(|l| !l.trim().is_empty()) {
            // A half written final line is skipped, not fatal. The rest of the
            // ledger is still the only record of what has been sent.
            if let Ok(row) = serde_json::from_str::<LedgerRow>(line) {
                out.insert(row.key, (row.state, row.reason));
            }
        }
        Ok(out)
    }

    /// Append to the ledger, and get it onto the disk before returning.
    ///
    /// Called before the entry is updated. If the process dies after this line
    /// the game is recorded as sent and will not be sent again, which is the
    /// safe direction: the other order resends it.
    fn record_terminal(
        &self,
        key: &CaptureKey,
        state: TerminalState,
        reason: Option<&str>,
    ) -> Result<()> {
        use std::io::Write;
        let path = self.ledger_path();
        let row = LedgerRow {
            key: key.clone(),
            state,
            at_ms: self.clock.now_ms(),
            reason: reason.map(|r| r.to_string()),
        };
        let line = serde_json::to_string(&row).map_err(|e| {
            Error::PlainIo(std::io::Error::other(format!(
                "could not encode ledger row: {e}"
            )))
        })?;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|source| Error::Io {
                path: path.clone(),
                source,
            })?;
        writeln!(file, "{line}").map_err(|source| Error::Io {
            path: path.clone(),
            source,
        })?;
        // The ledger is the only thing standing between a crash and a duplicate
        // result on a real fixture. Worth the flush.
        file.sync_all().map_err(|source| Error::Io { path, source })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::supervisor::FixedClock;

    /// A real capture, verbatim, from
    /// `User\LeagueGameStats\Falcons_vs_Chargers_001.json`, dated August 5
    /// 2026. Kept byte for byte because it is the only sample of the failing
    /// state there is: `sides_verified` is false, there is no `team_totals`
    /// block, side 0 is attributed to the Chargers while `teams[0]` is the
    /// Falcons, and it contains the quarterback credited with sacks.
    ///
    /// Every plausibility rule in this module was checked against this file and
    /// the other five in that folder. Replacing it with a handwritten fixture
    /// would remove the only evidence the rules are calibrated to real data.
    const REAL_FALCONS: &str = r#"{
  "schema": "nsr.game.v1",
  "source": "dolphin",
  "captured_at": "2026-08-05T04:58:39Z",
  "teams": [{"name": "Falcons", "score": 36}, {"name": "Chargers", "score": 24}],
  "style": [0, 0],
  "sides_verified": false,
  "per_player_matched": 14,
  "players": [
    {"name": "Tuli Tuipulotu", "team": "Chargers", "side": 0, "slot": 0, "digp": 21, "stats": {"pass_td": 0, "rush_td": 0, "rec_td": 0, "pick_six": 0, "fum_td": 0, "interceptions": 0, "forced_fumbles": 1, "fumble_rec": 0, "tackles": 4, "sacks": 2, "thrown_int": 0, "fumbles": 0}},
    {"name": "Khalil Mack", "team": "Chargers", "side": 0, "slot": 1, "digp": 19, "stats": {"pass_td": 0, "rush_td": 0, "rec_td": 0, "pick_six": 0, "fum_td": 0, "interceptions": 0, "forced_fumbles": 0, "fumble_rec": 0, "tackles": 0, "sacks": 0, "thrown_int": 0, "fumbles": 0}},
    {"name": "Justin Herbert", "team": "Chargers", "side": 0, "slot": 2, "digp": 15, "stats": {"pass_td": 2, "rush_td": 0, "rec_td": 0, "pick_six": 0, "fum_td": 0, "interceptions": 0, "forced_fumbles": 1, "fumble_rec": 0, "tackles": 2, "sacks": 0, "thrown_int": 0, "fumbles": 2}},
    {"name": "Omarion Hampton", "team": "Chargers", "side": 0, "slot": 3, "digp": 12, "stats": {"pass_td": 0, "rush_td": 1, "rec_td": 1, "pick_six": 0, "fum_td": 0, "interceptions": 0, "forced_fumbles": 0, "fumble_rec": 0, "tackles": 1, "sacks": 0, "thrown_int": 0, "fumbles": 1}},
    {"name": "Derwin James Jr.", "team": "Chargers", "side": 0, "slot": 4, "digp": 17, "stats": {"pass_td": 0, "rush_td": 0, "rec_td": 0, "pick_six": 1, "fum_td": 0, "interceptions": 4, "forced_fumbles": 0, "fumble_rec": 0, "tackles": 4, "sacks": 0, "thrown_int": 0, "fumbles": 0}},
    {"name": "Quentin Johnston", "team": "Chargers", "side": 0, "slot": 5, "digp": 18, "stats": {"pass_td": 0, "rush_td": 0, "rec_td": 1, "pick_six": 0, "fum_td": 0, "interceptions": 1, "forced_fumbles": 0, "fumble_rec": 0, "tackles": 4, "sacks": 0, "thrown_int": 0, "fumbles": 1}},
    {"name": "Donte Jackson", "team": "Chargers", "side": 0, "slot": 6, "digp": 16, "stats": {"pass_td": 0, "rush_td": 0, "rec_td": 0, "pick_six": 0, "fum_td": 0, "interceptions": 0, "forced_fumbles": 0, "fumble_rec": 0, "tackles": 2, "sacks": 0, "thrown_int": 0, "fumbles": 0}},
    {"name": "Michael Penix Jr.", "team": "Falcons", "side": 1, "slot": 0, "digp": 45, "stats": {"pass_td": 2, "rush_td": 0, "rec_td": 1, "pick_six": 0, "fum_td": 0, "interceptions": 0, "forced_fumbles": 1, "fumble_rec": 0, "tackles": 2, "sacks": 2, "thrown_int": 5, "fumbles": 0}},
    {"name": "Bijan Robinson", "team": "Falcons", "side": 1, "slot": 1, "digp": 47, "stats": {"pass_td": 0, "rush_td": 2, "rec_td": 1, "pick_six": 0, "fum_td": 0, "interceptions": 0, "forced_fumbles": 0, "fumble_rec": 0, "tackles": 2, "sacks": 1, "thrown_int": 0, "fumbles": 1}},
    {"name": "Drake London", "team": "Falcons", "side": 1, "slot": 2, "digp": 40, "stats": {"pass_td": 1, "rush_td": 0, "rec_td": 1, "pick_six": 0, "fum_td": 0, "interceptions": 0, "forced_fumbles": 0, "fumble_rec": 0, "tackles": 0, "sacks": 0, "thrown_int": 0, "fumbles": 1}},
    {"name": "Jessie Bates III", "team": "Falcons", "side": 1, "slot": 3, "digp": 37, "stats": {"pass_td": 0, "rush_td": 0, "rec_td": 0, "pick_six": 0, "fum_td": 0, "interceptions": 0, "forced_fumbles": 1, "fumble_rec": 1, "tackles": 2, "sacks": 0, "thrown_int": 0, "fumbles": 1}},
    {"name": "James Pearce Jr.", "team": "Falcons", "side": 1, "slot": 4, "digp": 44, "stats": {"pass_td": 0, "rush_td": 0, "rec_td": 0, "pick_six": 0, "fum_td": 0, "interceptions": 0, "forced_fumbles": 0, "fumble_rec": 0, "tackles": 1, "sacks": 1, "thrown_int": 0, "fumbles": 0}},
    {"name": "A.J. Terrell Jr.", "team": "Falcons", "side": 1, "slot": 5, "digp": 48, "stats": {"pass_td": 0, "rush_td": 0, "rec_td": 0, "pick_six": 0, "fum_td": 0, "interceptions": 0, "forced_fumbles": 1, "fumble_rec": 0, "tackles": 1, "sacks": 0, "thrown_int": 0, "fumbles": 0}},
    {"name": "Jalon Walker", "team": "Falcons", "side": 1, "slot": 6, "digp": 49, "stats": {"pass_td": 0, "rush_td": 0, "rec_td": 0, "pick_six": 0, "fum_td": 0, "interceptions": 0, "forced_fumbles": 0, "fumble_rec": 0, "tackles": 4, "sacks": 1, "thrown_int": 0, "fumbles": 0}}
  ]
}"#;

    /// A real capture, verbatim, from
    /// `User\LeagueGameStats\Outlaws_vs_Shadow_Knights_001.json`. The passing
    /// case: `sides_verified` is true and the `team_totals` block is present, so
    /// it exercises the paths the Falcons capture cannot reach.
    const REAL_OUTLAWS: &str = r#"{
  "schema": "nsr.game.v1",
  "source": "dolphin",
  "captured_at": "2026-07-28T00:41:31Z",
  "teams": [{"name": "Outlaws", "score": 6}, {"name": "Shadow Knights", "score": 0}],
  "style": [4000, 56824],
  "team_totals": [
    {"score": 6, "style": 56824, "total_td": 1, "rush_td": 0, "pass_td": 1, "def_td": 0, "sacks": 0, "safeties": 0, "int": 0, "ff": 0, "first_downs": 1, "gamebreakers": 0, "gb_2pt": 0, "hot_spots": 0, "credits": 0},
    {"score": 0, "style": 4000, "total_td": 0, "rush_td": 0, "pass_td": 0, "def_td": 0, "sacks": 0, "safeties": 0, "int": 0, "ff": 0, "first_downs": 0, "gamebreakers": 0, "gb_2pt": 0, "hot_spots": 0, "credits": 0}
  ],
  "sides_verified": true,
  "per_player_matched": 14,
  "players": [
    {"name": "Chubby JoeSchmoe", "team": "Shadow Knights", "side": 0, "slot": 0, "digp": 18, "stats": {"pass_td": 0, "rush_td": 0, "rec_td": 0, "pick_six": 0, "fum_td": 0, "interceptions": 0, "forced_fumbles": 0, "fumble_rec": 0, "tackles": 1, "sacks": 0, "thrown_int": 0, "fumbles": 0}},
    {"name": "Crosby Maxximilian", "team": "Shadow Knights", "side": 0, "slot": 1, "digp": 17, "stats": {"pass_td": 0, "rush_td": 0, "rec_td": 0, "pick_six": 0, "fum_td": 0, "interceptions": 0, "forced_fumbles": 0, "fumble_rec": 0, "tackles": 0, "sacks": 0, "thrown_int": 0, "fumbles": 0}},
    {"name": "Kel Quadir", "team": "Shadow Knights", "side": 0, "slot": 2, "digp": 12, "stats": {"pass_td": 0, "rush_td": 0, "rec_td": 0, "pick_six": 0, "fum_td": 0, "interceptions": 0, "forced_fumbles": 0, "fumble_rec": 0, "tackles": 0, "sacks": 0, "thrown_int": 0, "fumbles": 0}},
    {"name": "Da Middleman ", "team": "Shadow Knights", "side": 0, "slot": 3, "digp": 16, "stats": {"pass_td": 0, "rush_td": 0, "rec_td": 0, "pick_six": 0, "fum_td": 0, "interceptions": 0, "forced_fumbles": 0, "fumble_rec": 0, "tackles": 0, "sacks": 0, "thrown_int": 0, "fumbles": 0}},
    {"name": "Headless Chicken", "team": "Shadow Knights", "side": 0, "slot": 4, "digp": 14, "stats": {"pass_td": 0, "rush_td": 0, "rec_td": 0, "pick_six": 0, "fum_td": 0, "interceptions": 0, "forced_fumbles": 0, "fumble_rec": 0, "tackles": 0, "sacks": 0, "thrown_int": 0, "fumbles": 0}},
    {"name": "Tariq Bullet", "team": "Shadow Knights", "side": 0, "slot": 5, "digp": 15, "stats": {"pass_td": 0, "rush_td": 0, "rec_td": 0, "pick_six": 0, "fum_td": 0, "interceptions": 0, "forced_fumbles": 0, "fumble_rec": 0, "tackles": 1, "sacks": 0, "thrown_int": 0, "fumbles": 0}},
    {"name": "Bijan Luh-B", "team": "Shadow Knights", "side": 0, "slot": 6, "digp": 13, "stats": {"pass_td": 0, "rush_td": 0, "rec_td": 0, "pick_six": 0, "fum_td": 0, "interceptions": 0, "forced_fumbles": 0, "fumble_rec": 0, "tackles": 0, "sacks": 0, "thrown_int": 0, "fumbles": 0}},
    {"name": "Barry McKackiner", "team": "Outlaws", "side": 1, "slot": 0, "digp": 32, "stats": {"pass_td": 1, "rush_td": 0, "rec_td": 0, "pick_six": 0, "fum_td": 0, "interceptions": 0, "forced_fumbles": 0, "fumble_rec": 0, "tackles": 0, "sacks": 0, "thrown_int": 0, "fumbles": 0}},
    {"name": "Dixon Myaz", "team": "Outlaws", "side": 1, "slot": 1, "digp": 33, "stats": {"pass_td": 0, "rush_td": 0, "rec_td": 1, "pick_six": 0, "fum_td": 0, "interceptions": 0, "forced_fumbles": 0, "fumble_rec": 0, "tackles": 0, "sacks": 0, "thrown_int": 0, "fumbles": 0}},
    {"name": "Mike Oxlong", "team": "Outlaws", "side": 1, "slot": 2, "digp": 34, "stats": {"pass_td": 0, "rush_td": 0, "rec_td": 0, "pick_six": 0, "fum_td": 0, "interceptions": 0, "forced_fumbles": 0, "fumble_rec": 0, "tackles": 0, "sacks": 0, "thrown_int": 0, "fumbles": 0}},
    {"name": "Jack Offman", "team": "Outlaws", "side": 1, "slot": 3, "digp": 35, "stats": {"pass_td": 0, "rush_td": 0, "rec_td": 0, "pick_six": 0, "fum_td": 0, "interceptions": 0, "forced_fumbles": 0, "fumble_rec": 0, "tackles": 0, "sacks": 0, "thrown_int": 0, "fumbles": 0}},
    {"name": "Gabe  Itch", "team": "Outlaws", "side": 1, "slot": 4, "digp": 36, "stats": {"pass_td": 0, "rush_td": 0, "rec_td": 0, "pick_six": 0, "fum_td": 0, "interceptions": 0, "forced_fumbles": 0, "fumble_rec": 0, "tackles": 0, "sacks": 0, "thrown_int": 0, "fumbles": 0}},
    {"name": "Willie Stroker", "team": "Outlaws", "side": 1, "slot": 5, "digp": 37, "stats": {"pass_td": 0, "rush_td": 0, "rec_td": 0, "pick_six": 0, "fum_td": 0, "interceptions": 0, "forced_fumbles": 0, "fumble_rec": 0, "tackles": 0, "sacks": 0, "thrown_int": 0, "fumbles": 0}},
    {"name": "Craven Moorehead", "team": "Outlaws", "side": 1, "slot": 6, "digp": 38, "stats": {"pass_td": 0, "rush_td": 0, "rec_td": 0, "pick_six": 0, "fum_td": 0, "interceptions": 0, "forced_fumbles": 0, "fumble_rec": 0, "tackles": 0, "sacks": 0, "thrown_int": 0, "fumbles": 0}}
  ]
}"#;

    fn falcons() -> Capture {
        Capture::parse(REAL_FALCONS.as_bytes()).expect("the real capture must parse")
    }

    fn outlaws() -> Capture {
        Capture::parse(REAL_OUTLAWS.as_bytes()).expect("the real capture must parse")
    }

    /// Change exactly one thing in a real capture.
    ///
    /// Asserting the substring is present means a test cannot silently stop
    /// perturbing anything if a fixture is ever re-exported.
    fn doctor(source: &str, from: &str, to: &str) -> String {
        assert!(
            source.contains(from),
            "the fixture has no {from:?} to change"
        );
        source.replacen(from, to, 1)
    }

    // -----------------------------------------------------------------------
    // The twelve fields
    // -----------------------------------------------------------------------

    /// The capture records twelve counters and no yardage. An earlier box score
    /// was built on Total yards, Passing and Rushing, none of which exist, and
    /// the whole screen was invalid. If a yardage field is ever added here, this
    /// fails first.
    #[test]
    fn the_stat_schema_is_exactly_twelve_counters_and_none_is_yardage() {
        let stats = &falcons().players[0].stats;
        let json = serde_json::to_value(stats).expect("stats serialize");
        let keys: BTreeSet<String> = json
            .as_object()
            .expect("stats is an object")
            .keys()
            .cloned()
            .collect();

        let expected: BTreeSet<String> =
            StatField::ALL.iter().map(|f| f.key().to_string()).collect();
        assert_eq!(expected.len(), 12);
        assert_eq!(
            keys, expected,
            "the on-disk stat field set has drifted from the schema"
        );

        for field in StatField::ALL {
            let key = field.key();
            for banned in ["yard", "yds", "passing", "rushing"] {
                assert!(
                    !key.contains(banned),
                    "{key} looks like a yardage or aggregate field, and the capture \
                     has neither"
                );
            }
        }
    }

    /// [`StatTotals`] indexes by the enum discriminant, so a reordered `ALL`
    /// would add one statistic into a different one's slot and every aggregate
    /// check would quietly compare the wrong pair.
    #[test]
    fn stat_field_positions_match_their_place_in_all() {
        for (i, field) in StatField::ALL.iter().enumerate() {
            assert_eq!(field.index(), i, "{} is in the wrong slot", field.key());
        }
        let labels: BTreeSet<&str> = StatField::ALL.iter().map(|f| f.label()).collect();
        assert_eq!(labels.len(), 12, "two statistics share a column heading");
    }

    /// Real numbers off a real capture. If parsing reshuffles or defaults a
    /// counter, these are the values that move.
    #[test]
    fn the_real_capture_parses_with_every_counter_intact() {
        let c = falcons();
        assert_eq!(c.schema, GAME_SCHEMA);
        assert_eq!(c.players.len(), 14);
        assert_eq!(c.teams[0].name, "Falcons");
        assert_eq!(c.teams[0].score, 36);
        assert_eq!(c.teams[1].name, "Chargers");
        assert_eq!(c.teams[1].score, 24);

        let penix = c
            .players
            .iter()
            .find(|p| p.name == "Michael Penix Jr.")
            .expect("the passer is in the capture");
        assert_eq!(penix.stats.pass_td, 2);
        assert_eq!(penix.stats.rec_td, 1);
        assert_eq!(penix.stats.tackles, 2);
        assert_eq!(penix.stats.sacks, 2);
        assert_eq!(penix.stats.thrown_int, 5);
        assert_eq!(penix.stats.forced_fumbles, 1);
        assert_eq!(penix.digp, 45);
        assert_eq!(penix.side, 1);
    }

    /// A capture missing a counter is not a game in which that counter was zero.
    /// Filling it in would publish a measurement nobody took, so the file stays
    /// unparsed and is kept.
    #[test]
    fn a_counter_that_was_not_measured_is_never_defaulted_to_zero() {
        let broken = doctor(REAL_FALCONS, "\"tackles\": 4, \"sacks\": 2", "\"sacks\": 2");
        match Capture::parse(broken.as_bytes()) {
            Err(CaptureError::Incomplete(detail)) => {
                assert!(detail.contains("tackles"), "unhelpful detail: {detail}");
            }
            Err(other) => panic!("wrong error for a missing counter: {other}"),
            Ok(c) => panic!(
                "a missing tackles key was read as {} tackles",
                c.players[0].stats.tackles
            ),
        }
    }

    // -----------------------------------------------------------------------
    // Validation
    // -----------------------------------------------------------------------

    /// One real capture has no `sides_verified` key at all. Reading absence as
    /// agreement would count exactly the results that were never checked.
    #[test]
    fn a_missing_sides_verified_key_reads_as_false_and_never_as_true() {
        let stripped = doctor(REAL_OUTLAWS, "\"sides_verified\": true,", "");
        let c = Capture::parse(stripped.as_bytes()).expect("still valid JSON");
        assert_eq!(c.sides_verified, None, "the key must be absent, not false");
        assert!(
            !c.sides_are_verified(),
            "an absent key must never read as verified"
        );

        let v = c.validate(None);
        assert!(v.has(codes::SIDES_UNVERIFIED));
        let flag = v
            .flags
            .iter()
            .find(|f| f.code == codes::SIDES_UNVERIFIED)
            .expect("the flag");
        assert!(
            flag.message.contains("predates"),
            "the message should say the key is absent rather than false: {}",
            flag.message
        );
    }

    /// The Falcons capture is a well formed file that must still not be counted.
    /// Both halves matter: calling it malformed would throw away a real game,
    /// publishing it would count a result whose sides were never confirmed.
    #[test]
    fn the_falcons_capture_is_well_formed_and_still_held_for_review() {
        let c = falcons();
        let v = c.validate(Some(REAL_FALCONS.len()));
        assert!(
            v.is_well_formed(),
            "a real capture must not be called malformed: {:?}",
            v.codes()
        );
        assert!(v.needs_review(), "this capture must not be published as is");
        for code in [
            codes::SIDES_UNVERIFIED,
            codes::NO_TEAM_TOTALS,
            codes::PASSER_CREDITED_WITH_SACKS,
        ] {
            assert!(v.has(code), "expected {code} in {:?}", v.codes());
        }
        assert_eq!(c.winner(), Some(0), "Falcons 36 beat Chargers 24");
        assert!(
            !v.has(codes::BELOW_TARGET_SCORE),
            "this game reached {TARGET_SCORE}, so the incomplete-game floor must stay \
             silent. A floor that fires on a finished game would hold every real \
             result and the review queue would be worthless: {:?}",
            v.codes()
        );
    }

    /// The finding this validation pass exists for. A quarterback with two sacks
    /// and five thrown interceptions suggests defensive plays are attributed by
    /// roster slot rather than by player, so the record is flagged by name
    /// instead of going out as fact.
    #[test]
    fn a_quarterback_credited_with_sacks_is_flagged_by_name_not_published() {
        let v = falcons().validate(None);
        let hits: Vec<&Flag> = v
            .flags
            .iter()
            .filter(|f| f.code == codes::PASSER_CREDITED_WITH_SACKS)
            .collect();
        assert_eq!(
            hits.len(),
            1,
            "the check has to be narrow enough to name one line out of fourteen, \
             not most of a box score: {:?}",
            hits.iter().map(|f| &f.message).collect::<Vec<_>>()
        );
        assert!(hits[0].message.contains("Michael Penix Jr."));
        assert!(
            hits[0].message.contains("roster slot"),
            "the message must say what the suspicion is: {}",
            hits[0].message
        );
        assert_eq!(
            hits[0].severity,
            Severity::Plausibility,
            "this is a doubt about the numbers, not a malformed file, and it must \
             not stop a real game being kept"
        );
    }

    /// The passing case. Sides verified and a team_totals block present, so
    /// neither of those flags fires, and the game is still held because it
    /// finished 6 to 0 in a race to 36.
    #[test]
    fn a_verified_capture_is_still_held_by_the_incomplete_game_floor() {
        let c = outlaws();
        let v = c.validate(Some(REAL_OUTLAWS.len()));
        assert!(c.sides_are_verified());
        assert!(v.is_well_formed(), "{:?}", v.codes());
        assert!(!v.has(codes::SIDES_UNVERIFIED));
        assert!(!v.has(codes::NO_TEAM_TOTALS));
        assert!(
            v.has(codes::BELOW_TARGET_SCORE),
            "a 6 to 0 game in a race to {TARGET_SCORE} did not finish: {:?}",
            v.codes()
        );
    }

    /// The floor holds a result. It never rewrites one.
    #[test]
    fn the_incomplete_game_floor_never_decides_who_won() {
        let c = outlaws();
        assert!(c.teams[0].score < TARGET_SCORE);
        assert_eq!(
            c.winner(),
            Some(0),
            "Outlaws 6 beat Shadow Knights 0, short score or not"
        );
    }

    /// Verified against all six real captures: every interception thrown by one
    /// side is credited to a player on the other. A mismatch is the signature of
    /// plays landing on the wrong side, which is the bug class this whole pass
    /// exists for.
    #[test]
    fn interceptions_must_cross_over_between_the_two_sides() {
        for real in [REAL_FALCONS, REAL_OUTLAWS] {
            let c = Capture::parse(real.as_bytes()).unwrap();
            assert!(
                !c.validate(None).has(codes::INTERCEPTIONS_DO_NOT_CROSS_OVER),
                "a real capture must not trip this check"
            );
            assert_eq!(
                c.totals_for_side(1).get(StatField::ThrownInt),
                c.totals_for_side(0).get(StatField::Interceptions)
            );
        }

        let broken = doctor(REAL_FALCONS, "\"thrown_int\": 5", "\"thrown_int\": 4");
        let v = Capture::parse(broken.as_bytes()).unwrap().validate(None);
        assert!(
            v.has(codes::INTERCEPTIONS_DO_NOT_CROSS_OVER),
            "one interception moved to the wrong side went unnoticed: {:?}",
            v.codes()
        );
    }

    /// Verified against all six real captures: a touchdown pass is caught by a
    /// teammate, so the two counts agree per side.
    #[test]
    fn a_passing_score_requires_a_receiving_score_on_the_same_side() {
        for real in [REAL_FALCONS, REAL_OUTLAWS] {
            let v = Capture::parse(real.as_bytes()).unwrap().validate(None);
            assert!(!v.has(codes::PASSING_AND_RECEIVING_DISAGREE));
        }

        let broken = doctor(
            REAL_FALCONS,
            "\"rush_td\": 1, \"rec_td\": 1",
            "\"rush_td\": 1, \"rec_td\": 0",
        );
        let v = Capture::parse(broken.as_bytes()).unwrap().validate(None);
        assert!(
            v.has(codes::PASSING_AND_RECEIVING_DISAGREE),
            "a dropped receiving score went unnoticed: {:?}",
            v.codes()
        );
    }

    /// A forced fumble is one of the opponent's fumbles, so it cannot exceed
    /// them. Holds one-directionally in all six real captures.
    #[test]
    fn forced_fumbles_cannot_exceed_the_other_sides_fumbles() {
        assert!(!falcons()
            .validate(None)
            .has(codes::FORCED_FUMBLES_EXCEED_OPPONENT_FUMBLES));

        let broken = doctor(
            REAL_FALCONS,
            "\"forced_fumbles\": 1, \"fumble_rec\": 0, \"tackles\": 2, \"sacks\": 2",
            "\"forced_fumbles\": 9, \"fumble_rec\": 0, \"tackles\": 2, \"sacks\": 2",
        );
        let v = Capture::parse(broken.as_bytes()).unwrap().validate(None);
        assert!(
            v.has(codes::FORCED_FUMBLES_EXCEED_OPPONENT_FUMBLES),
            "{:?}",
            v.codes()
        );
    }

    /// The dedupe hash sorts players by digp. With a duplicate, the two clients
    /// of one online game can sort it two ways and hash it two ways, and the
    /// game is recorded twice. That is a structural fault, not a doubt.
    #[test]
    fn a_duplicate_digp_is_a_shape_fault_because_the_dedupe_sort_becomes_unknowable() {
        let broken = doctor(REAL_FALCONS, "\"digp\": 19", "\"digp\": 21");
        let v = Capture::parse(broken.as_bytes()).unwrap().validate(None);
        assert!(v.has(codes::DUPLICATE_DIGP), "{:?}", v.codes());
        assert!(
            !v.is_well_formed(),
            "a capture whose key is not reproducible must not be sent"
        );
    }

    /// A negative counter is impossible, and it must be reported rather than
    /// making the file unparseable. Parsing into an unsigned field would lose
    /// the only copy of the game.
    #[test]
    fn a_negative_counter_is_flagged_rather_than_making_the_file_unreadable() {
        let broken = doctor(
            REAL_FALCONS,
            "\"tackles\": 4, \"sacks\": 2",
            "\"tackles\": -4, \"sacks\": 2",
        );
        let c = Capture::parse(broken.as_bytes()).expect("a negative count must still parse");
        let v = c.validate(None);
        assert!(v.has(codes::NEGATIVE_COUNTER), "{:?}", v.codes());
        assert!(!v.is_well_formed());
    }

    /// A statistic this launcher does not model is kept, reported, and survives
    /// a round trip. The one thing it must never do is disappear, and the
    /// yardage episode is why: a field the code did not know about was assumed
    /// not to exist.
    #[test]
    fn an_unmodeled_statistic_is_preserved_and_reported_rather_than_dropped() {
        let extended = doctor(
            REAL_FALCONS,
            "\"stats\": {\"pass_td\": 0",
            "\"stats\": {\"two_point_conversions\": 3, \"pass_td\": 0",
        );
        let c = Capture::parse(extended.as_bytes()).expect("an extra field must not break parsing");
        assert_eq!(
            c.players[0].stats.extra.get("two_point_conversions"),
            Some(&Value::from(3)),
            "an unmodeled statistic was dropped on the floor"
        );
        let v = c.validate(None);
        assert!(v.has(codes::UNKNOWN_STAT_FIELD), "{:?}", v.codes());
        assert!(
            v.is_well_formed(),
            "an extra field is schema drift to report, not a reason to refuse a game"
        );

        let round_tripped: Capture =
            serde_json::from_slice(&serde_json::to_vec(&c).unwrap()).unwrap();
        assert_eq!(round_tripped, c);
    }

    /// Two different games must never share a key. If the canonical bytes ever
    /// came out empty or constant, every capture would collapse into one queue
    /// entry and every game after the first would be discarded as a duplicate.
    #[test]
    fn two_different_games_get_different_keys() {
        assert_ne!(falcons().content_key(), outlaws().content_key());
        let bytes = falcons().canonical_bytes();
        assert!(
            bytes.len() > 100,
            "the canonical bytes are suspiciously short"
        );
        assert_eq!(bytes[0], b'{');
    }

    /// The contract the deployed endpoint enforces, checked here so a capture is
    /// refused before a request is built rather than after it is rejected.
    #[test]
    fn the_ingest_contract_is_checked_before_anything_is_sent() {
        let mut wrong_schema = falcons();
        wrong_schema.schema = "nsr.game.v2".to_string();
        let v = wrong_schema.validate(None);
        assert!(v.has(codes::WRONG_SCHEMA), "{:?}", v.codes());
        assert!(!v.is_well_formed());

        let mut one_team = falcons();
        one_team.teams.pop();
        assert!(one_team.validate(None).has(codes::WRONG_TEAM_COUNT));

        let mut no_players = falcons();
        no_players.players.clear();
        let v = no_players.validate(None);
        assert!(v.has(codes::NO_PLAYERS), "{:?}", v.codes());
        let flag = v
            .flags
            .iter()
            .find(|f| f.code == codes::NO_PLAYERS)
            .unwrap();
        assert!(
            flag.message.contains("statread1"),
            "an empty player list has one known cause and the message should name \
             it: {}",
            flag.message
        );

        let v = falcons().validate(Some(MAX_PAYLOAD_BYTES + 1));
        assert!(v.has(codes::PAYLOAD_TOO_LARGE), "{:?}", v.codes());
        assert!(!v.is_well_formed());
        assert!(
            falcons().validate(Some(MAX_PAYLOAD_BYTES)).is_well_formed(),
            "exactly at the cap is inside it"
        );
    }

    /// The top level `style` array is in the inverted side order, confirmed in
    /// all five real captures that carry a team_totals block. Reading style
    /// straight out of it labels each team with the other one's score.
    #[test]
    fn style_is_read_from_team_totals_because_the_top_level_array_is_inverted() {
        let c = outlaws();
        assert_eq!(c.style, vec![4000, 56824]);
        assert_eq!(
            c.style_for_team(0),
            Some(56824),
            "the Outlaws scored 56824 style, which is style[1], not style[0]"
        );
        assert_eq!(c.style_for_team(1), Some(4000));

        assert_eq!(
            falcons().style_for_team(0),
            None,
            "with no team_totals the style cannot be attributed to a team, and a \
             number is not invented for it"
        );
    }

    /// Every code a validation can emit has to be in the published list, or a
    /// front end cannot know what it may be handed and a typo becomes a flag
    /// nobody ever matches.
    #[test]
    fn every_emitted_flag_code_is_in_the_published_list() {
        let listed: BTreeSet<&str> = codes::ALL.iter().copied().collect();
        assert_eq!(listed.len(), codes::ALL.len(), "codes::ALL has a duplicate");

        let mut probes: Vec<Capture> = vec![falcons(), outlaws()];
        let mut broken = falcons();
        broken.schema = "x".into();
        broken.teams.clear();
        broken.style.clear();
        broken.players[0].side = 7;
        broken.players[1].digp = broken.players[0].digp;
        probes.push(broken);

        for c in probes {
            for size in [None, Some(MAX_PAYLOAD_BYTES + 1)] {
                for flag in c.validate(size).flags {
                    assert!(
                        listed.contains(flag.code),
                        "{} is not in codes::ALL",
                        flag.code
                    );
                    assert!(
                        flag.message.ends_with('.'),
                        "a flag message must be a sentence: {}",
                        flag.message
                    );
                    assert!(flag.message.len() > 30, "too terse: {}", flag.message);
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // The content key
    // -----------------------------------------------------------------------

    /// The reason the key excludes `captured_at`, `source` and the identity
    /// block. Two clients play one lockstep game, each stamps its own time and
    /// writes its own seat, and if those changed the key the ladder would record
    /// the game twice.
    #[test]
    fn two_clients_of_one_game_produce_one_key() {
        let seat_zero = doctor(
            REAL_OUTLAWS,
            "\"captured_at\": \"2026-07-28T00:41:31Z\",",
            "\"captured_at\": \"2026-07-28T00:41:31Z\",\n  \"reborn_match\": \
             {\"match_id\": \"01J9Z8K2Q7X4T5V6W8Y0A1B2C3\", \"local_seat\": 0},",
        );
        let seat_one = doctor(
            REAL_OUTLAWS,
            "\"captured_at\": \"2026-07-28T00:41:31Z\",",
            "\"captured_at\": \"2026-07-28T00:41:33Z\",\n  \"reborn_match\": \
             {\"match_id\": \"01J9Z8K2Q7X4T5V6W8Y0A1B2C3\", \"local_seat\": 1},",
        );
        assert_ne!(seat_zero, seat_one, "the two files must really differ");

        let a = Capture::parse(seat_zero.as_bytes()).unwrap();
        let b = Capture::parse(seat_one.as_bytes()).unwrap();
        assert_ne!(a.captured_at, b.captured_at);
        assert!(
            a.extra.contains_key("reborn_match"),
            "the identity block is kept"
        );
        assert_ne!(a.extra, b.extra);
        assert_eq!(
            a.content_key(),
            b.content_key(),
            "one game seen from two seats must be one queue entry"
        );
    }

    /// The dedupe sort is by digp, so read order must not reach the key. A queue
    /// that keyed on read order would take the same game twice from two folders.
    #[test]
    fn players_may_arrive_in_any_order_and_still_key_the_same() {
        let mut reversed = falcons();
        reversed.players.reverse();
        assert_ne!(
            reversed.players[0].name,
            falcons().players[0].name,
            "the order really changed"
        );
        assert_eq!(reversed.content_key(), falcons().content_key());
    }

    /// The other half: if the numbers change it is a different game, or the same
    /// game recorded differently, and either way it must not be mistaken for one
    /// already sent.
    #[test]
    fn a_changed_counter_changes_the_key() {
        let doctored = doctor(
            REAL_FALCONS,
            "\"tackles\": 4, \"sacks\": 2",
            "\"tackles\": 3, \"sacks\": 2",
        );
        let changed = Capture::parse(doctored.as_bytes()).unwrap();
        assert_ne!(changed.content_key(), falcons().content_key());

        let mut rescored = falcons();
        rescored.teams[0].score = 37;
        assert_ne!(rescored.content_key(), falcons().content_key());
    }

    /// The canonical bytes have to be identical on another machine. Sorted keys
    /// are what makes that true, and enabling serde_json's `preserve_order`
    /// feature is what would silently break it and orphan every ledger row.
    #[test]
    fn the_canonical_bytes_have_sorted_keys() {
        let bytes = falcons().canonical_bytes();
        let text = String::from_utf8(bytes).expect("canonical bytes are UTF-8");

        let players = text.find("\"players\"").expect("players");
        let style = text.find("\"style\"").expect("style");
        let teams = text.find("\"teams\"").expect("teams");
        assert!(
            players < style && style < teams,
            "top level keys are not sorted"
        );

        let stats = text.find("\"stats\"").expect("stats");
        let first_stat = text[stats..]
            .find("\"forced_fumbles\"")
            .expect("forced_fumbles");
        let pass_td = text[stats..].find("\"pass_td\"").expect("pass_td");
        assert!(first_stat < pass_td, "the stat map keys are not sorted");

        assert!(
            !text.contains("captured_at") && !text.contains("nsr.game.v1"),
            "the key must not depend on the timestamp or the schema string"
        );
    }

    // -----------------------------------------------------------------------
    // The queue
    // -----------------------------------------------------------------------

    #[derive(Debug)]
    struct Recorder {
        calls: std::cell::RefCell<Vec<CaptureKey>>,
        outcome: SubmitOutcome,
        expect_bytes: Option<Vec<u8>>,
    }

    impl Recorder {
        fn new(outcome: SubmitOutcome) -> Self {
            Self {
                calls: Default::default(),
                outcome,
                expect_bytes: None,
            }
        }
        fn expecting(outcome: SubmitOutcome, bytes: &[u8]) -> Self {
            Self {
                calls: Default::default(),
                outcome,
                expect_bytes: Some(bytes.to_vec()),
            }
        }
        fn count(&self) -> usize {
            self.calls.borrow().len()
        }
    }

    impl Submitter for Recorder {
        fn submit(&self, entry: &QueueEntry, capture: &[u8]) -> SubmitOutcome {
            if let Some(expected) = &self.expect_bytes {
                assert_eq!(
                    capture,
                    expected.as_slice(),
                    "the submitter must be handed the bytes as captured, not a \
                     re-serialization: the server hashes what it is given"
                );
            }
            self.calls.borrow_mut().push(entry.key.clone());
            self.outcome.clone()
        }
    }

    fn queue(dir: &Path, now_ms: u64) -> StatsQueue {
        StatsQueue::open_at(dir, FixedClock::new(now_ms)).expect("open the queue")
    }

    #[test]
    fn a_capture_is_queued_with_its_summary_and_the_bytes_are_kept_verbatim() {
        let dir = tempfile::tempdir().unwrap();
        let q = queue(dir.path(), 1_000);

        let Enqueued::Queued(entry) = q
            .enqueue(REAL_FALCONS.as_bytes(), None, Some("1000-offline"))
            .unwrap()
        else {
            panic!("a well formed capture with a session must be queued");
        };
        assert_eq!(entry.first_seen_ms, 1_000);
        assert_eq!(entry.session_id.as_deref(), Some("1000-offline"));

        let summary = entry.summary.as_ref().expect("a summary");
        assert_eq!(summary.players, 14);
        assert_eq!(summary.teams[0].name, "Falcons");
        assert!(!summary.sides_verified);
        assert!(
            summary.needs_review,
            "the held state must survive the payload"
        );

        assert_eq!(
            q.capture_bytes(&entry.key).unwrap().as_deref(),
            Some(REAL_FALCONS.as_bytes()),
            "the queue must keep the captured bytes byte for byte"
        );
        assert_eq!(q.pending().unwrap().len(), 1);
        assert!(q.orphaned_payloads().unwrap().is_empty());
    }

    /// One game, two chances to be recorded twice. The content key is what makes
    /// the second one a no-op.
    #[test]
    fn one_game_cannot_be_queued_twice() {
        let dir = tempfile::tempdir().unwrap();
        let q = queue(dir.path(), 1_000);

        let first = q
            .enqueue(REAL_FALCONS.as_bytes(), None, Some("s1"))
            .unwrap();
        let second = q
            .enqueue(REAL_FALCONS.as_bytes(), None, Some("s1"))
            .unwrap();
        assert!(matches!(first, Enqueued::Queued(_)));
        assert!(
            matches!(second, Enqueued::AlreadyQueued(_)),
            "got {second:?}"
        );
        assert_eq!(q.entries().unwrap().len(), 1);
    }

    /// The ledger outliving the payload is the whole point. A player who still
    /// has the capture file, or a re-ingested folder, must not put a sent game
    /// back in the queue.
    #[test]
    fn a_sent_game_cannot_be_queued_again_even_after_its_payload_is_pruned() {
        let dir = tempfile::tempdir().unwrap();
        let key = {
            let q = queue(dir.path(), 1_000);
            let Enqueued::Queued(entry) = q
                .enqueue(REAL_FALCONS.as_bytes(), None, Some("s1"))
                .unwrap()
            else {
                panic!("expected a queued capture");
            };
            let report = q
                .submit_pending(&Recorder::new(SubmitOutcome::Accepted), 3)
                .unwrap();
            assert_eq!(report.accepted.len(), 1);
            entry.key
        };

        // A week later the payload is pruned.
        let later = queue(dir.path(), 1_000 + 7 * 86_400_000);
        assert_eq!(later.prune_terminal(86_400_000).unwrap(), vec![key.clone()]);
        assert!(later.capture_bytes(&key).unwrap().is_none());

        let again = later
            .enqueue(REAL_FALCONS.as_bytes(), None, Some("s2"))
            .unwrap();
        match again {
            Enqueued::AlreadySubmitted { state, .. } => {
                assert_eq!(state, TerminalState::Accepted)
            }
            other => panic!("a pruned but sent game was offered again: {other:?}"),
        }
        assert!(later.pending().unwrap().is_empty());
    }

    /// A review is a success. On July 27 2026 the uploader read it as a failure
    /// and retried forever, which took the endpoint down. It must be sent once
    /// and never again.
    #[test]
    fn held_for_review_is_a_success_and_is_never_sent_again() {
        let dir = tempfile::tempdir().unwrap();
        let q = queue(dir.path(), 1_000);
        q.enqueue(REAL_FALCONS.as_bytes(), None, Some("s1"))
            .unwrap();

        let submitter = Recorder::expecting(
            SubmitOutcome::Held {
                reason: "sides not verified".into(),
            },
            REAL_FALCONS.as_bytes(),
        );
        let first = q.submit_pending(&submitter, 3).unwrap();
        assert_eq!(first.held.len(), 1);
        assert_eq!(submitter.count(), 1);

        let second = q.submit_pending(&submitter, 3).unwrap();
        assert_eq!(
            submitter.count(),
            1,
            "a held result was sent a second time, which is the retry loop"
        );
        assert_eq!(second.sent(), 0);
        assert!(q.pending().unwrap().is_empty());
        match &q.entries().unwrap()[0].state {
            EntryState::Held { reason } => assert_eq!(reason, "sides not verified"),
            other => panic!("expected held, got {other:?}"),
        }
    }

    /// Offline is the normal case for this launcher, so a failure to deliver has
    /// to leave the game exactly where it was.
    #[test]
    fn a_deferred_capture_stays_on_disk_and_is_tried_again() {
        let dir = tempfile::tempdir().unwrap();
        let q = queue(dir.path(), 1_000);
        q.enqueue(REAL_FALCONS.as_bytes(), None, Some("s1"))
            .unwrap();

        let offline = Recorder::new(SubmitOutcome::Deferred {
            reason: "no connection".into(),
        });
        let report = q.submit_pending(&offline, 3).unwrap();
        assert_eq!(report.deferred.len(), 1);
        assert_eq!(report.still_waiting(), 1);

        let entry = &q.entries().unwrap()[0];
        assert_eq!(entry.attempts, 1);
        assert_eq!(entry.last_error.as_deref(), Some("no connection"));
        assert!(entry.state.is_pending(), "it must still be in the queue");
        assert!(
            q.capture_bytes(&entry.key).unwrap().is_some(),
            "a failed send must never delete the capture"
        );

        let good = Recorder::new(SubmitOutcome::Accepted);
        assert_eq!(q.submit_pending(&good, 3).unwrap().accepted.len(), 1);
    }

    /// Attempts are bounded so a dead endpoint does not spin, and a stalled
    /// capture is kept rather than expired. The game happened; nobody else has a
    /// copy.
    #[test]
    fn a_stalled_capture_is_kept_and_can_be_put_back_in_the_queue() {
        let dir = tempfile::tempdir().unwrap();
        let q = queue(dir.path(), 1_000);
        q.enqueue(REAL_FALCONS.as_bytes(), None, Some("s1"))
            .unwrap();

        let dead = Recorder::new(SubmitOutcome::Deferred {
            reason: "connection refused".into(),
        });
        for _ in 0..2 {
            q.submit_pending(&dead, 2).unwrap();
        }
        assert_eq!(dead.count(), 2, "the endpoint must stop being called");
        let stalled = q.stalled().unwrap();
        assert_eq!(stalled.len(), 1);
        assert!(q.pending().unwrap().is_empty());

        q.submit_pending(&dead, 2).unwrap();
        assert_eq!(
            dead.count(),
            2,
            "a stalled entry must not keep being retried"
        );
        assert!(
            q.capture_bytes(&stalled[0].key).unwrap().is_some(),
            "a stalled capture must never be deleted"
        );

        assert_eq!(q.retry_stalled().unwrap().len(), 1);
        assert_eq!(q.pending().unwrap()[0].attempts, 0);
    }

    /// Pruning is for results the server already has. Anything else is the only
    /// copy in existence, and no age makes it safe to delete.
    #[test]
    fn pruning_never_deletes_a_capture_that_has_not_been_sent() {
        let dir = tempfile::tempdir().unwrap();
        let q = queue(dir.path(), 1_000);

        // Queued, quarantined, and stalled, all left behind by a failed send.
        q.enqueue(REAL_FALCONS.as_bytes(), None, Some("s1"))
            .unwrap();
        q.enqueue(REAL_OUTLAWS.as_bytes(), None, None).unwrap();
        q.enqueue(b"{ truncated", None, Some("s1")).unwrap();

        let dead = Recorder::new(SubmitOutcome::Deferred {
            reason: "gone".into(),
        });
        q.submit_pending(&dead, 1).unwrap();
        assert_eq!(q.stalled().unwrap().len(), 1);

        let much_later = queue(dir.path(), 1_000 + 400 * 86_400_000);
        assert!(
            much_later.prune_terminal(0).unwrap().is_empty(),
            "a capture that was never sent must survive any prune"
        );
        assert_eq!(much_later.entries().unwrap().len(), 3);
        for entry in much_later.entries().unwrap() {
            assert!(
                much_later.capture_bytes(&entry.key).unwrap().is_some(),
                "the bytes of {} were deleted",
                entry.key
            );
        }
    }

    /// Where a result may go is decided at launch, not read out of the file. An
    /// offline game, a league game and a ranked game produce byte-identical
    /// captures, so a capture no launch accounts for has no known destination
    /// and must not be sent anywhere.
    #[test]
    fn a_capture_with_no_session_is_quarantined_and_never_offered_for_sending() {
        let dir = tempfile::tempdir().unwrap();
        let q = queue(dir.path(), 1_000);

        let Enqueued::Quarantined(entry) = q.enqueue(REAL_OUTLAWS.as_bytes(), None, None).unwrap()
        else {
            panic!("a capture with no session must be quarantined");
        };
        assert_eq!(
            entry.state,
            EntryState::Quarantined {
                reason: QuarantineReason::NoSession
            }
        );
        assert!(q.pending().unwrap().is_empty());
        assert_eq!(q.quarantined().unwrap().len(), 1);

        let submitter = Recorder::new(SubmitOutcome::Accepted);
        q.submit_pending(&submitter, 3).unwrap();
        assert_eq!(
            submitter.count(),
            0,
            "a quarantined capture must not be sent"
        );
        assert!(
            q.capture_bytes(&entry.key).unwrap().is_some(),
            "quarantine keeps the file, it does not discard it"
        );
    }

    /// An unreadable capture is still the only record of a game somebody played.
    /// It is kept and surfaced, never parsed into placeholder numbers and never
    /// deleted.
    #[test]
    fn an_unreadable_capture_is_kept_on_disk_rather_than_dropped() {
        let dir = tempfile::tempdir().unwrap();
        let q = queue(dir.path(), 1_000);

        let truncated = b"{\"schema\": \"nsr.game.v1\", \"teams\": [{\"name\": \"Fal";
        let Enqueued::Quarantined(entry) = q.enqueue(truncated, None, Some("s1")).unwrap() else {
            panic!("unreadable bytes must be quarantined, not discarded");
        };
        assert!(matches!(
            entry.state,
            EntryState::Quarantined {
                reason: QuarantineReason::Unreadable { .. }
            }
        ));
        assert!(
            entry.summary.is_none(),
            "a capture that could not be read must not be given a summary it does \
             not have"
        );
        assert_eq!(
            q.capture_bytes(&entry.key).unwrap().as_deref(),
            Some(&truncated[..])
        );
        assert!(q.pending().unwrap().is_empty());
    }

    /// Ingest runs after every game, so it has to be safe to run twice, and it
    /// must leave alone the files Dolphin keeps for itself.
    #[test]
    fn ingesting_the_dolphin_folder_moves_captures_and_is_safe_to_repeat() {
        let dir = tempfile::tempdir().unwrap();
        let user_dir = dir.path().join("user");
        let captures = dolphin_capture_dir(&user_dir);
        std::fs::create_dir_all(captures.join("_previous")).unwrap();
        std::fs::write(captures.join("Falcons_vs_Chargers_001.json"), REAL_FALCONS).unwrap();
        std::fs::write(
            captures.join("Outlaws_vs_Shadow_Knights_001.json"),
            REAL_OUTLAWS,
        )
        .unwrap();
        std::fs::write(captures.join("uploaded.txt"), "some ledger\n").unwrap();
        std::fs::write(captures.join("_previous").join("old.json"), REAL_OUTLAWS).unwrap();

        let q = queue(&dir.path().join("queue"), 5_000);
        let first = q.ingest_from(&captures, Some("5000-online")).unwrap();
        assert_eq!(first.len(), 2, "got {first:?}");
        assert!(first.iter().all(|e| matches!(e, Enqueued::Queued(_))));
        assert!(
            captures.join("uploaded.txt").exists(),
            "uploaded.txt is not a capture and must be left alone"
        );
        assert!(
            captures.join("_previous").join("old.json").exists(),
            "the _previous folder is not scanned, so its files must stay put"
        );
        assert!(!captures.join("Falcons_vs_Chargers_001.json").exists());

        let second = q.ingest_from(&captures, Some("5000-online")).unwrap();
        assert!(second.is_empty(), "the folder should be empty now");

        // The same files reappear, for instance because a crash interrupted the
        // move last time. Nothing is duplicated.
        std::fs::write(captures.join("Falcons_vs_Chargers_001.json"), REAL_FALCONS).unwrap();
        let third = q.ingest_from(&captures, Some("5000-online")).unwrap();
        assert!(
            matches!(third[0], Enqueued::AlreadyQueued(_)),
            "got {:?}",
            third[0]
        );
        assert_eq!(q.entries().unwrap().len(), 2);
    }

    /// The crash window the ordering exists for. The ledger row was written and
    /// the process died before the record was updated, so the record still says
    /// queued. The ledger wins, and the game is not sent twice.
    #[test]
    fn a_ledger_row_written_before_a_crash_stops_the_resend() {
        let dir = tempfile::tempdir().unwrap();
        let q = queue(dir.path(), 1_000);
        let Enqueued::Queued(entry) = q
            .enqueue(REAL_FALCONS.as_bytes(), None, Some("s1"))
            .unwrap()
        else {
            panic!("expected a queued capture");
        };
        q.submit_pending(&Recorder::new(SubmitOutcome::Accepted), 3)
            .unwrap();

        // Roll the record back to what it looked like an instant before the
        // crash. The ledger row stays.
        let mut rolled_back = q.read_entry(&entry.key).unwrap().unwrap();
        rolled_back.state = EntryState::Queued;
        rolled_back.finished_ms = None;
        q.write_entry(&rolled_back).unwrap();

        assert!(
            q.pending().unwrap().is_empty(),
            "the ledger must override a record that says queued"
        );
        let submitter = Recorder::new(SubmitOutcome::Accepted);
        let report = q.submit_pending(&submitter, 3).unwrap();
        assert_eq!(submitter.count(), 0, "the game was sent a second time");
        assert_eq!(report.repaired_from_ledger, vec![entry.key.clone()]);
        assert!(q
            .read_entry(&entry.key)
            .unwrap()
            .unwrap()
            .state
            .is_terminal());
    }

    /// A power cut mid-append must not make the ledger unreadable, because
    /// losing it means resending everything it recorded.
    #[test]
    fn the_ledger_survives_a_half_written_line() {
        let dir = tempfile::tempdir().unwrap();
        let q = queue(dir.path(), 1_000);
        q.enqueue(REAL_FALCONS.as_bytes(), None, Some("s1"))
            .unwrap();
        q.submit_pending(&Recorder::new(SubmitOutcome::Accepted), 3)
            .unwrap();

        let ledger = q.ledger_path();
        let good = std::fs::read_to_string(&ledger).unwrap();
        std::fs::write(&ledger, format!("{good}{{\"key\": \"half")).unwrap();

        assert_eq!(
            q.terminal_states().unwrap().len(),
            1,
            "one good row must still be readable"
        );
        let submitter = Recorder::new(SubmitOutcome::Accepted);
        q.submit_pending(&submitter, 3).unwrap();
        assert_eq!(submitter.count(), 0);
    }

    /// A queued record whose payload has vanished is surfaced, not sent as an
    /// empty body and not silently forgotten.
    #[test]
    fn a_queued_record_with_no_payload_is_quarantined_rather_than_sent_empty() {
        let dir = tempfile::tempdir().unwrap();
        let q = queue(dir.path(), 1_000);
        let Enqueued::Queued(entry) = q
            .enqueue(REAL_FALCONS.as_bytes(), None, Some("s1"))
            .unwrap()
        else {
            panic!("expected a queued capture");
        };
        std::fs::remove_file(q.payload_path(&entry.key)).unwrap();

        let submitter = Recorder::new(SubmitOutcome::Accepted);
        let report = q.submit_pending(&submitter, 3).unwrap();
        assert_eq!(
            submitter.count(),
            0,
            "nothing may be sent without a payload"
        );
        assert_eq!(report.lost_payload, vec![entry.key.clone()]);
        assert!(matches!(
            q.read_entry(&entry.key).unwrap().unwrap().state,
            EntryState::Quarantined { .. }
        ));
    }

    /// A record copied or renamed under another key would otherwise send one
    /// game under a different game's identity. It is ignored, and its files are
    /// left alone.
    #[test]
    fn a_renamed_record_is_ignored_rather_than_acted_on() {
        let dir = tempfile::tempdir().unwrap();
        let q = queue(dir.path(), 1_000);
        let Enqueued::Queued(entry) = q
            .enqueue(REAL_FALCONS.as_bytes(), None, Some("s1"))
            .unwrap()
        else {
            panic!("expected a queued capture");
        };

        let impostor = CaptureKey::of_bytes(b"not this game");
        std::fs::copy(q.entry_path(&entry.key), q.entry_path(&impostor)).unwrap();

        assert_eq!(
            q.entries().unwrap().len(),
            1,
            "a record whose filename and contents disagree must not be counted"
        );
        assert!(
            q.entry_path(&impostor).exists(),
            "it is ignored, not deleted, because deleting is not this code's call"
        );
    }

    /// The record format is ours to keep readable. If a field is renamed, this
    /// fails and whoever renamed it has to think about the queues already on
    /// players' disks.
    #[test]
    fn the_record_on_disk_shape_is_pinned() {
        let dir = tempfile::tempdir().unwrap();
        let q = queue(dir.path(), 4_242);
        let Enqueued::Queued(entry) = q
            .enqueue(REAL_FALCONS.as_bytes(), None, Some("s1"))
            .unwrap()
        else {
            panic!("expected a queued capture");
        };

        let json: Value =
            serde_json::from_slice(&std::fs::read(q.entry_path(&entry.key)).unwrap()).unwrap();
        assert_eq!(json["state"]["kind"], "queued");
        assert_eq!(json["first_seen_ms"], 4_242);
        assert_eq!(json["attempts"], 0);
        assert_eq!(json["session_id"], "s1");
        assert_eq!(json["summary"]["sides_verified"], false);
        assert_eq!(json["key"], entry.key.as_str());
        assert!(
            json.get("finished_ms").is_none(),
            "an unsent result must not carry a finish time"
        );

        let quarantined = q.enqueue(b"nonsense", None, Some("s1")).unwrap();
        let key = quarantined.key().unwrap();
        let json: Value =
            serde_json::from_slice(&std::fs::read(q.entry_path(key)).unwrap()).unwrap();
        assert_eq!(json["state"]["kind"], "quarantined");
        assert_eq!(json["state"]["reason"]["kind"], "unreadable");
        assert!(
            json.get("summary").is_none(),
            "a capture that could not be read must not carry an invented summary"
        );
    }

    #[test]
    fn every_enqueue_outcome_says_something_actionable() {
        let dir = tempfile::tempdir().unwrap();
        let q = queue(dir.path(), 1_000);

        let outcomes = vec![
            q.enqueue(REAL_FALCONS.as_bytes(), None, Some("s1"))
                .unwrap(),
            q.enqueue(REAL_FALCONS.as_bytes(), None, Some("s1"))
                .unwrap(),
            q.enqueue(REAL_OUTLAWS.as_bytes(), None, None).unwrap(),
            q.enqueue(b"{ truncated", None, Some("s1")).unwrap(),
            Enqueued::CouldNotRead {
                path: PathBuf::from("x.json"),
                detail: "access denied.".into(),
            },
        ];
        for outcome in outcomes {
            let m = outcome.message();
            assert!(m.len() > 20, "too terse to act on: {m}");
            assert!(m.ends_with('.'), "not a sentence: {m}");
        }

        for reason in [
            QuarantineReason::NoSession,
            QuarantineReason::Unreadable {
                detail: "bad JSON.".into(),
            },
            QuarantineReason::FailedValidation {
                flags: vec![codes::NO_PLAYERS.into()],
            },
        ] {
            let m = reason.message();
            assert!(m.ends_with('.'), "not a sentence: {m}");
            assert!(
                m.contains("kept"),
                "every held back capture must say it is being kept: {m}"
            );
        }
    }

    /// A malformed capture is refused before the network and kept on disk. Both
    /// halves are the point: it cannot be sent, and it cannot be lost.
    #[test]
    fn a_malformed_capture_is_quarantined_with_its_reason_and_never_sent() {
        let dir = tempfile::tempdir().unwrap();
        let q = queue(dir.path(), 1_000);
        let broken = doctor(REAL_FALCONS, "\"nsr.game.v1\"", "\"nsr.game.v2\"");

        let Enqueued::Quarantined(entry) = q.enqueue(broken.as_bytes(), None, Some("s1")).unwrap()
        else {
            panic!("a capture the endpoint would refuse must not be queued for sending");
        };
        match &entry.state {
            EntryState::Quarantined {
                reason: QuarantineReason::FailedValidation { flags },
            } => assert!(
                flags.contains(&codes::WRONG_SCHEMA.to_string()),
                "the reason must survive the payload: {flags:?}"
            ),
            other => panic!("expected a validation quarantine, got {other:?}"),
        }

        let submitter = Recorder::new(SubmitOutcome::Accepted);
        q.submit_pending(&submitter, 3).unwrap();
        assert_eq!(submitter.count(), 0);
        assert!(q.capture_bytes(&entry.key).unwrap().is_some());
    }

    /// A rejected result is not retried and is not thrown away. The player has
    /// to be able to report it, and the ledger has to remember it so a
    /// re-ingested folder does not send it again.
    #[test]
    fn a_rejected_result_is_kept_and_not_retried() {
        let dir = tempfile::tempdir().unwrap();
        let q = queue(dir.path(), 1_000);
        q.enqueue(REAL_FALCONS.as_bytes(), None, Some("s1"))
            .unwrap();

        let submitter = Recorder::new(SubmitOutcome::Rejected {
            reason: "no such fixture".into(),
        });
        let report = q.submit_pending(&submitter, 3).unwrap();
        assert_eq!(report.rejected.len(), 1);

        q.submit_pending(&submitter, 3).unwrap();
        assert_eq!(
            submitter.count(),
            1,
            "a permanent refusal must not be retried"
        );

        let entry = &q.entries().unwrap()[0];
        assert_eq!(entry.last_error.as_deref(), Some("no such fixture"));
        assert!(
            q.capture_bytes(&entry.key).unwrap().is_some(),
            "a rejected capture must stay on disk so it can be reported"
        );
        assert_eq!(
            q.terminal_state(&entry.key).unwrap(),
            Some(TerminalState::Rejected)
        );
    }

    /// The queue lives under a directory the launcher classes as user data, so a
    /// cache purge cannot delete results that have not been sent.
    #[test]
    fn the_queue_directory_is_user_data_and_never_purgeable() {
        assert!(DataDir::StatsQueue.is_user_data());
        assert!(!DataDir::StatsQueue.is_derived());
    }
}
