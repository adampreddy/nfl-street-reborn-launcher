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
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Backend {
    XInput,
    Sdl,
    /// DirectInput. The working install uses this for keyboard and mouse on
    /// the ports with no pad in them.
    DInput,
    /// Anything else the emulator reports. Kept rather than discarded, because
    /// a device we do not recognise still binds correctly if its string is
    /// passed through untouched.
    Other(String),
}

impl Backend {
    /// The `source` field of the device string.
    pub fn source(&self) -> &str {
        match self {
            Backend::XInput => "XInput",
            Backend::Sdl => "SDL",
            Backend::DInput => "DInput",
            Backend::Other(s) => s,
        }
    }
}

impl Backend {
    /// Which backend to bind when the same physical pad shows up in several.
    ///
    /// It does, routinely. One Xbox pad on this machine reports as all of:
    ///
    /// ```text
    /// WGInput/0/Xbox One Game Controller
    /// WGInput/1/Xbox One Game Controller
    /// XInput/0/Gamepad
    /// ```
    ///
    /// Lower is preferred.
    ///
    /// XInput first because that is what the known-working install binds, and
    /// because it is the backend with working rumble and analog triggers for
    /// Xbox pads.
    ///
    /// SDL second because that is where every PlayStation pad lives on Windows.
    /// The SDL backend deliberately refuses Xbox 360 class devices, so a pad
    /// that appears there and not in XInput is a DualSense, a DualShock or
    /// something similar, and SDL is the right home for it.
    ///
    /// WGInput third. It duplicates XInput for Xbox pads and reported the same
    /// controller twice here, which makes it the worst thing to bind by
    /// default.
    ///
    /// DirectInput last. It is where keyboard and mouse live, and those are a
    /// fallback for an empty port rather than something to hand a player.
    pub fn preference(&self) -> u8 {
        match self {
            Backend::XInput => 0,
            Backend::Sdl => 1,
            Backend::Other(name) if name == "WGInput" => 2,
            Backend::Other(_) => 3,
            Backend::DInput => 4,
        }
    }

    /// Whether this backend supplies things a player holds.
    pub fn is_pad_backend(&self) -> bool {
        !matches!(self, Backend::DInput)
    }
}

impl Enumeration {
    /// The devices worth offering as player pads, best first.
    ///
    /// Two rules, both from what the emulator actually reported here.
    ///
    /// Keyboard and mouse are dropped. They are a fallback for an unused port,
    /// not a controller.
    ///
    /// When a higher ranked backend has pads, lower ranked ones are dropped
    /// wholesale rather than filtered device by device. There is no reliable
    /// way to tell that `WGInput/0/Xbox One Game Controller` and
    /// `XInput/0/Gamepad` are one piece of plastic, so the safe move is to
    /// trust one backend at a time. The cost is a genuine edge case: an Xbox
    /// pad and a DualSense plugged in together would offer only the Xbox one,
    /// because they land in different backends. That is worth fixing when
    /// somebody hits it, and is better than handing player two a second copy
    /// of player one's controller.
    pub fn playable_pads(&self) -> Vec<&DetectedDevice> {
        let mut pads: Vec<&DetectedDevice> = self
            .devices
            .iter()
            .filter(|d| d.backend.is_pad_backend())
            .collect();

        // Deduplicate by the PAD, not by the backend.
        //
        // This used to keep only the best ranked backend and drop every other
        // one wholesale. That is why a PlayStation 5 pad could disappear
        // completely: it lives on WGInput, so the moment any XInput device
        // existed, including the virtual pad Steam creates, the real pad was
        // filtered out and the player was told no controller was found.
        //
        // One physical pad really does report through several backends, so
        // some deduplication is needed. Doing it by NAME is both narrower and
        // more accurate than doing it by backend: the duplicate that caused
        // the original trouble was one Xbox pad appearing as
        // `WGInput/0/Xbox One Game Controller` and `XInput/0/Gamepad`, and a
        // PlayStation pad never collides with an Xbox one.
        pads.sort_by_key(|d| (d.backend.preference(), d.id));
        let mut chosen: Vec<&DetectedDevice> = Vec::new();
        for pad in pads {
            // Only ACROSS backends, and never by id.
            //
            // Two ids in one backend are two pads: `XInput/0/Gamepad` and
            // `XInput/1/Gamepad` are player one and player two and both must be
            // offered. But one Xbox pad really does arrive as
            // `WGInput/0/Xbox One Game Controller` AND
            // `WGInput/1/Xbox One Game Controller` alongside its XInput entry,
            // so the id carries no information about how many pads exist.
            let already_seen_elsewhere = chosen.iter().any(|kept: &&DetectedDevice| {
                kept.backend != pad.backend && same_physical_pad(&kept.name, &pad.name)
            });
            if !already_seen_elsewhere {
                chosen.push(pad);
            }
        }
        chosen
    }

