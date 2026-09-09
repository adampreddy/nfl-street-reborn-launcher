//! The Keys page: the signing keys and the vault.
//!
//! `publisher.key` is the only key that can sign a release; every launcher
//! in the field carries `reborn_core::signing::TRUSTED_KEYS` and refuses
//! anything not signed by one of them. This page shows which public keys a
//! shipped launcher trusts, and whether the key file this machine has on
//! disk is actually one of them: a key that generates and signs fine but
//! is not in `TRUSTED_KEYS` produces releases every launcher rejects, which
//! is the single most expensive mistake this page can catch before a
//! release ships instead of after.
//!
//! The vault holds three things that must never come back into this page
//! once stored: the league site's API token, and the two halves of the R2
//! write credentials. `vault_store` reports only whether the store
//! succeeded, never the value, and nothing in this file formats one of
//! these secrets into a string for any reason, including a diagnostic one.
//! An import from `rclone.conf` follows the same rule: the file is read,
//! its two fields are pulled out and stored, and the parsed values are
//! dropped without ever appearing in a returned `Value`, a log line, or an
//! error message.

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use reborn_core::publish::keys::{keygen, read_key};
use reborn_core::publish::vault::{
    forget_r2_access, forget_r2_secret, forget_site_token, r2_access, r2_secret,
    site_token, store_r2_access, store_r2_secret, store_site_token,
};
use reborn_core::publish::{Outcome, ROTATION_KEY};
use reborn_core::signing::{encode_public_key, publisher as signing_publisher, TRUSTED_KEYS};
use reborn_core::Platform;

use crate::settings::{settings_path, Settings};

/// Where Adam keeps the R2 credentials `rclone` already uses, so this app
/// can offer to import them once instead of asking him to retype an access
/// key by hand. Never written to; only ever read, and read at most until
/// the one-time offer has been shown.
const RCLONE_CONF_PATH: &str = r"C:\Users\Adam\Reborn Keys\rclone.conf";

/// The `remembered` key that records the one-time R2 import offer has
/// already been shown, so it does not come back on every launch once Adam
/// has seen it, whether or not he used it.
const RCLONE_OFFER_SEEN_KEY: &str = "keys.rclone_offer_seen";

/// The everyday key's path, and the folder it and `rotation.key` (if
/// present) sit in, derived from settings. `None` when no key has been
/// configured yet: a fresh install, or one where Adam has not pointed the
/// app at a key or generated one.
fn key_paths(settings: &Settings) -> (Option<PathBuf>, Option<PathBuf>) {
    let publisher_path = settings.publisher_key.as_ref().map(PathBuf::from);
    let dir = publisher_path.as_ref().and_then(|p| p.parent()).map(Path::to_path_buf);
    (publisher_path, dir)
}

/// Which role, if any, a public key plays in the shipped launcher's trust
/// list. `None` means a launcher in the field will reject anything this
/// key signs.
fn trust_role(public: &[u8; 32]) -> Option<&'static str> {
    TRUSTED_KEYS.iter().position(|k| k == public).map(|i| match i {
        0 => "everyday",
        1 => "rotation",
        _ => "trusted",
    })
}

/// Whether `publisher.key` and `rotation.key` exist, and whether the
/// public key on disk matches one the shipped launcher trusts.
#[tauri::command]
pub fn keys_state() -> Result<Value, String> {
    let platform = Platform::detect();
    let settings_file = settings_path(&platform)?;
    let settings = Settings::load(&settings_file)?;

    let trusted: Vec<Value> = TRUSTED_KEYS
        .iter()
        .enumerate()
        .map(|(i, key)| {
            let role = match i {
                0 => "everyday",
                1 => "rotation",
                _ => "trusted",
            };
            json!({ "role": role, "public_hex": encode_public_key(key) })
        })
        .collect();

    let (publisher_path, dir) = key_paths(&settings);

    let publisher_state = match &publisher_path {
        None => json!({
            "configured": false,
            "path": null,
            "exists": false,
            "trust": "not_configured",
        }),
        Some(path) if !path.exists() => json!({
            "configured": true,
            "path": path.display().to_string(),
            "exists": false,
            "trust": "missing",
        }),
        Some(path) => match read_key(path) {
            Ok(Some(secret)) => {
                let public = signing_publisher::public_of(&secret);
                let trust = trust_role(&public).unwrap_or("untrusted");
                json!({
                    "configured": true,
                    "path": path.display().to_string(),
                    "exists": true,
                    "public_hex": encode_public_key(&public),
                    "trust": trust,
                })
            }
            Ok(None) => json!({
                "configured": true,
                "path": path.display().to_string(),
                "exists": true,
                "trust": "unreadable",
            }),
            Err(e) => json!({
                "configured": true,
                "path": path.display().to_string(),
                "exists": true,
                "trust": "error",
                "error": e.to_string(),
            }),
        },
    };

    let rotation_path = dir.as_ref().map(|d| d.join(ROTATION_KEY));
    let rotation_exists = rotation_path.as_ref().is_some_and(|p| p.exists());
    let publisher_exists = publisher_path.as_ref().is_some_and(|p| p.exists());

    Ok(json!({
        "trusted": trusted,
        "publisher_key": publisher_state,
        "rotation_key": {
            "path": rotation_path.map(|p| p.display().to_string()),
            "exists": rotation_exists,
        },
        // A rotation key that sits beside the everyday key defeats the
        // point of keeping it offline: the two are one breach apart
        // instead of two. True only when both files are actually there,
        // not merely configured.
        "same_folder_warning": rotation_exists && publisher_exists,
    }))
}

