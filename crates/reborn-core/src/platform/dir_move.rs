//! The operating-system pieces of "Move to another drive" (see
//! [`crate::relocate`]): what a volume can hold, a folder link that needs no
//! administrator rights, and whether a process is still alive.

use std::io;
use std::path::{Path, PathBuf};

/// What the move needs to know about the volume a folder lives on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VolumeFacts {
    pub mount: PathBuf,
    pub free_bytes: u64,
    pub filesystem: String,
    pub removable: bool,
}

/// A path without Windows' `\\?\` prefix, which `canonicalize` adds and which
/// no person, and no prefix comparison against a mount point, expects.
pub fn plain(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    match text.strip_prefix(r"\\?\UNC\") {
        Some(rest) => PathBuf::from(format!(r"\\{rest}")),
        None => match text.strip_prefix(r"\\?\") {
            Some(rest) => PathBuf::from(rest),
            None => path.to_path_buf(),
        },
    }
}

/// Where `path` really is, through any junction, or `path` itself when it
/// does not exist.
pub fn real(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).map(|p| plain(&p)).unwrap_or_else(|_| path.to_path_buf())
}

/// The volume holding `path` (or its nearest existing ancestor): the mounted
/// volume with the longest mount point that prefixes the real path.
pub fn volume_facts(path: &Path) -> Option<VolumeFacts> {
    let mut existing = path;
    while !existing.exists() {
        existing = existing.parent()?;
    }
    let at = real(existing).to_string_lossy().to_lowercase();
    let disks = sysinfo::Disks::new_with_refreshed_list_specifics(sysinfo::DiskRefreshKind::nothing().with_storage());
    disks
        .list()
        .iter()
        .filter(|d| at.starts_with(&d.mount_point().to_string_lossy().to_lowercase()))
        .max_by_key(|d| d.mount_point().as_os_str().len())
        .map(|d| VolumeFacts {
            mount: d.mount_point().to_path_buf(),
            free_bytes: d.available_space(),
            filesystem: d.file_system().to_string_lossy().into_owned(),
            removable: d.is_removable(),
        })
}

/// Whether `path` is itself a link to a folder (a junction or a symbolic
/// link) rather than a real folder.
pub fn is_dir_link(path: &Path) -> bool {
    std::fs::symlink_metadata(path)
        .map(|m| {
            let t = m.file_type();
            t.is_symlink() || (!t.is_dir() && !t.is_file())
        })
        .unwrap_or(false)
}

/// Make `link` a folder link to `target`. On Windows a directory junction:
/// unlike a symbolic link it needs no administrator rights or Developer Mode,
/// and every program follows it, Dolphin included.
#[cfg(windows)]
pub fn make_dir_link(link: &Path, target: &Path) -> io::Result<()> {
    use std::os::windows::process::CommandExt;
    let (l, t) = (plain(link), plain(target));
    for p in [&l, &t] {
        if p.to_string_lossy().contains('"') {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "a folder name contains a quotation mark"));
        }
    }
    let root = std::env::var_os("SystemRoot").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
    let output = std::process::Command::new(root.join("System32").join("cmd.exe"))
        .raw_arg(format!("/D /C mklink /J \"{}\" \"{}\"", l.display(), t.display()))
        .creation_flags(0x0800_0000)
        .stdin(std::process::Stdio::null())
        .output()?;
    if !output.status.success() {
        let said = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(io::Error::other(if said.is_empty() { "mklink failed".to_string() } else { said }));
    }
    if real(&l) != real(&t) {
        return Err(io::Error::other("the folder link was made but does not lead to the new folder"));
    }
    Ok(())
}

#[cfg(unix)]
pub fn make_dir_link(link: &Path, target: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(not(any(windows, unix)))]
pub fn make_dir_link(_link: &Path, _target: &Path) -> io::Result<()> {
    Err(io::Error::new(io::ErrorKind::Unsupported, "folder links are not available here"))
}

