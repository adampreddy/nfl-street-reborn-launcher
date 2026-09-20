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
    /// B binds to physical B and X binds to physical X. The game's own
    /// match-specific handling already supplies the intended behavior;
    /// duplicating the old Dolphin swap would reverse it again.
    ///
    /// The one thing dropped from the old pair of profiles is the
    /// shoulder-versus-trigger difference between Xbox and PlayStation. Adam's
    /// call: modern pads behave the same, so the family now decides the button
    /// art and nothing else.
    pub fn default_gamepad() -> Self {
        use GcInput::*;
        let pairs: &[(GcInput, &str)] = &[
            (ButtonA, "`Button A`"),
            // NOT swapped. The X/B swap is done inside the game, by a
            // match-gated DOL cave, and the note recording that work says
            // plainly it was done "so the Dolphin controller remap can be
            // turned off". Swapping here too cancels it out during a match
            // and wrongly swaps in the menus, which stay vanilla.
            //
            // Checked against the working install's own GCPadNew.ini, which
            // binds Button B to B and Button X to X.
            (ButtonB, "`Button B`"),
            (ButtonX, "`Button X`"),
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

    /// The names, in order of preference, that mean this GameCube input on a
    /// real device.
    ///
    /// Every entry here was READ OFF A DEVICE, not guessed. The launcher is
    /// never allowed to invent a control name, because a name that does not
    /// exist on the target backend parses cleanly and binds nothing, with no
    /// error anywhere. That is the whole PlayStation 5 bug.
    ///
    /// Three vocabularies appear, and they do not overlap:
    ///
    /// - **XInput** (`Button A`, `Shoulder R`, `Start`, `Pad N`). What the
    ///   working install has always used, and what an Xbox pad reports.
    /// - **WGInput with Windows button labels** (`Cross`, `Bumper R`, `Menu`,
    ///   `Switch 0 N`). What a PlayStation 5 pad reports on Windows. Measured
    ///   2026-09-12 from `WGInput/0/PS5 Controller`.
    /// - **Numeric** (`Button 7`, `Axis Zr+`, `Hat 0 N`). What DirectInput and
    ///   SDL report. Index order is NOT stable across pads, so nothing here
    ///   claims to know what `Button 7` means. Those pads go to the remap
    ///   wizard, which measures instead of guessing.
    ///
    /// Positions, not letters. GameCube B takes the pad's east face button,
    /// which is Xbox B and PlayStation Circle. GameCube X takes the west one,
    /// which is Xbox X and PlayStation Square. Pairing by letter would put
    /// Circle on X and cross the player's thumb over.
    pub fn candidates(input: GcInput) -> &'static [&'static str] {
        use GcInput::*;
        match input {
            // COMPLETE BY CONSTRUCTION, not by whichever pad was to hand.
            //
            // The first version of this list held the names ONE measured
            // DualSense reported, and every pad that says something else was
            // dead on exactly the buttons where it differs, in silence.
            // Reported 2026-09-19: a PlayStation pad and a Switch pad both
            // stuck at the press start screen. The names a button can have
            // are finite: they are the label table in the fork's WGInput.cpp
            // (Windows' GameControllerButtonLabel). Every label that can sit
            // in a position is listed for that position. Order is preference
            // and only matters to `auto_from`, so what already narrows to a
            // name keeps narrowing to the same one.
            //
            // Face buttons. South, east, west, north for the pads that label
            // by shape or by Xbox letter. A pad that letters its buttons
            // (`Letter A`) is a different case and needs no position guessed:
            // a GameCube pad has an A, a B, an X and a Y of its own, so each
            // goes on the button carrying its letter, which is what a
            // Nintendo player means by "A".
            ButtonA => &["Button A", "Cross", "Letter A", "Button S"],
            ButtonB => &["Button B", "Circle", "Letter B", "Button E"],
            ButtonX => &["Button X", "Square", "Letter X", "Button W"],
            ButtonY => &["Button Y", "Triangle", "Letter Y", "Button N"],
            // Z on the right shoulder, as the working install has it.
            // `Right 1` is the same button on a pad that numbers them (R1).
            ButtonZ => &["Shoulder R", "Bumper R", "Right 1"],
            // The button in the start position has four names. XInput says
            // Start; Windows labels a PlayStation pad's Menu or Options
            // depending on the driver; a Switch pad's is Plus.
            Start => &["Start", "Menu", "Options", "Plus"],
            MainStickUp => &["Left Y+"],
            MainStickDown => &["Left Y-"],
            MainStickLeft => &["Left X-"],
            MainStickRight => &["Left X+"],
            CStickUp => &["Right Y+"],
            CStickDown => &["Right Y-"],
            CStickLeft => &["Right X-"],
            CStickRight => &["Right X+"],
            TriggerL => &["Trigger L"],
            TriggerR => &["Trigger R"],
            // XInput calls the d-pad a pad. WGInput calls it switch zero.
            DPadUp => &["Pad N", "Switch 0 N"],
            DPadDown => &["Pad S", "Switch 0 S"],
            DPadLeft => &["Pad W", "Switch 0 W"],
            DPadRight => &["Pad E", "Switch 0 E"],
        }
    }

    /// One binding that works on every pad we know the vocabulary of, without
    /// being told which pad it is.
    ///
    /// # Why this is not a bodge
    ///
    /// Read out of the fork's own `ExpressionParser.cpp`, not assumed:
    ///
    /// - An unresolvable control is **zero**, silently:
    ///   `ControlExpression::GetValueIgnoringSuppression` opens with
    ///   `if (!m_input) return 0.0;`. That silence is exactly what made the
    ///   PlayStation bug invisible, and it is exactly what makes this safe.
    /// - `|` is `TOK_OR`, and `BinaryExpression` evaluates it as
    ///   `std::max(lhs, rhs)`. So `A|B` is "whichever of these exists".
    /// - For outputs, `BinaryExpression::SetValue` drives BOTH sides, and
    ///   `ControlExpression::SetValue` is a no-op when the output does not
    ///   resolve. So an OR is safe for the rumble motor too.
    ///
    /// So `` `Button A`|`Cross` `` binds the south face button on an Xbox pad
    /// AND on a PlayStation pad, with no detection of any kind. That matters
    /// because the launcher cannot always ask what a pad calls its controls:
    /// only a runtime carrying `--dump-device-inputs` can answer, and players
    /// keep the runtime they have until a content update reaches them.
    ///
    /// Matching on the device NAME instead was the previous attempt and it was
    /// wrong: Windows names a DualSense from its HID product string, which is
    /// `PS5 Controller` on one machine and other things elsewhere. A pad whose
    /// name was not in the list got a correct Device line and dead bindings,
    /// which is the original bug wearing a different hat.
    pub fn universal(input: GcInput) -> String {
        Self::candidates(input)
            .iter()
            .map(|name| Self::quote(name))
            .collect::<Vec<_>>()
            .join("|")
    }

    /// The mapping to use when the pad's own control names are NOT available.
    ///
    /// Every control binds every vocabulary at once. Exact names are better and
    /// [`Mapping::auto_from`] produces them whenever the runtime can be asked;
    /// this is what a player gets in the meantime, and it works.
    pub fn universal_gamepad() -> Self {
        Mapping(
            GcInput::ALL
                .iter()
                .map(|input| (*input, Self::universal(*input)))
                .collect(),
        )
    }

    /// True when `value` is the launcher's own vocabulary for this input:
    /// every name in it is one of the input's candidates.
    ///
    /// That covers the universal binding, the exact name a runtime dump
    /// narrows it to, and the XInput-only default every build before 3.0.6
    /// wrote. None of those is a player's choice, and none says anything the
    /// universal binding does not already say. A value naming anything else
    /// (`Circle` on A, `Trigger R` on Z) is the player's.
    pub fn is_own_vocabulary(input: GcInput, value: &str) -> bool {
        Self::names_are_all_in(value, Self::candidates(input))
    }

    /// [`Mapping::is_own_vocabulary`] for the three non-GameCube bindings.
    pub fn extra_is_own_vocabulary(extra: Extra, value: &str) -> bool {
        Self::names_are_all_in(value, Self::extra_candidates(extra))
    }

    fn names_are_all_in(value: &str, names: &[&str]) -> bool {
        let mut any = false;
        for part in value.split('|') {
            let bare = Self::bare(part);
            if bare.is_empty() || !names.contains(&bare) {
                return false;
            }
            any = true;
        }
        any
    }

    /// Strip the backticks Dolphin uses to quote a control name containing
    /// spaces, so a stored binding can be compared to a reported input name.
    pub fn bare(binding: &str) -> &str {
        binding.trim().trim_matches('`')
    }

    /// Quote a control name the way Dolphin's expression parser expects.
    ///
    /// Always quoted. A bare name is only legal while it contains nothing the
    /// parser treats as an operator, and `Switch 0 N` already contains spaces.
    /// Quoting unconditionally removes a whole class of "it parsed but bound
    /// nothing" that nobody would ever see reported.
    pub fn quote(name: &str) -> String {
        format!("`{name}`")
    }

    /// Build the best mapping this device can actually support, from the names
    /// it reports, plus the inputs that could not be matched.
    ///
    /// The second half of the pair is the point. An empty list means the pad is
    /// fully playable. A non-empty list is the exact set of controls the remap
    /// wizard has to ask the player to press, and nothing else.
    pub fn auto_from(available: &[String]) -> (Self, Vec<GcInput>) {
        let mut mapping = Mapping(BTreeMap::new());
        let mut unmatched = Vec::new();
        for input in GcInput::ALL {
            let found = Self::candidates(*input)
                .iter()
                .find(|candidate| available.iter().any(|a| Self::bare(a) == **candidate));
            match found {
                Some(name) => {
                    mapping.set(*input, Self::quote(name));
                }
                None => unmatched.push(*input),
            }
        }
        (mapping, unmatched)
    }

    /// Bindings that name a control this device does not have.
    ///
    /// THE GUARDRAIL. Dolphin accepts an unresolvable name in silence: the file
    /// looks right, the device line is right, and the pad is dead. Checking
    /// before the file is written is the only place this is catchable, and it
    /// is how a repeat of the PlayStation 5 bug gets caught on the first
    /// launch rather than in the Discord.
    ///
    /// An empty `available` means the device list could not be read at all, and
    /// nothing is reported, because claiming every binding is broken on no
    /// evidence would be worse than staying quiet.
    pub fn unresolvable(&self, available: &[String]) -> Vec<GcInput> {
        if available.is_empty() {
            return Vec::new();
        }
        GcInput::ALL
            .iter()
            .copied()
            .filter(|input| match self.get(*input) {
                // An unbound input is already reported by `unbound`.
                None => false,
                Some(binding) => {
                    let bare = Self::bare(binding);
                    // Only a plain control name is checked. Anything with an
                    // operator in it is a hand written expression, and this is
                    // not an expression evaluator: it must not veto something
                    // it cannot parse.
                    if bare.contains(['|', '&', '!', '+', '^', '(', ')']) && !bare.ends_with('+') {
                        return false;
                    }
                    !available.iter().any(|a| Self::bare(a) == bare)
                }
            })
            .collect()
    }

    /// Bindings the pad file needs that are NOT GameCube inputs.
    ///
    /// The analog trigger travel and the rumble motor are written into
    /// `GCPadNew.ini` alongside the twenty mapped controls, but they are not
    /// `GcInput`s, so for a long time they sat hardcoded in the generator as
    /// XInput names and were invisible to both the auto mapping and the
    /// guardrail. On a PlayStation 5 pad all three were dead, and the file
    /// still looked completely reasonable.
    ///
    /// MEASURED on a real DualSense 2026-09-12: the shoulders are `Bumper L`
    /// and `Bumper R`, and the motors are `Motor L` and `Motor R`. There is no
    /// `Shoulder L` and no `Motor` on that pad.
    pub fn extra_candidates(extra: Extra) -> &'static [&'static str] {
        match extra {
            Extra::AnalogTriggerL => &["Shoulder L", "Bumper L", "Left 1"],
            Extra::AnalogTriggerR => &["Shoulder R", "Bumper R", "Right 1"],
            // One motor, not an expression combining both. `Motor L` is
            // measured to exist and measured to resolve; a two motor
            // expression would be an assumption about the parser that nothing
            // here has checked.
            Extra::RumbleMotor => &["Motor", "Motor L"],
        }
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


/// One of the three non-GameCube bindings the pad file also carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Extra {
    AnalogTriggerL,
    AnalogTriggerR,
    RumbleMotor,
}

