//! Linux pads: evdev names, measured from the fork's evdev.cpp naming rules.
//! A Steam Deck tester saw "20 of 20 controls cannot be set up automatically"
//! on both the Deck's controller and Steam's virtual pad (2026-10-03).
use reborn_core::controller::{GcInput, LinuxPad, Mapping};

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

/// Steam Input's "Microsoft X-Box 360 pad": xpad keys, ABS X Y Z RX RY RZ HAT0X HAT0Y.
fn steam_virtual() -> Vec<String> {
    let mut v = names(&["SOUTH", "EAST", "NORTH", "WEST", "TL", "TR", "SELECT", "START", "MODE", "THUMBL", "THUMBR"]);
    for n in 0..8 {
        v.push(format!("Axis {n}-"));
        v.push(format!("Axis {n}+"));
    }
    v.extend(names(&["Strong", "Weak"]));
    v
}

/// The Deck's own controller through hid-steam: ABS X Y RX RY + pads/triggers as hats.
fn deck_raw() -> Vec<String> {
    let mut v = names(&["SOUTH", "EAST", "NORTH", "WEST", "TL", "TR", "TL2", "TR2", "SELECT", "START", "MODE",
        "THUMBL", "THUMBR", "DPAD_UP", "DPAD_DOWN", "DPAD_LEFT", "DPAD_RIGHT"]);
    for n in 0..10 {
        v.push(format!("Axis {n}-"));
        v.push(format!("Axis {n}+"));
    }
    v
}

#[test]
fn steams_virtual_pad_maps_every_control_by_itself() {
    let pad = steam_virtual();
    assert_eq!(LinuxPad::from_names(&pad), Some(LinuxPad::SteamVirtual));
    let (m, unmatched) = Mapping::auto_from(&pad);
    assert!(unmatched.is_empty(), "unmatched: {unmatched:?}");
    assert_eq!(m.get(GcInput::ButtonA), Some("`SOUTH`"));
    assert_eq!(m.get(GcInput::ButtonX), Some("`NORTH`"), "0x133 is Xbox X on xpad and Steam");
    assert_eq!(m.get(GcInput::Start), Some("`START`"));
    assert_eq!(m.get(GcInput::MainStickUp), Some("`Axis 1-`"));
    assert_eq!(m.get(GcInput::CStickRight), Some("`Axis 3+`"));
    assert_eq!(m.get(GcInput::TriggerL), Some("`Axis 2+`"));
    assert_eq!(m.get(GcInput::DPadUp), Some("`Axis 7-`"));
}

#[test]
fn the_decks_own_controller_maps_every_control_by_itself() {
    let pad = deck_raw();
    assert_eq!(LinuxPad::from_names(&pad), Some(LinuxPad::DeckRaw));
    let (m, unmatched) = Mapping::auto_from(&pad);
    assert!(unmatched.is_empty(), "unmatched: {unmatched:?}");
    assert_eq!(m.get(GcInput::CStickLeft), Some("`Axis 2-`"));
    assert_eq!(m.get(GcInput::TriggerR), Some("`TR2`"));
    assert_eq!(m.get(GcInput::DPadDown), Some("`DPAD_DOWN`"));
}

#[test]
fn older_libevdev_letters_still_map() {
    let mut pad = steam_virtual();
    for (new, old) in [("SOUTH", "A"), ("EAST", "B"), ("NORTH", "X"), ("WEST", "Y")] {
        let i = pad.iter().position(|n| n == new).unwrap();
        pad[i] = old.to_string();
    }
    let (m, unmatched) = Mapping::auto_from(&pad);
    assert!(unmatched.is_empty(), "unmatched: {unmatched:?}");
    assert_eq!(m.get(GcInput::ButtonY), Some("`Y`"));
}

#[test]
fn a_windows_xbox_pad_is_not_read_as_a_linux_one() {
    let xinput = names(&["Button A", "Button B", "Button X", "Button Y", "Shoulder L", "Shoulder R", "Back", "Start",
        "Left X-", "Left X+", "Left Y-", "Left Y+", "Right X-", "Right X+", "Right Y-", "Right Y+",
        "Trigger L", "Trigger R", "Pad N", "Pad S", "Pad W", "Pad E", "Motor L", "Motor R"]);
    assert_eq!(LinuxPad::from_names(&xinput), None);
    let (m, unmatched) = Mapping::auto_from(&xinput);
    assert!(unmatched.is_empty());
    assert_eq!(m.get(GcInput::ButtonA), Some("`Button A`"));
    assert_eq!(m.get(GcInput::TriggerL), Some("`Trigger L`"));
}
