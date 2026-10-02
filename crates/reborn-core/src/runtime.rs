//! The Reborn Dolphin runtime: packaging, configuration, and launch.
//!
//! # Configuration is generated, never edited
//!
//! The launcher writes `Dolphin.ini`, `GFX.ini` and `GCPadNew.ini` from its own
//! settings model on every launch. It never reads them back and never edits
//! them in place.
//!
//! The reason is that the user keeps the ability to open Reborn Dolphin
//! directly and change things there. Without generation, the launcher's
//! displayed settings and the emulator's actual settings drift apart silently,
//! and the user is shown a lie. Generation makes the Settings page the single
//! source of truth, at the cost that a change made inside Dolphin does not
//! survive the next launch. That tradeoff needs saying once in the UI, on the
//! Open Reborn Dolphin action.
//!
//! # Packaging strips the things that must never ship
//!
//! Two marker files in the selected runtime's `User\` directory change its
//! behavior, and both are present right now:
//!
//! - `nsr_track_offline.txt` disables the netplay-only gate on stat capture.
//!   The runtime folder's own README says that shipping it means every
//!   player's offline game against the CPU is captured and uploaded, and that
//!   a game using league teams can land on a real fixture.
//! - `nsr_debug.txt` prints gate state on screen.
//!
//! [`RuntimePackager::forbidden`] enumerates them and
//! [`RuntimePackager::audit`] fails a package that contains one. The stronger
//! fix, making the gate a compiled-in setting so a dropped text file cannot
//! defeat it, belongs in the fork and is tracked in the Dolphin integration
//! plan. This is the belt to that pair of braces.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::ini::Overlay;
use crate::platform::{Error, ExitKind, Platform, ProcessHandle, Result};

/// Which game build to boot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameBuild {
    /// Play Now. Customizable, custom music allowed.
    Offline,
    /// Matchmaking. Standardized, verified before every match.
    Online,
}

impl GameBuild {
    pub fn install_root(self) -> &'static str {
        match self {
            GameBuild::Offline => "library/offline",
            GameBuild::Online => "library/online",
        }
    }

    /// Whether the user may modify this build.
    ///
    /// Mirrors `customizable` in the component manifest. Music is offline only,
    /// which is what lets matchmaking trust the online image.
    pub fn is_customizable(self) -> bool {
        matches!(self, GameBuild::Offline)
    }

    /// The disc id the emulator files this build's textures under.
    ///
    /// `Load/Textures/<this>` is where Dolphin looks, and `library/textures/
    /// <this>` is where the pristine tree lives, so the two are joined by
    /// this one string. The offline and online images carry different ids
    /// even though most of their textures are byte-identical.
    pub fn game_id(self) -> &'static str {
        match self {
            GameBuild::Offline => "GN7E69",
            GameBuild::Online => "FR4N31",
        }
    }
}

/// Graphics presets offered in onboarding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphicsPreset {
    Performance,
    Balanced,
    Quality,
    Ultra,
}

impl GraphicsPreset {
    /// Internal resolution multiplier.
    pub fn internal_resolution(self) -> u8 {
        3
    }

    /// ReShade is Windows only, and the Performance preset turns it off.
    pub fn reshade_enabled(self) -> bool {
        cfg!(windows) && !matches!(self, GraphicsPreset::Performance)
    }

    pub fn label(self) -> &'static str {
        match self {
            GraphicsPreset::Performance => "ReShade Off",
            GraphicsPreset::Balanced => "ReShade Standard",
            GraphicsPreset::Quality => "ReShade Enhanced",
            GraphicsPreset::Ultra => "ReShade Enhanced",
        }
    }
}

/// Everything the launcher decides about a session.
#[derive(Debug, Clone)]
pub struct LaunchConfig {
    pub build: GameBuild,
    pub graphics: GraphicsPreset,
    pub fullscreen: bool,
    /// Path to the game image to boot directly.
    pub game_path: PathBuf,
    /// Path to the runtime's portable user directory.
    pub user_dir: PathBuf,
    /// Online match context file, if this is a matchmade session.
    ///
    /// A path to a file, never the contents and never a token. The Discord
    /// credential is held by the launcher and is never passed to the emulator
    /// in any form, including as an argument, an environment variable, or a
    /// field inside this file.
    pub match_context: Option<PathBuf>,
}

/// A generated INI file, as name and contents.
pub type GeneratedConfig = BTreeMap<String, String>;

/// Convenience for building an overlay without four lines per key.
fn set(o: &mut Overlay, section: &str, key: &str, value: impl Into<String>) {
    o.entry(section.to_string())
        .or_default()
        .insert(key.to_string(), value.into());
}

/// Generates emulator configuration from the launcher's settings model.
#[derive(Debug)]
pub struct ConfigGenerator;

impl ConfigGenerator {
    /// The keys the launcher actually decides, per file.
    ///
    /// This replaces generating whole files. Measured on a real install, the
    /// generated `Dolphin.ini` was 18 lines across 6 sections while the
    /// player's own was 215 lines across 16, including `[Core]`, `[GameList]`,
    /// `[Input]` and `[Movie]`. Writing the whole file deleted ten sections of
    /// tuned settings and the paths to their own game images.
    ///
    /// So the launcher states its opinion as keys and merges them, and anything
    /// it has no opinion about survives untouched. See [`crate::ini`].
    pub fn overlays(config: &LaunchConfig) -> BTreeMap<String, Overlay> {
        let mut out: BTreeMap<String, Overlay> = BTreeMap::new();
        for file in ["Dolphin.ini", "GFX.ini", "Logger.ini"] {
            out.insert(file.to_string(), crate::settings_law::locked_overlay(file));
        }

        // Fullscreen is not the launcher overriding a Dolphin setting: it is
        // the player's own Play-page toggle, read back out of their launcher
        // settings on the way in. Writing it carries out their instruction.
        set(
            out.get_mut("Dolphin.ini").unwrap(),
            "Display",
            "Fullscreen",
            bool_ini(config.fullscreen),
        );

        out
    }

