# Launcher Identity and Results Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The launcher signs the player in with Discord through the backend, records league intent at launch, ingests captures from `GameStats\Online` and `Offline`, and sends every result to the backend automatically after each session, with no token ever typed. It also learns the renamed emulator and writes the optimization keys into Dolphin's config.

**Architecture:** Two new core modules, `identity.rs` (device sign-in over the existing `Http` trait and the OS vault) and `backend.rs` (the `Submitter` that wraps a capture in `nsr.submission.v1`), replace `league.rs`. `stats.rs` gains the PHP-exact wire hash, the two capture folders and a waiting state for signed-out players. `session.rs` records intent before Dolphin starts and submits after ingest. The Play page gets a sign-in card and a league toggle; Settings gets Sign out. Every change is gated by `cargo test`, clippy and the page checkers.

**Tech Stack:** Rust workspace (`reborn-core`, `reborn-ui` on Tauri 2), `ureq` 2, `serde_json`, `sha2`, `hex`, `keyring` vault. Plain HTML/JS pages with the `shell.js` helpers. Node checkers in `Tools/`. PowerShell 5.1 for scripts.

**Spec:** `docs/superpowers/specs/2026-09-08-reborn-dolphin-and-stats-pipeline-design.md` sections 4.5, 4.6, 5.1 (folders), 6, 9, 10.3, 10.5.

## Global Constraints

- Repo: `C:\Users\Adam\Desktop\Reborn Installer\NFL Street Reborn Launcher`, branch `main`. Another agent edits `crates/reborn-ui/ui` concurrently: run `git status` before every commit and stage files by name. Never `git add -A`.
- `cargo test --workspace`, `cargo clippy --workspace --all-targets`, `node Tools/check-harness-args.mjs`, `node Tools/check-shell-pages.mjs`, `node Tools/check-page-call-args.mjs`, `node Tools/check-onboarding-ready.mjs`, `node Tools/check-mods-page.mjs` must all pass before a commit that touches their subject. `cargo build --workspace` before running `rebornctl` or the launcher; kill `reborn-launcher` before building.
- Multi-line patch scripts are written with the Write tool as PowerShell or Python files, never as heredocs.
- The wire hash is `si_capture_hash()` from the league site, proven against the six fixture captures (Task 2).
- Vault account `nsr.reborn.player`. Default backend origin `https://api.nflstreetreborn.com`. Nothing prints a token.
- US English in every string. No em dashes. UI stays plain: commands are the contract, every page value comes from a command.
- Version bumps to 3.1.0 (workspace).

---

## File structure

```
crates/reborn-core/src/net.rs           +get_text_as, +delete on the Http trait; UreqHttp and FakeHttp implement them
crates/reborn-core/src/identity.rs      NEW: device sign-in, token in vault, identity.json, sign out, open browser
crates/reborn-core/src/backend.rs       NEW: BackendSubmitter (Submitter), envelope, classify(), auto_submit()
crates/reborn-core/src/league.rs        DELETED
crates/reborn-core/src/stats.rs         wire_hash_of(), GameStats folders, capture_folder + note on QueueEntry,
                                        SubmitOutcome::{Recorded, Unauthorized}, waiting_for_sign_in on SubmitReport
crates/reborn-core/src/supervisor.rs    SessionIntent, SessionRecord.intent, begin_with_intent()
crates/reborn-core/src/session.rs       LaunchRequest.league_match, StatsAfterSession::Ingested{enqueued, sent}, dolphin_in() preference
crates/reborn-core/src/settings.rs      backend_origin
crates/reborn-core/src/runtime.rs       optimization overlays, Logger.ini overlay, packaging audit never-ship folders
crates/reborn-core/src/lib.rs           mod identity; mod backend; drop mod league
crates/reborn-core/tests/fixtures/captures/*.json, expected-hashes.json   copied from the backend repo
crates/reborn-ui/src/main.rs            identity_* commands, launch(league_match), stats_submit without confirm, backend_origin()
crates/reborn-ui/ui/home.js             sign-in card, league toggle, result line
crates/reborn-ui/ui/settings.js         accountBlock replaces leagueBlock; identity_status read
Tools/check-shell-pages.mjs             identity_status fake answer
installer/windows/build.ps1             exe name; -RuntimeOnly staging mode
Cargo.toml                              version 3.1.0
```

---

### Task 1: HTTP trait gains GET-with-bearer and DELETE

**Files:**
- Modify: `crates/reborn-core/src/net.rs`

**Interfaces:**
- Produces: `Http::get_text_as(&self, url, bearer: Option<&str>) -> Result<HttpResponse>`, `Http::delete(&self, url, bearer: Option<&str>) -> Result<HttpResponse>`. `FakeHttp` records `gets: Mutex<Vec<(String, Option<String>)>>` and `deletes: Mutex<Vec<(String, Option<String>)>>` and answers both from `replies`.

- [ ] **Step 1: Write the failing test** (append to the `#[cfg(test)]` block at the bottom of `net.rs`; if there is none, add `#[cfg(test)] mod tests { use super::*; use fake::FakeHttp; ... }`)

```rust
#[test]
fn the_fake_answers_get_with_bearer_and_delete_and_records_them() {
    let http = fake::FakeHttp::default().reply("https://b/v1/me", 200, r#"{"ok":true}"#);
    let r = http.get_text_as("https://b/v1/me", Some("tok")).unwrap();
    assert_eq!(r.status, 200);
    let d = http.delete("https://b/v1/me", Some("tok")).unwrap();
    assert_eq!(d.status, 200);
    assert!(http.get_text_as("https://b/nope", None).is_err(), "an unlisted URL is a transport failure");
    assert_eq!(http.gets.lock().unwrap()[0], ("https://b/v1/me".to_string(), Some("tok".to_string())));
    assert_eq!(http.deletes.lock().unwrap()[0], ("https://b/v1/me".to_string(), Some("tok".to_string())));
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p reborn-core net::`
Expected: compile error, `no method named get_text_as`.

- [ ] **Step 3: Add the two methods to the trait, with refusing defaults**

In `pub trait Http`, after `post_json`:
```rust
    /// GET a small text body with an `Authorization: Bearer` header.
    fn get_text_as(&self, url: &str, bearer: Option<&str>) -> Result<HttpResponse> {
        match bearer {
            None => self.get_text(url),
            Some(_) => Err(Error::PlainIo(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                format!("{url}: this HTTP client cannot send a bearer token"),
            ))),
        }
    }

    /// DELETE, optionally with a bearer. Used to revoke this device's token.
    fn delete(&self, url: &str, bearer: Option<&str>) -> Result<HttpResponse> {
        let _ = bearer;
        Err(Error::PlainIo(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            format!("{url}: this HTTP client cannot DELETE"),
        )))
    }
```

- [ ] **Step 4: Implement both in `impl Http for UreqHttp`**

```rust
    fn get_text_as(&self, url: &str, bearer: Option<&str>) -> Result<HttpResponse> {
        let mut req = self.agent.get(url).set("Accept", "application/json");
        if let Some(token) = bearer {
            req = req.set("Authorization", &format!("Bearer {token}"));
        }
        Self::reply(url, req.call())
    }

    fn delete(&self, url: &str, bearer: Option<&str>) -> Result<HttpResponse> {
        let mut req = self.agent.delete(url).set("Accept", "application/json");
        if let Some(token) = bearer {
            req = req.set("Authorization", &format!("Bearer {token}"));
        }
        Self::reply(url, req.call())
    }
```

- [ ] **Step 5: Teach the fake**

In `fake::FakeHttp` add fields `pub gets: Mutex<Vec<(String, Option<String>)>>` and `pub deletes: Mutex<Vec<(String, Option<String>)>>` (both `Default`). In `impl Http for FakeHttp` add:
```rust
        fn get_text_as(&self, url: &str, bearer: Option<&str>) -> Result<HttpResponse> {
            self.gets.lock().unwrap().push((url.to_string(), bearer.map(str::to_string)));
            match self.replies.get(url) {
                Some((status, body)) => Ok(HttpResponse { status: *status, body: String::from_utf8_lossy(body).into_owned() }),
                None => Err(transport(url, "no canned reply")),
            }
        }
        fn delete(&self, url: &str, bearer: Option<&str>) -> Result<HttpResponse> {
            self.deletes.lock().unwrap().push((url.to_string(), bearer.map(str::to_string)));
            match self.replies.get(url) {
                Some((status, body)) => Ok(HttpResponse { status: *status, body: String::from_utf8_lossy(body).into_owned() }),
                None => Err(transport(url, "no canned reply")),
            }
        }
```
(`transport` is the private helper above; it is in the same module so the fake can call it.)

- [ ] **Step 6: Run the tests**

