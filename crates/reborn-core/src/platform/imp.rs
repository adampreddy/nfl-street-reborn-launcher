//! Concrete implementations of the platform seams.
//!
//! What is real and what is not, stated plainly so nobody has to read the code
//! to find out:
//!
//! | Seam | Windows | macOS | Linux |
//! |---|---|---|---|
//! | [`Paths`] | real | real | real |
//! | [`ProcessHost`] | real | real | real |
//! | [`FileOps`] | real | real | real |
//! | [`CredentialVault`] | real, exercised | real, never run | real, never run |
//! | [`RemovableDrives::enumerate_volumes`] | real, exercised | real, never run | real, never run |
//! | [`RemovableDrives::eject`] | **refuses** | **refuses** | **refuses** |
//!
//! "never run" is not hedging. The credential and drive code is one
//! implementation for all three platforms, because `keyring` and `sysinfo` carry
//! the per-platform back ends, so there is no separate macOS or Linux branch to
//! be wrong. But nothing has executed it on either, and this table exists so the
//! difference between "written" and "verified" stays visible. What was actually
//! measured here, on Windows: a secret written by one process and read back by a
//! second one, and a single fixed NTFS volume that the drive seam refuses to
//! offer.
//!
//! # The two dependencies this file adds
//!
//! Both exist because this crate forbids `unsafe`, and neither job can be done
//! from the standard library.
//!
//! - `keyring` for the credential store. It is Windows Credential Manager, the
//!   macOS Keychain and the *nix Secret Service behind one API, and it never
//!   falls back to something weaker: with no store, every call fails. Hand
//!   rolling `CredWriteW` through `windows-sys` would mean `unsafe` in this
//!   crate, and a stored token is not worth spending that guarantee on.
//! - `sysinfo` for volume enumeration. It is the safe crate that reports a
//!   removable flag on all three platforms. Dolphin detection is separately
//!   implemented in runtime_process with query-only Windows access.
//!
//! # Why [`RemovableDrives::eject`] still refuses
//!
//! Telling Windows to release a volume is `DeviceIoControl` with
//! `IOCTL_STORAGE_EJECT_MEDIA`, which is `unsafe`, and no safe wrapper for it
//! exists. The alternative, invoking the shell's Eject verb through a child
//! `powershell`, was rejected for two concrete reasons: it cannot report whether
//! it worked, and when the volume is busy Windows raises its own modal dialog
//! that this launcher can neither see nor dismiss.
//!
//! So it returns [`Error::NotImplemented`] and the caller degrades: `xbox` turns
//! any error into `EjectOutcome::Refused`, whose message points at Windows' own
//! Eject. That matters because `EjectOutcome::Ejected` says "the drive has been
//! ejected and is safe to unplug", which must never be printed by code that did
//! nothing. Every file is closed by then, and since Windows 10 1809 removable
//! volumes default to quick removal with no write caching, so the copy is
//! already on the medium when the wizard finishes.

use super::*;

// ---------------------------------------------------------------------------
// Paths, implemented for all three platforms
// ---------------------------------------------------------------------------

struct RealPaths {
    channel: Channel,
}

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

/// The data root is `PRODUCT` plus the channel suffix, in the platform's
/// per-user data folder. On Windows that is `%APPDATA%\NFL Street Reborn`,
/// which the 2.1 build also used: a 2.1 install found there is moved into a
/// `Reborn 2.1` subfolder by `crate::layout` before the launcher writes
/// anything, so the launcher's `mods` can never be JSGME's `MODS` (Windows
/// paths are case insensitive, and `mods/staging` is purgeable).
///
/// Builds before 2026-09-12 used `NFL Street Reborn Launcher[ Beta]`; see
/// `previous_data_root`, which names that folder so `crate::layout` can adopt
/// what is in it.
const PREVIOUS_DATA_SUFFIX: &str = " Launcher";

impl RealPaths {
    /// The directory of `exe` when it is an install: the launcher itself
    /// sits there. `None` for anything else (a test binary under target,
    /// a copied tool), so the fixed per-channel path answers instead.
    /// Windows only in use: Linux answers with the executable's own folder.
    #[cfg_attr(not(windows), allow(dead_code))]
    pub(crate) fn install_dir_holding(exe: &Path) -> Option<PathBuf> {
        let dir = exe.parent()?.to_path_buf();
        dir.join("reborn-launcher.exe").is_file().then_some(dir)
    }
}

impl RealPaths {
    /// The Linux install root: the folder holding the AppImage when there is
    /// one, otherwise the folder holding the executable.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    fn install_root_linux(appimage: Option<&Path>, exe: &Path) -> PathBuf {
        appimage
            .and_then(Path::parent)
            .map(Path::to_path_buf)
            .unwrap_or_else(|| exe.parent().map(Path::to_path_buf).unwrap_or_else(|| exe.to_path_buf()))
    }
}

impl Paths for RealPaths {
    fn install_root(&self) -> Result<PathBuf> {
        #[cfg(windows)]
        {
            // The folder holding the running launcher. Since beta.8 the
            // installer puts the program in `app` INSIDE the data root
            // ({userappdata}\NFL Street Reborn Launcher[ Beta]\app); before
            // that it was {localappdata}\Programs\NFL Street Reborn[ Beta].
            // Answering with the fixed Programs path after the move made
            // the first run seed nothing from the shipped content and plan a
            // 6.4 GB download of what sat beside the exe (measured on the
            // beta.8 walk, 2026-09-11). The exe's own directory is right for
            // both layouts and for a payload folder run in place. A test
            // binary or a stray rebornctl.exe copied elsewhere has no
            // launcher beside it and gets the per-channel Programs path.
            if let Some(dir) = std::env::current_exe().ok().and_then(|exe| RealPaths::install_dir_holding(&exe)) {
                return Ok(dir);
            }
            let base = RealPaths::env_path("LOCALAPPDATA")
                .map(|p| p.join("Programs"))
                .ok_or(Error::NoHomeDirectory)?;
            Ok(base.join(format!("{PRODUCT}{}", self.channel.suffix())))
        }
        #[cfg(target_os = "macos")]
        {
            // The running bundle's Contents/Resources, wherever the player put
            // the app: that is where the .app carries the emulator, the tools
            // and anything to seed from, as an AppImage carries them in
            // $APPDIR. A plain binary (cargo run, rebornctl) answers with its
            // own folder. Read only once installed, like the AppImage mount.
            let exe = std::env::current_exe()?;
            Ok(crate::runtime::emulator_home(&exe).unwrap_or(exe))
        }
        #[cfg(target_os = "linux")]
        {
            // An AppImage runs from wherever the player put it. The running
            // executable sits in the read-only mount the AppImage runtime
            // unpacks into, so the file the player started (`APPIMAGE`) is
            // the honest answer; a plain binary answers with its own folder.
            let appimage = std::env::var_os("APPIMAGE").filter(|v| !v.is_empty()).map(PathBuf::from);
            Ok(RealPaths::install_root_linux(appimage.as_deref(), &std::env::current_exe()?))
        }
        #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
        {
            not_implemented("install_root")
        }
    }

