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
//! # Reading a `.rebornroster` without a zip or a deflate crate
//!
//! This task touches exactly two files (`roster.rs`, `roster.js`), so
//! adding a dependency to `Cargo.toml` is out of scope even though "the
//! roster's player and team counts read from the file" needs one: every
//! real export sampled while building this page
//! (`base-2026.08.28.rebornroster`, `boot4-test.rebornroster`, both PHP
//! `ZipArchive` output) stores every entry, including the small
//! `manifest.json` that already carries `counts.players` / `counts.teams`
//! / `counts.portraits`, with DEFLATE (zip method 8), not stored. So this
//! file carries its own minimal, read-only pieces of both formats: a raw
//! RFC 1951 inflate ([`inflate`], [`Huffman`], [`decode_symbol`], the
//! fixed and dynamic table readers) and just enough of a zip's local file
//! headers ([`find_zip_entry`]) to pull one named entry out by scanning
//! `PK\x03\x04` signatures from the front, with no central directory, no
//! zip64, no encryption and no streamed (data-descriptor) entries -- every
//! real export seen writes plain local headers with sizes known up front,
//! and anything else refuses by name rather than guessing. `inflate` was
//! proven against Python's `zlib` in raw-deflate mode on fixtures ranging
//! from an empty block through a 725 KB multi-block `players.json` before
//! being used here; see this file's own tests.
//!
//! `reborn_core::publish::roster::publish_roster` only ever checks that a
//! file *looks* like a `.rebornroster` (a zip magic number, then the three
//! entry names present somewhere in the clear); it does not read
//! `manifest.json` because the Builder, not the publisher, is the
//! authority on the disc-side format. Reading `manifest.json`'s own
//! `counts` here, once, is this page's own job and does not change what
//! the core accepts or refuses.
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
use std::time::SystemTime;

use serde_json::{json, Value};

