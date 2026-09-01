//! Driving the launcher itself with a gamepad.
//!
//! # Why this exists
//!
//! The window can currently only be used with a mouse. A controller-first
//! launcher for a controller-first game, that cannot be driven by a controller,
//! is not finished: the player picks up a pad, boots the launcher, and has to
//! put the pad down to press Play.
//!
//! This is **not** the emulator's controller binding. That is
//! [`crate::controller`], which writes `GCPadNew.ini` so the game reads a pad,
//! and [`crate::devices`], which finds pads and names them the way the emulator
//! names them. Nothing here is written to disk for the emulator. The only thing
//! borrowed from that side is `Tuning::deadzone_percent`, because a player who
//! has already told the launcher how much stick travel to ignore should not have
//! to say it twice.
//!
//! # The four failures this module is built around
//!
//! **Focus escaping to nothing.** A grid with a hole in it, a rail that shrank
//! while focus was inside it, or a screen whose default focus does not exist,
//! all end the same way: the focus ring is nowhere, A does nothing, and the only
//! way out is the mouse. So the grid is validated once at construction and the
//! row type cannot be empty, which makes "there is always a cell" a property of
//! the type rather than a check somebody has to remember.
//!
//! **Repeat that either feels dead or races.** A held direction with no repeat
//! makes a twenty row Settings page a twenty press page. A naive repeat that
//! catches up after a dropped frame flings the focus ring across the page. Both
//! are timing bugs that are impossible to test when the timer lives in a view,
//! so the timing is a state machine here and the clock is a parameter.
//!
//! **Confirming something the player never chose.** A destructive action gated
//! behind an 800 ms hold is only a gate if the hold was observed. A button that
//! was already down when the dialog opened, or a hold that spans a stall where
//! the launcher was not sampling, must not be credited. The audit already found
//! this class of bug once, in ready-up, and resolved it with a delay before
//! focus moves.
//!
//! **A plausible looking hint strip.** The hint strip is the only place button
//! bindings are stated, so an unbound contextual slot returns `None` and shows
//! nothing. It never shows a placeholder label, because a player who presses X
//! because the strip said "Options" and gets nothing has been lied to.
//!
//! # Where the numbers come from
//!
//! The shortcut table, the three level hierarchy, the modal kinds, the rule that
//! X and Y are never destructive, and the 800 ms hold with its five permitted
//! uses are all from `docs/design/INFORMATION_ARCHITECTURE.md` sections 4, 6 and
//! 10, and from `docs/design/CONTROLLER_AND_ACCESSIBILITY_AUDIT.md` section 3.
//!
//! The repeat delay and interval are **not** in either document. They are this
//! module's own defaults, chosen and defended at [`RepeatTiming::default`], and
//! adjustable. Nothing else here was invented: where the specification is silent
//! and a value cannot be defended, the type says the value is absent instead.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::controller::{DeviceFamily, Tuning};
use crate::platform::Error;

// ---------------------------------------------------------------------------
// Direction
// ---------------------------------------------------------------------------

/// One discrete focus move.
///
/// Discrete on purpose. Focus lands on controls, so there is no such thing as
/// moving 0.4 of the way to the next one, and treating the stick as analog here
/// is how a launcher ends up with a focus ring that drifts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

impl Direction {
    pub const ALL: &'static [Direction] = &[
        Direction::Up,
        Direction::Down,
        Direction::Left,
        Direction::Right,
    ];

    pub fn is_vertical(self) -> bool {
        matches!(self, Direction::Up | Direction::Down)
    }

    pub fn is_horizontal(self) -> bool {
        !self.is_vertical()
    }
}

// ---------------------------------------------------------------------------
// The focus grid
// ---------------------------------------------------------------------------

/// Something focus can land on.
///
/// `strip_len` is what makes a horizontal rail one focus stop rather than five,
/// which is exception 2 of the focus order rules: left and right move inside a
/// rail, up and down leave it. Without it, a rail of twelve mod cards swallows
/// vertical traversal and the player cannot get past it in one press.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FocusCell {
    id: String,
    strip_len: usize,
}

impl FocusCell {
    /// An ordinary control: one stop, one item.
    pub fn single(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            strip_len: 1,
        }
    }

    /// A horizontal rail or carousel holding `len` items behind one stop.
    ///
    /// `len` is the count that is really there. A rail with nothing in it must
    /// not be a focus stop at all, so a `len` of zero is refused by
    /// [`FocusGrid::new`] rather than quietly focusing an empty row of cards.
    pub fn strip(id: impl Into<String>, len: usize) -> Self {
        Self {
            id: id.into(),
            strip_len: len,
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn strip_len(&self) -> usize {
        self.strip_len
    }

    pub fn is_strip(&self) -> bool {
        self.strip_len > 1
    }
}

/// What happens at the end of a row or column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wrap {
    /// Movement stops. The caller gets [`PadEvent::FocusBlocked`] once.
    Clamp,
    /// Movement continues from the far end.
    Around,
}

/// Wrapping, per axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WrapPolicy {
    pub vertical: Wrap,
    pub horizontal: Wrap,
}

impl Default for WrapPolicy {
    /// Clamp on both axes.
    ///
    /// A held stick that wraps teleports the player from the bottom of Settings
    /// to the top, and with a repeat running they will not see it happen, only
    /// notice they are somewhere else. Wrapping is right for a short carousel
    /// and is available per grid, but it is not the safe default.
    fn default() -> Self {
        Self {
            vertical: Wrap::Clamp,
            horizontal: Wrap::Clamp,
        }
    }
}

/// A row with at least one cell.
///
/// The non-emptiness lives in the type because every focus operation has to
/// return *some* cell. An `Option` at this level forces each caller to invent a
/// fallback, and an invented fallback is precisely how focus ends up on nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Row {
    head: FocusCell,
    tail: Vec<FocusCell>,
}

impl Row {
    fn len(&self) -> usize {
        1 + self.tail.len()
    }

    fn get(&self, index: usize) -> Option<&FocusCell> {
        if index == 0 {
            Some(&self.head)
        } else {
            self.tail.get(index - 1)
        }
    }

    /// Total: an out of range index clamps to the first cell.
    ///
    /// Every index handed to this is kept in range by the cursor, so the clamp
    /// is unreachable in practice. It exists so this function cannot fail,
    /// because the alternative is an `unwrap` in the middle of navigation.
    fn at(&self, index: usize) -> &FocusCell {
        self.get(index).unwrap_or(&self.head)
    }

    fn cells(&self) -> Vec<&FocusCell> {
        let mut out = Vec::with_capacity(self.len());
        out.push(&self.head);
        out.extend(self.tail.iter());
        out
    }
}

/// Why a grid was refused.
///
/// Every one of these is a layout that would put the focus ring somewhere the
/// player cannot see or cannot act on, so they are refused at construction
/// rather than discovered by a player with a pad in their hands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GridProblem {
    /// No cells at all. There is nothing to focus, so there is no screen.
    Empty,
    /// A row with no cells. Vertical movement would land in it and focus
    /// nothing.
    EmptyRow { row: usize },
    /// A rail declared with no items. It must be absent from the grid, not
    /// present and empty, or focus lands on a rail with no cards in it.
    EmptyStrip { id: String },
    /// Two cells sharing an id. Focus restoration keys on the id, so a
    /// duplicate silently restores focus to the wrong control.
    DuplicateId { id: String },
    /// The named default focus is not in the grid. Falling back to the first
    /// cell is how a destructive action becomes default focus.
    UnknownDefault { id: String },
}

impl GridProblem {
    /// A sentence naming what to do about it.
    pub fn message(&self) -> String {
        match self {
            GridProblem::Empty => "this screen declares no focusable cells, so a pad cannot be \
                 used on it at all. Give it at least one cell, or do not build a \
                 grid for it."
                .to_string(),
            GridProblem::EmptyRow { row } => format!(
                "row {row} has no cells, so moving down into it would focus \
                 nothing. Remove the row rather than leaving it empty."
            ),
            GridProblem::EmptyStrip { id } => format!(
                "the rail \"{id}\" declares no items. A rail with nothing in it \
                 must be left out of the grid, not included as an empty focus \
                 stop."
            ),
            GridProblem::DuplicateId { id } => format!(
                "two cells both use the id \"{id}\". Focus restoration looks \
                 items up by id, so a duplicate restores focus to the wrong \
                 one. Make the ids unique, for example by prefixing the section."
            ),
            GridProblem::UnknownDefault { id } => format!(
                "the default focus \"{id}\" is not in the grid. Name a cell that \
                 exists: every screen has to choose its safest primary rather \
                 than falling back to whatever is first."
            ),
        }
    }
}

impl From<GridProblem> for Error {
    fn from(problem: GridProblem) -> Self {
        Error::PlainIo(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            problem.message(),
        ))
    }
}

/// The focusable layout of one screen, in reading order.
///
/// Rows may be ragged, because real screens are: a hero rail is one cell wide,
/// a pair of cards is two. Vertical movement remembers the column it wanted, so
/// passing through a one cell row does not permanently collapse the player into
/// the left edge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FocusGrid {
    head: Row,
    tail: Vec<Row>,
    default_id: String,
    wrap: WrapPolicy,
}

impl FocusGrid {
    /// Build and validate a grid.
    ///
    /// `default_focus` is required, not optional. Every screen has to state its
    /// safest primary: Home focuses Play Now, a confirm dialog focuses Cancel.
    /// Making it optional means the answer defaults to index 0, and index 0 is
    /// sometimes the destructive one.
    pub fn new(
        rows: Vec<Vec<FocusCell>>,
        default_focus: &str,
    ) -> std::result::Result<Self, GridProblem> {
        let mut built: Vec<Row> = Vec::new();
        for (index, cells) in rows.into_iter().enumerate() {
            let mut iter = cells.into_iter();
            let Some(head) = iter.next() else {
                return Err(GridProblem::EmptyRow { row: index });
            };
            built.push(Row {
                head,
                tail: iter.collect(),
            });
        }
        if built.is_empty() {
            return Err(GridProblem::Empty);
        }
        let head = built.remove(0);
        let grid = FocusGrid {
            head,
            tail: built,
            default_id: default_focus.to_string(),
            wrap: WrapPolicy::default(),
        };

        let mut seen: BTreeSet<&str> = BTreeSet::new();
        for row in grid.rows() {
            for cell in row.cells() {
                if cell.strip_len == 0 {
                    return Err(GridProblem::EmptyStrip {
                        id: cell.id.clone(),
                    });
                }
                if !seen.insert(cell.id()) {
                    return Err(GridProblem::DuplicateId {
                        id: cell.id.clone(),
                    });
                }
            }
        }
        if !seen.contains(default_focus) {
            return Err(GridProblem::UnknownDefault {
                id: default_focus.to_string(),
            });
        }
        Ok(grid)
    }

    /// Replace the wrapping policy.
    pub fn with_wrap(mut self, wrap: WrapPolicy) -> Self {
        self.wrap = wrap;
        self
    }

    pub fn wrap(&self) -> WrapPolicy {
        self.wrap
    }

    /// The cell focus starts on, and returns to when the previously focused
    /// item is gone.
    pub fn default_focus(&self) -> &str {
        &self.default_id
    }

    fn rows(&self) -> Vec<&Row> {
        let mut out = Vec::with_capacity(1 + self.tail.len());
        out.push(&self.head);
        out.extend(self.tail.iter());
        out
    }

    pub fn row_count(&self) -> usize {
        1 + self.tail.len()
    }

    fn row(&self, index: usize) -> Option<&Row> {
        if index == 0 {
            Some(&self.head)
        } else {
            self.tail.get(index - 1)
        }
    }

    /// Total, for the same reason as [`Row::at`].
    fn row_at(&self, index: usize) -> &Row {
        self.row(index).unwrap_or(&self.head)
    }

    /// Every id, in reading order.
    pub fn ids(&self) -> Vec<&str> {
        let mut out = Vec::new();
        for row in self.rows() {
            for cell in row.cells() {
                out.push(cell.id());
            }
        }
        out
    }

    pub fn contains(&self, id: &str) -> bool {
        self.locate(id).is_some()
    }

