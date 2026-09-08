//! Content updates as the window drives them: check, fetch, promote.
//!
//! The pipeline (`reborn_core::content::pipeline`) does the work. What lives
//! here is the state between the three calls, held in one place in the
//! process rather than in a page, because the launcher is several pages and
//! a person navigating from Play to Settings while a download runs must not
//! lose it. Any page asks `content_status` and gets the same answer.
//!
//! Promotion takes the launch guard for its duration, so the game cannot be
//! started while files are being swapped, and it refuses to start while a
//! game is running, which is the one rule the pipeline leaves to its caller.

use std::path::PathBuf;
use std::sync::Mutex;

use serde::Serialize;

use reborn_core::content::pipeline::{
    CheckRequest, ComponentState, ContentCheck, Depth, FetchOutcomeState, GamePatchResult, ImageHook,
    PromoteOutcome, PromoteState, RosterResult, Updater, UPDATABLE,
};
use reborn_core::origin::{Origin, DEFAULT_ORIGIN};
use reborn_core::signing::Ed25519Verifier;
use reborn_core::Platform;

use crate::LaunchGuard;

#[derive(Debug)]
enum Phase {
    Idle,
    Checking,
    UpToDate,
    Available,
    Fetching,
    Staged,
    Installing,
    Installed(InstalledSummary),
    Problem(String),
}

impl Phase {
    fn name(&self) -> &'static str {
        match self {
            Phase::Idle => "idle",
            Phase::Checking => "checking",
            Phase::UpToDate => "up_to_date",
            Phase::Available => "available",
            Phase::Fetching => "fetching",
            Phase::Staged => "staged",
            Phase::Installing => "installing",
            Phase::Installed(_) => "installed",
            Phase::Problem(_) => "problem",
        }
    }
}

/// One component as the page shows it.
#[derive(Debug, Clone, Serialize)]
pub struct ComponentView {
    pub component: String,
    /// "up_to_date", "available", "not_carried", "not_updatable", "refused", "problem".
    pub state: &'static str,
    pub installed: Option<String>,
    pub offered: Option<String>,
    pub files: u64,
    pub bytes: u64,
    /// A repair of the installed version rather than a move to a new one.
    pub repair: bool,
    pub message: Option<String>,
}

