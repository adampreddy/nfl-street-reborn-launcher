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
        "Motor",
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
        ps5().all_names(),
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


#[test]
fn the_exact_string_the_shipped_runtime_prints_is_recognised_end_to_end() {
    use reborn_core::controller::DeviceFamily;
    use reborn_core::devices::{known_inputs, Backend, Enumeration};

    // Verbatim output of `--list-input-devices` from the runtime players
    // already have, captured on Adam's PC with a DualSense over USB. This is
    // the exact path a player is on today, so every link is checked against
    // the real strings rather than against something plausible.
    let shipped = "DInput/0/Keyboard Mouse
WGInput/0/PS5 Controller
DInput/0/DualSense Wireless Controller";
    let scan: Enumeration = reborn_core::devices::parse_device_list(shipped);

    let pads = scan.playable_pads();
    assert_eq!(pads.len(), 1, "exactly one pad is offered, got {pads:?}");
    let pad = pads[0];
    assert_eq!(pad.device_string(), "WGInput/0/PS5 Controller");
    assert_eq!(pad.backend, Backend::Other("WGInput".into()));
    assert_eq!(
        pad.family,
        DeviceFamily::PlayStation,
        "so the game shows PlayStation button art"
    );

    // The recorded layout is found for that exact string, and mapping it
    // leaves nothing dead.
    let available = known_inputs(&pad.device_string()).expect("recognised");
    let (mapping, unmatched) = Mapping::auto_from(&available);
    assert!(unmatched.is_empty());
    assert!(mapping.unresolvable(&available).is_empty());
    assert!(mapping.unbound().is_empty());
}


