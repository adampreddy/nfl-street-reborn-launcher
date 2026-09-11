//! The Status page: what the bucket serves, what sits in the local origin,
//! and what this machine has ready.
//!
//! All three commands read something and hand back a `serde_json::Value`
//! that never fails as a whole, the same discipline
//! `reborn_core::publish::status::read_status` itself holds to: a source
//! that could not be read becomes a line of explanation next to whatever
//! else did read, never a blank field and never a panic. This matters more
//! here than on any other page, because Task 1 found that **a Tauri
//! command that panics hangs its promise forever instead of rejecting it**,
//! and this is the page every window opens to. Nothing below calls
//! `.unwrap()` or `.expect()` on anything that depends on the outside
//! world (a file, a socket, the credential vault); the one `.expect()` in
//! this crate's dependency graph that touches keys
//! (`Ed25519Verifier::embedded`) only fires if the hard-coded
//! `signing::TRUSTED_KEYS` bytes stop being valid Ed25519 points, which is
//! a build-time fact, not a runtime one.
//!
//! # Two things this file decides that no other file fixes for it
//!
//! `reborn_core::publish::status::Status` and its parts (`LocalOrigin`,
//! `HostedRelease`, `HostedLauncher`, `Pending`) do not derive `Serialize`,
//! so [`status_to_json`] and its helpers below hand-build the
//! `serde_json::Value` this page's `status.js` reads. Keep the two in sync
//! by hand; there is no compiler check tying them together.
//!
//! `publisher_environment` checks nine tools and inputs. Three
//! (`publisher.key`, the Builder, xdelta) already have a field on
//! [`crate::settings::Settings`] this app can persist a browsed path into.
//! The other six do not: `cargo`, Inno Setup's `ISCC.exe`, the R2
//! credentials, the site token, the base image and the game-import bundle
//! carry no dedicated setting. Two conventions fill that gap, both worth
//! knowing before touching this file again:
//!
//! - The R2 access key id and secret live in the platform credential vault
//!   under the account names `docs/superpowers/specs/2026-09-08-reborn-\
//!   publisher-design.md` section 3.5 fixes: `nsr.r2.access` and
//!   `nsr.r2.secret`, beside the site token's existing `nsr.site.export`
//!   (`reborn_core::publish::vault::SITE_TOKEN_ACCOUNT`). `vault.rs` only
//!   defines the site token's constant today; Task 8 owns adding the other
//!   two there. They are re-declared as local constants below rather than
//!   waited on, because this task may only touch this file, and the string
//!   values are fixed by the design doc regardless of which file declares
//!   them.
//! - A path a person browses to for `cargo`, `ISCC.exe`, the base image or
//!   the import bundle is read from and, once `keys::settings_browse`
//!   (Task 8) is implemented, expected to be written to
//!   `Settings.remembered["environment"]`, a small JSON object keyed by
//!   this file's own `EnvItem.key` strings (`"cargo"`, `"inno_setup"`,
//!   `"base_image"`, `"import_bundle"`). `remembered` already exists for
//!   exactly this purpose (see `settings.rs`'s doc comment); Task 8 saving
//!   a browsed path anywhere else would need this file updated to match.
//!
//! # The Upload button now has a path
//!
//! The brief for this page asks for "an Upload button that runs the
//! upload operation through the panel" beside the pending components list.
//! When this file's own doc comment was first written, no command
//! reachable from this page could do that: `main.rs`'s `generate_handler!`
//! list carried no page-agnostic "upload whatever the origin holds"
//! command, and every other page's own `_run` command bundled a fresh
//! publish (fetch, build, or hash) together with its upload rather than
//! exposing a bare re-upload. `upload.rs` is that command now: a plan/run
//! pair over `reborn_core::publish::upload::{plan_origin, upload}`, run
//! through `crate::ops::run_operation` exactly like every other page's
//! pair, so an upload started from Status is journalled like any other
//! operation. `status.js`'s own panel wires Plan and Verify to
//! `publisher_status` (a real read, unchanged) and Run to `upload_run`; see
//! `upload.rs`'s own doc comment for the rest.

