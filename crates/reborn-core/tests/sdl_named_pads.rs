//! Runtime 1.2.7: WGInput is off, Xbox pads are XInput's, and SDL names every
//! pad it recognises like an Xbox pad. These lists are what that runtime prints.

use reborn_core::{
    controller::{blocks_play, ControllerProfile},
    devices::parse_device_list,
    session::build_gcpad_for_slots,
};

/// Every input the runtime's SDL backend names on a recognised pad.
fn sdl_gamepad_inputs() -> Vec<String> {
    [
        "Button A", "Button B", "Button X", "Button Y", "Pad N", "Pad S", "Pad W", "Pad E",
        "Start", "Back", "Shoulder L", "Shoulder R", "Guide", "Thumb L", "Thumb R", "Touchpad",
        "Left X-", "Left X+", "Left Y-", "Left Y+", "Right X-", "Right X+", "Right Y-", "Right Y+",
        "Trigger L", "Trigger R", "Button 0", "Button 1", "Axis 0-", "Axis 0+", "Motor",
    ]
    .into_iter()
    .map(String::from)
    .collect()
}

fn pads(list: &str) -> Vec<String> {
    parse_device_list(list).playable_pads().iter().map(|d| d.device_string()).collect()
}

#[test]
fn a_ps5_pad_alone_is_player_one_through_sdl() {
    assert_eq!(
        pads("DInput/0/Keyboard Mouse\nDInput/0/Wireless Controller\nSDL/0/PS5 Controller"),
        ["SDL/0/PS5 Controller"]
    );
}

#[test]
fn a_ps5_pad_beside_an_xbox_pad_is_player_two() {
    let scan = parse_device_list(
        "DInput/0/Keyboard Mouse\nDInput/0/Wireless Controller\nSDL/0/PS5 Controller\nXInput/0/Gamepad",
    );
    let names: Vec<String> = scan.playable_pads().iter().map(|d| d.device_string()).collect();
    assert_eq!(names, ["XInput/0/Gamepad", "SDL/0/PS5 Controller"]);
    let (_, assignments) = build_gcpad_for_slots(&scan, &[], None, None).unwrap();
    assert_eq!(
        assignments,
        [(1, "XInput/0/Gamepad".to_string()), (2, "SDL/0/PS5 Controller".to_string())]
    );
}

#[test]
fn two_switch_pros_are_two_players() {
    assert_eq!(
        pads("SDL/0/Nintendo Switch Pro Controller\nSDL/1/Nintendo Switch Pro Controller"),
        ["SDL/0/Nintendo Switch Pro Controller", "SDL/1/Nintendo Switch Pro Controller"]
    );
}

#[test]
fn an_xbox_named_sdl_entry_never_doubles_an_xinput_pad() {
    assert_eq!(pads("SDL/0/Xbox Series X Controller\nXInput/0/Gamepad"), ["XInput/0/Gamepad"]);
}

#[test]
fn an_older_runtime_list_keeps_the_old_rule() {
    // 1.2.6 lists WGInput and numbers SDL's controls: SDL must not be offered.
    assert_eq!(
        pads("SDL/0/PS5 Controller\nWGInput/0/PS5 Controller\nXInput/0/Gamepad"),
        ["XInput/0/Gamepad", "WGInput/0/PS5 Controller"]
    );
}

#[test]
fn a_recognised_sdl_pad_maps_itself_completely_and_never_stops_play() {
    let device = parse_device_list("SDL/0/PS5 Controller").devices.remove(0);
    let available = sdl_gamepad_inputs();
    let profile = ControllerProfile::from_device(device.identity(), 1, &available);
    let dead = profile.dead_controls(&available);
    assert!(dead.is_empty(), "dead on a recognised SDL pad: {dead:?}");
    assert!(!blocks_play(&dead));
}
