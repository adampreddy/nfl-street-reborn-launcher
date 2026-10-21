//! The History page: the journal as a table, and rollback to an earlier
//! version of a component.
//!
//! Rollback is never a delete. Content on the CDN is content-addressed and
//! a release is signed; going back means signing a NEW release with a
//! HIGHER sequence that pins OLDER component versions. `rollback_plan` and
//! `rollback_run` share [`compute_rollback`], which reads the local
//! origin's currently-signed release, resolves the version every other
//! component is pinned at (never trusted from a file name alone), and
//! diffs the target manifest against what is live now to say how much a
//! player would actually have to fetch. Everything else about "would this
//! work" (the key, the sequence, whether every blob is still present) is
//! answered by [`release`] itself, at `apply: false` for the plan and
//! `apply: !dry_run` for the run: this file does not duplicate that
//! checking.
//!
//! # Why a Plan step never returns `Outcome::Described`
//!
//! `ops::run_operation`'s `finish` only ever populates `OpState.plan` (what
//! the panel shows above the Run button) for a `Step::Plan` whose body
//! returned `Outcome::Done`; a `Described` plan step leaves `state.plan`
//! as `Some(Value::Null)`, which the page draws as "no plan yet". So
//! `rollback_plan`'s body calls `release(apply: false, ..)` purely to
//! reuse its validation (a missing manifest, a missing blob, an unreadable
//! key all refuse with `release`'s own clear reason, satisfying "a
//! rollback to a version the origin no longer holds must refuse with that
//! reason"), then converts a successful `Described` into its own
//! `Outcome::Done` carrying the before/after this page needs to show.
//!
//! # Timestamps
//!
//! [`reborn_core::publish::journal::Record::at`] is stored UTC with an
//! explicit offset, on purpose (see that field's doc). This file passes it
//! through unchanged; `ui/history.js` renders it in the browser's local
//! time zone with `Date`/`Intl`, labelled, which is the only place in this
//! app that can honestly know what "local" means.
//!
//! # Upload: scoped to what this rollback actually changes
//!
//! A rollback pins one OLDER, already-once-published component version;
//! every other pinned component is carried forward unchanged and was
//! already uploaded when it was first published. So `rollback_run` does
//! not run a whole-origin upload plan: it uploads exactly the target
//! manifest, the blobs it names, and the two release files, skipping any
//! of those the bucket already holds at the right length (checked with a
//! signed HEAD, the same "already" shape
//! `reborn_core::publish::upload::plan_origin` uses, built here from the
//! public `sign`/`object_path`/[`upload::Transport`] rather than that
//! whole-origin scanner). The two release files are always sent, matching
//! `plan_origin`'s own rule: they change on every publish.
//!
//! # The R2 credentials
//!
//! Read with `reborn_core::publish::vault::{r2_access, r2_secret}`,
//! the same accessors Task 8's Keys page (built concurrently with this
//! one, and already landed in this tree) uses for `vault_state` and
//! `vault_store`. Until a person has stored both there, `rollback_run`
//! refuses cleanly, naming what is missing and where to store it, rather
//! than failing obscurely partway through a sign. `region` has no
//! setting of its own anywhere in this app; `"auto"`, Cloudflare R2's own
//! region value for an S3-compatible request, is hardcoded in
//! [`bucket_from`].
//!
//! # Why `read_last` is not what reads the journal here
//!
//! The journal is one file per origin, shared by every page (`roster`,
//! `launcher`, `release`, `history`, ...). `journal::read_last(origin, 1)`
//! would return literally the last record written, whichever page wrote
//! it, not "the last rollback". `history_list` and `rollback_verify` both
//! need `journal::read_all` instead: the former to also report
//! `ReadAll::skipped` (which `read_last` computes and discards), the
//! latter to search backward for the last record this page itself wrote.

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use reborn_core::inventory::ComponentId;
use reborn_core::manifest::{Component, Release, Sha256Hex};
use reborn_core::net::UreqHttp;
use reborn_core::origin::{blob_path, RELEASE_FILE, SIGNATURE_FILE};
use reborn_core::publish::journal;
use reborn_core::publish::release::{release, Pin, ReleaseRequest};
use reborn_core::publish::status::read_status;
use reborn_core::publish::upload::{self, Bucket, UploadPlan, UploadRequest, Uploaded, UreqTransport};
use reborn_core::publish::vault::{r2_access, r2_secret};
use reborn_core::publish::{manifest_path, manifest_url, next_sequence, refuse, Event, Outcome, SequenceProblem, Sink, COMPONENTS_DIR};
use reborn_core::signing::{Ed25519Verifier, TRUSTED_KEYS};