use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{json, Value};

use reborn_core::inventory::ComponentId;
use reborn_core::net::UreqHttp;
use reborn_core::publish::journal;
use reborn_core::publish::status::{
    read_status, HostedLauncher, HostedRelease, LocalOrigin, Pending, PendingReason, Status,
};
use reborn_core::publish::vault;
use reborn_core::signing::Ed25519Verifier;
use reborn_core::Platform;

use crate::settings::{self, Settings};

// ---------------------------------------------------------------------------
// publisher_status
// ---------------------------------------------------------------------------

/// `reborn_core::publish::status::read_status`, serialized.
#[tauri::command(rename_all = "snake_case")]
pub fn publisher_status() -> Value {
    let platform = Platform::detect();
    let mut settings_problems = Vec::new();
    let settings = load_settings(&platform, &mut settings_problems);

    let http = UreqHttp::new(&user_agent());
    let verifier = Ed25519Verifier::embedded();
    let origin = Path::new(&settings.origin);

    let mut status =
        read_status(&http, origin, &settings.public_base, &settings.launcher_manifest, &verifier);

    // A settings-read failure explains why everything else below might be
    // wrong (a stale origin path, a bucket base nobody chose), so it leads
    // the list rather than trailing it.
    if !settings_problems.is_empty() {
        settings_problems.extend(status.problems);
        status.problems = settings_problems;
    }

    status_to_json(&status)
}

fn user_agent() -> String {
    format!("reborn-publisher/{}", env!("CARGO_PKG_VERSION"))
}

fn pins_json(pins: &[(ComponentId, String)]) -> Vec<Value> {
    pins.iter().map(|(id, v)| json!({"component": id.as_str(), "version": v})).collect()
}

fn local_to_json(l: &LocalOrigin) -> Value {
    // A component whose manifest sits under `components/` but is not part
    // of the local release's own pins: republished on disk without a
    // release re-signed around it, the "half-finished publish" the core's
    // own doc comment names. Computed here, once, so status.js never has
    // to diff two lists itself to notice it.
    let unpinned: Vec<Value> = l
        .components_on_disk
        .iter()
        .filter(|(id, v, _)| !l.pins.iter().any(|(pid, pv)| pid == id && pv == v))
        .map(|(id, v, seq)| json!({"component": id.as_str(), "version": v, "sequence": seq}))
        .collect();

    json!({
        "version": l.version,
        "sequence": l.sequence,
        "pins": pins_json(&l.pins),
        "components_on_disk": l
            .components_on_disk
            .iter()
            .map(|(id, v, seq)| json!({"component": id.as_str(), "version": v, "sequence": seq}))
            .collect::<Vec<_>>(),
        "unpinned": unpinned,
    })
}

fn hosted_to_json(h: &HostedRelease) -> Value {
    json!({
        "version": h.version,
        "sequence": h.sequence,
        "pins": pins_json(&h.pins),
        "verified": h.verified,
    })
}

fn launcher_to_json(l: &HostedLauncher) -> Value {
    json!({
        "version": l.version,
        "bytes": l.bytes,
        "sha256": l.sha256,
        "file_name": l.file_name,
    })
}

/// A short, ready-made sentence per [`PendingReason`], so `status.js` never
/// has to encode what a reason means; it only ever reads one.
fn reason_text(p: &Pending) -> String {
    match p.reason {
        PendingReason::NotServed => {
            "the bucket's release does not pin this component at all".to_string()
        }
        PendingReason::OlderOnBucket => format!(
            "the bucket serves {} of this component, older than the {} on this machine, but this \
             machine's release is not at a higher sequence, so a pending re-release does not \
             explain it; most likely this component was republished without re-signing the \
             release around it",
            p.hosted.as_deref().unwrap_or("an older version"),
            p.local
        ),
        PendingReason::LocalReleaseNewer => format!(
            "this machine signed a newer release (carrying {}) than the bucket has ({}); it has \
             not been uploaded yet",
            p.local,
            p.hosted.as_deref().unwrap_or("an older version")
        ),
    }
}