impl Extra {
    pub const ALL: &'static [Extra] = &[
        Extra::AnalogTriggerL,
        Extra::AnalogTriggerR,
        Extra::RumbleMotor,
    ];
}

/// The resolved non-GameCube bindings for one pad.
///
/// Defaults to exactly what the generator used to hardcode, so an Xbox pad and
/// every already saved profile keep the behaviour they had.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Extras {
    pub analog_trigger_l: String,
    pub analog_trigger_r: String,
    pub rumble_motor: String,
}

impl Default for Extras {
    /// Every vocabulary at once, for the same reason as
    /// [`Mapping::universal`]: these three are bound by name too, and were
    /// dead on a PlayStation pad for exactly the same reason the buttons
    /// were. An OR is safe for the motor because
    /// `BinaryExpression::SetValue` drives both sides and an unresolved
    /// output is a no-op.
    fn default() -> Self {
        Self {
            analog_trigger_l: Extras::universal(Extra::AnalogTriggerL),
            analog_trigger_r: Extras::universal(Extra::AnalogTriggerR),
            rumble_motor: Extras::universal(Extra::RumbleMotor),
        }
    }
}

impl Extras {
    /// Every candidate for this extra, OR'd, so it binds whichever the pad has.
    pub fn universal(extra: Extra) -> String {
        Mapping::extra_candidates(extra)
            .iter()
            .map(|name| Mapping::quote(name))
            .collect::<Vec<_>>()
            .join("|")
    }

    pub fn get(&self, extra: Extra) -> &str {
        match extra {
            Extra::AnalogTriggerL => &self.analog_trigger_l,
            Extra::AnalogTriggerR => &self.analog_trigger_r,
            Extra::RumbleMotor => &self.rumble_motor,
        }
    }

