//! Controllers: identity, profiles, and generated emulator input config.
//!
//! # What exists today, measured
//!
//! Both shipped profiles, `Profiles/GCPad/Xbox.ini` and `Profiles/GCPad/PS.ini`,
//! bind `Device = XInput/0/Gamepad`. The PlayStation one is Xbox bindings with
//! PlayStation button art layered on by a texture pack. There is no PlayStation
//! device backend at all.
//!
//! The two files differ in exactly two bindings: Xbox put `Z` on the right
//! shoulder and the GameCube triggers on the analog triggers, PS did the
//! reverse. Adam's call is that modern pads should behave the same, so there is
//! now **one default mapping** and the controller family decides only which
//! button art is shown.
//!
//! # Defaults are defaults, not decisions
//!
//! [`Mapping::default_gamepad`] is a starting point. Every binding is overridable per
//! physical device, at any time, and [`ControllerProfile::reset`] puts any
//! subset back. The launcher never silently keeps a mapping the user replaced,
//! and it never prevents a change because a default exists.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// A GameCube input the emulator expects to be bound.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GcInput {
    ButtonA,
    ButtonB,
    ButtonX,
    ButtonY,
    ButtonZ,
    Start,
    MainStickUp,
    MainStickDown,
    MainStickLeft,
    MainStickRight,
    CStickUp,
    CStickDown,
    CStickLeft,
    CStickRight,
    TriggerL,
    TriggerR,
    DPadUp,
    DPadDown,
    DPadLeft,
    DPadRight,
}

impl GcInput {
    /// The key Dolphin uses in `GCPadNew.ini`.
    pub fn ini_key(self) -> &'static str {
        match self {
            GcInput::ButtonA => "Buttons/A",
            GcInput::ButtonB => "Buttons/B",
            GcInput::ButtonX => "Buttons/X",
            GcInput::ButtonY => "Buttons/Y",
            GcInput::ButtonZ => "Buttons/Z",
            GcInput::Start => "Buttons/Start",
            GcInput::MainStickUp => "Main Stick/Up",
            GcInput::MainStickDown => "Main Stick/Down",
            GcInput::MainStickLeft => "Main Stick/Left",
            GcInput::MainStickRight => "Main Stick/Right",
            GcInput::CStickUp => "C-Stick/Up",
            GcInput::CStickDown => "C-Stick/Down",
            GcInput::CStickLeft => "C-Stick/Left",
            GcInput::CStickRight => "C-Stick/Right",
            GcInput::TriggerL => "Triggers/L",
            GcInput::TriggerR => "Triggers/R",
            GcInput::DPadUp => "D-Pad/Up",
            GcInput::DPadDown => "D-Pad/Down",
            GcInput::DPadLeft => "D-Pad/Left",
            GcInput::DPadRight => "D-Pad/Right",
        }
    }

    /// What the user is shown in the remap screen.
    pub fn label(self) -> &'static str {
        match self {
            GcInput::ButtonA => "A",
            GcInput::ButtonB => "B",
            GcInput::ButtonX => "X",
            GcInput::ButtonY => "Y",
            GcInput::ButtonZ => "Z",
            GcInput::Start => "Start",
            GcInput::MainStickUp => "Move up",
            GcInput::MainStickDown => "Move down",
            GcInput::MainStickLeft => "Move left",
            GcInput::MainStickRight => "Move right",
            GcInput::CStickUp => "Camera up",
            GcInput::CStickDown => "Camera down",
            GcInput::CStickLeft => "Camera left",
            GcInput::CStickRight => "Camera right",
            GcInput::TriggerL => "Left trigger",
            GcInput::TriggerR => "Right trigger",
            GcInput::DPadUp => "D-pad up",
            GcInput::DPadDown => "D-pad down",
            GcInput::DPadLeft => "D-pad left",
            GcInput::DPadRight => "D-pad right",
        }
    }

    pub const ALL: &'static [GcInput] = &[
        GcInput::ButtonA,
        GcInput::ButtonB,
        GcInput::ButtonX,
        GcInput::ButtonY,
        GcInput::ButtonZ,
        GcInput::Start,
        GcInput::MainStickUp,
        GcInput::MainStickDown,
        GcInput::MainStickLeft,
        GcInput::MainStickRight,
        GcInput::CStickUp,
        GcInput::CStickDown,
        GcInput::CStickLeft,
        GcInput::CStickRight,
        GcInput::TriggerL,
        GcInput::TriggerR,
        GcInput::DPadUp,
        GcInput::DPadDown,
        GcInput::DPadLeft,
        GcInput::DPadRight,
    ];
}

