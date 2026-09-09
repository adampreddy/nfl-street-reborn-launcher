# Reborn Publisher, plan 1 of 4: publishing moves into the core

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move every publishing operation out of `crates/reborn-cli/src/publish.rs` into a new `reborn_core::publish` module with typed requests, typed results and an event sink, and rewire `rebornctl publish` to call it, so that the CLI prints exactly what it prints today and the three e2e scripts pass unchanged.

**Architecture:** `reborn_core::publish` (behind the existing `publish` cargo feature) holds one file per verb: keys, component, release, roster, game_patch, vault. Each verb takes a request struct, an `&mut dyn FnMut(Event)` sink for its human lines, and returns `Result<Outcome<T>>` where `Outcome` is `Done(T)`, `Described` (a dry run) or `Refused(Refusal)`. The CLI keeps its clap types and exit codes, builds requests, prints events, and maps outcomes to exit codes. Nothing visible changes.

**Tech Stack:** Rust 2021 (workspace rust-version 1.82), `reborn-core` with `ed25519-dalek`, `sha2`, `ureq`, `keyring`; `clap` 4 in the CLI; `tempfile` in dev-dependencies; Python 3 e2e scripts under `Tools/`.

**Spec:** `docs/superpowers/specs/2026-09-08-reborn-publisher-design.md`, sections 3.1 and 10 (step 1). Plans 2 to 4 (native upload and status; the app; installer set and delivery) follow this one and are written when it is done, because they consume the names defined here.

## Global Constraints

- Work in a git worktree on a branch, never in the main checkout: another agent (GPTAstra, a Codex session) edits the main tree concurrently and has uncommitted changes in `crates/reborn-core/src/inventory.rs` and `manifest.rs`. Reuse the existing worktree: `cd target\ship-tree && git checkout -B publisher-core main`. Never `git add -A`; add the exact paths each task names. Never use bare `git stash`.
- Never write to a real data root or real origin. Tests use `tempfile::tempdir()`; the e2e scripts use their own scratch folders under `target\`.
- Every human line the CLI prints today must print byte for byte the same after the move. The e2e scripts assert on these substrings: `3 new`, `2 new`, `Read back and verified`, `Fetched`, `Sequence      1`, `Sequence      2`, `pinning roster=2026.09.06, runtime=1.0.2`, `pinning game-patch=2026.09.07, runtime=1.0.0`, `not a .rebornroster`, `strictly higher sequence`, `Verified`, `Chain         now 1 step(s)`, and the regex `// everyday\s+([0-9a-f]{64})`. Moving a `println!` means keeping its format string.
- No em dashes anywhere. US English in every string.
- Secret key bytes are never formatted, logged or put in an event. Only public keys and paths.
- The `publish` feature gates the whole module: `#[cfg(feature = "publish")] pub mod publish;` in `lib.rs`. Run core tests with `cargo test -p reborn-core --features publish`. `cargo test --workspace` also enables it, because `reborn-cli` depends on the feature.
- Commit messages: a one-line subject in plain words, a body when the why is not obvious, and the trailer lines
  `Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>` and
  `Claude-Session: https://claude.ai/code/session_0162ntSTXPjrqYbCJ9aBnBmK`.

---

## File structure

| File | Responsibility |
|---|---|
| `crates/reborn-core/src/publish/mod.rs` (create) | `Event`, `Channel`, `Sink`, `Outcome`, `Refusal`, the `out!`/`err!` macros, constants, `manifest_path`, `manifest_url`, `next_sequence`, `thousands`, `io_err`, `refuse`. Re-exports the verbs. |
| `crates/reborn-core/src/publish/keys.rs` (create) | `keygen`, `read_key`, `write_key`, `rust_array`, `KeysWritten`. |
| `crates/reborn-core/src/publish/component.rs` (create) | `ComponentRequest`, `Published`, `publish_component`, `copy_blob`. |
| `crates/reborn-core/src/publish/release.rs` (create) | `Pin`, `ReleaseRequest`, `Signed`, `release`, `CarryForward`, `republish_pinning`. |
| `crates/reborn-core/src/publish/vault.rs` (create) | `SITE_TOKEN_ACCOUNT`, `site_token`, `store_site_token`, `forget_site_token`. |
| `crates/reborn-core/src/publish/roster.rs` (create) | `RosterRequest`, `Push`, `publish_roster`. |
| `crates/reborn-core/src/publish/game_patch.rs` (create) | `GamePatchRequest`, `publish_game_patch`, `find_xdelta_for_publishing`. |
| `crates/reborn-core/src/lib.rs` (modify, near line 70) | `#[cfg(feature = "publish")] pub mod publish;` |
| `crates/reborn-cli/src/publish.rs` (rewrite) | The clap enum unchanged; `run` builds requests, prints events, maps outcomes to exit codes; `Token` keeps its text and calls `vault`. |
| `docs/RUNNING_UPDATES.md` (modify, section 0) | One paragraph: where publishing lives now. |

The source being moved is `crates/reborn-cli/src/publish.rs` at commit `c4504de`. Line numbers below refer to that file at that commit. Open it beside each task.

---

### Task 1: The module skeleton, `Event`, `Outcome`, and the shared helpers

**Files:**
- Create: `crates/reborn-core/src/publish/mod.rs`
- Modify: `crates/reborn-core/src/lib.rs` (add one line after `pub mod origin;`)
- Test: inline `#[cfg(test)] mod tests` in `mod.rs`

**Interfaces:**
- Produces:
  - `pub enum Channel { Out, Err }`
  - `pub struct Event { pub channel: Channel, pub line: String }` with `Event::out(line)` and `Event::err(line)`
  - `pub type Sink<'a> = &'a mut dyn FnMut(Event);`
  - `pub struct Refusal { pub summary: String }`
  - `pub enum Outcome<T> { Done(T), Described, Refused(Refusal) }`
  - `pub fn refuse<T>(sink: Sink<'_>, lines: &[String]) -> Outcome<T>` (emits every line to `Out`, returns `Refused` with the first line as summary)
  - macros `out!(sink, "fmt", args)` and `err!(sink, "fmt", args)`
  - `pub const COMPONENTS_DIR: &str = "components";`, `PUBLISHER_KEY`, `ROTATION_KEY`
  - `pub fn manifest_path(origin: &Path, id: ComponentId, version: &str) -> PathBuf`
  - `pub fn manifest_url(id: ComponentId, version: &str) -> String`
  - `pub fn next_sequence(origin: &Path, component: ComponentId) -> Result<u64>`
  - `pub fn thousands(n: u64) -> String`
  - `pub(crate) fn io_err(path: &Path, source: std::io::Error) -> Error`

- [ ] **Step 1: Create the worktree branch**

```powershell
Set-Location "C:\Users\Adam\Desktop\Reborn Installer\NFL Street Reborn Launcher\target\ship-tree"
git checkout -B publisher-core main
git log --oneline -1
```
Expected: the last line names commit `c4504de` (or a later main commit).

- [ ] **Step 2: Write the failing tests**

Create `crates/reborn-core/src/publish/mod.rs` with only the test module for now:

```rust
//! The publisher's side of updates: turn a tree into an origin a launcher can
//! update from. One file per verb. Every verb takes a request, an event sink
//! for its human lines, and returns an [`Outcome`].
//!
//! The origin is a directory. Every file it holds is served as-is by any
//! static host. Nothing here talks to a network except the roster fetch.
//!
//! A published file is never rewritten. A blob is content-addressed and so
//! immutable; a component manifest that exists with different bytes is refused;
//! `release.json` only ever moves to a strictly higher sequence. The bytes in
//! the origin are hashed after every copy, and the signed release is read back
//! and verified before success is reported.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::ComponentId;

    #[test]
    fn thousands_groups_digits() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(1000), "1,000");
        assert_eq!(thousands(1234567), "1,234,567");
    }

    #[test]
    fn manifest_names_follow_component_and_version() {
        let origin = std::path::Path::new("C:\\o");
        assert_eq!(
            manifest_path(origin, ComponentId::Runtime, "1.0.2"),
            origin.join("components").join("runtime-1.0.2.json")
        );
        assert_eq!(manifest_url(ComponentId::Roster, "2026.09.06"), "components/roster-2026.09.06.json");
    }

    #[test]
    fn next_sequence_is_one_above_the_highest_manifest_of_that_component() {
        let dir = tempfile::tempdir().unwrap();
        let components = dir.path().join("components");
        std::fs::create_dir_all(&components).unwrap();
        // Two runtime manifests and one roster manifest, minimal but parseable.
        for (name, seq) in [("runtime-1.0.0.json", 1u64), ("runtime-1.0.1.json", 7), ("roster-2026.09.06.json", 3)] {
            let component = name.split('-').next().unwrap();
            let version = name.trim_start_matches(component).trim_start_matches('-').trim_end_matches(".json");
            let text = format!(
                "{{\"schema\":\"nsr.component.v1\",\"component\":\"{component}\",\"version\":\"{version}\",\"sequence\":{seq},\"install_root\":\"x\",\"customizable\":false,\"files\":[]}}"
            );
            std::fs::write(components.join(name), text).unwrap();
        }
        assert_eq!(next_sequence(dir.path(), ComponentId::Runtime).unwrap(), 8);
        assert_eq!(next_sequence(dir.path(), ComponentId::Roster).unwrap(), 4);
        assert_eq!(next_sequence(dir.path(), ComponentId::Textures).unwrap(), 1);
        let empty = tempfile::tempdir().unwrap();
        assert_eq!(next_sequence(empty.path(), ComponentId::Runtime).unwrap(), 1);
    }

    #[test]
    fn refuse_emits_every_line_and_keeps_the_first_as_summary() {
        let mut seen: Vec<Event> = Vec::new();
        let outcome: Outcome<()> = refuse(&mut |e| seen.push(e), &["first line".to_string(), "second".to_string()]);
        assert_eq!(seen.len(), 2);
        assert_eq!(seen[0], Event::out("first line"));
        assert_eq!(seen[1].channel, Channel::Out);
        match outcome {
            Outcome::Refused(r) => assert_eq!(r.summary, "first line"),
            _ => panic!("expected Refused"),
        }
    }

    #[test]
    fn out_and_err_macros_route_to_their_channels() {
        let mut seen: Vec<Event> = Vec::new();
        let sink: &mut dyn FnMut(Event) = &mut |e| seen.push(e);
        out!(sink, "hello {}", 1);
        err!(sink, "world");
        assert_eq!(seen, vec![Event::out("hello 1"), Event::err("world")]);
    }
}
```

The manifest JSON shape above must match what `crate::manifest::Component::parse` accepts. Check `crates/reborn-core/src/manifest.rs` for the field names of `Component` (`schema`, `component`, `version`, `sequence`, `install_root`, `customizable`, `files`) and the schema constant; if the constant is not `nsr.component.v1`, use the real one in the test.

Add to `crates/reborn-core/src/lib.rs`, directly after the line `pub mod origin;`:

```rust
#[cfg(feature = "publish")]
pub mod publish;
```

- [ ] **Step 3: Run the tests to see them fail**

```powershell
cargo test -p reborn-core --features publish publish::tests
```
Expected: compile errors, `thousands`, `manifest_path`, `Event` and the macros not found.

- [ ] **Step 4: Write the module**

Prepend to `mod.rs`, above the tests:

```rust
use std::path::{Path, PathBuf};

use crate::inventory::ComponentId;
use crate::manifest::Component;
use crate::{Error, Result};

pub mod keys;
pub mod component;
pub mod release;
pub mod vault;
pub mod roster;
pub mod game_patch;

/// Where component manifests live under the origin.
pub const COMPONENTS_DIR: &str = "components";
/// The everyday key file name.
pub const PUBLISHER_KEY: &str = "publisher.key";
/// The rotation key file name. Generated with the first, then taken offline.
pub const ROTATION_KEY: &str = "rotation.key";

/// Where a human line goes. The CLI prints `Out` to stdout and `Err` to
/// stderr, which is where each line went before the move. An app shows both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    Out,
    Err,
}

/// One human line from a publishing operation, exactly as the CLI prints it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    pub channel: Channel,
    pub line: String,
}

impl Event {
    pub fn out(line: impl Into<String>) -> Self {
        Self { channel: Channel::Out, line: line.into() }
    }
    pub fn err(line: impl Into<String>) -> Self {
        Self { channel: Channel::Err, line: line.into() }
    }
}

/// Where events go. A closure, so a CLI prints and an app forwards.
pub type Sink<'a> = &'a mut dyn FnMut(Event);

/// Why an operation stopped without writing. The lines were already emitted;
/// `summary` is the first of them, for a caller that shows one line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    pub summary: String,
}

/// What an operation came to.
///
/// `Described` is a dry run: everything was checked and described, nothing
/// was written, and that is success. `Refused` is a stop with a reason, which
/// the CLI reports as a failure exit. An error (`Err`) is something broken:
/// an unreadable file, a failed process.
#[derive(Debug)]
pub enum Outcome<T> {
    Done(T),
    Described,
    Refused(Refusal),
}

impl<T> Outcome<T> {
    pub fn is_done(&self) -> bool {
        matches!(self, Outcome::Done(_))
    }
    pub fn refused(&self) -> Option<&Refusal> {
        match self {
            Outcome::Refused(r) => Some(r),
            _ => None,
        }
    }
}

/// Emit every line to `Out` and stop with the first as the summary.
pub fn refuse<T>(sink: Sink<'_>, lines: &[String]) -> Outcome<T> {
    for line in lines {
        sink(Event::out(line.clone()));
    }
    Outcome::Refused(Refusal { summary: lines.first().cloned().unwrap_or_default() })
}

/// `out!(sink, "fmt", args)`: one stdout line. `out!(sink)` is a blank line.
#[macro_export]
macro_rules! out {
    ($sink:expr) => { ($sink)($crate::publish::Event::out(String::new())) };
    ($sink:expr, $($arg:tt)*) => { ($sink)($crate::publish::Event::out(format!($($arg)*))) };
}

/// `err!(sink, "fmt", args)`: one stderr line.
#[macro_export]
macro_rules! err {
    ($sink:expr, $($arg:tt)*) => { ($sink)($crate::publish::Event::err(format!($($arg)*))) };
}

pub use crate::{err, out};

/// Where a component manifest lives under the origin.
pub fn manifest_path(origin: &Path, id: ComponentId, version: &str) -> PathBuf {
    origin.join(COMPONENTS_DIR).join(format!("{}-{}.json", id.as_str(), version))
}

/// The address a release pins a component manifest at, relative to the origin.
pub fn manifest_url(id: ComponentId, version: &str) -> String {
    format!("{COMPONENTS_DIR}/{}-{}.json", id.as_str(), version)
}

/// One above the highest sequence of any manifest of this component in the
/// origin. 1 when there is none.
pub fn next_sequence(origin: &Path, component: ComponentId) -> Result<u64> {
    let dir = origin.join(COMPONENTS_DIR);
    let prefix = format!("{}-", component.as_str());
    let mut highest = 0u64;
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !name.starts_with(&prefix) || !name.ends_with(".json") {
                continue;
            }
            let bytes = std::fs::read(entry.path()).map_err(|e| io_err(&entry.path(), e))?;
            if let Ok(c) = Component::parse(&bytes) {
                highest = highest.max(c.sequence);
            }
        }
    }
    Ok(highest + 1)
}

/// 1234567 as "1,234,567". The CLI has its own copy; this one is for the
/// lines the core writes.
pub fn thousands(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

pub(crate) fn io_err(path: &Path, source: std::io::Error) -> Error {
    Error::Io { path: path.to_path_buf(), source }
}
```

Until Tasks 2 to 6 exist, create each submodule as an empty file so the crate compiles: `keys.rs`, `component.rs`, `release.rs`, `vault.rs`, `roster.rs`, `game_patch.rs`, each containing only `//! Filled in by a later task.`

A note on the macros: `#[macro_export]` puts them at the crate root as `reborn_core::out!` and `reborn_core::err!`; `pub use crate::{err, out};` makes `reborn_core::publish::out!` work too. Inside this crate, write `crate::out!(sink, ...)` or `use crate::{out, err};` at the top of each verb file.

- [ ] **Step 5: Run the tests**

```powershell
cargo test -p reborn-core --features publish publish::tests
```
Expected: 5 passed.

- [ ] **Step 6: Commit**

```powershell
git add crates/reborn-core/src/lib.rs crates/reborn-core/src/publish/mod.rs crates/reborn-core/src/publish/keys.rs crates/reborn-core/src/publish/component.rs crates/reborn-core/src/publish/release.rs crates/reborn-core/src/publish/vault.rs crates/reborn-core/src/publish/roster.rs crates/reborn-core/src/publish/game_patch.rs
git commit -F - <<'EOF'
core: a publish module with an event sink and a typed outcome

The first step of moving publishing out of the CLI: the module, its
Event/Outcome types, and the helpers every verb shares. The verbs follow.

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_0162ntSTXPjrqYbCJ9aBnBmK
EOF
```
(In PowerShell, write the message to a file and use `git commit -F path` if the heredoc form is not available.)

---

### Task 2: Keys

**Files:**
- Create: `crates/reborn-core/src/publish/keys.rs`
- Source to move: `crates/reborn-cli/src/publish.rs` lines 318-369 (`keygen`, `write_key`, `rust_array`, `read_key`)

**Interfaces:**
- Consumes: `Outcome`, `Sink`, `out!`, `refuse`, `io_err`, `PUBLISHER_KEY`, `ROTATION_KEY` from Task 1; `crate::signing::{self, publisher, PUBLIC_KEY_LEN}`.
- Produces:
  - `pub struct KeysWritten { pub publisher_path: PathBuf, pub rotation_path: PathBuf, pub everyday_public: [u8; PUBLIC_KEY_LEN], pub rotation_public: [u8; PUBLIC_KEY_LEN] }`
  - `pub fn keygen(out: &Path, sink: Sink<'_>) -> Result<Outcome<KeysWritten>>`
  - `pub fn read_key(path: &Path) -> Result<Option<[u8; publisher::SECRET_KEY_LEN]>>`
  - `pub fn write_key(path: &Path, secret: &[u8; publisher::SECRET_KEY_LEN]) -> Result<()>`
  - `pub fn rust_array(key: &[u8; PUBLIC_KEY_LEN]) -> String`

- [ ] **Step 1: Write the failing tests** (append to `keys.rs`)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::publish::{Event, Outcome};

    fn collect() -> (Vec<Event>, impl FnMut(Event)) {
        let seen: Vec<Event> = Vec::new();
        (seen, |_e: Event| {})
    }

    #[test]
    fn keygen_writes_both_keys_and_announces_both_public_halves() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("keys");
        let mut seen: Vec<Event> = Vec::new();
        let outcome = keygen(&out, &mut |e| seen.push(e)).unwrap();
        let written = match outcome {
            Outcome::Done(w) => w,
            other => panic!("expected Done, got {other:?}"),
        };
        assert!(written.publisher_path.is_file());
        assert!(written.rotation_path.is_file());
        // The secret on disk decodes and its public half is the announced one.
        let secret = read_key(&written.publisher_path).unwrap().expect("a key");
        assert_eq!(publisher::public_of(&secret), written.everyday_public);
        let text: Vec<String> = seen.iter().map(|e| e.line.clone()).collect();
        let everyday_hex = signing::encode_public_key(&written.everyday_public);
        assert!(text.iter().any(|l| l == &format!("    // everyday   {everyday_hex}")), "{text:?}");
        assert!(text.iter().any(|l| l.starts_with("Wrote ")));
        // The secret never appears in any line.
        let secret_hex = publisher::encode_secret(&secret);
        assert!(!text.iter().any(|l| l.contains(&secret_hex)));
    }

    #[test]
    fn keygen_never_overwrites() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("keys");
        let mut seen: Vec<Event> = Vec::new();
        assert!(keygen(&out, &mut |e| seen.push(e)).unwrap().is_done());
        let before = std::fs::read(out.join(PUBLISHER_KEY)).unwrap();
        let outcome = keygen(&out, &mut |e| seen.push(e)).unwrap();
        assert!(outcome.refused().is_some(), "{outcome:?}");
        assert_eq!(std::fs::read(out.join(PUBLISHER_KEY)).unwrap(), before);
        assert!(seen.iter().any(|e| e.line.contains("already exists")));
    }

    #[test]
    fn read_key_returns_none_for_a_file_that_is_not_a_key() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("junk.key");
        std::fs::write(&p, "not hex\n").unwrap();
        assert!(read_key(&p).unwrap().is_none());
    }

    #[test]
    fn rust_array_formats_thirty_two_bytes() {
        let key = [0u8; PUBLIC_KEY_LEN];
        let s = rust_array(&key);
        assert!(s.starts_with("[0x00, 0x00"));
        assert_eq!(s.matches("0x").count(), PUBLIC_KEY_LEN);
    }
}
```

Delete the unused `collect` helper before committing if it warns; it is shown only to make the shape obvious.

- [ ] **Step 2: Run to verify failure**

```powershell
cargo test -p reborn-core --features publish publish::keys
```
Expected: compile errors, `keygen` not found.

- [ ] **Step 3: Write the implementation**

`keys.rs`, above the tests. This is lines 318-369 of the CLI file with every `println!` turned into `out!(sink, ...)`, the early-return `Ok(PROBLEM)` turned into `refuse`, and the final `Ok(OK)` turned into `Ok(Outcome::Done(KeysWritten { .. }))`:

```rust
//! The two keys, generated together once, and the key file format.

use std::path::{Path, PathBuf};

use crate::publish::{io_err, out, refuse, Outcome, Sink, PUBLISHER_KEY, ROTATION_KEY};
use crate::signing::{self, publisher, PUBLIC_KEY_LEN};
use crate::Result;

/// What keygen wrote. Public halves only; the secrets are on disk and nowhere
/// else.
#[derive(Debug, Clone)]
pub struct KeysWritten {
    pub publisher_path: PathBuf,
    pub rotation_path: PathBuf,
    pub everyday_public: [u8; PUBLIC_KEY_LEN],
    pub rotation_public: [u8; PUBLIC_KEY_LEN],
}

