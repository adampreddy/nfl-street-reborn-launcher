//! The Content page: textures, the runtime and mods, and the runtime lint.
//!
//! These are the bulk components: a 3 GB texture pack, the customized
//! Dolphin runtime, and mods, each published from a folder on this
//! machine into the origin as a new version, then carried into a freshly
//! signed release with [`republish_pinning`]. Every other pin stays where
//! it is; only this one component's version changes.
//!
//! # The runtime lint (read this before touching it)
//!
//! Twice, a runtime folder was published after the launcher had already
//! started it: runtime 1.0.0 and 1.0.1 both shipped carrying
//! `ReShade.log`, `ReShade.log1`, `ReShade.ini.launcher-original`, a
//! launcher-managed `ReShade.ini`, and sometimes `User/` or `Dump/`. Those
//! files went out to every player as part of the component, so every
//! fresh install was then offered a pointless "update" whose only effect
//! was undoing the launcher's own staging. [`scan_runtime_marks`] is the
//! one place that knows the five marks that mean this, and
//! [`lint_gate`] is the one place [`content_plan`] and [`content_run`]
//! both call it from, so the rule cannot drift between Plan and Run. It
//! also runs on its own, synchronously, as [`content_lint_runtime`], so a
//! page can show the operator a folder is dirty before Plan is even
//! clicked.
//!
//! The right source for a runtime publish is the `runtime` folder of a
//! fresh build's payload, never a copy the launcher has pointed at and
//! run.
//!
//! # Plan does its own scan; it does not call [`publish_component`]
//!
//! [`publish_component`] at `apply: false` only *describes* what it would
//! do, as text lines; it does not hand back the file count, byte total, or
//! new-versus-shared blob split as data, because a CLI dry run has no
//! reason to want that as anything but text. This page's Plan needs those
//! numbers as data (to draw a fact table, and to compute what a player
//! already on the previous version would fetch), so [`content_plan`] calls
//! `reborn_core::inventory::scan_component` itself and re-derives the same
//! new/shared blob split `publish_component` computes internally. Run
//! still goes through `publish_component`, which is the only thing here
//! that actually copies a blob into place and re-hashes it after the
//! copy; nothing in this file reimplements that.
//!
//! One consequence: a component tree this large (8,192 texture files) is
//! hashed once for Plan and, separately, again for Run. That duplicates
//! the most expensive step, but it is the same shape every other page in
//! this app already takes (Plan validates fully, Run validates fully
//! again, because the two are separate operator actions with a review in
//! between), and re-hashing is the only way Run can prove the tree is
//! still the tree Plan looked at.
//!
//! # Wiring scan progress into the panel
//!
//! `ScanObserver::progress` takes `&self`, but writing a line needs the
//! mutable [`Sink`] this operation's body already owns for its own lines.
//! [`ContentScanObserver`] resolves that by holding a shared `&RefCell`
//! around the one sink, borrowed only for the instant it takes to push one
//! line; a second, thin closure (`relay`, built at each call site) borrows
//! the same cell for the lines this file prints directly. The two never
//! hold the borrow at the same time, so they never conflict, even though a
//! hash of 8,192 files calls the observer roughly 16,000 times while this
//! file's own lines are printed before and after it.
//!
//! # Textures needs a game id; runtime and mods do not
//!
//! `ComponentId::Textures` has no fixed install root: the same texture
//! bytes serve two different discs at two different paths
//! (`library/textures/GN7E69` offline, `library/textures/FR4N31` online),
//! and mixing them under one root would let one game's assets silently
//! overwrite the other's. [`install_root_for`] asks for a `game_id`
//! parameter that only textures uses; runtime and mods ignore it, since
//! [`reborn_core::inventory::ComponentId::install_root`] already knows
//! theirs.
//!
//! # A first-ever release
//!
//! [`republish_pinning`] carries every other pin forward from the release
//! already served, which means it cannot guess a version or sequence for
//! an origin that has never signed one. [`content_run`] covers that: when
//! [`read_current_release`] finds none, it hands `republish_pinning` the
//! new component's own version as the release version and `1` as the
//! sequence, so the very first thing ever published from this page can
//! also be the very first signed release. A release that already exists
//! is always carried forward at its own next sequence instead; this page
//! never invents a release label for an origin that already has one.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use reborn_core::inventory::{
    scan_component, CancelSignal, ComponentId, ComponentSpec, ScanControl, ScanObserver,
    ScanOutcome, ScanPhase, ScanProgress,
};
use reborn_core::manifest::{Component, Release, Sha256Hex};
use reborn_core::net::UreqHttp;
use reborn_core::origin::{blob_path, RELEASE_FILE, SIGNATURE_FILE};
use reborn_core::publish::component::{publish_component, ComponentRequest};
use reborn_core::publish::journal;
use reborn_core::publish::release::{republish_pinning, CarryForward};
use reborn_core::publish::status::read_status;
use reborn_core::publish::upload::{self, Bucket, UploadPlan, UploadRequest, Uploaded, UreqTransport};
use reborn_core::publish::vault::{r2_access, r2_secret};
use reborn_core::publish::{
    manifest_path, manifest_url, next_sequence, refuse, thousands, Event, Outcome, Sink,
};
use reborn_core::signing::{Ed25519Verifier, TRUSTED_KEYS};
use reborn_core::{DataDir, Platform};

