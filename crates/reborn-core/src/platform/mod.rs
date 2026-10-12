//! The platform seams.
//!
//! Everything the launcher does that differs by operating system lives behind
//! one of the traits in this module. Nothing else in the codebase is allowed to
//! use `#[cfg(windows)]`, a drive letter, a `.exe` suffix, or a Win32 call.
//!
//! # Why this exists
//!
//! The plan is to build on Windows first and then port. That is only cheap if
//! the Windows-specific parts are behind a seam from the first commit. Writing
//! `C:\` inline and abstracting later turns a port into a rewrite, which is the
//! usual way this goes wrong.
//!
//! There are five seams, and they were chosen by working out which operations
//! genuinely cannot share an implementation:
//!
//! | Seam | Why it cannot be shared |
//! |---|---|
//! | [`Paths`] | Three different conventions for where user data lives |
//! | [`CredentialVault`] | Credential Manager, Keychain, libsecret |
//! | [`ProcessHost`] | Exit-code semantics differ, especially for crashes |
//! | [`FileOps`] | Windows locks open files; Unix does not |
//! | [`RemovableDrives`] | Volume enumeration has no portable API |
//!
//! # Status
//!
//! Every seam is implemented on Windows. The others compile from day one, which
//! is what keeps the port honest: the seam list cannot drift without the other
//! platforms failing to build.
//!
//! [`CredentialVault`] and [`RemovableDrives`] were stubs until Discord sign-in
//! and the Xbox wizard needed them. They now go through safe cross-platform
//! wrapper crates, so macOS and Linux get the same code rather than a rewrite.
//! Nobody has run them there, and `imp.rs` says so per platform rather than
//! claiming three ticks for one measurement.
//!
//! The one remaining refusal is [`RemovableDrives::eject`], which needs a
//! driver-level call this crate will not make; `imp.rs` explains why and what
//! the caller does instead.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

pub mod dir_move;
pub mod file_identity;
pub mod imp;
#[cfg(windows)]
pub(crate) mod runtime_process;
pub(crate) mod runtime_process_unix;

/// Errors a platform seam can produce.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The current platform has not implemented this operation yet.
    ///
    /// This is a real, expected variant rather than a `todo!()`. Callers can
    /// degrade gracefully, and the launcher stays usable on a platform where
    /// one seam is still a stub.
    #[error("{operation} is not implemented on {platform} yet")]
    NotImplemented {
        operation: &'static str,
        platform: &'static str,
    },

    #[error("no home or profile directory could be determined")]
    NoHomeDirectory,

    #[error("credential store rejected the request: {0}")]
    CredentialStore(String),

    #[error("io error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error(transparent)]
    PlainIo(#[from] std::io::Error),

    /// A refusal whose text is already a finished sentence for a player.
    ///
    /// # The shape this variant exists to make explicit
    ///
    /// Several modules refuse something with wording that was written for the
    /// player and is complete as it stands: "Close Reborn Dolphin before
    /// applying settings, then try again." is not a fault report, it is an
    /// instruction. Those used to be carried in [`Error::PlainIo`], which is
    /// transparent over an operating system error, so they were
    /// indistinguishable from `os error 112` and
    /// [`Error::player_message`] flattened them all to
    /// [`SOMETHING_UNEXPECTED`]. The Settings page's open-the-emulator button
    /// then answered "Something unexpected happened. Try again." to its single
    /// most common failure, which is false advice: the emulator is open and
    /// the player has to close it.
    ///
    /// So the distinction is a variant rather than a list of known strings.
    /// Anything constructed with [`Error::player`] is a sentence somebody
    /// wrote for a player, and reaches them unchanged; everything else is a
    /// fault, and is summarized. A future refusal is carried through by using
    /// the constructor, and cannot be silently flattened by not being on a
    /// list.
    ///
    /// Put the wording in a named constant beside the code that refuses, and
    /// hold it to the same rules as every other player-facing sentence: no
    /// path, no status code, no internal name. That is not left to whoever
    /// reads this: [`Error::player`] takes only a `&'static str`, so a
    /// `format!` cannot reach it at all, and
    /// `every_player_message_is_a_named_constant_that_reads_as_a_sentence`
    /// reads every construction in the workspace and holds each constant to
    /// the six sentence rules.
    #[error("{0}")]
    PlayerMessage(String),
}

pub type Result<T> = std::result::Result<T, Error>;

/// Shown when an operation could not reach a file or folder it needs: a save
/// or a settings file locked by something else, a permission problem, a full
/// disk. The one thing a player can do about any of those is close whatever
/// else might have it open, make room, and try again; naming the actual path
/// would not help, because it is very often a folder inside this launcher's
/// own data root that the player never chose and cannot usefully act on.
pub const FILE_UNREACHABLE: &str =
    "A file this launcher needs could not be read or written. Close anything else that might have it open, make sure the disk has room, and try again.";

/// Shown for every other way a platform operation can fail. Nothing here
/// distinguishes one cause from another for a player, and there is nothing
/// different for them to do about any of them, so they collapse to this one
/// sentence; the real detail still reaches a developer through
/// [`Error::player_message`]'s own `tracing::warn!`.
pub const SOMETHING_UNEXPECTED: &str = "Something unexpected happened. Try again.";

