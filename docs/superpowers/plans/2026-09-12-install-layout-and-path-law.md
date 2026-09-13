# Install Layout and Path Law Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** One data root per channel named for the product in AppData Roaming, a 2.1 install tucked aside on first start, textures composed straight into each Dolphin user folder, a Dolphin.ini with no path keys, and a user-folder template staged from the tuned reference folder.

**Architecture:** A new core module `layout` runs before the supervisor lock and moves things by rename only (tuck a 2.1 install into `Reborn 2.1\`, adopt a `Launcher`-era root). `mods` composes into `<user>\Load\Textures\<game>` and records the pristine tree's identity so a texture update always rebuilds. `dolphin_preferences` removes path keys instead of rewriting them. `user_template` stages and sanitizes the reference User folder for `build.ps1`. The installer's names and safety checks follow.

**Tech Stack:** Rust workspace (`reborn-core` forbids `unsafe`, no `#[cfg(windows)]` outside `platform/`), Tauri 2 window (`reborn-ui`), `rebornctl` CLI (`reborn-cli`), Inno Setup 6 (`installer/windows/reborn-launcher.iss`), PowerShell 5.1 build script, Playwright over the WebView2 debugging port for real-window proof.

**Spec:** `docs/superpowers/specs/2026-09-12-install-layout-and-path-law-design.md`

## Global Constraints

- Work in the worktree `target\paths-tree` on branch `paths-overhaul` (off `integration`). Never `git add -A`; add named files only.
- Stable data root `%APPDATA%\NFL Street Reborn`, beta `%APPDATA%\NFL Street Reborn Beta`, program in `app\` inside each. Cache root unchanged.
- Installer output file names, AppIds, `latest.json` names, the Publisher, the HTTP user agent string `NFL Street Reborn Launcher/<v>` are unchanged.
- Nothing is ever deleted by tuck or adopt: rename only, same volume, collisions skipped and logged.
- `reborn-core` rules: no `unsafe`, no `#[cfg(windows)]` outside `platform/`, no `std::process::Command` outside `platform/` (an existing exception is `runtime::ensure_runtime_idle`; do not add more).
- Player-facing sentences: no paths, no URLs, no version numbers (`lib.rs::player_sentence::assert_player_safe`).
- Multi-line patch scripts are written with the Write tool as files, never Bash heredocs (heredocs ate backslashes before).
- Before `cargo test`, kill any running `reborn-launcher.exe` / `rebornctl.exe` (they lock their binaries). `cargo test --workspace` runs from `target\paths-tree`; the first build takes minutes.
- Commit message trailer on every commit:
  ```
  Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
  Claude-Session: https://claude.ai/code/session_01HnpEHG4zu2fw1bjBRsQ26w
  ```

---

## File map

| File | Responsibility |
|---|---|
| `crates/reborn-core/src/platform/imp.rs` | Root names per platform; `previous_data_root` |
| `crates/reborn-core/src/platform/mod.rs` | `Paths` trait (adds `previous_data_root`), `DataDir` (drops `ModsActive`) |
| `crates/reborn-core/src/layout.rs` (new) | Tuck a 2.1 install, adopt a `Launcher`-era root; the top-level name set |
| `crates/reborn-core/src/lib.rs` | `pub mod layout; pub mod user_template;`, layout tests |
| `crates/reborn-core/src/supervisor.rs` | Refusal wording when a 2.1 install could not be tucked |
| `crates/reborn-core/src/ini.rs` | `IniDocument::remove` |
| `crates/reborn-core/src/dolphin_preferences.rs` | `path_law_overlay`, `apply_path_law` (replaces `resolve_portable_paths`) |
| `crates/reborn-core/src/session.rs` | Calls the path law; `memory_card_overlay` without a path; sync request field |
| `crates/reborn-core/src/mods.rs` | Composed root in the user folder; transition; pristine identity; no junction |
| `crates/reborn-core/src/migration.rs` | `copy_tree_excluding` |
| `crates/reborn-core/src/user_template.rs` (new) | Stage and sanitize the reference User folder |
| `crates/reborn-core/src/runtime.rs` | Seed the whole template tree |
| `crates/reborn-ui/src/main.rs` | `layout::prepare_data_root` at start; legacy root default; sync view; `mods_unapply` |
| `crates/reborn-ui/src/setup_tools.rs` | Migration copies no textures |
| `crates/reborn-cli/src/main.rs` | `layout::prepare_data_root` at start; `runtime template`; `mods link` removed |
| `installer/windows/reborn-launcher.iss` | DataDir names, display names, safety check, previous-install cleanup |
| `installer/windows/build.ps1` | `-UserTemplate` |
| `installer/beta/update-review.ps1`, `installer/beta/sandbox-smoke.ps1` | Folder names in strings |
| `docs/architecture/SYSTEM_ARCHITECTURE.md`, `docs/architecture/CONTENT_AND_UPDATE_MODEL.md` | Layout sections |
| `Tools/walk-paths-overhaul.ps1` (new), `Tools/drive-paths-walk.mjs` (new) | Real-window proof |

---

### Task 1: Roots named for the product, and the previous root

**Files:**
- Modify: `crates/reborn-core/src/platform/imp.rs:82-103` (constants), `:156-180` (`data_root`)
- Modify: `crates/reborn-core/src/platform/mod.rs:568-586` (`Paths` trait)
- Modify: `crates/reborn-core/src/lib.rs:697-760` (three tests)
- Modify: `crates/reborn-core/src/xbox.rs:2290` (fixture name only)
- Test: `crates/reborn-core/src/platform/imp.rs` tests module (`a_beta_keeps_every_root_apart_from_stable` stays)

**Interfaces:**
- Produces: `Paths::previous_data_root(&self) -> Result<Option<PathBuf>>` (default `Ok(None)`; `RealPaths` returns `<parent of data_root>\NFL Street Reborn Launcher<suffix>` on every platform).
- Produces: data root = `<base>\NFL Street Reborn<suffix>` on every platform.

- [ ] **Step 1: Write the failing tests** in `crates/reborn-core/src/platform/imp.rs` tests module, next to `a_beta_keeps_every_root_apart_from_stable`:

```rust
    #[test]
    fn the_data_root_is_the_product_name_in_the_platform_data_folder() {
        let stable = RealPaths { channel: Channel::Stable }.data_root().unwrap();
        let beta = RealPaths { channel: Channel::Beta }.data_root().unwrap();
        assert_eq!(stable.file_name().unwrap().to_string_lossy(), "NFL Street Reborn");
        assert_eq!(beta.file_name().unwrap().to_string_lossy(), "NFL Street Reborn Beta");
        assert_eq!(stable.parent(), beta.parent());
    }

    #[test]
    fn the_previous_data_root_is_the_launcher_era_folder_beside_the_new_one() {
        for (channel, name) in [(Channel::Stable, "NFL Street Reborn Launcher"), (Channel::Beta, "NFL Street Reborn Launcher Beta")] {
            let paths = RealPaths { channel };
            let data = paths.data_root().unwrap();
            let previous = paths.previous_data_root().unwrap().expect("a previous root is always named");
            assert_eq!(previous.file_name().unwrap().to_string_lossy(), name);
            assert_eq!(previous.parent(), data.parent());
        }
    }
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p reborn-core the_data_root_is_the_product_name the_previous_data_root_is -- --nocapture`
Expected: FAIL (`file_name` is `NFL Street Reborn Launcher`; `previous_data_root` not found).

- [ ] **Step 3: Implement**

In `platform/mod.rs`, inside `pub trait Paths`, after `fn cache_root`:

```rust
    /// The data root a build before 2026-09-12 used (`... Launcher[ Beta]`),
    /// so a first start under the new name can adopt what is there. `None`
    /// where no such folder was ever used (test fakes).
    fn previous_data_root(&self) -> Result<Option<PathBuf>> {
        Ok(None)
    }
```

In `imp.rs`: delete `const PRODUCT_DATA` and its whole doc comment (lines 85-103). Replace the comment block with:

```rust
/// The data root is `PRODUCT` plus the channel suffix, in the platform's
/// per-user data folder. On Windows that is `%APPDATA%\NFL Street Reborn`,
/// which the 2.1 build also used: a 2.1 install found there is moved into a
/// `Reborn 2.1` subfolder by `crate::layout` before the launcher writes
/// anything, so the launcher's `mods` can never be JSGME's `MODS`.
/// Builds before 2026-09-12 used `NFL Street Reborn Launcher[ Beta]`; see
/// `previous_data_root`.
const PREVIOUS_DATA_SUFFIX: &str = " Launcher";
```

Replace every `PRODUCT_DATA` in `data_root` (three platforms) with `PRODUCT`. Fix the Windows comment (`// Not %APPDATA%\NFL Street Reborn: ...`) to `// %APPDATA%\NFL Street Reborn. A 2.1 install there is tucked aside at start; see crate::layout.` Then add to `impl Paths for RealPaths`:

```rust
    fn previous_data_root(&self) -> Result<Option<PathBuf>> {
        let data = self.data_root()?;
        let Some(parent) = data.parent() else { return Ok(None) };
        Ok(Some(parent.join(format!("{PRODUCT}{PREVIOUS_DATA_SUFFIX}{}", self.channel.suffix()))))
    }
```

In `lib.rs` tests: delete `the_data_root_is_not_the_legacy_install_folder` and `the_real_data_root_is_not_a_legacy_install` entirely (their doc comments too). Keep `data_root_is_absolute_and_named_for_the_product`. In `xbox.rs:2290` change the fixture folder name to `"NFL Street Reborn"` (it is a guard-list fixture; the name is incidental).

- [ ] **Step 4: Run the core tests**

Run: `cargo test -p reborn-core platform:: lib::` then `cargo test -p reborn-core`
Expected: PASS except `supervisor::a_legacy_install_is_refused_as_a_data_root` still passes (unchanged) and nothing else regresses. If `the_beta_data_root_sits_beside_the_stable_one_and_the_2_1_install` fails, keep it: it asserts parents are equal, which still holds.

- [ ] **Step 5: Commit**

```
git add crates/reborn-core/src/platform/imp.rs crates/reborn-core/src/platform/mod.rs crates/reborn-core/src/lib.rs crates/reborn-core/src/xbox.rs
git commit -m "The data root is the product name: NFL Street Reborn, NFL Street Reborn Beta"
```

---

### Task 2: `IniDocument::remove`

**Files:**
- Modify: `crates/reborn-core/src/ini.rs` (after `merge`, ~line 185)
- Test: same file, tests module

**Interfaces:**
- Produces: `pub fn remove(&mut self, section: &str, key: &str) -> bool` (true when a pair was removed; a document with no such key is untouched and renders byte for byte).

- [ ] **Step 1: Failing test**

```rust
    #[test]
    fn remove_takes_one_key_out_and_leaves_everything_else_byte_for_byte() {
        let text = "[General]\r\nLoadPath = User/Load///\r\nGDBPort = -1\r\n[Core]\r\nSlotA = 255\r\n";
        let mut doc = IniDocument::parse(text);
        assert!(doc.remove("General", "LoadPath"));
        assert!(!doc.remove("General", "LoadPath"), "gone already");
        assert!(!doc.remove("Nope", "GDBPort"), "wrong section");
        assert_eq!(doc.render(), "[General]\r\nGDBPort = -1\r\n[Core]\r\nSlotA = 255\r\n");
        let untouched = IniDocument::parse(text);
        assert_eq!(untouched.render(), text);
    }
```

- [ ] **Step 2: Run** `cargo test -p reborn-core ini::remove_takes` → FAIL (no method `remove`).

- [ ] **Step 3: Implement** in `impl IniDocument`, after `merge`:

```rust
    /// Take one key out of one section. Everything else, comments and blank
    /// lines included, stays exactly as it was. Returns whether a pair was
    /// removed.
    pub fn remove(&mut self, section: &str, key: &str) -> bool {
        let mut current: Option<String> = None;
        let mut index = None;
        for (i, line) in self.lines.iter().enumerate() {
            match line {
                Line::Section { name, .. } => current = Some(name.clone()),
                Line::Pair { key: k, .. } if current.as_deref() == Some(section) && k == key => {
                    index = Some(i);
                    break;
                }
                _ => {}
            }
        }
        match index {
            Some(i) => {
                self.lines.remove(i);
                true
            }
            None => false,
        }
    }
```

Check the `Line::Pair` variant's field names at the top of `ini.rs` (`key`, `raw`, `sep`, maybe `value`); match what exists.

- [ ] **Step 4: Run** `cargo test -p reborn-core ini::` → PASS.

- [ ] **Step 5: Commit** `git add crates/reborn-core/src/ini.rs && git commit -m "IniDocument::remove"`

---

### Task 3: The path law for Dolphin.ini

**Files:**
- Modify: `crates/reborn-core/src/dolphin_preferences.rs:39-112` and its tests `:113-160`
- Modify: `crates/reborn-core/src/session.rs:577` (call site), `:1082-1105` (`memory_card_overlay`) and its test `:1597-1625`

**Interfaces:**
- Produces: `pub fn path_law_removals(doc: &IniDocument, user: &Path) -> Vec<(String, String)>` (section, key pairs to remove) and `pub fn apply_path_law(user: &Path) -> Result<()>` (reads `<user>/Config/Dolphin.ini`, removes, writes back only when something changed, keeps a one-time `Dolphin.ini.before-path-law` copy).
- Removes: `resolve_portable_paths`, `portable_path_overlay`.
- Changes: `session::memory_card_overlay` sets `SlotA = 1` and never writes `MemcardAPath` unless keeping an existing one that is outside the user folder and exists.

- [ ] **Step 1: Failing tests** (replace the `portable_path_tests` module body):

```rust
#[cfg(test)] mod path_law_tests {
 use super::*;
 fn user()->std::path::PathBuf{ std::path::PathBuf::from(r"C:\Users\Adam\AppData\Roaming\NFL Street Reborn\dolphin-users\fresh-1") }
 fn keys(removed:&[(String,String)])->Vec<String>{ removed.iter().map(|(s,k)|format!("{s}/{k}")).collect() }
 #[test] fn paths_inside_this_or_another_dolphin_user_folder_are_removed(){
  let doc=crate::ini::IniDocument::parse("[General]\nLoadPath = C:/Users/Adam/AppData/Roaming/NFL Street Reborn/User/Load\nDumpPath = C:\\Users\\Adam\\AppData\\Roaming\\NFL Street Reborn\\dolphin-users\\fresh-1\\Dump\nResourcePackPath = D:/Packs\nWiiSDCardPath = None\n[Core]\nMemcardAPath = C:/Users/Adam/AppData/Roaming/NFL Street Reborn/User/GC/MemoryCardA.USA.raw\nMemcardBPath = D:/Backups/User/Cards/MemoryCardB.USA.raw\n");
  let removed=keys(&path_law_removals(&doc,&user()));
  assert_eq!(removed,["General/LoadPath","General/DumpPath","Core/MemcardAPath"]);
 }
 #[test] fn relative_paths_are_removed_and_foreign_absolute_ones_kept(){
  let doc=crate::ini::IniDocument::parse("[General]\nLoadPath = User/Load///\nDumpPath = .\\User\\Dump\\\nResourcePackPath = D:/Packs\n[Core]\nMemcardBPath = User/GC/Card.raw\nSlotA = 255\n[GBA]\nBIOS = None\nRom1 = D:/roms/a.gba\n");
  let removed=keys(&path_law_removals(&doc,&user()));
  assert_eq!(removed,["General/LoadPath","General/DumpPath","Core/MemcardBPath"]);
 }
 #[test] fn the_default_iso_and_iso_paths_are_always_removed(){
  let doc=crate::ini::IniDocument::parse("[General]\nISOPaths = 1\nISOPath0 = Games\n[Core]\nDefaultISO = C:\\Users\\Adam\\AppData\\Roaming\\NFL Street Reborn\\Games\\NSRV3.iso\n");
  let removed=keys(&path_law_removals(&doc,&user()));
  assert_eq!(removed,["General/ISOPaths","General/ISOPath0","Core/DefaultISO"]);
 }
 #[test] fn an_account_folder_called_user_is_not_a_dolphin_user_folder(){
  let doc=crate::ini::IniDocument::parse("[General]\nLoadPath = C:/Users/User/Documents/Dolphin Emulator/Load\n[Core]\nMemcardAPath = C:/Users/User/Documents/Dolphin Emulator/GC/MemoryCardA.USA.raw\n");
  assert!(path_law_removals(&doc,&user()).is_empty());
 }
 #[test] fn apply_writes_only_when_something_changed_and_keeps_one_backup(){
  let dir=tempfile::tempdir().unwrap();let user=dir.path().join("u");std::fs::create_dir_all(user.join("Config")).unwrap();
  let file=user.join("Config/Dolphin.ini");
  std::fs::write(&file,"[General]\nLoadPath = User/Load\n[DSP]\nVolume = 0\n").unwrap();
  apply_path_law(&user).unwrap();
  assert_eq!(std::fs::read_to_string(&file).unwrap(),"[DSP]\nVolume = 0\n".replace("[DSP]","[General]\n[DSP]"));
  assert!(user.join("Config/Dolphin.ini.before-path-law").is_file());
  let clean="[DSP]\nVolume = 0\n";std::fs::write(&file,clean).unwrap();
  let before=std::fs::metadata(&file).unwrap().modified().unwrap();
  apply_path_law(&user).unwrap();
  assert_eq!(std::fs::read_to_string(&file).unwrap(),clean);
  assert_eq!(std::fs::metadata(&file).unwrap().modified().unwrap(),before,"untouched when nothing to remove");
  assert!(apply_path_law(&dir.path().join("absent")).is_ok(),"no Dolphin.ini is not an error");
 }
}
```

Note on the first `assert_eq` in `apply_writes_only...`: removing the only pair leaves the `[General]` header, so the expected text is `"[General]\n[DSP]\nVolume = 0\n"`. Write that literal instead of the `replace` trick.

- [ ] **Step 2: Run** `cargo test -p reborn-core path_law_tests` → FAIL (functions missing).

- [ ] **Step 3: Implement.** Replace `resolve_portable_paths` and `portable_path_overlay` (and their doc comment) with:

```rust
/// The path law: a Reborn user folder is self-contained. Dolphin derives
/// every path from the `-u` folder, so Dolphin.ini carries no path key that
/// points inside a Dolphin user folder (this one or another) and none that
/// is relative. A 2.1 Dolphin.ini copied into a migrated folder kept the
/// game loading textures from the old install (Adam, 2026-09-11, second
/// PC); a relative `User/Load` resolves against the working directory.
/// Absolute paths outside every Dolphin user folder are a player's own
/// choice and stay. The disc keys go always: the launcher passes `-e`.
pub fn apply_path_law(user: &std::path::Path) -> crate::platform::Result<()> {
 let file=user.join("Config/Dolphin.ini");
 let text=match std::fs::read_to_string(&file){
  Ok(text)=>text,
  Err(e) if e.kind()==std::io::ErrorKind::NotFound=>return Ok(()),
  Err(e)=>return Err(e.into()),
 };
 let mut doc=crate::ini::IniDocument::parse(&text);
 let removals=path_law_removals(&doc,user);
 if removals.is_empty(){return Ok(());}
 for (section,key) in &removals{doc.remove(section,key);}
 let backup=file.with_extension("ini.before-path-law");
 if !backup.exists(){std::fs::write(&backup,&text)?;}
 std::fs::write(file,doc.render())?;
 Ok(())
}

const ALWAYS_REMOVED:&[(&str,&str)]=&[("General","ISOPaths"),("General","ISOPath0"),("General","ISOPath1"),("General","ISOPath2"),("General","ISOPath3"),("Core","DefaultISO")];
const PATH_KEYS:&[(&str,&[&str])]=&[
 ("General",&["LoadPath","DumpPath","NANDRootPath","ResourcePackPath","WiiSDCardPath"]),
 ("Core",&["MemcardAPath","MemcardBPath","GCIFolderAPathOverride","GCIFolderBPathOverride","AgpCartAPath","AgpCartBPath"]),
 ("GBA",&["BIOS","SavesPath","Rom1","Rom2","Rom3","Rom4"]),
];
const DOLPHIN_SUBFOLDERS:&[&str]=&["Load","Dump","ResourcePacks","GC","Wii","StateSaves","Config","GameSettings","Logs","GBA"];

/// Section and key of every pair the law removes from `doc`.
pub fn path_law_removals(doc:&crate::ini::IniDocument,user:&std::path::Path)->Vec<(String,String)>{
 let mut out=Vec::new();
 for (section,key) in ALWAYS_REMOVED{ if doc.get(section,key).is_some(){out.push(((*section).to_string(),(*key).to_string()));} }
 for (section,keys) in PATH_KEYS{
  for key in *keys{
   let Some(value)=doc.get(section,key) else{continue};
   let normalized=value.trim().trim_matches('"').replace('\\',"/");
   if normalized.is_empty()||normalized.eq_ignore_ascii_case("none"){continue;}
   if !is_absolute(&normalized)||under(&normalized,user)||inside_another_dolphin_user_folder(&normalized){
    out.push(((*section).to_string(),(*key).to_string()));
   }
  }
 }
 out
}

/// `.../User/<Dolphin subfolder>/...` where the `User` is not a Windows
/// account folder (`C:/Users/User/...`), or `.../dolphin-users/<name>/...`.
fn inside_another_dolphin_user_folder(normalized:&str)->bool{
 let all:Vec<&str>=normalized.split('/').filter(|p|!p.is_empty()&&*p!=".").collect();
 if all.iter().any(|p|p.eq_ignore_ascii_case("dolphin-users")){return true;}
 all.iter().rposition(|p|p.eq_ignore_ascii_case("User")).is_some_and(|i|{
  let before_is_users_root=i>0&&all[i-1].eq_ignore_ascii_case("Users");
  let dolphin_folder=all.get(i+1).is_some_and(|f|DOLPHIN_SUBFOLDERS.iter().any(|k|k.eq_ignore_ascii_case(f)));
  !before_is_users_root&&dolphin_folder
 })
}
```

Keep `is_absolute` and `under` as they are. In `session.rs:577` replace `crate::dolphin_preferences::resolve_portable_paths(&user_dir)?;` with `crate::dolphin_preferences::apply_path_law(&user_dir)?;` and fix the comment above it ("Before the portable-path rewrite" → "Before the path law strips path keys"). Grep the workspace for `resolve_portable_paths` and `portable_path_overlay` and replace every other caller (CLI included) the same way.

Then `memory_card_overlay` in `session.rs`:

```rust
/// The memory card the game saves to. A fresh user folder seeded from the
/// runtime template used to name no card at all (SlotA = 255), so the game
/// could never save (Adam, 2026-09-12: "it didn't change the memory card").
/// Slot A becomes a raw memory card with NO path: Dolphin then uses its
/// default, `<user>/GC/MemoryCardA.<region>.raw`, which is the path law. A
/// card path the file names is kept only when it lies outside this user
/// folder and the file exists (a player's own choice); one inside the user
/// folder is dropped for the default. A working slot (a raw card, a folder
/// of GCIs) is left as it is.
pub fn memory_card_overlay(doc: &crate::ini::IniDocument, user_dir: &Path) -> crate::ini::Overlay {
    let mut overlay = crate::ini::Overlay::new();
    let slot = doc.get("Core", "SlotA").map(str::trim).unwrap_or("");
    if !(slot.is_empty() || slot == "255" || slot == "0") {
        return overlay;
    }
    overlay.entry("Core".into()).or_default().insert("SlotA".into(), "1".into());
    let _ = user_dir;
    overlay
}
```

Update its test: assert `overlay["Core"]["SlotA"] == "1"`, assert `!overlay["Core"].contains_key("MemcardAPath")`, keep the "working slot untouched" asserts, and drop the "old card comes back" part (the path law removes in-folder card paths; a foreign existing one survives because `path_law_removals` keeps foreign absolute paths). Add to `path_law_tests`: `MemcardAPath = D:/cards/A.raw` is kept.

- [ ] **Step 4: Run** `cargo test -p reborn-core dolphin_preferences:: session::` → PASS. Then `cargo build --workspace` to catch the removed function names in the CLI and UI.

- [ ] **Step 5: Commit** `git add crates/reborn-core/src/dolphin_preferences.rs crates/reborn-core/src/session.rs <other callers> && git commit -m "The path law: Dolphin.ini carries no path keys, every path comes from the user folder"`

---

### Task 4: `layout`: tuck a 2.1 install aside, adopt a Launcher-era root

**Files:**
- Create: `crates/reborn-core/src/layout.rs`
- Modify: `crates/reborn-core/src/lib.rs` (add `pub mod layout;` beside the other `pub mod` lines)
- Modify: `crates/reborn-core/src/supervisor.rs:226-230` (wording), test `:790-810` (assert on the new wording)
- Modify: `crates/reborn-ui/src/main.rs:5344` (after `logging::start`), `:652-665` (`default_legacy_root`)
- Modify: `crates/reborn-cli/src/main.rs:609` (after `logging::start`)

**Interfaces:**
- Produces:
  ```rust
  pub const LEGACY_SUBFOLDER: &str = "Reborn 2.1";
  pub fn top_level_names() -> std::collections::BTreeSet<String>;   // lowercase
  pub struct Prepared { pub tucked: Vec<PathBuf>, pub adopted: Vec<PathBuf>, pub blocked: Vec<(PathBuf, String)>, pub legacy_root: Option<PathBuf> }
  pub fn prepare_data_root(platform: &Platform) -> Result<Prepared>;            // real roots
  pub fn prepare_at(data_root: &Path, previous_root: Option<&Path>) -> Result<Prepared>;
  pub fn tuck_legacy(data_root: &Path) -> Result<(Vec<PathBuf>, Vec<(PathBuf, String)>)>;
  pub fn adopt_previous(previous: &Path, data_root: &Path) -> Result<Vec<PathBuf>>;
  pub fn legacy_root(data_root: &Path) -> Option<PathBuf>;   // <root>/Reborn 2.1 when LegacyInstall::detect finds it there
  ```
- Consumes: `crate::migration::LegacyInstall::detect`, `DataDir::ALL` and `DataDir::relative()`, `Paths::previous_data_root` (Task 1).

- [ ] **Step 1: Failing tests** (bottom of the new `layout.rs`):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    fn put(root: &Path, rel: &str) {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, b"x").unwrap();
    }
    fn names(dir: &Path) -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        v.sort();
        v
    }

    #[test]
    fn a_2_1_install_at_the_data_root_is_tucked_into_its_own_subfolder_and_nothing_is_deleted() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("NFL Street Reborn");
        for rel in ["User/GC/MemoryCardA.USA.raw", "Games/NSR.iso", "MODS/!INSTLOGS/x.txt", "Tools/reborn-builder-cli.exe", "NFL Street Reborn Dolphin.exe", "unins000.exe", "JSGME.ini", "logs/launcher.log", "app/reborn-launcher.exe"] {
            put(&root, rel);
        }
        let p = prepare_at(&root, None).unwrap();
        assert!(p.blocked.is_empty(), "{:?}", p.blocked);
        assert_eq!(names(&root), ["Reborn 2.1", "app", "logs"]);
        assert_eq!(names(&root.join(LEGACY_SUBFOLDER)), ["Games", "JSGME.ini", "MODS", "NFL Street Reborn Dolphin.exe", "Tools", "User", "unins000.exe"]);
        assert!(root.join(LEGACY_SUBFOLDER).join("User/GC/MemoryCardA.USA.raw").is_file());
        assert_eq!(p.legacy_root.as_deref(), Some(root.join(LEGACY_SUBFOLDER).as_path()));
        assert!(crate::migration::LegacyInstall::detect(&root).is_none(), "the root no longer looks like a 2.1 install");
        // idempotent
        let again = prepare_at(&root, None).unwrap();
        assert!(again.tucked.is_empty() && again.adopted.is_empty());
        assert_eq!(names(&root), ["Reborn 2.1", "app", "logs"]);
    }

    #[test]
    fn a_data_root_that_is_not_a_2_1_install_is_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("NFL Street Reborn");
        put(&root, "config/settings.json");
        put(&root, "somebody-elses-folder/a.txt");
        let p = prepare_at(&root, None).unwrap();
        assert!(p.tucked.is_empty());
        assert_eq!(names(&root), ["config", "somebody-elses-folder"]);
    }

    #[test]
    fn a_launcher_era_root_is_adopted_by_rename_and_unknown_entries_stay() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("NFL Street Reborn");
        let previous = dir.path().join("NFL Street Reborn Launcher");
        put(&root, "app/reborn-launcher.exe");
        for rel in ["config/settings.json", "library/textures/GN7E69/a.dds", "dolphin-users/fresh-1/GC/MemoryCardA.USA.raw", "mods/packages/p/1.0.0/package.json", "journal.json", "publisher/app/x.exe", "app/old.exe"] {
            put(&previous, rel);
        }
        let p = prepare_at(&root, Some(&previous)).unwrap();
        assert_eq!(names(&root), ["app", "config", "dolphin-users", "journal.json", "library", "mods"]);
        assert!(root.join("dolphin-users/fresh-1/GC/MemoryCardA.USA.raw").is_file());
        assert!(root.join("app/reborn-launcher.exe").is_file(), "the new program folder is never touched");
        assert_eq!(names(&previous), ["app", "publisher"], "unknown entries and the old program folder stay behind");
        assert_eq!(p.adopted.len(), 5);
    }

    #[test]
    fn adoption_never_overwrites_and_only_happens_into_a_root_with_no_settings() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("NFL Street Reborn");
        let previous = dir.path().join("NFL Street Reborn Launcher");
        put(&previous, "config/settings.json");
        put(&previous, "library/a");
        put(&root, "library/b");
        let p = prepare_at(&root, Some(&previous)).unwrap();
        assert_eq!(names(&root), ["config", "library"]);
        assert!(root.join("library/b").is_file() && !root.join("library/a").is_file(), "an existing entry is skipped, not merged");
        assert!(previous.join("library/a").is_file());
        assert_eq!(p.adopted.len(), 1);
        // a root that already has settings adopts nothing
        put(&previous, "stats/queue/x");
        let again = prepare_at(&root, Some(&previous)).unwrap();
        assert!(again.adopted.is_empty());
        assert!(previous.join("stats/queue/x").is_file());
    }

    #[test]
    fn tuck_runs_before_adopt_so_mods_never_lands_inside_jsgme_mods() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("NFL Street Reborn");
        let previous = dir.path().join("NFL Street Reborn Launcher");
        put(&root, "User/GC/MemoryCardA.USA.raw");
        put(&root, "MODS/!BACKUP/x");
        put(&previous, "config/settings.json");
        put(&previous, "mods/packages/p/1.0.0/package.json");
        prepare_at(&root, Some(&previous)).unwrap();
        assert!(root.join("mods/packages/p/1.0.0/package.json").is_file());
        assert!(root.join(LEGACY_SUBFOLDER).join("MODS/!BACKUP/x").is_file());
    }

    #[test]
    fn the_top_level_names_come_from_the_layout_and_include_the_program_folder() {
        let names = top_level_names();
        for n in ["library", "runtime", "mods", "saves", "config", "cache", "logs", "dolphin-users", "app", "journal.json", "reborn 2.1"] {
            assert!(names.contains(n), "{n}");
        }
        assert!(!names.contains("user") && !names.contains("games"));
    }
}
```

- [ ] **Step 2: Run** `cargo test -p reborn-core layout::` → FAIL (module missing).

- [ ] **Step 3: Implement** `crates/reborn-core/src/layout.rs`:

```rust
//! The data root, made ready before anything writes into it.
//!
//! Two things can be sitting where the launcher's data goes:
//!
//! - A 2.1 install. `%APPDATA%\NFL Street Reborn` was 2.1's own folder, a
//!   live Dolphin tree with a JSGME `MODS` folder; on Windows the launcher's
//!   `mods` IS that `MODS`. Everything that is not part of the 3.0 layout is
//!   moved into one subfolder, `Reborn 2.1`, and migration reads from there.
//! - The data of a launcher built before 2026-09-12, in `NFL Street Reborn
//!   Launcher[ Beta]` beside the new root. Its known entries are moved in.
//!
//! Both are renames on one volume: instant, and nothing is ever deleted or
//! copied. Both are idempotent, and both run BEFORE the supervisor lock and
//! before seeding, from `main` in the window and the CLI.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::platform::{DataDir, Error, Platform, Result};