    /// The launcher's tuned defaults, to be applied with
    /// [`crate::ini::IniDocument::merge_absent`] and never with `merge`. A
    /// fresh user folder gets them; one where anybody has expressed a
    /// preference keeps what it has.
    pub fn seeded_overlays() -> BTreeMap<String, Overlay> {
        let mut out: BTreeMap<String, Overlay> = BTreeMap::new();
        for file in ["Dolphin.ini", "GFX.ini", "Logger.ini"] {
            out.insert(file.to_string(), crate::settings_law::seeded_overlay(file));
        }
        out
    }


    /// The Reborn requirements that go into the per-game layer.
    ///
    /// This is Dolphin's highest-precedence config layer: anything written here
    /// cannot be changed in Dolphin's own GUI at all, so it carries the five
    /// keys the game genuinely cannot run correctly without and nothing else.
    /// Every taste key that used to live here is retired and pruned by
    /// [`crate::settings_law::reconcile_once`]. See [`crate::settings_law`].
    pub fn required_game_settings() -> Overlay {
        crate::settings_law::locked_game_settings()
    }

    /// Produce every INI the runtime needs for this session.
    pub fn generate(config: &LaunchConfig) -> GeneratedConfig {
        let mut out = GeneratedConfig::new();
        out.insert("Dolphin.ini".into(), Self::dolphin_ini(config));
        out.insert("GFX.ini".into(), Self::gfx_ini(config));
        out.insert("Logger.ini".into(), Self::logger_ini());
        for (name,overlay) in Self::overlays(config){
            let mut doc=crate::ini::IniDocument::parse(out.get(&name).map(String::as_str).unwrap_or_default());
            doc.merge(&overlay);out.insert(name,doc.render());
        }
        out
    }

    fn dolphin_ini(config: &LaunchConfig) -> String {
        // Analytics off explicitly rather than by omission. The blueprint's
        // privacy requirement is no telemetry, and a default that changes
        // upstream should not quietly turn it on.
        format!(
            "[General]\n\
             ShowLag = False\n\
             ShowFrameCount = False\n\
             \n\
             [Display]\n\
             Fullscreen = {fullscreen}\n\
             RenderToMain = True\n\
             \n\
             [Analytics]\n\
             Enabled = False\n\
             PermissionAsked = True\n\
             \n\
             [Interface]\n\
             ConfirmStop = False\n\
             UsePanicHandlers = False\n\
             OnScreenDisplayMessages = True\n",
            fullscreen = bool_ini(config.fullscreen),
        )
    }

    fn gfx_ini(config: &LaunchConfig) -> String {
        format!(
            "[Settings]\n\
             InternalResolution = {res}\n\
             MSAA = 1\n\
             SSAA = False\n\
             MaxAnisotropy = {aniso}\n\
             ShaderCompilationMode = 2\n\
             WaitForShadersBeforeStarting = True\n\
             \n\
             [Hacks]\n\
             EFBAccessEnable = True\n\
             EFBToTextureEnable = False\n",
            res = config.graphics.internal_resolution(),
            aniso = 4,
        )
    }

    fn logger_ini() -> String {
        // Console, window, and file logging off. The Xbox payload ships them
        // on, which puts a log window in front of a player who never asked
        // for one, and file logging costs a disk write per line.
        "[Options]\n\
         Verbosity = 1\n\
         WriteToFile = False\n\
         WriteToConsole = False\n\
         WriteToWindow = False\n"
            .to_string()
    }
}

fn bool_ini(v: bool) -> &'static str {
    if v {
        "True"
    } else {
        "False"
    }
}

/// Validates a runtime package before it is allowed to ship or launch.
#[derive(Debug)]
pub struct RuntimePackager;

/// Something present in a runtime package that must not be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackagingViolation {
    pub path: String,
    pub reason: &'static str,
}

impl RuntimePackager {
    /// Files that change runtime behavior and must never reach a player.
    ///
    /// Paths are relative to the runtime's `User` directory.
    pub fn forbidden() -> &'static [(&'static str, &'static str)] {
        &[
            (
                "nsr_track_offline.txt",
                "disables the netplay-only gate, so every offline game against \
                 the CPU is captured and uploaded, and a game using league \
                 teams can land on a real fixture",
            ),
            (
                "nsr_debug.txt",
                "prints capture gate state on screen during play",
            ),
        ]
    }

    /// Patterns that indicate development leftovers rather than a release.
    fn is_leftover(name: &str) -> Option<&'static str> {
        if name.contains(".bak") {
            Some("a dated backup, not release content")
        } else if name.ends_with(".iso") {
            Some("a disc image inside the runtime package; game images live in library/")
        } else {
            None
        }
    }

    /// Inspect a prepared runtime directory. An empty result means shippable.
    pub fn audit(runtime_dir: &Path) -> Result<Vec<PackagingViolation>> {
        let mut found = Vec::new();

        let user_dir = runtime_dir.join("User");
        for (name, reason) in Self::forbidden() {
            if user_dir.join(name).exists() {
                found.push(PackagingViolation {
                    path: format!("User/{name}"),
                    reason,
                });
            }
        }

        for dir in ["LeagueGameStats", "GameStats"] {
            if user_dir.join(dir).is_dir() {
                found.push(PackagingViolation {
                    path: format!("User/{dir}"),
                    reason: "captured match results inside the runtime package; they belong to a player",
                });
            }
        }

        // One level deep is enough to catch the observed cases: dated .bak
        // copies of the executable and the ReShade add-on sit beside them, and
        // the disc images sit in Games/.
        for dir in [runtime_dir.to_path_buf(), runtime_dir.join("Games")] {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if let Some(reason) = Self::is_leftover(&name) {
                    let rel = if dir == runtime_dir {
                        name.clone()
                    } else {
                        format!("Games/{name}")
                    };
                    found.push(PackagingViolation { path: rel, reason });
                }
            }
        }

        found.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(found)
    }
}