use reborn_core::content::pipeline::{CheckRequest, ComponentState, Depth, Updater, UPDATABLE};
use reborn_core::content::{ContentService, InstalledVersion, Pointer, POINTER_SCHEMA};
use reborn_core::inventory::{CancelSignal, ComponentId, ScanObserver};
use reborn_core::manifest::{Component, Release, Sha256Hex};
use reborn_core::net::{Http, UreqHttp};
use reborn_core::origin::{Origin, RELEASE_FILE};
use reborn_core::publish::roster::{publish_roster, RosterRequest};
use reborn_core::publish::upload::{self, Bucket, UploadRequest, Uploaded, UreqTransport};
use reborn_core::publish::vault::{self, r2_access_key, r2_secret_key};
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
#[tauri::command]
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

        let counts = match read_roster_counts(&path) {
            Ok(c) => c,
            Err(msg) => return Ok(refuse(sink, &not_a_roster_lines(msg))),
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
#[tauri::command]
pub fn roster_run(source: Value, version: String, dry_run: bool) -> Result<Value, String> {
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

    let inputs = json!({"kind": "roster-run", "source": source.clone(), "version": version, "dry_run": dry_run});

    ops::run_operation("roster:run", dry_run, inputs, move |sink: Sink<'_>| -> reborn_core::Result<Outcome<Value>> {
        let cancel = CancelSignal::new();
        ops::set_cancel_signal(cancel.clone());

        let (path, _scratch) = match resolve_source(&source, &platform) {
            Ok(p) => p,
            Err(msg) => return Ok(refuse(sink, &[msg])),
        };

        let counts = match read_roster_counts(&path) {
            Ok(c) => c,
            Err(msg) => return Ok(refuse(sink, &not_a_roster_lines(msg))),
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
        let plan = upload::plan_origin(&origin, &|key| head_len(&transport, bucket, key))?;
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
#[tauri::command]
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
            Updater::new(&platform, &scratch, &origin_source, &verifier).check(&CheckRequest { components: &UPDATABLE, depth: Depth::Quick })?;

        let service = ContentService::new(&platform, &scratch);
        for id in UPDATABLE {
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
            Updater::new(&platform, &scratch, &origin_source, &verifier).check(&CheckRequest { components: &UPDATABLE, depth: Depth::Quick })?;

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
// Reading a `.rebornroster`: enough zip and enough DEFLATE, read-only
// ---------------------------------------------------------------------------

/// The three counts the Reborn Builder itself reports after an apply
/// (`counts.players`, `counts.teams`, `counts.portraits`), read straight
/// from the export's own `manifest.json` rather than re-derived from
/// `players.json` / `teams.json`, which the export already counted once.
#[derive(Debug)]
struct RosterCounts {
    players: u64,
    teams: u64,
    portraits: u64,
    base_label: Option<String>,
}

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

/// Read `path`, confirm it looks like a `.rebornroster`, and pull the
/// counts out of its `manifest.json` entry. Every failure is a plain
/// sentence, never a panic: a saved 404 page, a truncated download, or an
/// export from a future format this page does not understand all refuse
/// this way instead of crashing the page.
fn read_roster_counts(path: &Path) -> Result<RosterCounts, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;

    let looks_like_zip = bytes.starts_with(b"PK\x03\x04");
    let names_present = [&b"manifest.json"[..], &b"players.json"[..], &b"teams.json"[..]]
        .iter()
        .all(|n| bytes.windows(n.len()).any(|w| w == *n));
    if !looks_like_zip || !names_present {
        return Err(format!(
            "Got {} byte(s) starting {:?}.",
            bytes.len(),
            String::from_utf8_lossy(&bytes[..bytes.len().min(16)])
        ));
    }

    let manifest_bytes = find_zip_entry(&bytes, "manifest.json")?
        .ok_or_else(|| "manifest.json's own local file header could not be found.".to_string())?;
    let manifest: Value =
        serde_json::from_slice(&manifest_bytes).map_err(|e| format!("manifest.json does not parse as JSON: {e}"))?;
    let counts = manifest.get("counts").ok_or_else(|| "manifest.json has no \"counts\" object.".to_string())?;
    let players = counts
        .get("players")
        .and_then(Value::as_u64)
        .ok_or_else(|| "manifest.json's counts.players is missing or not a whole number.".to_string())?;
    let teams = counts
        .get("teams")
        .and_then(Value::as_u64)
        .ok_or_else(|| "manifest.json's counts.teams is missing or not a whole number.".to_string())?;
    let portraits = counts.get("portraits").and_then(Value::as_u64).unwrap_or(0);
    let base_label = manifest.get("base_label").and_then(Value::as_str).map(str::to_string);

    Ok(RosterCounts { players, teams, portraits, base_label })
}

/// The decompressed bytes of `name`'s entry in a zip's local file headers,
/// or `None` when no entry by that name is found. See this module's doc
/// for exactly what this does and does not support.
fn find_zip_entry(bytes: &[u8], name: &str) -> Result<Option<Vec<u8>>, String> {
    let mut pos = 0usize;
    while pos + 4 <= bytes.len() && &bytes[pos..pos + 4] == b"PK\x03\x04" {
        if pos + 30 > bytes.len() {
            return Err("a zip local file header is truncated.".to_string());
        }
        let flags = u16::from_le_bytes([bytes[pos + 6], bytes[pos + 7]]);
        let method = u16::from_le_bytes([bytes[pos + 8], bytes[pos + 9]]);
        let csize = u32::from_le_bytes([bytes[pos + 18], bytes[pos + 19], bytes[pos + 20], bytes[pos + 21]]) as usize;
        let usize_hint = u32::from_le_bytes([bytes[pos + 22], bytes[pos + 23], bytes[pos + 24], bytes[pos + 25]]) as usize;
        let nlen = u16::from_le_bytes([bytes[pos + 26], bytes[pos + 27]]) as usize;
        let elen = u16::from_le_bytes([bytes[pos + 28], bytes[pos + 29]]) as usize;
        let name_start = pos + 30;
        let data_start = name_start + nlen + elen;
        if data_start > bytes.len() || data_start + csize > bytes.len() {
            return Err("a zip entry's header names more data than the file holds.".to_string());
        }
        let entry_name = &bytes[name_start..name_start + nlen];
        if entry_name == name.as_bytes() {
            if flags & 0x8 != 0 {
                return Err(format!("{name} is stored with a streamed data descriptor, which this reader does not support."));
            }
            let raw = &bytes[data_start..data_start + csize];
            let out = match method {
                0 => raw.to_vec(),
                8 => inflate(raw)?,
                other => return Err(format!("{name} uses zip compression method {other}, which this reader does not support.")),
            };
            if out.len() != usize_hint {
                return Err(format!(
                    "{name} decompressed to {} byte(s), not the {usize_hint} the zip header promised.",
                    out.len()
                ));
            }
            return Ok(Some(out));
        }
        pos = data_start + csize;
    }
    Ok(None)
}

/// Reads DEFLATE's bit stream: LSB-first within each byte, per RFC 1951
/// section 3.1.1.
struct BitReader<'a> {
    data: &'a [u8],
    pos: usize,
    bitpos: u32,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0, bitpos: 0 }
    }

    fn bits(&mut self, n: u32) -> Result<u32, String> {
        let mut v: u32 = 0;
        for i in 0..n {
            if self.pos >= self.data.len() {
                return Err("unexpected end of compressed data.".to_string());
            }
            let bit = (self.data[self.pos] >> self.bitpos) & 1;
            v |= (bit as u32) << i;
            self.bitpos += 1;
            if self.bitpos == 8 {
                self.bitpos = 0;
                self.pos += 1;
            }
        }
        Ok(v)
    }

    /// Discard the remaining bits of the current byte, for a stored block.
    fn align(&mut self) {
        if self.bitpos != 0 {
            self.bitpos = 0;
            self.pos += 1;
        }
    }

    fn byte(&mut self) -> Result<u8, String> {
        if self.pos >= self.data.len() {
            return Err("unexpected end of compressed data.".to_string());
        }
        let b = self.data[self.pos];
        self.pos += 1;
        Ok(b)
    }
}

/// A canonical Huffman code table, built the way RFC 1951 section 3.2.2
/// defines one: `counts[len]` codes of each length, `symbols` holding
/// every symbol with a nonzero length, grouped by length and in symbol
/// order within a length.
struct Huffman {
    counts: [u16; 16],
    symbols: Vec<u16>,
}

fn build_huffman(lengths: &[u8]) -> Huffman {
    let mut counts = [0u16; 16];
    for &l in lengths {
        counts[l as usize] += 1;
    }
    counts[0] = 0;
    let mut next = [0usize; 16];
    for len in 1..16 {
        next[len] = next[len - 1] + counts[len - 1] as usize;
    }
    let total: usize = counts.iter().map(|&c| c as usize).sum();
    let mut symbols = vec![0u16; total];
    for (sym, &len) in lengths.iter().enumerate() {
        if len != 0 {
            symbols[next[len as usize]] = sym as u16;
            next[len as usize] += 1;
        }
    }
    Huffman { counts, symbols }
}

/// Decode one symbol, bit by bit, by the incremental-code method Mark
/// Adler's `puff.c` documents: build up `code` one bit at a time and
/// compare it against the range of codes of the current length, without
/// ever needing a code longer than what was actually read.
fn decode_symbol(br: &mut BitReader<'_>, h: &Huffman) -> Result<u16, String> {
    let mut code: i32 = 0;
    let mut first: i32 = 0;
    let mut index: usize = 0;
    for len in 1..=15usize {
        code |= br.bits(1)? as i32;
        let count = h.counts[len] as i32;
        if code - first < count {
            return Ok(h.symbols[index + (code - first) as usize]);
        }
        index += count as usize;
        first = (first + count) << 1;
        code <<= 1;
    }
    Err("invalid Huffman code (no matching length up to 15 bits).".to_string())
}

const LENGTH_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258,
];
const LENGTH_EXTRA: [u8; 29] = [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0];
const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537, 2049, 3073, 4097, 6145, 8193, 12289,
    16385, 24577,
];
const DIST_EXTRA: [u8; 30] = [0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13];