use crate::ops;
use crate::settings::Settings;

/// Components this page publishes. Roster, launcher, game-patch and the
/// rest each have their own page; a name this build knows but that is not
/// one of these three is refused by name rather than silently accepted.
const CONTENT_COMPONENTS: &[ComponentId] = &[ComponentId::Textures, ComponentId::Runtime, ComponentId::Mods];

const OFFLINE_GAME_ID: &str = "GN7E69";
const ONLINE_GAME_ID: &str = "FR4N31";

// ---------------------------------------------------------------------------
// content_plan
// ---------------------------------------------------------------------------

/// Plan: hash `folder`, and show the file count and total size, the
/// release sequence before and after, and what a player already on the
/// previous version of this component would fetch. Writes nothing.
#[tauri::command(rename_all = "snake_case")]
pub fn content_plan(
    component: String,
    folder: String,
    version: String,
    game_id: Option<String>,
) -> Result<Value, String> {
    let id = parse_content_component(&component)?;
    let root = PathBuf::from(folder.trim());
    if root.as_os_str().is_empty() {
        return Err("Choose a folder to publish from first.".to_string());
    }
    let version = version.trim().to_string();
    if version.is_empty() {
        return Err("Choose a version first.".to_string());
    }
    let install_root = install_root_for(id, game_id.as_deref())?;

    let (_platform, settings) = settings_and_platform()?;
    let origin = origin_dir(&settings);

    let inputs = json!({
        "kind": "content-plan",
        "component": id.as_str(),
        "version": version,
        "folder": root.display().to_string(),
    });

    ops::run_operation("content:plan", true, inputs, move |sink: Sink<'_>| -> reborn_core::Result<Outcome<Value>> {
        let cancel = CancelSignal::new();
        ops::set_cancel_signal(cancel.clone());

        if !root.is_dir() {
            return Ok(refuse(sink, &[format!("{} is not a folder. Choose the folder to publish from.", root.display())]));
        }
        if let Some(refusal) = lint_gate(sink, &root, id)? {
            return Ok(refusal);
        }
        if manifest_path(&origin, id, &version).is_file() {
            return Ok(refuse(sink, &[format!(
                "{} already holds a manifest for {} {version}. Choose a version the origin does not \
                 already hold; the History page shows every version it has.",
                origin.display(),
                id.as_str()
            )]));
        }

        let component_sequence = next_sequence(&origin, id)?;
        let spec = ComponentSpec::with_install_root(id, &version, component_sequence, &install_root)?;

        let sink_cell = RefCell::new(sink);
        let mut relay = |event: Event| {
            (*sink_cell.borrow_mut())(event);
        };
        let observer = ContentScanObserver::new(&sink_cell, cancel);

        relay(Event::out(format!("Tree          {}", root.display())));
        relay(Event::out(format!("Component     {} version {version}, sequence {component_sequence}", id.as_str())));
        relay(Event::out(format!("Install root  {install_root}")));
        relay(Event::out(String::new()));
        relay(Event::out("Hashing. Progress is reported below, at most once a second.".to_string()));

        let scan = match scan_component(&root, &spec, &observer)? {
            ScanOutcome::Cancelled { .. } => {
                return Ok(refuse(&mut relay, &["Cancelled while scanning. Nothing was written.".to_string()]));
            }
            ScanOutcome::Complete(scan) => scan,
        };

        let component = scan.component;
        let files = component.files.len() as u64;
        let bytes = component.total_bytes();

        let mut new_files = 0u64;
        let mut new_bytes = 0u64;
        let mut shared_files = 0u64;
        let mut shared_bytes = 0u64;
        for f in &component.files {
            let dst = origin.join(blob_path(&f.sha256));
            match std::fs::metadata(&dst) {
                Ok(m) if m.len() == f.bytes => {
                    shared_files += 1;
                    shared_bytes += f.bytes;
                }
                _ => {
                    new_files += 1;
                    new_bytes += f.bytes;
                }
            }
        }

        let release = read_current_release(&origin)?;
        let pinned = match &release {
            Some(r) => read_pinned_component(&origin, r, id)?,
            None => None,
        };
        let (fetch_bytes, fetch_files, fetch_note) = match &pinned {
            Some(current) => {
                let mut fb = 0u64;
                let mut ff = 0u64;
                for file in &component.files {
                    let unchanged = current.files.iter().any(|f| f.path == file.path && f.sha256 == file.sha256);
                    if !unchanged {
                        fb += file.bytes;
                        ff += 1;
                    }
                }
                (
                    fb,
                    ff,
                    format!(
                        "A player already on {} {} fetches about {} byte(s) across {} file(s).",
                        id.as_str(),
                        current.version,
                        thousands(fb),
                        thousands(ff)
                    ),
                )
            }
            None => (
                bytes,
                files,
                format!(
                    "This is the first published version of {}; a fresh install fetches all {} byte(s).",
                    id.as_str(),
                    thousands(bytes)
                ),
            ),
        };

        let (seq_before, seq_after) = match &release {
            Some(r) => (Some(r.sequence), r.sequence + 1),
            None => (None, 1u64),
        };
        let seq_before_text = seq_before.map(|s| s.to_string()).unwrap_or_else(|| "none yet".to_string());

        relay(Event::out(String::new()));
        relay(Event::out(format!(
            "Blobs         {} new ({} bytes), {} already in the origin ({} bytes)",
            thousands(new_files),
            thousands(new_bytes),
            thousands(shared_files),
            thousands(shared_bytes)
        )));
        relay(Event::out(format!("Release       sequence {seq_before_text} -> {seq_after}")));
        relay(Event::out(fetch_note.clone()));

        let summary = format!(
            "{} {version}: {} file(s), {} byte(s) hashed; {} new blob(s) to publish; release sequence {seq_before_text} -> {seq_after}.",
            id.as_str(),
            thousands(files),
            thousands(bytes),
            thousands(new_files)
        );

        Ok(Outcome::Done(json!({
            "summary": summary,
            "component": id.as_str(),
            "version": version,
            "install_root": install_root,
            "component_sequence": component_sequence,
            "files": files,
            "bytes": bytes,
            "new_blobs": new_files,
            "new_bytes": new_bytes,
            "shared_blobs": shared_files,
            "shared_bytes": shared_bytes,
            "release_sequence_before": seq_before,
            "release_sequence_after": seq_after,
            "player_fetch_bytes": fetch_bytes,
            "player_fetch_files": fetch_files,
            "player_fetch_note": fetch_note,
        })))
    })
    .map(|()| json!({"accepted": true}))
}

