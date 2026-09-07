//! Launcher self-update, the version that can ship without a signing key.
//!
//! `CONTENT_AND_UPDATE_MODEL.md` section 10 designs a supervisor that swaps
//! the launcher in place and rolls back on a failed ready handshake, and
//! section 6 makes Ed25519-signed manifests a requirement. Neither the
//! supervisor nor the key exists. The same document names the fallback when
//! the swap machinery is absent: the platform installer, which writes only
//! the install root and loses nothing. This module drives that fallback on
//! purpose.
//!
//! Three steps, each its own call so a page can show each:
//!
//!   1. [`check`]: fetch a small manifest and compare versions. Never an
//!      error: an unreachable manifest is a state, not a fault, because a
//!      launcher that cannot reach its update server must still play.
//!   2. [`download`]: fetch the installer to the downloads folder, hash it,
//!      and refuse a mismatch. The manifest is fetched over HTTPS and pins
//!      the SHA-256, which is the only integrity this can offer without a
//!      signing key; the same is true of a manual download from the release
//!      page, and this is that download with the hash checked.
//!   3. [`apply`]: start the installer and get out of its way. Inno replaces
//!      the program files and keeps the data root, and the installer's own
//!      locked-file handling covers a launcher that has not exited yet.
//!
//! The manifest is `latest.json`, produced by `installer/windows/build.ps1`
//! beside the installer, so the file that says what is current is written by
//! the build that made it.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::manifest::Sha256Hex;
use crate::net::Http;
use crate::{Error, Result};

pub const MANIFEST_SCHEMA: &str = "nsr.launcher-release.v1";

/// Where a launcher looks for its manifest when nothing else is set: the
/// `launcher/` folder of the content bucket, beside the installer it names.
/// Publishing a launcher update is uploading the one-file installer and this
/// manifest there (docs/RUNNING_UPDATES.md 7c).
///
/// Not a GitHub release, which is where this pointed first: the repository
/// is private, and a private repository's release assets are not fetchable
/// by a player's launcher. The bucket is public, serves `blobs/*` cached and
/// everything else uncached, and already hosts every other download.
pub const DEFAULT_SOURCE: &str = "https://files.nflstreetreborn.com/launcher/latest.json";
/// The beta channel reads its own file, so a beta never offers a stable
/// build as an update or the other way round.
pub const DEFAULT_BETA_SOURCE: &str = "https://files.nflstreetreborn.com/launcher/latest-beta.json";

/// One downloadable installer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Asset {
    pub url: String,
    pub sha256: Sha256Hex,
    pub bytes: u64,
    pub file_name: String,
}

/// What a release publishes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseManifest {
    pub schema: String,
    pub version: String,
    #[serde(default)]
    pub channel: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub windows: Option<Asset>,
    #[serde(default)]
    pub macos: Option<Asset>,
    #[serde(default)]
    pub linux: Option<Asset>,
}

impl ReleaseManifest {
    /// The asset for the platform this binary was built for.
    pub fn asset_for_this_platform(&self) -> Option<&Asset> {
        if cfg!(windows) {
            self.windows.as_ref()
        } else if cfg!(target_os = "macos") {
            self.macos.as_ref()
        } else {
            self.linux.as_ref()
        }
    }
}

/// The outcome of a check. Every variant is a state a page shows; none stops
/// the launcher.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateCheck {
    UpToDate { current: String, latest: String },
    Available { current: String, latest: String, asset: Asset, notes: Option<String> },
    /// A newer release exists and has nothing for this platform.
    NoAssetForThisPlatform { current: String, latest: String },
    /// The manifest could not be fetched. Offline, or the source is wrong.
    Unreachable { source: String, why: String },
    /// The manifest was fetched and is not one this launcher reads.
    Unreadable { source: String, why: String },
}

