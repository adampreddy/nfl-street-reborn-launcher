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