impl Error {
    /// A refusal whose wording is already the sentence the player should read.
    ///
    /// See [`Error::PlayerMessage`] for when this is the right constructor and
    /// when it is not. In short: an instruction somebody wrote for a player,
    /// never an operating system message and never a formatted fault.
    ///
    /// # Why this takes `&'static str` and not `impl Into<String>`
    ///
    /// Because what it is handed goes to a screen unchanged. It used to take
    /// anything a `String` could be made from, so `Error::player(format!("could
    /// not write {}: {e}", path.display()))` compiled, said nothing, and put a
    /// path and an operating system error number in front of a player. Nothing
    /// stopped it but a doc comment asking for a named constant, and a hundred
    /// and ninety more sites are queued to be routed through this.
    ///
    /// A `&'static str` is a constant or a literal and cannot be a `format!`,
    /// so the compiler now answers that at every call site. The scan named on
    /// [`Error::PlayerMessage`] closes the rest of the gap: it rejects a
    /// literal written at the call site as well, and holds each constant's
    /// wording to the six sentence rules.
    ///
    /// A sentence that genuinely has to name something the player chose is not
    /// what this is for. Give it its own variant, so the thing it interpolates
    /// is typed and can be checked, rather than widening this back to any
    /// string at all.
    pub fn player(message: &'static str) -> Self {
        Error::PlayerMessage(message.to_string())
    }

