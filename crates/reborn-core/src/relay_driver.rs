//! The launcher's half of the matchmaking relay: what to do NEXT, given the server's view of a match
//! and what this PC's Dolphin room is doing.
//!
//! PURE. It reads a [`MatchView`] (from the long-poll) and a [`Room`] (from the fork's launcher status
//! file) and returns [`Step`]s. It never posts, spawns or sleeps; `online_mm` runs the steps and calls
//! [`Memory::record`] for each one that landed, so a failed step is simply planned again next tick.
//!
//! The walk it drives, one tick a second (states are `match_state.js`'s RELAY list):
//!
//! ```text
//! ready          both: send ready + preflight (the server holds on a seed mismatch)
//! go             host: POST host-launched, THEN open the room (the server wants the POST first)
//! host_launching host: relay the 8-character traversal code once the fork shows it
//! code_ready     host: relay a NEW code after a relaunch; joiner: POST joiner-launched, join by code
//! joining        host: map seat 0 -> pad 1, seat 1 -> pad 2; both: report who is in the room
//! running        host: start the game (the SERVER decided the series began, not either launcher)
//! ```
//!
//! A room with a stranger in it is reported at once, not worked around: the server holds that match
//! for a person, and it can only do that if it hears about the stranger.

use crate::matchmaking::MatchView;
use std::collections::BTreeSet;

/// This PC's Dolphin NetPlay room, as the fork's `LauncherStatus` reports it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Room {
    /// The launcher runs a NetPlay Dolphin right now.
    pub active: bool,
    /// `idle` (no session), `connecting`, `lobby`, `running`, `error` or `closed`.
    pub phase: String,
    /// The traversal host code the fork shows, if any.
    pub code: Option<String>,
    pub players: Vec<RoomPlayer>,
    /// Pad 1..4 -> NetPlay player id, 0 = nobody.
    pub ports: Vec<u8>,
    /// The fork's own refusal text when `phase` is `error`.
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct RoomPlayer {
    pub id: u8,
    pub name: String,
    pub ping_ms: Option<u32>,
}

impl Room {
    /// Read `dolphin_settings::netplay_status()`. Anything missing reads as "no room".
    pub fn from_status(v: &serde_json::Value) -> Room {
        let players = v["players"].as_array().map(|a| a.iter().map(|p| RoomPlayer {
            id: p["id"].as_u64().unwrap_or(0).min(255) as u8,
            name: p["name"].as_str().unwrap_or("").trim().to_string(),
            ping_ms: ping(&p["ping"]),
        }).collect()).unwrap_or_default();
        Room {
            active: v["active"].as_bool().unwrap_or(false),
            phase: v["phase"].as_str().unwrap_or("idle").to_string(),
            code: v["code"].as_str().map(str::trim).filter(|c| is_host_code(c)).map(str::to_string),
            players,
            ports: v["ports"].as_array().map(|a| a.iter().map(|p| p.as_u64().unwrap_or(0).min(255) as u8).collect()).unwrap_or_default(),
            error: v["error"].as_str().map(str::trim).filter(|e| !e.is_empty()).map(str::to_string),
        }
    }

    fn pid_of(&self, name: &str) -> Option<u8> {
        self.players.iter().find(|p| p.name == name).map(|p| p.id)
    }
}

/// The fork's ping column is display text ("12 ms", "12"); only its leading number is kept.
fn ping(v: &serde_json::Value) -> Option<u32> {
    if let Some(n) = v.as_u64() { return Some(n.min(u32::MAX as u64) as u32); }
    let digits: String = v.as_str()?.trim().chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

/// This match's value for the game's random number generators. Both launchers derive it from the
/// match id, so both Dolphins start the same dice (lockstep holds) and no two matches start the
/// same dice (the seed save state alone would hand every match identical ones - the fork's
/// `State::SetBootRngSeed` explains). The server needs no column for it: the id IS the secret-free
/// shared value, and it is already on both screens.
pub fn rng_seed_for(match_id: &str) -> u32 {
    use sha2::{Digest, Sha256};
    let h = Sha256::digest(format!("reborn-rng:{match_id}").as_bytes());
    u32::from_be_bytes([h[0], h[1], h[2], h[3]])
}

/// The same rule as the server's `HOST_CODE_RE`: exactly 8 ASCII letters or digits. The fork's label
/// shows other text ("...", an error) before traversal answers, and none of it is a code.
pub fn is_host_code(code: &str) -> bool {
    code.len() == 8 && code.chars().all(|c| c.is_ascii_alphanumeric())
}

#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    /// Send ready with this PC's preflight (the runner measures it).
    Ready,
    PostHostLaunched,
    /// Start a NetPlay Dolphin: host a traversal room, or join `code`. The nickname is the seat's tag.
    OpenRoom { host: bool, nickname: String, code: Option<String> },
    PostHostCode(String),
    PostJoinerLaunched,
    /// Host only: set pad mapping. Always `[seat0 pid, seat1 pid, 0, 0]`.
    AssignPorts([u8; 4]),
    PostObserved { ports: [u8; 4], players: Vec<(String, Option<u32>)> },
    /// Host only: press Start in the room.
    StartGame,
    PostRoomLost(String),
    LeaveRoom,
}