/// Make `dir` private to this account (plus SYSTEM and Administrators),
/// for everything later created inside it.
///
/// A folder under the player's profile is private already. A folder on a
/// second drive inherits the drive's permissions, and on Windows those let
/// every signed-in account on the PC create and change files. The data root
/// holds the launcher and Dolphin themselves (`app`, `runtime`), so left
/// like that, another account could replace a program this player runs.
/// Inheritance is cut and three grants put in its place; what is copied in
/// afterwards inherits them.
#[cfg(windows)]
pub fn make_private(dir: &Path) -> io::Result<()> {
    use std::os::windows::process::CommandExt;
    let system = std::env::var_os("SystemRoot").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows")).join("System32");
    let who = std::process::Command::new(system.join("whoami.exe"))
        .args(["/user", "/fo", "csv", "/nh"])
        .creation_flags(0x0800_0000)
        .stdin(std::process::Stdio::null())
        .output()?;
    let sid = String::from_utf8_lossy(&who.stdout)
        .rsplit(',')
        .next()
        .map(|s| s.trim().trim_matches('"').to_string())
        .filter(|s| s.starts_with("S-1-"))
        .ok_or_else(|| io::Error::other("this account's security id could not be read"))?;
    let output = std::process::Command::new(system.join("icacls.exe"))
        .arg(plain(dir))
        .args(["/inheritance:r", "/grant:r"])
        .arg(format!("*{sid}:(OI)(CI)F"))
        // SYSTEM and Administrators, by well-known id so a non-English
        // Windows names them correctly.
        .args(["*S-1-5-18:(OI)(CI)F", "*S-1-5-32-544:(OI)(CI)F", "/Q"])
        .creation_flags(0x0800_0000)
        .stdin(std::process::Stdio::null())
        .output()?;
    if !output.status.success() {
        return Err(io::Error::other(String::from_utf8_lossy(&output.stdout).trim().to_string()));
    }
    Ok(())
}

#[cfg(unix)]
pub fn make_private(dir: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
}

#[cfg(not(any(windows, unix)))]
pub fn make_private(_dir: &Path) -> io::Result<()> {
    Ok(())
}

/// Whether the process `pid` is still running.
#[cfg(windows)]
pub fn pid_alive(pid: u32) -> bool {
    super::runtime_process::pid_alive(pid)
}

#[cfg(unix)]
pub fn pid_alive(pid: u32) -> bool {
    Path::new("/proc").join(pid.to_string()).exists()
        || std::process::Command::new("kill").args(["-0", &pid.to_string()]).status().is_ok_and(|s| s.success())
}

#[cfg(not(any(windows, unix)))]
pub fn pid_alive(_pid: u32) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_verbatim_prefix_is_dropped() {
        assert_eq!(plain(Path::new(r"\\?\D:\Games\NFL Street Reborn")), PathBuf::from(r"D:\Games\NFL Street Reborn"));
        assert_eq!(plain(Path::new(r"\\?\UNC\server\share\x")), PathBuf::from(r"\\server\share\x"));
        assert_eq!(plain(Path::new(r"C:\x")), PathBuf::from(r"C:\x"));
    }

    #[test]
    fn a_folder_link_leads_to_its_target_and_is_seen_as_a_link() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("real folder & co");
        std::fs::create_dir(&target).unwrap();
        std::fs::write(target.join("a.txt"), b"hi").unwrap();
        let link = dir.path().join("the link");
        make_dir_link(&link, &target).unwrap();
        assert!(is_dir_link(&link));
        assert!(!is_dir_link(&target));
        assert_eq!(std::fs::read(link.join("a.txt")).unwrap(), b"hi");
        assert_eq!(real(&link), real(&target));
        // Removing the link leaves the folder it led to.
        std::fs::remove_dir(&link).unwrap();
        assert!(target.join("a.txt").is_file());
    }

    /// The grants on `dir` as icacls lists them: one line per entry.
    #[cfg(windows)]
    pub(crate) fn grants(dir: &Path) -> Vec<String> {
        let out = std::process::Command::new("icacls").arg(dir).output().unwrap();
        let text = String::from_utf8_lossy(&out.stdout).replace(&dir.display().to_string(), "");
        text.lines().map(str::trim).filter(|l| l.contains(":(")).map(String::from).collect()
    }

    #[test]
    #[cfg(windows)]
    fn a_private_folder_has_exactly_its_three_grants_and_passes_them_down() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("moved");
        std::fs::create_dir(&folder).unwrap();
        make_private(&folder).unwrap();
        let own = grants(&folder);
        assert_eq!(own.len(), 3, "{own:?}");
        assert!(own.iter().all(|g| !g.contains("(I)")), "nothing inherited: {own:?}");
        // What is made inside afterwards inherits those three and nothing else.
        std::fs::create_dir(folder.join("app")).unwrap();
        std::fs::write(folder.join("app").join("x.exe"), b"x").unwrap();
        let child = grants(&folder.join("app").join("x.exe"));
        assert_eq!(child.len(), 3, "{child:?}");
        assert!(child.iter().all(|g| g.contains("(I)")), "{child:?}");
    }

    #[test]
    fn this_process_is_alive() {
        assert!(pid_alive(std::process::id()));
    }

    #[test]
    fn the_temp_folder_is_on_a_known_volume() {
        let dir = tempfile::tempdir().unwrap();
        let facts = volume_facts(&dir.path().join("not yet")).expect("a volume");
        assert!(facts.free_bytes > 0);
    }
}