    /// A sentence a player can read, in place of this error's own `Display`.
    ///
    /// [`Error::Io`] displays as `io error at {path}: {source}`, and
    /// [`Error::PlainIo`] carries the same operating system wording with no
    /// path attached. Neither is vocabulary a player was ever taught, and a
    /// permission problem or a full disk starting a game used to put a full
    /// file path and text like `os error 112` on the Play page because a
    /// caller wrote `.map_err(|e| e.to_string())`. This is the replacement,
    /// once, so every caller gets the same two sentences instead of
    /// repeating this decision at each call site.
    ///
    /// Only two categories, because a player can only act on two things
    /// differently: a file or folder problem (close whatever has it open,
    /// make room, try again) versus everything else (try again). The real
    /// error still reaches a developer, through `tracing::warn!` here rather
    /// than at each call site, so no caller has to remember to log it.
    pub fn player_message(&self) -> &str {
        match self {
            // Already a sentence somebody wrote for a player. Nothing to
            // summarize, nothing to log: this is not a fault, it is an answer.
            Error::PlayerMessage(message) => message,
            Error::Io { .. } | Error::NoHomeDirectory => {
                tracing::warn!("could not reach a file or folder: {self}");
                FILE_UNREACHABLE
            }
            Error::PlainIo(_) | Error::CredentialStore(_) | Error::NotImplemented { .. } => {
                tracing::warn!("a platform operation failed: {self}");
                SOMETHING_UNEXPECTED
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// gamescope announces itself three ways; any one is Game Mode. A Deck in
    /// Desktop Mode (KDE) and every PC have none of them.
    #[test]
    fn game_mode_is_recognised_by_any_of_gamescope_s_three_signs() {
        let env = |pairs: &'static [(&'static str, &'static str)]| {
            move |k: &str| pairs.iter().find(|(n, _)| *n == k).map(|(_, v)| v.to_string())
        };
        let linux = cfg!(target_os = "linux");
        assert_eq!(game_mode_from(env(&[("GAMESCOPE_WAYLAND_DISPLAY", "gamescope-0")])), linux);
        assert_eq!(game_mode_from(env(&[("XDG_CURRENT_DESKTOP", "gamescope")])), linux);
        assert_eq!(game_mode_from(env(&[("SteamGamepadUI", "1")])), linux);
        assert!(!game_mode_from(env(&[("XDG_CURRENT_DESKTOP", "KDE")])), "Desktop Mode");
        assert!(!game_mode_from(env(&[("GAMESCOPE_WAYLAND_DISPLAY", "")])), "set but empty");
        assert!(!game_mode_from(env(&[])), "a PC");
    }

    /// Every shape [`Error`] can take reads as a plain sentence through
    /// [`Error::player_message`], mechanically rather than as a list of
    /// expected strings, using the same six rules `identity.rs` established.
    #[test]
    fn every_platform_error_reads_as_a_plain_sentence() {
        use crate::player_sentence::assert_player_safe;

        let io = Error::Io {
            path: PathBuf::from(r"C:\Users\Adam\AppData\Roaming\NFL Street Reborn\settings.json"),
            source: std::io::Error::from_raw_os_error(112), // ERROR_DISK_FULL
        };
        assert_player_safe(io.player_message());

        assert_player_safe(Error::NoHomeDirectory.player_message());

        let plain = Error::PlainIo(std::io::Error::new(std::io::ErrorKind::Other, "something odd"));
        assert_player_safe(plain.player_message());

        let credential = Error::CredentialStore("the vault refused: 0x8009000".to_string());
        assert_player_safe(credential.player_message());

        let not_implemented = Error::NotImplemented {
            operation: "eject",
            platform: "Windows",
        };
        assert_player_safe(not_implemented.player_message());

        assert_player_safe(
            Error::player(crate::runtime::CLOSE_EMULATOR_FIRST).player_message(),
        );
    }

    /// A message that is already a finished player sentence survives.
    ///
    /// Two modules refuse something with wording written for the player:
    /// [`crate::runtime::CLOSE_EMULATOR_FIRST`] and
    /// [`crate::supervisor::OLDER_INSTALL_IN_THE_WAY`]. Both used to be
    /// carried in [`Error::PlainIo`], which is transparent over an operating
    /// system error, so [`Error::player_message`] could not tell them from
    /// `os error 112` and replaced both with "Something unexpected happened.
    /// Try again." The Settings page's open-the-emulator button then gave
    /// false advice for its single most common failure: the emulator is
    /// already open, and the player has to close it.
    ///
    /// The distinction is the variant, not the two strings. Anything built
    /// with [`Error::player`] reaches the player unchanged, so the next person
    /// who writes such a sentence is carried through by using the constructor
    /// rather than by being added to a list; anything else is a fault and is
    /// still summarized, which the second half asserts.
    #[test]
    fn a_message_that_is_already_a_player_sentence_survives() {
        use crate::player_sentence::assert_player_safe;

        for designed in [
            crate::runtime::CLOSE_EMULATOR_FIRST,
            crate::supervisor::OLDER_INSTALL_IN_THE_WAY,
        ] {
            assert_player_safe(designed);
            assert_eq!(
                Error::player(designed).player_message(),
                designed,
                "a sentence written for the player was flattened into a generic one"
            );
        }

        // A sentence nobody in this crate has written yet is carried too. The
        // shape is what decides, not a list of known strings.
        let future = "The other player left before the game finished. Nothing was lost.";
        assert_eq!(Error::player(future).player_message(), future);

        // And the half that must not change: an operating system failure is
        // still a fault, and is still summarized rather than shown.
        let disk_full = Error::PlainIo(std::io::Error::from_raw_os_error(112));
        assert_eq!(disk_full.player_message(), SOMETHING_UNEXPECTED);
        let locked = Error::Io {
            path: PathBuf::from(r"C:\Users\Adam\AppData\Roaming\settings.json"),
            source: std::io::Error::from_raw_os_error(32),
        };
        assert_eq!(locked.player_message(), FILE_UNREACHABLE);
    }

    /// Every player sentence carried by [`Error::PlayerMessage`] is a named
    /// constant, and every one of those constants reads as a plain sentence.
    ///
    /// [`Error::player`] hands its argument straight to a screen, unchanged
    /// and unsummarized. That is the point of it, and it is also the hazard:
    /// one `format!` in one careless call puts a path or an operating system
    /// error number in front of a player, and nothing stopped it. The doc
    /// comment asked for a named constant, which is a convention somebody has
    /// to remember, and a convention is exactly what this variant was chosen
    /// over a list of strings to avoid. There is recorded follow-up work
    /// covering a hundred and ninety similar sites in other modules; that is
    /// where the careless call gets written.
    ///
    /// So the rule is mechanical, in two halves:
    ///
    /// - nothing outside this file may name the variant, so the constructor is
    ///   the only way to build one
    /// - every argument to the constructor is a bare named constant: never a
    ///   `format!`, never a `to_string()`, never a string literal written at
    ///   the call site, never any other expression
    ///
    /// Then, because "it is a constant" says nothing about what the constant
    /// says, each one is resolved to its text and held to the same six rules
    /// every other player sentence is held to. That is thirteen constants
    /// rather than the two an earlier test named by hand.
    ///
    /// # How this is bounded so it cannot find itself
    ///
    /// Every file is read through [`crate::source_scan::production_only`],
    /// which blanks EVERY `#[cfg(test)]` module rather than cutting at the
    /// first one. This test is inside such a module, so the constructor calls
    /// written in it, and the words `format!` and `to_string` above, are not
    /// in what is searched. The scan also asserts it found constructions at
    /// all, so a rename that makes it match nothing fails rather than passing
    /// silently.
    #[test]
    fn every_player_message_is_a_named_constant_that_reads_as_a_sentence() {
        use crate::source_scan::{closing_bracket, code_matches, code_only, const_str_value,
            production_only, workspace_rust_files};

        let files = workspace_rust_files();
        assert!(
            files.len() > 20,
            "only {} source files were found, so this scan checked almost nothing",
            files.len()
        );

        let mut constructions = 0usize;
        for file in &files {
            let text = std::fs::read_to_string(file).expect("a source file this test just listed");
            let production = production_only(&text);
            let code = code_only(&production);
            let here = file.ends_with("platform/mod.rs") || file.ends_with("platform\\mod.rs");

            if !here {
                assert!(
                    code_matches(&code, "PlayerMessage").is_empty(),
                    "{} names the player-message variant directly, so it can build one without \
                     going through the constructor that is checked below",
                    file.display()
                );
            }

            for at in code_matches(&code, "Error::player(") {
                let open = at + "Error::player".len();
                let close = closing_bracket(&code, open).unwrap_or_else(|| {
                    panic!("an unclosed Error::player call in {}", file.display())
                });
                let argument = production[open + 1..close].trim();
                constructions += 1;
                let last = argument.rsplit("::").next().unwrap_or(argument);
                let named_constant = !last.is_empty()
                    && last
                        .chars()
                        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
                    && argument
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == ':');
                assert!(
                    named_constant,
                    "a sentence a player reads is built out of an expression rather than a named \
                     constant, in {}: Error::player({argument}). Anything but a constant can \
                     carry a path or an operating system error number straight to a screen",
                    file.display()
                );
                let value = const_str_value(last).unwrap_or_else(|| {
                    panic!(
                        "{} builds a player message from {last}, and no `const {last}: &str` \
                         declaring its wording could be found to check",
                        file.display()
                    )
                });
                crate::player_sentence::assert_player_safe(&value);
            }
        }

        // Twenty eight today, across three modules. The floor is well under
        // that so a module legitimately dropping a refusal does not fail this,
        // and well over zero so a rename that makes the scan match nothing
        // does.
        assert!(
            constructions >= 10,
            "only {constructions} player-message constructions were found, so this scan is not \
             reading what it thinks it is reading"
        );
    }

    /// A file or folder problem and an unexpected one read as two different
    /// sentences, because they are the two things a player can act on
    /// differently; everything else collapses to whichever of the two it is
    /// closest to, rather than the launcher inventing a third choice with
    /// nothing different to tell a player to do.
    #[test]
    fn a_file_problem_and_an_unexpected_one_are_told_apart() {
        let io = Error::Io {
            path: PathBuf::from("settings.json"),
            source: std::io::Error::from_raw_os_error(5), // ERROR_ACCESS_DENIED
        };
        assert_eq!(io.player_message(), FILE_UNREACHABLE);
        assert_eq!(Error::NoHomeDirectory.player_message(), FILE_UNREACHABLE);

        let plain = Error::PlainIo(std::io::Error::new(std::io::ErrorKind::Other, "x"));
        assert_eq!(plain.player_message(), SOMETHING_UNEXPECTED);
        assert_eq!(
            Error::CredentialStore("x".to_string()).player_message(),
            SOMETHING_UNEXPECTED
        );
        assert_eq!(
            Error::NotImplemented { operation: "eject", platform: "Windows" }.player_message(),
            SOMETHING_UNEXPECTED
        );
    }
}

/// Helper for the stub implementations.
pub(crate) fn not_implemented<T>(operation: &'static str) -> Result<T> {
    Err(Error::NotImplemented {
        operation,
        platform: CURRENT_PLATFORM,
    })
}

/// Whether the launcher is running inside Steam Deck's Game Mode.
///
/// Game Mode is gamescope, a compositor that shows ONE window at a time,
/// fullscreen, with no title bars and no desktop behind it. A launcher that
/// starts a game and stays visible can hide the game behind itself, so in
/// Game Mode the launcher steps aside while the game runs. Desktop Mode on
/// a Deck, and every PC, answers false and nothing changes.
pub fn in_game_mode() -> bool {
    game_mode_from(|key| std::env::var(key).ok())
}

/// [`in_game_mode`] over any source of environment variables, so the rule
/// is testable. gamescope announces itself three ways and any one counts:
/// its own Wayland display name, the desktop name it sets, and Steam's flag
/// for the gamepad UI it runs.
pub fn game_mode_from(var: impl Fn(&str) -> Option<String>) -> bool {
    if !cfg!(target_os = "linux") {
        return false;
    }
    let set = |k: &str| var(k).is_some_and(|v| !v.is_empty());
    set("GAMESCOPE_WAYLAND_DISPLAY")
        || var("XDG_CURRENT_DESKTOP").is_some_and(|v| v.to_ascii_lowercase().contains("gamescope"))
        || var("SteamGamepadUI").as_deref() == Some("1")
}

/// The platform this binary was compiled for.
pub const CURRENT_PLATFORM: &str = if cfg!(windows) {
    "Windows"
} else if cfg!(target_os = "macos") {
    "macOS"
} else if cfg!(target_os = "linux") {
    "Linux"
} else {
    "an unsupported platform"
};

// ---------------------------------------------------------------------------
// Seam 1: paths
// ---------------------------------------------------------------------------

/// Which release channel this launcher is, which decides where it keeps things.
///
/// Beta and stable install side by side on one machine, each with its own
/// install root, data root, cache and uninstall entry. That is what makes a
/// beta a test environment rather than a gamble with the player's real one:
/// nothing a beta writes lands where the stable launcher reads, and removing
/// it removes only itself.
///
/// Read from `REBORN_CHANNEL` first, so a developer can run any build as beta,
/// then from a `channel` file beside the executable, which the beta installer
/// lays down. Absent means stable. An unknown name ALSO means stable, and is
/// not an error, because a launcher must start; what was found is carried so
/// the interface can say it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Channel {
    #[default]
    Stable,
    Beta,
}