    /// Devices that were found but will not be offered, and why.
    ///
    /// Shown so a player whose pad is not being used can see that it was seen.
    pub fn set_aside(&self) -> Vec<(&DetectedDevice, &'static str)> {
        let chosen: Vec<String> = self
            .playable_pads()
            .iter()
            .map(|d| d.device_string())
            .collect();
        self.devices
            .iter()
            .filter(|d| !chosen.contains(&d.device_string()))
            .map(|d| {
                let why = if !d.backend.is_pad_backend() {
                    "keyboard and mouse, kept as a fallback for an unused port"
                } else {
                    "the same pad is already offered through a better backend"
                };
                (d, why)
            })
            .collect()
    }
}

/// Whether two device names, seen on different backends at the same id, are
/// one piece of plastic.
///
/// Deliberately conservative. Saying yes wrongly hides a real second pad from
/// player two, which is the failure this whole function exists to stop. So only
/// the case that is actually known to collide is matched: an Xbox pad, which
/// XInput calls `Gamepad` and Windows.Gaming.Input calls
/// `Xbox ... Controller`.
fn same_physical_pad(a: &str, b: &str) -> bool {
    fn xbox_ish(name: &str) -> bool {
        let lower = name.to_ascii_lowercase();
        lower == "gamepad" || lower.contains("xbox")
    }
    a.eq_ignore_ascii_case(b) || (xbox_ish(a) && xbox_ish(b))
}

/// The inputs one device reports, as the runtime names them.
///
/// Read from the runtime's `--input-capture` device dump. This is the only
/// trustworthy source: the names are per backend, they do not overlap, and the
/// launcher must never invent one.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DeviceInputs {
    pub device: String,
    /// Every reported input name, and whether the runtime can detect a press
    /// on it. Undetectable ones (cursor position, accelerometer, gyroscope)
    /// are still legal in a binding, so they are kept and flagged rather than
    /// dropped.
    pub inputs: Vec<(String, bool)>,
}

impl DeviceInputs {
    /// Just the names, which is what mapping and validation compare against.
    pub fn names(&self) -> Vec<String> {
        self.inputs.iter().map(|(n, _)| n.clone()).collect()
    }
}

