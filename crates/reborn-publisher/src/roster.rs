//! The Roster page: fetch the roster the site published, or a file, and
//! push it as a component plus a release carrying every other pin forward.
//! This is the operation Adam runs most weeks.
//!
//! Plan, Run and Verify all go through [`crate::ops::run_operation`], same
//! as every other page; nothing here computes a publishing rule the core
//! does not already own. `reborn_core::publish::roster::publish_roster` is
//! the authority on what gets published and how a junk file is refused;
//! this file's own job is reading what a `.rebornroster` says about itself
//! before Run is ever clicked, and reading back what a player's launcher
//! would actually do once it is live.
//!
//! # Never panic
//!
//! Task 1's own finding stands: a Tauri command that panics hangs its
//! promise forever instead of rejecting it. Nothing below calls `.unwrap()`
//! or `.expect()` on a file that came from the network or from whatever
//! path an operator typed; every failure is a `String` (for the two
//! commands that can refuse before an operation even starts) or an
//! `Outcome::Refused` / `Err` handed back through `ops::run_operation`
//! (which itself never panics reporting one).
//!
//! # Reading a `.rebornroster`
//!
//! Counting a roster's players and teams means opening a zip and
//! inflating one small entry, `manifest.json`. That work used to be
//! hand-rolled directly in this file: a raw RFC 1951 inflate plus just
//! enough of a zip's local file headers to pull one named entry out by
//! scanning `PK\x03\x04` signatures from the front. It worked and was
//! tested against real exports, but bit-level DEFLATE decoding is subtle
//! code with real failure modes on malformed input, and reading what a
//! `.rebornroster` says about itself is no more this page's job than
//! deciding what one *is* would be -- the core already owns that.
//!
//! `reborn_core::publish::roster::read_counts` does the actual reading
//! now, over the `zip` crate (with flate2's pure-Rust `miniz_oxide` back
//! end for zip method 8), and refuses a file that is not a roster with
//! the exact sentence `reborn_core::publish::roster::publish_roster`
//! itself refuses one with -- both share the same check in the core --
//! rather than a decompression error, so a bad file reads the same
//! whether it is caught here at Plan time or by the core at Run time.
//! This file's only remaining job is turning that `Err` into the page's
//! own refusal lines; see [`not_a_roster_lines`].
//!
//! # The site's export URL
//!
//! `docs/NEXT_STEPS.md`'s own "Commands that matter" section, current as
//! of this task, gives the exact command Adam runs by hand today:
//!
//! ```text
//! target\release\rebornctl.exe publish roster --from https://nflstreetreborn.com/rdatabase/api/download.php?file=rebornroster ...
//! ```
//!
//! [`SITE_EXPORT_URL`] is that URL, fixed the same way `status.rs` fixes
//! `IMPORT_BUNDLE_DEFAULT`: this app is single-operator, single-machine by
//! design (design doc section 2), so wherever the site actually drops the
//! export is the one fixed thing this constant needs to name. A URL that
//! turns out wrong here still leaves `{"kind": "file"}` as the escape
//! hatch `docs/RUNNING_UPDATES.md` names for exactly this situation, and a
//! fetch that fails says so instead of guessing further.
//!
//! # The seeded scratch-root check
//!
//! [`roster_verify`] does two things: reads the hosted release back over
//! HTTPS (the same request a player's launcher makes), then runs the same
//! `reborn_core::content::pipeline::Updater` check a player's launcher
//! runs, against a scratch data root under this machine's temp directory
//! that is created fresh and removed when the check ends, on every path
//! out (success or failure). The trick is seeding: a first check against
//! an *empty* scratch root reports every carried component "available" at
//! its full size, which is only useful for reading off each component's
//! actually-pinned `(version, sequence)`; those get written back as
//! `Pointer` files for every component except `roster`, so a *second*
//! check reports every one of them "up to date" (their pointer now agrees
//! with the release) while `roster`, never seeded, is still fully
//! "available". That second check's report is what "the roster pending at
//! its size and every other component at zero fetches" means, and it is
//! read off the real content pipeline rather than reimplemented here.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{json, Value};

