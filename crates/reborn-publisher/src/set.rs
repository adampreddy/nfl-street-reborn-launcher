//! The Set page: build, upload and point the download link at the full
//! first-run installer set -- the spanned installer plus its zip, the game
//! import bundle, the Dolphin runtime, the texture pack and the offline
//! WebView2 runtime. This is what a brand new player downloads.
//!
//! Three commands, the same Plan/Run/Verify shape every page uses:
//!
//! - [`set_plan`]: measures the four input folders (file count, total
//!   size), shows what `launcher/download.json` currently points at, warns
//!   when the version below equals that hosted one, and lists exactly the
//!   files a zip would be packed from. Writes nothing.
//! - [`set_run`]: runs the full `installer\windows\build.ps1` with
//!   `-GamePatch -Runtime -Textures -PromptPackages -OfflineWebView2
//!   -OfflineSuffix ""`, streaming its output live (the build takes about
//!   twenty minutes); on success, independently re-checks the zip it
//!   produced holds exactly the expected file set, then uploads the zip,
//!   then writes and uploads `launcher/download.json`.
//! - [`set_verify`]: reads `download.json` back over the public host,
//!   HEADs the zip it names (no re-download of a multi-gigabyte file), and
//!   compares both against the local zip still in `dist`.
//!
//! # The two guards, both learned the hard way
//!
//! **The staging folder is deleted before staging.** `build.ps1` computes
//! `target\installer-payload`, its scratch folder for this exact kind of
//! build, and deletes whatever is there before copying a fresh payload
//! into it (see that script's own comment on the rename-then-delete
//! trick it uses to do this safely). If `-GamePatch` (the game-import
//! bundle) pointed inside that folder, staging would delete the bundle
//! before it could be copied in -- this happened once and cost the only
//! staged copy of a 687 MB bundle. `build.ps1` already refuses this
//! itself, with a specific message; [`staging_guard`] runs the identical
//! check here, in both [`set_plan`] and [`set_run`], before `powershell.exe`
//! is even spawned, so the refusal reads as a clear, specific reason
//! instead of the generic "build.ps1 exited with status 1" an operator
//! would otherwise have to scroll the log to explain.
//!
//! **The zip is packed from an exact file set.** `build.ps1` packs the
//! setup exe plus its own numbered `.bin` slices for this version, named
//! by an exact match, never a wildcard (its own comment records the day a
//! wildcard swept another build's slices into a 5 GB zip over a good one).
//! [`exact_set_in_dist`] runs the identical name rule here: [`set_plan`]
//! lists exactly what it finds in `dist` matching that rule (with sizes,
//! or says plainly that nothing is built yet), and [`set_run`], after a
//! successful build, opens the zip `build.ps1` just produced
//! ([`verify_zip_contents`]) and refuses to upload it unless its entries
//! are exactly that same set -- never trusting the script's own internal
//! count check alone, the same "never trust the tool's own claim" rule
//! `launcher.rs` follows for its own exe hash.
//!
//! # Upload order: the zip first, `download.json` last
//!
//! One [`reborn_core::publish::upload::UploadPlan`], not two separate
//! calls: the zip goes in as [`UploadPlan::blob`], `download.json` as
//! [`UploadPlan::release_file`]. `UploadPlan::items` always yields blobs
//! before release files regardless of add order (see that type's own
//! doc), and `upload::upload` sends items strictly in that order, stopping
//! at the first failure -- so if the zip's own upload fails, `download.json`
//! is never even attempted. That is a type-level guarantee the site can
//! never be pointed at a zip still in flight, the same trick `launcher.rs`
//! uses for its exe and `latest.json`. `download.json`'s key,
//! `launcher/download.json`, does not match either name
//! `upload::upload`'s own automatic read-back checks for (`release.json`,
//! or a bare `latest.json`), so that check always skips here, exactly as
//! `launcher.rs` notes for its own `launcher/latest.json` key;
//! [`set_verify`] is this page's own dedicated read-back instead.
//!
//! # Why this does not go through `ProcessHost`
//!
//! Same reason, and the same fix, as `launcher.rs`'s own build step: see
//! its module doc, "Why this does not go through `ProcessHost`". This
//! page's [`run_build`] is that same shape -- `powershell.exe` spawned
//! directly, stdout and stderr read line by line off two reader threads so
//! the panel sees progress within a fraction of a second of it being
//! printed, with a `CancelSignal` checked between lines so the operator's
//! Cancel button can stop a twenty-minute build early -- generalized to
//! take an arbitrary argument list, since this page's invocation carries
//! far more flags than that page's own four.
//!
//! # Verify reads headers, not the whole zip
//!
//! `launcher_verify` downloads the whole installer it is checking, because
//! that installer is small. This page's zip is multiple gigabytes;
//! re-downloading it just to confirm a size would be exactly the kind of
//! waste the brief's "the zip's headers" wording rules out. [`net::Http`]
//! gained a `head` method for this (added here, not in a file this task's
//! brief names, because nothing in this crate could ask the public host
//! for a header-only reply without it): a HEAD request, no body
//! transferred. [`set_verify`] uses it to confirm the zip is reachable and
//! answers the size `download.json` claims, and separately re-hashes the
//! *local* copy of the zip (already on disk from the run that published
//! it) to confirm the pointer's hash is honest -- never re-hashing what
//! the network serves, which would mean downloading it anyway.

use std::ffi::OsString;
use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, SystemTime};

use serde_json::{json, Value};

use reborn_core::inventory::CancelSignal;
use reborn_core::library::hash_file;
use reborn_core::manifest::Sha256Hex;
use reborn_core::net::{Http, UreqHttp};
use reborn_core::publish::download::{self, DownloadPointer};
use reborn_core::publish::journal;
use reborn_core::publish::upload::{self, Bucket, UploadPlan, UploadRequest, Uploaded, UreqTransport};
use reborn_core::publish::vault;
use reborn_core::publish::{out, refuse, thousands, Event, Outcome, Sink};
use reborn_core::Platform;

use crate::ops;
use crate::settings::{self, Settings};

/// The product name `build.ps1` calls `$AppNameForZip` on the stable
/// channel, which this page always builds (there is no beta variant of
/// the full first-run set in this design, and `-Channel` is left at
/// `build.ps1`'s own "stable" default). Every name this file derives --
/// the setup exe, its slices, the zip -- is built from this the same way
/// that script builds them, so the two can never disagree about a name.
const APP_NAME_FOR_ZIP: &str = "NFL Street Reborn";

// ---------------------------------------------------------------------------
// Names build.ps1 derives from the version, mirrored exactly
// ---------------------------------------------------------------------------

/// `"<app> Launcher Setup <version>"`: the setup exe is this plus `.exe`,
/// and a slice is this plus `-<digits>.bin`.
fn zip_prefix(version: &str) -> String {
    format!("{APP_NAME_FOR_ZIP} Launcher Setup {version}")
}

/// `"<app> Launcher <version>.zip"`: the one file the whole set ships as.
fn zip_name(version: &str) -> String {
    format!("{APP_NAME_FOR_ZIP} Launcher {version}.zip")
}