impl UpdateCheck {
    pub fn explain(&self) -> String {
        match self {
            UpdateCheck::UpToDate { current, .. } => format!("Up to date at {current}."),
            UpdateCheck::Available { current, latest, asset, .. } => format!(
                "Version {latest} is available; this is {current}. {:.1} MB to download.",
                asset.bytes as f64 / 1_048_576.0
            ),
            UpdateCheck::NoAssetForThisPlatform { current, latest } => format!(
                "Version {latest} is published, but not for this platform yet. This is {current}."
            ),
            UpdateCheck::Unreachable { source, why } => {
                format!("Could not reach the update source {source}: {why}")
            }
            UpdateCheck::Unreadable { source, why } => {
                format!("The update source {source} did not answer with a release manifest: {why}")
            }
        }
    }
}

/// Fetch the manifest and compare it with the running version.
pub fn check(http: &dyn Http, source: &str, current: &str) -> UpdateCheck {
    let reply = match http.get_text(source) {
        Ok(reply) => reply,
        Err(e) => {
            return UpdateCheck::Unreachable { source: source.to_string(), why: e.to_string() }
        }
    };
    if !reply.is_success() {
        return UpdateCheck::Unreachable {
            source: source.to_string(),
            why: format!("it answered {}", reply.status),
        };
    }
    let manifest: ReleaseManifest = match serde_json::from_str(&reply.body) {
        Ok(m) => m,
        Err(e) => {
            return UpdateCheck::Unreadable { source: source.to_string(), why: e.to_string() }
        }
    };
    if manifest.schema != MANIFEST_SCHEMA {
        return UpdateCheck::Unreadable {
            source: source.to_string(),
            why: format!("schema is {:?}, this launcher reads {MANIFEST_SCHEMA}", manifest.schema),
        };
    }
    if !newer(&manifest.version, current) {
        return UpdateCheck::UpToDate { current: current.to_string(), latest: manifest.version };
    }
    match manifest.asset_for_this_platform() {
        Some(asset) => UpdateCheck::Available {
            current: current.to_string(),
            latest: manifest.version.clone(),
            asset: asset.clone(),
            notes: manifest.notes.clone(),
        },
        None => UpdateCheck::NoAssetForThisPlatform {
            current: current.to_string(),
            latest: manifest.version,
        },
    }
}

/// Whether `candidate` is a newer version than `current`.
///
/// Dotted numbers compared numerically, then a pre-release tag: `3.0.0-beta.1`
/// is older than `3.0.0` and newer than `2.9.9`. Anything unparseable never
/// counts as newer, so a garbled manifest cannot offer a downgrade.
pub fn newer(candidate: &str, current: &str) -> bool {
    fn parse(v: &str) -> Option<(Vec<u64>, Option<String>)> {
        let v = v.trim().trim_start_matches('v');
        let (numbers, pre) = match v.split_once('-') {
            Some((n, p)) => (n, Some(p.to_string())),
            None => (v, None),
        };
        let parts: Option<Vec<u64>> = numbers.split('.').map(|p| p.parse().ok()).collect();
        let parts = parts?;
        if parts.is_empty() {
            return None;
        }
        Some((parts, pre))
    }
    let (Some((a, a_pre)), Some((b, b_pre))) = (parse(candidate), parse(current)) else {
        return false;
    };
    let width = a.len().max(b.len());
    let pad = |v: &[u64]| -> Vec<u64> {
        let mut out = v.to_vec();
        out.resize(width, 0);
        out
    };
    let (a_n, b_n) = (pad(&a), pad(&b));
    if a_n != b_n {
        return a_n > b_n;
    }
    // Same numbers: a release beats a pre-release; two pre-releases compare as text.
    match (a_pre, b_pre) {
        (None, Some(_)) => true,
        (Some(_), None) => false,
        (None, None) => false,
        (Some(x), Some(y)) => x > y,
    }
}

/// What a download left on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Downloaded {
    pub path: PathBuf,
    pub bytes: u64,
    pub sha256: Sha256Hex,
}