/// Where a 2.1 install found at the data root is moved to.
pub const LEGACY_SUBFOLDER: &str = "Reborn 2.1";

/// Top-level names the layout owns that are not the first segment of a
/// `DataDir`.
const EXTRA_TOP_LEVEL: &[&str] = &["app", "dolphin-users", "journal.json", "migrated-effects", "migrated-textures", LEGACY_SUBFOLDER];

/// Every top-level name of the 3.0 layout, lowercase.
pub fn top_level_names() -> BTreeSet<String> {
    let mut names: BTreeSet<String> = DataDir::ALL
        .iter()
        .filter_map(|d| d.relative().split('/').next())
        .map(str::to_lowercase)
        .collect();
    names.extend(EXTRA_TOP_LEVEL.iter().map(|s| s.to_lowercase()));
    names
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Prepared {
    pub tucked: Vec<PathBuf>,
    pub adopted: Vec<PathBuf>,
    /// Entries that could not be moved, with the reason. A locked file (the
    /// old game still running) is the ordinary cause; the next start retries.
    pub blocked: Vec<(PathBuf, String)>,
    /// The tucked 2.1 install, when there is one.
    pub legacy_root: Option<PathBuf>,
}

impl Prepared {
    /// One line per thing done, for the log.
    pub fn log(&self) {
        for p in &self.tucked { tracing::warn!("moved a 2.1 entry aside: {}", p.display()); }
        for p in &self.adopted { tracing::warn!("adopted from the previous data root: {}", p.display()); }
        for (p, why) in &self.blocked { tracing::warn!("could not move {}: {why}", p.display()); }
    }
}

/// Make the real data root ready.
pub fn prepare_data_root(platform: &Platform) -> Result<Prepared> {
    let data = platform.paths.data_root()?;
    let previous = platform.paths.previous_data_root()?;
    prepare_at(&data, previous.as_deref())
}

/// Tuck first, then adopt: adopting `mods` into a folder that still holds
/// JSGME's `MODS` would skip it as a collision.
pub fn prepare_at(data_root: &Path, previous_root: Option<&Path>) -> Result<Prepared> {
    let mut out = Prepared::default();
    if crate::migration::LegacyInstall::detect(data_root).is_some() {
        let (tucked, blocked) = tuck_legacy(data_root)?;
        out.tucked = tucked;
        out.blocked = blocked;
    }
    out.legacy_root = legacy_root(data_root);
    if let Some(previous) = previous_root {
        if previous.is_dir() && !same_dir(previous, data_root) {
            let settings = |root: &Path| root.join(DataDir::Config.relative()).join("settings.json");
            if !settings(data_root).exists() && settings(previous).exists() {
                out.adopted = adopt_previous(previous, data_root)?;
            }
        }
    }
    Ok(out)
}

/// `<data root>/Reborn 2.1`, when a 2.1 install sits there.
pub fn legacy_root(data_root: &Path) -> Option<PathBuf> {
    let candidate = data_root.join(LEGACY_SUBFOLDER);
    crate::migration::LegacyInstall::detect(&candidate).map(|_| candidate)
}

/// Move every top-level entry that is not part of the 3.0 layout into
/// `Reborn 2.1`. Entries that cannot be moved are reported, not fatal.
pub fn tuck_legacy(data_root: &Path) -> Result<(Vec<PathBuf>, Vec<(PathBuf, String)>)> {
    let keep = top_level_names();
    let into = data_root.join(LEGACY_SUBFOLDER);
    std::fs::create_dir_all(&into).map_err(|source| Error::Io { path: into.clone(), source })?;
    let mut moved = Vec::new();
    let mut blocked = Vec::new();
    let entries = std::fs::read_dir(data_root).map_err(|source| Error::Io { path: data_root.to_path_buf(), source })?;
    for entry in entries.flatten() {
        let name = entry.file_name();
        if keep.contains(&name.to_string_lossy().to_lowercase()) {
            continue;
        }
        let from = entry.path();
        let to = into.join(&name);
        if to.exists() {
            blocked.push((from, format!("{} already holds an entry of that name", into.display())));
            continue;
        }
        match std::fs::rename(&from, &to) {
            Ok(()) => moved.push(to),
            Err(e) => blocked.push((from, e.to_string())),
        }
    }
    Ok((moved, blocked))
}

/// Move the known 3.0 entries of `previous` into `data_root`. The program
/// folder `app` and anything the layout does not know stay behind. An
/// entry already present in `data_root` is skipped. An emptied previous
/// root is removed.
pub fn adopt_previous(previous: &Path, data_root: &Path) -> Result<Vec<PathBuf>> {
    let known = top_level_names();
    let mut adopted = Vec::new();
    std::fs::create_dir_all(data_root).map_err(|source| Error::Io { path: data_root.to_path_buf(), source })?;
    let entries = std::fs::read_dir(previous).map_err(|source| Error::Io { path: previous.to_path_buf(), source })?;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let lower = name.to_string_lossy().to_lowercase();
        if lower == "app" || lower == LEGACY_SUBFOLDER.to_lowercase() || !known.contains(&lower) {
            continue;
        }
        let to = data_root.join(&name);
        if to.exists() {
            tracing::warn!("not adopting {}: {} already exists", entry.path().display(), to.display());
            continue;
        }
        match std::fs::rename(entry.path(), &to) {
            Ok(()) => adopted.push(to),
            Err(e) => tracing::warn!("could not adopt {}: {e}", entry.path().display()),
        }
    }
    if std::fs::read_dir(previous).map(|mut d| d.next().is_none()).unwrap_or(false) {
        let _ = std::fs::remove_dir(previous);
    }
    Ok(adopted)
}