    fn data_root(&self) -> Result<PathBuf> {
        #[cfg(windows)]
        {
            // %APPDATA%\NFL Street Reborn. A 2.1 install found there is
            // tucked into `Reborn 2.1` at start; see crate::layout.
            let base = RealPaths::env_path("APPDATA")
                .ok_or(Error::NoHomeDirectory)
                .or_else(|_| RealPaths::home().map(|h| h.join("AppData").join("Roaming")))?;
            Ok(base.join(format!("{PRODUCT}{}", self.channel.suffix())))
        }
        #[cfg(target_os = "macos")]
        {
            Ok(RealPaths::home()?
                .join("Library")
                .join("Application Support")
                .join(format!("{PRODUCT}{}", self.channel.suffix())))
        }
        #[cfg(target_os = "linux")]
        {
            let base = RealPaths::env_path("XDG_DATA_HOME")
                .map(Ok)
                .unwrap_or_else(|| RealPaths::home().map(|h| h.join(".local").join("share")))?;
            Ok(base.join(format!("{PRODUCT}{}", self.channel.suffix())))
        }
        #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
        {
            not_implemented("data_root")
        }
    }

    fn previous_data_root(&self) -> Result<Option<PathBuf>> {
        let data = self.data_root()?;
        let Some(parent) = data.parent() else { return Ok(None) };
        Ok(Some(parent.join(format!("{PRODUCT}{PREVIOUS_DATA_SUFFIX}{}", self.channel.suffix()))))
    }

    fn cache_root(&self) -> Result<PathBuf> {
        #[cfg(windows)]
        {
            let base = RealPaths::env_path("LOCALAPPDATA").ok_or(Error::NoHomeDirectory)?;
            Ok(base
                .join(format!("{PRODUCT}{}", self.channel.suffix()))
                .join("cache"))
        }
        #[cfg(target_os = "macos")]
        {
            Ok(RealPaths::home()?
                .join("Library")
                .join("Caches")
                .join(format!("{PRODUCT}{}", self.channel.suffix())))
        }
        #[cfg(target_os = "linux")]
        {
            let base = RealPaths::env_path("XDG_CACHE_HOME")
                .map(Ok)
                .unwrap_or_else(|| RealPaths::home().map(|h| h.join(".cache")))?;
            Ok(base.join(format!("{PRODUCT}{}", self.channel.suffix())))
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

    fn channel(&self) -> Channel {
        self.channel
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
        // Before the wait, because the id is what the crash record is matched on.
        let pid = child.id();
        let status = child.wait()?;
        Ok(with_fault_site(classify_exit(&status), pid))
    }

    fn is_running(&self, handle: &ProcessHandle) -> bool {
        // `Child::try_wait` needs `&mut`, and the handle may have been moved
        // into `wait`. A live handle with an `inner` is running until proven
        // otherwise; a consumed one is not.
        handle.inner.is_some()
    }

    fn run_capturing(
        &self,
        program: &Path,
        args: &[OsString],
        env: &BTreeMap<String, OsString>,
    ) -> Result<CapturedRun> {
        let mut cmd = std::process::Command::new(program);
        cmd.args(args);
        for (key, value) in env {
            cmd.env(key, value);
        }
        if let Some(dir) = program.parent() {
            cmd.current_dir(dir);
        }
        // Everything run this way is a console tool (xdelta3, DolphinTool,
        // the Builder) whose output is captured here; without this flag each
        // one flashed a console window on the player's screen.
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000);
        }
        let output = cmd.output().map_err(|source| Error::Io {
            path: program.to_path_buf(),
            source,
        })?;
        Ok(CapturedRun {
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            exit: classify_exit(&output.status),
        })
    }
}

// ---------------------------------------------------------------------------
// Opening a link in the player's browser
// ---------------------------------------------------------------------------

/// Hand `url` to the operating system so it opens in the player's default
/// browser.
///
/// # Why this is not `ProcessHost::spawn`
///
/// It was. On Windows it ran `explorer.exe <url>`, and Explorer does not use
/// the argument it is given: it re-parses its own command line and reads the
/// `?` that starts a query string as a path wildcard. Handed
/// `https://host/link?code=ABCD-EFGH` it therefore decides it has no url at
/// all and falls back to opening the default folder, which is Documents. That
/// is not a theory: `explorer.exe https://example.com/` opens the browser and
/// `explorer.exe https://example.com/?probe=1` opens a Documents window, on
/// the same machine, seconds apart. Every sign-in link carries a query string,
/// so the browser step could never have worked.
///
/// `ShellExecuteW` takes the url as one string and never builds or parses a
/// command line, so there is nothing left to misread. It is also the shell's
/// own entry point rather than a program that happens to host it, and it is
/// not a console application, so no window flashes up. A command interpreter
/// (`cmd /c start`) would have done the job too and is what most code reaches
/// for, but it flashes a console, so it is not an option here.
///
/// # What it never does
///
/// The url is not logged, not returned, and not put in an error, here or in
/// the caller. A sign-in link carries the player's own code.
#[cfg(windows)]
pub fn open_url(_platform: &Platform, url: &str) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    // Wide and nul terminated, which is what the api reads. `url` is checked
    // to be http or https before it gets here, so it holds no interior nul.
    let mut file: Vec<u16> = std::ffi::OsStr::new(url).encode_wide().collect();
    if file.contains(&0) {
        return Err(Error::PlainIo(std::io::Error::other(
            "the link could not be handed to the shell",
        )));
    }
    file.push(0);
    // "open", wide and nul terminated. Passing a null verb would pick the
    // default one, which is the same thing for a url, but naming it means a
    // machine with an odd association cannot pick something else.
    let verb: [u16; 5] = [b'o' as u16, b'p' as u16, b'e' as u16, b'n' as u16, 0];

    // SAFETY: every pointer is either null, which this api documents as "no
    // window" / "no argument" / "no working directory", or the start of a
    // local nul terminated buffer that outlives the call. Nothing is written
    // through any of them, and the call returns before either buffer is
    // dropped. The return value is a status disguised as a handle and is only
    // ever compared, never dereferenced.
    #[allow(unsafe_code)]
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };

    // Success is documented as a value greater than 32; anything at or below
    // that is an error code. The code is deliberately not reported: the caller
    // turns any failure here into one plain sentence, and a number on a
    // player's screen is the thing this project does not do.
    if result as isize > 32 {
        Ok(())
    } else {
        Err(Error::PlainIo(std::io::Error::other(
            "the shell would not open the link",
        )))
    }
}

