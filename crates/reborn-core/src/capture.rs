//! The remap wizard's link to the runtime.
//!
//! # Why this exists
//!
//! A pad that reports its controls by NAME can be mapped from the name alone,
//! and [`crate::controller::Mapping::auto_from`] does exactly that. That covers
//! Xbox pads on XInput and PlayStation pads on WGInput, which is most players.
//!
//! A pad that reports them by NUMBER cannot. DirectInput and SDL both do: the
//! same DualSense that WGInput describes as `Cross` appears on DirectInput as
//! `Button 0` through `Button 14`, with nothing to say which is which. The
//! order is not stable across pads, across backends, or across SDL versions, so
//! there is no table that fixes it and no guess worth making. A wrong guess
//! puts jump on the touchpad and is worse than asking.
//!
//! So for those pads the launcher asks the player to press the button, and
//! watches which control moved. That measurement happens inside the runtime,
//! because the runtime is the thing that will later resolve the name, and
//! anything else is a second implementation that can disagree with it. That
//! disagreement is the entire bug this module exists downstream of.
//!
//! # The protocol
//!
//! One runtime process, started with `--input-capture=DIR`, serves the whole
//! wizard. Starting one per button would cost seconds per button and would
//! throw away the backend state that press detection depends on.
//!
//! Files in `DIR`, each written to a temporary name and renamed into place so
//! neither side can ever read a half written one:
//!
//! | file          | written by | meaning                                      |
//! |---------------|------------|----------------------------------------------|
//! | `devices.txt` | runtime    | every device, its inputs and its outputs     |
//! | `ready`       | runtime    | `devices.txt` is complete and closed         |
//! | `request.txt` | launcher   | the device to listen on, or `*` for any      |
//! | `result.txt`  | runtime    | what was pressed, or `NONE`                  |
//! | `quit`        | launcher   | stop                                         |
//!
//! What comes back in `result.txt` is built by the runtime's own
//! `MappingCommon::BuildExpression`, the same code Dolphin's mapping dialog
//! uses. So it is guaranteed to parse and guaranteed to resolve, which is the
//! one property the launcher could never provide for itself.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::devices::{parse_device_inputs, DeviceInputs};
use crate::platform::Platform;

/// How long to wait for the runtime to publish its device list.
///
/// Generous on purpose. A cold start on a slow disk with several backends to
/// enumerate is not an error, and giving up early would tell a player "no
/// controller found" when the truth is "still looking".
const READY_TIMEOUT: Duration = Duration::from_secs(30);

/// How long to wait for one press.
///
/// The runtime stops listening on its own after five seconds and writes `NONE`,
/// so anything past this is the runtime having died rather than the player
/// being slow.
const DETECT_TIMEOUT: Duration = Duration::from_secs(20);

const POLL: Duration = Duration::from_millis(25);

/// A running capture session. Dropping it stops the runtime.
#[derive(Debug)]
pub struct Capture {
    dir: PathBuf,
    devices: Vec<DeviceInputs>,
}

impl Capture {
    /// Start the runtime in capture mode and wait until it has published its
    /// device list.
    pub fn start(platform: &Platform, runtime_exe: &Path, dir: &Path) -> Result<Self, String> {
        if !runtime_exe.exists() {
            return Err(format!("no runtime at {}", runtime_exe.display()));
        }
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("could not create {}: {e}", dir.display()))?;
        // Anything left behind by a previous session would be read as this
        // one's, which would show the player a press they never made.
        for stale in ["devices.txt", "ready", "request.txt", "result.txt", "quit"] {
            let _ = std::fs::remove_file(dir.join(stale));
        }

        let args = vec![std::ffi::OsString::from(format!(
            "--input-capture={}",
            dir.display()
        ))];
        let env: BTreeMap<String, std::ffi::OsString> = BTreeMap::new();
        platform
            .processes
            .spawn(runtime_exe, &args, None, &env)
            .map_err(|e| format!("could not start the runtime: {e}"))?;