fn same_dir(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}
```

In `supervisor.rs` change the constant to:

```rust
pub const OLDER_INSTALL_IN_THE_WAY: &str =
    "This folder still holds files from an older NFL Street Reborn that could \
     not be moved out of the way. Close the older game and any of its tools, \
     then open the launcher again.";
```

and in the test `a_legacy_install_is_refused_as_a_data_root` replace `shown.contains("Choose a different folder")` with `shown.contains("Close the older game")`.

In `crates/reborn-ui/src/main.rs::main`, right after `reborn_core::logging::start(&Platform::detect());`:

```rust
    // The data root, made ready before the lock, the seeding and the window:
    // a 2.1 install at the root goes into its own subfolder and the data of
    // a launcher named "Launcher" is adopted. Renames only; it says nothing
    // and it cannot fail a start.
    match reborn_core::layout::prepare_data_root(&Platform::detect()) {
        Ok(prepared) => prepared.log(),
        Err(why) => tracing::warn!("the data root could not be prepared: {why}"),
    }
```

Same block in `crates/reborn-cli/src/main.rs::main` after its `logging::start`.

`default_legacy_root` in `reborn-ui/src/main.rs` becomes:

```rust
fn default_legacy_root(platform: &Platform) -> Option<PathBuf> {
    if let Ok(file) = reborn_core::settings::settings_path(platform) {
        if let Ok(settings) = reborn_core::settings::Settings::load(&file) {
            if let Some(root) = settings.legacy_root { if root.is_dir() { return Some(root); } }
        }
    }
    let data = platform.paths.data_root().ok()?;
    // The 2.1 install tucked into the data root at first start, else the
    // folder 2.1 used beside a launcher-named root (builds before 09-12).
    if let Some(tucked) = reborn_core::layout::legacy_root(&data) { return Some(tucked); }
    let guess = data.parent()?.join("NFL Street Reborn");
    (guess.is_dir() && !reborn_core::layout::same_named(&guess, &data)).then_some(guess)
}
```

Add to `layout.rs` a tiny `pub fn same_named(a: &Path, b: &Path) -> bool { same_dir(a, b) }` (public wrapper) so the UI never guesses the data root itself as the legacy root.

- [ ] **Step 4: Run** `cargo test -p reborn-core layout:: supervisor::` → PASS; `cargo build --workspace` → OK.

- [ ] **Step 5: Commit** `git add crates/reborn-core/src/layout.rs crates/reborn-core/src/lib.rs crates/reborn-core/src/supervisor.rs crates/reborn-ui/src/main.rs crates/reborn-cli/src/main.rs && git commit -m "A 2.1 install at the data root is tucked into Reborn 2.1, a Launcher-era root is adopted"`

---

### Task 5: Textures composed into the user folder

**Files:**
- Modify: `crates/reborn-core/src/platform/mod.rs:593-680` (drop `ModsActive`)
- Modify: `crates/reborn-core/src/mods.rs`: `link_textures`/`TextureLink`/`make_junction`/`same_place` (`:820-1000`, remove), `composed_root` (`:1566`), `activation_path` (`:1580`), `SyncRequest` (`:2600-2640`), `SyncReport` (`:2660-2700`), `sync_active` (`:2703-2815`), tests using `link_textures` (`:4230-4300`) and `link_into_user_dir` (`:4333`, `:4542`)
- Modify: `crates/reborn-core/src/session.rs:1041`
- Modify: `crates/reborn-ui/src/main.rs:1357-1450` (`ModSyncView`, `sync_view`), `:1470` (`sync_active_now`), `:1716` (`mods_unapply`)
- Modify: `crates/reborn-cli/src/main.rs:452-520` (`ModsCommand::Link` removed), `:682-686` (dispatch), `:2238-2252` (status), `:2423-2516` (`mods_link` removed), `:2518-2580` (`mods_sync` report)
- Modify: `crates/reborn-core/src/lib.rs:713-735`, `supervisor.rs:334`, `platform/imp.rs:93` comments that mention `mods/active`

**Interfaces:**
- Produces: `pub fn composed_root(user_dir: &Path, game: &GameId) -> PathBuf` = `<user_dir>/Load/Textures/<game>`.
- Produces: `SyncRequest { game, pristine_root, composed_root, packages_root, activation_file, user_dir, active_override, manage_user_texture_folder: bool }`; `SyncRequest::in_data_root(platform, game, user_dir)` fills `composed_root` from `user_dir`.
- Produces: `TreeStep::LeftAlone` (new variant); `SyncReport { game, composed_root, active, missing, tree, moved_aside: Option<PathBuf> }` (no `link`).
- Produces: `pub fn prepare_texture_folder(at: &Path, now_ms: u64) -> Result<Option<PathBuf>>` (removes a junction, moves a foreign folder aside as `<name>.before-reborn.<ms>`, returns the aside path).
- Produces: `pub fn retire_old_composed_tree(platform: &Platform, game: &GameId)` (best effort removal of `<data>/mods/active/<game>` when it carries `composed.json`, then the empty `mods/active`).
- Removes: `link_textures`, `TextureLink`, `make_junction`, `same_place`, `DataDir::ModsActive`, `ModsCommand::Link`.

- [ ] **Step 1: Failing tests** in `mods.rs` tests (replace the three `link_textures` tests at `:4230-4300` with):

```rust
    #[test]
    fn a_junction_in_the_way_is_removed_and_a_foreign_folder_is_moved_aside_never_deleted() {
        let dir = tempfile::tempdir().unwrap();
        let user = dir.path().join("user");
        let at = user.join("Load").join("Textures").join("GN7E69");
        // nothing there: nothing to do
        assert_eq!(prepare_texture_folder(&at, 1).unwrap(), None);
        // a real folder of somebody's own textures
        write(&at.join("theirs.dds"), b"theirs");
        let aside = prepare_texture_folder(&at, 42).unwrap().expect("moved aside");
        assert_eq!(aside, user.join("Load").join("Textures").join("GN7E69.before-reborn.42"));
        assert!(aside.join("theirs.dds").is_file() && !at.exists());
        // an empty folder is simply reused
        std::fs::create_dir_all(&at).unwrap();
        assert_eq!(prepare_texture_folder(&at, 43).unwrap(), None);
        assert!(at.is_dir());
        // a composed tree of ours is left for verify/apply
        write(&at.join(COMPOSED_MARKER), b"{}");
        assert_eq!(prepare_texture_folder(&at, 44).unwrap(), None);
        assert!(at.join(COMPOSED_MARKER).is_file());
    }

    #[cfg(unix)]
    #[test]
    fn a_link_in_the_way_is_removed_without_touching_its_target() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("old-composed");
        write(&target.join("a.dds"), b"a");
        let at = dir.path().join("user").join("Load").join("Textures").join("GN7E69");
        std::fs::create_dir_all(at.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&target, &at).unwrap();
        assert_eq!(prepare_texture_folder(&at, 1).unwrap(), None);
        assert!(!at.exists() && target.join("a.dds").is_file());
    }
