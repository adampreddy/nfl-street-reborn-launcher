//! The Game page: the Reborn Builder's ISO work, and the game patch chain.
//!
//! Two sections, both routed through [`crate::ops::run_operation`] for the
//! steps that touch a whole game image (minutes of work, needs the panel's
//! progress and busy-lock): Build ISO's plan/run, and the game patch
//! chain's plan/run/verify. `game_dump` and `game_cardsync` are the
//! exception: like Keys page's `keys_generate` and `vault_store`, they are
//! synchronous one-shot commands that do not go through `run_operation`,
//! because reading a roster out of an ISO or syncing a small memory card
//! image is a read of kilobytes to a few megabytes, not the 1.4 GB images
//! the other four steps move. This is a deliberate deviation from
//! `ops.rs`'s own module doc, which uses `"game:dump"` as its example of a
//! one-shot op; recorded here rather than silently done, per that same
//! doc's own convention for deviations.
//!
//! # Build ISO's Plan is never truly free
//!
//! `reborn-builder-cli.exe` has no `--dry-run` for a real build: without
//! `--yes`, it still fully builds and writes `--out` the moment the roster
//! has nothing the site flagged (see `reborn_builder/cli.py`: the
//! site-flagged check gates only when there is something to flag). To keep
//! Plan non-destructive regardless of warnings, Plan and a dry-run Run
//! always target a scratch path under the temp directory, never the
//! operator's chosen `out`, and delete it afterward. This costs the same
//! time as a real build when there are no warnings; there is no cheaper
//! preview the tool offers. Only a real (non-dry-run) Run writes to the
//! real `out`.
//!
//! # `card-sync` grew a `dry_run` parameter the brief's sketch did not have
//!
//! The real CLI's `card-sync` needs `--card`, `--update` and `--state`, and
//! offers `--dry-run`; the brief's sketch was `game_cardsync(card,
//! roster)`. `--state` is derived here as `<card>.cardsync-state.json`
//! beside the card image, since main.rs's frozen command list carries no
//! separate input for it and this app has nowhere else to remember it per
//! card. `dry_run` was added because writing to a memory card is exactly
//! the kind of destructive action this task's rules ask to gate behind an
//! explicit, unchecked choice, matching every other write in this app.
//! Neither change touches `main.rs`: a command's parameter list is this
//! file's own to set: nothing in `generate_handler!` fixes an arity.
//!
//! # The chain's base is read, never hard-coded
//!
//! The game patch section only ever asks the operator for the new build
//! (`to`). The base (`from`) is resolved from
//! `settings.remembered["game_patch"]["current_build"]`, falling back to
//! `settings.remembered["environment"]["base_image"]` (the FIELDS 12-19
//! build) the first time, and advanced to `to` only after a real,
//! successful publish. That resolved file is never trusted on its own:
//! `game_patch_plan`/`game_patch_run` hand it straight to
//! `reborn_core::publish::game_patch::publish_game_patch`, which hashes it
//! itself and refuses a mismatch against the chain's own recorded head,
//! showing both hashes. This file does not re-implement that comparison;
//! it only resolves which file to offer as `from` and lets the core decide.
//!
//! # Verify does not re-decode the images
//!
//! `game_patch_verify` does not repeat the multi-minute decode-and-hash
//! `publish_game_patch` already did during Run (and it could not: the
//! superseded base image is not necessarily still on disk once the chain
//! moves past it). Instead it re-hashes every blob the newest published
//! `game-patch` component manifest claims, against the origin's own
//! content-addressed store. That is fast (the chain's files are the
//! manifest, `chain.json`, and each roughly-a-megabyte delta, never a full
//! image) and catches the failure mode a manifest-based check can: bytes
//! that changed or went missing under the origin since they were published.
//!
//! A local store that checks out is not proof a player can fetch any of
//! it: once every blob agrees with the manifest, `game_patch_verify` also
//! reads the hosted release back over HTTPS (the same read `content.rs`'s
//! `content_verify` and `roster.rs`'s `roster_verify` each do) and refuses
//! unless it pins this exact `game-patch` version with a signature that
//! verifies. Without that second read, a patch published locally but never
//! uploaded, or uploaded and then rolled back, would still read as
//! "verified".

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use reborn_core::game_patch::{Chain, CHAIN_FILE};
use reborn_core::inventory::{ComponentId, RunToCompletion};
use reborn_core::library::hash_file;
use reborn_core::manifest::{Component, Sha256Hex};
use reborn_core::net::{Http, UreqHttp};
use reborn_core::origin::blob_path;
use reborn_core::publish::game_patch::{publish_game_patch, GamePatchRequest};
use reborn_core::publish::roster::Push;
use reborn_core::publish::status::read_status;
use reborn_core::publish::upload::{self, Bucket, UploadRequest, Uploaded, UreqTransport};
use reborn_core::publish::vault::{r2_access, r2_secret};
use reborn_core::publish::{out, refuse, Outcome, Sink, COMPONENTS_DIR};
use reborn_core::signing::{Ed25519Verifier, TRUSTED_KEYS};
use reborn_core::{Error, ExitKind, Platform};

use crate::settings::{settings_path, Settings};

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn str_err(e: String) -> Error {
    Error::PlainIo(std::io::Error::other(e))
}

fn load_settings_with_path(platform: &Platform) -> reborn_core::Result<(PathBuf, Settings)> {
    let path = settings_path(platform).map_err(str_err)?;
    let settings = Settings::load(&path).map_err(str_err)?;
    Ok((path, settings))
}

fn load_settings(platform: &Platform) -> reborn_core::Result<Settings> {
    Ok(load_settings_with_path(platform)?.1)
}

/// A path as the operator typed it, made absolute. `run_capturing` sets the
/// child's working directory to the *program's* own folder (see
/// `platform/imp.rs`), not this process's, so a relative path handed to
/// `reborn-builder-cli.exe` would resolve beside the exe rather than
/// wherever the operator meant. Every path this file passes to it goes
/// through here first.
fn absolute_path(p: &str) -> PathBuf {
    let path = PathBuf::from(p);
    if path.is_absolute() {
        path
    } else {
        std::env::current_dir().map(|cwd| cwd.join(&path)).unwrap_or(path)
    }
}

