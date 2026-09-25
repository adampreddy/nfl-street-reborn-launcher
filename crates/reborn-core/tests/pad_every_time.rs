//! "Everyone's controller works every time."
//!
//! Three shipped fixes (3.0.4, 3.0.6, 3.0.8) corrected WHAT IS WRITTEN for a
//! pad the launcher can see. None of them covered the pad not being seen, and
//! none covered bindings being written under a device they were never made
//! for. Both are reproduced here from the reports that came in on 2026-09-18.
//!
//! Measured the same day on a development machine, wired pad, warm services:
//! the runtime's one-shot device scan left a connected pad out of 8 of 52
//! runs, because WGInput reports devices from a Windows callback that can land
//! after the list has been read. A PlayStation pad exists on WGInput and
//! nowhere else.

use reborn_core::controller::{
    adopt_external_edits, standing_pad_file, ControllerProfile, DeviceFamily, DeviceIdentity,
    GcInput, GcPadConfig, Mapping,
};

fn identity(guid: &str, name: &str) -> DeviceIdentity {
    DeviceIdentity {
        guid: guid.into(),
        serial: None,
        name: name.into(),
        family: DeviceFamily::from_name(name),
    }
}

/// The pad file the installer seeds a brand new player with. Xbox vocabulary
/// on an Xbox device line: correct for an Xbox pad, dead for anything else.
const SEEDED_TEMPLATE: &str = "[GCPad1]
Device = XInput/0/Gamepad
Buttons/A = `Button A`
Buttons/B = `Button B`
Buttons/X = `Button X`
Buttons/Y = `Button Y`
Buttons/Z = `Shoulder R`
Buttons/Start = Start
Main Stick/Up = `Left Y+`
Main Stick/Down = `Left Y-`
Main Stick/Left = `Left X-`
Main Stick/Right = `Left X+`
D-Pad/Up = `Pad N`
D-Pad/Down = `Pad S`
D-Pad/Left = `Pad W`
D-Pad/Right = `Pad E`
Triggers/L-Analog = `Shoulder L`
Triggers/R-Analog = `Shoulder R`
Main Stick/Dead Zone = 15.0
Rumble/Motor = `Motor L`
[GCPad2]
Main Stick/Modifier/Range = 50.0
";

// ---------------------------------------------------------------------------
// The scan did not see the pad
// ---------------------------------------------------------------------------

/// "i go to controller settings, test my ps5 controller and it shows its
/// working but when i open the app no connection."
///
/// The scan missed the pad, so the launcher found "no controller" and wrote
/// NOTHING, which left the seeded Xbox-only file in charge. The runtime now
/// binds player one to whichever pad is really connected when the named one is
/// absent, but it can only do that if the bindings speak that pad's language.
/// So a launch that sees no pad still leaves a file every pad can use.
#[test]
fn a_launch_that_sees_no_pad_still_leaves_bindings_every_pad_can_use() {
    let standing = standing_pad_file(SEEDED_TEMPLATE).expect("the seeded file must be rewritten");

    for (input, playstation_name) in [
        (GcInput::ButtonA, "`Cross`"),
        (GcInput::ButtonB, "`Circle`"),
        (GcInput::ButtonZ, "`Bumper R`"),
        (GcInput::Start, "`Menu`"),
        (GcInput::DPadUp, "`Switch 0 N`"),
    ] {
        let line = standing
            .lines()
            .find(|l| l.starts_with(&format!("{} = ", input.ini_key())))
            .unwrap_or_else(|| panic!("{} is missing:\n{standing}", input.ini_key()));
        assert!(
            line.contains(playstation_name),
            "a PlayStation pad cannot press {}: {line}",
            input.ini_key()
        );
        assert_eq!(
            line,
            format!("{} = {}", input.ini_key(), Mapping::universal(input)),
            "the launcher's own universal binding, not an invention"
        );
    }
    // The Xbox pad this file was right for is still right.
    assert!(standing.contains("Buttons/A = `Button A`|"), "{standing}");
    // Everything that is not a binding is left exactly as it was.
    assert!(standing.contains("Device = XInput/0/Gamepad"), "{standing}");
    assert!(standing.contains("Main Stick/Dead Zone = 15.0"), "{standing}");
    assert!(standing.contains("[GCPad2]\nMain Stick/Modifier/Range = 50.0"), "{standing}");
}