```

(On Windows the junction case is proven by the real-window walk in Task 13; `std::fs::symlink_metadata(...).file_type().is_symlink()` is true for a junction and `remove_dir` removes it.)

Update the sync tests: every `link_into_user_dir: true` → `manage_user_texture_folder: true`; in `sync_request(...)` helper set `composed_root: composed_root(&user_dir, &game())`; the test at `:4542` (`link_into_user_dir = false`) now asserts `report.tree == TreeStep::LeftAlone` and that `user_dir/Load/Textures/GN7E69` was not created. Add:

```rust
    #[test]
    fn the_composed_tree_lives_in_the_user_folder_and_the_game_folder_is_a_real_directory() {
        let dir = tempfile::tempdir().unwrap();
        let (p, fx, user_dir) = sync_fixture(dir.path());
        let guard = lock(&p, &fx);
        let report = done(sync_active(&p, &guard, &sync_request(&fx, &user_dir), 1).unwrap());
        let at = user_dir.join("Load").join("Textures").join("GN7E69");
        assert_eq!(report.composed_root, at);
        assert!(std::fs::symlink_metadata(&at).unwrap().is_dir(), "a real directory, not a link");
        assert!(at.join(COMPOSED_MARKER).is_file());
        assert!(at.join(BALL).is_file());
        assert!(!at.parent().unwrap().join("GN7E69.building").exists());
    }

    #[test]
    fn an_old_composed_tree_under_mods_active_is_retired_once() {
        let dir = tempfile::tempdir().unwrap();
        let (p, fx, user_dir) = sync_fixture(dir.path());
        let old = fx.data.join("mods").join("active").join("GN7E69");
        write(&old.join(COMPOSED_MARKER), b"{}");
        write(&old.join("a.dds"), b"a");
        let foreign = fx.data.join("mods").join("active").join("FR4N31");
        write(&foreign.join("keep.dds"), b"not ours: no marker");
        let guard = lock(&p, &fx);
        done(sync_active(&p, &guard, &sync_request(&fx, &user_dir), 1).unwrap());
        assert!(!old.exists(), "ours, retired");
        assert!(foreign.join("keep.dds").is_file(), "no marker, never deleted");
    }