/// Builds the command line for a direct boot.
#[derive(Debug)]
pub struct Launcher;

impl Launcher {
    /// Arguments that boot straight into the game.
    ///
    /// No game list, no NetPlay browser, no configuration window. The player
    /// never sees Dolphin's own interface in an ordinary flow.
    pub fn args(config: &LaunchConfig) -> Vec<OsString> {
        // -u is a portable user directory, so the launcher's generated config
        // is the one that applies and a system-wide Dolphin install is left
        // alone. -e boots the image directly, with no game list in between.
        //
        // -b is batch mode: no Dolphin window behind the game, and the process
        // ends when the game does. It is passed on every launch, windowed or
        // not. It used to be passed only in fullscreen, and that is how a
        // player could close their game and still have Reborn Dolphin running:
        // stopping emulation in a windowed session dropped them back to
        // Dolphin's own main window instead of ending the process, so the
        // launcher went on reporting a session in progress and every settings
        // write was refused with `CLOSE_EMULATOR_FIRST` until they found that
        // window and closed it. Whether the game is fullscreen is a
        // Dolphin.ini key (`[Display] Fullscreen`, written by
        // `ConfigGenerator`), never this flag.
        vec![
            "-u".into(),
            config.user_dir.clone().into_os_string(),
            "-e".into(),
            config.game_path.clone().into_os_string(),
            "-b".into(),
        ]
    }

    /// Environment for the child process.
    ///
    /// The match context is passed as a PATH to a file. The Discord token is
    /// never included, in any form.
    pub fn env(config: &LaunchConfig) -> BTreeMap<String, OsString> {
        let mut env = BTreeMap::new();
        // Linux: Reborn Dolphin draws through X11. Started from a Wayland
        // session (Steam Deck Game Mode is gamescope, which is Wayland with
        // XWayland beside it) Qt would otherwise pick Wayland, where this
        // Dolphin's renderer has no surface to draw into. A value the player
        // set themselves is left alone.
        if cfg!(target_os = "linux") && std::env::var_os("QT_QPA_PLATFORM").is_none() {
            env.insert("QT_QPA_PLATFORM".to_string(), OsString::from("xcb"));
        }
        if let Some(context) = &config.match_context {
            env.insert(
                "NSR_MATCH_CONTEXT".to_string(),
                context.clone().into_os_string(),
            );
        }
        env
    }

    /// Start the emulator.
    pub fn launch(
        platform: &Platform,
        runtime_exe: &Path,
        config: &LaunchConfig,
    ) -> Result<ProcessHandle> {
        if !runtime_exe.exists() {
            return Err(Error::Io {
                path: runtime_exe.to_path_buf(),
                source: std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "the Reborn Dolphin runtime is not installed",
                ),
            });
        }
        let args = Self::args(config);
        let env = Self::env(config);
        platform
            .processes
            .spawn(runtime_exe, &args, runtime_exe.parent(), &env)
    }
}

/// What the launcher should show after the game exits.
///
/// Derived from [`ExitKind`], but a separate type because the user-facing
/// decision is not the same as the technical one. A non-zero exit that is not a
/// crash still returns the player to the launcher quietly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionOutcome {
    /// Return to the page the player came from.
    ReturnToLauncher,
    /// Show the crash recovery page.
    ShowRecovery { detail: String },
}

impl SessionOutcome {
    pub fn from_exit(kind: &ExitKind) -> Self {
        match kind {
            ExitKind::Crashed { detail } => SessionOutcome::ShowRecovery {
                detail: detail.clone(),
            },
            _ => SessionOutcome::ReturnToLauncher,
        }
    }
}

#[cfg(test)]
mod tests {
    /// The requirements still win over live drift. Everything else does not.
    ///
    /// This replaces `mandatory_settings_override_live_drift_for_both_builds`,
    /// which parsed this same fixture and asserted the launcher overwrote the
    /// player's 4x internal resolution and 4x anti-aliasing. That assertion was
    /// the bug, written down and defended: it is why a graphics change made in
    /// Dolphin's own window did nothing.
    #[test]
    fn only_locked_keys_override_live_drift() {
        use super::*;
        use crate::ini::IniDocument;
        let mut doc=IniDocument::parse("[Core]\nSyncGPU=True\nCPUThread=False\n[Video_Settings]\nInternalResolution=4\nMSAA=0x00000004\n[Video_Hacks]\nEFBAccessEnable=False\nDeferEFBCopies=False\n");
        doc.merge(&ConfigGenerator::required_game_settings());

        // The one graphics key the player does not get to break: with EFB
        // access off the created-player portraits bake black into their save.
        assert_eq!(doc.get("Video_Hacks","EFBAccessEnable"),Some("True"));

        // Everything the player chose survives, including the settings this
        // test used to assert were overwritten.
        assert_eq!(doc.get("Video_Settings","InternalResolution"),Some("4"),"the player's resolution");
        assert_eq!(doc.get("Video_Settings","MSAA"),Some("0x00000004"),"the player's anti-aliasing");
        assert_eq!(doc.get("Core","SyncGPU"),Some("True"),"expensive, but theirs to spend");
        assert_eq!(doc.get("Video_Hacks","DeferEFBCopies"),Some("False"),"a performance trade, theirs");
        assert_eq!(doc.get("Core","CPUThread"),Some("False"),"the per-machine freeze workaround");

        assert!(doc.merge(&ConfigGenerator::required_game_settings()).is_noop(),"a second launch changes nothing");
    }