// ---------------------------------------------------------------------------
// content_run
// ---------------------------------------------------------------------------

/// Run: publish the component (writing its manifest and any new blobs
/// unless `dry_run`), re-sign the release carrying every other pin
/// forward, and upload. `dry_run` skips the component write, the release
/// write, and the upload; it never reaches the network.
#[tauri::command(rename_all = "snake_case")]
pub fn content_run(
    component: String,
    folder: String,
    version: String,
    dry_run: bool,
    game_id: Option<String>,
) -> Result<Value, String> {
    let id = parse_content_component(&component)?;
    let root = PathBuf::from(folder.trim());
    if root.as_os_str().is_empty() {
        return Err("Choose a folder to publish from first.".to_string());
    }
    let version = version.trim().to_string();
    if version.is_empty() {
        return Err("Choose a version first.".to_string());
    }
    let install_root = install_root_for(id, game_id.as_deref())?;

    let (platform, settings) = settings_and_platform()?;
    let origin = origin_dir(&settings);
    let public_base = settings.public_base.clone();

    // Resolved up front, before anything runs: a publish that hashes
    // thousands of files and only then discovers there is no key or no
    // bucket configured has wasted the operator's time for nothing
    // recoverable.
    let key = if dry_run { None } else { Some(key_path(&settings)?) };
    let bucket = if dry_run { None } else { Some(bucket_from(&platform, &settings)?) };

    let inputs = json!({
        "kind": "content-run",
        "component": id.as_str(),
        "version": version,
        "folder": root.display().to_string(),
        "dry_run": dry_run,
    });

    ops::run_operation("content:run", dry_run, inputs, move |sink: Sink<'_>| -> reborn_core::Result<Outcome<Value>> {
        let cancel = CancelSignal::new();
        ops::set_cancel_signal(cancel.clone());

        if !root.is_dir() {
            return Ok(refuse(sink, &[format!("{} is not a folder.", root.display())]));
        }
        if let Some(refusal) = lint_gate(sink, &root, id)? {
            return Ok(refusal);
        }
        if manifest_path(&origin, id, &version).is_file() {
            return Ok(refuse(sink, &[format!(
                "{} already holds a manifest for {} {version}. Choose a version the origin does not \
                 already hold.",
                origin.display(),
                id.as_str()
            )]));
        }

        let component_sequence = next_sequence(&origin, id)?;
        let spec = ComponentSpec::with_install_root(id, &version, component_sequence, &install_root)?;

        let sink_cell = RefCell::new(sink);
        let mut relay = |event: Event| {
            (*sink_cell.borrow_mut())(event);
        };
        let observer = ContentScanObserver::new(&sink_cell, cancel);

        let req = ComponentRequest { root: &root, spec, origin: &origin, apply: !dry_run };
        let published = match publish_component(&req, &observer, &mut relay)? {
            Outcome::Refused(r) => return Ok(Outcome::Refused(r)),
            Outcome::Described => {
                let summary = format!(
                    "Described: would publish {} {version} from {}, then re-sign the release \
                     (carrying every other pin forward) and upload to {public_base}. Nothing was written.",
                    id.as_str(),
                    root.display()
                );
                return Ok(Outcome::Done(json!({"summary": summary})));
            }
            Outcome::Done(p) => p,
        };

        // From here on, apply was true, which only happens when dry_run is
        // false, which is the only case a key and a bucket were resolved.
        let Some(key) = key.as_ref() else {
            return Ok(refuse(&mut relay, &[
                "No signing key was resolved for this real publish. Nothing was uploaded.".to_string(),
            ]));
        };
        let Some(bucket) = bucket.as_ref() else {
            return Ok(refuse(&mut relay, &[
                "No bucket was resolved for this real publish. Nothing was uploaded.".to_string(),
            ]));
        };

        let release = read_current_release(&origin)?;
        let (release_version, release_sequence): (Option<&str>, Option<u64>) = match &release {
            Some(_) => (None, None),
            // A genuinely fresh origin: nothing has ever been signed here.
            // See the module doc's "A first-ever release".
            None => (Some(version.as_str()), Some(1)),
        };
        let carry = CarryForward {
            origin: &origin,
            id,
            version: &version,
            key: Some(key),
            release_version,
            release_sequence,
            apply: true,
        };
        let signed = match republish_pinning(&carry, &mut relay)? {
            Outcome::Refused(r) => return Ok(Outcome::Refused(r)),
            Outcome::Described => {
                return Ok(refuse(&mut relay, &[
                    "The release was described rather than signed. This usually means the origin's \
                     release.json is missing and no bootstrap version or sequence could be inferred. \
                     Sign a release by hand once, then republish this component."
                        .to_string(),
                ]));
            }
            Outcome::Done(s) => s,
        };

        let transport = UreqTransport::new();
        let plan = build_upload_plan(&origin, id, &version, &transport, bucket)?;
        let http = UreqHttp::new(concat!("reborn-publisher/", env!("CARGO_PKG_VERSION")));
        let upload_req = UploadRequest {
            bucket,
            plan: &plan,
            multipart_threshold: upload::DEFAULT_MULTIPART_THRESHOLD,
            part_size: upload::DEFAULT_PART_SIZE,
            attempts: upload::DEFAULT_ATTEMPTS,
            public_base: Some(&public_base),
            apply: true,
        };
        let uploaded = match upload::upload(&transport, &http, &upload_req, &mut relay)? {
            Outcome::Refused(r) => return Ok(Outcome::Refused(r)),
            Outcome::Described => Uploaded::default(),
            Outcome::Done(u) => u,
        };

        let summary = format!(
            "Published {} {version}: release sequence {}, {} object(s) uploaded ({} byte(s)).",
            id.as_str(),
            signed.sequence,
            uploaded.objects,
            uploaded.bytes
        );
        Ok(Outcome::Done(json!({
            "summary": summary,
            "sequence": signed.sequence,
            "hashes": [[manifest_url(id, &version), published.sha256.to_string()]],
            "uploaded": plan.items().map(|i| i.key.clone()).collect::<Vec<_>>(),
            "verified": uploaded.verified.first().cloned(),
        })))
    })
    .map(|()| json!({"accepted": true}))
}

