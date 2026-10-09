//! Read-only Dolphin detection. Enumerate names without opening other processes,
//! then request only QUERY_LIMITED_INFORMATION for matching candidates. In
//! particular, do not request VM_READ (the sysinfo 0.38 path used in 3.0.1 did).
use std::{ffi::OsString, io, path::{Path, PathBuf}};
use std::os::windows::{ffi::OsStringExt, process::CommandExt};
use windows_sys::Win32::{Foundation::*, System::{Diagnostics::ToolHelp::*, Threading::*}};

#[derive(Debug)]
struct Entry { pid: u32, name: OsString }

struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: constructed only for a successful owned Win32 handle; closed once.
        #[allow(unsafe_code)]
        unsafe { CloseHandle(self.0); }
    }
}

fn snapshot() -> io::Result<Vec<Entry>> {
    // SAFETY: process-only snapshot; no pointer parameters or inherited handles.
    #[allow(unsafe_code)]
    let raw = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if raw == INVALID_HANDLE_VALUE { return Err(io::Error::last_os_error()); }
    let handle = Handle(raw);
    // SAFETY: PROCESSENTRY32W is a plain C record of integer fields and an array.
    #[allow(unsafe_code)]
    let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
    entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
    // SAFETY: live snapshot handle and writable, correctly sized local record.
    #[allow(unsafe_code)]
    let first = unsafe { Process32FirstW(handle.0, &mut entry) };
    if first == 0 { return Err(io::Error::last_os_error()); }
    let mut entries = Vec::new();
    loop {
        let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
        entries.push(Entry { pid: entry.th32ProcessID, name: OsString::from_wide(&entry.szExeFile[..len]) });
        // SAFETY: same valid handle and record as Process32FirstW.
        #[allow(unsafe_code)]
        let next = unsafe { Process32NextW(handle.0, &mut entry) };
        if next == 0 {
            let error = io::Error::last_os_error();
            if error.raw_os_error() != Some(ERROR_NO_MORE_FILES as i32) { return Err(error); }
            break;
        }
    }
    Ok(entries)
}

fn parse_tasklist(bytes: &[u8]) -> io::Result<Vec<Entry>> {
    // Only image names and numeric PIDs are needed. The remaining localized CSV
    // columns are ignored. Never interpret an error/empty output as an idle PC.
    let text = String::from_utf8_lossy(bytes);
    let mut entries = Vec::new();
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let parsed = line.trim().strip_prefix('"').and_then(|s| s.split_once("\",\""))
            .and_then(|(name, rest)| rest.split_once("\",\"").map(|(pid, _)| (name, pid)))
            .and_then(|(name, pid)| pid.parse::<u32>().ok().map(|pid| Entry { pid, name: name.into() }));
        entries.push(parsed.ok_or_else(|| io::Error::other("invalid tasklist process row"))?);
    }
    if entries.is_empty() { return Err(io::Error::other("empty tasklist process list")); }
    Ok(entries)
}

fn tasklist() -> io::Result<Vec<Entry>> {
    use std::{io::Read, process::{Command, Stdio}, time::{Duration, Instant}};
    let root = std::env::var_os("SystemRoot").ok_or_else(|| io::Error::other("SystemRoot missing"))?;
    let mut child = Command::new(PathBuf::from(root).join("System32/tasklist.exe"))
        .args(["/FO", "CSV", "/NH"]).creation_flags(0x08000000)
        .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn()?;
    let stdout = child.stdout.take().expect("piped stdout");
    // Drain while waiting so a large process list cannot fill the OS pipe.
    let reader = std::thread::spawn(move || { let mut bytes=Vec::new(); stdout.take(4*1024*1024).read_to_end(&mut bytes).map(|_| bytes) });
    let deadline = Instant::now() + Duration::from_secs(5);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            Ok(None) => { let _=child.kill(); let _=child.wait(); break Err(io::Error::new(io::ErrorKind::TimedOut,"tasklist timed out")); }
            Err(error) => { let _=child.kill(); let _=child.wait(); break Err(error); }
        }
    };
    let output = reader.join().map_err(|_| io::Error::other("tasklist reader failed"))?;
    if !status?.success() { return Err(io::Error::other("tasklist failed")); }
    parse_tasklist(&output?)
}

