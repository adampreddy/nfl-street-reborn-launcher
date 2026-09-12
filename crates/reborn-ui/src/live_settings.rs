//! Live, acknowledged changes to the Reborn Dolphin process using this User folder.
//! Dolphin owns its INIs while open; never race its in-memory configuration.
use super::*;
use serde_json::{json, Value};
use reborn_core::controller::{ControllerProfile, GcInput};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

static REQUEST: Mutex<()> = Mutex::new(());

pub fn status(user: &Path) -> Option<Value> {
    let path = user.join("Config/RebornLive.json");
    let modified = path.metadata().ok()?.modified().ok()?;
    if modified.elapsed().ok()? > Duration::from_secs(3) { return None; }
    let value: Value = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
    (value["protocol"] == 1 && value["pid"].as_u64().is_some_and(|id| id > 0) && value["instance"].as_str().is_some_and(|s|!s.is_empty())).then_some(value)
}

pub fn send(p: &Platform, user: &Path, mut command: Value) -> Result<(), String> {
    let _request = REQUEST.lock().map_err(|_| "Restart the launcher to change live settings.")?;
    let current = status(user).ok_or("This Dolphin build does not support live settings. Update Reborn Dolphin, then start your game again.")?;
    let id = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos().to_string();
    command["id"] = id.clone().into();
    command["pid"] = current["pid"].clone();
    command["instance"] = current["instance"].clone();
    let dest = user.join("Config/RebornLive.json.command");
    let temp = dest.with_extension("command.tmp");
    std::fs::write(&temp, serde_json::to_vec(&command).map_err(|_| "Could not prepare the setting.")?).map_err(|e| e.to_string())?;
    p.files.atomic_replace(&temp, &dest).map_err(|e| e.player_message().to_string())?;
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(4) {
        if let Some(reply) = status(user).filter(|r| r["pid"] == current["pid"] && r["instance"] == current["instance"] && r["id"] == id) {
            return match reply["error"].as_str().filter(|s| !s.is_empty()) {
                Some(error) => Err(error.into()), None => Ok(()),
            };
        }
        std::thread::sleep(Duration::from_millis(40));
    }
    // Do not leave a delayed command that could apply after reporting failure.
    let _ = std::fs::remove_file(dest);
    Err("Dolphin did not confirm the change. Bring the game back into view, then try again.".into())
}

// Only consume the launcher's generated pad INI, never arbitrary player files.
fn pads_from_ini(ini: &str) -> Vec<Value> {
    let doc = reborn_core::ini::IniDocument::parse(ini);
    (1..=4).map(|port| {
        let section = format!("GCPad{port}");
        let mut pad = serde_json::Map::new();
        pad.insert("Device".into(), json!(doc.get(&section, "Device").unwrap_or("")));
        for key in GcInput::ALL.iter().map(|input| input.ini_key()).chain([
            "Triggers/L-Analog", "Triggers/R-Analog", "Main Stick/Calibration",
            "C-Stick/Calibration", "Main Stick/Dead Zone", "C-Stick/Dead Zone", "Triggers/Threshold",
        ]) {
            if let Some(value) = doc.get(&section, key) { pad.insert(key.into(), json!(value)); }
        }
        Value::Object(pad)
    }).collect()
}

pub fn controllers(p: &Platform, runtime: Option<&str>, settings: &reborn_core::settings::Settings,
                   profiles: &[ControllerProfile], scan: &reborn_core::devices::Enumeration) -> Result<(), String> {
    let (exe, user) = crate::dolphin_settings::config_user(p, runtime)?;
    if status(&user).is_none() {
        return crate::dolphin_settings::require_closed(&exe);
    }
    let (ini, _) = reborn_core::session::build_gcpad_for_slots(scan, profiles,
        settings.primary_controller.as_deref(), settings.controller_slots.as_ref())
        .ok_or("Connect a controller before applying its mapping.")?;
    send(p, &user, json!({"action":"controllers", "pads":pads_from_ini(&ini)}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn live_pad_payload_preserves_four_ports_and_canonical_face_buttons() {
        let pads = pads_from_ini("[GCPad1]\nDevice = SDL/0/Xbox\nButtons/B = `Button B`\nButtons/X = `Button X`\n[GCPad4]\nDevice = SDL/1/PS5\nButtons/A = `Button A`\n");
        assert_eq!(pads.len(), 4);
        assert_eq!(pads[0]["Buttons/B"], "`Button B`");
        assert_eq!(pads[0]["Buttons/X"], "`Button X`");
        assert_eq!(pads[1]["Device"], "");
        assert_eq!(pads[3]["Device"], "SDL/1/PS5");
    }
    #[test]
    fn a_missing_heartbeat_does_not_offer_live_edits() {
        assert!(status(&std::env::temp_dir().join("reborn-missing-live-user")).is_none());
    }
}
