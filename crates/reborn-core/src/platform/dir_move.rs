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

/// Remove a folder link made by [`make_dir_link`], never what it leads to.
/// A junction is removed as a folder; a Unix symbolic link is a file.
pub fn remove_dir_link(link: &Path) -> io::Result<()> {
    if cfg!(windows) {
        std::fs::remove_dir(link)
    } else {
        std::fs::remove_file(link)
    }
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

/// The check behind [`others_can_replace_children`], as PowerShell: exit 0
/// when only this account, SYSTEM, Administrators, TrustedInstaller (and
/// CREATOR OWNER, which grants nothing to anyone else) can delete what is
/// inside the folder or change the folder's permissions or owner; exit 1
/// otherwise, and on any error (an unreadable answer is not a safe one).
/// The installer carries the same script (reborn-launcher.iss).
pub const REPLACE_CHECK_PS1: &str = r#"
$ErrorActionPreference = 'Stop'
$path = $env:REBORN_ACL_PATH
$me = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
$trusted = @('S-1-5-18', 'S-1-5-32-544', 'S-1-3-0', 'S-1-5-80-956008885-3418522649-1831038044-1853292631-2271478464', $me)
$acl = Get-Acl -LiteralPath $path
if ($trusted -notcontains $acl.GetOwner([Security.Principal.SecurityIdentifier]).Value) { exit 1 }
# FILE_DELETE_CHILD, WRITE_DAC, WRITE_OWNER, GENERIC_ALL
$risky = 0x40 -bor 0x40000 -bor 0x80000 -bor 0x10000000
foreach ($r in $acl.GetAccessRules($true, $true, [Security.Principal.SecurityIdentifier])) {
  if ($r.AccessControlType -ne 'Allow') { continue }
  if ($r.PropagationFlags -band [Security.AccessControl.PropagationFlags]::InheritOnly) { continue }
  if (([int64]$r.FileSystemRights -band $risky) -and ($trusted -notcontains $r.IdentityReference.Value)) { exit 1 }
}
exit 0
"#;

/// Whether another account on this PC could delete or rename what is
/// inside `dir` (and so swap a folder there for one of its own), or take
/// over `dir`'s permissions. A private folder only stays ours if the folder
/// holding it does not allow that (security review, 2026-10-09).
#[cfg(windows)]
pub fn others_can_replace_children(dir: &Path) -> io::Result<bool> {
    use std::os::windows::process::CommandExt;
    let system = std::env::var_os("SystemRoot").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows")).join("System32");
    let status = std::process::Command::new(system.join("WindowsPowerShell").join("v1.0").join("powershell.exe"))
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", REPLACE_CHECK_PS1])
        .env("REBORN_ACL_PATH", plain(dir))
        .creation_flags(0x0800_0000)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()?;
    Ok(!status.success())
}

/// Whether `path` is owned by this account. Another account cannot make a
/// folder owned by this one, so an owned folder is one this account made.
#[cfg(windows)]
pub fn owned_by_me(path: &Path) -> io::Result<bool> {
    use std::os::windows::process::CommandExt;
    let system = std::env::var_os("SystemRoot").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows")).join("System32");
    let script = "$ErrorActionPreference='Stop'; if ((Get-Acl -LiteralPath $env:REBORN_ACL_PATH).GetOwner([Security.Principal.SecurityIdentifier]).Value -eq [Security.Principal.WindowsIdentity]::GetCurrent().User.Value) { exit 0 } else { exit 1 }";
    let status = std::process::Command::new(system.join("WindowsPowerShell").join("v1.0").join("powershell.exe"))
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", script])
        .env("REBORN_ACL_PATH", plain(path))
        .creation_flags(0x0800_0000)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()?;
    Ok(status.success())
}

#[cfg(unix)]
pub fn owned_by_me(path: &Path) -> io::Result<bool> {
    use std::os::unix::fs::MetadataExt;
    let uid = std::fs::symlink_metadata(path)?.uid();
    // This process's own uid, read from a file it has just created itself.
    // create_new refuses a name that is already there, so another account
    // cannot plant the probe (or a link to its own file) ahead of it and
    // answer for us.
    let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let probe = std::env::temp_dir().join(format!("reborn-uid-{}-{nonce}", std::process::id()));
    let file = std::fs::OpenOptions::new().write(true).create_new(true).open(&probe)?;
    let me = file.metadata().map(|m| m.uid());
    drop(file);
    let _ = std::fs::remove_file(&probe);
    Ok(uid == me?)
}

#[cfg(not(any(windows, unix)))]
pub fn owned_by_me(_path: &Path) -> io::Result<bool> {
    Ok(false)
}

/// Unix: anyone but the owner may write the folder, and no sticky bit
/// stops them removing what others put there.
#[cfg(unix)]
pub fn others_can_replace_children(dir: &Path) -> io::Result<bool> {
    use std::os::unix::fs::PermissionsExt;
    let mode = std::fs::metadata(dir)?.permissions().mode();
    Ok(mode & 0o022 != 0 && mode & 0o1000 == 0)
}

#[cfg(not(any(windows, unix)))]
pub fn others_can_replace_children(_dir: &Path) -> io::Result<bool> {
    Ok(true)
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
        remove_dir_link(&link).unwrap();
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
    #[cfg(windows)]
    fn a_folder_others_may_empty_or_take_over_is_seen_as_replaceable() {
        let dir = tempfile::tempdir().unwrap();
        // A fresh private folder, plus one extra grant.
        let with = |name: &str, grant: Option<&str>| {
            let folder = dir.path().join(name);
            std::fs::create_dir(&folder).unwrap();
            make_private(&folder).unwrap();
            if let Some(g) = grant {
                assert!(std::process::Command::new("icacls").arg(&folder).args(["/grant", g]).output().unwrap().status.success());
            }
            folder
        };
        let private = with("private", None);
        assert!(!others_can_replace_children(&private).unwrap(), "only us, SYSTEM, Administrators");
        assert!(owned_by_me(&private).unwrap());
        // Everyone may delete what is inside: the D:\Games shape.
        assert!(others_can_replace_children(&with("delete-child", Some("*S-1-1-0:(DC)"))).unwrap());
        // Every signed-in account may change its permissions.
        assert!(others_can_replace_children(&with("write-dac", Some("*S-1-5-11:(WDAC)"))).unwrap());
        // An inherit-only grant reaches only what is made inside, not the
        // folder: the drive root's usual "Authenticated Users" entry.
        assert!(!others_can_replace_children(&with("inherit-only", Some("*S-1-5-11:(OI)(CI)(IO)(M)"))).unwrap());
    }

    #[test]
    #[cfg(windows)]
    fn the_system_drive_root_does_not_let_others_replace_its_folders() {
        let root = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into()) + "\\";
        assert!(!others_can_replace_children(Path::new(&root)).unwrap());
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