/// Generate `publisher.key` and `rotation.key` together, into `dir`.
/// Refuses if either file already exists.
#[tauri::command]
pub fn keys_generate(dir: String) -> Result<Value, String> {
    let out_dir = PathBuf::from(dir.trim());
    if out_dir.as_os_str().is_empty() {
        return Err("choose a folder to write the two new keys into first.".to_string());
    }

    let mut lines: Vec<String> = Vec::new();
    let mut sink = |e: reborn_core::publish::Event| lines.push(e.line);
    let outcome = keygen(&out_dir, &mut sink).map_err(|e| e.to_string())?;

    match outcome {
        Outcome::Refused(refusal) => Err(refusal.lines.join(" ")),
        Outcome::Described => {
            Err("keygen has nothing to describe; it either writes both keys or refuses.".to_string())
        }
        Outcome::Done(written) => {
            // Remember where the everyday key lives, so this page (and
            // every other command that needs to sign or check it) finds
            // it again without Adam being asked twice.
            let platform = Platform::detect();
            let settings_file = settings_path(&platform)?;
            let mut settings = Settings::load(&settings_file)?;
            settings.publisher_key = Some(written.publisher_path.display().to_string());
            settings.save(&settings_file)?;

            Ok(json!({
                "publisher_path": written.publisher_path.display().to_string(),
                "rotation_path": written.rotation_path.display().to_string(),
                "everyday_public_hex": encode_public_key(&written.everyday_public),
                "rotation_public_hex": encode_public_key(&written.rotation_public),
                "lines": lines,
            }))
        }
    }
}

/// The three vault fields: stored or missing, never the value.
#[tauri::command]
pub fn vault_state() -> Result<Value, String> {
    let platform = Platform::detect();
    let has_site = site_token(&platform).map_err(|e| e.to_string())?.is_some();
    let has_access = r2_access(&platform).map_err(|e| e.to_string())?.is_some();
    let has_secret = r2_secret(&platform).map_err(|e| e.to_string())?.is_some();

    let settings_file = settings_path(&platform)?;
    let mut settings = Settings::load(&settings_file)?;
    let already_offered = settings
        .remembered
        .get(RCLONE_OFFER_SEEN_KEY)
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let rclone_present = Path::new(RCLONE_CONF_PATH).is_file();
    let offer_import = rclone_present && !already_offered && !(has_access && has_secret);

    // "Offer once": the offer is marked seen the moment it is shown, right
    // here, rather than only when Adam acts on it. Importing later still
    // works through `vault_store("r2_import", ...)` at any time; what
    // this stops is the banner reappearing on every launch after Adam has
    // already dismissed or used it once.
    if offer_import {
        settings.remembered.insert(RCLONE_OFFER_SEEN_KEY.to_string(), Value::Bool(true));
        settings.save(&settings_file)?;
    }

    Ok(json!({
        "site_token": { "stored": has_site },
        "r2_access": { "stored": has_access },
        "r2_secret": { "stored": has_secret },
        "offer_import": offer_import,
    }))
}

/// The first `access_key_id`, `secret_access_key` and `endpoint` values
/// found anywhere in an rclone config's text, read top to bottom. Good
/// enough for a config that holds exactly one R2 remote, which is the only
/// shape this file's source ever takes; a section header is not required
/// to be named any particular thing, so this does not look for one.
fn parse_rclone_r2(text: &str) -> (Option<String>, Option<String>, Option<String>) {
    let mut access = None;
    let mut secret = None;
    let mut endpoint = None;
    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') || line.starts_with('[') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        match key.trim().to_ascii_lowercase().as_str() {
            "access_key_id" if access.is_none() => access = Some(value.to_string()),
            "secret_access_key" if secret.is_none() => secret = Some(value.to_string()),
            "endpoint" if endpoint.is_none() => endpoint = Some(value.to_string()),
            _ => {}
        }
    }
    (access, secret, endpoint)
}