    /// Row and column of `id`.
    pub fn locate(&self, id: &str) -> Option<(usize, usize)> {
        for (r, row) in self.rows().into_iter().enumerate() {
            for (c, cell) in row.cells().into_iter().enumerate() {
                if cell.id() == id {
                    return Some((r, c));
                }
            }
        }
        None
    }

    /// The cell at a position, if there is one.
    pub fn cell(&self, row: usize, col: usize) -> Option<&FocusCell> {
        self.row(row).and_then(|r| r.get(col))
    }
}

/// Where focus is, as a value a caller can store and compare.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FocusPoint {
    pub id: String,
    /// Which item inside a rail. Always 0 for an ordinary control.
    pub strip_index: usize,
}

/// Where focus is, borrowed, with the rail length that bounds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Focused<'a> {
    pub id: &'a str,
    pub strip_index: usize,
    pub strip_len: usize,
}

/// Focus, and the rules for walking it.
///
/// Owns its grid so there is no lifetime between the layout and the position,
/// and so replacing the layout and repairing the position are one operation.
/// Those two drifting apart is the bug: a list re-renders after an install
/// finishes, the grid is swapped, and focus is left pointing at a row index that
/// now means something else.
#[derive(Debug, Clone)]
pub struct FocusCursor {
    grid: FocusGrid,
    row: usize,
    col: usize,
    /// The column the player last chose horizontally.
    ///
    /// Kept separately from `col` so moving down through a one cell row and out
    /// the other side returns to the column they were in. Without it, one narrow
    /// row permanently pins the player to the left edge of everything below it.
    desired_col: usize,
    /// Position inside each rail, by cell id.
    ///
    /// Remembered so leaving a rail and coming back lands on the same card. A
    /// rail that resets to card one every time you glance away cannot be read.
    strip_at: BTreeMap<String, usize>,
}

impl FocusCursor {
    /// Start on the grid's declared default.
    pub fn new(grid: FocusGrid) -> Self {
        // `FocusGrid::new` refuses a default that is not present, so this
        // resolves for any grid that exists. The fallback is the first cell,
        // which is a real cell rather than nothing.
        let (row, col) = grid.locate(grid.default_focus()).unwrap_or((0, 0));
        Self {
            grid,
            row,
            col,
            desired_col: col,
            strip_at: BTreeMap::new(),
        }
    }

    pub fn grid(&self) -> &FocusGrid {
        &self.grid
    }

    fn cell(&self) -> &FocusCell {
        self.grid.row_at(self.row).at(self.col)
    }

    /// The strip index of the focused cell, clamped into range.
    ///
    /// Clamped on read as well as on write because a grid can be replaced by one
    /// whose rail is shorter, and reading a remembered index of 4 out of a rail
    /// that now holds 2 cards is exactly the "focus on nothing" case.
    fn strip_index(&self) -> usize {
        let cell = self.cell();
        let last = cell.strip_len.saturating_sub(1);
        self.strip_at.get(cell.id()).copied().unwrap_or(0).min(last)
    }

    pub fn focused(&self) -> Focused<'_> {
        let cell = self.cell();
        Focused {
            id: cell.id(),
            strip_index: self.strip_index(),
            strip_len: cell.strip_len(),
        }
    }

    pub fn point(&self) -> FocusPoint {
        let f = self.focused();
        FocusPoint {
            id: f.id.to_string(),
            strip_index: f.strip_index,
        }
    }

    pub fn id(&self) -> &str {
        self.cell().id()
    }

    /// Move one step. Returns where focus came from and went to, or `None` when
    /// nothing moved because the edge clamped.
    ///
    /// Focus is never left unset: every path either lands on a real cell or
    /// leaves the cursor exactly where it was.
    pub fn step(&mut self, direction: Direction) -> Option<(FocusPoint, FocusPoint)> {
        let from = self.point();
        let moved = if direction.is_horizontal() && self.cell().is_strip() {
            self.step_inside_strip(direction)
        } else if direction.is_horizontal() {
            self.step_across_row(direction)
        } else {
            self.step_between_rows(direction)
        };
        if moved {
            Some((from, self.point()))
        } else {
            None
        }
    }

    /// Left and right inside a rail.
    ///
    /// A rail never hands focus sideways to its neighbours, per focus order
    /// exception 2. That is not a focus trap: up and down still leave it, which
    /// the tests hold to.
    fn step_inside_strip(&mut self, direction: Direction) -> bool {
        let len = self.cell().strip_len();
        let id = self.cell().id().to_string();
        let index = self.strip_index();
        let next = match (direction, self.grid.wrap.horizontal) {
            (Direction::Right, _) if index + 1 < len => index + 1,
            (Direction::Right, Wrap::Around) => 0,
            (Direction::Left, _) if index > 0 => index - 1,
            (Direction::Left, Wrap::Around) => len.saturating_sub(1),
            _ => return false,
        };
        if next == index {
            return false;
        }
        self.strip_at.insert(id, next);
        true
    }

    fn step_across_row(&mut self, direction: Direction) -> bool {
        let len = self.grid.row_at(self.row).len();
        let next = match (direction, self.grid.wrap.horizontal) {
            (Direction::Right, _) if self.col + 1 < len => self.col + 1,
            (Direction::Right, Wrap::Around) => 0,
            (Direction::Left, _) if self.col > 0 => self.col - 1,
            (Direction::Left, Wrap::Around) => len.saturating_sub(1),
            _ => return false,
        };
        if next == self.col {
            return false;
        }
        self.col = next;
        self.desired_col = next;
        true
    }

    fn step_between_rows(&mut self, direction: Direction) -> bool {
        let rows = self.grid.row_count();
        let next = match (direction, self.grid.wrap.vertical) {
            (Direction::Down, _) if self.row + 1 < rows => self.row + 1,
            (Direction::Down, Wrap::Around) => 0,
            (Direction::Up, _) if self.row > 0 => self.row - 1,
            (Direction::Up, Wrap::Around) => rows.saturating_sub(1),
            _ => return false,
        };
        if next == self.row {
            return false;
        }
        self.row = next;
        // The desired column survives the move; only the effective one clamps.
        self.col = self
            .desired_col
            .min(self.grid.row_at(next).len().saturating_sub(1));
        true
    }

    /// Put focus on a named cell, for focus restoration when returning from a
    /// detail view. Returns false and changes nothing if the cell is gone.
    pub fn restore(&mut self, id: &str) -> bool {
        match self.grid.locate(id) {
            Some((row, col)) => {
                self.row = row;
                self.col = col;
                self.desired_col = col;
                true
            }
            None => false,
        }
    }

    /// Put focus inside a rail, for restoring the exact card that was opened.
    ///
    /// Clamped rather than refused: a rail that lost items between opening a
    /// card and closing it should land on the nearest card that still exists,
    /// not on nothing and not back at the start.
    pub fn restore_in_strip(&mut self, id: &str, strip_index: usize) -> bool {
        if !self.restore(id) {
            return false;
        }
        let last = self.cell().strip_len().saturating_sub(1);
        self.strip_at.insert(id.to_string(), strip_index.min(last));
        true
    }

    /// Swap in a re-rendered layout, keeping focus on the same item.
    ///
    /// This is the one that stops a background download finishing from throwing
    /// the player back to the top of the page. If the focused id survives, focus
    /// stays on it; if it does not, focus goes to the new grid's declared
    /// default, never to index 0 by accident.
    ///
    /// Remembered rail positions for cells that no longer exist are dropped, and
    /// the ones that survive are clamped to the new lengths.
    pub fn replace_grid(&mut self, grid: FocusGrid) {
        let previous = self.id().to_string();
        self.grid = grid;
        self.strip_at.retain(|id, _| self.grid.contains(id));
        let clamps: Vec<(String, usize)> = self
            .strip_at
            .iter()
            .filter_map(|(id, index)| {
                let (row, col) = self.grid.locate(id)?;
                let len = self.grid.cell(row, col)?.strip_len();
                let clamped = (*index).min(len.saturating_sub(1));
                (clamped != *index).then_some((id.clone(), clamped))
            })
            .collect();
        for (id, clamped) in clamps {
            self.strip_at.insert(id, clamped);
        }

        if !self.restore(&previous) {
            let default = self.grid.default_focus().to_string();
            if !self.restore(&default) {
                // Unreachable for a grid built by `FocusGrid::new`, which
                // refuses an absent default. Landing on the first cell is still
                // a real cell, which is the invariant that matters.
                self.row = 0;
                self.col = 0;
                self.desired_col = 0;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Held direction repeat
// ---------------------------------------------------------------------------

/// Floor on the repeat interval, about 25 steps a second.
///
/// A configuration file can hold anything, including zero, and a zero interval
/// means one step per poll: at 500 polls a second the focus ring crosses the
/// page before the player lets go. The floor is applied on the way in, so a bad
/// value degrades to fast rather than to uncontrollable.
pub const MIN_REPEAT_INTERVAL_MS: u32 = 40;
pub const MAX_REPEAT_INTERVAL_MS: u32 = 1_000;
/// Floor on the initial delay. Below this a single deliberate flick double
/// steps, which makes precise single moves impossible.
pub const MIN_REPEAT_DELAY_MS: u32 = 120;
pub const MAX_REPEAT_DELAY_MS: u32 = 1_000;

/// How a held direction repeats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepeatTiming {
    /// Wait after the first step before repeating at all.
    pub initial_delay_ms: u32,
    /// Gap between repeats once they start.
    pub interval_ms: u32,
}

impl Default for RepeatTiming {
    /// 400 ms then every 90 ms.
    ///
    /// Neither number is in the design documents, so both are argued rather than
    /// asserted:
    ///
    /// 400 ms initial delay, because the common action is a single step and a
    /// shorter delay turns one deliberate flick into two moves. 400 ms is longer
    /// than a flick and shorter than a hold anybody would call unresponsive.
    ///
    /// 90 ms interval, matching the `motion-instant` token of 90 ms, so the focus
    /// ring has finished arriving before the next step starts. Faster than the
    /// ring and the player is chasing a blur. That gives about 11 steps a second,
    /// which crosses the roughly 20 row Settings page in under two seconds.
    ///
    /// The first step is emitted on the press itself, not after the delay,
    /// because the alternative is a pad that feels dead for 400 ms.
    fn default() -> Self {
        Self {
            initial_delay_ms: 400,
            interval_ms: 90,
        }
    }
}

impl RepeatTiming {
    /// Bring a loaded or hand written value into a range that is usable.
    pub fn clamped(self) -> Self {
        Self {
            initial_delay_ms: self
                .initial_delay_ms
                .clamp(MIN_REPEAT_DELAY_MS, MAX_REPEAT_DELAY_MS),
            interval_ms: self
                .interval_ms
                .clamp(MIN_REPEAT_INTERVAL_MS, MAX_REPEAT_INTERVAL_MS),
        }
    }
}

/// The gap between polls above which a hold is no longer considered observed.
///
/// About fifteen frames at 60 Hz. Used by the repeater to refuse to catch up,
/// and by [`HoldToConfirm`] to refuse to credit time it did not watch.
pub const STALL_GAP_MS: u64 = 250;

#[derive(Debug, Clone, Copy)]
struct Held {
    direction: Direction,
    last_step_ms: u64,
    repeating: bool,
}

/// Turns a held direction into discrete steps.
///
/// Time is a parameter, so the behaviour is testable without sleeping. This is
/// the whole reason it is not a timer inside a view: "does a held stick repeat at
/// the rate we said" is a question about arithmetic, and it should be answered by
/// a test rather than by holding a stick and counting.
#[derive(Debug, Clone)]
pub struct DirectionRepeater {
    timing: RepeatTiming,
    held: Option<Held>,
}

impl DirectionRepeater {
    pub fn new(timing: RepeatTiming) -> Self {
        Self {
            timing: timing.clamped(),
            held: None,
        }
    }

    pub fn timing(&self) -> RepeatTiming {
        self.timing
    }

    pub fn held(&self) -> Option<Direction> {
        self.held.map(|h| h.direction)
    }

    /// Report the direction currently held, and take at most one step.
    ///
    /// At most one, deliberately. If the launcher was not polled for a second,
    /// the honest answer is not eleven steps: the player held the stick for a
    /// second, but the eleven steps would all land in one frame and the focus
    /// ring would appear somewhere unrelated with no motion in between. So the
    /// schedule is re-seated at the poll time instead of accumulating.
    ///
    /// Changing direction restarts the delay. Flicking from down to right must
    /// not inherit the fast repeat that the downward hold had earned.
    pub fn update(&mut self, direction: Option<Direction>, now_ms: u64) -> Option<Direction> {
        let Some(direction) = direction else {
            self.held = None;
            return None;
        };

        match self.held {
            Some(mut held) if held.direction == direction => {
                if now_ms < held.last_step_ms {
                    // A clock that went backwards, from a system time change.
                    // Re-seat rather than wait for it to catch up, which could
                    // stall navigation for as long as the jump.
                    held.last_step_ms = now_ms;
                    self.held = Some(held);
                    return None;
                }
                let wait = if held.repeating {
                    u64::from(self.timing.interval_ms)
                } else {
                    u64::from(self.timing.initial_delay_ms)
                };
                if now_ms.saturating_sub(held.last_step_ms) >= wait {
                    held.last_step_ms = now_ms;
                    held.repeating = true;
                    self.held = Some(held);
                    Some(direction)
                } else {
                    self.held = Some(held);
                    None
                }
            }
            _ => {
                self.held = Some(Held {
                    direction,
                    last_step_ms: now_ms,
                    repeating: false,
                });
                Some(direction)
            }
        }
    }

    /// Forget the held direction, so the next sample counts as a fresh press.
    ///
    /// Called when the screen changes: a stick still held through a page change
    /// should step once on arrival, not resume mid-repeat on a page the player
    /// has not seen yet.
    pub fn release(&mut self) {
        self.held = None;
    }
}

// ---------------------------------------------------------------------------
// Sticks and the deadzone
// ---------------------------------------------------------------------------

/// One analog stick, in the range -1 to 1, y positive up.
///
/// Y positive up matches how the emulator names the axis it binds, `Left Y+` for
/// up, so the two halves of the product do not disagree about which way a stick
/// points.
///
/// Fields are private because they are sanitized on the way in. A backend that
/// reports NaN, which happens with a disconnecting wireless pad, would otherwise
/// make every comparison false and silently freeze navigation with no error.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stick {
    x: f32,
    y: f32,
}

impl Stick {
    pub const CENTER: Stick = Stick { x: 0.0, y: 0.0 };

    /// NaN becomes centre, and out of range values clamp.
    pub fn new(x: f32, y: f32) -> Self {
        Self {
            x: sanitize_axis(x),
            y: sanitize_axis(y),
        }
    }

    pub fn x(&self) -> f32 {
        self.x
    }

    pub fn y(&self) -> f32 {
        self.y
    }

    pub fn magnitude(&self) -> f32 {
        (self.x * self.x + self.y * self.y).sqrt().min(1.0)
    }
}

impl Default for Stick {
    fn default() -> Self {
        Stick::CENTER
    }
}

fn sanitize_axis(value: f32) -> f32 {
    if value.is_nan() {
        0.0
    } else {
        value.clamp(-1.0, 1.0)
    }
}

/// The largest deadzone the launcher will honour.
///
/// `Tuning::deadzone_percent` is a `u8` the player sets for the game, where 100
/// is a strange but legitimate choice. Applied literally to launcher navigation
/// it would make the pad useless, and the pad is how they would get back into
/// Settings to undo it. So it is capped, and the cap is stated here rather than
/// buried in an expression.
pub const MAX_EFFECTIVE_DEADZONE_PERCENT: u8 = 90;

/// Fraction of the travel beyond the deadzone that a discrete step needs.
///
/// A focus step is not analog aiming: it needs a deliberate push, or a worn
/// stick resting off centre walks the focus ring on its own while the player
/// reads the screen.
const ENGAGE_FRACTION: f32 = 0.25;
/// Where an engaged direction lets go. Lower than the engage threshold on
/// purpose: a stick parked exactly on one threshold would otherwise chatter, and
/// a chattering direction reads as a fresh press every few frames, which either
/// double steps or resets the repeat delay forever and looks like a stall.
const RELEASE_FRACTION: f32 = 0.15;
/// How much the other axis has to beat the engaged one to take over.
///
/// A hold at almost exactly 45 degrees would otherwise alternate down, right,
/// down, right, and walk the focus diagonally across the page.
const AXIS_STICKINESS: f32 = 1.3;

/// The deadzone the launcher will apply, from the player's own tuning.
pub fn effective_deadzone(tuning: &Tuning) -> f32 {
    f32::from(tuning.deadzone_percent.min(MAX_EFFECTIVE_DEADZONE_PERCENT)) / 100.0
}

/// Turns stick position into a held direction, with hysteresis.
#[derive(Debug, Clone, Default)]
pub struct StickGate {
    engaged: Option<Direction>,
}

impl StickGate {
    pub fn engaged(&self) -> Option<Direction> {
        self.engaged
    }

    /// The direction this stick is currently asking for, if any.
    pub fn direction(&mut self, stick: Stick, tuning: &Tuning) -> Option<Direction> {
        let deadzone = effective_deadzone(tuning);
        let remaining = 1.0 - deadzone;
        let engage = deadzone + remaining * ENGAGE_FRACTION;
        let release = deadzone + remaining * RELEASE_FRACTION;
        let magnitude = stick.magnitude();

        let threshold = if self.engaged.is_some() {
            release
        } else {
            engage
        };
        if magnitude < threshold {
            self.engaged = None;
            return None;
        }

        let (ax, ay) = (stick.x().abs(), stick.y().abs());
        let vertical = match self.engaged {
            Some(current) if current.is_vertical() => ax <= ay * AXIS_STICKINESS,
            Some(_) => ay > ax * AXIS_STICKINESS,
            // A dead heat goes vertical: the launcher's lists are vertical, so
            // that is the move the player more often meant.
            None => ay >= ax,
        };
        let direction = if vertical {
            if stick.y() > 0.0 {
                Direction::Up
            } else {
                Direction::Down
            }
        } else if stick.x() > 0.0 {
            Direction::Right
        } else {
            Direction::Left
        };
        self.engaged = Some(direction);
        Some(direction)
    }

    /// Forget the engaged direction, so the stick has to cross the engage
    /// threshold again.
    pub fn release(&mut self) {
        self.engaged = None;
    }
}

/// Right stick scroll speed, as a signed fraction of full deflection.
///
/// Positive means toward the end of the list, which is what pushing the stick
/// down should do. Absent inside the deadzone, so the caller can stop its scroll
/// loop instead of integrating zeroes forever.
///
/// The value is rescaled so that it starts at zero where the deadzone ends. The
/// naive version, passing the raw axis through, makes the first perceptible
/// movement a jump straight to 15 percent speed, which is the difference between
/// a scroll you can place and one you fight.
///
/// Vertical only. The content region scrolls vertically; horizontal rails are
/// walked by focus, and a horizontal scroll axis would fight the focus model for
/// the same input.
pub fn scroll_axis(stick: Stick, tuning: &Tuning) -> Option<f32> {
    let deadzone = effective_deadzone(tuning);
    let magnitude = stick.y().abs();
    if magnitude <= deadzone {
        return None;
    }
    let span = (1.0 - deadzone).max(f32::EPSILON);
    let scaled = ((magnitude - deadzone) / span).clamp(0.0, 1.0);
    Some(if stick.y() > 0.0 { -scaled } else { scaled })
}

// ---------------------------------------------------------------------------
// The shortcut table
// ---------------------------------------------------------------------------

/// A button on the pad, named for its function rather than its glyph.
///
/// Named by function because the glyph depends on the hardware: the same button
/// is A on an Xbox pad and Cross on a PlayStation one. Storing the glyph would
/// mean the shortcut table changes when the player swaps pads.
///
/// Ordering matters: it is the order events are emitted in when several buttons
/// go down in the same sample, and a deterministic order is easier to reason
/// about than whichever the driver happened to list first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PadButton {
    A,
    B,
    X,
    Y,
    LeftBumper,
    RightBumper,
    LeftTrigger,
    RightTrigger,
    View,
    Menu,
    /// The Xbox or PlayStation button. Reserved to the operating system and
    /// bound to nothing here, in every context.
    Guide,
    DPadUp,
    DPadDown,
    DPadLeft,
    DPadRight,
}

