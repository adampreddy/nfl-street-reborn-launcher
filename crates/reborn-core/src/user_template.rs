//! The starting point of every new Dolphin user folder: the reference
//! folder Adam tuned, reduced to "the real basic stuff" and cleaned of
//! anything personal or machine-bound.
//!
//! Staged into `<runtime>\User` by the build (`rebornctl runtime-template`,
//! run by `installer/windows/build.ps1 -UserTemplate`); seeded into each
//! new user folder by `runtime::seed_user_from_template`. Only these ship:
//!
//! - `Config\*.ini`: the tuned emulator, pads, hotkeys, logger and window
//!   settings. `Dolphin.ini`, `GFX.ini` and `Qt.ini` are sanitized.
//! - `GameSettings\*.ini`: the game's Gecko codes and the settings pinned
//!   per game (resolution, MSAA, EFB access).
//! - `Cache\*.uidcache` and `Cache\Shaders\*.cache`: the pipeline and
//!   shader lists a tuned install has already met, so a first boot compiles
//!   them up front instead of stuttering through them.
//!
//! Never: `Load` (texture packs are content), `GC` (a memory card is a
//! player's), `Dump`, `Logs`, `GameCovers`, `gamelist.cache`, `Styles`, `Wii`.

use std::path::{Path, PathBuf};

use crate::platform::{Error, Result};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct StageReport {
    pub written: Vec<PathBuf>,
    pub skipped: Vec<PathBuf>,
}

/// Directory (forward slashes, relative to `User`) and the extension of
/// the files taken from it.
const RULES: &[(&str, &str)] = &[
    ("Config", "ini"),
    ("GameSettings", "ini"),
    ("Cache", "uidcache"),
    ("Cache/Shaders", "cache"),
];

/// Stage `from_user` into `into_user`.
pub fn stage(from_user: &Path, into_user: &Path) -> Result<StageReport> {
    let mut report = StageReport::default();
    for (dir, ext) in RULES {
        let mut src_dir = from_user.to_path_buf();
        for part in dir.split('/') {
            src_dir.push(part);
        }
        let Ok(entries) = std::fs::read_dir(&src_dir) else { continue };
        for entry in entries.flatten() {
            let src = entry.path();
            if !src.is_file() || !src.extension().is_some_and(|e| e.eq_ignore_ascii_case(ext)) {
                report.skipped.push(src);
                continue;
            }
            let mut dst = into_user.to_path_buf();
            for part in dir.split('/') {
                dst.push(part);
            }
            dst.push(entry.file_name());
            if let Some(parent) = dst.parent() {
                std::fs::create_dir_all(parent).map_err(|source| Error::Io { path: parent.to_path_buf(), source })?;
            }
            let name = entry.file_name().to_string_lossy().to_lowercase();
            let sanitizer: Option<fn(&str) -> String> = match (*dir, name.as_str()) {
                ("Config", "dolphin.ini") => Some(sanitize_dolphin_ini),
                ("Config", "gfx.ini") => Some(sanitize_gfx_ini),
                ("Config", "qt.ini") => Some(sanitize_qt_ini),
                _ => None,
            };
            match sanitizer {
                Some(f) => {
                    let text = std::fs::read_to_string(&src).map_err(|source| Error::Io { path: src.clone(), source })?;
                    std::fs::write(&dst, f(&text)).map_err(|source| Error::Io { path: dst.clone(), source })?;
                }
                None => {
                    std::fs::copy(&src, &dst).map_err(|source| Error::Io { path: dst.clone(), source })?;
                }
            }
            report.written.push(dst);
        }
    }
    Ok(report)
}

/// Dolphin.ini without the analytics identity, the netplay identity, the
/// disc keys and every path key (the path law), with slot A a memory card
/// at Dolphin's default place and analytics off.
pub fn sanitize_dolphin_ini(text: &str) -> String {
    let mut doc = crate::ini::IniDocument::parse(text);
    for (s, k) in [
        ("Analytics", "ID"),
        ("NetPlay", "HostCode"),
        ("NetPlay", "Nickname"),
        ("NetPlay", "IndexName"),
        ("NetPlay", "IndexPassword"),
    ] {
        doc.remove(s, k);
    }
    for (s, k) in crate::dolphin_preferences::path_law_removals(&doc, Path::new("")) {
        doc.remove(&s, &k);
    }
    let mut set = crate::ini::Overlay::new();
    set.entry("Core".into()).or_default().insert("SlotA".into(), "1".into());
    set.entry("Core".into()).or_default().insert("SlotB".into(), "255".into());
    set.entry("Analytics".into()).or_default().insert("Enabled".into(), "False".into());
    set.entry("Analytics".into()).or_default().insert("PermissionAsked".into(), "True".into());
    doc.merge(&set);
    doc.render()
}

/// GFX.ini with texture dumping and the FPS counter off. Everything else,
/// the anti-aliasing and filtering included, is the tuned value.
pub fn sanitize_gfx_ini(text: &str) -> String {
    let mut doc = crate::ini::IniDocument::parse(text);
    let mut off = crate::ini::Overlay::new();
    for k in ["DumpTextures", "DumpBaseTextures", "DumpMipTextures", "ShowFPS"] {
        off.entry("Settings".into()).or_default().insert(k.into(), "False".into());
    }
    doc.merge(&off);
    doc.render()
}

