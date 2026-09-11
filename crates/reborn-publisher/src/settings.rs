//! The publisher's own settings: paths and endpoints it needs to find things
//! again, and the last inputs a person typed per operation.
//!
//! Settings live at `<data>/publisher/settings.json`, one JSON object, read
//! whole and rewritten whole. A secret never belongs here. The signing key
//! is a path (this file records where, not what), and the R2 credentials
//! and the site token live in `reborn_core::publish::vault`, backed by the
//! platform credential store, instead. A settings file that happened to
//! hold a real secret would end up wherever this file ends up: a bug
//! report, a screenshot, a zipped "send me your Publisher folder", none of
//! which the vault is ever exposed to.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use reborn_core::Platform;

/// Where this app's settings live, under the platform's data root.
pub fn settings_path(platform: &Platform) -> Result<PathBuf, String> {
    let root = platform.paths.data_root().map_err(|e| e.to_string())?;
    Ok(root.join("publisher").join("settings.json"))
}

fn default_origin() -> String {
    r"C:\Reborn Origin".to_string()
}

fn default_public_base() -> String {
    reborn_core::origin::DEFAULT_ORIGIN.to_string()
}

fn default_launcher_manifest() -> String {
    reborn_core::update::DEFAULT_SOURCE.to_string()
}

/// The whole settings file. Every field a page needs to find something
/// again: a tool, a bucket, the origin on this machine. Nothing here is a
/// secret; see the module doc.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default = "default_origin")]
    pub origin: String,
    /// The `publisher.key` file `release` signs with. A path, never the
    /// key's bytes.
    #[serde(default)]
    pub publisher_key: Option<String>,
    /// `reborn-builder-cli.exe`.
    #[serde(default)]
    pub builder: Option<String>,
    /// `xdelta3`, for the game patch chain.
    #[serde(default)]
    pub xdelta: Option<String>,
    #[serde(default)]
    pub bucket_endpoint: Option<String>,
    #[serde(default)]
    pub bucket_name: Option<String>,
    #[serde(default = "default_public_base")]
    pub public_base: String,
    #[serde(default = "default_launcher_manifest")]
    pub launcher_manifest: String,
    /// The last inputs a person gave each operation, keyed by an operation
    /// name a page chooses (for example `"roster"` or `"launcher"`), so a
    /// page opened a week later starts from what was typed last instead of
    /// blank fields. Whatever a page stores here is shown back to the same
    /// person on the same machine, so this is exactly as sensitive as the
    /// rest of this file: never a secret.
    #[serde(default)]
    pub remembered: BTreeMap<String, serde_json::Value>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            origin: default_origin(),
            publisher_key: None,
            builder: None,
            xdelta: None,
            bucket_endpoint: None,
            bucket_name: None,
            public_base: default_public_base(),
            launcher_manifest: default_launcher_manifest(),
            remembered: BTreeMap::new(),
        }
    }
}

impl Settings {
    /// Read `settings.json`. An absent file is the default settings, not an
    /// error: the first run of a fresh install has never written one.
    pub fn load(path: &Path) -> Result<Self, String> {
        match std::fs::read_to_string(path) {
            Ok(text) => serde_json::from_str(&text)
                .map_err(|e| format!("{} did not parse as settings: {e}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Settings::default()),
            Err(e) => Err(format!("could not read {}: {e}", path.display())),
        }
    }

    /// Write the whole file, creating `publisher/` if it is absent.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("could not create {}: {e}", parent.display()))?;
        }
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(path, text).map_err(|e| format!("could not write {}: {e}", path.display()))
    }
}

/// The settings this machine holds, for a page to draw.
#[tauri::command(rename_all = "snake_case")]
pub fn publisher_settings() -> Result<Settings, String> {
    let platform = Platform::detect();
    let path = settings_path(&platform)?;
    Settings::load(&path)
}

