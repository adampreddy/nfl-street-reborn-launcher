//! Starting the game, and staying with it until it stops.
//!
//! # Why this is in the core and not in a front end
//!
//! The launch sequence used to live only in the command line tool. That meant
//! the window could show a Play button and had no way to press it: the
//! interface had five commands and every one of them was read-only. A sequence
//! that only one caller can perform is a sequence the other caller will
//! eventually reimplement slightly differently, and then the two will disagree
//! about which controller file was written.
//!
//! So the order lives here, once, and both the window and the command line ask
//! for it.
//!
//! # The order, and why it is this order
//!
//! 1. **Take the single-instance lock first.** Two copies of the game booting
//!    the same portable user directory write the same memory card, and the
//!    loser's franchise is whatever happens to be flushed last.
//! 2. **Recover anything left unfinished.** If the launcher died with the game
//!    last time, that record is the only evidence it happened.
//! 3. **Regenerate the emulator configuration.** Every launch, from scratch, so
//!    a change made inside Dolphin never persists and the launcher's settings
//!    stay the single source of truth.
//! 4. **Ask the runtime what controllers exist, then write the pad file.**
//!    Asked at launch rather than remembered: a pad can be swapped between
//!    sessions, and a remembered device string that no longer matches binds
//!    nothing without saying so.
//! 5. **Start the game and wait**, then classify and record how it ended.

use std::path::{Path, PathBuf};

use crate::controller::{ControllerProfile, GcPadConfig};
use crate::devices::{DeviceScanner, Enumeration};
use crate::platform::{DataDir, Error, Platform, Result};
use crate::runtime::{
    ConfigGenerator, GameBuild, GraphicsPreset, LaunchConfig, Launcher, SessionOutcome,
};
use crate::supervisor::{SessionEnd, Startup, Supervisor};

/// What the caller asks for.
#[derive(Debug, Clone)]
pub struct LaunchRequest {
    pub runtime_exe: PathBuf,
    pub game_path: PathBuf,
    pub graphics: GraphicsPreset,
    pub fullscreen: bool,
    pub build: GameBuild,
}

/// Why a launch could not even be attempted.
///
/// Separate from an error, because none of these are faults: they are states a
/// player can be in, and each one has a different thing to do about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// The runtime is not where the caller said it was.
    NoRuntime { path: PathBuf },
    /// The game image is not where the caller said it was.
    NoGame { path: PathBuf },
    /// Another launcher holds the lock.
    AlreadyRunning,
}

impl Refusal {
    /// A sentence a player can act on.
    pub fn message(&self) -> String {
        match self {
            Refusal::NoRuntime { path } => format!(
                "No Reborn Dolphin at {}. Set the runtime in Settings.",
                path.display()
            ),
            Refusal::NoGame { path } => format!(
                "No game image at {}. Set the game in Settings.",
                path.display()
            ),
            Refusal::AlreadyRunning => {
                "Another launcher is already running. Close it first.".to_string()
            }
        }
    }
}

/// What happened.
#[derive(Debug, Clone)]
pub enum Launched {
    /// The game ran and stopped.
    Finished {
        end: SessionEnd,
        outcome: SessionOutcome,
        minutes: f64,
        /// Which pad went to which port, so the caller can report it.
        pads: Vec<(u8, String)>,
        /// True when the pad list came from the runtime rather than the built in
        /// scan, which is the only authoritative source.
        pads_authoritative: bool,
    },
    /// Nothing was started, and this is why.
    Refused(Refusal),
}

/// Everything written for a launch, reported so a caller can show it.
#[derive(Debug, Clone, Default)]
pub struct WrittenConfig {
    pub files: Vec<PathBuf>,
    pub pads: Vec<(u8, String)>,
    pub pads_authoritative: bool,
    /// Present when no controller could be found, so the caller can say so
    /// rather than launching silently into nothing.
    pub no_controller: bool,
}

/// The launch sequence.
#[derive(Debug)]
pub struct Session;