/// Whether `name` is exactly a numbered slice of `prefix`: `"<prefix>-"`
/// followed by one or more ASCII digits and nothing else, then `.bin`.
/// Deliberately not a wildcard or a `starts_with`: a slice from a
/// different, similarly-named build (`"<prefix>-onboarding.1-1.bin"`) must
/// never match, which is exactly the rule `build.ps1`'s own comment
/// records learning the hard way.
fn is_slice_file(name: &str, prefix: &str) -> bool {
    let Some(rest) = name.strip_prefix(prefix).and_then(|s| s.strip_prefix('-')) else {
        return false;
    };
    let Some(digits) = rest.strip_suffix(".bin") else {
        return false;
    };
    !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())
}

/// One file in `dist` that matches the exact set rule for `version`: the
/// setup exe, or one of its own numbered slices.
#[derive(Debug, Clone)]
struct ZipMember {
    name: String,
    bytes: u64,
}

/// See the module doc's "The zip is packed from an exact file set". Every
/// file directly under `dist` whose name is exactly `"<prefix>.exe"` or a
/// slice of it, sorted by name. Empty (not an error) when `dist` does not
/// exist yet, or nothing in it matches: a fresh checkout, or a Plan run
/// before the first build, has nothing to list.
fn exact_set_in_dist(dist: &Path, version: &str) -> std::io::Result<Vec<ZipMember>> {
    let prefix = zip_prefix(version);
    let exe_name = format!("{prefix}.exe");
    let mut found = Vec::new();
    if !dist.is_dir() {
        return Ok(found);
    }
    for entry in std::fs::read_dir(dist)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == exe_name || is_slice_file(&name, &prefix) {
            found.push(ZipMember { bytes: entry.metadata()?.len(), name });
        }
    }
    found.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(found)
}

fn zip_member_json(members: &[ZipMember]) -> Vec<Value> {
    members.iter().map(|m| json!({"name": m.name, "bytes": m.bytes})).collect()
}

/// Open the zip [`set_run`] just built and confirm its entries are
/// *exactly* `expected` -- not a superset, not a subset. Never trusts
/// `build.ps1`'s own internal count check alone (see that script's own
/// "The zip holds N of M files" guard): this re-opens the finished file
/// and reads its central directory itself, the same "never trust the
/// tool's own claim" rule `launcher.rs` applies to its exe's hash. Cheap
/// even for a multi-gigabyte zip: `ZipArchive::new` reads the central
/// directory, never the (here, uncompressed) entry bytes.
fn verify_zip_contents(zip_path: &Path, expected: &[ZipMember]) -> Result<(), String> {
    let file = std::fs::File::open(zip_path).map_err(|e| format!("{}: {e}", zip_path.display()))?;
    let archive = zip::ZipArchive::new(file).map_err(|e| format!("{} does not open as a zip: {e}", zip_path.display()))?;
    let mut actual: Vec<String> = archive.file_names().map(str::to_string).collect();
    actual.sort();
    let mut wanted: Vec<String> = expected.iter().map(|m| m.name.clone()).collect();
    wanted.sort();
    if actual == wanted {
        return Ok(());
    }
    let missing: Vec<&String> = wanted.iter().filter(|w| !actual.contains(w)).collect();
    let extra: Vec<&String> = actual.iter().filter(|a| !wanted.contains(a)).collect();
    Err(format!(
        "{} does not hold exactly the setup exe plus its own numbered slices for this version. Missing: \
         [{}]. Extra: [{}]. Refusing to upload a zip that does not match the expected set.",
        zip_path.display(),
        missing.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", "),
        extra.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", "),
    ))
}

// ---------------------------------------------------------------------------
// The staging-folder guard
// ---------------------------------------------------------------------------

/// See the module doc's "The staging folder is deleted before staging".
/// `payload` is `<repo>\target\installer-payload`, the exact folder
/// `build.ps1` stages a full (non-launcher-only, non-Publisher) build
/// into, and deletes first. Refuses with a specific reason when `bundle`
/// is inside it; `Ok(())` otherwise.
fn staging_guard(bundle: &Path, repo_root: &Path) -> Result<(), String> {
    let payload = repo_root.join("target").join("installer-payload");
    if is_inside(bundle, &payload) {
        return Err(format!(
            "The game-import bundle at {} is inside {}, which build.ps1 deletes before it stages a new \
             build. Building from there would delete the bundle before it could be copied in; build.ps1 \
             already refuses this itself for the same reason. Move the bundle outside {} and point at it \
             there instead. Nothing was built.",
            bundle.display(),
            payload.display(),
            payload.display()
        ));
    }
    Ok(())
}

/// Whether `inner` is `outer` or somewhere under it, compared the same way
/// `build.ps1`'s own guard does: a plain, case-insensitive `StartsWith` on
/// each side's lexically-absolute form ([`std::path::absolute`], the same
/// thing PowerShell's `[System.IO.Path]::GetFullPath` does), with no
/// trailing separator added to either side. That means a folder whose name
/// merely starts with `outer`'s name (`installer-payload-2`) also matches,
/// which is `build.ps1`'s own known imprecision, deliberately not
/// tightened here: the two checks must never disagree about when they
/// refuse.
fn is_inside(inner: &Path, outer: &Path) -> bool {
    let inner_abs = std::path::absolute(inner).unwrap_or_else(|_| inner.to_path_buf());
    let outer_abs = std::path::absolute(outer).unwrap_or_else(|_| outer.to_path_buf());
    inner_abs.to_string_lossy().to_ascii_lowercase().starts_with(&outer_abs.to_string_lossy().to_ascii_lowercase())
}

// ---------------------------------------------------------------------------
// Measuring the four inputs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Default)]
struct FolderStats {
    files: u64,
    bytes: u64,
}

/// A plain recursive walk: every file's size, summed, and a count. No
/// hashing (these are build inputs, never published as content-addressed
/// blobs themselves), so this is cheap even for the texture tree's
/// thousands of files -- nothing like the multi-second hash `content.rs`
/// pays for a component publish of the same size.
fn scan_folder(path: &Path) -> std::io::Result<FolderStats> {
    let mut stats = FolderStats::default();
    let mut stack = vec![path.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let file_type = entry.file_type()?;
            if file_type.is_dir() {
                stack.push(entry.path());
            } else if file_type.is_file() {
                stats.files += 1;
                stats.bytes += entry.metadata()?.len();
            }
        }
    }
    Ok(stats)
}

fn folder_stats_json(path: &Path, stats: FolderStats) -> Value {
    json!({"path": path.display().to_string(), "files": stats.files, "bytes": stats.bytes})
}

// ---------------------------------------------------------------------------
// The four inputs, validated
// ---------------------------------------------------------------------------

const INPUT_LABELS: [&str; 4] = ["game-import bundle", "runtime tree", "texture tree", "prompt packages"];

/// A path as the operator typed it or picked with Browse, made absolute.
/// `run_build` sets the child's working directory to the repository root,
/// not this process's, so a relative path would resolve there instead of
/// wherever the operator meant. Same helper, duplicated, as `game.rs`'s
/// own `absolute_path`.
fn absolute_path(p: &str) -> PathBuf {
    let path = PathBuf::from(p);
    if path.is_absolute() {
        path
    } else {
        std::env::current_dir().map(|cwd| cwd.join(&path)).unwrap_or(path)
    }
}