/// Fetch the installer into `dir`, verify it against the manifest, and keep
/// it only if it matches.
///
/// Written to a `.partial` name first and renamed after the hash checks out,
/// so a file with the final name is always a verified one. A mismatch removes
/// the download and is an error that names both hashes.
pub fn download(http: &dyn Http, asset: &Asset, dir: &Path) -> Result<Downloaded> {
    std::fs::create_dir_all(dir).map_err(|source| Error::Io { path: dir.to_path_buf(), source })?;
    let name = safe_file_name(&asset.file_name);
    let final_path = dir.join(&name);
    let partial = dir.join(format!("{name}.partial"));

    let bytes = http.get_to_file(&asset.url, &partial)?;
    let data = std::fs::read(&partial).map_err(|source| Error::Io { path: partial.clone(), source })?;
    let actual = Sha256Hex::from_bytes(&data);
    if actual != asset.sha256 {
        let _ = std::fs::remove_file(&partial);
        return Err(Error::PlainIo(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!(
                "the download did not match its published hash, so it was deleted. Expected {}, got {}. \
                 Nothing was installed.",
                asset.sha256.as_str(),
                actual.as_str()
            ),
        )));
    }
    if asset.bytes != 0 && bytes != asset.bytes {
        let _ = std::fs::remove_file(&partial);
        return Err(Error::PlainIo(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("the download is {bytes} bytes, the manifest says {}. Deleted.", asset.bytes),
        )));
    }
    std::fs::rename(&partial, &final_path).map_err(|source| Error::Io { path: final_path.clone(), source })?;
    Ok(Downloaded { path: final_path, bytes, sha256: actual })
}

/// A file name with any path taken off, so a manifest cannot name a file
/// outside the downloads folder.
fn safe_file_name(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name).trim();
    if base.is_empty() || base == "." || base == ".." {
        "launcher-setup.exe".to_string()
    } else {
        base.to_string()
    }
}

/// Start the installer. The caller exits afterwards; Inno does the rest.
///
/// `/SILENT` shows progress and asks nothing; `/NORESTART` keeps the machine
/// up. The installer's own upgrade rules apply: same AppId, program files
/// only, the data root untouched.
pub fn apply(platform: &crate::Platform, installer: &Path) -> Result<()> {
    if !installer.is_file() {
        return Err(Error::PlainIo(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("no installer at {}", installer.display()),
        )));
    }
    // /RELAUNCH=1 is ours, read by the installer's [Run] check. A silent Inno
    // upgrade skips the "Launch" entry (skipifsilent), so without this the
    // launcher exits for the update and never comes back, and the player is
    // left looking at nothing. A plain first install does not pass it and does
    // not relaunch, which keeps the sandbox smoke test's single launch single.
    let mut args: Vec<std::ffi::OsString> =
        vec!["/SILENT".into(), "/NORESTART".into(), "/RELAUNCH=1".into()];
    // The install root, stated. The installer sets UsePreviousAppDir=no,
    // which is what keeps {app} off the 2.1 data folder, and it also means
    // a silent upgrade lands in the DEFAULT folder rather than the one this
    // launcher runs from. A player who chose D:\Games would get a second
    // copy there and a relaunch of the wrong one.
    if let Ok(root) = platform.paths.install_root() {
        args.push(format!("/DIR={}", root.display()).into());
    }
    let env = std::collections::BTreeMap::new();
    platform.processes.spawn(installer, &args, installer.parent(), &env)?;
    Ok(())
}