    fn set(&mut self, extra: Extra, value: String) {
        match extra {
            Extra::AnalogTriggerL => self.analog_trigger_l = value,
            Extra::AnalogTriggerR => self.analog_trigger_r = value,
            Extra::RumbleMotor => self.rumble_motor = value,
        }
    }

    /// Resolve all three from the names this device actually reports.
    ///
    /// Anything that cannot be resolved is left EMPTY, and an empty extra is
    /// not written to the file at all. A pad with no rumble output should get
    /// no `Rumble/Motor` line, not a line naming a motor it does not have:
    /// the second is indistinguishable from the first while playing, and
    /// indistinguishable from a bug while reading the file.
    pub fn from_device(available: &[String]) -> Self {
        let mut out = Self::default();
        for extra in Extra::ALL {
            let found = Mapping::extra_candidates(*extra)
                .iter()
                .find(|c| available.iter().any(|a| Mapping::bare(a) == **c));
            out.set(
                *extra,
                found.map(|name| Mapping::quote(name)).unwrap_or_default(),
            );
        }
        out
    }

    /// Which of the three name a control this device does not have.
    pub fn unresolvable(&self, available: &[String]) -> Vec<Extra> {
        if available.is_empty() {
            return Vec::new();
        }
        Extra::ALL
            .iter()
            .copied()
            .filter(|e| {
                let bare = Mapping::bare(self.get(*e));
                // Empty means "this pad has no such control and no line is
                // written", which is a resolved state, not a broken one.
                !bare.is_empty() && !available.iter().any(|a| Mapping::bare(a) == bare)
            })
            .collect()
    }
}

impl Default for Mapping {
    fn default() -> Self {
        Self::default_gamepad()
    }
}

/// Which button art the game shows.
///
/// A DISPLAY choice, deliberately separate from [`DeviceFamily`]. Someone
/// playing on an Xbox pad may still want GameCube icons, because the game is a
/// GameCube game and that is what its own tutorials show. Deriving this from the
/// pad would take that choice away, and would be wrong for anyone on an adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ControlsProfile {
    /// Xbox face buttons. The default, because it is what most players hold.
    #[default]
    Xbox,
    PlayStation,
    /// The game's own built-in art, and the game's own buttons.
    ///
    /// The one profile that applies NO pack, which is not a gap. The prompt
    /// textures are add-only: nothing on disk backs them up, because before a
    /// pack was applied there was no file there at all. So applying no pack does
    /// not restore anything, it uncovers what the disc always drew, and on a
    /// GameCube release that is GameCube buttons.
    ///
    /// It is also the profile that switches the disc's in-match X/B swap off
    /// for that player, so a GameCube controller plays as it did on the
    /// original game and in Reborn 2.1. See [`crate::gamecube_controls`].
    GameCube,
}

impl ControlsProfile {
    pub const ALL: &'static [ControlsProfile] = &[
        ControlsProfile::Xbox,
        ControlsProfile::PlayStation,
        ControlsProfile::GameCube,
    ];

    pub fn slug(self) -> &'static str {
        match self {
            ControlsProfile::Xbox => "xbox",
            ControlsProfile::PlayStation => "playstation",
            ControlsProfile::GameCube => "gamecube",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ControlsProfile::Xbox => "Xbox",
            ControlsProfile::PlayStation => "PlayStation",
            ControlsProfile::GameCube => "GameCube",
        }
    }

    /// One line saying what the choice looks like.
    pub fn describes(self) -> &'static str {
        match self {
            ControlsProfile::Xbox => "A, B, X and Y in the Xbox layout",
            ControlsProfile::PlayStation => "Cross, Circle, Square and Triangle",
            ControlsProfile::GameCube => "The game's own art and its original buttons, no X/B swap",
        }
    }

    /// The mod package this profile needs, if any.
    ///
    /// These are the ids the JSGME importer produces from the `Xbox Controls`
    /// and `PS Controls` folders, so they are what is actually on disk. An
    /// earlier version returned logical names like `xbox-prompts`, which
    /// resolved to nothing: the profile could be chosen and never applied.
    ///
    /// `None` for GameCube means apply no package. A caller that read `None` as
    /// "unknown" and fell back to one would override the single thing the player
    /// actually chose.
    ///
    /// Both packages write the same nineteen texture paths, so they are a hard
    /// conflict with each other and turning on two profiles at once is refused
    /// by [`crate::mods::detect_conflicts`] rather than by a rule here.
    pub fn package_id(self) -> Option<&'static str> {
        match self {
            ControlsProfile::Xbox => Some("jsgme.xbox-controls"),
            ControlsProfile::PlayStation => Some("jsgme.ps-controls"),
            ControlsProfile::GameCube => None,
        }
    }

    pub fn from_slug(slug: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|p| p.slug() == slug)
    }

    /// What to preselect for a pad, as a SUGGESTION only.
    ///
    /// Offered so the common case needs no thought, never applied without being
    /// shown. A player who picks something else has picked something else.
    pub fn suggested_for(family: DeviceFamily) -> Self {
        match family {
            DeviceFamily::PlayStation => ControlsProfile::PlayStation,
            DeviceFamily::Nintendo | DeviceFamily::GameCubeAdapter => ControlsProfile::GameCube,
            _ => ControlsProfile::Xbox,
        }
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
        } else if n.contains("nintendo")
            || n.contains("switch")
            || n.contains("joy-con")
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
    /// The analog trigger and rumble bindings, resolved for this device.
    ///
    /// Defaulted so every profile saved before this existed keeps the exact
    /// behaviour it had.
    #[serde(default)]
    pub extras: Extras,

    /// Bindings the user changed away from the default.
    ///
    /// Tracked so the remap screen can show what is customized and offer to
    /// reset just those, and so a future change to the shipping default reaches
    /// everything the user has not deliberately overridden.
    pub overridden: Vec<GcInput>,

    /// The player's own stick calibration, as Dolphin's Calibrate wizard wrote
    /// it, keyed by INI key (`Main Stick/Calibration`, `C-Stick/Calibration`).
    ///
    /// Empty means they never ran it, and the square gate is written instead.
    /// A pad whose sticks have been measured must never be re-squared: the
    /// hardcoded gate describes a perfect stick, and theirs is not one.
    #[serde(default)]
    pub calibration: BTreeMap<String, String>,

    /// Lines from the player's `GCPadNew.ini` this model cannot express, kept
    /// verbatim so regenerating the file does not silently delete them.
    ///
    /// Dolphin's own controller window writes plenty the launcher has no
    /// concept of: `Buttons/Threshold`, per-direction `Range`,
    /// `Main Stick/Modifier`, `Profile`. Before this, regenerating the file
    /// from the launcher's model deleted every one of them.
    #[serde(default)]
    pub carried: BTreeMap<String, String>,
}

