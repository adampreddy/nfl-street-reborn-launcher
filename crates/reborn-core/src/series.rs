//! The series, launcher side (series spec 2026-10-02 §3): the local live tally that triggers the ranked close, the
//! close's timing, and why the launcher left. Everything here is pure except `newest_capture_ms` (one folder read).

use std::collections::BTreeMap;

use serde_json::Value;

/// The live score that decides a game (backend src/mm/series.js GAME_TARGET).
pub const GAME_TARGET: i64 = 36;

fn bit(v: &Value) -> Option<usize> {
    v.as_i64().filter(|x| *x == 0 || *x == 1).map(|x| x as usize)
}

/// JavaScript truthiness for a summary flag (the backend reads `s.turnover ? ... : ...`).
fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|x| x != 0.0),
        Value::String(s) => !s.is_empty(),
        _ => true,
    }
}

/// A safety: 2 points to `who` when the game's tsst stat rises, else a decoder-labelled safety's points to the
/// defense (backend pbp/safety.js).
fn safety_of(p: &Value) -> Option<(usize, f64)> {
    for c in p["stat_changes"].as_array().into_iter().flatten() {
        let (before, after) = (c["before"].as_f64(), c["after"].as_f64());
        if truthy(&c["team"]) && c["field"] == "tsst" && bit(&c["who"]).is_some() && before.is_some() && after.is_some() && after > before {
            return Some((bit(&c["who"]).unwrap(), 2.0));
        }
    }
    let pts = p["summary"]["points"].as_f64().filter(|x| x.is_finite() && *x > 0.0)?;
    if p["summary"]["safety"] == true { return bit(&p["offense_side"]).map(|off| (1 - off, pts)); }
    None
}

/// The score by side after a game's plays (backend pbp/livegame.js scoreAfter), in seq order.
pub fn score_of(plays: &[Value]) -> [i64; 2] {
    let mut sorted: Vec<&Value> = plays.iter().filter(|p| p["seq"].as_i64().is_some()).collect();
    sorted.sort_by_key(|p| p["seq"].as_i64().unwrap_or(0));
    let mut s = [0f64; 2];
    for p in sorted {
        if let Some((side, pts)) = safety_of(p) { s[side] += pts; continue; }
        let pts = p["summary"]["points"].as_f64().unwrap_or(0.0);
        if let (Some(side), true) = (bit(&p["offense_side"]), pts.is_finite() && pts > 0.0) {
            s[if truthy(&p["summary"]["turnover"]) { 1 - side } else { side }] += pts;
        }
    }
    [s[0].round() as i64, s[1].round() as i64]
}

/// The seat that won a game by this launcher's own copy (R12): a side reached 36 and is ahead; its seat is the pad
/// in the newest `ports` (pad 0 = seat 0, pad 1 = seat 1). None when undecided or the seat is unknown (R6).
pub fn game_winner_seat(plays: &[Value]) -> Option<u8> {
    let s = score_of(plays);
    if s[0].max(s[1]) < GAME_TARGET || s[0] == s[1] { return None; }
    let side = if s[0] > s[1] { 0 } else { 1 };
    let newest = plays.iter()
        .filter(|p| p["ports"].as_array().is_some_and(|a| a.len() == 2))
        .max_by_key(|p| p["seq"].as_i64().unwrap_or(0))?;
    bit(&newest["ports"][side]).map(|seat| seat as u8)
}

/// Every play this launcher's Dolphin wrote for the match, by game: the input of the ranked close.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct LocalTally {
    games: BTreeMap<u32, BTreeMap<i64, Value>>,
}

impl LocalTally {
    /// One play line (Dolphin's own copy), as game `game_seq` of the match. A seq seen again replaces itself.
    pub fn add(&mut self, game_seq: u32, play: &Value) {
        if let Some(seq) = play["seq"].as_i64() {
            self.games.entry(game_seq).or_default().insert(seq, play.clone());
        }
    }

    fn winners(&self) -> impl Iterator<Item = u8> + '_ {
        self.games.values().filter_map(|g| game_winner_seat(&g.values().cloned().collect::<Vec<_>>()))
    }

    pub fn wins(&self) -> [u32; 2] {
        let mut w = [0u32; 2];
        for seat in self.winners() { w[seat as usize] += 1; }
        w
    }

    pub fn decided(&self) -> u32 {
        self.winners().count() as u32
    }
}