use crate::ops;
use crate::settings::Settings;

/// The last `limit` journal records, newest first, plus how many lines of
/// the journal could not be parsed. A journal with unreadable lines means
/// history is incomplete; `skipped` is how the page knows to say so.
#[tauri::command(rename_all = "snake_case")]
pub fn history_list(limit: usize) -> Result<serde_json::Value, String> {
    let (_platform, settings) = settings_and_platform()?;
    let origin = origin_dir(&settings);

    let all = journal::read_all(&origin).map_err(|e| e.to_string())?;
    let start = all.records.len().saturating_sub(limit);
    let records: Vec<Value> = all.records[start..]
        .iter()
        .rev()
        .map(|r| serde_json::to_value(r).unwrap_or(Value::Null))
        .collect();

    Ok(json!({
        "records": records,
        "shown": records.len(),
        "total": all.records.len(),
        "skipped": all.skipped,
    }))
}

/// Every version of one component the origin still holds a manifest for,
/// newest first, with which one (if any) the currently-signed release
/// pins.
#[tauri::command(rename_all = "snake_case")]
pub fn history_versions(component: String) -> Result<serde_json::Value, String> {
    let id = parse_component(&component)?;
    let (_platform, settings) = settings_and_platform()?;
    let origin = origin_dir(&settings);

    // `next_sequence` scans every manifest of this component to find the
    // highest sequence; a corrupt one among them now names itself rather
    // than this page guessing. Its actual number (one above the highest)
    // is shown too, as a small piece of context: what publishing a brand
    // new version of this component would be numbered next.
    let next_if_new = match next_sequence(&origin, id) {
        Ok(n) => n,
        Err(e) => {
            return Err(match SequenceProblem::from_error(&e) {
                Some(problem) => problem.to_string(),
                None => e.to_string(),
            });
        }
    };

    let current_version = match local_pins(&origin)? {
        Some((_, pins)) => pins.into_iter().find(|(cid, _)| *cid == id).map(|(_, v)| v),
        None => None,
    };

    let dir = origin.join(COMPONENTS_DIR);
    let prefix = format!("{}-", id.as_str());
    let mut versions: Vec<Value> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if !name.starts_with(&prefix) || !name.ends_with(".json") {
                continue;
            }
            let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            let parsed = Component::parse(&bytes).map_err(|e| format!("{} does not parse as a component manifest: {e}", path.display()))?;
            // The prefix match above is a filename guess; the manifest's own
            // `component` field is what actually decides it belongs here.
            if parsed.component != id.as_str() {
                continue;
            }
            versions.push(json!({
                "version": parsed.version,
                "sequence": parsed.sequence,
                "files": parsed.files.len(),
                "bytes": parsed.total_bytes(),
                "current": current_version.as_deref() == Some(parsed.version.as_str()),
            }));
        }
    }
    versions.sort_by(|a, b| {
        let sa = a.get("sequence").and_then(Value::as_u64).unwrap_or(0);
        let sb = b.get("sequence").and_then(Value::as_u64).unwrap_or(0);
        sb.cmp(&sa)
    });

    Ok(json!({
        "component": id.as_str(),
        "current_version": current_version,
        "versions": versions,
        "next_sequence_if_published_new": next_if_new,
    }))
}