/// The four folder arguments every command here takes, checked for
/// emptiness and made absolute. Does not check they exist on disk: that
/// happens inside the operation body, as a refusal (journaled), the same
/// split every other page in this app makes between "nothing was even
/// given" (an immediate `Err`, never journaled) and "what was given is not
/// real" (a `refuse`, journaled).
#[derive(Debug)]
struct Inputs {
    bundle: PathBuf,
    runtime: PathBuf,
    textures: PathBuf,
    prompt_packages: PathBuf,
    version: String,
}

fn parse_inputs(bundle: &str, runtime: &str, textures: &str, prompt_packages: &str, version: &str) -> Result<Inputs, String> {
    let version = version.trim().to_string();
    if version.is_empty() {
        return Err("Choose a version to build first.".to_string());
    }
    for (label, value) in INPUT_LABELS.iter().zip([bundle, runtime, textures, prompt_packages]) {
        if value.trim().is_empty() {
            return Err(format!("Choose the {label} folder first."));
        }
    }
    Ok(Inputs {
        bundle: absolute_path(bundle.trim()),
        runtime: absolute_path(runtime.trim()),
        textures: absolute_path(textures.trim()),
        prompt_packages: absolute_path(prompt_packages.trim()),
        version,
    })
}

impl Inputs {
    fn as_pairs(&self) -> [(&'static str, &Path); 4] {
        [
            (INPUT_LABELS[0], self.bundle.as_path()),
            (INPUT_LABELS[1], self.runtime.as_path()),
            (INPUT_LABELS[2], self.textures.as_path()),
            (INPUT_LABELS[3], self.prompt_packages.as_path()),
        ]
    }

    /// Every input that is not a real folder, named. `Ok(())` when all
    /// four are.
    fn check_dirs(&self) -> Result<(), Vec<String>> {
        let missing: Vec<String> =
            self.as_pairs().into_iter().filter(|(_, p)| !p.is_dir()).map(|(label, p)| format!("{label}: {} is not a folder.", p.display())).collect();
        if missing.is_empty() {
            Ok(())
        } else {
            Err(missing)
        }
    }
}

// ---------------------------------------------------------------------------
// set_plan
// ---------------------------------------------------------------------------

/// Plan: measure each input, show what `launcher/download.json` currently
/// points at (warning if `version` would replace it under the exact same
/// label), and list exactly what a zip for `version` would be packed from.
/// Writes nothing.
#[tauri::command(rename_all = "snake_case")]
pub fn set_plan(bundle: String, runtime: String, textures: String, prompt_packages: String, version: String) -> Result<Value, String> {
    let inputs = parse_inputs(&bundle, &runtime, &textures, &prompt_packages, &version)?;
    let (_platform, settings) = settings_and_platform()?;
    let public_base = settings.public_base.clone();
    let builder = builder_path(&settings);

    let record_inputs = json!({
        "kind": "set-plan",
        "version": inputs.version,
        "bundle": inputs.bundle.display().to_string(),
        "runtime": inputs.runtime.display().to_string(),
        "textures": inputs.textures.display().to_string(),
        "prompt_packages": inputs.prompt_packages.display().to_string(),
    });

    ops::run_operation("set:plan", true, record_inputs, move |sink: Sink<'_>| -> reborn_core::Result<Outcome<Value>> {
        if let Err(missing) = inputs.check_dirs() {
            return Ok(refuse(sink, &missing));
        }
        let repo = match repo_root() {
            Ok(r) => r,
            Err(e) => return Ok(refuse(sink, &[e])),
        };
        if let Err(e) = staging_guard(&inputs.bundle, &repo) {
            return Ok(refuse(sink, &[e]));
        }

        out!(sink, "Version       {}", inputs.version);
        out!(sink, "Bundle        {}", inputs.bundle.display());
        out!(sink, "Runtime       {}", inputs.runtime.display());
        out!(sink, "Textures      {}", inputs.textures.display());
        out!(sink, "Prompt pkgs   {}", inputs.prompt_packages.display());
        out!(sink);
        out!(sink, "Measuring each input (a plain walk for size and file count; nothing is hashed).");

        let bundle_stats = scan_folder(&inputs.bundle)?;
        let runtime_stats = scan_folder(&inputs.runtime)?;
        let textures_stats = scan_folder(&inputs.textures)?;
        let prompt_stats = scan_folder(&inputs.prompt_packages)?;
        out!(sink, "  bundle        {} file(s), {} byte(s)", thousands(bundle_stats.files), thousands(bundle_stats.bytes));
        out!(sink, "  runtime       {} file(s), {} byte(s)", thousands(runtime_stats.files), thousands(runtime_stats.bytes));
        out!(sink, "  textures      {} file(s), {} byte(s)", thousands(textures_stats.files), thousands(textures_stats.bytes));
        out!(sink, "  prompt pkgs   {} file(s), {} byte(s)", thousands(prompt_stats.files), thousands(prompt_stats.bytes));
        let total_bytes = bundle_stats.bytes + runtime_stats.bytes + textures_stats.bytes + prompt_stats.bytes;

        let builder_found = builder.as_deref().map(Path::is_file).unwrap_or(false);
        if let Some(b) = &builder {
            out!(sink);
            if builder_found {
                out!(sink, "Builder       {}", b.display());
            } else {
                out!(sink, "Builder       {} (not found; build.ps1 will refuse to build until one is there)", b.display());
            }
        }

        out!(sink);
        out!(sink, "Checking      {} for what is hosted now", download::DOWNLOAD_FILE);
        let http = UreqHttp::new(&user_agent());
        let (hosted_value, warning) = plan_hosted_facts(&http, &public_base, &inputs.version, sink);

        out!(sink);
        let prefix = zip_prefix(&inputs.version);
        let expected_zip = zip_name(&inputs.version);
        out!(sink, "Zip           {expected_zip}");
        out!(sink, "  exactly {prefix}.exe plus any {prefix}-<n>.bin slices for this version -- never a wildcard.");
        let dist = repo.join("dist");
        let existing = exact_set_in_dist(&dist, &inputs.version)?;
        let existing_total: u64 = existing.iter().map(|m| m.bytes).sum();
        if existing.is_empty() {
            out!(sink, "  nothing in {} matches that yet; Run will produce it.", dist.display());
        } else {
            for m in &existing {
                out!(sink, "  {}  ({} byte(s))", m.name, thousands(m.bytes));
            }
            out!(sink, "  {} file(s) already in {}, {} byte(s) total.", existing.len(), dist.display(), thousands(existing_total));
        }

        let summary = format!(
            "{}: {} byte(s) across the four inputs to stage.{}",
            inputs.version,
            thousands(total_bytes),
            warning.as_deref().map(|_| " WARNING: this would replace the version currently hosted under the same label.").unwrap_or("")
        );

        Ok(Outcome::Done(json!({
            "summary": summary,
            "version": inputs.version,
            "warning": warning,
            "hosted": hosted_value,
            "inputs": {
                "bundle": folder_stats_json(&inputs.bundle, bundle_stats),
                "runtime": folder_stats_json(&inputs.runtime, runtime_stats),
                "textures": folder_stats_json(&inputs.textures, textures_stats),
                "prompt_packages": folder_stats_json(&inputs.prompt_packages, prompt_stats),
            },
            "total_bytes": total_bytes,
            "builder": builder.as_ref().map(|b| json!({"path": b.display().to_string(), "found": builder_found})),
            "zip": {
                "expected_name": expected_zip,
                "expected_prefix": prefix,
                "existing_files": zip_member_json(&existing),
                "existing_total_bytes": existing_total,
            },
        })))
    })
    .map(|()| json!({"accepted": true}))
}

