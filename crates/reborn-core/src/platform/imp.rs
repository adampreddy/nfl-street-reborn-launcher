//! Concrete implementations of the platform seams.
//!
//! What is real and what is a stub, stated plainly so nobody has to read the
//! code to find out:
//!
//! | Seam | Windows | macOS | Linux |
//! |---|---|---|---|
//! | [`Paths`] | real | real | real |
//! | [`ProcessHost`] | real | real | real |
//! | [`FileOps`] | real | real | real |
//! | [`CredentialVault`] | **stub** | **stub** | **stub** |
//! | [`RemovableDrives`] | **stub** | **stub** | **stub** |
//!
//! The two stubs return [`Error::NotImplemented`] rather than panicking or
//! pretending to succeed. Credentials are not needed until Discord sign-in, and
//! drive enumeration is not needed until the Xbox wizard. Writing a plausible
//! fake for either would be worse than an honest failure, because a credential
//! vault that silently does not persist is a security bug that tests pass.

use super::*;

// ---------------------------------------------------------------------------
// Paths, implemented for all three platforms
// ---------------------------------------------------------------------------

struct RealPaths;

impl RealPaths {
    fn env_path(key: &str) -> Option<PathBuf> {
        std::env::var_os(key)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    }

    fn home() -> Result<PathBuf> {
        #[cfg(windows)]
        let home = Self::env_path("USERPROFILE");
        #[cfg(not(windows))]
        let home = Self::env_path("HOME");
        home.ok_or(Error::NoHomeDirectory)
    }
}

/// The product name, used identically on every platform so a user moving
/// between them sees the same folder.
const PRODUCT: &str = "NFL Street Reborn";

/// The directory name for the launcher's own data.
///
/// Deliberately NOT `PRODUCT`. The shipped 2.1 build already owns
/// `%APPDATA%\NFL Street Reborn`, and it is a live Dolphin user tree: `User`,
/// `Games`, `Sys`, screenshots, and a JSGME `MODS` folder holding the player's
/// installed mods.
///
/// Sharing it looked reasonable and is not. Windows paths are case insensitive,
/// so the launcher's `mods/` IS that `MODS/`, and `mods/staging` and
/// `mods/active` are classed derived, which means purgeable. A cache purge
/// would delete files out of the middle of the player's real mod library.
///
/// Confirmed on a real install rather than reasoned about: listing `mods` in
/// that folder returns `!BACKUP`, `!INSTLOGS`, `Aqua Football`, and the rest of
/// the JSGME tree.
///
/// Keeping the names apart also makes migration honest. The 2.1 folder is the
/// source and this is the destination, and a copy whose source and destination
/// are the same directory is not a migration.
const PRODUCT_DATA: &str = "NFL Street Reborn Launcher";

impl Paths for RealPaths {
    fn install_root(&self) -> Result<PathBuf> {
        #[cfg(windows)]
        {
            // Deliberately NOT %APPDATA%. The current 2.1 installer puts program
            // files in the same folder as the user's memory cards, which is why
            // its uninstaller deletes saves. Keeping the roots on separate
            // branches makes that bug unwritable.
            let base = RealPaths::env_path("LOCALAPPDATA")
                .map(|p| p.join("Programs"))
                .ok_or(Error::NoHomeDirectory)?;
            Ok(base.join(format!("{PRODUCT} Launcher")))
        }
        #[cfg(target_os = "macos")]
        {
            Ok(PathBuf::from("/Applications").join(format!("{PRODUCT} Launcher.app")))
        }
        #[cfg(target_os = "linux")]
        {
            // An AppImage runs from wherever the user put it, so the honest
            // answer is the directory holding the running executable.
            let exe = std::env::current_exe()?;
            Ok(exe.parent().map(Path::to_path_buf).unwrap_or(exe))
        }
        #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
        {
            not_implemented("install_root")
        }
    }

    fn data_root(&self) -> Result<PathBuf> {
        #[cfg(windows)]
        {
            // Not %APPDATA%\NFL Street Reborn: that is 2.1's live user tree.
            // See PRODUCT_DATA.
            let base = RealPaths::env_path("APPDATA")
                .ok_or(Error::NoHomeDirectory)
                .or_else(|_| RealPaths::home().map(|h| h.join("AppData").join("Roaming")))?;
            Ok(base.join(PRODUCT_DATA))
        }
        #[cfg(target_os = "macos")]
        {
            Ok(RealPaths::home()?
                .join("Library")
                .join("Application Support")
                .join(PRODUCT_DATA))
        }
        #[cfg(target_os = "linux")]
        {
            let base = RealPaths::env_path("XDG_DATA_HOME")
                .map(Ok)
                .unwrap_or_else(|| RealPaths::home().map(|h| h.join(".local").join("share")))?;
            Ok(base.join(PRODUCT_DATA))
        }
        #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
        {
            not_implemented("data_root")
        }
    }

