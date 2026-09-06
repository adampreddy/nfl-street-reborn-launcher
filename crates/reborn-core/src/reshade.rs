//! Select the launcher's effects without modifying the player's source presets.
use std::path::Path;
use crate::{ini::{IniDocument, Overlay}, platform::Result, runtime::GraphicsPreset};

pub fn apply(runtime: &Path, preset: GraphicsPreset) -> Result<()> {
    if !cfg!(windows) { return Ok(()); }
    let Some(root) = runtime.parent() else { return Ok(()); };
    let config = root.join("ReShade.ini");
    // A runtime without ReShade remains usable; this does not install its injector.
    if !config.is_file() { return Ok(()); }
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
    // An empty technique list and disabled-effect skipping keep Off free of effects,
    // even when the game's shaderless-menu add-on toggles the global effect state.
    general.insert("SkipLoadingDisabledEffects".into(), "1".into());
    overlay.entry("DEPTH".into()).or_default().extend([("FilterResolutionWidth".into(),"1920".into()),("FilterResolutionHeight".into(),"1584".into())]);
    if !doc.merge(&overlay).is_noop() { std::fs::write(&config, doc.render())?; }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
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
        assert!(std::fs::read_to_string(dir.path().join("ReShade.ini")).unwrap().contains("EffectSearchPaths=custom"));
        assert_eq!(std::fs::read_to_string(dir.path().join("ReShade.ini.launcher-original")).unwrap(), "[GENERAL]\nEffectSearchPaths=custom\n");
    }
}

pub fn apply_imported(runtime:&Path,settings:&str)->Result<()> {
    let Some(root)=runtime.parent()else{return Ok(())};let file=root.join("ReShade.ini");if !file.exists(){return Ok(())}
    let original=std::fs::read_to_string(&file)?;
    let backup=root.join("ReShade.ini.launcher-original");if !backup.exists(){std::fs::write(&backup,&original)?;}
    let imported=IniDocument::parse(settings);let mut overlay=Overlay::new();
    for key in ["PresetPath","StartupPresetPath","EffectSearchPaths","TextureSearchPaths"]{if let Some(value)=imported.get("GENERAL",key){overlay.entry("GENERAL".into()).or_default().insert(key.into(),value.into());}}
    let mut doc=IniDocument::parse(&std::fs::read_to_string(&file)?);overlay.entry("DEPTH".into()).or_default().extend([("FilterResolutionWidth".into(),"1920".into()),("FilterResolutionHeight".into(),"1584".into())]);
    if !doc.merge(&overlay).is_noop(){std::fs::write(file,doc.render())?;}Ok(())
}