use reborn_core::content::pipeline::{updatable, CheckRequest, ComponentState, Depth, Updater};
use reborn_core::content::{ContentService, InstalledVersion, Pointer, POINTER_SCHEMA};
use reborn_core::inventory::{CancelSignal, ComponentId, ScanObserver};
use reborn_core::manifest::{Component, Release};
use reborn_core::net::{Http, UreqHttp};
use reborn_core::origin::{Origin, RELEASE_FILE};
use reborn_core::publish::roster::{publish_roster, read_counts, RosterRequest};
use reborn_core::publish::upload::{self, Bucket, UploadRequest, Uploaded, UreqTransport};
use reborn_core::publish::vault::{self, r2_access, r2_secret};
use reborn_core::publish::{next_sequence, refuse, thousands, Event, Outcome, Sink};
use reborn_core::signing::Ed25519Verifier;
use reborn_core::Platform;

use crate::ops;
use crate::settings::Settings;

/// The site's login-only roster export, fetched with the vaulted site
/// token. See this module's doc for where this comes from.
const SITE_EXPORT_URL: &str = "https://nflstreetreborn.com/rdatabase/api/download.php?file=rebornroster";

// ---------------------------------------------------------------------------
// Plan
// ---------------------------------------------------------------------------

/// Plan: the roster's player and team counts, the sequence and pins before
/// and after, and one sentence on what a player will see. Writes nothing;
/// `resolve_source` may still fetch the site's export for real, since
/// there is no way to know the counts without reading the bytes.
#[tauri::command(rename_all = "snake_case")]
pub fn roster_plan(source: Value, version: String) -> Result<Value, String> {
    let (platform, settings) = settings_and_platform()?;
    let origin = origin_dir(&settings);
    let version = version.trim().to_string();
    if version.is_empty() {
        return Err("A version label is required, for example today's date.".to_string());
    }
    let has_key = non_empty(&settings.publisher_key).is_some();

    let inputs = json!({"kind": "roster-plan", "source": source.clone(), "version": version});

    ops::run_operation("roster:plan", true, inputs, move |sink: Sink<'_>| -> reborn_core::Result<Outcome<Value>> {
        let (path, _scratch) = match resolve_source(&source, &platform) {
            Ok(p) => p,
            Err(msg) => return Ok(refuse(sink, &[msg])),
        };
        sink(Event::out(format!("Roster        {}", path.display())));

        let counts = match read_counts(&path) {
            Ok(c) => c,
            Err(e) => return Ok(refuse(sink, &not_a_roster_lines(e.to_string()))),
        };
        let file_bytes = match std::fs::metadata(&path) {
            Ok(m) => m.len(),
            Err(e) => return Err(reborn_core::Error::PlainIo(e)),
        };
        sink(Event::out(format!(
            "Read          {} players, {} teams, {} portrait(s){}",
            counts.players,
            counts.teams,
            counts.portraits,
            counts.base_label.as_deref().map(|b| format!(", base {b}")).unwrap_or_default()
        )));

        let component_sequence_after = next_sequence(&origin, ComponentId::Roster)?;
        let component_sequence_before =
            if component_sequence_after > 1 { Some(component_sequence_after - 1) } else { None };

        let (release_version, release_sequence_before, current_pins) = match local_release(&origin) {
            Ok(Some((r, pins))) => (Some(r.version.clone()), Some(r.sequence), pins),
            Ok(None) => (None, None, Vec::new()),
            Err(msg) => return Ok(refuse(sink, &[msg])),
        };
        let release_sequence_after = release_sequence_before.map(|s| s + 1);
        let release_bootstrap_needed = release_version.is_none();

        let mut proposed_pins = current_pins.clone();
        match proposed_pins.iter_mut().find(|(n, _)| n == ComponentId::Roster.as_str()) {
            Some(entry) => entry.1 = version.clone(),
            None => proposed_pins.push((ComponentId::Roster.as_str().to_string(), version.clone())),
        }
        proposed_pins.sort();

        sink(Event::out(format!(
            "Sequence      roster {} -> {}, release {} -> {}",
            seq_text(component_sequence_before),
            component_sequence_after,
            seq_text(release_sequence_before),
            seq_text(release_sequence_after),
        )));

        let mut summary = format!(
            "Roster {version}: {} players, {} teams, {} byte(s). Roster sequence {} -> {}.",
            counts.players,
            counts.teams,
            thousands(file_bytes),
            seq_text(component_sequence_before),
            component_sequence_after,
        );
        if release_bootstrap_needed {
            summary.push_str(" No release has been signed in this origin yet; the release step needs a CLI override to carry anything forward.");
        } else if !has_key {
            summary.push_str(" No publisher key is configured; the component would publish but the release would only be described.");
        }

        Ok(Outcome::Done(json!({
            "summary": summary,
            "sequence": release_sequence_after,
            "component": ComponentId::Roster.as_str(),
            "version": version,
            "players": counts.players,
            "teams": counts.teams,
            "portraits": counts.portraits,
            "base_label": counts.base_label,
            "file_bytes": file_bytes,
            "component_sequence_before": component_sequence_before,
            "component_sequence_after": component_sequence_after,
            "release_version": release_version,
            "release_sequence_before": release_sequence_before,
            "release_sequence_after": release_sequence_after,
            "release_will_be_signed": has_key && !release_bootstrap_needed,
            "release_bootstrap_needed": release_bootstrap_needed,
            "has_key": has_key,
            "current_pins": current_pins,
            "proposed_pins": proposed_pins,
        })))
    })
    .map(|()| json!({"accepted": true}))
}