/// Wins that take a ranked series: ceil(best_of / 2), from the match's rules when the server sent them.
pub fn wins_needed(m: &crate::matchmaking::MatchView) -> u32 {
    let best = m.rules.as_ref().map(|r| r.ranked_best_of).filter(|b| *b >= 1).unwrap_or(m.best_of.max(1));
    (best as u32).div_ceil(2)
}

/// How long the close waits for the deciding game's capture file (spec §3.2: "at most 30 s").
pub const CLOSE_WAIT_MS: u64 = 30_000;
/// Then Dolphin is stopped about this much later (spec §3.2: "about 3 s later").
pub const STOP_DELAY_MS: u64 = 3_000;
/// A capture counts as the decider's if Dolphin wrote it no earlier than this before the decision was seen (the
/// file lands with the game's last play, and the tally can lag a poll or two behind it).
pub const CAPTURE_SLACK_MS: u64 = 120_000;

/// Is a ranked series over: this launcher's own tally reached `need`, or the server already closed it?
pub fn series_over(local: [u32; 2], need: u32, server_kind: Option<&str>) -> bool {
    local.iter().any(|w| need > 0 && *w >= need) || server_kind.is_some_and(|k| k != "open")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseStep { Nothing, Waiting, Stop }

/// The ranked close's clock (spec §3.2; ruling R11). Each launcher runs its own.
#[derive(Debug, Default, Clone)]
pub struct RankedClose {
    decided_at_ms: Option<u64>,
    capture_seen_ms: Option<u64>,
    stopped: bool,
}

impl RankedClose {
    pub fn step(&mut self, now_ms: u64, over: bool, newest_capture_ms: Option<u64>) -> CloseStep {
        if self.stopped || !over { return CloseStep::Nothing; }
        let at = *self.decided_at_ms.get_or_insert(now_ms);
        if self.capture_seen_ms.is_none() && newest_capture_ms.is_some_and(|t| t + CAPTURE_SLACK_MS >= at) {
            self.capture_seen_ms = Some(now_ms);
        }
        let ready_at = self.capture_seen_ms.unwrap_or(u64::MAX).min(at + CLOSE_WAIT_MS);
        if now_ms >= ready_at + STOP_DELAY_MS {
            self.stopped = true;
            return CloseStep::Stop;
        }
        CloseStep::Waiting
    }
}

/// One tick of the close; `stop` is called at most once. True on the tick that stopped Dolphin.
pub fn run_ranked_close(close: &mut RankedClose, now_ms: u64, over: bool, newest: Option<u64>, stop: &mut dyn FnMut() -> Result<(), String>) -> bool {
    match close.step(now_ms, over, newest) {
        CloseStep::Stop => { if let Err(e) = stop() { tracing::warn!("closing Dolphin after the series failed: {e}"); } true }
        _ => false,
    }
}

/// The newest capture file Dolphin wrote under `user_dir` (ms since the epoch), or None.
pub fn newest_capture_ms(user_dir: &std::path::Path) -> Option<u64> {
    crate::stats::dolphin_capture_dirs(user_dir).into_iter()
        .filter_map(|(dir, _)| std::fs::read_dir(dir).ok())
        .flatten()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|e| e.metadata().ok()?.modified().ok())
        .filter_map(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .max()
}

use crate::platform::ExitKind;

/// What made this launcher leave a running series (spec §3.4). The backend counts an unknown reason as a plain
/// leave (a ranked forfeit), so these strings are exact.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeaveReason { Leave, DolphinClosed, DolphinCrashed }

impl LeaveReason {
    pub fn as_str(self) -> &'static str {
        match self {
            LeaveReason::Leave => "leave",
            LeaveReason::DolphinClosed => "dolphin-closed",
            LeaveReason::DolphinCrashed => "dolphin-crashed",
        }
    }
}

/// Is this ranked series already decided when its Dolphin exits: this launcher's tally reached `need`, or the server
/// says win/forfeit. An exit then is not a leave, even before the launcher's own close fired (Task 18 review I1).
/// Unlike [`series_over`], a held or abandoned series is NOT decided (review L1): its exit is reported as usual.
/// Casual has no decider (any leave resolves it now), so it is never decided here.
pub fn decided_for_exit(mode: &str, local: [u32; 2], need: u32, server_kind: Option<&str>) -> bool {
    if mode != "ranked" { return false; }
    match server_kind {
        Some("win" | "forfeit") => true,
        Some("held" | "abandoned") => false,
        _ => local.iter().any(|w| need > 0 && *w >= need),
    }
}

