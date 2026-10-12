//! The Launcher page: build the launcher-only installer and publish it with
//! its update manifest, so a launcher already installed is offered the new
//! version.
//!
//! Three commands, the same Plan/Run/Verify shape every page uses:
//!
//! - [`launcher_plan`]: what the bucket serves now, read the way a launcher
//!   itself reads it (`reborn_core::update::check`), and a warning when the
//!   proposed version would not be newer -- the one thing this page exists
//!   to catch, because a build that is not newer is a build nobody is ever
//!   offered.
//! - [`launcher_run`]: `installer\windows\build.ps1 -LauncherOnly -Version
//!   <v>`, its output streamed into the panel as it prints, then this app's
//!   own SHA-256 of the exe it produced, compared against what the script
//!   wrote into `latest.json` -- never the script's own claim -- then the
//!   exe uploaded before the manifest that names it.
//! - [`launcher_verify`]: reads the manifest and the exe back over HTTPS,
//!   and asks `update::check` the same question a launcher still on the
//!   version this replaced would ask, confirming it is offered the new one.
//!
//! # Why this does not go through `ProcessHost`
//!
//! The brief for this page, and the design doc's section 3.4, both say a
//! child process runs "through the core's `ProcessHost`, output streamed
//! line by line into the log pane." `ProcessHost` (`reborn_core::platform`)
//! cannot do both halves of that at once: `run_capturing` blocks until the
//! child exits and hands back the whole transcript in one piece (see
//! `RealProcessHost::run_capturing` in `platform/imp.rs`, which is a plain
//! `Command::output()`), and `spawn`/`wait` inherit this process's own
//! stdio and hand back no text at all -- `ProcessHandle`'s `Child` is
//! `pub(crate)` to `reborn_core::platform`, out of reach from here. Neither
//! shape can feed the panel while a multi-minute build runs, which this
//! task's brief calls out by name as the trap that already cost time: "The
//! build takes minutes and prints a lot. The panel must stay responsive and
//! show progress throughout, not freeze until it finishes."
//!
//! [`run_build`] resolves that the only way available inside this page's own
//! two files: it spawns `powershell.exe` directly with `std::process::Command`,
//! the same construction `run_capturing` itself uses, with one difference --
//! stdout and stderr are read one line at a time off two reader threads
//! instead of collected whole, so every line reaches the panel within a
//! fraction of a second of the child printing it. This is a deliberate
//! deviation from the letter of "through `ProcessHost`", recorded here and
//! in this task's report rather than left silent, the same way `status.rs`
//! records the Upload button it could not fully wire up. It is not a
//! PowerShell-level redirect: this is the OS pipe `Command` always sets up
//! for a piped child. The trap this task's brief calls out separately --
//! `2>&1` wrapping a native command's stderr in an `ErrorRecord` under
//! Windows PowerShell 5.1 -- happens only when PowerShell's OWN script code
//! applies that redirect to a command it runs; `build.ps1` was already fixed
//! once for exactly that (see its own comment beside the ISCC call) and is
//! invoked here with `-File` and no redirect operator anywhere in its own
//! argument list.
//!
//! # The previous version, for Verify
//!
//! "Verify... asks the core's update check the way a launcher still at the
//! previously hosted version would." By the time Verify runs, the manifest
//! online already names the NEW version -- the old one is gone from the
//! bucket, so it cannot be read back from there. [`launcher_run`] reads it
//! once, synchronously, before the build even starts (a single small GET,
//! not the multi-minute build), and carries it as `previous_version` in the
//! `inputs` this run's journal record is written with, exactly the way
//! `history.rs`'s `rollback_verify` finds its own last rollback by searching
//! the journal for one page's own prior record rather than inventing a
//! second channel to carry it. [`launcher_verify`] searches for that record
//! and reads it back out; a launcher run whose record does not carry one
//! (never run from this page, or run before this field existed) falls back
//! to `"0.0.0"`, which still proves the new version is offered to *some*
//! older launcher, with a line in the log saying that fallback is weaker
//! than the real thing.
//!
//! # Plan warns; it does not refuse
//!
//! The brief says the version-equals-hosted case is a warning, not a
//! refusal, and deliberately so: refusing here would return
//! `Outcome::Refused`, which carries only a summary and no `Value`, and
//! would blank out the rest of the plan (the hosted state, the commit, the
//! Builder path) that the operator still wants to see. So both
//! [`launcher_plan`] and [`launcher_run`] always finish with
//! `Outcome::Done` when nothing has gone actually wrong, carrying a
//! `"warning"` field the operator reads beside everything else, exactly as
//! section 5.4 of the design doc describes.