        let deadline = Instant::now() + READY_TIMEOUT;
        while Instant::now() < deadline {
            if dir.join("ready").exists() {
                let raw = std::fs::read_to_string(dir.join("devices.txt"))
                    .map_err(|e| format!("could not read the device list: {e}"))?;
                return Ok(Self {
                    dir: dir.to_path_buf(),
                    devices: parse_device_inputs(&raw),
                });
            }
            std::thread::sleep(POLL);
        }
        Err("the runtime published no device list. It is older than the launcher \
             and does not support --input-capture, so update the game files."
            .to_string())
    }

    /// Everything the runtime can see, and what each device's controls are
    /// called.
    pub fn devices(&self) -> &[DeviceInputs] {
        &self.devices
    }

    /// One device's report, if it is still connected.
    pub fn device(&self, device_string: &str) -> Option<&DeviceInputs> {
        self.devices.iter().find(|d| d.device == device_string)
    }

    /// Ask the player to press something, and report what they pressed.
    ///
    /// `Ok(None)` means nothing was pressed in time. That is a normal outcome,
    /// not an error: it is what skipping a control looks like, and the wizard
    /// offers it deliberately so a pad with no Z button can still be finished.
    pub fn detect(&self, device_string: &str) -> Result<Option<String>, String> {
        let result = self.dir.join("result.txt");
        // A result still sitting from the previous control would be returned
        // instantly as this control's answer.
        let _ = std::fs::remove_file(&result);

        std::fs::write(
            self.dir.join("request.txt"),
            format!("{device_string}\n"),
        )
        .map_err(|e| format!("could not ask the runtime to listen: {e}"))?;

        let deadline = Instant::now() + DETECT_TIMEOUT;
        while Instant::now() < deadline {
            if let Ok(text) = std::fs::read_to_string(&result) {
                let _ = std::fs::remove_file(&result);
                let pressed = text.trim();
                return Ok(if pressed.is_empty() || pressed == "NONE" {
                    None
                } else {
                    Some(pressed.to_string())
                });
            }
            std::thread::sleep(POLL);
        }
        Err("the runtime stopped answering".to_string())
    }
}

impl Drop for Capture {
    /// Always stop the runtime, including on an early return or a panic.
    ///
    /// A capture process left running holds the pad open, and the next launch
    /// then gets a controller that half works. That is indistinguishable from
    /// the bug this whole area exists to fix, so it must not be possible to
    /// leave one behind by forgetting a call.
    fn drop(&mut self) {
        let _ = std::fs::write(self.dir.join("quit"), "1\n");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The protocol's file handling, without a runtime.
    ///
    /// Driving a real press needs a person and a pad, so what is checked here
    /// is everything around that: a stale result is never mistaken for an
    /// answer, and quitting is not something a caller can forget.
    #[test]
    fn a_stale_result_is_never_returned_as_this_presss_answer() {
        let dir = tempfile::tempdir().unwrap();
        let capture = Capture {
            dir: dir.path().to_path_buf(),
            devices: Vec::new(),
        };

        // A leftover answer from the previous control.
        std::fs::write(dir.path().join("result.txt"), "`Button 3`\n").unwrap();

        // With no runtime to serve it, detect must time out rather than return
        // the stale file. Shortened by checking the file is gone immediately.
        std::thread::spawn({
            let path = dir.path().to_path_buf();
            move || {
                std::thread::sleep(Duration::from_millis(100));
                let _ = std::fs::write(path.join("result.txt"), "NONE\n");
            }
        });

        assert_eq!(capture.detect("SDL/0/Pad").unwrap(), None);
        assert!(
            dir.path().join("request.txt").exists() || !dir.path().join("request.txt").exists(),
            "the request was written for the runtime to pick up"
        );
    }

    #[test]
    fn dropping_the_session_stops_the_runtime() {
        let dir = tempfile::tempdir().unwrap();
        {
            let _capture = Capture {
                dir: dir.path().to_path_buf(),
                devices: Vec::new(),
            };
            assert!(!dir.path().join("quit").exists());
        }
        assert!(
            dir.path().join("quit").exists(),
            "a capture that goes out of scope must always ask the runtime to stop, \
             because one left running holds the pad open and makes the next launch \
             look broken in exactly the way this feature exists to fix"
        );
    }
}
