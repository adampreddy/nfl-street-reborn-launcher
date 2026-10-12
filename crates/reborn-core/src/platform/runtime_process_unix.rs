//! Dolphin detection off Windows, under the same rule as `runtime_process`:
//! only this exact executable counts, so another install with the same file
//! name never blocks this one.
//!
//! Before this existed, `runtime::runtime_is_running` answered `false` on every
//! platform but Windows, so "close Reborn Dolphin first" protected nothing and
//! Dolphin's exit wrote its in-memory settings over whatever the launcher had
//! just saved.
//!
//! Linux reads `/proc/<pid>/exe`. A process this account may not inspect is
//! skipped rather than treated as an error: on Linux that is every root daemon
//! on the machine, and the Dolphin the launcher started always runs as the
//! player. macOS has no `/proc` and asks `ps`.
//!
//! Compiled on every platform so its rules are tested on the Windows machine
//! that builds the release; only [`is_running`] is Unix-only.
use std::{ffi::OsString, io, path::{Path, PathBuf}};

/// Whether a process is running this executable.
#[cfg(unix)]
pub(crate) fn is_running(exe: &Path) -> io::Result<bool> {
    let canonical = std::fs::canonicalize(exe).ok();
    Ok(running_images()?.iter().any(|image| same_image(image, exe, canonical.as_deref())))
}

#[cfg(target_os = "linux")]
fn running_images() -> io::Result<Vec<PathBuf>> {
    let entries = std::fs::read_dir("/proc")?
        .flatten()
        .filter(|entry| {
            // NUL-separated arguments; an unreadable one is not a helper.
            let cmdline = std::fs::read(entry.path().join("cmdline")).unwrap_or_default();
            !is_launcher_helper(&String::from_utf8_lossy(&cmdline).replace('\0', " "))
        })
        .map(|entry| {
            let name = entry.file_name();
            let image = std::fs::read_link(entry.path().join("exe"));
            (name, image)
        });
    Ok(images_from_proc(entries))
}

#[cfg(all(unix, not(target_os = "linux")))]
fn running_images() -> io::Result<Vec<PathBuf>> {
    let ps = |format: &str| -> io::Result<Vec<u8>> {
        let output = std::process::Command::new("ps")
            .args(["-axww", "-o", format])
            .stdin(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .output()?;
        if !output.status.success() {
            return Err(io::Error::other("ps failed"));
        }
        Ok(output.stdout)
    };
    // The executable path and the full command line, joined by pid: the path
    // alone cannot tell a game from the launcher's own helper runs, and the
    // command line alone cannot be split back into a path that has spaces.
    parse_ps_with_args(&ps("pid=,comm=")?, &ps("pid=,args=")?)
}

/// The launcher's own short runs of the emulator, which are not a game
/// session and must never block one: the remap wizard's device reader
/// (`--input-capture`) and the pad scan (`--dump-device-inputs`).
fn is_launcher_helper(command_line: &str) -> bool {
    command_line.split_whitespace().any(|arg| {
        arg.starts_with("--input-capture") || arg.starts_with("--dump-device-inputs")
    })
}

/// `ps -o pid=,comm=` and `ps -o pid=,args=`, joined by pid, without the
/// launcher's helper runs. A listing with no process at all is never read as
/// "nothing is running": `ps` itself was running. One holding only helpers is.
#[cfg_attr(any(windows, target_os = "linux"), allow(dead_code))]
fn parse_ps_with_args(comm: &[u8], args: &[u8]) -> io::Result<Vec<PathBuf>> {
    let split = |line: &str| -> Option<(u32, String)> {
        let line = line.trim_start();
        let (pid, rest) = line.split_once(char::is_whitespace)?;
        Some((pid.parse().ok()?, rest.trim().to_string()))
    };
    let args_text = String::from_utf8_lossy(args);
    let helpers: std::collections::HashSet<u32> = args_text
        .lines()
        .filter_map(split)
        .filter(|(_, line)| is_launcher_helper(line))
        .map(|(pid, _)| pid)
        .collect();
    let comm_text = String::from_utf8_lossy(comm);
    let all: Vec<(u32, String)> = comm_text.lines().filter_map(split).filter(|(_, p)| !p.is_empty()).collect();
    if all.is_empty() {
        return Err(io::Error::other("empty process list"));
    }
    Ok(all.into_iter().filter(|(pid, _)| !helpers.contains(pid)).map(|(_, p)| PathBuf::from(p)).collect())
}

/// The executables behind `/proc`'s numeric entries. Anything else there
/// (`self`, `sys`, `meminfo`) is not a process. A link that cannot be read is
/// a kernel thread, a process that exited between the listing and the read,
/// or one owned by another account; none of those is the Dolphin this
/// launcher started.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn images_from_proc(entries: impl Iterator<Item = (OsString, io::Result<PathBuf>)>) -> Vec<PathBuf> {
    entries
        .filter(|(name, _)| name.to_str().is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())))
        .filter_map(|(_, image)| image.ok())
        .collect()
}

