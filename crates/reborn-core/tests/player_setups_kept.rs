//! "Users on PS controllers are getting errors telling people to set up their
//! controllers over and over again ... controls that are getting set up are
//! just getting reset." (Adam, 2026-10-08)
//!
//! Two causes, both reproduced here through [`plan_pads`], the function Play
//! uses to decide the whole pad file:
//!
//! 1. A pad set up in Dolphin's own window on the entry Dolphin lists for it
//!    (`DInput/0/Wireless Controller`) was thrown away on every Play, because
//!    the launcher offers that pad as `WGInput/0/...` and only adopts edits made
//!    on its own entry. Its dead default came back, and so did the setup prompt.
//! 2. 3.0.37 counted Linux control names as the launcher's own on Windows, so a
//!    stick set up by pressing it (`Axis 1-`) was widened back to `Left Y+`.

use reborn_core::controller::{blocks_play, ControllerProfile, DeviceFamily, GcInput};
use reborn_core::devices::{parse_device_inputs, Backend, DetectedDevice, DeviceInputs, Enumeration};
use reborn_core::session::{build_gcpad_for_slots, plan_pads, player_one_blocked, release_player_sections, PadInputs, PadPlan};

/// A PlayStation pad Windows has no labels for: WGInput and DirectInput both
/// report numbers. Format as `--dump-device-inputs` writes it.
const PS_RAW: &str = "DEVICE\tDInput/0/Keyboard Mouse\nINPUT\tA\t1\n\
DEVICE\tDInput/0/Wireless Controller\nINPUT\tButton 0\t1\nINPUT\tButton 1\t1\nINPUT\tButton 2\t1\nINPUT\tButton 9\t1\nINPUT\tAxis X-\t1\nINPUT\tAxis X+\t1\nINPUT\tAxis Y-\t1\nINPUT\tAxis Y+\t1\n\
DEVICE\tWGInput/0/Wireless Controller\nINPUT\tButton 0\t1\nINPUT\tButton 1\t1\nINPUT\tButton 2\t1\nINPUT\tButton 9\t1\nINPUT\tAxis 0-\t1\nINPUT\tAxis 0+\t1\nINPUT\tAxis 1-\t1\nINPUT\tAxis 1+\t1\nINPUT\tSwitch 0 N\t1\n";

/// What a player maps in Dolphin's window after choosing their pad's
/// DirectInput entry.
const THEIRS: &str = "[GCPad1]\nDevice = DInput/0/Wireless Controller\nButtons/A = `Button 1`\nButtons/Start = `Button 9`\n\
Main Stick/Up = `Axis Y-`\nMain Stick/Down = `Axis Y+`\nMain Stick/Left = `Axis X-`\nMain Stick/Right = `Axis X+`\n";

const PORT_ONE_ON: [bool; 4] = [true, false, false, false];

fn device(string: &str) -> DetectedDevice {
    let mut it = string.splitn(3, '/');
    let (source, id, name) = (it.next().unwrap(), it.next().unwrap(), it.next().unwrap());
    DetectedDevice {
        backend: match source {
            "DInput" => Backend::DInput,
            "XInput" => Backend::XInput,
            "SDL" => Backend::Sdl,
            other => Backend::Other(other.into()),
        },
        id: id.parse().unwrap(),
        name: name.into(),
        // As the runtime's parser rules it: only Xbox-class hardware reaches XInput.
        family: match DeviceFamily::from_name(name) {
            DeviceFamily::Generic if source == "XInput" => DeviceFamily::Xbox,
            other => other,
        },
    }
}

fn scan_of(inputs: &[DeviceInputs]) -> Enumeration {
    Enumeration { devices: inputs.iter().map(|d| device(&d.device)).collect(), not_searched: vec![] }
}

struct Desk {
    inputs: Vec<DeviceInputs>,
    scan: Enumeration,
    profiles: Vec<ControllerProfile>,
    enabled: [bool; 4],
    slots: Option<[Option<String>; 4]>,
}

impl Desk {
    fn new(dump: &str) -> Self {
        let inputs = parse_device_inputs(dump);
        let scan = scan_of(&inputs);
        Desk { inputs, scan, profiles: vec![], enabled: PORT_ONE_ON, slots: None }
    }

    /// One Play: plan from what is on disk, keep the profiles it saved.
    fn play(&mut self, live: Option<&str>, last_written: &str) -> PadPlan {
        let plan = plan_pads(
            &PadInputs {
                scan: &self.scan,
                device_inputs: &self.inputs,
                live,
                last_written,
                enabled_ports: self.enabled,
                primary: None,
                slots: self.slots.as_ref(),
            },
            self.profiles.clone(),
        );
        self.profiles = plan.profiles.clone();
        plan
    }
}