impl Session {
    /// Write every configuration file a launch needs, and report what was
    /// written. Split out from [`Session::run`] so a caller can offer a dry run
    /// without duplicating the order.
    pub fn prepare(platform: &Platform, req: &LaunchRequest) -> Result<WrittenConfig> {
        let user_dir = platform
            .paths
            .data_root()?
            .join(DataDir::Runtime.relative())
            .join("user");
        let config_dir = user_dir.join("Config");
        std::fs::create_dir_all(&config_dir).map_err(|source| Error::Io {
            path: config_dir.clone(),
            source,
        })?;

        let config = LaunchConfig {
            build: req.build,
            graphics: req.graphics,
            fullscreen: req.fullscreen,
            game_path: req.game_path.clone(),
            user_dir: user_dir.clone(),
            match_context: None,
        };

        let mut written = WrittenConfig::default();

        for (name, contents) in ConfigGenerator::generate(&config) {
            let path = config_dir.join(&name);
            std::fs::write(&path, contents).map_err(|source| Error::Io {
                path: path.clone(),
                source,
            })?;
            written.files.push(path);
        }

        // Ask the runtime that is about to be launched. The built in scan sees
        // XInput only, which on a real machine misses an entire backend.
        let scratch = platform.paths.data_root()?.join(DataDir::Cache.relative());
        let (scan, authoritative) =
            match crate::devices::scan_via_runtime(platform, &req.runtime_exe, &scratch) {
                Ok(s) => (s, true),
                Err(_) => (DeviceScanner::scan(), false),
            };
        written.pads_authoritative = authoritative;

        match build_gcpad(&scan, 4) {
            Some((ini, assignment)) => {
                let path = config_dir.join("GCPadNew.ini");
                std::fs::write(&path, ini).map_err(|source| Error::Io {
                    path: path.clone(),
                    source,
                })?;
                written.files.push(path);
                written.pads = assignment;
            }
            None => written.no_controller = true,
        }

        Ok(written)
    }

    /// Start the game and block until it stops.
    ///
    /// Blocking is deliberate: something has to hold the lock and the session
    /// record for as long as the game runs, and that something is whoever
    /// called this. A window calls it off the interface thread.
    pub fn run(platform: &Platform, req: &LaunchRequest) -> Result<Launched> {
        if !req.runtime_exe.exists() {
            return Ok(Launched::Refused(Refusal::NoRuntime {
                path: req.runtime_exe.clone(),
            }));
        }
        if !req.game_path.exists() {
            return Ok(Launched::Refused(Refusal::NoGame {
                path: req.game_path.clone(),
            }));
        }

        // The lock, before anything is written or started.
        let Startup::Owner(supervisor) = Supervisor::acquire(platform)? else {
            return Ok(Launched::Refused(Refusal::AlreadyRunning));
        };
        let _ = supervisor.recover_unfinished()?;

        let written = Self::prepare(platform, req)?;

        let user_dir = platform
            .paths
            .data_root()?
            .join(DataDir::Runtime.relative())
            .join("user");
        let config = LaunchConfig {
            build: req.build,
            graphics: req.graphics,
            fullscreen: req.fullscreen,
            game_path: req.game_path.clone(),
            user_dir,
            match_context: None,
        };

        let record = supervisor.begin(req.build)?;
        let handle = Launcher::launch(platform, &req.runtime_exe, &config)?;
        let exit = platform.processes.wait(handle)?;
        let finished = supervisor.finish(record, &exit)?;

        let end = finished
            .end
            .clone()
            .unwrap_or(SessionEnd::Exited { code: None });
        Ok(Launched::Finished {
            outcome: end.outcome(),
            end,
            minutes: finished.duration_ms().unwrap_or(0) as f64 / 60_000.0,
            pads: written.pads,
            pads_authoritative: written.pads_authoritative,
        })
    }
}

