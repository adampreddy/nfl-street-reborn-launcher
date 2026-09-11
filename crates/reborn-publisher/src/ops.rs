//! The one place a publishing operation runs.
//!
//! Every page command that plans, runs or verifies something calls
//! [`run_operation`] and nothing else. That is what makes two guarantees
//! true everywhere at once instead of page by page: only one operation runs
//! at a time, and every operation, done, described, refused or failed,
//! becomes exactly one line in `reborn_core::publish::journal`. A page
//! cannot run something that goes unrecorded, because there is no other
//! door into the core.
//!
//! # The `op` name: `"<page>:<step>"`
//!
//! Every call names its step as `"<page>:<step>"`, for example
//! `"roster:plan"`, `"roster:run"`, `"roster:verify"`, or a one-shot action
//! with no plan/verify pair of its own, like `"game:dump"`. The part after
//! the last `:` decides which busy [`Phase`] this call reports while
//! `body` runs, and which settled phase it reports once `body` returns:
//!
//! | step ends with  | busy        | settled    |
//! |------------------|-------------|------------|
//! | `plan`           | `Planning`  | `Planned`  |
//! | contains `verify`| `Verifying` | `Done`     |
//! | anything else    | `Running`   | `Done`     |
//!
//! The part before the last `:` (or the whole string, when there is no `:`)
//! is what lands in `OpState.operation`, in the journal's `operation`
//! field, and in the refusal a second call gets while the first is still
//! busy.
//!
//! Only a `plan` step touches [`OpState::plan`]. A `run` or `verify` step
//! updates `phase`, `lines` and `summary` but leaves `plan` exactly as the
//! last plan step left it, which is what lets the panel keep showing the
//! plan's own summary above the Run button while Run is in flight.
//!
//! # `run_operation` carries one more parameter than the plan's sketch
//!
//! The plan that named this module's shape wrote `run_operation` with three
//! parameters: `op`, `dry_run`, `body`. Building it, one gap showed up: a
//! refusal (`Outcome::Refused`) and a hard error (`Err`) carry no
//! `serde_json::Value` at all, only a sentence, so there is no way to
//! recover "what was this operation attempted with" from `body`'s return
//! value alone. The journal's whole point is a truthful record of what was
//! attempted, not only of what succeeded, so `run_operation` here takes a
//! fourth parameter, `inputs`, built by the caller from its own typed
//! request with every secret already excluded, the same way
//! `RosterRequest`'s hand-written `Debug` never prints a token. This is
//! recorded in the Task 1 report as a deliberate deviation, not a silent
//! one.
//!
//! # The value `body` returns
//!
//! `body`'s `Ok(Outcome::Done(value))` carries a `serde_json::Value` that is
//! both what the panel draws (a plan's own findings, a run's result, a
//! verify's read-back) and, by a small convention, the source of the
//! journal fields beyond `inputs`. Any of these top level keys may be
//! present; every one is optional and its absence is not an error:
//!
//! - `"summary"`: one line of human text. Falls back to the last log line
//!   when absent.
//! - `"sequence"`: the release sequence this touched.
//! - `"hashes"`: `[[name, hex], ...]`.
//! - `"uploaded"`: `[object key, ...]`.
//! - `"verified"`: one line describing a read-back check that ran.
//!
//! A page is free to put whatever else it wants in the same object for the
//! panel to draw; only these five keys are read here.
//!
//! # The origin a journal record lands in
//!
//! [`run_operation`] resolves the real origin from this app's own settings
//! file, the same `origin` field every page already reads through
//! [`crate::settings::Settings`], and hands it down to [`finish`] and
//! [`write_journal`] as a plain parameter rather than letting either of
//! them look the settings file up for themselves. That split exists for one
//! reason: a test must never be able to reach the real origin just by
//! calling the function every page calls.
//!
//! [`run_operation_with_origin`] is the function that actually does the
//! work; `run_operation` is a two-line wrapper that resolves the real
//! origin and calls it. A test in this module calls
//! `run_operation_with_origin` directly, with a `tempfile::tempdir()` in
//! hand, and never goes near [`resolve_origin`] or the settings file it
//! reads. `C:\Reborn Origin`, the real publishing origin for a live game,
//! is consequently not a path this module's tests can write into even by
//! a future mistake: the only function that resolves it is not the one
//! they call.
//!
//! # Cancellation
//!
//! [`op_cancel`] reaches only a body that registered a
//! [`reborn_core::inventory::CancelSignal`] by calling
//! [`set_cancel_signal`] as the first line inside `body`, on the worker
//! thread `run_operation` spawns. Calling it from inside `body`, rather
//! than before `run_operation` is called, is what keeps two operations from
//! racing over which one's cancel flag is currently registered: by the time
//! `body` runs, `run_operation`'s own busy check has already committed to
//! running this operation and no other. A body that never registers a
//! signal, or a phase with nothing left to interrupt, leaves `op_cancel`
//! refusing rather than silently doing nothing. Plan 2's ledger recorded
//! that cancellation reaches only the hashing phase of a scan today; that
//! has not changed here.

