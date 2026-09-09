use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixtures")
}

#[test]
fn every_real_capture_hashes_exactly_as_the_site_php_does() {
    let expected: BTreeMap<String, String> =
        serde_json::from_slice(&fs::read(fixtures().join("expected-hashes.json")).unwrap()).unwrap();
    assert_eq!(expected.len(), 6);
    for (name, want) in &expected {
        let bytes = fs::read(fixtures().join("captures").join(name)).unwrap();
        let got = reborn_core::stats::wire_hash_of(&bytes).expect("a JSON object");
        assert_eq!(&got, want, "{name}");
    }
}

#[test]
fn php_shapes_are_reproduced() {
    let v: serde_json::Value = serde_json::json!({"teams":[{"name":"A","score":1},{"name":"B","score":0}],"style":[1,0],"players":[]});
    assert_eq!(reborn_core::stats::wire_canonical(&v), r#"{"teams":[{"name":"A","score":1},{"name":"B","score":0}],"style":[1,0],"team_totals":[],"players":[]}"#);
    let v: serde_json::Value = serde_json::json!({"teams":[],"style":[],"players":[{"digp":3,"name":" x ","side":1,"stats":{}}]});
    assert_eq!(reborn_core::stats::wire_canonical(&v), r#"{"teams":[],"style":[],"team_totals":[],"players":[{"digp":3,"name":"x","side":1,"stats":[]}]}"#);
    assert!(reborn_core::stats::wire_hash_of(b"[]").is_none());
}

/// `php_int` and `php_str` do not reproduce true PHP semantics for every
/// input; they reproduce what the backend's own JS casts produce, because
/// the backend recomputes this hash and refuses a mismatch against it. This
/// pins that agreement: a non-numeric string casts to 0 like `Number()`
/// does, a float truncates, a bool is 0 or 1, null is 0, and a numeric name
/// stringifies before trimming, just as `phpTrim(String(s ?? ""))` does in
/// `canonical.js`.
#[test]
fn numeric_and_string_casts_match_the_backends_js_semantics() {
    let v: serde_json::Value = serde_json::json!({
        "teams": [{"name": 123, "score": 0}],
        "style": [1.9, true, null, "7"],
        "players": [{"digp": 1, "name": "p", "side": 0, "stats": {"a": "12abc"}}]
    });
    assert_eq!(
        reborn_core::stats::wire_canonical(&v),
        r#"{"teams":[{"name":"123","score":0}],"style":[1,1,0,7],"team_totals":[],"players":[{"digp":1,"name":"p","side":0,"stats":{"a":0}}]}"#
    );
}
