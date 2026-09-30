# Launcher Glass Redesign, Plan 1: Mica spike + Foundation

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Every launcher page gets the frosted-glass look (tokens, material, sidebar, motion), plus two player settings (Window glass, Slow animations) and the controller-prompt rule. No page layouts change.

**Architecture:**
- A new `ui/theme.css` loads **last** on every page. It defines the spec's tokens and remaps the old stylesheet variables onto them, so every page changes look at once without layout breakage. Old files are retired in a later plan.
- A tiny synchronous `ui/look-boot.js` sets `html[data-glass]` / `html[data-motion]` before first paint from a cached value. `shell.js` then confirms the real values from the core.
- Rust: two new `Settings` fields in `reborn-core`, a pure material picker in `reborn-ui/src/launcher_look.rs`, and three Tauri commands in `main.rs` that call `WebviewWindow::set_effects`.

**Tech Stack:** Tauri 2.11 (Rust), static HTML/JS/CSS in `crates/reborn-ui/ui` (no npm, `withGlobalTauri`), Playwright + Edge for browser checks, `cargo test`.

**Spec:** `docs/superpowers/specs/2026-09-30-launcher-glass-redesign-design.md` (§3 tokens, §4 material, §5 motion, §6 shell, §7 save bar, §12 step 1–2, §13 tests). This is **Plan 1 of 7**. Later plans each cover one page and are written when this one has shipped:
- Play
- Settings
- Mods / Music / Controllers
- Stats
- Profile
- Cleanup

**Working copy:** worktree `C:\Users\Adam\Desktop\Reborn Installer\NFL Street Reborn Launcher redesign`, branch `feat/launcher-glass-redesign`. Never commit to `main`, `release/*` or anyone else's worktree.

## Global Constraints