impl ControllerProfile {
    pub fn new(identity: DeviceIdentity, port: u8) -> Self {
        Self {
            identity,
            port,
            // Not default_gamepad: that is XInput-only vocabulary and is
            // dead on a PlayStation pad. This binds every vocabulary at
            // once and is replaced with exact names by `repair_for` as
            // soon as the runtime can report them.
            mapping: Mapping::universal_gamepad(),
            tuning: Tuning::default(),
            extras: Extras::default(),
            overridden: Vec::new(),
            calibration: BTreeMap::new(),
            carried: BTreeMap::new(),
        }
    }

    /// Replace bindings this device cannot resolve with ones it can, and
    /// report what moved.
    ///
    /// A binding naming a control the device does not have is not a preference,
    /// it is a dead key. Dolphin says nothing about it: the file parses, the
    /// device line is right, and the button does nothing forever. So an
    /// unresolvable binding is repaired even when the player set it by hand,
    /// because leaving it would be honouring a choice that cannot work.
    ///
    /// A binding that DOES resolve is never touched, whatever it is. That is
    /// what keeps this from quietly rearranging a working pad.
    ///
    /// Returns the inputs that were repaired, so the caller can say so rather
    /// than changing the player's controls in silence.
    pub fn repair_for(&mut self, available: &[String]) -> Vec<GcInput> {
        if available.is_empty() {
            return Vec::new();
        }
        let mut needs: Vec<GcInput> = self.mapping.unresolvable(available);
        // An OR binding is a placeholder for "we did not know what this pad
        // calls its controls". Now that we do, replace it with the exact
        // name, so the file a person reads says `Cross` rather than every
        // vocabulary at once.
        for input in GcInput::ALL {
            if self
                .mapping
                .get(*input)
                .is_some_and(|b| b.contains('|'))
                && !needs.contains(input)
            {
                needs.push(*input);
            }
        }
        for input in self.mapping.unbound() {
            if !needs.contains(&input) {
                needs.push(input);
            }
        }
        if needs.is_empty() {
            // Still not a no-op: the extras can be dead even when all twenty
            // mapped controls resolve, which is exactly the PlayStation 5 case.
            if !self.extras.unresolvable(available).is_empty()
                || Extra::ALL.iter().any(|e| self.extras.get(*e).contains('|'))
            {
                self.extras = Extras::from_device(available);
            }
            return Vec::new();
        }

        // The analog triggers and the motor are bound by name too, and were
        // dead on a PlayStation 5 pad for the same reason. Repaired on the
        // same terms: only when the current name does not resolve.
        if !self.extras.unresolvable(available).is_empty()
            || Extra::ALL.iter().any(|e| self.extras.get(*e).contains('|'))
        {
            self.extras = Extras::from_device(available);
        }

        let (workable, _) = Mapping::auto_from(available);
        let mut repaired = Vec::new();
        for input in needs {
            if let Some(binding) = workable.get(input) {
                self.mapping.set(input, binding.to_string());
                // No longer the player's choice, because their choice was a
                // control this device does not have.
                self.overridden.retain(|i| *i != input);
                repaired.push(input);
            }
        }
        repaired.sort();
        repaired
    }

    /// A profile for a pad the launcher has never seen, built from the names
    /// that pad actually reports.
    ///
    /// The plain `new` carries the XInput default, which is correct for an Xbox
    /// pad and dead for a PlayStation one. This is what should be used whenever
    /// the device's real input list is available.
    pub fn from_device(identity: DeviceIdentity, port: u8, available: &[String]) -> Self {
        let mut profile = Self::new(identity, port);
        profile.repair_for(available);
        profile
    }

    /// Widen every binding that is the launcher's own vocabulary back to the
    /// universal binding, and say which.
    ///
    /// A saved profile can carry the XInput-only default in two ways: a build
    /// before 3.0.6 wrote it, or the first 3.0.6 launch adopted it out of the
    /// old pad file as twenty "overrides". On a PlayStation pad it is dead,
    /// and on a runtime that cannot list the pad's controls nothing can prove
    /// it so, which is why [`ControllerProfile::repair_for`] alone never
    /// reached those players. Widening needs no proof: the universal binding
    /// is a superset, an unresolvable name is silently zero, and an Xbox pad
    /// loses nothing. When the runtime CAN list controls, `repair_for` narrows
    /// it straight back to the exact name.
    ///
    /// A binding that names anything outside its input's own candidates is
    /// the player's, and is never touched.
    pub fn widen_stale_defaults(&mut self) -> Vec<GcInput> {
        let mut widened = Vec::new();
        for input in GcInput::ALL {
            let universal = Mapping::universal(*input);
            let Some(current) = self.mapping.get(*input).map(str::to_owned) else { continue };
            if !Mapping::is_own_vocabulary(*input, &current) {
                continue;
            }
            // Not a choice, so not an override, whether or not it needs
            // widening: a stick axis has one name in every vocabulary.
            self.overridden.retain(|i| i != input);
            if current != universal {
                self.mapping.set(*input, universal);
                widened.push(*input);
            }
        }
        for extra in Extra::ALL {
            let universal = Extras::universal(*extra);
            let current = self.extras.get(*extra).to_owned();
            if !current.is_empty()
                && current != universal
                && Mapping::extra_is_own_vocabulary(*extra, &current)
            {
                self.extras.set(*extra, universal);
            }
        }
        widened
    }