/// What this launcher has already done for the match it is driving. Only [`Memory::record`] writes it,
/// and only for steps that landed, so a failure is retried by planning it again.
#[derive(Debug, Clone, Default)]
pub struct Memory {
    match_id: String,
    last_relay: String,
    ready: bool,
    host_launched: bool,
    room_opened: bool,
    code_sent: Option<String>,
    joiner_launched: bool,
    ports_sent: Option<[u8; 4]>,
    observed_sent: Option<String>,
    started: bool,
    room_lost_sent: bool,
    left: bool,
}

impl Memory {
    pub fn record(&mut self, step: &Step) {
        match step {
            Step::Ready => self.ready = true,
            Step::PostHostLaunched => self.host_launched = true,
            Step::OpenRoom { .. } => { self.room_opened = true; self.left = false; }
            Step::PostHostCode(c) => self.code_sent = Some(c.clone()),
            Step::PostJoinerLaunched => self.joiner_launched = true,
            Step::AssignPorts(p) => self.ports_sent = Some(*p),
            Step::PostObserved { ports, players } => self.observed_sent = Some(signature(ports, players)),
            Step::StartGame => self.started = true,
            Step::PostRoomLost(_) => self.room_lost_sent = true,
            Step::LeaveRoom => self.left = true,
        }
    }

    /// Everything about getting two Dolphins into one room, forgotten; used when the server sends the
    /// pair back to `go` after a lost room, so the host launches again instead of waiting forever.
    fn forget_launch(&mut self) {
        let (id, relay, ready) = (std::mem::take(&mut self.match_id), std::mem::take(&mut self.last_relay), self.ready);
        *self = Memory { match_id: id, last_relay: relay, ready, ..Memory::default() };
    }
}

fn signature(ports: &[u8; 4], players: &[(String, Option<u32>)]) -> String {
    let names: BTreeSet<&str> = players.iter().map(|(n, _)| n.as_str()).collect();
    format!("{ports:?}{names:?}")
}

/// States after `go` in which a room was being built or used. Coming back to `go` from one of them
/// is the server's `room_lost`.
const PAST_GO: [&str; 4] = ["host_launching", "code_ready", "joining", "room_formed"];