/// Plan: the current pins against the proposed pins, and the bytes a player
/// would fetch. Writes nothing; see the module doc for why this still
/// calls `release` at `apply: false`.
#[tauri::command(rename_all = "snake_case")]
pub fn rollback_plan(component: String, version: String) -> Result<serde_json::Value, String> {
    let id = rollback_component(&component)?;
    let (_platform, settings) = settings_and_platform()?;
    let origin = origin_dir(&settings);
    let key = key_path(&settings)?;

    let inputs = json!({"kind": "rollback-plan", "component": id.as_str(), "version": version});

    ops::run_operation("history:rollback-plan", true, inputs, move |sink: Sink<'_>| -> reborn_core::Result<Outcome<Value>> {
        let facts = match compute_rollback(&origin, id, &version) {
            Ok(f) => f,
            Err(msg) => return Ok(refuse(sink, &[msg])),
        };

        let req = ReleaseRequest {
            origin: &origin,
            version: &facts.release_version,
            sequence: facts.sequence,
            key: &key,
            pins: &facts.pins,
            notes: None,
            apply: false,
        };
        match release(&req, sink)? {
            Outcome::Refused(r) => Ok(Outcome::Refused(r)),
            // `Described` (the expected path at apply: false) or `Done`
            // (impossible here, but exhaustive): either way validation
            // passed and nothing was written. Build the before/after.
            _ => {
                sink(Event::out(format!(
                    "Fetch         ~{} byte(s) across {} file(s), for a player already on {} {}",
                    thousands(facts.fetch_bytes),
                    facts.changed_files,
                    id.as_str(),
                    facts.current_version
                )));
                let summary = format!(
                    "Roll back {} from {} to {version}: sequence {} to {}, ~{} byte(s) to fetch.",
                    id.as_str(),
                    facts.current_version,
                    facts.sequence - 1,
                    facts.sequence,
                    thousands(facts.fetch_bytes)
                );
                Ok(Outcome::Done(json!({
                    "summary": summary,
                    // The plan's own proposed sequence, so a plan step's
                    // journal record shows one too, the same as a run's.
                    "sequence": facts.sequence,
                    "component": id.as_str(),
                    "current_version": facts.current_version,
                    "target_version": version,
                    "release_version": facts.release_version,
                    "current_sequence": facts.sequence - 1,
                    "proposed_sequence": facts.sequence,
                    "current_pins": facts.current_pins_display,
                    "proposed_pins": facts.proposed_pins_display,
                    "changed_files": facts.changed_files,
                    "fetch_bytes": facts.fetch_bytes,
                    "target_total_files": facts.target_total_files,
                    "target_total_bytes": facts.target_total_bytes,
                })))
            }
        }
    })
    .map(|()| json!({"accepted": true}))
}

/// Run: sign a release at the next sequence pinning the earlier version,
/// and upload it. `dry_run` skips both the write and the upload; see the
/// module doc's "Upload: scoped to what this rollback actually changes".
#[tauri::command(rename_all = "snake_case")]
pub fn rollback_run(component: String, version: String, dry_run: bool) -> Result<serde_json::Value, String> {
    let id = rollback_component(&component)?;
    let (platform, settings) = settings_and_platform()?;
    let origin = origin_dir(&settings);
    let key = key_path(&settings)?;

    // Checked up front, before anything is signed: a rollback that signs
    // locally but cannot upload would leave the local origin ahead of what
    // is actually served with no way to catch up from this page.
    let bucket = if dry_run { None } else { Some(bucket_from(&platform, &settings)?) };
    let public_base = settings.public_base.clone();

    let inputs = json!({"kind": "rollback-run", "component": id.as_str(), "version": version, "dry_run": dry_run});

    ops::run_operation("history:rollback-run", dry_run, inputs, move |sink: Sink<'_>| -> reborn_core::Result<Outcome<Value>> {
        let facts = match compute_rollback(&origin, id, &version) {
            Ok(f) => f,
            Err(msg) => return Ok(refuse(sink, &[msg])),
        };

        let req = ReleaseRequest {
            origin: &origin,
            version: &facts.release_version,
            sequence: facts.sequence,
            key: &key,
            pins: &facts.pins,
            notes: None,
            apply: !dry_run,
        };
        let signed = match release(&req, sink)? {
            Outcome::Refused(r) => return Ok(Outcome::Refused(r)),
            Outcome::Described => {
                let summary = format!(
                    "Described: would roll back {} to {version} at sequence {}. Nothing was written.",
                    id.as_str(),
                    facts.sequence
                );
                return Ok(Outcome::Done(json!({"summary": summary, "sequence": facts.sequence})));
            }
            Outcome::Done(s) => s,
        };

        let Some(bucket) = &bucket else {
            let summary = format!(
                "Signed release {} sequence {} locally. Not uploaded: this was a dry run.",
                signed.version, signed.sequence
            );
            return Ok(Outcome::Done(json!({"summary": summary, "sequence": signed.sequence})));
        };

        let transport = UreqTransport::new();
        let plan = build_upload_plan(&origin, id, &version, &transport, bucket, sink)?;
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
        let uploaded = match upload::upload(&transport, &http, &upload_req, sink)? {
            Outcome::Refused(r) => return Ok(Outcome::Refused(r)),
            Outcome::Described => Uploaded::default(),
            Outcome::Done(u) => u,
        };

        let summary = format!(
            "Rolled back {} to {version}, sequence {}: {} object(s), {} byte(s) uploaded.",
            id.as_str(),
            signed.sequence,
            uploaded.objects,
            uploaded.bytes
        );
        Ok(Outcome::Done(json!({
            "summary": summary,
            "sequence": signed.sequence,
            "uploaded": plan.items().map(|i| i.key.clone()).collect::<Vec<_>>(),
            "verified": uploaded.verified.first().cloned(),
        })))
    })
    .map(|()| json!({"accepted": true}))
}