/// `reborn-builder-cli.exe`: the configured path if one is set (even if it
/// is presently missing, which is a more specific problem than "never
/// configured"), else the usual location beside the launcher's tools
/// folder, matching `status.rs`'s own `builder_item` search.
fn resolve_builder(settings: &Settings) -> Result<PathBuf, String> {
    if let Some(p) = settings.builder.as_deref().filter(|s| !s.is_empty()) {
        let path = PathBuf::from(p);
        return if path.is_file() {
            Ok(path)
        } else {
            Err(format!(
                "The Reborn Builder is configured at {p}, but no file is there now. Fix it on the Status page."
            ))
        };
    }
    if let Some(appdata) = std::env::var_os("APPDATA") {
        let candidate = PathBuf::from(appdata).join("NFL Street Reborn").join("Tools").join("reborn-builder-cli.exe");
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    Err(
        "The Reborn Builder (reborn-builder-cli.exe) was not found. Set it on the Status page's \
         This Machine block."
            .to_string(),
    )
}

/// The pristine build this machine currently has, to offer as the game
/// patch chain's `from`. See this file's module doc: never a hard-coded
/// fact, only a starting point the core's own hash-and-chain check either
/// accepts or refuses.
fn resolve_current_build(settings: &Settings) -> Result<PathBuf, String> {
    let from_remembered = |key: &str, field: &str| -> Option<PathBuf> {
        let p = settings.remembered.get(key)?.get(field)?.as_str()?;
        let path = PathBuf::from(p);
        path.is_file().then_some(path)
    };
    if let Some(p) = from_remembered("game_patch", "current_build") {
        return Ok(p);
    }
    if let Some(p) = from_remembered("environment", "base_image") {
        return Ok(p);
    }
    Err(
        "No base image is known for the chain's current build. Set the Base image (FIELDS \
         12-19) on the Status page's This Machine block, or complete a game patch once to \
         establish one."
            .to_string(),
    )
}

/// Deletes the file it names when dropped. Used for a scratch build output
/// that must not outlive the call that made it, on every path out
/// (success, refusal, or an early `?`).
struct ScratchGuard(PathBuf);

impl Drop for ScratchGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn scratch_iso_path(tag: &str) -> PathBuf {
    std::env::temp_dir().join("reborn-publish").join(format!("game-build-{tag}-{}.iso", std::process::id()))
}

fn non_empty(s: &Option<String>) -> Option<String> {
    s.as_ref().map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

fn user_agent() -> String {
    concat!("reborn-publisher/", env!("CARGO_PKG_VERSION")).to_string()
}

/// The R2 bucket this app uploads to, from settings plus the vault. See
/// `roster.rs`'s own copy of this helper for why credentials are read here
/// rather than passed in as a command argument: a `#[tauri::command(rename_all = "snake_case")]`
/// argument is exactly what a browser devtools console in front of this
/// window could set to anything.
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


/// Whether `pins` (a hosted release's own list, or a local one) names `id`
/// at exactly `version`. Pulled out of [`game_patch_verify_body`] so the
/// exact-match rule (not "any version", not "any component whose name
/// matches") has its own name and its own tests.
fn release_pins_version(pins: &[(ComponentId, String)], id: ComponentId, version: &str) -> bool {
    pins.iter().any(|(cid, v)| *cid == id && v == version)
}

// ---------------------------------------------------------------------------
// Build ISO: reading what the Builder printed
// ---------------------------------------------------------------------------

/// What was pulled out of `reborn-builder-cli`'s own stdout/stderr. Never
/// trusted as this app's authority on the result (the hash is always
/// recomputed by this app; see [`hash_file`] calls below), only shown for
/// context.
#[derive(Debug, Default)]
struct BuildParsed {
    warnings: Vec<String>,
    players: Option<u64>,
    teams: Option<u64>,
    portraits: Option<u64>,
    base_label: Option<String>,
}

/// "%d players, %d teams, %d portraits written." -> the three counts.
fn parse_counts_line(line: &str) -> Option<(u64, u64, u64)> {
    let parts: Vec<&str> = line.splitn(3, ", ").collect();
    if parts.len() != 3 {
        return None;
    }
    let p = parts[0].split_whitespace().next()?.parse().ok()?;
    let t = parts[1].split_whitespace().next()?.parse().ok()?;
    let k = parts[2].split_whitespace().next()?.parse().ok()?;
    Some((p, t, k))
}

/// `reborn-builder`'s successful stdout: counts, base label, and any
/// "N warning(s) from the site:" block (site-flagged problems plus
/// anything the build itself noticed, per `pipeline.py`).
fn parse_build_stdout(stdout: &str) -> BuildParsed {
    let mut parsed = BuildParsed::default();
    let lines: Vec<&str> = stdout.lines().collect();
    for (i, raw) in lines.iter().enumerate() {
        let t = raw.trim();
        if t.ends_with(" portraits written.") {
            if let Some((p, te, k)) = parse_counts_line(t) {
                parsed.players = Some(p);
                parsed.teams = Some(te);
                parsed.portraits = Some(k);
            }
        } else if let (Some(a), Some(b)) = (t.find(" - base "), t.find(" - sha256 ")) {
            if b > a {
                parsed.base_label = Some(t[a + 8..b].to_string());
            }
        } else if t.ends_with("warning(s) from the site:") {
            let mut j = i + 1;
            while j < lines.len() && lines[j].starts_with("  ") {
                parsed.warnings.push(lines[j].trim_start().to_string());
                j += 1;
            }
        }
    }
    parsed
}

/// The site-flagged problem list from a `--yes`-less run that stopped
/// (exit code 3): every two-space-indented line.
fn parse_flagged_stderr(stderr: &str) -> Vec<String> {
    stderr.lines().filter(|l| l.starts_with("  ")).map(|l| l.trim_start().to_string()).collect()
}

// ---------------------------------------------------------------------------
// Build ISO: plan
// ---------------------------------------------------------------------------

/// Build ISO, plan: what the Builder would do with this base image, roster
/// and output path, and its warnings.
#[tauri::command(rename_all = "snake_case")]
pub fn game_build_plan(base: String, roster: String, out: String) -> Result<Value, String> {
    let inputs = json!({"base": base, "roster": roster, "out": out});
    crate::ops::run_operation("game_build:plan", false, inputs, move |sink| game_build_plan_body(&base, &roster, &out, sink))?;
    Ok(json!({"accepted": true}))
}

fn game_build_plan_body(base: &str, roster: &str, out: &str, sink: Sink<'_>) -> reborn_core::Result<Outcome<Value>> {
    let settings = load_settings(&Platform::detect())?;
    let builder = match resolve_builder(&settings) {
        Ok(p) => p,
        Err(e) => return Ok(refuse(sink, &[e])),
    };
    let base_path = absolute_path(base);
    let roster_path = absolute_path(roster);
    if !base_path.is_file() {
        return Ok(refuse(sink, &[format!("Base image {} is not a file. Nothing was checked.", base_path.display())]));
    }
    if !roster_path.is_file() {
        return Ok(refuse(sink, &[format!("Roster {} is not a file. Nothing was checked.", roster_path.display())]));
    }
    if out.trim().is_empty() {
        return Ok(refuse(sink, &["An output ISO path is required.".to_string()]));
    }

    let scratch = scratch_iso_path("plan");
    if let Some(parent) = scratch.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let _guard = ScratchGuard(scratch.clone());

    out!(sink, "Builder       {}", builder.display());
    out!(sink, "Base image    {}", base_path.display());
    out!(sink, "Roster        {}", roster_path.display());
    out!(sink);
    out!(sink, "Checking for site-flagged problems. This app never writes to {out} during");
    out!(sink, "Plan; a scratch copy stands in for it, deleted when this step finishes.");
    out!(sink);
    let platform = Platform::detect();
    let run = platform.processes.run_capturing(
        &builder,
        &[
            OsString::from("--iso"),
            base_path.as_os_str().to_os_string(),
            OsString::from("--update"),
            roster_path.as_os_str().to_os_string(),
            OsString::from("--out"),
            scratch.as_os_str().to_os_string(),
        ],
        &Default::default(),
    )?;

    match run.exit {
        ExitKind::Normal => {
            for line in run.stdout.lines() {
                out!(sink, "{line}");
            }
            let parsed = parse_build_stdout(&run.stdout);
            let preview_sha = if scratch.is_file() { hash_file(&scratch).ok() } else { None };
            let summary = format!(
                "No problems flagged. Preview build: {} player(s), {} team(s), base {}.",
                parsed.players.unwrap_or_default(),
                parsed.teams.unwrap_or_default(),
                parsed.base_label.as_deref().unwrap_or("?")
            );
            out!(sink);
            out!(sink, "{summary}");
            Ok(Outcome::Done(json!({
                "summary": summary,
                "warnings": parsed.warnings,
                "players": parsed.players,
                "teams": parsed.teams,
                "portraits": parsed.portraits,
                "base_label": parsed.base_label,
                "preview_sha256": preview_sha.map(|h| h.as_str().to_string()),
            })))
        }
        ExitKind::Error { code: 3 } => {
            for line in run.stderr.lines() {
                out!(sink, "{line}");
            }
            let warnings = parse_flagged_stderr(&run.stderr);
            let summary = format!(
                "{} problem(s) the site flagged. Nothing was built. Review them, then Build anyway.",
                warnings.len()
            );
            out!(sink);
            out!(sink, "{summary}");
            Ok(Outcome::Done(json!({
                "summary": summary,
                "warnings": warnings,
                "players": Value::Null,
                "teams": Value::Null,
                "portraits": Value::Null,
                "base_label": Value::Null,
                "preview_sha256": Value::Null,
            })))
        }
        other => Ok(refuse(
            sink,
            &[format!("reborn-builder-cli stopped ({other:?}). Nothing was built."), run.stdout, run.stderr],
        )),
    }
}

// ---------------------------------------------------------------------------
// Build ISO: run
// ---------------------------------------------------------------------------

/// Build ISO, run: build it for real (or, with `dry_run`, into a scratch
/// copy again), and hash the result ourselves.
#[tauri::command(rename_all = "snake_case")]
pub fn game_build_run(base: String, roster: String, out: String, dry_run: bool) -> Result<Value, String> {
    let inputs = json!({"base": base, "roster": roster, "out": out, "dry_run": dry_run});
    crate::ops::run_operation("game_build:run", dry_run, inputs, move |sink| {
        game_build_run_body(&base, &roster, &out, dry_run, sink)
    })?;
    Ok(json!({"accepted": true}))
}

fn game_build_run_body(base: &str, roster: &str, out: &str, dry_run: bool, sink: Sink<'_>) -> reborn_core::Result<Outcome<Value>> {
    let settings = load_settings(&Platform::detect())?;
    let builder = match resolve_builder(&settings) {
        Ok(p) => p,
        Err(e) => return Ok(refuse(sink, &[e])),
    };
    let base_path = absolute_path(base);
    let roster_path = absolute_path(roster);
    if !base_path.is_file() {
        return Ok(refuse(sink, &[format!("Base image {} is not a file. Nothing was built.", base_path.display())]));
    }
    if !roster_path.is_file() {
        return Ok(refuse(sink, &[format!("Roster {} is not a file. Nothing was built.", roster_path.display())]));
    }
    if out.trim().is_empty() {
        return Ok(refuse(sink, &["An output ISO path is required.".to_string()]));
    }
    let real_out = absolute_path(out);

    let target = if dry_run { scratch_iso_path("dry-run") } else { real_out.clone() };
    let _guard = dry_run.then(|| ScratchGuard(target.clone()));
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }

    out!(sink, "Builder       {}", builder.display());
    out!(sink, "Base image    {}", base_path.display());
    out!(sink, "Roster        {}", roster_path.display());
    if dry_run {
        out!(sink, "Output        {}  (dry run: a scratch copy, not the real output)", target.display());
    } else {
        out!(sink, "Output        {}", target.display());
    }
    out!(sink);
    out!(sink, "Building. This reads the base image once and writes a full new one; it can");
    out!(sink, "take a few minutes.");

    let platform = Platform::detect();
    let run = platform.processes.run_capturing(
        &builder,
        &[
            OsString::from("--iso"),
            base_path.as_os_str().to_os_string(),
            OsString::from("--update"),
            roster_path.as_os_str().to_os_string(),
            OsString::from("--out"),
            target.as_os_str().to_os_string(),
            OsString::from("--yes"),
        ],
        &Default::default(),
    )?;
    for line in run.stdout.lines() {
        out!(sink, "{line}");
    }
    for line in run.stderr.lines() {
        out!(sink, "ERR  {line}");
    }

    if run.exit != ExitKind::Normal {
        return Ok(refuse(sink, &[format!("reborn-builder-cli stopped ({:?}). Nothing was built.", run.exit)]));
    }
    if !target.is_file() {
        return Ok(refuse(sink, &["The builder reported success but the output file is not there.".to_string()]));
    }

    out!(sink);
    out!(sink, "Hashing the result ourselves.");
    let sha = hash_file(&target)?;
    let parsed = parse_build_stdout(&run.stdout);

    let summary = if dry_run {
        format!(
            "(dry run) would write {} - sha256 {} - {} player(s), {} team(s).",
            real_out.display(),
            sha.as_str(),
            parsed.players.unwrap_or_default(),
            parsed.teams.unwrap_or_default()
        )
    } else {
        format!(
            "Built {} - sha256 {} - {} player(s), {} team(s).",
            real_out.display(),
            sha.as_str(),
            parsed.players.unwrap_or_default(),
            parsed.teams.unwrap_or_default()
        )
    };
    out!(sink, "{summary}");

    let mut value = json!({
        "summary": summary,
        "out": real_out.display().to_string(),
        "sha256": sha.as_str(),
        "players": parsed.players,
        "teams": parsed.teams,
        "portraits": parsed.portraits,
        "base_label": parsed.base_label,
        "warnings": parsed.warnings,
        "dry_run": dry_run,
    });
    if !dry_run {
        value["hashes"] = json!([["out", sha.as_str()]]);
    }
    Ok(Outcome::Done(value))
}

// ---------------------------------------------------------------------------
// Dump and card-sync: synchronous one-shot commands (see module doc)
// ---------------------------------------------------------------------------

/// Dump an ISO's current roster through the Builder, for inspection.
/// Read-only on the ISO; writes only the new `.rebornroster` at `out`.
#[tauri::command(rename_all = "snake_case")]
pub fn game_dump(iso: String, out: String) -> Result<Value, String> {
    let platform = Platform::detect();
    let settings = Settings::load(&settings_path(&platform)?)?;
    let builder = resolve_builder(&settings)?;

    let iso_path = absolute_path(&iso);
    if !iso_path.is_file() {
        return Err(format!("{} is not a file.", iso_path.display()));
    }
    if out.trim().is_empty() {
        return Err("An output .rebornroster path is required.".to_string());
    }
    let out_path = absolute_path(&out);
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let sha = hash_file(&iso_path).map_err(|e| e.to_string())?;
    let run = platform
        .processes
        .run_capturing(
            &builder,
            &[
                OsString::from("dump"),
                OsString::from("--iso"),
                iso_path.as_os_str().to_os_string(),
                OsString::from("--out"),
                out_path.as_os_str().to_os_string(),
            ],
            &Default::default(),
        )
        .map_err(|e| e.to_string())?;
    if run.exit != ExitKind::Normal {
        return Err(format!("reborn-builder-cli dump stopped ({:?}): {}{}", run.exit, run.stdout, run.stderr));
    }

    Ok(json!({
        "summary": run.stdout.trim(),
        "iso": iso_path.display().to_string(),
        "iso_sha256": sha.as_str(),
        "out": out_path.display().to_string(),
    }))
}

/// Sync a memory card's gear and faces into a roster file through the
/// Builder, keeping in-game edits. `--state` is derived beside the card
/// image; see this file's module doc. Defaults to a dry run.
#[tauri::command(rename_all = "snake_case")]
pub fn game_cardsync(card: String, roster: String, dry_run: bool) -> Result<Value, String> {
    let platform = Platform::detect();
    let settings = Settings::load(&settings_path(&platform)?)?;
    let builder = resolve_builder(&settings)?;

    let card_path = absolute_path(&card);
    if !card_path.is_file() {
        return Err(format!("{} is not a file.", card_path.display()));
    }
    let roster_path = absolute_path(&roster);
    if !roster_path.is_file() {
        return Err(format!("{} is not a file.", roster_path.display()));
    }
    let state_path = card_path.with_extension("cardsync-state.json");

    let mut args: Vec<OsString> = vec![
        OsString::from("card-sync"),
        OsString::from("--card"),
        card_path.as_os_str().to_os_string(),
        OsString::from("--update"),
        roster_path.as_os_str().to_os_string(),
        OsString::from("--state"),
        state_path.as_os_str().to_os_string(),
    ];
    if dry_run {
        args.push(OsString::from("--dry-run"));
    }
    let run = platform.processes.run_capturing(&builder, &args, &Default::default()).map_err(|e| e.to_string())?;
    if run.exit != ExitKind::Normal {
        return Err(format!("reborn-builder-cli card-sync stopped ({:?}): {}{}", run.exit, run.stdout, run.stderr));
    }

    Ok(json!({
        "summary": run.stdout.lines().last().unwrap_or("").to_string(),
        "lines": run.stdout.lines().collect::<Vec<_>>(),
        "card": card_path.display().to_string(),
        "state": state_path.display().to_string(),
        "dry_run": dry_run,
    }))
}

// ---------------------------------------------------------------------------
// Game patch: plan
// ---------------------------------------------------------------------------

/// Game patch, plan: hash the new build, read the chain's head, and refuse
/// a base that is not the chain's tip, with both hashes shown. Delegates
/// entirely to `publish_game_patch` with `apply: false`, which already does
/// exactly this and nothing more (see this file's module doc); no chain
/// rule is re-implemented here.
#[tauri::command(rename_all = "snake_case")]
pub fn game_patch_plan(to: String) -> Result<Value, String> {
    let inputs = json!({"to": to});
    crate::ops::run_operation("game_patch:plan", false, inputs, move |sink| game_patch_plan_body(&to, sink))?;
    Ok(json!({"accepted": true}))
}

fn game_patch_plan_body(to: &str, sink: Sink<'_>) -> reborn_core::Result<Outcome<Value>> {
    let platform = Platform::detect();
    let settings = load_settings(&platform)?;
    let from_path = match resolve_current_build(&settings) {
        Ok(p) => p,
        Err(e) => return Ok(refuse(sink, &[e])),
    };
    let to_path = absolute_path(to);
    if !to_path.is_file() {
        return Ok(refuse(sink, &[format!("{} is not a file. Nothing was checked.", to_path.display())]));
    }
    let origin = PathBuf::from(&settings.origin);
    let xdelta = settings.xdelta.as_deref().map(Path::new);

    let req = GamePatchRequest {
        from: &from_path,
        to: &to_path,
        version: "(preview)",
        tree: None,
        xdelta,
        origin: &origin,
        sequence: None,
        key: None,
        release_version: None,
        release_sequence: None,
        apply: false,
    };
    match publish_game_patch(&platform, &req, &RunToCompletion, sink)? {
        Outcome::Refused(r) => Ok(Outcome::Refused(r)),
        Outcome::Described => Ok(Outcome::Done(json!({
            "summary": "No problems. This build is ready to publish as a game patch.",
            "from": from_path.display().to_string(),
            "to": to_path.display().to_string(),
        }))),
        Outcome::Done(_) => Ok(refuse(
            sink,
            &["publish_game_patch reported a completed publish during a plan step (apply was false). \
               Treating this as a refusal rather than trusting an unexpected result."
                .to_string()],
        )),
    }
}

// ---------------------------------------------------------------------------
// Game patch: run
// ---------------------------------------------------------------------------

/// Game patch, run: encode, decode-verify, publish, re-sign, upload.
#[tauri::command(rename_all = "snake_case")]
pub fn game_patch_run(to: String, version: String, dry_run: bool) -> Result<Value, String> {
    let platform = Platform::detect();
    let settings_file = settings_path(&platform)?;
    let settings = Settings::load(&settings_file)?;

    // Resolved up front, before anything is encoded: a patch that hashes,
    // deltas and chains for real (several minutes of work) and only then
    // discovers there is no bucket configured would leave the origin's
    // chain ahead of what the bucket actually serves, with no way to catch
    // up from here. Same reasoning `roster.rs`'s own `roster_run` uses.
    let bucket = if dry_run { None } else { Some(bucket_from(&platform, &settings)?) };
    let public_base = settings.public_base.clone();

    let inputs = json!({"to": to, "version": version, "dry_run": dry_run});
    crate::ops::run_operation("game_patch:run", dry_run, inputs, move |sink| {
        game_patch_run_body(&to, &version, dry_run, sink, settings_file, settings, bucket, &public_base)
    })?;
    Ok(json!({"accepted": true}))
}

fn push_to_json(push: &Push) -> Value {
    json!({
        "component": {
            "manifest_path": push.component.manifest_path.display().to_string(),
            "sha256": push.component.sha256.as_str(),
            "files": push.component.files,
            "bytes": push.component.bytes,
            "new_blobs": push.component.new_blobs,
            "new_bytes": push.component.new_bytes,
            "shared_blobs": push.component.shared_blobs,
            "shared_bytes": push.component.shared_bytes,
        },
        "release": push.release.as_ref().map(|s| json!({
            "version": s.version,
            "sequence": s.sequence,
            "public_hex": s.public_hex,
            "shipped": s.shipped,
            "files": s.files,
            "bytes": s.bytes,
        })),
    })
}

fn game_patch_run_body(
    to: &str,
    version: &str,
    dry_run: bool,
    sink: Sink<'_>,
    settings_file: PathBuf,
    mut settings: Settings,
    bucket: Option<Bucket>,
    public_base: &str,
) -> reborn_core::Result<Outcome<Value>> {
    let platform = Platform::detect();
    let from_path = match resolve_current_build(&settings) {
        Ok(p) => p,
        Err(e) => return Ok(refuse(sink, &[e])),
    };
    let to_path = absolute_path(to);
    if !to_path.is_file() {
        return Ok(refuse(sink, &[format!("{} is not a file. Nothing was published.", to_path.display())]));
    }
    if version.trim().is_empty() {
        return Ok(refuse(sink, &["A version label is required, for example today's date.".to_string()]));
    }
    let origin = PathBuf::from(&settings.origin);
    let xdelta = settings.xdelta.as_deref().map(Path::new);
    let key = settings.publisher_key.as_deref().map(Path::new);
    if !dry_run && key.is_none() {
        return Ok(refuse(
            sink,
            &["No publisher.key is configured. Set it on the Keys page before publishing for real, \
               or check dry run to preview only."
                .to_string()],
        ));
    }

    out!(sink, "Computing the new build's hash ourselves, not trusting any tool's claim.");
    let to_sha = hash_file(&to_path)?;
    out!(sink, "To is         {to_sha}");

    let req = GamePatchRequest {
        from: &from_path,
        to: &to_path,
        version,
        tree: None,
        xdelta,
        origin: &origin,
        sequence: None,
        key,
        release_version: None,
        release_sequence: None,
        apply: !dry_run,
    };
    match publish_game_patch(&platform, &req, &RunToCompletion, sink)? {
        Outcome::Refused(r) => Ok(Outcome::Refused(r)),
        Outcome::Described => Ok(Outcome::Done(json!({
            "summary": format!("(dry run) would publish {version} from the current build to {}.", to_path.display()),
            "dry_run": true,
            "to_sha256": to_sha.as_str(),
        }))),
        Outcome::Done(push) => {
            if !dry_run {
                // The next patch's default base moves forward only because
                // this real, successful publish moved it. See this file's
                // module doc: never a hard-coded fact.
                let mut game_patch_settings = settings.remembered.get("game_patch").cloned().unwrap_or_else(|| json!({}));
                if !game_patch_settings.is_object() {
                    // Never trust an external value's shape before indexing into
                    // it: `Value`'s `IndexMut` panics on anything but an object,
                    // and a panic on this worker thread would leave `OpState`
                    // stuck busy forever (see this file's and `ops.rs`'s own
                    // "never panic" rule). A shape this file did not itself
                    // write is replaced rather than trusted.
                    game_patch_settings = json!({});
                }
                game_patch_settings["current_build"] = json!(to_path.display().to_string());
                settings.remembered.insert("game_patch".to_string(), game_patch_settings);
                // Best effort: the publish already happened and stands on
                // its own terms regardless of whether this bookkeeping
                // write succeeds.
                let _ = settings.save(&settings_file);
            }

            let transport = UreqTransport::new();
            let http2 = UreqHttp::new(&user_agent());
            finish_game_patch_run(&transport, &http2, push, &to_sha, version, dry_run, &origin, bucket.as_ref(), public_base, sink)
        }
    }
}

/// The upload half of Run, once `publish_game_patch` has already published
/// the component and re-signed the release: with no bucket, say so and stop
/// (a dry run, the only time `bucket` is `None`); otherwise plan and upload
/// exactly what `roster.rs`'s own `roster_run` does, folding the result into
/// the same JSON shape `push_to_json` builds. Split out of
/// [`game_patch_run_body`] so a test can hand it a fake [`upload::Transport`]
/// and [`Http`] instead of a real socket -- both traits exist for exactly
/// this (see `upload.rs`'s own doc comment on [`upload::Transport`]).
#[allow(clippy::too_many_arguments)]
fn finish_game_patch_run(
    transport: &dyn upload::Transport,
    http: &dyn Http,
    push: Push,
    to_sha: &Sha256Hex,
    version: &str,
    dry_run: bool,
    origin: &Path,
    bucket: Option<&Bucket>,
    public_base: &str,
    sink: Sink<'_>,
) -> reborn_core::Result<Outcome<Value>> {
    // `req.apply` was `!dry_run`, and `publish_game_patch` only returns
    // `Outcome::Done` when `apply` was true, so in `game_patch_run_body`'s
    // own caller `dry_run` is always false here and `bucket` (resolved the
    // same way, in `game_patch_run`) is always `Some`. Handled explicitly
    // anyway rather than indexed into blindly: see this crate's "never
    // panic" rule, and this file's own tests exercise this branch directly.
    let Some(bucket) = bucket else {
        let mut value = push_to_json(&push);
        value["summary"] = json!(format!("Published game patch {version} locally. Not uploaded: this was a dry run."));
        value["dry_run"] = json!(dry_run);
        value["hashes"] = json!([["to", to_sha.as_str()]]);
        return Ok(Outcome::Done(value));
    };

    let (already, _how) = upload::already_on_bucket(transport, bucket)?;
    let plan = upload::plan_origin(origin, &*already)?;
    let upload_req = UploadRequest {
        bucket,
        plan: &plan,
        multipart_threshold: upload::DEFAULT_MULTIPART_THRESHOLD,
        part_size: upload::DEFAULT_PART_SIZE,
        attempts: upload::DEFAULT_ATTEMPTS,
        public_base: Some(public_base),
        apply: true,
    };
    let uploaded = match upload::upload(transport, http, &upload_req, sink)? {
        Outcome::Refused(r) => return Ok(Outcome::Refused(r)),
        Outcome::Described => Uploaded::default(),
        Outcome::Done(u) => u,
    };

    let mut value = push_to_json(&push);
    let sequence = push.release.as_ref().map(|s| s.sequence);
    value["summary"] = json!(match sequence {
        Some(seq) => format!(
            "Published game patch {version} at release sequence {seq}: {} object(s), {} byte(s) uploaded.",
            uploaded.objects, uploaded.bytes
        ),
        None => format!(
            "Published game patch {version} (component only; no key to sign a release): {} object(s), {} byte(s) uploaded.",
            uploaded.objects, uploaded.bytes
        ),
    });
    value["dry_run"] = json!(dry_run);
    value["hashes"] = json!([["to", to_sha.as_str()]]);
    if let Some(seq) = sequence {
        value["sequence"] = json!(seq);
    }
    value["uploaded"] = json!(plan.items().map(|i| i.key.clone()).collect::<Vec<_>>());
    value["verified"] = json!(uploaded.verified.first().cloned());
    Ok(Outcome::Done(value))
}

// ---------------------------------------------------------------------------
// Game patch: verify
// ---------------------------------------------------------------------------

/// Game patch, verify: re-hash every blob the newest published `game-patch`
/// component manifest claims, against the origin's own store. See this
/// file's module doc for why this does not re-decode the images.
#[tauri::command(rename_all = "snake_case")]
pub fn game_patch_verify() -> Result<Value, String> {
    crate::ops::run_operation("game_patch:verify", false, json!({}), move |sink| game_patch_verify_body(sink))?;
    Ok(json!({"accepted": true}))
}

/// The highest-sequence `game-patch-*.json` manifest under `origin`'s
/// components folder, parsed. A manifest of some OTHER component, or one
/// that fails to parse, is skipped rather than treated as a hard error:
/// this is a best-known-good lookup, not the strict origin-wide inventory
/// `next_sequence` performs.
fn latest_game_patch_manifest(origin: &Path) -> reborn_core::Result<Option<(PathBuf, Component)>> {
    let dir = origin.join(COMPONENTS_DIR);
    let prefix = format!("{}-", ComponentId::GamePatch.as_str());
    let mut best: Option<(u64, PathBuf, Component)> = None;
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Ok(None);
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with(&prefix) || !name.ends_with(".json") {
            continue;
        }
        let path = entry.path();
        let bytes = std::fs::read(&path)?;
        let Ok(component) = Component::parse(&bytes) else {
            continue;
        };
        if best.as_ref().map(|(seq, _, _)| component.sequence > *seq).unwrap_or(true) {
            best = Some((component.sequence, path, component));
        }
    }
    Ok(best.map(|(_, path, component)| (path, component)))
}