    fn cache_root(&self) -> Result<PathBuf> {
        #[cfg(windows)]
        {
            let base = RealPaths::env_path("LOCALAPPDATA").ok_or(Error::NoHomeDirectory)?;
            Ok(base.join(PRODUCT).join("cache"))
        }
        #[cfg(target_os = "macos")]
        {
            Ok(RealPaths::home()?
                .join("Library")
                .join("Caches")
                .join(PRODUCT))
        }
        #[cfg(target_os = "linux")]
        {
            let base = RealPaths::env_path("XDG_CACHE_HOME")
                .map(Ok)
                .unwrap_or_else(|| RealPaths::home().map(|h| h.join(".cache")))?;
            Ok(base.join(PRODUCT))
        }
        #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
        {
            not_implemented("cache_root")
        }
    }

    fn exe_suffix(&self) -> &'static str {
        if cfg!(windows) {
            ".exe"
        } else {
            ""
        }
    }
}

// ---------------------------------------------------------------------------
// Processes
// ---------------------------------------------------------------------------

struct RealProcessHost;

impl ProcessHost for RealProcessHost {
    fn spawn(
        &self,
        program: &Path,
        args: &[OsString],
        cwd: Option<&Path>,
        env: &BTreeMap<String, OsString>,
    ) -> Result<ProcessHandle> {
        let mut cmd = std::process::Command::new(program);
        cmd.args(args);
        if let Some(dir) = cwd {
            cmd.current_dir(dir);
        }
        for (key, value) in env {
            cmd.env(key, value);
        }
        let child = cmd.spawn().map_err(|source| Error::Io {
            path: program.to_path_buf(),
            source,
        })?;
        Ok(ProcessHandle {
            pid: child.id(),
            inner: Some(child),
        })
    }

    fn wait(&self, mut handle: ProcessHandle) -> Result<ExitKind> {
        let Some(mut child) = handle.inner.take() else {
            return Err(Error::PlainIo(std::io::Error::other(
                "process handle was already consumed",
            )));
        };
        let status = child.wait()?;
        Ok(classify_exit(&status))
    }

    fn is_running(&self, handle: &ProcessHandle) -> bool {
        // `Child::try_wait` needs `&mut`, and the handle may have been moved
        // into `wait`. A live handle with an `inner` is running until proven
        // otherwise; a consumed one is not.
        handle.inner.is_some()
    }
}

/// Turn an exit status into something the user can be told.
///
/// This is the part that genuinely differs. Unix reports a crash as a signal
/// and leaves `code()` as `None`. Windows has no signals and returns the
/// structured exception code as the exit status, so a crash arrives as a very
/// large unsigned number such as `0xC0000005` for an access violation.
fn classify_exit(status: &std::process::ExitStatus) -> ExitKind {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return match signal {
                // SIGINT and SIGTERM mean somebody asked it to stop.
                2 | 15 => ExitKind::Terminated,
                // SIGSEGV, SIGBUS, SIGILL, SIGFPE, SIGABRT are crashes.
                4 | 6 | 7 | 8 | 11 => ExitKind::Crashed {
                    detail: format!("signal {signal}"),
                },
                other => ExitKind::Crashed {
                    detail: format!("signal {other}"),
                },
            };
        }
    }

    match status.code() {
        Some(0) => ExitKind::Normal,
        Some(code) => {
            #[cfg(windows)]
            {
                // Windows structured exception codes occupy 0xC0000000 and up.
                // Read as i32 they are negative, which is how they arrive here.
                let unsigned = code as u32;
                if unsigned >= 0xC000_0000 {
                    return ExitKind::Crashed {
                        detail: match unsigned {
                            0xC000_0005 => "access violation (0xC0000005)".to_string(),
                            0xC000_0374 => "heap corruption (0xC0000374)".to_string(),
                            0xC000_00FD => "stack overflow (0xC00000FD)".to_string(),
                            0xC000_013A => return ExitKind::Terminated,
                            other => format!("exception 0x{other:08X}"),
                        },
                    };
                }
            }
            ExitKind::Error { code }
        }
        None => ExitKind::Terminated,
    }
}

// ---------------------------------------------------------------------------
// File operations
// ---------------------------------------------------------------------------

struct RealFileOps;