use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use serde_json::{json, Value};

use reborn_core::inventory::CancelSignal;
use reborn_core::manifest::Sha256Hex;
use reborn_core::net::{Http, UreqHttp};
use reborn_core::publish::journal;
use reborn_core::publish::upload::{self, Bucket, UploadPlan, UploadRequest, Uploaded, UreqTransport};
use reborn_core::publish::vault;
use reborn_core::publish::{out, refuse, Event, Outcome, Sink};
use reborn_core::update::{self, ReleaseManifest};
use reborn_core::Platform;

use crate::ops;
use crate::settings::{self, Settings};

// ---------------------------------------------------------------------------
// launcher_plan
// ---------------------------------------------------------------------------

/// Plan: what the bucket serves now (read the way a launcher itself reads
/// it), the commit and Builder this build would use, and a warning when the
/// proposed version would not be newer than what is already hosted. Writes
/// nothing.
#[tauri::command(rename_all = "snake_case")]
pub fn launcher_plan(version: String) -> Result<Value, String> {
    let version = version.trim().to_string();
    if version.is_empty() {
        return Err(
            "A version is required. Propose one from what the bucket currently serves, or type one."
                .to_string(),
        );
    }
    let settings = load_settings()?;
    let manifest_url = settings.launcher_manifest.clone();
    let repo = repo_root()?;
    let builder = builder_path(&settings);
    let setup_name = format!("NFL Street Reborn Launcher Update {version}.exe");

    let inputs = json!({"kind": "launcher-plan", "version": version});

    ops::run_operation(
        "launcher:plan",
        true,
        inputs,
        move |sink: Sink<'_>| -> reborn_core::Result<Outcome<Value>> {
            let http = UreqHttp::new(&user_agent());
            out!(sink, "Checking      {manifest_url}");
            let hosted = fetch_hosted(&http, &manifest_url);
            match (hosted.reachable, &hosted.version) {
                (true, Some(v)) => out!(sink, "Bucket serves {v} now"),
                (true, None) => out!(sink, "Bucket serves a manifest with no readable version"),
                (false, _) => {
                    out!(sink, "Bucket unreachable: {}", hosted.why.as_deref().unwrap_or("unknown reason"))
                }
            }

            let warning = version_warning(&version, hosted.version.as_deref());
            if let Some(w) = &warning {
                out!(sink, "WARNING: {w}");
            }

            let (commit, dirty) = git_commit(&repo);
            let builder_found = builder.as_deref().map(Path::is_file).unwrap_or(false);
            if let Some(b) = &builder {
                if !builder_found {
                    out!(
                        sink,
                        "Reborn Builder not found at {}: build.ps1 will refuse to build until one is there.",
                        b.display()
                    );
                }
            }

            let summary = match &hosted.version {
                Some(h) => format!("Build {version}. The bucket currently serves {h}."),
                None => format!("Build {version}. The bucket does not currently serve a readable launcher version."),
            };

            Ok(Outcome::Done(json!({
                "summary": summary,
                "proposed_version": version,
                "warning": warning,
                "hosted": {
                    "reachable": hosted.reachable,
                    "version": hosted.version,
                    "bytes": hosted.bytes,
                    "sha256": hosted.sha256,
                    "file_name": hosted.file_name,
                    "why": hosted.why,
                },
                "setup_name": setup_name,
                "manifest_url": manifest_url,
                "commit": commit,
                "dirty": dirty,
                "builder": builder.as_ref().map(|b| json!({"path": b.display().to_string(), "found": builder_found})),
            })))
        },
    )
    .map(|()| json!({"accepted": true}))
}

// ---------------------------------------------------------------------------
// launcher_run
// ---------------------------------------------------------------------------

