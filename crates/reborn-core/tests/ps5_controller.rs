//! The PlayStation 5 pad, against a device dump measured from real hardware.
//!
//! `fixtures/input-capture-ps5.txt` is not written by hand. It is the exact
//! output of `NFL Street Reborn Dolphin.exe --input-capture=DIR` on Adam's PC
//! on 2026-09-12 with a DualSense plugged in over USB and nothing else
//! connected. Regenerate it the same way if the fork's backends ever change.
//!
//! What these tests are defending
//! ------------------------------
//!
//! Players could not use a PlayStation 5 controller. The workaround going
//! around was "install DS4Windows", which works only because DS4Windows
//! manufactures a virtual Xbox pad, and an Xbox pad is the one thing the
//! launcher could bind.
//!
//! The cause was that the launcher had a single default mapping written in
//! XInput vocabulary and applied it to every pad. Dolphin resolves control
//! names PER BACKEND and reports nothing when a name does not resolve, so a
//! PlayStation pad got a correct `Device =` line followed by dead bindings,
//! with no error and nothing in any log.

use reborn_core::controller::{GcInput, Mapping};
use reborn_core::devices::{parse_device_inputs, DeviceInputs};

const CAPTURE: &str = include_str!("fixtures/input-capture-ps5.txt");

fn ps5() -> DeviceInputs {
    parse_device_inputs(CAPTURE)
        .into_iter()
        .find(|d| d.device == "WGInput/0/PS5 Controller")
        .expect("the measured capture contains the PS5 pad")
}

#[test]
fn the_measured_capture_is_the_shape_we_think_it_is() {
    let all = parse_device_inputs(CAPTURE);
    let names: Vec<&str> = all.iter().map(|d| d.device.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "DInput/0/Keyboard Mouse",
            "WGInput/0/PS5 Controller",
            "DInput/0/DualSense Wireless Controller",
        ],
        "a PS5 pad on Windows reaches WGInput and DInput, and never XInput; \
         that absence is the entire reason the old default could not work"
    );

    let pad = ps5();
    for expected in [
        "Cross", "Circle", "Square", "Triangle", "Bumper L", "Bumper R", "Menu", "View",
        "Switch 0 N", "Switch 0 S", "Switch 0 W", "Switch 0 E", "Trigger L", "Trigger R",
        "Left X+", "Left Y+", "Right X+", "Right Y+",
    ] {
        assert!(
            pad.names().iter().any(|n| n == expected),
            "the measured pad reports {expected}"
        );
    }

    // Undetectable inputs are kept and flagged, not dropped.
    let keyboard = &parse_device_inputs(CAPTURE)[0];
    assert!(
        keyboard.inputs.iter().any(|(n, detectable)| n == "Cursor X-" && !*detectable),
        "an undetectable input survives the parse, flagged"
    );
}

#[test]
fn the_old_xbox_default_is_dead_on_this_pad() {
    let pad = ps5();
    let broken = Mapping::default_gamepad().unresolvable(&pad.names());

    // Every face button, Z, Start and the whole d-pad: eleven controls.
    let mut expected = vec![
        GcInput::ButtonA,
        GcInput::ButtonB,
        GcInput::ButtonX,
        GcInput::ButtonY,
        GcInput::ButtonZ,
        GcInput::Start,
        GcInput::DPadUp,
        GcInput::DPadDown,
        GcInput::DPadLeft,
        GcInput::DPadRight,
    ];
    expected.sort();
    let mut got = broken.clone();
    got.sort();
    assert_eq!(
        got, expected,
        "this is the bug, stated as a test: on a real PS5 pad the shipped \
         default leaves no working buttons and no d-pad. Sticks and triggers \
         survive only because XInput and WGInput happen to agree on their names, \
         which is why the reports said the pad half worked."
    );
}