pub fn keygen(out_dir: &Path, sink: Sink<'_>) -> Result<Outcome<KeysWritten>> {
    let publisher_path = out_dir.join(PUBLISHER_KEY);
    let rotation_path = out_dir.join(ROTATION_KEY);
    for p in [&publisher_path, &rotation_path] {
        if p.exists() {
            return Ok(refuse(sink, &[
                format!("{} already exists. This tool never overwrites a key: a key that", p.display()),
                "is replaced by accident is a key that can no longer verify anything".to_string(),
                "already published. Move it aside deliberately if a new one is wanted.".to_string(),
            ]));
        }
    }
    std::fs::create_dir_all(out_dir).map_err(|e| io_err(out_dir, e))?;

    let everyday = publisher::generate()?;
    let rotation = publisher::generate()?;
    write_key(&publisher_path, &everyday.secret)?;
    write_key(&rotation_path, &rotation.secret)?;

    out!(sink, "Wrote {}", publisher_path.display());
    out!(sink, "Wrote {}", rotation_path.display());
    out!(sink);
    out!(sink, "Paste BOTH public keys into TRUSTED_KEYS in crates/reborn-core/src/signing.rs,");
    out!(sink, "everyday first. A launcher built with them will accept releases signed by");
    out!(sink, "either, which is what makes a rotation a launcher release and not a reinstall.");
    out!(sink);
    out!(sink, "    // everyday   {}", signing::encode_public_key(&everyday.public));
    out!(sink, "    {},", rust_array(&everyday.public));
    out!(sink, "    // rotation   {}", signing::encode_public_key(&rotation.public));
    out!(sink, "    {},", rust_array(&rotation.public));
    out!(sink);
    out!(sink, "Then:");
    out!(sink, "  {}  stays on the machine that publishes. Back it up. Never commit it.", PUBLISHER_KEY);
    out!(sink, "  {}   goes OFFLINE now: a USB stick in a drawer, or printed. It is only", ROTATION_KEY);
    out!(sink, "                 ever used if {} is lost or leaks, and a rotation key that", PUBLISHER_KEY);
    out!(sink, "                 sat beside the everyday key leaks in the same breach.");
    Ok(Outcome::Done(KeysWritten {
        publisher_path,
        rotation_path,
        everyday_public: everyday.public,
        rotation_public: rotation.public,
    }))
}

pub fn write_key(path: &Path, secret: &[u8; publisher::SECRET_KEY_LEN]) -> Result<()> {
    let text = format!("{}\n", publisher::encode_secret(secret));
    std::fs::write(path, text).map_err(|e| io_err(path, e))
}

pub fn rust_array(key: &[u8; PUBLIC_KEY_LEN]) -> String {
    let items: Vec<String> = key.iter().map(|b| format!("0x{b:02x}")).collect();
    format!("[{}]", items.join(", "))
}

pub fn read_key(path: &Path) -> Result<Option<[u8; publisher::SECRET_KEY_LEN]>> {
    let text = std::fs::read_to_string(path).map_err(|e| io_err(path, e))?;
    Ok(publisher::decode_secret(&text))
}
```

Check `publisher::generate()` returns a struct with public `secret` and `public` fields (`KeyPair` in `signing.rs` line 175); if the field names differ, use the real ones.

- [ ] **Step 4: Run the tests**

```powershell
cargo test -p reborn-core --features publish publish::keys
```
Expected: 4 passed.

- [ ] **Step 5: Commit**

```powershell
git add crates/reborn-core/src/publish/keys.rs
git commit -m "core publish: keygen and the key file, moved from the CLI" -m "Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>" -m "Claude-Session: https://claude.ai/code/session_0162ntSTXPjrqYbCJ9aBnBmK"
```

---

### Task 3: Component

**Files:**
- Create: `crates/reborn-core/src/publish/component.rs`
- Source to move: CLI `publish.rs` lines 375-560 (`publish_component`, `copy_blob`)

**Interfaces:**
- Consumes: Task 1 helpers; `crate::inventory::{scan_component, ComponentSpec, ScanObserver, ScanOutcome, SerializedComponent}`; `crate::library::hash_file`; `crate::manifest::{ManifestFile, Sha256Hex}`; `crate::origin::blob_path`.
- Produces:
  - `pub struct ComponentRequest<'a> { pub root: &'a Path, pub spec: ComponentSpec, pub origin: &'a Path, pub apply: bool }`
  - `pub struct Published { pub manifest_path: PathBuf, pub sha256: Sha256Hex, pub files: u64, pub bytes: u64, pub new_blobs: u64, pub new_bytes: u64, pub shared_blobs: u64, pub shared_bytes: u64 }`
  - `pub fn publish_component(req: &ComponentRequest<'_>, observer: &dyn ScanObserver, sink: Sink<'_>) -> Result<Outcome<Published>>`
  - `pub(crate) fn copy_blob(src: &Path, dst: &Path, want: &Sha256Hex, want_bytes: u64) -> Result<()>`

The CLI's `misuse` cases (an unknown component name, an `install_root` refusal from `ComponentSpec`) stay in the CLI: the request carries an already-built `ComponentSpec`.

- [ ] **Step 1: Write the failing tests** (append to `component.rs`)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::{ComponentId, ComponentSpec, RunToCompletion};
    use crate::publish::{Event, Outcome};

    fn tree(dir: &Path, files: &[(&str, &[u8])]) {
        for (name, bytes) in files {
            let p = dir.join(name);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, bytes).unwrap();
        }
    }

    fn run(root: &Path, origin: &Path, version: &str, sequence: u64, apply: bool) -> (Outcome<Published>, Vec<String>) {
        let spec = ComponentSpec::new(ComponentId::Runtime, version, sequence).unwrap();
        let req = ComponentRequest { root, spec, origin, apply };
        let mut lines: Vec<String> = Vec::new();
        let outcome = publish_component(&req, &RunToCompletion, &mut |e: Event| lines.push(e.line)).unwrap();
        (outcome, lines)
    }

    #[test]
    fn a_dry_run_describes_and_writes_nothing() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("tree");
        tree(&root, &[("a.txt", b"alpha\n"), ("sub/b.txt", b"beta\n")]);
        let origin = d.path().join("origin");
        let (outcome, lines) = run(&root, &origin, "1.0.0", 1, false);
        assert!(matches!(outcome, Outcome::Described), "{outcome:?}");
        assert!(!origin.exists() || std::fs::read_dir(&origin).unwrap().next().is_none());
        assert!(lines.iter().any(|l| l.contains("2 new")), "{lines:?}");
        assert!(lines.iter().any(|l| l.starts_with("Nothing was written.")));
    }

    #[test]
    fn a_first_publish_writes_every_blob_and_the_manifest() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("tree");
        tree(&root, &[("a.txt", b"alpha\n"), ("sub/b.txt", b"beta\n"), ("c.bin", &[0u8; 1500])]);
        let origin = d.path().join("origin");
        let (outcome, lines) = run(&root, &origin, "1.0.0", 1, true);
        let p = match outcome { Outcome::Done(p) => p, other => panic!("{other:?}") };
        assert_eq!((p.files, p.new_blobs, p.shared_blobs), (3, 3, 0));
        assert!(p.manifest_path.is_file());
        assert_eq!(p.manifest_path, origin.join("components").join("runtime-1.0.0.json"));
        assert!(lines.iter().any(|l| l.contains("3 new")), "{lines:?}");
        assert!(lines.iter().any(|l| l.starts_with("Wrote 3 blob(s) and ")));
        // Every blob is at its content address.
        let manifest = crate::manifest::Component::parse(&std::fs::read(&p.manifest_path).unwrap()).unwrap();
        for f in &manifest.files {
            let blob = origin.join(crate::origin::blob_path(&f.sha256));
            assert_eq!(std::fs::metadata(&blob).unwrap().len(), f.bytes, "{}", f.path);
        }
    }

    #[test]
    fn a_second_version_shares_unchanged_blobs() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("tree");
        tree(&root, &[("a.txt", b"alpha\n"), ("b.txt", b"beta\n")]);
        let origin = d.path().join("origin");
        assert!(run(&root, &origin, "1.0.0", 1, true).0.is_done());
        std::fs::write(root.join("b.txt"), b"beta 2\n").unwrap();
        std::fs::write(root.join("new.txt"), b"new\n").unwrap();
        let (outcome, lines) = run(&root, &origin, "1.0.1", 2, true);
        let p = match outcome { Outcome::Done(p) => p, other => panic!("{other:?}") };
        assert_eq!((p.new_blobs, p.shared_blobs), (2, 1));
        assert!(lines.iter().any(|l| l.contains("2 new")), "{lines:?}");
    }

    #[test]
    fn the_same_bytes_twice_is_not_rewritten_and_different_bytes_are_refused() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("tree");
        tree(&root, &[("a.txt", b"alpha\n")]);
        let origin = d.path().join("origin");
        assert!(run(&root, &origin, "1.0.0", 1, true).0.is_done());
        let (again, lines) = run(&root, &origin, "1.0.0", 1, true);
        assert!(again.is_done());
        assert!(lines.iter().any(|l| l.starts_with("Manifest already published with identical bytes")));
        // Same name, different content: refused, manifest untouched.
        let before = std::fs::read(origin.join("components").join("runtime-1.0.0.json")).unwrap();
        std::fs::write(root.join("a.txt"), b"changed\n").unwrap();
        let (refused, lines) = run(&root, &origin, "1.0.0", 1, true);
        assert!(refused.refused().is_some(), "{refused:?}");
        assert!(lines.iter().any(|l| l.contains("already exists with DIFFERENT contents")), "{lines:?}");
        assert_eq!(std::fs::read(origin.join("components").join("runtime-1.0.0.json")).unwrap(), before);
    }

    #[test]
    fn a_blob_at_the_wrong_length_is_refused() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("tree");
        tree(&root, &[("a.txt", b"alpha\n")]);
        let origin = d.path().join("origin");
        let sha = crate::library::hash_file(&root.join("a.txt")).unwrap();
        let blob = origin.join(crate::origin::blob_path(&sha));
        std::fs::create_dir_all(blob.parent().unwrap()).unwrap();
        std::fs::write(&blob, b"truncated").unwrap();
        let (outcome, lines) = run(&root, &origin, "1.0.0", 1, true);
        assert!(outcome.refused().is_some());
        assert!(lines.iter().any(|l| l.contains("cannot have two lengths")), "{lines:?}");
        assert!(!origin.join("components").join("runtime-1.0.0.json").exists());
    }

    #[test]
    fn copy_blob_proves_the_copy_and_leaves_no_part_file_on_mismatch() {
        let d = tempfile::tempdir().unwrap();
        let src = d.path().join("src.bin");
        std::fs::write(&src, b"hello").unwrap();
        let want = crate::library::hash_file(&src).unwrap();
        let dst = d.path().join("blobs").join("x");
        copy_blob(&src, &dst, &want, 5).unwrap();
        assert_eq!(std::fs::read(&dst).unwrap(), b"hello");
        let wrong = d.path().join("blobs").join("y");
        let err = copy_blob(&src, &wrong, &want, 4).unwrap_err();
        assert!(err.to_string().contains("The tree is not still"), "{err}");
        assert!(!wrong.exists() && !wrong.with_extension("part").exists());
    }
}
```

- [ ] **Step 2: Run to verify failure**

```powershell
cargo test -p reborn-core --features publish publish::component
```
Expected: compile errors, `publish_component` not found.

- [ ] **Step 3: Write the implementation**

Move CLI lines 375-560 into `component.rs` with these exact substitutions:

| In the CLI | In the core |
|---|---|
| `fn publish_component(root, name, version, sequence, install_root, origin, apply) -> Result<ExitCode>` | `pub fn publish_component(req: &ComponentRequest<'_>, observer: &dyn ScanObserver, sink: Sink<'_>) -> Result<Outcome<Published>>`; bind `let (root, origin, apply, spec) = (req.root, req.origin, req.apply, &req.spec);` |
| the `ComponentId::parse` and `ComponentSpec` blocks (lines 384-405) | delete; the request carries the spec |
| `println!()` | `out!(sink)` |
| `println!(...)` | `out!(sink, ...)` |
| `eprintln!(...)` in the copy loop | `err!(sink, ...)` |
| `let log = ScanLog::new(); scan_component(root, &spec, &log)?` | `let started = std::time::Instant::now(); scan_component(root, spec, observer)?` |
| `log.seconds()` | `started.elapsed().as_secs_f64()` |
| a block of `println!` lines followed by `return Ok(PROBLEM);` | `return Ok(refuse(sink, &[ ...each line as a String... ]));` (a blank `println!()` at the start of such a block becomes `String::new()` as the first element, so the output stays the same) |
| `return Ok(OK);` in the `!apply` branch | `return Ok(Outcome::Described);` |
| final `Ok(OK)` | `Ok(Outcome::Done(Published { .. }))` with every field filled from the locals (`files` = `serialized.component().files.len() as u64`, `bytes` = `serialized.component().total_bytes()`, `sha256` = `serialized.sha256().clone()`) |
| `thousands(...)` | `crate::publish::thousands(...)` |
| `io_err` | `crate::publish::io_err` |

The header of the file:

```rust
//! One component tree into the origin: its manifest and its blobs.