/// Did this PC's room lose the other player during the series (ruling R1; Task 19 review I2)? Only while the
/// match is `running` or between games (`intermission`) and our own Dolphin still runs: a room the fork reports
/// `closed` or `error`, or fewer than two players anywhere but a pre-start lobby that is still filling.
pub fn room_lost_peer(relay_state: &str, phase: &str, players: usize, active: bool, played_together: bool) -> bool {
    matches!(peer_reading(relay_state, phase, players, active, played_together), PeerReading::Alone | PeerReading::Blank)
}

/// One reading of this PC's room during a series (final fix L-I1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeerReading {
    /// Both players are in the running room or lobby: any earlier sighting is cleared.
    Present,
    /// Only our own player is left in a running room or lobby (our Dolphin still runs, so the one left is us): the
    /// peer is gone. Counted at once, whenever our Dolphin exits.
    Alone,
    /// The room reads closed, error or empty. Our own shutdown can look like this, so it counts only when seen at
    /// least PEER_LOST_MIN_MS before our exit.
    Blank,
    /// Nothing to conclude (not mid-series, our Dolphin gone, a pre-start lobby still filling).
    Unknown,
}

/// Classify one room reading; see [`PeerReading`]. Only while the match is `running` or between games
/// (`intermission`) and our own Dolphin still runs.
pub fn peer_reading(relay_state: &str, phase: &str, players: usize, active: bool, played_together: bool) -> PeerReading {
    if !active || !matches!(relay_state, "running" | "intermission") { return PeerReading::Unknown; }
    if matches!(phase, "closed" | "error") { return PeerReading::Blank; }
    let open_room = matches!(phase, "running" | "lobby");
    if open_room && players >= 2 { return PeerReading::Present; }
    if !(phase == "running" || relay_state == "intermission" || played_together) { return PeerReading::Unknown; }
    if players == 1 && open_room { PeerReading::Alone } else if players < 2 { PeerReading::Blank } else { PeerReading::Unknown }
}

/// A blank-room sighting first seen less than this before our own Dolphin exited is not counted: our own shutdown
/// blanks the room first, and counting that would turn a player's own close into a lost room (a forfeit-dodge).
pub const PEER_LOST_MIN_MS: u64 = 10_000;

/// The first sighting of a lost peer in one match: when, and whether it was the room left with only us.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PeerSighting { pub since_ms: u64, pub alone: bool }

/// Was the peer already lost before our Dolphin exited at `exited_ms`? A room left with only us counts at once; a
/// blank room only when first seen PEER_LOST_MIN_MS or more before the exit.
pub fn peer_lost_before_exit(sighting: Option<PeerSighting>, exited_ms: u64) -> bool {
    sighting.is_some_and(|s| s.alone || s.since_ms.saturating_add(PEER_LOST_MIN_MS) <= exited_ms)
}