/// Run: build `installer\windows\build.ps1 -LauncherOnly -Version <v>`,
/// streaming its output into the panel; hash the exe it produced and
/// compare that hash with what the script wrote into `latest.json`; then
/// upload the exe, then the manifest.
///
/// `dry_run` checks nothing and writes nothing: no build, no upload, only a
/// description of what would happen, the same rule `history.rs`'s
/// `rollback_run` follows with `apply: !dry_run`. A real build's minutes are
/// not spent on a version this app already knows would not be newer, or on
/// a bucket this app cannot reach: both the version check and, when this is
/// not a dry run, the R2 credentials are resolved before this call even
/// starts the operation, so a doomed run fails fast rather than after a
/// long compile.
#[tauri::command(rename_all = "snake_case")]
pub fn launcher_run(version: String, dry_run: bool) -> Result<Value, String> {
    let version = version.trim().to_string();
    if version.is_empty() {
        return Err(
            "A version is required. Propose one from what the bucket currently serves, or type one."
                .to_string(),
        );
    }
    let settings = load_settings()?;
    let repo = repo_root()?;
    let manifest_url = settings.launcher_manifest.clone();
    let builder = builder_path(&settings);

    // Checked up front, before anything is built: a build that cannot be
    // uploaded afterward would waste several minutes of compiling for
    // nothing. Same reasoning as `history.rs`'s `bucket_from` call in
    // `rollback_run`.
    let bucket = if dry_run { None } else { Some(bucket_from(&settings)?) };

    let http = UreqHttp::new(&user_agent());
    let previous_version = fetch_hosted(&http, &manifest_url).version;

    let inputs = json!({
        "kind": "launcher-run",
        "version": version,
        "dry_run": dry_run,
        // Read once, here, so `launcher_verify` can find it later. See this
        // module's doc, "The previous version, for Verify".
        "previous_version": previous_version,
    });

    ops::run_operation(
        "launcher:run",
        dry_run,
        inputs,
        move |sink: Sink<'_>| -> reborn_core::Result<Outcome<Value>> {
            if let Some(w) = version_warning(&version, previous_version.as_deref()) {
                out!(sink, "WARNING: {w}");
            }

            let script = repo.join("installer").join("windows").join("build.ps1");
            let dist = repo.join("dist");
            let setup_name = format!("NFL Street Reborn Launcher Update {version}.exe");

            if dry_run {
                let builder_arg =
                    builder.as_ref().map(|b| format!(" -Builder \"{}\"", b.display())).unwrap_or_default();
                out!(sink, "Dry run: build.ps1 was not executed and nothing was uploaded.");
                out!(
                    sink,
                    "Would run:    powershell.exe -File \"{}\" -LauncherOnly -Version {version}{builder_arg}",
                    script.display()
                );
                out!(
                    sink,
                    "Would then hash {} itself, compare it with the latest.json the build writes, and \
                     upload the exe before the manifest.",
                    dist.join(&setup_name).display()
                );
                let summary = format!(
                    "Described: would build {version} and, if its hash matches latest.json, upload it. \
                     Nothing was built or uploaded (dry run)."
                );
                return Ok(Outcome::Done(json!({
                    "summary": summary,
                    "dry_run": true,
                    "proposed_version": version,
                })));
            }

            let cancel = CancelSignal::new();
            ops::set_cancel_signal(cancel.clone());

            out!(
                sink,
                "Running       powershell.exe -File {} -LauncherOnly -Version {version}",
                script.display()
            );
            let build_outcome = run_build(&script, &repo, &version, builder.as_deref(), &cancel, sink)?;
            let status = match build_outcome {
                BuildOutcome::Cancelled => {
                    return Ok(refuse(
                        sink,
                        &["Cancelled by the operator. build.ps1 was stopped partway through; dist\\ may \
                           hold a partial or stale build. Nothing was uploaded."
                            .to_string()],
                    ));
                }
                BuildOutcome::Finished(status) => status,
            };
            if !status.success() {
                return Ok(refuse(
                    sink,
                    &[format!(
                        "build.ps1 exited with status {}. Nothing was uploaded; see the log above for what \
                         it printed.",
                        status.code().map(|c| c.to_string()).unwrap_or_else(|| "unknown".to_string())
                    )],
                ));
            }

            let exe_path = dist.join(&setup_name);
            let manifest_path = dist.join("latest.json");
            if !exe_path.is_file() {
                return Ok(refuse(
                    sink,
                    &[format!(
                        "build.ps1 exited successfully, but {} was not produced. Nothing was uploaded.",
                        exe_path.display()
                    )],
                ));
            }
            if !manifest_path.is_file() {
                return Ok(refuse(
                    sink,
                    &[format!(
                        "build.ps1 exited successfully, but {} was not written. Nothing was uploaded.",
                        manifest_path.display()
                    )],
                ));
            }

            out!(sink, "Hashing       {}", exe_path.display());
            let exe_bytes =
                std::fs::read(&exe_path).map_err(|e| plain_err(format!("{}: {e}", exe_path.display())))?;
            let our_hash = Sha256Hex::from_bytes(&exe_bytes);
            let our_len = exe_bytes.len() as u64;

            let manifest_bytes = std::fs::read(&manifest_path)
                .map_err(|e| plain_err(format!("{}: {e}", manifest_path.display())))?;
            let manifest: ReleaseManifest = match serde_json::from_slice(&manifest_bytes) {
                Ok(m) => m,
                Err(e) => {
                    return Ok(refuse(
                        sink,
                        &[format!(
                            "{} does not parse as a launcher release manifest: {e}",
                            manifest_path.display()
                        )],
                    ))
                }
            };
            if manifest.schema != update::MANIFEST_SCHEMA {
                return Ok(refuse(
                    sink,
                    &[format!(
                        "{} has schema {:?}, this app reads {:?}. Nothing was uploaded.",
                        manifest_path.display(),
                        manifest.schema,
                        update::MANIFEST_SCHEMA
                    )],
                ));
            }
            if manifest.version != version {
                return Ok(refuse(
                    sink,
                    &[format!(
                        "{} names version {}, not {version}. Nothing was uploaded.",
                        manifest_path.display(),
                        manifest.version
                    )],
                ));
            }
            let Some(asset) = manifest.asset_for_this_platform() else {
                return Ok(refuse(
                    sink,
                    &[format!("{} names no Windows asset. Nothing was uploaded.", manifest_path.display())],
                ));
            };

            // Never trust the script's own claim: compare what it wrote
            // against what this app independently computed from the file
            // actually on disk.
            if asset.sha256.as_str() != our_hash.as_str() || asset.bytes != our_len || asset.file_name != setup_name
            {
                return Ok(refuse(
                    sink,
                    &[format!(
                        "This app computed sha256 {} ({our_len} byte(s)) for {}, but {} names sha256 {} \
                         ({} byte(s), file {:?}). The manifest does not match what is actually on disk. \
                         Nothing was uploaded.",
                        our_hash.as_str(),
                        exe_path.display(),
                        manifest_path.display(),
                        asset.sha256.as_str(),
                        asset.bytes,
                        asset.file_name
                    )],
                ));
            }
            out!(
                sink,
                "Confirmed     {} matches latest.json independently: {our_len} byte(s), sha256 {}",
                exe_path.display(),
                our_hash.as_str()
            );

            // The exe first, the manifest last: `UploadPlan` always yields
            // blobs before release files regardless of add order (see
            // `upload.rs`'s own doc), which is what makes this ordering a
            // type-level guarantee rather than a habit to remember.
            let mut plan = UploadPlan::new();
            let exe_key = format!("launcher/{setup_name}");
            plan.blob(exe_key.clone(), exe_path.clone(), our_len, our_hash.clone());

            // Every edition, one version: the Steam Deck AppImage this
            // manifest names goes up with the exe, checked the same way, or
            // nothing does. (3.0.33 went live Windows-only; Decks were
            // offered nothing.) build.ps1 refuses to write a stable manifest
            // without it; this is the second lock.
            let Some(linux) = manifest.linux.clone() else {
                return Ok(refuse(sink, &[format!(
                    "{} names no Steam Deck build. Every edition ships at one version: run \
                     installer/linux/release-deck.sh build {version}, then build again. Nothing was uploaded.",
                    manifest_path.display()
                )]));
            };
            let app_path = PathBuf::from(r"C:\Reborn Origin-stage\launcher-linux").join(&linux.file_name);
            let app_bytes = std::fs::metadata(&app_path).map(|m| m.len()).unwrap_or(0);
            let app_hash = reborn_core::library::hash_file(&app_path).map_err(|e| plain_err(format!("{}: {e}", app_path.display())))?;
            if app_hash.as_str() != linux.sha256.as_str() || app_bytes != linux.bytes {
                return Ok(refuse(sink, &[format!(
                    "{} is sha256 {} ({app_bytes} byte(s)) but latest.json names {} ({} byte(s)). Nothing was uploaded.",
                    app_path.display(), app_hash.as_str(), linux.sha256.as_str(), linux.bytes
                )]));
            }
            out!(sink, "Confirmed     {} matches latest.json independently: {app_bytes} byte(s), sha256 {}", app_path.display(), app_hash.as_str());
            plan.blob(format!("launcher/{}", linux.file_name), app_path.clone(), app_bytes, app_hash.clone());

            // The Mac app the manifest names (a zipped .app, what a Mac's
            // self-update unpacks) goes up the same way, with the .dmg players
            // download beside it. A manifest with no Mac build (build.ps1
            // -NoMac) leaves Macs on the version they have.
            match manifest.macos.clone() {
                Some(macos) => {
                    let stage = PathBuf::from(r"C:\Reborn Origin-stage\launcher-macos");
                    let zip_path = stage.join(&macos.file_name);
                    let zip_bytes = std::fs::metadata(&zip_path).map(|m| m.len()).unwrap_or(0);
                    let zip_hash = reborn_core::library::hash_file(&zip_path).map_err(|e| plain_err(format!("{}: {e}", zip_path.display())))?;
                    if zip_hash.as_str() != macos.sha256.as_str() || zip_bytes != macos.bytes {
                        return Ok(refuse(sink, &[format!(
                            "{} is sha256 {} ({zip_bytes} byte(s)) but latest.json names {} ({} byte(s)). Nothing was uploaded.",
                            zip_path.display(), zip_hash.as_str(), macos.sha256.as_str(), macos.bytes
                        )]));
                    }
                    let dmg_name = format!("{}.dmg", macos.file_name.trim_end_matches(".app.zip"));
                    let dmg_path = stage.join(&dmg_name);
                    let Ok(dmg_meta) = std::fs::metadata(&dmg_path) else {
                        return Ok(refuse(sink, &[format!(
                            "{} is missing: the Mac download goes up with the Mac update. Nothing was uploaded.",
                            dmg_path.display()
                        )]));
                    };
                    let dmg_hash = reborn_core::library::hash_file(&dmg_path).map_err(|e| plain_err(format!("{}: {e}", dmg_path.display())))?;
                    out!(sink, "Confirmed     {} matches latest.json independently: {zip_bytes} byte(s), sha256 {}", zip_path.display(), zip_hash.as_str());
                    plan.blob(format!("launcher/{}", macos.file_name), zip_path.clone(), zip_bytes, zip_hash.clone());
                    plan.blob(format!("launcher/{dmg_name}"), dmg_path.clone(), dmg_meta.len(), dmg_hash.clone());
                }
                None => out!(sink, "No Mac build  latest.json names none, so Macs stay on the version they have"),
            }
            let manifest_key = "launcher/latest.json".to_string();
            let manifest_sha = Sha256Hex::from_bytes(&manifest_bytes);
            plan.release_file(
                manifest_key.clone(),
                manifest_path.clone(),
                manifest_bytes.len() as u64,
                manifest_sha.clone(),
            );

            let transport = UreqTransport::new();
            let http = UreqHttp::new(&user_agent());
            let bucket = bucket.expect("resolved before this operation started for a non-dry run");
            let upload_req = UploadRequest {
                bucket: &bucket,
                plan: &plan,
                multipart_threshold: upload::DEFAULT_MULTIPART_THRESHOLD,
                part_size: upload::DEFAULT_PART_SIZE,
                attempts: upload::DEFAULT_ATTEMPTS,
                // `upload::upload`'s own read-back only ever checks a
                // `Group::Release` item named exactly `release.json`, or any
                // item named bare `latest.json`; ours is keyed
                // `launcher/latest.json`, so that automatic check would
                // always skip. `launcher_verify` is this page's own,
                // dedicated read-back step instead.
                public_base: None,
                apply: true,
            };
            let uploaded = match upload::upload(&transport, &http, &upload_req, sink)? {
                Outcome::Refused(r) => return Ok(Outcome::Refused(r)),
                Outcome::Described => Uploaded::default(),
                Outcome::Done(u) => u,
            };

            let summary = format!(
                "Built and published launcher {version}: {} object(s), {} byte(s) uploaded.",
                uploaded.objects, uploaded.bytes
            );
            Ok(Outcome::Done(json!({
                "summary": summary,
                "version": version,
                "uploaded": [exe_key, manifest_key],
                "hashes": [["launcher-exe", our_hash.as_str()], ["latest.json", manifest_sha.as_str()]],
            })))
        },
    )
    .map(|()| json!({"accepted": true}))
}