/// RFC 1951 section 3.2.6's fixed tables, used by block type `01`.
fn fixed_tables() -> (Huffman, Huffman) {
    let mut lit_lengths = [0u8; 288];
    lit_lengths[0..144].fill(8);
    lit_lengths[144..256].fill(9);
    lit_lengths[256..280].fill(7);
    lit_lengths[280..288].fill(8);
    let dist_lengths = [5u8; 30];
    (build_huffman(&lit_lengths), build_huffman(&dist_lengths))
}

const CL_ORDER: [usize; 19] = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];

/// RFC 1951 section 3.2.7: the two Huffman tables a dynamic block (`10`)
/// carries ahead of its data, themselves compressed with a third,
/// smaller table over the 19-symbol code-length alphabet.
fn read_dynamic_tables(br: &mut BitReader<'_>) -> Result<(Huffman, Huffman), String> {
    let hlit = br.bits(5)? as usize + 257;
    let hdist = br.bits(5)? as usize + 1;
    let hclen = br.bits(4)? as usize + 4;

    let mut cl_lengths = [0u8; 19];
    for i in 0..hclen {
        cl_lengths[CL_ORDER[i]] = br.bits(3)? as u8;
    }
    let cl_huffman = build_huffman(&cl_lengths);

    let mut lengths: Vec<u8> = Vec::with_capacity(hlit + hdist);
    while lengths.len() < hlit + hdist {
        let sym = decode_symbol(br, &cl_huffman)?;
        match sym {
            0..=15 => lengths.push(sym as u8),
            16 => {
                let prev = *lengths.last().ok_or("a repeat-previous code has no previous length.")?;
                let n = br.bits(2)? + 3;
                for _ in 0..n {
                    lengths.push(prev);
                }
            }
            17 => {
                let n = br.bits(3)? + 3;
                for _ in 0..n {
                    lengths.push(0);
                }
            }
            18 => {
                let n = br.bits(7)? + 11;
                for _ in 0..n {
                    lengths.push(0);
                }
            }
            _ => return Err("bad code-length symbol.".to_string()),
        }
    }
    if lengths.len() != hlit + hdist {
        return Err("code length count did not match HLIT + HDIST.".to_string());
    }
    Ok((build_huffman(&lengths[..hlit]), build_huffman(&lengths[hlit..])))
}

