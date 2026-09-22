# Pack and Disc Reachability Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A player can see, on the Play page, that the texture pack will load and which version it is.

**Architecture:** One cheap probe in `reborn-core` answers "is the pack reachable, and what version is installed". `launch_check` reports it; the Play card shows it. No hashing, no directory walk beyond "does it contain files".

**Tech Stack:** Rust 2021, Tauri 2 commands in `reborn-ui`, vanilla JS in `crates/reborn-ui/ui/`.

**Spec:** `docs/superpowers/specs/2026-09-17-pack-and-disc-reachability-design.md`

## Global Constraints

- Work in an isolated worktree off `integration`. Do NOT work in `target/merge-tree` or the main checkout; both are shared with other sessions.
- Run `git status --short` immediately before any build. Never `git add -A`; stage explicit paths only.
- UI copy is US English. No em-dashes anywhere in code or copy.
- `.gitattributes` mandates `* text=auto eol=lf`. If a test fails claiming a function "moved" or is "not in this file any more", check working-tree line endings before believing it: `python -c "d=open(F,'rb').read(); print(d.count(b'\r\n'))"`. Fix with `rm <file> && git checkout -- <file>`.
- `reborn-core`'s `publish` module is behind a non-default Cargo feature. Any `cargo test` filter touching it without `--features publish` silently runs ZERO tests and prints "ok".
- Any new guard test that scans source text MUST run its body through `code_only()` (in `main.rs`'s test module) before asserting, AND must be demonstrated failing with its target removed and comments left intact. Six guards in the previous branch were at some point incapable of failing, in five different shapes.
- Verify `home.js` and `settings.js` edits with `node Tools/check-shell-pages.mjs`, not `node --check`. A `#[tauri::command]` signature change also requires `node Tools/check-harness-args.mjs`.

## Scope note, verified before writing this plan

Two of the spec's four checks turned out to be structurally guaranteed already, and this plan deliberately does NOT rebuild them:

- **The launch composes into the directory it boots.** `Session::run` passes the single resolved `written.user_dir.path()` into `sync_mods_for_launch` (`session.rs:1031`), and `mods::composed_root(user_dir, game)` (`mods.rs:1462`) derives `<user dir>/Load/Textures/<gameid>` from that same value. The runtime bug's divergence cannot occur on this path.
- **`HiresTextures` already wins.** `settings_law.rs:194` locks it to `True` in `GAME_SETTINGS`, which is `<user dir>/GameSettings/<id>.ini`, Dolphin's LocalGame layer. That beats the shipped `Sys/GameSettings/GN7.ini`.

What is genuinely missing is visibility: `mods::pristine_identity(platform, game) -> Option<String>` already reads the installed pack version, and a grep of the whole `reborn-ui` crate shows it is displayed nowhere.

---

### Task 1: The reachability probe

**Files:**
- Modify: `crates/reborn-core/src/mods.rs` (add the probe beside `composed_root` at `:1462` and `pristine_identity` at `:1479`)

**Interfaces:**
- Consumes: `mods::composed_root(user_dir: &Path, game: &GameId) -> PathBuf`; `mods::pristine_identity(platform: &Platform, game: &GameId) -> Option<String>`; `mods::COMPOSED_MARKER`.
- Produces:
  - `pub struct PackStatus { pub textures_dir: PathBuf, pub reachable: bool, pub composed_by_us: bool, pub version: Option<String> }`
  - `pub fn pack_status(platform: &Platform, user_dir: &Path, game: &GameId) -> PackStatus`

- [ ] **Step 1: Write the failing tests**

In `mods.rs`'s existing `#[cfg(test)] mod tests`:

```rust
/// The player's question is "do my textures load", and the honest answer is
/// whether the folder Dolphin reads exists and has files in it. A folder that
/// resolves but is empty loads nothing, so it is not reachable.
#[test]
fn a_missing_or_empty_texture_folder_is_not_reachable() {
    let dir = tempfile::tempdir().expect("temp dir");
    let game = GameId::parse("GN7E69").expect("game id");
    let user = dir.path().join("user");
    let platform = Platform::detect();

    // Nothing there at all.
    let absent = pack_status(&platform, &user, &game);
    assert!(!absent.reachable, "an absent folder cannot load textures");

    // Present but empty: resolves, loads nothing.
    let composed = composed_root(&user, &game);
    std::fs::create_dir_all(&composed).expect("create");
    let empty = pack_status(&platform, &user, &game);
    assert!(!empty.reachable, "an empty folder loads no textures");
    assert_eq!(empty.textures_dir, composed, "it must name the folder Dolphin reads");

    // One file is enough to load something.
    std::fs::write(composed.join("tex1_x.png"), b"not really a png").expect("write");
    let full = pack_status(&platform, &user, &game);
    assert!(full.reachable, "a folder with files in it loads textures");
}

/// A tree the launcher composed carries its marker. A folder the player or
/// another tool put there does not, and saying so is how the Play page can
/// tell "your pack" from "someone else's folder".
#[test]
fn a_folder_we_did_not_compose_is_reported_as_not_ours() {
    let dir = tempfile::tempdir().expect("temp dir");
    let game = GameId::parse("GN7E69").expect("game id");
    let user = dir.path().join("user");
    let platform = Platform::detect();
    let composed = composed_root(&user, &game);
    std::fs::create_dir_all(&composed).expect("create");
    std::fs::write(composed.join("tex1_x.png"), b"x").expect("write");

    assert!(!pack_status(&platform, &user, &game).composed_by_us);

    std::fs::write(composed.join(COMPOSED_MARKER), b"{}").expect("marker");
    assert!(pack_status(&platform, &user, &game).composed_by_us);
}
```

Check whether `GameId::parse` and `tempfile` are available in this module the way other tests here use them; match the surrounding tests' idiom rather than these exact constructors if they differ, and say so in your report.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p reborn-core pack_status`
Expected: FAIL, `pack_status` not found.

- [ ] **Step 3: Implement**

```rust
/// Whether the texture pack Dolphin will read is actually there, and which
/// version it is.
///
/// The player's question is "do my textures load". This answers exactly that
/// and nothing more: no hashing, and no walk beyond finding one entry. The
/// pack is 27,000 files and 2.83 GB of it is oversized upscales, so anything
/// that reads it in full would be paid on a path the player waits on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackStatus {
    /// `<user dir>/Load/Textures/<gameid>`, the folder Dolphin reads.
    pub textures_dir: PathBuf,
    /// It resolves and holds at least one entry. A broken junction does not
    /// resolve; an empty folder loads nothing.
    pub reachable: bool,
    /// It carries the launcher's marker, so it is the tree we composed
    /// rather than a folder someone else put there.
    pub composed_by_us: bool,
    /// What the textures content pointer says is installed.
    pub version: Option<String>,
}

pub fn pack_status(platform: &Platform, user_dir: &Path, game: &GameId) -> PackStatus {
    let textures_dir = composed_root(user_dir, game);
    let reachable = std::fs::read_dir(&textures_dir)
        .map(|mut entries| entries.next().is_some())
        .unwrap_or(false);
    PackStatus {
        composed_by_us: textures_dir.join(COMPOSED_MARKER).is_file(),
        version: pristine_identity(platform, game),
        textures_dir,
        reachable,
    }
}
```

`read_dir().next().is_some()` is the whole reachability test: it fails for an absent folder, a broken junction and an empty folder alike, and it stops after one entry.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p reborn-core pack_status` then `cargo test -p reborn-core`
Expected: both new tests pass; the rest of the suite unchanged.

- [ ] **Step 5: Commit**

```bash
git add crates/reborn-core/src/mods.rs
git commit -m "feat: answer whether the texture pack will load, and which version"
```

---

### Task 2: Report it from launch_check and show it on the Play card

**Files:**
- Modify: `crates/reborn-ui/src/main.rs` (`struct LaunchCheck` at `:575`, `async fn launch_check` at `:600`)
- Modify: `crates/reborn-ui/ui/home.js` (the Play card, which already renders from `data.check`)

**Interfaces:**
- Consumes: `mods::pack_status(platform, user_dir, game) -> PackStatus` from Task 1.
- Produces: `LaunchCheck` gains `pack: Option<PackView>` where `PackView { reachable: bool, version: Option<String> }`, serialized as `pack: { reachable, version }`.

- [ ] **Step 1: Write the failing test**

In `main.rs`'s existing `mod tests`:

```rust
/// The player's own test for whether their pack is right is "can I see
/// stuff, and what version is it". Neither was on any screen, so the answer
/// lived only in a support conversation. launch_check already runs on every
/// Play page load, so it is where the answer belongs.
#[test]
fn the_play_page_can_tell_the_player_which_pack_is_loading() {
    let body = code_only(command_body(include_str!("main.rs"), "async fn launch_check"));
    assert!(
        body.contains("pack_status"),
        "launch_check does not ask whether the texture pack will load"
    );
    let home = code_only(include_str!("../ui/home.js"));
    assert!(
        home.contains("pack"),
        "the Play page does not show the pack's state"
    );
}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test -p reborn-ui which_pack_is_loading`
Expected: FAIL, `launch_check does not ask whether the texture pack will load`.

- [ ] **Step 3: Add the field and populate it**

Add beside the other `LaunchCheck` fields, following the file's existing `#[serde(skip_serializing_if = "Option::is_none")]` convention:

```rust
    /// Whether the texture pack will load, and which version. Absent when no
    /// user directory could be resolved, in which case there is nothing to
    /// report about.
    pack: Option<PackView>,
```

and a view struct beside the other `#[derive(Serialize)]` views:

```rust
#[derive(Debug, Serialize)]
struct PackView {
    reachable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<String>,
}
```

In `launch_check`, after the user directory is resolved, populate it from the resolved directory rather than re-resolving one:

```rust
            pack: reborn_core::mods::GameId::parse(reborn_core::session::GameBuild::Offline.game_id())
                .ok()
                .and_then(|game| {
                    resolved_user_dir.as_ref().map(|dir| {
                        let status = reborn_core::mods::pack_status(&p, dir, &game);
                        PackView { reachable: status.reachable, version: status.version }
                    })
                }),
```

Match the real name of whatever `launch_check` already holds the resolved user directory in; do NOT call `resolve_user_dir_for` a second time, because the point of the check is to report on the directory the launch will actually use. Say in your report what that binding is called.

- [ ] **Step 4: Show it on the Play card**

In `home.js`, where the launch-check result is rendered, add a line stating the version, and a plain note when the pack will not load. Use the file's own `el()` signature and existing classes rather than inventing markup:

```js
  if (data.check?.pack) {
    const pack = data.check.pack;
    const text = pack.reachable
      ? (pack.version ? `Texture pack ${pack.version}` : 'Texture pack loaded')
      : 'Texture pack not loading';
    card.append(el('p', { class: pack.reachable ? 'play-note' : 'play-warning' }, text));
  }
```

Match `home.js`'s actual element helper and the class names already used on that card; the Task 12 badge added in the previous branch is a working example to copy from.

- [ ] **Step 5: Run everything**

Run: `cargo test -p reborn-ui`, `node Tools/check-shell-pages.mjs`, `node Tools/check-harness-args.mjs`
Expected: all pass. `check-harness-args` matters because `LaunchCheck` changed shape.

- [ ] **Step 6: Commit**

```bash
git add crates/reborn-ui/src/main.rs crates/reborn-ui/ui/home.js
git commit -m "feat: say on the Play page whether the pack loads and which version"
```

---

### Task 3: Say plainly what Open Dolphin does to textures

**Files:**
- Modify: `crates/reborn-ui/ui/settings.js:354` (the Dolphin toolbar: the `<p>` reading "Everyday settings below. Open Dolphin for its complete set of tools." sits beside the Open Dolphin button)
- Consider also: `crates/reborn-ui/ui/recovery.js:106` (the recovery page's own Open Dolphin row, whose description is "Open the emulator to review its settings.")

Three other call sites exist and are NOT copy surfaces: `home.js:201` (a handler), `app.js:173` (the developer harness).

**Interfaces:**
- Consumes: nothing.
- Produces: no new interface, one line of copy.

**Why.** `open_runtime` is deliberately out of scope for the game-image law: it hands the player Dolphin's own UI with no gate, and Reborn's texture pack is keyed by game ID, so any NFL Street 2 disc booted there picks up Reborn's faces. That is the last remaining route to the reported symptom. Adam's decision is to name it, not police it.

- [ ] **Step 1: Add the line**

In `settings.js:354`, extend the existing `<p>` beside the Open Dolphin button, in that file's own `el('p',{text:...})` idiom:

> Reborn's textures apply to any NFL Street 2 disc you open here, so another copy will show Reborn's players.

US English, no em-dashes, and it must not imply the player did something wrong.

- [ ] **Step 2: Decide on the recovery page too**

`recovery.js:106` offers the same action with its own one-line description. Either extend it the same way or leave it and say why in your report. A player on the recovery page is already troubleshooting, so the note is arguably more useful there, not less.

- [ ] **Step 3: Verify the pages still work**

Run: `node Tools/check-shell-pages.mjs`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add <the page you changed>
git commit -m "docs: say that Reborn's textures apply to any disc opened in Dolphin"
```

---

## Not in this plan, and why

- **Hashing the pack.** Adam's call: version plus visible result is the measure.
- **Repair.** The spec called for silent re-point or recompose on failure. `sync_mods_for_launch` already composes on every launch, so a missing tree is rebuilt before the game starts without anything new being written. Adding a second repair path would duplicate `Composer` for a case the launch already handles. If the probe turns out to report `reachable: false` on machines where a launch has just run, that is a real finding and earns its own task.
- **Gating direct Dolphin**, per Adam.
- **The oversized upscales and the open presnap frame-rate drop.** Content and performance, not reachability, and there is a standing instruction not to touch the pack unless positive.
