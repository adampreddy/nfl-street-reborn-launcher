//! The launcher's own log.
//!
//! # Why this module exists
//!
//! Three waves of fixes took raw technical detail off the screens a player
//! reads and said the detail "goes to the log". There was no log. The
//! workspace had 56 `tracing::warn!` call sites and no subscriber at all: no
//! `tracing-subscriber` anywhere, no `set_global_default` anywhere. A
//! `tracing` event with no subscriber is dropped without being written, so the
//! detail was not moved off the screen. It was deleted.
//!
//! This is the subscriber. It writes warnings and errors to a file under
//! [`DataDir::Logs`], and [`crate::platform::Error::player_message`],
//! `identity`, `stats`, `backend` and `session` all become true when they say
//! the detail is kept.
//!
//! # Why it is written here rather than taken from a crate
//!
//! `tracing-subscriber` and `tracing-appender` would do this, and would bring
//! roughly a dozen transitive dependencies into a product that ships as one
//! installer. What is needed is small and completely specified: warnings and
//! above, one line each, to one file, with a hard bound on how large that file
//! may become. Writing it here also means the bound is exactly the bound this
//! product wants rather than whatever an appender's rotation policy happens to
//! be, and there is no version of it that silently starts buffering a
//! player's log in a background thread that may not flush.
//!
//! # What is never written
//!
//! No token, no device code and no credential. That is not left to care at the
//! call sites: [`tests::no_warning_in_this_crate_interpolates_a_secret`] reads
//! every `tracing` call in this crate and fails if one of them interpolates a
//! value whose name says it is a secret.
//!
//! # The bound
//!
//! Two files, each at most [`MAX_LOG_BYTES`]. When the live file would pass
//! the cap it is renamed over the previous one and a new live file is started,
//! so the log occupies at most twice the cap forever and the most recent
//! entries are always the ones kept.

use std::fmt::Write as _;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use tracing::field::{Field, Visit};
use tracing::level_filters::LevelFilter;
use tracing::span::{Attributes, Id, Record};
use tracing::{Event, Level, Metadata, Subscriber};

use crate::platform::{DataDir, Platform, Result};

/// The live log file, inside [`DataDir::Logs`].
pub const LOG_FILE: &str = "launcher.log";

/// The previous log file. The live one is renamed over this when it fills.
pub const PREVIOUS_LOG_FILE: &str = "launcher.previous.log";

/// The most either file may hold. Two files, so the log is bounded at twice
/// this and never grows without limit on a machine that is never looked at.
pub const MAX_LOG_BYTES: u64 = 256 * 1024;

/// The longest single line. A `Debug` of something large would otherwise be
/// the whole file.
const MAX_LINE_CHARS: usize = 2000;

/// Where the live log file is.
pub fn log_path(platform: &Platform) -> Result<PathBuf> {
    Ok(platform.data_dir(DataDir::Logs)?.join(LOG_FILE))
}

/// Where the previous log file is.
pub fn previous_log_path(platform: &Platform) -> Result<PathBuf> {
    Ok(platform.data_dir(DataDir::Logs)?.join(PREVIOUS_LOG_FILE))
}

/// Start writing this process's warnings to the launcher's log.
///
/// Returns where it is writing, or `None` when there is nowhere to write and
/// when a log has already been started for this process. Never fails and never
/// reports: a launcher that cannot open its own log still has to start, and
/// there is no version of "the log could not be opened" that a player should
/// ever be shown.
///
/// Call it once, first thing in `main`.
pub fn start(platform: &Platform) -> Option<PathBuf> {
    let path = log_path(platform).ok()?;
    let previous = previous_log_path(platform).ok()?;
    let log = FileLog::new(&path, &previous);
    match tracing::subscriber::set_global_default(log) {
        Ok(()) => Some(path),
        Err(_) => None,
    }
}

/// A `tracing` subscriber that appends warnings and errors to a file.
///
/// Spans are accepted and ignored. Nothing in this codebase opens one, and a
/// subscriber that refuses them would make one added later silently panic.
#[derive(Debug)]
pub struct FileLog {
    sink: Mutex<Sink>,
    next_span: AtomicU64,
}