    /// The controls that cannot be pressed on a device reporting `available`.
    ///
    /// # Why this exists beside [`Mapping::unresolvable`]
    ///
    /// Every controller failure this project has shipped was SILENT: the file
    /// parses, the Device line is right, and nothing happens. The guardrail
    /// that was meant to catch it skips any binding containing `|`, and since
    /// 3.0.6 every default binding IS an OR, so for the pads that most need it
    /// the guardrail has been looking away. A pad Windows has no gamepad
    /// mapping for (a USB GameCube-style pad, a generic HID pad) reports
    /// `Button 3` and `Axis 1-`; nothing in the universal binding exists on
    /// it, its sticks have never worked, and the Controllers page told its
    /// owner "Automatic mapping is ready".
    ///
    /// This one sees through the OR: a binding made only of control names is
    /// alive when ANY of those names exists on the device, because `|` is a
    /// maximum and an unresolvable name is silently zero. The result is the
    /// exact list the player has to press, which is the only honest way to
    /// set such a pad up: `Button 3` means nothing until it is measured.
    ///
    /// # What it will not say
    ///
    /// An empty `available` means the device could not be asked, and no
    /// evidence is not an accusation. A binding using anything other than
    /// names joined by `|` is a hand written expression; this is not an
    /// expression evaluator and must not veto what it cannot read.
    pub fn dead_controls(&self, available: &[String]) -> Vec<GcInput> {
        if available.is_empty() {
            return Vec::new();
        }
        let exists = |name: &str| available.iter().any(|a| Mapping::bare(a) == name);
        GcInput::ALL
            .iter()
            .copied()
            .filter(|input| {
                let Some(binding) = self.mapping.get(*input).map(str::trim).filter(|b| !b.is_empty()) else {
                    return true;
                };
                let mut names = Vec::new();
                for part in binding.split('|') {
                    let part = part.trim();
                    let quoted = part.len() >= 2 && part.starts_with('`') && part.ends_with('`');
                    let name = if quoted { &part[1..part.len() - 1] } else { part };
                    // Quoted, a name may hold anything but a backtick. Bare,
                    // anything the parser reads as an operator makes this an
                    // expression, and that is not ours to judge. An axis name
                    // ends in its sign (`Left Y+`), which is part of the name.
                    let readable = if quoted {
                        !name.is_empty() && !name.contains('`')
                    } else {
                        let stem = name.strip_suffix(['+', '-']).unwrap_or(name);
                        !stem.is_empty() && !stem.contains(['&', '!', '+', '-', '^', '(', ')', '`', ',', '*', '/', '<', '>', '='])
                    };
                    if !readable {
                        return false;
                    }
                    names.push(name);
                }
                !names.iter().any(|name| exists(name))
            })
            .collect()
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
        // The universal binding, the same one `new` starts from, not the
        // XInput-only default: "Reset to defaults" on a PlayStation pad must
        // not put twenty dead bindings back.
        let defaults = Mapping::universal_gamepad();
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
            self.extras = Extras::default();
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
        let lower: Vec<String> = device_names
            .iter()
            .map(|n| n.to_ascii_lowercase())
            .collect();

        if lower
            .iter()
            .any(|n| n.contains("steam virtual") || n.contains("steam controller"))
        {
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
/// One `[GCPadN]` section as key/value pairs, in file order.
type PadSection = BTreeMap<String, String>;

/// Split a `GCPadNew.ini` into its `[GCPadN]` sections.
///
/// Deliberately forgiving: anything unparsed is skipped rather than refused,
/// because failing to read a file the player hand-edited would be worse than
/// not understanding part of it.
pub fn parse_gcpad(text: &str) -> BTreeMap<u8, PadSection> {
    let mut out: BTreeMap<u8, PadSection> = BTreeMap::new();
    let mut port: Option<u8> = None;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') && line.ends_with(']') {
            port = line[1..line.len() - 1]
                .strip_prefix("GCPad")
                .and_then(|n| n.parse::<u8>().ok())
                .filter(|n| (1..=4).contains(n));
            if let Some(port) = port {
                out.entry(port).or_default();
            }
            continue;
        }
        if line.is_empty() || line.starts_with([';', '#']) {
            continue;
        }
        let (Some(port), Some((key, value))) = (port, line.split_once('=')) else { continue };
        out.entry(port)
            .or_default()
            .insert(key.trim().to_string(), value.trim().to_string());
    }
    out
}

/// Fold everything the player changed in Dolphin's own Controller Settings
/// back into the launcher's profiles.
///
/// # Why this is not just "stop rewriting the file"
///
/// The launcher regenerates `GCPadNew.ini` on every launch for a real reason,
/// stated where it happens: a remembered device string that no longer matches
/// anything plugged in binds nothing, and Dolphin says nothing about it. The
/// file parses, the section looks right, and the pad is silently dead. So the
/// launcher keeps owning the `Device` line and the port layout.
///
/// What it stops owning is the bindings. `last_written` is the exact text the
/// launcher wrote last time, kept beside the file; anything in `live` that
/// differs from it was put there by somebody else, and is adopted rather than
/// discarded. A key the launcher's model cannot express is carried verbatim.
///
/// Returns a description of what was adopted, per port, so the launcher can
/// say so in the log rather than changing the player's controls in silence.
/// The pad file to leave behind when a launch sees NO pad, or `None` when the
/// one already on disk needs nothing.
///
/// # Why a launch with no pad writes anything at all
///
/// "No pad" is what the scan said, and the scan can be wrong. The runtime
/// reports WGInput devices from a Windows callback that can land after the
/// one-shot device list has been read: measured 2026-09-18, a connected pad
/// was missing from 8 of 52 scans, on a fast machine with a wired pad. It is
/// also simply true whenever a Bluetooth pad is switched on after Play.
///
/// The launcher used to write nothing in that case, which left whatever was
/// already there in charge, and for a new player that is the seeded file:
/// `Device = XInput/0/Gamepad` with Xbox-only names. A PlayStation pad lives
/// on WGInput and nowhere else, so it was dead for the whole session while the
/// Controllers page test, which asks the browser and not Dolphin, said it was
/// fine.
///
/// The runtime now binds player one to whichever pad is really connected when
/// the named one is absent. That only helps if the bindings can be pressed on
/// that pad, so every binding that is the launcher's own vocabulary is widened
/// to the universal one. See [`Mapping::universal`] for why that is safe.
///
/// # What is never touched
///
/// A binding that names anything outside its input's own candidates is the
/// player's, exactly as [`Mapping::is_own_vocabulary`] rules for adoption: a
/// keyboard layout, a remap, a pad set up by hand in Dolphin. So is every
/// line that is not a binding. The file is edited in place, line by line,
/// rather than regenerated, because with no pad in sight there is no profile
/// to regenerate it from.
pub fn standing_pad_file(live: &str) -> Option<String> {
    const DEVICE: &str = "XInput/0/Gamepad";
    let extra_keys = [
        ("Triggers/L-Analog", Extra::AnalogTriggerL),
        ("Triggers/R-Analog", Extra::AnalogTriggerR),
        ("Rumble/Motor", Extra::RumbleMotor),
    ];

    let sections = parse_gcpad(live);
    let player_one_is_bound = sections
        .get(&1)
        .is_some_and(|s| GcInput::ALL.iter().any(|i| s.get(i.ini_key()).is_some_and(|v| !v.is_empty())));

    // The bindings a player one with none of their own gets.
    let fresh = |with_header: bool, with_device: bool| -> String {
        let identity = DeviceIdentity {
            guid: "XInput:0".into(),
            serial: None,
            name: "Gamepad".into(),
            family: DeviceFamily::Xbox,
        };
        let profile = ControllerProfile::new(identity, 1);
        GcPadConfig::generate(std::slice::from_ref(&profile), |_| DEVICE.to_string())
            .lines()
            .filter(|line| with_header || !line.starts_with('['))
            .filter(|line| with_device || !line.starts_with("Device ="))
            .map(|line| format!("{line}\n"))
            .collect()
    };

    if !sections.contains_key(&1) {
        return Some(format!("{}{live}", fresh(true, true)));
    }

    let mut out = String::with_capacity(live.len() + 512);
    let mut changed = false;
    let mut in_pad = false;
    for raw in live.split_inclusive('\n') {
        let body = raw.trim_end_matches(['\r', '\n']);
        let ending = &raw[body.len()..];
        let line = body.trim();

        if line.starts_with('[') && line.ends_with(']') {
            let port = line[1..line.len() - 1]
                .strip_prefix("GCPad")
                .and_then(|n| n.parse::<u8>().ok())
                .filter(|n| (1..=4).contains(n));
            in_pad = port.is_some();
            out.push_str(raw);
            // A player-one section with no bindings in it binds nothing.
            if port == Some(1) && !player_one_is_bound {
                if ending.is_empty() {
                    out.push('\n');
                }
                let has_device = sections.get(&1).is_some_and(|s| s.get("Device").is_some_and(|d| !d.is_empty()));
                out.push_str(&fresh(false, !has_device));
                changed = true;
            }
            continue;
        }

        let widened = (|| {
            if !in_pad || line.starts_with([';', '#']) {
                return None;
            }
            let (key, value) = line.split_once('=')?;
            let (key, value) = (key.trim(), value.trim());
            if let Some(input) = GcInput::ALL.iter().find(|i| i.ini_key() == key) {
                let universal = Mapping::universal(*input);
                return (Mapping::is_own_vocabulary(*input, value) && value != universal)
                    .then(|| format!("{key} = {universal}"));
            }
            let (_, extra) = extra_keys.iter().find(|(k, _)| *k == key)?;
            // `<binding> <strength>` is the old broken rumble form; the number
            // was never legal there and is dropped with the rewrite.
            let binding = match value.rsplit_once(' ') {
                Some((binding, tail)) if parse_percent(tail).is_some() => binding,
                _ => value,
            };
            let universal = Extras::universal(*extra);
            (Mapping::extra_is_own_vocabulary(*extra, binding) && value != universal)
                .then(|| format!("{key} = {universal}"))
        })();

        match widened {
            Some(line) => {
                out.push_str(&line);
                out.push_str(ending);
                changed = true;
            }
            None => out.push_str(raw),
        }
    }

    changed.then_some(out)
}