- **Colours:** `--red #E0272E` (fill), `--red-ink #FF7A7F` (red text), `--ok #3DDC97`, `--fg #F4F5F7`, `--fg-2 #AEB5C0`, `--fg-3 #7C8491`, `--mica-tint rgba(18,19,23,.66)`.
- **Fallback background:** `radial-gradient(60% 60% at 50% 40%, #23262d 0, transparent 70%), #0d0e11`.
- **No glow anywhere:** no coloured `box-shadow` on buttons, focus rings or switches. Focus ring = `outline: 2px solid #fff; outline-offset: 3px`.
- **Font weights:** nothing above 700. Inter only in the shell.
- **Spacing:** 8 / 16 / 24 only. Radii 14 / 10 / 8 / 999.
- **Motion:**
  - One easing curve: `cubic-bezier(.2,.8,.2,1)`. Only `opacity`/`transform` animate (height via `grid-template-rows`/`max-height`).
  - Slow durations **480 / 720 / 960 ms**, quick durations **120 / 180 / 240 ms**. Slow is the default.
  - `prefers-reduced-motion: reduce` sets every duration to 0.
  - Set durations **directly** per `html[data-motion]` value, never through a multiplier variable (custom properties don't re-resolve).
- **Settings defaults:** `window_glass: None` means **on** where supported. `slow_animations: None` means **on**.
- **Logos:** don't resize, stretch or redraw `art/nsr-sidebar.png` or `art/play-logo.png`. Keep every existing nav icon SVG.
- **Commands are the contract:** page values come from commands. Commands live in `main.rs` (`Tools/check-page-call-args.mjs` reads only `main.rs`).
- **Safety:** server text only via `el()`/`textContent`, never `innerHTML`. CSP is `script-src 'self'`, so **no inline scripts**.
- **Build and test (from the worktree root):**
  - Kill any running `reborn-launcher` first (standing permission).
  - `cargo test --workspace -j 2`, then `cargo clippy --workspace --all-targets`.
  - `cargo build --workspace` before running the app.
  - Write multi-line patch scripts with the Write tool, never heredocs.
- **Line endings:** `.rs` files must stay LF. CRLF in a `.rs` file breaks source-scanning tests.

## Review Focus

1. **A pad is plugged in but the player uses the mouse:** no hint bar and no white focus ring. **Exception:** an unmapped (non-standard) pad still shows "Set up launcher controls", or it could never be set up. Test in Task 7.
2. **Glass is saved "on", but the OS can't do it** (Linux, or Win 10 if the spike says no Acrylic): the page shows the fallback, and Settings shows the switch disabled with the reason. It must never show a transparent window with nothing behind it. Test in Task 3 (picker) and Task 6 (UI state).
3. **The settings file already exists from an older launcher:** it must still load, with both new fields absent and read as on. Test in Task 2.
4. **The first paint uses the cached look before the core answers.** If the cache is wrong or `localStorage` throws, the page still renders with the defaults and corrects itself without an error. Test in Task 4.
5. **Discard after previewing a changed setting** restores the saved glass and motion, and the save bar disappears. Test in Task 6.

---

## File structure

| File | Responsibility |
|---|---|
| `crates/reborn-core/src/settings.rs` (modify) | `window_glass`, `slow_animations` fields + `launcher_look()` / `set_launcher_look()` |
| `crates/reborn-ui/src/launcher_look.rs` (create) | pure: `Material`, `pick_material()`, `parse_windows_build()`; no Tauri types |
| `crates/reborn-ui/src/main.rs` (modify) | commands `launcher_look`, `launcher_look_set`, `window_glass_apply`; startup apply in `.setup`; registration |
| `crates/reborn-ui/tauri.conf.json` (modify) | transparent window |
| `crates/reborn-ui/ui/theme.css` (create) | tokens, material, fallback, old-var remap, shell (sidebar, nav marker, hint bar), components (btn, switch, save bar), motion |
| `crates/reborn-ui/ui/look-boot.js` (create) | sync, pre-paint: sets `data-glass`/`data-motion` from cache |
| `crates/reborn-ui/ui/look.js` (create) | ES module: reads/sets look via commands, updates cache + attributes, sliding nav marker |
| `crates/reborn-ui/ui/savebar.js` (create) | ES module: the generic save bar (§7) |
| `crates/reborn-ui/ui/settings.js` (modify) | Display tab: "Launcher window" section using `look.js` + `savebar.js` |
| `crates/reborn-ui/ui/app-shell.js` (modify) | hints only in controller mode; mouse move and any key leave controller mode |
| `crates/reborn-ui/ui/*.html` (modify, 12 pages) | add `look-boot.js` in `<head>`, `theme.css` last, save lane on settings |
| `Tools/check-theme.mjs` (create) | static: every page loads the look correctly; theme defines every token; no glow/weights > 700 in theme |
| `Tools/check-look-browser.mjs` (create) | Playwright: motion durations, reduced motion, fallback vs glass, nav marker, save bar preview/discard |
| `Tools/check-controller-hints.mjs` (create) | Playwright: hint bar only in controller mode |
| `Tools/run-browser-tests.mjs` (modify) | import the two new browser checks |

---

### Task 1: Mica spike (throwaway, decides Win 10 + title bar)

**Files:** throwaway branch `spike/mica-window` only. **Nothing from this task is merged.**

**Interfaces:**
- Produces: a written verdict at `docs/superpowers/plans/2026-09-30-mica-spike-result.md` (committed on **this** branch, `feat/launcher-glass-redesign`) with three answers used by Task 3:
  - `WIN11_MICA_OK: yes|no`
  - `WIN10_ACRYLIC_OK: yes|no|untested`
  - `TITLEBAR_OK: yes|no`

- [ ] **Step 1: Create the spike branch from this branch**

```bash
cd "C:/Users/Adam/Desktop/Reborn Installer/NFL Street Reborn Launcher redesign"
git switch -c spike/mica-window
```

- [ ] **Step 2: Make the window transparent**

In `crates/reborn-ui/tauri.conf.json`, in `app.windows[0]`, add `"transparent": true` and remove `"backgroundColor": "#0B0C0E"`.

- [ ] **Step 3: Apply Mica at startup**

In `crates/reborn-ui/src/main.rs`, insert directly before `.invoke_handler(tauri::generate_handler![` (around line 6178):

```rust
        .setup(|app| {
            if let Some(w) = tauri::Manager::get_webview_window(app, "main") {
                use tauri::window::{Effect, EffectsBuilder};
                let effect = if std::env::var("SPIKE_EFFECT").as_deref() == Ok("acrylic") { Effect::Acrylic } else { Effect::Mica };
                if let Err(e) = w.set_effects(EffectsBuilder::new().effect(effect).build()) {
                    eprintln!("spike: set_effects failed: {e}");
                }
            }
            Ok(())
        })
```

In `crates/reborn-ui/ui/styles.css`, add at the very end: `html,body{background:rgba(18,19,23,.66)!important}`.

- [ ] **Step 4: Build and run**

```bash
taskkill /IM reborn-launcher.exe /F 2>NUL
cargo build -p reborn-ui
target/debug/reborn-launcher.exe
```
Expected: the launcher opens, and the desktop colour shows through blurred behind the sidebar and pages.

- [ ] **Step 5: Walk the checklist and screenshot each item**

Save the screenshots to `target/spike/`.
1. Mica visible on the sidebar and page background. Move the window over a colourful wallpaper.
2. The title bar (OS decorations) looks correct: no black band, no white flash on focus change.
3. Resize, maximise, restore, and Win+Z snap layouts all work. No flicker to black while resizing.
4. Minimise, then restore.
5. Launch a game (Play now) in fullscreen Dolphin, quit, and return: the launcher still shows Mica.
6. Toggle Windows dark/light theme: the launcher stays readable (the tint is ours).
7. If a Windows 10 machine or VM is available: run with `SPIKE_EFFECT=acrylic` and drag the window for 10 s. Record whether it lags.

- [ ] **Step 6: Write the verdict and discard the spike**

```bash
git add -A && git commit -m "spike: Mica window (throwaway, never merged)"   # stays on spike/mica-window only
git switch feat/launcher-glass-redesign
```
Create `docs/superpowers/plans/2026-09-30-mica-spike-result.md`:
```markdown
# Mica spike result (2026-09-30)
WIN11_MICA_OK: <yes|no>
WIN10_ACRYLIC_OK: <yes|no|untested>
TITLEBAR_OK: <yes|no>
Notes: <one line per checklist item 1–7, with screenshot names>
```
```bash
git add docs/superpowers/plans/2026-09-30-mica-spike-result.md
git commit -m "spike: Mica window result"
```
Never use `git stash` here: the stash stack is shared with every other worktree and session.

- [ ] **Step 7: GATE — show Adam and wait**

Show Adam the screenshots and the verdict.
- If `WIN11_MICA_OK: no` or `TITLEBAR_OK: no`: **stop the plan** and ask Adam how to proceed (spec §14 Q2).
- Otherwise continue.

---

### Task 2: Settings fields for the look

**Files:**
- Modify: `crates/reborn-core/src/settings.rs` (struct ~line 60–125, `impl Default` ~line 174, helpers after `set_automatic_updates` ~line 324, tests module ~line 335)

**Interfaces:**
- Produces:
  - `pub struct LauncherLook { pub window_glass: bool, pub slow_animations: bool }`
  - `pub fn launcher_look(path: &Path) -> Result<LauncherLook>` (absent = true)
  - `pub fn set_launcher_look(path: &Path, window_glass: Option<bool>, slow_animations: Option<bool>) -> Result<LauncherLook>` (`None` argument = leave as is), returning the new effective look.

- [ ] **Step 1: Write the failing tests**

Append inside `mod tests` in `settings.rs`:

```rust
    #[test]
    fn the_look_defaults_to_glass_and_slow_animations() {
        // Adam, 2026-09-30: glass where the OS has it, slow animations on by
        // default. Absent means never set, and never set means on.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(SETTINGS_FILE);
        assert_eq!(launcher_look(&path).unwrap(), LauncherLook { window_glass: true, slow_animations: true });
        assert!(!path.exists(), "reading must not create the file");
    }

    #[test]
    fn the_look_can_be_changed_one_field_at_a_time() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(SETTINGS_FILE);
        let after = set_launcher_look(&path, Some(false), None).unwrap();
        assert_eq!(after, LauncherLook { window_glass: false, slow_animations: true });
        let after = set_launcher_look(&path, None, Some(false)).unwrap();
        assert_eq!(after, LauncherLook { window_glass: false, slow_animations: false });
        assert_eq!(launcher_look(&path).unwrap(), after);
    }

    #[test]
    fn a_settings_file_from_before_the_look_still_reads() {
        // An older launcher wrote no look fields at all.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(SETTINGS_FILE);
        std::fs::write(&path, format!(r#"{{"schema":"{SETTINGS_SCHEMA}","automatic_updates":true}}"#)).unwrap();
        assert_eq!(launcher_look(&path).unwrap(), LauncherLook { window_glass: true, slow_animations: true });
        assert_eq!(Settings::load(&path).unwrap().automatic_updates, Some(true));
    }
```

- [ ] **Step 2: Run them and confirm they fail**

Run: `cargo test -p reborn-core the_look -- --nocapture` and `cargo test -p reborn-core a_settings_file_from_before_the_look`
Expected: compile error, `cannot find function launcher_look` / `LauncherLook`.

- [ ] **Step 3: Implement**

In `pub struct Settings`, after `unverified_game_ack`:
```rust
    /// Frosted window glass. `None` means on wherever the OS has it; the
    /// launcher falls back to a solid dark window where it does not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_glass: Option<bool>,
    /// Slow, smooth animations. `None` means on: Adam chose slow as the
    /// default on 2026-09-30. The OS reduce-motion setting always wins.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slow_animations: Option<bool>,
```
In `impl Default for Settings`, add `window_glass: None, slow_animations: None,`.

After `set_automatic_updates`:
```rust
/// How the launcher window looks, as the player set it (absent = on).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct LauncherLook {
    pub window_glass: bool,
    pub slow_animations: bool,
}

/// Read the look.
pub fn launcher_look(path: &Path) -> Result<LauncherLook> {
    let s = Settings::load(path)?;
    Ok(LauncherLook {
        window_glass: s.window_glass.unwrap_or(true),
        slow_animations: s.slow_animations.unwrap_or(true),
    })
}

/// Change either field; `None` leaves that field as it is. Returns the new look.
pub fn set_launcher_look(
    path: &Path,
    window_glass: Option<bool>,
    slow_animations: Option<bool>,
) -> Result<LauncherLook> {
    let mut s = Settings::load(path)?;
    if let Some(v) = window_glass {
        s.window_glass = Some(v);
    }
    if let Some(v) = slow_animations {
        s.slow_animations = Some(v);
    }
    s.save(path)?;
    launcher_look(path)
}
```

- [ ] **Step 4: Run the tests and confirm they pass**

Run: `cargo test -p reborn-core settings -j 2`
Expected: all `settings::tests` pass, including the three new ones.

- [ ] **Step 5: Commit**

```bash
git add crates/reborn-core/src/settings.rs
git commit -m "core: launcher look settings (window glass, slow animations), absent = on"
```

---

### Task 3: Material picker (pure)

**Files:**
- Create: `crates/reborn-ui/src/launcher_look.rs`
- Modify: `crates/reborn-ui/src/main.rs:25-34` (add `mod launcher_look;`)

**Interfaces:**
- Consumes: the spike verdict from Task 1 (the value of `WIN10_ACRYLIC_OK`).
- Produces:
  - `pub enum Material { Mica, Acrylic, Vibrancy }`
  - `pub enum Os { Windows, MacOs, Linux }`
  - `pub fn current_os() -> Os`
  - `pub fn parse_windows_build(reg_query_stdout: &str) -> Option<u32>`
  - `pub fn pick_material(os: Os, windows_build: Option<u32>) -> Option<Material>`
  - `pub fn windows_build() -> Option<u32>` (runs `reg query`, Windows only; `None` elsewhere)
  - `pub const WIN10_ACRYLIC: bool`

- [ ] **Step 1: Write the failing tests**

Create `crates/reborn-ui/src/launcher_look.rs` with only the tests first:

```rust
//! Which window material the OS can give the launcher. Pure: no Tauri types,
//! so the choice is testable on any machine.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_11_gets_mica() {
        assert_eq!(pick_material(Os::Windows, Some(26100)), Some(Material::Mica));
        assert_eq!(pick_material(Os::Windows, Some(22000)), Some(Material::Mica));
    }

    #[test]
    fn windows_10_follows_the_spike() {
        let expected = if WIN10_ACRYLIC { Some(Material::Acrylic) } else { None };
        assert_eq!(pick_material(Os::Windows, Some(19045)), expected);
    }

    #[test]
    fn an_unknown_windows_build_gets_no_glass() {
        // Never a transparent window with nothing behind it.
        assert_eq!(pick_material(Os::Windows, None), None);
    }

    #[test]
    fn mac_gets_vibrancy_and_linux_gets_nothing() {
        assert_eq!(pick_material(Os::MacOs, None), Some(Material::Vibrancy));
        assert_eq!(pick_material(Os::Linux, None), None);
    }

    #[test]
    fn the_build_number_is_read_from_reg_query_output() {
        let out = "\r\nHKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\r\n    CurrentBuildNumber    REG_SZ    26100\r\n\r\n";
        assert_eq!(parse_windows_build(out), Some(26100));
        assert_eq!(parse_windows_build("garbage"), None);
        assert_eq!(parse_windows_build("    CurrentBuildNumber    REG_SZ    abc"), None);
    }
}
```
Add `mod launcher_look;` to the `mod` list in `main.rs` (after `mod live_view;`).

- [ ] **Step 2: Run and confirm failure**

Run: `cargo test -p reborn-ui launcher_look -j 2`
Expected: compile errors (`pick_material`, `Os`, `Material`, `parse_windows_build`, `WIN10_ACRYLIC` not found).

- [ ] **Step 3: Implement**

Above the tests module in `launcher_look.rs`. Set `WIN10_ACRYLIC` from the spike verdict: `true` only if `WIN10_ACRYLIC_OK: yes`.

```rust
/// Set from docs/superpowers/plans/2026-09-30-mica-spike-result.md.
/// false means Windows 10 uses the solid dark fallback.
pub const WIN10_ACRYLIC: bool = false;

/// First Windows 11 build.
const WIN11_FIRST_BUILD: u32 = 22000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Material { Mica, Acrylic, Vibrancy }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os { Windows, MacOs, Linux }

pub fn current_os() -> Os {
    if cfg!(target_os = "windows") { Os::Windows } else if cfg!(target_os = "macos") { Os::MacOs } else { Os::Linux }
}

/// The material this OS can show, or None for the solid dark fallback.
pub fn pick_material(os: Os, windows_build: Option<u32>) -> Option<Material> {
    match os {
        Os::MacOs => Some(Material::Vibrancy),
        Os::Linux => None,
        Os::Windows => match windows_build {
            Some(b) if b >= WIN11_FIRST_BUILD => Some(Material::Mica),
            Some(_) if WIN10_ACRYLIC => Some(Material::Acrylic),
            _ => None,
        },
    }
}

/// `reg query ... /v CurrentBuildNumber` prints `CurrentBuildNumber REG_SZ 26100`.
pub fn parse_windows_build(reg_query_stdout: &str) -> Option<u32> {
    reg_query_stdout
        .lines()
        .find(|l| l.trim_start().starts_with("CurrentBuildNumber"))
        .and_then(|l| l.split_whitespace().last())
        .and_then(|n| n.parse().ok())
}

/// The running Windows build, or None (not Windows, or unreadable).
pub fn windows_build() -> Option<u32> {
    if current_os() != Os::Windows {
        return None;
    }
    let out = std::process::Command::new("reg")
        .args(["query", r"HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion", "/v", "CurrentBuildNumber"])
        .output()
        .ok()?;
    parse_windows_build(&String::from_utf8_lossy(&out.stdout))
}
```
On Windows, add `use std::os::windows::process::CommandExt;` and `.creation_flags(0x0800_0000)` (CREATE_NO_WINDOW) to the `Command` under `#[cfg(windows)]`, so no console window flashes. Copy the pattern from any existing `Command::new` in `main.rs` (`grep -n creation_flags crates/reborn-ui/src/*.rs`).

- [ ] **Step 4: Run and confirm the tests pass**

Run: `cargo test -p reborn-ui launcher_look -j 2`
Expected: 5 passed.

- [ ] **Step 5: Commit**

```bash
git add crates/reborn-ui/src/launcher_look.rs crates/reborn-ui/src/main.rs
git commit -m "ui: pick the window material per OS (Mica / Acrylic per spike / vibrancy / none)"
```

---

### Task 4: Commands, startup apply, transparent window

**Files:**
- Modify: `crates/reborn-ui/src/main.rs` (commands after `update_policy_set` ~line 4480; `.setup` before `.invoke_handler` ~line 6178; register in `generate_handler!`; tests in `mod tests` ~line 6348)
- Modify: `crates/reborn-ui/tauri.conf.json`

**Interfaces:**
- Consumes: `reborn_core::settings::{launcher_look, set_launcher_look, LauncherLook, settings_path}` (Task 2), `launcher_look::{pick_material, windows_build, current_os, Material}` (Task 3).
- Produces (JS calls these; argument names are camelCase at the JS boundary):
  - `invoke('launcher_look')` → `{ windowGlass: bool, slowAnimations: bool, glassSupported: bool, glassApplied: bool, glassNote: string|null }`
  - `invoke('launcher_look_set', { windowGlass: bool|null, slowAnimations: bool|null })` → same shape. **Persists.**
  - `invoke('window_glass_apply', { enabled: bool })` → same shape. **Preview only: applies without saving.**
  - Rust pure helper: `fn look_view(saved: LauncherLook, material: Option<Material>, applied: bool, os: launcher_look::Os) -> LookView`

- [ ] **Step 1: Write the failing test for the view**

In `mod tests` of `main.rs`:
```rust
    #[test]
    fn the_look_view_says_why_there_is_no_glass() {
        use reborn_core::settings::LauncherLook;
        use launcher_look::{Material, Os};
        let on = LauncherLook { window_glass: true, slow_animations: true };
        let v = look_view(on, Some(Material::Mica), true, Os::Windows);
        assert!(v.glass_supported && v.glass_applied && v.glass_note.is_none());
        let v = look_view(on, None, false, Os::Linux);
        assert!(!v.glass_supported && !v.glass_applied);
        assert_eq!(v.glass_note.as_deref(), Some("Frosted glass isn't available on Linux."));
        let v = look_view(on, None, false, Os::Windows);
        assert_eq!(v.glass_note.as_deref(), Some("Not available on this system."));
        let off = LauncherLook { window_glass: false, slow_animations: false };
        let v = look_view(off, Some(Material::Mica), false, Os::Windows);
        assert!(v.glass_supported && !v.glass_applied && !v.slow_animations && v.glass_note.is_none());
    }
```

- [ ] **Step 2: Run and confirm failure**

Run: `cargo test -p reborn-ui the_look_view -j 2`
Expected: compile error, `look_view` not found.

- [ ] **Step 3: Implement the view, the effect application and the commands**

After `update_policy_set` in `main.rs`:
```rust
/// How the launcher window looks right now, for the page.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct LookView {
    window_glass: bool,
    slow_animations: bool,
    glass_supported: bool,
    glass_applied: bool,
    glass_note: Option<String>,
}

fn look_view(
    saved: reborn_core::settings::LauncherLook,
    material: Option<launcher_look::Material>,
    applied: bool,
    os: launcher_look::Os,
) -> LookView {
    let glass_note = match (material, os) {
        (Some(_), _) => None,
        (None, launcher_look::Os::Linux) => Some("Frosted glass isn't available on Linux.".to_string()),
        (None, _) => Some("Not available on this system.".to_string()),
    };
    LookView {
        window_glass: saved.window_glass,
        slow_animations: saved.slow_animations,
        glass_supported: material.is_some(),
        glass_applied: applied,
        glass_note,
    }
}

/// Put the material on the window, or take it off. Returns whether glass is showing.
fn apply_glass(window: &tauri::WebviewWindow, material: Option<launcher_look::Material>, enabled: bool) -> bool {
    use tauri::window::{Effect, EffectsBuilder};
    let effect = match (enabled, material) {
        (true, Some(launcher_look::Material::Mica)) => Some(Effect::Mica),
        (true, Some(launcher_look::Material::Acrylic)) => Some(Effect::Acrylic),
        (true, Some(launcher_look::Material::Vibrancy)) => Some(Effect::UnderWindowBackground),
        _ => None,
    };
    match effect {
        Some(e) => window.set_effects(EffectsBuilder::new().effect(e).build()).is_ok(),
        None => {
            let _ = window.set_effects(None);
            false
        }
    }
}

fn material_here() -> Option<launcher_look::Material> {
    launcher_look::pick_material(launcher_look::current_os(), launcher_look::windows_build())
}

#[tauri::command]
async fn launcher_look(window: tauri::WebviewWindow) -> Result<LookView, String> {
    let saved = tauri::async_runtime::spawn_blocking(|| {
        let file = reborn_core::settings::settings_path(&Platform::detect()).map_err(|e| e.to_string())?;
        reborn_core::settings::launcher_look(&file).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;
    let material = material_here();
    let applied = saved.window_glass && material.is_some();
    Ok(look_view(saved, material, applied, launcher_look::current_os()))
}

/// Save a change (either field; null leaves it). Glass is re-applied to match.
#[tauri::command]
async fn launcher_look_set(
    window: tauri::WebviewWindow,
    window_glass: Option<bool>,
    slow_animations: Option<bool>,
) -> Result<LookView, String> {
    let saved = tauri::async_runtime::spawn_blocking(move || {
        let file = reborn_core::settings::settings_path(&Platform::detect()).map_err(|e| e.to_string())?;
        reborn_core::settings::set_launcher_look(&file, window_glass, slow_animations).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;
    let material = material_here();
    let applied = apply_glass(&window, material, saved.window_glass);
    Ok(look_view(saved, material, applied, launcher_look::current_os()))
}

/// Preview glass on or off WITHOUT saving (Settings uses this before Save).
#[tauri::command]
async fn window_glass_apply(window: tauri::WebviewWindow, enabled: bool) -> Result<LookView, String> {
    let saved = tauri::async_runtime::spawn_blocking(|| {
        let file = reborn_core::settings::settings_path(&Platform::detect()).map_err(|e| e.to_string())?;
        reborn_core::settings::launcher_look(&file).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;
    let material = material_here();
    let applied = apply_glass(&window, material, enabled);
    Ok(look_view(saved, material, applied, launcher_look::current_os()))
}
```
In `launcher_look`, the `window` parameter is unused. Rename it to `_window` so clippy stays clean, and keep the parameter so the signature matches the other two.

Startup: insert before `.invoke_handler(` (~6178):
```rust
        .setup(|app| {
            // Glass before the first paint; the page's cached look covers the rest.
            if let Some(w) = tauri::Manager::get_webview_window(app, "main") {
                let saved = reborn_core::settings::settings_path(&Platform::detect())
                    .and_then(|f| reborn_core::settings::launcher_look(&f))
                    .map(|l| l.window_glass)
                    .unwrap_or(true);
                apply_glass(&w, material_here(), saved);
            }
            Ok(())
        })
```
Register `launcher_look, launcher_look_set, window_glass_apply,` inside `generate_handler![`, next to `texture_policy_set`.

`tauri.conf.json`, in `app.windows[0]`: add `"transparent": true` and delete `"backgroundColor": "#0B0C0E"`. Also add `"macOSPrivateApi": true` under `"app"`. If Tauri reports that it needs the cargo feature, add `features = ["macos-private-api"]` to the `tauri` dependency in `crates/reborn-ui/Cargo.toml`.

- [ ] **Step 4: Run tests and clippy**

Run: `cargo test -p reborn-ui -j 2` then `cargo clippy --workspace --all-targets`
Expected: all pass. No new clippy warnings.

- [ ] **Step 5: Check the JS/Rust argument contract**

Run: `node Tools/check-page-call-args.mjs`
Expected: PASS. No page calls the new commands yet; this proves `main.rs` still parses for the checker.

- [ ] **Step 6: Commit**

```bash
git add crates/reborn-ui/src/main.rs crates/reborn-ui/tauri.conf.json crates/reborn-ui/Cargo.toml
git commit -m "ui: launcher_look commands, glass applied at startup, transparent window"
```

---

### Task 5: `theme.css`, pre-paint boot, every page on the new look

**Files:**
- Create: `crates/reborn-ui/ui/theme.css`, `crates/reborn-ui/ui/look-boot.js`, `crates/reborn-ui/ui/look.js`, `Tools/check-theme.mjs`
- Modify: all pages except `harness.html`: `index.html controls.html game.html live.html mods.html music.html onboarding.html online.html profile.html recovery.html settings.html stats.html`

**Interfaces:**
- Consumes: the `launcher_look` command (Task 4).
- Produces:
  - `look-boot.js` (classic script): sets `document.documentElement.dataset.glass = 'on'|'off'` and `.dataset.motion = 'slow'|'quick'` from `localStorage['reborn:look']` (JSON `{glass:bool, slow:bool}`), with default `{glass:false, slow:true}`. Glass defaults **off** until confirmed, so a transparent window is never shown with nothing behind it.
  - `look.js` (ES module):
    - `export function applyLook(view)`: sets both attributes from a `LookView` (`glass = view.glassApplied`, `motion = view.slowAnimations ? 'slow' : 'quick'`) and writes the cache.
    - `export async function refreshLook()`: calls `invoke('launcher_look')` → `applyLook`, and returns the view (or `null` on any error, leaving the page as it is).
    - `export function mountNavMarker()`: the single sliding sidebar marker.

- [ ] **Step 1: Write the failing static checker**

Create `Tools/check-theme.mjs`:
```js
// Every product page gets the new look the same way, and theme.css holds the rules.
//     node Tools/check-theme.mjs
import { readFileSync } from "node:fs";
import assert from "node:assert/strict";
const ui = "crates/reborn-ui/ui/";
const PAGES = ["index","controls","game","live","mods","music","onboarding","online","profile","recovery","settings","stats"];
for (const p of PAGES) {
  const html = readFileSync(ui + p + ".html", "utf8");
  const head = html.slice(0, html.indexOf("</head>"));
  assert.match(head, /<script src="look-boot\.js"><\/script>/, `${p}: look-boot.js must load in <head>, classic and synchronous`);
  const sheets = [...head.matchAll(/href="([^"]+\.css)"/g)].map(m => m[1]);
  assert.equal(sheets.at(-1), "theme.css", `${p}: theme.css must be the LAST stylesheet (got ${sheets.join(", ")})`);
  assert.doesNotMatch(html, /<script>(?!<\/script>)/, `${p}: no inline scripts (CSP script-src 'self')`);
}
const theme = readFileSync(ui + "theme.css", "utf8");
for (const t of ["--mica-tint","--glass-1","--glass-2","--glass-3","--stroke","--stroke-hi","--sheen","--shadow","--fg","--fg-2","--fg-3","--red","--red-hi","--red-ink","--ok","--warn","--ease","--d1","--d2","--d3"])
  assert.match(theme, new RegExp(`${t}\\s*:`), `theme.css must define ${t}`);
assert.match(theme, /html\[data-motion="slow"\][^{]*\{[^}]*--d1:\s*480ms/, "slow --d1 = 480ms, set directly");
assert.match(theme, /html\[data-motion="quick"\][^{]*\{[^}]*--d3:\s*240ms/, "quick --d3 = 240ms, set directly");
assert.match(theme, /prefers-reduced-motion:\s*reduce/, "reduced motion override");
assert.doesNotMatch(theme, /--slow\b|calc\([^)]*--slow/, "no motion multiplier variable");
for (const m of theme.matchAll(/font-weight:\s*(\d+)|font:\s*(\d{3})\s/g)) assert.ok(+(m[1]||m[2]) <= 700, `weight ${m[1]||m[2]} > 700`);
assert.doesNotMatch(theme, /box-shadow:[^;]*(rgba\(224,\s*39,\s*46|var\(--red\))/, "no red glow");
console.log(`check-theme: PASS (${PAGES.length} pages)`);
```

- [ ] **Step 2: Run and confirm failure**

Run: `node Tools/check-theme.mjs`
Expected: FAIL at `index: look-boot.js must load in <head>`.

- [ ] **Step 3: Create `look-boot.js`**

```js
// Before first paint: the look the player chose, from the last answer the core gave.
// Classic script on purpose (not a module) so it runs before the page paints.
// Glass defaults OFF until the core confirms it, so a window is never transparent with nothing behind it.
(function () {
  var look = { glass: false, slow: true };
  try { var c = JSON.parse(localStorage.getItem("reborn:look") || "null"); if (c && typeof c === "object") { look.glass = c.glass === true; look.slow = c.slow !== false; } } catch (e) {}
  var d = document.documentElement.dataset;
  d.glass = look.glass ? "on" : "off";
  d.motion = look.slow ? "slow" : "quick";
})();
```

- [ ] **Step 4: Create `theme.css`**

```css
/* The launcher's one design system. Spec: docs/superpowers/specs/2026-09-30-launcher-glass-redesign-design.md
   If a value is not in this file, it is not a decision anyone gets to make in a page stylesheet. */
@font-face{font-family:Inter;src:url("fonts/InterVariable.woff2") format("woff2");font-weight:100 900;font-display:block}

:root{
  --mica-tint:rgba(18,19,23,.66);
  --fallback-bg:radial-gradient(60% 60% at 50% 40%,#23262d 0,transparent 70%),#0d0e11;
  --glass-1:rgba(255,255,255,.045);--glass-2:rgba(255,255,255,.075);--glass-3:rgba(255,255,255,.11);
  --stroke:rgba(255,255,255,.09);--stroke-hi:rgba(255,255,255,.16);
  --sheen:inset 0 1px 0 rgba(255,255,255,.07);--shadow:0 12px 40px -12px rgba(0,0,0,.55);
  --fg:#F4F5F7;--fg-2:#AEB5C0;--fg-3:#7C8491;
  --red:#E0272E;--red-hi:#F04A50;--red-ink:#FF7A7F;--ok:#3DDC97;--warn:#FFC46B;
  --r-lg:14px;--r-md:10px;--r-sm:8px;
  --ease:cubic-bezier(.2,.8,.2,1);
  --d1:480ms;--d2:720ms;--d3:960ms;
  /* the old stylesheets' variables, pointed at the new tokens (retired in the Cleanup plan) */
  --bg:transparent;--panel:var(--glass-1);--line:var(--stroke);--muted:var(--fg-2);--accent:var(--red);
  --accent-border:var(--stroke-hi);--control-bg:var(--glass-2);--control-border:var(--stroke);
  --panel-border:var(--stroke);--selected-bg:var(--glass-3);--focus:#fff;--teal:var(--ok);
  --motion-ease:var(--ease);--motion-fast:var(--d1);--motion-normal:var(--d2);
}
html[data-motion="slow"]{--d1:480ms;--d2:720ms;--d3:960ms}
html[data-motion="quick"]{--d1:120ms;--d2:180ms;--d3:240ms}
@media (prefers-reduced-motion:reduce){
  html{--d1:0ms;--d2:0ms;--d3:0ms}
  *,*::before,*::after{animation:none!important;transition-duration:0s!important}
}

/* material: glass on = the OS material under our tint; off/unsupported = solid dark */
html,body{background:transparent}
html[data-glass="on"] body.shell{background:var(--mica-tint)!important}
html[data-glass="off"] body.shell{background:var(--fallback-bg)!important}
body.shell{color:var(--fg);font-family:Inter,system-ui,sans-serif}
body.shell .backdrop{display:none}
body.shell .app-header,body.shell .shell-nav,body.shell .input-footer{background:transparent!important}
body.shell .shell-nav{border-right:1px solid var(--stroke)!important}

/* sidebar: one sliding marker (look.js), no per-item marker */
body.shell .shell-nav{position:relative}
.shell-nav a:not(.brand){border-radius:var(--r-md)!important;color:var(--fg-2)!important;transition:background var(--d2) var(--ease),color var(--d2) var(--ease)}
.shell-nav a:not(.brand):hover{background:var(--glass-1)!important;color:var(--fg)!important}
.shell-nav a.current:not(.brand){background:var(--glass-3)!important;color:var(--fg)!important;box-shadow:var(--sheen),0 0 0 1px var(--stroke)}
.shell-nav a.current:not(.brand)::before{display:none!important}
.nav-marker{position:absolute;left:0;top:0;width:3px;height:18px;border-radius:0 3px 3px 0;background:var(--red);transition:transform var(--d3) var(--ease);pointer-events:none}
.nav-marker.still{transition:none}

/* page enter: content fades up 6px; the sidebar never moves */
@keyframes look-enter{from{opacity:0;transform:translateY(6px)}}
body.shell #main{animation:look-enter var(--d3) var(--ease) both}

/* buttons */
body.shell button,body.shell .button{font-family:inherit;border-radius:var(--r-md);transition:background var(--d1) var(--ease),border-color var(--d1) var(--ease),color var(--d1) var(--ease),transform var(--d1) var(--ease)}
body.shell button:active{transform:scale(.98)}
.btn{display:inline-flex;align-items:center;justify-content:center;gap:8px;height:38px;padding:0 16px;border-radius:var(--r-md);font:600 13px Inter;color:var(--fg);background:var(--glass-2);border:1px solid var(--stroke);box-shadow:var(--sheen);cursor:pointer}
.btn:hover{background:var(--glass-3);border-color:var(--stroke-hi)}
.btn.primary{background:linear-gradient(180deg,#ec3a41,#c91f26);border-color:rgba(255,255,255,.18);box-shadow:inset 0 1px 0 rgba(255,255,255,.22),0 6px 16px -8px rgba(0,0,0,.6)}
.btn.primary:hover{background:linear-gradient(180deg,#f4545a,#d6262d)}
.btn.quiet{background:transparent;box-shadow:none;color:var(--fg-2)}
.btn.sm{height:32px;padding:0 12px;font-size:12.5px}

/* focus: controller only (app-shell sets body.gamepad-input); keyboard keeps the browser default */
body.gamepad-input :focus-visible{outline:2px solid #fff!important;outline-offset:3px;box-shadow:none!important}

/* switch */
.look-switch{position:relative;width:40px;height:22px;border-radius:999px;border:0;padding:0;cursor:pointer;background:var(--red);transition:background var(--d2) var(--ease)}
.look-switch::after{content:"";position:absolute;left:3px;top:3px;width:16px;height:16px;border-radius:50%;background:#fff;transform:translateX(18px);transition:transform var(--d2) var(--ease)}
.look-switch[aria-checked="false"]{background:rgba(255,255,255,.14)}
.look-switch[aria-checked="false"]::after{transform:none;background:#cfd3da}
.look-switch:disabled{opacity:.45;cursor:not-allowed}

/* glass card */
.glass{background:var(--glass-1);border:1px solid var(--stroke);border-radius:var(--r-lg);box-shadow:var(--sheen),var(--shadow);backdrop-filter:blur(24px) saturate(1.3);-webkit-backdrop-filter:blur(24px) saturate(1.3)}

/* hint bar: only exists in controller mode (app-shell) */
body.shell .input-footer{border-top:1px solid var(--stroke)!important;color:var(--fg-3)}
body.shell:not(.gamepad-input) .input-footer #input-hints:empty{display:none}
.input-footer kbd{display:inline-grid;place-items:center;min-width:20px;height:18px;padding:0 5px;border-radius:5px;background:var(--glass-3);border:1px solid var(--stroke-hi);color:var(--fg);font:700 10px Inter}

/* save bar (savebar.js): its own lane, never over content, appears only when something changed */
.save-lane{flex:none;overflow:hidden;max-height:0;transition:max-height var(--d3) var(--ease)}
.save-lane.open{max-height:110px}
.save-bar{margin:10px 24px 18px;max-width:942px;display:flex;align-items:center;gap:16px;padding:12px 14px 12px 20px;border-radius:var(--r-lg);border:1px solid var(--stroke-hi);background:rgba(24,25,30,.78);backdrop-filter:blur(24px) saturate(1.3);box-shadow:var(--sheen),0 16px 40px -14px rgba(0,0,0,.75);opacity:0;transform:translateY(12px);transition:opacity var(--d3) var(--ease),transform var(--d3) var(--ease)}
.save-lane.open .save-bar{opacity:1;transform:none}
.save-bar .dot{width:8px;height:8px;border-radius:50%;background:var(--red)}
.save-bar .t{font-weight:600}.save-bar .d{color:var(--fg-2);font-size:12.5px}
.save-bar .btn:first-of-type{margin-left:auto}
```

- [ ] **Step 5: Create `look.js`**

```js
// The launcher's look: asks the core, sets the page, remembers for the next first paint.
import { bridge } from "./shell.js";

export function applyLook(view) {
  if (!view || typeof view !== "object") return;
  const glass = view.glassApplied === true, slow = view.slowAnimations !== false;
  const d = document.documentElement.dataset;
  d.glass = glass ? "on" : "off";
  d.motion = slow ? "slow" : "quick";
  try { localStorage.setItem("reborn:look", JSON.stringify({ glass, slow })); } catch (_) {}
}

export async function refreshLook() {
  if (!bridge) return null;
  try { const v = await bridge.invoke("launcher_look"); applyLook(v); return v; }
  catch (_) { return null; }
}

// One marker that glides to the current nav item (spec §5.2, §6.1).
export function mountNavMarker() {
  const nav = document.querySelector(".shell-nav");
  if (!nav || nav.querySelector(".nav-marker")) return;
  const marker = document.createElement("span");
  marker.className = "nav-marker still";
  marker.setAttribute("aria-hidden", "true");
  nav.append(marker);
  const place = () => {
    const a = nav.querySelector("a.current:not(.brand)");
    if (!a) { marker.style.opacity = "0"; return; }
    marker.style.opacity = "1";
    marker.style.transform = `translateY(${a.offsetTop + (a.offsetHeight - 18) / 2}px)`;
  };
  // Start where the previous page's marker was, then glide to this page's item.
  let from = null;
  try { from = sessionStorage.getItem("reborn:nav-marker"); } catch (_) {}
  if (from) marker.style.transform = from;
  requestAnimationFrame(() => {
    marker.classList.remove("still");
    place();
    try { sessionStorage.setItem("reborn:nav-marker", marker.style.transform); } catch (_) {}
  });
  addEventListener("resize", () => { marker.classList.add("still"); place(); requestAnimationFrame(() => marker.classList.remove("still")); });
}
```
At the end of `shell.js`, add:
```js
import("./look.js").then(m => { m.refreshLook(); m.mountNavMarker(); }).catch(() => {});
```
Use a dynamic import so `Tools/check-shell-pages.mjs`, which imports `shell.js` under a fake DOM, still loads; the promise's failure is swallowed.

- [ ] **Step 6: Wire every page**

For each of the 12 pages, in `<head>`:
- directly after the `<title>` line add `<script src="look-boot.js"></script>`
- after the last `<link rel="stylesheet" …>` add `<link rel="stylesheet" href="theme.css">`

Do it with a Python script written by the Write tool at `target/tmp/wire_theme.py`. It asserts each file has exactly one `</title>` and at least one stylesheet link, and is idempotent (skips a file that already has `look-boot.js`).

- [ ] **Step 7: Run the checkers**

Run: `node Tools/check-theme.mjs && node Tools/check-shell-pages.mjs && node Tools/check-page-call-args.mjs`
Expected: `check-theme: PASS (12 pages)`, and both existing checkers PASS.

- [ ] **Step 8: Screenshot every page (baseline for Adam)**

Run: `node Tools/run-browser-tests.mjs target/browser-review-foundation`
Expected: screenshots written.
- `review-finish`, `check-launch-polish` and `check-online-mm` already failed before this change on `release/3.0.27-beta-pbp`. Run them on `6ba10f7` first (`git stash`-free: `git worktree add ../tmp-base 6ba10f7`) to record the baseline, then compare. **Only new failures block.**
- Open the `reborn-launcher-*.png` shots. The pages must show the dark fallback (glass defaults off in a browser), a readable sidebar, and no layout shifts against the baseline shots.

- [ ] **Step 9: Commit**

```bash
git add crates/reborn-ui/ui/theme.css crates/reborn-ui/ui/look-boot.js crates/reborn-ui/ui/look.js crates/reborn-ui/ui/shell.js crates/reborn-ui/ui/*.html Tools/check-theme.mjs
git commit -m "ui: theme.css on every page, pre-paint look, sliding nav marker"
```

---

### Task 6: Settings › Display › Launcher window (+ the save bar)

**Files:**
- Create: `crates/reborn-ui/ui/savebar.js`, `Tools/check-look-browser.mjs`
- Modify: `crates/reborn-ui/ui/settings.js` (`draw()` ~line 354–360), `crates/reborn-ui/ui/settings.html` (add the save lane after `</main>`), `Tools/run-browser-tests.mjs`

**Interfaces:**
- Consumes: `applyLook`, `refreshLook` (Task 5); commands `launcher_look`, `launcher_look_set`, `window_glass_apply` (Task 4).
- Produces: `savebar.js`:
  - `export function mountSaveBar(lane: HTMLElement, { onSave: () => Promise<void>, onDiscard: () => Promise<void> }) → { update(changes: string[]): void }`
  - `update([])` closes the bar.
  - `update(['Window glass'])` opens it with "1 unsaved change" / "Window glass".
  - Later plans (Settings, Music) reuse this exact function.

- [ ] **Step 1: Write the failing browser check**

Create `Tools/check-look-browser.mjs`. It follows the server + stub pattern of `Tools/check-controller-navigation.mjs`:
- serve `crates/reborn-ui/ui`
- stub `window.__TAURI__.core.invoke`
- Edge headless

```js
import {chromium} from 'playwright';
import {createServer} from 'node:http';
import {readFileSync} from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
const root=path.resolve(import.meta.dirname,'..'),ui=path.join(root,'crates/reborn-ui/ui');
const types={'.js':'text/javascript','.css':'text/css','.svg':'image/svg+xml','.png':'image/png','.jpg':'image/jpeg','.woff2':'font/woff2','.html':'text/html'};
const server=createServer((req,res)=>{try{const u=new URL(req.url,'http://x');const f=path.resolve(ui,'.'+decodeURIComponent(u.pathname));if(!f.startsWith(ui+path.sep))throw 0;res.setHeader('Content-Type',types[path.extname(f)]||'application/octet-stream');res.end(readFileSync(f));}catch{res.writeHead(404).end();}});
await new Promise(r=>server.listen(0,'127.0.0.1',r));
const base=`http://127.0.0.1:${server.address().port}`;
const browser=await chromium.launch({channel:process.env.REBORN_TEST_BROWSER||'msedge',headless:true});
try{
 const ctx=await browser.newContext({viewport:{width:1280,height:800}});
 await ctx.addInitScript(()=>{
  window.saved={windowGlass:true,slowAnimations:true};window.calls=[];
  const view=(glass,slow)=>({windowGlass:saved.windowGlass,slowAnimations:slow,glassSupported:true,glassApplied:glass,glassNote:null});
  window.__TAURI__={core:{invoke:async(cmd,args={})=>{calls.push({cmd,args});
   if(cmd==='launcher_look')return view(saved.windowGlass,saved.slowAnimations);
   if(cmd==='window_glass_apply')return view(args.enabled,saved.slowAnimations);
   if(cmd==='launcher_look_set'){if(args.windowGlass!==null&&args.windowGlass!==undefined)saved.windowGlass=args.windowGlass;if(args.slowAnimations!==null&&args.slowAnimations!==undefined)saved.slowAnimations=args.slowAnimations;return view(saved.windowGlass,saved.slowAnimations);}
   return null;}}};
 });
 const page=await ctx.newPage();const errors=[];page.on('pageerror',e=>errors.push(e.message));
 await page.goto(base+'/settings.html#display');
 await page.waitForFunction(()=>document.documentElement.dataset.glass==='on');
 const dur=()=>page.evaluate(()=>getComputedStyle(document.querySelector('.nav-marker')).transitionDuration);
 assert.equal(await dur(),'0.96s','slow is the default');
 // Glass: preview off, then discard restores it and hides the bar
 await page.getByRole('switch',{name:'Window glass'}).click();
 assert.equal(await page.evaluate(()=>document.documentElement.dataset.glass),'off','glass previews off');
 await page.locator('.save-lane.open').waitFor();
 assert.match(await page.locator('.save-bar').innerText(),/1 unsaved change[\s\S]*Window glass/);
 await page.getByRole('button',{name:'Discard'}).click();
 assert.equal(await page.evaluate(()=>document.documentElement.dataset.glass),'on','discard restores glass');
 await page.waitForFunction(()=>!document.querySelector('.save-lane.open'));
 // Motion: switch off and save -> quick durations, persisted
 await page.getByRole('switch',{name:'Slow animations'}).click();
 assert.equal(await page.evaluate(()=>document.documentElement.dataset.motion),'quick','motion previews quick');
 await page.getByRole('button',{name:'Save changes'}).click();
 await page.waitForFunction(()=>!document.querySelector('.save-lane.open'));
 assert.equal(await page.evaluate(()=>saved.slowAnimations),false,'saved');
 assert.equal(await page.evaluate(()=>JSON.parse(localStorage.getItem('reborn:look')).slow),false,'cached for first paint');
 await page.reload();await page.waitForFunction(()=>document.documentElement.dataset.motion==='quick');
 assert.equal(await dur(),'0.24s','quick survives reload');
 // Reduced motion wins
 await page.emulateMedia({reducedMotion:'reduce'});
 assert.equal(await dur(),'0s','reduced motion');
 // A broken cache never breaks the page (Review Focus 4)
 await page.evaluate(()=>localStorage.setItem('reborn:look','{nope'));
 await page.reload();await page.waitForFunction(()=>document.documentElement.dataset.glass==='on');
 assert.deepEqual(errors,[]);
 console.log('check-look-browser: PASS');
}finally{await browser.close();server.close();}
```

- [ ] **Step 2: Run and confirm failure**

Run: `node Tools/check-look-browser.mjs`
Expected: FAIL. No `switch` named "Window glass" (timeout on `getByRole`).

- [ ] **Step 3: Create `savebar.js`**

```js
// The one save bar (spec §7). It lives in its own lane at the bottom of the page, so it never covers content.
import { el } from "./shell.js";

export function mountSaveBar(lane, { onSave, onDiscard }) {
  lane.classList.add("save-lane");
  const title = el("div", { class: "t" }), detail = el("div", { class: "d" });
  const discard = el("button", { type: "button", class: "btn", text: "Discard" });
  const save = el("button", { type: "button", class: "btn primary", text: "Save changes" });
  let busy = false;
  const run = (fn) => async () => { if (busy) return; busy = true; discard.disabled = save.disabled = true;
    try { await fn(); } finally { busy = false; discard.disabled = save.disabled = false; } };
  discard.addEventListener("click", run(onDiscard));
  save.addEventListener("click", run(onSave));
  lane.replaceChildren(el("div", { class: "save-bar", role: "region", "aria-label": "Unsaved changes" },
    el("span", { class: "dot", "aria-hidden": "true" }), el("div", {}, title, detail), discard, save));
  return {
    update(changes) {
      const n = changes.length;
      title.textContent = `${n} unsaved change${n === 1 ? "" : "s"}`;
      detail.textContent = changes.at(-1) || "";
      lane.classList.toggle("open", n > 0);
    },
  };
}
```

- [ ] **Step 4: Add the save lane to `settings.html` and the section to `settings.js`**

In `settings.html`, directly after `</main>` add: `<div id="save-lane"></div>`. Then add a rule to `theme.css` so the lane sits at the bottom of the content column:

```css
body.page-settings{display:grid}
body.page-settings #save-lane{grid-column:2;align-self:end}
```

If the page's existing grid areas differ, read `launcher.css:1-8` (`body.shell` grid template) and give `#save-lane` the same column as `#main`. Confirm in the Step 7 screenshot that the lane sits under `#main` and never overlaps it.

In `settings.js`, add the imports at the top:
```js
import { applyLook, refreshLook } from './look.js';
import { mountSaveBar } from './savebar.js';
```
Add the module state and section builder above `function draw()`:
```js
// Launcher window (spec §4.3, §5.3): previews live, saved through the save bar.
let look = null, lookSaved = null, lookDraft = null, saveBar = null;
const lookChanges = () => !lookSaved || !lookDraft ? [] : [
  ...(lookDraft.windowGlass !== lookSaved.windowGlass ? ['Window glass'] : []),
  ...(lookDraft.slowAnimations !== lookSaved.slowAnimations ? ['Slow animations'] : []),
];
function ensureSaveBar() {
  if (saveBar) return;
  const lane = document.getElementById('save-lane'); if (!lane) return;
  saveBar = mountSaveBar(lane, {
    onSave: async () => { look = await call('launcher_look_set', { windowGlass: lookDraft.windowGlass, slowAnimations: lookDraft.slowAnimations });
      applyLook(look); lookSaved = { windowGlass: look.windowGlass, slowAnimations: look.slowAnimations }; lookDraft = { ...lookSaved }; saveBar.update([]); draw(); },
    onDiscard: async () => { lookDraft = { ...lookSaved };
      look = await call('window_glass_apply', { enabled: lookSaved.windowGlass });
      applyLook({ ...look, slowAnimations: lookSaved.slowAnimations }); saveBar.update([]); draw(); },
  });
}
function lookSwitch(label, on, disabled, flip) {
  return el('button', { type: 'button', class: 'look-switch', role: 'switch', 'aria-label': label, 'aria-checked': String(on), disabled, onclick: flip });
}
function lookBlock() {
  if (!look) return el('p', { class: 'partial', text: 'Reading the launcher window settings…' });
  ensureSaveBar();
  const glassOn = lookDraft.windowGlass && look.glassSupported;
  const row = (label, sw, help) => el('div', { class: 'look-row' }, el('div', { class: 'look-label', text: label }), sw, el('p', { class: 'look-help', text: help }));
  return el('section', { class: 'glass look-section', 'aria-label': 'Launcher window' },
    el('h2', { text: 'Launcher window' }),
    el('p', { class: 'look-lead', text: 'How the launcher itself looks. The game is not affected.' }),
    row('Window glass', lookSwitch('Window glass', glassOn, !look.glassSupported, async () => {
      lookDraft.windowGlass = !lookDraft.windowGlass;
      look = await call('window_glass_apply', { enabled: lookDraft.windowGlass });
      applyLook({ ...look, slowAnimations: lookDraft.slowAnimations }); saveBar?.update(lookChanges()); draw(); }),
      look.glassNote || 'Frosted glass that picks up your desktop colours. Turn it off for a solid dark background.'),
    row('Slow animations', lookSwitch('Slow animations', lookDraft.slowAnimations, false, () => {
      lookDraft.slowAnimations = !lookDraft.slowAnimations;
      applyLook({ glassApplied: document.documentElement.dataset.glass === 'on', slowAnimations: lookDraft.slowAnimations });
      saveBar?.update(lookChanges()); draw(); }),
      'Slower, smoother movement when you change pages, open menus and expand matches. Turn it off for quick animations. Windows\u2019 reduce-motion setting always turns movement off.'));
}
async function loadLook() {
  look = await refreshLook();
  if (look) { lookSaved = { windowGlass: look.windowGlass, slowAnimations: look.slowAnimations }; lookDraft = { ...lookSaved }; }
  draw();
}
loadLook();
```
In `draw()`, change the tab branch so Display shows the section above the Dolphin boxes:
```js
 if(settingsTab==='Updates')parts.push(updatesBlock());else if(settingsTab==='Graphics')parts.push(graphicsBlock());else if(settingsTab==='Files & storage')parts.push(repairBlock(),unverifiedChoice(),boxesBlock(),foundBlock(),whereBlock());else if(settingsTab==='Display')parts.push(lookBlock(),dolphinBlock());else parts.push(dolphinBlock());
```
`call` is already imported by `settings.js` from `shell.js`. Check with `grep -n "^import" crates/reborn-ui/ui/settings.js` and add it to the import if it's missing.

Add the section's layout to `theme.css`:
```css
.look-section{padding:6px 0;margin:16px 0}
.look-section h2{font:600 15px Inter;padding:16px 22px 2px}
.look-lead{color:var(--fg-2);font-size:12.5px;padding:0 22px 10px}
.look-row{display:grid;grid-template-columns:150px 1fr auto;gap:8px 16px;align-items:center;padding:14px 22px;border-top:1px solid var(--stroke)}
.look-label{font-weight:600;font-size:13px}
.look-help{grid-column:1/-1;font-size:12px;color:var(--fg-3);margin:0}
```

- [ ] **Step 5: Run the checks**

Run: `node Tools/check-look-browser.mjs && node Tools/check-shell-pages.mjs && node Tools/check-page-call-args.mjs && node Tools/check-theme.mjs`
Expected: all PASS. `check-page-call-args` now sees `launcher_look_set {windowGlass, slowAnimations}` and `window_glass_apply {enabled}`, which match Rust's `window_glass`, `slow_animations` and `enabled` through Tauri's camelCase conversion.

- [ ] **Step 6: Add the new check to the suite**

In `Tools/run-browser-tests.mjs`, after `await import('./check-controller-navigation.mjs');` (line ~325), add `await import('./check-look-browser.mjs');`.

- [ ] **Step 7: Screenshot and commit**

Run: `node Tools/run-browser-tests.mjs target/browser-review-foundation`, then open the settings screenshot. Also take one manually with `page.goto(base+'/settings.html#display')` in a scratch copy of the check with the lane open, and look at it.
```bash
git add crates/reborn-ui/ui/savebar.js crates/reborn-ui/ui/settings.js crates/reborn-ui/ui/settings.html crates/reborn-ui/ui/theme.css Tools/check-look-browser.mjs Tools/run-browser-tests.mjs
git commit -m "settings: Launcher window (glass, slow animations) with live preview and the save bar"
```

---

### Task 7: Controller prompts only while a controller is driving

**Files:**
- Modify: `crates/reborn-ui/ui/app-shell.js` (`renderHints()` ~line 80–100, input listeners ~line 251–255)
- Create: `Tools/check-controller-hints.mjs`
- Modify: `Tools/run-browser-tests.mjs`

**Interfaces:**
- Consumes: the existing `controllerMode`, `mode(value)`, `connectedPads()`, `readLayout()` in `app-shell.js`.
- Produces: the rule "`#input-hints` has children only if `controllerMode` is true, **or** the connected pad needs setup (non-standard mapping with no saved layout)".

- [ ] **Step 1: Write the failing check**

Create `Tools/check-controller-hints.mjs`, using the same harness technique as `check-controller-navigation.mjs` (fake `navigator.getGamepads`, manual `requestAnimationFrame`):

```js
import {chromium} from 'playwright';
import {createServer} from 'node:http';
import {readFileSync} from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
const root=path.resolve(import.meta.dirname,'..'),ui=path.join(root,'crates/reborn-ui/ui');
const html=`<!doctype html><html><head></head><body class="shell"><nav class="shell-nav"><a href="index.html">Play</a></nav><main id="main"><button id="first">First</button><button id="second">Second</button></main><footer class="input-footer"><span id="input-hints"></span></footer><script type="module" src="app-shell.js"></script></body></html>`;
const types={'.js':'text/javascript','.css':'text/css','.html':'text/html'};
const server=createServer((req,res)=>{try{const u=new URL(req.url,'http://x');if(u.pathname==='/'){res.setHeader('Content-Type','text/html');res.end(html);return;}
 const f=path.resolve(ui,'.'+decodeURIComponent(u.pathname));if(!f.startsWith(ui+path.sep))throw 0;res.setHeader('Content-Type',types[path.extname(f)]||'application/octet-stream');res.end(readFileSync(f));}catch{res.writeHead(404).end();}});
await new Promise(r=>server.listen(0,'127.0.0.1',r));
const browser=await chromium.launch({channel:process.env.REBORN_TEST_BROWSER||'msedge',headless:true});
try{
 const page=await browser.newPage();const errors=[];page.on('pageerror',e=>errors.push(e.message));
 await page.addInitScript(()=>{
  window.pads=[{id:'Test pad',index:0,connected:true,mapping:'standard',axes:[0,0,0,0],buttons:Array.from({length:18},()=>({pressed:false,value:0}))}];
  Object.defineProperty(navigator,'getGamepads',{value:()=>pads});
  document.hasFocus=()=>true;
  window.__TAURI__={core:{invoke:async cmd=>cmd==='session_active'?false:null}};
  window.frames=[];window.clock=0;window.requestAnimationFrame=fn=>(frames.push(fn),frames.length);window.cancelAnimationFrame=()=>{};
  window.tick=()=>{clock+=16;frames.splice(0).forEach(fn=>fn(clock));};
 });
 await page.goto(`http://127.0.0.1:${server.address().port}`);await page.waitForFunction(()=>frames.length>0);
 const hints=()=>page.evaluate(()=>document.getElementById('input-hints').children.length);
 const gamepad=()=>page.evaluate(()=>document.body.classList.contains('gamepad-input'));
 await page.evaluate(()=>tick());
 assert.equal(await hints(),0,'pad connected, mouse user: no hints');
 await page.evaluate(()=>{pads[0].buttons[13]={pressed:true,value:1};tick();pads[0].buttons[13]={pressed:false,value:0};tick();});
 assert.ok(await hints()>0,'pad input: hints appear');assert.equal(await gamepad(),true);
 await page.mouse.move(200,200);await page.mouse.move(260,240);
 assert.equal(await hints(),0,'mouse move: hints go');assert.equal(await gamepad(),false);
 await page.evaluate(()=>{pads[0].buttons[13]={pressed:true,value:1};tick();pads[0].buttons[13]={pressed:false,value:0};tick();});
 await page.keyboard.press('a');
 assert.equal(await hints(),0,'any key: hints go');
 // Review Focus 1: an unmapped pad must still say how to set it up
 await page.evaluate(()=>{pads[0].mapping='';pads[0].id='Unknown USB pad';tick();});
 assert.match(await page.locator('#input-hints').innerText(),/Set up launcher controls/,'unmapped pad still offered setup');
 assert.deepEqual(errors,[]);
 console.log('check-controller-hints: PASS');
}finally{await browser.close();server.close();}
```

- [ ] **Step 2: Run and confirm failure**

Run: `node Tools/check-controller-hints.mjs`
Expected: FAIL at `pad connected, mouse user: no hints` (today the hints render as soon as a pad is connected).

- [ ] **Step 3: Implement in `app-shell.js`**

In `renderHints()`, after `const needsSetup = …`, insert:
```js
  // Prompts are for whoever is holding the controller (spec §6.3). A pad the launcher
  // cannot read yet still says how to set it up, or it could never be set up.
  if (!controllerMode && !(needsSetup && !testing)) { hints.replaceChildren(); delete hints.dataset.layout; return; }
