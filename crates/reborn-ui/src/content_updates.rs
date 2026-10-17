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

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::Serialize;

use reborn_core::content::pipeline::{
    CheckRequest, ComponentCheck, ComponentState, ContentCheck, Depth, FetchOutcomeState, GamePatchResult,
    updatable, ImageHook, PromoteOutcome, PromoteState, RosterResult, Updater,
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
    /// Bytes of this component in staging while a fetch runs (capped at
    /// `bytes`), so the setup screen can say which parts are done. Zero
    /// outside a fetch.
    pub fetched_bytes: u64,
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
    /// What the memory cards said once a new roster or build was in the
    /// image: gear rows brought up to date, in-game edits kept, or why a
    /// card was left as it was. Empty when nothing changed or it was quiet.
    pub cards: Vec<String>,
    pub note: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContentStatusView {
    /// "idle", "checking", "up_to_date", "available", "fetching", "staged",
    /// "installing", "installed", "problem".
    pub phase: &'static str,
    pub origin: String,
    pub release: Option<String>,
    /// The publisher's words for this release, from the signed manifest.
    /// None when the release carries none: the page lists components then.
    pub notes: Option<String>,
    pub components: Vec<ComponentView>,
    pub download_bytes: u64,
    /// Bytes on disk in staging while a fetch runs, so the page can show a
    /// percentage. Zero outside a fetch.
    pub fetched_bytes: u64,
    pub download_files: u64,
    pub repair_only: bool,
    pub note: String,
    pub installed: Option<InstalledSummary>,
}

/// What a running fetch has landed so far: the size of the staging tree.
/// Measured rather than counted, so nothing in the pipeline has to report
/// progress and a fetch that resumed still reads right. Capped at the plan.
fn staged_bytes(cap: u64) -> u64 {
    let Ok(root) = reborn_core::Platform::detect().paths.data_root() else { return 0 };
    tree_bytes(&root.join(reborn_core::DataDir::Staging.relative())).min(cap)
}

/// The same measure for one component: staging is `<component>/<version>`.
fn staged_component_bytes(component: &str, cap: u64) -> u64 {
    let Ok(root) = reborn_core::Platform::detect().paths.data_root() else { return 0 };
    tree_bytes(&root.join(reborn_core::DataDir::Staging.relative()).join(component)).min(cap)
}

fn tree_bytes(top: &std::path::Path) -> u64 {
    let mut total = 0u64;
    let mut stack = vec![top.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if let Ok(meta) = entry.metadata() {
                total += meta.len();
            }
        }
    }
    total
}

/// What a check found, kept in a shape the page can be shown while the
/// check itself is out on loan to a fetch or a promotion.
#[derive(Debug, Clone, Default)]
struct Snapshot {
    release: Option<String>,
    notes: Option<String>,
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
        notes: None,
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

pub(crate) fn close_state() -> &'static str {
    match &held().phase {
        Phase::Fetching => "downloading",
        Phase::Installing => "installing",
        _ => "idle",
    }
}

fn mb(bytes: u64) -> u64 {
    (bytes + 1_048_575) / 1_048_576
}

fn view(h: &Held) -> ContentStatusView {
    let s = &h.snapshot;
    let fetched_bytes = if matches!(h.phase, Phase::Fetching) { staged_bytes(s.download_bytes) } else { 0 };
    let percent = if s.download_bytes > 0 { (fetched_bytes * 100 / s.download_bytes).min(99) } else { 0 };
    let note = match &h.phase {
        Phase::Idle => "Not checked yet.".to_string(),
        Phase::Checking => {
            "Checking for content updates… On a fresh install the first check also verifies the shipped \
             files, 3 GB of them, which takes a few minutes once."
                .to_string()
        }
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
        Phase::Fetching => format!("Downloading {} MB… {percent}%", mb(s.download_bytes)),
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
        notes: s.notes.clone(),
        components: s
            .components
            .iter()
            .map(|c| {
                let mut c = c.clone();
                if matches!(h.phase, Phase::Fetching) && c.state == "available" {
                    c.fetched_bytes = staged_component_bytes(&c.component, c.bytes);
                }
                c
            })
            .collect(),
        download_bytes: s.download_bytes,
        fetched_bytes,
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
                fetched_bytes: 0,
                repair,
                message,
            }
        })
        .collect();
    Snapshot {
        release: Some(format!("{} sequence {}", check.release.version, check.release.sequence)),
        notes: check.release.notes.clone(),
        components,
        download_bytes: check.download_bytes(),
        download_files: check.download_files(),
        repair_only: check.is_repair_only(),
    }
}