// ---------------------------------------------------------------------------
// Run
// ---------------------------------------------------------------------------

/// Run: publish the roster component, sign a release carrying every other
/// pin forward, then upload through plan 2's whole-origin uploader.
/// `dry_run` still fetches and checks (there is no way to publish a file
/// that was never read), but writes nothing and sends nothing.
#[tauri::command(rename_all = "snake_case")]
pub fn roster_run(source: Value, version: String, notes: Option<String>, dry_run: bool) -> Result<Value, String> {
    let (platform, settings) = settings_and_platform()?;
    let origin = origin_dir(&settings);
    let version = version.trim().to_string();
    if version.is_empty() {
        return Err("A version label is required, for example today's date.".to_string());
    }
    let key = non_empty(&settings.publisher_key).map(PathBuf::from);
    let public_base = settings.public_base.clone();

    // Resolved up front, before anything is fetched: a roster signed
    // locally but that cannot be uploaded would leave the origin ahead of
    // what the bucket actually serves with no way to catch up from here.
    let bucket = if dry_run { None } else { Some(bucket_from(&platform, &settings)?) };

    // What players read on the update card, signed into the release. Blank
    // is none; the core normalizes line endings and trims.
    let notes = notes.filter(|n| !n.trim().is_empty());
    let inputs = json!({"kind": "roster-run", "source": source.clone(), "version": version, "notes": notes, "dry_run": dry_run});

    ops::run_operation("roster:run", dry_run, inputs, move |sink: Sink<'_>| -> reborn_core::Result<Outcome<Value>> {
        let cancel = CancelSignal::new();
        ops::set_cancel_signal(cancel.clone());

        let (path, _scratch) = match resolve_source(&source, &platform) {
            Ok(p) => p,
            Err(msg) => return Ok(refuse(sink, &[msg])),
        };

        let counts = match read_counts(&path) {
            Ok(c) => c,
            Err(e) => return Ok(refuse(sink, &not_a_roster_lines(e.to_string()))),
        };
        sink(Event::out(format!(
            "Read          {} players, {} teams, {} portrait(s)",
            counts.players, counts.teams, counts.portraits
        )));

        // `path` is always a local file by this point (`resolve_source`
        // already fetched the site's export if that was the source), so
        // `publish_roster` only ever copies here; it never touches the
        // network itself and needs no token.
        let from = path.to_string_lossy().into_owned();
        let http = UreqHttp::new(&user_agent());
        let req = RosterRequest {
            from: &from,
            origin: &origin,
            version: &version,
            sequence: None,
            key: key.as_deref(),
            release_version: None,
            release_sequence: None,
            notes: notes.as_deref(),
            apply: !dry_run,
            token: None,
        };
        let push = match publish_roster(&http, &req, &cancel as &dyn ScanObserver, sink)? {
            Outcome::Refused(r) => return Ok(Outcome::Refused(r)),
            Outcome::Described => {
                let summary = format!(
                    "Described: would publish roster {version} ({} players, {} teams). Nothing was written.",
                    counts.players, counts.teams
                );
                return Ok(Outcome::Done(json!({"summary": summary})));
            }
            Outcome::Done(p) => p,
        };

        let Some(bucket) = &bucket else {
            let summary = format!(
                "Published roster {version} locally ({} players, {} teams). Not uploaded: this was a dry run.",
                counts.players, counts.teams
            );
            return Ok(Outcome::Done(json!({
                "summary": summary,
                "sequence": push.release.as_ref().map(|s| s.sequence),
            })));
        };

        let transport = UreqTransport::new();
        let (already, how) = upload::already_on_bucket(&transport, bucket)?;
        // Said out loud: a bucket that would not list falls back to one
        // request per key, and that is slow enough to look hung.
        sink(Event::out(how));
        let plan = upload::plan_origin(&origin, &*already)?;
        let http2 = UreqHttp::new(&user_agent());
        let upload_req = UploadRequest {
            bucket,
            plan: &plan,
            multipart_threshold: upload::DEFAULT_MULTIPART_THRESHOLD,
            part_size: upload::DEFAULT_PART_SIZE,
            attempts: upload::DEFAULT_ATTEMPTS,
            public_base: Some(&public_base),
            apply: true,
        };
        let uploaded = match upload::upload(&transport, &http2, &upload_req, sink)? {
            Outcome::Refused(r) => return Ok(Outcome::Refused(r)),
            Outcome::Described => Uploaded::default(),
            Outcome::Done(u) => u,
        };

        let summary = format!(
            "Published roster {version} ({} players, {} teams): {} object(s), {} byte(s) uploaded.",
            counts.players, counts.teams, uploaded.objects, uploaded.bytes
        );
        Ok(Outcome::Done(json!({
            "summary": summary,
            "sequence": push.release.as_ref().map(|s| s.sequence),
            "uploaded": plan.items().map(|i| i.key.clone()).collect::<Vec<_>>(),
            "verified": uploaded.verified.first().cloned(),
        })))
    })
    .map(|()| json!({"accepted": true}))
}

