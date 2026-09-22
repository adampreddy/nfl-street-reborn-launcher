//! The publisher's side of updates: turn a tree into an origin a launcher can
//! update from. One file per verb. Every verb takes a request, an event sink
//! for its human lines, and returns an [`Outcome`].
//!
//! The origin is a directory. Every file it holds is served as-is by any
//! static host. Nothing here talks to a network except the roster fetch.
//!
//! A published file is never rewritten. A blob is content-addressed and so
//! immutable; a component manifest that exists with different bytes is refused;
//! `release.json` only ever moves to a strictly higher sequence. The bytes in
//! the origin are hashed after every copy, and the signed release is read back
//! and verified before success is reported.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::inventory::ComponentId;
use crate::manifest::Component;
use crate::{Error, Result};

pub mod keys;
pub mod component;
pub mod release;
pub mod vault;
pub mod roster;
pub mod game_patch;
pub mod game_full;
pub mod upload;
pub mod journal;
pub mod status;
pub mod download;

/// Where component manifests live under the origin.
pub const COMPONENTS_DIR: &str = "components";
/// The everyday key file name.
pub const PUBLISHER_KEY: &str = "publisher.key";
/// The rotation key file name. Generated with the first, then taken offline.
pub const ROTATION_KEY: &str = "rotation.key";

/// Where a human line goes. The CLI prints `Out` to stdout and `Err` to
/// stderr, which is where each line went before the move. An app shows both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    Out,
    Err,
}

/// How far a long transfer has come, in bytes, so a window can draw a bar.
/// Rides on an ordinary [`Event`] line (the line the CLI prints anyway), so
/// a console sees exactly what it always saw and an app gets the numbers.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Progress {
    pub done: u64,
    pub total: u64,
    /// What is moving, in a few words: "Uploading", "Uploading part 3 of 9".
    pub label: String,
}

/// One human line from a publishing operation, exactly as the CLI prints it.
///
/// `#[non_exhaustive]`: nothing outside this module builds one with a struct
/// literal (see [`Event::out`] and [`Event::err`]), and this makes that a
/// compiler rule instead of a convention, which is what let `progress` be
/// added without touching a single caller.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Event {
    pub channel: Channel,
    pub line: String,
    /// Present on the lines that mark bytes moving; `None` on every other
    /// line. A console ignores it; the Publisher window draws a bar from it.
    pub progress: Option<Progress>,
}

impl Event {
    pub fn out(line: impl Into<String>) -> Self {
        Self { channel: Channel::Out, line: line.into(), progress: None }
    }
    pub fn err(line: impl Into<String>) -> Self {
        Self { channel: Channel::Err, line: line.into(), progress: None }
    }
    /// The same line, carrying how far a transfer has come.
    pub fn with_progress(mut self, done: u64, total: u64, label: impl Into<String>) -> Self {
        self.progress = Some(Progress { done, total, label: label.into() });
        self
    }
}

/// Where events go. A closure, so a CLI prints and an app forwards.
pub type Sink<'a> = &'a mut dyn FnMut(Event);

/// Why an operation stopped without writing. The lines were already emitted;
/// `summary` is the first of them, for a caller that shows one line, and
/// `lines` is all of them, for a caller (a GUI dialog) that needs the whole
/// reason rather than scraping its log pane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    pub summary: String,
    pub lines: Vec<String>,
}

/// What an operation came to.
///
/// `Described` is a dry run: everything was checked and described, nothing
/// was written, and that is success. `Refused` is a stop with a reason, which
/// the CLI reports as a failure exit. An error (`Err`) is something broken:
/// an unreadable file, a failed process.
#[derive(Debug)]
pub enum Outcome<T> {
    Done(T),
    Described,
    Refused(Refusal),
}

impl<T> Outcome<T> {
    pub fn is_done(&self) -> bool {
        matches!(self, Outcome::Done(_))
    }
    pub fn refused(&self) -> Option<&Refusal> {
        match self {
            Outcome::Refused(r) => Some(r),
            _ => None,
        }
    }
}

/// Emit every line to `Out` and stop with the first as the summary and all
/// of them kept.
pub fn refuse<T>(sink: Sink<'_>, lines: &[String]) -> Outcome<T> {
    for line in lines {
        sink(Event::out(line.clone()));
    }
    Outcome::Refused(Refusal {
        summary: lines.first().cloned().unwrap_or_default(),
        lines: lines.to_vec(),
    })
}