fn blocked(plan: &PadPlan) -> bool {
    let mut ports = plan.assignment.clone();
    ports.extend(plan.kept.iter().cloned());
    player_one_blocked(&ports, &plan.dead_by_device).is_some()
}

#[test]
fn a_pad_set_up_in_dolphin_on_its_directinput_entry_is_kept_and_not_asked_about_again() {
    let mut desk = Desk::new(PS_RAW);
    let first = desk.play(None, "");
    assert!(blocked(&first), "a pad with numbered controls needs setting up first");
    let ours = first.ini.unwrap();

    // The player sets it up in Dolphin, as we told them to.
    let second = desk.play(Some(THEIRS), &ours);
    let written = second.ini.clone().unwrap();
    assert_eq!(written, THEIRS, "their section, exactly as they wrote it, and nothing else");
    assert_eq!(second.kept, [(1, "DInput/0/Wireless Controller".to_string())]);
    assert!(second.assignment.is_empty(), "the launcher's copy of the same pad takes no port: {:?}", second.assignment);
    assert!(!blocked(&second), "Start, A and the stick are bound on the device they name");

    // And on every Play after that.
    let third = desk.play(Some(&written), &written);
    assert_eq!(third.ini.as_deref(), Some(THEIRS));
    assert!(!blocked(&third));
}

#[test]
fn a_dolphin_setup_that_cannot_press_start_still_asks() {
    let mut desk = Desk::new(PS_RAW);
    let no_start = THEIRS.replace("Buttons/Start = `Button 9`\n", "");
    let plan = desk.play(Some(&no_start), "");
    assert_eq!(plan.ini.as_deref(), Some(no_start.as_str()), "still theirs");
    assert!(blocked(&plan), "judged as written: Start is not bound");
    assert!(plan.dead_by_device["DInput/0/Wireless Controller"].contains(&GcInput::Start));
}

#[test]
fn an_sdl_entry_chosen_in_dolphin_is_kept_too() {
    let dump = format!("{PS_RAW}DEVICE\tSDL/0/PS4 Controller\nINPUT\tButton 0\t1\nINPUT\tButton 6\t1\nINPUT\tAxis 1-\t1\nINPUT\tAxis 1+\t1\nINPUT\tAxis 0-\t1\nINPUT\tAxis 0+\t1\n");
    let mut desk = Desk::new(&dump);
    let theirs = "[GCPad1]\nDevice = SDL/0/PS4 Controller\nButtons/A = `Button 0`\nButtons/Start = `Button 6`\nMain Stick/Up = `Axis 1-`\nMain Stick/Down = `Axis 1+`\nMain Stick/Left = `Axis 0-`\nMain Stick/Right = `Axis 0+`\n";
    let plan = desk.play(Some(theirs), "");
    assert_eq!(plan.ini.as_deref(), Some(theirs));
    assert!(plan.assignment.is_empty(), "{:?}", plan.assignment);
    assert!(!blocked(&plan));
}

#[test]
fn an_xbox_pad_set_up_on_its_directinput_entry_is_not_player_two_as_well() {
    let dump = "DEVICE\tDInput/0/Controller (Xbox One For Windows)\nINPUT\tButton 0\t1\n\
DEVICE\tWGInput/0/Xbox One Game Controller\nINPUT\tButton A\t1\n\
DEVICE\tXInput/0/Gamepad\nINPUT\tButton A\t1\nINPUT\tStart\t1\nINPUT\tLeft Y+\t1\n";
    let mut desk = Desk::new(dump);
    let theirs = "[GCPad1]\nDevice = DInput/0/Controller (Xbox One For Windows)\nButtons/A = `Button 0`\n";
    let plan = desk.play(Some(theirs), "");
    assert_eq!(plan.ini.as_deref(), Some(theirs));
    assert!(plan.assignment.is_empty(), "XInput and WGInput are the same pad: {:?}", plan.assignment);
}