/// The switches [`apply`] passes, for the installer smoke test to mirror.
pub const APPLY_SWITCHES: [&str; 3] = ["/SILENT", "/NORESTART", "/RELAUNCH=1"];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::fake::FakeHttp;

    fn manifest(version: &str, sha: &str) -> String {
        format!(
            r#"{{"schema":"{MANIFEST_SCHEMA}","version":"{version}","notes":"fixes","windows":{{"url":"https://x/setup.exe","sha256":"{sha}","bytes":5,"file_name":"Setup {version}.exe"}},"macos":null,"linux":null}}"#
        )
    }

    #[test]
    fn version_order_is_numeric_and_a_release_beats_its_own_prerelease() {
        assert!(newer("3.0.1", "3.0.0"));
        assert!(newer("3.10.0", "3.9.9"), "10 is more than 9, not less as text");
        assert!(newer("3.0.0", "3.0.0-beta.1"));
        assert!(newer("3.0.0-beta.2", "3.0.0-beta.1"));
        assert!(!newer("3.0.0-beta.1", "3.0.0"));
        assert!(!newer("3.0.0", "3.0.0"));
        assert!(!newer("2.9.9", "3.0.0"));
        assert!(newer("v3.1", "3.0.7"), "a v prefix and a short form still compare");
        assert!(!newer("garbage", "3.0.0"), "unparseable is never an update");
    }

    #[test]
    fn check_reports_every_state_and_never_errors() {
        let sha = Sha256Hex::from_bytes(b"hello");
        let http = FakeHttp::default()
            .reply("https://s/newer.json", 200, manifest("3.1.0", sha.as_str()))
            .reply("https://s/same.json", 200, manifest("3.0.0", sha.as_str()))
            .reply("https://s/garbage.json", 200, "not json")
            .reply("https://s/wrong-schema.json", 200, r#"{"schema":"other","version":"9.9.9"}"#)
            .reply("https://s/missing.json", 404, "");

        let got = check(&http, "https://s/newer.json", "3.0.0");
        assert!(matches!(&got, UpdateCheck::Available { latest, .. } if latest == "3.1.0"), "{got:?}");
        assert!(got.explain().contains("3.1.0 is available"), "{}", got.explain());

        assert!(matches!(check(&http, "https://s/same.json", "3.0.0"), UpdateCheck::UpToDate { .. }));
        assert!(matches!(check(&http, "https://s/garbage.json", "3.0.0"), UpdateCheck::Unreadable { .. }));
        assert!(matches!(check(&http, "https://s/wrong-schema.json", "3.0.0"), UpdateCheck::Unreadable { .. }));
        assert!(matches!(check(&http, "https://s/missing.json", "3.0.0"), UpdateCheck::Unreachable { .. }));
        // No route at all: offline is a state, not a fault.
        let offline = check(&http, "https://s/nowhere.json", "3.0.0");
        assert!(matches!(offline, UpdateCheck::Unreachable { .. }));
        assert!(offline.explain().starts_with("Could not reach"));
    }

    #[test]
    fn a_download_is_kept_only_when_it_matches_the_published_hash() {
        let dir = tempfile::tempdir().unwrap();
        let good = Sha256Hex::from_bytes(b"hello");
        let http = FakeHttp::default().reply("https://x/setup.exe", 200, b"hello");

        let asset = Asset { url: "https://x/setup.exe".into(), sha256: good.clone(), bytes: 5, file_name: "../evil/Setup.exe".into() };
        let got = download(&http, &asset, dir.path()).unwrap();
        assert_eq!(got.path, dir.path().join("Setup.exe"), "the path in the name is stripped");
        assert_eq!(got.bytes, 5);
        assert!(got.path.is_file());
        assert!(!dir.path().join("Setup.exe.partial").exists());

        let wrong = Asset { sha256: Sha256Hex::from_bytes(b"other"), file_name: "Bad.exe".into(), ..asset.clone() };
        let err = download(&http, &wrong, dir.path()).unwrap_err().to_string();
        assert!(err.contains("did not match"), "{err}");
        assert!(!dir.path().join("Bad.exe").exists(), "a mismatch leaves nothing behind");
        assert!(!dir.path().join("Bad.exe.partial").exists());
    }
}