/// The three bindings that are not GameCube inputs were dead on a PlayStation
/// pad for exactly the same reason, and are widened on the same terms.
#[test]
fn the_analog_triggers_and_the_motor_are_widened_too() {
    let standing = standing_pad_file(SEEDED_TEMPLATE).expect("rewritten");
    let line = |key: &str| {
        standing
            .lines()
            .find(|l| l.starts_with(&format!("{key} = ")))
            .unwrap_or_else(|| panic!("{key} is missing:\n{standing}"))
            .to_string()
    };
    assert!(line("Triggers/L-Analog").contains("`Bumper L`"), "{}", line("Triggers/L-Analog"));
    assert!(line("Triggers/R-Analog").contains("`Bumper R`"), "{}", line("Triggers/R-Analog"));
    assert!(line("Rumble/Motor").contains("`Motor L`"), "{}", line("Rumble/Motor"));
}

/// A keyboard player, or anyone who set their pad up in Dolphin's own window,
/// has bindings that are THEIRS. A launch that sees no pad must not touch one
/// of them.
#[test]
fn a_players_own_bindings_are_never_rewritten_by_the_standing_file() {
    let theirs = "[GCPad1]
Device = DInput/0/Keyboard Mouse
Buttons/A = X
Buttons/B = Z
Main Stick/Up = UP
Main Stick/Down = DOWN
";
    assert_eq!(standing_pad_file(theirs), None, "nothing in that file is ours to change");
}

/// A player's remap sits among the launcher's own lines. Only the launcher's
/// own lines are widened.
#[test]
fn a_remap_survives_beside_the_lines_that_are_widened() {
    let mixed = SEEDED_TEMPLATE.replace("Buttons/A = `Button A`", "Buttons/A = `Trigger R`");
    let standing = standing_pad_file(&mixed).expect("the other lines still need widening");
    assert!(standing.contains("Buttons/A = `Trigger R`\n"), "their remap:\n{standing}");
    assert!(standing.contains("Buttons/B = `Button B`|`Circle`"), "{standing}");
}

/// Already universal means nothing to do, so nothing is written: the launcher
/// does not touch a file on every launch for no reason.
#[test]
fn a_file_that_is_already_universal_is_left_alone() {
    let once = standing_pad_file(SEEDED_TEMPLATE).expect("first pass rewrites");
    assert_eq!(standing_pad_file(&once), None, "the second pass has nothing to say");
}

/// No pad file at all still has to end with player one bound.
#[test]
fn a_missing_pad_file_gets_a_universal_player_one() {
    let standing = standing_pad_file("").expect("player one needs a section");
    assert!(standing.starts_with("[GCPad1]\n"), "{standing}");
    assert!(
        standing.contains(&format!("Buttons/A = {}", Mapping::universal(GcInput::ButtonA))),
        "{standing}"
    );
}

// ---------------------------------------------------------------------------
// Bindings written under a device they were never made for
// ---------------------------------------------------------------------------

/// "I have a USB gamecube controller that I would always use but now it won't
/// read the stick inputs, only the buttons work."
///
/// Their pad file came from 2.1, set up in Dolphin against the DirectInput
/// device. The launcher owns the Device line and gives the pad its WGInput
/// one, and adoption used to be keyed by PORT alone, so every DirectInput
/// binding was carried across. `Button 1` happens to exist on both backends,
/// so the buttons worked. `Axis Y-` is DirectInput only (raw WGInput calls it
/// `Axis 1-`), so the sticks were dead.
///
/// Bindings and the device they were made for travel together or not at all.
#[test]
fn bindings_made_for_another_device_are_not_adopted() {
    let from_2_1 = "[GCPad1]
Device = DInput/0/Generic USB Joystick
Buttons/A = `Button 1`
Buttons/B = `Button 2`
Main Stick/Up = `Axis Y-`
Main Stick/Down = `Axis Y+`
Main Stick/Left = `Axis X-`
Main Stick/Right = `Axis X+`
";
    let profile = ControllerProfile::new(identity("WGInput:0", "Generic USB Joystick"), 1);
    let mut profiles = [profile.clone()];

    let adopted = adopt_external_edits(from_2_1, "", &mut profiles, |_| {
        Some("WGInput/0/Generic USB Joystick".to_string())
    });

    assert!(adopted.is_empty(), "adopted across devices: {adopted:?}");
    assert_eq!(profiles[0], profile, "a DirectInput binding was written into a WGInput profile");
}