fn game_patch_verify_body(sink: Sink<'_>) -> reborn_core::Result<Outcome<Value>> {
    let settings = load_settings(&Platform::detect())?;
    let origin = PathBuf::from(&settings.origin);
    let public_base = settings.public_base.clone();
    let launcher_manifest = settings.launcher_manifest.clone();

    let Some((manifest_path, component)) = latest_game_patch_manifest(&origin)? else {
        return Ok(refuse(sink, &["No game-patch component has been published to this origin yet. Nothing to verify.".to_string()]));
    };
    out!(sink, "Manifest      {}", manifest_path.display());
    out!(sink, "Version       {} sequence {}", component.version, component.sequence);
    out!(sink, "Checking {} file(s) in the origin's blob store against this manifest.", component.files.len());

    let mut mismatches: Vec<String> = Vec::new();
    let mut chain_report: Option<(usize, String, String)> = None;
    for file in &component.files {
        let blob = origin.join(blob_path(&file.sha256));
        match std::fs::metadata(&blob) {
            Err(_) => mismatches.push(format!("{}: blob is missing at {}", file.path, blob.display())),
            Ok(m) if m.len() != file.bytes => mismatches.push(format!(
                "{}: {} is {} bytes, the manifest says {}",
                file.path,
                blob.display(),
                m.len(),
                file.bytes
            )),
            Ok(_) => match hash_file(&blob) {
                Ok(h) if h == file.sha256 => {
                    if file.path == CHAIN_FILE {
                        if let Ok(chain_bytes) = std::fs::read(&blob) {
                            if let Ok(chain) = Chain::parse(&chain_bytes) {
                                chain_report = Some((
                                    chain.steps.len(),
                                    chain.base().unwrap_or("(empty)").to_string(),
                                    chain.head().unwrap_or("(empty)").to_string(),
                                ));
                            }
                        }
                    }
                }
                Ok(h) => {
                    mismatches.push(format!("{}: {} hashes to {h}, the manifest says {}", file.path, blob.display(), file.sha256))
                }
                Err(e) => mismatches.push(format!("{}: {} could not be hashed: {e}", file.path, blob.display())),
            },
        }
    }

    if !mismatches.is_empty() {
        let mut lines = vec![format!("{} of {} file(s) did not verify against the origin:", mismatches.len(), component.files.len())];
        lines.extend(mismatches);
        return Ok(refuse(sink, &lines));
    }

    // The local store checking out is not proof a player can fetch any of
    // it: read the hosted release back over HTTPS, the same read
    // `content_verify` and `roster_verify` each do, and refuse unless it
    // pins this exact game-patch version with a signature that verifies.
    // See this file's module doc.
    out!(sink, "Checking      {public_base} pins game-patch at {}", component.version);
    let http = UreqHttp::new(&user_agent());
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
    if !hosted.verified {
        return Ok(refuse(sink, &[format!("The hosted release at sequence {} did not verify: {}", hosted.sequence, problem_text())]));
    }
    if !release_pins_version(&hosted.pins, ComponentId::GamePatch, &component.version) {
        return Ok(refuse(sink, &[format!(
            "{public_base} does not pin game-patch at {}. The upload may not have completed; \
             re-run Run (Run always encodes, publishes and uploads together).",
            component.version
        )]));
    }

    let summary = match &chain_report {
        Some((steps, base, head)) => format!(
            "{} file(s) verified. Chain has {steps} step(s), base {}\u{2026}, head {}\u{2026}. {public_base} \
             serves {} with a verified signature.",
            component.files.len(),
            &base[..12.min(base.len())],
            &head[..12.min(head.len())],
            component.version
        ),
        None => format!(
            "{} file(s) verified. {public_base} serves {} with a verified signature.",
            component.files.len(),
            component.version
        ),
    };
    out!(sink, "{summary}");
    Ok(Outcome::Done(json!({
        "summary": summary,
        "verified": summary,
        "version": component.version,
        "sequence": component.sequence,
        "release_sequence": hosted.sequence,
        "files_checked": component.files.len(),
        "chain_steps": chain_report.as_ref().map(|(s, _, _)| *s),
        "chain_base": chain_report.as_ref().map(|(_, b, _)| b.clone()),
        "chain_head": chain_report.as_ref().map(|(_, _, h)| h.clone()),
    })))
}

