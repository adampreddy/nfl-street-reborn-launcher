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
        match self {
            GraphicsPreset::Performance => 1,
            GraphicsPreset::Balanced => 3,
            GraphicsPreset::Quality => 4,
            GraphicsPreset::Ultra => 6,
        }
    }

    /// ReShade is Windows only, and the Performance preset turns it off.
    pub fn reshade_enabled(self) -> bool {
        cfg!(windows) && !matches!(self, GraphicsPreset::Performance)
    }

    pub fn label(self) -> &'static str {
        match self {
            GraphicsPreset::Performance => "Performance",
            GraphicsPreset::Balanced => "Balanced",
            GraphicsPreset::Quality => "Quality",
            GraphicsPreset::Ultra => "Ultra",
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

/// Generates emulator configuration from the launcher's settings model.
#[derive(Debug)]
pub struct ConfigGenerator;

impl ConfigGenerator {
    /// Produce every INI the runtime needs for this session.
    pub fn generate(config: &LaunchConfig) -> GeneratedConfig {
        let mut out = GeneratedConfig::new();
        out.insert("Dolphin.ini".into(), Self::dolphin_ini(config));
        out.insert("GFX.ini".into(), Self::gfx_ini(config));
        out.insert("Logger.ini".into(), Self::logger_ini());
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
            aniso = match config.graphics {
                GraphicsPreset::Performance => 0,
                GraphicsPreset::Balanced => 2,
                GraphicsPreset::Quality | GraphicsPreset::Ultra => 4,
            },
        )
    }

    fn logger_ini() -> String {
        // Console and window logging off. The Xbox payload ships them on, which
        // puts a log window in front of a player who never asked for one.
        "[Options]\n\
         Verbosity = 2\n\
         WriteToFile = True\n\
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
        let mut args: Vec<OsString> = vec![
            "-u".into(),
            config.user_dir.clone().into_os_string(),
            "-e".into(),
            config.game_path.clone().into_os_string(),
        ];

        if config.fullscreen {
            args.push("-b".into());
        }

        args
    }

    /// Environment for the child process.
    ///
    /// The match context is passed as a PATH to a file. The Discord token is
    /// never included, in any form.
    pub fn env(config: &LaunchConfig) -> BTreeMap<String, OsString> {
        let mut env = BTreeMap::new();
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
        platform
            .processes
            .spawn(runtime_exe, &args, runtime_exe.parent())
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
    fn presets_increase_resolution_monotonically() {
        let order = [
            GraphicsPreset::Performance,
            GraphicsPreset::Balanced,
            GraphicsPreset::Quality,
            GraphicsPreset::Ultra,
        ];
        for pair in order.windows(2) {
            assert!(
                pair[0].internal_resolution() < pair[1].internal_resolution(),
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
}