#[test]
fn auto_mapping_makes_the_pad_fully_playable() {
    let pad = ps5();
    let (mapping, unmatched) = Mapping::auto_from(&pad.names());

    assert!(
        unmatched.is_empty(),
        "a PS5 pad needs no wizard: Windows gives WGInput real button labels, \
         so every GameCube control finds a home. Unmatched: {unmatched:?}"
    );
    assert!(
        mapping.unresolvable(&pad.names()).is_empty(),
        "and nothing it produced can fail to resolve"
    );
    assert!(mapping.unbound().is_empty(), "every control is bound");

    // Face buttons go on by POSITION, not by letter. GameCube B is the east
    // face button, which is Circle; GameCube X is the west one, which is
    // Square. Pairing by letter would cross the player's thumb over.
    assert_eq!(mapping.get(GcInput::ButtonA), Some("`Cross`"));
    assert_eq!(mapping.get(GcInput::ButtonB), Some("`Circle`"));
    assert_eq!(mapping.get(GcInput::ButtonX), Some("`Square`"));
    assert_eq!(mapping.get(GcInput::ButtonY), Some("`Triangle`"));
    assert_eq!(mapping.get(GcInput::ButtonZ), Some("`Bumper R`"));
    // A PlayStation pad has no Start. Windows labels Options as Menu.
    assert_eq!(mapping.get(GcInput::Start), Some("`Menu`"));
    assert_eq!(mapping.get(GcInput::DPadUp), Some("`Switch 0 N`"));
    assert_eq!(mapping.get(GcInput::DPadRight), Some("`Switch 0 E`"));
    assert_eq!(mapping.get(GcInput::TriggerL), Some("`Trigger L`"));
    assert_eq!(mapping.get(GcInput::MainStickUp), Some("`Left Y+`"));
}