    /// The per-game file is the layer Dolphin's own GUI cannot beat, so what
    /// goes in it is exactly what the player must not be able to change.
    #[test]
    fn the_per_game_layer_carries_requirements_and_no_taste() {
        use super::*;
        let required = ConfigGenerator::required_game_settings();
        for (section, key) in [
            ("Video_Settings", "InternalResolution"),
            ("Video_Settings", "MSAA"),
            ("Video_Settings", "SSAA"),
            ("Core", "SyncGPU"),
            ("Core", "GFXBackend"),
            ("Video_Hacks", "DeferEFBCopies"),
            ("Video_Settings", "DumpTextures"),
            ("Video_Enhancements", "ArbitraryMipmapDetection"),
        ] {
            assert!(
                required.get(section).is_none_or(|s| !s.contains_key(key)),
                "{section}/{key} is taste and must not be pinned where the Graphics window cannot beat it",
            );
        }
    }

    /// The two settings a live 0-FPS freeze on 2026-09-11 turned on.
    ///
    /// `CacheHiresTextures` prefetches the whole HD texture pack decoded into
    /// RAM at boot. On a 24 GB machine that measured 12 GB of private bytes,
    /// 384 MB of free RAM and a 28 GB pagefile in use, which is the "running
    /// slow" the player reported. The pack is BC7 DDS now, so on-demand loads
    /// are sub-millisecond and prefetch buys nothing. It must stay `False`, and
    /// because the launcher merges these on every session prep, a stray edit
    /// back to `True` re-arms the regression on every player's machine. This
    /// test is the tripwire.
    ///
    /// `CPUThread` must NOT be forced here. Single core is the freeze
    /// workaround (it removes the GPU→CPU interrupt hand-off Dolphin loses), but
    /// it costs performance and is a per-machine call, so it is left to the
    /// user's own `GameSettings` and must survive a launcher session untouched.
    #[test]
    fn the_texture_prefetch_stays_off_and_single_core_is_left_to_the_user() {
        use super::*;
        let required = ConfigGenerator::required_game_settings();
        assert_eq!(
            required.get("Video_Settings").and_then(|s| s.get("CacheHiresTextures")),
            Some(&"False".to_string()),
            "CacheHiresTextures must stay False: True prefetches the HD pack to ~12 GB RAM (the slowness)",
        );
        assert!(
            required.get("Core").map_or(true, |s| !s.contains_key("CPUThread")),
            "the launcher must not force CPUThread: single core is the user's per-machine freeze workaround",
        );
        // Adam 2026-09-12: "We need ram override". The imported content depends on the 64 MB MEM1.
        assert_eq!(
            required.get("Core").and_then(|s| s.get("RAMOverrideEnable")),
            Some(&"True".to_string()),
            "the RAM override is required by the imported content and must not be dropped",
        );
        assert_eq!(
            required.get("Core").and_then(|s| s.get("MEM1Size")),
            Some(&"67108864".to_string()),
            "MEM1 must stay 64 MB",
        );
    }

    /// Both of these were forced for defensible reasons, and both are still
    /// the player's to spend.
    ///
    /// `SyncGPU` was forced True on 09-05 to chase the 0-FPS freeze and the
    /// 09-11 probe proved the freeze recurs with it on, so it bought nothing
    /// and cost a lot of frames. `DeferEFBCopies` is a performance trade whose
    /// forced value flipped between shipped builds, changing the game under
    /// players who had chosen otherwise. The launcher seeds its preference in
    /// the tuned template and does not weld either one in.
    #[test]
    fn sync_gpu_and_defer_efb_copies_belong_to_the_player() {
        use super::*;
        let required = ConfigGenerator::required_game_settings();
        assert!(
            required.get("Core").is_none_or(|s| !s.contains_key("SyncGPU")),
            "SyncGPU is one of the most expensive options in Dolphin and never fixed the freeze; it is not the launcher's to pin",
        );
        assert!(
            required.get("Video_Hacks").is_none_or(|s| !s.contains_key("DeferEFBCopies")),
            "DeferEFBCopies is a performance trade, and its forced value already flipped under players once",
        );
    }

    /// The graphics tier selects a ReShade preset. It has never varied internal
    /// resolution in any shipped build, and it does not now: resolution is the
    /// player's, offered in Settings and in Dolphin's own Graphics window.
    #[test]
    fn the_graphics_tier_does_not_decide_resolution() {
        use super::*;
        use crate::settings_law::{find, Owner};
        for preset in [
            GraphicsPreset::Performance,
            GraphicsPreset::Balanced,
            GraphicsPreset::Quality,
            GraphicsPreset::Ultra,
        ] {
            assert_eq!(preset.internal_resolution(), 3, "the seed default, not a per-tier force");
        }
        assert_eq!(
            find("GFX.ini", "Settings", "InternalResolution").map(|k| k.owner),
            Some(Owner::Player),
        );
    }

    use super::*;

    fn config() -> LaunchConfig {
        LaunchConfig {
            build: GameBuild::Offline,
            graphics: GraphicsPreset::Balanced,
            fullscreen: true,
            game_path: PathBuf::from("/data/library/offline/NSR.iso"),
            user_dir: PathBuf::from("/data/runtime/windows/User"),
            match_context: None,
        }
    }

    #[test]
    fn only_the_offline_build_is_customizable() {
        assert!(GameBuild::Offline.is_customizable());
        assert!(
            !GameBuild::Online.is_customizable(),
            "the online image must stay pristine or matchmaking cannot trust it"
        );
    }

    #[test]
    fn the_two_builds_never_share_an_install_root() {
        assert_ne!(
            GameBuild::Offline.install_root(),
            GameBuild::Online.install_root()
        );
    }

    #[test]
    fn the_performance_preset_turns_reshade_off() {
        assert!(!GraphicsPreset::Performance.reshade_enabled());
        if cfg!(windows) {
            assert!(GraphicsPreset::Balanced.reshade_enabled());
        } else {
            // ReShade is Windows only. Linux and macOS never enable it, which
            // Adam accepted rather than porting the per-field preset work.
            assert!(!GraphicsPreset::Balanced.reshade_enabled());
            assert!(!GraphicsPreset::Ultra.reshade_enabled());
        }
    }