/// What a completed installation did, kept until the person dismisses it.
#[derive(Debug, Clone, Serialize)]
pub struct InstalledSummary {
    pub components: Vec<String>,
    pub game_patch: Option<String>,
    pub roster: Option<String>,
    pub problems: Vec<String>,
    pub note: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContentStatusView {
    /// "idle", "checking", "up_to_date", "available", "fetching", "staged",
    /// "installing", "installed", "problem".
    pub phase: &'static str,
    pub origin: String,
    pub release: Option<String>,
    pub components: Vec<ComponentView>,
    pub download_bytes: u64,
    pub download_files: u64,
    pub repair_only: bool,
    pub note: String,
    pub installed: Option<InstalledSummary>,
}

/// What a check found, kept in a shape the page can be shown while the
/// check itself is out on loan to a fetch or a promotion.
#[derive(Debug, Clone, Default)]
struct Snapshot {
    release: Option<String>,
    components: Vec<ComponentView>,
    download_bytes: u64,
    download_files: u64,
    repair_only: bool,
}

#[derive(Debug)]
struct Held {
    phase: Phase,
    origin: String,
    snapshot: Snapshot,
    /// The check the later steps are made from. Taken out for the duration
    /// of a fetch or a promotion and put back after, so the lock is never
    /// held across the network.
    check: Option<ContentCheck>,
    /// A message worth showing in the available phase: why the last fetch
    /// did not complete.
    note: Option<String>,
}

static HELD: Mutex<Held> = Mutex::new(Held {
    phase: Phase::Idle,
    origin: String::new(),
    snapshot: Snapshot {
        release: None,
        components: Vec::new(),
        download_bytes: 0,
        download_files: 0,
        repair_only: false,
    },
    check: None,
    note: None,
});

fn held() -> std::sync::MutexGuard<'static, Held> {
    HELD.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn mb(bytes: u64) -> u64 {
    (bytes + 1_048_575) / 1_048_576
}

fn view(h: &Held) -> ContentStatusView {
    let s = &h.snapshot;
    let note = match &h.phase {
        Phase::Idle => "Not checked yet.".to_string(),
        Phase::Checking => "Checking for content updates…".to_string(),
        Phase::UpToDate => match &s.release {
            Some(r) => format!("Everything is current (release {r})."),
            None => "Everything is current.".to_string(),
        },
        Phase::Available => {
            let what = if s.repair_only {
                format!("{} file(s) need to be downloaded again, {} MB.", s.download_files, mb(s.download_bytes))
            } else {
                format!("Update available: {} MB across {} file(s).", mb(s.download_bytes), s.download_files)
            };
            match &h.note {
                Some(n) => format!("{what} {n}"),
                None => what,
            }
        }
        Phase::Fetching => format!("Downloading {} MB…", mb(s.download_bytes)),
        Phase::Staged => {
            if crate::session_active() {
                format!("Update ready. {} MB. Installs the next time you close the game.", mb(s.download_bytes))
            } else {
                format!("Update ready. {} MB, verified. Installs when you say so, or on its own if updates are automatic.", mb(s.download_bytes))
            }
        }
        Phase::Installing => "Installing the update…".to_string(),
        Phase::Installed(summary) => summary.note.clone(),
        Phase::Problem(m) => m.clone(),
    };
    ContentStatusView {
        phase: h.phase.name(),
        origin: h.origin.clone(),
        release: s.release.clone(),
        components: s.components.clone(),
        download_bytes: s.download_bytes,
        download_files: s.download_files,
        repair_only: s.repair_only,
        note,
        installed: match &h.phase {
            Phase::Installed(summary) => Some(summary.clone()),
            _ => None,
        },
    }
}

fn snapshot_of(check: &ContentCheck) -> Snapshot {
    let components = check
        .components
        .iter()
        .map(|c| {
            let (state, offered, files, bytes, repair, message) = match &c.state {
                ComponentState::UpToDate { version, .. } => ("up_to_date", Some(version.clone()), 0, 0, false, None),
                ComponentState::Available(plan) => (
                    "available",
                    Some(plan.version().to_string()),
                    plan.fetches.len() as u64,
                    plan.download_bytes(),
                    plan.is_repair(),
                    None,
                ),
                ComponentState::NotCarried => ("not_carried", None, 0, 0, false, None),
                ComponentState::NotUpdatable => ("not_updatable", None, 0, 0, false, None),
                ComponentState::Refused(r) => ("refused", None, 0, 0, false, Some(r.message())),
                ComponentState::Problem(m) => ("problem", None, 0, 0, false, Some(m.clone())),
            };
            ComponentView {
                component: c.component.as_str().to_string(),
                state,
                installed: c.installed.as_ref().map(|v| v.version.clone()),
                offered,
                files,
                bytes,
                repair,
                message,
            }
        })
        .collect();
    Snapshot {
        release: Some(format!("{} sequence {}", check.release.version, check.release.sequence)),
        components,
        download_bytes: check.download_bytes(),
        download_files: check.download_files(),
        repair_only: check.is_repair_only(),
    }
}

/// Where content comes from: the setting when a person set one, else the
/// product's origin.
fn origin_url(settings: &reborn_core::settings::Settings) -> String {
    settings
        .content_origin
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(DEFAULT_ORIGIN)
        .to_string()
}

fn load_settings(p: &Platform) -> Result<reborn_core::settings::Settings, String> {
    let file = reborn_core::settings::settings_path(p).map_err(|e| e.to_string())?;
    reborn_core::settings::Settings::load(&file).map_err(|e| e.to_string())
}

fn verifier() -> Result<Ed25519Verifier, String> {
    let v = Ed25519Verifier::embedded();
    if !v.has_trust_anchor() {
        return Err(
            "This build carries no publisher key, so no content release can be trusted and nothing was fetched."
                .to_string(),
        );
    }
    Ok(v)
}

/// The tools the image is rebuilt with, and where portraits go.
fn image_hook(p: &Platform, settings: &reborn_core::settings::Settings) -> ImageHook {
    let mut extra_dirs: Vec<PathBuf> = Vec::new();
    if let Some(exe_dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(|d| d.to_path_buf())) {
        extra_dirs.push(exe_dir.join(reborn_core::game_patch::BUNDLE_DIR));
        extra_dirs.push(exe_dir.join("tools"));
        extra_dirs.push(exe_dir);
    }
    if let Some(legacy) = &settings.legacy_root {
        extra_dirs.push(legacy.join("Tools"));
    }
    if let Ok(appdata) = std::env::var("APPDATA") {
        extra_dirs.push(PathBuf::from(appdata).join("NFL Street Reborn").join("Tools"));
    }
    let textures_root = reborn_core::mods::GameId::parse("GN7E69")
        .ok()
        .and_then(|game| reborn_core::mods::pristine_root(p, &game).ok());
    ImageHook {
        builder: None,
        xdelta: None,
        extra_dirs,
        textures_root,
    }
}

fn run_check(depth: Depth) -> Result<(String, ContentCheck), String> {
    let p = Platform::detect();
    let settings = load_settings(&p)?;
    let url = origin_url(&settings);
    let verifier = verifier()?;
    let data_root = p.paths.data_root().map_err(|e| e.to_string())?;
    // What the installer shipped beside the launcher, into the data root
    // by hardlink, before anything is planned. Without this the first check
    // on a fresh install plans the whole texture pack again (measured:
    // 3.18 GB). Idempotent, and a failure here is logged, not fatal: the
    // check still runs, it just plans more.
    if let Ok(install_root) = p.paths.install_root() {
        match reborn_core::content::seed::seed_from_install(&p, &data_root, &install_root, "GN7E69") {
            Ok(r) if !r.seeded.is_empty() => {
                for s in &r.seeded {
                    eprintln!("seeded {} from {} ({} files, {:?})", s.component.as_str(), s.from.display(), s.files, s.how);
                }
            }
            Ok(_) => {}
            Err(e) => eprintln!("seeding from the install root failed: {e}"),
        }
    }
    let origin = Origin::new(&url);
    let updater = Updater::new(&p, &data_root, &origin, &verifier);
    let mut check = updater
        .check(&CheckRequest {
            components: &UPDATABLE,
            depth,
        })
        .map_err(|e| e.to_string())?;
    // Adoption. A component whose files are all already right but which has
    // no pointer yet (the seeded trees, on a first run) plans an update of
    // zero bytes. Promote it here and now: nothing is downloaded, nothing is
    // swapped, the pointer and the manifest cache are written, and the
    // player never sees an "update" of 0 MB. Only when no game is running,
    // which is what the launch guard says.
    if check.has_updates() && check.download_bytes() == 0 && !crate::session_active() {
        if let Ok(guard) = LaunchGuard::acquire() {
            let settings = load_settings(&p)?;
            let hook = image_hook(&p, &settings);
            let adopted = updater
                .fetch(&check)
                .and_then(|_| updater.promote(&check, &hook))
                .map_err(|e| e.to_string());
            drop(guard);
            match adopted {
                Ok(po) if po.all_applied() => {
                    check = updater
                        .check(&CheckRequest {
                            components: &UPDATABLE,
                            depth: Depth::Quick,
                        })
                        .map_err(|e| e.to_string())?;
                }
                Ok(po) => eprintln!("adoption of the shipped content did not fully apply: {po:?}"),
                Err(e) => eprintln!("adoption of the shipped content failed: {e}"),
            }
        }
    }
    Ok((url, check))
}

/// Where things stand. Reads nothing from disk or the network.
#[tauri::command]
pub fn content_status() -> ContentStatusView {
    view(&held())
}

/// Open the release and plan every component. `depth` is "verify" to hash
/// the live tree even when the versions match (the repair path), anything
/// else for the quick comparison that runs at startup.
#[tauri::command]
pub async fn content_check(depth: Option<String>) -> Result<ContentStatusView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        {
            let mut h = held();
            if matches!(h.phase, Phase::Checking | Phase::Fetching | Phase::Installing) {
                return Ok(view(&h));
            }
            h.phase = Phase::Checking;
            h.check = None;
            h.note = None;
        }
        let depth = match depth.as_deref().map(str::trim) {
            Some("verify") => Depth::Verify,
            _ => Depth::Quick,
        };
        let result = run_check(depth);
        let mut h = held();
        match result {
            Ok((url, check)) => {
                h.origin = url;
                h.snapshot = snapshot_of(&check);
                let problems = check.problems();
                h.phase = if check.has_updates() {
                    Phase::Available
                } else if !problems.is_empty() {
                    Phase::Problem(problems.join(" "))
                } else {
                    Phase::UpToDate
                };
                h.check = Some(check);
            }
            Err(e) => {
                h.phase = Phase::Problem(e);
            }
        }
        Ok(view(&h))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Download and stage everything the check found. Writes only under
/// `downloads` and `staging`, so it runs while the game is up. Resumes
/// whatever a previous attempt left.
#[tauri::command]
pub async fn content_fetch() -> Result<ContentStatusView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let (url, check) = {
            let mut h = held();
            if !matches!(h.phase, Phase::Available) {
                return Ok(view(&h));
            }
            let Some(check) = h.check.take() else {
                h.phase = Phase::Problem("The check has expired. Check again.".to_string());
                return Ok(view(&h));
            };
            h.phase = Phase::Fetching;
            h.note = None;
            (h.origin.clone(), check)
        };
        let p = Platform::detect();
        let outcome = (|| -> Result<_, String> {
            let verifier = verifier()?;
            let data_root = p.paths.data_root().map_err(|e| e.to_string())?;
            let origin = Origin::new(&url);
            let updater = Updater::new(&p, &data_root, &origin, &verifier);
            updater.fetch(&check).map_err(|e| e.to_string())
        })();
        let mut h = held();
        h.check = Some(check);
        match outcome {
            Ok(f) if f.all_staged() => h.phase = Phase::Staged,
            Ok(f) => {
                let mut parts = Vec::new();
                for c in f.not_staged() {
                    match &c.state {
                        FetchOutcomeState::Incomplete { incomplete, corrupt, remaining_bytes } => parts.push(format!(
                            "{}: {} file(s) did not arrive whole and {} were corrupt; {} MB still to fetch, and the next attempt resumes.",
                            c.component.as_str(),
                            incomplete.len(),
                            corrupt.len(),
                            mb(*remaining_bytes)
                        )),
                        FetchOutcomeState::NotReady { missing, wrong } => parts.push(format!(
                            "{}: the staged files did not verify ({} missing, {} wrong). Download again.",
                            c.component.as_str(),
                            missing.len(),
                            wrong.len()
                        )),
                        FetchOutcomeState::Staged { .. } => {}
                    }
                }
                h.phase = Phase::Available;
                h.note = Some(parts.join(" "));
            }
            Err(e) => {
                h.phase = Phase::Available;
                h.note = Some(format!("The download stopped: {e} The next attempt resumes."));
            }
        }
        Ok(view(&h))
    })
    .await
    .map_err(|e| e.to_string())?
}

