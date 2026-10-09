//! What the launcher saw of the pads at Play, sent to the backend when it changes.
//!
//! Adam, 2026-10-09: "I never want my users to deal with controller issues ever." Every controller bug so far
//! reached us as one sentence in Discord, days late, with nothing to go on: no device list, no pad file, no idea
//! which backend the pad came through. This is that evidence, sent by the launcher itself: the devices the
//! emulator listed, which pad went on which port (the launcher's pick or a setup the player made in Dolphin), and
//! the controls that cannot be pressed on it. Model names and versions only; never a path, never a file.
//!
//! Sent only when it differs from the last report this install sent, so a player who plays every night with the
//! same pad sends one report, not one a night. Best effort: a report that cannot be sent is never a reason to
//! delay or refuse Play.

use crate::devices::Enumeration;
use crate::platform::{DataDir, Platform};
use crate::session::PadPlan;

pub const SCHEMA: &str = "nsr.pad_report.v1";
const LAST_SENT: &str = "pad-report.last.json";

/// The report for one Play, from what the runtime listed and what [`crate::session::plan_pads`] decided.
pub fn build(scan: &Enumeration, plan: &PadPlan, authoritative: bool, runtime_version: Option<&str>) -> serde_json::Value {
    let dead = |device: &str| -> Vec<&'static str> {
        plan.dead_by_device.get(device).map(|d| d.iter().map(|i| i.ini_key()).collect()).unwrap_or_default()
    };
    let mut ports: Vec<serde_json::Value> = plan
        .assignment
        .iter()
        .map(|(port, device)| serde_json::json!({ "port": port, "device": device, "source": "launcher", "dead": dead(device) }))
        .chain(plan.kept.iter().map(|(port, device)| {
            serde_json::json!({ "port": port, "device": device, "source": "player_setup", "dead": dead(device) })
        }))
        .collect();
    ports.sort_by_key(|p| p["port"].as_u64());
    serde_json::json!({
        "schema": SCHEMA,
        "launcher_version": crate::build_version(),
        "runtime_version": runtime_version,
        "os": std::env::consts::OS,
        "authoritative": authoritative,
        "devices": scan.devices.iter().map(|d| d.device_string()).collect::<Vec<_>>(),
        "ports": ports,
        "needs_setup": !plan.dead_by_device.is_empty(),
    })
}

/// The installed emulator's version, from its content pointer (`versions/<runtime>/current.json`).
pub fn runtime_version(platform: &Platform) -> Option<String> {
    let pointer = platform
        .paths
        .data_root()
        .ok()?
        .join(DataDir::Versions.relative())
        .join(crate::inventory::ComponentId::runtime_here().as_str())
        .join("current.json");
    let json: serde_json::Value = serde_json::from_slice(&std::fs::read(pointer).ok()?).ok()?;
    json["current"]["version"].as_str().map(str::to_string)
}

/// Whether `report` differs from the one last sent from this install.
pub fn is_new(last_sent: Option<&str>, report: &serde_json::Value) -> bool {
    last_sent.and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok()).as_ref() != Some(report)
}

/// Send `report` in the background if it is new. Never blocks, never fails Play.
pub fn send_if_new(platform: &Platform, report: serde_json::Value) {
    let Ok(cache) = platform.paths.data_root().map(|r| r.join(DataDir::Cache.relative())) else { return };
    let last_path = cache.join(LAST_SENT);
    if !is_new(std::fs::read_to_string(&last_path).ok().as_deref(), &report) {
        return;
    }
    let token = crate::identity::token(platform).ok().flatten();
    let origin = crate::settings::settings_path(platform)
        .ok()
        .and_then(|p| crate::settings::Settings::load(&p).ok())
        .and_then(|s| s.backend_origin)
        .filter(|o| !o.trim().is_empty())
        .unwrap_or_else(|| crate::identity::DEFAULT_ORIGIN.to_string());
    std::thread::spawn(move || {
        let http = crate::net::UreqHttp::new(&format!("NFL Street Reborn Launcher/{}", crate::build_version()));
        let body = serde_json::to_vec(&report).unwrap_or_default();
        let url = format!("{}/v1/pad-reports", origin.trim_end_matches('/'));
        match crate::net::Http::post_json(&http, &url, &body, token.as_deref()) {
            Ok(response) if (200..300).contains(&response.status) => {
                let _ = std::fs::create_dir_all(&cache);
                let _ = std::fs::write(&last_path, &body);
            }
            Ok(answer) => {
                let status = answer.status;
                tracing::info!("pad report: the server answered {status}");
            }
            Err(e) => tracing::info!("pad report: not sent ({e})"),
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controller::GcInput;
    use crate::devices::{Backend, DetectedDevice};

    fn scan() -> Enumeration {
        Enumeration {
            devices: vec![
                DetectedDevice { backend: Backend::DInput, id: 0, name: "Keyboard Mouse".into(), family: crate::controller::DeviceFamily::Keyboard },
                DetectedDevice { backend: Backend::Sdl, id: 0, name: "PS4 Controller".into(), family: crate::controller::DeviceFamily::PlayStation },
                DetectedDevice { backend: Backend::DInput, id: 0, name: "Wireless Controller".into(), family: crate::controller::DeviceFamily::PlayStation },
            ],
            not_searched: vec![],
        }
    }

    #[test]
    fn the_report_names_each_port_its_source_and_its_dead_controls_and_nothing_else() {
        let mut plan = PadPlan::default();
        plan.assignment = vec![(2, "SDL/0/PS4 Controller".into())];
        plan.kept = vec![(1, "DInput/0/Wireless Controller".into())];
        plan.dead_by_device.insert("DInput/0/Wireless Controller".into(), vec![GcInput::Start]);
        let r = build(&scan(), &plan, true, Some("1.2.13"));
        assert_eq!(r["schema"], SCHEMA);
        assert_eq!(r["runtime_version"], "1.2.13");
        assert_eq!(r["needs_setup"], true);
        assert_eq!(r["devices"].as_array().unwrap().len(), 3);
        assert_eq!(r["ports"][0], serde_json::json!({ "port": 1, "device": "DInput/0/Wireless Controller", "source": "player_setup", "dead": ["Buttons/Start"] }));
        assert_eq!(r["ports"][1], serde_json::json!({ "port": 2, "device": "SDL/0/PS4 Controller", "source": "launcher", "dead": [] }));
        let text = r.to_string();
        assert!(!text.contains(":\\") && !text.contains("/Users/"), "no paths ever leave the launcher: {text}");
    }

    #[test]
    fn the_same_pads_again_send_nothing() {
        let mut plan = PadPlan::default();
        plan.assignment = vec![(1, "SDL/0/PS4 Controller".into())];
        let r = build(&scan(), &plan, true, Some("1.2.13"));
        assert!(is_new(None, &r));
        assert!(!is_new(Some(&r.to_string()), &r));
        assert!(is_new(Some("not json"), &r));
        let other = build(&scan(), &plan, true, Some("1.2.14"));
        assert!(is_new(Some(&r.to_string()), &other), "a new runtime is worth one more report");
    }
}