/// Two pads, one player, one port. A remap made while the Xbox pad was bound
/// belongs to the Xbox pad; it must not follow the player onto a PlayStation
/// pad, where `Button B` is a name that does not exist.
#[test]
fn a_remap_made_on_one_pad_does_not_land_on_another() {
    let xbox = ControllerProfile::new(identity("XInput:0", "Gamepad"), 1);
    let playstation = ControllerProfile::new(identity("WGInput:0", "PS5 Controller"), 1);
    let ours = GcPadConfig::generate(std::slice::from_ref(&xbox), |_| "XInput/0/Gamepad".into());
    let theirs = ours.replace(
        &format!("Buttons/A = {}", Mapping::universal(GcInput::ButtonA)),
        "Buttons/A = `Button B`",
    );
    assert_ne!(ours, theirs, "the edit must have landed");

    let mut profiles = [xbox, playstation.clone()];
    adopt_external_edits(&theirs, &ours, &mut profiles, |id| match id.guid.as_str() {
        "XInput:0" => Some("XInput/0/Gamepad".to_string()),
        "WGInput:0" => Some("WGInput/0/PS5 Controller".to_string()),
        _ => None,
    });

    assert_eq!(profiles[0].mapping.get(GcInput::ButtonA), Some("`Button B`"), "the pad it was made on");
    assert_eq!(profiles[1], playstation, "and no other");
}

/// A pad that is not plugged in this session has no section in the file that
/// can be its own, so nothing is adopted into it.
#[test]
fn nothing_is_adopted_into_a_pad_that_is_not_connected() {
    let profile = ControllerProfile::new(identity("XInput:0", "Gamepad"), 1);
    let theirs = "[GCPad1]\nDevice = XInput/0/Gamepad\nButtons/A = `Button B`\n";
    let mut profiles = [profile.clone()];
    let adopted = adopt_external_edits(theirs, "", &mut profiles, |_| None);
    assert!(adopted.is_empty(), "{adopted:?}");
    assert_eq!(profiles[0], profile);
}

/// The positive control: the same device, so the remap IS the player's and is
/// kept, whichever port the file happens to have it on.
#[test]
fn a_remap_on_the_same_device_is_adopted_whatever_port_it_is_on() {
    let mut profiles = [ControllerProfile::new(identity("WGInput:1", "Controller"), 1)];
    let theirs = "[GCPad2]\nDevice = WGInput/1/Controller\nButtons/A = Circle\n";
    let adopted = adopt_external_edits(theirs, "", &mut profiles, |_| {
        Some("WGInput/1/Controller".to_string())
    });
    assert!(!adopted.is_empty());
    assert_eq!(profiles[0].mapping.get(GcInput::ButtonA), Some("Circle"));
}

// ---------------------------------------------------------------------------
// Never silent: which controls cannot be pressed on THIS pad
// ---------------------------------------------------------------------------

fn ps5_names() -> Vec<String> {
    reborn_core::devices::parse_device_inputs(include_str!("fixtures/input-capture-ps5.txt"))
        .into_iter()
        .find(|d| d.device.starts_with("WGInput/"))
        .expect("the captured PS5 dump has a WGInput device")
        .all_names()
}

/// What the fork's WGInput backend reports for a pad Windows has no gamepad
/// mapping for (a USB GameCube-style pad, a generic HID pad). The name FORMATS
/// are read out of the fork's WGInput.cpp: `Button {i}`, `Axis {i}+/-`,
/// `Switch {i} N/E/S/W`. Nothing here says what `Button 3` IS, which is the
/// whole problem: such a pad can only be set up by pressing its buttons.
fn unlabelled_pad_names() -> Vec<String> {
    let mut names: Vec<String> = (0..12).map(|i| format!("Button {i}")).collect();
    for axis in 0..4 {
        names.push(format!("Axis {axis}+"));
        names.push(format!("Axis {axis}-"));
    }
    for direction in ["N", "E", "S", "W"] {
        names.push(format!("Switch 0 {direction}"));
    }
    names
}

