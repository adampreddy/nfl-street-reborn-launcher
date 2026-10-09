//! Print the `GCPadNew.ini` Play would write for a captured device list.
//!
//! `cargo run -p reborn-core --example pad_plan -- <--dump-device-inputs file>`

use reborn_core::devices::{parse_device_inputs, Backend, DetectedDevice, Enumeration};
use reborn_core::session::{plan_pads, PadInputs};

fn main() {
    let path = std::env::args().nth(1).expect("a device dump");
    let inputs = parse_device_inputs(&std::fs::read_to_string(path).expect("readable"));
    let devices = inputs
        .iter()
        .filter_map(|d| {
            let mut it = d.device.splitn(3, '/');
            let (source, id, name) = (it.next()?, it.next()?, it.next()?);
            let backend = match source {
                "DInput" => Backend::DInput,
                "XInput" => Backend::XInput,
                "SDL" => Backend::Sdl,
                other => Backend::Other(other.into()),
            };
            let family = match reborn_core::controller::DeviceFamily::from_name(name) {
                reborn_core::controller::DeviceFamily::Generic if backend == Backend::XInput => reborn_core::controller::DeviceFamily::Xbox,
                other => other,
            };
            Some(DetectedDevice { backend, id: id.parse().ok()?, name: name.into(), family })
        })
        .collect();
    let scan = Enumeration { devices, not_searched: vec![] };
    let plan = plan_pads(
        &PadInputs { scan: &scan, device_inputs: &inputs, live: None, last_written: "", enabled_ports: [true, false, false, false], primary: None, slots: None },
        vec![],
    );
    eprintln!("assignment {:?} dead {:?}", plan.assignment, plan.dead_by_device);
    print!("{}", plan.ini.unwrap_or_default());
}