/// Verify: fetch the actual hosted release fresh over HTTPS, check its
/// signature, and confirm it pins the component and version the last
/// rollback in this origin's journal signed, at the sequence it signed.
#[tauri::command(rename_all = "snake_case")]
pub fn rollback_verify() -> Result<serde_json::Value, String> {
    let (_platform, settings) = settings_and_platform()?;
    let origin = origin_dir(&settings);
    let public_base = settings.public_base.clone();
    let launcher_manifest = settings.launcher_manifest.clone();

    let inputs = json!({"kind": "rollback-verify"});

    ops::run_operation("history:rollback-verify", false, inputs, move |sink: Sink<'_>| -> reborn_core::Result<Outcome<Value>> {
        let all = journal::read_all(&origin)?;
        let last = all.records.iter().rev().find(|r| {
            r.operation == "history" && r.outcome == "done" && r.inputs.get("kind").and_then(|v| v.as_str()) == Some("rollback-run")
        });
        let Some(record) = last else {
            return Ok(refuse(sink, &["No rollback has been run yet in this origin's journal. Nothing to verify.".to_string()]));
        };
        let Some(sequence) = record.sequence else {
            return Ok(refuse(sink, &["The last rollback's journal record carries no sequence. Nothing to verify.".to_string()]));
        };
        let component = record.inputs.get("component").and_then(|v| v.as_str()).unwrap_or_default().to_string();
        let target_version = record.inputs.get("version").and_then(|v| v.as_str()).unwrap_or_default().to_string();

        sink(Event::out(format!("Checking      {public_base} pins {component} at {target_version}, sequence {sequence}")));

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
                "The hosted release is at sequence {}, not the {sequence} this rollback signed. The upload may not have completed.",
                hosted.sequence
            )]));
        }
        if !hosted.verified {
            return Ok(refuse(sink, &[format!("The hosted release at sequence {sequence} did not verify: {}", problem_text())]));
        }
        let pinned = hosted.pins.iter().any(|(cid, v)| cid.as_str() == component && v == &target_version);
        if !pinned {
            return Ok(refuse(sink, &[format!("The hosted release does not pin {component} at {target_version}.")]));
        }

        let summary =
            format!("Confirmed: the hosted release at sequence {sequence} pins {component} at {target_version}, signature verified.");
        sink(Event::out(summary.clone()));
        Ok(Outcome::Done(json!({"summary": summary, "verified": summary, "sequence": sequence})))
    })
    .map(|()| json!({"accepted": true}))
}

// -- shared plumbing ----------------------------------------------------

fn settings_and_platform() -> Result<(reborn_core::Platform, Settings), String> {
    let platform = reborn_core::Platform::detect();
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
        _ => Err("No publisher key is configured. Set one on the Keys page before rolling back.".to_string()),
    }
}

fn parse_component(name: &str) -> Result<ComponentId, String> {
    ComponentId::parse(name).ok_or_else(|| {
        let names: Vec<&str> = ComponentId::ALL.iter().map(|c| c.as_str()).collect();
        format!("{name:?} is not a component this build knows. Use one of: {}.", names.join(", "))
    })
}

/// A component this page can roll back. Members-only components
/// (`league-game`, `league-content`) are refused by name: a rollback here
/// uploads the target's blobs to the PUBLIC bucket, and theirs live only in
/// the private one. Checked first, before settings, the key or the vault.
fn rollback_component(name: &str) -> Result<ComponentId, String> {
    let id = parse_component(name)?;
    if id.is_members_only() {
        return Err(format!(
            "{} is members-only. League files can't be rolled back here; publish a new league version \
             instead (on the League files page).",
            id.as_str()
        ));
    }
    Ok(id)
}