use std::path::{Path, PathBuf};

use crate::inventory::{scan_component, ComponentSpec, ScanObserver, ScanOutcome, SerializedComponent};
use crate::library::hash_file;
use crate::manifest::{ManifestFile, Sha256Hex};
use crate::origin::blob_path;
use crate::publish::{err, io_err, manifest_path, out, refuse, thousands, Outcome, Sink};
use crate::{Error, Result};

/// What to publish, from where, into which origin.
#[derive(Debug)]
pub struct ComponentRequest<'a> {
    pub root: &'a Path,
    pub spec: ComponentSpec,
    pub origin: &'a Path,
    /// Write. Without it the tree is hashed and the work is described.
    pub apply: bool,
}

/// What a publish wrote, or would write.
#[derive(Debug, Clone)]
pub struct Published {
    pub manifest_path: PathBuf,
    pub sha256: Sha256Hex,
    pub files: u64,
    pub bytes: u64,
    pub new_blobs: u64,
    pub new_bytes: u64,
    pub shared_blobs: u64,
    pub shared_bytes: u64,
}
```

Two lines need care. The cancelled scan (`ScanOutcome::Cancelled`) becomes `return Ok(refuse(sink, &["Cancelled. Nothing was written.".to_string()]))`. The `Sha256Hex` type may not be `Clone`; if it is not, derive `Clone` on it in `manifest.rs` (a one-line change; `manifest.rs` is not in the other agent's uncommitted set at the time of writing, check `git status` first) or store `sha256: String` via `.to_string()`.

- [ ] **Step 4: Run the tests**

```powershell
cargo test -p reborn-core --features publish publish::component
```
Expected: 6 passed.

- [ ] **Step 5: Commit**

```powershell
git add crates/reborn-core/src/publish/component.rs
git commit -m "core publish: a component tree into the origin, moved from the CLI" -m "Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>" -m "Claude-Session: https://claude.ai/code/session_0162ntSTXPjrqYbCJ9aBnBmK"
```

---

### Task 4: Release and carry-forward

**Files:**
- Create: `crates/reborn-core/src/publish/release.rs`
- Source to move: CLI `publish.rs` lines 566-749 (`release`) and 1054-1127 (`republish_pinning`)

**Interfaces:**
- Consumes: Task 1, Task 2 (`read_key`); `crate::content::TrustedRelease`; `crate::inventory::{ComponentId, SerializedComponent, UnsignedReleaseBuilder}`; `crate::manifest::{Component, Release, Sha256Hex}`; `crate::origin::{blob_path, RELEASE_FILE, SIGNATURE_FILE}`; `crate::signing::{self, publisher, Ed25519Verifier}`.
- Produces:
  - `pub struct Pin { pub id: ComponentId, pub version: String }`
  - `pub struct ReleaseRequest<'a> { pub origin: &'a Path, pub version: &'a str, pub sequence: u64, pub key: &'a Path, pub pins: &'a [Pin], pub apply: bool }`
  - `pub struct Signed { pub release_path: PathBuf, pub sig_path: PathBuf, pub version: String, pub sequence: u64, pub public_hex: String, pub shipped: bool, pub files: u64, pub bytes: u64 }`
  - `pub fn release(req: &ReleaseRequest<'_>, sink: Sink<'_>) -> Result<Outcome<Signed>>`
  - `pub struct CarryForward<'a> { pub origin: &'a Path, pub id: ComponentId, pub version: &'a str, pub key: Option<&'a Path>, pub release_version: Option<&'a str>, pub release_sequence: Option<u64>, pub apply: bool }`
  - `pub fn republish_pinning(req: &CarryForward<'_>, sink: Sink<'_>) -> Result<Outcome<Signed>>`

`republish_pinning` returns `Described` in the two cases where the CLI printed advice and exited 0 without signing: no release to carry forward, and no key.

- [ ] **Step 1: Write the failing tests** (append to `release.rs`)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::{ComponentId, ComponentSpec, RunToCompletion};
    use crate::publish::component::{publish_component, ComponentRequest};
    use crate::publish::keys::keygen;
    use crate::publish::{Event, Outcome};

    struct World {
        _dir: tempfile::TempDir,
        origin: PathBuf,
        key: PathBuf,
        public: [u8; signing::PUBLIC_KEY_LEN],
    }

    fn world() -> World {
        let dir = tempfile::tempdir().unwrap();
        let origin = dir.path().join("origin");
        let keys = dir.path().join("keys");
        let written = match keygen(&keys, &mut |_| {}).unwrap() { Outcome::Done(w) => w, o => panic!("{o:?}") };
        World { _dir: dir, origin, key: written.publisher_path, public: written.everyday_public }
    }

    fn publish(w: &World, id: ComponentId, version: &str, sequence: u64, files: &[(&str, &[u8])]) {
        let root = w._dir.path().join(format!("tree-{}-{version}", id.as_str()));
        for (name, bytes) in files {
            std::fs::create_dir_all(root.join(name).parent().unwrap()).unwrap();
            std::fs::write(root.join(name), bytes).unwrap();
        }
        let spec = ComponentSpec::new(id, version, sequence).unwrap();
        let req = ComponentRequest { root: &root, spec, origin: &w.origin, apply: true };
        assert!(publish_component(&req, &RunToCompletion, &mut |_| {}).unwrap().is_done());
    }

    fn sign(w: &World, version: &str, sequence: u64, pins: &[Pin], apply: bool) -> (Outcome<Signed>, Vec<String>) {
        let mut lines = Vec::new();
        let req = ReleaseRequest { origin: &w.origin, version, sequence, key: &w.key, pins, apply };
        (release(&req, &mut |e: Event| lines.push(e.line)).unwrap(), lines)
    }

    fn pin(id: ComponentId, version: &str) -> Pin {
        Pin { id, version: version.to_string() }
    }

    #[test]
    fn a_release_signs_writes_and_reads_back() {
        let w = world();
        publish(&w, ComponentId::Runtime, "1.0.0", 1, &[("a.txt", b"alpha\n")]);
        let (outcome, lines) = sign(&w, "3.0.0", 1, &[pin(ComponentId::Runtime, "1.0.0")], true);
        let s = match outcome { Outcome::Done(s) => s, o => panic!("{o:?}") };
        assert_eq!(s.sequence, 1);
        assert!(s.release_path.is_file() && s.sig_path.is_file());
        assert!(lines.iter().any(|l| l.starts_with("Read back and verified")), "{lines:?}");
        // The file verifies with the public key alone, the way a launcher does.
        let verifier = Ed25519Verifier::with_keys(&[w.public]).unwrap();
        let sig = signing::decode_signature(&std::fs::read_to_string(&s.sig_path).unwrap()).unwrap();
        let trusted = TrustedRelease::open(&verifier, &std::fs::read(&s.release_path).unwrap(), &sig).unwrap();
        assert_eq!(trusted.release().sequence, 1);
    }

    #[test]
    fn a_dry_run_signs_nothing() {
        let w = world();
        publish(&w, ComponentId::Runtime, "1.0.0", 1, &[("a.txt", b"alpha\n")]);
        let (outcome, lines) = sign(&w, "3.0.0", 1, &[pin(ComponentId::Runtime, "1.0.0")], false);
        assert!(matches!(outcome, Outcome::Described));
        assert!(!w.origin.join(RELEASE_FILE).exists());
        assert!(lines.iter().any(|l| l.starts_with("Would write")));
    }

    #[test]
    fn a_replay_is_refused_with_the_words_the_scripts_look_for() {
        let w = world();
        publish(&w, ComponentId::Runtime, "1.0.0", 1, &[("a.txt", b"alpha\n")]);
        assert!(sign(&w, "3.0.0", 2, &[pin(ComponentId::Runtime, "1.0.0")], true).0.is_done());
        let (outcome, lines) = sign(&w, "3.0.0", 2, &[pin(ComponentId::Runtime, "1.0.0")], true);
        assert!(outcome.refused().is_some());
        assert!(lines.iter().any(|l| l.contains("strictly higher sequence")), "{lines:?}");
    }

    #[test]
    fn a_missing_manifest_and_a_missing_blob_are_refused() {
        let w = world();
        let (outcome, lines) = sign(&w, "3.0.0", 1, &[pin(ComponentId::Runtime, "9.9.9")], true);
        assert!(outcome.refused().is_some());
        assert!(lines.iter().any(|l| l.contains("Publish the component first")), "{lines:?}");
        publish(&w, ComponentId::Runtime, "1.0.0", 1, &[("a.txt", b"alpha\n")]);
        let sha = crate::library::hash_file(&w._dir.path().join("tree-runtime-1.0.0").join("a.txt")).unwrap();
        std::fs::remove_file(w.origin.join(blob_path(&sha))).unwrap();
        let (outcome, lines) = sign(&w, "3.0.0", 1, &[pin(ComponentId::Runtime, "1.0.0")], true);
        assert!(outcome.refused().is_some());
        assert!(lines.iter().any(|l| l.contains("not in the origin at their published")), "{lines:?}");
    }

    #[test]
    fn pinning_nothing_is_refused() {
        let w = world();
        let (outcome, _) = sign(&w, "3.0.0", 1, &[], true);
        assert!(outcome.refused().is_some());
    }

    #[test]
    fn carry_forward_replaces_one_pin_and_keeps_the_others_sorted() {
        let w = world();
        publish(&w, ComponentId::Runtime, "1.0.2", 1, &[("a.txt", b"alpha\n")]);
        publish(&w, ComponentId::Roster, "2026.09.06", 1, &[("roster.rebornroster", b"PK")]);
        assert!(sign(&w, "3.0.0", 3, &[pin(ComponentId::Runtime, "1.0.2")], true).0.is_done());
        let mut lines = Vec::new();
        let req = CarryForward {
            origin: &w.origin, id: ComponentId::Roster, version: "2026.09.06",
            key: Some(&w.key), release_version: None, release_sequence: None, apply: true,
        };
        let outcome = republish_pinning(&req, &mut |e: Event| lines.push(e.line)).unwrap();
        let s = match outcome { Outcome::Done(s) => s, o => panic!("{o:?}") };
        assert_eq!((s.version.as_str(), s.sequence), ("3.0.0", 4));
        assert!(lines.iter().any(|l| l.contains("pinning roster=2026.09.06, runtime=1.0.2")), "{lines:?}");
    }

    #[test]
    fn carry_forward_without_a_key_or_a_release_only_describes() {
        let w = world();
        publish(&w, ComponentId::Roster, "2026.09.06", 1, &[("roster.rebornroster", b"PK")]);
        let mut lines = Vec::new();
        let no_release = CarryForward {
            origin: &w.origin, id: ComponentId::Roster, version: "2026.09.06",
            key: Some(&w.key), release_version: None, release_sequence: None, apply: true,
        };
        assert!(matches!(republish_pinning(&no_release, &mut |e: Event| lines.push(e.line)).unwrap(), Outcome::Described));
        assert!(lines.iter().any(|l| l.contains("serves no release yet")), "{lines:?}");
        assert!(sign(&w, "3.0.0", 1, &[pin(ComponentId::Roster, "2026.09.06")], true).0.is_done());
        let no_key = CarryForward { key: None, ..no_release };
        lines.clear();
        assert!(matches!(republish_pinning(&no_key, &mut |e: Event| lines.push(e.line)).unwrap(), Outcome::Described));
        assert!(lines.iter().any(|l| l.contains("No --key")), "{lines:?}");
    }
}
```

- [ ] **Step 2: Run to verify failure**

```powershell
cargo test -p reborn-core --features publish publish::release
```
Expected: compile errors.

- [ ] **Step 3: Write the implementation**

Move CLI lines 566-749 and 1054-1127 into `release.rs` with the same substitution table as Task 3, plus:

| In the CLI | In the core |
|---|---|
| `fn release(origin, version, sequence, key, components: &[String], apply)` | `pub fn release(req: &ReleaseRequest<'_>, sink: Sink<'_>) -> Result<Outcome<Signed>>` |
| `if components.is_empty() { eprintln!(..); return Ok(misuse()); }` | `if req.pins.is_empty() { return Ok(refuse(sink, &["A release pinning nothing is refused: it is useless to a client.".to_string()])); }` |
| `for spec in components { let Some((name, ver)) = spec.split_once('=') ...; let Some(id) = ComponentId::parse(name) ...` | `for pin in req.pins { let (id, name, ver) = (pin.id, pin.id.as_str(), pin.version.as_str());` and delete the two parse-failure branches (the CLI validates) |
| `let Some(secret) = read_key(key)? else { println!(..); return Ok(PROBLEM) }` | `let Some(secret) = crate::publish::keys::read_key(req.key)? else { return Ok(refuse(sink, &[format!("{} does not hold a key: expected 64 hex characters.", req.key.display())])); };` |
| `components.len()` in the Total line | `req.pins.len()` |
| final `Ok(OK)` | `Ok(Outcome::Done(Signed { release_path, sig_path, version: trusted.release().version.clone(), sequence: trusted.release().sequence, public_hex, shipped, files: total_files, bytes: total_bytes }))` |
| `fn republish_pinning(origin, name: &str, version, key, release_version, release_sequence, apply)` | `pub fn republish_pinning(req: &CarryForward<'_>, sink: Sink<'_>) -> Result<Outcome<Signed>>`; inside, `let name = req.id.as_str();` |
| `pins: Vec<String>` built as `"{name}={version}"` then `release(origin, &rv, rs, key, &pins, apply)` | keep the `Vec<String>` for the `pinning {}` line (its text is asserted), and build `Vec<Pin>` beside it: `Pin { id: ComponentId::parse(pinned_name).expect("a served pin names a component"), version: component.version.clone() }` for carried pins and `Pin { id: req.id, version: req.version.to_string() }` for the new one; sort both by the string form |
| the two `return Ok(OK)` advice branches ("serves no release yet", "No --key") | `return Ok(Outcome::Described);` after emitting the same lines |

The pinned name in a served release is a component name string; `ComponentId::parse` returns `None` only for a name this build does not know. Rather than `expect`, refuse: `let Some(id) = ComponentId::parse(pinned_name) else { return Ok(refuse(sink, &[format!("The served release pins {pinned_name:?}, which this build does not know. Nothing was signed.")])); };`

Header:

```rust
//! Assemble release.json from components already in the origin, sign it, and
//! prove the signed file verifies before saying so.

use std::path::{Path, PathBuf};

use crate::content::TrustedRelease;
use crate::inventory::{ComponentId, SerializedComponent, UnsignedReleaseBuilder};
use crate::manifest::{Component, Release, Sha256Hex};
use crate::origin::{blob_path, RELEASE_FILE, SIGNATURE_FILE};
use crate::publish::keys::read_key;
use crate::publish::{io_err, manifest_path, manifest_url, out, refuse, thousands, Outcome, Sink};
use crate::signing::{self, publisher, Ed25519Verifier};
use crate::{Error, Result};

/// One component at one version, to pin in a release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pin {
    pub id: ComponentId,
    pub version: String,
}

#[derive(Debug)]
pub struct ReleaseRequest<'a> {
    pub origin: &'a Path,
    /// The label players see, for example 3.0.0.
    pub version: &'a str,
    /// Strictly higher than the release already in the origin.
    pub sequence: u64,
    /// The publisher.key file.
    pub key: &'a Path,
    pub pins: &'a [Pin],
    /// Write and sign. Without it everything is checked and nothing written.
    pub apply: bool,
}

