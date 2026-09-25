use reborn_core::{controller::{ControllerProfile, DeviceFamily, DeviceIdentity, GcInput, Mapping}, devices};

fn scan(text: &str) -> devices::Enumeration {
    devices::parse_device_list(text)
}

fn profile() -> ControllerProfile {
    ControllerProfile::new(DeviceIdentity {guid: "test".into(),serial: None,name: "USB controller".into(),family: DeviceFamily::Generic},1)
}

/// SDL and DirectInput report the same plastic as XInput/WGInput under other
/// names (`DInput/0/Wireless Controller` for a DualSense), so while a backend
/// that names its controls has a pad, they are not offered: a pad kept on two
/// backends gets two ports and drives two players.
#[test]
fn a_named_backend_pad_hides_sdl_and_directinput_copies() {
    let scan=scan("XInput/0/Gamepad
SDL/0/USB GameCube Controller
DInput/0/USB Arcade Pad
DInput/0/Keyboard Mouse");
    let names:Vec<_>=scan.playable_pads().iter().map(|p|p.device_string()).collect();
    assert_eq!(names,["XInput/0/Gamepad"]);
}

#[test]
fn directinput_joysticks_are_never_offered() {
    let scan=scan("DInput/0/Generic USB Joystick
DInput/0/Keyboard Mouse");
    assert!(scan.playable_pads().is_empty());
}

#[test]
fn custom_dpad_and_stick_binding_survives_repair_and_ini_generation() {
    let mut p=profile();
    // The axis-positive last term used to defeat OR detection and be replaced.
    p.rebind(GcInput::MainStickUp,"`Pad N`|`Left Y+`");
    p.rebind(GcInput::MainStickDown,"`Pad S`|`Left Y-`");
    p.extras.analog_trigger_l="`Trigger L`|`Button 9`".into();
    p.extras.rumble_motor="`Missing motor`".into();
    let available=["Pad N","Pad S","Left Y+","Left Y-","Trigger L","Button 9","Motor L"].map(String::from);
    p.widen_stale_defaults();
    p.repair_for(&available);
    assert_eq!(p.mapping.get(GcInput::MainStickUp),Some("`Pad N`|`Left Y+`"));
    assert_eq!(p.mapping.get(GcInput::MainStickDown),Some("`Pad S`|`Left Y-`"));
    assert_eq!(p.extras.analog_trigger_l,"`Trigger L`|`Button 9`");
    assert_eq!(p.extras.rumble_motor,"`Motor L`");
    assert!(p.overridden.contains(&GcInput::MainStickUp));
    let ini=reborn_core::controller::GcPadConfig::generate(&[p], |_|"XInput/0/Gamepad".into());
    assert!(ini.contains("Main Stick/Up = `Pad N`|`Left Y+`"));
}

#[test]
fn generic_stick_polarity_and_dpad_are_not_rewritten_as_xinput() {
    let mut p=profile();
    for (input,name) in [(GcInput::MainStickUp,"Axis Y-"),(GcInput::MainStickDown,"Axis Y+"),(GcInput::DPadUp,"Hat 0 N")] {p.rebind(input,Mapping::quote(name));}
    let available=["Axis Y-","Axis Y+","Hat 0 N"].map(String::from);
    p.widen_stale_defaults();p.repair_for(&available);
    assert_eq!(p.mapping.get(GcInput::MainStickUp),Some("`Axis Y-`"));
    assert_eq!(p.mapping.get(GcInput::MainStickDown),Some("`Axis Y+`"));
    assert_eq!(p.mapping.get(GcInput::DPadUp),Some("`Hat 0 N`"));
}

#[test]
fn a_ps5_pad_on_wginput_is_not_doubled_by_its_sdl_copy() {
    let pads=scan("WGInput/0/PS5 Controller
SDL/0/DualSense Wireless Controller");
    let names:Vec<_>=pads.playable_pads().iter().map(|p|p.device_string()).collect();
    assert_eq!(names,["WGInput/0/PS5 Controller"]);
}

#[test]
fn two_sdl_only_pads_are_two_pads_even_when_one_is_called_gamepad() {
    let pads=scan("SDL/0/Gamepad
SDL/0/Xbox Controller
SDL/1/Xbox Controller
DInput/0/Gamepad");
    let names:Vec<_>=pads.playable_pads().iter().map(|p|p.device_string()).collect();
    assert_eq!(names,["SDL/0/Gamepad","SDL/0/Xbox Controller","SDL/1/Xbox Controller"]);
}

#[test]
#[ignore = "requires REBORN_TEST_RUNTIME pointing at a locally built Dolphin runtime"]
fn real_runtime_list_dump_and_capture_agree() {
    let exe=std::path::PathBuf::from(std::env::var_os("REBORN_TEST_RUNTIME").expect("set REBORN_TEST_RUNTIME"));
    let dir=tempfile::tempdir().unwrap();
    let platform=reborn_core::platform::Platform::detect();
    let inherited=std::env::var_os("REBORN_RAW_CONTROLLERS");
    let list=devices::scan_via_runtime(&platform,&exe,dir.path()).unwrap();
    let (dump,inputs)=devices::dump_via_runtime(&platform,&exe,dir.path()).unwrap();
    assert_eq!(list,dump,"hardware must remain connected throughout the probe");
    let capture=reborn_core::capture::Capture::start(&platform,&exe,dir.path()).unwrap();
    assert_eq!(capture.devices(),inputs);
    assert_eq!(capture.detect("SDL/9999/Reborn test missing device").unwrap(),None);
    drop(capture);
    assert!(dir.path().join("quit").exists());
    assert_eq!(std::env::var_os("REBORN_RAW_CONTROLLERS"),inherited,"query opt-in must not mutate the launcher's environment");
}