impl Channel {
    /// The marker file beside the executable.
    pub const MARKER_FILE: &'static str = "channel";
    /// The environment override.
    pub const ENV: &'static str = "REBORN_CHANNEL";

    /// Parse a channel name. Case and surrounding whitespace do not matter;
    /// an empty name is stable, because an empty marker file is the natural
    /// way to say "nothing special".
    pub fn from_name(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "" | "stable" => Some(Channel::Stable),
            "beta" => Some(Channel::Beta),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Channel::Stable => "stable",
            Channel::Beta => "beta",
        }
    }

    /// What is appended to every product-named directory.
    pub fn suffix(self) -> &'static str {
        match self {
            Channel::Stable => "",
            Channel::Beta => " Beta",
        }
    }

    /// Decide from the two sources, in order. Pure, so it can be tested
    /// without touching the process environment or the disk.
    pub fn resolve(env: Option<&str>, marker: Option<&str>) -> Resolved {
        if let Some(value) = env {
            return match Channel::from_name(value) {
                Some(channel) => Resolved { channel, source: ChannelSource::Environment },
                None => Resolved {
                    channel: Channel::Stable,
                    source: ChannelSource::Unknown(value.trim().to_string()),
                },
            };
        }
        if let Some(value) = marker {
            return match Channel::from_name(value) {
                Some(channel) => Resolved { channel, source: ChannelSource::MarkerFile },
                None => Resolved {
                    channel: Channel::Stable,
                    source: ChannelSource::Unknown(value.trim().to_string()),
                },
            };
        }
        Resolved { channel: Channel::Stable, source: ChannelSource::Absent }
    }

    /// Read the environment and the marker file beside the running executable.
    pub fn detect() -> Resolved {
        let env = std::env::var(Self::ENV).ok();
        let marker = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(|d| d.join(Self::MARKER_FILE)))
            .and_then(|path| std::fs::read_to_string(path).ok());
        Self::resolve(env.as_deref(), marker.as_deref())
    }
}

