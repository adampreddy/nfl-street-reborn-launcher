//! A deliberately narrow controller report. No logs, credentials, save files,
//! profile serial numbers, or general settings are exported.
use crate::{controller::GcInput, ini::IniDocument, platform::Platform};
use serde_json::{json, Value};
use std::path::Path;

pub const SCHEMA: &str = "nsr.controller-report.v1";

pub fn pad_config(text: &str) -> Value {
    let doc = IniDocument::parse(text);
    let mut keys = vec![
        "Device",
        "Triggers/L-Analog",
        "Triggers/R-Analog",
        "Rumble/Motor",
        "Main Stick/Dead Zone",
        "C-Stick/Dead Zone",
        "Main Stick/Calibration",
        "C-Stick/Calibration",
        "Main Stick/Radius",
        "C-Stick/Radius",
        "Triggers/Threshold",
    ];
    keys.extend(GcInput::ALL.iter().map(|i| i.ini_key()));
    Value::Array(
        (1..=4)
            .map(|port| {
                let section = format!("GCPad{port}");
                let pairs: serde_json::Map<String, Value> = keys
                    .iter()
                    .filter_map(|k| doc.get(&section, k).map(|v| (k.to_string(), json!(v))))
                    .collect();
                json!({"port":port,"values":pairs})
            })
            .collect(),
    )
}

pub fn context(platform: &Platform, runtime: Option<&Path>) -> Value {
    let mut unavailable = Vec::new();
    let profiles = crate::controller_profiles::path(platform)
        .ok()
        .and_then(|p| crate::controller_profiles::load(&p).ok());
    if profiles.is_none() {
        unavailable.push("saved_mappings");
    }
    let profiles:Vec<_>=profiles.unwrap_or_default().iter().map(|p|json!({
        "name":p.identity.name,"source_key":p.identity.guid,"family":p.identity.family,"port":p.port,
        "bindings":GcInput::ALL.iter().map(|i|(i.ini_key(),p.mapping.get(*i))).collect::<std::collections::BTreeMap<_,_>>(),
        "tuning":p.tuning,"extras":p.extras
    })).collect();
    let settings = crate::settings::settings_path(platform)
        .ok()
        .and_then(|p| crate::settings::Settings::load(&p).ok());
    if settings.is_none() {
        unavailable.push("player_assignments");
    }
    let chosen_user = settings.as_ref().and_then(|s| s.user_dir.as_deref());
    let ini = crate::session::resolve_user_dir_for(platform, chosen_user, runtime)
        .ok()
        .and_then(|u| std::fs::read_to_string(u.path().join("Config/GCPadNew.ini")).ok());
    if ini.is_none() {
        unavailable.push("game_pad_config");
    }
    let hash = runtime.and_then(|p| crate::game_import::hash(p).ok());
    if hash.is_none() {
        unavailable.push("runtime_fingerprint");
    }
    json!({"launcher_version":crate::build_version(),"os":std::env::consts::OS,
        "architecture":std::env::consts::ARCH,"runtime_sha256":hash,
        "saved_mappings":profiles,"player_assignments":settings.and_then(|s|s.controller_slots),
        "game_pad_config":ini.as_deref().map(pad_config),"unavailable":unavailable})
}