#[test]
fn an_xbox_pad_still_gets_exactly_what_it_had() {
    // The names an Xbox pad reports through XInput. Nothing about the
    // PlayStation work is allowed to move an Xbox player's buttons.
    let xbox: Vec<String> = [
        "Button A", "Button B", "Button X", "Button Y", "Shoulder L", "Shoulder R", "Trigger L",
        "Trigger R", "Start", "Back", "Thumb L", "Thumb R", "Pad N", "Pad S", "Pad W", "Pad E",
        "Left X-", "Left X+", "Left Y-", "Left Y+", "Right X-", "Right X+", "Right Y-", "Right Y+",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();

    let shipped = Mapping::default_gamepad();
    assert!(
        shipped.unresolvable(&xbox).is_empty(),
        "the shipped default was always correct for an Xbox pad, and stays correct"
    );

    let (auto, unmatched) = Mapping::auto_from(&xbox);
    assert!(unmatched.is_empty());
    for input in GcInput::ALL {
        let (a, b) = (auto.get(*input), shipped.get(*input));
        assert_eq!(
            Mapping::bare(a.unwrap()),
            Mapping::bare(b.unwrap()),
            "auto mapping an Xbox pad reproduces the shipped mapping for {input:?}, \
             so no existing player's controls move"
        );
    }
}

#[test]
fn a_numeric_only_pad_is_sent_to_the_wizard_rather_than_guessed() {
    // The same DualSense through DirectInput, which reports no labels at all.
    // Index order is not stable across pads, so the honest answer is "ask the
    // player", never a guess that silently puts jump on the touchpad.
    let dinput = parse_device_inputs(CAPTURE)
        .into_iter()
        .find(|d| d.device == "DInput/0/DualSense Wireless Controller")
        .expect("the capture contains the DirectInput view of the pad");

    let (_, unmatched) = Mapping::auto_from(&dinput.names());
    let mut got = unmatched.clone();
    got.sort();
    let mut expected = vec![
        GcInput::ButtonA,
        GcInput::ButtonB,
        GcInput::ButtonX,
        GcInput::ButtonY,
        GcInput::ButtonZ,
        GcInput::Start,
        GcInput::TriggerL,
        GcInput::TriggerR,
        GcInput::DPadUp,
        GcInput::DPadDown,
        GcInput::DPadLeft,
        GcInput::DPadRight,
        GcInput::MainStickUp,
        GcInput::MainStickDown,
        GcInput::MainStickLeft,
        GcInput::MainStickRight,
        GcInput::CStickUp,
        GcInput::CStickDown,
        GcInput::CStickLeft,
        GcInput::CStickRight,
    ];
    expected.sort();
    assert_eq!(
        got, expected,
        "nothing on a numeric-only device is auto mapped. `Button 3` could be \
         any button on any pad, so it is measured by the wizard, never assumed."
    );
}

#[test]
fn validation_stays_quiet_when_it_has_no_evidence() {
    assert!(
        Mapping::default_gamepad().unresolvable(&[]).is_empty(),
        "an empty device list means the dump could not be read, not that every \
         binding is broken; refusing a launch on no evidence would be worse \
         than the bug"
    );
}

#[test]
fn a_ps5_pad_is_not_hidden_by_the_presence_of_an_xbox_pad() {
    use reborn_core::controller::DeviceFamily;
    use reborn_core::devices::{Backend, DetectedDevice, Enumeration};

    let pad = |backend: Backend, id: u8, name: &str, family| DetectedDevice {
        backend,
        id,
        name: name.to_string(),
        family,
    };

    // An Xbox pad and a PS5 pad plugged in together. They land on different
    // backends, which is not a coincidence: a PS5 pad never reaches XInput.
    //
    // This used to return the Xbox pad ONLY. `playable_pads` kept the best
    // ranked backend and dropped every other one wholesale, so any XInput
    // device at all, including the virtual pad Steam creates for a PlayStation
    // controller, made the real pad vanish and the launcher report that no
    // second controller was connected.
    let scan = Enumeration {
        devices: vec![
            pad(Backend::DInput, 0, "Keyboard Mouse", DeviceFamily::Keyboard),
            pad(Backend::XInput, 0, "Gamepad", DeviceFamily::Xbox),
            pad(Backend::Other("WGInput".into()), 0, "Xbox One Game Controller", DeviceFamily::Xbox),
            pad(Backend::Other("WGInput".into()), 1, "PS5 Controller", DeviceFamily::PlayStation),
        ],
        not_searched: Vec::new(),
    };

    let chosen: Vec<String> = scan
        .playable_pads()
        .iter()
        .map(|d| d.device_string())
        .collect();

    assert_eq!(
        chosen,
        vec![
            "XInput/0/Gamepad".to_string(),
            "WGInput/1/PS5 Controller".to_string()
        ],
        "both players get a pad: the Xbox one deduplicated down to its XInput \
         entry, the PS5 one kept because nothing on a better backend is it"
    );
}

#[test]
fn two_pads_of_the_same_kind_are_two_pads() {
    use reborn_core::controller::DeviceFamily;
    use reborn_core::devices::{Backend, DetectedDevice, Enumeration};

    // Same backend, two ids, two players. Deduplication must never touch this.
    let scan = Enumeration {
        devices: vec![
            DetectedDevice { backend: Backend::XInput, id: 0, name: "Gamepad".into(), family: DeviceFamily::Xbox },
            DetectedDevice { backend: Backend::XInput, id: 1, name: "Gamepad".into(), family: DeviceFamily::Xbox },
        ],
        not_searched: Vec::new(),
    };
    assert_eq!(scan.playable_pads().len(), 2);
}

#[test]
fn the_recorded_layout_is_exactly_what_the_hardware_reported() {
    use reborn_core::devices::known_inputs;

    // `known_inputs` exists so PlayStation players are fixed by a launcher
    // update alone, without waiting for a runtime that can list inputs. That is
    // only legitimate while the recording is a RECORDING. This test is what
    // keeps it one: it compares the table against the captured dump, so the two
    // cannot drift apart quietly.
    let recorded = known_inputs("WGInput/0/PS5 Controller").expect("the PS5 pad is recorded");
    assert_eq!(
        recorded,
        ps5().names(),
        "the recorded layout must match the measured capture exactly; \
         if the hardware report changed, recapture it, do not edit the table"
    );

    // And it has to be good enough to actually play on.
    let (mapping, unmatched) = Mapping::auto_from(&recorded);
    assert!(unmatched.is_empty());
    assert!(mapping.unresolvable(&recorded).is_empty());
    assert_eq!(mapping.get(GcInput::ButtonA), Some("`Cross`"));
    assert_eq!(mapping.get(GcInput::Start), Some("`Menu`"));

    // Nothing else is claimed. An Xbox pad is never routed through here, and an
    // unmeasured pad returns nothing rather than a guess.
    assert!(known_inputs("XInput/0/Gamepad").is_none());
    assert!(known_inputs("WGInput/0/Xbox One Game Controller").is_none());
    assert!(known_inputs("DInput/0/DualSense Wireless Controller").is_none());
}
