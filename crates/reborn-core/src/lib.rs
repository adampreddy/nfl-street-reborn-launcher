//! Platform-agnostic core for NFL Street Reborn Launcher.
//!
//! # The one rule
//!
//! Outside [`platform`], this crate contains no `#[cfg(windows)]`, no drive
//! letter, no `.exe` suffix, and no Win32 call. Everything that differs by
//! operating system lives behind a trait in that module.
//!
//! The plan is to build on Windows first and port afterwards. That is only
//! cheap if the seams exist from the first commit, so they do.
//!
//! # What is here so far
//!
//! - [`platform`]: the five seams, Windows implemented, macOS and Linux
//!   compiling.
//! - [`manifest`]: the signed release and component schemas.
//! - [`library`]: verify and repair, including the rule that keeps repair from
//!   deleting a user's custom soundtrack.
//! - [`runtime`]: generated emulator configuration, direct boot, and the
//!   packaging audit that keeps the debug markers out of a release.
//! - [`controller`]: device identity, one default mapping that is overridable
//!   at any time, and generated GCPadNew.ini.
//! - [`migration`]: moving a 2.1 install, saves first, mods un-applied before
//!   textures are trusted.
//!
//! - [`session`]: the launch sequence, in the core rather than in a front end,
//!   so the window and the command line cannot drift about what a launch does.
//! - [`supervisor`]: the single-instance lock, and the session record that
//!   survives the launcher going down with the game.
//!
//! - [`devices`]: finding controllers and naming them the way the emulator
//!   names them, which is the difference between a pad that works and a file
//!   that looks correct and binds nothing.
//!
//! Not written yet: SDL device enumeration, mods,
//! music, online, and the Xbox wizard.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

pub mod backend;
pub mod content;
pub mod controller;
pub mod controller_profiles;
pub mod devices;
pub mod gamepad;
pub mod game_image;
pub mod game_import;
pub mod game_patch;
pub mod identity;
pub mod ini;
pub mod inventory;
pub mod jsgme;
pub mod library;
pub mod logging;
pub mod manifest;
pub mod migration;
pub mod mods;
pub mod music;
pub mod net;
pub mod onboarding;
pub mod origin;
pub mod platform;
pub mod roster;
pub mod runtime;
pub mod reshade;
pub mod session;
pub mod settings;
pub mod signing;
pub mod stats;
pub mod supervisor;
pub mod update;
pub mod xbox;

pub use platform::{
    CredentialVault, DataDir, Error, ExitKind, FileOps, Paths, Platform, ProcessHost,
    RemovableDrive, RemovableDrives, Result, CURRENT_PLATFORM, Channel, ChannelSource};

/// The wall clock, in UTC, as `YYYY-MM-DDTHH:MM:SSZ`.
///
/// Here rather than in one module because two need it and neither owns time:
/// [`backend`] stamps every envelope with it, and [`logging`] stamps every
/// line. No dependency: civil time from seconds is Howard Hinnant's algorithm,
/// which is a dozen lines and has no calendar to get out of date.
pub fn utc_now_iso() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = (secs / 86_400) as i64;
    let (h, m, s) = ((secs % 86_400) / 3600, (secs % 3600) / 60, secs % 60);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mth = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if mth <= 2 { y + 1 } else { y };
    format!("{y:04}-{mth:02}-{d:02}T{h:02}:{m:02}:{s:02}Z")
}

/// The mechanical "no errors ever" check, shared by every module that produces
/// a sentence a player reads.
///
/// Task 10b established this in [`identity`]: named constants for the wording,
/// the technical detail to `tracing::warn!`, and a test that asserts the
/// wording mechanically rather than against a hand-maintained list of expected
/// strings, so a future edit cannot slip a bad message past it just by not
/// being on the list. It lives here now because four modules need exactly the
/// same rules: [`identity`], [`stats`], [`backend`] and [`session`].
///
/// The rules, and what each one is really guarding against:
///
/// - no `http`, and no `://`: a URL or a scheme word is our vocabulary, and a
///   verify link can carry the player's own sign-in code
/// - no run of three or more digits: a status code, or an operating system
///   error number
/// - no `\` and no `/`: a file path, which names a folder the player cannot
///   see and can do nothing about
/// - starts with a capital letter and ends with `.`, `?` or `!`: a sentence
///   rather than a fragment. Task 10b round 3 added this after two fragments
///   reached the test and were rewritten rather than the check being loosened
#[cfg(test)]
pub(crate) mod player_sentence {
    pub fn has_run_of_three_or_more_digits(s: &str) -> bool {
        let mut run = 0;
        for c in s.chars() {
            if c.is_ascii_digit() {
                run += 1;
                if run >= 3 {
                    return true;
                }
            } else {
                run = 0;
            }
        }
        false
    }

