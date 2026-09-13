//! Select the launcher's effects without modifying the player's source presets.
use std::path::Path;
use crate::{ini::{IniDocument, Overlay}, platform::Result, runtime::GraphicsPreset};

/// The GameCube's native framebuffer, which internal resolution multiplies.
const NATIVE: (u32, u32) = (640, 528);

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
    } else if baseline.get("GENERAL", "EffectSearchPaths").is_none() {
        general.insert("EffectSearchPaths".into(), root.join("reshade-shaders/Shaders/**").to_string_lossy().into_owned());
    }
    // An empty technique list and disabled-effect skipping keep Off free of effects,
    // even when the game's shaderless-menu add-on toggles the global effect state.
    general.insert("SkipLoadingDisabledEffects".into(), "1".into());
    // Derived from the resolution the player is running at, not pinned to 3x.
    let (width, height) = depth_filter_for(internal_resolution);
    overlay.entry("DEPTH".into()).or_default().extend([
        ("FilterResolutionWidth".into(), width.to_string()),
        ("FilterResolutionHeight".into(), height.to_string()),
    ]);
    let mut changed = !doc.merge(&overlay).is_noop();
    changed |= !doc.merge_absent(&seed).is_noop();
    if changed { std::fs::write(&config, doc.render())?; }
    Ok(())
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
}

pub fn apply_imported(runtime:&Path,settings:&str,internal_resolution:u8)->Result<()> {
    let Some(root)=runtime.parent()else{return Ok(())};let file=root.join("ReShade.ini");if !file.exists(){return Ok(())}
    manage_stage_presets(root, false)?;
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
