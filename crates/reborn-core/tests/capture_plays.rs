use reborn_core::stats::{wire_hash_of, MAX_PAYLOAD_BYTES};

const G4: &[u8] = include_bytes!("fixtures/captures-pbp/g4-vikings-bengals.json");

#[test]
fn plays_and_team_index_do_not_change_the_wire_hash() {
    let full: serde_json::Value = serde_json::from_slice(G4).unwrap();
    let mut bare = full.clone();
    bare.as_object_mut().unwrap().remove("plays");
    for p in bare["players"].as_array_mut().unwrap() {
        p.as_object_mut().unwrap().remove("team_index");
    }
    let a = wire_hash_of(&serde_json::to_vec(&full).unwrap()).unwrap();
    let b = wire_hash_of(&serde_json::to_vec(&bare).unwrap()).unwrap();
    assert_eq!(a, b, "plays and team_index must stay outside the wire hash (the backend compares plays with its own playsHash)");
}

#[test]
fn a_long_game_with_plays_fits_the_payload_cap() {
    // 400 plays at the G4 average size is the backend's own ceiling for one capture.
    let full: serde_json::Value = serde_json::from_slice(G4).unwrap();
    let per_play = serde_json::to_vec(&full["plays"]).unwrap().len() / full["plays"].as_array().unwrap().len();
    assert!(per_play * 400 + 64 * 1024 <= MAX_PAYLOAD_BYTES, "cap {MAX_PAYLOAD_BYTES} too small for 400 plays of {per_play} bytes");
}

#[test]
fn queued_capture_reaches_the_envelope_with_plays_and_team_index() {
    let dir = tempfile::tempdir().unwrap();
    let user = dir.path().join("User");
    let online = user.join("GameStats").join("Online");
    std::fs::create_dir_all(&online).unwrap();
    std::fs::write(online.join("Vikings_vs_Bengals_001.json"), G4).unwrap();
    // A Live folder of play lines sits beside it and must not be read as a capture.
    let live = user.join("GameStats").join("Live");
    std::fs::create_dir_all(&live).unwrap();
    std::fs::write(live.join("20260923-101500-1.plays.jsonl"), b"{\"schema\":\"nsr.play.v1\"}\n").unwrap();

    let q = reborn_core::stats::StatsQueue::open_at(&dir.path().join("queue"), reborn_core::supervisor::FixedClock::new(1_758_600_000_000)).unwrap();
    q.ingest_all(&user, Some("s1")).unwrap();
    let pending = q.pending().unwrap();
    assert_eq!(pending.len(), 1, "one capture; the Live folder yields none");
    assert!(live.join("20260923-101500-1.plays.jsonl").exists(), "nothing in Live is moved or deleted");

    let bytes = q.capture_bytes(&pending[0].key).unwrap().unwrap();
    let env = reborn_core::backend::envelope(&pending[0], &bytes, None, "3.1.0", "2026-09-23T12:00:00Z").unwrap();
    let fixture: serde_json::Value = serde_json::from_slice(G4).unwrap();
    assert_eq!(env["capture"]["plays"].as_array().unwrap().len(), 44);
    for (sent, orig) in env["capture"]["players"].as_array().unwrap().iter().zip(fixture["players"].as_array().unwrap()) {
        assert!(sent.get("team_index").is_some(), "team_index survives the queue");
        assert_eq!(sent["team_index"], orig["team_index"]);
    }
}