#[cfg(test)]
mod tests {
    //! Two things here are genuinely new logic this task adds, not glue
    //! over an already-tested `reborn_core` function, so they are what
    //! this module's own tests target (the same policy `set.rs`'s own
    //! test module states): [`release_pins_version`], the exact-match rule
    //! Verify's new hosted-pin check turns on, and [`finish_game_patch_run`],
    //! the upload half of Run that used to not exist at all. Both are
    //! exercised without a real socket: [`upload::Transport`] and
    //! [`Http`] are narrow traits for exactly this (see `upload.rs`'s own
    //! doc comment on [`upload::Transport`]), so a `FakeTransport` and a
    //! `FakeHttp` stand in for the bucket and the public CDN the way
    //! `set.rs`'s own tests stand a fake script in for `build.ps1`.

    use super::*;
    use reborn_core::manifest::{ManifestFile, COMPONENT_SCHEMA};
    use reborn_core::origin::{RELEASE_FILE, SIGNATURE_FILE};
    use reborn_core::publish::component::Published;
    use reborn_core::publish::Event;

    // -- release_pins_version ------------------------------------------

    #[test]
    fn release_pins_version_true_only_for_an_exact_component_and_version_match() {
        let pins = vec![(ComponentId::GamePatch, "2026.09.09".to_string()), (ComponentId::Roster, "2026.09.01".to_string())];
        assert!(release_pins_version(&pins, ComponentId::GamePatch, "2026.09.09"));
        // Same component, a different version: not a match. This is the
        // exact failure mode a rolled-back or not-yet-uploaded patch takes.
        assert!(!release_pins_version(&pins, ComponentId::GamePatch, "2026.09.08"));
        // A component the release does not pin at all.
        assert!(!release_pins_version(&pins, ComponentId::Launcher, "1.0.0"));
    }