// ---------------------------------------------------------------------------
// content_verify
// ---------------------------------------------------------------------------

/// Verify: read the bucket back fresh over HTTPS, and confirm it pins the
/// version this page's last real (non-dry-run) publish of `component`
/// signed, at the sequence it signed, with a signature that verifies.
#[tauri::command(rename_all = "snake_case")]
pub fn content_verify(component: String) -> Result<Value, String> {
    let id = parse_content_component(&component)?;
    let (_platform, settings) = settings_and_platform()?;
    let origin = origin_dir(&settings);
    let public_base = settings.public_base.clone();
    let launcher_manifest = settings.launcher_manifest.clone();

    let inputs = json!({"kind": "content-verify", "component": id.as_str()});

    ops::run_operation("content:verify", false, inputs, move |sink: Sink<'_>| -> reborn_core::Result<Outcome<Value>> {
        let all = journal::read_all(&origin)?;
        let last = all.records.iter().rev().find(|r| {
            r.operation == "content"
                && r.outcome == "done"
                && r.inputs.get("kind").and_then(|v| v.as_str()) == Some("content-run")
                && r.inputs.get("component").and_then(|v| v.as_str()) == Some(id.as_str())
                && r.inputs.get("dry_run").and_then(|v| v.as_bool()) == Some(false)
        });
        let Some(record) = last else {
            return Ok(refuse(sink, &[format!(
                "No real (non-dry-run) publish of {} has been run yet in this origin's journal. Nothing to verify.",
                id.as_str()
            )]));
        };
        let Some(sequence) = record.sequence else {
            return Ok(refuse(sink, &["The last publish's journal record carries no release sequence. Nothing to verify.".to_string()]));
        };
        let version = record.inputs.get("version").and_then(|v| v.as_str()).unwrap_or_default().to_string();

        sink(Event::out(format!("Checking      {public_base} pins {} at {version}, sequence {sequence}", id.as_str())));

        let http = UreqHttp::new(concat!("reborn-publisher/", env!("CARGO_PKG_VERSION")));
        let verifier = Ed25519Verifier::with_keys(TRUSTED_KEYS)?;
        let status = read_status(&http, &origin, &public_base, &launcher_manifest, &verifier);

        let problem_text = || {
            if status.problems.is_empty() {
                "no reason was given".to_string()
            } else {
                status.problems.join("; ")
            }
        };

        let Some(hosted) = &status.hosted else {
            return Ok(refuse(sink, &[format!("The hosted release could not be read: {}", problem_text())]));
        };
        if hosted.sequence != sequence {
            return Ok(refuse(sink, &[format!(
                "The hosted release is at sequence {}, not the {sequence} this publish signed. The upload may not have completed.",
                hosted.sequence
            )]));
        }
        if !hosted.verified {
            return Ok(refuse(sink, &[format!("The hosted release at sequence {sequence} did not verify: {}", problem_text())]));
        }
        let pinned = hosted.pins.iter().any(|(cid, v)| cid.as_str() == id.as_str() && v == &version);
        if !pinned {
            return Ok(refuse(sink, &[format!("The hosted release does not pin {} at {version}.", id.as_str())]));
        }

        let summary = format!(
            "Confirmed: the hosted release at sequence {sequence} pins {} at {version}, signature verified.",
            id.as_str()
        );
        sink(Event::out(summary.clone()));
        Ok(Outcome::Done(json!({"summary": summary, "verified": summary, "sequence": sequence})))
    })
    .map(|()| json!({"accepted": true}))
}

