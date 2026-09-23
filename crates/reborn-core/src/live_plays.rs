//! The live feed's reader: Dolphin's play tracker appends each play to
//! `User\GameStats\Live\<session>.plays.jsonl` the moment it settles; this reads those lines in order and
//! remembers, in a small cursor file, how far the server has taken them. The cursor IS the durable queue:
//! a play that failed to send is simply the next one read. Pure file I/O -- no network, no clocks.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// The backend refuses a live play body over 16 KB; a longer line is skipped, never sent.
pub const MAX_LINE_BYTES: usize = 16 * 1024;
const SUFFIX: &str = ".plays.jsonl";

pub fn live_dir(user_dir: &Path) -> PathBuf {
    user_dir.join(crate::stats::DOLPHIN_CAPTURE_DIR).join(crate::stats::LIVE_PLAYS_DIR)
}

/// Up to `max` bytes of `path` starting at `from`.
fn read_from(path: &Path, from: u64, max: u64) -> Option<Vec<u8>> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(path).ok()?;
    f.seek(SeekFrom::Start(from)).ok()?;
    let mut buf = Vec::new();
    f.take(max).read_to_end(&mut buf).ok()?;
    Some(buf)
}

/// Offset just past the first newline at or after `from`, reading 64 KB at a time (only for an over-long line).
fn newline_after(path: &Path, from: u64) -> Option<u64> {
    let mut at = from;
    loop {
        let chunk = read_from(path, at, 64 * 1024)?;
        if chunk.is_empty() {
            return None;
        }
        if let Some(nl) = chunk.iter().position(|b| *b == b'\n') {
            return Some(at + nl as u64 + 1);
        }
        at += chunk.len() as u64;
    }
}

/// `<YYYYMMDD-HHMMSS>-<n>`: the stamp sorts as text, the per-process counter `n` must sort as a number
/// (`-10` after `-2`).
fn sort_key(name: &str) -> (String, u64) {
    let stem = name.trim_end_matches(SUFFIX);
    match stem.rsplit_once('-') {
        Some((stamp, n)) => (stamp.to_string(), n.parse().unwrap_or(u64::MAX)),
        None => (stem.to_string(), u64::MAX),
    }
}

pub fn session_files(live_dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(live_dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n.ends_with(SUFFIX))
        .collect();
    names.sort_by_key(|n| sort_key(n));
    names
}

#[derive(Debug, Clone, PartialEq)]
pub struct LivePlay {
    pub file: String,
    /// Byte offset just past this line's newline.
    pub end: u64,
    pub game_no: u32,
    pub attempt: u32,
    pub seq: u32,
    pub human_sides: i64,
    /// The line exactly as Dolphin wrote it.
    pub raw: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Next {
    Play(LivePlay),
    Skip { file: String, end: u64, why: &'static str },
    Nothing,
}

#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Cursor {
    pub match_id: String,
    /// Session files that already existed when this match began: another game's, never posted.
    pub baseline: BTreeSet<String>,
    /// File -> byte offset the server has taken it through.
    pub done: BTreeMap<String, u64>,
    /// File -> games this match had finished when the file was first seen (Task 5 fills it).
    pub game_base: BTreeMap<String, u32>,
}

impl Cursor {
    pub fn begin(match_id: &str, live_dir: &Path) -> Cursor {
        Cursor { match_id: match_id.to_string(), baseline: session_files(live_dir).into_iter().collect(), ..Default::default() }
    }

    pub fn load(path: &Path) -> Option<Cursor> {
        serde_json::from_slice(&std::fs::read(path).ok()?).ok()
    }

    /// Write-then-rename, so a crash mid-save leaves the previous cursor, never half of one.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, serde_json::to_vec(self).map_err(std::io::Error::other)?)?;
        std::fs::rename(&tmp, path)
    }

    pub fn advance(&mut self, file: &str, end: u64) {
        self.done.insert(file.to_string(), end);
    }