/// Hand `url` to the operating system so it opens in the player's default
/// browser. See the Windows version for why that one does not go through
/// [`ProcessHost::spawn`]; these two do, unchanged, because `open` and
/// `xdg-open` take their argument as an argument.
#[cfg(not(windows))]
pub fn open_url(platform: &Platform, url: &str) -> Result<()> {
    let program: &str = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let args = vec![OsString::from(url)];
    platform
        .processes
        .spawn(
            Path::new(program),
            &args,
            None,
            &std::collections::BTreeMap::new(),
        )
        .map(|_| ())
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
// Where a crash happened
// ---------------------------------------------------------------------------

/// The module that faulted and the offset inside it, as Windows recorded them.
///
/// An exit code says a crash happened. It does not say where, and "access
/// violation" on its own has been worth nothing every time a player reported
/// one: the same code covers a null dereference in the emulator, a driver
/// fault and a half-written save. The offset is the part that names a site,
/// because it resolves back to a function in the build that produced it.
///
/// Windows only, with the two readers below: the record comes from the
/// Windows event log, and nothing else writes one in this shape.
#[cfg(windows)]
#[derive(Debug, Clone, PartialEq, Eq)]
struct FaultSite {
    module: String,
    offset: String,
}

/// One named field out of an `Application Error` payload.
///
/// `wevtutil` emits `<Data Name='ModuleName'>`. Both quote styles are accepted
/// because the attribute quoting is the emitter's choice, not a guarantee, and
/// getting it wrong here fails silently -- which is the one outcome this whole
/// path exists to stop.
#[cfg(windows)]
fn event_field<'a>(event: &'a str, name: &str) -> Option<&'a str> {
    for open in [format!("<Data Name='{name}'>"), format!("<Data Name=\"{name}\">")] {
        let Some(at) = event.find(&open) else { continue };
        let rest = &event[at + open.len()..];
        let Some(end) = rest.find("</Data>") else { continue };
        return Some(rest[..end].trim());
    }
    None
}

/// Pull the fault site for one process id out of `wevtutil` XML.
///
/// Windows writes an `Application Error` record for every process it ends this
/// way. Read by field name rather than by position: the payload order is
/// Windows' business, and a record that gains a field must not silently start
/// reporting the wrong one. Matching on the process id rather than the image
/// name is what keeps a second copy of the emulator, or a record left by an
/// earlier session, out of the answer.
///
/// Scanned as text rather than parsed as XML: the shape is fixed, this runs
/// once after a crash, and the input is output this code asked for.
#[cfg(windows)]
fn fault_site_in(xml: &str, pid: u32) -> Option<FaultSite> {
    for event in xml.split("<Event ").skip(1) {
        let Some(recorded) = event_field(event, "ProcessId") else { continue };
        let Ok(recorded) = u32::from_str_radix(recorded.trim_start_matches("0x"), 16) else {
            continue;
        };
        if recorded != pid {
            continue;
        }
        let Some(module) = event_field(event, "ModuleName") else { continue };
        let Some(offset) = event_field(event, "FaultingOffset") else { continue };
        return Some(FaultSite { module: module.to_string(), offset: offset.to_string() });
    }
    None
}

/// Ask Windows where the crash was.
///
/// The record is written a moment after the process dies, so this polls rather
/// than reading once and reporting nothing. Bounded, because the player is
/// waiting on the launcher to come back: a second and a half at worst. Every
/// failure is silent on purpose -- a missing record must never turn a crash
/// into an error, it just leaves the detail as short as it was before.
#[cfg(windows)]
fn fault_site(pid: u32) -> Option<FaultSite> {
    use std::os::windows::process::CommandExt;
    for attempt in 0..6u32 {
        if attempt > 0 {
            std::thread::sleep(std::time::Duration::from_millis(250));
        }
        let out = std::process::Command::new("wevtutil")
            .args([
                "qe",
                "Application",
                "/q:*[System[Provider[@Name='Application Error']]]",
                "/c:20",
                "/rd:true",
                "/f:xml",
            ])
            .creation_flags(0x0800_0000)
            .output()
            .ok()?;
        if let Some(site) = fault_site_in(&String::from_utf8_lossy(&out.stdout), pid) {
            return Some(site);
        }
    }
    None
}

/// Attach the fault site to a crash, when Windows recorded one.
fn with_fault_site(kind: ExitKind, pid: u32) -> ExitKind {
    let _ = pid;
    #[cfg(windows)]
    if let ExitKind::Crashed { detail } = &kind {
        if let Some(site) = fault_site(pid) {
            // WARN, because a shipped build logs nothing quieter, and this line
            // is the whole reason the lookup happens: it turns "it crashed" into
            // an address somebody can resolve to a function.
            tracing::warn!(
                "the game crashed in {} at +{} ({detail})",
                site.module,
                site.offset
            );
            return ExitKind::Crashed {
                detail: format!("{detail} in {} +{}", site.module, site.offset),
            };
        }
    }
    kind
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
// Credentials, in the operating system's own store or nowhere
// ---------------------------------------------------------------------------

/// The service name every launcher credential is filed under.
///
/// Names the launcher and not the game. On
/// Windows the credential shows up in Credential Manager as
/// `{account}.NFL Street Reborn Launcher`, a name the user can find and delete
/// without us. Confirmed with `cmdkey /list` after a write.
// The vault entry name every stored credential was written under. It is a
// contract with what is already in Windows Credential Manager on every PC,
// so it keeps the old data-root name on purpose.
const CREDENTIAL_SERVICE: &str = "NFL Street Reborn Launcher";

/// What the platform's store is called.
///
/// An error that says "the credential store" names nothing the user can go and
/// look at. This one does.
const CREDENTIAL_STORE_NAME: &str = if cfg!(windows) {
    "Windows Credential Manager"
} else if cfg!(target_os = "macos") {
    "the macOS Keychain"
} else if cfg!(target_os = "linux") {
    "the Secret Service (gnome-keyring or kwallet)"
} else {
    "this platform's credential store"
};

/// Secrets in the operating system's store, and nowhere else.
///
/// There is no fallback, on purpose, except one: Linux with no Secret Service
/// (a Steam Deck in Game Mode), see `linux_file_vault`. Elsewhere, if the platform store cannot be reached,
/// every method fails and says so. The tempting alternative, a file under the
/// data root, would turn a Discord token into something any process running as
/// the user can read, and it would pass every test in this file.
struct OsCredentialVault;

impl OsCredentialVault {
    fn entry(account: &str) -> Result<keyring::Entry> {
        // An empty account name is accepted by the platform store: it files the
        // secret under the bare service name, which in Credential Manager looks
        // like `.NFL Street Reborn Launcher`, owned by no feature and
        // attributable to nothing. Measured, not assumed.
        if account.is_empty() {
            return Err(Error::CredentialStore(format!(
                "a credential needs an account name to be filed under in \
                 {CREDENTIAL_STORE_NAME}; pass the name of the feature that owns \
                 the secret, for example \"discord\""
            )));
        }
        keyring::Entry::new(CREDENTIAL_SERVICE, account)
            .map_err(|e| credential_error("opening the entry for", account, e))
    }
}

/// Where secrets go on Linux when no Secret Service answers, or None when one
/// does (then the platform store is used, as everywhere else).
///
/// The one exception to "the operating system's store, and nowhere else". A
/// Steam Deck in Game Mode runs no desktop session, so neither KWallet nor
/// gnome-keyring is there to answer, and every sign-in and stats upload failed
/// ("results could not be sent", Deck build, 2026-10). Steam keeps its own
/// login in a file on the same machine. So here, and only here: one file per
/// account under `<data root>/credentials/`, the folder 0700 and the file 0600
/// from the moment they are created, written whole and read back.
#[cfg(target_os = "linux")]
fn linux_file_vault() -> Option<PathBuf> {
    if keyring::Entry::store_status().is_ok() {
        return None;
    }
    RealPaths { channel: Channel::detect().channel }.data_root().ok().map(|d| d.join("credentials"))
}

#[cfg(target_os = "linux")]
fn file_vault_path(dir: &Path, account: &str) -> Result<PathBuf> {
    if account.is_empty() || !account.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')) {
        return Err(Error::CredentialStore(format!("{account:?} cannot name a credential file")));
    }
    Ok(dir.join(account))
}