/// Where a channel decision came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChannelSource {
    Environment,
    MarkerFile,
    /// Neither source had anything: stable by default.
    Absent,
    /// A name this launcher does not know, carried verbatim. Stable was used.
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub channel: Channel,
    pub source: ChannelSource,
}

impl Resolved {
    /// One line for the interface.
    pub fn explain(&self) -> String {
        match &self.source {
            ChannelSource::Environment => {
                format!("{} channel, set by {}.", self.channel.name(), Channel::ENV)
            }
            ChannelSource::MarkerFile => format!(
                "{} channel, from the {} file beside the launcher.",
                self.channel.name(),
                Channel::MARKER_FILE
            ),
            ChannelSource::Absent => "stable channel.".to_string(),
            ChannelSource::Unknown(name) => format!(
                "stable channel. A channel called {name:?} was named and is not one this \
                 launcher knows, so it was ignored."
            ),
        }
    }
}

/// Where the launcher is allowed to put things.
///
/// Only two roots exist, and keeping them apart is a safety property rather
/// than tidiness. The current 2.1 installer runs `DelTree` over an app folder
/// that contains the user's memory cards, which is why uninstalling deletes
/// saves. If the install root physically cannot contain user data, that class
/// of bug cannot be written.
pub trait Paths: Send + Sync {
    /// Program files. The installer owns this and may replace it wholesale.
    fn install_root(&self) -> Result<PathBuf>;

    /// User data. The launcher owns this and it survives uninstall by default.
    fn data_root(&self) -> Result<PathBuf>;

    /// Per-platform cache, safe to purge at any time.
    fn cache_root(&self) -> Result<PathBuf>;

    /// The data root a build before 2026-09-12 used (`... Launcher[ Beta]`),
    /// so a first start under the new name can adopt what is there. `None`
    /// where no such folder was ever used (test fakes).
    fn previous_data_root(&self) -> Result<Option<PathBuf>> {
        Ok(None)
    }

    /// The executable suffix, empty on Unix.
    fn exe_suffix(&self) -> &'static str;

    /// Which channel these roots belong to. Stable unless an implementation
    /// says otherwise, so test fakes need not care.
    fn channel(&self) -> Channel {
        Channel::Stable
    }
}

/// Subdirectories of the data root.
///
/// From `docs/architecture/SYSTEM_ARCHITECTURE.md` section 4, which is the sole
/// authority for the layout. Three documents used to restate it and all three
/// differed, so it is expressed once, here, in code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DataDir {
    Downloads,
    Staging,
    LibraryOffline,
    LibraryOnline,
    LibraryTextures,
    LibraryXbox,
    LibraryWork,
    /// The roster the site published, as delivered. One small file that the
    /// launcher applies to the live image with the Reborn Builder.
    LibraryRoster,
    /// The game-patch component: `chain.json` and the xdeltas between
    /// consecutive Reborn builds, applied to the pristine image.
    LibraryGamePatch,
    /// The `game-full` recovery component: the downloaded copy of the
    /// complete disc, before it becomes a pristine image. Deliberately its
    /// own directory rather than `LibraryOffline`: that one holds the
    /// BOOTED image (`crate::game_image::booted_path`), which a roster apply
    /// rewrites in place, and pristine images live at
    /// `library/imported/<sha256>` (`crate::game_image`). A downloaded disc
    /// is neither of those until something promotes it, so its landing spot
    /// must collide with neither.
    LibraryGameFull,
    /// The game-import bundle (patch manifest + reborn.xdelta) as content:
    /// what the setup wizard patches a player's own disc with.
    LibraryGameImport,
    Runtime,
    ModsStaging,
    ModsPackages,
    MusicSources,
    MusicPacks,
    /// Memory cards. Never deleted without a separate explicit confirmation.
    Saves,
    /// Screenshots the player took. Not regenerable, so not derived.
    Screenshots,
    StatsQueue,
    Versions,
    Recovery,
    BackupsXbox,
    Cache,
    Logs,
    Config,
}