/// What `download.json` currently points at, as the JSON `set_plan` shows,
/// plus a warning when `version` would replace it under the exact same
/// label. A read failure is shown as data (`"reachable": false`), not a
/// refusal: the rest of the plan (input sizes, the zip's expected
/// contents) is still worth showing even when the bucket cannot be
/// reached right now.
fn plan_hosted_facts(http: &dyn Http, public_base: &str, version: &str, sink: Sink<'_>) -> (Value, Option<String>) {
    match download::read_hosted(http, public_base) {
        Ok(Some(p)) => {
            out!(sink, "Bucket serves {} now, published {}", p.version, p.published);
            let warning = hosted_version_warning(version, &p);
            (
                json!({
                    "reachable": true, "version": p.version, "url": p.url, "bytes": p.bytes,
                    "sha256": p.sha256, "published": p.published, "why": Value::Null,
                }),
                warning,
            )
        }
        Ok(None) => {
            out!(sink, "Nothing hosted yet: this would be the first published download.");
            (
                json!({
                    "reachable": true, "version": Value::Null, "url": Value::Null, "bytes": Value::Null,
                    "sha256": Value::Null, "published": Value::Null, "why": Value::Null,
                }),
                None,
            )
        }
        Err(e) => {
            out!(sink, "Could not be read: {e}");
            (
                json!({
                    "reachable": false, "version": Value::Null, "url": Value::Null, "bytes": Value::Null,
                    "sha256": Value::Null, "published": Value::Null, "why": e.to_string(),
                }),
                None,
            )
        }
    }
}

/// A warning when `version` equals the version `hosted` already names: the
/// one case the brief calls out. Unlike `launcher.rs`'s own
/// `version_warning`, this does not compare "newer" -- `download.json`
/// carries no self-update logic a player's launcher reads to decide
/// whether to fetch, so the only real hazard is silently replacing the
/// exact label a cached link already points at with different bytes.
fn hosted_version_warning(version: &str, hosted: &DownloadPointer) -> Option<String> {
    if hosted.version != version {
        return None;
    }
    Some(format!(
        "The bucket already serves version {} at {}. Building {version} would replace it under the exact \
         same label, so anything already pointing at that link (a cached page, a player mid-download) \
         would silently start getting different bytes under the same name. Pick a different version, or \
         continue only if replacing it under this exact label is deliberate.",
        hosted.version, hosted.url
    ))
}

// ---------------------------------------------------------------------------
// set_run
// ---------------------------------------------------------------------------