/// Build `GCPadNew.ini` from whatever the runtime says is plugged in.
///
/// Returns the file contents and the port assignment, or `None` when there is
/// nothing to bind.
pub fn build_gcpad(scan: &Enumeration, ports: u8) -> Option<(String, Vec<(u8, String)>)> {
    let pads = scan.playable_pads();
    if pads.is_empty() {
        return None;
    }

    let profiles: Vec<ControllerProfile> = pads
        .iter()
        .take(ports as usize)
        .enumerate()
        .map(|(i, d)| ControllerProfile::new(d.identity(), (i + 1) as u8))
        .collect();

    let by_key: std::collections::BTreeMap<String, String> = pads
        .iter()
        .map(|d| (d.identity().profile_key(), d.device_string()))
        .collect();

    let ini = GcPadConfig::generate(&profiles, |id| {
        by_key
            .get(&id.profile_key())
            .cloned()
            // Never invent one. A made up Device line parses fine and binds
            // nothing, which looks exactly like a broken controller.
            .unwrap_or_else(|| "UNKNOWN-DEVICE".to_string())
    });

    let assignment = pads
        .iter()
        .take(ports as usize)
        .enumerate()
        .map(|(i, d)| ((i + 1) as u8, d.device_string()))
        .collect();
    Some((ini, assignment))
}

/// Where the game image is expected to be, if the caller has not said.
pub fn default_game_path(platform: &Platform) -> Result<PathBuf> {
    Ok(platform
        .paths
        .data_root()?
        .join(DataDir::LibraryOffline.relative())
        .join("game.iso"))
}

/// Whether a path looks like a GameCube image this launcher can boot.
pub fn is_game_image(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase())
            .as_deref(),
        Some("iso") | Some("rvz") | Some("gcm") | Some("ciso")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(runtime: PathBuf, game: PathBuf) -> LaunchRequest {
        LaunchRequest {
            runtime_exe: runtime,
            game_path: game,
            graphics: GraphicsPreset::Balanced,
            fullscreen: true,
            build: GameBuild::Offline,
        }
    }

    /// A missing runtime is a state, not a crash, and the message has to say
    /// what to do about it.
    #[test]
    fn a_missing_runtime_is_refused_with_a_usable_message() {
        let p = Platform::detect();
        let r = req(
            PathBuf::from("does-not-exist.exe"),
            PathBuf::from("does-not-exist.iso"),
        );
        match Session::run(&p, &r).expect("refusal is not an error") {
            Launched::Refused(Refusal::NoRuntime { .. }) => {}
            other => panic!("expected a runtime refusal, got {other:?}"),
        }
        let msg = Refusal::NoRuntime {
            path: PathBuf::from("x"),
        }
        .message();
        assert!(msg.contains("Settings"), "no way forward given: {msg}");
    }

    /// The runtime is checked before the game, because a missing runtime makes
    /// the game path unverifiable anyway.
    #[test]
    fn a_present_runtime_with_no_game_is_refused_for_the_game() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("Dolphin.exe");
        std::fs::write(&exe, b"not really an exe").unwrap();
        let p = Platform::detect();
        let r = req(exe, dir.path().join("absent.iso"));
        match Session::run(&p, &r).expect("refusal is not an error") {
            Launched::Refused(Refusal::NoGame { .. }) => {}
            other => panic!("expected a game refusal, got {other:?}"),
        }
    }

    #[test]
    fn every_refusal_says_something_actionable() {
        for r in [
            Refusal::NoRuntime {
                path: PathBuf::from("a"),
            },
            Refusal::NoGame {
                path: PathBuf::from("b"),
            },
            Refusal::AlreadyRunning,
        ] {
            let m = r.message();
            assert!(m.len() > 20, "too terse to act on: {m}");
            assert!(m.ends_with('.'), "not a sentence: {m}");
        }
    }

    #[test]
    fn game_images_are_recognised_by_extension() {
        assert!(is_game_image(Path::new("a/b/NSR.iso")));
        assert!(is_game_image(Path::new("a/b/NSR.RVZ")));
        assert!(is_game_image(Path::new("a/b/NSR.gcm")));
        assert!(!is_game_image(Path::new("a/b/NSR.txt")));
        assert!(!is_game_image(Path::new("a/b/NSR")));
    }

    /// Nothing to bind must produce no file rather than an empty one: an empty
    /// GCPadNew.ini is indistinguishable from a broken pad.
    #[test]
    fn no_controller_produces_no_pad_file() {
        let empty = Enumeration {
            devices: Vec::new(),
            not_searched: Vec::new(),
        };
        assert!(build_gcpad(&empty, 4).is_none());
    }
}