    #[test]
    fn every_preset_renders_at_three_times_native() {
        let order = [
            GraphicsPreset::Performance,
            GraphicsPreset::Balanced,
            GraphicsPreset::Quality,
            GraphicsPreset::Ultra,
        ];
        for pair in order.windows(2) {
            assert!(
                pair[0].internal_resolution() == 3 && pair[1].internal_resolution() == 3,
                "{} should render below {}",
                pair[0].label(),
                pair[1].label()
            );
        }
    }

    #[test]
    fn generated_config_disables_analytics_explicitly() {
        let generated = ConfigGenerator::generate(&config());
        let dolphin = &generated["Dolphin.ini"];
        assert!(
            dolphin.contains("[Analytics]") && dolphin.contains("Enabled = False"),
            "telemetry must be off explicitly, not by omission:\n{dolphin}"
        );
        assert!(
            dolphin.contains("PermissionAsked = True"),
            "the player must never see Dolphin's own analytics prompt"
        );
    }

    #[test]
    fn generated_logging_never_opens_a_console_or_a_window() {
        let generated = ConfigGenerator::generate(&config());
        let logger = &generated["Logger.ini"];
        assert!(logger.contains("WriteToConsole = False"), "{logger}");
        assert!(logger.contains("WriteToWindow = False"), "{logger}");
    }

    /// The tuned defaults still reach a fresh install. They are seeded now
    /// rather than forced, which is the whole difference: a new user folder
    /// gets them, and a player who changes one afterwards keeps their change.
    ///
    /// This replaces `the_optimization_keys_are_generated_and_the_freeze_fix_is_kept`,
    /// which asserted these same keys were in the per-launch override overlay.
    #[test]
    fn the_optimization_keys_are_seeded_not_forced() {
        let cfg = LaunchConfig { build: GameBuild::Offline, graphics: GraphicsPreset::Balanced, fullscreen: true, game_path: "g.iso".into(), user_dir: "u".into(), match_context: None };
        let forced = ConfigGenerator::overlays(&cfg);
        let seeded = ConfigGenerator::seeded_overlays();

        // Quiet logging is FORCED, not seeded, since 2026-09-13.
        //
        // This assertion used to read the other way, and it was wrong in the
        // way that matters: it pinned the value the fix needed to change, so
        // the suite stayed green while every install shipped with Dolphin's
        // file logger on at Verbosity 4. Seeding writes only where a key is
        // ABSENT, and the freeze instrumentation had already set all four
        // Logger keys on every machine that ran it, so the seed reached none
        // of them. Measured on the dev install the day this was found:
        // dolphin.log 21.3 MB, 154,693 lines, one synchronous write per line
        // on the emulation thread, worst where the game is busiest.
        //
        // "A tester who turns logging on keeps it" was the intent, and it is
        // not worth a shipped build writing a log nobody reads. If a tester
        // needs logging, the launcher should offer it as a control rather
        // than leave it on for everyone.
        assert!(
            seeded.get("Logger.ini").is_none_or(|f| f.is_empty()),
            "logging is locked now, so nothing about it is merely seeded",
        );
        assert_eq!(forced["Logger.ini"]["Options"]["WriteToFile"], "False");
        assert_eq!(forced["Logger.ini"]["Options"]["Verbosity"], "1");

        // The modder's switches and the mipmap cost are the player's now.
        for (section, key) in [("Settings","DumpTextures"),("Settings","DumpBaseTextures"),("Enhancements","ArbitraryMipmapDetection")] {
            assert!(
                forced["GFX.ini"].get(section).is_none_or(|s| !s.contains_key(key)),
                "{section}/{key} must not be re-forced on every launch",
            );
        }
        assert!(forced["Dolphin.ini"].get("General").is_none_or(|s| !s.contains_key("UseDiscordPresence")));

        // The freeze fix itself is a requirement and stays forced: True
        // prefetches the whole 9,115-file HD pack decoded into RAM (~12 GB
        // measured 09-11) and starves the machine. That was the 3.0.0 "runs
        // slow everywhere" report.
        let g = ConfigGenerator::required_game_settings();
        assert_eq!(g["Video_Settings"]["CacheHiresTextures"], "False");
        assert_eq!(g["Video_Settings"]["HiresTextures"], "True");
        assert_eq!(g["Video_Hacks"]["EFBAccessEnable"], "True");
    }

    /// The launcher writes config and never reads it back, so the Settings page
    /// stays the single source of truth. Two identical configs must therefore
    /// produce byte-identical output.
    #[test]
    fn generation_is_deterministic() {
        assert_eq!(
            ConfigGenerator::generate(&config()),
            ConfigGenerator::generate(&config())
        );
    }

    #[test]
    fn launch_args_boot_directly_with_a_portable_user_directory() {
        let args = Launcher::args(&config());
        let joined: Vec<String> = args.iter().map(|a| a.to_string_lossy().into()).collect();
        assert!(joined.contains(&"-u".to_string()), "{joined:?}");
        assert!(joined.contains(&"-e".to_string()), "{joined:?}");
        assert!(
            joined.iter().any(|a| a.ends_with("NSR.iso")),
            "the image must be booted directly, not chosen from a game list: {joined:?}"
        );
    }