/// Run: build the full set for real (or describe it under `dry_run`,
/// touching nothing), verify the zip it produced holds exactly the
/// expected file set, upload the zip, then write and upload
/// `launcher/download.json` -- always after the zip, never before. See the
/// module doc's "Upload order".
#[tauri::command(rename_all = "snake_case")]
pub fn set_run(bundle: String, runtime: String, textures: String, prompt_packages: String, version: String, dry_run: bool) -> Result<Value, String> {
    let inputs = parse_inputs(&bundle, &runtime, &textures, &prompt_packages, &version)?;
    let (platform, settings) = settings_and_platform()?;
    let public_base = settings.public_base.clone();
    let builder = builder_path(&settings);

    // Resolved up front, before a twenty-minute build: a build that cannot
    // be uploaded afterward would waste that time for nothing recoverable.
    // Same reasoning as `launcher.rs`'s own `launcher_run`.
    let bucket = if dry_run { None } else { Some(bucket_from(&platform, &settings)?) };

    let record_inputs = json!({
        "kind": "set-run",
        "version": inputs.version,
        "bundle": inputs.bundle.display().to_string(),
        "runtime": inputs.runtime.display().to_string(),
        "textures": inputs.textures.display().to_string(),
        "prompt_packages": inputs.prompt_packages.display().to_string(),
        "dry_run": dry_run,
    });

    ops::run_operation("set:run", dry_run, record_inputs, move |sink: Sink<'_>| -> reborn_core::Result<Outcome<Value>> {
        if let Err(missing) = inputs.check_dirs() {
            return Ok(refuse(sink, &missing));
        }
        let repo = match repo_root() {
            Ok(r) => r,
            Err(e) => return Ok(refuse(sink, &[e])),
        };
        if let Err(e) = staging_guard(&inputs.bundle, &repo) {
            return Ok(refuse(sink, &[e]));
        }

        let http = UreqHttp::new(&user_agent());
        if let Ok(Some(hosted)) = download::read_hosted(&http, &public_base) {
            if let Some(w) = hosted_version_warning(&inputs.version, &hosted) {
                out!(sink, "WARNING: {w}");
            }
        }

        let script = repo.join("installer").join("windows").join("build.ps1");
        let dist = repo.join("dist");
        let prefix = zip_prefix(&inputs.version);
        let expected_zip = dist.join(zip_name(&inputs.version));

        let mut args: Vec<OsString> = vec![
            OsString::from("-Version"),
            OsString::from(&inputs.version),
            OsString::from("-GamePatch"),
            inputs.bundle.as_os_str().to_os_string(),
            OsString::from("-Runtime"),
            inputs.runtime.as_os_str().to_os_string(),
            OsString::from("-Textures"),
            inputs.textures.as_os_str().to_os_string(),
            OsString::from("-PromptPackages"),
            inputs.prompt_packages.as_os_str().to_os_string(),
            OsString::from("-OfflineWebView2"),
            OsString::from("-OfflineSuffix"),
            OsString::from(""),
        ];
        if let Some(b) = &builder {
            args.push(OsString::from("-Builder"));
            args.push(b.as_os_str().to_os_string());
        }

        if dry_run {
            out!(sink, "Dry run: build.ps1 was not executed and nothing was uploaded.");
            out!(sink, "Would run:    powershell.exe -File \"{}\" {}", script.display(), display_args(&args));
            out!(sink);
            out!(
                sink,
                "Would then confirm {} holds exactly {prefix}.exe plus its own numbered slices, upload it, \
                 then write and upload {} last.",
                expected_zip.display(),
                download::DOWNLOAD_FILE
            );
            let summary = format!(
                "Described: would build {} and, if the zip matches exactly, upload it and point the download \
                 link at it. Nothing was built or uploaded (dry run).",
                inputs.version
            );
            return Ok(Outcome::Done(json!({"summary": summary, "dry_run": true, "version": inputs.version})));
        }

        let cancel = CancelSignal::new();
        ops::set_cancel_signal(cancel.clone());

        out!(sink, "Running       powershell.exe -File {} {}", script.display(), display_args(&args));
        out!(sink, "This takes about twenty minutes; every line build.ps1 prints streams in below.");
        let build_outcome = run_build(&script, &repo, &args, &cancel, sink)?;
        let status = match build_outcome {
            BuildOutcome::Cancelled => {
                return Ok(refuse(
                    sink,
                    &["Cancelled by the operator. build.ps1 was stopped partway through; dist\\ and \
                       target\\installer-payload may hold a partial build. Nothing was uploaded."
                        .to_string()],
                ));
            }
            BuildOutcome::Finished(status) => status,
        };
        if !status.success() {
            return Ok(refuse(
                sink,
                &[format!(
                    "build.ps1 exited with status {}. Nothing was uploaded; see the log above for what it \
                     printed.",
                    status.code().map(|c| c.to_string()).unwrap_or_else(|| "unknown".to_string())
                )],
            ));
        }

        if !expected_zip.is_file() {
            return Ok(refuse(
                sink,
                &[format!(
                    "build.ps1 exited successfully, but {} was not produced. A full set always spans more \
                     than one file, so a zip should always exist here; nothing was uploaded.",
                    expected_zip.display()
                )],
            ));
        }

        out!(sink);
        out!(sink, "Checking the zip holds exactly the expected file set before uploading it.");
        let expected = exact_set_in_dist(&dist, &inputs.version)?;
        if let Err(e) = verify_zip_contents(&expected_zip, &expected) {
            return Ok(refuse(sink, &[e]));
        }
        out!(sink, "Confirmed     {} holds exactly {} file(s): {prefix}.exe plus its own slices.", expected_zip.display(), expected.len());

        let Some(bucket) = bucket.as_ref() else {
            return Ok(refuse(sink, &["No bucket was resolved for this real publish. Nothing was uploaded.".to_string()]));
        };

        out!(sink, "Hashing       {}", expected_zip.display());
        let zip_bytes = std::fs::metadata(&expected_zip).map_err(|e| plain_err(format!("{}: {e}", expected_zip.display())))?.len();
        let zip_sha = hash_file(&expected_zip)?;
        out!(sink, "  {} byte(s), sha256 {}", thousands(zip_bytes), zip_sha.as_str());

        let pointer = download::pointer_for(&expected_zip, &inputs.version, &public_base, SystemTime::now())?;
        let pointer_bytes = serde_json::to_vec_pretty(&pointer)
            .map_err(|e| plain_err(format!("{} would not serialize as JSON: {e}", download::DOWNLOAD_FILE)))?;
        let pointer_path = dist.join("download.json");
        std::fs::write(&pointer_path, &pointer_bytes).map_err(|e| plain_err(format!("{}: {e}", pointer_path.display())))?;
        let pointer_sha = Sha256Hex::from_bytes(&pointer_bytes);
        out!(sink, "Wrote         {} locally (not uploaded yet)", pointer_path.display());

        let zip_key = format!("launcher/{}", zip_name(&inputs.version));
        let mut plan = UploadPlan::new();
        // The zip first, download.json last: see the module doc's "Upload
        // order". `plan.blob` and `plan.release_file` are the same two
        // calls `launcher.rs` uses for its exe and `latest.json`, for the
        // identical reason.
        plan.blob(zip_key.clone(), expected_zip.clone(), zip_bytes, zip_sha.clone());
        plan.release_file(download::DOWNLOAD_FILE.to_string(), pointer_path.clone(), pointer_bytes.len() as u64, pointer_sha.clone());

        let transport = UreqTransport::new();
        let upload_req = UploadRequest {
            bucket,
            plan: &plan,
            multipart_threshold: upload::DEFAULT_MULTIPART_THRESHOLD,
            part_size: upload::DEFAULT_PART_SIZE,
            attempts: upload::DEFAULT_ATTEMPTS,
            // `download.json`'s key never matches what `upload::upload`'s
            // own automatic read-back checks for; see the module doc.
            // `set_verify` is this page's own dedicated read-back.
            public_base: None,
            apply: true,
        };
        out!(sink);
        out!(sink, "Uploading the zip, then download.json.");
        let uploaded = match upload::upload(&transport, &http, &upload_req, sink)? {
            Outcome::Refused(r) => return Ok(Outcome::Refused(r)),
            Outcome::Described => Uploaded::default(),
            Outcome::Done(u) => u,
        };

        let summary = format!(
            "Published the full set {}: {} object(s), {} byte(s) uploaded. Download link: {}",
            inputs.version, uploaded.objects, uploaded.bytes, pointer.url
        );
        Ok(Outcome::Done(json!({
            "summary": summary,
            "version": inputs.version,
            "hashes": [["zip", zip_sha.as_str()], ["download.json", pointer_sha.as_str()]],
            "uploaded": [zip_key, download::DOWNLOAD_FILE],
            "download_url": pointer.url,
        })))
    })
    .map(|()| json!({"accepted": true}))
}

