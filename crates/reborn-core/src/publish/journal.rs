//! The publisher's journal: one JSON line per operation, appended to a file
//! in the origin. What a later History page reads to answer "what happened
//! here and when."
//!
//! A corrupt line must not hide the rest of the history: [`read_all`] skips
//! one it cannot parse and counts it, rather than failing the whole read.
//!
//! Nothing written here may be a secret. [`Record::inputs`] is free-form
//! JSON, and this module trusts the caller to have already turned a token
//! into `<set>`/`<unset>` (see `publish::roster::RosterRequest`'s
//! hand-written `Debug` for the pattern) before handing it in; see the test
//! `a_record_never_carries_the_secret_a_request_held` below for the shape
//! that test proves.

use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::publish::io_err;
use crate::{Error, Result};

/// The file this journal lives in, directly under the origin.
pub const JOURNAL_FILE: &str = "publisher-journal.jsonl";

/// Every record must carry exactly this.
pub const JOURNAL_SCHEMA: &str = "nsr.publisher-journal.v1";

/// One line of the journal: one publishing operation, done, described, or
/// refused.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Record {
    /// Must be exactly [`JOURNAL_SCHEMA`].
    pub schema: String,
    /// RFC 3339, stamped by [`now`] as UTC with an explicit `+00:00` offset,
    /// never a local time. Rendering this in the operator's local time, if
    /// wanted, is a display-layer concern; nothing that reads this field
    /// should treat it as already local.
    pub at: String,
    /// `"roster"`, `"game-patch"`, `"release"`, `"component"`, `"upload"`,
    /// `"launcher"`.
    pub operation: String,
    pub dry_run: bool,
    /// Paths and versions the operation was given. Never a secret: the
    /// caller that builds this is responsible for redacting one first, the
    /// same way `RosterRequest`'s `Debug` prints `<set>` rather than a
    /// token.
    pub inputs: serde_json::Value,
    pub sequence: Option<u64>,
    /// (what, hex), for example `("runtime-1.0.2.json", "9f2a...")`.
    pub hashes: Vec<(String, String)>,
    /// Object keys uploaded to the CDN.
    pub uploaded: Vec<String>,
    /// What the read-back after upload said, when the operation checked one.
    pub verified: Option<String>,
    /// `"done"`, `"described"`, or `"refused"`.
    pub outcome: String,
    pub summary: String,
}

/// What [`read_all`] found: the records it could parse, in file order, and
/// how many lines it could not. A blank line is not counted as skipped; it
/// is simply not a record.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ReadAll {
    pub records: Vec<Record>,
    pub skipped: usize,
}

/// Append one record. Creates the file, and the origin directory, if either
/// is absent. Never rewrites or reorders a line already there.
pub fn append(origin: &Path, record: &Record) -> Result<()> {
    std::fs::create_dir_all(origin).map_err(|e| io_err(origin, e))?;
    let path = origin.join(JOURNAL_FILE);
    let line = serde_json::to_string(record).map_err(|e| {
        Error::PlainIo(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("a publisher-journal record would not serialize: {e}"),
        ))
    })?;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| io_err(&path, e))?;
    writeln!(file, "{line}").map_err(|e| io_err(&path, e))?;
    Ok(())
}

/// Every record in the journal, in the order they were appended. An absent
/// file is an empty journal, not an error: an origin nothing has been
/// published to yet has no history rather than a broken one.
pub fn read_all(origin: &Path) -> Result<ReadAll> {
    let path = origin.join(JOURNAL_FILE);
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(ReadAll::default()),
        Err(e) => return Err(io_err(&path, e)),
    };
    let mut records = Vec::new();
    let mut skipped = 0usize;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        match serde_json::from_str::<Record>(line) {
            Ok(record) => records.push(record),
            Err(_) => skipped += 1,
        }
    }
    Ok(ReadAll { records, skipped })
}

/// The last `n` records, in the same order they were appended (oldest of the
/// `n` first). Fewer than `n` records in the journal is not an error: every
/// good record found comes back.
pub fn read_last(origin: &Path, n: usize) -> Result<Vec<Record>> {
    let all = read_all(origin)?;
    let start = all.records.len().saturating_sub(n);
    Ok(all.records[start..].to_vec())
}

/// The current moment as an RFC 3339 timestamp, stamped UTC with an explicit
/// `+00:00` offset. That is deliberate and correct, not a stand-in for a
/// local reading: unambiguous, sortable as text, and never wrong across a
/// daylight-saving change. It is also the only option available from inside
/// this file: finding the operator's local offset needs either an OS call
/// (`GetTimeZoneInformation` on Windows, `tzset`/`localtime_r` elsewhere),
/// which is `unsafe` on every platform and this crate is
/// `#![forbid(unsafe_code)]` (see `lib.rs`), or a timezone dependency, which
/// this task's brief says to avoid rather than add for this. Rendering this
/// in the viewer's local time, if wanted, belongs to whatever displays the
/// record, not to this stamp.
pub fn now() -> String {
    format_rfc3339(SystemTime::now())
}