/// The universal binding on a PlayStation pad: every control can be pressed.
/// The judgement has to see through the OR, which the older `unresolvable`
/// guardrail deliberately does not.
#[test]
fn a_playstation_pad_on_the_universal_binding_has_no_dead_controls() {
    let profile = ControllerProfile::new(identity("WGInput:0", "PS5 Controller"), 1);
    assert_eq!(profile.dead_controls(&ps5_names()), Vec::<GcInput>::new());
}

/// "only the buttons work... it won't read the stick". On a pad with numbered
/// controls nothing in the universal binding exists except the d-pad, and the
/// launcher said nothing at all. Now it can say exactly which controls.
#[test]
fn a_pad_with_numbered_controls_reports_every_control_the_player_must_press() {
    let profile = ControllerProfile::new(identity("WGInput:0", "Generic USB Joystick"), 1);
    let dead = profile.dead_controls(&unlabelled_pad_names());

    for input in [GcInput::ButtonA, GcInput::Start, GcInput::MainStickUp, GcInput::CStickLeft, GcInput::TriggerR] {
        assert!(dead.contains(&input), "{input:?} cannot be pressed and was not reported: {dead:?}");
    }
    // `Switch 0 N` is in the universal binding and this pad has it.
    assert!(!dead.contains(&GcInput::DPadUp), "the d-pad resolves: {dead:?}");
    assert_eq!(dead.len(), 16, "twenty controls less the four d-pad directions: {dead:?}");
}

/// Once the player has pressed them, they are no longer dead.
#[test]
fn a_control_the_player_pressed_is_no_longer_dead() {
    let mut profile = ControllerProfile::new(identity("WGInput:0", "Generic USB Joystick"), 1);
    profile.rebind(GcInput::ButtonA, "`Button 2`");
    profile.rebind(GcInput::MainStickUp, "`Axis 1-`");
    let dead = profile.dead_controls(&unlabelled_pad_names());
    assert!(!dead.contains(&GcInput::ButtonA), "{dead:?}");
    assert!(!dead.contains(&GcInput::MainStickUp), "{dead:?}");
    assert!(dead.contains(&GcInput::ButtonB), "{dead:?}");
}

/// The transplant the USB GameCube player is living with today: DirectInput
/// names under a WGInput device. `Button 1` exists on both, `Axis Y-` does not.
#[test]
fn bindings_left_over_from_another_backend_are_reported_dead() {
    let mut profile = ControllerProfile::new(identity("WGInput:0", "Generic USB Joystick"), 1);
    profile.rebind(GcInput::ButtonA, "`Button 1`");
    profile.rebind(GcInput::MainStickUp, "`Axis Y-`");
    let dead = profile.dead_controls(&unlabelled_pad_names());
    assert!(!dead.contains(&GcInput::ButtonA), "Button 1 exists here: {dead:?}");
    assert!(dead.contains(&GcInput::MainStickUp), "Axis Y- is DirectInput only: {dead:?}");
}

/// No device report is no evidence, and no evidence is not an accusation.
#[test]
fn with_no_device_report_nothing_is_called_dead() {
    let profile = ControllerProfile::new(identity("WGInput:0", "Anything"), 1);
    assert!(profile.dead_controls(&[]).is_empty());
}

/// A hand written expression is not something this can evaluate, and it must
/// not veto what it cannot read.
#[test]
fn a_hand_written_expression_is_never_called_dead() {
    let mut profile = ControllerProfile::new(identity("WGInput:0", "Generic USB Joystick"), 1);
    profile.rebind(GcInput::ButtonA, "`Button 1` & !`Button 2`");
    assert!(!profile.dead_controls(&unlabelled_pad_names()).contains(&GcInput::ButtonA));
}

// ---------------------------------------------------------------------------
// Every name the runtime can give a button, not just the ones one pad gave it
// ---------------------------------------------------------------------------