fn entries_with_fallback(primary: impl FnOnce() -> io::Result<Vec<Entry>>, fallback: impl FnOnce() -> io::Result<Vec<Entry>>) -> io::Result<Vec<Entry>> {
    match primary() {
        Ok(entries) if !entries.is_empty() => Ok(entries),
        result => {
            tracing::warn!(reason=?result.as_ref().err(), "Native process list unavailable; trying tasklist");
            let entries=fallback()?;
            if entries.is_empty() { return Err(io::Error::other("no process list available")); }
            Ok(entries)
        }
    }
}

fn image_path(pid: u32) -> io::Result<Option<PathBuf>> {
    // SAFETY: query-only access, no inheritance. No VM_READ or mutation rights.
    #[allow(unsafe_code)]
    let raw = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if raw.is_null() {
        let error=io::Error::last_os_error();
        if error.raw_os_error()==Some(ERROR_INVALID_PARAMETER as i32) { return Ok(None); } // exited between snapshot and query
        return Err(error);
    }
    let handle=Handle(raw);
    let mut path=vec![0u16;32768];
    let mut len=path.len() as u32;
    // SAFETY: owned live handle and writable buffer whose capacity is passed in len.
    #[allow(unsafe_code)]
    let found=unsafe { QueryFullProcessImageNameW(handle.0, 0, path.as_mut_ptr(), &mut len) };
    if found==0 {
        let error=io::Error::last_os_error();
        let mut code=0;
        // SAFETY: owned live handle and a writable local exit-code integer.
        #[allow(unsafe_code)]
        let read=unsafe { GetExitCodeProcess(handle.0,&mut code) };
        if read!=0 && code!=259 { return Ok(None); }
        return Err(error);
    }
    if len==0 || len as usize>=path.len() { return Err(io::Error::other("invalid process image path")); }
    Ok(Some(OsString::from_wide(&path[..len as usize]).into()))
}

fn normalized(path: &Path) -> String {
    path.to_string_lossy().replace('/',"\\").trim_start_matches("\\\\?\\").to_lowercase()
}

fn matches_runtime(exe: &Path, entries: &[Entry], mut query: impl FnMut(u32) -> io::Result<Option<PathBuf>>) -> io::Result<bool> {
    let target=normalized(exe);
    let canonical=std::fs::canonicalize(exe).ok().map(|p|normalized(&p));
    let name=exe.file_name().unwrap_or_default().to_string_lossy();
    let mut unreadable=None;
    for entry in entries {
        if !entry.name.to_string_lossy().eq_ignore_ascii_case(&name) { continue; }
        match query(entry.pid) {
            Ok(Some(path)) => {
                let path=normalized(&path);
                if path==target || canonical.as_ref()==Some(&path) { return Ok(true); }
            }
            Ok(None) => {}, // exited; never latch that stale snapshot
            Err(error) => { tracing::warn!(pid=entry.pid, %error,"Could not query candidate Dolphin image"); unreadable=Some(error); }
        }
    }
    if let Some(error)=unreadable { return Err(error); }
    Ok(false)
}

/// Whether this exact program file is held the way Windows holds a running
/// program's image.
///
/// The fallback for a candidate this launcher is not allowed to inspect.
/// `QueryFullProcessImageNameW` needs PROCESS_QUERY_LIMITED_INFORMATION, and a
/// process at a higher integrity level or under another account refuses it, so
/// a single unreadable process named `Dolphin.exe` used to turn the whole
/// answer into an error. That error is not neutral: the Settings page goes
/// read only on it and every write on it fails, which is the same dead end as
/// a false "close Dolphin first" (Adam's launcher log, 2026-09-13T08:05Z, pid
/// 11516, "Access is denied"). Windows maps a running image with no write
/// sharing, so asking for write access to this one file answers the question
/// that matters, about this runtime only, with no process rights at all.
/// Nothing is written: the handle is opened and dropped, and the open neither
/// creates nor truncates.
fn image_in_use(exe: &Path) -> io::Result<bool> {
    match std::fs::OpenOptions::new().write(true).open(exe) {
        Ok(_) => Ok(false),
        Err(error) if error.raw_os_error()==Some(ERROR_SHARING_VIOLATION as i32) => Ok(true),
        // A read-only file, or one this account may not write, says nothing
        // either way. The caller keeps the denial it already had.
        Err(error) => Err(error),
    }
}

/// Whether the process `pid` is still running. Used by the "Move to another
/// drive" helper to wait for the launcher it replaces to close.
pub(crate) fn pid_alive(pid: u32) -> bool {
    matches!(image_path(pid), Ok(Some(_)))
}