impl FileOps for RealFileOps {
    fn atomic_replace(&self, src: &Path, dest: &Path) -> Result<()> {
        if !self.same_volume(src, dest)? {
            return Err(Error::PlainIo(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!(
                    "refusing to rename across volumes: {} -> {}. \
                     A rename is only atomic within one filesystem; \
                     copy to the destination volume first",
                    src.display(),
                    dest.display()
                ),
            )));
        }
        std::fs::rename(src, dest).map_err(|source| Error::Io {
            path: dest.to_path_buf(),
            source,
        })
    }

    fn same_volume(&self, a: &Path, b: &Path) -> Result<bool> {
        #[cfg(windows)]
        {
            // Compares the path prefix, which is the drive letter or UNC share.
            //
            // LIMITATION, stated rather than hidden: a volume mounted onto an
            // empty NTFS folder shares the prefix of its parent, so this
            // returns true when a rename would actually fail. Detecting that
            // needs GetVolumeInformationByHandleW. Folder mount points are rare
            // for the paths this launcher uses, and the failure mode is a
            // rename error rather than a corrupted install, so the simple
            // comparison ships first and the Win32 version is a follow-up.
            fn prefix(p: &Path) -> Option<std::ffi::OsString> {
                use std::path::Component;
                p.components().next().and_then(|c| match c {
                    Component::Prefix(pre) => Some(pre.as_os_str().to_ascii_uppercase()),
                    _ => None,
                })
            }
            let (pa, pb) = (absolute(a)?, absolute(b)?);
            Ok(prefix(&pa) == prefix(&pb))
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            // On Unix the device id is exact, but it requires the path to
            // exist. Walk up to the nearest existing ancestor, which is what a
            // rename would ultimately land in anyway.
            fn device_of(path: &Path) -> Result<u64> {
                let mut current = absolute(path)?;
                loop {
                    match std::fs::metadata(&current) {
                        Ok(meta) => return Ok(meta.dev()),
                        Err(_) => match current.parent() {
                            Some(parent) if parent != current => current = parent.to_path_buf(),
                            _ => {
                                return Err(Error::Io {
                                    path: path.to_path_buf(),
                                    source: std::io::Error::new(
                                        std::io::ErrorKind::NotFound,
                                        "no existing ancestor to determine the volume from",
                                    ),
                                })
                            }
                        },
                    }
                }
            }
            Ok(device_of(a)? == device_of(b)?)
        }
        #[cfg(not(any(windows, unix)))]
        {
            let _ = (a, b);
            not_implemented("same_volume")
        }
    }

    fn hardlink(&self, src: &Path, dest: &Path) -> Result<()> {
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(|source| Error::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        std::fs::hard_link(src, dest).map_err(|source| Error::Io {
            path: dest.to_path_buf(),
            source,
        })
    }

    fn is_locked(&self, path: &Path) -> bool {
        #[cfg(windows)]
        {
            // On Windows, opening for write fails while another process holds
            // the file. That is exactly the condition that makes an update
            // unsafe, so the cheap check is also the correct one.
            match std::fs::OpenOptions::new().write(true).open(path) {
                Ok(_) => false,
                Err(e) => e.kind() == std::io::ErrorKind::PermissionDenied,
            }
        }
        #[cfg(not(windows))]
        {
            // Unix advisory locking tells us nothing useful here, and a running
            // executable can be replaced. Reporting false is the honest answer
            // rather than a guess dressed as a check.
            let _ = path;
            false
        }
    }

    fn lock_exclusive(&self, path: &Path) -> Result<Option<LockGuard>> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| Error::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .map_err(|source| Error::Io {
                path: path.to_path_buf(),
                source,
            })?;

        match try_lock(&file) {
            Ok(true) => Ok(Some(LockGuard {
                file,
                path: path.to_path_buf(),
            })),
            Ok(false) => Ok(None),
            Err(source) => Err(Error::Io {
                path: path.to_path_buf(),
                source,
            }),
        }
    }
}

fn absolute(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}

// ---------------------------------------------------------------------------
// Stubs, honest about being stubs
// ---------------------------------------------------------------------------

struct StubCredentialVault;

impl CredentialVault for StubCredentialVault {
    fn store(&self, _account: &str, _secret: &str) -> Result<()> {
        not_implemented("credential store")
    }
    fn retrieve(&self, _account: &str) -> Result<Option<String>> {
        not_implemented("credential retrieve")
    }
    fn delete(&self, _account: &str) -> Result<()> {
        not_implemented("credential delete")
    }
}

struct StubRemovableDrives;

impl RemovableDrives for StubRemovableDrives {
    fn enumerate(&self) -> Result<Vec<RemovableDrive>> {
        not_implemented("removable drive enumeration")
    }
    fn eject(&self, _mount_point: &Path) -> Result<()> {
        not_implemented("eject")
    }
}

/// Assemble the seam set for this platform.
pub fn platform() -> Platform {
    Platform {
        paths: Box::new(RealPaths),
        credentials: Box::new(StubCredentialVault),
        processes: Box::new(RealProcessHost),
        files: Box::new(RealFileOps),
        drives: Box::new(StubRemovableDrives),
    }
}

// ---------------------------------------------------------------------------
// Exclusive locking, implemented for all three platforms
// ---------------------------------------------------------------------------

/// `Ok(true)` if the lock was taken, `Ok(false)` if someone else holds it.
///
/// This goes through `fs4` rather than calling `LockFileEx` and `flock`
/// directly, because doing it by hand would mean `unsafe`, and this crate
/// forbids `unsafe` outright. A file lock is not worth spending that guarantee
/// on. The lock is held by the kernel either way, so it is still released on
/// process death by any means, which is the property the supervisor needs.
fn try_lock(file: &std::fs::File) -> std::io::Result<bool> {
    use fs4::fs_std::FileExt;
    match FileExt::try_lock_exclusive(file) {
        Ok(true) => Ok(true),
        Ok(false) => Ok(false),
        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(false),
        Err(e) => Err(e),
    }
}