/// The signed release in the LOCAL origin, and its pins resolved to each
/// pinned manifest's own version field, never just trusted from the file
/// name. `Ok(None)` when nothing has been signed here yet.
fn local_pins(origin: &Path) -> Result<Option<(Release, Vec<(ComponentId, String)>)>, String> {
    let release_path = origin.join(RELEASE_FILE);
    let bytes = match std::fs::read(&release_path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("{}: {e}", release_path.display())),
    };
    let release = Release::parse(&bytes).map_err(|e| format!("{} does not parse as a release manifest: {e}", release_path.display()))?;

    let mut pins = Vec::new();
    for (name, pinned) in &release.components {
        let Some(id) = ComponentId::parse(name) else {
            return Err(format!("{} pins a component named {name:?}, which this build does not know.", release_path.display()));
        };
        let manifest_local = origin.join(pinned.url.trim_start_matches('/').replace('/', std::path::MAIN_SEPARATOR_STR));
        let manifest_bytes = std::fs::read(&manifest_local)
            .map_err(|e| format!("{} pins {name} at {}, which could not be read: {e}", release_path.display(), manifest_local.display()))?;
        let component =
            Component::parse(&manifest_bytes).map_err(|e| format!("{} does not parse as a component manifest: {e}", manifest_local.display()))?;
        pins.push((id, component.version));
    }
    pins.sort_by(|a, b| a.0.as_str().cmp(b.0.as_str()));
    Ok(Some((release, pins)))
}

/// What `rollback_plan` shows and `rollback_run` signs, computed once so
/// the two can never disagree about what "this rollback" means.
struct RollbackFacts {
    release_version: String,
    /// The release sequence this rollback would sign at: one above the
    /// local origin's currently-signed release.
    sequence: u64,
    current_version: String,
    /// `(component, version)`, every component the current release pins.
    current_pins_display: Vec<(String, String)>,
    /// The same, with `component`'s entry replaced by the rollback target.
    proposed_pins_display: Vec<(String, String)>,
    /// The same, as the `Pin` list `release()` takes.
    pins: Vec<Pin>,
    target_total_files: usize,
    target_total_bytes: u64,
    /// Bytes a player already on `current_version` would fetch to reach
    /// the rollback target: every target file whose path is new or whose
    /// hash differs from what is live now.
    fetch_bytes: u64,
    changed_files: usize,
}

fn compute_rollback(origin: &Path, id: ComponentId, version: &str) -> Result<RollbackFacts, String> {
    let Some((release, current_pins)) = local_pins(origin)? else {
        return Err("This origin has not signed a release yet. There is nothing to roll back.".to_string());
    };
    let Some((_, current_version)) = current_pins.iter().find(|(cid, _)| *cid == id) else {
        return Err(format!("{} is not part of the currently signed release. Nothing to roll back.", id.as_str()));
    };
    let current_version = current_version.clone();
    if current_version == version {
        return Err(format!("{} is already pinned at {version}. Choose an earlier version to roll back to.", id.as_str()));
    }

    let current_manifest_path = manifest_path(origin, id, &current_version);
    let current_bytes =
        std::fs::read(&current_manifest_path).map_err(|e| format!("{}: {e}", current_manifest_path.display()))?;
    let current_component = Component::parse(&current_bytes)
        .map_err(|e| format!("{} does not parse as a component manifest: {e}", current_manifest_path.display()))?;

    let target_path = manifest_path(origin, id, version);
    let target_bytes = match std::fs::read(&target_path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(format!(
                "The origin does not hold a manifest for {} {version} ({}). It cannot be rolled back to.",
                id.as_str(),
                target_path.display()
            ));
        }
        Err(e) => return Err(format!("{}: {e}", target_path.display())),
    };
    let target =
        Component::parse(&target_bytes).map_err(|e| format!("{} does not parse as a component manifest: {e}", target_path.display()))?;

    let mut fetch_bytes = 0u64;
    let mut changed_files = 0usize;
    for file in &target.files {
        let unchanged = current_component.files.iter().any(|f| f.path == file.path && f.sha256 == file.sha256);
        if !unchanged {
            fetch_bytes += file.bytes;
            changed_files += 1;
        }
    }

    let mut pins = Vec::new();
    let mut current_display = Vec::new();
    let mut proposed_display = Vec::new();
    for (cid, cver) in &current_pins {
        current_display.push((cid.as_str().to_string(), cver.clone()));
        let use_version = if *cid == id { version.to_string() } else { cver.clone() };
        proposed_display.push((cid.as_str().to_string(), use_version.clone()));
        pins.push(Pin { id: *cid, version: use_version });
    }

    Ok(RollbackFacts {
        release_version: release.version,
        sequence: release.sequence + 1,
        current_version,
        current_pins_display: current_display,
        proposed_pins_display: proposed_display,
        pins,
        target_total_files: target.files.len(),
        target_total_bytes: target.total_bytes(),
        fetch_bytes,
        changed_files,
    })
}

