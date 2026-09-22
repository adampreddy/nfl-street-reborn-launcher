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
        .map(|entry| {
            let name = entry.file_name();
            let image = std::fs::read_link(entry.path().join("exe"));
            (name, image)
        });
    Ok(images_from_proc(entries))
}

#[cfg(all(unix, not(target_os = "linux")))]
fn running_images() -> io::Result<Vec<PathBuf>> {
    let output = std::process::Command::new("ps")
        .args(["-axww", "-o", "comm="])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()?;
    if !output.status.success() {
        return Err(io::Error::other("ps failed"));
    }
    parse_ps(&output.stdout)
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

/// `ps -o comm=` prints one executable path per line. An empty listing is
/// never read as "nothing is running": `ps` itself was running.
#[cfg_attr(any(windows, target_os = "linux"), allow(dead_code))]
fn parse_ps(bytes: &[u8]) -> io::Result<Vec<PathBuf>> {
    let images: Vec<PathBuf> = String::from_utf8_lossy(bytes)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .collect();
    if images.is_empty() {
        return Err(io::Error::other("empty process list"));
    }
    Ok(images)
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

    #[test]
    fn an_empty_ps_listing_is_an_error_not_an_idle_machine() {
        assert!(parse_ps(b"").is_err());
        assert!(parse_ps(b"\n  \n").is_err());
        assert_eq!(
            parse_ps(b"/sbin/launchd\n/Applications/NFL Street Reborn.app/Contents/MacOS/Dolphin\n").unwrap(),
            vec![
                PathBuf::from("/sbin/launchd"),
                PathBuf::from("/Applications/NFL Street Reborn.app/Contents/MacOS/Dolphin"),
            ]
        );
    }
}
