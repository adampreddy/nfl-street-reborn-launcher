//! The live feed's reader: Dolphin's play tracker appends each play to
//! `User\GameStats\Live\<session>.plays.jsonl` the moment it settles; this reads those lines in order and
//! remembers, in a small cursor file, how far the server has taken them. The cursor IS the durable queue:
//! a play that failed to send is simply the next one read. Pure file I/O -- no network, no clocks.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// The backend refuses a live play body over 16 KB; a longer line is skipped, never sent.
pub const MAX_LINE_BYTES: usize = 16 * 1024;
const SUFFIX: &str = ".plays.jsonl";
/// A session file (or a saved cursor) untouched this long is swept on `LivePoster::start`, so a Live
/// folder fed by many finished matches never grows the per-tick directory listing, or the next
/// `Cursor::begin`'s baseline map, without bound.
const PRUNE_AFTER: Duration = Duration::from_secs(14 * 24 * 60 * 60);
/// A UTF-8 BOM some editors and some versions of Windows' own tools still write at a file's first byte.
/// It is never valid JSON, so left in place it would make every parse of line 1 fail forever.
const UTF8_BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];

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

// A thread-local rather than a shared atomic: `cargo test` runs tests on separate threads, and each
// test's own calls -- including everything a poster does on its behalf, with no spawning in between --
// land on that test's own thread, so counts from other tests running at the same time never leak in.
#[cfg(test)]
thread_local! {
    static SESSION_FILES_CALLS: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

pub fn session_files(live_dir: &Path) -> Vec<String> {
    #[cfg(test)]
    SESSION_FILES_CALLS.with(|c| c.set(c.get() + 1));
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
    /// File -> byte length it already had when this match's cursor first saw it. A session file can
    /// predate the cursor two ways: it is a leftover from another game (nothing in it is ever this
    /// match's), or -- Task 5's resume case -- this match's own poster is only now starting to watch a
    /// file Dolphin had already begun writing. Both look identical at this instant: a name with some
    /// bytes already in it. There is no way to tell them apart from the file alone, so this records only
    /// "already accounted for up to here" rather than excluding the name outright: whatever was already
    /// there is never posted, but bytes appended afterward are read exactly like any other file's. A
    /// leftover file simply never grows again, so in practice it stays silent forever anyway.
    pub baseline: BTreeMap<String, u64>,
    /// File -> byte offset the server has taken it through.
    pub done: BTreeMap<String, u64>,
    /// File -> games this match had finished when the file was first seen (Task 5 fills it).
    pub game_base: BTreeMap<String, u32>,
}

impl Cursor {
    pub fn begin(match_id: &str, live_dir: &Path) -> Cursor {
        let baseline = session_files(live_dir)
            .into_iter()
            .map(|f| {
                let len = std::fs::metadata(live_dir.join(&f)).map(|m| m.len()).unwrap_or(0);
                (f, len)
            })
            .collect();
        Cursor { match_id: match_id.to_string(), baseline, ..Default::default() }
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

    /// Lists the live directory itself, one read_dir per call. [`Cursor::next_in`] takes that listing as
    /// an argument instead, so a caller walking many plays in a row -- [`LivePoster::tick`] -- can list
    /// once and reuse it, rather than paying for a fresh read_dir and sort per play.
    pub fn next(&self, live_dir: &Path) -> Next {
        self.next_in(live_dir, &session_files(live_dir))
    }

    pub fn next_in(&self, live_dir: &Path, files: &[String]) -> Next {
        for (i, file) in files.iter().enumerate() {
            let path = live_dir.join(file);
            let from = self.done.get(file).copied().unwrap_or_else(|| self.baseline.get(file).copied().unwrap_or(0));
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
            // A UTF-8 BOM can only ever sit at a file's first three bytes, so it is only ever in play when
            // this is the very first read of the file (`from == 0`); `end` is computed above, from `nl`'s
            // position in the untouched `rest`, so it keeps counting the BOM's bytes regardless.
            let mut text = &rest[..nl];
            if from == 0 {
                text = text.strip_prefix(&UTF8_BOM).unwrap_or(text);
            }
            if text.len() > MAX_LINE_BYTES {
                return Next::Skip { file: file.clone(), end, why: "play line over 16 KB" };
            }
            let Ok(raw) = serde_json::from_slice::<serde_json::Value>(text) else {
                return Next::Skip { file: file.clone(), end, why: "unreadable play line" };
            };
            let num = |k: &str| raw.get(k).and_then(|v| v.as_u64());
            // A field over u32::MAX is refused rather than silently truncated by an `as u32`, which would
            // otherwise wrap it into some unrelated, plausible-looking small number.
            let field_u32 = |v: Option<u64>, default: u32| -> Option<u32> {
                match v {
                    None => Some(default),
                    Some(n) => u32::try_from(n).ok(),
                }
            };
            let schema_ok = raw.get("schema").and_then(|v| v.as_str()) == Some("nsr.play.v1");
            let seq = num("seq").and_then(|n| u32::try_from(n).ok());
            let game_no = field_u32(num("game_no"), 1);
            let attempt = field_u32(num("attempt"), 1);
            let (true, Some(seq), Some(game_no), Some(attempt)) = (schema_ok, seq, game_no, attempt) else {
                return Next::Skip { file: file.clone(), end, why: "not a play line" };
            };
            return Next::Play(LivePlay {
                file: file.clone(),
                end,
                game_no,
                attempt,
                seq,
                human_sides: raw.get("human_sides").and_then(|v| v.as_i64()).unwrap_or(-1),
                raw,
            });
        }
        Next::Nothing
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MatchCtx {
    pub match_id: String,
    pub best_of: u8,
    /// Games of this match already recorded by the server when the current Dolphin process started.
    pub games_before: u32,
}

/// The live feed runs only while the server says the match is being played.
pub fn match_ctx(m: &crate::matchmaking::MatchView) -> Option<MatchCtx> {
    (m.status == "running").then(|| MatchCtx {
        match_id: m.id.clone(),
        best_of: m.best_of,
        games_before: m.series.iter().map(|w| *w as u32).sum(),
    })
}

#[derive(Debug, Clone, PartialEq)]
pub enum Decision {
    Post { game_seq: u32, body: Vec<u8> },
    Skip(&'static str),
}

/// Whether a play goes out, and as which game of the series.
pub fn decide(ctx: &MatchCtx, game_base: u32, play: &LivePlay) -> Decision {
    if play.human_sides == 0 {
        return Decision::Skip("no human in control");
    }
    // Both PCs restart together, but the first try's plays are already on the feed under the same seqs;
    // the final capture is the record for a restarted game.
    if play.attempt > 1 {
        return Decision::Skip("restarted game");
    }
    let Some(game_seq) = game_base.checked_add(play.game_no) else {
        return Decision::Skip("bad game number");
    };
    if game_seq < 1 || game_seq > ctx.best_of as u32 {
        return Decision::Skip("past the last game of the series");
    }
    let body = serde_json::json!({ "match_id": ctx.match_id, "game_seq": game_seq, "play": play.raw });
    let body = serde_json::to_vec(&body).unwrap_or_default();
    if body.len() > MAX_LINE_BYTES {
        return Decision::Skip("play over 16 KB");
    }
    Decision::Post { game_seq, body }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Sent {
    Done,
    Drop(String),
    Retry,
    Stop(String),
}

/// Any 2xx is final -- `dropped` included: the server lost it and a resend cannot recover it (backend
/// README). 429 / 5xx / no connection: keep it and send it again. Any 4xx other than 401 or 429: the
/// server refused that specific play; move on. 401: this device is signed out; stop. 1xx and 3xx (a
/// captive-portal redirect, say) are not a real reply from the backend either way, so they are treated
/// like a transport hiccup: retry rather than discard a play on a status the backend never actually sends.
pub fn classify(reply: &crate::platform::Result<crate::net::HttpResponse>) -> Sent {
    match reply {
        Err(_) => Sent::Retry,
        Ok(r) if (200..300).contains(&r.status) => Sent::Done,
        Ok(r) if r.status == 401 => Sent::Stop("signed out".into()),
        Ok(r) if r.status == 429 || r.status >= 500 => Sent::Retry,
        Ok(r) if (400..500).contains(&r.status) => Sent::Drop(format!("{} {}", r.status, r.body.chars().take(120).collect::<String>())),
        Ok(_) => Sent::Retry,
    }
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct TickReport {
    pub posted: u32,
    pub skipped: u32,
    pub dropped: u32,
    pub retrying: bool,
    pub stopped: Option<String>,
    /// The status plus at most 120 characters of the server's error from the most recent `Drop` this
    /// tick, for one log line. Built only from `classify`'s own formatting of the server's reply, which
    /// never sees the bearer token, so this can never carry it.
    pub last_drop: Option<String>,
}

/// Whether `cursor` still has bytes of `file` (current length `len`) left to read: the offset it has
/// taken that file through -- `done`, or failing that `baseline` -- is short of `len`. A file with no
/// entry in either map at all counts as fully unposted (offset 0), so an untracked file is never swept
/// out from under a cursor that has not even looked at it yet.
fn has_unposted_bytes(cursor: &Cursor, file: &str, len: u64) -> bool {
    let through = cursor.done.get(file).copied().unwrap_or_else(|| cursor.baseline.get(file).copied().unwrap_or(0));
    through < len
}

fn is_stale(modified: std::io::Result<SystemTime>, now: SystemTime) -> bool {
    let Ok(modified) = modified else { return false };
    now.duration_since(modified).is_ok_and(|age| age >= PRUNE_AFTER)
}

/// Sweeps session files this match's cursor has fully consumed and that have sat untouched for
/// [`PRUNE_AFTER`], so a Live folder fed by many finished matches never grows the per-tick directory
/// listing without bound. Best-effort: every error is swallowed, since a failed sweep is never worse than
/// no sweep, and must never be allowed to stop the poster from starting.
fn prune_live_dir(live: &Path, cursor: &Cursor, now: SystemTime) {
    for file in session_files(live) {
        let path = live.join(&file);
        let Ok(meta) = std::fs::metadata(&path) else { continue };
        if has_unposted_bytes(cursor, &file, meta.len()) {
            continue;
        }
        if is_stale(meta.modified(), now) {
            let _ = std::fs::remove_file(&path);
        }
    }
}

/// Sweeps other matches' saved cursors once they have sat untouched for [`PRUNE_AFTER`]. Never the
/// current match's own file, however old: it is this poster's only durable record of what it has sent.
fn prune_state_dir(state_dir: &Path, keep: &Path, now: SystemTime) {
    let Ok(entries) = std::fs::read_dir(state_dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path == keep || !entry.file_name().to_string_lossy().ends_with(".cursor.json") {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        if is_stale(meta.modified(), now) {
            let _ = std::fs::remove_file(&path);
        }
    }
}

#[derive(Debug)]
pub struct LivePoster {
    ctx: MatchCtx,
    live: PathBuf,
    cursor_path: PathBuf,
    cursor: Cursor,
    fails: u32,
    not_before_ms: u64,
    stopped: Option<String>,
}

/// One tick sends at most this many plays, so a long backlog never holds the loop.
const PER_TICK: u32 = 20;

impl LivePoster {
    pub fn start(ctx: MatchCtx, user_dir: &Path, state_dir: &Path) -> LivePoster {
        let live = live_dir(user_dir);
        let cursor_path = state_dir.join(format!("{}.cursor.json", ctx.match_id));
        let cursor = Cursor::load(&cursor_path)
            .filter(|c| c.match_id == ctx.match_id)
            .unwrap_or_else(|| Cursor::begin(&ctx.match_id, &live));
        let now = SystemTime::now();
        prune_live_dir(&live, &cursor, now);
        prune_state_dir(state_dir, &cursor_path, now);
        LivePoster { ctx, live, cursor_path, cursor, fails: 0, not_before_ms: 0, stopped: None }
    }

    pub fn set_games_before(&mut self, games_before: u32) {
        self.ctx.games_before = games_before;
    }

    pub fn caught_up(&self) -> bool {
        matches!(self.cursor.next(&self.live), Next::Nothing)
    }

    /// False while backing off after a failure, or after a 401: a tick now would send nothing, so the
    /// caller can skip the credential vault. (Such a tick still lists and latches game bases; call
    /// `tick` with an empty token for that -- it never sends while not ready.)
    pub fn ready(&self, now_ms: u64) -> bool {
        self.stopped.is_none() && now_ms >= self.not_before_ms
    }

    /// Gives every currently listed session file a durable `game_base`, the moment its name is first
    /// seen at all -- never the moment its first play is actually read, which can be much later than
    /// that if an earlier file's play is stuck retrying (a base must be caught before `games_before` can
    /// move again, not whenever the backlog finally clears). Dolphin's own per-process file counter `n`
    /// (the number after the last `-`) restarts at 1 only when the emulator itself restarts: a Rematch
    /// keeps writing new files, with `n` climbing, in the very same process. So a file starting a new
    /// process (`n == 1`, or nothing earlier in session order has a base yet) takes whatever
    /// `ctx.games_before` is right now; a later file from that same process inherits the base already
    /// latched for the file just before it in session order. That way a game the server records mid
    /// process never renumbers that same process's own later games.
    fn latch_game_bases(&mut self, files: &[String]) {
        for (i, file) in files.iter().enumerate() {
            if self.cursor.game_base.contains_key(file) {
                continue;
            }
            let starts_a_process = sort_key(file).1 == 1 || i == 0 || !self.cursor.game_base.contains_key(&files[i - 1]);
            let base = if starts_a_process { self.ctx.games_before } else { self.cursor.game_base[&files[i - 1]] };
            self.cursor.game_base.insert(file.clone(), base);
        }
    }

    pub fn tick(&mut self, http: &dyn crate::net::Http, origin: &str, token: &str, now_ms: u64) -> TickReport {
        let mut report = TickReport { stopped: self.stopped.clone(), ..Default::default() };
        // One read_dir + sort for the whole tick, not one per play: a Skip or a post only ever moves the
        // cursor's offset *within* an already-listed file, it never renames or removes one, so the same
        // listing stays correct for every iteration below. Listed and latched even on a tick that is
        // about to back off without sending anything, so a file's base is caught the moment it is first
        // seen rather than whenever its backlog finally clears.
        let files = session_files(&self.live);
        self.latch_game_bases(&files);
        if self.stopped.is_none() && now_ms >= self.not_before_ms {
            let url = format!("{}/v1/live/plays", origin.trim_end_matches('/'));
            for _ in 0..PER_TICK {
                let play = match self.cursor.next_in(&self.live, &files) {
                    Next::Nothing => break,
                    Next::Skip { file, end, .. } => {
                        report.skipped += 1;
                        self.cursor.advance(&file, end);
                        continue;
                    }
                    Next::Play(p) => p,
                };
                let base = self.cursor.game_base.get(&play.file).copied().unwrap_or(self.ctx.games_before);
                let body = match decide(&self.ctx, base, &play) {
                    Decision::Skip(_) => {
                        report.skipped += 1;
                        self.cursor.advance(&play.file, play.end);
                        continue;
                    }
                    Decision::Post { body, .. } => body,
                };
                match classify(&http.post_json(&url, &body, Some(token))) {
                    Sent::Done => {
                        report.posted += 1;
                        self.fails = 0;
                        self.cursor.advance(&play.file, play.end);
                    }
                    Sent::Drop(reason) => {
                        report.dropped += 1;
                        report.last_drop = Some(reason);
                        self.fails = 0;
                        self.cursor.advance(&play.file, play.end);
                    }
                    Sent::Retry => {
                        self.fails += 1;
                        // 1 s, 2 s, 4 s ... capped at 30 s.
                        self.not_before_ms = now_ms + (1000u64 << (self.fails - 1).min(5)).min(30_000);
                        report.retrying = true;
                        break;
                    }
                    Sent::Stop(why) => {
                        self.stopped = Some(why.clone());
                        report.stopped = Some(why);
                        break;
                    }
                }
            }
        } else {
            report.retrying = self.fails > 0 && self.stopped.is_none();
        }
        // Once per tick, not once per advance: a crash right after this can resend at most this one
        // tick's worth of plays, and the backend answers "duplicate" for whichever of those already landed.
        let _ = self.cursor.save(&self.cursor_path);
        report
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
    fn match_ctx_only_while_running_and_counts_recorded_games() {
        let mut m = crate::matchmaking::MatchView::default();
        m.id = "3f1c0f3e-0000-4000-8000-000000000001".into();
        m.best_of = 3;
        m.series = vec![1, 0];
        m.status = "running".into();
        assert_eq!(match_ctx(&m), Some(MatchCtx { match_id: m.id.clone(), best_of: 3, games_before: 1 }));
        for s in ["pairing", "lobby", "complete", "held", "void", "abandoned"] {
            m.status = s.into();
            assert_eq!(match_ctx(&m), None, "{s}");
        }
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
        let limit = if cfg!(debug_assertions) { 500 } else { 250 };
        assert!(t.elapsed() < std::time::Duration::from_millis(limit), "{:?}", t.elapsed());
    }

    #[test]
    fn load_of_a_missing_or_corrupt_cursor_is_none() {
        let state = tempfile::tempdir().unwrap();
        assert!(Cursor::load(&state.path().join("nope.json")).is_none());
        std::fs::write(state.path().join("bad.json"), "{").unwrap();
        assert!(Cursor::load(&state.path().join("bad.json")).is_none());
    }

    use crate::net::{Http, HttpResponse};
    use std::sync::Mutex;

    /// Replies in order; `None` is a transport failure. Records every POST body.
    struct Script { replies: Mutex<Vec<Option<(u16, &'static str)>>>, posts: Mutex<Vec<serde_json::Value>> }
    impl Script {
        fn new(r: Vec<Option<(u16, &'static str)>>) -> Self { Script { replies: Mutex::new(r), posts: Mutex::new(vec![]) } }
    }
    impl Http for Script {
        fn get_text(&self, url: &str) -> crate::platform::Result<HttpResponse> { panic!("no GET expected: {url}") }
        fn get_to_file(&self, url: &str, _: &Path) -> crate::platform::Result<u64> { panic!("no download expected: {url}") }
        fn post_json(&self, url: &str, body: &[u8], bearer: Option<&str>) -> crate::platform::Result<HttpResponse> {
            assert_eq!(url, "https://api.test/v1/live/plays");
            assert_eq!(bearer, Some("tok"));
            self.posts.lock().unwrap().push(serde_json::from_slice(body).unwrap());
            let mut r = self.replies.lock().unwrap();
            match if r.is_empty() { Some((200, "{\"ok\":true,\"status\":\"live\"}")) } else { r.remove(0) } {
                Some((status, body)) => Ok(HttpResponse { status, body: body.to_string() }),
                None => Err(crate::platform::Error::PlainIo(std::io::Error::new(std::io::ErrorKind::NotConnected, "down"))),
            }
        }
    }

    fn ctx(best_of: u8, games_before: u32) -> MatchCtx {
        MatchCtx { match_id: "3f1c0f3e-0000-4000-8000-000000000001".into(), best_of, games_before }
    }
    fn setup() -> (tempfile::TempDir, tempfile::TempDir) {
        let user = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(live_dir(user.path())).unwrap();
        (user, tempfile::tempdir().unwrap())
    }
    fn live_append(user: &Path, name: &str, s: &str) { append(&live_dir(user), name, s) }

    #[test]
    fn decide_maps_game_no_to_game_seq_and_wraps_the_raw_line() {
        let p = LivePlay { file: "f".into(), end: 1, game_no: 2, attempt: 1, seq: 7, human_sides: 3, raw: serde_json::json!({"schema":"nsr.play.v1","seq":7}) };
        match decide(&ctx(3, 0), 0, &p) {
            Decision::Post { game_seq, body } => {
                assert_eq!(game_seq, 2);
                let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
                assert_eq!(v, serde_json::json!({"match_id": ctx(3, 0).match_id, "game_seq": 2, "play": {"schema":"nsr.play.v1","seq":7}}));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn decide_skips_no_human_restarts_and_games_past_the_series() {
        let base = LivePlay { file: "f".into(), end: 1, game_no: 1, attempt: 1, seq: 1, human_sides: 0, raw: serde_json::json!({}) };
        assert_eq!(decide(&ctx(3, 0), 0, &base), Decision::Skip("no human in control"));
        let restarted = LivePlay { human_sides: 1, attempt: 2, ..base.clone() };
        assert_eq!(decide(&ctx(3, 0), 0, &restarted), Decision::Skip("restarted game"));
        let past = LivePlay { human_sides: 1, game_no: 2, ..base.clone() };
        assert_eq!(decide(&ctx(1, 0), 0, &past), Decision::Skip("past the last game of the series"));
        let unknown = LivePlay { human_sides: -1, ..base };
        assert!(matches!(decide(&ctx(1, 0), 0, &unknown), Decision::Post { game_seq: 1, .. }), "-1 (unknown) is posted");
    }

    #[test]
    fn classify_follows_the_backend_contract() {
        let ok = |s: u16, b: &str| -> crate::platform::Result<HttpResponse> { Ok(HttpResponse { status: s, body: b.into() }) };
        for status in ["live", "confirmed", "disputed", "duplicate", "ignored", "dropped"] {
            assert_eq!(classify(&ok(200, &format!("{{\"ok\":true,\"status\":\"{status}\"}}"))), Sent::Done, "{status} is final");
        }
        for s in [400, 403, 404, 409, 413] {
            assert!(matches!(classify(&ok(s, "{\"ok\":false,\"error\":\"x\"}")), Sent::Drop(_)), "{s}");
        }
        assert!(matches!(classify(&ok(401, "")), Sent::Stop(_)));
        for s in [429, 500, 502, 503] {
            assert_eq!(classify(&ok(s, "")), Sent::Retry, "{s}");
        }
        let down: crate::platform::Result<HttpResponse> = Err(crate::platform::Error::PlainIo(std::io::Error::new(std::io::ErrorKind::NotConnected, "x")));
        assert_eq!(classify(&down), Sent::Retry);
    }

    #[test]
    fn posts_new_plays_in_order_and_saves_the_cursor() {
        let (user, state) = setup();
        let mut poster = LivePoster::start(ctx(3, 0), user.path(), state.path());
        live_append(user.path(), "20260923-100000-1.plays.jsonl", &(line(1, 1, 1, 3) + &line(2, 1, 1, 3)));
        let http = Script::new(vec![]);
        let r = poster.tick(&http, "https://api.test", "tok", 0);
        assert_eq!(r.posted, 2);
        let posts = http.posts.lock().unwrap();
        assert_eq!(posts.iter().map(|p| p["play"]["seq"].as_u64().unwrap()).collect::<Vec<_>>(), vec![1, 2]);
        assert!(poster.caught_up());
        let saved = Cursor::load(&state.path().join(format!("{}.cursor.json", ctx(3, 0).match_id))).unwrap();
        assert_eq!(saved.done.get("20260923-100000-1.plays.jsonl").copied(), Some((line(1, 1, 1, 3).len() + line(2, 1, 1, 3).len()) as u64));
    }

    #[test]
    fn transport_failure_keeps_the_play_and_backs_off() {
        let (user, state) = setup();
        let mut poster = LivePoster::start(ctx(3, 0), user.path(), state.path());
        live_append(user.path(), "20260923-100000-1.plays.jsonl", &(line(1, 1, 1, 3) + &line(2, 1, 1, 3)));
        let http = Script::new(vec![None, Some((503, "")), None]);
        assert!(poster.tick(&http, "https://api.test", "tok", 0).retrying);
        assert_eq!(poster.tick(&http, "https://api.test", "tok", 500).posted, 0, "still backing off: nothing sent");
        assert_eq!(http.posts.lock().unwrap().len(), 1);
        assert!(poster.tick(&http, "https://api.test", "tok", 2_000).retrying);   // 503
        assert!(poster.tick(&http, "https://api.test", "tok", 10_000).retrying);  // down again
        let r = poster.tick(&http, "https://api.test", "tok", 60_000);            // recovers
        assert_eq!(r.posted, 2);
        let seqs: Vec<u64> = http.posts.lock().unwrap().iter().map(|p| p["play"]["seq"].as_u64().unwrap()).collect();
        assert_eq!(seqs, vec![1, 1, 1, 1, 2], "the same play is resent until it lands, then the next");
    }

    #[test]
    fn backoff_never_exceeds_thirty_seconds() {
        let (user, state) = setup();
        let mut poster = LivePoster::start(ctx(3, 0), user.path(), state.path());
        live_append(user.path(), "20260923-100000-1.plays.jsonl", &line(1, 1, 1, 3));
        let http = Script::new(vec![None; 20]);
        let mut now = 0u64;
        for _ in 0..10 {
            poster.tick(&http, "https://api.test", "tok", now);
            now += 30_000;
        }
        assert_eq!(http.posts.lock().unwrap().len(), 10, "one attempt every 30 s at worst");
    }

    #[test]
    fn a_refused_play_is_dropped_and_the_next_one_sent() {
        let (user, state) = setup();
        let mut poster = LivePoster::start(ctx(3, 0), user.path(), state.path());
        live_append(user.path(), "20260923-100000-1.plays.jsonl", &(line(1, 1, 1, 3) + &line(2, 1, 1, 3)));
        let http = Script::new(vec![Some((409, "{\"ok\":false,\"error\":\"over\"}"))]);
        let r = poster.tick(&http, "https://api.test", "tok", 0);
        assert_eq!((r.dropped, r.posted), (1, 1));
    }

    #[test]
    fn unauthorized_stops_the_poster() {
        let (user, state) = setup();
        let mut poster = LivePoster::start(ctx(3, 0), user.path(), state.path());
        live_append(user.path(), "20260923-100000-1.plays.jsonl", &(line(1, 1, 1, 3) + &line(2, 1, 1, 3)));
        let http = Script::new(vec![Some((401, ""))]);
        assert!(poster.tick(&http, "https://api.test", "tok", 0).stopped.is_some());
        assert!(poster.tick(&http, "https://api.test", "tok", 60_000).stopped.is_some());
        assert_eq!(http.posts.lock().unwrap().len(), 1, "nothing more after a 401");
    }

    #[test]
    fn restarted_attempt_is_not_posted() {
        let (user, state) = setup();
        let mut poster = LivePoster::start(ctx(3, 0), user.path(), state.path());
        live_append(user.path(), "20260923-100000-1.plays.jsonl", &line(1, 1, 1, 3));
        live_append(user.path(), "20260923-100500-2.plays.jsonl", &(line(1, 1, 2, 3) + &line(2, 1, 2, 3)));
        let http = Script::new(vec![]);
        let r = poster.tick(&http, "https://api.test", "tok", 0);
        assert_eq!((r.posted, r.skipped), (1, 2));
    }

    #[test]
    fn files_keep_the_game_base_they_were_first_seen_with() {
        let (user, state) = setup();
        let mut poster = LivePoster::start(ctx(3, 0), user.path(), state.path());
        live_append(user.path(), "20260923-100000-1.plays.jsonl", &line(1, 1, 1, 3));
        let http = Script::new(vec![None]);
        poster.tick(&http, "https://api.test", "tok", 0); // file seen with base 0, the post fails
        // Dolphin relaunched after game 1 was recorded; its new process numbers games from 1 again.
        poster.set_games_before(1);
        live_append(user.path(), "20260923-101000-1.plays.jsonl", &line(1, 1, 1, 3));
        poster.tick(&http, "https://api.test", "tok", 60_000);
        let seqs: Vec<u64> = http.posts.lock().unwrap().iter().map(|p| p["game_seq"].as_u64().unwrap()).collect();
        assert_eq!(seqs, vec![1, 1, 2], "the old file stays game 1; the relaunched process's game 1 is game 2");
    }

    #[test]
    fn start_resumes_a_saved_cursor_for_the_same_match_only() {
        let (user, state) = setup();
        live_append(user.path(), "20260923-100000-1.plays.jsonl", &line(1, 1, 1, 3));
        {
            let mut p = LivePoster::start(ctx(3, 0), user.path(), state.path());
            p.tick(&Script::new(vec![]), "https://api.test", "tok", 0);
        }
        // The launcher restarts: the same match resumes where it was.
        let mut again = LivePoster::start(ctx(3, 0), user.path(), state.path());
        live_append(user.path(), "20260923-100000-1.plays.jsonl", &line(2, 1, 1, 3));
        let http = Script::new(vec![]);
        again.tick(&http, "https://api.test", "tok", 0);
        assert_eq!(http.posts.lock().unwrap().iter().map(|p| p["play"]["seq"].as_u64().unwrap()).collect::<Vec<_>>(), vec![2]);
    }

    // -- Fix round 1 (Task 4 review) --

    #[test]
    fn a_leading_utf8_bom_does_not_block_line_one() {
        let (user, state) = setup();
        let mut poster = LivePoster::start(ctx(3, 0), user.path(), state.path());
        let bom = "\u{FEFF}"; // encodes to EF BB BF in UTF-8, exactly like a file written with it.
        live_append(user.path(), "20260923-100000-1.plays.jsonl", &(bom.to_string() + &line(1, 1, 1, 3) + &line(2, 1, 1, 3)));
        let http = Script::new(vec![]);
        let r = poster.tick(&http, "https://api.test", "tok", 0);
        assert_eq!(r.posted, 2);
        let seqs: Vec<u64> = http.posts.lock().unwrap().iter().map(|p| p["play"]["seq"].as_u64().unwrap()).collect();
        assert_eq!(seqs, vec![1, 2]);
    }

    fn set_mtime(path: &Path, when: SystemTime) {
        std::fs::OpenOptions::new().write(true).open(path).unwrap().set_modified(when).unwrap();
    }
    const DAY: Duration = Duration::from_secs(24 * 60 * 60);

    #[test]
    fn prune_deletes_a_fully_read_file_once_it_is_old() {
        let (user, state) = setup();
        let live = live_dir(user.path());
        let f = "20260101-000000-1.plays.jsonl";
        live_append(user.path(), f, &line(1, 1, 1, 3));
        let match_id = ctx(3, 0).match_id;
        let c = Cursor { match_id: match_id.clone(), done: BTreeMap::from([(f.to_string(), line(1, 1, 1, 3).len() as u64)]), ..Default::default() };
        c.save(&state.path().join(format!("{match_id}.cursor.json"))).unwrap();
        set_mtime(&live.join(f), SystemTime::now() - 20 * DAY);
        let _poster = LivePoster::start(ctx(3, 0), user.path(), state.path());
        assert!(!live.join(f).exists(), "fully read and 20 days old: swept");
    }

    #[test]
    fn prune_never_deletes_a_file_with_bytes_still_unposted() {
        let (user, state) = setup();
        let live = live_dir(user.path());
        let f = "20260101-000000-1.plays.jsonl";
        live_append(user.path(), f, &(line(1, 1, 1, 3) + &line(2, 1, 1, 3)));
        let match_id = ctx(3, 0).match_id;
        // Only play 1 is accounted for; play 2's bytes are still sitting past `done`.
        let c = Cursor { match_id: match_id.clone(), done: BTreeMap::from([(f.to_string(), line(1, 1, 1, 3).len() as u64)]), ..Default::default() };
        c.save(&state.path().join(format!("{match_id}.cursor.json"))).unwrap();
        set_mtime(&live.join(f), SystemTime::now() - 20 * DAY);
        let _poster = LivePoster::start(ctx(3, 0), user.path(), state.path());
        assert!(live.join(f).exists(), "play 2 is still unposted; the sweep must leave the file alone however old it is");
    }

    #[test]
    fn prune_leaves_a_recent_file_alone_even_if_fully_read() {
        let (user, state) = setup();
        let live = live_dir(user.path());
        let f = "20260101-000000-1.plays.jsonl";
        live_append(user.path(), f, &line(1, 1, 1, 3));
        let match_id = ctx(3, 0).match_id;
        let c = Cursor { match_id: match_id.clone(), done: BTreeMap::from([(f.to_string(), line(1, 1, 1, 3).len() as u64)]), ..Default::default() };
        c.save(&state.path().join(format!("{match_id}.cursor.json"))).unwrap();
        set_mtime(&live.join(f), SystemTime::now() - DAY);
        let _poster = LivePoster::start(ctx(3, 0), user.path(), state.path());
        assert!(live.join(f).exists(), "one day old is under the 14-day sweep age");
    }

    #[test]
    fn prune_deletes_old_cursor_files_for_other_matches_but_never_this_ones() {
        let (user, state) = setup();
        let mine_id = ctx(3, 0).match_id;
        let mine = state.path().join(format!("{mine_id}.cursor.json"));
        Cursor::begin(&mine_id, &live_dir(user.path())).save(&mine).unwrap();
        set_mtime(&mine, SystemTime::now() - 20 * DAY);
        let other = state.path().join("11111111-0000-4000-8000-000000000002.cursor.json");
        std::fs::write(&other, "{}").unwrap();
        set_mtime(&other, SystemTime::now() - 20 * DAY);
        let _poster = LivePoster::start(ctx(3, 0), user.path(), state.path());
        assert!(mine.exists(), "this match's own cursor is never swept, however old");
        assert!(!other.exists(), "a 20-day-old cursor for a different match is swept");
    }

    #[test]
    fn tick_lists_the_live_dir_once_per_call_not_once_per_play() {
        let (user, state) = setup();
        let mut poster = LivePoster::start(ctx(3, 0), user.path(), state.path());
        live_append(user.path(), "20260923-100000-1.plays.jsonl", &(line(1, 1, 1, 3) + &line(2, 1, 1, 3) + &line(3, 1, 1, 3)));
        let http = Script::new(vec![]);
        SESSION_FILES_CALLS.with(|c| c.set(0));
        let r = poster.tick(&http, "https://api.test", "tok", 0);
        assert_eq!(r.posted, 3);
        assert_eq!(SESSION_FILES_CALLS.with(|c| c.get()), 1, "one directory listing for the whole tick, not one per play");
    }

    #[test]
    fn ready_is_false_while_backing_off_and_after_a_401() {
        let (user, state) = setup();
        let mut poster = LivePoster::start(ctx(3, 0), user.path(), state.path());
        assert!(poster.ready(0));
        live_append(user.path(), "20260923-100000-1.plays.jsonl", &line(1, 1, 1, 3));
        let http = Script::new(vec![None, Some((401, ""))]);
        poster.tick(&http, "https://api.test", "tok", 0); // transport failure: back off 1 s
        assert!(!poster.ready(500));
        // A not-ready tick with no token sends nothing.
        poster.tick(&http, "", "", 500);
        assert_eq!(http.posts.lock().unwrap().len(), 1);
        assert!(poster.ready(1_000));
        poster.tick(&http, "https://api.test", "tok", 1_000); // 401
        assert!(!poster.ready(60_000));
    }

    // -- Fix round 2 (Task 5 review) --

    #[test]
    fn a_rematch_in_the_same_process_keeps_the_process_base() {
        // The reviewer's case: game 1 posts, the server records it, then a Rematch -- no Dolphin restart,
        // so the file counter climbs instead of resetting -- writes game 2 in the very same process.
        let (user, state) = setup();
        let mut poster = LivePoster::start(ctx(3, 0), user.path(), state.path());
        let f1 = "20260923-100000-1.plays.jsonl";
        live_append(user.path(), f1, &line(1, 1, 1, 3));
        let http = Script::new(vec![]);
        poster.tick(&http, "https://api.test", "tok", 0);
        poster.set_games_before(1); // the server now has game 1 recorded
        let f2 = "20260923-100000-2.plays.jsonl"; // same stamp, counter climbs: still this process
        live_append(user.path(), f2, &line(1, 2, 1, 3));
        poster.tick(&http, "https://api.test", "tok", 1_000);
        let seqs: Vec<u64> = http.posts.lock().unwrap().iter().map(|p| p["game_seq"].as_u64().unwrap()).collect();
        assert_eq!(seqs, vec![1, 2], "the process's own base (0) carries to its next game, not the bumped games_before");
    }

    #[test]
    fn a_relaunched_process_takes_the_current_games_before() {
        let (user, state) = setup();
        let mut poster = LivePoster::start(ctx(3, 0), user.path(), state.path());
        let f1 = "20260923-100000-1.plays.jsonl";
        live_append(user.path(), f1, &line(1, 1, 1, 3));
        let http = Script::new(vec![]);
        poster.tick(&http, "https://api.test", "tok", 0);
        poster.set_games_before(1);
        // A later stamp, counter back at 1: a genuinely new Dolphin process, not the same one continuing.
        let f2 = "20260923-110000-1.plays.jsonl";
        live_append(user.path(), f2, &line(1, 1, 1, 3));
        poster.tick(&http, "https://api.test", "tok", 1_000);
        let seqs: Vec<u64> = http.posts.lock().unwrap().iter().map(|p| p["game_seq"].as_u64().unwrap()).collect();
        assert_eq!(seqs, vec![1, 2], "a new process's own first file takes whatever games_before is now");
    }

    #[test]
    fn a_base_latched_while_retrying_survives_a_later_games_before_bump() {
        // The reviewer's second case: while game 1's play is stuck retrying, game 2's file appears. Its
        // base must be caught then -- at list time -- not whenever game 1's backlog finally clears.
        let (user, state) = setup();
        let mut poster = LivePoster::start(ctx(3, 0), user.path(), state.path());
        let f1 = "20260923-100000-1.plays.jsonl";
        live_append(user.path(), f1, &line(1, 1, 1, 3));
        let http = Script::new(vec![None]);
        assert!(poster.tick(&http, "https://api.test", "tok", 0).retrying, "play 1 fails to post: stuck retrying");
        let f2 = "20260923-100000-2.plays.jsonl";
        live_append(user.path(), f2, &line(1, 2, 1, 3));
        // Still inside the backoff window: this tick lists (and latches) but sends nothing.
        let r = poster.tick(&http, "https://api.test", "tok", 500);
        assert_eq!(r.posted, 0);
        assert_eq!(http.posts.lock().unwrap().len(), 1, "no new attempt while still backing off");
        // Some other signal now reports 2 games recorded, before play 1's post has ever landed.
        poster.set_games_before(2);
        let http2 = Script::new(vec![]);
        poster.tick(&http2, "https://api.test", "tok", 60_000);
        let seqs: Vec<u64> = http2.posts.lock().unwrap().iter().map(|p| p["game_seq"].as_u64().unwrap()).collect();
        assert_eq!(seqs, vec![1, 2], "game 2's base was latched from game 1's own base before the bump");
    }

    #[test]
    fn a_new_processs_base_is_caught_at_list_time_even_while_backing_off() {
        // The exact ordering the ruling calls for: listing (and latching) happens even on a tick that is
        // about to back off without sending anything, so a fresh process's OWN first file -- one that
        // reads `games_before` directly, unlike a same-process file that only ever inherits its neighbor
        // -- grabs whatever `games_before` is right now, not whatever it has drifted to by the time its
        // play finally sends.
        let (user, state) = setup();
        let mut poster = LivePoster::start(ctx(3, 0), user.path(), state.path());
        let f1 = "20260923-100000-1.plays.jsonl";
        live_append(user.path(), f1, &line(1, 1, 1, 3));
        let http = Script::new(vec![None]);
        assert!(poster.tick(&http, "https://api.test", "tok", 0).retrying, "play 1 fails to post: stuck retrying");
        // A different, later-launched process's first file appears while f1 is still stuck retrying.
        let f2 = "20260923-110000-1.plays.jsonl";
        live_append(user.path(), f2, &line(1, 1, 1, 3));
        let r = poster.tick(&http, "https://api.test", "tok", 500); // still inside the backoff window
        assert_eq!(r.posted, 0);
        // Some unrelated signal now claims 5 games recorded, well after f2's file was already listed.
        poster.set_games_before(5);
        let http2 = Script::new(vec![]);
        poster.tick(&http2, "https://api.test", "tok", 60_000);
        let seqs: Vec<u64> = http2.posts.lock().unwrap().iter().map(|p| p["game_seq"].as_u64().unwrap()).collect();
        assert_eq!(seqs, vec![1, 1], "f2's base (0) was caught when it was first listed, before the later bump to 5");
    }

    #[test]
    fn classify_retries_1xx_and_3xx() {
        let ok = |s: u16| -> crate::platform::Result<HttpResponse> { Ok(HttpResponse { status: s, body: String::new() }) };
        for s in [100, 101, 300, 301, 302, 307] {
            assert_eq!(classify(&ok(s)), Sent::Retry, "{s}");
        }
    }

    #[test]
    fn last_drop_never_contains_the_token() {
        let (user, state) = setup();
        let mut poster = LivePoster::start(ctx(3, 0), user.path(), state.path());
        live_append(user.path(), "20260923-100000-1.plays.jsonl", &line(1, 1, 1, 3));
        let http = Script::new(vec![Some((409, "{\"ok\":false,\"error\":\"over\"}"))]);
        // The fake's own bearer assertion pins the token to "tok"; `classify` -- and so `last_drop`, which
        // is built purely from its own formatting of the server's reply -- never even receives it, since
        // `classify`'s signature takes no bearer at all. This proves that structurally, not by collision.
        let r = poster.tick(&http, "https://api.test", "tok", 0);
        let drop = r.last_drop.expect("a 409 sets last_drop");
        assert!(drop.starts_with("409"), "{drop}");
        assert!(!drop.contains("tok"), "{drop}");
    }

    #[test]
    fn decide_skips_on_game_number_overflow() {
        let p = LivePlay { file: "f".into(), end: 1, game_no: u32::MAX, attempt: 1, seq: 1, human_sides: 1, raw: serde_json::json!({}) };
        assert_eq!(decide(&ctx(3, 0), 1, &p), Decision::Skip("bad game number"));
    }

    #[test]
    fn a_field_over_u32_max_is_skipped_as_not_a_play_line() {
        let d = tempfile::tempdir().unwrap();
        let mut c = Cursor::begin("m1", d.path());
        let f = "20260923-100000-1.plays.jsonl";
        let huge_seq = format!("{{\"schema\":\"nsr.play.v1\",\"seq\":{},\"human_sides\":1}}\n", u64::from(u32::MAX) + 1);
        append(d.path(), f, &(huge_seq + &line(9, 1, 1, 3)));
        match c.next(d.path()) {
            Next::Skip { file, end, why } => {
                assert_eq!(why, "not a play line");
                c.advance(&file, end);
            }
            other => panic!("expected a skip, got {other:?}"),
        }
        assert_eq!(play(c.next(d.path())).seq, 9);
    }
}