impl FileLog {
    /// Write to `path`, rolling into `previous` when it fills.
    pub fn new(path: &Path, previous: &Path) -> Self {
        Self {
            sink: Mutex::new(Sink::new(path, previous)),
            next_span: AtomicU64::new(1),
        }
    }

    /// One line for one event: when, how bad, where from, and what it said.
    fn line(event: &Event<'_>) -> String {
        let meta = event.metadata();
        let mut line = String::new();
        let _ = write!(
            line,
            "{} {:<5} {}",
            crate::utc_now_iso(),
            meta.level().as_str(),
            meta.target()
        );
        let mut fields = OneLine {
            out: &mut line,
            wrote_message: false,
        };
        event.record(&mut fields);
        line.push('\n');
        line
    }
}

impl Subscriber for FileLog {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        // `Level` orders TRACE highest and ERROR lowest, so this is warnings
        // and errors and nothing else.
        *metadata.level() <= Level::WARN
    }

    /// Warnings and above, stated so `tracing` can skip the rest of the
    /// callsites without ever calling this subscriber.
    fn max_level_hint(&self) -> Option<LevelFilter> {
        Some(LevelFilter::WARN)
    }

    fn new_span(&self, _span: &Attributes<'_>) -> Id {
        // Ids must be non-zero, and each has to differ from the last.
        Id::from_u64(self.next_span.fetch_add(1, Ordering::Relaxed))
    }

    fn record(&self, _span: &Id, _values: &Record<'_>) {}

    fn record_follows_from(&self, _span: &Id, _follows: &Id) {}

    fn event(&self, event: &Event<'_>) {
        if !self.enabled(event.metadata()) {
            return;
        }
        let line = Self::line(event);
        // A poisoned lock is recovered rather than propagated. The log is the
        // least important thing in the process and must never be the reason
        // anything else stops.
        let mut sink = match self.sink.lock() {
            Ok(sink) => sink,
            Err(poisoned) => poisoned.into_inner(),
        };
        sink.append(&line);
    }

    fn enter(&self, _span: &Id) {}

    fn exit(&self, _span: &Id) {}
}

/// Formats an event's fields onto one line.
///
/// The `message` field is the sentence and goes first, unquoted, because
/// `tracing` hands it over as `fmt::Arguments`. Everything else is appended as
/// `name=value`.
struct OneLine<'a> {
    out: &'a mut String,
    wrote_message: bool,
}

impl Visit for OneLine<'_> {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" && !self.wrote_message {
            self.wrote_message = true;
            let _ = write!(self.out, "  {value:?}");
        } else {
            let _ = write!(self.out, " {}={value:?}", field.name());
        }
    }
}

/// The file itself, and the rule that keeps it from growing without bound.
#[derive(Debug)]
struct Sink {
    path: PathBuf,
    previous: PathBuf,
    file: Option<std::fs::File>,
    written: u64,
}

impl Sink {
    fn new(path: &Path, previous: &Path) -> Self {
        Self {
            path: path.to_path_buf(),
            previous: previous.to_path_buf(),
            file: None,
            written: 0,
        }
    }

    fn append(&mut self, line: &str) {
        // One line, always. A newline inside a message would otherwise make
        // one event look like several, and a very long `Debug` would be the
        // whole file.
        let mut text: String = line
            .trim_end_matches('\n')
            .chars()
            .map(|c| if c.is_control() { ' ' } else { c })
            .take(MAX_LINE_CHARS)
            .collect();
        text.push('\n');

        if self.file.is_none() && !self.open() {
            return;
        }
        if self.written.saturating_add(text.len() as u64) > MAX_LOG_BYTES {
            self.roll();
            if self.file.is_none() {
                return;
            }
        }
        let Some(file) = self.file.as_mut() else {
            return;
        };
        // A write that fails is dropped. There is nowhere else to report it:
        // reporting a log failure through the log is a loop, and reporting it
        // to a player is exactly the raw technical text this log exists to
        // keep off their screen.
        if file.write_all(text.as_bytes()).is_ok() {
            self.written = self.written.saturating_add(text.len() as u64);
            let _ = file.flush();
        } else {
            self.file = None;
        }
    }