/// "i got a ps5 controller and ive tried with my switch controller but after i
/// open the app i cant get pass the press start screen" (2026-09-19).
///
/// The names bound to Start were `Start` and `Menu`, because the one
/// PlayStation pad that could be measured called its button `Menu`. The
/// fork's WGInput backend has FOUR names for the button in that position
/// (WGInput.cpp's label table, from Windows' GameControllerButtonLabel):
/// `Start`, `Menu`, `Options`, `Plus`. A Switch controller says `Plus`; a
/// PlayStation pad says `Options` or `Menu` depending on the driver. A name
/// that is not bound binds nothing and says nothing, and the first thing the
/// game asks for is Start.
///
/// The list of labels is finite and lives in the fork's source, so this is
/// made complete from that list, never again from whichever pad was to hand.
#[test]
fn every_name_the_runtime_has_for_the_start_button_presses_start() {
    for name in ["Start", "Menu", "Options", "Plus"] {
        let profile = ControllerProfile::new(identity("WGInput:0", "Some Controller"), 1);
        let available = vec![name.to_string(), "Left Y+".to_string()];
        assert!(
            !profile.dead_controls(&available).contains(&GcInput::Start),
            "a pad that calls it `{name}` cannot get past the press start screen: Start = {:?}",
            profile.mapping.get(GcInput::Start)
        );
    }
}

/// A pad that labels its face buttons with letters gets each GameCube letter
/// on the button carrying that letter. GameCube pads have an A, a B, an X and
/// a Y of their own, so this is the one case where the letter IS the answer
/// and no position has to be guessed: a Switch player pressing A gets A.
#[test]
fn a_pad_that_letters_its_face_buttons_gets_each_letter_on_its_own_button() {
    let profile = ControllerProfile::new(identity("WGInput:0", "Pro Controller"), 1);
    let available: Vec<String> =
        ["Letter A", "Letter B", "Letter X", "Letter Y"].iter().map(|s| s.to_string()).collect();
    let dead = profile.dead_controls(&available);
    for (input, letter) in [
        (GcInput::ButtonA, "`Letter A`"),
        (GcInput::ButtonB, "`Letter B`"),
        (GcInput::ButtonX, "`Letter X`"),
        (GcInput::ButtonY, "`Letter Y`"),
    ] {
        assert!(!dead.contains(&input), "{input:?} does nothing on a lettered pad: {dead:?}");
        assert!(
            profile.mapping.get(input).is_some_and(|b| b.contains(letter)),
            "{input:?} must be on the button with its own letter: {:?}",
            profile.mapping.get(input)
        );
    }
}

/// PlayStation-style labels for the shoulder buttons: `Right 1` is R1.
#[test]
fn z_is_pressable_on_a_pad_that_numbers_its_shoulders() {
    let profile = ControllerProfile::new(identity("WGInput:0", "Some Controller"), 1);
    assert!(!profile.dead_controls(&["Right 1".to_string()]).contains(&GcInput::ButtonZ));
}

/// Widening the vocabulary must not move a pad that already works: the names
/// measured on the real DualSense still win, in the same order.
#[test]
fn the_measured_playstation_pad_still_narrows_to_the_names_it_always_did() {
    let (mapping, unmatched) = Mapping::auto_from(&ps5_names());
    assert!(unmatched.is_empty(), "{unmatched:?}");
    assert_eq!(mapping.get(GcInput::Start), Some("`Menu`"));
    assert_eq!(mapping.get(GcInput::ButtonA), Some("`Cross`"));
    assert_eq!(mapping.get(GcInput::ButtonZ), Some("`Bumper R`"));
}

// ---------------------------------------------------------------------------
// Never start the game on a pad that cannot press Start
// ---------------------------------------------------------------------------

use reborn_core::session::{player_one_blocked, BlockedPad, Refusal};
use std::collections::BTreeMap;

/// The launcher has known at every launch which controls cannot be pressed,
/// and wrote it to a log nobody reads. The player found out at the press start
/// screen, holding a pad that did nothing, with no idea why. Start and A are
/// the two controls without which the game cannot be entered at all.
#[test]
fn a_pad_that_cannot_press_start_or_a_blocks_play() {
    use reborn_core::controller::blocks_play;
    assert!(blocks_play(&[GcInput::Start]));
    assert!(blocks_play(&[GcInput::ButtonA]));
    assert!(blocks_play(&[GcInput::CStickUp, GcInput::Start]));
}