impl DataDir {
    /// The path relative to the data root.
    pub fn relative(self) -> &'static str {
        match self {
            DataDir::Downloads => "downloads",
            DataDir::Staging => "staging",
            DataDir::LibraryOffline => "library/offline",
            DataDir::LibraryOnline => "library/online",
            DataDir::LibraryTextures => "library/textures",
            DataDir::LibraryXbox => "library/xbox",
            DataDir::LibraryWork => "library/work",
            DataDir::LibraryRoster => "library/roster",
            DataDir::LibraryGamePatch => "library/game-patch",
            DataDir::LibraryGameFull => "library/game-full",
            DataDir::LibraryGameImport => "library/game-import",
            DataDir::Runtime => "runtime",
            DataDir::ModsStaging => "mods/staging",
            DataDir::ModsPackages => "mods/packages",
            DataDir::MusicSources => "music/sources",
            DataDir::MusicPacks => "music/packs",
            DataDir::Saves => "saves",
            DataDir::StatsQueue => "stats/queue",
            DataDir::Versions => "versions",
            DataDir::Recovery => "recovery",
            DataDir::BackupsXbox => "backups/xbox",
            DataDir::Cache => "cache",
            DataDir::Screenshots => "screenshots",
            DataDir::Logs => "logs",
            DataDir::Config => "config",
        }
    }

    /// Every directory, for creation and for tests.
    pub const ALL: &'static [DataDir] = &[
        DataDir::Downloads,
        DataDir::Staging,
        DataDir::LibraryOffline,
        DataDir::LibraryOnline,
        DataDir::LibraryTextures,
        DataDir::LibraryXbox,
        DataDir::LibraryWork,
        DataDir::LibraryRoster,
        DataDir::LibraryGamePatch,
        DataDir::LibraryGameFull,
        DataDir::LibraryGameImport,
        DataDir::Runtime,
        DataDir::ModsStaging,
        DataDir::ModsPackages,
        DataDir::MusicSources,
        DataDir::MusicPacks,
        DataDir::Saves,
        DataDir::StatsQueue,
        DataDir::Versions,
        DataDir::Recovery,
        DataDir::BackupsXbox,
        DataDir::Cache,
        DataDir::Screenshots,
        DataDir::Logs,
        DataDir::Config,
    ];

    /// Whether deleting this directory destroys something the user cannot get
    /// back.
    ///
    /// Used to gate any bulk removal. `music/sources` holds files the user
    /// supplied, `saves` holds memory cards, and `stats` holds captures that
    /// may not have uploaded yet.
    pub fn is_user_data(self) -> bool {
        matches!(
            self,
            DataDir::Saves
                | DataDir::Screenshots
                | DataDir::MusicSources
                | DataDir::MusicPacks
                | DataDir::StatsQueue
        )
    }

    /// Whether the contents can be regenerated, so purging is safe.
    pub fn is_derived(self) -> bool {
        matches!(
            self,
            DataDir::Cache
                | DataDir::Downloads
                | DataDir::Staging
                | DataDir::LibraryWork
                | DataDir::ModsStaging
        )
    }
}

// ---------------------------------------------------------------------------
// Seam 2: credentials
// ---------------------------------------------------------------------------

/// Secret storage backed by the operating system.
///
/// Nothing the launcher holds is ever written to disk in plaintext. The reason
/// this seam is not optional: the shipped Dolphin currently carries a bearer
/// token compiled into the binary, which cannot be rotated without a rebuild
/// and is readable by anyone with the executable.
pub trait CredentialVault: Send + Sync {
    fn store(&self, account: &str, secret: &str) -> Result<()>;
    fn retrieve(&self, account: &str) -> Result<Option<String>>;
    fn delete(&self, account: &str) -> Result<()>;
}

// ---------------------------------------------------------------------------
// Seam 3: processes
// ---------------------------------------------------------------------------

/// How a child process ended.
///
/// The supervisor has to distinguish these to decide what the user is told.
/// "The game crashed" and "you quit the game" look identical if all you have is
/// a non-zero exit code, and Windows and Unix disagree about how a crash is
/// even reported: Unix delivers a signal, Windows returns an exception code as
/// the exit status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExitKind {
    /// Exited cleanly with status 0.
    Normal,
    /// Exited with a non-zero status that does not look like a crash.
    Error { code: i32 },
    /// Crashed. Access violation, signal, or an exception exit code.
    Crashed { detail: String },
    /// Killed by the launcher or the user.
    Terminated,
}

impl ExitKind {
    /// Whether this should show the crash recovery page.
    pub fn is_crash(&self) -> bool {
        matches!(self, ExitKind::Crashed { .. })
    }
}