// ---------------------------------------------------------------------------
// content_lint_runtime
// ---------------------------------------------------------------------------

/// The runtime lint, on its own: a quick, synchronous check of `folder`,
/// so a page can show the operator a folder is dirty before Plan is even
/// offered. [`content_plan`] and [`content_run`] run the exact same check
/// (through [`lint_gate`]) before writing anything; this command exists so
/// the page does not have to run a whole Plan just to ask the question.
#[tauri::command(rename_all = "snake_case")]
pub fn content_lint_runtime(folder: String) -> Result<Value, String> {
    let dir = PathBuf::from(folder.trim());
    if dir.as_os_str().is_empty() {
        return Err("Choose a runtime folder to check first.".to_string());
    }
    if !dir.is_dir() {
        return Err(format!("{} is not a folder.", dir.display()));
    }
    let marks = scan_runtime_marks(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    if marks.is_empty() {
        return Ok(json!({
            "clean": true,
            "offending": Vec::<Value>::new(),
            "message": "No sign this folder has ever been started by the launcher. Safe to publish.",
        }));
    }
    let offending: Vec<Value> =
        marks.iter().map(|m| json!({"path": m.path.display().to_string(), "why": m.why})).collect();
    Ok(json!({
        "clean": false,
        "offending": offending,
        "message": RUNTIME_LINT_REASON,
    }))
}

// ---------------------------------------------------------------------------
// The runtime lint itself
// ---------------------------------------------------------------------------

/// One thing found at the top level of a runtime tree that marks it as
/// already started by the launcher.
struct RuntimeMark {
    path: PathBuf,
    why: &'static str,
}

/// Why these marks refuse a publish, in one sentence: this is the exact
/// fact that runtimes 1.0.0 and 1.0.1 both got wrong.
const RUNTIME_LINT_REASON: &str = "These files and folders appear only after the launcher has already \
     started this exact runtime folder and staged its own configuration into it; runtimes 1.0.0 and \
     1.0.1 both shipped that way, and every fresh install was then offered a pointless update whose \
     only effect was undoing the launcher's own staging.";

/// Checked at the top level of `dir` only, which is where Dolphin and
/// ReShade both write; nothing here descends into `Sys/`, `Qt/`, or any
/// other shipped subfolder.
fn scan_runtime_marks(dir: &Path) -> std::io::Result<Vec<RuntimeMark>> {
    let mut marks = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        let lower = entry.file_name().to_string_lossy().to_ascii_lowercase();

        if file_type.is_dir() {
            if lower == "user" {
                marks.push(RuntimeMark { path, why: "a User folder, which Dolphin creates only once it has actually run" });
            } else if lower == "dump" {
                marks.push(RuntimeMark { path, why: "a Dump folder, which Dolphin creates only once it has actually run" });
            }
            continue;
        }

        if lower.starts_with("reshade.log") {
            marks.push(RuntimeMark { path, why: "a ReShade log, written only once ReShade has actually loaded into a running game" });
        } else if lower == "reshade.ini.launcher-original" {
            marks.push(RuntimeMark {
                path,
                why: "the launcher's own backup of ReShade.ini, written only when the launcher has staged this exact runtime",
            });
        } else if lower == "reshadepreset.ini" {
            let len = entry.metadata().map(|m| m.len()).unwrap_or(0);
            if len > 0 {
                marks.push(RuntimeMark { path, why: "a non-empty ReShade preset, written only once a game session has actually saved one" });
            }
        }
    }
    marks.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(marks)
}