impl PadButton {
    pub const ALL: &'static [PadButton] = &[
        PadButton::A,
        PadButton::B,
        PadButton::X,
        PadButton::Y,
        PadButton::LeftBumper,
        PadButton::RightBumper,
        PadButton::LeftTrigger,
        PadButton::RightTrigger,
        PadButton::View,
        PadButton::Menu,
        PadButton::Guide,
        PadButton::DPadUp,
        PadButton::DPadDown,
        PadButton::DPadLeft,
        PadButton::DPadRight,
    ];

    pub fn is_dpad(self) -> bool {
        self.as_direction().is_some()
    }

    pub fn as_direction(self) -> Option<Direction> {
        match self {
            PadButton::DPadUp => Some(Direction::Up),
            PadButton::DPadDown => Some(Direction::Down),
            PadButton::DPadLeft => Some(Direction::Left),
            PadButton::DPadRight => Some(Direction::Right),
            _ => None,
        }
    }

    /// The name to print in the hint strip.
    ///
    /// Only two glyph sets are named in the design documents, Xbox and
    /// PlayStation, so only those two are distinguished. Every other family falls
    /// back to the Xbox names, which is the same fallback
    /// `DeviceFamily::prompt_pack` already makes for texture art.
    ///
    /// That fallback is wrong for two families and is recorded rather than
    /// papered over: a Nintendo pad has A and B swapped physically, and a
    /// GameCube pad has a different face layout again. Showing Xbox names there
    /// is a known gap, not a claim.
    pub fn label(self, family: DeviceFamily) -> &'static str {
        let sony = matches!(family, DeviceFamily::PlayStation);
        match (self, sony) {
            (PadButton::A, false) => "A",
            (PadButton::A, true) => "Cross",
            (PadButton::B, false) => "B",
            (PadButton::B, true) => "Circle",
            (PadButton::X, false) => "X",
            (PadButton::X, true) => "Square",
            (PadButton::Y, false) => "Y",
            (PadButton::Y, true) => "Triangle",
            (PadButton::LeftBumper, false) => "LB",
            (PadButton::LeftBumper, true) => "L1",
            (PadButton::RightBumper, false) => "RB",
            (PadButton::RightBumper, true) => "R1",
            (PadButton::LeftTrigger, false) => "LT",
            (PadButton::LeftTrigger, true) => "L2",
            (PadButton::RightTrigger, false) => "RT",
            (PadButton::RightTrigger, true) => "R2",
            (PadButton::View, false) => "View",
            (PadButton::View, true) => "Share",
            (PadButton::Menu, false) => "Menu",
            (PadButton::Menu, true) => "Options",
            (PadButton::Guide, false) => "Guide",
            (PadButton::Guide, true) => "PS",
            (PadButton::DPadUp, _) => "D-pad up",
            (PadButton::DPadDown, _) => "D-pad down",
            (PadButton::DPadLeft, _) => "D-pad left",
            (PadButton::DPadRight, _) => "D-pad right",
        }
    }
}

/// Which of the two contextual slots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ContextualSlot {
    /// X or Square. The secondary action, labelled in the hint strip.
    X,
    /// Y or Triangle. The tertiary action, labelled in the hint strip.
    Y,
}

impl ContextualSlot {
    pub fn button(self) -> PadButton {
        match self {
            ContextualSlot::X => PadButton::X,
            ContextualSlot::Y => PadButton::Y,
        }
    }
}

/// Previous or next, for sections and pages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Previous,
    Next,
}

/// What is on screen, which decides which shortcuts are live.
///
/// The four modal kinds are the ones in the information architecture, and they
/// differ here only in what they let through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputContext {
    /// An ordinary page. Everything is available.
    Page,
    /// Content over the page: a package detail, a box score, a remap screen.
    Sheet,
    /// A decision with two or three outcomes.
    Dialog,
    /// A destructive decision. Dismissed only explicitly, and its primary is
    /// gated by [`HoldToConfirm`].
    Confirm,
    /// Not dismissible: a required update, or first run preparation.
    Blocking,
}

impl InputContext {
    pub fn is_modal(self) -> bool {
        !matches!(self, InputContext::Page)
    }
}