    pub fn assert_player_safe(msg: &str) {
        let lower = msg.to_ascii_lowercase();
        assert!(!lower.contains("http"), "message leaked transport jargon: {msg}");
        assert!(!msg.contains("://"), "message leaked a URL: {msg}");
        assert!(
            !has_run_of_three_or_more_digits(msg),
            "message leaked a status code or an error number: {msg}"
        );
        assert!(!msg.contains('\\'), "message leaked a file path: {msg}");
        assert!(!msg.contains('/'), "message leaked a file path: {msg}");
        let first = msg.chars().next().expect("a non-empty message");
        assert!(
            first.is_ascii_uppercase(),
            "message is a fragment, not a sentence (no capital letter to start): {msg}"
        );
        let last = msg.chars().last().expect("a non-empty message");
        assert!(
            matches!(last, '.' | '?' | '!'),
            "message is a fragment, not a sentence (no closing punctuation): {msg}"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The retired league uploader is gone, and so is its credential.
    ///
    /// `league.rs` was the launcher-side port of Dolphin's `NflsStatUpload`,
    /// posting to `ingest.php` with a bearer token. `backend.rs` replaced it,
    /// and by the end of this branch nothing called anything in it: it was
    /// reachable only from its own `pub mod` line. Dead code that still
    /// compiles is dead code somebody will one day call.
    ///
    /// The credential is the part that mattered. This branch removed the
    /// commands that could forget the old league token, so any installation
    /// that ever stored one would keep it in the Windows credential vault
    /// forever with no way to remove it from the product. That is a token this
    /// project has already had to rotate.
    #[test]
    fn the_retired_league_uploader_is_gone_and_its_credential_is_named_for_removal() {
        // The declarations only. This test's own text mentions the line it is
        // looking for, so reading the whole file would find itself.
        let lib = include_str!("lib.rs");
        let declarations = &lib[..lib.find("#[cfg(test)]").unwrap_or(lib.len())];
        assert!(
            !declarations.contains("pub mod league;"),
            "the retired league uploader is still declared"
        );
        let file = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/src/league.rs"));
        assert!(
            !file.exists(),
            "src/league.rs is still on disk, so it is still one line from being called again"
        );
        assert_eq!(
            identity::RETIRED_LEAGUE_TOKEN_ACCOUNT,
            "nsr.league.ingest",
            "the account the retired token was stored under has to keep its exact name, \
             or the cleanup deletes nothing"
        );
    }

    #[test]
    fn every_data_dir_has_a_distinct_relative_path() {
        let mut seen = std::collections::HashSet::new();
        for dir in DataDir::ALL {
            assert!(
                seen.insert(dir.relative()),
                "duplicate relative path: {}",
                dir.relative()
            );
        }
        assert_eq!(seen.len(), DataDir::ALL.len());
    }

    #[test]
    fn data_dir_paths_are_relative_and_use_forward_slashes() {
        for dir in DataDir::ALL {
            let rel = dir.relative();
            assert!(!rel.starts_with('/'), "{rel} must be relative");
            assert!(!rel.contains('\\'), "{rel} must not hardcode a separator");
            assert!(!rel.contains(':'), "{rel} must not contain a drive letter");
        }
    }

    /// User data and derived data must never overlap. If a directory were both,
    /// a cache purge could delete somebody's memory cards.
    #[test]
    fn user_data_is_never_also_derived() {
        for dir in DataDir::ALL {
            assert!(
                !(dir.is_user_data() && dir.is_derived()),
                "{:?} is classed as both user data and derived",
                dir
            );
        }
    }

    #[test]
    fn saves_are_user_data_and_never_purgeable() {
        assert!(DataDir::Saves.is_user_data());
        assert!(!DataDir::Saves.is_derived());
        assert!(DataDir::MusicSources.is_user_data());
        assert!(!DataDir::MusicSources.is_derived());
    }

    /// The bug this whole layout exists to prevent: the shipped 2.1 uninstaller
    /// deletes the install root, and the user's memory cards live inside it.
    #[test]
    fn install_root_never_contains_the_data_root() {
        let p = Platform::detect();
        let install = p.paths.install_root().expect("install root");
        let data = p.paths.data_root().expect("data root");
        assert!(
            !data.starts_with(&install),
            "data root {} is inside install root {}, which is how uninstalling deletes saves",
            data.display(),
            install.display()
        );
    }

    #[test]
    fn data_root_is_absolute_and_named_for_the_product() {
        let p = Platform::detect();
        let data = p.paths.data_root().expect("data root");
        assert!(data.is_absolute(), "{} must be absolute", data.display());
        assert!(
            data.to_string_lossy().contains("NFL Street Reborn"),
            "{} should be under a product-named directory",
            data.display()
        );
    }

    /// The 2.1 build owns `%APPDATA%\NFL Street Reborn` and fills it with a
    /// live Dolphin user tree, including a JSGME `MODS` folder full of the
    /// player's mods.
    ///
    /// The launcher must not share it. Windows is case insensitive, so our
    /// `mods/` would be their `MODS/`, and `mods/staging` and `mods/active`
    /// are classed derived, which means a cache purge could delete files out
    /// of the middle of somebody's real mod library.
    ///
    /// Found by running the CLI against a real machine, not by a test.
    #[test]
    fn the_data_root_is_not_the_legacy_install_folder() {
        let p = Platform::detect();
        let data = p.paths.data_root().expect("data root");
        let leaf = data
            .file_name()
            .expect("data root has a name")
            .to_string_lossy()
            .to_lowercase();
        assert_ne!(
            leaf, "nfl street reborn",
            "the data root is 2.1's own folder; see PRODUCT_DATA in platform::imp"
        );
    }

    /// A data root must never be a 2.1 install. Sharing one means the
    /// launcher's `mods/` is the player's JSGME `MODS/`, because Windows paths
    /// are case insensitive, and `mods/staging` and `mods/active` are classed
    /// derived, which means a cache purge can delete files out of the middle of
    /// a real mod library.
    ///
    /// Found by running the CLI on a real machine. The shipped data root was
    /// exactly that folder.
    #[test]
    fn the_real_data_root_is_not_a_legacy_install() {
        let p = Platform::detect();
        let data = p.paths.data_root().expect("data root");
        assert!(
            crate::migration::LegacyInstall::detect(&data).is_none(),
            "the data root {} is a 2.1 install; the launcher would write into \
             the player's live Dolphin tree",
            data.display()
        );
    }

    #[test]
    fn exe_suffix_matches_the_platform() {
        let p = Platform::detect();
        if cfg!(windows) {
            assert_eq!(p.paths.exe_suffix(), ".exe");
        } else {
            assert_eq!(p.paths.exe_suffix(), "");
        }
    }

    #[test]
    fn a_clean_exit_is_not_a_crash() {
        assert!(!ExitKind::Normal.is_crash());
        assert!(!ExitKind::Terminated.is_crash());
        assert!(!ExitKind::Error { code: 1 }.is_crash());
        assert!(ExitKind::Crashed {
            detail: "access violation".into()
        }
        .is_crash());
    }

    #[test]
    fn same_volume_agrees_with_itself() {
        let dir = tempfile::tempdir().expect("tempdir");
        let p = Platform::detect();
        let a = dir.path().join("a.txt");
        let b = dir.path().join("b.txt");
        std::fs::write(&a, b"a").unwrap();
        assert!(
            p.files.same_volume(&a, &b).expect("same_volume"),
            "two paths in one directory must be on one volume"
        );
    }

    #[test]
    fn atomic_replace_moves_the_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let p = Platform::detect();
        let src = dir.path().join("new");
        let dest = dir.path().join("live");
        std::fs::write(&src, b"new contents").unwrap();
        std::fs::write(&dest, b"old contents").unwrap();

        p.files.atomic_replace(&src, &dest).expect("replace");

        assert_eq!(std::fs::read(&dest).unwrap(), b"new contents");
        assert!(!src.exists(), "source should be gone after a rename");
    }

    #[test]
    fn hardlink_shares_content_or_reports_why_not() {
        let dir = tempfile::tempdir().expect("tempdir");
        let p = Platform::detect();
        let src = dir.path().join("pristine.dds");
        let dest = dir.path().join("composed").join("pristine.dds");
        std::fs::write(&src, b"texture").unwrap();

        // Composition falls back to copying on filesystems without hardlinks,
        // notably exFAT and FAT32, so a failure here is a supported outcome
        // rather than a test failure.
        match p.files.hardlink(&src, &dest) {
            Ok(()) => assert_eq!(std::fs::read(&dest).unwrap(), b"texture"),
            Err(e) => eprintln!("hardlink unsupported here, composition would copy: {e}"),
        }
    }

    #[test]
    fn spawning_and_waiting_classifies_a_clean_exit() {
        let p = Platform::detect();
        let (program, args): (&str, Vec<std::ffi::OsString>) = if cfg!(windows) {
            ("cmd", vec!["/C".into(), "exit 0".into()])
        } else {
            ("sh", vec!["-c".into(), "exit 0".into()])
        };
        let handle = p
            .processes
            .spawn(
                std::path::Path::new(program),
                &args,
                None,
                &Default::default(),
            )
            .expect("spawn");
        assert!(handle.pid() > 0);
        assert_eq!(p.processes.wait(handle).expect("wait"), ExitKind::Normal);
    }

    #[test]
    fn a_nonzero_exit_is_an_error_not_a_crash() {
        let p = Platform::detect();
        let (program, args): (&str, Vec<std::ffi::OsString>) = if cfg!(windows) {
            ("cmd", vec!["/C".into(), "exit 3".into()])
        } else {
            ("sh", vec!["-c".into(), "exit 3".into()])
        };
        let handle = p
            .processes
            .spawn(
                std::path::Path::new(program),
                &args,
                None,
                &Default::default(),
            )
            .expect("spawn");
        let kind = p.processes.wait(handle).expect("wait");
        assert_eq!(kind, ExitKind::Error { code: 3 });
        assert!(!kind.is_crash(), "exit 3 is a failure, not a crash");
    }

    /// The credential vault must really persist, and must never fall back
    /// to plaintext. A vault that silently does not store is a security bug
    /// that passes tests, so this stores, reads back, deletes, and checks the
    /// value is gone rather than asserting any of it.
    ///
    /// Skipped rather than failed when no platform store is available, since
    /// a headless CI box legitimately has none and that is not this crate's
    /// fault. The skip is printed, so it cannot pass quietly forever.
    #[test]
    fn the_credential_vault_persists_and_deletes() {
        let p = Platform::detect();
        let name = "nsr-launcher-selftest";
        let secret = "value-that-must-round-trip";

        match p.credentials.store(name, secret) {
            Ok(()) => {}
            Err(e) => {
                eprintln!("SKIPPED: no usable credential store here: {e}");
                return;
            }
        }

        let got = p
            .credentials
            .retrieve(name)
            .expect("a stored secret must be retrievable");
        assert_eq!(
            got.as_deref(),
            Some(secret),
            "the vault did not return what was stored"
        );

        p.credentials.delete(name).expect("delete must succeed");
        assert!(
            p.credentials
                .retrieve(name)
                .map(|v| v.is_none())
                .unwrap_or(true),
            "a deleted secret is still readable"
        );
    }

    /// Drive enumeration must not report a fixed disk as removable. The Xbox
    /// wizard copies to whatever this returns, and picking the system drive
    /// by mistake is the failure worth preventing.
    #[test]
    fn drive_enumeration_never_calls_a_fixed_disk_removable() {
        let p = Platform::detect();
        let drives = match p.drives.enumerate() {
            Ok(d) => d,
            Err(e) => {
                eprintln!("SKIPPED: drive enumeration unavailable here: {e}");
                return;
            }
        };

        // enumerate() returns ONLY removable volumes by construction, so the
        // system drive appearing here at all is the failure, not a flag on it.
        if let Ok(exe) = std::env::current_exe() {
            for d in &drives {
                assert!(
                    !exe.starts_with(&d.mount_point),
                    "{:?} holds the running executable and must never be offered as removable",
                    d.mount_point
                );
            }
        }
        eprintln!("enumerated {} drive(s)", drives.len());
    }
}

pub mod dolphin_preferences;
