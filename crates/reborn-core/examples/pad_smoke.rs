//! Generate the real pad file, from the real runtime, against whatever is
//! actually plugged into this machine.
//!
//! The unit tests prove the logic and a fixture proves the parsing, but neither
//! starts the runtime. This does: it runs the same three steps a launch runs,
//! in the same order, and prints what would be written to `GCPadNew.ini`.
//!
//! It touches nothing. The scratch directory is passed in, the profile is built
//! in memory, and no file belonging to a player or to this machine is read or
//! written.
//!
//!     cargo run -p reborn-core --example pad_smoke -- "<path to the runtime exe>"

use reborn_core::controller::ControllerProfile;
use reborn_core::devices::{dump_via_runtime, known_inputs, DeviceInputs};
use reborn_core::platform::Platform;
use reborn_core::session::build_gcpad_for_slots;

fn main() {
    let Some(exe) = std::env::args().nth(1) else {
        eprintln!("usage: pad_smoke <runtime exe>");
        std::process::exit(2);
    };
    let platform = Platform::detect();
    let scratch = std::env::temp_dir().join("reborn-pad-smoke");

    println!("runtime: {exe}");
    let (scan, reported): (_, Vec<DeviceInputs>) =
        match dump_via_runtime(&platform, std::path::Path::new(&exe), &scratch) {
            Ok(pair) => {
                println!("dump: the runtime listed its devices and their controls\n");
                pair
            }
            Err(why) => {
                eprintln!("dump refused: {why}");
                std::process::exit(1);
            }
        };

    for device in &reported {
        println!(
            "  {} : {} input(s), {} output(s)",
            device.device,
            device.inputs.len(),
            device.outputs.len()
        );
    }

    let pads = scan.playable_pads();
    println!("\noffered as pads: {}", pads.len());
    if pads.is_empty() {
        eprintln!("no controller is connected, so there is nothing to prove here");
        std::process::exit(1);
    }

    let mut profiles = Vec::new();
    for (index, pad) in pads.iter().enumerate() {
        let available = reported
            .iter()
            .find(|d| d.device == pad.device_string())
            .map(|d| d.all_names())
            .or_else(|| known_inputs(&pad.device_string()))
            .unwrap_or_default();

        let profile =
            ControllerProfile::from_device(pad.identity(), index as u8 + 1, &available);
        let dead = profile.mapping.unresolvable(&available);
        let dead_extras = profile.extras.unresolvable(&available);

        println!(
            "\n  {}  family={:?}\n    unresolved controls: {:?}\n    unresolved extras:   {:?}",
            pad.device_string(),
            pad.family,
            dead,
            dead_extras
        );
        if !dead.is_empty() {
            println!(
                "    ^ these need the remap wizard; this pad reports numbers, not names"
            );
        }
        profiles.push(profile);
    }

    match build_gcpad_for_slots(&scan, &profiles, None, None) {
        Some((ini, assignment)) => {
            println!("\nports: {assignment:?}\n\n---- GCPadNew.ini ----\n{ini}---- end ----");
        }
        None => {
            eprintln!("no pad file was produced");
            std::process::exit(1);
        }
    }
}