/// A signed release, read back and verified.
#[derive(Debug, Clone)]
pub struct Signed {
    pub release_path: PathBuf,
    pub sig_path: PathBuf,
    pub version: String,
    pub sequence: u64,
    pub public_hex: String,
    /// Whether the signing key's public half is compiled into shipped launchers.
    pub shipped: bool,
    pub files: u64,
    pub bytes: u64,
}

/// Re-sign the release with one pin replaced or added and every other pin
/// carried forward at the version it is served at.
#[derive(Debug)]
pub struct CarryForward<'a> {
    pub origin: &'a Path,
    pub id: ComponentId,
    pub version: &'a str,
    /// Without a key the release step is only described.
    pub key: Option<&'a Path>,
    /// Defaults to the label already served.
    pub release_version: Option<&'a str>,
    /// Defaults to one above the release already served.
    pub release_sequence: Option<u64>,
    pub apply: bool,
}
```

- [ ] **Step 4: Run the tests**

```powershell
cargo test -p reborn-core --features publish publish::release
```
Expected: 7 passed.

- [ ] **Step 5: Commit**

```powershell
git add crates/reborn-core/src/publish/release.rs
git commit -m "core publish: signing a release and carrying pins forward, moved from the CLI" -m "Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>" -m "Claude-Session: https://claude.ai/code/session_0162ntSTXPjrqYbCJ9aBnBmK"
```

---

### Task 5: The vault and the roster push

**Files:**
- Create: `crates/reborn-core/src/publish/vault.rs`
- Create: `crates/reborn-core/src/publish/roster.rs`
- Source to move: CLI `publish.rs` line 211 (`SITE_TOKEN_ACCOUNT`), lines 284-312 (the vault calls inside `site_token`), lines 758-852 (`publish_roster`)

**Interfaces:**
- Consumes: Tasks 1, 3, 4; `crate::net::Http`; `crate::roster::ROSTER_FILE`; `crate::Platform`.
- Produces (`vault.rs`):
  - `pub const SITE_TOKEN_ACCOUNT: &str = "nsr.site.export";`
  - `pub fn site_token(platform: &Platform) -> Result<Option<String>>`
  - `pub fn store_site_token(platform: &Platform, token: &str) -> Result<()>`
  - `pub fn forget_site_token(platform: &Platform) -> Result<()>`
- Produces (`roster.rs`):
  - `pub struct RosterRequest<'a> { pub from: &'a str, pub origin: &'a Path, pub version: &'a str, pub sequence: Option<u64>, pub key: Option<&'a Path>, pub release_version: Option<&'a str>, pub release_sequence: Option<u64>, pub apply: bool, pub token: Option<&'a str> }`
  - `pub struct Push { pub component: Published, pub release: Option<Signed> }`
  - `pub fn publish_roster(http: &dyn Http, req: &RosterRequest<'_>, observer: &dyn ScanObserver, sink: Sink<'_>) -> Result<Outcome<Push>>`

Token resolution (a file, else the vault) stays in the CLI; the core takes the token as a string, so the vault is never read by a function that also fetches. `Push.release` is `None` when the release step was only described (no key).