/// Read `rclone.conf`, store whichever of the two R2 credential fields it
/// holds, and note its endpoint into settings if none is set yet. The
/// file itself is only ever read here, never written or deleted: Adam
/// keeps using it with `rclone` outside this app.
fn import_r2_from_rclone(platform: &Platform) -> Result<Value, String> {
    let path = Path::new(RCLONE_CONF_PATH);
    if !path.is_file() {
        return Err(format!("{} was not found. Nothing was imported.", path.display()));
    }
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("could not read {}: {e}", path.display()))?;
    let (access, secret, endpoint) = parse_rclone_r2(&text);
    drop(text);

    let mut imported_access = false;
    let mut imported_secret = false;
    if let Some(value) = access.filter(|v| !v.trim().is_empty()) {
        store_r2_access(platform, value.trim()).map_err(|e| e.to_string())?;
        imported_access = true;
    }
    if let Some(value) = secret.filter(|v| !v.trim().is_empty()) {
        store_r2_secret(platform, value.trim()).map_err(|e| e.to_string())?;
        imported_secret = true;
    }

    let settings_file = settings_path(platform)?;
    let mut settings = Settings::load(&settings_file)?;
    let mut imported_endpoint = false;
    if settings.bucket_endpoint.is_none() {
        if let Some(value) = endpoint.filter(|v| !v.trim().is_empty()) {
            settings.bucket_endpoint = Some(value);
            imported_endpoint = true;
        }
    }
    settings.remembered.insert(RCLONE_OFFER_SEEN_KEY.to_string(), Value::Bool(true));
    settings.save(&settings_file)?;

    if !imported_access && !imported_secret {
        return Err(format!(
            "{} was read but held neither an access key id nor a secret access key. Nothing was stored.",
            path.display()
        ));
    }

    Ok(json!({
        "which": "r2_import",
        "imported": {
            "r2_access": imported_access,
            "r2_secret": imported_secret,
            "bucket_endpoint": imported_endpoint,
        },
    }))
}

/// Store one vault value. Returns only whether it stored; the value is
/// never read back into the page.
///
/// `which` is `site_token`, `r2_access` or `r2_secret` with `value`
/// holding the secret to store, or `r2_import` (`value` ignored) to pull
/// both R2 fields from `rclone.conf` in one step; see
/// [`import_r2_from_rclone`].
#[tauri::command]
pub fn vault_store(which: String, value: String) -> Result<Value, String> {
    let platform = Platform::detect();

    if which == "r2_import" {
        return import_r2_from_rclone(&platform);
    }

    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err("that field cannot be stored empty. Use Forget to remove a stored value instead.".to_string());
    }

    match which.as_str() {
        "site_token" => store_site_token(&platform, trimmed).map_err(|e| e.to_string())?,
        "r2_access" => store_r2_access(&platform, trimmed).map_err(|e| e.to_string())?,
        "r2_secret" => store_r2_secret(&platform, trimmed).map_err(|e| e.to_string())?,
        other => {
            return Err(format!(
                "{other:?} is not a vault field. Use one of site_token, r2_access, r2_secret, r2_import."
            ));
        }
    }

    Ok(json!({ "which": which, "stored": true }))
}

/// Forget one vault value.
#[tauri::command]
pub fn vault_forget(which: String) -> Result<Value, String> {
    let platform = Platform::detect();
    match which.as_str() {
        "site_token" => forget_site_token(&platform).map_err(|e| e.to_string())?,
        "r2_access" => forget_r2_access(&platform).map_err(|e| e.to_string())?,
        "r2_secret" => forget_r2_secret(&platform).map_err(|e| e.to_string())?,
        other => {
            return Err(format!(
                "{other:?} is not a vault field. Use one of site_token, r2_access, r2_secret."
            ));
        }
    }
    Ok(json!({ "which": which, "stored": false }))
}

/// Open a file or folder picker for one settings field and save the chosen
/// path into settings.
#[tauri::command]
pub fn settings_browse(which: String) -> Result<Value, String> {
    let dialog = rfd::FileDialog::new().set_title("Reborn Publisher");
    let picked: Option<PathBuf> = match which.as_str() {
        "origin" => dialog.pick_folder(),
        "publisher_key" => dialog.add_filter("Signing key", &["key"]).pick_file(),
        "builder" => dialog.add_filter("Reborn Builder", &["exe"]).pick_file(),
        "xdelta" => dialog.add_filter("xdelta3", &["exe"]).pick_file(),
        other => {
            return Err(format!(
                "{other:?} is not a setting this page can browse for. Use one of origin, publisher_key, builder, xdelta."
            ));
        }
    };

    let platform = Platform::detect();
    let settings_file = settings_path(&platform)?;
    let mut settings = Settings::load(&settings_file)?;

    // A cancelled picker changes nothing, and that is success, not a
    // refusal: Adam looked and decided not to change this field.
    let Some(chosen) = picked else {
        return serde_json::to_value(&settings).map_err(|e| e.to_string());
    };

    let value = chosen.display().to_string();
    match which.as_str() {
        "origin" => settings.origin = value,
        "publisher_key" => settings.publisher_key = Some(value),
        "builder" => settings.builder = Some(value),
        "xdelta" => settings.xdelta = Some(value),
        _ => unreachable!("validated above"),
    }
    settings.save(&settings_file)?;
    serde_json::to_value(&settings).map_err(|e| e.to_string())
}