    pub fn next(&self, live_dir: &Path) -> Next {
        let files: Vec<String> = session_files(live_dir).into_iter().filter(|f| !self.baseline.contains(f)).collect();
        for (i, file) in files.iter().enumerate() {
            let path = live_dir.join(file);
            let from = self.done.get(file).copied().unwrap_or(0);
            // Cheap first: a caught-up file costs one size check and no read. This runs every second during netplay.
            let Ok(len) = std::fs::metadata(&path).map(|m| m.len()) else { continue };
            if from >= len {
                continue;
            }
            // Read only what is new, at most one maximum line plus its newline.
            let Some(rest) = read_from(&path, from, MAX_LINE_BYTES as u64 + 2) else { continue };
            let Some(nl) = rest.iter().position(|b| *b == b'\n') else {
                if rest.len() > MAX_LINE_BYTES {
                    // Over-long line: skip to just past its newline once it has one.
                    if let Some(end) = newline_after(&path, from) {
                        return Next::Skip { file: file.clone(), end, why: "play line over 16 KB" };
                    }
                }
                // Dolphin may still be writing this line. Once a newer session exists, it never will.
                if i + 1 < files.len() {
                    return Next::Skip { file: file.clone(), end: len, why: "unfinished line from an ended session" };
                }
                return Next::Nothing;
            };
            let end = from + nl as u64 + 1;
            let text = &rest[..nl];
            if text.len() > MAX_LINE_BYTES {
                return Next::Skip { file: file.clone(), end, why: "play line over 16 KB" };
            }
            let Ok(raw) = serde_json::from_slice::<serde_json::Value>(text) else {
                return Next::Skip { file: file.clone(), end, why: "unreadable play line" };
            };
            let num = |k: &str| raw.get(k).and_then(|v| v.as_u64());
            let (Some(seq), true) = (num("seq"), raw.get("schema").and_then(|v| v.as_str()) == Some("nsr.play.v1")) else {
                return Next::Skip { file: file.clone(), end, why: "not a play line" };
            };
            return Next::Play(LivePlay {
                file: file.clone(),
                end,
                game_no: num("game_no").unwrap_or(1) as u32,
                attempt: num("attempt").unwrap_or(1) as u32,
                seq: seq as u32,
                human_sides: raw.get("human_sides").and_then(|v| v.as_i64()).unwrap_or(-1),
                raw,
            });
        }
        Next::Nothing
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn line(seq: u32, game_no: u32, attempt: u32, human_sides: i64) -> String {
        format!("{{\"schema\":\"nsr.play.v1\",\"session\":\"s\",\"game_no\":{game_no},\"attempt\":{attempt},\"seq\":{seq},\"human_sides\":{human_sides},\"summary\":{{\"text\":\"play {seq}\"}}}}\n")
    }
    fn append(dir: &Path, name: &str, s: &str) {
        let mut f = std::fs::OpenOptions::new().create(true).append(true).open(dir.join(name)).unwrap();
        f.write_all(s.as_bytes()).unwrap();
    }
    fn play(n: Next) -> LivePlay {
        match n { Next::Play(p) => p, other => panic!("expected a play, got {other:?}") }
    }

    #[test]
    fn session_files_sort_by_stamp_then_numeric_counter() {
        let d = tempfile::tempdir().unwrap();
        for n in ["20260923-101500-10.plays.jsonl", "20260923-101500-2.plays.jsonl", "20260922-230000-7.plays.jsonl", "notes.txt"] {
            std::fs::write(d.path().join(n), "").unwrap();
        }
        assert_eq!(session_files(d.path()), vec!["20260922-230000-7.plays.jsonl", "20260923-101500-2.plays.jsonl", "20260923-101500-10.plays.jsonl"]);
    }

    #[test]
    fn files_present_at_begin_are_never_posted() {
        let d = tempfile::tempdir().unwrap();
        append(d.path(), "20260923-090000-1.plays.jsonl", &line(1, 1, 1, 3));
        let c = Cursor::begin("m1", d.path());
        assert_eq!(c.next(d.path()), Next::Nothing);
        append(d.path(), "20260923-100000-2.plays.jsonl", &line(1, 1, 1, 3));
        assert_eq!(play(c.next(d.path())).file, "20260923-100000-2.plays.jsonl");
    }

    #[test]
    fn lines_come_in_order_and_advance_moves_past_them() {
        let d = tempfile::tempdir().unwrap();
        let mut c = Cursor::begin("m1", d.path());
        let f = "20260923-100000-1.plays.jsonl";
        append(d.path(), f, &(line(1, 1, 1, 3) + &line(2, 1, 1, 3)));
        let a = play(c.next(d.path()));
        assert_eq!((a.seq, a.game_no, a.attempt, a.human_sides), (1, 1, 1, 3));
        assert_eq!(c.next(d.path()), Next::Play(a.clone()), "next() never moves by itself");
        c.advance(f, a.end);
        let b = play(c.next(d.path()));
        assert_eq!(b.seq, 2);
        c.advance(f, b.end);
        assert_eq!(c.next(d.path()), Next::Nothing);
    }

    #[test]
    fn partial_last_line_waits() {
        let d = tempfile::tempdir().unwrap();
        let c = Cursor::begin("m1", d.path());
        let full = line(1, 1, 1, 3);
        append(d.path(), "20260923-100000-1.plays.jsonl", &full[..full.len() - 10]);
        assert_eq!(c.next(d.path()), Next::Nothing);
        append(d.path(), "20260923-100000-1.plays.jsonl", &full[full.len() - 10..]);
        assert_eq!(play(c.next(d.path())).seq, 1);
    }

    #[test]
    fn abandoned_partial_tail_is_skipped_once_a_newer_file_exists() {
        let d = tempfile::tempdir().unwrap();
        let mut c = Cursor::begin("m1", d.path());
        append(d.path(), "20260923-100000-1.plays.jsonl", "{\"schema\":\"nsr.pl");
        append(d.path(), "20260923-101000-2.plays.jsonl", &line(1, 2, 1, 3));
        match c.next(d.path()) {
            Next::Skip { file, end, .. } => { assert_eq!(file, "20260923-100000-1.plays.jsonl"); c.advance(&file, end); }
            other => panic!("expected a skip, got {other:?}"),
        }
        assert_eq!(play(c.next(d.path())).game_no, 2);
    }

    #[test]
    fn a_bad_or_oversized_line_is_skipped_not_stuck() {
        let d = tempfile::tempdir().unwrap();
        let mut c = Cursor::begin("m1", d.path());
        let f = "20260923-100000-1.plays.jsonl";
        let huge = format!("{{\"schema\":\"nsr.play.v1\",\"pad\":\"{}\"}}\n", "x".repeat(MAX_LINE_BYTES));
        append(d.path(), f, &("not json\n".to_string() + &huge + &line(3, 1, 1, 3)));
        for _ in 0..2 {
            match c.next(d.path()) { Next::Skip { file, end, .. } => c.advance(&file, end), other => panic!("expected a skip, got {other:?}") }
        }
        assert_eq!(play(c.next(d.path())).seq, 3);
    }

    #[test]
    fn a_line_without_game_no_counts_as_game_one_attempt_one() {
        // Lines from a tracker built before Task 1.
        let d = tempfile::tempdir().unwrap();
        let c = Cursor::begin("m1", d.path());
        append(d.path(), "20260923-100000-1.plays.jsonl", "{\"schema\":\"nsr.play.v1\",\"seq\":4,\"human_sides\":1}\n");
        let p = play(c.next(d.path()));
        assert_eq!((p.game_no, p.attempt, p.seq), (1, 1, 4));
    }

    #[test]
    fn resumes_after_restart_from_the_saved_cursor() {
        let d = tempfile::tempdir().unwrap();
        let state = tempfile::tempdir().unwrap();
        let path = state.path().join("m1.cursor.json");
        let mut c = Cursor::begin("m1", d.path());
        let f = "20260923-100000-1.plays.jsonl";
        append(d.path(), f, &(line(1, 1, 1, 3) + &line(2, 1, 1, 3)));
        let a = play(c.next(d.path()));
        c.advance(f, a.end);
        c.game_base.insert(f.to_string(), 0);
        c.save(&path).unwrap();
        let back = Cursor::load(&path).unwrap();
        assert_eq!(back, c);
        assert_eq!(play(back.next(d.path())).seq, 2);
    }

    #[test]
    fn caught_up_checks_are_cheap() {
        let d = tempfile::tempdir().unwrap();
        let mut c = Cursor::begin("m1", d.path());
        let f = "20260923-100000-1.plays.jsonl";
        let big: String = (1..=500).map(|i| line(i, 1, 1, 3)).collect();
        append(d.path(), f, &big);
        c.advance(f, big.len() as u64);
        let t = std::time::Instant::now();
        for _ in 0..1000 {
            assert_eq!(c.next(d.path()), Next::Nothing);
        }
        let limit = if cfg!(debug_assertions) { 500 } else { 100 };
        assert!(t.elapsed() < std::time::Duration::from_millis(limit), "{:?}", t.elapsed());
    }

    #[test]
    fn load_of_a_missing_or_corrupt_cursor_is_none() {
        let state = tempfile::tempdir().unwrap();
        assert!(Cursor::load(&state.path().join("nope.json")).is_none());
        std::fs::write(state.path().join("bad.json"), "{").unwrap();
        assert!(Cursor::load(&state.path().join("bad.json")).is_none());
    }
}