/// Where content comes from: the setting when a person set one, else the
/// product's origin.
pub(crate) fn origin_url(settings: &reborn_core::settings::Settings) -> String {
    settings
        .content_origin
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(DEFAULT_ORIGIN)
        .to_string()
}

/// Shown when the patch a one-file launcher needs before it can import a
/// disc could not be downloaded.
pub(crate) const IMPORT_BUNDLE_UNAVAILABLE: &str =
    "The game patch could not be downloaded. Check your internet connection, then try again.";

/// Make sure what a disc import needs is on disk before a disc is imported:
/// the game-import bundle (patch manifest + reborn.xdelta) and the game-patch
/// chain, downloading the `game-import` and `game-patch` components when
/// either is missing.
///
/// The Windows installer carries both beside the launcher, so there this
/// finds them and returns at once. A Linux launcher is one AppImage file and
/// both are content; first-run setup imports the disc BEFORE any content
/// check runs. Without the bundle the first import on a Steam Deck had
/// nothing to patch with; without the chain the imported disc could not be
/// recognised as a Reborn build and setup said "Your game file isn't NFL
/// Street Reborn" (a Deck tester's first run, 2026-10-02).
pub(crate) fn ensure_import_bundle(p: &Platform) -> Result<PathBuf, String> {
    let data_root = p.paths.data_root().map_err(|e| e.to_string())?;
    let bundle = reborn_core::game_patch::bundle_dir(p, &data_root);
    let chain = reborn_core::game_patch::component_root(&data_root).join(reborn_core::game_patch::CHAIN_FILE);
    if bundle.join("manifest.json").is_file() && chain.is_file() {
        return Ok(bundle);
    }
    let settings = load_settings(p)?;
    let origin = Origin::new(&origin_url(&settings));
    let verifier = verifier()?;
    let updater = Updater::new(p, &data_root, &origin, &verifier);
    // Promoting game-patch with no disc yet only records it; setup's own
    // patch step moves the imported disc to the newest build with it.
    let wanted = [reborn_core::inventory::ComponentId::GameImport, reborn_core::inventory::ComponentId::GamePatch];
    let request = CheckRequest { components: &wanted, depth: Depth::Verify };
    if let Err(e) = updater.run(&request, &ImageHook::default()) {
        eprintln!("downloading the game-import bundle failed: {e}");
        return Err(IMPORT_BUNDLE_UNAVAILABLE.to_string());
    }
    let bundle = reborn_core::game_patch::bundle_dir(p, &data_root);
    if bundle.join("manifest.json").is_file() && chain.is_file() {
        Ok(bundle)
    } else {
        Err(IMPORT_BUNDLE_UNAVAILABLE.to_string())
    }
}

/// Make sure the game-patch chain is on disk, downloading the `game-patch`
/// component when it is not. The chain is how a disc is recognised as a
/// Reborn build at all: with none, `game_identity` answers "not Reborn" even
/// for Reborn's own downloaded game (the Deck tester's second screen,
/// 2026-10-02: "The downloaded game is not a Reborn build"). The Windows
/// installer ships it; a one-file Linux launcher gets it here.
pub(crate) fn ensure_patch_chain(p: &Platform) -> Result<(), String> {
    let data_root = p.paths.data_root().map_err(|e| e.to_string())?;
    let chain = reborn_core::game_patch::component_root(&data_root).join(reborn_core::game_patch::CHAIN_FILE);
    if chain.is_file() {
        return Ok(());
    }
    let settings = load_settings(p)?;
    let origin = Origin::new(&origin_url(&settings));
    let verifier = verifier()?;
    let updater = Updater::new(p, &data_root, &origin, &verifier);
    let request = CheckRequest { components: &[reborn_core::inventory::ComponentId::GamePatch], depth: Depth::Verify };
    if let Err(e) = updater.run(&request, &ImageHook::default()) {
        eprintln!("downloading the game-patch chain failed: {e}");
    }
    if chain.is_file() { Ok(()) } else { Err(IMPORT_BUNDLE_UNAVAILABLE.to_string()) }
}