```

Look at the existing test helpers (`fixture()`, `sync_request`, `lock`, `done`) and add a `sync_fixture(root) -> (Platform, Fixture, PathBuf)` if one is not there; the fixture struct must expose `data` (the data root) so the old tree can be planted under `mods/active`.

- [ ] **Step 2: Run** `cargo test -p reborn-core mods::` → FAIL (names missing).

- [ ] **Step 3: Implement.**

`platform/mod.rs`: remove `ModsActive` from the enum, `relative()`, `ALL`, `is_derived()`. Update the doc comment on `DataDir` to say the composed texture tree lives in each Dolphin user folder.

`mods.rs`:
- Module doc: replace the `mods/active/<gameid>/  COMPOSED` line with `<user folder>/Load/Textures/<gameid>/   COMPOSED. What Dolphin loads, one per user folder.` and add a sentence: "It is a real directory, so no junction and no absolute path joins the two; a content update rebuilds it because the record carries the pristine tree's identity (Task 6)."
- Delete `TextureLink`, `link_textures`, `same_place`, both `make_junction` and the `#[cfg]` pair. Add:

```rust
/// Make room for the composed tree in the user folder.
///
/// A link (the junction earlier builds made) is removed: removing a link
/// deletes no content. A real folder that is not ours and not empty is
/// moved aside as `<game>.before-reborn.<ms>`, never deleted. Our own
/// composed tree, or an empty folder, is left for verify and apply.
pub fn prepare_texture_folder(at: &Path, now_ms: u64) -> Result<Option<PathBuf>> {
    let Ok(meta) = std::fs::symlink_metadata(at) else { return Ok(None) };
    if meta.file_type().is_symlink() {
        std::fs::remove_dir(at).or_else(|_| std::fs::remove_file(at)).map_err(|source| Error::Io { path: at.to_path_buf(), source })?;
        return Ok(None);
    }
    if !meta.is_dir() {
        return Err(refused(format!("{} is a file, not a folder; move it aside so the game's texture folder can be made", at.display())));
    }
    if at.join(COMPOSED_MARKER).is_file() {
        return Ok(None);
    }
    let empty = std::fs::read_dir(at).map_err(|source| Error::Io { path: at.to_path_buf(), source })?.next().is_none();
    if empty {
        return Ok(None);
    }
    let name = at.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let aside = at.with_file_name(format!("{name}.before-reborn.{now_ms}"));
    std::fs::rename(at, &aside).map_err(|source| Error::Io { path: aside.clone(), source })?;
    Ok(Some(aside))
}

/// The composed tree before 2026-09-12 lived at `mods/active/<game>` in the
/// data root. One that carries our marker is removed; anything else there
/// is left alone. Best effort, logged, never fatal.
pub fn retire_old_composed_tree(platform: &Platform, game: &GameId) {
    let Ok(data) = platform.paths.data_root() else { return };
    let active = data.join("mods").join("active");
    let old = active.join(game.as_str());
    if old.join(COMPOSED_MARKER).is_file() {
        if let Err(e) = std::fs::remove_dir_all(&old) {
            tracing::warn!("could not remove the old composed tree {}: {e}", old.display());
        }
    }
    if std::fs::read_dir(&active).map(|mut d| d.next().is_none()).unwrap_or(false) {
        let _ = std::fs::remove_dir(&active);
    }
}

/// `<user folder>/Load/Textures/<gameid>/`, the composed tree Dolphin loads.
pub fn composed_root(user_dir: &Path, game: &GameId) -> PathBuf {
    user_dir.join("Load").join("Textures").join(game.as_str())
}
```

- `activation_path`: `let packages = platform.data_dir(DataDir::ModsPackages)?; let mods_dir = packages.parent()...` (same error text).
- `SyncRequest`: rename `link_into_user_dir` → `manage_user_texture_folder` with the doc: "False composes nothing and leaves `Load/Textures/<game>` alone: the texture author's mode." `in_data_root` sets `composed_root: composed_root(user_dir, &game)`.
- `TreeStep`: add `/// Not composed, on purpose (the folder is managed by hand).\n LeftAlone,`.
- `SyncReport`: replace `pub link: TextureLink` with `pub moved_aside: Option<PathBuf>`. `explain()`: tree `LeftAlone` → "left alone, because the launcher was asked not to manage it"; drop the link words; mention `moved aside to <name>` when `moved_aside` is `Some` (name only, not the full path).
- `sync_active`: at the top, after loading `active`: `if !req.manage_user_texture_folder { return Ok(SyncOutcome::Done(Box::new(SyncReport { game: req.game.clone(), composed_root: req.composed_root.clone(), active, missing: Vec::new(), tree: TreeStep::LeftAlone, moved_aside: None }))); }`. Before `Composer::plan`: `let moved_aside = prepare_texture_folder(&req.composed_root, now_ms)?;`. After the tree step: `retire_old_composed_tree(platform, &req.game);`. Remove the `link` block; build the report with `moved_aside`.

`session.rs:1041`: `req.manage_user_texture_folder = crate::settings::manage_texture_folder(&settings_file)?;`

`reborn-ui/src/main.rs`:
- `ModSyncView`: keep the fields (the page reads none of `link`, `link_at`, `moved_aside`; they are kept for the harness page). `sync_view`: `tree` → `"left_alone" | "already_current" | "rebuilt"`; `link: Some("composed_in_user_folder")` (or `"left_alone"`), `link_at: Some(composed_root)`, `moved_aside: r.moved_aside`.
- `sync_active_now`: `req.manage_user_texture_folder = preferences.manage_texture_folder.unwrap_or(true);`
- `mods_unapply`: resolve the user folder the way `sync_active_now` does (`resolve_user_dir_for(&p, preferences.user_dir.as_deref(), exe.as_deref())`) and `let composed = reborn_core::mods::composed_root(user.path(), &game);`.

`reborn-cli/src/main.rs`: delete `ModsCommand::Link` and `fn mods_link`; in `mods status` (`:2238-2252`) compute the composed path from `resolve_user_dir(platform, None)` and say "composed in the user folder at ..." ; in `mods_sync` replace the `report.link` match with a line for `moved_aside`.

Comments in `lib.rs:713-735`, `supervisor.rs:334`, `platform/imp.rs:93` that mention `mods/active` as purgeable: reword to "the launcher's `mods` would be that `MODS`, and `mods/staging` is purgeable" (drop `mods/active`).

- [ ] **Step 4: Run** `cargo test --workspace` → PASS; `cargo clippy --workspace --all-targets` → no new warnings.

- [ ] **Step 5: Commit** named files: `git commit -m "Textures are composed into the user folder itself; no junction, no mods/active"`

---

### Task 6: A texture update always rebuilds: the pristine identity

**Files:**
- Modify: `crates/reborn-core/src/mods.rs`: `ComposeRequest` (`:1455`), `CompositionPlan` (`:1386`), `record()` (`:1441`), `ComposedRecord` (`:1793`), `Discrepancy` (`:1834-1870`), `verify` (`:2349-2430`), `Composer::plan` (`:2140-2160`), `SyncRequest::in_data_root` (`:2630`), `sync_active` (`:2775`)
- Modify: `crates/reborn-core/src/mods.rs:1499-1525` add `pub fn pristine_identity(platform: &Platform, game: &GameId) -> Option<String>`

**Interfaces:**
- Produces: `ComposeRequest.pristine_identity: Option<String>`, `CompositionPlan.pristine_identity: Option<String>`, `ComposedRecord.pristine_identity: Option<String>` (`#[serde(default)]`), `Discrepancy::PristineIdentityDiffers { recorded: Option<String>, planned: Option<String> }`, `SyncRequest.pristine_identity: Option<String>`.
- Identity string: `format!("{}#{}", pointer.current.version, pointer.current.sequence)` from `content::Pointer::load(ContentLayout::new(data_root).pointer_path("textures"))`, `None` when there is no pointer.

- [ ] **Step 1: Failing test** (mods tests):

```rust
    /// The update engine swaps the whole pristine folder. The composed tree
    /// is hardlinks into the OLD folder, so with the same names and count
    /// nothing used to notice and the game kept the old bytes. The record
    /// carries the pristine tree's identity, so a new identity rebuilds.
    #[test]
    fn a_pristine_tree_swapped_under_the_same_names_rebuilds_the_composed_tree() {
        let dir = tempfile::tempdir().unwrap();
        let (p, fx, user_dir) = sync_fixture(dir.path());
        let guard = lock(&p, &fx);
        let mut req = sync_request(&fx, &user_dir);
        req.pristine_identity = Some("1.1.0#2".into());
        done(sync_active(&p, &guard, &req, 1).unwrap());
        let composed = composed_root(&user_dir, &game()).join(BALL);
        assert_eq!(std::fs::read(&composed).unwrap(), b"the original football");

        // the swap: same names, new bytes, new identity
        let prior = fx.pristine.with_file_name("GN7E69.prior");
        std::fs::rename(&fx.pristine, &prior).unwrap();
        lay_pristine(&fx.pristine);
        std::fs::write(fx.pristine.join(BALL), b"the NEW football").unwrap();
        let same = done(sync_active(&p, &guard, &req, 2).unwrap());
        assert_eq!(same.tree, TreeStep::AlreadyCurrent, "same identity: nothing notices, by design of the count check");
        req.pristine_identity = Some("1.2.0#3".into());
        let rebuilt = done(sync_active(&p, &guard, &req, 3).unwrap());
        assert!(matches!(rebuilt.tree, TreeStep::Rebuilt { .. }));
        assert_eq!(std::fs::read(&composed).unwrap(), b"the NEW football");
    }
```