pub fn adopt_external_edits(
    live: &str,
    last_written: &str,
    profiles: &mut [ControllerProfile],
    device_of: impl Fn(&DeviceIdentity) -> Option<String>,
) -> Vec<String> {
    let live = parse_gcpad(live);
    let ours = parse_gcpad(last_written);
    let mut adopted = Vec::new();

    for profile in profiles.iter_mut() {
        // Bindings and the device they were made for travel together or not
        // at all.
        //
        // This used to match a section to a profile by PORT alone and skip
        // the Device line, so whatever was bound on port 1 was adopted into
        // whichever pad held port 1 now. Control names are per backend and do
        // not overlap: a 2.1 player's DirectInput setup (`Button 1`,
        // `Axis Y-`) landed under the WGInput device line the launcher gives
        // that pad, where `Button 1` happens to exist and `Axis Y-` does not.
        // Reported 2026-09-18 as "only the buttons work, it won't read the
        // stick". The same hole carried an Xbox pad's remap onto a
        // PlayStation pad sharing its port.
        //
        // `device_of` answers what this pad is called THIS session, and only
        // the section naming exactly that device is this pad's. A pad that is
        // not connected has no section that can be its own.
        let Some(device) = device_of(&profile.identity) else { continue };
        let names_device = |s: &PadSection| s.get("Device").is_some_and(|d| *d == device);
        let Some((port, section)) = live.iter().find(|(_, s)| names_device(s)) else { continue };
        // What we wrote last time only counts as a baseline for the same pad.
        let previous = ours.get(port).filter(|s| names_device(s));
        let port = *port;

        for (key, value) in section {
            // The launcher owns which device is in which port. Everything else
            // in the section is up for adoption.
            if key == "Device" {
                continue;
            }
            // Unchanged since we wrote it means nobody touched it.
            if previous.and_then(|p| p.get(key)).is_some_and(|ours| ours == value) {
                continue;
            }

            if let Some(input) = GcInput::ALL.iter().find(|i| i.ini_key() == key) {
                // The launcher's own vocabulary is not a choice. A value that
                // names nothing but this input's own candidates is what some
                // build of the launcher wrote: this one, or the XInput-only
                // default every build before 3.0.6 wrote. There was no
                // sidecar before 3.0.6 either, so on the first launch after
                // upgrading, adopting it took the dead file a PlayStation
                // player already had for twenty deliberate remaps, wrote it
                // straight back out, and saved it as theirs. The universal
                // binding already says everything such a value says.
                if Mapping::is_own_vocabulary(*input, value) {
                    continue;
                }
                profile.rebind(*input, value.clone());
                adopted.push(format!("port {}: {key} = {value}", port));
            } else if key == "Main Stick/Calibration" || key == "C-Stick/Calibration" {
                profile.calibration.insert(key.clone(), value.clone());
                adopted.push(format!("port {}: {key} (their own measurement)", port));
            } else if key == "Main Stick/Dead Zone" || key == "C-Stick/Dead Zone" {
                if let Some(percent) = parse_percent(value) {
                    profile.tuning.deadzone_percent = percent;
                    adopted.push(format!("port {}: dead zone {percent}", port));
                }
            } else if key == "Triggers/Threshold" {
                if let Some(percent) = parse_percent(value) {
                    profile.tuning.trigger_threshold_percent = percent;
                    adopted.push(format!("port {}: trigger threshold {percent}", port));
                }
            } else if key == "Rumble/Motor" {
                // `<binding> <strength>` was the old broken form: the
                // strength is the trailing token only when it is a number. A
                // motor's own name can contain a space (`Motor L`), so the
                // split is not taken on faith.
                let (binding, strength) = match value.rsplit_once(' ') {
                    Some((binding, tail)) if parse_percent(tail).is_some() => {
                        (binding, parse_percent(tail))
                    }
                    _ => (value.as_str(), None),
                };
                let mut changed = false;
                if let Some(percent) = strength.filter(|p| *p != profile.tuning.rumble_percent) {
                    profile.tuning.rumble_percent = percent;
                    changed = true;
                }
                if !binding.is_empty()
                    && !Mapping::extra_is_own_vocabulary(Extra::RumbleMotor, binding)
                    && profile.extras.get(Extra::RumbleMotor) != binding
                {
                    profile.extras.set(Extra::RumbleMotor, binding.to_string());
                    changed = true;
                }
                if changed {
                    adopted.push(format!("port {}: rumble", port));
                }
            } else if key == "Triggers/L-Analog" {
                if !Mapping::extra_is_own_vocabulary(Extra::AnalogTriggerL, value) {
                    profile.extras.set(Extra::AnalogTriggerL, value.clone());
                    adopted.push(format!("port {}: {key}", port));
                }
            } else if key == "Triggers/R-Analog" {
                if !Mapping::extra_is_own_vocabulary(Extra::AnalogTriggerR, value) {
                    profile.extras.set(Extra::AnalogTriggerR, value.clone());
                    adopted.push(format!("port {}: {key}", port));
                }
            } else {
                // Dolphin's window writes plenty this model has no concept of.
                // Keep it exactly, so regenerating does not delete it.
                profile.carried.insert(key.clone(), value.clone());
                adopted.push(format!("port {}: {key} (carried)", port));
            }
        }
    }

    adopted
}

fn parse_percent(value: &str) -> Option<u8> {
    let number: f64 = value.trim().parse().ok()?;
    if !(0.0..=255.0).contains(&number) {
        return None;
    }
    Some(number.round() as u8)
}

#[derive(Debug)]
pub struct GcPadConfig;