/// `args` as a person would type them at a prompt: quoted only where a
/// space (or emptiness) would otherwise make the boundary ambiguous. For
/// the dry-run "Would run" line only; the real invocation never goes
/// through a shell (see [`run_build`]), so this never has to be exact,
/// only readable.
fn display_args(args: &[OsString]) -> String {
    args.iter()
        .map(|a| {
            let s = a.to_string_lossy();
            if s.is_empty() || s.contains(' ') {
                format!("\"{s}\"")
            } else {
                s.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

// ---------------------------------------------------------------------------
// set_verify
// ---------------------------------------------------------------------------

/// Verify: read `download.json` back over the public host, HEAD the zip it
/// names (headers only -- see the module doc), and compare both against
/// the local zip this page's last real run left in `dist`.
#[tauri::command(rename_all = "snake_case")]
pub fn set_verify() -> Result<Value, String> {
    let (_platform, settings) = settings_and_platform()?;
    let public_base = settings.public_base.clone();
    let origin = PathBuf::from(&settings.origin);

    let record_inputs = json!({"kind": "set-verify"});

    ops::run_operation("set:verify", false, record_inputs, move |sink: Sink<'_>| -> reborn_core::Result<Outcome<Value>> {
        let all = journal::read_all(&origin)?;
        let last = all.records.iter().rev().find(|r| {
            r.operation == "set"
                && r.outcome == "done"
                && r.inputs.get("kind").and_then(|v| v.as_str()) == Some("set-run")
                && r.inputs.get("dry_run").and_then(|v| v.as_bool()) == Some(false)
        });
        let Some(record) = last else {
            return Ok(refuse(
                sink,
                &["No real (non-dry-run) publish of the full set has been run yet in this origin's journal. \
                   Nothing to verify."
                    .to_string()],
            ));
        };
        let version = record.inputs.get("version").and_then(|v| v.as_str()).unwrap_or_default().to_string();
        if version.is_empty() {
            return Ok(refuse(sink, &["The last set:run journal record carries no version. Nothing to verify.".to_string()]));
        }

        let repo = match repo_root() {
            Ok(r) => r,
            Err(e) => return Ok(refuse(sink, &[e])),
        };
        let zip_path = repo.join("dist").join(zip_name(&version));

        out!(sink, "Checking      {}", download::DOWNLOAD_FILE);
        let http = UreqHttp::new(&user_agent());
        let pointer = match download::read_hosted(&http, &public_base) {
            Ok(Some(p)) => p,
            Ok(None) => return Ok(refuse(sink, &["No download.json has been published yet. Nothing to verify.".to_string()])),
            Err(e) => return Ok(refuse(sink, &[format!("{} could not be read: {e}", download::DOWNLOAD_FILE)])),
        };
        if pointer.version != version {
            return Ok(refuse(
                sink,
                &[format!(
                    "The hosted download pointer names version {}, not the {version} this page's last real \
                     run published. The upload may not have completed, or something newer has replaced it.",
                    pointer.version
                )],
            ));
        }
        out!(sink, "Hosted        version {}, {} byte(s), sha256 {}, published {}", pointer.version, pointer.bytes, pointer.sha256, pointer.published);

        if !zip_path.is_file() {
            return Ok(refuse(
                sink,
                &[format!(
                    "{} is not on disk any more, so its bytes cannot be compared against the pointer. \
                     Nothing else here can be checked.",
                    zip_path.display()
                )],
            ));
        }
        let local_bytes = std::fs::metadata(&zip_path).map_err(|e| plain_err(format!("{}: {e}", zip_path.display())))?.len();
        let local_sha = hash_file(&zip_path)?;
        if local_bytes != pointer.bytes || local_sha.as_str() != pointer.sha256 {
            return Ok(refuse(
                sink,
                &[format!(
                    "{} is {local_bytes} byte(s), sha256 {}, but the published pointer names {} byte(s), \
                     sha256 {}. They do not match.",
                    zip_path.display(),
                    local_sha.as_str(),
                    pointer.bytes,
                    pointer.sha256
                )],
            ));
        }
        out!(sink, "Local zip     matches the pointer: {} byte(s), sha256 {}", local_bytes, local_sha.as_str());

        out!(sink, "Checking      {} over the public host (a HEAD request; no body transferred)", pointer.url);
        let head = http.head(&pointer.url).map_err(|e| plain_err(format!("{}: {e}", pointer.url)))?;
        if !head.is_success() {
            return Ok(refuse(sink, &[format!("{} answered {} on a HEAD request. The zip is not reachable there.", pointer.url, head.status)]));
        }
        match head.content_length {
            Some(len) if len == pointer.bytes => {
                out!(sink, "Confirmed     {} answers {}, Content-Length {} byte(s), matching the pointer.", pointer.url, head.status, len);
            }
            Some(len) => {
                return Ok(refuse(
                    sink,
                    &[format!(
                        "{} answers Content-Length {len} byte(s), but the pointer names {} byte(s). The edge \
                         may be serving something else.",
                        pointer.url, pointer.bytes
                    )],
                ));
            }
            None => {
                out!(
                    sink,
                    "{} answered {} with no Content-Length header; size could not be cross-checked over the \
                     network, only locally.",
                    pointer.url,
                    head.status
                );
            }
        }

        let summary = format!(
            "Confirmed: {} names version {version}, {} byte(s), sha256 {}, matching the local zip and \
             reachable at {}. This is the link the site will render.",
            download::DOWNLOAD_FILE,
            pointer.bytes,
            pointer.sha256,
            pointer.url
        );
        out!(sink, "{summary}");
        Ok(Outcome::Done(json!({"summary": summary, "verified": summary, "version": version, "download_url": pointer.url})))
    })
    .map(|()| json!({"accepted": true}))
}

// ---------------------------------------------------------------------------
// Streaming build.ps1
// ---------------------------------------------------------------------------

/// How [`run_build`] ended. See `launcher.rs`'s own copy for the full
/// reasoning; this is the identical shape.
enum BuildOutcome {
    Finished(ExitStatus),
    Cancelled,
}

/// Run `powershell.exe -File <script> <args...>`, streaming every line it
/// prints into `sink` as it is produced, and stopping it early if `cancel`
/// is signalled. See this module's doc, "Why this does not go through
/// `ProcessHost`".
fn run_build(script: &Path, repo: &Path, args: &[OsString], cancel: &CancelSignal, sink: Sink<'_>) -> reborn_core::Result<BuildOutcome> {
    let mut cmd = Command::new("powershell.exe");
    cmd.arg("-NoProfile").arg("-ExecutionPolicy").arg("Bypass").arg("-File").arg(script);
    for a in args {
        cmd.arg(a);
    }
    cmd.current_dir(repo);
    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| plain_err(format!("{}: {e}", script.display())))?;
    let stdout = child.stdout.take().expect("stdout was requested as piped");
    let stderr = child.stderr.take().expect("stderr was requested as piped");

    let (tx, rx) = mpsc::channel::<Event>();
    let tx_err = tx.clone();
    let out_handle = thread::spawn(move || {
        for line in std::io::BufReader::new(stdout).lines().map_while(|l| l.ok()) {
            if tx.send(Event::out(line)).is_err() {
                break;
            }
        }
    });
    let err_handle = thread::spawn(move || {
        for line in std::io::BufReader::new(stderr).lines().map_while(|l| l.ok()) {
            if tx_err.send(Event::err(line)).is_err() {
                break;
            }
        }
    });

    let mut cancelled = false;
    loop {
        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(event) => sink(event),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if cancel.is_cancelled() {
                    cancelled = true;
                    let _ = child.kill();
                    break;
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    // Whatever the two reader threads already had in flight when the loop
    // above stopped -- ordinary completion or a cancel -- drains here
    // rather than being lost.
    while let Ok(event) = rx.recv_timeout(Duration::from_millis(200)) {
        sink(event);
    }
    let _ = out_handle.join();
    let _ = err_handle.join();

    if cancelled {
        let _ = child.wait();
        return Ok(BuildOutcome::Cancelled);
    }
    let status = child.wait().map_err(|e| plain_err(format!("{}: {e}", script.display())))?;
    Ok(BuildOutcome::Finished(status))
}

// ---------------------------------------------------------------------------
// Shared plumbing
// ---------------------------------------------------------------------------

fn settings_and_platform() -> Result<(Platform, Settings), String> {
    let platform = Platform::detect();
    let path = settings::settings_path(&platform)?;
    let settings = Settings::load(&path)?;
    Ok((platform, settings))
}

fn user_agent() -> String {
    format!("reborn-publisher/{}", env!("CARGO_PKG_VERSION"))
}

/// The repository root, found by walking up from this process's own
/// executable looking for `installer\windows\build.ps1`. Same technique,
/// duplicated rather than shared, as `launcher.rs`'s own `repo_root`: see
/// that file's doc for why (this app is single-machine, single-operator,
/// and always runs from this repository's own `target\` output).
fn repo_root() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| format!("this app's own executable path could not be read: {e}"))?;
    let mut dir = exe.parent().map(Path::to_path_buf);
    for _ in 0..6 {
        let Some(candidate) = dir.clone() else { break };
        if candidate.join("installer").join("windows").join("build.ps1").is_file() {
            return Ok(candidate);
        }
        dir = candidate.parent().map(Path::to_path_buf);
    }
    Err(format!(
        "installer\\windows\\build.ps1 could not be found above {}. This app expects to run from inside \
         this repository's own target\\ output; if it was copied elsewhere, that is why.",
        exe.display()
    ))
}

/// Where `-Builder` should point: the operator's configured path if set,
/// else the same default expression `build.ps1` itself falls back to.
/// Same shape, duplicated, as `launcher.rs`'s own `builder_path`.
fn builder_path(settings: &Settings) -> Option<PathBuf> {
    if let Some(p) = settings.builder.as_deref() {
        let p = p.trim();
        if !p.is_empty() {
            return Some(PathBuf::from(p));
        }
    }
    std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("NFL Street Reborn").join("Tools").join("reborn-builder-cli.exe"))
}

/// The R2 bucket this app uploads to, from settings plus the vault. Same
/// shape, duplicated, as every other page's own `bucket_from`.
fn bucket_from(platform: &Platform, settings: &Settings) -> Result<Bucket, String> {
    let endpoint = non_empty(&settings.bucket_endpoint).ok_or_else(|| "No bucket endpoint is configured. Set one in Settings before publishing.".to_string())?;
    let bucket_name = non_empty(&settings.bucket_name).ok_or_else(|| "No bucket name is configured. Set one in Settings before publishing.".to_string())?;
    let access_key_id = vault::r2_access(platform)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "No R2 access key is stored in the vault. Store one on the Keys page before publishing.".to_string())?;
    let secret_access_key = vault::r2_secret(platform)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "No R2 secret key is stored in the vault. Store one on the Keys page before publishing.".to_string())?;
    Ok(Bucket { endpoint, bucket: bucket_name, region: "auto".to_string(), access_key_id, secret_access_key })
}