pub(crate) fn is_running(exe: &Path) -> io::Result<bool> {
    let entries=entries_with_fallback(snapshot,tasklist)?;
    match matches_runtime(exe,&entries,image_path) {
        Ok(found) => Ok(found),
        Err(denied) => {
            tracing::warn!(%denied,"A candidate Dolphin could not be inspected; asking the image file instead");
            image_in_use(exe).map_err(|_| denied)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn row(pid:u32,name:&str)->Entry { Entry{pid,name:name.into()} }
    fn denied<T>() -> io::Result<T> { Err(io::Error::from_raw_os_error(5)) }

    #[test]
    fn unavailable_snapshot_uses_independent_listing_and_does_not_block_idle_startup() {
        for primary in [denied(),Ok(vec![])] {
            let entries=entries_with_fallback(||primary,||parse_tasklist(b"\"System\",\"4\",\"Services\",\"0\",\"100 K\"\r\n")).unwrap();
            assert!(!matches_runtime(Path::new(r"C:\Reborn\Dolphin.exe"),&entries, |_|panic!("must not query unrelated protected processes")).unwrap());
        }
    }
    #[test]
    fn fallback_keeps_an_actual_game_protected() {
        let exe=Path::new(r"C:\Reborn\Dolphin.exe");
        let entries=entries_with_fallback(denied,||Ok(vec![row(10,"Dolphin.exe")])).unwrap();
        assert!(matches_runtime(exe,&entries,|_|Ok(Some(exe.into()))).unwrap());
    }
    #[test]
    fn missing_process_is_not_latched_and_another_install_is_not_this_runtime() {
        let exe=Path::new(r"C:\Reborn\Dolphin.exe"); let entries=vec![row(10,"Dolphin.exe")];
        assert!(!matches_runtime(exe,&entries,|_|Ok(None)).unwrap());
        assert!(!matches_runtime(exe,&entries,|_|Ok(Some(r"C:\Other\Dolphin.exe".into()))).unwrap());
        assert!(matches_runtime(exe,&entries,|_|Ok(Some(r"\\?\c:\reborn\DOLPHIN.EXE".into()))).unwrap());
        assert!(matches_runtime(exe,&entries,|_|denied()).is_err());
    }
    #[test]
    fn failed_or_malformed_fallback_is_not_treated_as_idle() {
        assert!(entries_with_fallback(denied,denied).is_err());
        for text in ["", "ERROR: Access denied", "\"Dolphin.exe\",\"not a pid\",\"Services\""] {
            assert!(parse_tasklist(text.as_bytes()).is_err());
        }
    }
    /// A process this launcher may not inspect must not cost the player their
    /// Settings page. The file the runtime runs from is evidence no privilege
    /// is needed to read.
    #[test]
    fn an_uninspectable_candidate_falls_back_to_the_image_file() {
        use std::os::windows::fs::OpenOptionsExt;
        let dir=tempfile::tempdir().unwrap();
        let exe=dir.path().join("Dolphin.exe");
        std::fs::write(&exe,b"MZ").unwrap();
        assert!(!image_in_use(&exe).unwrap(),"a file nothing holds is not a running program");
        let held=std::fs::OpenOptions::new().write(true).share_mode(0).open(&exe).unwrap();
        assert!(image_in_use(&exe).unwrap(),"a file held the way Windows holds a running image reads as in use");
        drop(held);
        assert!(!image_in_use(&exe).unwrap(),"the lock must not outlive the holder");
        assert_eq!(std::fs::read(&exe).unwrap(),b"MZ","the probe must never write to the program it asks about");
        // The real thing, both ways round: this test process is running from
        // its own image, and a denied candidate that is not this runtime
        // leaves the runtime editable.
        assert!(image_in_use(&std::env::current_exe().unwrap()).unwrap());
        assert!(matches_runtime(&exe,&[row(10,"Dolphin.exe")],|_|denied()).is_err());
        assert!(!is_running(&exe).unwrap(),"another account's Dolphin is not this one");
    }

    #[test]
    fn real_fallback_listing_and_limited_query_find_this_process() {
        let exe=std::env::current_exe().unwrap();
        assert!(matches_runtime(&exe,&tasklist().unwrap(),image_path).unwrap());
        assert!(is_running(&exe).unwrap());
    }
}