- [ ] **Step 2: Run** → FAIL (no field `pristine_identity`).

- [ ] **Step 3: Implement.** Add the field to `ComposeRequest`, `CompositionPlan` (copied in `plan()`), `ComposedRecord` (`#[serde(default, skip_serializing_if = "Option::is_none")] pub pristine_identity: Option<String>`), `record()`, `SyncRequest` (set in `in_data_root` from `pristine_identity(platform, &game)`), and in `sync_active` pass `pristine_identity: req.pristine_identity.clone()` into `ComposeRequest`. In `verify`, after the `PristineRootDiffers` check:

```rust
        if record.pristine_identity != plan.pristine_identity {
            found.push(Discrepancy::PristineIdentityDiffers {
                recorded: record.pristine_identity.clone(),
                planned: plan.pristine_identity.clone(),
            });
        }
```

Add the variant and its line in `Verification::report()` ("the pristine textures changed (was {recorded:?}, now {planned:?})"). The identity helper:

```rust
/// What the textures content pointer says is installed, as one string, or
/// `None` when no update has delivered textures yet. A tree without a
/// pointer is never swapped by anything, so `None` is a stable identity.
pub fn pristine_identity(platform: &Platform, game: &GameId) -> Option<String> {
    let data = platform.paths.data_root().ok()?;
    if game.as_str() != "GN7E69" { return None; }
    let layout = crate::content::ContentLayout::new(&data);
    let pointer = crate::content::Pointer::load(&layout.pointer_path("textures")).ok().flatten()?;
    Some(format!("{}#{}", pointer.current.version, pointer.current.sequence))
}
```

Every other `ComposeRequest { .. }` literal in tests gets `pristine_identity: None`.

- [ ] **Step 4: Run** `cargo test --workspace` → PASS.

- [ ] **Step 5: Commit** `git commit -m "A texture update rebuilds the composed tree: the record carries the pristine identity"`

---

### Task 7: Migration copies no textures

**Files:**
- Modify: `crates/reborn-core/src/migration.rs:286-345` (add `copy_tree_excluding`)
- Modify: `crates/reborn-ui/src/setup_tools.rs:451` (User copy), `:479-499` (delete the `migrated-textures` block)

**Interfaces:**
- Produces: `pub fn copy_tree_excluding(from: &Path, to: &Path, exclude: &[&str], progress: &mut dyn FnMut(u64)) -> Result<u64>` where `exclude` holds relative paths from `from` with forward slashes (`"Load/Textures"`), compared case-insensitively.

- [ ] **Step 1: Failing test** (migration tests):

```rust
    #[test]
    fn copying_can_leave_a_subtree_behind() {
        let dir = tempfile::tempdir().unwrap();
        let from = dir.path().join("User");
        for rel in ["GC/MemoryCardA.USA.raw", "Config/Dolphin.ini", "Load/Textures/GN7E69/a.dds", "Load/GraphicMods/x.txt"] {
            let p = from.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, b"1").unwrap();
        }
        let to = dir.path().join("migrated");
        let bytes = copy_tree_excluding(&from, &to, &["load/textures"], &mut |_| {}).unwrap();
        assert_eq!(bytes, 3);
        assert!(to.join("GC/MemoryCardA.USA.raw").is_file());
        assert!(to.join("Load/GraphicMods/x.txt").is_file());
        assert!(!to.join("Load/Textures").exists());
    }
```

- [ ] **Step 2: Run** `cargo test -p reborn-core migration::copying_can_leave` → FAIL.

- [ ] **Step 3: Implement.** Thread an `exclude: &[String]` (lowercase, forward slashes) and a `relative: String` through `copy_tree_checked`; skip an entry whose `relative` lowercase equals an exclusion. Keep `copy_tree_no_clobber` and `copy_tree_with_progress` as wrappers passing `&[]`.

```rust
pub fn copy_tree_excluding(from: &Path, to: &Path, exclude: &[&str], progress: &mut dyn FnMut(u64)) -> Result<u64> {
    let exclude: Vec<String> = exclude.iter().map(|e| e.trim_matches('/').to_lowercase()).collect();
    copy_tree_checked(from, to, &mut Vec::new(), &exclude, "", progress)
}
```

In `copy_tree_checked`, when iterating: `let rel = if relative.is_empty() { name.clone() } else { format!("{relative}/{name}") }; if exclude.iter().any(|e| *e == rel.to_lowercase()) { continue; }` and pass `&rel` into the recursive call.

In `setup_tools.rs:451`: `copy_tree_excluding(&root.join("User"), &user, &["Load/Textures"], &mut |n| { ... })`. Delete lines 479-499 (`let textures = user.join("Load/Textures/GN7E69"); if textures.is_dir() ... saved.migrated_texture_root = Some(base); }`) and keep `saved.save(&file)`. Add a comment: "The 2.1 texture pack is not copied: the installer ships the pack, the composed folder is built from it, and the 2.1 pack stays intact under Reborn 2.1."

- [ ] **Step 4: Run** `cargo test --workspace` and `cargo build -p reborn-ui` → PASS.

- [ ] **Step 5: Commit** `git commit -m "Migration copies saves and settings, never the 2 GB texture pack"`

---

### Task 8: The user-folder template, staged by core code

**Files:**
- Create: `crates/reborn-core/src/user_template.rs`
- Modify: `crates/reborn-core/src/lib.rs` (`pub mod user_template;`)
- Modify: `crates/reborn-core/src/runtime.rs:839-876` (`seed_user_from_template` copies the whole tree; drop `USER_TEMPLATE_DIRS`), tests `:878-940`
- Modify: `crates/reborn-cli/src/main.rs:133-137` (add `RuntimeTemplate`), dispatch near `:650`
- Modify: `installer/windows/build.ps1:10-89` (param), `:280-296` (staging)

**Interfaces:**
- Produces: `pub fn stage(from_user: &Path, into_user: &Path) -> Result<StageReport>` with `StageReport { pub written: Vec<PathBuf>, pub skipped: Vec<PathBuf> }`; `pub fn sanitize_dolphin_ini(text: &str) -> String`, `pub fn sanitize_gfx_ini(text: &str) -> String`, `pub fn sanitize_qt_ini(text: &str) -> String`.
- CLI: `rebornctl runtime-template --from <User> --into <runtime>\User` (clap variant `RuntimeTemplate { from: PathBuf, into: PathBuf }`), prints one line per file, exit 0.
- `build.ps1 -UserTemplate <User folder>`; the build throws without it (unless `-LauncherOnly`, `-Publisher`, or no `-Runtime`).

- [ ] **Step 1: Failing tests** (in `user_template.rs`):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    fn put(root: &Path, rel: &str, text: &str) {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }
    #[test]
    fn only_the_basic_files_are_staged() {
        let dir = tempfile::tempdir().unwrap();
        let from = dir.path().join("User");
        put(&from, "Config/Dolphin.ini", "[Core]\nSlotA = 255\n");
        put(&from, "Config/GFX.ini", "[Settings]\nShowFPS = True\n");
        put(&from, "Config/Qt.ini", "[General]\nlastdir=C:/x\n");
        put(&from, "Config/GCPadNew.ini", "[GCPad1]\nDevice = XInput/0/Gamepad\n");
        put(&from, "Config/Dolphin.ini.launcher-original", "old");
        put(&from, "GameSettings/GN7E69.ini", "[Gecko]\n$Point Remover\n");
        put(&from, "Cache/GN7E69.uidcache", "uid");
        put(&from, "Cache/Shaders/D3D-gs-1.cache", "gs");
        put(&from, "Cache/gamelist.cache", "no");
        put(&from, "Cache/GameCovers/a.png", "no");
        put(&from, "GC/MemoryCardA.USA.raw", "no");
        put(&from, "Load/Textures/CR4E12/a.dds", "no");
        put(&from, "Logs/dolphin.log", "no");
        let into = dir.path().join("runtime").join("User");
        let report = stage(&from, &into).unwrap();
        let mut got: Vec<String> = report.written.iter().map(|p| p.strip_prefix(&into).unwrap().to_string_lossy().replace('\\', "/")).collect();
        got.sort();
        assert_eq!(got, ["Cache/GN7E69.uidcache", "Cache/Shaders/D3D-gs-1.cache", "Config/Dolphin.ini", "Config/GCPadNew.ini", "Config/GFX.ini", "Config/Qt.ini", "GameSettings/GN7E69.ini"]);
        assert!(!into.join("GC").exists() && !into.join("Load").exists() && !into.join("Logs").exists());
        assert!(!into.join("Config/Dolphin.ini.launcher-original").exists());
    }
    #[test]
    fn dolphin_ini_is_sanitized() {
        let text = "[Analytics]\nID = de6680a75ec16a1d97fb464825c34211\nEnabled = False\n[General]\nISOPaths = 1\nLoadPath = User/Load///\nGDBPort = -1\nISOPath0 = Games\n[Core]\nSlotA = 255\nSlotB = 255\nDefaultISO = C:\\x\\NSRV3.iso\n[NetPlay]\nHostCode = 1db46c2c\nNickname = YourName\nTraversalPort = 0x1876\n";
        let out = sanitize_dolphin_ini(text);
        let doc = crate::ini::IniDocument::parse(&out);
        assert!(doc.get("Analytics", "ID").is_none());
        assert_eq!(doc.get("Analytics", "Enabled"), Some("False"));
        for (s, k) in [("General", "ISOPaths"), ("General", "ISOPath0"), ("General", "LoadPath"), ("Core", "DefaultISO"), ("NetPlay", "HostCode"), ("NetPlay", "Nickname")] {
            assert!(doc.get(s, k).is_none(), "{s}/{k} should be gone");
        }
        assert_eq!(doc.get("General", "GDBPort"), Some("-1"));
        assert_eq!(doc.get("NetPlay", "TraversalPort"), Some("0x1876"));
        assert_eq!(doc.get("Core", "SlotA"), Some("1"));
        assert_eq!(doc.get("Core", "SlotB"), Some("255"));
    }
    #[test]
    fn gfx_and_qt_are_sanitized() {
        let gfx = sanitize_gfx_ini("[Settings]\nDumpTextures = True\nDumpBaseTextures = True\nDumpMipTextures = False\nShowFPS = True\nMSAA = 0x00000004\n");
        let doc = crate::ini::IniDocument::parse(&gfx);
        for k in ["DumpTextures", "DumpBaseTextures", "DumpMipTextures", "ShowFPS"] { assert_eq!(doc.get("Settings", k), Some("False"), "{k}"); }
        assert_eq!(doc.get("Settings", "MSAA"), Some("0x00000004"));
        let qt = sanitize_qt_ini("[General]\nlastdir=C:/Users/Adam/AppData/Roaming/NFL Street Reborn/Games\nfoo=1\n");
        assert!(!qt.contains("lastdir") && qt.contains("foo=1"));
    }
    #[test]
    fn staging_the_reference_folder_on_this_machine_when_it_exists() {
        let from = Path::new(r"C:\Users\Adam\nflstreet2_clean\DOLPHIN ONLINE TEST\NFL Street Reborn Dolphin\User");
        if !from.is_dir() { return; }
        let dir = tempfile::tempdir().unwrap();
        let into = dir.path().join("User");
        let report = stage(from, &into).unwrap();
        assert!(report.written.len() >= 10, "{}", report.written.len());
        let text = std::fs::read_to_string(into.join("Config/Dolphin.ini")).unwrap();
        assert!(!text.contains("C:") && !text.contains("ID ="), "{text}");
        assert!(into.join("GameSettings/GN7E69.ini").is_file());
        assert!(!into.join("Load").exists());
    }
}
```

- [ ] **Step 2: Run** `cargo test -p reborn-core user_template::` → FAIL.

- [ ] **Step 3: Implement** `user_template.rs`:

```rust
//! The starting point of every new Dolphin user folder: the reference
//! folder Adam tuned, reduced to "the real basic stuff" and cleaned of
//! anything personal or machine-bound. Staged into `<runtime>\User` by the
//! build; seeded into each new user folder by `runtime::seed_user_from_template`.

