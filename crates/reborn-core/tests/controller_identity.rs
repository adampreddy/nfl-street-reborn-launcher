use reborn_core::{
    controller::{ControllerProfile, DeviceFamily, GcInput},
    controller_profiles,
    devices::{parse_device_list, Backend, DetectedDevice},
    ini::IniDocument,
    session::build_gcpad_for_slots,
};

fn pad(name: &str) -> DetectedDevice {
    DetectedDevice {
        backend: Backend::Sdl,
        id: 0,
        name: name.into(),
        family: DeviceFamily::Generic,
    }
}

#[test]
fn different_models_at_the_same_backend_index_keep_separate_saved_controls_and_ports() {
    // Dolphin allocates IDs within each source/name pair, so these are all
    // valid device-zero entries, not aliases for the same physical pad.
    let scan = parse_device_list(
        "SDL/0/USB GameCube Controller\nSDL/0/Arcade Stick\nSDL/0/Flight Stick\nSDL/0/Retro Pad",
    );
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("profiles.json");
    for (index, device) in scan.devices.iter().enumerate() {
        let mut profile = ControllerProfile::new(device.identity(), index as u8 + 1);
        profile.rebind(GcInput::ButtonA, format!("`Button {index}`"));
        controller_profiles::save(&file, profile).unwrap();
    }
    let profiles = controller_profiles::load(&file).unwrap();
    assert_eq!(
        profiles.len(),
        4,
        "saving another controller must not replace its neighbor"
    );
    let slots = std::array::from_fn(|index| Some(scan.devices[index].device_string()));
    let (text, assignments) = build_gcpad_for_slots(&scan, &profiles, None, Some(&slots)).unwrap();
    assert_eq!(assignments.len(), 4);
    let ini = IniDocument::parse(&text);
    for (index, device) in scan.devices.iter().enumerate() {
        let section = format!("GCPad{}", index + 1);
        assert_eq!(
            ini.get(&section, "Device"),
            Some(device.device_string().as_str())
        );
        assert_eq!(
            ini.get(&section, "Buttons/A"),
            Some(format!("`Button {index}`").as_str())
        );
    }
}

#[test]
fn legacy_profile_uses_its_recorded_name_without_claiming_another_model() {
    let original_pad = pad("USB GameCube Controller");
    let mut old = ControllerProfile::new(original_pad.identity(), 2);
    old.identity.guid = "SDL:0".into();
    old.rebind(GcInput::MainStickUp, "`Axis 1-`");
    old.tuning.deadzone_percent = 24;
    old.calibration
        .insert("Main Stick/Calibration".into(), "1.1 1.2 1.3".into());
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("profiles.json");
    std::fs::write(&file, serde_json::to_vec(&vec![old.clone()]).unwrap()).unwrap();
    let loaded = controller_profiles::load(&file).unwrap();
    let mut expected = old.clone();
    expected.identity = original_pad.identity();
    assert_eq!(loaded, [expected]);
    assert_ne!(
        loaded[0].identity.profile_key(),
        pad("Arcade Stick").identity().profile_key()
    );
    // Migration is read-only until a mapping is saved; loading does not risk
    // replacing the only surviving copy of a profile from the old format.
    assert_eq!(
        serde_json::from_slice::<Vec<ControllerProfile>>(&std::fs::read(&file).unwrap()).unwrap(),
        [old.clone()]
    );
    controller_profiles::save(&file, old).unwrap();
    assert_eq!(controller_profiles::load(&file).unwrap(), loaded);
}

#[test]
fn current_profile_wins_over_a_duplicate_legacy_profile() {
    let mut current = ControllerProfile::new(pad("Retro Pad").identity(), 1);
    current.rebind(GcInput::ButtonA, "`Button 4`");
    let mut old = current.clone();
    old.identity.guid = "SDL:0".into();
    old.rebind(GcInput::ButtonA, "`Button 1`");
    for profiles in [
        vec![old.clone(), current.clone()],
        vec![current.clone(), old.clone()],
    ] {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("profiles.json");
        std::fs::write(&file, serde_json::to_vec(&profiles).unwrap()).unwrap();
        assert_eq!(controller_profiles::load(&file).unwrap(), [current.clone()]);
    }
}