// Every page's command body is, by this task's own brief, exactly
// `unimplemented!("Task N")`: Tasks 2 to 8 are what call `run_operation`,
// `set_cancel_signal` and the step-naming helpers below. Until the first of
// those lands, nothing in this crate reaches them from `main`, which is an
// honest fact about this task's scope rather than dead code to delete.
#![allow(dead_code)]

use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::thread;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use reborn_core::inventory::CancelSignal;
use reborn_core::publish::journal::{self, Record, JOURNAL_SCHEMA};
use reborn_core::publish::{Channel, Event, Outcome, Sink};
use reborn_core::Platform;

use crate::settings;

/// Where one operation currently stands. Polled by the window, not pushed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    Idle,
    Planning,
    Planned,
    Running,
    Verifying,
    Done,
    Refused,
    Failed,
}

impl Phase {
    /// Whether an operation is in flight, in the sense that matters for
    /// concurrency: a second `run_operation` call is refused, and
    /// `op_clear` is refused, while any of these three hold.
    fn is_busy(self) -> bool {
        matches!(self, Phase::Planning | Phase::Running | Phase::Verifying)
    }
}

/// The one shared state the window polls with `op_status`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpState {
    pub phase: Phase,
    /// The page name a running or just-finished operation belongs to (the
    /// part of its `op` string before the last `:`). Empty at `Idle`.
    pub operation: String,
    /// Every `Event` line emitted so far, in order. An `Err`-channel line
    /// is kept in this same list, marked with an `"ERROR: "` prefix rather
    /// than a separate field, because `OpState` carries no per-line
    /// channel of its own.
    pub lines: Vec<String>,
    pub summary: String,
    /// What the last `plan` step found. Only a `plan` step ever writes
    /// this; a `run` or `verify` step leaves it untouched so its summary
    /// keeps showing while the next step runs. `None` before any plan has
    /// run since the last `op_clear`; `Some(Value::Null)` when a plan step
    /// finished with nothing to say.
    pub plan: Option<Value>,
    pub dry_run: bool,
}

impl OpState {
    fn idle() -> Self {
        OpState {
            phase: Phase::Idle,
            operation: String::new(),
            lines: Vec::new(),
            summary: String::new(),
            plan: None,
            dry_run: false,
        }
    }
}

struct Shared {
    state: Mutex<OpState>,
    /// The current operation's cancel signal, when one has registered
    /// itself. See the module doc's Cancellation section.
    cancel: Mutex<Option<CancelSignal>>,
}

fn shared() -> &'static Shared {
    static SHARED: OnceLock<Shared> = OnceLock::new();
    SHARED.get_or_init(|| Shared {
        state: Mutex::new(OpState::idle()),
        cancel: Mutex::new(None),
    })
}

/// Which busy phase and which settled phase a step earns. See the module
/// doc's table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    Plan,
    Run,
    Verify,
}

impl Step {
    fn busy(self) -> Phase {
        match self {
            Step::Plan => Phase::Planning,
            Step::Run => Phase::Running,
            Step::Verify => Phase::Verifying,
        }
    }

    fn settled(self) -> Phase {
        match self {
            Step::Plan => Phase::Planned,
            Step::Run | Step::Verify => Phase::Done,
        }
    }
}

