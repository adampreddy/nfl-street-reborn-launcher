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
//! Windows is implemented. macOS and Linux compile and return
//! [`Error::NotImplemented`] so the whole workspace builds on every platform
//! from day one. A stub that compiles is worth more than an absent module,
//! because it keeps the port honest: the seam list cannot drift without the
//! other platforms failing to build.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

pub mod imp;

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
}

pub type Result<T> = std::result::Result<T, Error>;

/// Helper for the stub implementations.
pub(crate) fn not_implemented<T>(operation: &'static str) -> Result<T> {
    Err(Error::NotImplemented {
        operation,
        platform: CURRENT_PLATFORM,
    })
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

    /// The executable suffix, empty on Unix.
    fn exe_suffix(&self) -> &'static str;
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
    Runtime,
    ModsStaging,
    ModsPackages,
    ModsActive,
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
            DataDir::Runtime => "runtime",
            DataDir::ModsStaging => "mods/staging",
            DataDir::ModsPackages => "mods/packages",
            DataDir::ModsActive => "mods/active",
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
        DataDir::Runtime,
        DataDir::ModsStaging,
        DataDir::ModsPackages,
        DataDir::ModsActive,
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
                | DataDir::ModsActive
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
    pub volume_identity: Option<String>,
}

/// Enumerating drives the Xbox wizard may write to.
///
/// The wizard never auto-selects when more than one removable drive exists, and
/// system volumes are never offered at all. Both rules are enforced above this
/// seam; the seam's only job is to report honestly what is present.
pub trait RemovableDrives: Send + Sync {
    fn enumerate(&self) -> Result<Vec<RemovableDrive>>;

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