/// A physical control on the connected device, as Dolphin names it.
pub type PhysicalBinding = String;

/// A complete set of bindings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mapping(BTreeMap<GcInput, PhysicalBinding>);

impl Mapping {
    /// The shipping default, one mapping for every modern pad.
    ///
    /// Taken from the working `Xbox.ini`, including the deliberate B and X
    /// swap: GameCube B sits on the physical X button and GameCube X on the
    /// physical B. That is not a transcription error, it is how NFL Street has
    /// always been played here and changing it would break muscle memory.
    ///
    /// The one thing dropped from the old pair of profiles is the
    /// shoulder-versus-trigger difference between Xbox and PlayStation. Adam's
    /// call: modern pads behave the same, so the family now decides the button
    /// art and nothing else.
    pub fn default_gamepad() -> Self {
        use GcInput::*;
        let pairs: &[(GcInput, &str)] = &[
            (ButtonA, "`Button A`"),
            (ButtonB, "`Button X`"),
            (ButtonX, "`Button B`"),
            (ButtonY, "`Button Y`"),
            (ButtonZ, "`Shoulder R`"),
            (Start, "Start"),
            (MainStickUp, "`Left Y+`"),
            (MainStickDown, "`Left Y-`"),
            (MainStickLeft, "`Left X-`"),
            (MainStickRight, "`Left X+`"),
            (CStickUp, "`Right Y+`"),
            (CStickDown, "`Right Y-`"),
            (CStickLeft, "`Right X-`"),
            (CStickRight, "`Right X+`"),
            (TriggerL, "`Trigger L`"),
            (TriggerR, "`Trigger R`"),
            (DPadUp, "`Pad N`"),
            (DPadDown, "`Pad S`"),
            (DPadLeft, "`Pad W`"),
            (DPadRight, "`Pad E`"),
        ];
        Mapping(pairs.iter().map(|(k, v)| (*k, v.to_string())).collect())
    }

    pub fn get(&self, input: GcInput) -> Option<&str> {
        self.0.get(&input).map(String::as_str)
    }

    pub fn set(&mut self, input: GcInput, binding: impl Into<String>) {
        self.0.insert(input, binding.into());
    }

    /// Inputs with no binding. A profile with any of these is incomplete and
    /// the remap screen should say which.
    pub fn unbound(&self) -> Vec<GcInput> {
        GcInput::ALL
            .iter()
            .copied()
            .filter(|i| !self.0.contains_key(i))
            .collect()
    }

    /// Physical controls bound to more than one GameCube input.
    ///
    /// Not always an error. Someone may deliberately put Z and a trigger on the
    /// same control. The remap screen warns rather than refuses.
    pub fn conflicts(&self) -> BTreeMap<&str, Vec<GcInput>> {
        let mut by_binding: BTreeMap<&str, Vec<GcInput>> = BTreeMap::new();
        for (input, binding) in &self.0 {
            by_binding.entry(binding.as_str()).or_default().push(*input);
        }
        by_binding.retain(|_, inputs| inputs.len() > 1);
        by_binding
    }
}

impl Default for Mapping {
    fn default() -> Self {
        Self::default_gamepad()
    }
}

/// The family of a connected device.
///
/// Decides the button art and the device backend, and nothing else.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceFamily {
    Xbox,
    PlayStation,
    Nintendo,
    SteamDeck,
    GameCubeAdapter,
    Generic,
    Keyboard,
}