fn runtime_lint_lines(dir: &Path, marks: &[RuntimeMark]) -> Vec<String> {
    let mut lines = vec![format!(
        "{} carries {} mark(s) of a runtime the launcher has already started:",
        dir.display(),
        marks.len()
    )];
    for m in marks {
        lines.push(format!("  {}: {}", m.path.display(), m.why));
    }
    lines.push(RUNTIME_LINT_REASON.to_string());
    lines.push("Publish from the runtime folder of a fresh build's payload instead.".to_string());
    lines
}

/// Run the runtime lint for `id`, and refuse if it finds anything.
/// `None` for a component other than runtime, and for a clean runtime
/// folder: either way, nothing stops the caller from continuing.
fn lint_gate(sink: Sink<'_>, root: &Path, id: ComponentId) -> reborn_core::Result<Option<Outcome<Value>>> {
    if id != ComponentId::Runtime {
        return Ok(None);
    }
    let marks = scan_runtime_marks(root).map_err(|e| plain_err(format!("{}: {e}", root.display())))?;
    if marks.is_empty() {
        return Ok(None);
    }
    Ok(Some(refuse(sink, &runtime_lint_lines(root, &marks))))
}

// ---------------------------------------------------------------------------
// Scan progress into the panel
// ---------------------------------------------------------------------------

/// See the module doc's "Wiring scan progress into the panel".
struct ContentScanObserver<'a, 'b> {
    sink: &'a RefCell<Sink<'b>>,
    cancel: CancelSignal,
    last: Cell<Instant>,
    started: Instant,
}

impl<'a, 'b> ContentScanObserver<'a, 'b> {
    fn new(sink: &'a RefCell<Sink<'b>>, cancel: CancelSignal) -> Self {
        let now = Instant::now();
        Self { sink, cancel, last: Cell::new(now), started: now }
    }

    fn emit(&self, line: String) {
        (*self.sink.borrow_mut())(Event::out(line));
    }
}

impl<'a, 'b> std::fmt::Debug for ContentScanObserver<'a, 'b> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ContentScanObserver").finish()
    }
}