#[cfg(target_os = "linux")]
fn file_vault_store(dir: &Path, account: &str, secret: &str) -> Result<()> {
    use std::io::Write;
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
    let io = |path: &Path, source: std::io::Error| Error::Io { path: path.to_path_buf(), source };
    let path = file_vault_path(dir, account)?;
    std::fs::DirBuilder::new().recursive(true).mode(0o700).create(dir).map_err(|e| io(dir, e))?;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700)).map_err(|e| io(dir, e))?;
    let tmp = dir.join(format!(".{account}.tmp"));
    let _ = std::fs::remove_file(&tmp);
    {
        let mut f = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&tmp).map_err(|e| io(&tmp, e))?;
        f.write_all(secret.as_bytes()).and_then(|_| f.sync_all()).map_err(|e| io(&tmp, e))?;
    }
    std::fs::rename(&tmp, &path).map_err(|e| io(&path, e))?;
    match std::fs::read_to_string(&path) {
        Ok(back) if back == secret => Ok(()),
        Ok(_) => Err(Error::CredentialStore(format!("the credential file for {account:?} read back different"))),
        Err(e) => Err(io(&path, e)),
    }
}

#[cfg(target_os = "linux")]
fn file_vault_retrieve(dir: &Path, account: &str) -> Result<Option<String>> {
    let path = file_vault_path(dir, account)?;
    match std::fs::read_to_string(&path) {
        Ok(s) => Ok(Some(s)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(Error::Io { path, source }),
    }
}

#[cfg(target_os = "linux")]
fn file_vault_delete(dir: &Path, account: &str) -> Result<()> {
    let path = file_vault_path(dir, account)?;
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(Error::Io { path, source }),
    }
}

impl CredentialVault for OsCredentialVault {
    /// Store a secret, then read it straight back before reporting success.
    ///
    /// The read-back is the point. A write the store accepts and does not keep
    /// is the exact failure this seam exists to prevent, and without the check
    /// the launcher reports a successful sign-in and then finds nothing on the
    /// next start, which looks like the sign-in being broken.
    ///
    /// The trade, stated: on a store that acknowledges a write but only answers
    /// reads later, this reports failure for a secret that is in fact stored,
    /// and the user signs in twice. The other way round is a user who is told
    /// they are signed in and is not.
    fn store(&self, account: &str, secret: &str) -> Result<()> {
        #[cfg(target_os = "linux")]
        if let Some(dir) = linux_file_vault() {
            return file_vault_store(&dir, account, secret);
        }
        let entry = Self::entry(account)?;
        entry
            .set_password(secret)
            .map_err(|e| credential_error("storing the secret for", account, e))?;
        match entry.get_password() {
            Ok(read_back) if read_back == secret => Ok(()),
            Ok(_) => Err(Error::CredentialStore(format!(
                "{CREDENTIAL_STORE_NAME} accepted the secret for {account:?} and \
                 then returned a different one, so it cannot be trusted to hold \
                 it. Nothing was written anywhere else. Delete the {account:?} \
                 entry there and try again"
            ))),
            Err(e) => Err(credential_error(
                "reading back the secret just stored for",
                account,
                e,
            )),
        }
    }

    /// Read a secret, or report honestly that there is not one.
    ///
    /// `Ok(None)` and an error are different answers and the caller has to tell
    /// them apart: a player who has never signed in has no token, and showing
    /// them a store failure sends them hunting for a problem that does not
    /// exist.
    fn retrieve(&self, account: &str) -> Result<Option<String>> {
        #[cfg(target_os = "linux")]
        if let Some(dir) = linux_file_vault() {
            return file_vault_retrieve(&dir, account);
        }
        let entry = Self::entry(account)?;
        match entry.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(credential_error("reading the secret for", account, e)),
        }
    }

    /// Delete a secret. Already absent counts as done.
    ///
    /// Signing out must not fail because there was nothing to sign out of, and a
    /// caller forced to special-case "it was already gone" will eventually leave
    /// a token behind instead.
    fn delete(&self, account: &str) -> Result<()> {
        #[cfg(target_os = "linux")]
        if let Some(dir) = linux_file_vault() {
            return file_vault_delete(&dir, account);
        }
        let entry = Self::entry(account)?;
        match entry.delete_credential() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(credential_error("deleting the secret for", account, e)),
        }
    }
}

/// Turn a credential store failure into a message that names a next step.
///
/// `action` is a phrase such as `"storing the secret for"`, so the sentence
/// begins with which operation failed instead of leaving the caller to guess
/// from a bare platform error.
fn credential_error(action: &str, account: &str, error: keyring::Error) -> Error {
    let advice = match &error {
        // No store at all. This is the branch that must never quietly become a
        // file on disk, so it says plainly that nothing was written.
        keyring::Error::NoDefaultStore => {
            let why = match keyring::Entry::store_status() {
                Err(e) => e.to_string(),
                Ok(()) => "no reason was reported".to_string(),
            };
            format!(
                "{CREDENTIAL_STORE_NAME} could not be opened ({why}). No secret \
                 was written anywhere else. Unlock it and try again."
            )
        }
        keyring::Error::NoStorageAccess(_) => format!(
            "{CREDENTIAL_STORE_NAME} refused access, which usually means it is \
             locked. Unlock it and try again."
        ),
        // Measured on Windows with keyring 4.2: the cap is 2560 bytes, that is
        // 1280 UTF-16 units, and the store refuses rather than truncating. Worth
        // saying, because a truncated token looks valid and fails at the server
        // as an authentication error, which sends everyone hunting in the wrong
        // place.
        keyring::Error::TooLong(what, limit) => format!(
            "{CREDENTIAL_STORE_NAME} caps a secret at {limit} bytes ({what}) and \
             this one is longer. Nothing was truncated and nothing was stored. \
             Store a reference to the value rather than the value."
        ),
        keyring::Error::Ambiguous(_) => format!(
            "more than one credential in {CREDENTIAL_STORE_NAME} matches \
             {account:?}, so there is no single answer. Delete the duplicates \
             there and sign in again."
        ),
        keyring::Error::BadEncoding(_) => format!(
            "the stored value for {account:?} is not text, so this launcher did \
             not write it. Delete that entry in {CREDENTIAL_STORE_NAME} and sign \
             in again."
        ),
        _ => format!(
            "Nothing was written anywhere else. If this repeats, delete the \
             {account:?} entry in {CREDENTIAL_STORE_NAME} and sign in again."
        ),
    };
    Error::CredentialStore(format!("{action} {account:?} failed: {error}. {advice}"))
}