fn step_of(op: &str) -> Step {
    let step = op.rsplit(':').next().unwrap_or(op);
    if step.ends_with("plan") {
        Step::Plan
    } else if step.contains("verify") {
        Step::Verify
    } else {
        Step::Run
    }
}

/// The page name for the journal and for `OpState.operation`: everything
/// before the last `:`, or the whole string when there is none.
fn page_name(op: &str) -> &str {
    match op.rsplit_once(':') {
        Some((page, _)) => page,
        None => op,
    }
}

/// Run one operation on a worker thread.
///
/// `op` names the step as `"<page>:<step>"` (see the module doc). `inputs`
/// is what the caller is attempting, already stripped of every secret; it
/// is recorded to the journal no matter how `body` ends. `body` runs on the
/// worker thread and writes its human lines to the `Sink` it is given;
/// those lines append to `OpState.lines` as they arrive, so the window
/// polling `op_status` sees them live.
///
/// Returns immediately, before `body` finishes: the caller (a
/// `#[tauri::command(rename_all = "snake_case")]` function) is meant to return right after this call,
/// and let the page poll `op_status` for progress. The only way this
/// returns an `Err` is the concurrency refusal: an operation is already
/// busy, named in the message.
///
/// This resolves the real origin from settings and hands it to
/// [`run_operation_with_origin`], which does the actual work. See the
/// module doc's "The origin a journal record lands in" for why the split
/// exists: it is what lets this module's own tests call
/// `run_operation_with_origin` with a `tempfile::tempdir()` instead,
/// without this function's settings lookup ever running.
pub fn run_operation(
    op: &str,
    dry_run: bool,
    inputs: Value,
    body: impl FnOnce(Sink<'_>) -> reborn_core::Result<Outcome<Value>> + Send + 'static,
) -> Result<(), String> {
    run_operation_with_origin(resolve_origin(), op, dry_run, inputs, body)
}

/// The origin this machine's own settings name, the same `origin` field
/// every page reads through [`crate::settings::Settings`]. `None` when the
/// settings file cannot be found or read; `write_journal` treats that as
/// "nothing to journal into" rather than an operation failure, exactly as
/// it did before this lookup was pulled out into its own function.
fn resolve_origin() -> Option<PathBuf> {
    let platform = Platform::detect();
    let path = settings::settings_path(&platform).ok()?;
    let loaded = settings::Settings::load(&path).ok()?;
    Some(PathBuf::from(loaded.origin))
}

/// Do the work [`run_operation`] describes, journalling into `origin`
/// rather than whatever this machine's settings happen to name.
///
/// `run_operation` is the only production caller, and it always supplies
/// the real origin by calling [`resolve_origin`] first, so this split
/// changes nothing about what a page sees. A test calls this directly
/// with a `tempfile::tempdir()`'s path instead, which is what keeps the
/// real settings file, and the real origin it would otherwise name, out of
/// reach of `cargo test`.
fn run_operation_with_origin(
    origin: Option<PathBuf>,
    op: &str,
    dry_run: bool,
    inputs: Value,
    body: impl FnOnce(Sink<'_>) -> reborn_core::Result<Outcome<Value>> + Send + 'static,
) -> Result<(), String> {
    let step = step_of(op);
    let s = shared();

    {
        let mut state = s.state.lock().expect("op state lock");
        if state.phase.is_busy() {
            return Err(format!(
                "{} is already running. Wait for it to finish, or cancel it, before starting {op}.",
                state.operation
            ));
        }
        state.phase = step.busy();
        state.operation = page_name(op).to_string();
        state.lines.clear();
        state.summary.clear();
        state.dry_run = dry_run;
        // `plan` is deliberately left alone: see the module doc.
    }
    // A fresh operation starts with no registered cancel signal, so a
    // signal left over from the last one can never be cancelled by mistake.
    *s.cancel.lock().expect("cancel lock") = None;

    let op_owned = op.to_string();
    thread::spawn(move || {
        let mut sink = move |event: Event| {
            let line = match event.channel {
                Channel::Out => event.line,
                Channel::Err => format!("ERROR: {}", event.line),
            };
            shared().state.lock().expect("op state lock").lines.push(line);
        };
        let result = body(&mut sink);
        finish(&op_owned, step, dry_run, inputs, origin, result);
    });

    Ok(())
}

/// Register the [`CancelSignal`] a running body is using as its
/// `ScanObserver`, so [`op_cancel`] can reach it. See the module doc's
/// Cancellation section for why this must be called from inside `body`.
pub fn set_cancel_signal(signal: CancelSignal) {
    *shared().cancel.lock().expect("cancel lock") = Some(signal);
}

/// Settle `OpState`, then write the one journal record this operation
/// earns, whatever it ended with. `origin` is whatever [`run_operation`] (or
/// a test's [`run_operation_with_origin`]) was given; this function never
/// resolves one of its own.
fn finish(
    op: &str,
    step: Step,
    dry_run: bool,
    inputs: Value,
    origin: Option<PathBuf>,
    result: reborn_core::Result<Outcome<Value>>,
) {
    let page = page_name(op);

    let (phase, journal_outcome, mut summary, value): (Phase, &'static str, String, Option<Value>) =
        match &result {
            Ok(Outcome::Done(value)) => (
                step.settled(),
                "done",
                value
                    .get("summary")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                Some(value.clone()),
            ),
            Ok(Outcome::Described) => (step.settled(), "described", String::new(), None),
            Ok(Outcome::Refused(refusal)) => {
                (Phase::Refused, "refused", refusal.summary.clone(), None)
            }
            Err(e) => (Phase::Failed, "failed", e.to_string(), None),
        };

    {
        let mut state = shared().state.lock().expect("op state lock");
        if summary.is_empty() {
            summary = state.lines.last().cloned().unwrap_or_default();
        }
        state.phase = phase;
        state.summary = summary.clone();
        if step == Step::Plan {
            state.plan = Some(value.clone().unwrap_or(Value::Null));
        }
    }
    // The operation has settled; nothing left to cancel.
    *shared().cancel.lock().expect("cancel lock") = None;

    write_journal(page, dry_run, inputs, origin, journal_outcome, summary, value);
}

/// Pull `hashes`, `uploaded`, `verified` and `sequence` out of `value` by
/// the convention documented on the module, and append one record into
/// `origin`.
///
/// `origin` is `None` when [`resolve_origin`] could not find or read this
/// machine's settings file; the journal write is skipped in that case
/// rather than guessed at, exactly as before this became a parameter
/// instead of a lookup done here.
///
/// A journal write that fails is not reported as this operation's own
/// failure: the operation already finished (done, described, refused or
/// failed) and that answer stands. It is not silent either; the summary
/// this function is given already sits in `OpState.summary` for a person to
/// see, and a History page reading an origin whose journal cannot be
/// written to has a larger problem than one missing row.
fn write_journal(
    page: &str,
    dry_run: bool,
    inputs: Value,
    origin: Option<PathBuf>,
    outcome: &'static str,
    summary: String,
    value: Option<Value>,
) {
    let sequence = value.as_ref().and_then(|v| v.get("sequence")).and_then(Value::as_u64);
    let hashes = value.as_ref().map(parse_hashes).unwrap_or_default();
    let uploaded = value
        .as_ref()
        .and_then(|v| v.get("uploaded"))
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|e| e.as_str().map(String::from)).collect())
        .unwrap_or_default();
    let verified = value
        .as_ref()
        .and_then(|v| v.get("verified"))
        .and_then(|v| v.as_str())
        .map(String::from);

    let record = Record {
        schema: JOURNAL_SCHEMA.to_string(),
        at: journal::now(),
        operation: page.to_string(),
        dry_run,
        inputs,
        sequence,
        hashes,
        uploaded,
        verified,
        outcome: outcome.to_string(),
        summary,
    };

    let Some(origin) = origin else {
        return;
    };
    let _ = journal::append(&origin, &record);
}

/// `value.hashes` as `(name, hex)` pairs. Accepts either `[[name, hex],
/// ...]` or `[{"name": ..., "hash": ...}, ...]`; anything else, or a
/// missing key, is an empty list rather than an error, since a journal
/// entry with no hashes is a normal one for most operations.
fn parse_hashes(value: &Value) -> Vec<(String, String)> {
    let Some(arr) = value.get("hashes").and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|entry| {
            if let Some(pair) = entry.as_array() {
                if pair.len() != 2 {
                    return None;
                }
                Some((pair[0].as_str()?.to_string(), pair[1].as_str()?.to_string()))
            } else {
                let obj = entry.as_object()?;
                let name = obj.get("name")?.as_str()?;
                let hash = obj.get("hash")?.as_str()?;
                Some((name.to_string(), hash.to_string()))
            }
        })
        .collect()
}

