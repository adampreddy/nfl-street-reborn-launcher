//! Finding controllers, and naming them the way the emulator names them.
//!
//! # The only thing that matters here
//!
//! `GCPadNew.ini` binds a pad with a line like:
//!
//! ```text
//! Device = XInput/0/Gamepad
//! ```
//!
//! If that string does not match what the emulator's own controller interface
//! calls the device, the pad silently does nothing. There is no error, no
//! warning, and no clue: the player just has no controller. So this module does
//! not invent a naming scheme. It reproduces the emulator's.
//!
//! # What the emulator actually does
//!
//! Read out of the Reborn fork's `InputCommon/ControllerInterface`, not guessed:
//!
//! - The string is `source/id/name`, from `DeviceQualifier::ToString`.
//! - XInput devices report source `XInput` and a name from the device subtype,
//!   which is `Gamepad` for an ordinary pad.
//! - **XInput devices get a preferred id, and it is the XInput user index.**
//!   `Device::GetPreferredId` returns `m_index`, so slot 2 is `XInput/2/Gamepad`
//!   and not the second device found.
//! - Everything else falls back to the lowest id not already used by a device
//!   with the same source and name.
//!
//! # Why an Xbox pad is not an SDL device on Windows
//!
//! It would be reasonable to assume every pad shows up through SDL. On Windows
//! it does not. The SDL backend deliberately drops Xbox 360 class devices:
//!
//! ```text
//! if ((std::string::npos != lcasename.find("xbox 360")) &&
//!     (11 == SDL_JoystickNumButtons(joystick)) && (6 == SDL_JoystickNumAxes(joystick)) &&
//!     (1 == SDL_JoystickNumHats(joystick)) && (0 == SDL_JoystickNumBalls(joystick)))
//!   return;  // this device won't be used
//! ```
//!
//! So on Windows an Xbox pad is `XInput/0/Gamepad`. Writing
//! `SDL/0/Xbox Wireless Controller` produces a file that looks right, parses
//! fine, and binds nothing.

use crate::controller::{DeviceFamily, DeviceIdentity};

/// Which of the emulator's controller backends a device belongs to.
///
/// The string values are the emulator's `GetSource()` return values and are not
/// ours to choose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Backend {
    XInput,
    Sdl,
}

impl Backend {
    /// The `source` field of the device string.
    pub fn source(self) -> &'static str {
        match self {
            Backend::XInput => "XInput",
            Backend::Sdl => "SDL",
        }
    }
}

/// A controller the emulator would find, named the way it would name it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectedDevice {
    pub backend: Backend,
    /// The `id` field. For XInput this is the user index, not a discovery order.
    pub id: u8,
    /// The `name` field, exactly as the emulator reports it.
    pub name: String,
    /// What the launcher shows the player, which is not the same thing.
    pub family: DeviceFamily,
}

impl DetectedDevice {
    /// The `Device =` line for `GCPadNew.ini`.
    pub fn device_string(&self) -> String {
        format!("{}/{}/{}", self.backend.source(), self.id, self.name)
    }

    /// A stable key for remembering this player's settings.
    ///
    /// XInput exposes no serial or GUID, so the slot is all there is. That is
    /// worth saying out loud: unplug two Xbox pads and plug them back in the
    /// other way round, and their profiles follow the slots rather than the
    /// hardware. SDL devices carry a GUID and do not have this problem.
    pub fn identity(&self) -> DeviceIdentity {
        DeviceIdentity {
            guid: format!("{}:{}", self.backend.source(), self.id),
            serial: None,
            name: self.name.clone(),
            family: self.family,
        }
    }
}

/// The result of looking for controllers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Enumeration {
    pub devices: Vec<DetectedDevice>,
    /// Backends that were not searched, and why.
    ///
    /// Present so the caller can say "no controller found" honestly rather than
    /// implying nothing is plugged in when half the sources were never checked.
    pub not_searched: Vec<(Backend, &'static str)>,
}

impl Enumeration {
    /// Whether any backend was skipped, which makes an empty result inconclusive.
    pub fn is_complete(&self) -> bool {
        self.not_searched.is_empty()
    }
}

/// Finds controllers the way the emulator would.
#[derive(Debug)]
pub struct DeviceScanner;

impl DeviceScanner {
    /// Look for connected controllers.
    pub fn scan() -> Enumeration {
        let mut devices = Vec::new();
        let mut not_searched = Vec::new();

        match scan_xinput() {
            Ok(found) => devices.extend(found),
            Err(reason) => not_searched.push((Backend::XInput, reason)),
        }

        // SDL is where every non-Xbox pad lives on Windows, including
        // DualSense and DualShock. Enumerating it means matching SDL's own
        // ordering and its whitespace stripping, and getting that subtly wrong
        // produces a file that binds the wrong pad rather than no pad, which is
        // worse. Not written yet, and reported as not searched rather than
        // quietly treated as empty.
        not_searched.push((
            Backend::Sdl,
            "SDL enumeration is not implemented, so non-XInput pads such as \
             DualSense and DualShock are not listed yet",
        ));

        devices.sort_by_key(|d| (d.backend, d.id));
        Enumeration {
            devices,
            not_searched,
        }
    }
}