// ---------------------------------------------------------------------------
// Removable drives
// ---------------------------------------------------------------------------

struct RealRemovableDrives;

impl RemovableDrives for RealRemovableDrives {
    /// Every volume, with the removable flag the operating system reports.
    ///
    /// Only the storage numbers are refreshed. `DiskRefreshKind::everything`
    /// also opens each device to ask whether it is an SSD and to read its i/o
    /// counters, neither of which the wizard uses, and both of which mean waking
    /// an idle external disk just to draw a list.
    ///
    /// A volume reporting zero capacity is still returned. That is a card reader
    /// with no card in it, and the wizard has a refusal that tells the user
    /// their drive reported no size. Dropping it here would turn that into a
    /// drive silently missing from the picker.
    fn enumerate_volumes(&self) -> Result<Vec<Volume>> {
        let disks = sysinfo::Disks::new_with_refreshed_list_specifics(
            sysinfo::DiskRefreshKind::nothing().with_storage(),
        );

        let mut volumes: Vec<Volume> = disks
            .list()
            .iter()
            .map(|disk| Volume {
                kind: if disk.is_removable() {
                    VolumeKind::Removable
                } else {
                    VolumeKind::Fixed
                },
                drive: RemovableDrive {
                    mount_point: disk.mount_point().to_path_buf(),
                    label: text_or_absent(disk.name()),
                    total_bytes: disk.total_space(),
                    free_bytes: disk.available_space(),
                    filesystem: text_or_absent(disk.file_system()),
                    // Not knowable through this seam yet, so it says absent.
                    //
                    // A drive letter is not an identity and neither is a label:
                    // two sticks can both be called XBOX-USB, and treating them
                    // as one drive would make the wizard believe it had already
                    // written to a drive it has never seen. The real answer is
                    // the volume GUID path, which the enumerator computes
                    // internally and does not expose, and which otherwise needs
                    // GetVolumeNameForVolumeMountPointW and therefore unsafe.
                    volume_identity: None,
                },
            })
            .collect();

        // Sorted so a second scan lists the same drives in the same order. The
        // wizard requires an explicit path to be chosen, so order is not what
        // makes it safe, but a list that reshuffles under a user picking a
        // destination for 3 GB is its own hazard.
        volumes.sort_by(|a, b| a.drive.mount_point.cmp(&b.drive.mount_point));
        Ok(volumes)
    }

    /// Refuses. The module header has the reasoning and what the caller does
    /// instead.
    fn eject(&self, mount_point: &Path) -> Result<()> {
        let _ = mount_point;
        not_implemented("ejecting a drive from inside the launcher")
    }
}