/// Should this exit be reported for snapshot match `m` (final fix L-I2/L-I3)? Only for the match this Dolphin
/// session started with (`session_match`, fixed at launch), never whatever match is current now, and only when the
/// server publishes the series `outcome`: an older backend holds every running leave, so a report there would hold
/// every matchmade match and refuse its captures.
pub fn exit_report_applies(m: &crate::matchmaking::MatchView, session_match: Option<&str>) -> bool {
    session_match == Some(m.id.as_str()) && m.outcome.is_some()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExitReport { Nothing, Leave(LeaveReason), RoomLost(&'static str) }

/// What to tell the server when the NetPlay Dolphin exits (ruling R13). `peer_lost_first`: the room had already
/// lost the other player while this Dolphin still ran, so this exit is a lost room, not a leave (R1).
pub fn exit_report(exit: Option<&ExitKind>, series_running: bool, closing_by_us: bool, peer_lost_first: bool) -> ExitReport {
    if !series_running || closing_by_us { return ExitReport::Nothing; }
    if peer_lost_first { return ExitReport::RoomLost("peer-gone"); }
    match exit {
        Some(ExitKind::Normal) => ExitReport::Leave(LeaveReason::DolphinClosed),
        _ => ExitReport::Leave(LeaveReason::DolphinCrashed),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};
    use crate::platform::ExitKind;

    #[test]
    fn exit_codes_map_to_leave_reasons() {
        assert_eq!(exit_report(Some(&ExitKind::Normal), true, false, false), ExitReport::Leave(LeaveReason::DolphinClosed));
        assert_eq!(exit_report(Some(&ExitKind::Error { code: 3 }), true, false, false), ExitReport::Leave(LeaveReason::DolphinCrashed));
        assert_eq!(exit_report(Some(&ExitKind::Crashed { detail: "AV".into() }), true, false, false), ExitReport::Leave(LeaveReason::DolphinCrashed));
        assert_eq!(exit_report(Some(&ExitKind::Terminated), true, false, false), ExitReport::Leave(LeaveReason::DolphinCrashed), "R13: no exit code 0, held not forfeited");
        assert_eq!(exit_report(None, true, false, false), ExitReport::Leave(LeaveReason::DolphinCrashed));
    }

    #[test]
    fn nothing_is_reported_outside_a_series_or_while_the_launcher_closes_it_and_a_lost_peer_is_a_lost_room() {
        assert_eq!(exit_report(Some(&ExitKind::Normal), false, false, false), ExitReport::Nothing);
        assert_eq!(exit_report(Some(&ExitKind::Normal), true, true, false), ExitReport::Nothing);
        assert_eq!(exit_report(Some(&ExitKind::Normal), true, false, true), ExitReport::RoomLost("peer-gone"), "R1");
        assert_eq!(LeaveReason::DolphinClosed.as_str(), "dolphin-closed");
        assert_eq!(LeaveReason::DolphinCrashed.as_str(), "dolphin-crashed");
        assert_eq!(LeaveReason::Leave.as_str(), "leave");
    }

    #[test]
    fn the_room_lost_its_peer_on_a_drop_mid_game_in_the_lobby_between_games_or_a_closed_or_errored_room() {
        // (relay_state, phase, players, our Dolphin active, played together in this match)
        assert!(room_lost_peer("running", "running", 1, true, true), "peer gone mid-game");
        assert!(room_lost_peer("running", "running", 1, true, false));
        assert!(room_lost_peer("intermission", "lobby", 1, true, true), "peer left between games (review I2)");
        assert!(room_lost_peer("intermission", "lobby", 1, true, false));
        assert!(room_lost_peer("running", "lobby", 1, true, true), "back in the lobby after playing, alone");
        assert!(room_lost_peer("running", "closed", 2, true, true), "room closed under us (review I2)");
        assert!(room_lost_peer("intermission", "error", 2, true, false));
        // Not lost:
        assert!(!room_lost_peer("running", "running", 2, true, true), "both still there");
        assert!(!room_lost_peer("running", "lobby", 1, true, false), "a pre-start lobby still filling");
        assert!(!room_lost_peer("running", "connecting", 0, true, false));
        assert!(!room_lost_peer("running", "running", 1, false, true), "our own Dolphin is gone: that is our exit, not the peer's");
        assert!(!room_lost_peer("joining", "closed", 1, true, false), "pre-series states belong to the relay driver");
        assert!(!room_lost_peer("complete", "running", 1, true, true));
    }

    #[test]
    fn a_blank_room_counts_only_when_seen_well_before_our_own_exit_but_a_room_left_with_only_us_counts_at_once() {
        // Our own Dolphin shutting down can blank its room (closed, error, nobody) before the process ends: a blank
        // sighting that close to the exit is that shutdown, not the peer (a forfeit-dodge otherwise).
        let blank = |t| Some(PeerSighting { since_ms: t, alone: false });
        assert!(peer_lost_before_exit(blank(10_000), 10_000 + PEER_LOST_MIN_MS));
        assert!(peer_lost_before_exit(blank(10_000), 60_000));
        assert!(!peer_lost_before_exit(blank(10_000), 10_000 + PEER_LOST_MIN_MS - 1));
        assert!(!peer_lost_before_exit(None, 60_000));
        // Final fix L-I1: our own player left alone in a running room or lobby is the peer gone, whenever we exit
        // (the fork shutting the joiner's Dolphin right after a host drop must not forfeit the honest player).
        let alone = |t| Some(PeerSighting { since_ms: t, alone: true });
        assert!(peer_lost_before_exit(alone(10_000), 10_000));
        assert!(peer_lost_before_exit(alone(10_000), 10_500));
    }

    #[test]
    fn a_room_reading_is_present_alone_blank_or_unknown() {
        // (relay_state, phase, players, our Dolphin active, played together in this match)
        assert_eq!(peer_reading("running", "running", 2, true, true), PeerReading::Present);
        assert_eq!(peer_reading("intermission", "lobby", 2, true, true), PeerReading::Present, "both back in the lobby");
        assert_eq!(peer_reading("running", "running", 1, true, true), PeerReading::Alone, "only our own player left");
        assert_eq!(peer_reading("intermission", "lobby", 1, true, false), PeerReading::Alone);
        assert_eq!(peer_reading("running", "closed", 2, true, true), PeerReading::Blank);
        assert_eq!(peer_reading("intermission", "error", 2, true, false), PeerReading::Blank);
        assert_eq!(peer_reading("running", "running", 0, true, true), PeerReading::Blank, "an empty room is guarded");
        assert_eq!(peer_reading("running", "lobby", 1, true, false), PeerReading::Unknown, "a pre-start lobby still filling");
        assert_eq!(peer_reading("running", "running", 1, false, true), PeerReading::Unknown, "our own Dolphin is gone");
        assert_eq!(peer_reading("joining", "closed", 1, true, false), PeerReading::Unknown);
    }

    #[test]
    fn the_exit_is_reported_only_for_the_sessions_own_match_on_a_backend_that_knows_series() {
        let mut m = crate::matchmaking::MatchView { id: "m1".into(), mode: "ranked".into(), best_of: 3, ..Default::default() };
        assert!(!exit_report_applies(&m, Some("m1")), "an old backend (no outcome): its leave would hold every match");
        m.outcome = Some(crate::matchmaking::Outcome { kind: "open".into(), ..Default::default() });
        assert!(exit_report_applies(&m, Some("m1")));
        assert!(!exit_report_applies(&m, Some("m2")), "the match pointer moved on: not this session's match");
        assert!(!exit_report_applies(&m, None), "Play and private rooms carry no match id");
    }

    #[test]
    fn a_ranked_series_already_won_is_decided_for_an_exit_but_a_held_or_abandoned_one_is_not() {
        // I1: closed by the player between the deciding play and the launcher's own stop.
        assert!(decided_for_exit("ranked", [2, 0], 2, None));
        assert!(decided_for_exit("ranked", [1, 2], 2, Some("open")));
        assert!(decided_for_exit("ranked", [0, 0], 2, Some("win")));
        assert!(decided_for_exit("ranked", [0, 0], 2, Some("forfeit")));
        // L1: series_over is true for these, but they are not a decided series: the exit is reported.
        assert!(!decided_for_exit("ranked", [0, 0], 2, Some("held")));
        assert!(!decided_for_exit("ranked", [2, 0], 2, Some("held")));
        assert!(!decided_for_exit("ranked", [0, 0], 2, Some("abandoned")));
        assert!(!decided_for_exit("ranked", [1, 1], 2, Some("open")));
        assert!(!decided_for_exit("ranked", [1, 0], 2, None));
        // Casual has no decider: every leave resolves it now.
        assert!(!decided_for_exit("casual", [2, 0], 1, Some("win")));
    }

    fn fixture(name: &str) -> Value {
        let text = match name {
            "644488b4" => include_str!("../tests/fixtures/series/644488b4.json"),
            _ => include_str!("../tests/fixtures/series/796c0bb4.json"),
        };
        serde_json::from_str(text).unwrap()
    }

    fn tally_of(fx: &Value) -> LocalTally {
        let mut t = LocalTally::default();
        for p in fx["plays"].as_array().unwrap() {
            t.add(p["game_seq"].as_u64().unwrap() as u32, &p["body"]);
        }
        t
    }

    #[test]
    fn the_local_tally_matches_the_backend_on_the_shared_fixtures() {
        // 644488b4: Playz (seat 0) won both games 36-8 and 36-6 -> 2-0, the backend's backfill result.
        let t = tally_of(&fixture("644488b4"));
        assert_eq!(t.wins(), [2, 0]);
        assert_eq!(t.decided(), 2);
        // 796c0bb4: stopped at 6-6 -> nothing decided, on either side's rule.
        let t = tally_of(&fixture("796c0bb4"));
        assert_eq!(t.wins(), [0, 0]);
    }

    #[test]
    fn scoring_follows_the_backend_rule() {
        let td = |seq: i64, side: i64, pts: i64| json!({ "seq": seq, "offense_side": side, "ports": [0, 1], "summary": { "points": pts } });
        // A turnover's points go to the defense; a tsst rise is a 2-point safety for `who`, never also offense points.
        let pick6 = json!({ "seq": 3, "offense_side": 0, "ports": [0, 1], "summary": { "points": 6, "turnover": true } });
        let safety = json!({ "seq": 4, "offense_side": 1, "ports": [0, 1], "summary": { "points": 0 },
            "stat_changes": [{ "who": 0, "team": true, "field": "tsst", "before": 0, "after": 1 }] });
        assert_eq!(score_of(&[td(1, 0, 6), td(2, 1, 6), pick6, safety]), [8, 12]);
        let won: Vec<Value> = (1..=6).map(|s| td(s, 1, 6)).collect();
        assert_eq!(game_winner_seat(&won), Some(1));
        let no_ports: Vec<Value> = (1..=6).map(|s| json!({ "seq": s, "offense_side": 0, "summary": { "points": 6 } })).collect();
        assert_eq!(game_winner_seat(&no_ports), None, "R6: no known seat counts for nobody");
    }

    #[test]
    fn a_play_seen_twice_counts_once() {
        let mut t = LocalTally::default();
        for s in 1..=6 { let p = json!({ "seq": s, "offense_side": 0, "ports": [0, 1], "summary": { "points": 6 } }); t.add(1, &p); t.add(1, &p); }
        assert_eq!(t.wins(), [1, 0]);
    }

    #[test]
    fn wins_needed_comes_from_the_rules_then_best_of() {
        let mut m = crate::matchmaking::MatchView { mode: "ranked".into(), best_of: 3, ..Default::default() };
        assert_eq!(wins_needed(&m), 2);
        m.rules = Some(crate::matchmaking::SeriesRules { ranked_best_of: 5, ..Default::default() });
        assert_eq!(wins_needed(&m), 3);
    }

    #[test]
    fn the_series_is_over_at_need_wins_or_when_the_server_says_so() {
        assert!(!series_over([1, 0], 2, Some("open")));
        assert!(series_over([2, 0], 2, Some("open")));
        assert!(series_over([1, 0], 2, Some("win")), "the server's close also closes the room");
        assert!(!series_over([1, 1], 2, None));
    }

    #[test]
    fn the_ranked_close_waits_for_the_capture_then_stops_three_seconds_later_once() {
        let mut c = RankedClose::default();
        assert_eq!(c.step(1_000, false, None), CloseStep::Nothing);
        assert_eq!(c.step(10_000, true, None), CloseStep::Waiting, "decided at 10 s, no capture yet");
        assert_eq!(c.step(14_000, true, Some(13_500)), CloseStep::Waiting, "capture seen at 14 s");
        assert_eq!(c.step(16_999, true, Some(13_500)), CloseStep::Waiting);
        assert_eq!(c.step(17_000, true, Some(13_500)), CloseStep::Stop, "3 s after the capture");
        assert_eq!(c.step(18_000, true, Some(13_500)), CloseStep::Nothing, "never twice");
    }

    #[test]
    fn the_ranked_close_gives_up_waiting_after_30_seconds() {
        // An old capture (game 1's, written long before the decision) is not the decider's.
        let mut c = RankedClose::default();
        assert_eq!(c.step(1_000_000, true, Some(1)), CloseStep::Waiting);
        assert_eq!(c.step(1_010_000, true, Some(1)), CloseStep::Waiting);
        assert_eq!(c.step(1_032_999, true, Some(1)), CloseStep::Waiting);
        assert_eq!(c.step(1_033_000, true, Some(1)), CloseStep::Stop, "30 s wait + 3 s");
    }

    #[test]
    fn run_ranked_close_calls_the_mocked_stop_exactly_once() {
        let mut c = RankedClose::default();
        let mut calls = 0;
        let mut stop = || { calls += 1; Ok(()) };
        assert!(!run_ranked_close(&mut c, 0, true, Some(0), &mut stop));
        assert!(run_ranked_close(&mut c, 3_000, true, Some(0), &mut stop));
        assert!(!run_ranked_close(&mut c, 9_000, true, Some(0), &mut stop));
        assert_eq!(calls, 1);
    }

    #[test]
    fn newest_capture_ms_reads_the_capture_folders() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(newest_capture_ms(d.path()), None);
        let (dir, _) = crate::stats::dolphin_capture_dirs(d.path()).into_iter().next().unwrap();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("g.json"), b"{}").unwrap();
        assert!(newest_capture_ms(d.path()).is_some());
    }
}