impl DeviceFamily {
    /// Which prompt texture pack to activate.
    ///
    /// Reuses the existing `PS Controls` and `Xbox Controls` mods, which were
    /// only ever texture overlays. That is the whole of what they did, so they
    /// map cleanly onto this.
    pub fn prompt_pack(self) -> &'static str {
        match self {
            DeviceFamily::PlayStation => "ps-prompts",
            DeviceFamily::Nintendo => "nintendo-prompts",
            DeviceFamily::GameCubeAdapter => "gamecube-prompts",
            // Xbox art is the default the game already ships with.
            _ => "xbox-prompts",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            DeviceFamily::Xbox => "Xbox controller",
            DeviceFamily::PlayStation => "PlayStation controller",
            DeviceFamily::Nintendo => "Nintendo controller",
            DeviceFamily::SteamDeck => "Steam Deck",
            DeviceFamily::GameCubeAdapter => "GameCube adapter",
            DeviceFamily::Generic => "Controller",
            DeviceFamily::Keyboard => "Keyboard",
        }
    }

    /// Classify from an SDL controller name.
    ///
    /// Deliberately generous. A device we cannot place still works, it just
    /// shows Xbox art, which is the default the game ships with anyway.
    pub fn from_name(name: &str) -> Self {
        let n = name.to_ascii_lowercase();
        // Xbox is tested FIRST and deliberately. SDL reports Sony pads as the
        // bare string "Wireless Controller", so that phrase has to count as a
        // PlayStation signal, but it also appears inside "Xbox Wireless
        // Controller". Testing PlayStation first classified every modern Xbox
        // pad as a PlayStation one, which showed the wrong button art on the
        // most common controller there is. Found by a test, not by inspection.
        if n.contains("xbox") || n.contains("xinput") {
            DeviceFamily::Xbox
        } else if Self::looks_playstation(&n) {
            DeviceFamily::PlayStation
        } else if n.contains("steam deck") {
            DeviceFamily::SteamDeck
        } else if n.contains("nintendo") || n.contains("switch") || n.contains("joy-con")
            || n.contains("pro controller")
        {
            DeviceFamily::Nintendo
        } else if n.contains("gamecube") || n.contains("wup-028") {
            DeviceFamily::GameCubeAdapter
        } else {
            DeviceFamily::Generic
        }
    }

    /// Whether a lowercased device name looks like a Sony pad.
    ///
    /// Shared by [`DeviceFamily::from_name`] and the conflict detector so the
    /// two cannot drift apart and disagree about the same device.
    fn looks_playstation(lower: &str) -> bool {
        if lower.contains("xbox") {
            return false;
        }
        lower.contains("dualsense")
            || lower.contains("dualshock")
            || lower.contains("ps5")
            || lower.contains("ps4")
            || lower.contains("playstation")
            || lower.contains("wireless controller")
    }
}

/// A device the launcher has seen, identified so it is recognized again.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceIdentity {
    /// SDL's joystick GUID: bus, vendor, product and version.
    ///
    /// Stable across reconnects and across reboots, unlike an index. Two
    /// identical pads share a GUID, which is why `serial` exists.
    pub guid: String,
    /// Hardware serial where the platform exposes one. Distinguishes two
    /// identical controllers so their profiles do not collide.
    pub serial: Option<String>,
    pub name: String,
    pub family: DeviceFamily,
}

impl DeviceIdentity {
    /// The key a profile is stored under.
    ///
    /// Never an index or a port number. Both change when a device is
    /// unplugged, which is exactly when a profile most needs to survive.
    pub fn profile_key(&self) -> String {
        match &self.serial {
            Some(serial) if !serial.is_empty() => format!("{}:{}", self.guid, serial),
            _ => self.guid.clone(),
        }
    }
}

/// Per-device tuning, all of it adjustable at any time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tuning {
    /// Percentage of stick travel ignored at rest.
    pub deadzone_percent: u8,
    /// How far a trigger must travel before it counts as pressed.
    pub trigger_threshold_percent: u8,
    pub rumble_percent: u8,
    /// Stick sensitivity, 100 being unmodified.
    pub sensitivity_percent: u8,
}

impl Default for Tuning {
    fn default() -> Self {
        Self {
            deadzone_percent: 15,
            trigger_threshold_percent: 50,
            rumble_percent: 100,
            sensitivity_percent: 100,
        }
    }
}

/// A saved configuration for one physical device.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ControllerProfile {
    pub identity: DeviceIdentity,
    /// Which player slot, 1 through 4.
    pub port: u8,
    pub mapping: Mapping,
    pub tuning: Tuning,
    /// Bindings the user changed away from the default.
    ///
    /// Tracked so the remap screen can show what is customized and offer to
    /// reset just those, and so a future change to the shipping default reaches
    /// everything the user has not deliberately overridden.
    pub overridden: Vec<GcInput>,
}

impl ControllerProfile {
    pub fn new(identity: DeviceIdentity, port: u8) -> Self {
        Self {
            identity,
            port,
            mapping: Mapping::default_gamepad(),
            tuning: Tuning::default(),
            overridden: Vec::new(),
        }
    }

    /// Rebind one input and remember that the user chose it.
    pub fn rebind(&mut self, input: GcInput, binding: impl Into<String>) {
        self.mapping.set(input, binding);
        if !self.overridden.contains(&input) {
            self.overridden.push(input);
            self.overridden.sort();
        }
    }