/// `out!(sink, "fmt", args)`: one stdout line. `out!(sink)` is a blank line.
#[macro_export]
macro_rules! out {
    ($sink:expr) => { ($sink)($crate::publish::Event::out(String::new())) };
    ($sink:expr, $($arg:tt)*) => { ($sink)($crate::publish::Event::out(format!($($arg)*))) };
}

/// `err!(sink, "fmt", args)`: one stderr line.
#[macro_export]
macro_rules! err {
    ($sink:expr, $($arg:tt)*) => { ($sink)($crate::publish::Event::err(format!($($arg)*))) };
}

pub use crate::{err, out};

/// Where a component manifest lives under the origin.
pub fn manifest_path(origin: &Path, id: ComponentId, version: &str) -> PathBuf {
    origin.join(COMPONENTS_DIR).join(format!("{}-{}.json", id.as_str(), version))
}

/// The address a release pins a component manifest at, relative to the origin.
pub fn manifest_url(id: ComponentId, version: &str) -> String {
    format!("{COMPONENTS_DIR}/{}-{}.json", id.as_str(), version)
}

/// Why a component manifest already in the origin could not be used by
/// [`next_sequence`] to compute the next sequence number.
///
/// This does not add a variant to [`crate::Error`]: that type lives in
/// [`crate::platform`], out of scope for this fix. Instead it travels as the
/// source of an [`Error::Io`], which is otherwise also how a genuine
/// filesystem failure (permission denied, not found) is reported here. A
/// caller that needs to tell "this manifest is corrupt, fix or remove it"
/// apart from "the disk could not be read" (the desktop app's History page
/// is exactly such a caller) uses [`SequenceProblem::from_error`] rather
/// than being unable to tell the two apart, which is what plainly formatting
/// the message into `Error::Io` used to mean.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SequenceProblem {
    /// The file at `path` was read from disk successfully but its bytes did
    /// not parse as a component manifest. `why` is the parse error.
    Unreadable { path: PathBuf, why: String },
}

impl std::fmt::Display for SequenceProblem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SequenceProblem::Unreadable { path, .. } => write!(
                f,
                "Failed to parse component manifest {}. Fix or remove the file before retrying.",
                path.display()
            ),
        }
    }
}

impl std::error::Error for SequenceProblem {}

impl SequenceProblem {
    /// `err`'s [`SequenceProblem`], if it carries one. `None` for every other
    /// `Error`, including an `Error::Io` that is a genuine filesystem
    /// failure rather than a corrupt manifest.
    pub fn from_error(err: &Error) -> Option<&SequenceProblem> {
        match err {
            Error::Io { source, .. } => source.get_ref().and_then(|e| e.downcast_ref::<SequenceProblem>()),
            _ => None,
        }
    }
}

/// One above the highest sequence of any manifest of this component in the
/// origin. 1 when there is none.
pub fn next_sequence(origin: &Path, component: ComponentId) -> Result<u64> {
    let dir = origin.join(COMPONENTS_DIR);
    let prefix = format!("{}-", component.as_str());
    let mut highest = 0u64;
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !name.starts_with(&prefix) || !name.ends_with(".json") {
                continue;
            }
            let bytes = std::fs::read(entry.path()).map_err(|e| io_err(&entry.path(), e))?;
            match Component::parse(&bytes) {
                Ok(c) => {
                    highest = highest.max(c.sequence);
                }
                Err(parse_err) => {
                    let path = entry.path();
                    let problem = SequenceProblem::Unreadable { path: path.clone(), why: parse_err.to_string() };
                    return Err(Error::Io {
                        path,
                        source: std::io::Error::new(std::io::ErrorKind::InvalidData, problem),
                    });
                }
            }
        }
    }
    Ok(highest + 1)
}

/// 1234567 as "1,234,567". The CLI has its own copy; this one is for the
/// lines the core writes.
pub fn thousands(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

pub(crate) fn io_err(path: &Path, source: std::io::Error) -> Error {
    Error::Io { path: path.to_path_buf(), source }
}

/// Days since 1970-01-01 to a (year, month, day) triple, proleptic
/// Gregorian. Howard Hinnant's `civil_from_days`:
/// <http://howardhinnant.github.io/date_algorithms.html>. Shared by
/// [`format_rfc3339`] and `upload::format_amz_date`, which each need the
/// same civil-date conversion for a different string format and used to
/// carry an identical, independently hand-typed copy of it; a fix to one
/// used to have no way of reaching the other. Chosen over reaching for a
/// time crate for this one conversion: it is a small, closed, well-known
/// algorithm, not a timezone database.
pub(crate) fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    let year = if month <= 2 { y + 1 } else { y };
    (year, month, day)
}