#[test]
fn a_second_pad_moves_to_a_port_the_player_does_not_hold() {
    let dump = format!("{PS_RAW}DEVICE\tXInput/0/Gamepad\nINPUT\tButton A\t1\nINPUT\tStart\t1\nINPUT\tLeft Y+\t1\nINPUT\tLeft Y-\t1\nINPUT\tLeft X+\t1\nINPUT\tLeft X-\t1\n");
    let mut desk = Desk::new(&dump);
    let plan = desk.play(Some(THEIRS), "");
    assert_eq!(plan.assignment, [(2, "XInput/0/Gamepad".to_string())], "the PlayStation pad's WGInput copy is gone, the Xbox pad is player two");
    let ini = plan.ini.unwrap();
    assert!(ini.starts_with(THEIRS), "{ini}");
    assert!(ini.contains("[GCPad2]\nDevice = XInput/0/Gamepad\n"), "{ini}");
    assert!(!ini.contains("WGInput"), "{ini}");
}

#[test]
fn a_keyboard_or_a_switched_off_port_is_never_kept() {
    let mut desk = Desk::new(PS_RAW);
    let keyboard = "[GCPad1]\nDevice = DInput/0/Keyboard Mouse\nButtons/A = X\n";
    let plan = desk.play(Some(keyboard), "");
    assert!(plan.kept.is_empty());
    assert_eq!(plan.assignment, [(1, "WGInput/0/Wireless Controller".to_string())]);

    // An old section on port 2, which Dolphin has off, must not become a player.
    let stale = THEIRS.replace("[GCPad1]", "[GCPad2]");
    let plan = desk.play(Some(&stale), "");
    assert!(plan.kept.is_empty(), "{:?}", plan.kept);
    assert_eq!(plan.assignment, [(1, "WGInput/0/Wireless Controller".to_string())]);
}

#[test]
fn a_setup_on_another_port_never_leaves_player_one_empty() {
    let mut desk = Desk::new(PS_RAW);
    desk.enabled = [true, true, false, false];
    let on_two = THEIRS.replace("[GCPad1]", "[GCPad2]");
    let plan = desk.play(Some(&on_two), "");
    assert_eq!(plan.kept, [(2, "DInput/0/Wireless Controller".to_string())]);
    assert_eq!(plan.assignment, [(1, "WGInput/0/Wireless Controller".to_string())], "player one still has a pad");
}

#[test]
fn setting_the_pad_up_on_the_controllers_page_takes_it_back() {
    let dir = tempfile::tempdir().unwrap();
    let pad = dir.path().join("GCPadNew.ini");
    let dolphin = dir.path().join("Dolphin.ini");
    std::fs::write(&pad, THEIRS).unwrap();
    let mut desk = Desk::new(PS_RAW);

    // Saved on the page for the pad the launcher offers.
    assert!(release_player_sections(&pad, &dolphin, &desk.scan, Some("WGInput/0/Wireless Controller"), None).unwrap());
    let live = std::fs::read_to_string(&pad).unwrap();
    assert!(!live.contains("DInput"), "{live}");
    let plan = desk.play(Some(&live), "");
    assert!(plan.kept.is_empty());
    assert_eq!(plan.assignment, [(1, "WGInput/0/Wireless Controller".to_string())]);

    // Assigning player one on the page does the same.
    std::fs::write(&pad, THEIRS).unwrap();
    assert!(release_player_sections(&pad, &dolphin, &desk.scan, None, Some(1)).unwrap());
    // Nothing of theirs left: nothing to do.
    assert!(!release_player_sections(&pad, &dolphin, &desk.scan, None, Some(1)).unwrap());
}

/// 3.0.37: `Axis 1-` is a Linux name in the Deck tables, and on Windows it is
/// what a pad without labels calls its left stick.
#[test]
fn a_stick_set_up_by_pressing_it_survives_the_next_play() {
    let mut desk = Desk::new(PS_RAW);
    let first = desk.play(None, "");
    assert!(blocked(&first));
    // The Controllers page's press-to-map: what the runtime captured, per control.
    let profile = desk.profiles.iter_mut().find(|p| p.identity.profile_key() == "WGInput/0/Wireless Controller").unwrap();
    for (input, binding) in [
        (GcInput::ButtonA, "`Button 1`"),
        (GcInput::Start, "`Button 9`"),
        (GcInput::MainStickUp, "`Axis 1-`"),
        (GcInput::MainStickDown, "`Axis 1+`"),
        (GcInput::MainStickLeft, "`Axis 0-`"),
        (GcInput::MainStickRight, "`Axis 0+`"),
    ] {
        profile.rebind(input, binding);
    }
    let set_up = profile.clone();

    let second = desk.play(None, "");
    let after = desk.profiles.iter().find(|p| p.identity.profile_key() == "WGInput/0/Wireless Controller").unwrap();
    assert_eq!(after.mapping.get(GcInput::MainStickUp), Some("`Axis 1-`"), "the stick was reset");
    assert_eq!(after.overridden, set_up.overridden, "still the player's choices");
    assert!(!blocked(&second), "asked again: {:?}", second.dead_by_device);
}