    #[test]
    fn release_pins_version_false_against_an_empty_release() {
        assert!(!release_pins_version(&[], ComponentId::GamePatch, "2026.09.09"));
    }

    // -- finish_game_patch_run, against fakes rather than a real socket --

    #[derive(Debug, Default)]
    struct FakeTransport;

    impl upload::Transport for FakeTransport {
        fn send(&self, _req: &upload::SignedRequest, _body: upload::Body<'_>) -> reborn_core::Result<upload::Reply> {
            // Every HEAD (the "already" check) and every PUT this test's
            // small files ever trigger is answered the same plain way a
            // fresh bucket that has nothing yet would: a bare 200, no
            // headers. `head_len` reads that as "not already there" (no
            // content-length to report), so the plan always includes
            // everything this test seeds, and `put_single` reads any 2xx
            // as success. Real signature validation is the server's job,
            // not this fake's.
            Ok(upload::Reply { status: 200, headers: Vec::new(), body: Vec::new() })
        }
    }

    /// Answers every read-back GET with the same bytes, so
    /// `upload::upload`'s own read-back check (which compares the reply
    /// byte for byte against `release.json` on disk) matches without this
    /// test needing to fake a real edge.
    struct FakeHttp {
        body: String,
    }

    impl Http for FakeHttp {
        fn get_text(&self, _url: &str) -> reborn_core::Result<reborn_core::net::HttpResponse> {
            Ok(reborn_core::net::HttpResponse { status: 200, body: self.body.clone() })
        }
        fn get_to_file(&self, url: &str, _to: &Path) -> reborn_core::Result<u64> {
            Err(str_err(format!("{url}: this test's FakeHttp never downloads a whole file")))
        }
        fn post_json(&self, url: &str, _body: &[u8], _bearer: Option<&str>) -> reborn_core::Result<reborn_core::net::HttpResponse> {
            Err(str_err(format!("{url}: this test's FakeHttp never posts")))
        }
    }