fn summarize(outcome: PromoteOutcome) -> InstalledSummary {
    let mut components = Vec::new();
    let mut problems = Vec::new();
    let mut game_patch = None;
    let mut roster = None;
    for c in outcome.components {
        match c.state {
            PromoteState::Applied { receipt, game_patch: gp, roster: r } => {
                components.push(format!("{} {}", receipt.component, receipt.version));
                if let Some(gp) = gp {
                    let m = gp.message();
                    if !gp.reached_the_image() {
                        problems.push(m.clone());
                    }
                    game_patch = Some(match gp {
                        GamePatchResult::Applied(r) => format!("Game updated to build {}.", r.steps.last().cloned().unwrap_or_default()),
                        _ => m,
                    });
                }
                if let Some(r) = r {
                    let m = r.message();
                    if !r.reached_the_image() {
                        problems.push(m.clone());
                    }
                    roster = Some(match r {
                        RosterResult::Applied(a) => format!("Roster applied: {} players, {} teams.", a.report.players, a.report.teams),
                        RosterResult::Unchanged { .. } => "The roster was already in the game.".to_string(),
                        RosterResult::NothingToApply => m,
                        _ => m,
                    });
                }
            }
            PromoteState::Refused(r) => problems.push(format!("{}: {}", c.component.as_str(), r.message())),
            PromoteState::RolledBack { reason, restored } => problems.push(format!(
                "{}: the swap failed part way and was reversed ({reason}); {restored} file(s) put back, nothing lost.",
                c.component.as_str()
            )),
        }
    }
    let mut note = if components.is_empty() {
        "Nothing was installed.".to_string()
    } else {
        format!("Updated: {}.", components.join(", "))
    };
    if let Some(g) = &game_patch {
        if game_patch_is_good(g) {
            note.push(' ');
            note.push_str(g);
        }
    }
    if let Some(r) = &roster {
        if r.starts_with("Roster applied") || r.starts_with("The roster was already") {
            note.push(' ');
            note.push_str(r);
        }
    }
    if !problems.is_empty() {
        note.push(' ');
        note.push_str(&problems.join(" "));
    }
    InstalledSummary {
        components,
        game_patch,
        roster,
        problems,
        note,
    }
}