    /// Open the live file for appending, and remember how large it already is.
    fn open(&mut self) -> bool {
        if let Some(parent) = self.path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
        {
            Ok(file) => {
                self.written = file.metadata().map(|m| m.len()).unwrap_or(0);
                self.file = Some(file);
                true
            }
            Err(_) => false,
        }
    }

    /// Rename the live file over the previous one and start a new live file,
    /// so the two together are bounded and the newest entries are kept.
    fn roll(&mut self) {
        self.file = None;
        let _ = std::fs::remove_file(&self.previous);
        if std::fs::rename(&self.path, &self.previous).is_err() {
            // Rolling failed, so truncating is the only way left to honor the
            // bound. Losing the oldest warnings is better than a file that
            // fills a player's disk.
            let _ = std::fs::write(&self.path, b"");
        }
        self.written = 0;
        self.open();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A warning actually reaches a file on disk.
    ///
    /// The whole point of this module, and the thing three fix waves assumed
    /// without checking. This drives the real `tracing::warn!` macro through
    /// the real subscriber and then reads the bytes back off the disk, so it
    /// proves the path from the macro to the file rather than that the code
    /// compiles.
    #[test]
    fn a_warning_reaches_a_file_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(LOG_FILE);
        let previous = dir.path().join(PREVIOUS_LOG_FILE);

        tracing::subscriber::with_default(FileLog::new(&path, &previous), || {
            tracing::warn!("the results queue could not be opened");
            tracing::info!("this is below the level and must not be written");
        });

        let written = std::fs::read_to_string(&path)
            .expect("a warning was raised, so the log file has to exist");
        assert!(
            written.contains("the results queue could not be opened"),
            "the warning never reached the file: {written:?}"
        );
        assert!(
            !written.contains("must not be written"),
            "an info event was written to a warnings-and-above log: {written:?}"
        );
        assert_eq!(
            written.lines().count(),
            1,
            "one event has to be one line: {written:?}"
        );
    }

    /// A message with a value interpolated into it keeps the value.
    ///
    /// Every call site in this crate carries its detail in the message, so a
    /// subscriber that recorded only the level and the target would be the
    /// same silence in a different place.
    #[test]
    fn the_detail_a_call_site_carries_is_what_lands_in_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(LOG_FILE);
        let previous = dir.path().join(PREVIOUS_LOG_FILE);

        tracing::subscriber::with_default(FileLog::new(&path, &previous), || {
            let code = "hash-mismatch";
            tracing::warn!("the server refused a result and gave the reason {code:?}");
        });