/// The R2 bucket this app uploads to, from settings plus the vault.
fn bucket_from(platform: &reborn_core::Platform, settings: &Settings) -> Result<Bucket, String> {
    let endpoint =
        non_empty(&settings.bucket_endpoint).ok_or_else(|| "No bucket endpoint is configured. Set one in Settings before rolling back.".to_string())?;
    let bucket_name =
        non_empty(&settings.bucket_name).ok_or_else(|| "No bucket name is configured. Set one in Settings before rolling back.".to_string())?;
    let access_key_id = r2_access(platform)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "No R2 access key is stored in the vault. Store one on the Keys page before rolling back.".to_string())?;
    let secret_access_key = r2_secret(platform)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "No R2 secret key is stored in the vault. Store one on the Keys page before rolling back.".to_string())?;
    Ok(Bucket { endpoint, bucket: bucket_name, region: "auto".to_string(), access_key_id, secret_access_key })
}

fn non_empty(s: &Option<String>) -> Option<String> {
    s.as_ref().map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

/// Exactly what this rollback needs on the bucket: the target manifest,
/// the blobs it names, and the two release files, skipping any the bucket
/// already holds at the right length. See the module doc.
fn build_upload_plan(
    origin: &Path,
    id: ComponentId,
    version: &str,
    transport: &dyn upload::Transport,
    bucket: &Bucket,
    sink: Sink<'_>,
) -> reborn_core::Result<UploadPlan> {
    let mut plan = UploadPlan::new();

    let manifest_local = manifest_path(origin, id, version);
    let manifest_bytes = std::fs::read(&manifest_local).map_err(|e| plain_err(format!("{}: {e}", manifest_local.display())))?;
    let component = Component::parse(&manifest_bytes)
        .map_err(|e| plain_err(format!("{} does not parse as a component manifest: {e}", manifest_local.display())))?;

    // One listing, not one HEAD per file: a texture set is eight thousand.
    // Said out loud either way: a bucket that would not list falls back to
    // one request per key, and that is slow enough to look hung.
    let (already, how) = upload::already_on_bucket(transport, bucket)?;
    sink(Event::out(how));
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

    // Always sent: they change on every publish, the same rule
    // `plan_origin` follows for these two files.
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

fn thousands(n: u64) -> String {
    reborn_core::publish::thousands(n)
}

#[cfg(test)]
mod members_only_tests {
    use super::*;

    /// A rollback uploads the target's blobs to the PUBLIC bucket; a
    /// members-only component's blobs must never go there, so both commands
    /// refuse league-game and league-content before reading settings or
    /// touching anything.
    #[test]
    fn rollback_refuses_members_only_components_before_anything_runs() {
        for name in ["league-game", "league-content"] {
            let plan = rollback_plan(name.to_string(), "1.0.0".to_string()).unwrap_err();
            assert!(plan.contains("League files can't be rolled back here"), "{name}: {plan}");
            assert!(plan.contains("publish a new league version instead"), "{name}: {plan}");
            let run = rollback_run(name.to_string(), "1.0.0".to_string(), true).unwrap_err();
            assert!(run.contains("League files can't be rolled back here"), "{name}: {run}");
            // The real (non-dry) run is not called: it would read the real
            // settings/vault. The guard it opens with is asserted directly.
            let guard = rollback_component(name).unwrap_err();
            assert!(guard.contains("League files can't be rolled back here"), "{name}: {guard}");
        }
    }

    #[test]
    fn rollback_still_accepts_public_components() {
        assert_eq!(rollback_component("roster").unwrap(), ComponentId::Roster);
        assert_eq!(rollback_component("textures").unwrap(), ComponentId::Textures);
    }
}
