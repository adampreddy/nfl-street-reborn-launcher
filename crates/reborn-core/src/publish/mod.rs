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

use crate::inventory::ComponentId;
use crate::manifest::Component;
use crate::{Error, Result};

pub mod keys;
pub mod component;
pub mod release;
pub mod vault;
pub mod roster;
pub mod game_patch;
pub mod upload;
pub mod journal;

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

/// One human line from a publishing operation, exactly as the CLI prints it.
///
/// `#[non_exhaustive]`: nothing outside this module builds one with a struct
/// literal (see [`Event::out`] and [`Event::err`]), and this makes that a
/// compiler rule instead of a convention, so a progress field can be added
/// later without a breaking change.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Event {
    pub channel: Channel,
    pub line: String,
}

impl Event {
    pub fn out(line: impl Into<String>) -> Self {
        Self { channel: Channel::Out, line: line.into() }
    }
    pub fn err(line: impl Into<String>) -> Self {
        Self { channel: Channel::Err, line: line.into() }
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
                Err(_) => {
                    let path = entry.path();
                    return Err(Error::Io {
                        path: path.clone(),
                        source: std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            format!("Failed to parse component manifest {}. Fix or remove the file before retrying.", path.display()),
                        ),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::ComponentId;

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