/// Decode one block's symbols (literals, and length/distance
/// back-references) into `out`, until the end-of-block symbol (256).
fn inflate_block(br: &mut BitReader<'_>, lit: &Huffman, dist: &Huffman, out: &mut Vec<u8>) -> Result<(), String> {
    loop {
        let sym = decode_symbol(br, lit)?;
        if sym < 256 {
            out.push(sym as u8);
        } else if sym == 256 {
            return Ok(());
        } else {
            let idx = (sym - 257) as usize;
            if idx >= LENGTH_BASE.len() {
                return Err("bad length code.".to_string());
            }
            let length = LENGTH_BASE[idx] as usize + br.bits(LENGTH_EXTRA[idx] as u32)? as usize;
            let dsym = decode_symbol(br, dist)? as usize;
            if dsym >= DIST_BASE.len() {
                return Err("bad distance code.".to_string());
            }
            let distance = DIST_BASE[dsym] as usize + br.bits(DIST_EXTRA[dsym] as u32)? as usize;
            if distance == 0 || distance > out.len() {
                return Err("a back-reference points further back than any data decoded so far.".to_string());
            }
            let start = out.len() - distance;
            for i in 0..length {
                let b = out[start + i];
                out.push(b);
            }
        }
    }
}

/// Decompress a raw RFC 1951 DEFLATE stream (zip compression method 8; no
/// zlib or gzip wrapper). See this module's doc for why this exists
/// rather than a crate, and this file's own tests for what it was proven
/// against.
fn inflate(data: &[u8]) -> Result<Vec<u8>, String> {
    let mut br = BitReader::new(data);
    let mut out = Vec::new();
    loop {
        let bfinal = br.bits(1)?;
        let btype = br.bits(2)?;
        match btype {
            0 => {
                br.align();
                let len = br.byte()? as usize | ((br.byte()? as usize) << 8);
                let _nlen = br.byte()? as usize | ((br.byte()? as usize) << 8);
                for _ in 0..len {
                    out.push(br.byte()?);
                }
            }
            1 => {
                let (lit, dist) = fixed_tables();
                inflate_block(&mut br, &lit, &dist, &mut out)?;
            }
            2 => {
                let (lit, dist) = read_dynamic_tables(&mut br)?;
                inflate_block(&mut br, &lit, &dist, &mut out)?;
            }
            _ => return Err("reserved DEFLATE block type 3.".to_string()),
        }
        if bfinal == 1 {
            break;
        }
    }
    Ok(out)
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
    let access_key_id = r2_access_key(platform)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "No R2 access key is stored in the vault. Store one on the Keys page before publishing.".to_string())?;
    let secret_access_key = r2_secret_key(platform)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "No R2 secret key is stored in the vault. Store one on the Keys page before publishing.".to_string())?;
    Ok(Bucket { endpoint, bucket: bucket_name, region: "auto".to_string(), access_key_id, secret_access_key })
}