    /// Stopping the game must end the process, whether or not it was
    /// fullscreen.
    ///
    /// Batch mode used to be tied to `config.fullscreen`. A windowed session
    /// therefore returned the player to Dolphin's own main window when they
    /// stopped the game: Reborn Dolphin stayed running, the launcher went on
    /// waiting on it and reporting a session in progress, and the Settings
    /// page refused every write with [`CLOSE_EMULATOR_FIRST`] until the player
    /// found that window and closed it.
    #[test]
    fn every_launch_is_batch_mode_so_closing_the_game_closes_the_emulator() {
        for fullscreen in [true, false] {
            let mut c = config();
            c.fullscreen = fullscreen;
            let args: Vec<String> = Launcher::args(&c).iter().map(|a| a.to_string_lossy().into()).collect();
            assert!(
                args.contains(&"-b".to_string()),
                "a {} launch left Dolphin's own window behind the game: {args:?}",
                if fullscreen { "fullscreen" } else { "windowed" }
            );
        }
    }

    /// Linux Dolphin is started on X11 even from a Wayland session such as
    /// Game Mode; Windows and macOS are untouched.
    #[test]
    fn linux_dolphin_is_started_on_x11() {
        let env = Launcher::env(&config());
        let qpa = env.get("QT_QPA_PLATFORM").map(|v| v.to_string_lossy().into_owned());
        if cfg!(target_os = "linux") && std::env::var_os("QT_QPA_PLATFORM").is_none() {
            assert_eq!(qpa.as_deref(), Some("xcb"));
        } else if !cfg!(target_os = "linux") {
            assert_eq!(qpa, None);
        }
    }

    /// The single most important thing this module must not do.
    #[test]
    fn no_credential_ever_reaches_the_emulator() {
        let mut c = config();
        c.match_context = Some(PathBuf::from("/data/stats/queue/match-1234.json"));

        let args: Vec<String> = Launcher::args(&c)
            .iter()
            .map(|a| a.to_string_lossy().into())
            .collect();
        let env = Launcher::env(&c);
        let env_text: String = env
            .iter()
            .map(|(k, v)| format!("{k}={}", v.to_string_lossy()))
            .collect::<Vec<_>>()
            .join(" ");
        let all = format!("{} {}", args.join(" "), env_text);

        for forbidden in ["token", "bearer", "secret", "password", "authorization"] {
            assert!(
                !all.to_ascii_lowercase().contains(forbidden),
                "{forbidden:?} appears in the child process invocation: {all}"
            );
        }
        assert!(
            all.contains("NSR_MATCH_CONTEXT"),
            "the match context is passed as a file path: {all}"
        );
    }

    #[test]
    fn a_clean_runtime_package_passes_the_audit() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("User")).unwrap();
        std::fs::write(dir.path().join("Dolphin.exe"), b"binary").unwrap();
        assert!(RuntimePackager::audit(dir.path()).unwrap().is_empty());
    }

    /// Both marker files are present in the selected runtime right now, and its
    /// own README says shipping the first one uploads every player's offline
    /// game.
    #[test]
    fn the_offline_capture_marker_fails_the_audit() {
        let dir = tempfile::tempdir().unwrap();
        let user = dir.path().join("User");
        std::fs::create_dir_all(&user).unwrap();
        std::fs::write(user.join("nsr_track_offline.txt"), b"").unwrap();
        std::fs::write(user.join("nsr_debug.txt"), b"").unwrap();

        let violations = RuntimePackager::audit(dir.path()).unwrap();
        assert_eq!(violations.len(), 2, "{violations:#?}");
        assert!(violations
            .iter()
            .any(|v| v.path.contains("nsr_track_offline")));
        assert!(violations
            .iter()
            .any(|v| v.reason.contains("land on a real fixture")));
    }

    #[test]
    fn captured_results_inside_a_runtime_package_are_a_violation() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("User").join("GameStats").join("Online")).unwrap();
        let found = RuntimePackager::audit(dir.path()).unwrap();
        assert!(found.iter().any(|v| v.path == "User/GameStats"), "{found:?}");
    }

    #[test]
    fn dated_backups_and_disc_images_fail_the_audit() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("User")).unwrap();
        std::fs::create_dir_all(dir.path().join("Games")).unwrap();
        std::fs::write(
            dir.path().join("Dolphin.exe.bak_2026-08-29_pre_uisplit"),
            b"",
        )
        .unwrap();
        std::fs::write(dir.path().join("Games").join("CR4E12.iso"), b"").unwrap();

        let violations = RuntimePackager::audit(dir.path()).unwrap();
        assert_eq!(violations.len(), 2, "{violations:#?}");
        assert!(violations.iter().any(|v| v.path.ends_with("CR4E12.iso")));
    }

    #[test]
    fn only_a_crash_shows_the_recovery_page() {
        assert_eq!(
            SessionOutcome::from_exit(&ExitKind::Normal),
            SessionOutcome::ReturnToLauncher
        );
        assert_eq!(
            SessionOutcome::from_exit(&ExitKind::Error { code: 1 }),
            SessionOutcome::ReturnToLauncher,
            "a non-zero exit is not necessarily a crash and must not alarm the player"
        );
        assert_eq!(
            SessionOutcome::from_exit(&ExitKind::Terminated),
            SessionOutcome::ReturnToLauncher
        );
        assert!(matches!(
            SessionOutcome::from_exit(&ExitKind::Crashed {
                detail: "access violation (0xC0000005)".into()
            }),
            SessionOutcome::ShowRecovery { .. }
        ));
    }

    #[test]
    fn launching_a_missing_runtime_says_so_rather_than_panicking() {
        let p = Platform::detect();
        let err = Launcher::launch(&p, Path::new("/nonexistent/Dolphin"), &config()).unwrap_err();
        assert!(
            err.to_string().contains("not installed"),
            "the message should name the real problem: {err}"
        );
    }
    /// `env()` existed, was documented as how the match context reaches the
    /// emulator, and `launch()` never passed it. An online session would have
    /// booted with no context and no error.
    ///
    /// This spawns a real child that inspects the variable and reports through
    /// its exit code, because the exit code is the one channel the process seam
    /// actually exposes.
    ///
    /// The structural fix matters more than this test: `spawn` now takes `env`
    /// as a required parameter, so no caller can start a child without having
    /// decided what environment it gets. The old signature made the omission
    /// invisible.
    #[test]
    fn the_match_context_actually_reaches_the_child_process() {
        let dir = tempfile::tempdir().unwrap();
        let context = dir.path().join("match.json");

        let config = LaunchConfig {
            build: GameBuild::Online,
            graphics: GraphicsPreset::Balanced,
            fullscreen: false,
            game_path: dir.path().join("game.iso"),
            user_dir: dir.path().join("user"),
            match_context: Some(context),
        };

        let env = Launcher::env(&config);
        assert!(env.contains_key("NSR_MATCH_CONTEXT"));

        let platform = Platform::detect();
        let (program, args): (&str, Vec<OsString>) = if cfg!(windows) {
            (
                "cmd",
                vec![
                    "/C".into(),
                    "echo %NSR_MATCH_CONTEXT%| findstr /C:match.json >nul".into(),
                ],
            )
        } else {
            (
                "sh",
                vec![
                    "-c".into(),
                    "case \"$NSR_MATCH_CONTEXT\" in *match.json*) exit 0;; *) exit 9;; esac".into(),
                ],
            )
        };

        let handle = platform
            .processes
            .spawn(std::path::Path::new(program), &args, None, &env)
            .expect("spawn");
        assert_eq!(
            platform.processes.wait(handle).expect("wait"),
            ExitKind::Normal,
            "the child did not see NSR_MATCH_CONTEXT"
        );
    }
}