/// What a button means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavAction {
    Activate,
    Back,
    Contextual(ContextualSlot),
    /// Previous or next L1 section.
    Section(Step),
    /// Previous or next L2 page inside the section.
    Page(Step),
    MoveFocus(Direction),
    OpenSearch,
    /// The overflow menu for the focused item, which is where destructive
    /// actions live.
    OpenOverflow,
}

/// The global shortcut table.
///
/// One function, so there is exactly one answer to "what does this button do"
/// and the hint strip and the handler cannot disagree.
///
/// What a modal blocks, and why:
///
/// - **Shoulders and triggers.** Changing section or page underneath an open
///   modal leaves the modal describing a page nobody can see, and B then returns
///   the player somewhere they never were.
/// - **View and Menu.** Search and overflow act on the page behind the scrim.
///   Only one modal is allowed at a time, so they cannot open either.
/// - **X and Y.** Contextual actions belong to the focused page item, which is
///   behind the scrim as well.
/// - **B in a blocking modal.** Nothing, because a blocking modal is not
///   dismissible. The hint strip omits Back rather than showing a dead button.
///
/// What no context permits is [`PadButton::Guide`], which belongs to the
/// operating system.
///
/// One rule this cannot express, because it is per screen rather than per
/// context: B at an L1 section root does nothing. The caller owns that, since
/// only the caller knows what level it is showing.
pub fn action_for(button: PadButton, context: InputContext) -> Option<NavAction> {
    if let Some(direction) = button.as_direction() {
        return Some(NavAction::MoveFocus(direction));
    }
    match button {
        PadButton::Guide => None,
        PadButton::A => Some(NavAction::Activate),
        PadButton::B => match context {
            InputContext::Blocking => None,
            _ => Some(NavAction::Back),
        },
        PadButton::X => (!context.is_modal()).then_some(NavAction::Contextual(ContextualSlot::X)),
        PadButton::Y => (!context.is_modal()).then_some(NavAction::Contextual(ContextualSlot::Y)),
        PadButton::LeftBumper => {
            (!context.is_modal()).then_some(NavAction::Section(Step::Previous))
        }
        PadButton::RightBumper => (!context.is_modal()).then_some(NavAction::Section(Step::Next)),
        PadButton::LeftTrigger => (!context.is_modal()).then_some(NavAction::Page(Step::Previous)),
        PadButton::RightTrigger => (!context.is_modal()).then_some(NavAction::Page(Step::Next)),
        PadButton::View => (!context.is_modal()).then_some(NavAction::OpenSearch),
        PadButton::Menu => (!context.is_modal()).then_some(NavAction::OpenOverflow),
        PadButton::DPadUp | PadButton::DPadDown | PadButton::DPadLeft | PadButton::DPadRight => {
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Destructive actions and hold-to-confirm
// ---------------------------------------------------------------------------

/// The complete list of things that may be gated behind a hold.
///
/// Closed on purpose. The specification says hold-to-confirm is used only for
/// these five and "nothing else", and an open ended `bool` would let a hold gate
/// spread onto every button in the product, which trains players to hold
/// everything and makes the gate meaningless where it matters.
///
/// Adding a sixth is a deliberate edit here, next to this paragraph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Destructive {
    DeleteMod,
    RestoreSoundtrack,
    ResetSettingsGroup,
    FormatUsbDrive,
    DeleteAccountData,
}

impl Destructive {
    pub const ALL: &'static [Destructive] = &[
        Destructive::DeleteMod,
        Destructive::RestoreSoundtrack,
        Destructive::ResetSettingsGroup,
        Destructive::FormatUsbDrive,
        Destructive::DeleteAccountData,
    ];

    /// The wording from the specification, so the product and the document say
    /// the same thing.
    pub fn describe(self) -> &'static str {
        match self {
            Destructive::DeleteMod => "delete a mod",
            Destructive::RestoreSoundtrack => "restore the entire soundtrack",
            Destructive::ResetSettingsGroup => "reset a settings group",
            Destructive::FormatUsbDrive => "format a USB drive",
            Destructive::DeleteAccountData => "delete account data",
        }
    }
}

/// Something the launcher can be asked to do from a pad.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LauncherAction {
    label: String,
    destructive: Option<Destructive>,
}

impl LauncherAction {
    pub fn safe(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            destructive: None,
        }
    }

    pub fn destructive(label: impl Into<String>, what: Destructive) -> Self {
        Self {
            label: label.into(),
            destructive: Some(what),
        }
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn is_destructive(&self) -> bool {
        self.destructive.is_some()
    }

    /// The hold gate for this action, if it needs one.
    ///
    /// `None` for a safe action, and that is the point: a hold gate cannot be
    /// attached to something harmless, so the gesture keeps meaning "this one is
    /// dangerous".
    pub fn hold(&self) -> Option<HoldToConfirm> {
        self.destructive.map(HoldToConfirm::new)
    }
}

/// Why a contextual binding was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BindRefusal {
    /// X and Y are never destructive.
    Destructive {
        slot: ContextualSlot,
        label: String,
        what: Destructive,
    },
}

impl BindRefusal {
    /// A sentence naming what to do instead.
    pub fn message(&self) -> String {
        match self {
            BindRefusal::Destructive { slot, label, what } => format!(
                "\"{label}\" would {}, so it cannot go on {:?} as a contextual \
                 action. X and Y sit one press from arrival with no confirmation \
                 step, so they are never destructive. Put it in the Menu \
                 overflow instead, where it gets hold-to-confirm.",
                what.describe(),
                slot.button(),
            ),
        }
    }
}

/// The two contextual slots for the current screen.
///
/// Unbound is the default and stays visible as unbound: the hint strip shows a
/// label only when there is a real action behind it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContextualBindings {
    x: Option<LauncherAction>,
    y: Option<LauncherAction>,
}

impl ContextualBindings {
    /// Bind a slot, refusing anything destructive.
    pub fn bind(
        &mut self,
        slot: ContextualSlot,
        action: LauncherAction,
    ) -> std::result::Result<(), BindRefusal> {
        if let Some(what) = action.destructive {
            return Err(BindRefusal::Destructive {
                slot,
                label: action.label,
                what,
            });
        }
        match slot {
            ContextualSlot::X => self.x = Some(action),
            ContextualSlot::Y => self.y = Some(action),
        }
        Ok(())
    }

    pub fn clear(&mut self, slot: ContextualSlot) {
        match slot {
            ContextualSlot::X => self.x = None,
            ContextualSlot::Y => self.y = None,
        }
    }

    pub fn action(&self, slot: ContextualSlot) -> Option<&LauncherAction> {
        match slot {
            ContextualSlot::X => self.x.as_ref(),
            ContextualSlot::Y => self.y.as_ref(),
        }
    }

    /// The label for a slot, absent when nothing is bound.
    pub fn label(&self, slot: ContextualSlot) -> Option<&str> {
        self.action(slot).map(|a| a.label())
    }

    /// Button name and label for each bound slot, for the hint strip.
    ///
    /// Only bound slots appear. An unbound slot contributes nothing rather than a
    /// plausible placeholder, because the hint strip is the only place bindings
    /// are stated and a wrong one there is worse than a missing one.
    pub fn hints(&self, family: DeviceFamily) -> Vec<(&'static str, &str)> {
        [ContextualSlot::X, ContextualSlot::Y]
            .into_iter()
            .filter_map(|slot| {
                self.label(slot)
                    .map(|label| (slot.button().label(family), label))
            })
            .collect()
    }
}

/// How long a destructive action has to be held. From the specification, and
/// deliberately not configurable: a view that shortened it would be weakening a
/// safety gate locally.
pub const HOLD_TO_CONFIRM_MS: u32 = 800;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HoldPhase {
    /// The button has not been seen released yet, so a press cannot be
    /// attributed to a choice the player made about this dialog.
    Unarmed,
    Idle,
    Holding {
        since_ms: u64,
    },
    Fired,
}

/// What an observation did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HoldEvent {
    Nothing,
    Started,
    /// The hold was abandoned because the launcher stopped watching. The
    /// button is still down and the ring starts again from empty.
    Restarted {
        discarded_ms: u64,
    },
    /// The player let go early.
    Cancelled {
        held_ms: u64,
    },
    Fired,
}

/// The 800 ms hold in front of a destructive action, as a state machine.
///
/// A state machine rather than a timer in a view because the interesting cases
/// are all about time the view would not notice:
///
/// - The button was **already down** when the confirm opened, because it is the
///   same button that opened it. Credited, that deletes a mod with one press.
///   This machine starts unarmed and needs to see the button up first.
/// - The launcher **stopped sampling** mid-hold. Wall clock time passed, but
///   nobody watched the button, so the hold is restarted rather than credited.
///   The player also saw no filling ring during that gap, which means firing
///   would contradict the only feedback they had.
/// - The clock **went backwards**. Treated as a stall, for the same reason.
///
/// Feed it one call per frame with the button state. One entry point, because
/// separate press, poll and release methods can be called in the wrong order and
/// the wrong order is where a gate leaks.
#[derive(Debug, Clone)]
pub struct HoldToConfirm {
    what: Destructive,
    required_ms: u32,
    phase: HoldPhase,
    last_seen_ms: Option<u64>,
}

impl HoldToConfirm {
    pub fn new(what: Destructive) -> Self {
        Self {
            what,
            required_ms: HOLD_TO_CONFIRM_MS,
            phase: HoldPhase::Unarmed,
            last_seen_ms: None,
        }
    }

    pub fn what(&self) -> Destructive {
        self.what
    }

    pub fn required_ms(&self) -> u32 {
        self.required_ms
    }

    pub fn has_fired(&self) -> bool {
        matches!(self.phase, HoldPhase::Fired)
    }

    pub fn is_holding(&self) -> bool {
        matches!(self.phase, HoldPhase::Holding { .. })
    }

    /// How full the ring is, 0 to 1, absent when nothing is being held.
    ///
    /// Absent rather than 0.0 so a view cannot draw an empty ring on a control
    /// nobody is touching, which reads as "this needs a hold" on every item.
    pub fn progress(&self, now_ms: u64) -> Option<f32> {
        match self.phase {
            HoldPhase::Holding { since_ms } => {
                let held = now_ms.saturating_sub(since_ms) as f32;
                Some((held / self.required_ms.max(1) as f32).clamp(0.0, 1.0))
            }
            _ => None,
        }
    }

    /// Report the button state for this frame.
    pub fn observe(&mut self, down: bool, now_ms: u64) -> HoldEvent {
        let stalled = match self.last_seen_ms {
            Some(previous) => now_ms < previous || now_ms - previous > STALL_GAP_MS,
            None => false,
        };
        self.last_seen_ms = Some(now_ms);

        match self.phase {
            HoldPhase::Unarmed => {
                if !down {
                    self.phase = HoldPhase::Idle;
                }
                HoldEvent::Nothing
            }
            HoldPhase::Idle => {
                if down {
                    self.phase = HoldPhase::Holding { since_ms: now_ms };
                    HoldEvent::Started
                } else {
                    HoldEvent::Nothing
                }
            }
            HoldPhase::Holding { since_ms } => {
                let held = now_ms.saturating_sub(since_ms);
                if !down {
                    self.phase = HoldPhase::Idle;
                    return HoldEvent::Cancelled { held_ms: held };
                }
                if stalled {
                    self.phase = HoldPhase::Holding { since_ms: now_ms };
                    return HoldEvent::Restarted { discarded_ms: held };
                }
                if held >= u64::from(self.required_ms) {
                    self.phase = HoldPhase::Fired;
                    return HoldEvent::Fired;
                }
                HoldEvent::Nothing
            }
            // Terminal. The action has happened, so a machine that keeps being
            // fed must not fire twice; a second confirmation is a second
            // machine, or an explicit reset.
            HoldPhase::Fired => HoldEvent::Nothing,
        }
    }

    /// Make this machine usable again, unarmed, so the button has to be released
    /// and pressed once more.
    pub fn reset(&mut self) {
        self.phase = HoldPhase::Unarmed;
        self.last_seen_ms = None;
    }
}

// ---------------------------------------------------------------------------
// Samples, events, and the navigator
// ---------------------------------------------------------------------------