- [ ] **Step 1: Write the failing tests** (append to `roster.rs`)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::RunToCompletion;
    use crate::publish::keys::keygen;
    use crate::publish::{Event, Outcome};
    use std::io::Write;

    /// A site that serves one file at one URL, and remembers the bearer it saw.
    #[derive(Debug)]
    struct FakeSite {
        url: String,
        body: Vec<u8>,
        seen_bearer: std::sync::Mutex<Option<Option<String>>>,
    }

    impl Http for FakeSite {
        fn get_to_file_as(&self, url: &str, to: &Path, bearer: Option<&str>) -> Result<u64> {
            *self.seen_bearer.lock().unwrap() = Some(bearer.map(str::to_string));
            if url != self.url {
                return Err(Error::PlainIo(std::io::Error::new(std::io::ErrorKind::NotFound, format!("{url} answered 404"))));
            }
            std::fs::File::create(to)?.write_all(&self.body)?;
            Ok(self.body.len() as u64)
        }
    }

    fn roster_zip() -> Vec<u8> {
        // A zip with the three entry names in the clear, which is all the
        // publisher checks. Local file headers only; the Builder never reads it.
        let mut z = Vec::new();
        for name in ["manifest.json", "players.json", "teams.json"] {
            z.extend_from_slice(b"PK\x03\x04");
            z.extend_from_slice(&[0u8; 22]);
            z.extend_from_slice(&(name.len() as u16).to_le_bytes());
            z.extend_from_slice(&0u16.to_le_bytes());
            z.extend_from_slice(name.as_bytes());
        }
        z
    }

    fn origin_with_runtime_and_keys() -> (tempfile::TempDir, PathBuf, PathBuf) {
        use crate::inventory::{ComponentId, ComponentSpec};
        use crate::publish::component::{publish_component, ComponentRequest};
        use crate::publish::release::{release, Pin, ReleaseRequest};
        let dir = tempfile::tempdir().unwrap();
        let origin = dir.path().join("origin");
        let written = match keygen(&dir.path().join("keys"), &mut |_| {}).unwrap() { Outcome::Done(w) => w, o => panic!("{o:?}") };
        let tree = dir.path().join("runtime");
        std::fs::create_dir_all(&tree).unwrap();
        std::fs::write(tree.join("a.txt"), b"alpha\n").unwrap();
        let spec = ComponentSpec::new(ComponentId::Runtime, "1.0.2", 1).unwrap();
        assert!(publish_component(&ComponentRequest { root: &tree, spec, origin: &origin, apply: true }, &RunToCompletion, &mut |_| {}).unwrap().is_done());
        let pins = [Pin { id: ComponentId::Runtime, version: "1.0.2".into() }];
        assert!(release(&ReleaseRequest { origin: &origin, version: "3.0.0", sequence: 3, key: &written.publisher_path, pins: &pins, apply: true }, &mut |_| {}).unwrap().is_done());
        (dir, origin, written.publisher_path)
    }

    #[test]
    fn a_roster_from_a_url_is_fetched_published_and_pinned_beside_the_runtime() {
        let (_dir, origin, key) = origin_with_runtime_and_keys();
        let site = FakeSite { url: "https://site.test/export".into(), body: roster_zip(), seen_bearer: Default::default() };
        let mut lines = Vec::new();
        let req = RosterRequest {
            from: "https://site.test/export", origin: &origin, version: "2026.09.06", sequence: None,
            key: Some(&key), release_version: None, release_sequence: None, apply: true, token: Some("tok"),
        };
        let outcome = publish_roster(&site, &req, &RunToCompletion, &mut |e: Event| lines.push(e.line)).unwrap();
        let push = match outcome { Outcome::Done(p) => p, o => panic!("{o:?}") };
        assert_eq!(push.release.as_ref().map(|s| s.sequence), Some(4));
        assert_eq!(*site.seen_bearer.lock().unwrap(), Some(Some("tok".to_string())));
        assert!(lines.iter().any(|l| l.starts_with("Fetched")), "{lines:?}");
        assert!(lines.iter().any(|l| l == "Sequence      1 (one above the highest roster already in the origin)"), "{lines:?}");
        assert!(lines.iter().any(|l| l.contains("pinning roster=2026.09.06, runtime=1.0.2")), "{lines:?}");
        assert!(origin.join("components").join("roster-2026.09.06.json").is_file());
    }

    #[test]
    fn junk_is_refused_before_anything_is_published() {
        let (_dir, origin, key) = origin_with_runtime_and_keys();
        let site = FakeSite { url: "https://site.test/export".into(), body: b"<html>404</html>".to_vec(), seen_bearer: Default::default() };
        let mut lines = Vec::new();
        let req = RosterRequest {
            from: "https://site.test/export", origin: &origin, version: "2026.09.07", sequence: None,
            key: Some(&key), release_version: None, release_sequence: None, apply: true, token: None,
        };
        let outcome = publish_roster(&site, &req, &RunToCompletion, &mut |e: Event| lines.push(e.line)).unwrap();
        assert!(outcome.refused().is_some());
        assert!(lines.iter().any(|l| l.contains("not a .rebornroster")), "{lines:?}");
        assert!(!origin.join("components").join("roster-2026.09.07.json").exists());
    }

    #[test]
    fn a_local_file_needs_no_site_and_no_key_leaves_the_release_described() {
        let (dir, origin, _key) = origin_with_runtime_and_keys();
        let file = dir.path().join("local.rebornroster");
        std::fs::write(&file, roster_zip()).unwrap();
        let site = FakeSite { url: String::new(), body: Vec::new(), seen_bearer: Default::default() };
        let mut lines = Vec::new();
        let from = file.to_string_lossy().into_owned();
        let req = RosterRequest {
            from: &from, origin: &origin, version: "2026.09.06", sequence: Some(5),
            key: None, release_version: None, release_sequence: None, apply: true, token: None,
        };
        let outcome = publish_roster(&site, &req, &RunToCompletion, &mut |e: Event| lines.push(e.line)).unwrap();
        let push = match outcome { Outcome::Done(p) => p, o => panic!("{o:?}") };
        assert!(push.release.is_none());
        assert!(site.seen_bearer.lock().unwrap().is_none(), "no fetch for a local file");
        assert!(lines.iter().any(|l| l.contains("No --key")), "{lines:?}");
    }

    #[test]
    fn a_401_is_explained_and_refused() {
        #[derive(Debug)]
        struct Locked;
        impl Http for Locked {
            fn get_to_file_as(&self, url: &str, _to: &Path, _bearer: Option<&str>) -> Result<u64> {
                Err(Error::PlainIo(std::io::Error::new(std::io::ErrorKind::PermissionDenied, format!("{url} answered 401"))))
            }
        }
        let (_dir, origin, key) = origin_with_runtime_and_keys();
        let mut lines = Vec::new();
        let req = RosterRequest {
            from: "https://site.test/export", origin: &origin, version: "2026.09.06", sequence: None,
            key: Some(&key), release_version: None, release_sequence: None, apply: true, token: None,
        };
        let outcome = publish_roster(&Locked, &req, &RunToCompletion, &mut |e: Event| lines.push(e.line)).unwrap();
        assert!(outcome.refused().is_some());
        assert!(lines.iter().any(|l| l.contains("answered 401")), "{lines:?}");
    }
}
```

`Http` may have more required methods than `get_to_file_as` (see `crates/reborn-core/src/net.rs` line 35); implement each required one on the fakes with `unimplemented!()`. If `get_to_file_as` is a provided method built on a required `get`, implement `get` instead so the provided method works.

- [ ] **Step 2: Run to verify failure**

```powershell
cargo test -p reborn-core --features publish publish::roster
```
Expected: compile errors.

- [ ] **Step 3: Write `vault.rs`**

```rust
//! The secrets the publisher needs, kept in the platform credential vault so
//! none of them ever sits on a command line or in a file.

use crate::{Platform, Result};

/// The vault account the site's API token is kept under.
pub const SITE_TOKEN_ACCOUNT: &str = "nsr.site.export";

pub fn site_token(platform: &Platform) -> Result<Option<String>> {
    platform.credentials.retrieve(SITE_TOKEN_ACCOUNT)
}

pub fn store_site_token(platform: &Platform, token: &str) -> Result<()> {
    platform.credentials.store(SITE_TOKEN_ACCOUNT, token)
}

pub fn forget_site_token(platform: &Platform) -> Result<()> {
    platform.credentials.delete(SITE_TOKEN_ACCOUNT)
}
```

- [ ] **Step 4: Write `roster.rs`**

Move CLI lines 758-852 with the substitution tables of Tasks 3 and 4, plus:

| In the CLI | In the core |
|---|---|
| the token block (lines 784-793) | delete; `let token = req.token;` |
| `let http = UreqHttp::new(..)` | delete; use the `http` parameter |
| `publish_component(&stage, "roster", version, sequence, None, origin, apply)?` and `if published != OK { return Ok(published) }` | build `ComponentSpec::new(ComponentId::Roster, req.version, sequence)` (map its `Err` to a refusal with the error's text), then `match publish_component(&ComponentRequest { root: &stage, spec, origin: req.origin, apply: req.apply }, observer, sink)? { Outcome::Refused(r) => return Ok(Outcome::Refused(r)), Outcome::Described => { return Ok(match republish_pinning(&carry, sink)? { Outcome::Refused(r) => Outcome::Refused(r), _ => Outcome::Described }); } Outcome::Done(component) => component }` |
| `republish_pinning(origin, "roster", version, key, release_version, release_sequence, apply)` | `let carry = CarryForward { origin: req.origin, id: ComponentId::Roster, version: req.version, key: req.key, release_version: req.release_version, release_sequence: req.release_sequence, apply: req.apply };` then `Ok(match republish_pinning(&carry, sink)? { Outcome::Done(s) => Outcome::Done(Push { component, release: Some(s) }), Outcome::Described => Outcome::Done(Push { component, release: None }), Outcome::Refused(r) => Outcome::Refused(r) })` |
| `next_sequence(origin, "roster")` | `crate::publish::next_sequence(req.origin, ComponentId::Roster)` |

Header:

```rust
//! Fetch the roster the site published and push it: the component, then a
//! release that carries every other pinned component forward.

use std::path::{Path, PathBuf};

use crate::inventory::{ComponentId, ComponentSpec, ScanObserver};
use crate::net::Http;
use crate::publish::component::{publish_component, ComponentRequest, Published};
use crate::publish::release::{republish_pinning, CarryForward, Signed};
use crate::publish::{io_err, next_sequence, out, refuse, thousands, Outcome, Sink};
use crate::roster::ROSTER_FILE;
use crate::{Error, Result};

#[derive(Debug)]
pub struct RosterRequest<'a> {
    /// A URL, or a local .rebornroster path.
    pub from: &'a str,
    pub origin: &'a Path,
    /// The roster's label, for example 2026.09.06.
    pub version: &'a str,
    /// Defaults to one above the highest roster sequence in the origin.
    pub sequence: Option<u64>,
    /// Without it the component is published and the release only described.
    pub key: Option<&'a Path>,
    pub release_version: Option<&'a str>,
    pub release_sequence: Option<u64>,
    pub apply: bool,
    /// The site's bearer token, for a login-only export. Resolved by the caller.
    pub token: Option<&'a str>,
}

/// A component pushed, and the release that pins it when one was signed.
#[derive(Debug, Clone)]
pub struct Push {
    pub component: Published,
    pub release: Option<Signed>,
}
```

Note the `Described` branch after `publish_component`: the CLI ran `republish_pinning` on a dry run too, so the release step is described as well. Keep that.

- [ ] **Step 5: Run the tests**

```powershell
cargo test -p reborn-core --features publish publish::roster
```
Expected: 4 passed.

- [ ] **Step 6: Commit**

```powershell
git add crates/reborn-core/src/publish/vault.rs crates/reborn-core/src/publish/roster.rs
git commit -m "core publish: the roster push and the vault accounts, moved from the CLI" -m "Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>" -m "Claude-Session: https://claude.ai/code/session_0162ntSTXPjrqYbCJ9aBnBmK"
```

---

### Task 6: The game patch

**Files:**
- Create: `crates/reborn-core/src/publish/game_patch.rs`
- Source to move: CLI `publish.rs` lines 858-1049 (`publish_game_patch`, `find_xdelta_for_publishing`)

**Interfaces:**
- Consumes: Tasks 1, 3, 4, 5 (`Push`); `crate::game_patch::{Chain, Step, Patcher, Xdelta3, CHAIN_FILE, BUNDLE_DIR, find_xdelta}`; `crate::library::hash_file`; `crate::Platform`.
- Produces:
  - `pub struct GamePatchRequest<'a> { pub from: &'a Path, pub to: &'a Path, pub version: &'a str, pub tree: Option<&'a Path>, pub xdelta: Option<&'a Path>, pub origin: &'a Path, pub sequence: Option<u64>, pub key: Option<&'a Path>, pub release_version: Option<&'a str>, pub release_sequence: Option<u64>, pub apply: bool }`
  - `pub fn publish_game_patch(platform: &Platform, req: &GamePatchRequest<'_>, observer: &dyn ScanObserver, sink: Sink<'_>) -> Result<Outcome<Push>>`
  - `pub fn find_xdelta_for_publishing(platform: &Platform, given: Option<&Path>) -> Option<PathBuf>`

The encode and decode steps need a real `xdelta3.exe`, so they are covered by `Tools/e2e-game-patch.py` in Task 7. The unit tests here cover every refusal that happens before xdelta runs.