// ---------------------------------------------------------------------------
// launcher_verify
// ---------------------------------------------------------------------------

/// Verify: read the manifest back over HTTPS the way a launcher itself
/// does (`update::check`, as a launcher still on the version this build
/// replaced), confirm it names `version`, then download the installer it
/// names (`update::download`, the exact function a real launcher's
/// self-update calls) and confirm that matches too.
#[tauri::command(rename_all = "snake_case")]
pub fn launcher_verify(version: String) -> Result<Value, String> {
    let version = version.trim().to_string();
    if version.is_empty() {
        return Err(
            "A version is required. Propose one from what the bucket currently serves, or type one."
                .to_string(),
        );
    }
    let settings = load_settings()?;
    let manifest_url = settings.launcher_manifest.clone();
    let origin = PathBuf::from(&settings.origin);

    let inputs = json!({"kind": "launcher-verify", "version": version});

    ops::run_operation(
        "launcher:verify",
        false,
        inputs,
        move |sink: Sink<'_>| -> reborn_core::Result<Outcome<Value>> {
            let baseline = find_baseline(&origin, &version, sink)?;

            let http = UreqHttp::new(&user_agent());
            out!(sink, "Checking      {manifest_url}, as a launcher still on {baseline}");
            let check = update::check(&http, &manifest_url, &baseline);

            let asset = match &check {
                update::UpdateCheck::Available { latest, asset, .. } if latest == &version => asset.clone(),
                other => {
                    return Ok(refuse(
                        sink,
                        &[format!(
                            "A launcher on {baseline} was not offered {version} from {manifest_url}: {}",
                            other.explain()
                        )],
                    ));
                }
            };
            out!(sink, "Offered       {version}, {} byte(s), sha256 {}", asset.bytes, asset.sha256.as_str());

            let scratch_dir = std::env::temp_dir().join("reborn-publisher-verify");
            out!(sink, "Downloading   {} to confirm it reads back byte for byte", asset.url);
            match update::download(&http, &asset, &scratch_dir) {
                Ok(downloaded) => {
                    let _ = std::fs::remove_file(&downloaded.path);
                    out!(
                        sink,
                        "Confirmed     {} byte(s), sha256 {}, matches the manifest.",
                        downloaded.bytes,
                        downloaded.sha256.as_str()
                    );
                }
                Err(e) => {
                    return Ok(refuse(
                        sink,
                        &[format!("The installer did not read back as the manifest claims: {e}")],
                    ));
                }
            }

            let summary = format!(
                "Confirmed: {manifest_url} and its installer read back correctly, and a launcher on \
                 {baseline} is offered {version}."
            );
            out!(sink, "{summary}");
            Ok(Outcome::Done(json!({"summary": summary, "verified": summary, "baseline": baseline})))
        },
    )
    .map(|()| json!({"accepted": true}))
}