#[test]
fn hardware_guids_serials_and_xinput_slots_keep_their_existing_keys() {
    let mut hardware = ControllerProfile::new(pad("Retro Pad").identity(), 1);
    hardware.identity.guid = "030000004c050000cc09000000006800".into();
    hardware.identity.serial = Some("controller-one".into());
    let xbox = ControllerProfile::new(
        DetectedDevice {
            backend: Backend::XInput,
            id: 2,
            name: "Gamepad".into(),
            family: DeviceFamily::Xbox,
        }
        .identity(),
        2,
    );
    assert_eq!(xbox.identity.profile_key(), "XInput:2");
    let mut foreign = hardware.clone();
    foreign.identity.guid = "vendor:123".into();
    foreign.identity.serial = None;
    let original = vec![hardware, xbox, foreign];
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("profiles.json");
    std::fs::write(&file, serde_json::to_vec(&original).unwrap()).unwrap();
    assert_eq!(controller_profiles::load(&file).unwrap(), original);
}

#[test]
fn every_known_raw_runtime_source_migrates_with_its_exact_device_name() {
    for source in [
        "SDL",
        "DInput",
        "WGInput",
        "evdev",
        "Quartz",
        "Input",
        "DSUClient",
        "Bluetooth",
        "Pipe",
        "Android",
        "XInput2",
    ] {
        let device = parse_device_list(&format!("{source}/3/USB pad: arcade/gamecube"))
            .devices
            .remove(0);
        let mut old = ControllerProfile::new(device.identity(), 1);
        old.identity.guid = format!("{source}:3");
        old.rebind(GcInput::ButtonA, "`Button 7`");
        let mut expected = old.clone();
        expected.identity = device.identity();
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("profiles.json");
        std::fs::write(&file, serde_json::to_vec(&vec![old]).unwrap()).unwrap();
        assert_eq!(
            controller_profiles::load(&file).unwrap(),
            [expected],
            "{source}"
        );
    }
}

#[test]
fn one_dualsense_seen_on_three_backends_is_one_player() {
    let scan = parse_device_list(
        "DInput/0/Keyboard Mouse\nDInput/0/Wireless Controller\nWGInput/0/PS5 Controller\nSDL/0/PS5 Controller",
    );
    let pads: Vec<String> = scan.playable_pads().iter().map(|d| d.device_string()).collect();
    assert_eq!(pads, ["WGInput/0/PS5 Controller"], "{pads:?}");
}

#[test]
fn a_switch_pro_seen_on_wginput_and_sdl_is_one_player() {
    let scan = parse_device_list(
        "DInput/0/Keyboard Mouse\nWGInput/0/Pro Controller\nSDL/0/Nintendo Switch Pro Controller",
    );
    let pads: Vec<String> = scan.playable_pads().iter().map(|d| d.device_string()).collect();
    assert_eq!(pads, ["WGInput/0/Pro Controller"], "{pads:?}");
}

#[test]
fn a_keyboard_player_with_a_joystick_driver_has_no_pad() {
    let scan = parse_device_list("DInput/0/Keyboard Mouse
DInput/0/vJoy Device");
    assert!(scan.playable_pads().is_empty());
}

#[test]
fn an_sdl_only_pad_is_still_offered() {
    let scan = parse_device_list("DInput/0/Keyboard Mouse
SDL/0/USB GameCube Controller");
    let pads: Vec<String> = scan.playable_pads().iter().map(|d| d.device_string()).collect();
    assert_eq!(pads, ["SDL/0/USB GameCube Controller"]);
}

/// Measured 2026-09-24 with runtime 1.2.6 and REBORN_RAW_CONTROLLERS=1 on a PC
/// with ONE wired Xbox pad: the runtime reports it four times, twice on SDL.
#[test]
fn one_measured_xbox_pad_on_runtime_1_2_6_is_one_player() {
    let scan = parse_device_list(
        "DInput/0/Keyboard Mouse\nSDL/0/Xbox Series X Controller\nSDL/1/Xbox Series X Controller\nWGInput/0/Xbox One Game Controller\nXInput/0/Gamepad",
    );
    let pads: Vec<String> = scan.playable_pads().iter().map(|d| d.device_string()).collect();
    assert_eq!(pads, ["XInput/0/Gamepad"]);
    let (_, assignments) = build_gcpad_for_slots(&scan, &[], None, None).unwrap();
    assert_eq!(assignments, [(1, "XInput/0/Gamepad".to_string())]);
}
