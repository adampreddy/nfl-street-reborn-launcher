//! Persistent per-device mappings, used both by setup and every game launch.
use crate::{controller::ControllerProfile, platform::{DataDir, Platform, Result}};
use std::path::{Path, PathBuf};
pub fn path(platform: &Platform) -> Result<PathBuf> {
    Ok(platform.paths.data_root()?.join(DataDir::Config.relative()).join("controller-profiles.json"))
}
pub fn load(file: &Path) -> Result<Vec<ControllerProfile>> {
    match std::fs::read(file) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData,e).into()),
        Err(e) if e.kind()==std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(e.into()),
    }
}
pub fn save(file: &Path, profile: ControllerProfile) -> Result<()> {
    let mut profiles=load(file)?;
    profiles.retain(|p| p.identity.profile_key()!=profile.identity.profile_key());
    profiles.push(profile);
    std::fs::create_dir_all(file.parent().unwrap())?;
    let bytes=serde_json::to_vec_pretty(&profiles).map_err(std::io::Error::other)?;
    let temporary=file.with_extension("json.tmp");
    std::fs::write(&temporary,bytes)?;
    std::fs::rename(temporary,file)?;
    Ok(())
}

/// Adopt a migrated binding set when that device first reconnects. Existing
/// launcher mappings always win, including the user's explicit reset to defaults.
pub fn adopt_migrated(platform: &Platform, scan: &crate::devices::Enumeration) -> Result<()> {
    // Profiles adopted by an earlier build may still carry the 2.1 swap.
    normalize_saved(platform)?;
    let file=path(platform)?;
    let legacy=file.with_file_name("migrated-controller.ini");
    let Ok(text)=std::fs::read_to_string(&legacy) else {return Ok(())};
    let doc=crate::ini::IniDocument::parse(&text);
    let existing=load(&file)?;
    for pad in scan.playable_pads() {
        if existing.iter().any(|p|p.identity.profile_key()==pad.identity().profile_key()){continue;}
        for port in 1..=4 {
            let section=format!("GCPad{port}");
            if doc.get(&section,"Device")!=Some(pad.device_string().as_str()){continue;}
            let mut profile=ControllerProfile::new(pad.identity(),port);
            for input in crate::controller::GcInput::ALL {
                if let Some(binding)=doc.get(&section,input.ini_key()).filter(|v|!v.is_empty()){profile.rebind(*input,binding.to_string());}
            }
            let undone=undo_legacy_bx_swap(&mut profile);
            save(&file,profile)?;
            if undone { crate::notices::add(platform, crate::notices::BX_SWAP_UNDONE)?; }
            break;
        }
    }
    Ok(())
}

/// 2.1 told players to swap B and X in Dolphin because the game had them the
/// wrong way round. NSRV3 swaps them inside the game (a match-gated pad
/// cave), so a mapping that still carries the Dolphin swap cancels it out
/// in a match and wrongly swaps the menus, which stay vanilla. Undo it: B is
/// B, X is X. True when the profile changed. Adam, 2026-09-11: "for
/// returning installs X and B are back to normal".
pub fn undo_legacy_bx_swap(profile: &mut ControllerProfile) -> bool {
    use crate::controller::GcInput::{ButtonB, ButtonX};
    let swapped=profile.mapping.get(ButtonB)==Some("`Button X`") && profile.mapping.get(ButtonX)==Some("`Button B`");
    if !swapped { return false; }
    profile.reset(&[ButtonB, ButtonX]);
    true
}