/// Launching and supervising the game.
pub trait ProcessHost: Send + Sync {
    /// Start a child process and return an opaque handle.
    ///
    /// `env` is added to the inherited environment. It is a parameter rather
    /// than something the caller sets globally because the launcher is one
    /// process that may start several children, and mutating its own
    /// environment to configure a child is a race.
    fn spawn(
        &self,
        program: &Path,
        args: &[OsString],
        cwd: Option<&Path>,
        env: &BTreeMap<String, OsString>,
    ) -> Result<ProcessHandle>;

    /// Block until the child exits, then classify how it ended.
    fn wait(&self, handle: ProcessHandle) -> Result<ExitKind>;

    /// Whether a process with this identifier is still running.
    fn is_running(&self, handle: &ProcessHandle) -> bool;

    /// Run a program to completion and capture what it printed.
    ///
    /// Separate from `spawn` because the game is watched and never read,
    /// while a short query is read and never watched. One method doing both
    /// would have to buffer the output of a process that runs for an hour.
    fn run_capturing(
        &self,
        program: &Path,
        args: &[OsString],
        env: &BTreeMap<String, OsString>,
    ) -> Result<CapturedRun>;
}

/// An opaque handle to a spawned child.
#[derive(Debug)]
pub struct ProcessHandle {
    pub(crate) pid: u32,
    pub(crate) inner: Option<std::process::Child>,
}

impl ProcessHandle {
    pub fn pid(&self) -> u32 {
        self.pid
    }
}

// ---------------------------------------------------------------------------
// Seam 4: file operations
// ---------------------------------------------------------------------------

/// Filesystem operations whose guarantees differ by platform.
///
/// The dangerous one is [`FileOps::atomic_replace`]. A rename is only atomic
/// within a single filesystem, and the launcher's install root and data root
/// are frequently on different volumes. A swap that renames between them is not
/// atomic and can leave the product in neither state, which is why the launcher
/// self-update copies into the install root first and only then renames within
/// it.
pub trait FileOps: Send + Sync {
    /// Replace `dest` with `src` atomically. Both must be on one volume.
    fn atomic_replace(&self, src: &Path, dest: &Path) -> Result<()>;

    /// Whether these two paths are on the same volume.
    ///
    /// Callers must check this before relying on [`FileOps::atomic_replace`],
    /// and before assuming a hardlink will work.
    fn same_volume(&self, a: &Path, b: &Path) -> Result<bool>;

    /// Create a hardlink, used to compose the active mod tree over the pristine
    /// texture tree without ever writing into the pristine one.
    ///
    /// Fails on filesystems without hardlink support, notably exFAT and FAT32,
    /// where the caller must fall back to copying. On a handheld with an SD
    /// card that fallback is the common case, not an edge case.
    fn hardlink(&self, src: &Path, dest: &Path) -> Result<()>;

    /// Whether the file is currently held open by another process.
    ///
    /// Meaningful on Windows, where an open file cannot be replaced. On Unix it
    /// is advisory at best, and the honest answer is usually `false`.
    fn is_locked(&self, path: &Path) -> bool;

    /// Take an exclusive advisory lock on `path`, creating it if needed.
    ///
    /// Returns `Ok(None)` when another live process already holds the lock.
    ///
    /// Why a lock and not a recorded process id: a stale pid file cannot be
    /// told apart from a live one, because the operating system reuses process
    /// ids. A process that dies without cleaning up leaves a pid that may
    /// already belong to something unrelated, so "is that pid alive" is the
    /// wrong question. An OS-held lock is released when the holder dies for any
    /// reason, including a kill, a power cut, or a crash, so the question
    /// becomes "can I take it", which has no false positive.
    fn lock_exclusive(&self, path: &Path) -> Result<Option<LockGuard>>;
}

// ---------------------------------------------------------------------------
// Seam 5: removable drives
// ---------------------------------------------------------------------------

/// A removable volume, as offered to the user in the Xbox wizard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemovableDrive {
    /// What the user sees, for example `E:` or `/Volumes/XBOX-USB`.
    pub mount_point: PathBuf,
    /// Volume label, if the filesystem has one.
    pub label: Option<String>,
    pub total_bytes: u64,
    pub free_bytes: u64,
    /// `exFAT`, `NTFS`, `FAT32`, and so on. Matters because hardlinks and file
    /// size limits differ.
    pub filesystem: Option<String>,
    /// A value stable across remounts, used to recognize a drive the wizard has
    /// written before. A drive letter is not stable and must never be used for
    /// this.
    ///
    /// `None` today on every platform, and that is not a placeholder waiting to
    /// be filled in with something approximate. The stable answer is the volume
    /// GUID path, which `sysinfo` computes internally and does not expose, and
    /// which otherwise needs a Win32 call this crate will not make. A label is
    /// not a substitute: two sticks can both be called `XBOX-USB`, and treating
    /// them as one drive is worse than having no identity at all.
    ///
    /// What absent costs, so it is not discovered later: `xbox::Target`'s
    /// journal key is `None` without an identity, so a copy that is interrupted
    /// starts over instead of resuming. That is the intended degradation, not a
    /// second bug.
    pub volume_identity: Option<String>,
}