impl GcPadConfig {
    /// Generate the whole file for up to four players.
    ///
    /// Regenerated on every launch from the launcher's own model, so a change
    /// made inside Dolphin does not persist and the Settings page stays the
    /// single source of truth.
    pub fn generate(
        profiles: &[ControllerProfile],
        device_string: impl Fn(&DeviceIdentity) -> String,
    ) -> String {
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
            // The analog trigger axes. The digital Triggers/L and R above are
            // the click; these are the travel, and without them a trigger held
            // part way reads as nothing. Present in the working install and
            // absent here until the two were compared.
            // An empty extra means this pad has no such control. Write
            // nothing rather than a line that cannot resolve.
            for (key, extra) in [
                ("Triggers/L-Analog", Extra::AnalogTriggerL),
                ("Triggers/R-Analog", Extra::AnalogTriggerR),
            ] {
                let binding = profile.extras.get(extra);
                if !binding.is_empty() {
                    out.push_str(&format!("{key} = {binding}\n"));
                }
            }

            // Square gate calibration, the values Dolphin writes itself.
            // 141.42 is the diagonal, 100 * sqrt(2). Without these the stick
            // is treated as a circle and the corners cannot be reached.
            //
            // A player who has run Dolphin's Calibrate wizard has measured
            // their actual stick, and that measurement replaces the gate. The
            // gate describes a perfect stick; theirs is not one, and writing
            // the gate back over a real calibration every launch undid the
            // work and made the pad feel wrong again.
            const GATE: &str = "100.00 141.42 100.00 141.42 100.00 141.42 100.00 141.42";
            for key in ["Main Stick/Calibration", "C-Stick/Calibration"] {
                let value = profile.calibration.get(key).map(String::as_str).unwrap_or(GATE);
                out.push_str(&format!("{key} = {value}\n"));
            }

            out.push_str(&format!("Main Stick/Dead Zone = {dz:.2}\n"));
            out.push_str(&format!("C-Stick/Dead Zone = {dz:.2}\n"));
            out.push_str(&format!(
                "Triggers/Threshold = {:.2}\n",
                f64::from(profile.tuning.trigger_threshold_percent)
            ));
            // The strength is its OWN key, not a number after the
            // expression.
            //
            // ControlGroup::LoadConfig reads the strength from
            // `<group><name>/Range` and reads everything on the binding
            // line as the expression. And the expression parser ends with
            //
            //   if (Peek().type == TOK_EOF) return result;
            //   return ParseResult::MakeErrorResult(Peek(), "Expected end
            //                                              of expression.");
            //
            // so a trailing number is a SYNTAX ERROR and the whole binding
            // is discarded, in silence, exactly the way an unresolvable
            // control name is. `Rumble/Motor = `Motor` 100.00` has been
            // written since 2.1 and has therefore never rumbled anything,
            // on any pad, including an Xbox one.
            let motor = profile.extras.get(Extra::RumbleMotor);
            if !motor.is_empty() {
                out.push_str(&format!("Rumble/Motor = {motor}\n"));
                out.push_str(&format!(
                    "Rumble/Motor/Range = {:.2}\n",
                    f64::from(profile.tuning.rumble_percent)
                ));
            }
            // Everything Dolphin's own window wrote that this model has no
            // concept of, replayed exactly as it was found.
            for (key, value) in &profile.carried {
                out.push_str(&format!("{key} = {value}\n"));
            }
            out.push('\n');
        }
        out
    }
}

#[cfg(test)]
mod external_edit_tests {
    use super::*;

    fn identity(name: &str) -> DeviceIdentity {
        DeviceIdentity {
            guid: "030000004c050000cc09000000006800".into(),
            serial: None,
            name: name.into(),
            family: DeviceFamily::from_name(name),
        }
    }

    /// The complaint, as a test: a player opens Dolphin's own Controller
    /// Settings, remaps a button, tightens the dead zone, calibrates the stick,
    /// and sets two things the launcher has never heard of. All of it must be
    /// there after the next launch.
    #[test]
    fn a_dolphin_remap_survives_the_next_launch() {
        let mut profile = ControllerProfile::new(identity("Xbox Controller"), 1);
        let ours = GcPadConfig::generate(std::slice::from_ref(&profile), |_| "XInput/0/Gamepad".into());

        // What the player did in Dolphin, on top of what we wrote. Replaced a
        // whole line at a time: a binding is a multi-name string like
        // "`Button A`|`Cross`|`Button S`", so matching a prefix would edit the
        // middle of one.
        let theirs: String = ours
            .lines()
            .flat_map(|line| {
                let replacement = match line.split_once(" = ").map(|(key, _)| key) {
                    Some("Buttons/A") => "Buttons/A = `Button B`",
                    Some("Main Stick/Dead Zone") => "Main Stick/Dead Zone = 5.00",
                    Some("Main Stick/Calibration") => {
                        "Main Stick/Calibration = 97.31 132.08 99.40 138.77 101.22 140.01 98.55 131.60"
                    }
                    Some("Triggers/Threshold") => {
                        "Triggers/Threshold = 60.00\nButtons/Threshold = 40.00\nMain Stick/Modifier/Range = 50.0"
                    }
                    _ => line,
                };
                [replacement, "\n"]
            })
            .collect();

        let mut profiles = [profile.clone()];
        let adopted = adopt_external_edits(&theirs, &ours, &mut profiles, |_| Some("XInput/0/Gamepad".into()));
        assert!(!adopted.is_empty(), "the player changed things; we must notice");
        profile = profiles.into_iter().next().unwrap();

        assert_eq!(profile.mapping.get(GcInput::ButtonA), Some("`Button B`"), "their remap");
        assert!(profile.overridden.contains(&GcInput::ButtonA), "recorded as their choice");
        assert_eq!(profile.tuning.deadzone_percent, 5, "their dead zone");
        assert_eq!(profile.tuning.trigger_threshold_percent, 60, "their threshold");
        assert_eq!(
            profile.calibration.get("Main Stick/Calibration").map(String::as_str),
            Some("97.31 132.08 99.40 138.77 101.22 140.01 98.55 131.60"),
            "their own stick measurement",
        );
        assert_eq!(profile.carried.get("Buttons/Threshold").map(String::as_str), Some("40.00"));
        assert_eq!(profile.carried.get("Main Stick/Modifier/Range").map(String::as_str), Some("50.0"));

        // And it all comes back out on the next launch.
        let next = GcPadConfig::generate(std::slice::from_ref(&profile), |_| "XInput/0/Gamepad".into());
        assert!(next.contains("Buttons/A = `Button B`"), "{next}");
        assert!(next.contains("Main Stick/Dead Zone = 5.00"), "{next}");
        assert!(next.contains("Triggers/Threshold = 60.00"), "{next}");
        assert!(next.contains("Buttons/Threshold = 40.00"), "{next}");
        assert!(next.contains("Main Stick/Modifier/Range = 50.0"), "{next}");
        assert!(
            next.contains("Main Stick/Calibration = 97.31 132.08"),
            "a measured stick must never be re-squared:\n{next}"
        );
        // Still ours: the device line and the port layout.
        assert!(next.contains("Device = XInput/0/Gamepad"), "{next}");
    }

    /// Adoption is driven by what WE wrote, not by what differs from the
    /// shipping default. A file we wrote and nobody touched adopts nothing.
    #[test]
    fn a_file_nobody_touched_adopts_nothing() {
        let profile = ControllerProfile::new(identity("Xbox Controller"), 1);
        let ours = GcPadConfig::generate(std::slice::from_ref(&profile), |_| "XInput/0/Gamepad".into());
        let mut profiles = [profile.clone()];
        assert!(adopt_external_edits(&ours, &ours, &mut profiles, |_| Some("XInput/0/Gamepad".into())).is_empty());
        assert_eq!(profiles[0], profile, "nothing about the profile changed");
    }