/// The length the bucket already serves `key` at, or `None` when it does
/// not have it. A signed HEAD, the same request shape `history.rs`'s own
/// rollback upload and plan 2's `upload.rs` both use for the same
/// `already` callback.
fn head_len(transport: &dyn upload::Transport, bucket: &Bucket, key: &str) -> Option<u64> {
    let empty_sha = Sha256Hex::from_bytes(&[]);
    let path = upload::object_path(bucket, key);
    let req = upload::sign(bucket, "HEAD", &path, &[], &[], empty_sha.as_str(), SystemTime::now());
    let reply = transport.send(&req, upload::Body::Empty).ok()?;
    if !(200..300).contains(&reply.status) {
        return None;
    }
    reply.headers.iter().find(|(n, _)| n.eq_ignore_ascii_case("content-length")).and_then(|(_, v)| v.parse::<u64>().ok())
}

fn plain_err(message: impl Into<String>) -> reborn_core::Error {
    reborn_core::Error::PlainIo(std::io::Error::new(std::io::ErrorKind::Other, message.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Raw DEFLATE fixtures, generated with Python's `zlib` in raw mode
    /// (`zlib.compressobj(level, zlib.DEFLATED, -15)`), one per block type
    /// this reader has to handle, plus the actual `manifest.json` and
    /// `players.json` entries pulled out of a real
    /// `base-2026.08.28.rebornroster` the site produced. `inflate` was
    /// developed against these before being wired into `read_roster_counts`.
    #[test]
    fn inflate_decodes_a_stored_block() {
        assert_eq!(inflate(&[3, 0]).unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn inflate_decodes_a_fixed_huffman_block() {
        let comp: &[u8] = &[203, 72, 205, 201, 201, 87, 40, 207, 47, 202, 73, 1, 0];
        assert_eq!(inflate(comp).unwrap(), b"hello world");
    }

    #[test]
    fn inflate_decodes_back_references() {
        let comp: &[u8] = &[75, 76, 74, 78, 28, 69, 73, 68, 33, 0];
        let plain = "abc".repeat(100);
        assert_eq!(inflate(comp).unwrap(), plain.as_bytes());
    }

    #[test]
    fn inflate_decodes_a_dynamic_huffman_block() {
        let comp: &[u8] = &[
            237, 211, 203, 17, 212, 48, 20, 5, 209, 84, 84, 179, 246, 194, 87, 127, 145, 10, 197, 2, 162, 160, 138, 34, 119, 186, 69, 24, 224, 173,
            207, 243, 140, 95, 203, 191, 62, 223, 63, 95, 202, 215, 247, 41, 121, 74, 125, 74, 123, 74, 127, 202, 120, 202, 124, 202, 122, 202, 126,
            202, 225, 150, 183, 185, 31, 64, 16, 129, 4, 19, 80, 80, 129, 5, 87, 113, 213, 231, 224, 42, 174, 226, 42, 174, 226, 42, 174, 226, 42,
            174, 225, 26, 174, 249, 131, 184, 134, 107, 184, 134, 107, 184, 134, 107, 184, 142, 235, 184, 142, 235, 254, 51, 92, 199, 117, 92, 199,
            117, 92, 199, 13, 220, 192, 13, 220, 192, 13, 95, 1, 55, 112, 3, 55, 112, 3, 55, 113, 19, 55, 113, 19, 55, 113, 211, 119, 197, 77, 220,
            196, 77, 220, 194, 45, 220, 194, 45, 220, 194, 45, 220, 114, 41, 184, 133, 91, 184, 141, 219, 184, 141, 219, 184, 141, 219, 184, 141,
            219, 110, 15, 183, 113, 7, 119, 112, 7, 119, 112, 7, 119, 112, 7, 119, 112, 199, 53, 223, 61, 187, 232, 215, 77, 191, 174, 250, 117,
            215, 175, 203, 126, 221, 246, 235, 186, 95, 247, 253, 186, 240, 215, 137, 191, 105, 156, 184, 113, 110, 157, 155, 231, 246, 185, 129,
            110, 161, 155, 200, 70, 49, 82, 234, 173, 233, 132, 157, 98, 168, 88, 42, 166, 138, 173, 98, 172, 88, 43, 230, 138, 189, 210, 238, 1,
            112, 194, 100, 177, 89, 140, 22, 171, 197, 108, 177, 91, 12, 23, 203, 197, 116, 233, 247, 204, 56, 97, 189, 152, 47, 246, 139, 1, 99,
            193, 152, 48, 54, 140, 17, 99, 197, 140, 123, 204, 156, 48, 100, 44, 25, 83, 198, 150, 49, 102, 172, 25, 115, 198, 158, 49, 104, 230,
            61, 153, 78, 216, 52, 70, 141, 85, 99, 214, 216, 53, 134, 141, 101, 99, 218, 216, 54, 235, 30, 102, 39, 204, 27, 251, 198, 192, 177,
            112, 76, 28, 27, 199, 200, 177, 114, 204, 156, 125, 207, 191, 19, 150, 142, 169, 99, 235, 24, 59, 214, 142, 185, 99, 239, 24, 60, 22,
            207, 185, 159, 204, 249, 246, 148, 207, 15, 62, 192, 207, 207, 255, 215, 63, 119, 125, 126, 255, 1,
        ];
        // Python's `json.dumps` default separators (`", "`, `": "`), which
        // is what actually produced `comp`; `serde_json`'s compact output
        // has no spaces and would not match the bytes this fixture holds.
        let numbers = (0..200).map(|n: i32| n.to_string()).collect::<Vec<_>>().join(", ");
        let plain = format!("{{\"a\": [{numbers}], \"b\": \"{}\"}}", "x".repeat(500));
        assert_eq!(inflate(comp).unwrap(), plain.as_bytes());
    }

    /// The actual `manifest.json` entry from a real
    /// `base-2026.08.28.rebornroster` (569 players, 51 teams), compressed
    /// bytes and all, so this is proven against a genuine export rather
    /// than only against hand-built fixtures.
    const REAL_MANIFEST_COMPRESSED: &[u8] = &[
        93, 145, 203, 110, 195, 32, 20, 68, 247, 254, 10, 196, 58, 177, 192, 188, 140, 191, 163, 171, 86, 149, 197, 227, 90, 117, 101, 155, 20, 112,
        213, 40, 202, 191, 87, 144, 68, 74, 187, 131, 185, 71, 195, 220, 225, 210, 32, 108, 77, 130, 113, 49, 22, 22, 60, 32, 220, 145, 78, 182,
        164, 111, 187, 30, 31, 26, 132, 93, 216, 183, 156, 240, 128, 46, 13, 66, 248, 180, 152, 51, 196, 114, 21, 82, 31, 170, 18, 98, 142, 102,
        174, 8, 169, 74, 6, 179, 86, 130, 54, 232, 90, 44, 224, 167, 64, 224, 71, 147, 31, 15, 28, 73, 127, 236, 250, 23, 162, 6, 66, 7, 33, 95,
        241, 31, 206, 158, 11, 231, 247, 245, 52, 186, 61, 70, 216, 114, 157, 79, 33, 174, 55, 139, 8, 54, 196, 45, 134, 148, 33, 62, 141, 198, 111,
        136, 105, 14, 27, 30, 16, 45, 234, 10, 171, 189, 165, 125, 14, 223, 126, 166, 138, 224, 244, 97, 58, 33, 7, 223, 83, 0, 169, 39, 7, 154,
        117, 150, 82, 63, 57, 71, 128, 77, 78, 17, 161, 173, 103, 2, 148, 6, 43, 173, 210, 94, 72, 167, 56, 87, 102, 226, 158, 75, 45, 152, 166,
        178, 247, 37, 192, 125, 235, 255, 214, 166, 215, 92, 121, 227, 192, 137, 201, 233, 142, 112, 61, 105, 67, 168, 83, 76, 113, 225, 28, 229,
        78, 122, 207, 172, 98, 132, 251, 73, 90, 174, 25, 183, 222, 18, 174, 13, 56, 71, 9, 197, 247, 6, 79, 49, 216, 5, 106, 171, 111, 239, 69,
        136, 240, 181, 207, 17, 210, 88, 254, 238, 177, 110, 154, 115, 57, 223, 219, 57, 218, 125, 94, 60, 68, 84, 122, 68, 164, 165, 45, 169, 85,
        165, 205, 156, 210, 71, 200, 227, 236, 241, 128, 182, 125, 89, 154, 107, 243, 11,
    ];

    #[test]
    fn inflate_decodes_the_manifest_from_a_real_export() {
        let plain = inflate(REAL_MANIFEST_COMPRESSED).unwrap();
        let json: Value = serde_json::from_slice(&plain).unwrap();
        assert_eq!(json["counts"]["players"], 569);
        assert_eq!(json["counts"]["teams"], 51);
        assert_eq!(json["base_label"], "2026.08.28");
    }

    fn roster_zip_with_manifest(manifest_json: &[u8]) -> Vec<u8> {
        // Every entry stored (method 0), which `find_zip_entry` also
        // supports: only `manifest.json` carries real bytes here, since
        // that is the only entry this page ever reads.
        let mut z = Vec::new();
        let mut add = |name: &str, data: &[u8]| {
            z.extend_from_slice(b"PK\x03\x04");
            z.extend_from_slice(&20u16.to_le_bytes()); // version needed
            z.extend_from_slice(&0u16.to_le_bytes()); // flags
            z.extend_from_slice(&0u16.to_le_bytes()); // method: stored
            z.extend_from_slice(&0u16.to_le_bytes()); // mod time
            z.extend_from_slice(&0u16.to_le_bytes()); // mod date
            z.extend_from_slice(&0u32.to_le_bytes()); // crc32 (unchecked here)
            z.extend_from_slice(&(data.len() as u32).to_le_bytes()); // compressed size
            z.extend_from_slice(&(data.len() as u32).to_le_bytes()); // uncompressed size
            z.extend_from_slice(&(name.len() as u16).to_le_bytes());
            z.extend_from_slice(&0u16.to_le_bytes()); // extra field length
            z.extend_from_slice(name.as_bytes());
            z.extend_from_slice(data);
        };
        add("manifest.json", manifest_json);
        add("players.json", b"[]");
        add("teams.json", b"[]");
        z
    }

    #[test]
    fn read_roster_counts_reads_a_stored_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.rebornroster");
        let manifest = br#"{"counts":{"players":569,"teams":51,"portraits":12},"base_label":"2026.08.28"}"#;
        std::fs::write(&path, roster_zip_with_manifest(manifest)).unwrap();

        let counts = read_roster_counts(&path).unwrap();
        assert_eq!(counts.players, 569);
        assert_eq!(counts.teams, 51);
        assert_eq!(counts.portraits, 12);
        assert_eq!(counts.base_label.as_deref(), Some("2026.08.28"));
    }

    #[test]
    fn read_roster_counts_refuses_junk_before_reading_anything_else() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("404.html");
        std::fs::write(&path, b"<html>not found</html>").unwrap();

        let err = read_roster_counts(&path).unwrap_err();
        assert!(err.contains("byte(s) starting"), "{err}");
    }

    #[test]
    fn read_roster_counts_refuses_a_manifest_with_no_counts() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.rebornroster");
        std::fs::write(&path, roster_zip_with_manifest(b"{}")).unwrap();

        let err = read_roster_counts(&path).unwrap_err();
        assert!(err.contains("\"counts\""), "{err}");
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