/// A one-file launcher (Linux AppImage) has no emulator until it downloads
/// one: fetch this OS's runtime component when none can be found.
///
/// The Windows installer lays Dolphin beside the launcher and seeding mirrors
/// it into the data root, so on Windows this returns at once and changes
/// nothing. On a Deck, first-run setup asked for the emulator before any
/// content check had run and stopped with "No Reborn emulator was found"
/// (the tester's third screen, 2026-10-02). Called before every place the
/// launcher looks for the emulator. A failure is logged, not returned: the
/// search that follows then reports where it looked, as it always has.
pub(crate) fn ensure_runtime(p: &Platform) -> Option<String> {
    if cfg!(windows) || reborn_core::session::resolve_runtime(p, None, None).is_ok() {
        return None;
    }
    // One download at a time: the background start below and any command that
    // needs the emulator meanwhile share it, the command waiting for it to end
    // rather than starting a second copy of the same 119 MB.
    let _one = RUNTIME_DOWNLOAD.lock().unwrap_or_else(|e| e.into_inner());
    if reborn_core::session::resolve_runtime(p, None, None).is_ok() {
        return None;
    }
    let fail = |why: String| {
        // WARN reaches launcher.log; a Deck tester's report is that file.
        tracing::warn!("downloading the emulator failed: {why}");
        *LAST_RUNTIME_FAILURE.lock().unwrap_or_else(|e| e.into_inner()) = Some(why.clone());
        Some(why)
    };
    let data_root = match p.paths.data_root() { Ok(d) => d, Err(e) => return fail(e.to_string()) };
    let settings = match load_settings(p) { Ok(s) => s, Err(e) => return fail(e) };
    let verifier = match verifier() { Ok(v) => v, Err(e) => return fail(e) };
    let origin = Origin::new(&origin_url(&settings));
    let updater = Updater::new(p, &data_root, &origin, &verifier);
    // Verify, not Quick: this runs only when the files are missing, and a
    // Deck that had half-installed it under an earlier build kept a record
    // saying it was there, so Quick found nothing to do and the tester got
    // "the download finished but no emulator was installed" (2026-10-03).
    let request = CheckRequest { components: &[reborn_core::inventory::ComponentId::runtime_here()], depth: Depth::Verify };
    tracing::warn!("no emulator on disk: downloading {}", reborn_core::inventory::ComponentId::runtime_here().as_str());
    match updater.run(&request, &ImageHook::default()) {
        Err(e) => fail(e.to_string()),
        Ok(out) => {
            // A run can return without error and still not have installed it
            // (a refused plan, a problem the check reported): say what it said.
            if reborn_core::session::resolve_runtime(p, None, None).is_ok() {
                tracing::warn!("emulator downloaded");
                *LAST_RUNTIME_FAILURE.lock().unwrap_or_else(|e| e.into_inner()) = None;
                None
            } else {
                let problems = out.check.problems();
                fail(if problems.is_empty() { "the download finished but no emulator was installed".to_string() } else { problems.join("; ") })
            }
        }
    }
}

static RUNTIME_DOWNLOAD: Mutex<()> = Mutex::new(());
static LAST_RUNTIME_FAILURE: Mutex<Option<String>> = Mutex::new(None);

/// Where the emulator stands, for a page that must not wait for it.
pub(crate) enum RuntimeNow {
    Ready,
    Downloading,
    Failed(String),
}

