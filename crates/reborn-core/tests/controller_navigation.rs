use reborn_core::{controller::{ControllerProfile, DeviceFamily, DeviceIdentity, GcInput, Mapping}, devices};

fn scan(text: &str) -> devices::Enumeration {
    devices::parse_device_list(text)
}

fn profile() -> ControllerProfile {
    ControllerProfile::new(DeviceIdentity {guid: "test".into(),serial: None,name: "USB controller".into(),family: DeviceFamily::Generic},1)
}

#[test]
fn mixed_backends_keep_unique_usb_controllers() {
    let scan=scan("XInput/0/Gamepad\nSDL/0/USB GameCube Controller\nDInput/0/USB Arcade Pad\nDInput/0/Keyboard Mouse");
    let pads=scan.playable_pads();
    assert_eq!(pads.len(),3);
    assert!(pads.iter().any(|p|p.name=="USB GameCube Controller"));
    assert!(pads.iter().any(|p|p.name=="USB Arcade Pad"));
}

#[test]
fn raw_directinput_controller_is_not_a_keyboard() {
    let scan=scan("DInput/0/Generic USB Joystick\nDInput/0/Keyboard Mouse");
    assert_eq!(scan.playable_pads().len(),1);
    assert_eq!(scan.playable_pads()[0].name,"Generic USB Joystick");
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
fn additional_same_model_pad_on_another_backend_is_retained() {
    let pads=scan("WGInput/0/PS5 Controller\nSDL/0/DualSense Wireless Controller\nSDL/1/DualSense Wireless Controller");
    assert_eq!(pads.playable_pads().len(),2);
    assert_eq!(pads.playable_pads()[1].device_string(),"SDL/1/DualSense Wireless Controller");
}

#[test]
fn generic_gamepad_is_not_assumed_to_be_the_xinput_pad() {
    let pads=scan("XInput/0/Gamepad\nSDL/0/Gamepad\nDInput/0/Gamepad");
    let names:Vec<_>=pads.playable_pads().iter().map(|p|p.device_string()).collect();
    assert_eq!(names, ["XInput/0/Gamepad", "SDL/0/Gamepad"]);
}

#[test]
fn extra_xbox_named_sdl_pad_is_not_hidden_by_one_xinput_pad() {
    let pads=scan("XInput/0/Gamepad\nSDL/0/Xbox Controller\nSDL/1/Xbox Controller");
    assert_eq!(pads.playable_pads().len(),2);
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