```
Replace the `keydown` and `pointerdown` listeners (lines ~251–255) with:
```js
document.addEventListener('keydown',e => {
  if (e.key === 'Escape') { e.preventDefault(); back(); }
  if (e.key === 'Tab') endSelect(false);
  mode(false);
});
document.addEventListener('pointerdown',() => { endSelect(false); mode(false); });
// A real mouse move (not the synthetic one a scroll under a still cursor fires) leaves controller mode.
document.addEventListener('mousemove',e => { if (controllerMode && (Math.abs(e.movementX) + Math.abs(e.movementY)) > 2) mode(false); });
```

- [ ] **Step 4: Run the new check and the existing controller checks**

Run: `node Tools/check-controller-hints.mjs && node Tools/check-controller-navigation.mjs && node Tools/check-play-controller-gate.mjs`
Expected: all PASS. If `check-controller-navigation` relied on hints showing without pad input, update only that assertion, and say so in the commit message.

- [ ] **Step 5: Add to the suite and commit**

In `Tools/run-browser-tests.mjs`, after `await import('./check-controller-navigation.mjs');` add `await import('./check-controller-hints.mjs');`.
```bash
git add crates/reborn-ui/ui/app-shell.js Tools/check-controller-hints.mjs Tools/run-browser-tests.mjs
git commit -m "shell: controller prompts only while a controller is driving; unmapped pads still offered setup"
```

---

### Task 8: Whole-branch verification and the real window

**Files:** none new, unless a check fails.

- [ ] **Step 1: Full Rust suite**

Run (worktree root): `taskkill /IM reborn-launcher.exe /F 2>NUL & cargo test --workspace -j 2 && cargo clippy --workspace --all-targets`
Expected: all tests pass (count ≥ the baseline on `6ba10f7` + 9 new). No new clippy warnings.

- [ ] **Step 2: All JS checks**

Run:
```bash
node --test crates/reborn-ui/ui/stats-model.test.mjs crates/reborn-ui/ui/stats-rows.test.mjs crates/reborn-ui/ui/cover-color.test.mjs
node Tools/check-shell-pages.mjs && node Tools/check-page-call-args.mjs && node Tools/check-theme.mjs
node Tools/run-browser-tests.mjs target/browser-review-foundation
```
Expected: PASS, except any check that failed on the Task 5 Step 8 baseline. List those by name in the handoff.

- [ ] **Step 3: The real launcher on this PC**

`cargo build --workspace`, then run `target/debug/reborn-launcher.exe` (dev build, not the installed beta). Check and screenshot to `target/foundation-real/`:
1. Mica shows. Every page reads well. The sidebar marker glides between pages, slowly.
2. Settings › Display: Window glass off → solid dark window immediately, save bar appears. Discard → glass back. Off + Save, restart the app → it starts dark, with no glass flash.
3. Slow animations off + Save → quick. Restart → still quick.
4. Pad plugged in, mouse used → no hints. Press the D-pad → hints + white focus ring. Move the mouse → gone.
5. Play now still launches the game.

- [ ] **Step 4: Hand off to Adam (GATE)**

Report to Adam:
- the screenshots
- the check results with numbers
- the list of baseline failures that were already there

**Do not build or publish a beta installer without his go.** When he says go, the beta build is `installer/windows/build.ps1 -Channel beta -Version <next beta> -Runtime <dir>` (see `project_reborn_update_system` in memory).