/// Qt.ini without the last-used folder or any other absolute path.
pub fn sanitize_qt_ini(text: &str) -> String {
    text.lines()
        .filter(|l| {
            let t = l.trim_start().to_lowercase();
            !(t.starts_with("lastdir") || t.contains(":/") || t.contains(":\\"))
        })
        .map(|l| format!("{l}\n"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn put(root: &Path, rel: &str, text: &str) {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }

    #[test]
    fn only_the_basic_files_are_staged() {
        let dir = tempfile::tempdir().unwrap();
        let from = dir.path().join("User");
        put(&from, "Config/Dolphin.ini", "[Core]\nSlotA = 255\n");
        put(&from, "Config/GFX.ini", "[Settings]\nShowFPS = True\n");
        put(&from, "Config/Qt.ini", "[General]\nlastdir=C:/x\n");
        put(&from, "Config/GCPadNew.ini", "[GCPad1]\nDevice = XInput/0/Gamepad\n");
        put(&from, "Config/Dolphin.ini.launcher-original", "old");
        put(&from, "GameSettings/GN7E69.ini", "[Gecko]\n$Point Remover\n");
        put(&from, "Cache/GN7E69.uidcache", "uid");
        put(&from, "Cache/Shaders/D3D-gs-1.cache", "gs");
        put(&from, "Cache/gamelist.cache", "no");
        put(&from, "Cache/GameCovers/a.png", "no");
        put(&from, "GC/MemoryCardA.USA.raw", "no");
        put(&from, "Load/Textures/CR4E12/a.dds", "no");
        put(&from, "Logs/dolphin.log", "no");
        let into = dir.path().join("runtime").join("User");
        let report = stage(&from, &into).unwrap();
        let mut got: Vec<String> = report
            .written
            .iter()
            .map(|p| p.strip_prefix(&into).unwrap().to_string_lossy().replace('\\', "/"))
            .collect();
        got.sort();
        assert_eq!(
            got,
            [
                "Cache/GN7E69.uidcache",
                "Cache/Shaders/D3D-gs-1.cache",
                "Config/Dolphin.ini",
                "Config/GCPadNew.ini",
                "Config/GFX.ini",
                "Config/Qt.ini",
                "GameSettings/GN7E69.ini"
            ]
        );
        assert!(!into.join("GC").exists() && !into.join("Load").exists() && !into.join("Logs").exists());
        assert!(!into.join("Config/Dolphin.ini.launcher-original").exists());
        assert_eq!(std::fs::read_to_string(into.join("Cache/Shaders/D3D-gs-1.cache")).unwrap(), "gs");
    }

    #[test]
    fn dolphin_ini_is_sanitized() {
        let text = "[Analytics]\nID = de6680a75ec16a1d97fb464825c34211\nEnabled = False\n[General]\nISOPaths = 1\nLoadPath = User/Load///\nGDBPort = -1\nISOPath0 = Games\n[Core]\nSlotA = 255\nSlotB = 255\nDefaultISO = C:\\x\\NSRV3.iso\n[NetPlay]\nHostCode = 1db46c2c\nNickname = YourName\nTraversalPort = 0x1876\n";
        let out = sanitize_dolphin_ini(text);
        let doc = crate::ini::IniDocument::parse(&out);
        assert!(doc.get("Analytics", "ID").is_none());
        assert_eq!(doc.get("Analytics", "Enabled"), Some("False"));
        for (s, k) in [
            ("General", "ISOPaths"),
            ("General", "ISOPath0"),
            ("General", "LoadPath"),
            ("Core", "DefaultISO"),
            ("NetPlay", "HostCode"),
            ("NetPlay", "Nickname"),
        ] {
            assert!(doc.get(s, k).is_none(), "{s}/{k} should be gone");
        }
        assert_eq!(doc.get("General", "GDBPort"), Some("-1"));
        assert_eq!(doc.get("NetPlay", "TraversalPort"), Some("0x1876"));
        assert_eq!(doc.get("Core", "SlotA"), Some("1"));
        assert_eq!(doc.get("Core", "SlotB"), Some("255"));
    }

    #[test]
    fn gfx_and_qt_are_sanitized() {
        let gfx = sanitize_gfx_ini("[Settings]\nDumpTextures = True\nDumpBaseTextures = True\nDumpMipTextures = False\nShowFPS = True\nMSAA = 0x00000004\n");
        let doc = crate::ini::IniDocument::parse(&gfx);
        for k in ["DumpTextures", "DumpBaseTextures", "DumpMipTextures", "ShowFPS"] {
            assert_eq!(doc.get("Settings", k), Some("False"), "{k}");
        }
        assert_eq!(doc.get("Settings", "MSAA"), Some("0x00000004"));
        let qt = sanitize_qt_ini("[General]\nlastdir=C:/Users/Adam/AppData/Roaming/NFL Street Reborn/Games\nfoo=1\n");
        assert!(!qt.contains("lastdir") && qt.contains("foo=1"));
    }

    /// The real reference folder, when this runs on Adam's PC.
    #[test]
    fn staging_the_reference_folder_on_this_machine_when_it_exists() {
        let from = Path::new(r"C:\Users\Adam\nflstreet2_clean\DOLPHIN ONLINE TEST\NFL Street Reborn Dolphin\User");
        if !from.is_dir() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let into = dir.path().join("User");
        let report = stage(from, &into).unwrap();
        assert!(report.written.len() >= 10, "{}", report.written.len());
        let text = std::fs::read_to_string(into.join("Config/Dolphin.ini")).unwrap();
        let doc = crate::ini::IniDocument::parse(&text);
        assert!(doc.get("Analytics", "ID").is_none(), "{text}");
        assert!(doc.get("Core", "DefaultISO").is_none() && doc.get("General", "LoadPath").is_none(), "{text}");
        assert!(!text.contains("C:\\") && !text.contains("C:/"), "an absolute path survived: {text}");
        assert!(into.join("GameSettings/GN7E69.ini").is_file());
        assert!(!into.join("Load").exists() && !into.join("GC").exists());
    }
}
