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
//!
//! Not written yet: the supervisor process, SDL device enumeration, mods,
//! music, online, and the Xbox wizard.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

pub mod controller;
pub mod library;
pub mod manifest;
pub mod platform;
pub mod runtime;

pub use platform::{
    CredentialVault, DataDir, Error, ExitKind, FileOps, Paths, Platform, ProcessHost,
    RemovableDrive, RemovableDrives, Result, CURRENT_PLATFORM,
};

#[cfg(test)]
mod tests {
    use super::*;

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
            .spawn(std::path::Path::new(program), &args, None)
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
            .spawn(std::path::Path::new(program), &args, None)
            .expect("spawn");
        let kind = p.processes.wait(handle).expect("wait");
        assert_eq!(kind, ExitKind::Error { code: 3 });
        assert!(!kind.is_crash(), "exit 3 is a failure, not a crash");
    }

    /// The unimplemented seams must fail honestly, not panic and not pretend to
    /// succeed. A credential vault that silently drops secrets is a security
    /// bug that passes tests.
    #[test]
    fn stub_seams_return_not_implemented() {
        let p = Platform::detect();
        let err = p.credentials.store("discord", "secret").unwrap_err();
        assert!(matches!(err, Error::NotImplemented { .. }), "got {err:?}");
        assert!(p.credentials.retrieve("discord").is_err());

        let err = p.drives.enumerate().unwrap_err();
        assert!(matches!(err, Error::NotImplemented { .. }), "got {err:?}");
    }

    #[test]
    fn not_implemented_names_the_platform_and_the_operation() {
        let p = Platform::detect();
        let msg = p.drives.enumerate().unwrap_err().to_string();
        assert!(msg.contains(CURRENT_PLATFORM), "{msg}");
        assert!(msg.contains("removable drive enumeration"), "{msg}");
    }
}