- [ ] **Step 1: Write the failing tests** (append to `game_patch.rs`)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::RunToCompletion;
    use crate::publish::{Event, Outcome};

    fn req<'a>(from: &'a Path, to: &'a Path, origin: &'a Path, tree: &'a Path, xdelta: &'a Path) -> GamePatchRequest<'a> {
        GamePatchRequest {
            from, to, version: "2026.09.07", tree: Some(tree), xdelta: Some(xdelta), origin,
            sequence: None, key: None, release_version: None, release_sequence: None, apply: true,
        }
    }

    fn run(r: &GamePatchRequest<'_>) -> (Outcome<Push>, Vec<String>) {
        let platform = crate::Platform::detect();
        let mut lines = Vec::new();
        (publish_game_patch(&platform, r, &RunToCompletion, &mut |e: Event| lines.push(e.line)).unwrap(), lines)
    }

    #[test]
    fn a_missing_image_is_refused_before_anything_else() {
        let d = tempfile::tempdir().unwrap();
        let to = d.path().join("b.iso");
        std::fs::write(&to, b"B").unwrap();
        let fake_xdelta = d.path().join("xdelta3.exe");
        std::fs::write(&fake_xdelta, b"").unwrap();
        let r = req(&d.path().join("missing.iso"), &to, &d.path().join("origin"), &d.path().join("stage"), &fake_xdelta);
        let (outcome, lines) = run(&r);
        assert!(outcome.refused().is_some());
        assert!(lines.iter().any(|l| l.contains("--from") && l.contains("is not a file")), "{lines:?}");
    }

    #[test]
    fn the_same_build_twice_is_refused() {
        let d = tempfile::tempdir().unwrap();
        let a = d.path().join("a.iso");
        std::fs::write(&a, b"same").unwrap();
        let fake_xdelta = d.path().join("xdelta3.exe");
        std::fs::write(&fake_xdelta, b"").unwrap();
        let r = req(&a, &a, &d.path().join("origin"), &d.path().join("stage"), &fake_xdelta);
        let (outcome, lines) = run(&r);
        assert!(outcome.refused().is_some());
        assert!(lines.iter().any(|l| l.contains("Those are the same build")), "{lines:?}");
    }

    #[test]
    fn a_from_that_is_not_the_chain_head_is_refused() {
        let d = tempfile::tempdir().unwrap();
        let a = d.path().join("a.iso");
        let b = d.path().join("b.iso");
        std::fs::write(&a, b"A").unwrap();
        std::fs::write(&b, b"B").unwrap();
        let stage = d.path().join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        // A chain whose head is some other build.
        let mut chain = Chain::new();
        chain.push(Step {
            from_sha256: "0".repeat(64), to_sha256: "1".repeat(64), to_bytes: 1,
            patch: Chain::patch_name(&"0".repeat(64), &"1".repeat(64)), label: "old".into(),
        }).unwrap();
        chain.save(&stage.join(CHAIN_FILE)).unwrap();
        let fake_xdelta = d.path().join("xdelta3.exe");
        std::fs::write(&fake_xdelta, b"").unwrap();
        let r = req(&a, &b, &d.path().join("origin"), &stage, &fake_xdelta);
        let (outcome, lines) = run(&r);
        assert!(outcome.refused().is_some());
        assert!(lines.iter().any(|l| l.contains("A game patch is always made from the")), "{lines:?}");
    }

    #[test]
    fn a_dry_run_describes_the_step_and_encodes_nothing() {
        let d = tempfile::tempdir().unwrap();
        let a = d.path().join("a.iso");
        let b = d.path().join("b.iso");
        std::fs::write(&a, b"A").unwrap();
        std::fs::write(&b, b"B").unwrap();
        let fake_xdelta = d.path().join("xdelta3.exe");
        std::fs::write(&fake_xdelta, b"").unwrap();
        let stage = d.path().join("stage");
        let mut r = req(&a, &b, &d.path().join("origin"), &stage, &fake_xdelta);
        r.apply = false;
        let (outcome, lines) = run(&r);
        assert!(matches!(outcome, Outcome::Described), "{outcome:?}");
        assert!(lines.iter().any(|l| l.starts_with("Step          ")), "{lines:?}");
        assert!(!stage.join(CHAIN_FILE).exists());
    }

    #[test]
    fn xdelta_is_found_where_given_and_only_where_a_file_is() {
        let d = tempfile::tempdir().unwrap();
        let platform = crate::Platform::detect();
        let there = d.path().join("xdelta3.exe");
        std::fs::write(&there, b"").unwrap();
        assert_eq!(find_xdelta_for_publishing(&platform, Some(&there)), Some(there.clone()));
        assert_eq!(find_xdelta_for_publishing(&platform, Some(&d.path().join("nope.exe"))), None);
    }
}
```

`Step` and `Chain::push` come from `crate::game_patch` (lines 55, 69, 206 of `game_patch.rs`); if `push` validates that `from_sha256` matches the current head, the test's single step on an empty chain is fine.

- [ ] **Step 2: Run to verify failure**

```powershell
cargo test -p reborn-core --features publish publish::game_patch
```
Expected: compile errors.

- [ ] **Step 3: Write the implementation**

Move CLI lines 858-1049 with the substitution tables of Tasks 3 to 5, plus:

| In the CLI | In the core |
|---|---|
| `let platform = reborn_core::Platform::detect();` | delete; use the `platform` parameter |
| `publish_component(&tree, "game-patch", version, sequence, None, origin, true)?` then `if published != OK ...` | `ComponentSpec::new(ComponentId::GamePatch, req.version, sequence)` and the same `match` as in Task 5 (with `apply: true`), so `Refused` propagates and `Done(component)` continues |
| `republish_pinning(origin, "game-patch", ...)` | a `CarryForward { id: ComponentId::GamePatch, apply: true, .. }` and the same result mapping as Task 5 into `Push` |
| `next_sequence(origin, "game-patch")` | `next_sequence(req.origin, ComponentId::GamePatch)` |
| `!apply` early return `Ok(OK)` | `Ok(Outcome::Described)` |
| `reborn_core::` paths | `crate::` paths |

Header:

```rust
//! A game update: the xdelta from the build players have to the new build,
//! appended to the chain, then a release carrying every other pin forward.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::game_patch::{find_xdelta, Chain, Patcher, Step, Xdelta3, BUNDLE_DIR, CHAIN_FILE};
use crate::inventory::{ComponentId, ComponentSpec, ScanObserver};
use crate::library::hash_file;
use crate::publish::component::{publish_component, ComponentRequest};
use crate::publish::release::{republish_pinning, CarryForward};
use crate::publish::roster::Push;
use crate::publish::{io_err, next_sequence, out, refuse, thousands, Outcome, Sink};
use crate::{Platform, Result};

#[derive(Debug)]
pub struct GamePatchRequest<'a> {
    /// The pristine image players have now: the head of the chain.
    pub from: &'a Path,
    /// The new pristine build.
    pub to: &'a Path,
    /// The build's label, for example 2026.09.07.
    pub version: &'a str,
    /// The tree the chain accumulates in. Defaults to `<origin>-stage\game-patch`.
    pub tree: Option<&'a Path>,
    /// xdelta3. Found in the import bundle when absent.
    pub xdelta: Option<&'a Path>,
    pub origin: &'a Path,
    pub sequence: Option<u64>,
    pub key: Option<&'a Path>,
    pub release_version: Option<&'a str>,
    pub release_sequence: Option<u64>,
    /// Encode, verify, write and sign. Without it both images are hashed and
    /// the work is described; no delta is made.
    pub apply: bool,
}
```

- [ ] **Step 4: Run the tests**

```powershell
cargo test -p reborn-core --features publish publish::game_patch
```
Expected: 5 passed.

- [ ] **Step 5: Commit**

```powershell
git add crates/reborn-core/src/publish/game_patch.rs
git commit -m "core publish: the game patch chain step, moved from the CLI" -m "Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>" -m "Claude-Session: https://claude.ai/code/session_0162ntSTXPjrqYbCJ9aBnBmK"
```

---

### Task 7: Rewire the CLI and prove nothing changed

**Files:**
- Rewrite: `crates/reborn-cli/src/publish.rs` (keep lines 1-208, the doc comment and the clap enum; replace everything from line 210 on)
- Modify: `crates/reborn-cli/src/main.rs` only if an import of `ScanLog` or `thousands` becomes unused (remove the warning, nothing else)

**Interfaces:**
- Consumes: every `pub` item of Tasks 1 to 6, by these paths: `reborn_core::publish::{Event, Channel, Outcome}`, `reborn_core::publish::keys::keygen`, `reborn_core::publish::component::{publish_component, ComponentRequest}`, `reborn_core::publish::release::{release, Pin, ReleaseRequest}`, `reborn_core::publish::roster::{publish_roster, RosterRequest}`, `reborn_core::publish::game_patch::{publish_game_patch, GamePatchRequest}`, `reborn_core::publish::vault::{site_token, store_site_token, forget_site_token, SITE_TOKEN_ACCOUNT}`.
- Produces: the same `pub fn run(cmd: PublishCommand) -> reborn_core::Result<std::process::ExitCode>`.

- [ ] **Step 1: Build the CLI as it is, and record the baseline**

```powershell
Set-Location "C:\Users\Adam\Desktop\Reborn Installer\NFL Street Reborn Launcher\target\ship-tree"
cargo build -p reborn-cli
py -3 Tools\e2e-update.py
```
Expected: `PASS: 13 steps`. This is the baseline the rewire must keep.

- [ ] **Step 2: Replace the body of `publish.rs`**

Everything after the clap enum (from `/// The vault account` at line 210 to the end) becomes:

```rust
use reborn_core::inventory::{ComponentId, ComponentSpec, ScanObserver};
use reborn_core::publish::component::{publish_component, ComponentRequest};
use reborn_core::publish::game_patch::{publish_game_patch, GamePatchRequest};
use reborn_core::publish::keys::keygen;
use reborn_core::publish::release::{release, Pin, ReleaseRequest};
use reborn_core::publish::roster::{publish_roster, RosterRequest};
use reborn_core::publish::vault::{forget_site_token, site_token, store_site_token, SITE_TOKEN_ACCOUNT};
use reborn_core::publish::{Channel, Event, Outcome};

use super::{misuse, ScanLog, OK, PROBLEM};

/// Print an event where it went before the move: `Out` to stdout, `Err` to
/// stderr.
fn print(event: Event) {
    match event.channel {
        Channel::Out => println!("{}", event.line),
        Channel::Err => eprintln!("{}", event.line),
    }
}

/// The exit code an outcome earns. A dry run is a success.
fn exit_of<T>(outcome: &Outcome<T>) -> std::process::ExitCode {
    match outcome {
        Outcome::Done(_) | Outcome::Described => OK,
        Outcome::Refused(_) => PROBLEM,
    }
}

pub fn run(cmd: PublishCommand) -> reborn_core::Result<std::process::ExitCode> {
    let mut sink = print;
    match cmd {
        PublishCommand::Keygen { out } => Ok(exit_of(&keygen(&out, &mut sink)?)),
        PublishCommand::Component {
            root,
            component,
            version,
            sequence,
            install_root,
            origin,
            apply,
        } => {
            let Some(id) = ComponentId::parse(&component) else {
                eprintln!("--component {component:?} is not a component name. Use one of:");
                for known in ComponentId::ALL {
                    eprintln!("  {}", known.as_str());
                }
                return Ok(misuse());
            };
            let spec = match install_root.as_deref() {
                Some(given) => ComponentSpec::with_install_root(id, &version, sequence, given),
                None => ComponentSpec::new(id, &version, sequence),
            };
            let spec = match spec {
                Ok(s) => s,
                Err(e) => {
                    println!("{e}");
                    if install_root.is_none() {
                        println!();
                        println!("Pass --install-root <relative path> to state it.");
                    }
                    return Ok(PROBLEM);
                }
            };
            let req = ComponentRequest { root: &root, spec, origin: &origin, apply };
            let log = ScanLog::new();
            Ok(exit_of(&publish_component(&req, &log as &dyn ScanObserver, &mut sink)?))
        }
        PublishCommand::Release {
            origin,
            version,
            sequence,
            key,
            components,
            apply,
        } => {
            if components.is_empty() {
                eprintln!("--component NAME=VERSION is required at least once. A release pinning");
                eprintln!("nothing is refused by the builder and useless to a client.");
                return Ok(misuse());
            }
            let mut pins = Vec::with_capacity(components.len());
            for spec in &components {
                let Some((name, ver)) = spec.split_once('=') else {
                    eprintln!("--component {spec:?}: expected NAME=VERSION.");
                    return Ok(misuse());
                };
                let Some(id) = ComponentId::parse(name) else {
                    eprintln!("--component {spec:?}: {name:?} is not a component name.");
                    return Ok(misuse());
                };
                pins.push(Pin { id, version: ver.to_string() });
            }
            let req = ReleaseRequest { origin: &origin, version: &version, sequence, key: &key, pins: &pins, apply };
            Ok(exit_of(&release(&req, &mut sink)?))
        }
        PublishCommand::Roster {
            from,
            origin,
            version,
            sequence,
            key,
            release_version,
            release_sequence,
            apply,
            token_file,
        } => {
            // The token: a file first, then the vault. Its value is never
            // printed; only whether one was sent.
            let token: Option<String> = match token_file.as_deref() {
                Some(p) => {
                    let t = std::fs::read_to_string(p).map_err(|e| reborn_core::Error::Io { path: p.to_path_buf(), source: e })?;
                    Some(t.trim().to_string()).filter(|t| !t.is_empty())
                }
                None => site_token(&reborn_core::Platform::detect()).unwrap_or(None),
            };
            let http = reborn_core::net::UreqHttp::new(concat!("rebornctl/", env!("CARGO_PKG_VERSION")));
            let req = RosterRequest {
                from: &from,
                origin: &origin,
                version: &version,
                sequence,
                key: key.as_deref(),
                release_version: release_version.as_deref(),
                release_sequence,
                apply,
                token: token.as_deref(),
            };
            let log = ScanLog::new();
            Ok(exit_of(&publish_roster(&http, &req, &log as &dyn ScanObserver, &mut sink)?))
        }
        PublishCommand::GamePatch {
            from,
            to,
            version,
            tree,
            xdelta,
            origin,
            sequence,
            key,
            release_version,
            release_sequence,
            apply,
        } => {
            let platform = reborn_core::Platform::detect();
            let req = GamePatchRequest {
                from: &from,
                to: &to,
                version: &version,
                tree: tree.as_deref(),
                xdelta: xdelta.as_deref(),
                origin: &origin,
                sequence,
                key: key.as_deref(),
                release_version: release_version.as_deref(),
                release_sequence,
                apply,
            };
            let log = ScanLog::new();
            Ok(exit_of(&publish_game_patch(&platform, &req, &log as &dyn ScanObserver, &mut sink)?))
        }
        PublishCommand::Token { from_file, forget } => token_command(from_file.as_deref(), forget),
    }
}

/// Store or forget the site's API token in the platform vault.
fn token_command(from_file: Option<&Path>, forget: bool) -> reborn_core::Result<std::process::ExitCode> {
    let platform = reborn_core::Platform::detect();
    if forget {
        forget_site_token(&platform)?;
        println!("Forgot the site token ({SITE_TOKEN_ACCOUNT}).");
        return Ok(OK);
    }
    let Some(path) = from_file else {
        let present = site_token(&platform)?.is_some();
        println!(
            "Site token ({SITE_TOKEN_ACCOUNT}): {}. Pass --from-file <file> to set it, --forget to remove it.",
            if present { "stored" } else { "not stored" }
        );
        return Ok(OK);
    };
    let text = std::fs::read_to_string(path).map_err(|e| reborn_core::Error::Io { path: path.to_path_buf(), source: e })?;
    let token = text.trim();
    if token.is_empty() || token.chars().any(char::is_whitespace) {
        println!("{} does not hold one token on one line. Nothing was stored.", path.display());
        return Ok(PROBLEM);
    }
    store_site_token(&platform, token)?;
    println!(
        "Stored the site token ({SITE_TOKEN_ACCOUNT}), {} characters. Delete {} now; the vault is its home.",
        token.len(),
        path.display()
    );
    Ok(OK)
}
```

Fix the file's `use` block at the top (lines 37-50): keep `use std::path::{Path, PathBuf};` and `use clap::Subcommand;`, delete the imports that only the moved code used (`TrustedRelease`, `scan_component`, `ScanOutcome`, `SerializedComponent`, `UnsignedReleaseBuilder`, `hash_file`, `Component`, `Release`, `Sha256Hex`, `blob_path`, `RELEASE_FILE`, `SIGNATURE_FILE`, `signing`, `publisher`, `Ed25519Verifier`, `thousands`). `ScanLog` in `main.rs` stays: it is the CLI's stderr progress printer and is still used here. If `ScanLog::new` is private to `main.rs`, make it `pub(crate)`. If `thousands` in `main.rs` becomes unused, keep it and allow the warning only if nothing else calls it; otherwise leave it alone.

One behavior to keep: the moved `publish_component` used to print "Hashing. Progress goes to standard error." and the scan progress went to stderr through `ScanLog`. That still happens, because `ScanLog` is passed as the observer.

- [ ] **Step 3: Build and run the unit suites**

```powershell
cargo build -p reborn-cli
cargo test --workspace
```
Expected: builds clean (no new warnings), every suite green. Rust test count before the move was 639 passed; expect that plus the 31 new core tests from Tasks 1 to 6.

- [ ] **Step 4: Run the three e2e scripts**

```powershell
py -3 Tools\e2e-update.py
py -3 Tools\e2e-roster.py
py -3 Tools\e2e-game-patch.py
```
Expected: `PASS: 13 steps`, `PASS` (3 steps), `PASS` (7 steps). `e2e-game-patch.py` needs a real disc image and `xdelta3.exe`; it finds xdelta under `target\installer-payload*\game-import` (present in the ship tree) or `REBORN_XDELTA`, and the image at the path named at the top of the script. If the image is not on this machine, say so in the task report; do not mark the step passed.

If a script fails on a missing substring, the moved line differs from the original: diff the `out!` line against the `println!` at the cited CLI line and fix the core, never the script.

- [ ] **Step 5: Run the CLI's own checker suites that touch publishing**

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\Tools\test-all.ps1 -NoRust
```
Expected: every line `ok`. (This also proves `dist` checks still hold; they are unaffected.)

- [ ] **Step 6: Commit**

```powershell
git add crates/reborn-cli/src/publish.rs crates/reborn-cli/src/main.rs
git commit -F - <<'EOF'
cli: rebornctl publish prints the core's events; the logic lives in reborn_core::publish

Every line prints as before; the three e2e scripts pass unchanged. The
CLI keeps argument parsing, exit codes, the token file lookup and the
stderr scan progress; the core owns everything that writes an origin.

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_0162ntSTXPjrqYbCJ9aBnBmK
EOF
```

---

### Task 8: Say where publishing lives now, and land the branch

**Files:**
- Modify: `docs/RUNNING_UPDATES.md` section 0 ("Build the tool", around line 43)
- Modify: `docs/NEXT_STEPS.md` (append one bullet under section 4c)

- [ ] **Step 1: Add the paragraph to `docs/RUNNING_UPDATES.md`**, at the end of section 0:

```markdown
Since 2026-09-09 the publishing logic is `reborn_core::publish` (behind the
`publish` cargo feature): `keys`, `component`, `release`, `roster`,
`game_patch`, `vault`. `rebornctl publish` builds a request, prints the
core's events and maps the outcome to an exit code; the Reborn Publisher
app calls the same functions. The lines this runbook quotes are the core's
lines. Unit tests: `cargo test -p reborn-core --features publish publish::`.
```

- [ ] **Step 2: Append to `docs/NEXT_STEPS.md` section 4c**

```markdown
- **Publisher plan 1 landed (2026-09-09):** publishing moved into
  `reborn_core::publish` with typed requests, outcomes and an event sink;
  `rebornctl publish` is a printer over it. 31 core unit tests; the three
  e2e scripts pass unchanged. Plans 2 to 4 (native upload and status, the
  app, the installer set) are next, written against these names.
```

- [ ] **Step 3: Commit**

```powershell
git add docs/RUNNING_UPDATES.md docs/NEXT_STEPS.md
git commit -m "docs: publishing lives in reborn_core::publish" -m "Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>" -m "Claude-Session: https://claude.ai/code/session_0162ntSTXPjrqYbCJ9aBnBmK"
```

- [ ] **Step 4: Land on main without touching the other agent's files**

```powershell
Set-Location "C:\Users\Adam\Desktop\Reborn Installer\NFL Street Reborn Launcher"
git status --short
```
If any file this plan touched (`crates/reborn-core/src/lib.rs`, `crates/reborn-core/src/publish/*`, `crates/reborn-cli/src/publish.rs`, `crates/reborn-cli/src/main.rs`, the two docs) shows as modified in main's working tree, stop and report; another session has it in flight. Otherwise:

```powershell
git merge --no-ff publisher-core -m "merge publisher-core: publishing moves into the core (plan 1)"
cargo build -p reborn-cli
```
Expected: a clean merge and a clean build in main. If the merge conflicts in `inventory.rs` or `manifest.rs` because of the other agent's uncommitted edits, `git merge --abort` and report; do not resolve by hand.

---

## Self-review

**Spec coverage (section 3.1, step 1 of section 10):** keygen (Task 2), component (Task 3), release and rollback path (Task 4; rollback is `release` with older pins, the app chooses them in plan 3), roster (Task 5), game_patch (Task 6), next_sequence over the origin (Task 1; the bucket half arrives with `status` in plan 2), the event sink (Task 1), the CLI as a thin printer (Task 7). Not in this plan, on purpose: `status`, `upload`, `journal`, the runtime lint (plan 2), and the typed progress payloads on `Event` (plan 3 adds fields through the `Event::out`/`Event::err` constructors, which is why no code builds `Event` with a struct literal outside `mod.rs`).

**Placeholders:** none; every code step carries its code or an exact substitution table against cited line ranges of the source file at `c4504de`.

**Type consistency:** `Sink<'_>` everywhere; `Outcome<Published>`, `Outcome<Signed>`, `Outcome<Push>` match between producers (Tasks 3 to 6) and the consumer (Task 7); `Pin { id: ComponentId, version: String }` in Tasks 4 and 7; `next_sequence(&Path, ComponentId)` in Tasks 1, 5, 6; `read_key` lives in `keys.rs` and is used by `release.rs`; `Push` lives in `roster.rs` and is reused by `game_patch.rs`.