/// An RFC 3339 timestamp, stamped UTC with an explicit `+00:00` offset:
/// `"{y}-{m}-{d}T{h}:{m}:{s}+00:00"`, built from [`civil_from_days`]. Used
/// for every timestamp this crate writes into a file a launcher or the
/// site reads back (a journal record's `at`, a download pointer's
/// `published`), so the two can never drift into disagreeing about the
/// format.
///
/// This used to be two independently hand-typed copies, one in
/// `journal.rs` (predating `download.rs`) and one in `download.rs` (added
/// by a task scoped to that one file, which could not reach into
/// `journal.rs`'s private copy). Hoisted here once a task touched both:
/// the same shared home `civil_from_days` already has, for the same
/// reason -- one algorithm, one place a fix reaches every caller.
pub(crate) fn format_rfc3339(t: SystemTime) -> String {
    let secs = t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
    let days = (secs / 86_400) as i64;
    let time_of_day = secs % 86_400;
    let (year, month, day) = civil_from_days(days);
    let hour = time_of_day / 3600;
    let minute = (time_of_day % 3600) / 60;
    let second = time_of_day % 60;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}+00:00")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::ComponentId;

    // -------------------------------------------------------------------
    // format_rfc3339: moved here from journal.rs's own test module when
    // the function itself was hoisted to be shared with download.rs. Same
    // known-instants table, unchanged.
    // -------------------------------------------------------------------

    #[test]
    fn format_rfc3339_matches_known_utc_instants() {
        let cases: &[(u64, &str)] = &[
            (0, "1970-01-01T00:00:00+00:00"),
            (86_399, "1970-01-01T23:59:59+00:00"),
            (951_782_400, "2000-02-29T00:00:00+00:00"), // leap day
            (1_757_430_727, "2025-09-09T15:12:07+00:00"),
            (1_893_456_000, "2030-01-01T00:00:00+00:00"), // year boundary
        ];
        for (epoch, want) in cases {
            let t = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(*epoch);
            assert_eq!(&format_rfc3339(t), want, "epoch {epoch}");
        }
    }

    #[test]
    fn thousands_groups_digits() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(1000), "1,000");
        assert_eq!(thousands(1234567), "1,234,567");
    }

    #[test]
    fn manifest_names_follow_component_and_version() {
        let origin = std::path::Path::new("C:\\o");
        assert_eq!(
            manifest_path(origin, ComponentId::Runtime, "1.0.2"),
            origin.join("components").join("runtime-1.0.2.json")
        );
        assert_eq!(manifest_url(ComponentId::Roster, "2026.09.06"), "components/roster-2026.09.06.json");
    }

    #[test]
    fn next_sequence_is_one_above_the_highest_manifest_of_that_component() {
        let dir = tempfile::tempdir().unwrap();
        let components = dir.path().join("components");
        std::fs::create_dir_all(&components).unwrap();
        // Two runtime manifests and one roster manifest, minimal but parseable.
        for (name, seq) in [("runtime-1.0.0.json", 1u64), ("runtime-1.0.1.json", 7), ("roster-2026.09.06.json", 3)] {
            let component = name.split('-').next().unwrap();
            let version = name.trim_start_matches(component).trim_start_matches('-').trim_end_matches(".json");
            let text = format!(
                "{{\"schema\":\"nsr.component.v1\",\"component\":\"{component}\",\"version\":\"{version}\",\"sequence\":{seq},\"install_root\":\"x\",\"customizable\":false,\"files\":[]}}"
            );
            std::fs::write(components.join(name), text).unwrap();
        }
        assert_eq!(next_sequence(dir.path(), ComponentId::Runtime).unwrap(), 8);
        assert_eq!(next_sequence(dir.path(), ComponentId::Roster).unwrap(), 4);
        assert_eq!(next_sequence(dir.path(), ComponentId::Textures).unwrap(), 1);
        let empty = tempfile::tempdir().unwrap();
        assert_eq!(next_sequence(empty.path(), ComponentId::Runtime).unwrap(), 1);
    }

    #[test]
    fn refuse_emits_every_line_and_keeps_the_first_as_summary() {
        let mut seen: Vec<Event> = Vec::new();
        let outcome: Outcome<()> = refuse(&mut |e| seen.push(e), &["first line".to_string(), "second".to_string()]);
        assert_eq!(seen.len(), 2);
        assert_eq!(seen[0], Event::out("first line"));
        assert_eq!(seen[1].channel, Channel::Out);
        match outcome {
            Outcome::Refused(r) => {
                assert_eq!(r.summary, "first line");
                assert_eq!(r.lines, vec!["first line".to_string(), "second".to_string()]);
            }
            _ => panic!("expected Refused"),
        }
    }

    #[test]
    fn out_and_err_macros_route_to_their_channels() {
        let mut seen: Vec<Event> = Vec::new();
        let sink: &mut dyn FnMut(Event) = &mut |e| seen.push(e);
        out!(sink, "hello {}", 1);
        err!(sink, "world");
        assert_eq!(seen, vec![Event::out("hello 1"), Event::err("world")]);
    }

    #[test]
    fn next_sequence_refuses_a_corrupt_manifest_of_the_target_component() {
        let dir = tempfile::tempdir().unwrap();
        let components = dir.path().join("components");
        std::fs::create_dir_all(&components).unwrap();
        // Good runtime-1.0.0.json
        let text = format!("{{\"schema\":\"nsr.component.v1\",\"component\":\"runtime\",\"version\":\"1.0.0\",\"sequence\":1,\"install_root\":\"x\",\"customizable\":false,\"files\":[]}}");
        std::fs::write(components.join("runtime-1.0.0.json"), text).unwrap();
        // Corrupt runtime-1.0.1.json (truncated JSON)
        std::fs::write(components.join("runtime-1.0.1.json"), "{\"schema\":\"nsr").unwrap();

        let result = next_sequence(dir.path(), ComponentId::Runtime);
        assert!(result.is_err(), "Expected error for corrupt runtime manifest, got Ok");
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("runtime-1.0.1.json"), "Error should name the corrupt file: {}", err_msg);
    }

    /// The typed distinction a caller (the desktop app's History page) needs:
    /// a manifest that parsed the bytes fine and found garbage must be
    /// reported differently than a manifest whose bytes could not be read at
    /// all, even though both surface as `Error::Io`.
    #[test]
    fn next_sequence_distinguishes_a_corrupt_manifest_from_an_unreadable_one() {
        let dir = tempfile::tempdir().unwrap();
        let components = dir.path().join("components");
        std::fs::create_dir_all(&components).unwrap();

        // Corrupt: the bytes are read fine, but do not parse.
        std::fs::write(components.join("runtime-1.0.1.json"), "{\"schema\":\"nsr").unwrap();
        let corrupt = next_sequence(dir.path(), ComponentId::Runtime).unwrap_err();
        match SequenceProblem::from_error(&corrupt) {
            Some(SequenceProblem::Unreadable { path, why }) => {
                assert!(path.ends_with("runtime-1.0.1.json"), "{}", path.display());
                assert!(!why.is_empty(), "why should carry the parse error");
            }
            None => panic!("a corrupt manifest must carry a SequenceProblem: {corrupt}"),
        }

        // Unreadable: nothing named runtime-1.0.1.json can be read as a file
        // at all (a directory sits where the manifest was), so this fails at
        // the `std::fs::read` step rather than at parse. This must NOT carry
        // a SequenceProblem: the file was never read, so there is nothing to
        // say did not parse.
        std::fs::remove_file(components.join("runtime-1.0.1.json")).unwrap();
        std::fs::create_dir_all(components.join("runtime-1.0.1.json")).unwrap();
        let unreadable = next_sequence(dir.path(), ComponentId::Runtime).unwrap_err();
        assert!(
            SequenceProblem::from_error(&unreadable).is_none(),
            "an unreadable file must not be mistaken for a corrupt manifest: {unreadable}"
        );
    }

    #[test]
    fn next_sequence_ignores_corrupt_manifest_of_different_component() {
        let dir = tempfile::tempdir().unwrap();
        let components = dir.path().join("components");
        std::fs::create_dir_all(&components).unwrap();
        // Good runtime-1.0.0.json
        let text = format!("{{\"schema\":\"nsr.component.v1\",\"component\":\"runtime\",\"version\":\"1.0.0\",\"sequence\":5,\"install_root\":\"x\",\"customizable\":false,\"files\":[]}}");
        std::fs::write(components.join("runtime-1.0.0.json"), text).unwrap();
        // Corrupt roster-x.json (truncated JSON) - should be ignored
        std::fs::write(components.join("roster-x.json"), "{\"schema\":\"nsr").unwrap();

        let result = next_sequence(dir.path(), ComponentId::Runtime);
        assert_eq!(result.unwrap(), 6, "Should ignore corrupt roster manifest and return one above runtime");
    }
}
