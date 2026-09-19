//! Select the launcher's effects without modifying the player's source presets.
use std::path::Path;
use crate::{ini::{IniDocument, Overlay}, platform::Result, runtime::GraphicsPreset};

/// The GameCube's native framebuffer, which internal resolution multiplies.
const NATIVE: (u32, u32) = (640, 528);

/// ReShade's injector. Dolphin finds it beside the executable before the system's own.
const INJECTOR: &str = "dxgi.dll";
/// Where Performance keeps the injector while ReShade is out of the runtime.
const PARKED_INJECTOR: &str = "dxgi.dll.performance-off";

/// Carried by a Dolphin fork whose field/HUD split does nothing without ReShade.
///
/// Byte-identical to `kInertWithoutReShade` in the fork's `NflsUiSplit.cpp`.
/// An older fork runs its whole boundary pass when the add-on is missing: it
/// resamples the field through a display-sized target and back, which is the
/// scanline round trip. Taking ReShade out from under one of those would make
/// Performance look worse rather than run faster.
const INERT_WITHOUT_RESHADE: &[u8] = b"NSR-UISPLIT-INERT-WITHOUT-RESHADE";

fn inert_without_reshade(runtime: &Path) -> bool {
    std::fs::read(runtime).is_ok_and(|exe| {
        exe.windows(INERT_WITHOUT_RESHADE.len()).any(|w| w == INERT_WITHOUT_RESHADE)
    })
}

/// Put ReShade's injector back, or take it out of the runtime.
///
/// An empty preset is not ReShade off. ReShade is still in the process: every
/// Direct3D call goes through its wrappers, the menu add-on hooks every
/// render-target bind, and its depth tracking watches every draw, all of which
/// grows with what is on screen. Renaming the injector means Windows loads its
/// own `dxgi.dll` and ReShade never starts.
///
/// A Repair or a runtime update restores the file, because the manifest lists
/// it; the next Performance launch parks the fresh copy again.
fn set_injector(root: &Path, loaded: bool) -> Result<()> {
    let live = root.join(INJECTOR);
    let parked = root.join(PARKED_INJECTOR);
    if loaded {
        if !parked.is_file() { return Ok(()); }
        if live.is_file() {
            // Repaired or updated while parked: the parked copy is the stale one.
            std::fs::remove_file(&parked)?;
        } else {
            std::fs::rename(&parked, &live)?;
        }
    } else if live.is_file() {
        // Replaces any older parked copy with what the last update delivered.
        std::fs::rename(&live, &parked)?;
    }
    Ok(())
}

/// The `[DEPTH]` filter for a given internal resolution.
///
/// ReShade's depth detection is filtered to an exact buffer size, and the live
/// `ReShade.ini` runs in exact-match mode (`UseAspectRatioHeuristics=4`). The
/// shipped `User` template records what happens when the two disagree: the
/// filter "matches nothing, and depth effects vanish". That is why internal
/// resolution used to be pinned at 3 in the layer nothing could beat.
///
/// Deriving the filter is what lets the resolution belong to the player. It is
/// not an improvement to the preset system, it is the precondition for the
/// whole graphics half of this fix.
pub fn depth_filter_for(internal_resolution: u8) -> (u32, u32) {
    let multiplier = u32::from(internal_resolution.max(1));
    (NATIVE.0 * multiplier, NATIVE.1 * multiplier)
}

/// The resolution the player is actually running at, read from their own
/// `GFX.ini`. Falls back to the Reborn default when they have not set one.
pub fn internal_resolution_of(user_dir: &Path) -> u8 {
    std::fs::read_to_string(user_dir.join("Config/GFX.ini"))
        .ok()
        .and_then(|text| {
            IniDocument::parse(&text)
                .get("Settings", "InternalResolution")
                .and_then(|v| v.trim().parse::<u8>().ok())
        })
        .filter(|r| (1..=8).contains(r))
        .unwrap_or(3)
}

