//! Real hardware, real runtime: what `plan_pads` keeps when a connected pad's
//! other entry (the one Dolphin's own window lists) holds a setup.
//!
//! `cargo run -p reborn-core --example player_setup_smoke -- <runtime exe> <scratch dir>`
//! Reads the pads, writes nothing outside the scratch dir.

use reborn_core::platform::Platform;
use reborn_core::session::{plan_pads, PadInputs};

fn main() {
    let mut args = std::env::args().skip(1);
    let exe = std::path::PathBuf::from(args.next().expect("runtime exe"));
    let scratch = std::path::PathBuf::from(args.next().expect("scratch dir"));
    let platform = Platform::detect();
    let (scan, inputs) = reborn_core::devices::dump_via_runtime(&platform, &exe, &scratch).expect("dump");
    println!("DEVICES");
    for d in &scan.devices {
        println!("  {} ({:?})", d.device_string(), d.family);
    }
    let offered: Vec<String> = scan.playable_pads().iter().map(|d| d.device_string()).collect();
    println!("OFFERED {offered:?}");

    let first = plan_pads(&PadInputs { scan: &scan, device_inputs: &inputs, live: None, last_written: "", enabled_ports: [true, false, false, false], primary: None, slots: None }, vec![]);
    println!("PLAY 1 assignment {:?} kept {:?}", first.assignment, first.kept);

    // The player picks a pad entry the launcher does not offer and binds A and Start to it.
    let Some(other) = scan.devices.iter().find(|d| d.is_controller() && !offered.contains(&d.device_string()) && d.family != reborn_core::controller::DeviceFamily::Keyboard) else {
        println!("no second entry for any pad on this machine; nothing more to show");
        return;
    };
    let names = inputs.iter().find(|i| i.device == other.device_string()).map(|i| i.names()).unwrap_or_default();
    println!("THEIR DEVICE {} with {} inputs, first {:?}", other.device_string(), names.len(), names.iter().take(4).collect::<Vec<_>>());
    let theirs = format!("[GCPad1]\nDevice = {}\nButtons/A = `{}`\n", other.device_string(), names.first().cloned().unwrap_or_default());
    let ours = first.ini.unwrap_or_default();
    let second = plan_pads(&PadInputs { scan: &scan, device_inputs: &inputs, live: Some(&theirs), last_written: &ours, enabled_ports: [true, false, false, false], primary: None, slots: None }, first.profiles.clone());
    println!("PLAY 2 assignment {:?} kept {:?} dead {:?}", second.assignment, second.kept, second.dead_by_device.keys().collect::<Vec<_>>());
    println!("PLAY 2 FILE\n{}", second.ini.unwrap_or_default());
}