/// The next steps, in order. The runner executes them in order and stops at the first failure.
pub fn plan(m: Option<&MatchView>, room: &Room, mem: &mut Memory) -> Vec<Step> {
    let mut steps = Vec::new();
    let Some(m) = m else {
        // No match any more (left, or finished and cleared). A room the relay opened and that is not
        // mid-game is closed; a game in progress is never pulled out from under a player.
        if mem.room_opened && room.active && room.phase != "running" && !mem.left { steps.push(Step::LeaveRoom); }
        return steps;
    };
    if mem.match_id != m.id {
        *mem = Memory { match_id: m.id.clone(), ..Memory::default() };
    }
    let relay = m.relay_state.as_str();
    if relay == "go" && PAST_GO.contains(&mem.last_relay.as_str()) { mem.forget_launch(); }
    mem.last_relay = relay.to_string();

    let (Some(mine), Some(theirs)) = (m.nsr_tag.clone(), m.opponent_nsr_tag.clone()) else { return steps };
    let (tag0, tag1) = if m.seat == 0 { (mine.clone(), theirs) } else { (theirs, mine.clone()) };
    let host = m.you_host;

    match relay {
        "abandoned" | "void" | "held" => {
            if mem.room_opened && room.active && room.phase != "running" && !mem.left { steps.push(Step::LeaveRoom); }
        }
        "ready" => {
            if !m.your_ready && !mem.ready { steps.push(Step::Ready); }
        }
        "go" => {
            if host {
                if room.active {
                    // A room from the attempt that was lost. Close it before launching again.
                    if !mem.left { steps.push(Step::LeaveRoom); }
                    return steps;
                }
                if !mem.host_launched { steps.push(Step::PostHostLaunched); }
                if !mem.room_opened { steps.push(Step::OpenRoom { host: true, nickname: mine, code: None }); }
            }
        }
        "host_launching" | "code_ready" | "joining" | "room_formed" => {
            if host {
                if !mem.room_opened && !room.active {
                    steps.push(Step::OpenRoom { host: true, nickname: mine, code: None });
                    return steps;
                }
                if let Some(code) = &room.code {
                    // `code_ready` also takes a NEW code: a host relaunch legitimately produces one.
                    if relay != "joining" && mem.code_sent.as_deref() != Some(code) && m.host_code.as_deref() != Some(code) {
                        steps.push(Step::PostHostCode(code.clone()));
                    }
                }
            } else if relay == "code_ready" || relay == "joining" {
                if let Some(code) = m.host_code.as_deref().filter(|c| is_host_code(c)) {
                    if relay == "code_ready" && !mem.joiner_launched { steps.push(Step::PostJoinerLaunched); }
                    if !mem.room_opened && !room.active {
                        steps.push(Step::OpenRoom { host: false, nickname: mine, code: Some(code.to_string()) });
                    }
                }
            }
            if mem.room_opened && matches!(room.phase.as_str(), "closed" | "error") && !mem.room_lost_sent
                && matches!(relay, "joining" | "room_formed") {
                steps.push(Step::PostRoomLost(format!("room-{}", room.phase)));
                return steps;
            }
            if relay == "joining" || relay == "room_formed" { observe(&tag0, &tag1, host, room, mem, &mut steps); }
        }
        "running" => {
            if host && room.phase == "lobby" && !mem.started {
                if let Some(want) = desired_ports(&tag0, &tag1, room) {
                    if room.ports != want { steps.push(Step::AssignPorts(want)); }
                    steps.push(Step::StartGame);
                }
            }
        }
        _ => {}
    }
    steps
}

fn desired_ports(tag0: &str, tag1: &str, room: &Room) -> Option<[u8; 4]> {
    Some([room.pid_of(tag0)?, room.pid_of(tag1)?, 0, 0])
}