fn format_rfc3339(t: SystemTime) -> String {
    let secs = t.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    let days = (secs / 86_400) as i64;
    let time_of_day = secs % 86_400;
    let (year, month, day) = crate::publish::civil_from_days(days);
    let hour = time_of_day / 3600;
    let minute = (time_of_day % 3600) / 60;
    let second = time_of_day % 60;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}+00:00")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(operation: &str, summary: &str) -> Record {
        Record {
            schema: JOURNAL_SCHEMA.to_string(),
            at: now(),
            operation: operation.to_string(),
            dry_run: false,
            inputs: serde_json::json!({ "version": "1.0.0" }),
            sequence: Some(1),
            hashes: vec![("runtime-1.0.0.json".to_string(), "abcd".to_string())],
            uploaded: vec!["components/runtime-1.0.0.json".to_string()],
            verified: Some("release.json read back and verified".to_string()),
            outcome: "done".to_string(),
            summary: summary.to_string(),
        }
    }

    // -------------------------------------------------------------------
    // Time formatting
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
            let t = UNIX_EPOCH + std::time::Duration::from_secs(*epoch);
            assert_eq!(&format_rfc3339(t), want, "epoch {epoch}");
        }
    }

    #[test]
    fn now_produces_a_plausible_rfc3339_string() {
        let stamp = now();
        // "YYYY-MM-DDTHH:MM:SS+00:00"
        assert_eq!(stamp.len(), 25, "{stamp}");
        assert_eq!(stamp.as_bytes()[4], b'-');
        assert_eq!(stamp.as_bytes()[10], b'T');
        assert!(stamp.ends_with("+00:00"), "{stamp}");
    }

    // -------------------------------------------------------------------
    // append / read_all / read_last
    // -------------------------------------------------------------------

    #[test]
    fn three_appended_records_read_back_in_order() {
        let dir = tempfile::tempdir().unwrap();
        let origin = dir.path();
        let a = record("roster", "published roster 2026.09.06");
        let b = record("component", "published runtime 1.0.1");
        let c = record("release", "signed release 8");
        append(origin, &a).unwrap();
        append(origin, &b).unwrap();
        append(origin, &c).unwrap();

        let found = read_all(origin).unwrap();
        assert_eq!(found.skipped, 0);
        assert_eq!(found.records, vec![a, b, c]);
    }

    #[test]
    fn read_all_is_empty_when_no_journal_exists_yet() {
        let dir = tempfile::tempdir().unwrap();
        let found = read_all(dir.path()).unwrap();
        assert_eq!(found, ReadAll::default());
    }

    #[test]
    fn a_malformed_line_between_two_good_ones_is_skipped_not_fatal() {
        let dir = tempfile::tempdir().unwrap();
        let origin = dir.path();
        let a = record("roster", "first");
        let c = record("roster", "third");
        append(origin, &a).unwrap();
        // A hand-written malformed line, appended directly: not a JSON
        // object at all.
        let path = origin.join(JOURNAL_FILE);
        let mut file = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
        writeln!(file, "{{not json").unwrap();
        drop(file);
        append(origin, &c).unwrap();

        let found = read_all(origin).unwrap();
        assert_eq!(found.skipped, 1);
        assert_eq!(found.records, vec![a, c]);
    }

    #[test]
    fn read_last_returns_the_last_n_in_order() {
        let dir = tempfile::tempdir().unwrap();
        let origin = dir.path();
        let records: Vec<Record> = (0..4).map(|i| record("component", &format!("publish {i}"))).collect();
        for r in &records {
            append(origin, r).unwrap();
        }

        let last_two = read_last(origin, 2).unwrap();
        assert_eq!(last_two, records[2..].to_vec());
    }

    #[test]
    fn read_last_with_more_than_exist_returns_everything() {
        let dir = tempfile::tempdir().unwrap();
        let origin = dir.path();
        let a = record("roster", "only one");
        append(origin, &a).unwrap();
        assert_eq!(read_last(origin, 5).unwrap(), vec![a]);
    }

    // -------------------------------------------------------------------
    // Secret hygiene
    // -------------------------------------------------------------------

    /// A request struct carrying a real token, the way `RosterRequest`
    /// does. `inputs` is built the safe way: the token's presence, never
    /// its value, the same pattern as `RosterRequest`'s hand-written
    /// `Debug`. The serialized journal line must not contain the secret.
    #[test]
    fn a_record_never_carries_the_secret_a_request_held() {
        struct FakeRosterRequest<'a> {
            version: &'a str,
            token: Option<&'a str>,
        }
        let secret = "Bot MTIzNDU2Nzg5MC.aBcDeF.gHiJkL-super-secret-token";
        let req = FakeRosterRequest { version: "2026.09.09", token: Some(secret) };

        let inputs = serde_json::json!({
            "version": req.version,
            "token": if req.token.is_some() { "<set>" } else { "<unset>" },
        });
        let mut r = record("roster", "published roster 2026.09.09");
        r.inputs = inputs;

        let dir = tempfile::tempdir().unwrap();
        append(dir.path(), &r).unwrap();
        let line = std::fs::read_to_string(dir.path().join(JOURNAL_FILE)).unwrap();

        assert!(!line.contains(secret), "journal line must never contain the token: {line}");
        assert!(line.contains("<set>"), "{line}");
    }
}