// ---------------------------------------------------------------------------
// Verify
// ---------------------------------------------------------------------------

/// Verify: read the hosted release back over HTTPS, then a seeded
/// scratch-root check that must show the roster pending at its size and
/// every other component at zero fetches. See this module's doc for how
/// the seeding works. The scratch root lives under this machine's temp
/// directory and is removed on every path out of this function.
#[tauri::command(rename_all = "snake_case")]
pub fn roster_verify() -> Result<Value, String> {
    let (platform, settings) = settings_and_platform()?;
    let public_base = settings.public_base.clone();

    let inputs = json!({"kind": "roster-verify"});

    ops::run_operation("roster:verify", false, inputs, move |sink: Sink<'_>| -> reborn_core::Result<Outcome<Value>> {
        let verifier = Ed25519Verifier::embedded();
        let origin_source = Origin::new(&public_base);

        sink(Event::out(format!("Reading       {public_base}")));
        let fetched = match origin_source.fetch_release(&verifier) {
            Ok(f) => f,
            Err(e) => return Ok(refuse(sink, &[format!("The hosted release could not be read: {e}")])),
        };
        let release = fetched.trusted.release();
        sink(Event::out(format!(
            "Verified      signature over {} byte(s); release {} sequence {}",
            fetched.bytes.len(),
            release.version,
            release.sequence
        )));

        if !release.components.contains_key(ComponentId::Roster.as_str()) {
            return Ok(refuse(sink, &["The hosted release does not pin a roster component yet. Nothing to verify.".to_string()]));
        }

        let scratch = scratch_dir_path("roster-verify");
        std::fs::create_dir_all(&scratch).map_err(|e| plain_err(format!("{}: {e}", scratch.display())))?;
        let _guard = ScratchDir(scratch.clone());
        sink(Event::out(format!("Scratch root  {}", scratch.display())));

        // Pass 1: an empty root reports every carried component as fully
        // available, which is the only way to read off the (version,
        // sequence) each one is actually pinned at right now.
        let pass_one =
            Updater::new(&platform, &scratch, &origin_source, &verifier).check(&CheckRequest { components: updatable(), depth: Depth::Quick })?;

        let service = ContentService::new(&platform, &scratch);
        for id in updatable().iter().copied() {
            if id == ComponentId::Roster {
                continue;
            }
            let Some(plan) = pass_one.component(id).and_then(|cc| cc.plan()) else {
                continue;
            };
            let pointer = Pointer {
                schema: POINTER_SCHEMA.to_string(),
                component: id.as_str().to_string(),
                current: InstalledVersion { version: plan.version().to_string(), sequence: plan.sequence() },
                previous: None,
            };
            pointer.save(&platform, &service.layout().pointer_path(id.as_str()))?;
        }

        // Pass 2: every seeded component now agrees with the release it
        // was seeded from, so the only thing left "available" should be
        // roster, never seeded. This is the check a player's launcher runs.
        let pass_two =
            Updater::new(&platform, &scratch, &origin_source, &verifier).check(&CheckRequest { components: updatable(), depth: Depth::Quick })?;

        let mut problems = Vec::new();
        let mut roster_bytes: Option<u64> = None;
        for cc in &pass_two.components {
            let (label, bytes) = match &cc.state {
                ComponentState::Available(plan) => ("pending", plan.download_bytes()),
                ComponentState::UpToDate { .. } => ("up to date", 0),
                ComponentState::NotCarried => ("not carried by this release", 0),
                ComponentState::NotUpdatable => ("not updatable", 0),
                ComponentState::Refused(r) => {
                    problems.push(format!("{}: {}", cc.component.as_str(), r.message()));
                    ("refused", 0)
                }
                ComponentState::Problem(m) => {
                    problems.push(format!("{}: {m}", cc.component.as_str()));
                    ("a problem", 0)
                }
            };
            sink(Event::out(format!("{:<13} {label}, {} byte(s)", cc.component.as_str(), thousands(bytes))));

            if cc.component == ComponentId::Roster {
                if label != "pending" {
                    problems.push(format!(
                        "roster: expected a pending update on a scratch root that was never seeded for it; found {label} instead"
                    ));
                }
                roster_bytes = Some(bytes);
            } else if bytes != 0 || !matches!(label, "up to date" | "not carried by this release") {
                problems.push(format!(
                    "{}: expected zero fetches once seeded; found {label} ({} byte(s))",
                    cc.component.as_str(),
                    thousands(bytes)
                ));
            }
        }

        if !problems.is_empty() {
            return Ok(refuse(sink, &problems));
        }

        let roster_bytes = roster_bytes.unwrap_or(0);
        let summary = format!(
            "Confirmed: release sequence {} verifies, and a seeded scratch install sees the roster pending at {} byte(s) with every other component at zero fetches.",
            release.sequence,
            thousands(roster_bytes)
        );
        sink(Event::out(summary.clone()));
        Ok(Outcome::Done(json!({
            "summary": summary,
            "verified": summary,
            "sequence": release.sequence,
        })))
    })
    .map(|()| json!({"accepted": true}))
}