/// The pad state for one frame, as a front end reports it.
///
/// A state sample rather than a stream of press and release messages, because
/// that is what every backend can supply honestly and because it makes a lost
/// message impossible: if a release is dropped from a queue, a button stays down
/// forever, and the launcher believes A is held for the rest of the session.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PadSample {
    pub left_stick: Stick,
    pub right_stick: Stick,
    /// Every button currently down.
    pub buttons: BTreeSet<PadButton>,
}

impl PadSample {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_button(mut self, button: PadButton) -> Self {
        self.buttons.insert(button);
        self
    }

    pub fn with_left(mut self, stick: Stick) -> Self {
        self.left_stick = stick;
        self
    }

    pub fn with_right(mut self, stick: Stick) -> Self {
        self.right_stick = stick;
        self
    }

    pub fn is_down(&self, button: PadButton) -> bool {
        self.buttons.contains(&button)
    }

    /// The direction the d-pad is asking for.
    ///
    /// Real hardware cannot report up and down together, but a remapper or a
    /// broken driver can. Resolved by a fixed priority rather than by whichever
    /// arrives first, because alternating between two answers on consecutive
    /// frames would restart the repeat delay forever and look like a dead pad.
    pub fn dpad(&self) -> Option<Direction> {
        for button in [
            PadButton::DPadUp,
            PadButton::DPadDown,
            PadButton::DPadLeft,
            PadButton::DPadRight,
        ] {
            if self.is_down(button) {
                return button.as_direction();
            }
        }
        None
    }
}

/// Something the launcher should do about the pad.
///
/// Carries the focused id with the actions that need one. Reading focus
/// separately afterwards is a race: a single sample can activate an item and then
/// move focus, and by the time the caller looks, focus is somewhere else.
#[derive(Debug, Clone, PartialEq)]
pub enum PadEvent {
    Activate {
        id: String,
        strip_index: usize,
    },
    Back,
    Contextual {
        slot: ContextualSlot,
        id: String,
    },
    Section(Step),
    Page(Step),
    OpenSearch,
    Overflow {
        id: String,
        strip_index: usize,
    },
    FocusMoved {
        from: FocusPoint,
        to: FocusPoint,
    },
    /// Movement was refused by the edge of the grid. Emitted once per hold, not
    /// at the repeat rate, so a caller that plays a sound does not machine gun
    /// it while the player leans on the stick.
    FocusBlocked(Direction),
    /// Right stick scroll, as a signed fraction of full speed. Positive is
    /// toward the end of the list.
    Scroll {
        fraction: f32,
    },
}

/// The pad, driving one screen.
///
/// Owns the focus cursor, the repeat timing, the stick gate and the button edge
/// state, and turns a per frame sample into events. Everything with a clock in it
/// is in here rather than in a view, so all of it is testable by passing
/// timestamps.
///
/// Order within one update, and why:
///
/// 1. **Button presses, against the focus as it was.** A player who presses A and
///    pushes down in the same frame meant to activate the thing they were looking
///    at. Moving first would install the wrong mod.
/// 2. **Focus movement.**
/// 3. **Scroll.**
///
/// Events are returned in that order and the caller is expected to handle them in
/// order. A sample really can contain both A and B, and the caller decides what
/// happens when it does.
#[derive(Debug, Clone)]
pub struct PadNavigator {
    cursor: FocusCursor,
    repeater: DirectionRepeater,
    stick: StickGate,
    context: InputContext,
    tuning: Tuning,
    /// Buttons down at the previous sample.
    ///
    /// `None` means "not armed": the next sample is adopted as the baseline and
    /// produces no presses. That is what stops a button that was already held
    /// when the screen appeared from acting on the screen it revealed, which is
    /// the same class of bug the audit found in ready-up.
    down: Option<BTreeSet<PadButton>>,
    /// The direction an edge has already been reported for.
    edge_reported: Option<Direction>,
}

impl PadNavigator {
    pub fn new(grid: FocusGrid) -> Self {
        Self {
            cursor: FocusCursor::new(grid),
            repeater: DirectionRepeater::new(RepeatTiming::default()),
            stick: StickGate::default(),
            context: InputContext::Page,
            tuning: Tuning::default(),
            down: None,
            edge_reported: None,
        }
    }

    pub fn with_timing(mut self, timing: RepeatTiming) -> Self {
        self.repeater = DirectionRepeater::new(timing);
        self
    }

    /// Use this device's tuning, so the launcher honours the deadzone the player
    /// already set for the game.
    pub fn with_tuning(mut self, tuning: Tuning) -> Self {
        self.tuning = tuning;
        self
    }

    pub fn with_context(mut self, context: InputContext) -> Self {
        self.context = context;
        self
    }

    pub fn cursor(&self) -> &FocusCursor {
        &self.cursor
    }

    pub fn cursor_mut(&mut self) -> &mut FocusCursor {
        &mut self.cursor
    }