/// The emulator's state without waiting for its download.
///
/// -5 still held Welcome on "Loading…" for the whole download: the page's
/// launch_check looked for the emulator through ensure_runtime, which waited
/// on the background download's lock (e2e run, 2026-10-03). Pages ask here
/// instead; only Play and confirming the game wait (ensure_runtime). With no
/// download running and no emulator, one is started again, so a failed
/// first attempt is retried the next time a page looks.
pub(crate) fn runtime_now(p: &Platform) -> RuntimeNow {
    if cfg!(windows) || reborn_core::session::resolve_runtime(p, None, None).is_ok() {
        return RuntimeNow::Ready;
    }
    match RUNTIME_DOWNLOAD.try_lock() {
        Err(std::sync::TryLockError::WouldBlock) => RuntimeNow::Downloading,
        _ => {
            let last = LAST_RUNTIME_FAILURE.lock().unwrap_or_else(|e| e.into_inner()).clone();
            start_runtime_download();
            match last {
                Some(why) => RuntimeNow::Failed(why),
                None => RuntimeNow::Downloading,
            }
        }
    }
}

/// Start the emulator download as the launcher opens, off every page's path.
///
/// Run from inside a page's command, the 119 MB download held that page on
/// "Loading…" for its whole length (57 s here; minutes on a Deck's Wi-Fi), and
/// a Deck tester reported the launcher "stuck all the time" (2026-10-02). Here
/// it runs on its own thread from the moment the window opens; a command that
/// truly needs the emulator waits on the same download in ensure_runtime.
pub(crate) fn start_runtime_download() {
    if cfg!(windows) {
        return;
    }
    std::thread::spawn(|| {
        let _ = ensure_runtime(&Platform::detect());
    });
}

pub(crate) fn load_settings(p: &Platform) -> Result<reborn_core::settings::Settings, String> {
    let file = reborn_core::settings::settings_path(p).map_err(|e| e.to_string())?;
    reborn_core::settings::Settings::load(&file).map_err(|e| e.to_string())
}

