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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

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
}