/// Shown when the emulator is open and something is about to write the files
/// it has in memory.
///
/// A finished instruction rather than a fault report, so it is carried as
/// [`crate::Error::player`] and reaches the player unchanged. It used to be an
/// [`std::io::Error`] wrapped in `Error::PlainIo`, which is transparent over
/// the operating system's own wording, so `Error::player_message` could not
/// tell it from a full disk and answered "Something unexpected happened. Try
/// again." That is false advice for the single most common failure of the
/// Settings page's open-the-emulator button.
pub const CLOSE_EMULATOR_FIRST: &str =
    "Close Reborn Dolphin before applying settings, then try again.";
#[cfg(windows)]
const PROCESS_CHECK_UNAVAILABLE: &str =
    "Windows could not check whether Reborn Dolphin is running. Try again.";
#[cfg(not(windows))]
const PROCESS_CHECK_UNAVAILABLE: &str =
    "Reborn could not check whether Reborn Dolphin is running. Try again.";

/// Avoid Dolphin's in-memory settings overwriting configuration written during a session.
pub fn ensure_runtime_idle(exe:&Path)->Result<()> {
    if runtime_is_running(exe)? { return Err(crate::Error::player(CLOSE_EMULATOR_FIRST)); }
    Ok(())
}

/// A fresh OS snapshot of this executable. A different install with the same
/// filename is not this runtime, and an inspection error is not proof it is open.
// Each platform's block ends in `return` because the others are compiled out.
#[allow(clippy::needless_return)]
pub fn runtime_is_running(exe: &Path) -> Result<bool> {
    #[cfg(windows)] {
        if !exe.is_file() { return Ok(false); }
        return crate::platform::runtime_process::is_running(exe).map_err(|error| {
            tracing::warn!(%error, "Dolphin process detection failed after fallback");
            crate::Error::player(PROCESS_CHECK_UNAVAILABLE)
        });
    }
    #[cfg(unix)] {
        if !exe.is_file() { return Ok(false); }
        return crate::platform::runtime_process_unix::is_running(exe).map_err(|error| {
            tracing::warn!(%error, "Dolphin process detection failed");
            crate::Error::player(PROCESS_CHECK_UNAVAILABLE)
        });
    }
    #[cfg(not(any(windows, unix)))] { let _ = exe; Ok(false) }
}

#[cfg(all(test, windows))]
mod process_detection_tests {
    use super::*;
    use std::os::windows::process::CommandExt;

    #[test]
    fn exact_runtime_detection_recovers_after_exit_and_ignores_another_install() {
        struct Child(std::process::Child);
        impl Drop for Child { fn drop(&mut self) { let _=self.0.kill(); let _=self.0.wait(); } }
        let root=tempfile::tempdir().unwrap();
        let (first, second)=(root.path().join("first"), root.path().join("second"));
        std::fs::create_dir(&first).unwrap();std::fs::create_dir(&second).unwrap();
        let (first, second)=(first.join("NSR Process Probe.exe"), second.join("NSR Process Probe.exe"));
        let source=std::env::var_os("ComSpec").unwrap();
        std::fs::copy(&source,&first).unwrap();std::fs::copy(&source,&second).unwrap();
        assert!(!runtime_is_running(&first).unwrap());
        let mut child=Child(std::process::Command::new(&first)
            .args(["/D","/Q","/C","set /p REBORN_TEST_WAIT="])
            .stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null()).creation_flags(0x08000000).spawn().unwrap());
        assert!(child.0.try_wait().unwrap().is_none());
        assert!(runtime_is_running(&first).unwrap(), "The actual child must be protected");
        assert!(ensure_runtime_idle(&first).is_err());
        assert!(!runtime_is_running(&second).unwrap(), "Another installation must not be blocked by its filename");
        drop(child);
        assert!(!runtime_is_running(&first).unwrap(), "Exited processes must not leave stale running state");
        assert!(ensure_runtime_idle(&first).is_ok());
    }
}