/// A volume label or filesystem name, with empty meaning absent.
///
/// Windows returns an empty string for an unlabeled volume, and `Some("")` would
/// draw a drive in the picker with a blank where a name should be, which reads
/// as a label that failed to load rather than a drive that has none.
fn text_or_absent(value: &std::ffi::OsStr) -> Option<String> {
    let text = value.to_string_lossy().trim().to_string();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

/// Assemble the seam set for this platform.
pub fn platform() -> Platform {
    Platform {
        paths: Box::new(RealPaths {
            channel: Channel::detect().channel,
        }),
        credentials: Box::new(OsCredentialVault),
        processes: Box::new(RealProcessHost),
        files: Box::new(RealFileOps),
        drives: Box::new(RealRemovableDrives),
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Removable drives
    // -----------------------------------------------------------------------

    fn volume(mount_point: &str, kind: VolumeKind) -> Volume {
        Volume {
            kind,
            drive: RemovableDrive {
                mount_point: PathBuf::from(mount_point),
                label: None,
                total_bytes: 64_000_000_000,
                free_bytes: 63_000_000_000,
                filesystem: Some("exFAT".to_string()),
                volume_identity: None,
            },
        }
    }

    /// A machine with a system disk and a USB stick, so the filter can be tested
    /// without waiting for someone to plug something in.
    struct SystemDiskAndStick;

    impl RemovableDrives for SystemDiskAndStick {
        fn enumerate_volumes(&self) -> Result<Vec<Volume>> {
            Ok(vec![
                volume("/system", VolumeKind::Fixed),
                volume("/media/stick", VolumeKind::Removable),
            ])
        }
        fn eject(&self, _mount_point: &Path) -> Result<()> {
            Ok(())
        }
    }

    /// The failure this seam exists to prevent. The Xbox wizard copies about
    /// 3 GB to whatever `enumerate` hands it, so a fixed disk appearing in that
    /// list once is that payload written into somebody's system drive.
    #[test]
    fn a_fixed_volume_is_never_offered_as_a_removable_drive() {
        let offered = SystemDiskAndStick.enumerate().expect("enumerate");
        assert_eq!(
            offered.iter().map(|d| &d.mount_point).collect::<Vec<_>>(),
            vec![&PathBuf::from("/media/stick")],
            "only the removable volume may be offered"
        );
    }

    /// A removable volume the launcher is running from is not a destination.
    /// On a Mac that is the opened .dmg (a player who never dragged the app
    /// out); on Windows, a launcher run from the very stick it would format.
    #[test]
    fn the_volume_holding_the_running_launcher_is_never_offered() {
        struct RunningFromStick(PathBuf);
        impl RemovableDrives for RunningFromStick {
            fn enumerate_volumes(&self) -> Result<Vec<Volume>> {
                Ok(vec![
                    volume(self.0.to_str().unwrap(), VolumeKind::Removable),
                    volume("/media/other-stick", VolumeKind::Removable),
                ])
            }
            fn eject(&self, _mount_point: &Path) -> Result<()> {
                Ok(())
            }
        }
        let exe_dir = std::env::current_exe().unwrap().parent().unwrap().to_path_buf();
        let offered = RunningFromStick(exe_dir).enumerate().expect("enumerate");
        assert_eq!(
            offered.iter().map(|d| &d.mount_point).collect::<Vec<_>>(),
            vec![&PathBuf::from("/media/other-stick")],
        );
    }

    /// Same rule, against whatever is actually attached to this machine rather
    /// than a fake. On the machine this was written on that is one fixed NTFS
    /// volume and no removable drives, so `enumerate` is empty and that is the
    /// correct answer.
    #[test]
    fn the_real_enumerator_offers_no_fixed_volume() {
        let drives = RealRemovableDrives;
        let volumes = drives.enumerate_volumes().expect("enumerate_volumes");
        assert!(
            !volumes.is_empty(),
            "a running machine has at least the volume it booted from"
        );
        let offered = drives.enumerate().expect("enumerate");

        for volume in &volumes {
            if volume.kind == VolumeKind::Fixed {
                assert!(
                    !offered
                        .iter()
                        .any(|d| d.mount_point == volume.drive.mount_point),
                    "{} is a fixed disk and was offered as removable",
                    volume.drive.mount_point.display()
                );
            }
        }
    }

    /// Free space above capacity would mean the two numbers came from different
    /// units, and the wizard checks free space to decide whether about 3 GB
    /// fits. It would then start a copy that cannot finish.
    #[test]
    fn reported_sizes_are_consistent_and_paths_are_absolute() {
        for volume in RealRemovableDrives
            .enumerate_volumes()
            .expect("enumerate_volumes")
        {
            let drive = &volume.drive;
            assert!(
                drive.mount_point.is_absolute(),
                "{} must be an absolute mount point",
                drive.mount_point.display()
            );
            if drive.total_bytes > 0 {
                assert!(
                    drive.free_bytes <= drive.total_bytes,
                    "{} reports {} free of {} total",
                    drive.mount_point.display(),
                    drive.free_bytes,
                    drive.total_bytes
                );
            }
            // An empty label must be absent rather than blank, or the picker
            // draws a drive with nothing where its name goes.
            assert_ne!(drive.label.as_deref(), Some(""));
            assert_ne!(drive.filesystem.as_deref(), Some(""));
        }
    }

    /// A picker that reshuffles between scans moves the row under the hand of
    /// somebody choosing where to write 3 GB.
    #[test]
    fn volumes_come_back_in_a_stable_order() {
        let listed: Vec<PathBuf> = RealRemovableDrives
            .enumerate_volumes()
            .expect("enumerate_volumes")
            .into_iter()
            .map(|v| v.drive.mount_point)
            .collect();
        let mut sorted = listed.clone();
        sorted.sort();
        assert_eq!(listed, sorted, "enumeration must be ordered");
    }

    /// `xbox::eject` turns `Ok(())` into "the drive has been ejected and is safe
    /// to unplug". Nothing that did not eject anything may return `Ok`.
    #[test]
    fn eject_refuses_instead_of_claiming_success() {
        let err = RealRemovableDrives
            .eject(Path::new("/no/such/mount"))
            .expect_err("eject must not claim success");
        assert!(matches!(err, Error::NotImplemented { .. }), "got {err:?}");
        let message = err.to_string();
        assert!(message.contains(CURRENT_PLATFORM), "{message}");
    }

    // -----------------------------------------------------------------------
    // Credentials
    // -----------------------------------------------------------------------

    /// Passed to the child process so it knows which credential to read.
    const CHILD_ACCOUNT_VAR: &str = "REBORN_VAULT_PROBE_ACCOUNT";
    const CHILD_SECRET_VAR: &str = "REBORN_VAULT_PROBE_SECRET";

    /// Printed by the child only after it has actually read the secret.
    ///
    /// The parent requires this in the child's output rather than trusting the
    /// exit status, because a test filter that matches nothing also exits zero,
    /// and a persistence proof that passes when it ran nothing is worse than no
    /// proof.
    const CHILD_PROOF: &str = "REBORN-VAULT-PERSISTED";

    /// The child test's full path, used as an exact filter.
    const CHILD_TEST: &str = "platform::imp::tests::child_reads_the_secret_the_parent_stored";

    /// A name unique to this run, so two test processes, or a second checkout,
    /// cannot delete each other's credential mid-test.
    fn probe_account(tag: &str) -> String {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        format!("reborn-test-{tag}-{}-{nanos}", std::process::id())
    }

    /// The Linux file vault: owner-only, round trip, absent is None, delete is idempotent.
    #[cfg(target_os = "linux")]
    #[test]
    fn the_linux_file_vault_keeps_a_secret_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(probe_account("vault")).join("credentials");
        assert_eq!(file_vault_retrieve(&dir, "discord").unwrap(), None);
        file_vault_store(&dir, "discord", "tok-123").unwrap();
        assert_eq!(file_vault_retrieve(&dir, "discord").unwrap().as_deref(), Some("tok-123"));
        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&dir), 0o700);
        assert_eq!(mode(&dir.join("discord")), 0o600);
        file_vault_store(&dir, "discord", "tok-456").unwrap();
        assert_eq!(file_vault_retrieve(&dir, "discord").unwrap().as_deref(), Some("tok-456"));
        file_vault_delete(&dir, "discord").unwrap();
        file_vault_delete(&dir, "discord").unwrap();
        assert_eq!(file_vault_retrieve(&dir, "discord").unwrap(), None);
        assert!(file_vault_store(&dir, "../escape", "x").is_err());
    }

    /// Whether to stop before touching the credential store.
    ///
    /// A Linux box with no Secret Service running genuinely has nowhere to put a
    /// secret, and that is the platform's state rather than a bug in this file.
    /// Windows always has a Credential Manager, so there the same condition is a
    /// real failure and is not skipped.
    fn no_store_here() -> bool {
        match keyring::Entry::store_status() {
            Ok(()) => false,
            Err(why) => {
                if cfg!(windows) {
                    panic!(
                        "Windows always has a Credential Manager, so this is a \
                         real failure and must not be skipped: {why}"
                    );
                }
                eprintln!("skipped: no platform credential store here ({why})");
                true
            }
        }
    }

    #[test]
    fn a_secret_round_trips_and_delete_actually_removes_it() {
        if no_store_here() {
            return;
        }
        let vault = OsCredentialVault;
        let account = probe_account("round-trip");
        // Shaped like a real bearer token, plus the awkward characters, because
        // a vault that mangles whitespace fails at the server with an
        // authentication error and nobody looks here.
        let secret = "Bot MTIzNDU2Nzg5MC.aBcDeF.gHiJkL-_09 with spaces\n\tand a tab";

        vault.store(&account, secret).expect("store");
        let read = vault.retrieve(&account);
        // Cleaned up before asserting: a failed assertion must not leave a
        // credential behind in the user's own store.
        let deleted = vault.delete(&account);

        assert_eq!(
            read.expect("retrieve").as_deref(),
            Some(secret),
            "the secret came back changed"
        );
        deleted.expect("delete");
        assert_eq!(
            vault.retrieve(&account).expect("retrieve after delete"),
            None,
            "delete reported success and the secret is still readable"
        );
        vault
            .delete(&account)
            .expect("deleting an absent credential must be a no-op, or signing out twice fails");
    }

    /// "Never signed in" and "the store would not answer" are different facts. A
    /// launcher that cannot tell them apart shows a store error to somebody who
    /// has simply never signed in.
    #[test]
    fn an_account_that_was_never_stored_reads_as_absent_not_as_an_error() {
        if no_store_here() {
            return;
        }
        let account = probe_account("never-stored");
        assert_eq!(
            OsCredentialVault.retrieve(&account).expect("retrieve"),
            None
        );
    }

    /// Refused before the store is touched: an empty account name is accepted by
    /// Windows Credential Manager and files the secret under the bare service
    /// name, where it belongs to no feature and nothing can be told to delete
    /// it.
    #[test]
    fn an_empty_account_name_is_refused_rather_than_filed_under_nothing() {
        let err = OsCredentialVault
            .store("", "a secret")
            .expect_err("an unnamed credential must be refused");
        assert!(matches!(err, Error::CredentialStore(_)), "got {err:?}");
        assert!(
            err.to_string().contains("account name"),
            "the message must say what is wrong: {err}"
        );
    }

    /// A secret must come back whole or not be stored at all. A silently
    /// truncated token looks valid and fails at the server, which sends everyone
    /// hunting in the wrong place.
    #[test]
    fn a_secret_is_either_stored_whole_or_refused_never_truncated() {
        if no_store_here() {
            return;
        }
        let vault = OsCredentialVault;
        let account = probe_account("too-long");
        // Windows Credential Manager caps a secret at 2560 bytes, that is 1280
        // UTF-16 units, measured with keyring 4.2 on this machine. Other
        // platforms may well accept this, which is why both outcomes are
        // allowed and only truncation is not.
        let secret = "x".repeat(4096);

        let stored = vault.store(&account, &secret);
        let read = vault.retrieve(&account);
        let _ = vault.delete(&account);

        match stored {
            Ok(()) => assert_eq!(
                read.expect("retrieve").as_deref(),
                Some(secret.as_str()),
                "the store accepted the secret and must return all of it"
            ),
            Err(e) => {
                let message = e.to_string();
                assert!(
                    message.contains("truncated"),
                    "a refusal must say the secret was not truncated: {message}"
                );
            }
        }
    }

    /// The invariant that cannot be asserted in one process, only demonstrated:
    /// the secret is in the operating system's store and not in this process's
    /// memory. A vault that forgot everything on exit would pass every other
    /// test in this file.
    ///
    /// So this stores a secret and then runs this same test binary again as a
    /// separate process, which reads it back. The child is `#[ignore]`d so an
    /// ordinary run does not execute it without the environment it needs.
    #[test]
    fn a_stored_secret_outlives_the_process_that_stored_it() {
        if no_store_here() {
            return;
        }
        let vault = OsCredentialVault;
        let account = probe_account("persistence");
        let secret = "persisted-across-a-process-boundary";
        vault.store(&account, secret).expect("store");

        let exe = std::env::current_exe().expect("this test binary's own path");
        let run = std::process::Command::new(&exe)
            .args([CHILD_TEST, "--exact", "--ignored", "--nocapture"])
            .env(CHILD_ACCOUNT_VAR, &account)
            .env(CHILD_SECRET_VAR, secret)
            .output();

        // Cleaned up before asserting, so a failure does not leave a credential
        // behind in the user's own store.
        let _ = vault.delete(&account);

        let run = run.expect("running the test binary as a child process");
        let stdout = String::from_utf8_lossy(&run.stdout);
        let stderr = String::from_utf8_lossy(&run.stderr);
        assert!(
            run.status.success(),
            "the child process could not read the secret:\n{stdout}\n{stderr}"
        );
        assert!(
            stdout.contains(CHILD_PROOF),
            "the child never confirmed the read, so nothing was proved:\n{stdout}"
        );
    }

    /// Run only by [`a_stored_secret_outlives_the_process_that_stored_it`], in a
    /// second process.
    #[test]
    #[ignore = "run as a child process to prove the vault persists across processes"]
    fn child_reads_the_secret_the_parent_stored() {
        let (Ok(account), Ok(secret)) = (
            std::env::var(CHILD_ACCOUNT_VAR),
            std::env::var(CHILD_SECRET_VAR),
        ) else {
            // Somebody ran the ignored tests by hand. There is nothing to prove
            // and nothing to fail, so say so and stop.
            println!("no account was passed in the environment, nothing to read");
            return;
        };

        let read = OsCredentialVault
            .retrieve(&account)
            .expect("retrieve in the child process");
        assert_eq!(
            read.as_deref(),
            Some(secret.as_str()),
            "the secret did not survive the process that stored it"
        );
        println!("{CHILD_PROOF}");
    }

    // -----------------------------------------------------------------------
    // Channels: a beta never shares a root with stable
    // -----------------------------------------------------------------------

    #[test]
    fn the_install_root_is_the_folder_holding_the_running_launcher() {
        // Since beta.8 the program lives in `app` inside the data root; the
        // fixed Programs path made a fresh beta.8 seed nothing and plan a
        // 6.4 GB download of the content sitting beside its own exe.
        let dir = tempfile::tempdir().unwrap();
        let tool = dir.path().join("rebornctl.exe");
        std::fs::write(&tool, b"").unwrap();
        assert_eq!(RealPaths::install_dir_holding(&tool), None, "no launcher beside it: not an install");
        std::fs::write(dir.path().join("reborn-launcher.exe"), b"").unwrap();
        assert_eq!(RealPaths::install_dir_holding(&tool).as_deref(), Some(dir.path()));
        // This test binary has no launcher beside it, so the channel path
        // still answers and the roots below stay apart.
        assert!(RealPaths::install_dir_holding(&std::env::current_exe().unwrap()).is_none());
    }

    /// The AppImage's own folder is where the player put the launcher; the
    /// executable's folder is a mount that is gone at exit.
    #[test]
    fn the_linux_install_root_is_the_appimage_folder_when_there_is_one() {
        let appimage = Path::new("/home/deck/Applications/Reborn.AppImage");
        let exe = Path::new("/tmp/.mount_Reborn123/usr/bin/reborn-launcher");
        assert_eq!(RealPaths::install_root_linux(Some(appimage), exe), PathBuf::from("/home/deck/Applications"));
        assert_eq!(RealPaths::install_root_linux(None, exe), PathBuf::from("/tmp/.mount_Reborn123/usr/bin"));
    }

    #[test]
    fn the_data_root_is_the_product_name_in_the_platform_data_folder() {
        let stable = RealPaths { channel: Channel::Stable }.data_root().unwrap();
        let beta = RealPaths { channel: Channel::Beta }.data_root().unwrap();
        assert_eq!(stable.file_name().unwrap().to_string_lossy(), "NFL Street Reborn");
        assert_eq!(beta.file_name().unwrap().to_string_lossy(), "NFL Street Reborn Beta");
        assert_eq!(stable.parent(), beta.parent());
    }

    #[test]
    fn the_previous_data_root_is_the_launcher_era_folder_beside_the_new_one() {
        for (channel, name) in [(Channel::Stable, "NFL Street Reborn Launcher"), (Channel::Beta, "NFL Street Reborn Launcher Beta")] {
            let paths = RealPaths { channel };
            let data = paths.data_root().unwrap();
            let previous = paths.previous_data_root().unwrap().expect("a previous root is always named");
            assert_eq!(previous.file_name().unwrap().to_string_lossy(), name);
            assert_eq!(previous.parent(), data.parent());
        }
    }

    #[test]
    fn a_beta_keeps_every_root_apart_from_stable() {
        // The property that makes a beta a test environment. If any root were
        // shared, a beta installer under test would be writing into the real
        // launcher's data, and uninstalling the beta would remove it.
        let stable = RealPaths { channel: Channel::Stable };
        let beta = RealPaths { channel: Channel::Beta };
        // On Linux and macOS the install root is where the running app is (an
        // AppImage or a .app lives wherever the player put it), so two
        // channels run from one test binary share it. The roots an uninstall
        // could reach, data and cache, are the ones that must stay apart there.
        let mut roots = vec![
            ("data", stable.data_root(), beta.data_root()),
            ("cache", stable.cache_root(), beta.cache_root()),
        ];
        if !cfg!(any(target_os = "linux", target_os = "macos")) {
            roots.push(("install", stable.install_root(), beta.install_root()));
        }
        for (name, s, b) in roots {
            let (s, b) = (s.unwrap(), b.unwrap());
            assert_ne!(s, b, "{name} root must differ");
            assert!(!b.starts_with(&s) && !s.starts_with(&b), "{name}: {} vs {}", s.display(), b.display());
            assert!(b.to_string_lossy().contains("Beta"), "{name}: {} should say Beta", b.display());
            assert!(!s.to_string_lossy().contains("Beta"), "{name}: {} must not", s.display());
        }
        // And the beta's own two roots stay on separate branches, the way
        // stable's do: an uninstall of the beta cannot reach beta saves either.
        let install = beta.install_root().unwrap();
        let data = beta.data_root().unwrap();
        assert!(!data.starts_with(&install));
    }

    #[test]
    fn the_beta_data_root_sits_beside_the_stable_one_and_the_2_1_install() {
        // Migration finds the 2.1 install as a sibling of the data root. A beta
        // whose data root were nested one level deeper would stop finding it.
        let stable = RealPaths { channel: Channel::Stable }.data_root().unwrap();
        let beta = RealPaths { channel: Channel::Beta }.data_root().unwrap();
        assert_eq!(stable.parent(), beta.parent());
    }

    #[test]
    fn channel_names_are_forgiving_and_unknown_ones_fall_back_to_stable() {
        assert_eq!(Channel::from_name("beta"), Some(Channel::Beta));
        assert_eq!(Channel::from_name("  BETA\r\n"), Some(Channel::Beta));
        assert_eq!(Channel::from_name(""), Some(Channel::Stable));
        assert_eq!(Channel::from_name("stable"), Some(Channel::Stable));
        assert_eq!(Channel::from_name("beat"), None);

        // The environment wins over the file, so a developer can run an
        // installed beta as stable, or the reverse, without touching it.
        let r = Channel::resolve(Some("stable"), Some("beta"));
        assert_eq!((r.channel, r.source), (Channel::Stable, ChannelSource::Environment));
        let r = Channel::resolve(None, Some("beta\n"));
        assert_eq!((r.channel, r.source), (Channel::Beta, ChannelSource::MarkerFile));
        let r = Channel::resolve(None, None);
        assert_eq!((r.channel, r.source), (Channel::Stable, ChannelSource::Absent));

        // A typo must not silently become a third channel with its own roots,
        // and must not stop the launcher either. It is stable, and it says so.
        let r = Channel::resolve(None, Some("beat"));
        assert_eq!(r.channel, Channel::Stable);
        assert_eq!(r.source, ChannelSource::Unknown("beat".to_string()));
        assert!(r.explain().contains("\"beat\""), "{}", r.explain());
    }

    /// A real `Application Error` record, as `wevtutil qe Application /f:xml`
    /// emitted it on 2026-09-20 for the shipped runtime 1.2.4 build. Kept
    /// verbatim rather than hand written: the first version of this parser read
    /// unnamed `<Data>` elements, which this payload does not have, and it would
    /// have found nothing forever without ever saying so.
    #[cfg(windows)]
    const CRASH_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<Event xmlns='http://schemas.microsoft.com/win/2004/08/events/event'><System>