/// The emulator's XInput backend, reproduced.
///
/// It calls `XInputGetCapabilities` for user indices 0 through 3 and adds a
/// device for each success, naming it from the subtype. This does the same.
#[cfg(windows)]
fn scan_xinput() -> Result<Vec<DetectedDevice>, &'static str> {
    let handle = rusty_xinput::XInputHandle::load_default()
        .map_err(|_| "the XInput library could not be loaded")?;

    let mut found = Vec::new();
    for index in 0u32..4 {
        // get_state rather than get_capabilities: the emulator uses
        // capabilities, but rusty-xinput's capabilities call is not available
        // in every build of the library it loads, and a connected pad answers
        // both. A pad that answers neither would not work in the emulator
        // either.
        if handle.get_state(index).is_ok() {
            found.push(DetectedDevice {
                backend: Backend::XInput,
                // The emulator's preferred id IS the user index.
                id: index as u8,
                // XINPUT_DEVSUBTYPE_GAMEPAD maps to "Gamepad". Other subtypes
                // exist (Wheel, Guitar, Drum Kit) but a football game is only
                // ever bound to a pad, and claiming a subtype we did not read
                // would be a guess.
                name: "Gamepad".to_string(),
                family: DeviceFamily::Xbox,
            });
        }
    }
    Ok(found)
}

#[cfg(not(windows))]
fn scan_xinput() -> Result<Vec<DetectedDevice>, &'static str> {
    Err("XInput is a Windows API and does not exist on this platform")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The format is the emulator's, from `DeviceQualifier::ToString`:
    /// `source << '/' << cid << '/' << name`.
    #[test]
    fn the_device_string_matches_the_emulator_format() {
        let d = DetectedDevice {
            backend: Backend::XInput,
            id: 0,
            name: "Gamepad".into(),
            family: DeviceFamily::Xbox,
        };
        assert_eq!(d.device_string(), "XInput/0/Gamepad");
    }

    /// The id is the XInput user index, not a discovery counter. A pad in slot
    /// 2 with nothing in slots 0 and 1 is `XInput/2/Gamepad`. Getting this
    /// wrong binds nothing and says nothing.
    #[test]
    fn an_xinput_id_is_the_slot_not_the_discovery_order() {
        let d = DetectedDevice {
            backend: Backend::XInput,
            id: 2,
            name: "Gamepad".into(),
            family: DeviceFamily::Xbox,
        };
        assert_eq!(d.device_string(), "XInput/2/Gamepad");
    }

    /// An Xbox pad on Windows is not an SDL device, because the SDL backend
    /// explicitly drops Xbox 360 class hardware. This is the bug the module
    /// exists to prevent.
    #[test]
    fn an_xbox_pad_is_not_named_as_an_sdl_device() {
        let scan = DeviceScanner::scan();
        for d in &scan.devices {
            if d.family == DeviceFamily::Xbox {
                assert_eq!(
                    d.backend,
                    Backend::XInput,
                    "an Xbox pad named {} would bind nothing in the emulator",
                    d.device_string()
                );
            }
        }
    }

    /// An empty list must not be reported as "nothing is plugged in" while a
    /// whole backend went unsearched.
    #[test]
    fn an_unsearched_backend_makes_the_result_inconclusive() {
        let scan = DeviceScanner::scan();
        assert!(
            !scan.is_complete(),
            "SDL is not implemented yet, so no scan can honestly claim to be complete"
        );
        assert!(
            scan.not_searched.iter().any(|(b, _)| *b == Backend::Sdl),
            "the SDL gap must be reported, not hidden"
        );
        for (_, reason) in &scan.not_searched {
            assert!(!reason.is_empty(), "a skipped backend must say why");
        }
    }

    /// Two XInput pads must not collide on the key their settings are stored
    /// under, or player two's rebinds land on player one.
    #[test]
    fn two_xinput_pads_get_distinct_identities() {
        let a = DetectedDevice {
            backend: Backend::XInput,
            id: 0,
            name: "Gamepad".into(),
            family: DeviceFamily::Xbox,
        };
        let b = DetectedDevice {
            backend: Backend::XInput,
            id: 1,
            name: "Gamepad".into(),
            family: DeviceFamily::Xbox,
        };
        assert_ne!(a.identity().profile_key(), b.identity().profile_key());
    }

    /// Scanning must never panic or hang, whatever is or is not plugged in.
    #[test]
    fn scanning_is_safe_with_no_controllers_attached() {
        let scan = DeviceScanner::scan();
        for d in &scan.devices {
            assert!(d.id < 4, "XInput only has four slots, got {}", d.id);
            assert!(!d.name.is_empty());
        }
    }
}