fn pending_to_json(p: &Pending) -> Value {
    let reason_code = match p.reason {
        PendingReason::NotServed => "not_served",
        PendingReason::OlderOnBucket => "older_on_bucket",
        PendingReason::LocalReleaseNewer => "local_release_newer",
    };
    json!({
        "component": p.component.as_str(),
        "local": p.local,
        "hosted": p.hosted,
        "reason": reason_code,
        "reason_text": reason_text(p),
    })
}

fn status_to_json(status: &Status) -> Value {
    json!({
        "problems": status.problems,
        "local": status.local.as_ref().map(local_to_json),
        "hosted": status.hosted.as_ref().map(hosted_to_json),
        "launcher": status.launcher.as_ref().map(launcher_to_json),
        "pending": status.pending.iter().map(pending_to_json).collect::<Vec<_>>(),
    })
}

// ---------------------------------------------------------------------------
// publisher_environment
// ---------------------------------------------------------------------------

/// One tool or input this app needs, and what checking it found. `kind`
/// is `"vault"` for the two secrets (Store/Forget lives on the Keys page;
/// this page only ever shows "stored" or "missing" for them, never a
/// Browse button) or `"file"` / `"dir"` for everything else, where a
/// missing item gets a Browse button on the page.
#[derive(Debug, Clone, Serialize)]
struct EnvItem {
    key: &'static str,
    label: &'static str,
    kind: &'static str,
    found: bool,
    path: Option<String>,
    note: Option<String>,
}

/// Every tool and input this app needs: found or missing, with the path.
#[tauri::command(rename_all = "snake_case")]
pub fn publisher_environment() -> Value {
    let platform = Platform::detect();
    let mut problems = Vec::new();
    let settings = load_settings(&platform, &mut problems);
    let remembered = settings.remembered.get("environment").cloned().unwrap_or_else(|| json!({}));

    let bundle_item = import_bundle_item(&remembered);
    let xdelta = xdelta_item(&settings, bundle_item.path.as_deref());

    let items = vec![
        publisher_key_item(&settings),
        builder_item(&settings),
        xdelta,
        cargo_item(&remembered),
        inno_setup_item(&remembered),
        r2_item(&platform),
        site_token_item(&platform),
        base_image_item(&remembered),
        bundle_item,
    ];

    json!({ "items": items, "problems": problems })
}

/// The default location this app hardcodes for the game-import bundle.
/// Not a made-up guess: this whole app is single-operator, single-machine
/// by design (`docs/superpowers/specs/2026-09-08-reborn-publisher-design.md`
/// section 2, "Who uses it: Adam only, on this PC"), and this is where
/// the bundle actually sits on that machine today. A different machine, or
/// a moved bundle, overrides it through Browse, same as every other item
/// here.
const IMPORT_BUNDLE_DEFAULT: &str = r"C:\Reborn Origin-stage\game-import-bundle-fields-12-19";

/// Account names the R2 secrets live under in the platform credential
/// vault, fixed by
/// `docs/superpowers/specs/2026-09-08-reborn-publisher-design.md` section
/// 3.5. See this file's module doc for why they are declared here rather
/// than read from `reborn_core::publish::vault`.
const R2_ACCESS_ACCOUNT: &str = "nsr.r2.access";
const R2_SECRET_ACCOUNT: &str = "nsr.r2.secret";

/// A file this app needs at one path: `configured` (a `Settings` field,
/// when one exists) wins if it is set, whether or not the file is still
/// there, because a stale configured path is a different, more specific
/// problem than "never configured": the operator chose a path and it is
/// gone. Failing that, the first of `candidates` that exists on disk wins.
/// Neither found: missing, with `missing_note` explaining what was
/// checked.
fn file_item(
    key: &'static str,
    label: &'static str,
    configured: Option<&str>,
    candidates: &[PathBuf],
    missing_note: Option<&str>,
) -> EnvItem {
    if let Some(c) = configured.filter(|c| !c.is_empty()) {
        return if Path::new(c).is_file() {
            EnvItem { key, label, kind: "file", found: true, path: Some(c.to_string()), note: None }
        } else {
            EnvItem {
                key,
                label,
                kind: "file",
                found: false,
                path: None,
                note: Some(format!("configured at {c}, but no file is there now")),
            }
        };
    }
    for candidate in candidates {
        if candidate.is_file() {
            return EnvItem {
                key,
                label,
                kind: "file",
                found: true,
                path: Some(candidate.display().to_string()),
                note: None,
            };
        }
    }
    EnvItem { key, label, kind: "file", found: false, path: None, note: missing_note.map(str::to_string) }
}