<Provider Name='Application Error' Guid='{a0e9b465-b939-57d7-b27d-95d8e925ff57}'/>
<EventID>1000</EventID><Level>2</Level>
<TimeCreated SystemTime='2026-09-20T02:40:10.8602063Z'/>
<Execution ProcessID='22676' ThreadID='28744'/>
<Channel>Application</Channel></System><EventData>
<Data Name='AppName'>NFL Street Reborn Dolphin.exe</Data>
<Data Name='AppVersion'>1.1.0.0</Data>
<Data Name='AppTimeStamp'>45e4af26</Data>
<Data Name='ModuleName'>NFL Street Reborn Dolphin.exe</Data>
<Data Name='ModuleVersion'>1.1.0.0</Data>
<Data Name='ModuleTimeStamp'>45e4af26</Data>
<Data Name='ExceptionCode'>c0000005</Data>
<Data Name='FaultingOffset'>0000000000637940</Data>
<Data Name='ProcessId'>0x541c</Data>
<Data Name='ProcessCreationTime'>0x1dd48a863d5733d</Data>
<Data Name='AppPath'>C:\Users\Adam\AppData\Roaming\NFL Street Reborn\runtime\NFL Street Reborn Dolphin.exe</Data>
<Data Name='ModulePath'>C:\Users\Adam\AppData\Roaming\NFL Street Reborn\runtime\NFL Street Reborn Dolphin.exe</Data>
</EventData></Event>"#;

    #[test]
    #[cfg(windows)]
    fn a_crash_record_gives_up_the_module_and_the_offset() {
        // 0x541c is the id Windows recorded for the process that died.
        let site = fault_site_in(CRASH_XML, 0x541c).expect("the record is for this process");
        assert_eq!(site.module, "NFL Street Reborn Dolphin.exe");
        // The offset is the whole point: this one resolved to
        // DX11::D3D::StateManager::UnsetTexture in the build that produced it.
        assert_eq!(site.offset, "0000000000637940");
    }

    /// The trap this test exists for: `<Execution ProcessID='22676'>` in the
    /// System block is the service that WROTE the record, not the process that
    /// crashed. Reading that one would attach a stranger's fault site to every
    /// crash, and it would look entirely plausible in the log.
    #[test]
    #[cfg(windows)]
    fn the_reporting_service_is_not_mistaken_for_the_crashed_process() {
        assert_eq!(fault_site_in(CRASH_XML, 22676), None);
    }

    #[test]
    #[cfg(windows)]
    fn a_record_belonging_to_another_process_is_never_borrowed() {
        assert_eq!(fault_site_in(CRASH_XML, 0x1234), None);
    }

    #[test]
    #[cfg(windows)]
    fn double_quoted_attributes_read_the_same_as_single_quoted() {
        let doubled = CRASH_XML.replace('\'', "\"");
        let site = fault_site_in(&doubled, 0x541c).expect("quoting is the emitter's choice");
        assert_eq!(site.offset, "0000000000637940");
    }

    #[test]
    #[cfg(windows)]
    fn nothing_to_read_is_not_an_answer() {
        assert_eq!(fault_site_in("", 0x541c), None);
        assert_eq!(fault_site_in("<Event ><EventData></EventData></Event>", 0x541c), None);
    }

    /// The fault site is extra detail, never a precondition: a crash Windows
    /// recorded nothing for is still reported as a crash, in its own words.
    #[test]
    fn a_crash_without_a_record_still_reads_as_a_crash() {
        let plain = ExitKind::Crashed { detail: "access violation (0xC0000005)".into() };
        assert!(with_fault_site(plain, 0).is_crash());
        assert_eq!(with_fault_site(ExitKind::Normal, 0), ExitKind::Normal);
    }

}