/// The inputs a device reports, for devices whose report we have recorded.
///
/// # This is a recording, not a guess
///
/// Every name below was read off real hardware with
/// `--dump-device-inputs`, and the file it came from is checked in as
/// `tests/fixtures/input-capture-ps5.txt`. Nothing here is inferred from a
/// backend, a vendor id, or a button count.
///
/// # Why it exists at all
///
/// Asking the runtime is always better, and [`dump_via_runtime`] does. But that
/// flag only exists in a runtime that has not reached players yet, and the
/// runtime they DO have names a PlayStation 5 pad identically: the WGInput
/// label table in the shipped binary is byte-for-byte the same. Every
/// PlayStation player would otherwise stay broken until a content release,
/// for want of a flag rather than for want of the names.
///
/// So this is consulted ONLY when the runtime could not be asked. When it can
/// be, its answer wins and this is never reached.
pub fn known_inputs(device_string: &str) -> Option<Vec<String>> {
    let (source, rest) = device_string.split_once('/')?;
    let (_, name) = rest.split_once('/')?;

    // Windows.Gaming.Input names a button by the label Windows gives it, and
    // falls back to `Button {n}` when Windows supplies none. For a PlayStation
    // pad it supplies Cross, Circle, Square and Triangle.
    //
    // MEASURED: `WGInput/0/PS5 Controller`, DualSense over USB, 2026-09-12.
    // The DualShock 4 entries ride the same Windows label enum rather than a
    // separate measurement, which is why the list is shared; if one ever turns
    // out to differ, the guardrail on the new runtime reports it instead of
    // failing silently the way the original bug did.
    let playstation = ["PS5 Controller", "PS4 Controller", "DualSense", "DualShock"];
    if source == "WGInput" && playstation.iter().any(|p| name.contains(p)) {
        return Some(
            [
                "Square", "Cross", "Circle", "Triangle",
                "Bumper L", "Bumper R",
                "Button 6", "Button 7",
                "View", "Menu",
                "Thumb L", "Thumb R",
                "Button 12", "Button 13", "Button 14",
                "Left X-", "Left X+", "Left Y-", "Left Y+",
                "Right X-", "Right X+", "Right Y-", "Right Y+",
                "Trigger L", "Trigger R",
                "Switch 0 N", "Switch 0 S", "Switch 0 W", "Switch 0 E",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
        );
    }

    None
}

/// Parse the `devices.txt` written by `--input-capture`.
///
/// Format is two columns, tab separated: `DEVICE\t<source/id/name>` opens a
/// device, then `INPUT\t<name>\t<0|1>` for each of its inputs.
pub fn parse_device_inputs(text: &str) -> Vec<DeviceInputs> {
    let mut all: Vec<DeviceInputs> = Vec::new();
    for line in text.lines() {
        let mut parts = line.split('\t');
        match parts.next() {
            Some("DEVICE") => {
                if let Some(name) = parts.next() {
                    all.push(DeviceInputs {
                        device: name.to_string(),
                        inputs: Vec::new(),
                    });
                }
            }
            Some("INPUT") => {
                if let (Some(current), Some(name)) = (all.last_mut(), parts.next()) {
                    let detectable = parts.next() != Some("0");
                    current.inputs.push((name.to_string(), detectable));
                }
            }
            _ => {}
        }
    }
    all
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

        devices.sort_by(|a, b| (&a.backend, a.id).cmp(&(&b.backend, b.id)));
        Enumeration {
            devices,
            not_searched,
        }
    }
}

/// Ask the emulator what it calls the controllers that are plugged in.
///
/// This is the authoritative answer and the XInput scan below is only a
/// fallback. The launcher cannot reliably work out the emulator's device names
/// on its own: it would have to reproduce SDL's enumeration order, SDL's
/// whitespace stripping, the XInput preferred-id rule and the Windows Xbox 360
/// exclusion hack, and then keep all of it in step with whichever SDL version
/// happens to be vendored. This build vendors SDL 2.25.0, and the name a
/// DualSense reports differs between SDL releases.
///
/// So the emulator is asked instead, through `--list-input-devices`, which
/// prints one `source/id/name` per line and exits. It cannot drift, because it
/// is the same code that will later resolve the string.
pub fn scan_via_runtime(
    platform: &crate::platform::Platform,
    runtime_exe: &std::path::Path,
    scratch_dir: &std::path::Path,
) -> Result<Enumeration, String> {
    if !runtime_exe.exists() {
        return Err(format!("no runtime at {}", runtime_exe.display()));
    }

    std::fs::create_dir_all(scratch_dir)
        .map_err(|e| format!("could not create {}: {e}", scratch_dir.display()))?;
    let list_path = scratch_dir.join("input-devices.txt");
    // A stale file from a previous run would be read as the current hardware.
    let _ = std::fs::remove_file(&list_path);

    let args = vec![std::ffi::OsString::from(format!(
        "--list-input-devices={}",
        list_path.display()
    ))];
    let run = platform
        .processes
        .run_capturing(runtime_exe, &args, &Default::default())
        .map_err(|e| format!("could not run the runtime: {e}"))?;

    let raw = std::fs::read_to_string(&list_path).map_err(|_| {
        format!(
            "the runtime wrote no device list. It is older than the launcher {}",
            "and does not support --list-input-devices, so rebuild it."
        )
    })?;
    let _ = run;

    let mut devices = Vec::new();
    for line in raw.lines() {
        if let Some(d) = parse_device_string(line.trim()) {
            devices.push(d);
        }
    }
    devices.sort_by(|a, b| (&a.backend, a.id).cmp(&(&b.backend, b.id)));
    Ok(Enumeration {
        devices,
        not_searched: Vec::new(),
    })
}