/// Set one field of settings and save the whole file back.
///
/// One command for every field, rather than one command per field: a
/// settings page with eight text boxes and a remembered-inputs map should
/// not need nine command names to stay in sync with this struct. `key` is
/// one of `origin`, `publisher_key`, `builder`, `xdelta`, `bucket_endpoint`,
/// `bucket_name`, `public_base`, `launcher_manifest`, or
/// `remembered.<operation>` to set one entry of the remembered-inputs map.
/// An unknown key is refused, naming the ones this understands, rather than
/// silently doing nothing: a settings write that appears to succeed and
/// changes nothing is worse than one that says why.
#[tauri::command(rename_all = "snake_case")]
pub fn publisher_set_setting(key: String, value: serde_json::Value) -> Result<Settings, String> {
    let platform = Platform::detect();
    let path = settings_path(&platform)?;
    let mut settings = Settings::load(&path)?;

    fn as_string(v: &serde_json::Value) -> Option<String> {
        v.as_str().map(str::to_string)
    }
    fn as_opt_string(v: &serde_json::Value) -> Option<Option<String>> {
        if v.is_null() {
            Some(None)
        } else {
            as_string(v).map(Some)
        }
    }

    if let Some(operation) = key.strip_prefix("remembered.") {
        if operation.is_empty() {
            return Err(
                "remembered.<operation> needs an operation name after the dot.".to_string(),
            );
        }
        settings.remembered.insert(operation.to_string(), value);
    } else {
        match key.as_str() {
            "origin" => settings.origin = as_string(&value).ok_or("origin must be a string")?,
            "publisher_key" => {
                settings.publisher_key =
                    as_opt_string(&value).ok_or("publisher_key must be a string or null")?
            }
            "builder" => {
                settings.builder = as_opt_string(&value).ok_or("builder must be a string or null")?
            }
            "xdelta" => {
                settings.xdelta = as_opt_string(&value).ok_or("xdelta must be a string or null")?
            }
            "bucket_endpoint" => {
                settings.bucket_endpoint =
                    as_opt_string(&value).ok_or("bucket_endpoint must be a string or null")?
            }
            "bucket_name" => {
                settings.bucket_name =
                    as_opt_string(&value).ok_or("bucket_name must be a string or null")?
            }
            "public_base" => {
                settings.public_base = as_string(&value).ok_or("public_base must be a string")?
            }
            "launcher_manifest" => {
                settings.launcher_manifest =
                    as_string(&value).ok_or("launcher_manifest must be a string")?
            }
            other => {
                return Err(format!(
                    "{other:?} is not a setting this app knows. Use one of origin, \
                     publisher_key, builder, xdelta, bucket_endpoint, bucket_name, \
                     public_base, launcher_manifest, or remembered.<operation>."
                ));
            }
        }
    }

    settings.save(&path)?;
    Ok(settings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_documented_values() {
        let s = Settings::default();
        assert_eq!(s.origin, r"C:\Reborn Origin");
        assert_eq!(s.public_base, "https://files.nflstreetreborn.com/");
        assert!(s.launcher_manifest.ends_with("/launcher/latest.json"));
        assert!(s.publisher_key.is_none());
        assert!(s.remembered.is_empty());
    }

    #[test]
    fn a_missing_settings_file_loads_as_default_rather_than_an_error() {
        let dir = std::env::temp_dir().join(format!(
            "reborn-publisher-settings-test-{}-{}",
            std::process::id(),
            reborn_core::publish::journal::now().replace([':', '+'], "-")
        ));
        let path = dir.join("settings.json");
        let loaded = Settings::load(&path).expect("an absent file is not an error");
        assert_eq!(loaded.origin, Settings::default().origin);
    }

    #[test]
    fn save_then_load_round_trips_every_field() {
        let dir = std::env::temp_dir().join(format!(
            "reborn-publisher-settings-roundtrip-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        let mut s = Settings::default();
        s.origin = r"D:\Origin".to_string();
        s.publisher_key = Some(r"D:\keys\publisher.key".to_string());
        s.remembered
            .insert("roster".to_string(), serde_json::json!({"version": "2026.09.09"}));
        s.save(&path).unwrap();

        let loaded = Settings::load(&path).unwrap();
        assert_eq!(loaded.origin, s.origin);
        assert_eq!(loaded.publisher_key, s.publisher_key);
        assert_eq!(loaded.remembered.get("roster"), s.remembered.get("roster"));

        std::fs::remove_dir_all(&dir).ok();
    }
}
