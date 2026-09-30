//! Which window material the OS can give the launcher. Pure: no Tauri types,
//! so the choice is testable on any machine.

/// Set from docs/superpowers/plans/2026-09-30-mica-spike-result.md.
/// false means Windows 10 uses the solid dark fallback.
pub const WIN10_ACRYLIC: bool = false;

/// First Windows 11 build.
const WIN11_FIRST_BUILD: u32 = 22000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Material { Mica, Acrylic, Vibrancy }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os { Windows, MacOs, Linux }

pub fn current_os() -> Os {
    if cfg!(target_os = "windows") { Os::Windows } else if cfg!(target_os = "macos") { Os::MacOs } else { Os::Linux }
}

/// The material this OS can show, or None for the solid dark fallback.
pub fn pick_material(os: Os, windows_build: Option<u32>) -> Option<Material> {
    match os {
        Os::MacOs => Some(Material::Vibrancy),
        Os::Linux => None,
        Os::Windows => match windows_build {
            Some(b) if b >= WIN11_FIRST_BUILD => Some(Material::Mica),
            Some(_) if WIN10_ACRYLIC => Some(Material::Acrylic),
            _ => None,
        },
    }
}

/// `reg query ... /v CurrentBuildNumber` prints `CurrentBuildNumber REG_SZ 26100`.
pub fn parse_windows_build(reg_query_stdout: &str) -> Option<u32> {
    reg_query_stdout
        .lines()
        .find(|l| l.trim_start().starts_with("CurrentBuildNumber"))
        .and_then(|l| l.split_whitespace().last())
        .and_then(|n| n.parse().ok())
}

/// The running Windows build, or None (not Windows, or unreadable).
pub fn windows_build() -> Option<u32> {
    if current_os() != Os::Windows {
        return None;
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let out = std::process::Command::new("reg")
            .args(["query", r"HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion", "/v", "CurrentBuildNumber"])
            .creation_flags(0x0800_0000)
            .output()
            .ok()?;
        parse_windows_build(&String::from_utf8_lossy(&out.stdout))
    }
    #[cfg(not(windows))]
    {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_11_gets_mica() {
        assert_eq!(pick_material(Os::Windows, Some(26100)), Some(Material::Mica));
        assert_eq!(pick_material(Os::Windows, Some(22000)), Some(Material::Mica));
    }

    #[test]
    fn windows_10_follows_the_spike() {
        let expected = if WIN10_ACRYLIC { Some(Material::Acrylic) } else { None };
        assert_eq!(pick_material(Os::Windows, Some(19045)), expected);
    }

    #[test]
    fn an_unknown_windows_build_gets_no_glass() {
        // Never a transparent window with nothing behind it.
        assert_eq!(pick_material(Os::Windows, None), None);
    }

    #[test]
    fn mac_gets_vibrancy_and_linux_gets_nothing() {
        assert_eq!(pick_material(Os::MacOs, None), Some(Material::Vibrancy));
        assert_eq!(pick_material(Os::Linux, None), None);
    }

    #[test]
    fn the_build_number_is_read_from_reg_query_output() {
        let out = "\r\nHKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\r\n    CurrentBuildNumber    REG_SZ    26100\r\n\r\n";
        assert_eq!(parse_windows_build(out), Some(26100));
        assert_eq!(parse_windows_build("garbage"), None);
        assert_eq!(parse_windows_build("    CurrentBuildNumber    REG_SZ    abc"), None);
    }
}