Run: `cargo test -p reborn-core net::`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/reborn-core/src/net.rs
git commit -m "Http: GET with a bearer and DELETE, for sign-in and sign-out"
```

---

### Task 2: The wire hash, proven against the same six captures

**Files:**
- Modify: `crates/reborn-core/src/stats.rs`
- Create: `crates/reborn-core/tests/fixtures/captures/*.json`, `crates/reborn-core/tests/fixtures/expected-hashes.json`, `crates/reborn-core/tests/wire_hash.rs`

**Interfaces:**
- Produces: `pub fn wire_canonical(cap: &serde_json::Value) -> String`, `pub fn wire_hash_of(bytes: &[u8]) -> Option<String>` (None when the bytes are not a JSON object), in `stats.rs`.

- [ ] **Step 1: Copy the fixtures from the backend repo**

```powershell
$src = "C:\Users\Adam\Desktop\Reborn Installer\Reborn Backend\test\fixtures"
$dst = "crates\reborn-core\tests\fixtures"
New-Item -ItemType Directory "$dst\captures" -Force | Out-Null
Copy-Item "$src\captures\*.json" "$dst\captures\"
Copy-Item "$src\expected-hashes.json" "$dst\"
```
If the backend repo does not exist yet, run the backend plan's Task 2 steps 1 to 3 first; the hashes must come from PHP, never from this crate.

- [ ] **Step 2: Write the failing integration test** `crates/reborn-core/tests/wire_hash.rs`

```rust
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
```

- [ ] **Step 3: Run to verify it fails**

Run: `cargo test -p reborn-core --test wire_hash`
Expected: compile error, `cannot find function wire_hash_of`.

- [ ] **Step 4: Add the functions to `stats.rs`** (near `canonical_bytes`; keep `canonical_bytes` and `content_key` unchanged, they name queue files)

```rust
/// PHP `trim()`: only " \t\n\r\0\x0B". Rust's `trim()` strips more, so it is not used here.
fn php_trim(s: &str) -> &str {
    s.trim_matches(|c| matches!(c, ' ' | '\t' | '\n' | '\r' | '\0' | '\x0B'))
}

/// PHP `(int)` for the values a capture carries.
fn php_int(v: &Value) -> i64 {
    match v {
        Value::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|f| f.trunc() as i64)).unwrap_or(0),
        Value::String(s) => s.trim().parse::<f64>().map(|f| f.trunc() as i64).unwrap_or(0),
        Value::Bool(b) => i64::from(*b),
        _ => 0,
    }
}

/// A PHP assoc array after ksort(): keys in byte order, values cast to int, and an EMPTY one is `[]`.
fn php_sorted_int_map(v: Option<&Value>) -> String {
    let Some(Value::Object(m)) = v else { return "[]".to_string() };
    if m.is_empty() {
        return "[]".to_string();
    }
    let sorted: BTreeMap<&String, i64> = m.iter().map(|(k, v)| (k, php_int(v))).collect();
    let body: Vec<String> = sorted
        .iter()
        .map(|(k, v)| format!("{}:{}", serde_json::to_string(k).unwrap_or_default(), v))
        .collect();
    format!("{{{}}}", body.join(","))
}

fn php_str(v: Option<&Value>) -> String {
    let s = match v {
        Some(Value::String(s)) => s.as_str(),
        _ => "",
    };
    serde_json::to_string(php_trim(s)).unwrap_or_else(|_| "\"\"".to_string())
}

/// The wire hash's input: `si_capture_hash()` from the league site, byte for byte.
///
/// Key order is the PHP array's insertion order (teams, style, team_totals, players),
/// players are sorted by digp, every map is ksorted and int-cast, an absent
/// team_totals is `[]`, and an empty stats map is `[]` because PHP encodes an empty
/// array that way. `serde_json` and PHP's json_encode with UNESCAPED_SLASHES and
/// UNESCAPED_UNICODE escape strings identically for everything a team or player
/// name can hold, except U+2028 and U+2029, which PHP escapes and serde does not.
pub fn wire_canonical(cap: &Value) -> String {
    let arr = |k: &str| -> Vec<&Value> {
        match cap.get(k) {
            Some(Value::Array(a)) => a.iter().collect(),
            _ => Vec::new(),
        }
    };
    let teams: Vec<String> = arr("teams")
        .iter()
        .map(|t| format!("{{\"name\":{},\"score\":{}}}", php_str(t.get("name")), php_int(t.get("score").unwrap_or(&Value::Null))))
        .collect();
    let style: Vec<String> = arr("style").iter().map(|s| php_int(s).to_string()).collect();
    let totals: Vec<String> = arr("team_totals").iter().map(|row| php_sorted_int_map(Some(row))).collect();
    let mut players: Vec<(i64, String)> = arr("players")
        .iter()
        .map(|p| {
            let digp = php_int(p.get("digp").unwrap_or(&Value::Null));
            let line = format!(
                "{{\"digp\":{},\"name\":{},\"side\":{},\"stats\":{}}}",
                digp,
                php_str(p.get("name")),
                php_int(p.get("side").unwrap_or(&Value::Null)),
                php_sorted_int_map(p.get("stats"))
            );
            (digp, line)
        })
        .collect();
    players.sort_by_key(|(d, _)| *d); // stable, like PHP 8's usort
    format!(
        "{{\"teams\":[{}],\"style\":[{}],\"team_totals\":[{}],\"players\":[{}]}}",
        teams.join(","),
        style.join(","),
        totals.join(","),
        players.into_iter().map(|(_, l)| l).collect::<Vec<_>>().join(",")
    )
}

/// The wire `capture_hash`: sha256 hex of [`wire_canonical`]. None when the bytes are not a JSON object.
pub fn wire_hash_of(bytes: &[u8]) -> Option<String> {
    let v: Value = serde_json::from_slice(bytes).ok()?;
    if !v.is_object() {
        return None;
    }
    use sha2::Digest;
    Some(hex::encode(sha2::Sha256::digest(wire_canonical(&v).as_bytes())))
}
```

- [ ] **Step 5: Run the tests**

Run: `cargo test -p reborn-core --test wire_hash`
Expected: 2 passing, six files matched. If one file differs, print `wire_canonical` for it and compare with the backend's `canonicalBytes` on the same file; they must agree with each other and with PHP.

- [ ] **Step 6: Commit**

```bash
git add crates/reborn-core/src/stats.rs crates/reborn-core/tests/wire_hash.rs crates/reborn-core/tests/fixtures
git commit -m "Wire capture hash: the site's PHP recipe, proven on six real captures"
```

---

### Task 3: Queue changes: two capture folders, a waiting state, a note

**Files:**
- Modify: `crates/reborn-core/src/stats.rs`

**Interfaces:**
- Produces:
  - `pub const DOLPHIN_CAPTURE_DIR: &str = "GameStats"`, `pub const LEGACY_CAPTURE_DIR: &str = "LeagueGameStats"`, `pub const CAPTURE_FOLDERS: [&str; 2] = ["Online", "Offline"]`
  - `pub fn dolphin_capture_dirs(user_dir: &Path) -> Vec<(PathBuf, Option<&'static str>)>` (three entries: Online, Offline, legacy with `None`)
  - `QueueEntry.capture_folder: Option<String>` (`"online"` / `"offline"`), `QueueEntry.note: Option<String>`
  - `StatsQueue::ingest_all(&self, user_dir: &Path, session_id: Option<&str>) -> Result<Vec<Enqueued>>`
  - `StatsQueue::ingest_from_folder(&self, dir, session_id, folder: Option<&str>) -> Result<Vec<Enqueued>>` (`ingest_from` keeps its signature and delegates with `None`)
  - `SubmitOutcome::Recorded { note: String }`, `SubmitOutcome::Unauthorized`
  - `SubmitReport.recorded: Vec<CaptureKey>`, `SubmitReport.waiting_for_sign_in: bool`

- [ ] **Step 1: Write the failing tests** (in `stats.rs`'s test module; reuse its existing helpers for a temp queue and a sample capture, which the existing tests around line 2968 use)

```rust
#[test]
fn ingest_all_reads_online_offline_and_the_legacy_folder_and_records_the_folder() {
    let root = tempdir();
    let user = root.path().join("User");
    for (dir, body) in [
        ("GameStats/Online", sample_capture_with_style([10, 20])),
        ("GameStats/Offline", sample_capture_with_style([30, 40])),
        ("LeagueGameStats", sample_capture_with_style([50, 60])),
    ] {
        std::fs::create_dir_all(user.join(dir)).unwrap();
        std::fs::write(user.join(dir).join("g.json"), body).unwrap();
    }
    let q = StatsQueue::open_at(&root.path().join("q"), FixedClock::new(1)).unwrap();
    let got = q.ingest_all(&user, Some("s1")).unwrap();
    assert_eq!(got.len(), 3);
    let folders: Vec<Option<String>> = q.entries().unwrap().into_iter().map(|e| e.capture_folder).collect();
    assert!(folders.contains(&Some("online".into())));
    assert!(folders.contains(&Some("offline".into())));
    assert!(folders.contains(&None));
    assert!(!user.join("GameStats/Online/g.json").exists(), "ingested files are removed");
}

#[test]
fn an_unauthorized_reply_leaves_everything_queued_and_says_so() {
    struct Unauth;
    impl Submitter for Unauth {
        fn submit(&self, _e: &QueueEntry, _c: &[u8]) -> SubmitOutcome { SubmitOutcome::Unauthorized }
    }
    let root = tempdir();
    let q = StatsQueue::open_at(root.path(), FixedClock::new(1)).unwrap();
    q.enqueue(&sample_capture_with_style([1, 2]), None, Some("s1")).unwrap();
    q.enqueue(&sample_capture_with_style([3, 4]), None, Some("s1")).unwrap();
    let r = q.submit_pending(&Unauth, 5).unwrap();
    assert!(r.waiting_for_sign_in);
    assert_eq!(r.accepted.len() + r.held.len() + r.rejected.len(), 0);
    assert!(q.entries().unwrap().iter().all(|e| e.state == EntryState::Queued && e.attempts == 0));
}

#[test]
fn a_recorded_reply_is_terminal_and_keeps_its_note() {
    struct Rec;
    impl Submitter for Rec {
        fn submit(&self, _e: &QueueEntry, _c: &[u8]) -> SubmitOutcome { SubmitOutcome::Recorded { note: "Recorded as incomplete.".into() } }
    }
    let root = tempdir();
    let q = StatsQueue::open_at(root.path(), FixedClock::new(1)).unwrap();
    q.enqueue(&sample_capture_with_style([1, 2]), None, Some("s1")).unwrap();
    let r = q.submit_pending(&Rec, 5).unwrap();
    assert_eq!(r.recorded.len(), 1);
    let e = &q.entries().unwrap()[0];
    assert_eq!(e.state, EntryState::Accepted);
    assert_eq!(e.note.as_deref(), Some("Recorded as incomplete."));
}
```
`sample_capture_with_style([a, b])` returns the bytes of a well-formed `nsr.game.v1` capture whose `style` array is `[a, b]` so each call hashes differently; write it beside the module's existing sample helper if no such helper exists (copy the existing sample JSON and substitute the style values).

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p reborn-core stats::`
Expected: compile errors for `ingest_all`, `capture_folder`, `Unauthorized`, `Recorded`, `waiting_for_sign_in`, `note`.

- [ ] **Step 3: Constants and folder listing**

Replace `pub const DOLPHIN_CAPTURE_DIR: &str = "LeagueGameStats";` with:
```rust
/// The folder Dolphin writes captures into, inside the per-launch user directory,
/// with one subfolder per mode. Dolphin decides the subfolder from its NetPlay state.
pub const DOLPHIN_CAPTURE_DIR: &str = "GameStats";
pub const CAPTURE_FOLDERS: [&str; 2] = ["Online", "Offline"];
/// The folder older runtimes wrote. Still read, so nothing left behind is lost.
pub const LEGACY_CAPTURE_DIR: &str = "LeagueGameStats";

/// Every folder a capture can be in, with the lowercase mode it implies (None for legacy).
pub fn dolphin_capture_dirs(user_dir: &Path) -> Vec<(PathBuf, Option<&'static str>)> {
    vec![
        (user_dir.join(DOLPHIN_CAPTURE_DIR).join("Online"), Some("online")),
        (user_dir.join(DOLPHIN_CAPTURE_DIR).join("Offline"), Some("offline")),
        (user_dir.join(LEGACY_CAPTURE_DIR), None),
    ]
}
```
Keep `dolphin_capture_dir(user_dir)` returning `user_dir.join(DOLPHIN_CAPTURE_DIR)` for the one caller in `main.rs` (`stats_ingest`), which Task 8 switches to `ingest_all`.

- [ ] **Step 4: Entry fields**

In `QueueEntry` add, after `session_id`:
```rust
    /// "online" or "offline", from the folder Dolphin wrote into. Absent for the legacy folder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capture_folder: Option<String>,
    /// A sentence from the server about a terminal result, such as "Recorded as incomplete".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
```
In `enqueue`, add a parameter `folder: Option<&str>` and set `capture_folder: folder.map(str::to_string), note: None` when building the entry. Update every existing `enqueue(` call in the crate (tests included) to pass `None` as the last argument, except the new folder-aware path below.

- [ ] **Step 5: Ingest from a folder, and from all of them**

Rename the body of `ingest_from` to `ingest_from_folder(&self, dir: &Path, session_id: Option<&str>, folder: Option<&str>)`, passing `folder` into `enqueue`. Then:
```rust
    /// Everything Dolphin wrote, from both mode folders and the legacy folder.
    pub fn ingest_all(&self, user_dir: &Path, session_id: Option<&str>) -> Result<Vec<Enqueued>> {
        let mut out = Vec::new();
        for (dir, folder) in dolphin_capture_dirs(user_dir) {
            out.extend(self.ingest_from_folder(&dir, session_id, folder)?);
        }
        Ok(out)
    }

    /// Legacy entry point: one folder, no mode.
    pub fn ingest_from(&self, dir: &Path, session_id: Option<&str>) -> Result<Vec<Enqueued>> {
        self.ingest_from_folder(dir, session_id, None)
    }
```

- [ ] **Step 6: Outcomes and the report**

In `pub enum SubmitOutcome` add:
```rust
    /// The server stored it as a completed record that does not count as a win or loss,
    /// for example a game that ended below 36. Terminal, with a note for the player.
    Recorded { note: String },
    /// The token was refused. The entry stays queued; the caller clears the token.
    Unauthorized,
```
In `SubmitReport` add `pub recorded: Vec<CaptureKey>` and `pub waiting_for_sign_in: bool`. In `submit_pending`'s match add:
```rust
                SubmitOutcome::Recorded { note } => {
                    self.record_terminal(&entry.key, TerminalState::Accepted, None)?;
                    entry.state = EntryState::Accepted;
                    entry.finished_ms = Some(now);
                    entry.last_error = None;
                    entry.note = Some(note);
                    self.write_entry(&entry)?;
                    report.recorded.push(entry.key.clone());
                }
                SubmitOutcome::Unauthorized => {
                    // Not a failed attempt: nothing about the capture was wrong.
                    entry.last_attempt_ms = None;
                    entry.last_error = Some("waiting for sign-in".to_string());
                    self.write_entry(&entry)?;
                    report.waiting_for_sign_in = true;
                    break;
                }
```
(`last_attempt_ms` was set before the match; resetting it keeps the entry looking untouched.) Update `SubmitReport::sent()` to include `recorded.len()`.

- [ ] **Step 7: Run the tests**

Run: `cargo test -p reborn-core stats::`
Expected: all passing, including the three new ones.

- [ ] **Step 8: Commit**

```bash
git add crates/reborn-core/src/stats.rs
git commit -m "Queue: GameStats Online and Offline plus legacy ingest, a waiting state for signed-out players, terminal notes"
```

---

### Task 4: identity.rs

**Files:**
- Create: `crates/reborn-core/src/identity.rs`
- Modify: `crates/reborn-core/src/lib.rs` (add `pub mod identity;`)

**Interfaces:**
- Produces:
```rust
pub const TOKEN_ACCOUNT: &str = "nsr.reborn.player";
pub const DEFAULT_ORIGIN: &str = "https://api.nflstreetreborn.com";
pub struct PlayerFacts { pub id: String, pub display_name: String, pub avatar_hash: Option<String>, pub show_offline: bool }
pub struct IdentityFile { pub player: PlayerFacts, pub signed_in_at_ms: u64 }
pub struct DeviceStart { pub device_code: String, pub user_code: String, pub verify_url: String, pub expires_in: u64, pub interval: u64 }
pub enum Poll { Pending, Expired, Done { token: String, player: PlayerFacts } }
pub fn start(http: &dyn Http, origin: &str, label: &str) -> Result<DeviceStart>
pub fn poll(http: &dyn Http, origin: &str, device_code: &str) -> Result<Poll>
pub fn identity_path(platform: &Platform) -> Result<PathBuf>
pub fn status(platform: &Platform) -> Result<Option<IdentityFile>>
pub fn token(platform: &Platform) -> Result<Option<String>>
pub fn save_signed_in(platform: &Platform, token: &str, player: PlayerFacts, now_ms: u64) -> Result<()>
pub fn clear_token(platform: &Platform) -> Result<()>
pub fn sign_out(platform: &Platform, http: &dyn Http, origin: &str) -> Result<()>
pub fn set_show_offline(platform: &Platform, http: &dyn Http, origin: &str, show: bool) -> Result<PlayerFacts>
pub fn open_in_browser(platform: &Platform, url: &str) -> Result<()>
pub fn machine_label() -> String
```

- [ ] **Step 1: Write the failing tests** (bottom of `identity.rs`)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::fake::FakeHttp;

    #[test]
    fn start_posts_the_label_and_reads_the_device_facts() {
        let http = FakeHttp::default().reply(
            "https://b/v1/auth/device", 200,
            r#"{"device_code":"dc","user_code":"ABCD-EFGH","verify_url":"https://b/link?code=ABCD-EFGH","expires_in":600,"interval":3}"#,
        );
        let s = start(&http, "https://b", "ADAM-PC").unwrap();
        assert_eq!(s.user_code, "ABCD-EFGH");
        assert_eq!(s.interval, 3);
        let posts = http.posts.lock().unwrap();
        assert_eq!(posts[0].0, "https://b/v1/auth/device");
        assert_eq!(posts[0].1, br#"{"label":"ADAM-PC"}"#.to_vec());
        assert_eq!(posts[0].2, None);
    }

    #[test]
    fn poll_maps_428_410_and_200() {
        let pending = FakeHttp::default().reply("https://b/v1/auth/device/poll", 428, r#"{"ok":false,"status":"pending"}"#);
        assert!(matches!(poll(&pending, "https://b", "dc").unwrap(), Poll::Pending));
        let expired = FakeHttp::default().reply("https://b/v1/auth/device/poll", 410, "");
        assert!(matches!(poll(&expired, "https://b", "dc").unwrap(), Poll::Expired));
        let done = FakeHttp::default().reply(
            "https://b/v1/auth/device/poll", 200,
            r#"{"ok":true,"token":"tok","player":{"id":"p1","display_name":"Adam","avatar_hash":null,"show_offline":false}}"#,
        );
        match poll(&done, "https://b", "dc").unwrap() {
            Poll::Done { token, player } => { assert_eq!(token, "tok"); assert_eq!(player.display_name, "Adam"); }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_debug_print_of_a_poll_result_never_shows_the_token() {
        let p = Poll::Done { token: "s3cr3t".into(), player: PlayerFacts { id: "p".into(), display_name: "A".into(), avatar_hash: None, show_offline: false } };
        assert!(!format!("{p:?}").contains("s3cr3t"));
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p reborn-core identity::`
Expected: compile error, module missing.

- [ ] **Step 3: Write identity.rs**

```rust
//! Who the player is: Discord, through the backend, with a per-device token in the OS vault.
//!
//! Device-code sign-in: the launcher asks the backend for a code, opens the verify
//! page in the system browser, and polls until the browser step is done. No port is
//! opened here and no Discord token ever reaches this process. The only secret the
//! launcher holds is the backend's own device token, stored in the vault under
//! [`TOKEN_ACCOUNT`] and never written to disk or logged.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::net::Http;
use crate::platform::{DataDir, Error, Platform, Result};

pub const TOKEN_ACCOUNT: &str = "nsr.reborn.player";
pub const DEFAULT_ORIGIN: &str = "https://api.nflstreetreborn.com";
const IDENTITY_FILE: &str = "identity.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerFacts {
    pub id: String,
    pub display_name: String,
    #[serde(default)]
    pub avatar_hash: Option<String>,
    #[serde(default)]
    pub show_offline: bool,
}

/// The non-secret half of a sign-in, kept beside settings so pages can say who is signed in
/// without asking the vault or the network.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdentityFile {
    pub player: PlayerFacts,
    pub signed_in_at_ms: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeviceStart {
    pub device_code: String,
    pub user_code: String,
    pub verify_url: String,
    pub expires_in: u64,
    pub interval: u64,
}

pub enum Poll {
    Pending,
    Expired,
    Done { token: String, player: PlayerFacts },
}

impl std::fmt::Debug for Poll {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Poll::Pending => f.write_str("Pending"),
            Poll::Expired => f.write_str("Expired"),
            // The token is deliberately absent.
            Poll::Done { player, .. } => f.debug_struct("Done").field("player", player).finish(),
        }
    }
}

fn plain(msg: String) -> Error {
    Error::PlainIo(std::io::Error::other(msg))
}

fn origin_url(origin: &str, path: &str) -> String {
    format!("{}{}", origin.trim_end_matches('/'), path)
}

pub fn start(http: &dyn Http, origin: &str, label: &str) -> Result<DeviceStart> {
    let body = serde_json::json!({ "label": label });
    let url = origin_url(origin, "/v1/auth/device");
    let r = http.post_json(&url, &serde_json::to_vec(&body).unwrap_or_default(), None)?;
    if !r.is_success() {
        return Err(plain(format!("{url} answered {}: {}", r.status, r.body.chars().take(200).collect::<String>())));
    }
    serde_json::from_str(&r.body).map_err(|e| plain(format!("the sign-in reply was not understood: {e}")))
}

pub fn poll(http: &dyn Http, origin: &str, device_code: &str) -> Result<Poll> {
    #[derive(Deserialize)]
    struct Reply { token: Option<String>, player: Option<PlayerFacts> }
    let body = serde_json::json!({ "device_code": device_code });
    let url = origin_url(origin, "/v1/auth/device/poll");
    let r = http.post_json(&url, &serde_json::to_vec(&body).unwrap_or_default(), None)?;
    match r.status {
        428 => Ok(Poll::Pending),
        410 | 404 => Ok(Poll::Expired),
        s if (200..300).contains(&s) => {
            let reply: Reply = serde_json::from_str(&r.body).map_err(|e| plain(format!("the poll reply was not understood: {e}")))?;
            match (reply.token, reply.player) {
                (Some(token), Some(player)) => Ok(Poll::Done { token, player }),
                _ => Err(plain("the poll reply had no token".to_string())),
            }
        }
        s => Err(plain(format!("{url} answered {s}"))),
    }
}

pub fn identity_path(platform: &Platform) -> Result<PathBuf> {
    Ok(platform.data_dir(DataDir::Config)?.join(IDENTITY_FILE))
}

fn read_identity(path: &Path) -> Result<Option<IdentityFile>> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes).ok()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(Error::Io { path: path.to_path_buf(), source }),
    }
}

/// Signed in means BOTH halves exist: the facts file and a token in the vault.
pub fn status(platform: &Platform) -> Result<Option<IdentityFile>> {
    let Some(file) = read_identity(&identity_path(platform)?)? else { return Ok(None) };
    Ok(token(platform)?.map(|_| file))
}

pub fn token(platform: &Platform) -> Result<Option<String>> {
    platform.credentials.retrieve(TOKEN_ACCOUNT)
}

pub fn save_signed_in(platform: &Platform, token: &str, player: PlayerFacts, now_ms: u64) -> Result<()> {
    platform.credentials.store(TOKEN_ACCOUNT, token)?;
    let path = identity_path(platform)?;
    let file = IdentityFile { player, signed_in_at_ms: now_ms };
    std::fs::write(&path, serde_json::to_vec_pretty(&file).unwrap_or_default()).map_err(|source| Error::Io { path, source })
}

/// The server said 401. Drop the token; keep nothing that would make a page claim a sign-in.
pub fn clear_token(platform: &Platform) -> Result<()> {
    let _ = platform.credentials.delete(TOKEN_ACCOUNT);
    let path = identity_path(platform)?;
    if path.exists() {
        std::fs::remove_file(&path).map_err(|source| Error::Io { path, source })?;
    }
    Ok(())
}

/// Revoke this device on the server (best effort), then forget locally.
pub fn sign_out(platform: &Platform, http: &dyn Http, origin: &str) -> Result<()> {
    if let Some(tok) = token(platform)? {
        let _ = http.delete(&origin_url(origin, "/v1/auth/device"), Some(&tok));
    }
    clear_token(platform)
}

pub fn set_show_offline(platform: &Platform, http: &dyn Http, origin: &str, show: bool) -> Result<PlayerFacts> {
    #[derive(Deserialize)]
    struct Reply { player: PlayerFacts }
    let tok = token(platform)?.ok_or_else(|| plain("not signed in".to_string()))?;
    // PATCH is not on the Http trait; the backend accepts the same body on POST /v1/me/show_offline.
    let url = origin_url(origin, "/v1/me/show_offline");
    let r = http.post_json(&url, &serde_json::to_vec(&serde_json::json!({ "show_offline": show })).unwrap_or_default(), Some(&tok))?;
    if r.status == 401 { clear_token(platform)?; return Err(plain("Signed out. Sign in again.".to_string())); }
    if !r.is_success() { return Err(plain(format!("{url} answered {}", r.status))); }
    let reply: Reply = serde_json::from_str(&r.body).map_err(|e| plain(format!("reply not understood: {e}")))?;
    if let Some(mut file) = read_identity(&identity_path(platform)?)? {
        file.player = reply.player.clone();
        let path = identity_path(platform)?;
        std::fs::write(&path, serde_json::to_vec_pretty(&file).unwrap_or_default()).map_err(|source| Error::Io { path, source })?;
    }
    Ok(reply.player)
}

/// Open a URL in the player's default browser through the platform process host.
/// On Windows `explorer.exe <url>` opens the default browser; it exits non-zero even on success,
/// so the handle is dropped, exactly as the folder button does.
pub fn open_in_browser(platform: &Platform, url: &str) -> Result<()> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err(plain(format!("refusing to open a non-http URL: {url}")));
    }
    let program: &str = if cfg!(windows) { "explorer.exe" } else if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
    let args = vec![std::ffi::OsString::from(url)];
    platform.processes.spawn(Path::new(program), &args, None, &std::collections::BTreeMap::new()).map(|_| ())
}

/// The device label shown on the backend: the machine name, or "this PC".
pub fn machine_label() -> String {
    std::env::var("COMPUTERNAME").or_else(|_| std::env::var("HOSTNAME")).unwrap_or_else(|_| "this PC".to_string())
}
```

Backend note carried into the backend plan: add `POST /v1/me/show_offline` as an alias of `PATCH /v1/me` (same handler). Record this in the backend repo's Task 4 router when implementing: `r.post("/v1/me/show_offline", requirePlayer(pool, clock), <same handler as PATCH /v1/me>)`.

- [ ] **Step 4: Register the module**

In `crates/reborn-core/src/lib.rs` add `pub mod identity;` in alphabetical position.

- [ ] **Step 5: Run the tests**

Run: `cargo test -p reborn-core identity::`
Expected: 3 passing. If `std::io::Error::other` is unavailable on the pinned toolchain, use `std::io::Error::new(std::io::ErrorKind::Other, msg)`.

- [ ] **Step 6: Commit**

```bash
git add crates/reborn-core/src/identity.rs crates/reborn-core/src/lib.rs
git commit -m "identity: device-code Discord sign-in, token in the vault, sign out"
```

---

### Task 5: backend.rs replaces league.rs

**Files:**
- Create: `crates/reborn-core/src/backend.rs`
- Delete: `crates/reborn-core/src/league.rs`
- Modify: `crates/reborn-core/src/lib.rs`

**Interfaces:**
- Consumes: `wire_hash_of` (Task 2), `SubmitOutcome::{Recorded, Unauthorized}` (Task 3), `identity::token` (Task 4), `SessionRecord` with `intent` (Task 6, added there; this task compiles against the field, so run Task 6 before running this task's tests, or do Tasks 5 and 6 together).
- Produces:
```rust
pub struct SessionFacts { pub session_id: String, pub league_match: bool, pub exit: String, pub exit_code: Option<i32> }
pub struct BackendSubmitter<'a> { pub http: &'a dyn Http, pub origin: String, token: String, sessions: BTreeMap<String, SessionFacts>, launcher_version: String }
impl BackendSubmitter { pub fn new(http, origin, token, sessions: Vec<SessionFacts>, launcher_version) -> Self }
pub fn envelope(entry: &QueueEntry, capture_bytes: &[u8], facts: Option<&SessionFacts>, launcher_version: &str, now: &str) -> Option<serde_json::Value>
pub fn classify(status: u16, body: &str) -> SubmitOutcome
pub enum SendAttempt { NotSignedIn, Sent(SubmitReport) }
pub fn auto_submit(platform: &Platform, queue: &StatsQueue, history: &[SessionRecord]) -> Result<SendAttempt>
pub fn session_facts(history: &[SessionRecord]) -> Vec<SessionFacts>
```

- [ ] **Step 1: Write the failing tests** (bottom of `backend.rs`)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::fake::FakeHttp;
    use crate::stats::{EntryState, QueueEntry, SubmitOutcome};

    fn entry(session: &str, folder: Option<&str>) -> QueueEntry {
        serde_json::from_str(&format!(
            r#"{{"key":"{}","state":{{"kind":"queued"}},"attempts":0,"first_seen_ms":1,"session_id":"{session}"{}}}"#,
            "0".repeat(64),
            folder.map(|f| format!(r#","capture_folder":"{f}""#)).unwrap_or_default()
        )).unwrap()
    }
    const CAP: &[u8] = br#"{"schema":"nsr.game.v1","source":"dolphin","captured_at":"2026-09-08T00:00:00Z","teams":[{"name":"A","score":36},{"name":"B","score":24}],"style":[1,2],"players":[],"reborn_match":{"schema":"nsr.reborn_match.v1","netplay_running":false,"runtime_version":"1.1.0","local_pads":[]}}"#;

    #[test]
    fn the_envelope_carries_the_capture_the_hash_the_folder_the_session_and_the_intent() {
        let facts = SessionFacts { session_id: "s1".into(), league_match: true, exit: "clean".into(), exit_code: Some(0) };
        let env = envelope(&entry("s1", Some("offline")), CAP, Some(&facts), "3.1.0", "2026-09-08T21:00:00Z").unwrap();
        assert_eq!(env["schema"], "nsr.submission.v1");
        assert_eq!(env["capture_hash"], crate::stats::wire_hash_of(CAP).unwrap());
        assert_eq!(env["capture_folder"], "offline");
        assert_eq!(env["capture"]["schema"], "nsr.game.v1");
        assert_eq!(env["session"]["session_id"], "s1");
        assert_eq!(env["session"]["launcher_version"], "3.1.0");
        assert_eq!(env["session"]["runtime_version"], "1.1.0");
        assert_eq!(env["session"]["exit"], "clean");
        assert_eq!(env["intent"]["league"], true);
        assert_eq!(env["submitted_at"], "2026-09-08T21:00:00Z");
    }

    #[test]
    fn a_legacy_folder_entry_is_sent_as_offline_and_no_session_means_no_league_intent() {
        let env = envelope(&entry("s9", None), CAP, None, "3.1.0", "t").unwrap();
        assert_eq!(env["capture_folder"], "offline");
        assert_eq!(env["intent"]["league"], false);
    }

    #[test]
    fn classify_maps_every_reply() {
        assert_eq!(classify(200, r#"{"ok":true,"status":"accepted","game_id":"g"}"#), SubmitOutcome::Accepted);
        assert_eq!(classify(200, r#"{"ok":true,"status":"duplicate","game_id":"g"}"#), SubmitOutcome::Accepted);
        assert!(matches!(classify(200, r#"{"ok":true,"status":"held","reason":"seat-conflict"}"#), SubmitOutcome::Held { reason } if reason.contains("seat-conflict")));
        assert!(matches!(classify(200, r#"{"ok":true,"status":"incomplete","reason":"below-target-score"}"#), SubmitOutcome::Recorded { .. }));
        assert_eq!(classify(401, "x"), SubmitOutcome::Unauthorized);
        assert!(matches!(classify(400, r#"{"ok":false,"error":"hash-mismatch"}"#), SubmitOutcome::Rejected { reason } if reason == "hash-mismatch"));
        assert!(matches!(classify(429, ""), SubmitOutcome::Deferred { .. }));
        assert!(matches!(classify(503, ""), SubmitOutcome::Deferred { .. }));
    }

    #[test]
    fn the_submitter_posts_with_the_bearer_and_never_prints_it() {
        let http = FakeHttp::default().reply("https://b/v1/results", 200, r#"{"ok":true,"status":"accepted"}"#);
        let s = BackendSubmitter::new(&http, "https://b", "s3cr3t", vec![], "3.1.0");
        assert_eq!(s.submit(&entry("s1", Some("online")), CAP), SubmitOutcome::Accepted);
        let posts = http.posts.lock().unwrap();
        assert_eq!(posts[0].0, "https://b/v1/results");
        assert_eq!(posts[0].2.as_deref(), Some("s3cr3t"));
        assert!(!format!("{s:?}").contains("s3cr3t"));
        let _ = EntryState::Queued;
    }
}
```
`SubmitOutcome` needs `PartialEq` for these assertions; add `#[derive(PartialEq, Eq)]` to it in `stats.rs` if absent.

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p reborn-core backend::`
Expected: compile error, module missing.

- [ ] **Step 3: Write backend.rs**

```rust
//! Sending results to the Reborn backend. The launcher-side owner of the network
//! half of the stats pipeline; Dolphin never posts anything.
//!
//! One destination, always: the backend decides internally whether a game is
//! offline, online, or a league fixture, from the folder Dolphin wrote into, the
//! capture's own identity block, and the league intent this launcher recorded
//! at launch. Nothing here reads a capture to decide where it goes.

use std::collections::BTreeMap;

use crate::identity;
use crate::net::Http;
use crate::platform::{Platform, Result};
use crate::stats::{QueueEntry, StatsQueue, SubmitOutcome, SubmitReport, Submitter};
use crate::supervisor::{SessionEnd, SessionRecord};

/// What the launcher knew about a session, carried into every envelope from it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionFacts {
    pub session_id: String,
    pub league_match: bool,
    /// "clean" | "error" | "crashed" | "terminated" | "interrupted" | "unknown"
    pub exit: String,
    pub exit_code: Option<i32>,
}

pub fn session_facts(history: &[SessionRecord]) -> Vec<SessionFacts> {
    history
        .iter()
        .map(|r| {
            let (exit, code) = match &r.end {
                Some(SessionEnd::Exited { code: Some(0) }) => ("clean", Some(0)),
                Some(SessionEnd::Exited { code }) => ("error", *code),
                Some(SessionEnd::Crashed { .. }) => ("crashed", None),
                Some(SessionEnd::Terminated) => ("terminated", None),
                Some(SessionEnd::Interrupted) => ("interrupted", None),
                None => ("unknown", None),
            };
            SessionFacts {
                session_id: r.id.clone(),
                league_match: r.intent.as_ref().is_some_and(|i| i.league_match),
                exit: exit.to_string(),
                exit_code: code,
            }
        })
        .collect()
}

/// Wrap one capture. None when the bytes are not a JSON object (the queue quarantines those before this).
pub fn envelope(entry: &QueueEntry, capture_bytes: &[u8], facts: Option<&SessionFacts>, launcher_version: &str, now: &str) -> Option<serde_json::Value> {
    let capture: serde_json::Value = serde_json::from_slice(capture_bytes).ok()?;
    if !capture.is_object() {
        return None;
    }
    let hash = crate::stats::wire_hash_of(capture_bytes)?;
    let runtime_version = capture.pointer("/reborn_match/runtime_version").cloned().unwrap_or(serde_json::Value::Null);
    let folder = entry.capture_folder.clone().unwrap_or_else(|| "offline".to_string());
    Some(serde_json::json!({
        "schema": "nsr.submission.v1",
        "capture": capture,
        "capture_hash": hash,
        "capture_folder": folder,
        "session": {
            "session_id": entry.session_id.clone().unwrap_or_default(),
            "launcher_version": launcher_version,
            "runtime_version": runtime_version,
            "exit": facts.map(|f| f.exit.as_str()).unwrap_or("unknown"),
            "exit_code": facts.and_then(|f| f.exit_code),
        },
        "intent": { "league": facts.is_some_and(|f| f.league_match) },
        "flags": entry.summary.as_ref().map(|s| s.flags.clone()).unwrap_or_default(),
        "submitted_at": now,
    }))
}

/// What the backend's answer means for the queue.
pub fn classify(status: u16, body: &str) -> SubmitOutcome {
    #[derive(serde::Deserialize, Default)]
    struct Reply { #[serde(default)] ok: bool, #[serde(default)] status: String, #[serde(default)] reason: Option<String>, #[serde(default)] error: Option<String> }
    let parsed: Option<Reply> = serde_json::from_str(body).ok();
    match status {
        401 => SubmitOutcome::Unauthorized,
        200..=299 => match parsed {
            Some(r) if r.ok => match r.status.as_str() {
                "accepted" | "duplicate" => SubmitOutcome::Accepted,
                "held" => SubmitOutcome::Held { reason: r.reason.unwrap_or_else(|| "held for review".to_string()) },
                "incomplete" => SubmitOutcome::Recorded { note: "Recorded as incomplete: the winner did not reach 36 points.".to_string() },
                other => SubmitOutcome::Held { reason: format!("server status {other:?}") },
            },
            Some(r) => SubmitOutcome::Rejected { reason: r.error.or(r.reason).unwrap_or_else(|| "the server said ok: false".to_string()) },
            None => SubmitOutcome::Rejected { reason: "the server answered 200 with something that is not its reply format".to_string() },
        },
        429 | 500..=599 => SubmitOutcome::Deferred { reason: format!("server answered {status}") },
        _ => SubmitOutcome::Rejected { reason: parsed.and_then(|r| r.error.or(r.reason)).unwrap_or_else(|| format!("refused with {status}: {}", body.chars().take(200).collect::<String>().trim())) },
    }
}

pub struct BackendSubmitter<'a> {
    pub http: &'a dyn Http,
    pub origin: String,
    token: String,
    sessions: BTreeMap<String, SessionFacts>,
    launcher_version: String,
}

impl<'a> BackendSubmitter<'a> {
    pub fn new(http: &'a dyn Http, origin: impl Into<String>, token: impl Into<String>, sessions: Vec<SessionFacts>, launcher_version: impl Into<String>) -> Self {
        Self {
            http,
            origin: origin.into(),
            token: token.into(),
            sessions: sessions.into_iter().map(|s| (s.session_id.clone(), s)).collect(),
            launcher_version: launcher_version.into(),
        }
    }
}

impl std::fmt::Debug for BackendSubmitter<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The token is deliberately absent.
        f.debug_struct("BackendSubmitter").field("origin", &self.origin).finish()
    }
}

fn now_iso() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    // Civil time from seconds, no dependency: days since epoch to y-m-d (Howard Hinnant's algorithm).
    let days = (secs / 86_400) as i64;
    let (h, m, s) = ((secs % 86_400) / 3600, (secs % 3600) / 60, secs % 60);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mth = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if mth <= 2 { y + 1 } else { y };
    format!("{y:04}-{mth:02}-{d:02}T{h:02}:{m:02}:{s:02}Z")
}

impl Submitter for BackendSubmitter<'_> {
    fn submit(&self, entry: &QueueEntry, capture: &[u8]) -> SubmitOutcome {
        let facts = entry.session_id.as_deref().and_then(|id| self.sessions.get(id));
        let Some(env) = envelope(entry, capture, facts, &self.launcher_version, &now_iso()) else {
            return SubmitOutcome::Rejected { reason: "the capture is not a JSON object".to_string() };
        };
        let url = format!("{}/v1/results", self.origin.trim_end_matches('/'));
        match self.http.post_json(&url, &serde_json::to_vec(&env).unwrap_or_default(), Some(&self.token)) {
            Ok(reply) => classify(reply.status, &reply.body),
            Err(e) => SubmitOutcome::Deferred { reason: e.to_string() },
        }
    }
}

pub enum SendAttempt {
    NotSignedIn,
    Sent(SubmitReport),
}

/// Send everything queued, if a token is stored. Clears the token when the server refuses it.
pub fn auto_submit(platform: &Platform, queue: &StatsQueue, history: &[SessionRecord]) -> Result<SendAttempt> {
    let Some(token) = identity::token(platform)? else { return Ok(SendAttempt::NotSignedIn) };
    let origin = crate::settings::Settings::load(&crate::settings::settings_path(platform)?)?
        .backend_origin
        .filter(|o| !o.trim().is_empty())
        .unwrap_or_else(|| identity::DEFAULT_ORIGIN.to_string());
    let http = crate::net::UreqHttp::new(&format!("NFL Street Reborn Launcher/{}", env!("CARGO_PKG_VERSION")));
    let sender = BackendSubmitter::new(&http, origin, token, session_facts(history), env!("CARGO_PKG_VERSION"));
    let report = queue.submit_pending(&sender, 5)?;
    if report.waiting_for_sign_in {
        identity::clear_token(platform)?;
    }
    Ok(SendAttempt::Sent(report))
}
```

- [ ] **Step 4: Remove league.rs and re-point lib.rs**

Delete `crates/reborn-core/src/league.rs`. In `lib.rs` replace `pub mod league;` with `pub mod backend;`. Search the workspace for `league::` and fix each use (Task 8 handles `main.rs`; core has none after this).

- [ ] **Step 5: Run the tests** (after Task 6 has added `SessionRecord.intent`)

Run: `cargo test -p reborn-core backend::`
Expected: 4 passing.

- [ ] **Step 6: Commit**

```bash
git add crates/reborn-core/src/backend.rs crates/reborn-core/src/lib.rs
git rm -q crates/reborn-core/src/league.rs
git commit -m "backend: the submitter that wraps a capture as nsr.submission.v1; league.rs retired"
```

---

### Task 6: Intent recorded at launch; settings gain the backend origin

**Files:**
- Modify: `crates/reborn-core/src/supervisor.rs`, `crates/reborn-core/src/settings.rs`

**Interfaces:**
- Produces: `pub struct SessionIntent { pub league_match: bool }`, `SessionRecord.intent: Option<SessionIntent>`, `Supervisor::begin_with_intent(&self, build: GameBuild, intent: Option<SessionIntent>) -> Result<SessionRecord>` (`begin` delegates with `None`), `Settings.backend_origin: Option<String>`.

- [ ] **Step 1: Write the failing tests**

In `supervisor.rs` tests (there is a test module using `acquire_at` with a temp dir and `FixedClock`; copy its setup):
```rust
#[test]
fn a_session_records_its_intent_and_the_log_keeps_it() {
    let (platform, dir) = test_platform_and_dir();
    let Startup::Owner(sup) = Supervisor::acquire_at(&platform, &dir, FixedClock::new(10)).unwrap() else { panic!() };
    let rec = sup.begin_with_intent(GameBuild::Offline, Some(SessionIntent { league_match: true })).unwrap();
    assert!(rec.intent.as_ref().unwrap().league_match);
    let done = sup.finish(rec, &ExitKind::Normal).unwrap();
    let hist = sup.history().unwrap();
    assert_eq!(hist.last().unwrap().id, done.id);
    assert!(hist.last().unwrap().intent.as_ref().unwrap().league_match);
    let plain = sup.begin(GameBuild::Offline).unwrap();
    assert!(plain.intent.is_none());
}
```
In `settings.rs` tests:
```rust
#[test]
fn backend_origin_round_trips_and_an_old_file_without_it_loads() {
    let dir = tempdir();
    let path = dir.path().join("settings.json");
    std::fs::write(&path, r#"{"schema":"reborn.settings.v1"}"#).unwrap();
    let mut s = Settings::load(&path).unwrap();
    assert_eq!(s.backend_origin, None);
    s.backend_origin = Some("https://api.test".into());
    s.save(&path).unwrap();
    assert_eq!(Settings::load(&path).unwrap().backend_origin.as_deref(), Some("https://api.test"));
}
```
(Use whatever `schema` string the existing settings tests write.)

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p reborn-core supervisor:: settings::`
Expected: compile errors.

- [ ] **Step 3: supervisor.rs**

Add near `SessionRecord`:
```rust
/// What the launcher asserted about a session before the game started. The only routing
/// fact the launcher supplies; online or offline comes from Dolphin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionIntent {
    pub league_match: bool,
}
```
In `SessionRecord` add after `end`:
```rust
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent: Option<SessionIntent>,
```
Set `intent: None` in `SessionRecord::new`. Replace `begin` with:
```rust
    pub fn begin(&self, build: GameBuild) -> Result<SessionRecord> {
        self.begin_with_intent(build, None)
    }

    /// Mark a session as started, recording what the launcher intends it to be.
    pub fn begin_with_intent(&self, build: GameBuild, intent: Option<SessionIntent>) -> Result<SessionRecord> {
        let mut record = SessionRecord::new(build.into(), self.clock.now_ms());
        record.intent = intent;
        self.write_current(&record)?;
        Ok(record)
    }
```

- [ ] **Step 4: settings.rs**

Add to `Settings`, beside `league_ingest_url` (which stays, unused, so older files load; put `/// Superseded by backend_origin. Kept so older settings files still parse.` on it):
```rust
    /// Where sign-in and match results go. Absent means the product backend;
    /// see crate::identity::DEFAULT_ORIGIN. Set only by tests and by anyone running
    /// their own backend.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend_origin: Option<String>,
```
Add `backend_origin: None` wherever `Settings` is constructed literally (the `Default` impl around line 172).

- [ ] **Step 5: Run the tests**

Run: `cargo test -p reborn-core supervisor:: settings:: backend::`
Expected: PASS (Task 5's tests compile now too).

- [ ] **Step 6: Commit**

```bash
git add crates/reborn-core/src/supervisor.rs crates/reborn-core/src/settings.rs
git commit -m "Session intent recorded before launch; backend_origin setting"
```

---

### Task 7: Session run: ingest both folders, then send

**Files:**
- Modify: `crates/reborn-core/src/session.rs`

**Interfaces:**
- Produces: `LaunchRequest.league_match: bool`; `StatsAfterSession::Ingested { enqueued: Vec<Enqueued>, sent: Option<SubmitReport> }` (variant reshaped; `Failed(String)` unchanged) with `explain()` covering sending; `dolphin_in` prefers `NFL Street Reborn Dolphin.exe`.

- [ ] **Step 1: Write the failing tests** (session.rs test module, near the existing `dolphin_in` tests around line 1442)

```rust
#[test]
fn the_new_exe_name_wins_when_both_are_present() {
    let dir = tempdir();
    std::fs::write(dir.path().join("Dolphin.exe"), b"").unwrap();
    std::fs::write(dir.path().join("NFL Street Reborn Dolphin.exe"), b"").unwrap();
    assert_eq!(dolphin_in(dir.path()).unwrap().file_name().unwrap(), "NFL Street Reborn Dolphin.exe");
}

#[test]
fn explain_counts_sent_held_recorded_and_waiting() {
    let mut r = crate::stats::SubmitReport::default();
    r.accepted.push(crate::stats::CaptureKey::of_bytes(b"a"));
    r.held.push(crate::stats::CaptureKey::of_bytes(b"b"));
    let s = StatsAfterSession::Ingested { enqueued: vec![], sent: Some(r) };
    let text = s.explain().unwrap();
    assert!(text.contains("1 result sent"), "{text}");
    assert!(text.contains("1 held for review"), "{text}");
    let mut w = crate::stats::SubmitReport::default();
    w.waiting_for_sign_in = true;
    let s = StatsAfterSession::Ingested { enqueued: vec![], sent: Some(w) };
    assert!(s.explain().unwrap().contains("waiting for sign-in"));
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p reborn-core session::`
Expected: compile errors.

- [ ] **Step 3: LaunchRequest and run()**

Add to `LaunchRequest`:
```rust
    /// The player said this is a league match. Recorded in the session before Dolphin
    /// starts; the backend forwards the game to the league site only when it is true.
    pub league_match: bool,
```
Add `league_match: false` to every `LaunchRequest { .. }` literal in the crate's tests. In `Session::run` replace `let record = supervisor.begin(req.build)?;` with:
```rust
        let record = supervisor.begin_with_intent(
            req.build,
            Some(crate::supervisor::SessionIntent { league_match: req.league_match }),
        )?;
```
Replace the `let stats = { ... }` block with:
```rust
        // The match results, collected while the lock is still held and the
        // session id is still in hand, then sent if the player is signed in.
        let stats = {
            let attempt = crate::stats::StatsQueue::open(platform).and_then(|queue| {
                let enqueued = queue.ingest_all(written.user_dir.path(), Some(&finished.id))?;
                let history = supervisor.history()?;
                let sent = match crate::backend::auto_submit(platform, &queue, &history)? {
                    crate::backend::SendAttempt::Sent(report) => Some(report),
                    crate::backend::SendAttempt::NotSignedIn => None,
                };
                Ok((enqueued, sent))
            });
            match attempt {
                Ok((enqueued, sent)) => StatsAfterSession::Ingested { enqueued, sent },
                Err(e) => StatsAfterSession::Failed(e.to_string()),
            }
        };
```

- [ ] **Step 4: StatsAfterSession**

```rust
#[derive(Debug)]
pub enum StatsAfterSession {
    /// One entry per capture file found, including the ones refused, and what sending did.
    /// `sent` is None when nobody is signed in, so the entries wait.
    Ingested {
        enqueued: Vec<crate::stats::Enqueued>,
        sent: Option<crate::stats::SubmitReport>,
    },
    Failed(String),
}

impl StatsAfterSession {
    /// One line for a player. Silent when there was nothing to do.
    pub fn explain(&self) -> Option<String> {
        use crate::stats::Enqueued;
        match self {
            StatsAfterSession::Failed(why) => Some(format!("Match results could not be collected: {why}")),
            StatsAfterSession::Ingested { enqueued, sent } => {
                let found = enqueued.iter().filter(|e| matches!(e, Enqueued::Queued(_) | Enqueued::Quarantined(_))).count();
                let mut parts: Vec<String> = Vec::new();
                if found > 0 {
                    parts.push(format!("{found} result{} recorded", if found == 1 { "" } else { "s" }));
                }
                match sent {
                    None if found > 0 => parts.push("waiting for sign-in".to_string()),
                    None => {}
                    Some(r) => {
                        let n = r.accepted.len() + r.recorded.len();
                        if n > 0 { parts.push(format!("{n} result{} sent", if n == 1 { "" } else { "s" })); }
                        if !r.held.is_empty() { parts.push(format!("{} held for review", r.held.len())); }
                        if !r.rejected.is_empty() { parts.push(format!("{} refused by the server", r.rejected.len())); }
                        if !r.deferred.is_empty() || !r.stalled.is_empty() { parts.push(format!("{} could not be delivered yet", r.deferred.len() + r.stalled.len())); }
                        if r.waiting_for_sign_in { parts.push("waiting for sign-in".to_string()); }
                    }
                }
                if parts.is_empty() { None } else { Some(format!("{}.", parts.join(", "))) }
            }
        }
    }
}
```
Fix every other `StatsAfterSession::Ingested(` construction or match in the workspace (`main.rs` uses `stats.explain()` only; `grep -rn "StatsAfterSession" crates/` to be sure).

- [ ] **Step 5: dolphin_in preference**

Replace `found.sort();` with:
```rust
    // The Reborn name wins over a stock name when both are present, which happens on a
    // tree mid-update. Alphabetical order alone would pick "dolphin.exe".
    found.sort_by_key(|p| {
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("").to_lowercase();
        (if name == "nfl street reborn dolphin.exe" { 0 } else { 1 }, name)
    });
```
Also update the doc comment above `dolphin_in` and the refusal text near line 1051 to name `NFL Street Reborn Dolphin.exe`.

- [ ] **Step 6: Run the tests**

Run: `cargo test -p reborn-core`
Expected: all passing.

- [ ] **Step 7: Commit**

```bash
git add crates/reborn-core/src/session.rs
git commit -m "Session: record league intent, ingest both capture folders, send automatically, prefer the Reborn exe"
```

---

### Task 8: Runtime config keys, packaging audit, and the installer allowlist

**Files:**
- Modify: `crates/reborn-core/src/runtime.rs`, `installer/windows/build.ps1`

**Interfaces:**
- Produces: overlays for `Dolphin.ini` (`[General] UseDiscordPresence=False`), `GFX.ini` (`[Settings] DumpTextures/DumpBaseTextures/DumpMipTextures=False`, `[Enhancements] ArbitraryMipmapDetection=False`), a new `Logger.ini` overlay (`[Options] WriteToFile=False, Verbosity=1, WriteToConsole=False, WriteToWindow=False`); `required_game_settings` adds `("Video_Settings","DumpTextures","False")`, `("Video_Settings","DumpBaseTextures","False")`, `("Video_Settings","DumpMipTextures","False")`, `("Video_Enhancements","ArbitraryMipmapDetection","False")`; `RuntimePackager::audit` flags `User/LeagueGameStats` and `User/GameStats` directories; `build.ps1` stages `NFL Street Reborn Dolphin.exe` and gains `-RuntimeOnly`.

- [ ] **Step 1: Write the failing tests** (runtime.rs test module)

```rust
#[test]
fn the_optimization_keys_are_generated_and_the_freeze_fix_is_kept() {
    let cfg = LaunchConfig { build: GameBuild::Offline, graphics: GraphicsPreset::Balanced, fullscreen: true, game_path: "g.iso".into(), user_dir: "u".into(), match_context: None };
    let o = ConfigGenerator::overlays(&cfg);
    assert_eq!(o["GFX.ini"]["Settings"]["DumpTextures"], "False");
    assert_eq!(o["GFX.ini"]["Settings"]["DumpBaseTextures"], "False");
    assert_eq!(o["GFX.ini"]["Enhancements"]["ArbitraryMipmapDetection"], "False");
    assert_eq!(o["Dolphin.ini"]["General"]["UseDiscordPresence"], "False");
    assert_eq!(o["Logger.ini"]["Options"]["WriteToFile"], "False");
    assert_eq!(o["Logger.ini"]["Options"]["Verbosity"], "1");
    let g = ConfigGenerator::required_game_settings();
    assert_eq!(g["Core"]["SyncGPU"], "True");
    assert_eq!(g["Video_Settings"]["CacheHiresTextures"], "True");
    assert_eq!(g["Video_Settings"]["DumpTextures"], "False");
    assert_eq!(g["Video_Enhancements"]["ArbitraryMipmapDetection"], "False");
}

#[test]
fn captured_results_inside_a_runtime_package_are_a_violation() {
    let dir = tempdir();
    std::fs::create_dir_all(dir.path().join("User").join("GameStats").join("Online")).unwrap();
    let found = RuntimePackager::audit(dir.path()).unwrap();
    assert!(found.iter().any(|v| v.path == "User/GameStats"), "{found:?}");
}
```
(Match the real `LaunchConfig` field list and `GraphicsPreset` variant names in this file; the test above names the fields shown in `session.rs` line 496.)

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p reborn-core runtime::`
Expected: the two new tests fail.

- [ ] **Step 3: overlays()**

In `overlays`, after the `UsePanicHandlers`/`ConfirmStop` lines add:
```rust
        // Dolphin's own Discord presence is retired so the launcher can own presence later;
        // two publishers for one account race on the local IPC socket.
        set(&mut dolphin, "General", "UseDiscordPresence", "False");
```
After the `EFBAccessEnable` line add:
```rust
        // Texture dumping was left on in the reference installs: a disk write per new texture.
        set(&mut gfx, "Settings", "DumpTextures", "False");
        set(&mut gfx, "Settings", "DumpBaseTextures", "False");
        set(&mut gfx, "Settings", "DumpMipTextures", "False");
        // Per-upload CPU cost that the HD pack makes pure waste.
        set(&mut gfx, "Enhancements", "ArbitraryMipmapDetection", "False");
```
Before `out.insert("GFX.ini".to_string(), gfx);` add:
```rust
        // The freeze instrumentation left file logging on; it costs a write per line.
        let mut logger = Overlay::new();
        set(&mut logger, "Options", "WriteToFile", "False");
        set(&mut logger, "Options", "Verbosity", "1");
        set(&mut logger, "Options", "WriteToConsole", "False");
        set(&mut logger, "Options", "WriteToWindow", "False");
        out.insert("Logger.ini".to_string(), logger);
```
Note the `Video_` prefix loop copies `required_game_settings` into `gfx`; `Video_Enhancements` becomes `Enhancements` through the existing `strip_prefix("Video_")`, so no change is needed there. Update `logger_ini()` to the same four values.

- [ ] **Step 4: required_game_settings()**

Add to the array:
```rust
            ("Video_Settings","DumpTextures","False"),("Video_Settings","DumpBaseTextures","False"),("Video_Settings","DumpMipTextures","False"),
            ("Video_Enhancements","ArbitraryMipmapDetection","False"),
```

- [ ] **Step 5: audit()**

After the forbidden-files loop in `audit`:
```rust
        for dir in ["LeagueGameStats", "GameStats"] {
            if user_dir.join(dir).is_dir() {
                found.push(PackagingViolation {
                    path: format!("User/{dir}"),
                    reason: "captured match results inside the runtime package; they belong to a player",
                });
            }
        }
```

- [ ] **Step 6: build.ps1**

In `$runtimeFiles` replace `"Dolphin.exe"` with `"NFL Street Reborn Dolphin.exe"`. Replace the check
`if (-not (Test-Path (Join-Path $dest "Dolphin.exe")))` with the new name and the message `"No NFL Street Reborn Dolphin.exe in $Runtime. Nothing was built."`. Add a switch `[switch]$RuntimeOnly` to `param(...)`. Immediately after the runtime staging block ends (after the `Write-Host ("Runtime: {0} file(s)...` line), add:
```powershell
if ($RuntimeOnly) {
    # Stage the runtime alone, for a runtime component publish. Nothing else is built.
    $only = Join-Path $PSScriptRoot "..\..\target\installer-payload-runtime"
    if (Test-Path $only) { Remove-Item -Recurse -Force $only }
    New-Item -ItemType Directory (Join-Path $only "runtime") -Force | Out-Null
    Copy-Item (Join-Path $payload "runtime\*") (Join-Path $only "runtime") -Recurse -Force
    Write-Host "Runtime staged alone at $only\runtime"
    exit 0
}
```
and relax the argument check so `-RuntimeOnly` needs only `-Runtime`: change the `if (-not $Runtime -or -not $Textures -or -not $PromptPackages)` throw to `if (-not $RuntimeOnly -and (-not $Runtime -or -not $Textures -or -not $PromptPackages))`. Also search `installer/` for other `Dolphin.exe` literals (`Select-String -Path installer\* -Pattern "Dolphin.exe" -Recurse`) and update each to the new name, including any Inno Setup smoke check.

- [ ] **Step 7: Run the tests and a dry staging**

Run: `cargo test -p reborn-core runtime::`
Expected: PASS.
Run: `powershell -File installer\windows\build.ps1 -RuntimeOnly -Version 3.1.0 -Runtime "C:\Users\Adam\nflstreet2_clean\DOLPHIN ONLINE TEST\CREATION LEAGUE AND NEW DOLPHIN"`
Expected (before the Dolphin plan runs): it throws `No NFL Street Reborn Dolphin.exe in ...`, which is the right answer until the exe is renamed. After the Dolphin plan's Task 6, it stages and exits 0.

- [ ] **Step 8: Commit**

```bash
git add crates/reborn-core/src/runtime.rs installer/windows/build.ps1
git commit -m "Runtime config: dumping off, logger quiet, Discord presence off; audit refuses captured results; installer knows the Reborn exe"
```

---

### Task 9: Commands in main.rs

**Files:**
- Modify: `crates/reborn-ui/src/main.rs`

**Interfaces:**
- Produces commands: `identity_status() -> IdentityView`, `identity_sign_in_start() -> SignInStartView`, `identity_sign_in_poll(device_code: String) -> SignInPollView`, `identity_sign_out() -> IdentityView`, `identity_set_show_offline(show: bool) -> IdentityView`; `launch(..., league_match: Option<bool>)`; `stats_submit(confirm: Option<bool>)` sends without gating. Removed: `stats_credential_set`, `stats_credential_forget`, `league_url`.

```rust
#[derive(Debug, Serialize)]
struct IdentityView { signed_in: bool, display_name: Option<String>, player_id: Option<String>, show_offline: Option<bool> }
#[derive(Debug, Serialize)]
struct SignInStartView { device_code: String, user_code: String, verify_url: String, interval: u64, opened: bool }
#[derive(Debug, Serialize)]
struct SignInPollView { state: &'static str, display_name: Option<String> }
```

- [ ] **Step 1: Write the failing unit test** (main.rs `mod tests`)

```rust
#[test]
fn identity_view_from_a_file_and_from_nothing() {
    let file = reborn_core::identity::IdentityFile {
        player: reborn_core::identity::PlayerFacts { id: "p1".into(), display_name: "Adam".into(), avatar_hash: None, show_offline: true },
        signed_in_at_ms: 1,
    };
    let v = identity_view(Some(&file));
    assert!(v.signed_in);
    assert_eq!(v.display_name.as_deref(), Some("Adam"));
    assert_eq!(v.show_offline, Some(true));
    let none = identity_view(None);
    assert!(!none.signed_in && none.player_id.is_none());
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p reborn-ui identity_view`
Expected: compile error.

- [ ] **Step 3: Add the identity commands** (near the old credential commands)

```rust
fn identity_view(file: Option<&reborn_core::identity::IdentityFile>) -> IdentityView {
    match file {
        Some(f) => IdentityView { signed_in: true, display_name: Some(f.player.display_name.clone()), player_id: Some(f.player.id.clone()), show_offline: Some(f.player.show_offline) },
        None => IdentityView { signed_in: false, display_name: None, player_id: None, show_offline: None },
    }
}

/// The backend: the setting, or the product origin.
fn backend_origin(p: &Platform) -> Result<String, String> {
    let file = reborn_core::settings::settings_path(p).map_err(|e| e.to_string())?;
    let settings = reborn_core::settings::Settings::load(&file).map_err(|e| e.to_string())?;
    Ok(settings.backend_origin.filter(|u| !u.trim().is_empty()).unwrap_or_else(|| reborn_core::identity::DEFAULT_ORIGIN.to_string()))
}

#[tauri::command]
async fn identity_status() -> Result<IdentityView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let p = Platform::detect();
        let file = reborn_core::identity::status(&p).map_err(|e| e.to_string())?;
        Ok(identity_view(file.as_ref()))
    }).await.map_err(|e| e.to_string())?
}

/// Start a Discord sign-in and open the verify page in the default browser.
#[tauri::command]
async fn identity_sign_in_start() -> Result<SignInStartView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let p = Platform::detect();
        let origin = backend_origin(&p)?;
        let http = http_client();
        let s = reborn_core::identity::start(&http, &origin, &reborn_core::identity::machine_label()).map_err(|e| e.to_string())?;
        let opened = reborn_core::identity::open_in_browser(&p, &s.verify_url).is_ok();
        Ok(SignInStartView { device_code: s.device_code, user_code: s.user_code, verify_url: s.verify_url, interval: s.interval.max(2), opened })
    }).await.map_err(|e| e.to_string())?
}

/// Ask whether the browser step finished. On "done" the token is already in the vault.
#[tauri::command]
async fn identity_sign_in_poll(device_code: String) -> Result<SignInPollView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let p = Platform::detect();
        let origin = backend_origin(&p)?;
        let http = http_client();
        match reborn_core::identity::poll(&http, &origin, &device_code).map_err(|e| e.to_string())? {
            reborn_core::identity::Poll::Pending => Ok(SignInPollView { state: "pending", display_name: None }),
            reborn_core::identity::Poll::Expired => Ok(SignInPollView { state: "expired", display_name: None }),
            reborn_core::identity::Poll::Done { token, player } => {
                let name = player.display_name.clone();
                reborn_core::identity::save_signed_in(&p, &token, player, now_ms()).map_err(|e| e.to_string())?;
                // The backlog goes out the moment a token exists.
                if let Ok(Startup::Owner(sup)) = Supervisor::acquire(&p) {
                    if let Ok(queue) = StatsQueue::open(&p) {
                        let history = sup.history().unwrap_or_default();
                        let _ = reborn_core::backend::auto_submit(&p, &queue, &history);
                    }
                }
                Ok(SignInPollView { state: "done", display_name: Some(name) })
            }
        }
    }).await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn identity_sign_out() -> Result<IdentityView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let p = Platform::detect();
        let origin = backend_origin(&p)?;
        let http = http_client();
        reborn_core::identity::sign_out(&p, &http, &origin).map_err(|e| e.to_string())?;
        Ok(identity_view(None))
    }).await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn identity_set_show_offline(show: bool) -> Result<IdentityView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let p = Platform::detect();
        let origin = backend_origin(&p)?;
        let http = http_client();
        reborn_core::identity::set_show_offline(&p, &http, &origin, show).map_err(|e| e.to_string())?;
        let file = reborn_core::identity::status(&p).map_err(|e| e.to_string())?;
        Ok(identity_view(file.as_ref()))
    }).await.map_err(|e| e.to_string())?
}
```
`now_ms()` already exists in main.rs (used by the tests at the bottom); `http_client()` exists.

- [ ] **Step 4: launch and stats_submit**

Add `league_match: Option<bool>` as the last parameter of `launch`, and `league_match: league_match.unwrap_or(false),` to its `LaunchRequest`. Rewrite `stats_submit`:
```rust
/// Send everything queued to the backend. No confirmation: sending is what the queue is for,
/// and the server's idempotency makes a repeat harmless.
#[tauri::command]
async fn stats_submit(confirm: Option<bool>) -> Result<StatsSubmitView, String> {
    let _ = confirm;
    tauri::async_runtime::spawn_blocking(move || {
        let p = Platform::detect();
        let Startup::Owner(sup) = Supervisor::acquire(&p).map_err(|e| e.to_string())? else { return Err(already_running()); };
        let url = backend_origin(&p)?;
        let queue = StatsQueue::open(&p).map_err(|e| e.to_string())?;
        let history = sup.history().map_err(|e| e.to_string())?;
        match reborn_core::backend::auto_submit(&p, &queue, &history).map_err(|e| e.to_string())? {
            reborn_core::backend::SendAttempt::NotSignedIn => Ok(StatsSubmitView { url, accepted: 0, held: 0, rejected: 0, deferred: 0, stalled: 0, note: "Sign in with Discord to send your results. They are kept until then.".to_string() }),
            reborn_core::backend::SendAttempt::Sent(report) => Ok(StatsSubmitView {
                url, accepted: report.accepted.len() + report.recorded.len(), held: report.held.len(), rejected: report.rejected.len(), deferred: report.deferred.len(), stalled: report.stalled.len(),
                note: if report.waiting_for_sign_in { "The server no longer accepts this sign-in. Sign in again; nothing was lost.".to_string() } else { submit_note(&report) },
            }),
        }
    }).await.map_err(|e| e.to_string())?
}
```
Update `submit_note` to count `r.recorded` with accepted. Delete `stats_credential_set`, `stats_credential_forget`, `league_url` and the `CredentialSetView` struct if nothing else uses it. In `stats_ingest`, replace `dolphin_capture_dir` + `ingest_from` with `queue.ingest_all(Path::new(&user_dir), session_id.as_deref())` and set `from` to `user_dir`'s `GameStats` path for the view.

- [ ] **Step 5: Register the commands**

In the `invoke_handler` list: remove `stats_credential_set, stats_credential_forget,`; add `identity_status, identity_sign_in_start, identity_sign_in_poll, identity_sign_out, identity_set_show_offline,` under a comment `// Who the player is.`

- [ ] **Step 6: Build and test**

Run: `cargo build --workspace` then `cargo test -p reborn-ui`
Expected: builds; `identity_view_from_a_file_and_from_nothing` passes.

- [ ] **Step 7: Commit**

```bash
git add crates/reborn-ui/src/main.rs
git commit -m "Commands: Discord sign-in, sign out, offline visibility; launch records league intent; submit needs no confirm"
```

---

### Task 10: Pages: sign-in card, league toggle, account block

**Files:**
- Modify: `crates/reborn-ui/ui/home.js`, `crates/reborn-ui/ui/settings.js`, `Tools/check-shell-pages.mjs`

- [ ] **Step 1: Extend the checker's fake core first**

In `Tools/check-shell-pages.mjs` `FULL`, add:
```js
  identity_status: { signed_in: true, display_name: "Adam", player_id: "p1", show_offline: false },
```
and in the Play block of assertions add:
```js
  check("Play shows who is signed in", t.includes("Signed in as Adam"));
  check("Play offers the league toggle", t.includes("This is a league match"));
```
and in the Settings block:
```js
 check("Settings has the account block", t.includes("Signed in as Adam") && t.includes("Sign out"));
```
Run: `node Tools/check-shell-pages.mjs`
Expected: FAIL on the three new checks (the pages do not draw them yet).

- [ ] **Step 2: home.js**

Add `identity: call("identity_status"),` to the `reads` in `load()`. Add state near the top:
```js
let leagueMatch = false;
const signIn = { active: false, deviceCode: null, userCode: null, timer: null, message: null };
```
Pass the toggle into `launch`: add `leagueMatch,` to the `call("launch", { ... })` object, and reset `leagueMatch = false` after `play()` completes (the toggle is per launch and not remembered).

Add the flow and the card:
```js
async function startSignIn() {
  if (busy || signIn.active) return;
  signIn.message = null;
  try {
    const s = await call("identity_sign_in_start");
    signIn.active = true; signIn.deviceCode = s.device_code; signIn.userCode = s.user_code;
    if (!s.opened) signIn.message = `Open ${s.verify_url} in a browser.`;
    draw();
    signIn.timer = setInterval(async () => {
      try {
        const r = await call("identity_sign_in_poll", { deviceCode: signIn.deviceCode });
        if (r.state === "done") { stopSignIn(); await refresh(); }
        else if (r.state === "expired") { stopSignIn("The sign-in code expired. Try again."); draw(); }
      } catch (e) { stopSignIn(reason(e)); draw(); }
    }, Math.max(2, s.interval || 3) * 1000);
  } catch (e) { signIn.message = reason(e); draw(); }
}

function stopSignIn(message) {
  if (signIn.timer) clearInterval(signIn.timer);
  signIn.timer = null; signIn.active = false; signIn.deviceCode = null; signIn.userCode = null;
  if (message) signIn.message = message;
}

function identityCard() {
  const id = state.identity;
  const card = el("section", { class: "block play-identity", "aria-label": "Account" });
  if (signIn.active) {
    card.append(el("p", { text: `Waiting for Discord. Your code is ${signIn.userCode}.` }));
    card.append(el("button", { type: "button", text: "Cancel", onclick: () => { stopSignIn(); draw(); } }));
  } else if (ok(id) && id.signed_in) {
    card.append(el("p", { text: `Signed in as ${id.display_name}` }));
    const box = el("input", { type: "checkbox", id: "league-match", onchange: (e) => { leagueMatch = !!e.target.checked; } });
    box.checked = leagueMatch;
    card.append(el("label", { for: "league-match", class: "name" }, box, el("span", { text: " This is a league match" })));
    if (result?.stats_note) card.append(el("p", { class: "note", text: result.stats_note }));
  } else {
    card.append(el("p", { text: ok(id) ? "Sign in with Discord to record your games." : (id && id.error) || "Sign-in state could not be read." }));
    card.append(el("button", { type: "button", text: "Sign in with Discord", disabled: busy, onclick: startSignIn }));
  }
  if (signIn.message) card.append(el("p", { class: "note", text: signIn.message }));
  return card;
}
```
In `draw()`, after `controls.append(bar);` add `controls.append(identityCard());`. Check that `el` handles `on*` attributes as listeners (it does for `onclick` throughout the file; if `onchange` is not wired, add `box.addEventListener("change", ...)` instead).

- [ ] **Step 3: settings.js**

Replace `token: call("credential_stored", { account: "nsr.league.ingest" }),` with `identity: call("identity_status"),`. Replace `leagueBlock()` with:
```js
function accountBlock() {
  const sec = el("section", { class: "block" }, el("h2", { text: "Account and match results" }));
  const id = state.identity;
  const q = state.queue;
  const rows = [];
  rows.push(fact("Discord", ok(id) ? (id.signed_in ? `Signed in as ${id.display_name}` : "not signed in") : null));
  rows.push(fact("Waiting to send", ok(q) ? String((q.counts && q.counts.queued) ?? q.queued ?? 0) : null));
  sec.append(facts(rows));
  const buttons = el("div", { class: "mod" });
  if (ok(id) && id.signed_in) {
    const box = el("input", { type: "checkbox", id: "show-offline", onchange: (e) => act("Offline visibility", "identity_set_show_offline", { show: !!e.target.checked }, () => { leagueSaid = "Saved."; }) });
    box.checked = !!id.show_offline;
    sec.append(el("label", { for: "show-offline", class: "name" }, box, el("span", { text: " Show my offline games on my profile" })));
    buttons.append(el("button", { type: "button", text: "Sign out", disabled: busy, onclick: () => act("Sign out", "identity_sign_out", {}, () => { leagueSaid = "Signed out."; }) }));
  } else {
    sec.append(el("p", { class: "note", text: "Sign in from the Play page to send results." }));
  }
  buttons.append(el("button", { type: "button", class: "write", text: "Send results now", disabled: busy, onclick: () => act("Send", "stats_submit", {}, (r) => { leagueSaid = r.note; }) }));
  sec.append(buttons);
  if (leagueSaid) sec.append(el("div", { class: "said" }, el("p", { text: leagueSaid })));
  return sec;
}
```
Find where `leagueBlock()` was appended (`Select-String leagueBlock crates\reborn-ui\ui\settings.js`) and append `accountBlock()` there instead; if it was never appended, append it in `draw()` for the General tab: `else parts.push(dolphinBlock(), accountBlock());` on the last branch of line 298's chain. Rename any visible "League results" text to "Match results".

- [ ] **Step 4: Run the checkers**

Run: `node Tools/check-shell-pages.mjs && node Tools/check-page-call-args.mjs && node Tools/check-harness-args.mjs && node Tools/check-onboarding-ready.mjs && node Tools/check-mods-page.mjs`
Expected: all PASS. If `check-harness-args` complains about the removed credential commands, remove their rows from `harness.html`/`app.js` too.

- [ ] **Step 5: Commit**

```bash
git add crates/reborn-ui/ui/home.js crates/reborn-ui/ui/settings.js Tools/check-shell-pages.mjs
git commit -m "Pages: Discord sign-in card and league toggle on Play; account block with sign out and offline visibility in Settings"
```

---

### Task 11: Version 3.1.0, the full gate, and a real session

**Files:**
- Modify: `Cargo.toml` (workspace version), `crates/reborn-ui/tauri.conf.json` if it carries a version, `docs/NEXT_STEPS.md`

- [ ] **Step 1: Bump the version**

In the root `Cargo.toml` `[workspace.package]` set `version = "3.1.0"`. `Select-String -Path crates\reborn-ui\tauri.conf.json -Pattern '"version"'`; if present, set it to `3.1.0` too.

- [ ] **Step 2: The whole gate**

Run: `powershell -File Tools\test-all.ps1`
Expected: every layer `ok`. Then `cargo clippy --workspace --all-targets` with no warnings in changed files.

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock crates/reborn-ui/tauri.conf.json
git commit -m "3.1.0: Discord sign-in and automatic results"
```

- [ ] **Step 4: Real session on Adam's machine (after the backend is live, spec 11 step 2)**

Kill any running `reborn-launcher`. `cargo build --workspace --release`. Run `target\release\reborn-launcher.exe`. On the Play page press Sign in with Discord, complete it in the browser, and confirm the card reads "Signed in as <name>". Press Play, play one offline game to the Game Stats and Player of the Game screens, quit Dolphin. Observe, in order:
1. A file appeared under `<User>\GameStats\Offline\` (old runtime: `LeagueGameStats\`) while Dolphin ran, and it is gone after the session (ingested).
2. `%APPDATA%\NFL Street Reborn Launcher\stats\queue\*.entry.json` has the new entry with `"state":{"kind":"accepted"}` (or `held` with a reason).
3. `curl -s https://api.nflstreetreborn.com/v1/players/<player_id>` shows `games` incremented.
4. The Play page's card shows the result line, for example `1 result recorded, 1 result sent.`
Record the observed values in `docs/NEXT_STEPS.md`. Anything that did not happen is a bug to fix before shipping the update, not a note.

- [ ] **Step 5: Ship the launcher update** (spec 11 step 3)

`powershell -File installer\windows\build.ps1 -LauncherOnly -Version 3.1.0`, then upload the Update exe and `latest.json` to the bucket with the existing rclone script (see `docs/RUNNING_UPDATES.md`, launcher self-update section), and verify `rebornctl update check` reports 3.1.0 available from a 3.0.0 install.

---

## Self-review

**Spec coverage.** 4.5 and 4.6 (UI, signed-out waiting): Tasks 3, 9, 10. 5.1 folders: Task 3. 6 envelope: Task 5. 6.1 hash proof: Task 2. 9.1 core modules and settings: Tasks 4, 5, 6, 7, 8. 9.2 commands and pages: Tasks 9, 10. 10.3 optimization keys in the launcher's config: Task 8. 10.5 allowlist name and `-RuntimeOnly`: Task 8. 11 step 3: Task 11. 12 launcher verification: Task 11 step 4. The `PATCH /v1/me` call became `POST /v1/me/show_offline` because the `Http` trait has no PATCH; the backend plan is told to add that alias.

**Placeholders.** None. Every step has content or an exact command.

**Type consistency.** `SubmitOutcome::Recorded { note }` and `Unauthorized` (Task 3) are what `classify` produces (Task 5) and `submit_pending` consumes (Task 3). `SessionFacts` fields match `envelope`'s reads. `SessionIntent { league_match }` (Task 6) is what `Session::run` writes (Task 7) and `session_facts` reads (Task 5). `identity_view(Option<&IdentityFile>)` matches its test. `ingest_all(&Path, Option<&str>)` is called identically in Tasks 7 and 9.