fn publisher_key_item(settings: &Settings) -> EnvItem {
    file_item(
        "publisher_key",
        "publisher.key",
        settings.publisher_key.as_deref(),
        &[],
        Some("not set; browse to it, or generate one from the Keys page"),
    )
}

fn builder_item(settings: &Settings) -> EnvItem {
    let mut candidates = Vec::new();
    if let Some(appdata) = std::env::var_os("APPDATA") {
        candidates.push(
            PathBuf::from(appdata).join("NFL Street Reborn").join("Tools").join("reborn-builder-cli.exe"),
        );
    }
    file_item(
        "builder",
        "Reborn Builder",
        settings.builder.as_deref(),
        &candidates,
        Some("not found beside the launcher's tools folder; browse to reborn-builder-cli.exe"),
    )
}

/// Design section 3.4: xdelta "found in the import bundle." `bundle_path`
/// is [`import_bundle_item`]'s own resolved path, so this checks the same
/// bundle the This Machine block already reported on, not a second guess.
fn xdelta_item(settings: &Settings, bundle_path: Option<&str>) -> EnvItem {
    let base = bundle_path.map(PathBuf::from).unwrap_or_else(|| PathBuf::from(IMPORT_BUNDLE_DEFAULT));
    let candidates = vec![base.join("xdelta3.exe")];
    file_item(
        "xdelta",
        "xdelta3",
        settings.xdelta.as_deref(),
        &candidates,
        Some("not found in the game-import bundle; browse to xdelta3.exe"),
    )
}

fn cargo_item(remembered: &Value) -> EnvItem {
    let configured = remembered.get("cargo").and_then(Value::as_str);
    let mut candidates = Vec::new();
    if let Some(profile) = std::env::var_os("USERPROFILE") {
        candidates.push(PathBuf::from(profile).join(".cargo").join("bin").join("cargo.exe"));
    }
    file_item("cargo", "cargo", configured, &candidates, Some("not found at the usual rustup location; browse to cargo.exe"))
}

fn inno_setup_item(remembered: &Value) -> EnvItem {
    let configured = remembered.get("inno_setup").and_then(Value::as_str);
    let mut candidates = Vec::new();
    for var in ["ProgramFiles(x86)", "ProgramFiles"] {
        if let Some(pf) = std::env::var_os(var) {
            candidates.push(PathBuf::from(pf).join("Inno Setup 6").join("ISCC.exe"));
        }
    }
    file_item(
        "inno_setup",
        "Inno Setup (ISCC.exe)",
        configured,
        &candidates,
        Some("not found at the usual install location; browse to ISCC.exe"),
    )
}

fn base_image_item(remembered: &Value) -> EnvItem {
    let configured = remembered.get("base_image").and_then(Value::as_str);
    file_item(
        "base_image",
        "Base image (FIELDS 12-19)",
        configured,
        &[],
        Some("no default location is known; browse to the FIELDS 12-19 base ISO the Builder needs for a full build"),
    )
}