fn non_empty(s: &Option<String>) -> Option<String> {
    s.as_ref().map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

fn plain_err(message: impl Into<String>) -> reborn_core::Error {
    reborn_core::Error::PlainIo(std::io::Error::new(std::io::ErrorKind::Other, message.into()))
}

#[cfg(test)]
mod tests {
    //! Two things here are genuinely novel logic, not glue over an
    //! already-tested `reborn_core` function, so they are what this file's
    //! own tests target: the exact-name rules ([`is_slice_file`],
    //! [`exact_set_in_dist`], [`verify_zip_contents`]) that stand in for
    //! the wildcard that once swept another build's slices into a bad zip,
    //! and [`run_build`], proven here against a fast fake script rather
    //! than the real, twenty-minute `build.ps1` -- the same approach
    //! `launcher.rs`'s own tests take for its shorter build.

    use super::*;
    #[cfg(windows)]
    use std::time::Instant;

    #[cfg(windows)]
    fn write_script(dir: &Path, name: &str, body: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, body).unwrap();
        path
    }

    // -- naming rules --------------------------------------------------

    #[test]
    fn is_slice_file_matches_only_digits_after_the_exact_prefix() {
        let prefix = "NFL Street Reborn Launcher Setup 3.0.0";
        assert!(is_slice_file("NFL Street Reborn Launcher Setup 3.0.0-1.bin", prefix));
        assert!(is_slice_file("NFL Street Reborn Launcher Setup 3.0.0-42.bin", prefix));
        // The exact bug this rule exists to prevent: a similarly-prefixed
        // build from a different (onboarding, or beta) label must never
        // match.
        assert!(!is_slice_file("NFL Street Reborn Launcher Setup 3.0.0-onboarding.1-1.bin", prefix));
        assert!(!is_slice_file("NFL Street Reborn Launcher Setup 3.0.1-1.bin", prefix));
        assert!(!is_slice_file("NFL Street Reborn Launcher Setup 3.0.0.exe", prefix));
        assert!(!is_slice_file("NFL Street Reborn Launcher Setup 3.0.0-.bin", prefix));
    }

    #[test]
    fn exact_set_in_dist_lists_only_the_exe_and_its_own_slices() {
        let dir = tempfile::tempdir().unwrap();
        let dist = dir.path();
        std::fs::write(dist.join("NFL Street Reborn Launcher Setup 3.0.0.exe"), b"exe").unwrap();
        std::fs::write(dist.join("NFL Street Reborn Launcher Setup 3.0.0-1.bin"), b"aa").unwrap();
        std::fs::write(dist.join("NFL Street Reborn Launcher Setup 3.0.0-2.bin"), b"bbb").unwrap();
        // A stray file from a DIFFERENT build that must never be swept in.
        std::fs::write(dist.join("NFL Street Reborn Launcher Setup 3.0.0-onboarding.1-1.bin"), b"stray").unwrap();
        std::fs::write(dist.join("latest.json"), b"{}").unwrap();

        let found = exact_set_in_dist(dist, "3.0.0").unwrap();
        let names: Vec<&str> = found.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "NFL Street Reborn Launcher Setup 3.0.0-1.bin",
                "NFL Street Reborn Launcher Setup 3.0.0-2.bin",
                "NFL Street Reborn Launcher Setup 3.0.0.exe",
            ]
        );
        assert_eq!(found.iter().find(|m| m.name.ends_with("-1.bin")).unwrap().bytes, 2);
    }

    #[test]
    fn exact_set_in_dist_is_empty_before_anything_is_built() {
        let dir = tempfile::tempdir().unwrap();
        let found = exact_set_in_dist(&dir.path().join("dist-does-not-exist"), "3.0.0").unwrap();
        assert!(found.is_empty());
    }

    // -- the staging guard -----------------------------------------------

    #[test]
    fn staging_guard_refuses_a_bundle_inside_the_payload_folder() {
        let repo = tempfile::tempdir().unwrap();
        let payload = repo.path().join("target").join("installer-payload");
        std::fs::create_dir_all(&payload).unwrap();
        let bundle = payload.join("game-import-bundle");
        let err = staging_guard(&bundle, repo.path()).unwrap_err();
        assert!(err.contains("installer-payload"), "{err}");
        assert!(err.contains("Nothing was built"), "{err}");
    }

    #[test]
    fn staging_guard_allows_a_bundle_anywhere_else() {
        let repo = tempfile::tempdir().unwrap();
        let bundle = tempfile::tempdir().unwrap();
        staging_guard(bundle.path(), repo.path()).unwrap();
    }

    // -- verify_zip_contents ------------------------------------------

    fn write_zip(path: &Path, entries: &[(&str, &[u8])]) {
        let file = std::fs::File::create(path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        for (name, bytes) in entries {
            writer.start_file(*name, options).unwrap();
            std::io::Write::write_all(&mut writer, bytes).unwrap();
        }
        writer.finish().unwrap();
    }

    #[test]
    fn verify_zip_contents_accepts_exactly_the_expected_set() {
        let dir = tempfile::tempdir().unwrap();
        let zip_path = dir.path().join("set.zip");
        write_zip(&zip_path, &[("a.exe", b"exe"), ("a-1.bin", b"bb")]);
        let expected = vec![
            ZipMember { name: "a.exe".to_string(), bytes: 3 },
            ZipMember { name: "a-1.bin".to_string(), bytes: 2 },
        ];
        verify_zip_contents(&zip_path, &expected).unwrap();
    }

    #[test]
    fn verify_zip_contents_refuses_an_extra_file() {
        let dir = tempfile::tempdir().unwrap();
        let zip_path = dir.path().join("set.zip");
        write_zip(&zip_path, &[("a.exe", b"exe"), ("a-1.bin", b"bb"), ("a-1.bin.from-another-build", b"x")]);
        let expected = vec![ZipMember { name: "a.exe".to_string(), bytes: 3 }, ZipMember { name: "a-1.bin".to_string(), bytes: 2 }];
        let err = verify_zip_contents(&zip_path, &expected).unwrap_err();
        assert!(err.contains("a-1.bin.from-another-build"), "{err}");
    }

    #[test]
    fn verify_zip_contents_refuses_a_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let zip_path = dir.path().join("set.zip");
        write_zip(&zip_path, &[("a.exe", b"exe")]);
        let expected = vec![ZipMember { name: "a.exe".to_string(), bytes: 3 }, ZipMember { name: "a-1.bin".to_string(), bytes: 2 }];
        let err = verify_zip_contents(&zip_path, &expected).unwrap_err();
        assert!(err.contains("a-1.bin"), "{err}");
    }

    // -- run_build, against a fast fake script -----------------------

    #[cfg(windows)] // runs powershell.exe; the publisher only runs on Windows
    #[test]
    fn run_build_streams_both_channels_and_reports_success() {
        let dir = tempfile::tempdir().unwrap();
        let script = write_script(
            dir.path(),
            "fake-build.ps1",
            r#"param([string]$Version, [string]$GamePatch, [string]$Runtime, [string]$Textures, [string]$PromptPackages, [switch]$OfflineWebView2, [string]$OfflineSuffix, [string]$Builder)
Write-Host "line one"
Write-Host "line two"
[Console]::Error.WriteLine("an error line")
exit 0
"#,
        );
        let args: Vec<OsString> = vec![OsString::from("-Version"), OsString::from("9.9.9")];
        let mut lines: Vec<(reborn_core::publish::Channel, String)> = Vec::new();
        let mut sink = |e: Event| lines.push((e.channel, e.line));
        let cancel = CancelSignal::new();

        let outcome = run_build(&script, dir.path(), &args, &cancel, &mut sink).unwrap();

        match outcome {
            BuildOutcome::Finished(status) => assert!(status.success(), "expected success, got {status:?}"),
            BuildOutcome::Cancelled => panic!("should not have been cancelled"),
        }
        let out: Vec<&str> = lines.iter().filter(|(c, _)| *c == reborn_core::publish::Channel::Out).map(|(_, l)| l.as_str()).collect();
        assert!(out.contains(&"line one"), "{lines:?}");
        assert!(out.contains(&"line two"), "{lines:?}");
        assert!(
            lines.iter().any(|(c, l)| *c == reborn_core::publish::Channel::Err && l.contains("an error line")),
            "the stderr line should have arrived on the Err channel: {lines:?}"
        );
    }

    #[cfg(windows)] // runs powershell.exe; the publisher only runs on Windows
    #[test]
    fn run_build_reports_a_non_zero_exit_without_treating_it_as_a_hard_error() {
        let dir = tempfile::tempdir().unwrap();
        let script = write_script(dir.path(), "fake-fail.ps1", "exit 7\n");
        let mut sink = |_e: Event| {};
        let cancel = CancelSignal::new();

        let outcome = run_build(&script, dir.path(), &[], &cancel, &mut sink).unwrap();

        match outcome {
            BuildOutcome::Finished(status) => assert_eq!(status.code(), Some(7)),
            BuildOutcome::Cancelled => panic!("should not have been cancelled"),
        }
    }

    #[cfg(windows)] // runs powershell.exe; the publisher only runs on Windows
    #[test]
    fn run_build_stops_early_when_the_operator_cancels() {
        let dir = tempfile::tempdir().unwrap();
        let script = write_script(dir.path(), "fake-slow.ps1", "Start-Sleep -Seconds 30\n");
        let cancel = CancelSignal::new();
        let canceller_signal = cancel.clone();
        let canceller = thread::spawn(move || {
            thread::sleep(Duration::from_millis(300));
            canceller_signal.cancel();
        });
        let mut sink = |_e: Event| {};

        let started = Instant::now();
        let outcome = run_build(&script, dir.path(), &[], &cancel, &mut sink).unwrap();
        canceller.join().unwrap();

        assert!(matches!(outcome, BuildOutcome::Cancelled), "expected Cancelled");
        assert!(started.elapsed() < Duration::from_secs(20), "cancelling should stop the 30s sleep quickly; took {:?}", started.elapsed());
    }

    // -- version warning --------------------------------------------

    fn sample_pointer(version: &str) -> DownloadPointer {
        DownloadPointer {
            schema: download::DOWNLOAD_SCHEMA.to_string(),
            version: version.to_string(),
            url: "https://files.nflstreetreborn.com/launcher/NFL%20Street%20Reborn%20Launcher%203.0.0.zip".to_string(),
            bytes: 123,
            sha256: "a".repeat(64),
            published: "2025-09-09T15:12:07+00:00".to_string(),
        }
    }

    #[test]
    fn hosted_version_warning_fires_only_on_an_exact_match() {
        assert!(hosted_version_warning("3.0.0", &sample_pointer("3.0.0")).is_some());
        assert!(hosted_version_warning("3.0.1", &sample_pointer("3.0.0")).is_none());
        // Unlike launcher.rs's version_warning, an OLDER version than
        // hosted is not itself a warning case here: only an exact match is.
        assert!(hosted_version_warning("2.9.0", &sample_pointer("3.0.0")).is_none());
    }

    // -- parse_inputs / Inputs -----------------------------------------

    #[test]
    fn parse_inputs_refuses_an_empty_version() {
        let err = parse_inputs("b", "r", "t", "p", "  ").unwrap_err();
        assert!(err.contains("version"), "{err}");
    }

    #[test]
    fn parse_inputs_names_which_folder_is_missing() {
        let err = parse_inputs("", "r", "t", "p", "1.0.0").unwrap_err();
        assert!(err.contains("game-import bundle"), "{err}");
    }

    #[test]
    fn check_dirs_names_every_folder_that_is_not_real() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("real");
        std::fs::create_dir_all(&real).unwrap();
        let missing = dir.path().join("missing");
        let inputs = Inputs {
            bundle: real.clone(),
            runtime: missing.clone(),
            textures: real.clone(),
            prompt_packages: missing.clone(),
            version: "1.0.0".to_string(),
        };
        let err = inputs.check_dirs().unwrap_err();
        assert_eq!(err.len(), 2);
        assert!(err.iter().any(|e| e.contains("runtime tree")), "{err:?}");
        assert!(err.iter().any(|e| e.contains("prompt packages")), "{err:?}");
    }
}