// ---------------------------------------------------------------------------
// Resolving a source to a local file
// ---------------------------------------------------------------------------

/// A temp file this page fetched, removed when it drops. Never applied to
/// a `{"kind": "file"}` source: that path is the operator's own and is
/// never touched.
#[derive(Debug)]
struct ScratchFile(PathBuf);

impl Drop for ScratchFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// A temp directory this page created, removed (recursively) when it
/// drops, on every path out of the function that made it.
struct ScratchDir(PathBuf);

impl Drop for ScratchDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// `source` down to one local file: `{"kind": "site"}` fetches
/// [`SITE_EXPORT_URL`] with the vaulted token into a scratch file this
/// page owns and cleans up; `{"kind": "file", "path": ...}` uses the given
/// path as-is, untouched and uncleaned, because it belongs to whoever
/// typed it.
fn resolve_source(source: &Value, platform: &Platform) -> Result<(PathBuf, Option<ScratchFile>), String> {
    match source.get("kind").and_then(Value::as_str).unwrap_or("") {
        "site" => {
            let path = fetch_site_export(platform)?;
            Ok((path.clone(), Some(ScratchFile(path))))
        }
        "file" => {
            let raw = source
                .get("path")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|p| !p.is_empty())
                .ok_or_else(|| "Choose a .rebornroster file first.".to_string())?;
            let path = PathBuf::from(raw);
            if !path.is_file() {
                return Err(format!("{} is not a file. Choose a .rebornroster file that exists.", path.display()));
            }
            Ok((path, None))
        }
        other => Err(format!("{other:?} is not a roster source this page knows. Use the site export or a file.")),
    }
}