    /// Put specific inputs back to the shipping default.
    ///
    /// Passing an empty slice resets everything, which is what the "Reset to
    /// defaults" button does. Resetting a subset is what the per-row reset
    /// does.
    pub fn reset(&mut self, inputs: &[GcInput]) {
        let defaults = Mapping::default_gamepad();
        let targets: Vec<GcInput> = if inputs.is_empty() {
            GcInput::ALL.to_vec()
        } else {
            inputs.to_vec()
        };
        for input in targets {
            if let Some(binding) = defaults.get(input) {
                self.mapping.set(input, binding.to_string());
            }
            self.overridden.retain(|i| *i != input);
        }
        if inputs.is_empty() {
            self.tuning = Tuning::default();
        }
    }

    pub fn is_customized(&self) -> bool {
        !self.overridden.is_empty() || self.tuning != Tuning::default()
    }
}

/// Something on the system that will fight the launcher for a controller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputConflict {
    pub what: &'static str,
    pub symptom: &'static str,
    pub remedy: &'static str,
}

/// Detects the two known causes of duplicated input on Windows.
#[derive(Debug)]
pub struct ConflictDetector;

impl ConflictDetector {
    /// Steam Input presents a second virtual pad alongside the real one, so
    /// every press registers twice.
    ///
    /// The detectable signal is two devices whose names differ but which report
    /// the same underlying hardware, or the presence of a device SDL names as a
    /// Steam virtual controller.
    pub fn inspect(device_names: &[String]) -> Vec<InputConflict> {
        let mut found = Vec::new();
        let lower: Vec<String> = device_names.iter().map(|n| n.to_ascii_lowercase()).collect();

        if lower.iter().any(|n| n.contains("steam virtual") || n.contains("steam controller")) {
            found.push(InputConflict {
                what: "Steam Input",
                symptom: "every button press registers twice, because Steam adds \
                          a second virtual controller alongside the real one",
                remedy: "In Steam, right click NFL Street Reborn, Properties, \
                         Controller, and set it to Disable Steam Input",
            });
        }

        // DS4Windows maps a PlayStation pad onto a virtual Xbox pad. If both
        // the real and the virtual device are visible, input arrives twice.
        // Uses the same predicate as classification, so the two cannot
        // disagree about whether a given device is a Sony pad.
        let has_playstation = lower.iter().any(|n| DeviceFamily::looks_playstation(n));
        let has_virtual_xbox = lower
            .iter()
            .any(|n| n.contains("xbox 360") || n.contains("virtual gamepad"));
        if has_playstation && has_virtual_xbox {
            found.push(InputConflict {
                what: "DS4Windows or a similar remapper",
                symptom: "your PlayStation controller appears twice, once as \
                          itself and once as a virtual Xbox pad, so input is \
                          doubled",
                remedy: "Close DS4Windows. Reborn talks to PlayStation \
                         controllers directly and does not need it",
            });
        }

        found
    }
}

/// Writes `GCPadNew.ini` from profiles.
#[derive(Debug)]
pub struct GcPadConfig;