/// Every saved profile, unswapped. Returns how many changed; a change also
/// queues the notice players read once on Play and Controllers.
pub fn normalize_saved(platform: &Platform) -> Result<usize> {
    let file=path(platform)?;
    let mut changed=0;
    for mut profile in load(&file)? {
        if undo_legacy_bx_swap(&mut profile) { save(&file,profile)?; changed+=1; }
    }
    if changed>0 { crate::notices::add(platform, crate::notices::BX_SWAP_UNDONE)?; }
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{controller::{DeviceFamily, GcInput}, devices::{Backend, DetectedDevice, Enumeration}};
    #[test]
    fn a_2_1_mapping_that_swapped_b_and_x_is_put_back_and_nothing_else_is_touched() {
        let device=DetectedDevice{backend:Backend::XInput,id:0,name:"Gamepad".into(),family:DeviceFamily::Xbox};
        let mut profile=ControllerProfile::new(device.identity(),1);
        profile.rebind(GcInput::ButtonB,"`Button X`");
        profile.rebind(GcInput::ButtonX,"`Button B`");
        profile.rebind(GcInput::ButtonZ,"`Shoulder L`");
        assert!(undo_legacy_bx_swap(&mut profile), "a swapped pair is undone");
        assert_eq!(profile.mapping.get(GcInput::ButtonB),Some("`Button B`"));
        assert_eq!(profile.mapping.get(GcInput::ButtonX),Some("`Button X`"));
        assert_eq!(profile.mapping.get(GcInput::ButtonZ),Some("`Shoulder L`"),"the player's other choices stay");
        assert!(!profile.overridden.contains(&GcInput::ButtonB) && !profile.overridden.contains(&GcInput::ButtonX),"B and X count as default again");
        assert!(!undo_legacy_bx_swap(&mut profile),"nothing to undo the second time");
        // A deliberate one-sided rebind is not a 2.1 swap and is left alone.
        let mut other=ControllerProfile::new(device.identity(),1);
        other.rebind(GcInput::ButtonB,"`Button X`");
        assert!(!undo_legacy_bx_swap(&mut other));
        assert_eq!(other.mapping.get(GcInput::ButtonB),Some("`Button X`"));
    }
    #[test]
    fn four_players_keep_independent_mappings_and_disconnected_slots() {
        let devices=(0..4).map(|id|DetectedDevice{backend:Backend::XInput,id,name:"Gamepad".into(),family:DeviceFamily::Xbox}).collect::<Vec<_>>();
        let scan=Enumeration{devices:devices.clone(),not_searched:vec![]};
        let slots=std::array::from_fn(|i|Some(devices[i].device_string()));
        let profiles=devices.iter().enumerate().map(|(i,pad)|{let mut p=ControllerProfile::new(pad.identity(),i as u8+1);p.rebind(GcInput::ButtonA,format!("`Player {} A`",i+1));p}).collect::<Vec<_>>();
        let (ini,ports)=crate::session::build_gcpad_for_slots(&scan,&profiles,None,Some(&slots)).unwrap();
        let doc=crate::ini::IniDocument::parse(&ini);
        for i in 0..4 {let section=format!("GCPad{}",i+1);assert_eq!(doc.get(&section,"Device"),slots[i].as_deref());assert_eq!(doc.get(&section,"Buttons/A"),Some(format!("`Player {} A`",i+1).as_str()));}
        assert_eq!(ports.len(),4);
        let overlay=crate::session::controller_port_overlay(&ports);for i in 0..4 {assert_eq!(overlay["Core"][&format!("SIDevice{i}")],"6");}
        let disconnected=Enumeration{devices:devices.into_iter().filter(|p|p.id!=1).collect(),not_searched:vec![]};
        let (ini,ports)=crate::session::build_gcpad_for_slots(&disconnected,&profiles,None,Some(&slots)).unwrap();
        assert_eq!(ports.iter().map(|(p,_)|*p).collect::<Vec<_>>(),vec![1,3,4]);
        assert_eq!(crate::ini::IniDocument::parse(&ini).get("GCPad2","Device"),Some(""));
        assert_eq!(crate::session::controller_port_overlay(&ports)["Core"]["SIDevice1"],"0");
    }
    #[test]
    fn explicit_slots_persist_and_never_bind_a_device_twice() {
        let device=DetectedDevice{backend:Backend::XInput,id:0,name:"Gamepad".into(),family:DeviceFamily::Xbox};
        let slots=[Some(device.device_string()),None,Some(device.device_string()),None];
        let mut settings=crate::settings::Settings::default();settings.controller_slots=Some(slots.clone());
        let dir=tempfile::tempdir().unwrap();let file=dir.path().join("settings.json");settings.save(&file).unwrap();
        assert_eq!(crate::settings::Settings::load(&file).unwrap().controller_slots,Some(slots.clone()));
        let (_,ports)=crate::session::build_gcpad_for_slots(&Enumeration{devices:vec![device],not_searched:vec![]},&[],None,Some(&slots)).unwrap();assert_eq!(ports.len(),1);
        let empty=[None,None,None,None];let (ini,ports)=crate::session::build_gcpad_for_slots(&Enumeration{devices:vec![],not_searched:vec![]},&[],None,Some(&empty)).unwrap();assert!(ports.is_empty());assert!(ini.contains("[GCPad4]"));
    }
    #[test]
    fn saved_mapping_survives_restart_and_selected_pad_becomes_player_one() {
        let dir=tempfile::tempdir().unwrap();let file=dir.path().join("profiles.json");
        let first=DetectedDevice { backend:Backend::XInput,id:0,name:"Gamepad".into(),family:DeviceFamily::Xbox };
        let second=DetectedDevice { id:1,..first.clone() };
        let mut profile=ControllerProfile::new(second.identity(),2);
        profile.rebind(GcInput::ButtonA,"`Button Y`");
        save(&file,profile.clone()).unwrap();
        profile.rebind(GcInput::ButtonA,"`Button X`");
        save(&file,profile).unwrap();
        let loaded=load(&file).unwrap();assert_eq!(loaded.len(),1);
        let scan=Enumeration { devices:vec![first,second],not_searched:vec![] };
        let (ini,ports)=crate::session::build_gcpad_for_player(&scan,4,&loaded,Some("XInput/1/Gamepad")).unwrap();
        assert_eq!(ports[0],(1,"XInput/1/Gamepad".into()));
        let player_one=ini.split("[GCPad2]").next().unwrap();
        assert!(player_one.contains("Buttons/A = `Button X`"),"{ini}");
        assert!(player_one.contains("Device = XInput/1/Gamepad"),"{ini}");
    }
}