/// The version [`launcher_run`]'s own journal record, for this exact
/// `version`, found hosted before it built -- see this module's doc, "The
/// previous version, for Verify". Falls back to `"0.0.0"` with a log line
/// explaining the weaker guarantee when no such record exists.
fn find_baseline(origin: &Path, version: &str, sink: Sink<'_>) -> reborn_core::Result<String> {
    let all = journal::read_all(origin)?;
    let found = all.records.iter().rev().find(|r| {
        r.operation == "launcher"
            && r.outcome == "done"
            && r.inputs.get("kind").and_then(|v| v.as_str()) == Some("launcher-run")
            && r.inputs.get("version").and_then(|v| v.as_str()) == Some(version)
    });
    match found.and_then(|r| r.inputs.get("previous_version")).and_then(|v| v.as_str()) {
        Some(b) => Ok(b.to_string()),
        None => {
            out!(
                sink,
                "No launcher:run journal record for {version} names a previous version; using 0.0.0. That \
                 only proves {version} is offered to something, not specifically to a launcher on the \
                 version this replaced."
            );
            Ok("0.0.0".to_string())
        }
    }
}

// ---------------------------------------------------------------------------
// streaming the build
// ---------------------------------------------------------------------------

/// How [`run_build`] ended: the child's own exit status, or the operator's
/// own Cancel button reaching it before it finished.
enum BuildOutcome {
    Finished(ExitStatus),
    Cancelled,
}