fn game_patch_is_good(message: &str) -> bool {
    message.starts_with("Game updated") || message.starts_with("The game is already")
}

/// Swap the staged files into the live install and rebuild the image.
///
/// Refused while a game session is active; holds the launch guard so no
/// session can start until the swap is done.
#[tauri::command]
pub async fn content_promote(confirm: bool) -> Result<ContentStatusView, String> {
    crate::confirmed(confirm, "content_promote", "swap the downloaded update into the live installation")?;
    tauri::async_runtime::spawn_blocking(move || {
        let guard = LaunchGuard::acquire()
            .map_err(|_| "The game is running. The update installs after it closes.".to_string())?;
        let (url, check) = {
            let mut h = held();
            if !matches!(h.phase, Phase::Staged) {
                return Ok(view(&h));
            }
            let Some(check) = h.check.take() else {
                h.phase = Phase::Problem("The check has expired. Check again.".to_string());
                return Ok(view(&h));
            };
            h.phase = Phase::Installing;
            (h.origin.clone(), check)
        };
        let p = Platform::detect();
        let outcome = (|| -> Result<_, String> {
            let settings = load_settings(&p)?;
            let verifier = verifier()?;
            let data_root = p.paths.data_root().map_err(|e| e.to_string())?;
            let origin = Origin::new(&url);
            let updater = Updater::new(&p, &data_root, &origin, &verifier);
            let hook = image_hook(&p, &settings);
            updater.promote(&check, &hook).map_err(|e| e.to_string())
        })();
        drop(guard);
        let mut h = held();
        match outcome {
            Ok(po) => {
                h.phase = Phase::Installed(summarize(po));
                h.check = None;
            }
            Err(e) => {
                h.phase = Phase::Problem(format!("The update could not be installed: {e}"));
                h.check = Some(check);
            }
        }
        Ok(view(&h))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Put the installed summary away.
#[tauri::command]
pub fn content_dismiss() -> ContentStatusView {
    let mut h = held();
    if matches!(h.phase, Phase::Installed(_)) {
        h.phase = Phase::Idle;
        h.snapshot = Snapshot::default();
    }
    view(&h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn megabytes_round_up_so_a_small_update_never_reads_as_zero() {
        assert_eq!(mb(0), 0);
        assert_eq!(mb(1), 1);
        assert_eq!(mb(1_048_576), 1);
        assert_eq!(mb(1_048_577), 2);
    }

    #[test]
    fn a_summary_names_what_landed_and_keeps_problems_visible() {
        let s = summarize(PromoteOutcome::default());
        assert_eq!(s.note, "Nothing was installed.");
        assert!(s.problems.is_empty());
    }

    #[test]
    fn the_origin_is_the_product_one_unless_a_person_set_another() {
        let mut settings = reborn_core::settings::Settings::default();
        assert_eq!(origin_url(&settings), DEFAULT_ORIGIN);
        settings.content_origin = Some("   ".into());
        assert_eq!(origin_url(&settings), DEFAULT_ORIGIN, "blank means the default");
        settings.content_origin = Some("http://127.0.0.1:8080/".into());
        assert_eq!(origin_url(&settings), "http://127.0.0.1:8080/");
    }
}
