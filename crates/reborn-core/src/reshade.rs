//! Select the launcher's effects without modifying the player's source presets.
use std::path::Path;
use crate::{ini::{IniDocument, Overlay}, platform::Result, runtime::GraphicsPreset};

pub fn apply(runtime: &Path, preset: GraphicsPreset) -> Result<()> {
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
    if std::fs::read_to_string(&selected).ok().as_deref() != Some(contents) {
        std::fs::write(&selected, contents)?;
    }
    let original = std::fs::read_to_string(&config)?;
    let backup = root.join("ReShade.ini.launcher-original");
    if !backup.exists() { std::fs::write(&backup, &original)?; }
    let mut doc = IniDocument::parse(&original);
    let mut overlay = Overlay::new();
    let general = overlay.entry("GENERAL".into()).or_default();
    for key in ["PresetPath", "StartupPresetPath"] {
        general.insert(key.into(), selected.to_string_lossy().into_owned());
    }
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
    overlay.entry("DEPTH".into()).or_default().extend([("FilterResolutionWidth".into(),"1920".into()),("FilterResolutionHeight".into(),"1584".into())]);
    if !doc.merge(&overlay).is_noop() { std::fs::write(&config, doc.render())?; }
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
        apply(&runtime, GraphicsPreset::Balanced).unwrap();
        let selected = dir.path().join("reshade-presets/Reborn Launcher.ini");
        let light = std::fs::read_to_string(&selected).unwrap();
        assert!(light.contains("Techniques=MXAO"));
        apply(&runtime, GraphicsPreset::Quality).unwrap();
        assert_ne!(light, std::fs::read_to_string(&selected).unwrap());
        apply(&runtime, GraphicsPreset::Performance).unwrap();
        assert_eq!(std::fs::read_to_string(&selected).unwrap(), "Techniques=\nTechniqueSorting=\n");
        let empty = dir.path().join("reshade-presets").join("disabled-effects");
        assert!(empty.is_dir());
        let off = IniDocument::parse(&std::fs::read_to_string(dir.path().join("ReShade.ini")).unwrap());
        assert_eq!(off.get("GENERAL", "EffectSearchPaths"), Some(empty.to_string_lossy().as_ref()));
        apply(&runtime, GraphicsPreset::Balanced).unwrap();
        let enabled = IniDocument::parse(&std::fs::read_to_string(dir.path().join("ReShade.ini")).unwrap());
        assert_eq!(enabled.get("GENERAL", "EffectSearchPaths"), Some("custom"));
        apply(&runtime, GraphicsPreset::Performance).unwrap();
        apply_imported(&runtime, "[GENERAL]\nPresetPath=previous.ini\n").unwrap();
        let imported = IniDocument::parse(&std::fs::read_to_string(dir.path().join("ReShade.ini")).unwrap());
        assert_eq!(imported.get("GENERAL", "EffectSearchPaths"), Some("custom"));
        assert_eq!(imported.get("GENERAL", "PresetPath"), Some("previous.ini"));
        assert_eq!(std::fs::read_to_string(dir.path().join("ReShade.ini.launcher-original")).unwrap(), "[GENERAL]\nEffectSearchPaths=custom\n");
    }
}

pub fn apply_imported(runtime:&Path,settings:&str)->Result<()> {
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
    let mut doc=IniDocument::parse(&std::fs::read_to_string(&file)?);overlay.entry("DEPTH".into()).or_default().extend([("FilterResolutionWidth".into(),"1920".into()),("FilterResolutionHeight".into(),"1584".into())]);
    if !doc.merge(&overlay).is_noop(){std::fs::write(file,doc.render())?;}Ok(())
}