#[test]
fn a_pad_windows_renumbered_keeps_its_setup_and_its_slot() {
    let mut desk = Desk::new(PS_RAW);
    desk.play(None, "");
    let profile = desk.profiles.iter_mut().find(|p| p.identity.profile_key() == "WGInput/0/Wireless Controller").unwrap();
    profile.rebind(GcInput::ButtonA, "`Button 1`");
    profile.rebind(GcInput::Start, "`Button 9`");
    for (input, axis) in [(GcInput::MainStickUp, "`Axis 1-`"), (GcInput::MainStickDown, "`Axis 1+`"), (GcInput::MainStickLeft, "`Axis 0-`"), (GcInput::MainStickRight, "`Axis 0+`")] {
        profile.rebind(input, axis);
    }
    desk.slots = Some([Some("WGInput/0/Wireless Controller".into()), None, None, None]);

    // Next time Windows calls it number 1.
    let renumbered = PS_RAW.replace("WGInput/0/", "WGInput/1/");
    desk.inputs = parse_device_inputs(&renumbered);
    desk.scan = scan_of(&desk.inputs);
    let plan = desk.play(None, "");
    assert_eq!(plan.assignment, [(1, "WGInput/1/Wireless Controller".to_string())], "the slot follows the pad");
    assert!(plan.profiles_changed);
    let keys: Vec<String> = desk.profiles.iter().map(|p| p.identity.profile_key()).collect();
    assert_eq!(keys, ["WGInput/1/Wireless Controller"], "re-keyed, not duplicated");
    assert!(!blocked(&plan), "its setup came with it: {:?}", plan.dead_by_device);
}

#[test]
fn two_identical_pads_keep_their_own_numbers() {
    let two = format!("{PS_RAW}{}", PS_RAW.split("DEVICE\tWGInput/0/").nth(1).map(|rest| format!("DEVICE\tWGInput/1/{rest}")).unwrap());
    let inputs = parse_device_inputs(&two);
    let scan = scan_of(&inputs);
    let slots = [Some("WGInput/0/Wireless Controller".to_string()), Some("WGInput/1/Wireless Controller".to_string()), None, None];
    let (_, assignment) = build_gcpad_for_slots(&scan, &[], None, Some(&slots)).unwrap();
    assert_eq!(assignment, [(1, "WGInput/0/Wireless Controller".to_string()), (2, "WGInput/1/Wireless Controller".to_string())]);
    assert!(blocks_play(&[GcInput::Start]));
}

/// The same reset, wherever a player's working choice is spelled like one of
/// the launcher's defaults: on the Deck an evdev stick is `Axis 1-`, which is
/// also a launcher default there. With the pad's controls listed, a binding
/// that works is the player's and is never widened away.
#[test]
fn a_working_choice_spelled_like_a_default_is_not_widened_away() {
    let dump = "DEVICE\tWGInput/0/Arcade Pad\nINPUT\tButton A\t1\nINPUT\tStart\t1\nINPUT\tMenu\t1\n\
INPUT\tLeft Y+\t1\nINPUT\tLeft Y-\t1\nINPUT\tLeft X+\t1\nINPUT\tLeft X-\t1\n";
    let mut desk = Desk::new(dump);
    desk.play(None, "");
    let profile = desk.profiles.iter_mut().find(|p| p.identity.profile_key() == "WGInput/0/Arcade Pad").unwrap();
    profile.rebind(GcInput::Start, "`Menu`");
    desk.play(None, "");
    let after = desk.profiles.iter().find(|p| p.identity.profile_key() == "WGInput/0/Arcade Pad").unwrap();
    assert_eq!(after.mapping.get(GcInput::Start), Some("`Menu`"), "the player's Start was put back to the launcher's pick");
}