impl GcPadConfig {
    /// Generate the whole file for up to four players.
    ///
    /// Regenerated on every launch from the launcher's own model, so a change
    /// made inside Dolphin does not persist and the Settings page stays the
    /// single source of truth.
    pub fn generate(profiles: &[ControllerProfile], device_string: impl Fn(&DeviceIdentity) -> String) -> String {
        let mut out = String::new();
        for profile in profiles.iter().filter(|p| (1..=4).contains(&p.port)) {
            out.push_str(&format!("[GCPad{}]\n", profile.port));
            out.push_str(&format!("Device = {}\n", device_string(&profile.identity)));

            for input in GcInput::ALL {
                if let Some(binding) = profile.mapping.get(*input) {
                    out.push_str(&format!("{} = {}\n", input.ini_key(), binding));
                }
            }

            let dz = f64::from(profile.tuning.deadzone_percent);
            out.push_str(&format!("Main Stick/Dead Zone = {dz:.2}\n"));
            out.push_str(&format!("C-Stick/Dead Zone = {dz:.2}\n"));
            out.push_str(&format!(
                "Triggers/Threshold = {:.2}\n",
                f64::from(profile.tuning.trigger_threshold_percent)
            ));
            out.push_str(&format!(
                "Rumble/Motor = `Motor` {:.2}\n",
                f64::from(profile.tuning.rumble_percent)
            ));
            out.push('\n');
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity(name: &str) -> DeviceIdentity {
        DeviceIdentity {
            guid: "030000004c050000cc09000000006800".into(),
            serial: None,
            name: name.into(),
            family: DeviceFamily::from_name(name),
        }
    }

    #[test]
    fn the_default_binds_every_gamecube_input() {
        assert!(
            Mapping::default_gamepad().unbound().is_empty(),
            "an unbound input means a control the player cannot use"
        );
    }

    /// The B and X swap is deliberate, taken from the working Xbox.ini. It is
    /// how NFL Street has always been played here.
    #[test]
    fn the_b_and_x_swap_is_preserved() {
        let m = Mapping::default_gamepad();
        assert_eq!(m.get(GcInput::ButtonB), Some("`Button X`"));
        assert_eq!(m.get(GcInput::ButtonX), Some("`Button B`"));
        assert_eq!(m.get(GcInput::ButtonA), Some("`Button A`"));
        assert_eq!(m.get(GcInput::ButtonY), Some("`Button Y`"));
    }

    #[test]
    fn the_default_has_no_accidental_double_bindings() {
        assert!(
            Mapping::default_gamepad().conflicts().is_empty(),
            "{:?}",
            Mapping::default_gamepad().conflicts()
        );
    }

    /// Adam's call: modern PlayStation and Xbox pads behave the same. The old
    /// PS.ini differed only in putting Z on the right trigger and the GameCube
    /// triggers on the shoulders. One mapping now serves both.
    #[test]
    fn playstation_and_xbox_share_one_mapping() {
        let xbox = ControllerProfile::new(identity("Xbox Wireless Controller"), 1);
        let ps = ControllerProfile::new(identity("DualSense Wireless Controller"), 2);
        assert_eq!(xbox.mapping, ps.mapping);
        assert_ne!(
            xbox.identity.family.prompt_pack(),
            ps.identity.family.prompt_pack(),
            "the family should still change the button art"
        );
    }

    #[test]
    fn families_are_recognized_from_their_names() {
        for (name, expected) in [
            ("DualSense Wireless Controller", DeviceFamily::PlayStation),
            ("Sony DualShock 4", DeviceFamily::PlayStation),
            ("Xbox Series X Controller", DeviceFamily::Xbox),
            ("Nintendo Switch Pro Controller", DeviceFamily::Nintendo),
            ("Steam Deck Built-in Controller", DeviceFamily::SteamDeck),
            ("Mayflash GameCube Adapter", DeviceFamily::GameCubeAdapter),
            ("Some Unknown Pad", DeviceFamily::Generic),
        ] {
            assert_eq!(DeviceFamily::from_name(name), expected, "{name}");
        }
    }

    #[test]
    fn an_unrecognized_device_still_works_with_default_art() {
        let f = DeviceFamily::from_name("Brand New Pad 9000");
        assert_eq!(f, DeviceFamily::Generic);
        assert_eq!(
            f.prompt_pack(),
            "xbox-prompts",
            "an unknown device falls back to the art the game already ships"
        );
    }

    /// A profile must survive unplugging, so it cannot be keyed on an index.
    #[test]
    fn profiles_are_keyed_on_hardware_not_on_a_port() {
        let a = identity("DualSense Wireless Controller");
        let key = a.profile_key();
        assert!(key.contains("030000004c05"), "{key}");
        assert!(!key.contains('1'), "a port number must not appear in the key: {key}");
    }

    /// Two identical pads must not share one profile.
    #[test]
    fn two_identical_controllers_get_separate_profiles() {
        let mut a = identity("DualSense Wireless Controller");
        let mut b = a.clone();
        assert_eq!(a.profile_key(), b.profile_key(), "same model, same guid");

        a.serial = Some("AAAA".into());
        b.serial = Some("BBBB".into());
        assert_ne!(
            a.profile_key(),
            b.profile_key(),
            "a serial must separate two identical pads"
        );
    }

    #[test]
    fn rebinding_records_the_override_and_resetting_clears_it() {
        let mut p = ControllerProfile::new(identity("Xbox Wireless Controller"), 1);
        assert!(!p.is_customized());

        p.rebind(GcInput::ButtonZ, "`Trigger R`");
        assert_eq!(p.mapping.get(GcInput::ButtonZ), Some("`Trigger R`"));
        assert_eq!(p.overridden, vec![GcInput::ButtonZ]);
        assert!(p.is_customized());

        p.reset(&[GcInput::ButtonZ]);
        assert_eq!(p.mapping.get(GcInput::ButtonZ), Some("`Shoulder R`"));
        assert!(!p.is_customized());
    }

    #[test]
    fn resetting_everything_also_restores_tuning() {
        let mut p = ControllerProfile::new(identity("Xbox Wireless Controller"), 1);
        p.rebind(GcInput::ButtonA, "`Button Y`");
        p.tuning.deadzone_percent = 40;
        assert!(p.is_customized());

        p.reset(&[]);
        assert_eq!(p.mapping, Mapping::default_gamepad());
        assert_eq!(p.tuning, Tuning::default());
        assert!(!p.is_customized());
    }

    /// Resetting one binding must not touch the others the user changed.
    #[test]
    fn a_partial_reset_leaves_other_overrides_alone() {
        let mut p = ControllerProfile::new(identity("Xbox Wireless Controller"), 1);
        p.rebind(GcInput::ButtonZ, "`Trigger R`");
        p.rebind(GcInput::TriggerL, "`Shoulder L`");

        p.reset(&[GcInput::ButtonZ]);
        assert_eq!(p.mapping.get(GcInput::TriggerL), Some("`Shoulder L`"));
        assert_eq!(p.overridden, vec![GcInput::TriggerL]);
    }

    #[test]
    fn double_bindings_are_reported_but_not_forbidden() {
        let mut p = ControllerProfile::new(identity("Xbox Wireless Controller"), 1);
        p.rebind(GcInput::ButtonZ, "`Trigger R`");
        p.rebind(GcInput::TriggerR, "`Trigger R`");
        let conflicts = p.mapping.conflicts();
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts["`Trigger R`"].len(), 2);
    }

    #[test]
    fn generated_config_has_a_section_per_player_and_every_binding() {
        let profiles = vec![
            ControllerProfile::new(identity("Xbox Wireless Controller"), 1),
            ControllerProfile::new(identity("DualSense Wireless Controller"), 2),
        ];
        let ini = GcPadConfig::generate(&profiles, |_| "SDL/0/Gamepad".into());

        assert!(ini.contains("[GCPad1]"));
        assert!(ini.contains("[GCPad2]"));
        for input in GcInput::ALL {
            assert!(
                ini.contains(input.ini_key()),
                "{} is missing from the generated file",
                input.ini_key()
            );
        }
        assert!(ini.contains("Buttons/B = `Button X`"), "swap preserved:\n{ini}");
    }

    #[test]
    fn tuning_reaches_the_generated_config() {
        let mut p = ControllerProfile::new(identity("Xbox Wireless Controller"), 1);
        p.tuning.deadzone_percent = 22;
        p.tuning.trigger_threshold_percent = 65;
        let ini = GcPadConfig::generate(&[p], |_| "SDL/0/Gamepad".into());
        assert!(ini.contains("Main Stick/Dead Zone = 22.00"), "{ini}");
        assert!(ini.contains("Triggers/Threshold = 65.00"), "{ini}");
    }

    #[test]
    fn ports_outside_one_to_four_are_ignored() {
        let mut p = ControllerProfile::new(identity("Xbox Wireless Controller"), 9);
        p.port = 9;
        let ini = GcPadConfig::generate(&[p], |_| "SDL/0/Gamepad".into());
        assert!(ini.is_empty(), "a GameCube has four ports:\n{ini}");
    }

    #[test]
    fn steam_input_is_detected_and_explained() {
        let names = vec![
            "Xbox Wireless Controller".to_string(),
            "Steam Virtual Gamepad".to_string(),
        ];
        let conflicts = ConflictDetector::inspect(&names);
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].what, "Steam Input");
        assert!(conflicts[0].remedy.contains("Disable Steam Input"));
    }

    #[test]
    fn a_playstation_pad_doubled_by_a_remapper_is_detected() {
        let names = vec![
            "DualShock 4 Wireless Controller".to_string(),
            "Xbox 360 Controller for Windows".to_string(),
        ];
        let conflicts = ConflictDetector::inspect(&names);
        assert_eq!(conflicts.len(), 1);
        assert!(conflicts[0].what.contains("DS4Windows"));
    }

    #[test]
    fn an_ordinary_setup_reports_no_conflict() {
        let names = vec!["DualSense Wireless Controller".to_string()];
        assert!(ConflictDetector::inspect(&names).is_empty());
    }
}