#[test]
fn working_buttons_with_unmapped_usb_axes_need_setup_before_play() {
    let profile = ControllerProfile::new(identity("SDL/0/USB pad", "USB pad"), 1);
    let names: Vec<String> = ["Start", "Button A", "Axis 0+", "Axis 0-", "Axis 1+", "Axis 1-"]
        .into_iter().map(String::from).collect();
    let dead = profile.dead_controls(&names);
    assert!(!dead.contains(&GcInput::Start));
    assert!(!dead.contains(&GcInput::ButtonA));
    for direction in [GcInput::MainStickUp, GcInput::MainStickDown, GcInput::MainStickLeft, GcInput::MainStickRight] {
        assert!(dead.contains(&direction));
        assert!(reborn_core::controller::blocks_play(&[direction]));
    }
    let message = Refusal::ControllerNeedsSetup(BlockedPad {
        device: "SDL/0/USB pad".into(), controls: dead,
    }).message();
    assert!(message.contains("Move down"), "{message}");
}

#[test]
fn measured_usb_axis_mapping_allows_play_without_a_c_stick() {
    let mut profile = ControllerProfile::new(identity("SDL/0/USB pad", "USB pad"), 1);
    for (input, name) in [(GcInput::MainStickUp,"Axis 1-"), (GcInput::MainStickDown,"Axis 1+"),
        (GcInput::MainStickLeft,"Axis 0-"), (GcInput::MainStickRight,"Axis 0+")] {
        profile.rebind(input, Mapping::quote(name));
    }
    let names: Vec<String> = ["Start", "Button A", "Axis 0+", "Axis 0-", "Axis 1+", "Axis 1-"]
        .into_iter().map(String::from).collect();
    let dead = profile.dead_controls(&names);
    assert!(dead.contains(&GcInput::CStickUp));
    assert!(!reborn_core::controller::blocks_play(&dead));
}

/// A pad with a dead C-stick is a worse pad, not an unplayable one. Stopping
/// Play for it would be nagging, and the Controllers page already says so.
#[test]
fn a_pad_missing_only_lesser_controls_does_not_block_play() {
    use reborn_core::controller::blocks_play;
    assert!(!blocks_play(&[]));
    assert!(!blocks_play(&[GcInput::CStickUp, GcInput::CStickDown, GcInput::ButtonZ]));
}

fn dead(entries: &[(&str, &[GcInput])]) -> BTreeMap<String, Vec<GcInput>> {
    entries.iter().map(|(d, c)| (d.to_string(), c.to_vec())).collect()
}

/// Only PLAYER ONE's pad decides. They are the one at the press start screen.
#[test]
fn only_player_ones_pad_can_block_play() {
    let pads = vec![(1u8, "XInput/0/Gamepad".to_string()), (2u8, "WGInput/0/Generic USB Joystick".to_string())];
    let broken_p2 = dead(&[("WGInput/0/Generic USB Joystick", &[GcInput::Start, GcInput::ButtonA])]);
    assert_eq!(player_one_blocked(&pads, &broken_p2), None, "player two's pad must not stop player one playing");

    let broken_p1 = dead(&[("XInput/0/Gamepad", &[GcInput::Start])]);
    assert_eq!(
        player_one_blocked(&pads, &broken_p1),
        Some(BlockedPad { device: "XInput/0/Gamepad".into(), controls: vec![GcInput::Start] })
    );
}

/// No pad at all is a keyboard player, or a pad that turns up later and is
/// picked up by the runtime. Neither is this launch's business to refuse.
#[test]
fn no_pad_at_all_never_blocks_play() {
    assert_eq!(player_one_blocked(&[], &dead(&[])), None);
    assert_eq!(player_one_blocked(&[], &dead(&[("WGInput/0/Pad", &[GcInput::Start])])), None);
}

/// What the player reads. It has to say what is wrong in their words, which
/// controls, and the one place to fix it. Never "error", and never the word
/// this project does not use for its own faults.
#[test]
fn the_refusal_tells_the_player_which_controls_and_where_to_fix_them() {
    let refusal = Refusal::ControllerNeedsSetup(BlockedPad {
        device: "WGInput/0/Pro Controller".into(),
        controls: vec![GcInput::Start, GcInput::ButtonA],
    });
    let message = refusal.message();
    assert!(message.contains("Start") && message.contains("A"), "{message}");
    assert!(message.contains("Controllers"), "it must say where to go: {message}");
    for banned in ["error", "Error", "custom", "Custom", "Dolphin", "WGInput"] {
        assert!(!message.contains(banned), "a player should never read `{banned}`: {message}");
    }
}