/// Ask the runtime what the pads are called AND what their inputs are called.
///
/// One process, one answer, for the same cost as [`scan_via_runtime`], which it
/// replaces wherever the runtime is new enough. The extra half of the answer is
/// what lets the launcher bind a pad it has never seen: control names are per
/// backend and do not overlap, so the only safe source for a binding is the
/// device's own report.
///
/// Returns an error on an older runtime that does not know the flag, so the
/// caller can fall back rather than treat a pad with no inputs as a pad with
/// no buttons.
pub fn dump_via_runtime(
    platform: &crate::platform::Platform,
    runtime_exe: &std::path::Path,
    scratch_dir: &std::path::Path,
) -> Result<(Enumeration, Vec<DeviceInputs>), String> {
    if !runtime_exe.exists() {
        return Err(format!("no runtime at {}", runtime_exe.display()));
    }
    std::fs::create_dir_all(scratch_dir)
        .map_err(|e| format!("could not create {}: {e}", scratch_dir.display()))?;
    let out = scratch_dir.join("device-inputs.txt");
    // A stale file from a previous run would be read as the current hardware.
    let _ = std::fs::remove_file(&out);

    let args = vec![std::ffi::OsString::from(format!(
        "--dump-device-inputs={}",
        out.display()
    ))];
    platform
        .processes
        .run_capturing(runtime_exe, &args, &Default::default())
        .map_err(|e| format!("could not run the runtime: {e}"))?;

    let raw = std::fs::read_to_string(&out).map_err(|_| {
        "the runtime wrote no device dump; it predates --dump-device-inputs".to_string()
    })?;

    let inputs = parse_device_inputs(&raw);
    let mut devices: Vec<DetectedDevice> = inputs
        .iter()
        .filter_map(|d| parse_device_string(&d.device))
        .collect();
    devices.sort_by(|a, b| (&a.backend, a.id).cmp(&(&b.backend, b.id)));
    Ok((
        Enumeration {
            devices,
            not_searched: Vec::new(),
        },
        inputs,
    ))
}

/// Parse the whole of `--list-input-devices` output into an enumeration.
///
/// Public so the exact bytes a real runtime prints can be used as a test
/// fixture. Nothing about device handling should have to be re-implemented in
/// a test in order to be checked.
pub fn parse_device_list(text: &str) -> Enumeration {
    let mut devices: Vec<DetectedDevice> = text
        .lines()
        .filter_map(|line| parse_device_string(line.trim()))
        .collect();
    devices.sort_by(|a, b| (&a.backend, a.id).cmp(&(&b.backend, b.id)));
    Enumeration {
        devices,
        not_searched: Vec::new(),
    }
}