fn import_bundle_item(remembered: &Value) -> EnvItem {
    const REQUIRED: &[&str] = &["manifest.json", "xdelta3.exe", "DolphinTool.exe"];
    let key = "import_bundle";
    let label = "Game-import bundle";

    let configured = remembered.get("import_bundle").and_then(Value::as_str).filter(|c| !c.is_empty());
    let dir = match configured {
        Some(c) => PathBuf::from(c),
        None => PathBuf::from(IMPORT_BUNDLE_DEFAULT),
    };

    if !dir.is_dir() {
        let note = match configured {
            Some(c) => format!("configured at {c}, but that folder is not there now"),
            None => "the usual location does not have it; browse to the bundle folder".to_string(),
        };
        return EnvItem { key, label, kind: "dir", found: false, path: None, note: Some(note) };
    }

    let missing: Vec<&str> = REQUIRED.iter().copied().filter(|f| !dir.join(f).is_file()).collect();
    if missing.is_empty() {
        EnvItem { key, label, kind: "dir", found: true, path: Some(dir.display().to_string()), note: None }
    } else {
        EnvItem {
            key,
            label,
            kind: "dir",
            found: false,
            path: None,
            note: Some(format!("{} is missing {}", dir.display(), missing.join(", "))),
        }
    }
}

fn vault_check(platform: &Platform, account: &str) -> (bool, Option<String>) {
    match platform.credentials.retrieve(account) {
        Ok(Some(_)) => (true, None),
        Ok(None) => (false, None),
        Err(e) => (false, Some(format!("the credential vault could not be checked: {e}"))),
    }
}

fn r2_item(platform: &Platform) -> EnvItem {
    let (access_found, access_note) = vault_check(platform, R2_ACCESS_ACCOUNT);
    let (secret_found, secret_note) = vault_check(platform, R2_SECRET_ACCOUNT);
    let found = access_found && secret_found;
    let mut parts = Vec::new();
    if !access_found {
        parts.push(access_note.unwrap_or_else(|| "access key id not stored".to_string()));
    }
    if !secret_found {
        parts.push(secret_note.unwrap_or_else(|| "secret not stored".to_string()));
    }
    EnvItem {
        key: "r2_credentials",
        label: "R2 credentials",
        kind: "vault",
        found,
        path: None,
        note: if parts.is_empty() { None } else { Some(parts.join("; ")) },
    }
}

fn site_token_item(platform: &Platform) -> EnvItem {
    match vault::site_token(platform) {
        Ok(Some(_)) => {
            EnvItem { key: "site_token", label: "Site token", kind: "vault", found: true, path: None, note: None }
        }
        Ok(None) => {
            EnvItem { key: "site_token", label: "Site token", kind: "vault", found: false, path: None, note: None }
        }
        Err(e) => EnvItem {
            key: "site_token",
            label: "Site token",
            kind: "vault",
            found: false,
            path: None,
            note: Some(format!("the credential vault could not be checked: {e}")),
        },
    }
}

// ---------------------------------------------------------------------------
// publisher_recent
// ---------------------------------------------------------------------------

/// `journal::read_last`, serialized, newest first.
///
/// `read_last` itself returns the last `n` records oldest-first (see its
/// own doc comment); this page wants newest first, so the reversal happens
/// once, here, rather than asking `status.js` to know which end is which.
#[tauri::command(rename_all = "snake_case")]
pub fn publisher_recent(n: usize) -> Value {
    let platform = Platform::detect();
    let mut problems = Vec::new();
    let settings = load_settings(&platform, &mut problems);
    let origin = Path::new(&settings.origin);

    match journal::read_last(origin, n) {
        Ok(mut records) => {
            records.reverse();
            json!({ "records": records, "error": Value::Null, "problems": problems })
        }
        Err(e) => json!({ "records": Vec::<Value>::new(), "error": e.to_string(), "problems": problems }),
    }
}

// ---------------------------------------------------------------------------
// shared
// ---------------------------------------------------------------------------

/// This app's own settings, or the defaults with a note explaining why:
/// an absent settings file is normal (see `Settings::load`'s own doc), but
/// a settings file that exists and does not parse is not, and every read
/// below (the origin path, the bucket, the launcher manifest) would
/// otherwise silently fall back to a default nobody chose.
fn load_settings(platform: &Platform, problems: &mut Vec<String>) -> Settings {
    match settings::settings_path(platform).and_then(|p| Settings::load(&p)) {
        Ok(s) => s,
        Err(e) => {
            problems.push(format!(
                "This app's own settings could not be read ({e}). Showing defaults; nothing below \
                 reflects what was actually configured."
            ));
            Settings::default()
        }
    }
}