/// Whether the operating system says a volume can be unplugged.
///
/// This distinction is the reason the seam exists. The Xbox wizard copies about
/// 3 GB to whatever it is handed, so a fixed disk offered once is that payload
/// written into the middle of somebody's system drive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VolumeKind {
    /// The operating system reported this volume as removable media.
    Removable,
    /// Everything else, which deliberately includes "the operating system would
    /// not say".
    ///
    /// There is no `Unknown` variant on purpose. Every enumerator behind this
    /// seam answers an unknown volume with "not removable", so the failure
    /// direction is a legitimate USB hard disk that Windows calls a fixed disk
    /// not appearing in the picker. That is a user who has to be told why their
    /// drive is missing. The other direction is a wizard writing 3 GB to `C:`.
    Fixed,
}

/// A volume, and whether it can be unplugged.
///
/// The payload is a [`RemovableDrive`] even when the volume is fixed, because
/// the measurements are identical and a second struct with the same six fields
/// is a second struct to keep in step. What the name promises is enforced by
/// [`RemovableDrives::enumerate`], which is the only thing that hands out a
/// `RemovableDrive` on its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Volume {
    pub drive: RemovableDrive,
    pub kind: VolumeKind,
}

impl Volume {
    pub fn is_removable(&self) -> bool {
        self.kind == VolumeKind::Removable
    }
}

/// Enumerating drives the Xbox wizard may write to.
///
/// The wizard never auto-selects when more than one removable drive exists;
/// that rule is enforced above this seam. The removable filter itself belongs
/// here, because this is the only layer that knows what the operating system
/// actually said.
pub trait RemovableDrives: Send + Sync {
    /// Every volume the operating system will admit to, removable or not.
    ///
    /// This is the method a platform implements. Reporting fixed volumes too is
    /// not for show: "no drives found" and "one drive found and it is your
    /// system disk" are different facts, and a picker that shows neither leaves
    /// the user believing their stick is unplugged.
    fn enumerate_volumes(&self) -> Result<Vec<Volume>>;

    /// Only the volumes the Xbox wizard may write to.
    ///
    /// Deliberately a provided method. If each platform filtered for itself,
    /// then a port that forgot to would be a port that offers the system drive,
    /// and nothing about the signature would show it. Written once, it cannot be
    /// forgotten once.
    ///
    /// The volume the launcher runs from is never offered either, removable or
    /// not: a Mac player running the app straight out of the opened .dmg, or a
    /// launcher started from the very stick the wizard would write.
    fn enumerate(&self) -> Result<Vec<RemovableDrive>> {
        let exe = std::env::current_exe().ok();
        Ok(self
            .enumerate_volumes()?
            .into_iter()
            .filter(Volume::is_removable)
            .filter(|v| !exe.as_ref().is_some_and(|e| e.starts_with(&v.drive.mount_point)))
            .map(|volume| volume.drive)
            .collect())
    }

    /// Flush writes and make the volume safe to unplug.
    fn eject(&self, mount_point: &Path) -> Result<()>;
}

// ---------------------------------------------------------------------------
// The bundle
// ---------------------------------------------------------------------------

/// Every seam, resolved for the platform this binary was built for.
///
/// Application code takes a `&Platform` and never asks which operating system
/// it is running on.
pub struct Platform {
    pub paths: Box<dyn Paths>,
    pub credentials: Box<dyn CredentialVault>,
    pub processes: Box<dyn ProcessHost>,
    pub files: Box<dyn FileOps>,
    pub drives: Box<dyn RemovableDrives>,
}

impl Platform {
    /// Build the seam set for the current platform.
    pub fn detect() -> Self {
        imp::platform()
    }

    /// Absolute path of a data directory, creating it if absent.
    pub fn data_dir(&self, dir: DataDir) -> Result<PathBuf> {
        let path = self.paths.data_root()?.join(dir.relative());
        std::fs::create_dir_all(&path).map_err(|source| Error::Io {
            path: path.clone(),
            source,
        })?;
        Ok(path)
    }
}

impl std::fmt::Debug for Platform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Platform")
            .field("platform", &CURRENT_PLATFORM)
            .field("exe_suffix", &self.paths.exe_suffix())
            .finish()
    }
}

/// An exclusive lock held for as long as this value is alive.
///
/// The lock is advisory and is released by the operating system when the
/// holding process exits, however it exits. Dropping the guard closes the file,
/// which releases the lock; the file itself is deliberately left on disk,
/// because deleting it would race another process that has already opened it.
#[derive(Debug)]
pub struct LockGuard {
    #[allow(dead_code)]
    pub(crate) file: std::fs::File,
    pub(crate) path: PathBuf,
}

impl LockGuard {
    /// The file backing this lock.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// The result of running a program that was expected to finish quickly.
#[derive(Debug, Clone)]
pub struct CapturedRun {
    pub stdout: String,
    pub stderr: String,
    pub exit: ExitKind,
}

impl CapturedRun {
    /// Whether the program finished successfully.
    pub fn succeeded(&self) -> bool {
        self.exit == ExitKind::Normal
    }
}