/// Run `powershell.exe -File <script> -LauncherOnly -Version <version>
/// [-Builder <builder>]`, streaming every line it prints into `sink` as it
/// is produced, and stopping it early if `cancel` is signalled. See this
/// module's doc, "Why this does not go through `ProcessHost`", for why this
/// spawns the child directly rather than through
/// `reborn_core::platform::ProcessHost`.
fn run_build(
    script: &Path,
    repo: &Path,
    version: &str,
    builder: Option<&Path>,
    cancel: &CancelSignal,
    sink: Sink<'_>,
) -> reborn_core::Result<BuildOutcome> {
    let mut cmd = Command::new("powershell.exe");
    cmd.arg("-NoProfile")
        .arg("-ExecutionPolicy")
        .arg("Bypass")
        .arg("-File")
        .arg(script)
        .arg("-LauncherOnly")
        .arg("-Version")
        .arg(version);
    if let Some(b) = builder {
        cmd.arg("-Builder").arg(b);
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
    // above stopped -- ordinary completion or a cancel -- drains here rather
    // than being lost.
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
// what the bucket serves, and the warning
// ---------------------------------------------------------------------------

/// What `settings.launcher_manifest` currently serves, read the same way a
/// launcher itself would: through [`update::check`], comparing against a
/// baseline low enough (`"0.0.0"`) that any real published version reads as
/// newer. This is [`launcher_plan`]'s and [`launcher_run`]'s own look at
/// "what does the bucket serve now"; [`launcher_verify`] asks the same
/// question from the ACTUAL previous version instead, which is the point of
/// Verify.
struct HostedInfo {
    reachable: bool,
    version: Option<String>,
    bytes: Option<u64>,
    sha256: Option<String>,
    file_name: Option<String>,
    why: Option<String>,
}

fn fetch_hosted(http: &dyn Http, manifest_url: &str) -> HostedInfo {
    match update::check(http, manifest_url, "0.0.0") {
        update::UpdateCheck::Available { latest, asset, .. } => HostedInfo {
            reachable: true,
            version: Some(latest),
            bytes: Some(asset.bytes),
            sha256: Some(asset.sha256.as_str().to_string()),
            file_name: Some(asset.file_name),
            why: None,
        },
        update::UpdateCheck::UpToDate { latest, .. } => {
            HostedInfo { reachable: true, version: Some(latest), bytes: None, sha256: None, file_name: None, why: None }
        }
        update::UpdateCheck::NoAssetForThisPlatform { latest, .. } => HostedInfo {
            reachable: true,
            version: Some(latest),
            bytes: None,
            sha256: None,
            file_name: None,
            why: Some("published, but names no asset for this platform".to_string()),
        },
        update::UpdateCheck::Unreachable { why, .. } | update::UpdateCheck::Unreadable { why, .. } => {
            HostedInfo { reachable: false, version: None, bytes: None, sha256: None, file_name: None, why: Some(why) }
        }
    }
}

/// A warning when `version` would not be newer than `hosted`, the one thing
/// this page exists to catch: "no launcher would be offered anything."
/// `None` when there is nothing hosted yet, or `version` is genuinely newer.
fn version_warning(version: &str, hosted: Option<&str>) -> Option<String> {
    let hosted = hosted?;
    if update::newer(version, hosted) {
        return None;
    }
    Some(if version == hosted {
        format!(
            "The bucket already serves version {hosted}. Building {version} would offer nothing: a \
             launcher already at {hosted} would not be told to update. Pick a version newer than {hosted}."
        )
    } else {
        format!(
            "The bucket serves {hosted}, which {version} is not newer than. Building {version} would offer \
             nothing to a launcher already at {hosted}. Pick a version newer than {hosted}."
        )
    })
}

// ---------------------------------------------------------------------------
// shared plumbing
// ---------------------------------------------------------------------------

fn load_settings() -> Result<Settings, String> {
    let platform = Platform::detect();
    let path = settings::settings_path(&platform)?;
    Settings::load(&path)
}

fn user_agent() -> String {
    format!("reborn-publisher/{}", env!("CARGO_PKG_VERSION"))
}

/// The repository root, found by walking up from this process's own
/// executable looking for `installer\windows\build.ps1`.
///
/// This app is single-operator, single-machine
/// (`docs/superpowers/specs/2026-09-08-reborn-publisher-design.md` section
/// 2) and always runs from this repository's own `target\` output, so the
/// script it needs to drive is always a fixed number of parents above
/// wherever `reborn-publisher.exe` (or a private copy of it, built the same
/// way) sits. A build placed anywhere else is refused by name rather than
/// silently reaching for a stale path.
fn repo_root() -> Result<PathBuf, String> {
    let exe = std::env::current_exe()
        .map_err(|e| format!("this app's own executable path could not be read: {e}"))?;
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
/// else the same default expression `build.ps1` itself falls back to
/// (`%APPDATA%\NFL Street Reborn\Tools\reborn-builder-cli.exe`), computed
/// here and passed explicitly rather than left to the script's own default,
/// so Plan can show it and Run never depends on this process's environment
/// happening to match what the script would have picked on its own.
fn builder_path(settings: &Settings) -> Option<PathBuf> {
    if let Some(p) = settings.builder.as_deref() {
        let p = p.trim();
        if !p.is_empty() {
            return Some(PathBuf::from(p));
        }
    }
    std::env::var_os("APPDATA")
        .map(|a| PathBuf::from(a).join("NFL Street Reborn").join("Tools").join("reborn-builder-cli.exe"))
}

/// The repository's current commit, best-effort. `None` when `git` is not
/// on this machine, this is not a git checkout, or the command otherwise
/// fails; a plan showing no commit is a smaller loss than a plan that
/// cannot be shown at all because a stray machine has no `git` on its PATH.
fn git_commit(repo: &Path) -> (Option<String>, bool) {
    let short = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .current_dir(repo)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty());
    let dirty = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(repo)
        .output()
        .ok()
        .map(|o| o.status.success() && !o.stdout.is_empty())
        .unwrap_or(false);
    (short, dirty)
}

/// The R2 bucket this app uploads to, from settings plus the vault. Same
/// shape as `history.rs`'s own `bucket_from`; duplicated rather than shared,
/// consistent with this crate's per-page files never needing to touch a
/// common module to stay out of each other's way (see `main.rs`'s doc).
fn bucket_from(settings: &Settings) -> Result<Bucket, String> {
    let endpoint = non_empty(&settings.bucket_endpoint).ok_or_else(|| {
        "No bucket endpoint is configured. Set one in Settings before publishing the launcher.".to_string()
    })?;
    let bucket_name = non_empty(&settings.bucket_name).ok_or_else(|| {
        "No bucket name is configured. Set one in Settings before publishing the launcher.".to_string()
    })?;
    let platform = Platform::detect();
    let access_key_id = vault::r2_access(&platform)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| {
            "No R2 access key is stored in the vault. Store one on the Keys page before publishing the \
             launcher."
                .to_string()
        })?;
    let secret_access_key = vault::r2_secret(&platform)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| {
            "No R2 secret key is stored in the vault. Store one on the Keys page before publishing the \
             launcher."
                .to_string()
        })?;
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
    //! `run_build` is the one genuinely novel piece of logic in this file --
    //! everything else is glue over already-tested `reborn_core` functions --
    //! so it is the one thing here worth its own test, run against a tiny
    //! fake script rather than the real, multi-minute `build.ps1`. This is
    //! how this task's own report proves the streaming and cancellation
    //! wiring actually works without spending several minutes, and a
    //! `cargo`/Inno Setup/Reborn-Builder install, on a real build.

    use super::*;
    #[cfg(windows)]
    use reborn_core::publish::Channel;
    #[cfg(windows)]
    use std::time::Instant;

    #[cfg(windows)]
    fn write_script(dir: &Path, name: &str, body: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, body).unwrap();
        path
    }

    #[cfg(windows)] // runs powershell.exe; the publisher only runs on Windows
    #[test]
    fn run_build_streams_both_channels_and_reports_success() {
        let dir = tempfile::tempdir().unwrap();
        let script = write_script(
            dir.path(),
            "fake-build.ps1",
            r#"param([switch]$LauncherOnly, [string]$Version, [string]$Builder)
Write-Host "line one"
Write-Host "line two"
[Console]::Error.WriteLine("an error line")
exit 0
"#,
        );
        let mut lines: Vec<(Channel, String)> = Vec::new();
        let mut sink = |e: Event| lines.push((e.channel, e.line));
        let cancel = CancelSignal::new();

        let outcome = run_build(&script, dir.path(), "9.9.9", None, &cancel, &mut sink).unwrap();

        match outcome {
            BuildOutcome::Finished(status) => assert!(status.success(), "expected success, got {status:?}"),
            BuildOutcome::Cancelled => panic!("should not have been cancelled"),
        }
        let out: Vec<&str> =
            lines.iter().filter(|(c, _)| *c == Channel::Out).map(|(_, l)| l.as_str()).collect();
        assert!(out.contains(&"line one"), "{lines:?}");
        assert!(out.contains(&"line two"), "{lines:?}");
        assert!(
            lines.iter().any(|(c, l)| *c == Channel::Err && l.contains("an error line")),
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

        let outcome = run_build(&script, dir.path(), "1.0.0", None, &cancel, &mut sink).unwrap();

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
        let outcome = run_build(&script, dir.path(), "1.0.0", None, &cancel, &mut sink).unwrap();
        canceller.join().unwrap();

        assert!(matches!(outcome, BuildOutcome::Cancelled), "expected Cancelled");
        assert!(
            started.elapsed() < Duration::from_secs(20),
            "cancelling should stop the 30s sleep quickly; took {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn version_warning_fires_on_equal_or_older_and_stays_silent_when_newer_or_unknown() {
        assert!(version_warning("3.0.1", Some("3.0.1")).is_some(), "equal versions must warn");
        assert!(version_warning("3.0.0", Some("3.0.1")).is_some(), "an older version must warn too");
        assert!(version_warning("3.0.2", Some("3.0.1")).is_none(), "a genuinely newer version must not warn");
        assert!(version_warning("3.0.2", None).is_none(), "nothing hosted yet is not a warning case");
    }
}