impl<'a, 'b> ScanObserver for ContentScanObserver<'a, 'b> {
    fn progress(&self, progress: &ScanProgress) -> ScanControl {
        if self.cancel.is_cancelled() {
            return ScanControl::Cancel;
        }
        let now = Instant::now();
        if now.duration_since(self.last.get()) >= Duration::from_secs(1) {
            self.last.set(now);
            match progress.phase {
                ScanPhase::Enumerating => {
                    self.emit(format!(
                        "  walking, {} file(s) found so far, {:.0} s",
                        thousands(progress.files_found),
                        self.started.elapsed().as_secs_f64()
                    ));
                }
                ScanPhase::Hashing => {
                    let total = progress.files_total().map(thousands).unwrap_or_else(|| "?".to_string());
                    match progress.fraction() {
                        Some(f) => self.emit(format!(
                            "  hashing {} of {} file(s), {:.0}% of the bytes, {:.0} s",
                            thousands(progress.files_hashed),
                            total,
                            f * 100.0,
                            self.started.elapsed().as_secs_f64()
                        )),
                        None => self.emit(format!(
                            "  hashing {} of {} file(s), {} bytes so far, {:.0} s",
                            thousands(progress.files_hashed),
                            total,
                            thousands(progress.bytes_hashed),
                            self.started.elapsed().as_secs_f64()
                        )),
                    }
                }
            }
        }
        if self.cancel.is_cancelled() {
            ScanControl::Cancel
        } else {
            ScanControl::Continue
        }
    }
}

// ---------------------------------------------------------------------------
// The release: what is pinned now, and what a player would fetch
// ---------------------------------------------------------------------------

/// The signed release in `origin`, or `None` when nothing has been signed
/// there yet.
fn read_current_release(origin: &Path) -> reborn_core::Result<Option<Release>> {
    let release_path = origin.join(RELEASE_FILE);
    if !release_path.is_file() {
        return Ok(None);
    }
    let bytes = std::fs::read(&release_path).map_err(|e| plain_err(format!("{}: {e}", release_path.display())))?;
    Release::parse(&bytes)
        .map(Some)
        .map_err(|e| plain_err(format!("{} does not parse as a release manifest: {e}", release_path.display())))
}

/// The component manifest `release` pins for `id`, read from `origin`.
/// `None` when `release` does not currently pin this component at all.
fn read_pinned_component(origin: &Path, release: &Release, id: ComponentId) -> reborn_core::Result<Option<Component>> {
    let Some(pinned) = release.components.get(id.as_str()) else {
        return Ok(None);
    };
    let manifest_local = origin.join(pinned.url.trim_start_matches('/').replace('/', std::path::MAIN_SEPARATOR_STR));
    let bytes = std::fs::read(&manifest_local).map_err(|e| plain_err(format!("{}: {e}", manifest_local.display())))?;
    Component::parse(&bytes)
        .map(Some)
        .map_err(|e| plain_err(format!("{} does not parse as a component manifest: {e}", manifest_local.display())))
}

// ---------------------------------------------------------------------------
// Upload: exactly what this publish needs
// ---------------------------------------------------------------------------

/// Exactly what publishing `id` `version` needs on the bucket: its
/// manifest, the blobs it names, and the two release files, skipping any
/// the bucket already holds at the right length (checked with a signed
/// HEAD). The two release files are always sent: they change on every
/// publish.
fn build_upload_plan(
    origin: &Path,
    id: ComponentId,
    version: &str,
    transport: &dyn upload::Transport,
    bucket: &Bucket,
) -> reborn_core::Result<UploadPlan> {
    let mut plan = UploadPlan::new();

    let manifest_local = manifest_path(origin, id, version);
    let manifest_bytes = std::fs::read(&manifest_local).map_err(|e| plain_err(format!("{}: {e}", manifest_local.display())))?;
    let component = Component::parse(&manifest_bytes)
        .map_err(|e| plain_err(format!("{} does not parse as a component manifest: {e}", manifest_local.display())))?;

    // One listing, not one HEAD per file: a texture set is eight thousand.
    let (already, _how) = upload::already_on_bucket(transport, bucket)?;
    let manifest_key = manifest_url(id, version);
    if already(&manifest_key) != Some(manifest_bytes.len() as u64) {
        let sha = Sha256Hex::from_bytes(&manifest_bytes);
        plan.manifest(manifest_key, manifest_local, manifest_bytes.len() as u64, sha);
    }
    for file in &component.files {
        let key = blob_path(&file.sha256);
        if already(&key) != Some(file.bytes) {
            let local = origin.join(&key);
            plan.blob(key, local, file.bytes, file.sha256.clone());
        }
    }

    let release_local = origin.join(RELEASE_FILE);
    let release_bytes = std::fs::read(&release_local).map_err(|e| plain_err(format!("{}: {e}", release_local.display())))?;
    plan.release_file(RELEASE_FILE.to_string(), release_local, release_bytes.len() as u64, Sha256Hex::from_bytes(&release_bytes));

    let sig_local = origin.join(SIGNATURE_FILE);
    let sig_bytes = std::fs::read(&sig_local).map_err(|e| plain_err(format!("{}: {e}", sig_local.display())))?;
    plan.release_file(SIGNATURE_FILE.to_string(), sig_local, sig_bytes.len() as u64, Sha256Hex::from_bytes(&sig_bytes));

    Ok(plan)
}