/// Fetch [`SITE_EXPORT_URL`] with whatever site token is stored in the
/// vault (fetched with none when nothing is stored, the same as an
/// anonymous `curl`), into a scratch file under this machine's temp
/// directory. A 401 is explained the same way
/// `reborn_core::publish::roster::publish_roster` explains one for a
/// direct URL fetch, since this is the same request.
fn fetch_site_export(platform: &Platform) -> Result<PathBuf, String> {
    let token = vault::site_token(platform).map_err(|e| e.to_string())?;
    let http = UreqHttp::new(&user_agent());
    let dir = std::env::temp_dir().join("reborn-publisher").join("roster-fetch");
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let path = dir.join(format!("export-{}-{}.rebornroster", std::process::id(), next_call_id()));
    match http.get_to_file_as(SITE_EXPORT_URL, &path, token.as_deref()) {
        Ok(_) => Ok(path),
        Err(e) if e.to_string().contains("answered 401") => Err(format!(
            "{SITE_EXPORT_URL} answered 401: that export needs a login. Store the site's API \
             token on the Keys page, then try again."
        )),
        Err(e) => {
            Err(format!("Could not fetch {SITE_EXPORT_URL}: {e}. Choose a .rebornroster file instead if the site is unreachable."))
        }
    }
}

/// A path unique to this call, under `std::env::temp_dir()`, for a scratch
/// directory this page owns for the lifetime of one command.
fn scratch_dir_path(tag: &str) -> PathBuf {
    std::env::temp_dir().join("reborn-publisher").join(tag).join(format!("{}-{}", std::process::id(), next_call_id()))
}

fn next_call_id() -> u64 {
    static CALLS: AtomicU64 = AtomicU64::new(0);
    CALLS.fetch_add(1, Ordering::Relaxed)
}

// ---------------------------------------------------------------------------
// Reading a `.rebornroster`
// ---------------------------------------------------------------------------

/// Lines refusing a file that does not read as a `.rebornroster`, in the
/// same voice `publish_roster`'s own refusal for a junk file uses, so a
/// bad file reads the same whether it is caught here at Plan time or by
/// the core at Run time.
fn not_a_roster_lines(why: String) -> Vec<String> {
    vec![
        String::new(),
        "That does not read as a .rebornroster: expected a zip holding manifest.json,".to_string(),
        format!("players.json and teams.json with a valid counts block. {why}"),
        "Nothing was published.".to_string(),
    ]
}

// ---------------------------------------------------------------------------
// The local origin's release and pins
// ---------------------------------------------------------------------------

/// The signed release in the LOCAL origin, and its pins resolved to each
/// pinned manifest's own `version` field (never trusted from the file
/// name alone), by component name. `Ok(None)` when nothing has been
/// signed here yet.
fn local_release(origin: &Path) -> Result<Option<(Release, Vec<(String, String)>)>, String> {
    let release_path = origin.join(RELEASE_FILE);
    let bytes = match std::fs::read(&release_path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("{}: {e}", release_path.display())),
    };
    let release =
        Release::parse(&bytes).map_err(|e| format!("{} does not parse as a release manifest: {e}", release_path.display()))?;

    let mut pins = Vec::new();
    for (name, pinned) in &release.components {
        let manifest_local = origin.join(pinned.url.trim_start_matches('/').replace('/', std::path::MAIN_SEPARATOR_STR));
        let manifest_bytes = std::fs::read(&manifest_local)
            .map_err(|e| format!("{} pins {name} at {}, which could not be read: {e}", release_path.display(), manifest_local.display()))?;
        let component = Component::parse(&manifest_bytes)
            .map_err(|e| format!("{} does not parse as a component manifest: {e}", manifest_local.display()))?;
        pins.push((name.clone(), component.version));
    }
    pins.sort();
    Ok(Some((release, pins)))
}

fn seq_text(s: Option<u64>) -> String {
    s.map(|n| n.to_string()).unwrap_or_else(|| "none".to_string())
}

// ---------------------------------------------------------------------------
// Shared plumbing
// ---------------------------------------------------------------------------