    pub fn focused(&self) -> Focused<'_> {
        self.cursor.focused()
    }

    pub fn context(&self) -> InputContext {
        self.context
    }

    pub fn timing(&self) -> RepeatTiming {
        self.repeater.timing()
    }

    /// Change what is on screen, and disarm.
    ///
    /// Opening a modal with A must not let the still held A press the modal's
    /// primary, and closing one must not let the still held B walk back another
    /// level. Both are one frame away without this.
    pub fn set_context(&mut self, context: InputContext) {
        self.context = context;
        self.disarm();
    }

    /// Swap in a re-rendered layout, keeping focus where it was, and disarm.
    pub fn replace_grid(&mut self, grid: FocusGrid) {
        self.cursor.replace_grid(grid);
        self.disarm();
    }

    /// Require every button to be released before it counts again, and treat a
    /// held direction as a fresh press.
    pub fn disarm(&mut self) {
        self.down = None;
        self.repeater.release();
        self.stick.release();
        self.edge_reported = None;
    }

    /// Feed one frame.
    pub fn update(&mut self, sample: &PadSample, now_ms: u64) -> Vec<PadEvent> {
        let mut events = Vec::new();
        let focused = self.cursor.point();

        if let Some(previous) = self.down.take() {
            for button in sample.buttons.difference(&previous) {
                // The d-pad goes through the repeater instead, so that holding it
                // repeats like the stick. Handling it here as well would emit a
                // second step for the same press.
                if button.is_dpad() {
                    continue;
                }
                match action_for(*button, self.context) {
                    Some(NavAction::Activate) => events.push(PadEvent::Activate {
                        id: focused.id.clone(),
                        strip_index: focused.strip_index,
                    }),
                    Some(NavAction::Back) => events.push(PadEvent::Back),
                    Some(NavAction::Contextual(slot)) => events.push(PadEvent::Contextual {
                        slot,
                        id: focused.id.clone(),
                    }),
                    Some(NavAction::Section(step)) => events.push(PadEvent::Section(step)),
                    Some(NavAction::Page(step)) => events.push(PadEvent::Page(step)),
                    Some(NavAction::OpenSearch) => events.push(PadEvent::OpenSearch),
                    Some(NavAction::OpenOverflow) => events.push(PadEvent::Overflow {
                        id: focused.id.clone(),
                        strip_index: focused.strip_index,
                    }),
                    // Unreachable: only d-pad buttons map to MoveFocus, and they
                    // were skipped above.
                    Some(NavAction::MoveFocus(_)) | None => {}
                }
            }
        }
        self.down = Some(sample.buttons.clone());

        // The stick is always sampled, even when the d-pad wins, so its gate
        // cannot be left holding a stale direction from before.
        let from_stick = self.stick.direction(sample.left_stick, &self.tuning);
        let direction = sample.dpad().or(from_stick);
        if direction.is_none() {
            self.edge_reported = None;
        }
        if let Some(step) = self.repeater.update(direction, now_ms) {
            match self.cursor.step(step) {
                Some((from, to)) => {
                    self.edge_reported = None;
                    events.push(PadEvent::FocusMoved { from, to });
                }
                None => {
                    if self.edge_reported != Some(step) {
                        self.edge_reported = Some(step);
                        events.push(PadEvent::FocusBlocked(step));
                    }
                }
            }
        }

        if let Some(fraction) = scroll_axis(sample.right_stick, &self.tuning) {
            events.push(PadEvent::Scroll { fraction });
        }
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Home page from the information architecture, which is the shape that
    /// exercises everything: a rail above the default focus, a ragged pair of
    /// cards, a second rail, and a three wide row.
    fn home_grid() -> FocusGrid {
        FocusGrid::new(
            vec![
                vec![FocusCell::strip("hero", 3)],
                vec![FocusCell::single("play-now")],
                vec![
                    FocusCell::single("profile"),
                    FocusCell::single("last-match"),
                ],
                vec![FocusCell::strip("mods-rail", 5)],
                vec![
                    FocusCell::single("news-1"),
                    FocusCell::single("news-2"),
                    FocusCell::single("news-3"),
                ],
            ],
            "play-now",
        )
        .expect("the home grid is valid")
    }

    fn cursor() -> FocusCursor {
        FocusCursor::new(home_grid())
    }

    fn walk(cursor: &mut FocusCursor, directions: &[Direction]) {
        for d in directions {
            cursor.step(*d);
        }
    }

    // -- the grid ----------------------------------------------------------

    /// Default focus is the safest primary, which on Home is Play Now. It is
    /// deliberately not the first cell, so a grid that silently fell back to
    /// index 0 would land on the hero carousel and this test would catch it.
    #[test]
    fn focus_starts_on_the_declared_default_not_on_the_first_cell() {
        let c = cursor();
        assert_eq!(c.id(), "play-now");
        assert_ne!(
            c.id(),
            c.grid().ids()[0],
            "the default must not coincide with index 0, or this test proves nothing"
        );
    }

    /// A default that is not in the grid would fall back to whatever is first,
    /// and the thing that is first is sometimes destructive.
    #[test]
    fn a_default_focus_that_does_not_exist_is_refused() {
        let problem = FocusGrid::new(vec![vec![FocusCell::single("only")]], "typo")
            .expect_err("an absent default must be refused");
        assert_eq!(
            problem,
            GridProblem::UnknownDefault {
                id: "typo".to_string()
            }
        );
        let message = problem.message();
        assert!(
            message.contains("typo"),
            "does not name the cell: {message}"
        );
        assert!(
            message.contains("safest primary"),
            "does not say what to do: {message}"
        );
    }

    /// Focus restoration looks items up by id, so two cells with one id restore
    /// focus to whichever comes first, silently and wrongly.
    #[test]
    fn duplicate_ids_are_refused_because_restoration_keys_on_the_id() {
        let problem = FocusGrid::new(
            vec![
                vec![FocusCell::single("card")],
                vec![FocusCell::single("card")],
            ],
            "card",
        )
        .expect_err("duplicate ids must be refused");
        assert_eq!(
            problem,
            GridProblem::DuplicateId {
                id: "card".to_string()
            }
        );
    }

    /// A row with no cells is a hole: moving down lands in it and focuses
    /// nothing. An empty rail is the same failure one level down.
    #[test]
    fn holes_in_a_grid_are_refused_rather_than_focused() {
        assert_eq!(
            FocusGrid::new(vec![vec![FocusCell::single("a")], vec![]], "a"),
            Err(GridProblem::EmptyRow { row: 1 })
        );
        assert_eq!(
            FocusGrid::new(vec![vec![FocusCell::strip("rail", 0)]], "rail"),
            Err(GridProblem::EmptyStrip {
                id: "rail".to_string()
            })
        );
        assert_eq!(FocusGrid::new(vec![], "a"), Err(GridProblem::Empty));
    }

    /// Every refusal has to be actionable, in the same way session refusals are.
    #[test]
    fn every_grid_refusal_says_something_actionable() {
        for problem in [
            GridProblem::Empty,
            GridProblem::EmptyRow { row: 3 },
            GridProblem::EmptyStrip { id: "rail".into() },
            GridProblem::DuplicateId { id: "card".into() },
            GridProblem::UnknownDefault { id: "gone".into() },
        ] {
            let message = problem.message();
            assert!(message.len() > 40, "too terse to act on: {message}");
            assert!(message.ends_with('.'), "not a sentence: {message}");
            let as_error: Error = problem.into();
            assert!(!as_error.to_string().is_empty());
        }
    }

    // -- focus never escapes ------------------------------------------------

    /// The invariant the whole module exists for. Whatever sequence of moves is
    /// made, focus is on a cell that exists and inside a rail that is long
    /// enough to hold it. If this ever fails, the focus ring is nowhere and the
    /// only way out of the launcher is a mouse.
    #[test]
    fn focus_never_escapes_to_nothing_however_far_it_is_walked() {
        for wrap in [
            WrapPolicy::default(),
            WrapPolicy {
                vertical: Wrap::Around,
                horizontal: Wrap::Around,
            },
        ] {
            let mut c = FocusCursor::new(home_grid().with_wrap(wrap));
            let ids: Vec<String> = c.grid().ids().iter().map(|s| s.to_string()).collect();
            // A fixed pseudo-random walk, so a failure is reproducible.
            let mut state: u32 = 0x5eed;
            for _ in 0..2_000 {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let direction = Direction::ALL[(state >> 16) as usize % 4];
                c.step(direction);
                let focused = c.focused();
                assert!(
                    ids.iter().any(|id| id == focused.id),
                    "focus left the grid entirely: {focused:?}"
                );
                assert!(
                    focused.strip_index < focused.strip_len,
                    "focus is on item {} of a rail holding {}: {focused:?}",
                    focused.strip_index,
                    focused.strip_len
                );
            }
        }
    }

    /// With the default policy the ends stop, and the block is reported so a
    /// caller can give feedback rather than looking broken.
    #[test]
    fn clamping_holds_focus_at_the_ends() {
        let mut c = cursor();
        walk(&mut c, &[Direction::Up]); // hero, the top row
        assert_eq!(c.id(), "hero");
        assert!(
            c.step(Direction::Up).is_none(),
            "the top row must not move up"
        );
        assert_eq!(c.id(), "hero", "a blocked move must change nothing");

        walk(
            &mut c,
            &[
                Direction::Down,
                Direction::Down,
                Direction::Down,
                Direction::Down,
            ],
        );
        assert_eq!(c.id(), "news-1");
        assert!(
            c.step(Direction::Down).is_none(),
            "the bottom row must not move down"
        );
        assert!(
            c.step(Direction::Left).is_none(),
            "the leftmost cell must not move left"
        );
        walk(&mut c, &[Direction::Right, Direction::Right]);
        assert_eq!(c.id(), "news-3");
        assert!(
            c.step(Direction::Right).is_none(),
            "the rightmost cell must not move right"
        );
    }

    /// With wrapping on, both axes come round, and they come round to the far
    /// end rather than to somewhere in the middle.
    #[test]
    fn wrapping_continues_from_the_far_end() {
        let mut c = FocusCursor::new(home_grid().with_wrap(WrapPolicy {
            vertical: Wrap::Around,
            horizontal: Wrap::Around,
        }));
        walk(&mut c, &[Direction::Up]);
        assert_eq!(c.id(), "hero");
        walk(&mut c, &[Direction::Up]);
        assert_eq!(
            c.id(),
            "news-1",
            "up from the top row must reach the last row"
        );
        walk(&mut c, &[Direction::Left]);
        assert_eq!(
            c.id(),
            "news-3",
            "left from the first cell must reach the last"
        );
        walk(&mut c, &[Direction::Right]);
        assert_eq!(c.id(), "news-1");
        walk(&mut c, &[Direction::Down]);
        assert_eq!(
            c.id(),
            "hero",
            "down from the last row must reach the first"
        );
    }

    /// Exception 2 of the focus order: left and right stay inside a rail, up and
    /// down leave it. The second half matters as much as the first, because a
    /// container you cannot leave is a focus trap.
    #[test]
    fn a_rail_keeps_horizontal_movement_but_is_not_a_focus_trap() {
        let mut c = cursor();
        walk(&mut c, &[Direction::Down, Direction::Down]);
        assert_eq!(c.id(), "mods-rail");

        walk(&mut c, &[Direction::Right, Direction::Right]);
        let focused = c.focused();
        assert_eq!(
            focused.id, "mods-rail",
            "left and right must stay in the rail"
        );
        assert_eq!(focused.strip_index, 2);

        walk(
            &mut c,
            &[Direction::Right, Direction::Right, Direction::Right],
        );
        assert_eq!(
            c.focused().strip_index,
            4,
            "the last card must clamp rather than hand focus to a neighbour"
        );
        assert_eq!(c.id(), "mods-rail");

        assert!(
            c.step(Direction::Up).is_some(),
            "up must leave the rail, or it is a focus trap"
        );
        assert_ne!(c.id(), "mods-rail");
    }

    /// A rail that resets to the first card every time focus glances away
    /// cannot be read: the player loses their place in a twelve card list every
    /// time they check something above it.
    #[test]
    fn leaving_a_rail_and_returning_lands_on_the_same_card() {
        let mut c = cursor();
        walk(&mut c, &[Direction::Down, Direction::Down]);
        walk(
            &mut c,
            &[Direction::Right, Direction::Right, Direction::Right],
        );
        assert_eq!(c.focused().strip_index, 3);

        walk(
            &mut c,
            &[
                Direction::Up,
                Direction::Up,
                Direction::Down,
                Direction::Down,
            ],
        );
        let focused = c.focused();
        assert_eq!(focused.id, "mods-rail");
        assert_eq!(
            focused.strip_index, 3,
            "the rail must remember which card the player was on"
        );
    }

    /// Rows are ragged, so vertical movement passes through narrow ones. The
    /// column has to survive the trip, or one single cell row pins the player to
    /// the left edge of everything below it for the rest of the session.
    #[test]
    fn a_narrow_row_does_not_permanently_collapse_the_column() {
        let mut c = cursor();
        walk(
            &mut c,
            &[
                Direction::Down,
                Direction::Down,
                Direction::Down,
                Direction::Right,
            ],
        );
        assert_eq!(c.id(), "news-2", "column 1 of the bottom row");

        walk(&mut c, &[Direction::Up]);
        assert_eq!(
            c.id(),
            "mods-rail",
            "the single cell row clamps to column 0"
        );

        walk(&mut c, &[Direction::Up]);
        assert_eq!(
            c.id(),
            "last-match",
            "the wanted column must come back on a row wide enough for it"
        );
    }

    // -- grid replacement --------------------------------------------------

    /// A background download finishing re-renders the page. Focus must stay
    /// where the player put it, not jump to the top.
    #[test]
    fn replacing_the_grid_keeps_focus_on_the_same_item() {
        let mut c = cursor();
        walk(&mut c, &[Direction::Down, Direction::Right]);
        assert_eq!(c.id(), "last-match");

        c.replace_grid(home_grid());
        assert_eq!(
            c.id(),
            "last-match",
            "a re-render must not throw focus back to the default"
        );
    }

    /// When the focused item really is gone, focus goes to the new grid's
    /// declared default. Not to index 0, which is the hero carousel here.
    #[test]
    fn a_vanished_focus_falls_back_to_the_declared_default() {
        let mut c = cursor();
        walk(&mut c, &[Direction::Down, Direction::Right]);
        assert_eq!(c.id(), "last-match");

        let smaller = FocusGrid::new(
            vec![
                vec![FocusCell::strip("hero", 3)],
                vec![FocusCell::single("play-now")],
            ],
            "play-now",
        )
        .expect("valid");
        c.replace_grid(smaller);
        assert_eq!(c.id(), "play-now");
    }

    /// The rail shrank while focus was inside it: a mod was deleted, or a
    /// filter was applied. A remembered index of 4 read out of a rail holding 2
    /// is focus on a card that does not exist.
    #[test]
    fn a_shrunken_rail_clamps_the_remembered_card() {
        let mut c = cursor();
        walk(&mut c, &[Direction::Down, Direction::Down]);
        walk(
            &mut c,
            &[
                Direction::Right,
                Direction::Right,
                Direction::Right,
                Direction::Right,
            ],
        );
        assert_eq!(c.focused().strip_index, 4);

        let shorter = FocusGrid::new(
            vec![
                vec![FocusCell::single("play-now")],
                vec![FocusCell::strip("mods-rail", 2)],
            ],
            "play-now",
        )
        .expect("valid");
        c.replace_grid(shorter);

        let focused = c.focused();
        assert_eq!(focused.id, "mods-rail");
        assert_eq!(focused.strip_len, 2);
        assert!(
            focused.strip_index < focused.strip_len,
            "focus is on card {} of a rail holding {}",
            focused.strip_index,
            focused.strip_len
        );
    }

    /// Returning from a package sheet has to land on the exact card that was
    /// opened, which is the single most common controller navigation failure in
    /// launchers.
    #[test]
    fn focus_can_be_restored_to_an_exact_card() {
        let mut c = cursor();
        assert!(c.restore_in_strip("mods-rail", 3));
        let focused = c.focused();
        assert_eq!(focused.id, "mods-rail");
        assert_eq!(focused.strip_index, 3);

        assert!(
            !c.restore("no-such-card"),
            "restoring something absent must fail rather than move focus somewhere arbitrary"
        );
        assert_eq!(c.id(), "mods-rail", "a failed restore must change nothing");

        assert!(c.restore_in_strip("mods-rail", 99));
        assert_eq!(
            c.focused().strip_index,
            4,
            "an out of range restore clamps to the last card that exists"
        );
    }

    // -- repeat timing -----------------------------------------------------

    /// The rate is the rate. Counted on a 1 ms grid so the expected number is
    /// arithmetic rather than a tolerance: one step on the press, the first
    /// repeat at 400 ms, then every 90 ms.
    #[test]
    fn a_held_direction_repeats_at_the_stated_rate() {
        let timing = RepeatTiming {
            initial_delay_ms: 400,
            interval_ms: 90,
        };
        let mut r = DirectionRepeater::new(timing);
        let mut steps = Vec::new();
        for t in 0..=1_000u64 {
            if r.update(Some(Direction::Down), t).is_some() {
                steps.push(t);
            }
        }
        assert_eq!(
            steps,
            vec![0, 400, 490, 580, 670, 760, 850, 940],
            "a held direction must step immediately, wait the initial delay, then \
             repeat at the interval"
        );
    }

    /// A pad that does nothing for 400 ms after the first press feels dead, and
    /// a first step that waits makes single moves impossible to place.
    #[test]
    fn the_first_step_happens_on_the_press_not_after_the_delay() {
        let mut r = DirectionRepeater::new(RepeatTiming::default());
        assert_eq!(
            r.update(Some(Direction::Right), 5_000),
            Some(Direction::Right)
        );
        assert_eq!(r.update(Some(Direction::Right), 5_001), None);
    }

    /// Flicking from one direction to another must not inherit the fast repeat
    /// the previous direction had earned, or a change of mind fires several
    /// steps the player did not ask for.
    #[test]
    fn changing_direction_restarts_the_delay() {
        let mut r = DirectionRepeater::new(RepeatTiming::default());
        for t in 0..=1_000u64 {
            r.update(Some(Direction::Down), t);
        }
        assert_eq!(
            r.update(Some(Direction::Right), 1_001),
            Some(Direction::Right),
            "a new direction steps at once"
        );
        assert_eq!(
            r.update(Some(Direction::Right), 1_100),
            None,
            "and then waits the full initial delay, not the repeat interval"
        );
    }

    /// The failure a naive implementation has: the window was not pumped for
    /// five seconds, and eleven steps of repeat are owed. Paying them all in one
    /// frame teleports the focus ring with no motion in between, so the schedule
    /// is re-seated instead.
    #[test]
    fn a_stalled_poll_yields_one_step_not_a_burst() {
        let mut r = DirectionRepeater::new(RepeatTiming::default());
        assert!(r.update(Some(Direction::Down), 0).is_some());
        assert!(
            r.update(Some(Direction::Down), 5_000).is_some(),
            "the hold is still real, so one step is owed"
        );
        assert!(
            r.update(Some(Direction::Down), 5_001).is_none(),
            "but only one: the rest would land in the same frame"
        );
        assert!(r.update(Some(Direction::Down), 5_091).is_some());
    }

    /// A system clock that jumps backwards must not freeze navigation until it
    /// catches up, which for a time zone change is an hour.
    #[test]
    fn a_backwards_clock_does_not_freeze_navigation() {
        let mut r = DirectionRepeater::new(RepeatTiming::default());
        assert!(r.update(Some(Direction::Down), 10_000).is_some());
        assert_eq!(r.update(Some(Direction::Down), 5_000), None);
        assert!(
            r.update(Some(Direction::Down), 5_400).is_some(),
            "the schedule must re-seat at the new time"
        );
    }

    /// A configuration file can hold zero. Zero means one step per poll, and the
    /// poll rate is whatever the front end feels like.
    #[test]
    fn a_zero_interval_from_a_config_file_cannot_race() {
        let clamped = RepeatTiming {
            initial_delay_ms: 0,
            interval_ms: 0,
        }
        .clamped();
        assert_eq!(clamped.initial_delay_ms, MIN_REPEAT_DELAY_MS);
        assert_eq!(clamped.interval_ms, MIN_REPEAT_INTERVAL_MS);

        let mut r = DirectionRepeater::new(RepeatTiming {
            initial_delay_ms: 0,
            interval_ms: 0,
        });
        let mut steps = 0;
        for t in 0..=1_000u64 {
            if r.update(Some(Direction::Down), t).is_some() {
                steps += 1;
            }
        }
        assert!(
            steps <= 26,
            "a zero interval produced {steps} steps in one second, which is not \
             controllable"
        );
    }

    /// Releasing has to actually release, or a stick that was let go keeps
    /// walking the focus ring.
    #[test]
    fn releasing_a_direction_stops_the_repeat() {
        let mut r = DirectionRepeater::new(RepeatTiming::default());
        r.update(Some(Direction::Down), 0);
        assert_eq!(r.update(None, 1), None);
        assert_eq!(r.held(), None);
        assert_eq!(
            r.update(Some(Direction::Down), 2),
            Some(Direction::Down),
            "the next press is a fresh press"
        );
    }

    // -- sticks and the deadzone -------------------------------------------

    /// The deadzone is the player's own, from the tuning they set for the game.
    #[test]
    fn the_deadzone_comes_from_the_controller_tuning() {
        let mut tuning = Tuning::default();
        assert!((effective_deadzone(&tuning) - 0.15).abs() < 1e-6);
        tuning.deadzone_percent = 40;
        assert!((effective_deadzone(&tuning) - 0.40).abs() < 1e-6);
    }

    /// A deadzone of 100 is a legitimate value for the game and a disaster for
    /// the launcher: with the pad dead, the player cannot reach Settings to undo
    /// it.
    #[test]
    fn an_extreme_deadzone_cannot_make_the_launcher_unnavigable() {
        for percent in [100u8, 200, 255] {
            let tuning = Tuning {
                deadzone_percent: percent,
                ..Tuning::default()
            };
            let mut gate = StickGate::default();
            assert_eq!(
                gate.direction(Stick::new(0.0, -1.0), &tuning),
                Some(Direction::Down),
                "a deadzone of {percent} left the launcher undrivable"
            );
        }
    }

    /// A disconnecting wireless pad reports NaN. Every comparison against NaN is
    /// false, so an unsanitized axis silently freezes navigation with no error
    /// anywhere.
    #[test]
    fn a_nan_axis_reads_as_centre_rather_than_freezing_navigation() {
        let stick = Stick::new(f32::NAN, f32::NAN);
        assert_eq!(stick.x(), 0.0);
        assert_eq!(stick.y(), 0.0);
        assert_eq!(stick.magnitude(), 0.0);

        let tuning = Tuning::default();
        let mut gate = StickGate::default();
        assert_eq!(gate.direction(stick, &tuning), None);
        assert_eq!(scroll_axis(stick, &tuning), None);

        let out_of_range = Stick::new(-9.0, 9.0);
        assert_eq!(out_of_range.x(), -1.0);
        assert_eq!(out_of_range.y(), 1.0);
    }

    /// A stick parked near the threshold would chatter without hysteresis, and a
    /// chattering direction looks to the repeater like a fresh press every few
    /// frames: the delay resets forever and the pad appears dead.
    #[test]
    fn a_stick_resting_near_the_threshold_does_not_chatter() {
        let tuning = Tuning::default();
        let mut gate = StickGate::default();
        assert_eq!(
            gate.direction(Stick::new(0.0, -0.5), &tuning),
            Some(Direction::Down)
        );
        assert_eq!(
            gate.direction(Stick::new(0.0, -0.30), &tuning),
            Some(Direction::Down),
            "between the release and engage thresholds an engaged direction holds"
        );
        assert_eq!(gate.direction(Stick::new(0.0, -0.20), &tuning), None);
        assert_eq!(
            gate.direction(Stick::new(0.0, -0.30), &tuning),
            None,
            "once released it takes the full engage threshold to come back"
        );
    }

    /// A hold at almost exactly 45 degrees would otherwise alternate between
    /// axes and walk the focus diagonally across the page.
    #[test]
    fn a_diagonal_hold_does_not_alternate_axes() {
        let tuning = Tuning::default();
        let mut gate = StickGate::default();
        assert_eq!(
            gate.direction(Stick::new(0.10, -0.90), &tuning),
            Some(Direction::Down)
        );
        assert_eq!(
            gate.direction(Stick::new(0.65, -0.64), &tuning),
            Some(Direction::Down),
            "a barely horizontal diagonal must not take over"
        );
        assert_eq!(
            gate.direction(Stick::new(0.90, -0.10), &tuning),
            Some(Direction::Right),
            "a clearly horizontal push must"
        );
    }

    /// Scroll speed starts from zero where the deadzone ends. Passing the raw
    /// axis through makes the first perceptible movement a jump to 15 percent
    /// speed, which cannot be placed.
    #[test]
    fn scroll_speed_starts_from_zero_at_the_edge_of_the_deadzone() {
        let tuning = Tuning::default();
        assert_eq!(scroll_axis(Stick::CENTER, &tuning), None);
        assert_eq!(scroll_axis(Stick::new(0.0, 0.15), &tuning), None);

        let just_outside =
            scroll_axis(Stick::new(0.0, -0.16), &tuning).expect("outside the deadzone");
        assert!(
            just_outside.abs() < 0.05,
            "scrolling started at {just_outside} of full speed instead of near zero"
        );
        let full = scroll_axis(Stick::new(0.0, -1.0), &tuning).expect("full deflection");
        assert!((full - 1.0).abs() < 1e-6, "full deflection gave {full}");
        let upward = scroll_axis(Stick::new(0.0, 1.0), &tuning).expect("full deflection");
        assert!(upward < 0.0, "pushing up must scroll toward the start");
    }

    // -- the shortcut table -------------------------------------------------

    const CONTEXTS: &[InputContext] = &[
        InputContext::Page,
        InputContext::Sheet,
        InputContext::Dialog,
        InputContext::Confirm,
        InputContext::Blocking,
    ];

    /// The Guide button belongs to the operating system. Binding it means the
    /// launcher fights the Xbox overlay for the same press.
    #[test]
    fn the_guide_button_is_never_bound_in_any_context() {
        for context in CONTEXTS {
            assert_eq!(
                action_for(PadButton::Guide, *context),
                None,
                "Guide is bound in {context:?}"
            );
        }
    }

    /// Changing section under an open modal leaves the modal describing a page
    /// nobody can see, and B then returns the player somewhere they never were.
    #[test]
    fn a_modal_cannot_navigate_the_page_behind_it() {
        for context in CONTEXTS.iter().filter(|c| c.is_modal()) {
            for button in [
                PadButton::LeftBumper,
                PadButton::RightBumper,
                PadButton::LeftTrigger,
                PadButton::RightTrigger,
                PadButton::View,
                PadButton::Menu,
                PadButton::X,
                PadButton::Y,
            ] {
                assert_eq!(
                    action_for(button, *context),
                    None,
                    "{button:?} still acts on the page behind a {context:?}"
                );
            }
            assert_eq!(
                action_for(PadButton::A, *context),
                Some(NavAction::Activate)
            );
            assert_eq!(
                action_for(PadButton::DPadDown, *context),
                Some(NavAction::MoveFocus(Direction::Down)),
                "focus must still move inside a modal"
            );
        }
    }

    /// A blocking modal is not dismissible, so Back does nothing and the hint
    /// strip omits it rather than showing a dead button.
    #[test]
    fn a_blocking_modal_has_no_back() {
        assert_eq!(action_for(PadButton::B, InputContext::Blocking), None);
        for context in CONTEXTS.iter().filter(|c| **c != InputContext::Blocking) {
            assert_eq!(
                action_for(PadButton::B, *context),
                Some(NavAction::Back),
                "B must go up one level in {context:?}"
            );
        }
    }

    /// The full table on an ordinary page, from information architecture
    /// section 4.
    #[test]
    fn the_page_shortcuts_match_the_specification() {
        use InputContext::Page as P;
        assert_eq!(action_for(PadButton::A, P), Some(NavAction::Activate));
        assert_eq!(action_for(PadButton::B, P), Some(NavAction::Back));
        assert_eq!(
            action_for(PadButton::X, P),
            Some(NavAction::Contextual(ContextualSlot::X))
        );
        assert_eq!(
            action_for(PadButton::Y, P),
            Some(NavAction::Contextual(ContextualSlot::Y))
        );
        assert_eq!(
            action_for(PadButton::LeftBumper, P),
            Some(NavAction::Section(Step::Previous))
        );
        assert_eq!(
            action_for(PadButton::RightBumper, P),
            Some(NavAction::Section(Step::Next))
        );
        assert_eq!(
            action_for(PadButton::LeftTrigger, P),
            Some(NavAction::Page(Step::Previous))
        );
        assert_eq!(
            action_for(PadButton::RightTrigger, P),
            Some(NavAction::Page(Step::Next))
        );
        assert_eq!(action_for(PadButton::View, P), Some(NavAction::OpenSearch));
        assert_eq!(
            action_for(PadButton::Menu, P),
            Some(NavAction::OpenOverflow)
        );
    }

    // -- contextual bindings ------------------------------------------------

    /// X and Y sit one press from arrival with no confirmation step. Binding a
    /// destructive action there means a player exploring the pad deletes
    /// something.
    #[test]
    fn a_destructive_action_cannot_be_bound_to_x_or_y() {
        for slot in [ContextualSlot::X, ContextualSlot::Y] {
            let mut bindings = ContextualBindings::default();
            let refusal = bindings
                .bind(
                    slot,
                    LauncherAction::destructive("Delete", Destructive::DeleteMod),
                )
                .expect_err("a destructive contextual binding must be refused");
            let message = refusal.message();
            assert!(
                message.contains("overflow"),
                "the refusal must say where it belongs instead: {message}"
            );
            assert!(
                message.contains("hold-to-confirm"),
                "the refusal must name the gate it would be missing: {message}"
            );
            assert!(
                bindings.label(slot).is_none(),
                "a refused binding must leave the slot empty"
            );
        }
    }

    /// The hint strip is the only place bindings are stated. An unbound slot
    /// shows nothing, because a label with nothing behind it is worse than no
    /// label at all.
    #[test]
    fn an_unbound_contextual_slot_has_no_label_and_no_hint() {
        let mut bindings = ContextualBindings::default();
        assert!(bindings.label(ContextualSlot::X).is_none());
        assert!(bindings.hints(DeviceFamily::Xbox).is_empty());

        bindings
            .bind(ContextualSlot::Y, LauncherAction::safe("Verify"))
            .expect("a safe action binds");
        assert!(bindings.label(ContextualSlot::X).is_none());
        assert_eq!(bindings.hints(DeviceFamily::Xbox), vec![("Y", "Verify")]);
        assert_eq!(
            bindings.hints(DeviceFamily::PlayStation),
            vec![("Triangle", "Verify")],
            "the hint strip follows the connected pad"
        );

        bindings.clear(ContextualSlot::Y);
        assert!(bindings.hints(DeviceFamily::Xbox).is_empty());
    }

    /// A hold gate on something harmless trains players to hold everything, and
    /// then the gate means nothing where it matters.
    #[test]
    fn only_a_destructive_action_gets_a_hold_gate() {
        assert!(LauncherAction::safe("Install").hold().is_none());
        let hold = LauncherAction::destructive("Delete", Destructive::DeleteMod)
            .hold()
            .expect("a destructive action must be gated");
        assert_eq!(hold.what(), Destructive::DeleteMod);
    }

    // -- hold to confirm ----------------------------------------------------

    fn armed_hold() -> HoldToConfirm {
        let mut hold = HoldToConfirm::new(Destructive::DeleteMod);
        // One observation with the button up is what arms it.
        hold.observe(false, 0);
        hold
    }

    /// 800 ms, for every one of the five, from the specification. Pinned so a
    /// view cannot quietly shorten a safety gate.
    #[test]
    fn every_destructive_action_holds_for_the_specified_time() {
        for what in Destructive::ALL {
            assert_eq!(HoldToConfirm::new(*what).required_ms(), 800);
            assert!(!what.describe().is_empty());
        }
        assert_eq!(HOLD_TO_CONFIRM_MS, 800);
    }

    /// One millisecond early is early. This is the whole gate.
    #[test]
    fn a_hold_does_not_fire_early() {
        let mut hold = armed_hold();
        assert_eq!(hold.observe(true, 100), HoldEvent::Started);
        for t in 101..=899u64 {
            assert_eq!(
                hold.observe(true, t),
                HoldEvent::Nothing,
                "fired at {}ms of an 800ms hold",
                t - 100
            );
        }
        assert_eq!(hold.observe(true, 900), HoldEvent::Fired);
        assert!(hold.has_fired());
    }

    /// A released button must never fire, however long the dialog stays open
    /// afterwards.
    #[test]
    fn a_released_button_never_fires() {
        let mut hold = armed_hold();
        hold.observe(true, 10);
        assert_eq!(
            hold.observe(false, 500),
            HoldEvent::Cancelled { held_ms: 490 }
        );
        for t in 501..=10_000u64 {
            assert_eq!(hold.observe(false, t), HoldEvent::Nothing);
        }
        assert!(!hold.has_fired());
        assert!(hold.progress(10_000).is_none());
    }

    /// The confirm was opened by pressing A, and A is still down. Crediting that
    /// hold deletes a mod with a single press and no ring the player ever saw.
    #[test]
    fn a_button_already_held_when_the_confirm_opens_never_fires() {
        let mut hold = HoldToConfirm::new(Destructive::FormatUsbDrive);
        for t in 0..=5_000u64 {
            assert_eq!(hold.observe(true, t), HoldEvent::Nothing);
        }
        assert!(
            !hold.has_fired(),
            "a button that was down before the dialog existed must not confirm it"
        );
        assert!(hold.progress(5_000).is_none());

        assert_eq!(hold.observe(false, 5_001), HoldEvent::Nothing);
        assert_eq!(hold.observe(true, 5_002), HoldEvent::Started);
        for t in 5_003..5_802u64 {
            hold.observe(true, t);
        }
        assert_eq!(hold.observe(true, 5_802), HoldEvent::Fired);
    }

    /// Time the launcher did not watch is not time the player held the button,
    /// and it is certainly not time they saw a ring filling. A hold spanning a
    /// stall restarts rather than completing.
    #[test]
    fn a_hold_that_spans_a_stall_is_not_credited() {
        let mut hold = armed_hold();
        assert_eq!(hold.observe(true, 1), HoldEvent::Started);
        assert_eq!(hold.observe(true, 200), HoldEvent::Nothing);
        assert_eq!(
            hold.observe(true, 1_200),
            HoldEvent::Restarted {
                discarded_ms: 1_199
            },
            "a gap longer than the stall threshold must not be credited"
        );
        assert!(!hold.has_fired());

        for t in 1_201..2_000u64 {
            assert_eq!(hold.observe(true, t), HoldEvent::Nothing);
        }
        assert_eq!(
            hold.observe(true, 2_000),
            HoldEvent::Fired,
            "the restarted hold completes on observed time"
        );
    }

    /// Fires once. A machine that keeps being fed must not delete twice.
    #[test]
    fn a_hold_fires_exactly_once() {
        let mut hold = armed_hold();
        let mut fired = 0;
        for t in 1..=5_000u64 {
            if hold.observe(true, t) == HoldEvent::Fired {
                fired += 1;
            }
        }
        assert_eq!(fired, 1);

        hold.observe(false, 5_001);
        for t in 5_002..=8_000u64 {
            assert_ne!(
                hold.observe(true, t),
                HoldEvent::Fired,
                "a spent machine must not fire again without an explicit reset"
            );
        }

        hold.reset();
        assert!(!hold.has_fired());
        assert_eq!(
            hold.observe(true, 9_000),
            HoldEvent::Nothing,
            "a reset machine is unarmed, so the button has to be released first"
        );
    }

    /// The ring is only drawn while something is being held. An empty ring on
    /// every control reads as "everything here needs a hold".
    #[test]
    fn ring_progress_is_absent_unless_a_hold_is_running() {
        let mut hold = armed_hold();
        assert!(hold.progress(0).is_none());
        hold.observe(true, 0);
        let quarter = hold.progress(200).expect("holding");
        assert!((quarter - 0.25).abs() < 1e-3, "got {quarter}");
        assert_eq!(
            hold.progress(5_000),
            Some(1.0),
            "progress must not exceed one"
        );
    }

    // -- the navigator ------------------------------------------------------

    fn navigator() -> PadNavigator {
        let mut nav = PadNavigator::new(home_grid());
        // Arm it: one frame with nothing held.
        nav.update(&PadSample::new(), 0);
        nav
    }

    /// A held A on a mod card must install once, not sixty times a second.
    #[test]
    fn a_held_button_acts_once_not_every_frame() {
        let mut nav = navigator();
        let held = PadSample::new().with_button(PadButton::A);
        let mut activations = 0;
        for t in 1..=300u64 {
            activations += nav
                .update(&held, t)
                .iter()
                .filter(|e| matches!(e, PadEvent::Activate { .. }))
                .count();
        }
        assert_eq!(activations, 1);

        nav.update(&PadSample::new(), 301);
        let again = nav.update(&held, 302);
        assert!(
            again.iter().any(|e| matches!(e, PadEvent::Activate { .. })),
            "a released and pressed button acts again"
        );
    }

    /// A button already down when the navigator is created belongs to whatever
    /// the player was doing before this screen existed.
    #[test]
    fn a_button_already_held_when_a_screen_opens_does_not_act_on_it() {
        let mut nav = PadNavigator::new(home_grid());
        let held = PadSample::new().with_button(PadButton::A);
        for t in 0..=200u64 {
            assert!(
                nav.update(&held, t).is_empty(),
                "a button held from before the screen opened acted on it"
            );
        }
        nav.update(&PadSample::new(), 201);
        assert!(nav
            .update(&held, 202)
            .iter()
            .any(|e| matches!(e, PadEvent::Activate { .. })));
    }

    /// Opening or closing a modal with a button that is still down must not let
    /// that same press act on what it revealed.
    #[test]
    fn changing_context_disarms_the_buttons_that_are_still_down() {
        let mut nav = navigator();
        let held = PadSample::new().with_button(PadButton::A);
        assert!(!nav.update(&held, 1).is_empty());
        nav.set_context(InputContext::Confirm);
        assert_eq!(nav.context(), InputContext::Confirm);
        for t in 2..=200u64 {
            assert!(
                nav.update(&held, t).is_empty(),
                "the press that opened the modal also pressed its primary"
            );
        }
    }

    /// A player who presses A while pushing down meant the item they were
    /// looking at. Moving first installs the wrong mod.
    #[test]
    fn activate_uses_the_focus_from_the_start_of_the_frame() {
        let mut nav = navigator();
        assert_eq!(nav.focused().id, "play-now");
        let sample = PadSample::new()
            .with_button(PadButton::A)
            .with_button(PadButton::DPadDown);
        let events = nav.update(&sample, 1);
        assert_eq!(
            events,
            vec![
                PadEvent::Activate {
                    id: "play-now".to_string(),
                    strip_index: 0,
                },
                PadEvent::FocusMoved {
                    from: FocusPoint {
                        id: "play-now".to_string(),
                        strip_index: 0,
                    },
                    to: FocusPoint {
                        id: "profile".to_string(),
                        strip_index: 0,
                    },
                },
            ],
            "the activation must name the item that was focused, then focus moves"
        );
    }

    /// The overflow menu acts on the focused item too, and the id has to travel
    /// with the event for the same reason.
    #[test]
    fn the_overflow_menu_names_the_item_it_was_opened_for() {
        let mut nav = navigator();
        nav.cursor_mut().restore_in_strip("mods-rail", 2);
        let events = nav.update(&PadSample::new().with_button(PadButton::Menu), 1);
        assert_eq!(
            events,
            vec![PadEvent::Overflow {
                id: "mods-rail".to_string(),
                strip_index: 2,
            }]
        );
    }

    /// Leaning on the stick at the bottom of a list must report the edge once.
    /// At the repeat rate it is a machine gun, and a caller that plays a sound
    /// on it becomes unbearable.
    #[test]
    fn holding_against_an_edge_reports_the_block_once() {
        let mut nav = navigator();
        let up = PadSample::new().with_button(PadButton::DPadUp);
        let mut moves = 0;
        let mut blocks = 0;
        for t in 1..=2_000u64 {
            for event in nav.update(&up, t) {
                match event {
                    PadEvent::FocusMoved { .. } => moves += 1,
                    PadEvent::FocusBlocked(_) => blocks += 1,
                    other => panic!("unexpected event: {other:?}"),
                }
            }
        }
        assert_eq!(moves, 1, "there is one row above the default focus");
        assert_eq!(blocks, 1, "the edge was reported {blocks} times");
        assert_eq!(nav.focused().id, "hero");
    }

    /// The d-pad goes through the repeater, so it must not also fire as a
    /// button press. Two paths for one press is two steps for one press.
    #[test]
    fn the_dpad_moves_focus_once_per_press_not_twice() {
        let mut nav = navigator();
        let events = nav.update(&PadSample::new().with_button(PadButton::DPadDown), 1);
        assert_eq!(
            events.len(),
            1,
            "one press produced {} events: {events:?}",
            events.len()
        );
        assert!(matches!(events[0], PadEvent::FocusMoved { .. }));
    }

    /// The right stick scrolls, and it does not move focus.
    #[test]
    fn the_right_stick_scrolls_without_moving_focus() {
        let mut nav = navigator();
        let sample = PadSample::new().with_right(Stick::new(0.0, -1.0));
        let events = nav.update(&sample, 1);
        assert_eq!(events, vec![PadEvent::Scroll { fraction: 1.0 }]);
        assert_eq!(nav.focused().id, "play-now");
    }

    /// The left stick moves focus and repeats exactly as the d-pad does, so a
    /// player using either gets the same launcher.
    #[test]
    fn the_left_stick_moves_focus_at_the_same_rate_as_the_dpad() {
        let mut nav = navigator();
        let sample = PadSample::new().with_left(Stick::new(0.0, -1.0));
        let mut moves = 0;
        for t in 1..=1_000u64 {
            moves += nav
                .update(&sample, t)
                .iter()
                .filter(|e| matches!(e, PadEvent::FocusMoved { .. }))
                .count();
        }
        // One on the press, then the repeats: the same schedule the repeater
        // test pins, less the steps the bottom edge refuses.
        assert!(moves >= 3, "the stick barely repeated: {moves} moves");
        assert_eq!(nav.focused().id, "news-1", "it should have reached the end");
    }

    /// Whatever a front end sends, focus is always somewhere real. This is the
    /// same invariant as the cursor walk, through the full input path including
    /// the deadzone, the repeater and the button table.
    #[test]
    fn no_sequence_of_samples_leaves_focus_on_nothing() {
        let mut nav = navigator();
        let ids: Vec<String> = nav
            .cursor()
            .grid()
            .ids()
            .iter()
            .map(|s| s.to_string())
            .collect();
        let mut state: u32 = 0xc0ffee;
        for t in 1..=4_000u64 {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let pick = (state >> 8) as usize;
            let mut sample = PadSample::new()
                .with_left(Stick::new(
                    ((state >> 3) % 21) as f32 / 10.0 - 1.0,
                    ((state >> 11) % 21) as f32 / 10.0 - 1.0,
                ))
                .with_button(PadButton::ALL[pick % PadButton::ALL.len()]);
            if (pick & 7) == 0 {
                sample = sample.with_button(PadButton::ALL[(pick / 7) % PadButton::ALL.len()]);
            }
            nav.update(&sample, t);
            let focused = nav.focused();
            assert!(
                ids.iter().any(|id| id == focused.id),
                "focus left the grid: {focused:?}"
            );
            assert!(
                focused.strip_index < focused.strip_len,
                "focus is outside its rail: {focused:?}"
            );
        }
    }
}