pub(crate) fn verifier() -> Result<Ed25519Verifier, String> {
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
/// Walk a pristine image the chain knows, but that is behind its head, to
/// the head, then re-apply the roster; quiet and fast when it is current.
///
/// Before 2026-10 a disc imported through the bundle stopped at the bundle's
/// build (9a6fde…) and only moved forward when a game-patch update arrived,
/// so a player could play an old build with the newest one on disk. Called
/// before a launch for the players already in that state; the folder name
/// is the hash the importer verified, so the check reads no disc. The
/// caller holds the launch guard.
pub(crate) fn walk_image_to_head(p: &Platform) -> Option<String> {
    use reborn_core::game_patch::{component_root, Chain, Route, CHAIN_FILE};
    let data_root = p.paths.data_root().ok()?;
    let chain = Chain::load(&component_root(&data_root).join(CHAIN_FILE)).ok()??;
    let pristine = reborn_core::game_image::find_pristine(&data_root)?;
    if chain.head() == Some(pristine.folder_sha256.as_str()) {
        return None;
    }
    if !matches!(chain.route(&pristine.folder_sha256), Route::Steps(_)) {
        return None;
    }
    let settings = load_settings(p).ok()?;
    let verifier = verifier().ok()?;
    let origin = Origin::new(&origin_url(&settings));
    let updater = Updater::new(p, &data_root, &origin, &verifier);
    let hook = image_hook(p, &settings);
    tracing::warn!("game image {} is behind the newest build: updating it before the launch", &pristine.folder_sha256[..12]);
    let gp = match updater.apply_current_game_patch(&hook) {
        Ok(gp) => gp,
        Err(e) => return Some(format!("The game could not be updated to the newest build: {e}")),
    };
    let mut notes = vec![gp.message()];
    if matches!(gp, GamePatchResult::Applied(_)) {
        match updater.apply_current_roster(&hook) {
            Ok(r) => notes.push(r.message()),
            Err(e) => notes.push(format!("The roster could not be re-applied: {e}")),
        }
        notes.extend(sync_cards(p, &settings, &data_root));
    }
    let said = notes.join(" ");
    tracing::warn!("{said}");
    Some(said)
}

pub(crate) fn image_hook(p: &Platform, settings: &reborn_core::settings::Settings) -> ImageHook {
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
    // The Builder appends the game id itself, so this is the pack root's
    // PARENT. See mods::builder_textures_arg.
    let textures_root = reborn_core::mods::GameId::parse("GN7E69")
        .ok()
        .and_then(|game| reborn_core::mods::builder_textures_arg(p, &game).ok());
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
    // A Linux launcher's install root is the folder its AppImage sits in,
    // which holds nothing; the emulator rides INSIDE the AppImage, at
    // $APPDIR/usr/bin/runtime. Seeded from there too, or the first check
    // downloaded the 111 MB runtime it already carries (Deck build, 2026-10).
    #[cfg(target_os = "linux")]
    if let Some(appdir) = std::env::var_os("APPDIR") {
        let bundled = std::path::PathBuf::from(appdir).join("usr").join("bin");
        match reborn_core::content::seed::seed_from_install(&p, &data_root, &bundled, "GN7E69") {
            Ok(r) => for s in &r.seeded {
                tracing::warn!("seeded {} from the AppImage ({} files)", s.component.as_str(), s.files);
            },
            Err(e) => tracing::warn!("seeding from the AppImage failed: {e}"),
        }
    }
    let origin = Origin::new(&url);
    let updater = Updater::new(&p, &data_root, &origin, &verifier);
    let mut check = updater
        .check(&CheckRequest {
            components: updatable(),
            depth,
        })
        .map_err(|e| e.to_string())?;
    // Adoption. A component whose files are all already right but which has
    // no pointer yet (the seeded trees, on a first run) plans an update of
    // zero bytes. Promote those, and only those, here and now: nothing is
    // downloaded, nothing is swapped, the pointer and the manifest cache are
    // written, and the player never sees an "update" of 0 MB. Each such
    // component on its own, whatever else is pending: with a 32 MB roster
    // waiting and updates set to ask first, the first version of this rule
    // ("adopt only when nothing at all needs downloading") never fired, and
    // every check re-hashed the 3 GB texture pack. Seen in the sandbox on
    // 2026-09-08. Only when no game is running, which the launch guard says.
    let (adoptable, pending): (Vec<ComponentCheck>, Vec<ComponentCheck>) = check
        .components
        .into_iter()
        .partition(|c| c.plan().map(|p| p.download_bytes() == 0).unwrap_or(false));
    check.components = pending;
    if !adoptable.is_empty() && !crate::session_active() {
        if let Ok(guard) = LaunchGuard::acquire() {
            let adopt = ContentCheck {
                origin: check.origin.clone(),
                release: check.release.clone(),
                signed_bytes: check.signed_bytes,
                carried: check.carried.clone(),
                components: adoptable,
            };
            let settings = load_settings(&p)?;
            let hook = image_hook(&p, &settings);
            let adopted = updater
                .fetch(&adopt)
                .and_then(|_| updater.promote(&adopt, &hook))
                .map_err(|e| e.to_string());
            // Adopting a shipped roster can rebuild the image; the memory
            // cards follow, under the same guard. Said on the log only: this
            // path draws nothing.
            if let Ok(po) = &adopted {
                if image_changed(po) {
                    for line in sync_cards(&p, &settings, &data_root) {
                        eprintln!("memory cards after adoption: {line}");
                    }
                }
            }
            drop(guard);
            match adopted {
                Ok(po) if po.all_applied() => {}
                Ok(po) => eprintln!("adoption of the shipped content did not fully apply: {po:?}"),
                Err(e) => eprintln!("adoption of the shipped content failed: {e}"),
            }
            // Fresh states for everything, against the pointers just
            // written: the adopted components now answer up to date without
            // a byte hashed, and the pending ones are reported as before.
            check = updater
                .check(&CheckRequest {
                    components: updatable(),
                    depth: Depth::Quick,
                })
                .map_err(|e| e.to_string())?;
        } else {
            // A game started meanwhile; put the components back unadopted.
            check.components.extend(adoptable);
        }
    } else {
        check.components.extend(adoptable);
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

/// Whether a promote put a new roster or a new build into the image, which
/// is when the memory cards' gear rows can be stale.
fn image_changed(outcome: &PromoteOutcome) -> bool {
    outcome.components.iter().any(|c| match &c.state {
        PromoteState::Applied { game_patch, roster, .. } => {
            matches!(roster, Some(RosterResult::Applied(_))) || matches!(game_patch, Some(GamePatchResult::Applied(_)))
        }
        _ => false,
    })
}

/// The memory cards, brought up to date with the roster that is now in the
/// image. Called with the launch guard held, so the game is closed. Every
/// bad state is a sentence; nothing here fails an update that already landed.
fn sync_cards(p: &Platform, settings: &reborn_core::settings::Settings, data_root: &Path) -> Vec<String> {
    let user = match reborn_core::session::resolve_user_dir(p, settings.user_dir.as_deref()) {
        Ok(u) => u,
        Err(e) => return vec![format!("The memory cards were left as they were: {}", e.player_message())],
    };
    match reborn_core::cards::after_apply(p, data_root, user.path(), &image_hook(p, settings).extra_dirs) {
        Ok(outcome) => outcome.facts(),
        Err(e) => {
            eprintln!("memory cards after the update: {e}");
            vec![reborn_core::cards::NOT_SYNCED.to_string()]
        }
    }
}

fn summarize(outcome: PromoteOutcome, cards: Vec<String>) -> InstalledSummary {
    let mut components = Vec::new();
    let mut problems = Vec::new();
    let mut game_patch = None;
    let mut roster = None;
    for c in outcome.components {
        match c.state {
            PromoteState::Applied { receipt, game_patch: gp, roster: r, soundtrack } => {
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
                // The custom soundtrack, replayed onto the rebuilt image so an
                // update never silently drops the player's songs.
                if let Some(st) = soundtrack {
                    use reborn_core::content::pipeline::SoundtrackResult;
                    match st {
                        SoundtrackResult::Replayed { slots, skipped } => {
                            let mut line = format!("Your custom soundtrack is back on the disc ({} song(s)).", slots.len());
                            if !skipped.is_empty() {
                                line.push_str(&format!(" {} slot(s) went back to the original song because their audio was missing.", skipped.len()));
                                problems.push(line.clone());
                            }
                            components.push(line);
                        }
                        SoundtrackResult::Unchanged => {}
                        SoundtrackResult::Refused(why) => problems.push(format!("Your custom soundtrack was not put back: {why}")),
                    }
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
    if !cards.is_empty() {
        note.push(' ');
        note.push_str(&cards.join(" "));
    }
    InstalledSummary {
        components,
        game_patch,
        roster,
        problems,
        cards,
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
        let guard = LaunchGuard::acquire()?;
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
            let po = updater.promote(&check, &hook).map_err(|e| e.to_string())?;
            // The memory cards, while the guard is still held and the game is
            // closed: a new roster or build in the image means their gear
            // rows may be stale. Quiet when nothing changed.
            let cards = if image_changed(&po) { sync_cards(&p, &settings, &data_root) } else { Vec::new() };
            Ok((po, cards))
        })();
        drop(guard);
        let mut h = held();
        match outcome {
            Ok((po, cards)) => {
                h.phase = Phase::Installed(summarize(po, cards));
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

/// Walk the game image to the head of the installed patch chain and re-apply the roster, with
/// no new component involved. Promote only patches the image while it installs something; a
/// player whose game-patch component was already current but whose image had stayed an old
/// build (09-26: NSRV3, so every online copy he built differed from everyone else's) had no
/// way back short of importing the disc again. Returns whether the image is current now and
/// the pipeline's own sentence about it.
#[tauri::command]
pub async fn content_repair_image(app: tauri::AppHandle) -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let guard = LaunchGuard::acquire()?;
        let p = Platform::detect();
        let outcome = (|| -> Result<(bool, String), String> {
            let settings = load_settings(&p)?;
            let verifier = verifier()?;
            let data_root = p.paths.data_root().map_err(|e| e.to_string())?;
            let origin = Origin::new(&origin_url(&settings));
            let updater = Updater::new(&p, &data_root, &origin, &verifier);
            let hook = image_hook(&p, &settings);
            let mut gp = updater.apply_current_game_patch(&hook).map_err(|e| e.to_string())?;
            // A pristine image no game update knows (or none at all) used to end
            // here with "import from your disc", which a player whose disc
            // cannot be converted cannot do (a Deck, 2026-10: online refused
            // for good). The Reborn game is downloaded instead, as setup does
            // for such a disc, and walked to the newest build.
            if matches!(
                gp,
                GamePatchResult::Refused(reborn_core::game_patch::Refusal::UnknownBase { .. })
                    | GamePatchResult::Refused(reborn_core::game_patch::Refusal::NoPristine { .. })
            ) {
                tracing::warn!("repair: {} Downloading the Reborn game instead.", gp.message());
                // The unknown image goes aside first (kept, not deleted). The
                // launcher uses the NEWEST pristine image, and a download that
                // was already on disk lands with its old date: the unknown one
                // was picked again and Repair ended on the same error (tester,
                // 2026-10-09). With it moved, the download is the only one.
                if let Some(bad) = reborn_core::game_image::find_pristine(&data_root) {
                    if let Some(folder) = bad.path.parent() {
                        let aside = data_root.join("library").join("imported-unrecognised");
                        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
                        let target = aside.join(format!("{}-{stamp}", bad.folder_sha256));
                        std::fs::create_dir_all(&aside).map_err(|e| e.to_string())?;
                        std::fs::rename(folder, &target).map_err(|e| format!("{}: {e}", folder.display()))?;
                        tracing::warn!("repair: moved the unrecognised game image to {}", target.display());
                    }
                }
                crate::setup_tools::download_reborn_game(&app)?;
                gp = updater.apply_current_game_patch(&hook).map_err(|e| e.to_string())?;
            }
            let mut notes = vec![gp.message()];
            if matches!(gp, GamePatchResult::Applied(_)) {
                // A freshly patched image has no roster yet; the same step promote takes next.
                let r = updater.apply_current_roster(&hook).map_err(|e| e.to_string())?;
                notes.push(r.message());
                notes.extend(sync_cards(&p, &settings, &data_root));
            }
            let mut current = gp.reached_the_image();
            // The online copy too, now rather than when a room opens: 09-26 a repaired image
            // was refused by the ready check on the stale record of a copy that the room
            // would have rebuilt seconds later.
            if current {
                match crate::dolphin_settings::netplay_image(&p) {
                    Ok(Some(_)) => {}
                    Ok(None) => { current = false; notes.push("No game image to build the online copy from.".to_string()); }
                    Err(e) => { current = false; notes.push(e); }
                }
            }
            Ok((current, notes.join(" ")))
        })();
        drop(guard);
        let (current, note) = outcome?;
        Ok(serde_json::json!({ "current": current, "note": note }))
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
        let s = summarize(PromoteOutcome::default(), Vec::new());
        assert_eq!(s.note, "Nothing was installed.");
        assert!(s.problems.is_empty());
        assert!(s.cards.is_empty());
    }

    #[test]
    fn what_the_memory_cards_said_rides_in_the_note_the_page_shows() {
        let line = "Memory card MemoryCardB.USA.raw: 12 gear row(s) brought up to date, 1 in-game edit(s) kept.".to_string();
        let s = summarize(PromoteOutcome::default(), vec![line.clone()]);
        assert_eq!(s.cards, vec![line.clone()]);
        assert_eq!(s.note, format!("Nothing was installed. {line}"));
    }

    #[test]
    fn an_empty_promote_did_not_change_the_image() {
        assert!(!image_changed(&PromoteOutcome::default()));
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