    fn test_bucket() -> Bucket {
        Bucket {
            endpoint: "https://example-bucket.invalid".to_string(),
            bucket: "example".to_string(),
            region: "auto".to_string(),
            access_key_id: "AKIAEXAMPLE".to_string(),
            secret_access_key: "example-secret".to_string(),
        }
    }

    /// A local origin holding exactly what a real `publish_game_patch`
    /// would have just written: one blob, one component manifest, and a
    /// signed release -- the shape `upload::plan_origin` (which
    /// `finish_game_patch_run` calls) scans for. Returns the release
    /// bytes too, since the fake HTTP read-back needs to answer with them.
    fn seed_published_origin(origin: &Path, version: &str) -> (Vec<u8>, u64) {
        let blob_bytes = b"a tiny fake game-patch delta, standing in for a real xdelta3 step";
        let blob_sha = Sha256Hex::from_bytes(blob_bytes);
        let blob_full = origin.join(blob_path(&blob_sha));
        std::fs::create_dir_all(blob_full.parent().unwrap()).unwrap();
        std::fs::write(&blob_full, blob_bytes).unwrap();

        std::fs::create_dir_all(origin.join(COMPONENTS_DIR)).unwrap();
        let component = Component {
            schema: COMPONENT_SCHEMA.to_string(),
            component: ComponentId::GamePatch.as_str().to_string(),
            version: version.to_string(),
            sequence: 1,
            install_root: ComponentId::GamePatch.install_root().unwrap().to_string(),
            customizable: false,
            files: vec![ManifestFile { path: CHAIN_FILE.to_string(), bytes: blob_bytes.len() as u64, sha256: blob_sha }],
        };
        let manifest_bytes = serde_json::to_vec(&component).unwrap();
        std::fs::write(origin.join(COMPONENTS_DIR).join(format!("game-patch-{version}.json")), &manifest_bytes).unwrap();

        let release_bytes = format!(r#"{{"schema":"nsr.release.v1","version":"{version}","sequence":1}}"#).into_bytes();
        std::fs::write(origin.join(RELEASE_FILE), &release_bytes).unwrap();
        std::fs::write(origin.join(SIGNATURE_FILE), b"not-a-real-signature-this-is-a-test").unwrap();

        // blob + manifest + release.json + release.json.sig
        let total_objects = 4;
        (release_bytes, total_objects)
    }

    fn fake_push(version: &str) -> Push {
        Push {
            component: Published {
                manifest_path: PathBuf::from(format!("game-patch-{version}.json")),
                sha256: Sha256Hex::from_bytes(b"fake component manifest bytes"),
                files: 1,
                bytes: 66,
                new_blobs: 1,
                new_bytes: 66,
                shared_blobs: 0,
                shared_bytes: 0,
            },
            release: None,
        }
    }

    #[test]
    fn finish_game_patch_run_with_no_bucket_reports_a_dry_run_and_uploads_nothing() {
        let push = fake_push("2026.09.09");
        let to_sha = Sha256Hex::from_bytes(b"the new build's bytes");
        let dir = tempfile::tempdir().unwrap();
        let origin = dir.path().join("origin");
        let transport = FakeTransport;
        let http = FakeHttp { body: String::new() };
        let mut lines = Vec::new();
        let mut sink = |e: Event| lines.push(e.line);

        let outcome =
            finish_game_patch_run(&transport, &http, push, &to_sha, "2026.09.09", true, &origin, None, "https://files.example.invalid", &mut sink)
                .unwrap();

        let value = match outcome {
            Outcome::Done(v) => v,
            other => panic!("expected Done, got {other:?}"),
        };
        assert!(value["summary"].as_str().unwrap().contains("Not uploaded: this was a dry run."), "{value}");
        assert!(value["dry_run"].as_bool().unwrap());
        // No HTTP or transport call was made at all: had one been, the
        // fake's own answers would have let it through silently, so the
        // real proof is that nothing in `origin` (which does not even
        // exist on disk here) was ever needed.
        assert!(!origin.exists());
    }

    #[test]
    fn finish_game_patch_run_with_a_bucket_uploads_the_whole_plan_and_reports_it() {
        let dir = tempfile::tempdir().unwrap();
        let origin = dir.path().join("origin");
        let version = "2026.09.09";
        let (release_bytes, total_objects) = seed_published_origin(&origin, version);

        let push = fake_push(version);
        let to_sha = Sha256Hex::from_bytes(b"the new build's bytes");
        let bucket = test_bucket();
        let transport = FakeTransport;
        let http = FakeHttp { body: String::from_utf8(release_bytes).unwrap() };
        let mut lines = Vec::new();
        let mut sink = |e: Event| lines.push(e.line);

        let outcome = finish_game_patch_run(
            &transport,
            &http,
            push,
            &to_sha,
            version,
            false,
            &origin,
            Some(&bucket),
            "https://files.example.invalid",
            &mut sink,
        )
        .unwrap();

        let value = match outcome {
            Outcome::Done(v) => v,
            other => panic!("expected Done, got {other:?}"),
        };
        let summary = value["summary"].as_str().unwrap();
        assert!(summary.contains(&format!("{total_objects} object(s)")), "{summary}");
        assert!(summary.contains("no key to sign a release"), "{summary}");
        let uploaded = value["uploaded"].as_array().unwrap();
        assert_eq!(uploaded.len() as u64, total_objects, "{uploaded:?}");
        assert!(uploaded.iter().any(|k| k.as_str() == Some(RELEASE_FILE)), "{uploaded:?}");
        assert!(uploaded.iter().any(|k| k.as_str() == Some(SIGNATURE_FILE)), "{uploaded:?}");
        // The read-back matched byte for byte, so `verified` names it.
        assert!(value["verified"].as_str().unwrap().contains(RELEASE_FILE), "{value}");
        assert!(!value["dry_run"].as_bool().unwrap());
        // No release was signed in this test's `Push`, so there is nothing
        // for the journal to key a release sequence on.
        assert!(value.get("sequence").is_none());
    }

    // -- latest_game_patch_manifest --------------------------------------

    fn write_manifest(origin: &Path, version: &str, sequence: u64) {
        std::fs::create_dir_all(origin.join(COMPONENTS_DIR)).unwrap();
        let component = Component {
            schema: COMPONENT_SCHEMA.to_string(),
            component: ComponentId::GamePatch.as_str().to_string(),
            version: version.to_string(),
            sequence,
            install_root: ComponentId::GamePatch.install_root().unwrap().to_string(),
            customizable: false,
            files: Vec::new(),
        };
        std::fs::write(
            origin.join(COMPONENTS_DIR).join(format!("game-patch-{version}.json")),
            serde_json::to_vec(&component).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn latest_game_patch_manifest_picks_the_highest_sequence_present() {
        let dir = tempfile::tempdir().unwrap();
        let origin = dir.path().join("origin");
        write_manifest(&origin, "2026.09.01", 1);
        write_manifest(&origin, "2026.09.09", 3);
        write_manifest(&origin, "2026.09.05", 2);

        let (_, component) = latest_game_patch_manifest(&origin).unwrap().unwrap();
        assert_eq!(component.version, "2026.09.09");
        assert_eq!(component.sequence, 3);
    }

    #[test]
    fn latest_game_patch_manifest_is_none_before_anything_is_published() {
        let dir = tempfile::tempdir().unwrap();
        let origin = dir.path().join("origin-does-not-exist");
        assert!(latest_game_patch_manifest(&origin).unwrap().is_none());
    }
}