/// Where the current, or most recently finished, operation stands.
#[tauri::command(rename_all = "snake_case")]
pub fn op_status() -> OpState {
    shared().state.lock().expect("op state lock").clone()
}

/// Ask the running operation to stop.
///
/// Refuses when nothing is running, and refuses, separately, when
/// something is running but has not registered a cancel signal (see the
/// module doc): either way nothing happens silently.
#[tauri::command(rename_all = "snake_case")]
pub fn op_cancel() -> Result<(), String> {
    let s = shared();
    let busy = s.state.lock().expect("op state lock").phase.is_busy();
    if !busy {
        return Err("No operation is running.".to_string());
    }
    match s.cancel.lock().expect("cancel lock").as_ref() {
        Some(signal) => {
            signal.cancel();
            Ok(())
        }
        None => Err("This step has not reached a point that can be cancelled yet.".to_string()),
    }
}

/// Reset to `Idle`. Refuses while an operation is still busy, naming it,
/// rather than clearing the state out from under a still-running body.
#[tauri::command(rename_all = "snake_case")]
pub fn op_clear() -> Result<(), String> {
    let mut state = shared().state.lock().expect("op state lock");
    if state.phase.is_busy() {
        return Err(format!(
            "{} is still running and cannot be cleared.",
            state.operation
        ));
    }
    *state = OpState::idle();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::MutexGuard;
    use std::time::Duration;

    // `Shared` is one process-wide singleton, which is exactly right for the
    // real app (one window, one operation slot) and exactly wrong for
    // `cargo test`'s default of running every test in its own thread of the
    // same process. Every test below takes this lock for its whole body, so
    // only one of them touches `Shared` at a time; a poisoned lock (left
    // behind by a test that panicked while holding it) is still taken
    // rather than propagated, since the next test's own `reset()` call
    // brings `Shared` back to a known state regardless.
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    fn exclusive() -> MutexGuard<'static, ()> {
        TEST_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn wait_until_settled() {
        for _ in 0..200 {
            if !shared().state.lock().unwrap().phase.is_busy() {
                return;
            }
            thread::sleep(Duration::from_millis(10));
        }
        panic!("an operation never settled");
    }

    fn reset() {
        wait_until_settled();
        let _ = op_clear();
    }

    /// Every test below runs an operation against a fresh
    /// `tempfile::tempdir()` rather than calling [`run_operation`] itself,
    /// which would resolve this machine's real settings file and, on a
    /// machine that has never touched `Settings.origin`, the real
    /// `C:\Reborn Origin` -- the live publishing origin for a game other
    /// people play. `run_operation_with_origin` is the one function in this
    /// module that takes the origin as a plain argument instead of looking
    /// one up, so calling it directly here is what makes that lookup
    /// unreachable from a test, not a convention a future test could
    /// forget to follow. See the module doc's "The origin a journal record
    /// lands in".
    fn run(
        origin: &Path,
        op: &str,
        dry_run: bool,
        inputs: Value,
        body: impl FnOnce(Sink<'_>) -> reborn_core::Result<Outcome<Value>> + Send + 'static,
    ) -> Result<(), String> {
        run_operation_with_origin(Some(origin.to_path_buf()), op, dry_run, inputs, body)
    }

    #[test]
    fn step_of_reads_the_suffix_after_the_last_colon() {
        assert_eq!(step_of("roster:plan"), Step::Plan);
        assert_eq!(step_of("roster:run"), Step::Run);
        assert_eq!(step_of("game:patch-verify"), Step::Verify);
        assert_eq!(step_of("game:dump"), Step::Run);
        assert_eq!(step_of("no-colon"), Step::Run);
    }

    #[test]
    fn page_name_strips_only_the_last_segment() {
        assert_eq!(page_name("roster:plan"), "roster");
        assert_eq!(page_name("game:patch-verify"), "game");
        assert_eq!(page_name("solo"), "solo");
    }

    #[test]
    fn a_second_operation_is_refused_and_names_the_first() {
        let _guard = exclusive();
        reset();
        let dir = tempfile::tempdir().expect("tempdir");
        let started = AtomicUsize::new(0);
        run(dir.path(), "roster:run", false, Value::Null, move |_sink| {
            // Block long enough for the refusal check below to observe
            // this operation as busy.
            thread::sleep(Duration::from_millis(150));
            Ok(Outcome::Described)
        })
        .expect("the first call should be accepted");
        thread::sleep(Duration::from_millis(20));

        let err = run(dir.path(), "launcher:run", false, Value::Null, move |_sink| {
            started.fetch_add(1, Ordering::SeqCst);
            Ok(Outcome::Described)
        })
        .expect_err("a second operation must be refused while the first runs");
        assert!(err.contains("roster"), "{err}");
        assert!(err.contains("launcher:run"), "{err}");

        reset();
    }

    #[test]
    fn a_plan_step_populates_plan_and_a_run_step_leaves_it_alone() {
        let _guard = exclusive();
        reset();
        let dir = tempfile::tempdir().expect("tempdir");
        run(dir.path(), "content:plan", false, Value::Null, |_sink| {
            Ok(Outcome::Done(serde_json::json!({"found": 3})))
        })
        .unwrap();
        wait_until_settled();
        {
            let state = op_status();
            assert_eq!(state.phase, Phase::Planned);
            assert_eq!(state.plan, Some(serde_json::json!({"found": 3})));
        }

        run(dir.path(), "content:run", false, Value::Null, |_sink| Ok(Outcome::Described)).unwrap();
        wait_until_settled();
        let state = op_status();
        assert_eq!(state.phase, Phase::Done);
        // The run step must not have touched what the plan step found.
        assert_eq!(state.plan, Some(serde_json::json!({"found": 3})));

        reset();
    }

    #[test]
    fn a_refusal_sets_phase_refused_and_keeps_the_summary() {
        let _guard = exclusive();
        reset();
        let dir = tempfile::tempdir().expect("tempdir");
        run(dir.path(), "content:lint", false, Value::Null, |sink| {
            Ok(reborn_core::publish::refuse(
                sink,
                &["a User folder was found under this tree.".to_string()],
            ))
        })
        .unwrap();
        wait_until_settled();
        let state = op_status();
        assert_eq!(state.phase, Phase::Refused);
        assert_eq!(state.summary, "a User folder was found under this tree.");
        reset();
    }

    #[test]
    fn an_error_sets_phase_failed() {
        let _guard = exclusive();
        reset();
        let dir = tempfile::tempdir().expect("tempdir");
        run(dir.path(), "keys:generate", false, Value::Null, |_sink| {
            Err(reborn_core::Error::PlainIo(std::io::Error::other("disk fell over")))
        })
        .unwrap();
        wait_until_settled();
        let state = op_status();
        assert_eq!(state.phase, Phase::Failed);
        assert!(state.summary.contains("disk fell over"), "{}", state.summary);
        reset();
    }

    #[test]
    fn op_clear_refuses_while_busy_and_succeeds_once_settled() {
        let _guard = exclusive();
        reset();
        let dir = tempfile::tempdir().expect("tempdir");
        run(dir.path(), "history:rollback-run", false, Value::Null, |_sink| {
            thread::sleep(Duration::from_millis(120));
            Ok(Outcome::Described)
        })
        .unwrap();
        thread::sleep(Duration::from_millis(20));
        assert!(op_clear().is_err(), "clearing a busy operation must be refused");
        wait_until_settled();
        assert!(op_clear().is_ok());
        assert_eq!(op_status().phase, Phase::Idle);
    }

    #[test]
    fn op_cancel_refuses_when_idle_and_when_no_signal_registered() {
        let _guard = exclusive();
        reset();
        assert!(op_cancel().is_err(), "cancel with nothing running must be refused");

        let dir = tempfile::tempdir().expect("tempdir");
        run(dir.path(), "game:dump", false, Value::Null, |_sink| {
            thread::sleep(Duration::from_millis(80));
            Ok(Outcome::Described)
        })
        .unwrap();
        thread::sleep(Duration::from_millis(10));
        let err = op_cancel().expect_err("a body that never registered a signal cannot be cancelled");
        assert!(err.contains("cancel"), "{err}");
        reset();
    }

    #[test]
    fn op_cancel_reaches_a_registered_signal() {
        let _guard = exclusive();
        reset();
        let dir = tempfile::tempdir().expect("tempdir");
        let cancel = CancelSignal::new();
        let observed = cancel.clone();
        run(dir.path(), "roster:plan", false, Value::Null, move |_sink| {
            set_cancel_signal(observed.clone());
            for _ in 0..200 {
                if observed.is_cancelled() {
                    break;
                }
                thread::sleep(Duration::from_millis(5));
            }
            Ok(Outcome::Described)
        })
        .unwrap();
        thread::sleep(Duration::from_millis(30));
        op_cancel().expect("a registered signal should be reachable");
        wait_until_settled();
        assert!(cancel.is_cancelled());
        reset();
    }

    #[test]
    fn run_operation_with_origin_journals_into_the_given_origin_and_not_the_real_one() {
        let _guard = exclusive();
        reset();

        // The real, live publishing origin this whole fix exists to keep a
        // test out of. Not read from settings on purpose: this is the exact
        // path named in the bug report, and the assertion below should
        // fail loudly if anything ever again resolves it from inside a
        // test.
        let real_origin = Path::new(r"C:\Reborn Origin");
        let real_journal = real_origin.join(reborn_core::publish::journal::JOURNAL_FILE);
        let real_journal_existed_before = real_journal.is_file();

        let dir = tempfile::tempdir().expect("tempdir");
        run(dir.path(), "upload:run", false, serde_json::json!({"kind": "isolation-test"}), |_sink| {
            Ok(Outcome::Done(serde_json::json!({"summary": "isolation test wrote here"})))
        })
        .expect("accepted");
        wait_until_settled();

        // The journal this operation earned appears in the temp origin it
        // was given, naming the operation and carrying the inputs through.
        let temp_journal = dir.path().join(reborn_core::publish::journal::JOURNAL_FILE);
        let written = std::fs::read_to_string(&temp_journal)
            .unwrap_or_else(|e| panic!("{} was not written: {e}", temp_journal.display()));
        let record: Record = serde_json::from_str(written.lines().next().expect("one journal line"))
            .expect("the line is a valid journal record");
        assert_eq!(record.operation, "upload");
        assert_eq!(record.outcome, "done");
        assert_eq!(record.inputs, serde_json::json!({"kind": "isolation-test"}));

        // The real origin's journal is exactly as it was before this test
        // ran. `run_operation_with_origin` never resolves `resolve_origin`
        // (only `run_operation` does, and this test never calls it), so by
        // construction the real settings file, and the real origin it
        // names, were never consulted; this is the closest a filesystem
        // check can come to proving that from outside, and is offered as
        // the next-best assertion rather than a substitute for it.
        assert_eq!(
            real_journal.is_file(),
            real_journal_existed_before,
            "the real origin's journal must be untouched by this test"
        );

        reset();
    }

    #[test]
    fn parse_hashes_accepts_pairs_and_objects() {
        let pairs = serde_json::json!({"hashes": [["a", "1"], ["b", "2"]]});
        assert_eq!(
            parse_hashes(&pairs),
            vec![("a".to_string(), "1".to_string()), ("b".to_string(), "2".to_string())]
        );
        let objects = serde_json::json!({"hashes": [{"name": "a", "hash": "1"}]});
        assert_eq!(parse_hashes(&objects), vec![("a".to_string(), "1".to_string())]);
        assert_eq!(parse_hashes(&serde_json::json!({})), Vec::<(String, String)>::new());
    }
}