/// Linux reports an executable that was replaced on disk as `<path> (deleted)`.
/// A runtime update renames the new file over the one Dolphin is running
/// from, and that Dolphin is still this runtime, holding this user folder in
/// memory.
#[cfg_attr(windows, allow(dead_code))]
fn without_deleted_marker(image: &Path) -> PathBuf {
    let text = image.as_os_str().to_string_lossy();
    match text.strip_suffix(" (deleted)") {
        Some(original) => PathBuf::from(original),
        None => image.to_path_buf(),
    }
}

#[cfg_attr(windows, allow(dead_code))]
fn same_image(image: &Path, exe: &Path, canonical: Option<&Path>) -> bool {
    let image = without_deleted_marker(image);
    image == exe || canonical.is_some_and(|c| image == c)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, image: io::Result<&str>) -> (OsString, io::Result<PathBuf>) {
        (name.into(), image.map(PathBuf::from))
    }

    #[test]
    fn only_process_entries_with_a_readable_image_count() {
        let denied = || Err(io::Error::from(io::ErrorKind::PermissionDenied));
        let images = images_from_proc(
            [
                entry("1", denied()),
                entry("42", Ok("/opt/reborn/runtime/dolphin-emu")),
                entry("self", Ok("/usr/bin/ls")),
                entry("sys", denied()),
                entry("77", Err(io::Error::from(io::ErrorKind::NotFound))),
                entry("", Ok("/nothing")),
            ]
            .into_iter(),
        );
        assert_eq!(images, vec![PathBuf::from("/opt/reborn/runtime/dolphin-emu")]);
    }

    #[test]
    fn another_install_with_the_same_name_is_not_this_runtime() {
        let exe = Path::new("/home/deck/.local/share/NFL Street Reborn/runtime/dolphin-emu");
        assert!(same_image(exe, exe, None));
        assert!(!same_image(Path::new("/usr/bin/dolphin-emu"), exe, None));
        assert!(!same_image(Path::new("/home/deck/Other/runtime/dolphin-emu"), exe, None));
    }

    #[test]
    fn a_runtime_replaced_while_running_is_still_running() {
        let exe = Path::new("/data/runtime/dolphin-emu");
        assert!(same_image(Path::new("/data/runtime/dolphin-emu (deleted)"), exe, None));
    }

    #[test]
    fn a_link_to_the_runtime_matches_through_its_real_path() {
        let exe = Path::new("/home/deck/Reborn/runtime/dolphin-emu");
        let real = Path::new("/run/media/sd/Reborn/runtime/dolphin-emu");
        assert!(same_image(real, exe, Some(real)));
    }

    /// The launcher's own short runs of the emulator (the remap wizard's
    /// device reader, the pad scan) are not a game session. Counting them
    /// told a Mac first run to "Close Reborn Dolphin to continue" about a
    /// process the launcher itself had started (2026-10-08).
    #[test]
    fn the_launchers_own_helper_runs_are_not_a_session() {
        assert!(is_launcher_helper("/rt/dolphin-emu --input-capture=/data/cache/remap"));
        assert!(is_launcher_helper("/rt/dolphin-emu -u /u --dump-device-inputs /tmp/d.txt"));
        assert!(!is_launcher_helper("/rt/dolphin-emu -u /u -e /games/NSR.iso"));
        assert!(!is_launcher_helper("/rt/dolphin-emu"), "a Dolphin opened by hand is a session");
    }

    #[test]
    fn a_mac_listing_drops_helper_runs_by_pid() {
        let exe = "/Applications/NFL Street Reborn.app/Contents/Resources/runtime/NFL Street Reborn Dolphin.app/Contents/MacOS/NFL Street Reborn Dolphin";
        let comm = format!("    1 /sbin/launchd\n  501 {exe}\n  777 {exe}\n");
        let args = format!("    1 /sbin/launchd\n  501 {exe} --input-capture=/x/remap\n  777 {exe} -u /u -e /g/NSR.iso\n");
        let images = parse_ps_with_args(comm.as_bytes(), args.as_bytes()).unwrap();
        assert_eq!(images, vec![PathBuf::from("/sbin/launchd"), PathBuf::from(exe)], "the game stays, the helper goes");
        let only_helper = format!("  501 {exe}\n");
        let helper_args = format!("  501 {exe} --input-capture=/x/remap\n");
        assert!(parse_ps_with_args(only_helper.as_bytes(), helper_args.as_bytes()).unwrap().is_empty(),
            "a listing holding only a helper is a machine with no session, not an error");
        assert!(parse_ps_with_args(b"", b"").is_err());
    }
}