// Bound the report before showing a save dialog. The frontend supplies only
// this allowlisted recording shape; backend context is supplied separately.
pub fn validate_recording(recording: &Value) -> Result<(), &'static str> {
    let invalid = "The controller recording is incomplete or too large. Record it again.";
    let Some(obj) = recording.as_object() else {
        return Err(invalid);
    };
    if obj.keys().any(|k| {
        !matches!(
            k.as_str(),
            "location"
                | "connection"
                | "translation"
                | "description"
                | "runtime_device"
                | "devices"
                | "steps"
        )
    }) {
        return Err(invalid);
    }
    for key in ["location", "connection", "translation", "description"] {
        if recording[key]
            .as_str()
            .is_none_or(|s| s.chars().count() > 2000)
        {
            return Err(invalid);
        }
    }
    if !recording["runtime_device"].is_null()
        && recording["runtime_device"]
            .as_str()
            .is_none_or(|s| s.len() > 512)
    {
        return Err(invalid);
    }
    let Some(devices) = recording["devices"].as_array().filter(|a| a.len() <= 32) else {
        return Err(invalid);
    };
    for d in devices {
        if d["id"].as_str().is_none_or(|s| s.chars().count() > 512)
            || d["mapping"].as_str().is_none_or(|s| s.len() > 32)
            || d["axes"].as_u64().is_none_or(|n| n > 64)
            || d["buttons"].as_u64().is_none_or(|n| n > 128)
            || d.as_object().is_none_or(|o| {
                o.keys().any(|k| {
                    !matches!(
                        k.as_str(),
                        "id" | "mapping" | "axes" | "buttons" | "menu_layout"
                    )
                })
            })
        {
            return Err(invalid);
        }
        if !d["menu_layout"].is_null() {
            let layout = &d["menu_layout"];
            if layout.as_object().is_none_or(|o| o.len() != 1) {
                return Err(invalid);
            }
            let Some(bindings) = layout["bindings"].as_object().filter(|o| o.len() <= 12) else {
                return Err(invalid);
            };
            for (action, b) in bindings {
                if !matches!(
                    action.as_str(),
                    "up" | "down"
                        | "left"
                        | "right"
                        | "stickUp"
                        | "stickDown"
                        | "stickLeft"
                        | "stickRight"
                        | "accept"
                        | "back"
                        | "previous"
                        | "next"
                ) {
                    return Err(invalid);
                }
                let kind = b["kind"].as_str().unwrap_or("");
                if !matches!(kind, "button" | "axis" | "hat")
                    || b["index"].as_u64().is_none_or(|i| {
                        i >= d[if kind == "button" { "buttons" } else { "axes" }]
                            .as_u64()
                            .unwrap()
                    })
                    || b.as_object().is_none_or(|o| {
                        o.keys()
                            .any(|k| !matches!(k.as_str(), "kind" | "index" | "rest" | "value"))
                    })
                {
                    return Err(invalid);
                }
                if kind != "button"
                    && (b["rest"]
                        .as_f64()
                        .is_none_or(|v| !v.is_finite() || v.abs() > 16.0)
                        || b["value"]
                            .as_f64()
                            .is_none_or(|v| !v.is_finite() || v.abs() > 16.0)
                        || b["value"] == b["rest"])
                {
                    return Err(invalid);
                }
            }
        }
    }
    let Some(steps) = recording["steps"]
        .as_array()
        .filter(|a| !a.is_empty() && a.len() <= 12)
    else {
        return Err(invalid);
    };
    for s in steps {
        if s["name"].as_str().is_none_or(|n| {
            !matches!(
                n,
                "rest"
                    | "dpad_up"
                    | "dpad_down"
                    | "dpad_left"
                    | "dpad_right"
                    | "stick_up"
                    | "stick_down"
                    | "stick_left"
                    | "stick_right"
                    | "accept"
                    | "back"
            )
        }) || s["status"]
            .as_str()
            .is_none_or(|n| !matches!(n, "recorded" | "skipped" | "interrupted"))
            || s["runtime_status"].as_str().is_none_or(|n| {
                !matches!(
                    n,
                    "detected" | "no_input" | "unavailable" | "not_requested" | "error"
                )
            })
            || (!s["binding"].is_null() && s["binding"].as_str().is_none_or(|b| b.len() > 2048))
            || s.as_object().is_none_or(|o| {
                o.keys().any(|k| {
                    !matches!(
                        k.as_str(),
                        "name" | "status" | "runtime_status" | "binding" | "samples"
                    )
                })
            })
        {
            return Err(invalid);
        }
        let Some(samples) = s["samples"].as_array().filter(|a| a.len() <= 160) else {
            return Err(invalid);
        };
        let mut previous = 0.0;
        for sample in samples {
            let Some(t) = sample["t"]
                .as_f64()
                .filter(|t| t.is_finite() && *t >= previous && *t <= 10000.0)
            else {
                return Err(invalid);
            };
            previous = t;
            if !sample["focused"].is_boolean()
                || sample.as_object().is_none_or(|o| {
                    o.keys()
                        .any(|k| !matches!(k.as_str(), "t" | "focused" | "pads"))
                })
            {
                return Err(invalid);
            }
            let Some(pads) = sample["pads"].as_array().filter(|a| a.len() <= 8) else {
                return Err(invalid);
            };
            for p in pads {
                let Some(d) = p["device"].as_u64().and_then(|i| devices.get(i as usize)) else {
                    return Err(invalid);
                };
                if p["index"].as_u64().is_none_or(|i| i > 255)
                    || p.as_object().is_none_or(|o| {
                        o.keys()
                            .any(|k| !matches!(k.as_str(), "device" | "index" | "axes" | "buttons"))
                    })
                {
                    return Err(invalid);
                }
                let Some(axes) = p["axes"]
                    .as_array()
                    .filter(|a| a.len() == d["axes"].as_u64().unwrap() as usize)
                else {
                    return Err(invalid);
                };
                if axes
                    .iter()
                    .any(|a| a.as_f64().is_none_or(|v| !v.is_finite() || v.abs() > 16.0))
                {
                    return Err(invalid);
                }
                let Some(buttons) = p["buttons"]
                    .as_array()
                    .filter(|a| a.len() == d["buttons"].as_u64().unwrap() as usize)
                else {
                    return Err(invalid);
                };
                if buttons.iter().any(|b| {
                    b.as_array().is_none_or(|a| a.len() != 2)
                        || b[0].as_f64().is_none_or(|v| !(0.0..=1.0).contains(&v))
                        || !b[1].is_boolean()
                }) {
                    return Err(invalid);
                }
            }
        }
    }
    if serde_json::to_vec(recording).map_err(|_| invalid)?.len() > 8 * 1024 * 1024 {
        return Err(invalid);
    }
    Ok(())
}