use std::path::{Path, PathBuf};
use crate::platform::{Error, Result};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct StageReport {
    pub written: Vec<PathBuf>,
    pub skipped: Vec<PathBuf>,
}

/// Stage `from_user` into `into_user`. Only these ship: `Config\*.ini`,
/// `GameSettings\*.ini`, `Cache\*.uidcache`, `Cache\Shaders\*.cache`.
/// Dolphin.ini, GFX.ini and Qt.ini are sanitized on the way.
pub fn stage(from_user: &Path, into_user: &Path) -> Result<StageReport> {
    let mut report = StageReport::default();
    let rules: [(&str, &str, bool); 4] = [("Config", "ini", false), ("GameSettings", "ini", false), ("Cache", "uidcache", false), ("Cache/Shaders", "cache", false)];
    for (dir, ext, _) in rules {
        let src_dir = from_user.join(dir.replace('/', std::path::MAIN_SEPARATOR_STR));
        let Ok(entries) = std::fs::read_dir(&src_dir) else { continue };
        for entry in entries.flatten() {
            let src = entry.path();
            if !src.is_file() || !src.extension().is_some_and(|e| e.eq_ignore_ascii_case(ext)) {
                report.skipped.push(src);
                continue;
            }
            let dst = into_user.join(dir).join(entry.file_name());
            std::fs::create_dir_all(dst.parent().unwrap()).map_err(|source| Error::Io { path: dst.clone(), source })?;
            let name = entry.file_name().to_string_lossy().to_lowercase();
            if dir == "Config" && (name == "dolphin.ini" || name == "gfx.ini" || name == "qt.ini") {
                let text = std::fs::read_to_string(&src).map_err(|source| Error::Io { path: src.clone(), source })?;
                let out = match name.as_str() { "dolphin.ini" => sanitize_dolphin_ini(&text), "gfx.ini" => sanitize_gfx_ini(&text), _ => sanitize_qt_ini(&text) };
                std::fs::write(&dst, out).map_err(|source| Error::Io { path: dst.clone(), source })?;
            } else {
                std::fs::copy(&src, &dst).map_err(|source| Error::Io { path: dst.clone(), source })?;
            }
            report.written.push(dst);
        }
    }
    Ok(report)
}

pub fn sanitize_dolphin_ini(text: &str) -> String {
    let mut doc = crate::ini::IniDocument::parse(text);
    for (s, k) in [("Analytics", "ID"), ("NetPlay", "HostCode"), ("NetPlay", "Nickname"), ("NetPlay", "IndexName"), ("NetPlay", "IndexPassword")] { doc.remove(s, k); }
    for (s, k) in crate::dolphin_preferences::path_law_removals(&doc, Path::new("")) { doc.remove(&s, &k); }
    let mut slots = crate::ini::Overlay::new();
    slots.entry("Core".into()).or_default().insert("SlotA".into(), "1".into());
    slots.entry("Core".into()).or_default().insert("SlotB".into(), "255".into());
    slots.entry("Analytics".into()).or_default().insert("Enabled".into(), "False".into());
    slots.entry("Analytics".into()).or_default().insert("PermissionAsked".into(), "True".into());
    doc.merge(&slots);
    doc.render()
}

pub fn sanitize_gfx_ini(text: &str) -> String {
    let mut doc = crate::ini::IniDocument::parse(text);
    let mut off = crate::ini::Overlay::new();
    for k in ["DumpTextures", "DumpBaseTextures", "DumpMipTextures", "ShowFPS"] { off.entry("Settings".into()).or_default().insert(k.into(), "False".into()); }
    doc.merge(&off);
    doc.render()
}

pub fn sanitize_qt_ini(text: &str) -> String {
    text.lines().filter(|l| { let t = l.trim_start().to_lowercase(); !(t.starts_with("lastdir") || t.contains(":/") || t.contains(":\\")) }).map(|l| format!("{l}\n")).collect()
}
```

Note: `path_law_removals(&doc, Path::new(""))` treats every relative and in-user-folder path as removable; with an empty `user` the `under` check matches nothing, and the "another Dolphin user folder" rule plus the relative rule do the work. Check the test `dolphin_ini_is_sanitized` passes with `LoadPath = User/Load///` (relative → removed) and `DefaultISO` (always removed).

`runtime.rs`: replace `USER_TEMPLATE_DIRS` and `seed_user_from_template` with a recursive walk of `<runtime>\User` copying every file, no clobber; keep the signature `(runtime_exe: &Path, user_dir: &Path) -> Result<Vec<PathBuf>>`. Update the template tests: `a_new_user_folder_gets_every_template_ini_and_nothing_else` becomes `..._gets_every_template_file` (add `User/Cache/GN7E69.uidcache` to the fixture and expect it; keep the `notes.txt` case but now it IS copied, so rename the test and expectation accordingly).

CLI: add to the `Command` enum after `AuditRuntime`:

```rust
    /// Stage the Dolphin user-folder template the runtime ships, from a
    /// tuned User folder: Config and GameSettings INIs and the shader
    /// caches, cleaned of anything personal. build.ps1 runs this.
    RuntimeTemplate {
        /// The tuned User folder.
        #[arg(long)]
        from: PathBuf,
        /// The runtime's User folder to write.
        #[arg(long)]
        into: PathBuf,
    },
```

dispatch: `Command::RuntimeTemplate { from, into } => { let r = reborn_core::user_template::stage(&from, &into)?; for p in &r.written { println!("staged {}", p.display()); } println!("{} file(s) staged into {}", r.written.len(), into.display()); Ok(OK) }`.

`build.ps1`: add `[string]$UserTemplate = "",` after `$Runtime` with the comment "The tuned Dolphin User folder the template is made from (Adam's DOLPHIN ONLINE TEST User). Required with -Runtime." Replace the `foreach ($d in "User\Config", "User\GameSettings") { ... }` block (lines 282-296) with:

```powershell
    # The baseline for every new Dolphin user folder, staged from the tuned
    # reference folder by the core (rebornctl runtime-template): Config and
    # GameSettings INIs and the shader caches, cleaned of anything personal.
    if (-not $UserTemplate) { throw "-UserTemplate <tuned User folder> is required with -Runtime. Nothing was built." }
    if (-not (Test-Path (Join-Path $UserTemplate "Config\Dolphin.ini"))) { throw "$UserTemplate has no Config\Dolphin.ini. Nothing was built." }
    $ctl = Join-Path $root "target\release\rebornctl.exe"
    & $ctl runtime-template --from $UserTemplate --into (Join-Path $dest "User")
    if ($LASTEXITCODE -ne 0) { throw "rebornctl runtime-template failed. Nothing was built." }
```

`-RuntimeOnly` builds `reborn-cli` too (line 155/157 both include `-p reborn-cli`), so `rebornctl.exe` exists by then.

- [ ] **Step 4: Run** `cargo test --workspace` → PASS; `cargo build --release -p reborn-cli` then `.\target\release\rebornctl.exe runtime-template --from "C:\Users\Adam\nflstreet2_clean\DOLPHIN ONLINE TEST\NFL Street Reborn Dolphin\User" --into "$env:TEMP\tpl\User"` and list the result: Config (about 13 INIs), GameSettings, Cache with uidcaches and Shaders.

- [ ] **Step 5: Commit** `git commit -m "The user-folder template is the tuned reference folder, staged and sanitized by the core"`

---

### Task 9: Installer names and safety

**Files:**
- Modify: `installer/windows/reborn-launcher.iss:70-86` (defines), `:202` (`UninstallDisplayName`), `:290-308` (icons), `:372-404` (`OldDataRoot`, `InstallDirIsSafe`), `:412-460` (`PreviousProgramsInstall`, `RemovePreviousProgramsInstall`), `:490` (call), plus the header notes `:20-35`

- [ ] **Step 1: The defines.** Beta: `#define DataDir "NFL Street Reborn Beta"`; stable: `#define DataDir "NFL Street Reborn"`. Update the two comments ("reborn_core's PRODUCT plus the channel suffix"; "The 2.1 folder is the stable data root now; a 2.1 install found there is moved into Reborn 2.1 by the launcher's first start, and the 2.1 uninstaller registered under this AppId is never run, see RemovePreviousInstall").

- [ ] **Step 2: Names.** `UninstallDisplayName={#AppName}`. Icons: `{group}\{#AppName}` (was `... Launcher`), `{group}\{cm:UninstallProgram,{#AppName}}`, `{app}\Uninstall {#AppName}`, `{userdesktop}\{#AppName}`. Leave "Play {#AppName}" and "{#AppName} files".

- [ ] **Step 3: `InstallDirIsSafe`** becomes:

```pascal
function InstallDirIsSafe(Dir: String): Boolean;
var
  Normalized, Own, I: Integer;
  Roots: array[0..3] of String;
  N: String;
begin
  N := Lowercase(RemoveBackslashUnlessRoot(ExpandFileName(Dir)));
  Own := Lowercase(ExpandConstant('{userappdata}\{#DataDir}\app'));
  if N = Own then begin Result := True; exit; end;
  Roots[0] := Lowercase(ExpandConstant('{userappdata}\NFL Street Reborn'));
  Roots[1] := Lowercase(ExpandConstant('{userappdata}\NFL Street Reborn Beta'));
  Roots[2] := Lowercase(ExpandConstant('{userappdata}\NFL Street Reborn Launcher'));
  Roots[3] := Lowercase(ExpandConstant('{userappdata}\NFL Street Reborn Launcher Beta'));
  Result := True;
  for I := 0 to 3 do
    if Pos(Roots[I] + '\', N + '\') = 1 then Result := False;
end;
```

(Declare `Own` and `N` as `String`, `I` as `Integer`; the sketch above mixes them, fix the `var` block.) Delete `OldDataRoot` if nothing else uses it (grep the file).