/// The Mac report (Adam, 2026-10-08: "the main issue is with the analog
/// sticks"): the Mac emulator reads pads through SDL, which numbers every
/// control. Buttons set up by pressing them survived Play, sticks did not:
/// `Axis 1-` read as a launcher default and became `Left Y+` again.
#[test]
fn an_sdl_pad_set_up_by_pressing_keeps_its_sticks() {
    let mut dump = String::from("DEVICE\tSDL/0/Xbox Series X Controller\n");
    for b in 0..16 {
        dump.push_str(&format!("INPUT\tButton {b}\t1\n"));
    }
    for a in 0..6 {
        dump.push_str(&format!("INPUT\tAxis {a}-\t1\nINPUT\tAxis {a}+\t1\n"));
    }
    let mut desk = Desk::new(&dump);
    let first = desk.play(None, "");
    assert!(blocked(&first), "numbered controls: the wizard first");
    let profile = desk.profiles.iter_mut().find(|p| p.identity.profile_key() == "SDL/0/Xbox Series X Controller").unwrap();
    for (input, binding) in [
        (GcInput::ButtonA, "`Button 0`"),
        (GcInput::Start, "`Button 6`"),
        (GcInput::MainStickUp, "`Axis 1-`"),
        (GcInput::MainStickDown, "`Axis 1+`"),
        (GcInput::MainStickLeft, "`Axis 0-`"),
        (GcInput::MainStickRight, "`Axis 0+`"),
        (GcInput::CStickUp, "`Axis 3-`"),
        (GcInput::CStickRight, "`Axis 2+`"),
    ] {
        profile.rebind(input, binding);
    }
    for play in 1..=2 {
        let plan = desk.play(None, "");
        let after = desk.profiles.iter().find(|p| p.identity.profile_key() == "SDL/0/Xbox Series X Controller").unwrap();
        assert_eq!(after.mapping.get(GcInput::MainStickUp), Some("`Axis 1-`"), "play {play}: left stick reset");
        assert_eq!(after.mapping.get(GcInput::CStickUp), Some("`Axis 3-`"), "play {play}: C-stick reset");
        assert!(plan.ini.as_deref().unwrap().contains("Main Stick/Up = `Axis 1-`"), "play {play}: not what the game reads");
        assert!(!blocked(&plan), "play {play}: asked again");
    }
}

// ---------------------------------------------------------------------------
// One pad, one device, named by position (runtime 1.2.13)
// ---------------------------------------------------------------------------

/// Captured 2026-10-09 from a virtual DualShock 4 (ViGEmBus) on the shipped
/// runtime 1.2.12: the one pad is SDL/0, SDL/1, WGInput and DInput.
const DS4_ON_1_2_12: &str = include_str!("fixtures/input-capture-ds4-runtime-1.2.12.txt");
/// The same pad on runtime 1.2.13: WGInput off, SDL adds it once and names
/// its controls by position (`Button A`, `Start`, `Left Y+`).
const DS4_ON_1_2_13: &str = include_str!("fixtures/input-capture-ds4-sdl-runtime-1.2.13.txt");

#[test]
fn a_dualshock_4_on_runtime_1_2_13_is_one_player_with_nothing_to_set_up() {
    let mut desk = Desk::new(DS4_ON_1_2_13);
    let plan = desk.play(None, "");
    assert_eq!(plan.assignment, [(1, "SDL/0/PS4 Controller".to_string())]);
    assert!(plan.dead_by_device.is_empty(), "{:?}", plan.dead_by_device);
    let ini = plan.ini.unwrap();
    for line in ["Device = SDL/0/PS4 Controller", "Buttons/A = `Button A`", "Buttons/Start = `Start`", "Main Stick/Up = `Left Y+`", "C-Stick/Left = `Right X-`", "Triggers/L = `Trigger L`"] {
        assert!(ini.contains(line), "missing {line}:\n{ini}");
    }
}

#[test]
fn a_dualshock_4_on_runtime_1_2_12_is_still_one_player() {
    let mut desk = Desk::new(DS4_ON_1_2_12);
    let plan = desk.play(None, "");
    assert_eq!(plan.assignment, [(1, "WGInput/0/Sony DualShock4 Gamepad".to_string())], "SDL/0 and SDL/1 are the same pad");
    assert!(!blocked(&plan), "{:?}", plan.dead_by_device);
}

/// The update itself: a player who played on 1.2.12 presses Play on 1.2.13.
/// The file on disk names a WGInput device that no longer exists; the pad is
/// SDL's now and works with nothing to set up.
#[test]
fn a_player_from_1_2_12_plays_on_1_2_13_with_nothing_to_set_up() {
    let mut desk = Desk::new(DS4_ON_1_2_12);
    let old = desk.play(None, "").ini.unwrap();
    desk.inputs = parse_device_inputs(DS4_ON_1_2_13);
    desk.scan = scan_of(&desk.inputs);
    let plan = desk.play(Some(&old), &old);
    assert_eq!(plan.assignment, [(1, "SDL/0/PS4 Controller".to_string())]);
    assert!(plan.kept.is_empty(), "{:?}", plan.kept);
    assert!(plan.dead_by_device.is_empty(), "{:?}", plan.dead_by_device);
}