#[cfg(all(test, target_os = "linux"))]
mod unix_process_detection_tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    /// The Linux half of the Windows test above: a real child, a second
    /// install with the same file name, and no stale answer after it exits.
    #[test]
    fn exact_runtime_detection_recovers_after_exit_and_ignores_another_install() {
        struct Child(std::process::Child);
        impl Drop for Child { fn drop(&mut self) { let _=self.0.kill(); let _=self.0.wait(); } }
        let root=tempfile::tempdir().unwrap();
        let (first, second)=(root.path().join("first"), root.path().join("second"));
        std::fs::create_dir(&first).unwrap();std::fs::create_dir(&second).unwrap();
        let (first, second)=(first.join("dolphin-emu"), second.join("dolphin-emu"));
        for copy in [&first, &second] {
            std::fs::copy("/bin/sleep", copy).unwrap();
            std::fs::set_permissions(copy, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        assert!(!runtime_is_running(&first).unwrap());
        let mut child=Child(std::process::Command::new(&first).arg("30").spawn().unwrap());
        assert!(child.0.try_wait().unwrap().is_none());
        assert!(runtime_is_running(&first).unwrap(), "The actual child must be protected");
        assert!(ensure_runtime_idle(&first).is_err());
        assert!(!runtime_is_running(&second).unwrap(), "Another installation must not be blocked by its filename");
        drop(child);
        assert!(!runtime_is_running(&first).unwrap(), "Exited processes must not leave stale running state");
        assert!(ensure_runtime_idle(&first).is_ok());
    }
}

/// Seed a user folder from the runtime's template (`User` beside the
/// emulator, staged by `crate::user_template`), file by file, never
/// replacing one that exists. A folder a player has already run in keeps
/// every edit; a folder the setup just created gets the whole baseline: the
/// tuned INIs, the game settings and the shader lists. The session layers
/// the launcher's required keys on afterwards, so those still win. Returns
/// the files written, for the launch report.
pub fn seed_user_from_template(runtime_exe: &Path, user_dir: &Path) -> Result<Vec<PathBuf>> {
    let Some(template) = runtime_exe.parent().map(|d| d.join("User")) else {
        return Ok(Vec::new());
    };
    if !template.is_dir() {
        return Ok(Vec::new());
    }
    let mut written = Vec::new();
    seed_dir(&template, user_dir, &mut written)?;
    Ok(written)
}

fn seed_dir(from: &Path, to: &Path, written: &mut Vec<PathBuf>) -> Result<()> {
    let entries = std::fs::read_dir(from).map_err(|source| Error::Io { path: from.to_path_buf(), source })?;
    for entry in entries.flatten() {
        let src = entry.path();
        let dst = to.join(entry.file_name());
        if src.is_dir() {
            seed_dir(&src, &dst, written)?;
        } else if src.is_file() && !dst.exists() {
            std::fs::create_dir_all(to).map_err(|source| Error::Io { path: to.to_path_buf(), source })?;
            std::fs::copy(&src, &dst).map_err(|source| Error::Io { path: dst.clone(), source })?;
            written.push(dst);
        }
    }
    Ok(())
}

#[cfg(test)]
mod template_tests {
    use super::*;

    fn runtime_with_template(dir: &Path) -> PathBuf {
        let exe = dir.join("runtime").join("NFL Street Reborn Dolphin.exe");
        std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
        std::fs::write(&exe, b"").unwrap();
        for (rel, body) in [
            ("User/Config/Dolphin.ini", "[Core]\nCPUThread = True\n"),
            ("User/Config/GFX.ini", "[Settings]\nMSAA = 0x00000004\n"),
            ("User/GameSettings/GN7E69.ini", "[Gecko]\n$Point Remover\n"),
            ("User/Config/notes.txt", "not an ini"),
            ("User/Cache/GN7E69.uidcache", "the shader list"),
        ] {
            let p = exe.parent().unwrap().join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, body).unwrap();
        }
        exe
    }

    /// Everything the runtime ships as its template is the baseline: the
    /// build's staging (crate::user_template) decides what ships, and the
    /// seed copies it whole.
    #[test]
    fn a_new_user_folder_gets_every_template_file() {
        let dir = tempfile::tempdir().unwrap();
        let exe = runtime_with_template(dir.path());
        let user = dir.path().join("dolphin-users").join("fresh-1");
        let written = seed_user_from_template(&exe, &user).unwrap();
        let mut names: Vec<String> = written
            .iter()
            .map(|p| p.strip_prefix(&user).unwrap().to_string_lossy().replace('\\', "/"))
            .collect();
        names.sort();
        assert_eq!(names, ["Cache/GN7E69.uidcache", "Config/Dolphin.ini", "Config/GFX.ini", "Config/notes.txt", "GameSettings/GN7E69.ini"]);
        assert_eq!(std::fs::read_to_string(user.join("GameSettings/GN7E69.ini")).unwrap(), "[Gecko]\n$Point Remover\n");
        assert_eq!(std::fs::read_to_string(user.join("Cache/GN7E69.uidcache")).unwrap(), "the shader list");
    }

    #[test]
    fn a_folder_a_player_has_run_in_keeps_every_edit() {
        let dir = tempfile::tempdir().unwrap();
        let exe = runtime_with_template(dir.path());
        let user = dir.path().join("dolphin-users").join("theirs");
        std::fs::create_dir_all(user.join("Config")).unwrap();
        std::fs::write(user.join("Config/Dolphin.ini"), "[Core]\nCPUThread = False\n").unwrap();
        let written = seed_user_from_template(&exe, &user).unwrap();
        assert_eq!(std::fs::read_to_string(user.join("Config/Dolphin.ini")).unwrap(), "[Core]\nCPUThread = False\n");
        assert_eq!(written.len(), 4, "the four files they did not have: {written:?}");
        assert!(user.join("Config/GFX.ini").is_file());
        assert!(user.join("GameSettings/GN7E69.ini").is_file());
    }

    #[test]
    fn a_runtime_without_a_template_seeds_nothing_and_is_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("runtime").join("Dolphin.exe");
        std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
        std::fs::write(&exe, b"").unwrap();
        let user = dir.path().join("user");
        assert!(seed_user_from_template(&exe, &user).unwrap().is_empty());
        assert!(!user.exists(), "nothing to seed, nothing created");
    }
}