pub fn apply(runtime: &Path, preset: GraphicsPreset, internal_resolution: u8) -> Result<()> {
    if !cfg!(windows) { return Ok(()); }
    let Some(root) = runtime.parent() else { return Ok(()); };
    let config = root.join("ReShade.ini");
    // A runtime without ReShade remains usable; this does not install its injector.
    if !config.is_file() { return Ok(()); }
    manage_stage_presets(root, true)?;
    let contents = match preset {
        GraphicsPreset::Performance => "Techniques=\nTechniqueSorting=\n",
        GraphicsPreset::Balanced => include_str!("../assets/reshade-light.ini"),
        GraphicsPreset::Quality | GraphicsPreset::Ultra => include_str!("../assets/reshade-full.ini"),
    };
    let presets = root.join("reshade-presets");
    std::fs::create_dir_all(&presets)?;
    let selected = presets.join("Reborn Launcher.ini");

    // Write the tier's preset only while the file still matches what the
    // launcher last wrote there.
    //
    // ReShade saves the player's in-overlay tuning straight back into this
    // file, and the launcher pointed PresetPath at it, so every effect value a
    // player adjusted was overwritten on the next launch. Worse, the bundled
    // asset changes between releases, so a launcher update reverted every
    // installed player's preset whether they had touched it or not.
    let sidecar = presets.join("Reborn Launcher.ini.launcher-wrote");
    let live = std::fs::read_to_string(&selected).ok();
    let ours = std::fs::read_to_string(&sidecar).ok();
    let untouched = match (&live, &ours) {
        (None, _) => true,
        (Some(live), Some(ours)) => live == ours,
        // No sidecar yet: an install from before this existed. Adopt whatever
        // is there as the player's rather than overwriting it once more.
        (Some(_), None) => false,
    };
    if untouched && live.as_deref() != Some(contents) {
        std::fs::write(&selected, contents)?;
        std::fs::write(&sidecar, contents)?;
    } else if ours.is_none() {
        // Record what is there so the next launch can tell ours from theirs.
        if let Some(live) = &live { std::fs::write(&sidecar, live)?; }
    }
    let original = std::fs::read_to_string(&config)?;
    let backup = root.join("ReShade.ini.launcher-original");
    if !backup.exists() { std::fs::write(&backup, &original)?; }
    let mut doc = IniDocument::parse(&original);
    let mut overlay = Overlay::new();
    // Which preset is active is the player's: the stage add-on writes it
    // during play, and the launcher used to force it back on every launch.
    // Only claimed when it is unset or names a file that is not there, which
    // is the case a forced value genuinely fixes.
    let mut seed = Overlay::new();
    for key in ["PresetPath", "StartupPresetPath"] {
        let names_a_missing_file = doc
            .get("GENERAL", key)
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .is_some_and(|v| !root.join(v.trim_start_matches(r".\").trim_start_matches("./")).exists() && !Path::new(v).exists());
        if names_a_missing_file {
            overlay.entry("GENERAL".into()).or_default().insert(key.into(), selected.to_string_lossy().into_owned());
        } else {
            seed.entry("GENERAL".into()).or_default().insert(key.into(), selected.to_string_lossy().into_owned());
        }
    }
    let general = overlay.entry("GENERAL".into()).or_default();
    // Restore the bundled search paths when leaving imported graphics.
    let baseline=IniDocument::parse(&std::fs::read_to_string(&backup)?);
    for key in ["EffectSearchPaths","TextureSearchPaths"] {if let Some(value)=baseline.get("GENERAL",key){general.insert(key.into(),value.into());}}
    // ReShade scans every shader when a preset has no techniques, even with
    // SkipLoadingDisabledEffects enabled. Keep the menu add-on available but
    // give Performance an empty search directory so it does no shader work.
    if matches!(preset, GraphicsPreset::Performance) {
        let empty = presets.join("disabled-effects");
        std::fs::create_dir_all(&empty)?;
        general.insert("EffectSearchPaths".into(), empty.to_string_lossy().into_owned());
    } else if let Some(staged) = stage_active_effects(root, contents) {
        // Every other tier gets the same treatment, for the same reason.
        general.insert("EffectSearchPaths".into(), staged.to_string_lossy().into_owned());
    } else if baseline.get("GENERAL", "EffectSearchPaths").is_none() {
        general.insert("EffectSearchPaths".into(), root.join("reshade-shaders/Shaders/**").to_string_lossy().into_owned());
    }
    // An empty technique list and disabled-effect skipping keep Off free of effects,
    // even when the game's shaderless-menu add-on toggles the global effect state.
    general.insert("SkipLoadingDisabledEffects".into(), "1".into());
    // Where ReShade keeps the compiled effects it does not want to build twice.
    //
    // Forced, because this is a machine's own path and the shipped ReShade.ini
    // carried a developer's: `C:\Users\Adam\AppData\Local\Temp\ReShade`, a
    // directory no player has. A cache that cannot be written is a cache that
    // never hits, so every launch recompiled from nothing. Unset is no better:
    // ReShade then falls back to the system temp directory, which Windows
    // clears on its own schedule and takes the cache with it.
    //
    // Beside the runtime rather than inside it, so a runtime update replaces
    // the emulator without also throwing away the player's compiled effects.
    if let Some(cache) = root.parent().map(|d| d.join("cache/reshade")) {
        let _ = std::fs::create_dir_all(&cache);
        general.insert("IntermediateCachePath".into(), cache.to_string_lossy().into_owned());
    }
    // Derived from the resolution the player is running at, not pinned to 3x.
    let (width, height) = depth_filter_for(internal_resolution);
    overlay.entry("DEPTH".into()).or_default().extend([
        ("FilterResolutionWidth".into(), width.to_string()),
        ("FilterResolutionHeight".into(), height.to_string()),
    ]);
    let mut changed = !doc.merge(&overlay).is_noop();
    changed |= !doc.merge_absent(&seed).is_noop();
    if changed { std::fs::write(&config, doc.render())?; }
    // Last, so the empty preset above is still what an older fork gets: on one
    // of those ReShade has to stay loaded for its split to stand down.
    let off = matches!(preset, GraphicsPreset::Performance) && inert_without_reshade(runtime);
    set_injector(root, !off)
}

/// The effect files a preset's `Techniques` line actually turns on.
///
/// Read straight from the text rather than through `IniDocument`, because a
/// preset keeps these at the top of the file with no section header above them.
fn enabled_effects(preset: &str) -> Vec<String> {
    preset
        .lines()
        .find_map(|line| line.trim().strip_prefix("Techniques="))
        .map(|value| {
            value
                .split(',')
                .filter_map(|technique| technique.rsplit('@').next())
                .map(str::trim)
                .filter(|name| name.ends_with(".fx"))
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

/// The file names an effect includes, without their directories.
fn includes_of(source: &str) -> Vec<String> {
    source
        .lines()
        .filter_map(|line| line.trim().strip_prefix("#include"))
        .filter_map(|rest| {
            let rest = rest.trim();
            let close = match rest.chars().next()? { '"' => '"', '<' => '>', _ => return None };
            let rest = &rest[1..];
            let name = &rest[..rest.find(close)?];
            Some(name.rsplit(['/', '\\']).next()?.to_owned())
        })
        .collect()
}

/// Every `.fx` and `.fxh` under the bundled shader tree, by file name.
fn shader_index(dir: &Path, found: &mut std::collections::HashMap<String, std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() { shader_index(&path, found); continue; }
        let is_shader = path.extension().is_some_and(|e| e == "fx" || e == "fxh");
        if let (true, Some(name)) = (is_shader, path.file_name().and_then(|n| n.to_str())) {
            found.entry(name.to_owned()).or_insert(path);
        }
    }
}

/// Stage only the effects a preset enables, plus the headers they include,
/// into one flat directory, and return it for `EffectSearchPaths`.
///
/// `EffectSearchPaths` is a glob. `reshade-shaders/Shaders/**` hands ReShade
/// all 87 bundled effects to enumerate whichever tier is active, so Quality and
/// Ultra paid ~80 file loads to turn on ten techniques. That is the "80 files
/// compiling" a player watches on a cold cache, and in Netplay every other
/// player waits at the boot barrier while they do it. `SkipLoadingDisabledEffects`
/// does not contain it - the launcher already had to hand Performance an empty
/// directory to get the same effect - so the search path has to.
///
/// `None` whenever anything is missing, which leaves the bundled glob in place:
/// a staging problem must cost a player some load time, never their effects.
fn stage_active_effects(root: &Path, preset: &str) -> Option<std::path::PathBuf> {
    let shaders = root.join("reshade-shaders/Shaders");
    if !shaders.is_dir() { return None; }
    let wanted = enabled_effects(preset);
    if wanted.is_empty() { return None; }
    let mut index = std::collections::HashMap::new();
    shader_index(&shaders, &mut index);

    // Resolve the effects and everything they include before touching disk, so
    // a name this tree does not carry cannot leave a half-built directory.
    let mut staged: Vec<&std::path::PathBuf> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut queue = wanted.clone();
    while let Some(name) = queue.pop() {
        if !seen.insert(name.clone()) { continue; }
        let Some(source) = index.get(&name) else {
            // A missing effect means this preset does not belong to this tree.
            // A missing header is not in the tree at all, so the glob would not
            // have found it either: skip it and let the compiler report it.
            if wanted.contains(&name) { return None; }
            continue;
        };
        staged.push(source);
        if let Ok(text) = std::fs::read_to_string(source) { queue.extend(includes_of(&text)); }
    }

    // Rebuilt only when the set changes. Restaging on every launch would rewrite
    // the paths ReShade hashes into its effect cache and cost the player the
    // very recompile this exists to avoid.
    let directory = root.join("reshade-shaders/active");
    let mut names: Vec<&str> = seen.iter().map(String::as_str).collect();
    names.sort_unstable();
    let manifest = directory.join(".staged");
    let wanted_manifest = names.join("\n");
    if std::fs::read_to_string(&manifest).ok().as_deref() == Some(wanted_manifest.as_str()) {
        return Some(directory);
    }
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).ok()?;
    for source in staged {
        std::fs::copy(source, directory.join(source.file_name()?)).ok()?;
    }
    std::fs::write(&manifest, &wanted_manifest).ok()?;
    Some(directory)
}

// The menu gate must remain active, but stage switching must not replace the
// player's Performance/Balanced/All-Out choice with an unrelated venue preset.
fn manage_stage_presets(root: &Path, managed: bool) -> Result<()> {
    let file = root.join("NSRShaderlessMenu.ini");
    if !file.is_file() { return Ok(()); }
    let original = std::fs::read_to_string(&file)?;
    let mut doc = IniDocument::parse(&original);
    let backup = root.join("NSRShaderlessMenu.ini.launcher-original");
    let value = if managed {
        // Older add-ons without this feature need no configuration change.
        if doc.get("Presets", "Enabled").is_none() { return Ok(()); }
        if !backup.exists() { std::fs::write(&backup, &original)?; }
        "0".to_owned()
    } else {
        if !backup.is_file() { return Ok(()); }
        let baseline = IniDocument::parse(&std::fs::read_to_string(&backup)?);
        let Some(value) = baseline.get("Presets", "Enabled") else { return Ok(()); };
        value.to_owned()
    };
    let mut overlay = Overlay::new();
    overlay.entry("Presets".into()).or_default().insert("Enabled".into(), value);
    if !doc.merge(&overlay).is_noop() { std::fs::write(file, doc.render())?; }
    Ok(())
}

#[cfg(test)]
mod depth_filter_tests {
    use super::*;

    /// The precondition for internal resolution belonging to the player.
    ///
    /// ReShade's depth detection runs in exact-match mode, so the filter has
    /// to be the buffer the game is actually rendering. Pinned at 1920x1584 it
    /// matched only 3x, which is why 3x used to be forced in the layer nothing
    /// could beat. If this test fails, resolution must go back to being forced
    /// rather than shipping a build where changing it makes every depth-driven
    /// effect silently disappear.
    #[test]
    fn the_reshade_depth_filter_follows_the_players_resolution() {
        assert_eq!(depth_filter_for(1), (640, 528), "native");
        assert_eq!(depth_filter_for(2), (1280, 1056));
        assert_eq!(depth_filter_for(3), (1920, 1584), "what was hardcoded");
        assert_eq!(depth_filter_for(4), (2560, 2112));
        assert_eq!(depth_filter_for(6), (3840, 3168));
        // A nonsense value must not produce a zero-sized filter, which would
        // match nothing at all.
        assert_eq!(depth_filter_for(0), (640, 528));
    }

    /// Read from the player's own GFX.ini, because that is now the only place
    /// the resolution lives.
    #[test]
    fn the_resolution_comes_from_the_players_own_config() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("Config")).unwrap();
        assert_eq!(internal_resolution_of(dir.path()), 3, "no file: the Reborn default");

        let gfx = dir.path().join("Config/GFX.ini");
        std::fs::write(&gfx, "[Settings]\nInternalResolution = 4\n").unwrap();
        assert_eq!(internal_resolution_of(dir.path()), 4);
        assert_eq!(depth_filter_for(internal_resolution_of(dir.path())), (2560, 2112));

        std::fs::write(&gfx, "[Settings]\nInternalResolution = 1\n").unwrap();
        assert_eq!(internal_resolution_of(dir.path()), 1);

        // Dolphin writes "0" for Auto, which is not a multiplier we can derive
        // a filter from; fall back rather than compute a wrong one.
        std::fs::write(&gfx, "[Settings]\nInternalResolution = 0\n").unwrap();
        assert_eq!(internal_resolution_of(dir.path()), 3);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stage_switching_cannot_override_managed_presets_and_import_restores_it() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("NSRShaderlessMenu.ini");
        let original = "[NSRShaderlessMenu]\nGate = menuonly\nHoldFrames = 20\n[Presets]\nEnabled = 1\nField00 = MyField.ini\n";
        std::fs::write(&file, original).unwrap();
        manage_stage_presets(dir.path(), true).unwrap();
        manage_stage_presets(dir.path(), true).unwrap();
        let doc = IniDocument::parse(&std::fs::read_to_string(&file).unwrap());
        assert_eq!(doc.get("Presets", "Enabled"), Some("0"));
        assert_eq!(doc.get("Presets", "Field00"), Some("MyField.ini"));
        assert_eq!(doc.get("NSRShaderlessMenu", "Gate"), Some("menuonly"));
        assert_eq!(std::fs::read_to_string(dir.path().join("NSRShaderlessMenu.ini.launcher-original")).unwrap(), original);
        manage_stage_presets(dir.path(), false).unwrap();
        assert_eq!(IniDocument::parse(&std::fs::read_to_string(&file).unwrap()).get("Presets", "Enabled"), Some("1"));
        // A player who already disabled stage presets keeps that choice.
        let disabled = tempfile::tempdir().unwrap();
        std::fs::write(disabled.path().join("NSRShaderlessMenu.ini"), "[Presets]\nEnabled = 0\n").unwrap();
        manage_stage_presets(disabled.path(), true).unwrap();
        manage_stage_presets(disabled.path(), false).unwrap();
        assert_eq!(IniDocument::parse(&std::fs::read_to_string(disabled.path().join("NSRShaderlessMenu.ini")).unwrap()).get("Presets", "Enabled"), Some("0"));
    }
    #[test]
    fn switches_effects_preserving_other_settings_and_original() {
        if !cfg!(windows) { return; }
        let dir = tempfile::tempdir().unwrap();
        let runtime = dir.path().join("Dolphin.exe");
        std::fs::write(dir.path().join("ReShade.ini"), "[GENERAL]\nEffectSearchPaths=custom\n").unwrap();
        apply(&runtime, GraphicsPreset::Balanced, 3).unwrap();
        let selected = dir.path().join("reshade-presets/Reborn Launcher.ini");
        let light = std::fs::read_to_string(&selected).unwrap();
        assert!(light.contains("Techniques=MXAO"));
        apply(&runtime, GraphicsPreset::Quality, 3).unwrap();
        assert_ne!(light, std::fs::read_to_string(&selected).unwrap());
        apply(&runtime, GraphicsPreset::Performance, 3).unwrap();
        assert_eq!(std::fs::read_to_string(&selected).unwrap(), "Techniques=\nTechniqueSorting=\n");
        let empty = dir.path().join("reshade-presets").join("disabled-effects");
        assert!(empty.is_dir());
        let off = IniDocument::parse(&std::fs::read_to_string(dir.path().join("ReShade.ini")).unwrap());
        assert_eq!(off.get("GENERAL", "EffectSearchPaths"), Some(empty.to_string_lossy().as_ref()));
        apply(&runtime, GraphicsPreset::Balanced, 3).unwrap();
        let enabled = IniDocument::parse(&std::fs::read_to_string(dir.path().join("ReShade.ini")).unwrap());
        assert_eq!(enabled.get("GENERAL", "EffectSearchPaths"), Some("custom"));
        apply(&runtime, GraphicsPreset::Performance, 3).unwrap();
        apply_imported(&runtime, "[GENERAL]\nPresetPath=previous.ini\n", 3).unwrap();
        let imported = IniDocument::parse(&std::fs::read_to_string(dir.path().join("ReShade.ini")).unwrap());
        assert_eq!(imported.get("GENERAL", "EffectSearchPaths"), Some("custom"));
        assert_eq!(imported.get("GENERAL", "PresetPath"), Some("previous.ini"));
        assert_eq!(std::fs::read_to_string(dir.path().join("ReShade.ini.launcher-original")).unwrap(), "[GENERAL]\nEffectSearchPaths=custom\n");
    }

    /// A runtime laid out like the shipped one: the fork, ReShade's injector
    /// beside it, and ReShade's own config.
    fn runtime_with(dir: &Path, fork_is_inert: bool) -> std::path::PathBuf {
        let exe = dir.join("NFL Street Reborn Dolphin.exe");
        let marker: &[u8] = if fork_is_inert { INERT_WITHOUT_RESHADE } else { b"an older fork" };
        std::fs::write(&exe, [b"MZ\0\0".as_slice(), marker, b"\0\0".as_slice()].concat()).unwrap();
        std::fs::write(dir.join("ReShade.ini"), "[GENERAL]\n").unwrap();
        std::fs::write(dir.join(INJECTOR), "reshade").unwrap();
        exe
    }

    /// Performance is ReShade out of the process, not ReShade with an empty preset.
    #[test]
    fn performance_takes_reshade_out_of_the_runtime_and_every_other_tier_puts_it_back() {
        if !cfg!(windows) { return; }
        let dir = tempfile::tempdir().unwrap();
        let runtime = runtime_with(dir.path(), true);
        let injector = dir.path().join(INJECTOR);
        let parked = dir.path().join(PARKED_INJECTOR);

        apply(&runtime, GraphicsPreset::Performance, 3).unwrap();
        assert!(!injector.exists(), "Windows must load its own dxgi.dll, so ReShade never starts");
        assert_eq!(std::fs::read_to_string(&parked).unwrap(), "reshade", "kept, not deleted");
        apply(&runtime, GraphicsPreset::Performance, 3).unwrap();
        assert!(!injector.exists() && parked.is_file(), "a second launch changes nothing");

        for tier in [GraphicsPreset::Balanced, GraphicsPreset::Quality, GraphicsPreset::Ultra] {
            apply(&runtime, GraphicsPreset::Performance, 3).unwrap();
            apply(&runtime, tier, 3).unwrap();
            assert_eq!(std::fs::read_to_string(&injector).unwrap(), "reshade", "{tier:?} needs ReShade");
            assert!(!parked.exists());
        }

        apply(&runtime, GraphicsPreset::Performance, 3).unwrap();
        apply_imported(&runtime, "[GENERAL]\nPresetPath=previous.ini\n", 3).unwrap();
        assert!(injector.is_file(), "imported settings are ReShade settings");
    }

    /// An older fork resamples the field when ReShade is missing, so under one
    /// of those Performance stays what it was: ReShade loaded, nothing enabled.
    #[test]
    fn an_older_fork_keeps_reshade_loaded_on_performance() {
        if !cfg!(windows) { return; }
        let dir = tempfile::tempdir().unwrap();
        let runtime = runtime_with(dir.path(), false);
        apply(&runtime, GraphicsPreset::Performance, 3).unwrap();
        assert!(dir.path().join(INJECTOR).is_file());
        assert_eq!(
            std::fs::read_to_string(dir.path().join("reshade-presets/Reborn Launcher.ini")).unwrap(),
            "Techniques=\nTechniqueSorting=\n",
        );

        // Rolled back from a newer runtime with the injector parked.
        std::fs::rename(dir.path().join(INJECTOR), dir.path().join(PARKED_INJECTOR)).unwrap();
        apply(&runtime, GraphicsPreset::Performance, 3).unwrap();
        assert!(dir.path().join(INJECTOR).is_file(), "the rollback must get ReShade back");
    }

    /// Repair and runtime updates restore the injector, because the manifest
    /// lists it. What they delivered is the copy to keep.
    #[test]
    fn a_repaired_injector_replaces_the_parked_one() {
        if !cfg!(windows) { return; }
        let dir = tempfile::tempdir().unwrap();
        let runtime = runtime_with(dir.path(), true);
        let injector = dir.path().join(INJECTOR);
        let parked = dir.path().join(PARKED_INJECTOR);
        apply(&runtime, GraphicsPreset::Performance, 3).unwrap();

        std::fs::write(&injector, "reshade, updated").unwrap();
        apply(&runtime, GraphicsPreset::Performance, 3).unwrap();
        assert!(!injector.exists());
        assert_eq!(std::fs::read_to_string(&parked).unwrap(), "reshade, updated");

        std::fs::write(&injector, "reshade, repaired").unwrap();
        apply(&runtime, GraphicsPreset::Balanced, 3).unwrap();
        assert_eq!(std::fs::read_to_string(&injector).unwrap(), "reshade, repaired");
        assert!(!parked.exists(), "the stale parked copy goes");
    }
}

pub fn apply_imported(runtime:&Path,settings:&str,internal_resolution:u8)->Result<()> {
    let Some(root)=runtime.parent()else{return Ok(())};let file=root.join("ReShade.ini");if !file.exists(){return Ok(())}
    manage_stage_presets(root, false)?;
    // Imported settings are ReShade settings: they need ReShade loaded.
    set_injector(root, true)?;
    let original=std::fs::read_to_string(&file)?;
    let backup=root.join("ReShade.ini.launcher-original");if !backup.exists(){std::fs::write(&backup,&original)?;}
    let imported=IniDocument::parse(settings);let mut overlay=Overlay::new();
    for key in ["PresetPath","StartupPresetPath","EffectSearchPaths","TextureSearchPaths"]{if let Some(value)=imported.get("GENERAL",key){overlay.entry("GENERAL".into()).or_default().insert(key.into(),value.into());}}
    if imported.get("GENERAL", "EffectSearchPaths").is_none() {
        let baseline = IniDocument::parse(&std::fs::read_to_string(&backup)?);
        let paths = baseline.get("GENERAL", "EffectSearchPaths").map(str::to_owned)
            .unwrap_or_else(|| root.join("reshade-shaders/Shaders/**").to_string_lossy().into_owned());
        overlay.entry("GENERAL".into()).or_default().insert("EffectSearchPaths".into(), paths);
    }
    let mut doc=IniDocument::parse(&std::fs::read_to_string(&file)?);
    // Derived, not pinned: see depth_filter_for.
    let (width,height)=depth_filter_for(internal_resolution);
    overlay.entry("DEPTH".into()).or_default().extend([("FilterResolutionWidth".into(),width.to_string()),("FilterResolutionHeight".into(),height.to_string())]);
    if !doc.merge(&overlay).is_noop(){std::fs::write(file,doc.render())?;}Ok(())
}

#[cfg(test)]
mod staging_tests {
    use super::*;

    /// A tree shaped like the bundled one: the enabled effects sit in
    /// subdirectories, their headers sit above them, and far more effects are
    /// present than any preset turns on.
    fn tree(root: &Path) {
        let shaders = root.join("reshade-shaders/Shaders");
        for dir in ["qUINT", "NFLStreetReborn", "PD80"] {
            std::fs::create_dir_all(shaders.join(dir)).unwrap();
        }
        std::fs::write(shaders.join("ReShade.fxh"), "// shared\n").unwrap();
        std::fs::write(shaders.join("ReShadeUI.fxh"), "// shared ui\n").unwrap();
        std::fs::write(shaders.join("qUINT/qUINT_common.fxh"), "// qUINT\n").unwrap();
        std::fs::write(shaders.join("qUINT/qUINT_mxao.fx"), "#include \"ReShade.fxh\"\n#include \"qUINT_common.fxh\"\n").unwrap();
        std::fs::write(shaders.join("NFLStreetReborn/NSR_DepthInk.fx"), "#include \"ReShadeUI.fxh\"\n").unwrap();
        // The 70-odd nobody enabled.
        for n in 0..70 { std::fs::write(shaders.join(format!("PD80/PD80_{n:02}.fx")), "// unused\n").unwrap(); }
    }

    const PRESET: &str = "Techniques=MXAO@qUINT_mxao.fx,NSR_DepthInk@NSR_DepthInk.fx\nTechniqueSorting=MXAO@qUINT_mxao.fx\n";

    /// The whole point: what ReShade is handed to enumerate is the enabled set
    /// and its headers, not every effect that happens to be bundled.
    #[test]
    fn staging_hands_reshade_only_what_the_preset_turns_on() {
        let dir = tempfile::tempdir().unwrap();
        tree(dir.path());
        let staged = stage_active_effects(dir.path(), PRESET).expect("staged");
        let mut names: Vec<String> = std::fs::read_dir(&staged).unwrap()
            .flatten()
            .filter_map(|e| e.file_name().to_str().map(str::to_owned))
            .filter(|n| n != ".staged")
            .collect();
        names.sort();
        assert_eq!(names, [
            "NSR_DepthInk.fx", "ReShade.fxh", "ReShadeUI.fxh",
            "qUINT_common.fxh", "qUINT_mxao.fx",
        ], "only the enabled effects and the headers they include");
        assert_eq!(names.iter().filter(|n| n.ends_with(".fx")).count(), 2, "72 effects present, 2 staged");
    }

    /// A staging problem must cost load time, never a player's effects.
    #[test]
    fn a_preset_this_tree_cannot_satisfy_keeps_the_bundled_glob() {
        let dir = tempfile::tempdir().unwrap();
        tree(dir.path());
        assert!(stage_active_effects(dir.path(), "Techniques=X@NotHere.fx\n").is_none());
        assert!(stage_active_effects(dir.path(), "Techniques=\n").is_none(), "an empty preset is Performance's job, not staging's");
    }

    /// Restaging would rewrite the paths ReShade hashes into its effect cache,
    /// which is the recompile this exists to prevent.
    #[test]
    fn an_unchanged_set_is_not_rebuilt() {
        let dir = tempfile::tempdir().unwrap();
        tree(dir.path());
        let staged = stage_active_effects(dir.path(), PRESET).unwrap();
        let marker = staged.join("qUINT_mxao.fx");
        let before = std::fs::metadata(&marker).unwrap().modified().unwrap();
        assert_eq!(stage_active_effects(dir.path(), PRESET).unwrap(), staged);
        assert_eq!(std::fs::metadata(&marker).unwrap().modified().unwrap(), before, "the same set must not be recopied");
    }
    /// The shipped `ReShade.ini` carried a developer's own cache directory, so
    /// no player's cache could ever be written, let alone hit.
    #[test]
    fn the_effect_cache_lands_somewhere_the_player_can_write() {
        if !cfg!(windows) { return; }
        let dir = tempfile::tempdir().unwrap();
        let runtime = dir.path().join("runtime");
        std::fs::create_dir_all(&runtime).unwrap();
        let theirs = r"C:\Users\SomeoneElse\AppData\Local\Temp\ReShade";
        std::fs::write(
            runtime.join("ReShade.ini"),
            format!("[GENERAL]\nIntermediateCachePath={theirs}\n"),
        ).unwrap();
        apply(&runtime.join("Dolphin.exe"), GraphicsPreset::Balanced, 3).unwrap();
        let live = std::fs::read_to_string(runtime.join("ReShade.ini")).unwrap();
        let cache = IniDocument::parse(&live).get("GENERAL", "IntermediateCachePath").unwrap().to_owned();
        assert!(!cache.contains("SomeoneElse"), "a developer's path must not survive: {cache}");
        assert_eq!(std::path::Path::new(&cache), dir.path().join("cache/reshade"));
        assert!(std::path::Path::new(&cache).is_dir(), "and it must exist to be written to");
        // Beside the runtime, so replacing the emulator keeps the compiled effects.
        assert!(!cache.starts_with(runtime.to_string_lossy().as_ref()));
    }

}