pub fn bundle(context: Value, recording: Value) -> Result<Vec<u8>, &'static str> {
    validate_recording(&recording)?;
    serde_json::to_vec_pretty(
        &json!({"schema":SCHEMA,"context":context,"recording":recording,
        "scope":"Guided launcher recording; does not record in-game state or keyboard input."}),
    )
    .map_err(|_| "The controller report could not be created.")
}

#[cfg(test)]
mod tests {
    use super::*;
    fn recording() -> Value {
        json!({"location":"both","connection":"usb","translation":"unknown","description":"Down moves up","runtime_device":null,
        "devices":[{"id":"USB Pad","mapping":"","axes":1,"buttons":1}],
        "steps":[{"name":"dpad_up","status":"recorded","runtime_status":"unavailable","binding":null,
            "samples":[{"t":0,"focused":true,"pads":[{"device":0,"index":0,"axes":[3.0],"buttons":[[0.0,false]]}]}]}]})
    }
    #[test]
    fn raw_hat_neutral_and_unknown_runtime_can_be_exported() {
        assert!(bundle(json!({}), recording()).is_ok());
    }
    #[test]
    fn description_limit_counts_characters_not_utf8_bytes() {
        let mut r = recording();
        r["description"] = json!("方向".repeat(1000));
        assert!(validate_recording(&r).is_ok());
        r["description"] = json!("方向".repeat(1001));
        assert!(validate_recording(&r).is_err());
    }
    #[test]
    fn config_export_excludes_unrelated_paths_comments_and_sections() {
        let v=pad_config("[GCPad1]\nDevice = SDL/0/Pad\nMain Stick/Up = `Axis 1-`\n# private note\nProfile = C:/Users/Private/profile.ini\n[General]\nToken = secret\n");
        let text = v.to_string();
        assert!(text.contains("Axis 1-"));
        assert!(!text.contains("Private"));
        assert!(!text.contains("secret"));
        assert!(!text.contains("private note"));
    }
    #[test]
    fn invalid_samples_and_extra_payload_are_rejected() {
        let mut r = recording();
        r["steps"][0]["samples"][0]["pads"][0]["device"] = json!(9);
        assert!(validate_recording(&r).is_err());
        let mut r = recording();
        r["files"] = json!(["private.txt"]);
        assert!(validate_recording(&r).is_err());
        let mut r = recording();
        r["steps"][0]["samples"] = json!(vec![r["steps"][0]["samples"][0].clone(); 161]);
        assert!(validate_recording(&r).is_err());
    }
    #[test]
    fn captured_raw_layout_round_trips_but_unknown_metadata_is_rejected() {
        let mut r = recording();
        r["devices"][0]["menu_layout"] =
            json!({"bindings":{"up":{"kind":"hat","index":0,"rest":3.0,"value":-1.0}}});
        assert!(bundle(json!({"launcher_version":"test"}), r.clone()).is_ok());
        r["devices"][0]["menu_layout"]["secret"] = json!("not a mapping");
        assert!(validate_recording(&r).is_err());
    }
    #[test]
    fn exported_browser_fixture_is_accepted_when_provided() {
        let Some(path) = std::env::var_os("REBORN_TEST_CONTROLLER_REPORT") else {
            return;
        };
        let report: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        validate_recording(&report["recording"]).unwrap();
    }
}
