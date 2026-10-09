//! Which file a path really is: the volume it lives on, its index there, and
//! how many names (hard links) it has. Two paths that answer the same volume
//! and index are one file.
//!
//! Opening the path follows junctions and symbolic links, so a folder reached
//! through a junction answers the volume the junction points at. That is the
//! property a drive-letter comparison lacks once the data root has been moved
//! to another drive and a junction left in its place.

use std::io;
use std::path::Path;

/// One file's identity on its volume.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FileId {
    pub volume: u64,
    pub index: u64,
    pub links: u64,
}

#[cfg(windows)]
pub fn of(path: &Path) -> io::Result<FileId> {
    use std::os::windows::fs::OpenOptionsExt;
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION, FILE_FLAG_BACKUP_SEMANTICS,
        FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
    };
    // No access rights asked for: reading the identity needs none, and a file
    // another program holds open for writing still answers. Backup semantics
    // is what lets a directory be opened at all.
    let file = std::fs::OpenOptions::new()
        .access_mode(0)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)?;
    // SAFETY: BY_HANDLE_FILE_INFORMATION is a plain C record of integers.
    #[allow(unsafe_code)]
    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    // SAFETY: `file` owns a live handle for the duration of the call and
    // `info` is a writable, correctly sized local record.
    #[allow(unsafe_code)]
    let ok = unsafe { GetFileInformationByHandle(file.as_raw_handle() as _, &mut info) };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(FileId {
        volume: u64::from(info.dwVolumeSerialNumber),
        index: (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow),
        links: u64::from(info.nNumberOfLinks),
    })
}

#[cfg(unix)]
pub fn of(path: &Path) -> io::Result<FileId> {
    use std::os::unix::fs::MetadataExt;
    let meta = std::fs::metadata(path)?;
    Ok(FileId { volume: meta.dev(), index: meta.ino(), links: meta.nlink() })
}

#[cfg(not(any(windows, unix)))]
pub fn of(_path: &Path) -> io::Result<FileId> {
    Err(io::Error::new(io::ErrorKind::Unsupported, "file identity is not available here"))
}

/// The identity of `path`, or of its nearest ancestor that exists: what a
/// file created at `path` would share a volume with.
pub fn of_nearest(path: &Path) -> io::Result<FileId> {
    let mut current = path;
    loop {
        match of(current) {
            Ok(id) => return Ok(id),
            Err(error) => match current.parent() {
                Some(parent) if parent != current => current = parent,
                _ => return Err(error),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_names_of_one_file_answer_one_identity_and_count_both_links() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.bin");
        let b = dir.path().join("b.bin");
        let c = dir.path().join("c.bin");
        std::fs::write(&a, b"x").unwrap();
        std::fs::hard_link(&a, &b).unwrap();
        std::fs::write(&c, b"x").unwrap();
        let (ia, ib, ic) = (of(&a).unwrap(), of(&b).unwrap(), of(&c).unwrap());
        assert_eq!((ia.volume, ia.index), (ib.volume, ib.index));
        assert_ne!(ia.index, ic.index);
        assert_eq!(ia.links, 2);
        assert_eq!(ic.links, 1);
    }

    #[test]
    fn a_path_not_made_yet_answers_its_folder_s_volume() {
        let dir = tempfile::tempdir().unwrap();
        let near = of_nearest(&dir.path().join("not").join("yet")).unwrap();
        assert_eq!(near.volume, of(dir.path()).unwrap().volume);
    }
}