    /// A pad the launcher never wrote a section for, whose bindings the player
    /// set up entirely in Dolphin, is adopted rather than replaced.
    ///
    /// The player here swapped A and B by hand. `Circle` is not one of the
    /// names the launcher would ever put on A, so it is unmistakably theirs.
    #[test]
    fn a_port_we_never_wrote_is_adopted_whole() {
        let mut profiles = [ControllerProfile::new(identity("Wireless Controller"), 2)];
        let theirs = "[GCPad2]\nDevice = WGInput/1/Controller\nButtons/A = Circle\nButtons/B = Cross\nMain Stick/Dead Zone = 12.00\n";
        let adopted = adopt_external_edits(theirs, "", &mut profiles, |_| Some("WGInput/1/Controller".into()));
        assert!(!adopted.is_empty());
        assert_eq!(profiles[0].mapping.get(GcInput::ButtonA), Some("Circle"));
        assert_eq!(profiles[0].mapping.get(GcInput::ButtonB), Some("Cross"));
        assert_eq!(profiles[0].tuning.deadzone_percent, 12);
    }

    /// Exactly the file a launcher before 3.0.6 left on a PlayStation
    /// player's disk: the XInput default on a WGInput device, the hardcoded
    /// XInput extras, and the rumble line in its old broken form. Dead, every
    /// line of it, which is the bug 3.0.6 was meant to end.
    fn what_3_0_5_wrote(device: &str) -> String {
        let mut old = ControllerProfile::new(identity("Wireless Controller"), 1);
        old.mapping = Mapping::default_gamepad();
        old.extras = Extras {
            analog_trigger_l: "`Shoulder L`".into(),
            analog_trigger_r: "`Shoulder R`".into(),
            rumble_motor: "`Motor`".into(),
        };
        GcPadConfig::generate(std::slice::from_ref(&old), |_| device.into()).replace(
            "Rumble/Motor = `Motor`\nRumble/Motor/Range = 100.00\n",
            "Rumble/Motor = `Motor` 100.00\n",
        )
    }

    /// The Discord complaint after 3.0.6 and 3.0.7 shipped, as a test.
    ///
    /// A PlayStation player upgrades. The pad file on disk is the one the OLD
    /// launcher wrote, and there is no sidecar, because the sidecar did not
    /// exist before 3.0.6. With nothing to compare against, adoption took
    /// every line of that dead file as the player's own choice, wrote it
    /// straight back out, and saved it as twenty overrides. The universal
    /// binding never reached the file, on any install that had the bug.
    ///
    /// The previous launcher's own vocabulary is not a choice. A value that
    /// names nothing but this input's own candidates carries no information
    /// the universal binding does not already carry, and must be left alone.
    #[test]
    fn the_previous_launchers_dead_defaults_are_not_adopted_as_the_players_choice() {
        let profile = ControllerProfile::new(identity("Wireless Controller"), 1);
        let left_behind = what_3_0_5_wrote("WGInput/0/Wireless Controller");
        assert!(left_behind.contains("Buttons/A = `Button A`"), "{left_behind}");

        let mut profiles = [profile.clone()];
        adopt_external_edits(&left_behind, "", &mut profiles, |_| Some("WGInput/0/Wireless Controller".into()));

        assert_eq!(
            profiles[0].mapping, profile.mapping,
            "the old launcher's XInput-only vocabulary was adopted over the universal binding"
        );
        assert!(
            profiles[0].overridden.is_empty(),
            "nothing in that file was the player's choice: {:?}",
            profiles[0].overridden
        );
        assert_eq!(
            profiles[0].extras, profile.extras,
            "the old hardcoded XInput extras were adopted over the universal ones"
        );
    }

    /// The positive control for the test above: with no sidecar, a change
    /// the player really made in that same old file is still adopted.
    #[test]
    fn a_real_remap_in_a_file_with_no_sidecar_is_still_adopted() {
        let theirs = what_3_0_5_wrote("WGInput/0/Wireless Controller")
            .replace("Buttons/A = `Button A`", "Buttons/A = `Circle`")
            .replace("Main Stick/Dead Zone = 15.00", "Main Stick/Dead Zone = 5.00");
        let mut profiles = [ControllerProfile::new(identity("Wireless Controller"), 1)];
        adopt_external_edits(&theirs, "", &mut profiles, |_| Some("WGInput/0/Wireless Controller".into()));
        assert_eq!(profiles[0].mapping.get(GcInput::ButtonA), Some("`Circle`"), "their remap");
        assert_eq!(profiles[0].overridden, vec![GcInput::ButtonA], "only that one was theirs");
        assert_eq!(profiles[0].tuning.deadzone_percent, 5, "their dead zone");
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

    /// The launcher must NOT swap B and X.
    ///
    /// An earlier version did, and this test asserted it. The swap is done
    /// inside the game by a match-gated DOL cave, and the note recording that
    /// work states it was done "so the Dolphin controller remap can be turned
    /// off". Doing it here as well cancels it out during a match and wrongly
    /// swaps in the menus, which stay vanilla.
    ///
    /// Ground truth is the working install's own GCPadNew.ini, which binds
    /// Button B to B and Button X to X.
    #[test]
    fn b_and_x_are_not_swapped_because_the_game_does_it() {
        let m = Mapping::default_gamepad();
        assert_eq!(m.get(GcInput::ButtonB), Some("`Button B`"));
        assert_eq!(m.get(GcInput::ButtonX), Some("`Button X`"));
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
        assert!(
            !key.contains('1'),
            "a port number must not appear in the key: {key}"
        );
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
        // Back to what `new` starts from, which binds every pad, not the
        // XInput-only name that is dead on a PlayStation pad.
        assert_eq!(p.mapping.get(GcInput::ButtonZ), Some(Mapping::universal(GcInput::ButtonZ).as_str()));
        assert!(!p.is_customized());
    }

    #[test]
    fn resetting_everything_also_restores_tuning() {
        let mut p = ControllerProfile::new(identity("Xbox Wireless Controller"), 1);
        p.rebind(GcInput::ButtonA, "`Button Y`");
        p.tuning.deadzone_percent = 40;
        p.extras.rumble_motor = String::new();
        assert!(p.is_customized());

        p.reset(&[]);
        assert_eq!(p.mapping, Mapping::universal_gamepad());
        assert_eq!(p.tuning, Tuning::default());
        assert_eq!(p.extras, Extras::default());
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
        assert!(
            ini.contains("Buttons/B = `Button B`"),
            "the game swaps B and X itself; the config must not:
{ini}"
        );
        // Without the analog axes a partly held trigger reads as nothing.
        assert!(ini.contains("Triggers/L-Analog"), "{ini}");
        assert!(ini.contains("Triggers/R-Analog"), "{ini}");
        // Without calibration the stick is a circle and the corners are lost.
        assert!(
            ini.contains("Main Stick/Calibration = 100.00 141.42"),
            "{ini}"
        );
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