fn settings_and_platform() -> Result<(Platform, Settings), String> {
    let platform = Platform::detect();
    let path = crate::settings::settings_path(&platform)?;
    let settings = Settings::load(&path)?;
    Ok((platform, settings))
}

fn origin_dir(settings: &Settings) -> PathBuf {
    PathBuf::from(&settings.origin)
}

fn non_empty(s: &Option<String>) -> Option<String> {
    s.as_ref().map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

fn user_agent() -> String {
    concat!("reborn-publisher/", env!("CARGO_PKG_VERSION")).to_string()
}

/// The R2 bucket this app uploads to, from settings plus the vault. Never
/// built from an argument the page passed in: a secret has no business
/// being something a browser devtools console in front of this window
/// could set.
fn bucket_from(platform: &Platform, settings: &Settings) -> Result<Bucket, String> {
    let endpoint =
        non_empty(&settings.bucket_endpoint).ok_or_else(|| "No bucket endpoint is configured. Set one in Settings before publishing.".to_string())?;
    let bucket_name =
        non_empty(&settings.bucket_name).ok_or_else(|| "No bucket name is configured. Set one in Settings before publishing.".to_string())?;
    let access_key_id = r2_access(platform)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "No R2 access key is stored in the vault. Store one on the Keys page before publishing.".to_string())?;
    let secret_access_key = r2_secret(platform)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "No R2 secret key is stored in the vault. Store one on the Keys page before publishing.".to_string())?;
    Ok(Bucket { endpoint, bucket: bucket_name, region: "auto".to_string(), access_key_id, secret_access_key })
}


fn plain_err(message: impl Into<String>) -> reborn_core::Error {
    reborn_core::Error::PlainIo(std::io::Error::new(std::io::ErrorKind::Other, message.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real zip, with a proper central directory (the `zip` crate,
    /// unlike the old hand-rolled reader, needs one), built the way
    /// `reborn_core::publish::roster`'s own tests build one, so this
    /// page's delegation to `read_counts` is proven against the same
    /// shape a real export takes rather than only exercised end to end
    /// through a real file on Adam's machine.
    fn roster_zip_with_manifest(manifest_json: &[u8]) -> Vec<u8> {
        use std::io::Write;
        let mut buf = Vec::new();
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
        let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for (name, data) in [("manifest.json", manifest_json), ("players.json", b"[]"), ("teams.json", b"[]")] {
            writer.start_file(name, options).unwrap();
            writer.write_all(data).unwrap();
        }
        writer.finish().unwrap();
        buf
    }

    #[test]
    fn plan_reads_a_rosters_counts_through_the_core() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.rebornroster");
        let manifest = br#"{"counts":{"players":569,"teams":51,"portraits":12},"base_label":"2026.08.28"}"#;
        std::fs::write(&path, roster_zip_with_manifest(manifest)).unwrap();

        let counts = read_counts(&path).unwrap();
        assert_eq!(counts.players, 569);
        assert_eq!(counts.teams, 51);
        assert_eq!(counts.portraits, 12);
        assert_eq!(counts.base_label.as_deref(), Some("2026.08.28"));
    }

    #[test]
    fn a_junk_file_is_refused_in_this_pages_own_words() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("404.html");
        std::fs::write(&path, b"<html>not found</html>").unwrap();

        let err = read_counts(&path).unwrap_err();
        let lines = not_a_roster_lines(err.to_string());
        assert!(lines.iter().any(|l| l.contains("does not read as a .rebornroster")), "{lines:?}");
        assert!(lines.iter().any(|l| l.contains("Nothing was published")), "{lines:?}");
    }

    #[test]
    fn resolve_source_refuses_a_file_that_does_not_exist() {
        let platform = Platform::detect();
        let source = json!({"kind": "file", "path": r"C:\nowhere\at\all.rebornroster"});
        let err = resolve_source(&source, &platform).unwrap_err();
        assert!(err.contains("is not a file"), "{err}");
    }

    #[test]
    fn resolve_source_refuses_an_unknown_kind() {
        let platform = Platform::detect();
        let source = json!({"kind": "carrier pigeon"});
        let err = resolve_source(&source, &platform).unwrap_err();
        assert!(err.contains("carrier pigeon"), "{err}");
    }
}
