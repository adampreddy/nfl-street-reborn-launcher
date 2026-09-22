//! Read-only guided controller diagnostics, saved only through a native picker.
use reborn_core::{capture::Capture, controller_diagnostics, platform::Platform};
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

struct Session {
    token: String,
    capture: Option<Arc<Capture>>,
    detecting: Arc<AtomicBool>,
    context: Value,
}
impl Drop for Session {
    fn drop(&mut self) {
        if let Some(c) = &self.capture {
            c.stop();
        }
    }
}
static SESSION: Mutex<Option<Session>> = Mutex::new(None);
const CLOSED: &str = "The recording has closed. Start a new recording.";

#[tauri::command]
pub async fn controller_report_start(
    token: String,
    runtime: Option<String>,
) -> Result<Value, String> {
    if token.is_empty()
        || token.len() > 64
        || !token
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        return Err(CLOSED.into());
    }
    if crate::session_active() {
        return Err("Close your game before recording controller inputs.".into());
    }
    *SESSION.lock().map_err(|_| CLOSED)? = Some(Session {
        token: token.clone(),
        capture: None,
        detecting: Arc::new(AtomicBool::new(false)),
        context: Value::Null,
    });
    tauri::async_runtime::spawn_blocking(move || {
        let platform = Platform::detect();
        let exe = crate::runtime_for(&platform, runtime.as_deref());
        let mut context = controller_diagnostics::context(&platform, exe.as_deref());
        let mut capture = None;
        let mut status = "runtime_missing";
        if let Some(exe) = exe {
            if let Ok(root) = platform.paths.data_root() {
                let dir = root.join("cache/controller-reports").join(&token);
                match Capture::start(&platform, &exe, &dir) {
                    Ok(c) => {
                        capture = Some(Arc::new(c));
                        status = "ready";
                    }
                    Err(_) => {
                        let _ = std::fs::write(dir.join("quit"), "1\n");
                        status = "unavailable";
                    }
                }
            } else {
                status = "unavailable";
            }
        }
        let devices: Vec<Value> = capture
            .as_ref()
            .map(|c| {
                c.devices()
                    .iter()
                    .filter(|d| {
                        reborn_core::devices::parse_device_list(&d.device)
                            .devices
                            .first()
                            .is_some_and(|d| d.is_controller())
                    })
                    .map(|d| json!({"device":d.device,"inputs":d.inputs,"outputs":d.outputs}))
                    .collect()
            })
            .unwrap_or_default();
        context["runtime_capture_status"] = json!(status);
        context["runtime_devices"] = json!(devices);
        let mut guard = SESSION.lock().map_err(|_| CLOSED)?;
        let Some(session) = guard.as_mut().filter(|s| s.token == token) else {
            if let Some(c) = capture {
                c.stop();
            }
            return Err(CLOSED.into());
        };
        session.capture = capture;
        session.context = context.clone();
        Ok(json!({"context":context,"devices":devices,"status":status}))
    })
    .await
    .map_err(|_| "The controller recorder could not start.".to_string())?
}

#[tauri::command]
pub async fn controller_report_detect(token: String, device: String) -> Result<Value, String> {
    let (capture, busy) = {
        let guard = SESSION.lock().map_err(|_| CLOSED)?;
        let session = guard.as_ref().filter(|s| s.token == token).ok_or(CLOSED)?;
        let Some(c) = &session.capture else {
            return Ok(json!({"status":"unavailable","binding":null}));
        };
        if !c.devices().iter().any(|d| d.device == device)
            || !reborn_core::devices::parse_device_list(&device)
                .devices
                .first()
                .is_some_and(|d| d.is_controller())
        {
            return Err("Choose the controller you want to record.".into());
        }
        if session.detecting.swap(true, Ordering::SeqCst) {
            return Err("Wait for the current input recording to finish.".into());
        }
        (c.clone(), session.detecting.clone())
    };
    tauri::async_runtime::spawn_blocking(move || {
        let result = match capture.detect(&device) {
            Ok(Some(binding)) => json!({"status":"detected","binding":binding}),
            Ok(None) => json!({"status":"no_input","binding":null}),
            Err(_) => json!({"status":"error","binding":null}),
        };
        busy.store(false, Ordering::SeqCst);
        result
    })
    .await
    .map_err(|_| "The controller recorder stopped responding.".to_string())
}

#[tauri::command]
pub fn controller_report_stop(token: String) -> Result<(), String> {
    let mut guard = SESSION.lock().map_err(|_| CLOSED)?;
    if guard.as_ref().is_some_and(|s| s.token == token) {
        *guard = None;
    }
    Ok(())
}

#[tauri::command]
pub async fn controller_report_save(
    token: String,
    recording: Value,
) -> Result<Option<String>, String> {
    let bytes = {
        let mut guard = SESSION.lock().map_err(|_| CLOSED)?;
        let session = guard
            .as_mut()
            .filter(|s| s.token == token && !s.context.is_null())
            .ok_or(CLOSED)?;
        let bytes = controller_diagnostics::bundle(session.context.clone(), recording)
            .map_err(str::to_string)?;
        if let Some(c) = session.capture.take() {
            c.stop();
        }
        bytes
    };
    let Some(file) = rfd::AsyncFileDialog::new()
        .set_title("Save controller problem report")
        .add_filter("Controller report", &["json"])
        .set_file_name("reborn-controller-report.json")
        .save_file()
        .await
    else {
        return Ok(None);
    };
    let path = file.path().to_path_buf();
    tauri::async_runtime::spawn_blocking(move || {
        std::fs::write(&path, bytes).map_err(|_| {
            "The report could not be saved. Choose another folder and try again.".to_string()
        })?;
        Ok(Some(
            path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
        ))
    })
    .await
    .map_err(|_| "The report could not be saved.".to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires REBORN_TEST_RUNTIME and a local runtime build"]
    fn real_report_context_and_capture_lifecycle() {
        let runtime = std::env::var("REBORN_TEST_RUNTIME").expect("set REBORN_TEST_RUNTIME");
        let token = format!("report-test-{}", std::process::id());
        let result =
            tauri::async_runtime::block_on(controller_report_start(token.clone(), Some(runtime)))
                .unwrap();
        assert_eq!(result["status"], "ready");
        assert!(result["context"]["runtime_sha256"]
            .as_str()
            .is_some_and(|h| h.len() == 64));
        assert!(result["devices"]
            .as_array()
            .unwrap()
            .iter()
            .all(|d| !d["device"].as_str().unwrap().contains("Keyboard")));
        let context = &result["context"];
        assert!(context.get("data_root").is_none());
        assert!(context["saved_mappings"]
            .as_array()
            .unwrap()
            .iter()
            .all(|p| p.get("identity").is_none()));
        controller_report_stop("a-stale-dialog".into()).unwrap();
        assert!(SESSION
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|s| s.token == token));
        controller_report_stop(token).unwrap();
        assert!(SESSION.lock().unwrap().is_none());
    }
}