/// Report the room once it holds both seats, or at once when it holds anyone else.
fn observe(tag0: &str, tag1: &str, host: bool, room: &Room, mem: &Memory, steps: &mut Vec<Step>) {
    if room.phase != "lobby" { return; }
    let stranger = room.players.iter().any(|p| p.name != tag0 && p.name != tag1);
    let desired = desired_ports(tag0, tag1, room);
    let ports: [u8; 4] = match (stranger, desired) {
        (false, Some(want)) => {
            if room.ports != want {
                // The host sets the mapping; the joiner waits to see it arrive.
                if host && mem.ports_sent != Some(want) { steps.push(Step::AssignPorts(want)); }
                return;
            }
            want
        }
        (true, _) => {
            let mut p = [0u8; 4];
            for (slot, v) in p.iter_mut().zip(room.ports.iter()) { *slot = (*v).min(4); }
            p
        }
        (false, None) => return, // the other seat has not arrived yet
    };
    let players: Vec<(String, Option<u32>)> = room.players.iter().map(|p| (p.name.clone(), p.ping_ms)).collect();
    if mem.observed_sent.as_deref() != Some(signature(&ports, &players).as_str()) {
        steps.push(Step::PostObserved { ports, players });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: &str = "nsr:aaaa0000";
    const T1: &str = "nsr:bbbb1111";

    fn view(relay: &str, seat: u8) -> MatchView {
        MatchView {
            id: "m1".into(), relay_state: relay.into(), seat, you_host: seat == 0,
            nsr_tag: Some(if seat == 0 { T0 } else { T1 }.into()),
            opponent_nsr_tag: Some(if seat == 0 { T1 } else { T0 }.into()),
            ..MatchView::default()
        }
    }

    fn room(phase: &str, players: &[(u8, &str)], ports: [u8; 4]) -> Room {
        Room {
            active: true, phase: phase.into(), code: Some("AbCd1234".into()),
            players: players.iter().map(|(id, n)| RoomPlayer { id: *id, name: n.to_string(), ping_ms: Some(20) }).collect(),
            ports: ports.to_vec(),
            error: None,
        }
    }

    fn run(m: &MatchView, r: &Room, mem: &mut Memory) -> Vec<Step> {
        let s = plan(Some(m), r, mem);
        for step in &s { mem.record(step); }
        s
    }

    #[test]
    fn the_host_posts_host_launched_before_it_opens_the_room_and_only_once() {
        let mut mem = Memory::default();
        let s = run(&view("go", 0), &Room::default(), &mut mem);
        assert_eq!(s, vec![Step::PostHostLaunched, Step::OpenRoom { host: true, nickname: T0.into(), code: None }]);
        assert!(run(&view("go", 0), &Room::default(), &mut mem).is_empty(), "a step that landed is not repeated");
        assert!(plan(Some(&view("go", 1)), &Room::default(), &mut Memory::default()).is_empty(), "the joiner waits");
    }

    #[test]
    fn a_failed_step_is_planned_again() {
        let mut mem = Memory::default();
        let s = plan(Some(&view("go", 0)), &Room::default(), &mut mem);
        mem.record(&s[0]); // host-launched landed, the spawn failed
        assert_eq!(plan(Some(&view("go", 0)), &Room::default(), &mut mem),
            vec![Step::OpenRoom { host: true, nickname: T0.into(), code: None }]);
    }

    #[test]
    fn only_a_real_traversal_code_is_relayed_and_a_new_one_follows_a_relaunch() {
        let mut mem = Memory::default();
        run(&view("go", 0), &Room::default(), &mut mem);
        let mut r = room("lobby", &[(1, T0)], [1, 0, 0, 0]);
        r.code = None;
        assert!(run(&view("host_launching", 0), &r, &mut mem).is_empty(), "no code yet, nothing to relay");
        let s = Room::from_status(&serde_json::json!({"active": true, "phase": "lobby", "code": "..."}));
        assert_eq!(s.code, None, "the label's placeholder is not a code");
        r.code = Some("AbCd1234".into());
        assert_eq!(run(&view("host_launching", 0), &r, &mut mem), vec![Step::PostHostCode("AbCd1234".into())]);
        let mut v = view("code_ready", 0);
        v.host_code = Some("AbCd1234".into());
        assert!(run(&v, &r, &mut mem).is_empty());
        r.code = Some("ZzZz9999".into());
        assert_eq!(run(&v, &r, &mut mem), vec![Step::PostHostCode("ZzZz9999".into())]);
    }

    #[test]
    fn the_joiner_announces_then_joins_by_the_relayed_code() {
        let mut v = view("code_ready", 1);
        v.host_code = Some("AbCd1234".into());
        let mut mem = Memory::default();
        assert_eq!(run(&v, &Room::default(), &mut mem), vec![
            Step::PostJoinerLaunched,
            Step::OpenRoom { host: false, nickname: T1.into(), code: Some("AbCd1234".into()) },
        ]);
    }

    #[test]
    fn the_host_maps_seats_to_pads_and_both_report_only_the_mapped_room() {
        let mut mem = Memory::default();
        run(&view("go", 0), &Room::default(), &mut mem);
        mem.record(&Step::PostHostCode("AbCd1234".into()));
        // Joiner has pid 2 but joined first in the fork's list: the mapping is by TAG, never by order.
        let r = room("lobby", &[(2, T1), (1, T0)], [1, 0, 0, 0]);
        assert_eq!(run(&view("joining", 0), &r, &mut mem), vec![Step::AssignPorts([1, 2, 0, 0])]);
        let mapped = room("lobby", &[(2, T1), (1, T0)], [1, 2, 0, 0]);
        let s = run(&view("joining", 0), &mapped, &mut mem);
        assert!(matches!(&s[..], [Step::PostObserved { ports: [1, 2, 0, 0], .. }]), "{s:?}");
        assert!(run(&view("joining", 0), &mapped, &mut mem).is_empty(), "one report per room shape");

        // The joiner never sets ports; it waits for the host's mapping, then reports the same room.
        let mut jm = Memory::default();
        jm.record(&Step::OpenRoom { host: false, nickname: T1.into(), code: None });
        assert!(run(&view("joining", 1), &r, &mut jm).is_empty());
        assert!(matches!(&run(&view("joining", 1), &mapped, &mut jm)[..], [Step::PostObserved { .. }]));
    }

    #[test]
    fn a_stranger_is_reported_at_once_so_the_server_can_hold_the_match() {
        let mut mem = Memory::default();
        mem.record(&Step::OpenRoom { host: true, nickname: T0.into(), code: None });
        let r = room("lobby", &[(1, T0), (3, "Randy")], [1, 0, 0, 0]);
        let s = run(&view("joining", 0), &r, &mut mem);
        match &s[..] {
            [Step::PostObserved { players, .. }] => assert!(players.iter().any(|(n, _)| n == "Randy")),
            other => panic!("expected a stranger report, got {other:?}"),
        }
    }

    #[test]
    fn only_the_host_starts_and_only_once_the_server_says_running() {
        let mapped = room("lobby", &[(1, T0), (2, T1)], [1, 2, 0, 0]);
        let mut mem = Memory::default();
        assert!(!run(&view("joining", 0), &mapped, &mut mem).contains(&Step::StartGame));
        assert_eq!(run(&view("running", 0), &mapped, &mut mem), vec![Step::StartGame]);
        assert!(run(&view("running", 0), &mapped, &mut mem).is_empty());
        assert!(run(&view("running", 1), &mapped, &mut Memory::default()).is_empty());
    }

    #[test]
    fn a_lost_room_is_reported_and_the_host_launches_again_after_go() {
        let mut mem = Memory::default();
        run(&view("go", 0), &Room::default(), &mut mem);
        mem.record(&Step::PostHostCode("AbCd1234".into()));
        let mut dead = room("closed", &[], [0; 4]);
        dead.active = false;
        assert_eq!(run(&view("joining", 0), &dead, &mut mem), vec![Step::PostRoomLost("room-closed".into())]);
        assert_eq!(run(&view("go", 0), &dead, &mut mem),
            vec![Step::PostHostLaunched, Step::OpenRoom { host: true, nickname: T0.into(), code: None }]);
    }

    #[test]
    fn an_ended_match_closes_a_waiting_room_but_never_a_game_in_progress() {
        let lobby = room("lobby", &[(1, T0)], [1, 0, 0, 0]);
        let mut mem2 = Memory::default();
        plan(Some(&view("go", 0)), &Room::default(), &mut mem2);
        mem2.record(&Step::OpenRoom { host: true, nickname: T0.into(), code: None });
        assert_eq!(plan(Some(&view("abandoned", 0)), &lobby, &mut mem2), vec![Step::LeaveRoom]);
        let playing = room("running", &[(1, T0), (2, T1)], [1, 2, 0, 0]);
        assert!(plan(Some(&view("held", 0)), &playing, &mut mem2).is_empty());
        assert!(plan(None, &playing, &mut mem2).is_empty());
        assert_eq!(plan(None, &lobby, &mut mem2), vec![Step::LeaveRoom]);
    }

    #[test]
    fn ready_is_sent_once_and_not_when_the_server_already_has_it() {
        let mut mem = Memory::default();
        assert_eq!(run(&view("ready", 1), &Room::default(), &mut mem), vec![Step::Ready]);
        assert!(run(&view("ready", 1), &Room::default(), &mut mem).is_empty());
        let mut v = view("ready", 0);
        v.your_ready = true;
        assert!(plan(Some(&v), &Room::default(), &mut Memory::default()).is_empty());
    }

    #[test]
    fn every_match_gets_its_own_dice_and_both_seats_the_same_ones() {
        let a = rng_seed_for("f772e15e-28e4-40ab-b667-70483f736542");
        assert_eq!(a, rng_seed_for("f772e15e-28e4-40ab-b667-70483f736542"), "both seats derive the same value");
        let others: std::collections::BTreeSet<u32> =
            (0..1000).map(|i| rng_seed_for(&format!("match-{i}"))).collect();
        assert_eq!(others.len(), 1000, "a thousand matches, a thousand different starts");
    }

    #[test]
    fn the_fork_status_file_is_read_as_it_is_written() {
        let r = Room::from_status(&serde_json::json!({
            "phase": "lobby", "active": true, "host": true, "code": "AbCd1234", "ports": [1, 2, 0, 0],
            "players": [{"id": 1, "name": "nsr:aaaa0000", "status": "OK", "ping": "12 ms", "mapping": "1"}],
        }));
        assert_eq!(r.code.as_deref(), Some("AbCd1234"));
        assert_eq!(r.players[0].ping_ms, Some(12));
        assert_eq!(r.ports, vec![1, 2, 0, 0]);
        assert_eq!(Room::from_status(&serde_json::json!({"phase": "idle", "active": false})).active, false);
        let refused = Room::from_status(&serde_json::json!({"phase": "error", "active": true, "error": "Everyone needs the same game file before starting."}));
        assert_eq!(refused.error.as_deref(), Some("Everyone needs the same game file before starting."));
    }
}