fn plain_err(message: impl Into<String>) -> reborn_core::Error {
    reborn_core::Error::PlainIo(std::io::Error::new(std::io::ErrorKind::Other, message.into()))
}

// ---------------------------------------------------------------------------
// Shared plumbing
// ---------------------------------------------------------------------------

fn parse_content_component(name: &str) -> Result<ComponentId, String> {
    match ComponentId::parse(name) {
        Some(id) if CONTENT_COMPONENTS.contains(&id) => Ok(id),
        Some(other) => Err(format!(
            "{:?} is a component this build knows, but not one this page publishes. Content publishes \
             textures, runtime or mods.",
            other.as_str()
        )),
        None => {
            let names: Vec<&str> = CONTENT_COMPONENTS.iter().map(|c| c.as_str()).collect();
            Err(format!("{name:?} is not a component this build knows. Use one of: {}.", names.join(", ")))
        }
    }
}

/// `library/textures/<game id>` for textures (the only one of the three
/// this page publishes whose root does not follow from its identity
/// alone); runtime and mods ignore `game_id` and use their own fixed root.
fn install_root_for(id: ComponentId, game_id: Option<&str>) -> Result<String, String> {
    match id {
        ComponentId::Textures => {
            let raw = game_id.map(str::trim).filter(|s| !s.is_empty()).unwrap_or(OFFLINE_GAME_ID);
            if raw == OFFLINE_GAME_ID || raw == ONLINE_GAME_ID {
                Ok(format!("{}/{raw}", DataDir::LibraryTextures.relative()))
            } else {
                Err(format!(
                    "{raw:?} is not a game id this page publishes textures for. Use {OFFLINE_GAME_ID} \
                     (offline) or {ONLINE_GAME_ID} (online)."
                ))
            }
        }
        ComponentId::Runtime | ComponentId::Mods => match id.install_root() {
            Some(root) => Ok(root.to_string()),
            None => Err(format!(
                "{} has no fixed install root in this build. This is an internal inconsistency.",
                id.as_str()
            )),
        },
        other => Err(format!("{} is not a component this page publishes.", other.as_str())),
    }
}

fn settings_and_platform() -> Result<(Platform, Settings), String> {
    let platform = Platform::detect();
    let path = crate::settings::settings_path(&platform)?;
    let settings = Settings::load(&path)?;
    Ok((platform, settings))
}

fn origin_dir(settings: &Settings) -> PathBuf {
    PathBuf::from(&settings.origin)
}

fn key_path(settings: &Settings) -> Result<PathBuf, String> {
    match settings.publisher_key.as_deref() {
        Some(p) if !p.trim().is_empty() => Ok(PathBuf::from(p)),
        _ => Err("No publisher key is configured. Set one on the Keys page before publishing.".to_string()),
    }
}

/// The R2 bucket this app uploads to, from settings plus the vault. See
/// `history.rs`'s own copy of this helper for why credentials are read
/// here rather than passed in as a command argument: a `#[tauri::command(rename_all = "snake_case")]`
/// argument is exactly what a browser devtools console in front of this
/// window could set to anything.
fn bucket_from(platform: &Platform, settings: &Settings) -> Result<Bucket, String> {
    let endpoint = non_empty(&settings.bucket_endpoint)
        .ok_or_else(|| "No bucket endpoint is configured. Set one in Settings before publishing.".to_string())?;
    let bucket_name = non_empty(&settings.bucket_name)
        .ok_or_else(|| "No bucket name is configured. Set one in Settings before publishing.".to_string())?;
    let access_key_id = r2_access(platform)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "No R2 access key is stored in the vault. Store one on the Keys page before publishing.".to_string())?;
    let secret_access_key = r2_secret(platform)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "No R2 secret key is stored in the vault. Store one on the Keys page before publishing.".to_string())?;
    Ok(Bucket { endpoint, bucket: bucket_name, region: "auto".to_string(), access_key_id, secret_access_key })
}

fn non_empty(s: &Option<String>) -> Option<String> {
    s.as_ref().map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}