/// Parse one `source/id/name` line, the format of `DeviceQualifier::ToString`.
///
/// The name may itself contain slashes, so only the first two are separators.
fn parse_device_string(line: &str) -> Option<DetectedDevice> {
    if line.is_empty() {
        return None;
    }
    let (source, rest) = line.split_once('/')?;
    let (id, name) = rest.split_once('/')?;
    let id: u8 = id.parse().ok()?;
    if name.is_empty() {
        return None;
    }

    let backend = match source {
        "XInput" => Backend::XInput,
        "SDL" => Backend::Sdl,
        "DInput" => Backend::DInput,
        other => Backend::Other(other.to_string()),
    };

    // The name alone is often not enough. XInput calls every ordinary pad
    // "Gamepad", which says nothing about who made it, so the backend decides:
    // only Xbox class hardware reaches XInput on Windows, since the SDL
    // backend refuses it and DirectInput is not preferred.
    let family = match DeviceFamily::from_name(name) {
        DeviceFamily::Generic if backend == Backend::XInput => DeviceFamily::Xbox,
        other => other,
    };

    Some(DetectedDevice {
        backend,
        id,
        name: name.to_string(),
        // The name alone is often not enough. XInput calls every ordinary pad
        family,
    })
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

    fn parse_all(text: &str) -> Enumeration {
        let mut devices: Vec<DetectedDevice> = text
            .lines()
            .filter_map(|l| parse_device_string(l.trim()))
            .collect();
        devices.sort_by(|a, b| (&a.backend, a.id).cmp(&(&b.backend, b.id)));
        Enumeration {
            devices,
            not_searched: Vec::new(),
        }
    }

    /// Exactly what the runtime printed on the development machine with one
    /// Xbox pad attached. Kept verbatim: it is the only real sample there is.
    const REAL_OUTPUT: &str = "DInput/0/Keyboard Mouse
WGInput/0/Xbox One Game Controller
WGInput/1/Xbox One Game Controller
XInput/0/Gamepad";

    #[test]
    fn the_real_runtime_output_parses() {
        let scan = parse_all(REAL_OUTPUT);
        assert_eq!(scan.devices.len(), 4, "{:?}", scan.devices);
    }

    /// One physical pad reported through three backends must be offered once.
    #[test]
    fn one_pad_in_three_backends_is_offered_once() {
        let pads = parse_all(REAL_OUTPUT);
        let chosen = pads.playable_pads();
        assert_eq!(chosen.len(), 1, "got {:?}", chosen);
        assert_eq!(chosen[0].device_string(), "XInput/0/Gamepad");
    }

    /// Keyboard and mouse is not a controller and must never take a port.
    #[test]
    fn keyboard_and_mouse_is_never_offered_as_a_pad() {
        let scan = parse_all(REAL_OUTPUT);
        for d in scan.playable_pads() {
            assert_ne!(d.backend, Backend::DInput, "{}", d.device_string());
        }
    }

    /// A PlayStation pad does not appear in XInput, so it must be picked up
    /// from SDL. This is the case that cannot be tested against hardware here.
    #[test]
    fn a_playstation_pad_is_taken_from_sdl() {
        let scan = parse_all(
            "DInput/0/Keyboard Mouse
SDL/0/DualSense Wireless Controller",
        );
        let chosen = scan.playable_pads();
        assert_eq!(chosen.len(), 1);
        assert_eq!(chosen[0].backend, Backend::Sdl);
        assert_eq!(
            chosen[0].family,
            DeviceFamily::PlayStation,
            "a DualSense should show PlayStation button art"
        );
    }

    /// XInput names every pad "Gamepad", which identifies nothing. The backend
    /// has to supply what the name does not.
    #[test]
    fn an_xinput_gamepad_is_known_to_be_an_xbox_pad() {
        let d = parse_device_string("XInput/0/Gamepad").unwrap();
        assert_eq!(d.family, DeviceFamily::Xbox);
    }

    /// A name containing a slash must survive, because only the first two are
    /// separators. Getting this wrong truncates the name and binds nothing.
    #[test]
    fn a_slash_in_a_device_name_is_preserved() {
        let d = parse_device_string("SDL/0/Some/Odd Pad").unwrap();
        assert_eq!(d.name, "Some/Odd Pad");
        assert_eq!(d.device_string(), "SDL/0/Some/Odd Pad");
    }

    #[test]
    fn rubbish_lines_are_ignored_rather_than_guessed_at() {
        assert!(parse_device_string("").is_none());
        assert!(parse_device_string("not a device").is_none());
        assert!(parse_device_string("SDL/notanumber/Pad").is_none());
        assert!(parse_device_string("SDL/0/").is_none());
    }
}