- [ ] **Step 4: Previous install cleanup.** Rename `PreviousProgramsInstall` → `PreviousLauncherInstall` and accept two shapes of `UninstallString` folder: under `{localappdata}\Programs\` (as today) OR equal to `{userappdata}\NFL Street Reborn Launcher\app` / `{userappdata}\NFL Street Reborn Launcher Beta\app`. Keep both guards: `FileExists(Value)` and `FileExists(ExtractFileDir(Value) + '\{#LauncherExe}')`. The 2.1 uninstaller sits at `{userappdata}\NFL Street Reborn\unins000.exe` with no `reborn-launcher.exe` beside it, so it never qualifies. Rename `RemovePreviousProgramsInstall` → `RemovePreviousLauncherInstall`; its `DelTree` stays limited to `ExtractFileDir(Uninstaller)` (the old `app` folder). Update the call in `PrepareToInstall` and the header notes.

- [ ] **Step 5: Compile check.** `& "C:\Program Files (x86)\Inno Setup 6\ISCC.exe" /O"$env:TEMP\isscheck" /DPayload="<any small folder>" installer\windows\reborn-launcher.iss` (find ISCC where build.ps1 finds it: grep `ISCC` in build.ps1 and use the same path). Expected: compiles. Also `Tools\test-all.ps1` installer-input checks still pass (the `InfoBeforeFile` directives are untouched).

- [ ] **Step 6: Commit** `git add installer/windows/reborn-launcher.iss && git commit -m "Installer: NFL Street Reborn is the folder and the name; the old app folder is cleaned up"`

---

### Task 10: Scripts, docs, and the remaining spellings

**Files:**
- Modify: `installer/beta/update-review.ps1:30-31`, `installer/beta/sandbox-smoke.ps1:354`, `:463` (expected data root names)
- Modify: `docs/architecture/SYSTEM_ARCHITECTURE.md:137-250` (section 4), `docs/architecture/CONTENT_AND_UPDATE_MODEL.md` (add a dated note under section 10 "Layout" and section 13 "Activation")
- Modify: `crates/reborn-core/src/content/seed.rs:1-20` module doc (mentions `mods/active`? check), `crates/reborn-core/src/platform/imp.rs` remaining comments

- [ ] **Step 1:** `grep -rn "NFL Street Reborn Launcher" crates installer Tools docs --include=*.rs --include=*.ps1 --include=*.iss --include=*.mjs --include=*.md | grep -v "Launcher Setup\|Launcher Update\|Launcher/\|user agent"` and fix every hit that names a DATA ROOT (not an installer file name, not the user agent). `grep -rn "mods/active\|mods\\\\active" crates docs` and fix every hit.

- [ ] **Step 2:** SYSTEM_ARCHITECTURE.md section 4: data roots become `%APPDATA%\NFL Street Reborn[ Beta]` (macOS/Linux likewise without "Launcher"), program at `<data root>\app`; the tree listing drops `mods/active/<gameid>/` and adds `dolphin-users/<name>/Load/Textures/<gameid>/  the composed tree Dolphin loads (one per user folder)`; the "Ownership classes" table: Derived = `library/work/`, `cache/`, `mods/staging/`, and the composed folders in user folders; the `%APPDATA%\NFL Street Reborn root` row becomes: "The stable data root since 2026-09-12. A 2.1 install found there is moved into `Reborn 2.1\` by `layout` before anything else runs (renames only); migration reads from there. See the 2026-09-12 spec." Add a short "Path law" paragraph under "Mods compose, they never overwrite": Dolphin.ini carries no path keys; every path derives from `-u`.

- [ ] **Step 3:** CONTENT_AND_UPDATE_MODEL.md: under section 10 "Layout" add "2026-09-12: the composed tree is `<user folder>/Load/Textures/<gameid>`; the composed record carries the textures pointer identity so a swap rebuilds it." Under section 16 add "2026-09-12: DataDir is `NFL Street Reborn[ Beta]`; the old `Launcher` roots are adopted by the launcher and the old `app` uninstalled by the installer."

- [ ] **Step 4:** `Tools\test-all.ps1` (no `-Browser`): all green. Commit: `git commit -m "Docs and scripts follow the new layout"`.

---

### Task 11: Full verification of the branch

- [ ] **Step 1:** Kill any running `reborn-launcher.exe`, `rebornctl.exe`, `reborn-publisher.exe`. Run `cargo test --workspace` and `cargo clippy --workspace --all-targets`. Expected: every test passes, no new clippy warnings (compare the warning count against `integration` if in doubt: `git stash` is forbidden; use `git worktree` `target\merge-tree` for the baseline count).
- [ ] **Step 2:** `.\Tools\test-all.ps1 -Browser`. Expected: rust, checkers, browser suites, installer inputs all PASS. Paste the summary lines into the commit message of a small "test-all green" note only if something needed fixing; otherwise no commit.
- [ ] **Step 3:** `cargo build --release -p reborn-ui -p reborn-cli` so the walk uses fresh binaries.

---

### Task 12: Build a payload for the walk

**Files:**
- Uses: `installer/windows/build.ps1`

- [ ] **Step 1:** Run a beta payload build with the runtime, template, textures, mod packs and patch chain, without compiling the installer if the script offers a payload-only switch (grep `build.ps1` for `-PayloadOnly` or `-NoCompile`; if none, run the full `-Channel beta` build, ~20 min, via `schtasks` as the memory rule says for long builds). Inputs, from the memories:
  - `-Runtime "C:\Reborn Origin-stage\runtime-3.1.0-clean"`
  - `-UserTemplate "C:\Users\Adam\nflstreet2_clean\DOLPHIN ONLINE TEST\NFL Street Reborn Dolphin\User"`
  - `-Textures "C:\Reborn Publisher\textures\GN7E69"` (the drop folder, hardlinks)
  - `-ModPackages "C:\Reborn Publisher\mods\packages"`
  - `-GamePatchChain "C:\Reborn Origin-stage\game-patch"`
  - `-GamePatch "C:\Reborn Origin-stage\game-import-bundle-nsrv3"`
  - `-PromptPackages` as the last beta build used (see `C:\Users\Adam\.claude\jobs\f4d73a58\tmp\build_beta3_full.ps1` for the exact arguments)
  - `-Channel beta -Version 3.0.1-beta.24`
- [ ] **Step 2:** Confirm `target\installer-payload\runtime\User` holds `Config\*.ini` (13), `GameSettings\GN7E69.ini`, `Cache\*.uidcache`, `Cache\Shaders\*.cache`, and that `Config\Dolphin.ini` has no `LoadPath`, no `DefaultISO`, no `ID =`, and `SlotA = 1`.

---

### Task 13: Real-window proof on a scratch root with a fake 2.1 install

**Files:**
- Create: `Tools/walk-paths-overhaul.ps1`, `Tools/drive-paths-walk.mjs`

**What the walk proves (each a PASS/FAIL line):**
1. A scratch `APPDATA` holds `NFL Street Reborn Launcher Beta\{config\settings.json (schema only), library\a.txt}` (a Launcher-era root) and `NFL Street Reborn Beta\{User\GC\MemoryCardA.USA.raw (16 MB zeros), User\Config\Dolphin.ini (a copy of the 2.1 one), Games\NSR.iso (hardlink of `C:\Users\Adam\AppData\Roaming\NFL Street Reborn\NSR.iso`), MODS\!INSTLOGS, MODS\!BACKUP, JSGME.ini, NFL Street Reborn Dolphin.exe (a copy)}` (a fake 2.1 install AT the beta data root, since the walk runs the beta channel).
2. Start `target\installer-payload\reborn-launcher.exe` with `$env:APPDATA = <scratch>`, `$env:REBORN_CHANNEL = "beta"`, `$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=9333"`.
3. After 5 s: `NFL Street Reborn Beta\Reborn 2.1\{User,Games,MODS,...}` exists, the root has no `User`/`Games`, `library\a.txt` was adopted, the old root is gone or holds only what was unknown.
4. Drive onboarding: choose "I already have Reborn"; the installation step must show ready without browsing (the tucked root is the default); import runs (patch chain to NSRV3, ~90 s); controller "none"; defaults; completion; Play page.
5. `prepare_launch` (see `drive-setup-window.mjs` step 4b for the invoke) then read back from the migrated user folder: `Config\Dolphin.ini` has none of LoadPath/DumpPath/NANDRootPath/ResourcePackPath/MemcardAPath/DefaultISO and has `SlotA = 1`; `Load\Textures\GN7E69` is a real directory (`Get-Item ... | % Attributes` has no ReparsePoint) with `composed.json` and more than 8,000 entries; no `mods\active` under the data root.
6. Launch the game for real (`Play`), wait 40 s, then: `GC\MemoryCardA.USA.raw` exists in the user folder (Dolphin created the default card, or the migrated copy is there and is the one in use: assert the file's mtime moved), and Dolphin's log (`<user>\Logs\dolphin.log` when file logging is on; the launcher turns it off, so read the on-screen fact instead: take a screenshot of the game window and look at it) shows the HD textures. Then close the game.
7. After the game exits, `Config\Dolphin.ini` STILL has no path keys (the fork does not write them back). If it does, record the fact in the spec's section 4 and keep the launch-time removal as the guarantee.

- [ ] **Step 1:** Write `Tools/walk-paths-overhaul.ps1` (prepare scratch, start the launcher with the environment, run the driver, print PASS/FAIL, leave the scratch folder for inspection, kill the launcher at the end). Write `Tools/drive-paths-walk.mjs` by copying `drive-setup-window.mjs` and changing step 1 to `migrate`, adding the readbacks above via `invoke("prepare_launch", ...)` plus Node `fs` reads under the scratch root (path passed by environment variable `REBORN_WALK_ROOT`).
- [ ] **Step 2:** Run it. Fix whatever fails; re-run until every line is PASS. Keep screenshots in `target\review\paths-*.png`.
- [ ] **Step 3:** Commit the two tools: `git commit -m "Walk: tuck, adopt, migrate, compose into the user folder, path law, observed in the real window"` with the PASS lines in the message body.

---

### Task 14: Merge and hand-off

- [ ] **Step 1:** Update the spec's section 4 with what the fork was observed to do about path keys on exit (Task 13 step 7) and section 8 with the walk's result. Commit.
- [ ] **Step 2:** `git checkout integration` in `target\merge-tree`, `git merge --no-ff paths-overhaul`. Resolve conflicts (another session commits to `integration`; `mods.rs` is the likely spot). `cargo test --workspace` on `integration` after the merge.
- [ ] **Step 3:** Write the memory file (project) with: new roots, `Reborn 2.1`, composed tree in user folder, path law, template staging flag, the walk result, and the two release steps that MUST follow the build: (a) the runtime content release pinning the new `runtime\User` template, (b) beta.24 self-update so friends' roots rename. Update `MEMORY.md`.
- [ ] **Step 4:** Report to Adam: what changed, what was observed, what he must do (publish runtime, build beta.24, then stable 3.0.0 from `integration`; delete or rename his stale `NFL Street Reborn Launcher` root first if he does not want it adopted; his dev tree will be tucked when he installs stable on this PC).

---

## Self-review

**Spec coverage.** §1 names → Task 1, 9. §2 tuck/adopt before the lock → Task 4 (window and CLI mains). Supervisor wording → Task 4. Legacy root default → Task 4. §3 composed in user folder, transition, retire `mods/active`, marker identity, migration no textures → Tasks 5, 6, 7. §4 path law, memory card default, fork proof → Tasks 3, 13. §5 template staging, sanitizing, `-UserTemplate`, whole-tree seeding, runtime release law → Tasks 8, 14. §6 installer → Task 9. §7 consequences → Task 14 report. §8 proof → Tasks 11, 13.

**Placeholders.** None: every step carries code or an exact command. The `InstallDirIsSafe` sketch notes its `var` block must be typed correctly.

**Type consistency.** `manage_user_texture_folder` (Tasks 5, 6, walk), `prepare_texture_folder`, `retire_old_composed_tree`, `composed_root(user_dir, game)`, `pristine_identity`, `path_law_removals`/`apply_path_law`, `layout::{prepare_data_root, prepare_at, tuck_legacy, adopt_previous, legacy_root, same_named, LEGACY_SUBFOLDER}`, `user_template::{stage, sanitize_dolphin_ini, sanitize_gfx_ini, sanitize_qt_ini}`, `Paths::previous_data_root`, `IniDocument::remove` are spelled the same everywhere above.