/// The literal file. Not the logic that builds it: the bytes that land on disk.
#[test]
fn the_generated_gcpad_file_is_live_on_a_ps5_pad() {
    use reborn_core::controller::ControllerProfile;
    use reborn_core::devices::{known_inputs, parse_device_list};
    use reborn_core::session::build_gcpad_for_slots;

    let scan = parse_device_list(
        "DInput/0/Keyboard Mouse
WGInput/0/PS5 Controller
DInput/0/DualSense Wireless Controller",
    );
    let pad = scan.playable_pads()[0].clone();
    let available = known_inputs(&pad.device_string()).unwrap();
    let profile = ControllerProfile::from_device(pad.identity(), 1, &available);

    let (ini, assignment) = build_gcpad_for_slots(&scan, &[profile], None, None)
        .expect("a pad is connected, so a file is produced");

    eprintln!("---- GCPadNew.ini as written ----
{ini}
---- end ----");
    assert_eq!(assignment.len(), 1);
    assert_eq!(assignment[0], (1, "WGInput/0/PS5 Controller".to_string()));

    // The device line, and every control, naming something this pad HAS.
    for expected in [
        "Device = WGInput/0/PS5 Controller",
        "Buttons/A = `Cross`",
        "Buttons/B = `Circle`",
        "Buttons/X = `Square`",
        "Buttons/Y = `Triangle`",
        "Buttons/Z = `Bumper R`",
        "Buttons/Start = `Menu`",
        "D-Pad/Up = `Switch 0 N`",
        "Triggers/L = `Trigger L`",
        "Main Stick/Up = `Left Y+`",
        // The three that are NOT GcInputs, and were hardcoded XInput names
        // until the generated file itself was read. All three were dead.
        "Triggers/L-Analog = `Bumper L`",
        "Triggers/R-Analog = `Bumper R`",
        "Rumble/Motor = `Motor L`",
    ] {
        assert!(ini.contains(expected), "the written file contains: {expected}
{ini}");
    }

    // And none of the dead XInput names survive anywhere in it.
    for dead in [
        "`Button A`",
        "`Shoulder R`",
        "`Shoulder L`",
        "`Pad N`",
        "Rumble/Motor = `Motor` ",
    ] {
        assert!(!ini.contains(dead), "the written file must not contain {dead}
{ini}");
    }
}

/// The case the first fix got wrong, and that players actually hit.
///
/// A launcher can only learn a pad's control names from a runtime carrying
/// --dump-device-inputs, and players keep the runtime they have until a content
/// update reaches them. The first attempt covered that gap by matching the
/// device NAME against a list, which failed the moment Windows called the pad
/// something not on it: the pad was detected, offered, and completely dead.
///
/// The universal mapping needs no identification at all.
#[test]
fn an_unidentified_pad_still_works_because_the_binding_names_both_vocabularies() {
    use reborn_core::controller::ControllerProfile;
    use reborn_core::devices::{Backend, DetectedDevice};
    use reborn_core::controller::DeviceFamily;

    let pad = ps5();
    let available = pad.all_names();

    // A pad the launcher knows NOTHING about: no dump, no recognised name.
    let identity = DetectedDevice {
        backend: Backend::Other("WGInput".into()),
        id: 0,
        // Deliberately a name no table would recognise. Sony's own HID product
        // string is often exactly this.
        name: "Wireless Controller".to_string(),
        family: DeviceFamily::PlayStation,
    }
    .identity();
    let profile = ControllerProfile::from_device(identity, 1, &[]);

    // Every GameCube control resolves against the real pad, with no lookup.
    for input in GcInput::ALL {
        let binding = profile.mapping.get(*input).expect("bound");
        let resolves = binding
            .split('|')
            .any(|part| available.iter().any(|a| Mapping::bare(a) == Mapping::bare(part)));
        assert!(
            resolves,
            "{input:?} is {binding}, and none of those exist on the pad"
        );
    }

    // And the three that are not GcInputs.
    for (label, binding) in [
        ("analog L", profile.extras.analog_trigger_l.as_str()),
        ("analog R", profile.extras.analog_trigger_r.as_str()),
        ("rumble", profile.extras.rumble_motor.as_str()),
    ] {
        let resolves = binding
            .split('|')
            .any(|part| available.iter().any(|a| Mapping::bare(a) == Mapping::bare(part)));
        assert!(resolves, "{label} is {binding}, and none of those exist on the pad");
    }

    // The same binding is simultaneously correct for an Xbox pad, which is the
    // whole point: `A|B` is max(A, B), and an unresolvable side is 0.
    let xbox: Vec<String> = ["Button A", "Shoulder R", "Start", "Pad N", "Shoulder L", "Motor"]
        .iter().map(|s| s.to_string()).collect();
    for input in [GcInput::ButtonA, GcInput::ButtonZ, GcInput::Start, GcInput::DPadUp] {
        let binding = profile.mapping.get(input).unwrap();
        assert!(
            binding.split('|').any(|p| xbox.iter().any(|x| x == Mapping::bare(p))),
            "{input:?} is {binding}, which binds nothing on an Xbox pad"
        );
    }
}

/// The exact path a player is on today, proven without the hardware.
///
/// The pad this was captured from has gone back to its owner, so this cannot
/// be re-run live. It does not need to be: the ONLY thing the old runtime can
/// tell the launcher is the device list, and that string is recorded here
/// verbatim. Everything after it is the real code. Each produced line is then
/// checked against the captured hardware dump, which is what a pad plugged in
/// would have reported.
#[test]
fn the_file_written_on_the_old_runtime_is_live_on_the_real_pad() {
    use reborn_core::controller::ControllerProfile;
    use reborn_core::devices::parse_device_list;
    use reborn_core::session::build_gcpad_for_slots;

    // Verbatim --list-input-devices output, the whole of what a runtime without
    // --dump-device-inputs can say.
    let scan = parse_device_list(
        "DInput/0/Keyboard Mouse
WGInput/0/PS5 Controller
DInput/0/DualSense Wireless Controller",
    );
    let pad = scan.playable_pads()[0].clone();

    // No input list, because the old runtime cannot produce one.
    let profile = ControllerProfile::from_device(pad.identity(), 1, &[]);
    let (ini, _) = build_gcpad_for_slots(&scan, &[profile], None, None).expect("a file");
    eprintln!("---- GCPadNew.ini on the OLD runtime ----
{ini}---- end ----");

    // What the pad really reports, captured while it was connected.
    let available = ps5().all_names();

    let mut checked = 0;
    for line in ini.lines() {
        let Some((key, value)) = line.split_once(" = ") else { continue };
        // Only binding lines. The numeric settings are not expressions.
        if !(key.starts_with("Buttons/")
            || key.starts_with("Main Stick/")
            || key.starts_with("C-Stick/")
            || key.starts_with("D-Pad/")
            || key.starts_with("Triggers/")
            || key == "Rumble/Motor")
            || key.contains("Calibration")
            || key.contains("Dead Zone")
            || key.contains("Threshold")
            || key.contains("Range")
        {
            continue;
        }

        // A trailing token after the expression is a syntax error in Dolphin
        // and silently discards the whole binding. Nothing may have one.
        assert!(
            !value.trim_end().ends_with(|c: char| c.is_ascii_digit()) || value.contains('`'),
            "{key} = {value} has a bare trailing value"
        );

        let resolves = value
            .split('|')
            .any(|part| available.iter().any(|a| Mapping::bare(a) == Mapping::bare(part)));
        assert!(resolves, "{key} = {value}, and none of those exist on the real pad");
        checked += 1;
    }
    assert!(checked >= 22, "expected every binding checked, saw {checked}");

    // The strength is its own key now, never a number glued to the expression.
    assert!(ini.contains("Rumble/Motor/Range = 100.00"), "{ini}");
    assert!(!ini.contains("` 100.00"), "the old broken rumble form survives:
{ini}");
}

/// Every binding line in a generated pad file names at least one control the
/// real pad reported. Returns how many lines were checked.
fn assert_every_binding_is_live_on_the_pad(ini: &str, available: &[String]) -> usize {
    let mut checked = 0;
    for line in ini.lines() {
        let Some((key, value)) = line.split_once(" = ") else { continue };
        if !(key.starts_with("Buttons/")
            || key.starts_with("Main Stick/")
            || key.starts_with("C-Stick/")
            || key.starts_with("D-Pad/")
            || key.starts_with("Triggers/")
            || key == "Rumble/Motor")
            || key.contains("Calibration")
            || key.contains("Dead Zone")
            || key.contains("Threshold")
            || key.contains("Range")
        {
            continue;
        }
        let resolves = value
            .split('|')
            .any(|part| available.iter().any(|a| Mapping::bare(a) == Mapping::bare(part)));
        assert!(resolves, "{key} = {value}, and none of those exist on the real pad:\n{ini}");
        checked += 1;
    }
    checked
}

/// The pad file a launcher before 3.0.6 left on every PlayStation player's
/// disk: the XInput default on the WGInput device, the hardcoded XInput
/// extras, the rumble line in its old broken form. Every binding dead.
fn what_3_0_5_left_behind(pad: &reborn_core::devices::DetectedDevice) -> String {
    use reborn_core::controller::{ControllerProfile, Extras, GcPadConfig};
    let mut old = ControllerProfile::new(pad.identity(), 1);
    old.mapping = Mapping::default_gamepad();
    old.extras = Extras {
        analog_trigger_l: "`Shoulder L`".into(),
        analog_trigger_r: "`Shoulder R`".into(),
        rumble_motor: "`Motor`".into(),
    };
    GcPadConfig::generate(std::slice::from_ref(&old), |_| pad.device_string()).replace(
        "Rumble/Motor = `Motor`\nRumble/Motor/Range = 100.00\n",
        "Rumble/Motor = `Motor` 100.00\n",
    )
}

/// "Users STILL cannot use their PlayStation controllers", 2026-09-13, after
/// 3.0.6 and 3.0.7 had both shipped the universal binding.
///
/// This is the first launch after the upgrade, step for step as
/// `Session::prepare` does it: seed the pad (the old runtime cannot list its
/// controls, so no exact names), adopt whatever differs from the sidecar,
/// generate. The sidecar does not exist yet, so BEFORE the fix every line of
/// the dead file 3.0.5 wrote was adopted as the player's choice and written
/// straight back out. The universal binding never reached the disk of a
/// single player who had the bug.
#[test]
fn an_install_upgraded_from_3_0_5_gets_a_live_pad_on_its_first_launch() {
    use reborn_core::controller::{adopt_external_edits, ControllerProfile};
    use reborn_core::devices::parse_device_list;
    use reborn_core::session::build_gcpad_for_slots;

    let scan = parse_device_list(
        "DInput/0/Keyboard Mouse
WGInput/0/PS5 Controller
DInput/0/DualSense Wireless Controller",
    );
    let pad = scan.playable_pads()[0].clone();
    let left_behind = what_3_0_5_left_behind(&pad);
    assert!(left_behind.contains("Buttons/A = `Button A`"), "{left_behind}");

    let mut profiles = vec![ControllerProfile::from_device(pad.identity(), 1, &[])];
    let adopted = adopt_external_edits(&left_behind, "", &mut profiles);
    let (ini, _) = build_gcpad_for_slots(&scan, &profiles, None, None).expect("a file");
    eprintln!("---- first launch after upgrading from 3.0.5 ----\n{ini}---- adopted: {adopted:?}");

    let checked = assert_every_binding_is_live_on_the_pad(&ini, &ps5().all_names());
    assert!(checked >= 22, "expected every binding checked, saw {checked}");
    assert!(
        profiles[0].overridden.is_empty(),
        "the old launcher's file is not the player's choice: {:?}",
        profiles[0].overridden
    );
}

/// The saved profile the first 3.0.6 or 3.0.7 launch already left on every
/// PlayStation player's disk: every binding the old XInput default, every
/// one recorded as the player's own override, the extras the old hardcoded
/// names. That is the shape of `config/controller-profiles.json` on this PC.
///
/// On a runtime that cannot list the pad's controls, nothing can prove those
/// bindings dead, so `repair_for` alone leaves them forever. Widening needs
/// no proof. And once the runtime can list controls, the widened binding
/// narrows to the exact name, so an Xbox pad ends up exactly where it was.
#[test]
fn a_profile_poisoned_by_the_first_3_0_6_launch_is_healed_without_the_runtime_dump() {
    use reborn_core::controller::{ControllerProfile, DeviceFamily, Extras};
    use reborn_core::devices::{Backend, DetectedDevice};

    let identity = DetectedDevice {
        backend: Backend::Other("WGInput".into()),
        id: 0,
        name: "Wireless Controller".to_string(),
        family: DeviceFamily::PlayStation,
    }
    .identity();

    let mut poisoned = ControllerProfile::new(identity.clone(), 1);
    let old_default = Mapping::default_gamepad();
    for input in GcInput::ALL {
        poisoned.rebind(*input, old_default.get(*input).unwrap().to_string());
    }
    poisoned.extras = Extras {
        analog_trigger_l: "`Shoulder L`".into(),
        analog_trigger_r: "`Shoulder R`".into(),
        rumble_motor: "`Motor`".into(),
    };
    assert_eq!(poisoned.overridden.len(), 20, "the shape adoption left behind");

    // No dump: the old runtime. Repair alone does nothing, by design.
    assert!(poisoned.clone().repair_for(&[]).is_empty());

    // The sticks and triggers have one name in every vocabulary, so the old
    // default already IS the universal binding for them; only the inputs
    // with more than one name change.
    let more_than_one_name = GcInput::ALL
        .iter()
        .filter(|i| Mapping::candidates(**i).len() > 1)
        .count();
    let widened = poisoned.widen_stale_defaults();
    assert_eq!(widened.len(), more_than_one_name, "every one of them was the launcher's own vocabulary");
    assert!(poisoned.overridden.is_empty(), "none of them was ever a choice: {:?}", poisoned.overridden);

    let available = ps5().all_names();
    for input in GcInput::ALL {
        let binding = poisoned.mapping.get(*input).expect("bound");
        let resolves = binding
            .split('|')
            .any(|part| available.iter().any(|a| Mapping::bare(a) == Mapping::bare(part)));
        assert!(resolves, "{input:?} is still {binding}, dead on the real pad");
    }
    for (label, binding) in [
        ("analog L", poisoned.extras.analog_trigger_l.as_str()),
        ("analog R", poisoned.extras.analog_trigger_r.as_str()),
        ("rumble", poisoned.extras.rumble_motor.as_str()),
    ] {
        let resolves = binding
            .split('|')
            .any(|part| available.iter().any(|a| Mapping::bare(a) == Mapping::bare(part)));
        assert!(resolves, "{label} is still {binding}, dead on the real pad");
    }

    // Widening is idempotent: the universal binding is not widened again.
    assert!(poisoned.widen_stale_defaults().is_empty());

    // The moment the runtime can list controls, the exact names come back.
    let repaired = poisoned.repair_for(&available);
    assert_eq!(repaired.len(), more_than_one_name);
    assert_eq!(poisoned.mapping.get(GcInput::ButtonA), Some("`Cross`"));
    assert_eq!(poisoned.mapping.get(GcInput::ButtonZ), Some("`Bumper R`"));
    assert_eq!(poisoned.extras.rumble_motor, "`Motor L`");

    // A choice the player really made is never widened away.
    let mut chosen = ControllerProfile::new(identity, 1);
    chosen.rebind(GcInput::ButtonA, "`Circle`");
    assert!(chosen.widen_stale_defaults().is_empty());
    assert_eq!(chosen.mapping.get(GcInput::ButtonA), Some("`Circle`"));
    assert_eq!(chosen.overridden, vec![GcInput::ButtonA]);
}

/// The profile store on the machine this is running on, when it is the one
/// the launcher really wrote: every profile in it that carries only the
/// launcher's own vocabulary comes out with no overrides and with every
/// binding either universal or exact. Read from a copy; nothing is written.
#[test]
fn the_profile_store_on_this_machine_is_healed_when_it_exists() {
    use reborn_core::controller::Mapping;
    let Some(appdata) = std::env::var_os("APPDATA") else { return };
    let file = std::path::Path::new(&appdata)
        .join("NFL Street Reborn")
        .join("config")
        .join("controller-profiles.json");
    if !file.is_file() {
        eprintln!("no live profile store on this machine; nothing to check");
        return;
    }
    let profiles = reborn_core::controller_profiles::load(&file).expect("the live store parses");
    for mut profile in profiles {
        let all_own = GcInput::ALL.iter().all(|i| {
            profile
                .mapping
                .get(*i)
                .is_some_and(|b| Mapping::is_own_vocabulary(*i, b))
        });
        let before = profile.overridden.len();
        let widened = profile.widen_stale_defaults();
        eprintln!(
            "{}: {} override(s) before, {} widened, {} override(s) after",
            profile.identity.profile_key(),
            before,
            widened.len(),
            profile.overridden.len()
        );
        if all_own {
            assert!(
                profile.overridden.is_empty(),
                "{}: the launcher's own vocabulary is still recorded as the player's choice: {:?}",
                profile.identity.profile_key(),
                profile.overridden
            );
        }
        for input in GcInput::ALL {
            let binding = profile.mapping.get(*input).expect("bound");
            assert!(
                !Mapping::is_own_vocabulary(*input, binding) || binding == Mapping::universal(*input),
                "{input:?} = {binding} is the launcher's vocabulary but was not widened"
            );
        }
    }
}