        let written = std::fs::read_to_string(&path).unwrap();
        assert!(
            written.contains("hash-mismatch"),
            "the detail the call site carries was dropped: {written:?}"
        );
    }

    /// The log cannot grow without bound.
    #[test]
    fn the_log_rolls_rather_than_growing_forever() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(LOG_FILE);
        let previous = dir.path().join(PREVIOUS_LOG_FILE);

        tracing::subscriber::with_default(FileLog::new(&path, &previous), || {
            // Comfortably more than the cap, in messages of a known size.
            let filler = "x".repeat(500);
            for _ in 0..2000 {
                tracing::warn!("{filler}");
            }
            tracing::warn!("the newest warning of all");
        });

        let live = std::fs::metadata(&path).unwrap().len();
        assert!(
            live <= MAX_LOG_BYTES,
            "the live log passed its cap: {live} bytes"
        );
        let rolled = std::fs::metadata(&previous).unwrap().len();
        assert!(
            rolled <= MAX_LOG_BYTES,
            "the rolled log passed its cap: {rolled} bytes"
        );
        assert!(
            std::fs::read_to_string(&path)
                .unwrap()
                .contains("the newest warning of all"),
            "rolling kept the old entries and lost the newest one"
        );
    }

    /// A multi-line value cannot make one event look like several.
    #[test]
    fn one_event_is_always_one_line() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(LOG_FILE);
        let previous = dir.path().join(PREVIOUS_LOG_FILE);

        tracing::subscriber::with_default(FileLog::new(&path, &previous), || {
            let detail = "first line\nsecond line\r\nthird";
            tracing::warn!("something failed: {detail}");
        });

        let written = std::fs::read_to_string(&path).unwrap();
        assert_eq!(written.lines().count(), 1, "got {written:?}");
        assert!(written.contains("third"), "got {written:?}");
    }

    /// Nothing this crate logs interpolates a secret.
    ///
    /// The moment a subscriber exists, every `tracing` call becomes a line in
    /// a file on a player's disk. A bearer token or a device code in one of
    /// them would be a credential written to plaintext, which is the one thing
    /// [`crate::platform::CredentialVault`] exists to prevent.
    ///
    /// # How this is bounded so it cannot find itself
    ///
    /// It reads every `.rs` file under this crate's `src`, and cuts each one
    /// at its first `#[cfg(test)]` before looking at it. This test is inside
    /// `logging.rs`'s own test module, which is after that cut, so the
    /// patterns written below are never part of what is searched. It also
    /// asserts it found call sites at all, so a change that makes the scan
    /// match nothing fails instead of passing silently.
    #[test]
    fn no_warning_in_this_crate_interpolates_a_secret() {
        let src = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/src"));
        let mut files = Vec::new();
        collect_rust_files(src, &mut files);
        assert!(
            files.len() > 20,
            "the source tree was not found, so this test checked nothing"
        );

        // Interpolated into the message, or handed over as a trailing
        // argument. Prose is deliberately not matched: "the poll reply had no
        // token or player" is a sentence about a token, not a token.
        let interpolated = [
            "{token", "{secret", "{password", "{device_code", "{bearer", "{api_key",
        ];
        let arguments = [
            ", token", ", secret", ", password", ", device_code", ", bearer", ", api_key",
            ", self.token", ", &token", ", &self.token",
        ];

        let mut checked = 0usize;
        for file in &files {
            let text = std::fs::read_to_string(file).unwrap();
            let production = &text[..text.find("#[cfg(test)]").unwrap_or(text.len())];
            for call in tracing_calls(production) {
                checked += 1;
                let lower = call.to_ascii_lowercase();
                for needle in interpolated.iter().chain(arguments.iter()) {
                    assert!(
                        !lower.contains(needle),
                        "a log line would write a secret to a file on disk, in {}: {call}",
                        file.display()
                    );
                }
            }
        }
        assert!(
            checked > 40,
            "only {checked} log call sites were found, so the scan is not reading what it \
             thinks it is reading"
        );
    }

    fn collect_rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(listing) = std::fs::read_dir(dir) else {
            return;
        };
        for item in listing.flatten() {
            let path = item.path();
            if path.is_dir() {
                collect_rust_files(&path, out);
            } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                out.push(path);
            }
        }
    }

    /// Every `tracing::<level>!(...)` invocation in `text`, as source slices.
    fn tracing_calls(text: &str) -> Vec<String> {
        let mut calls = Vec::new();
        let mut rest = text;
        while let Some(at) = rest.find("tracing::") {
            let tail = &rest[at..];
            let Some(open) = tail.find('(') else { break };
            // A macro, not a path such as `tracing::warn` in a doc comment:
            // the character before the paren has to be `!`.
            if tail.as_bytes().get(open.wrapping_sub(1)) == Some(&b'!') {
                let mut depth = 0usize;
                let mut end = None;
                for (i, c) in tail[open..].char_indices() {
                    match c {
                        '(' => depth += 1,
                        ')' => {
                            depth -= 1;
                            if depth == 0 {
                                end = Some(open + i);
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                if let Some(end) = end {
                    calls.push(tail[..=end].to_string());
                }
            }
            rest = &tail[open.max(1)..];
        }
        calls
    }
}
