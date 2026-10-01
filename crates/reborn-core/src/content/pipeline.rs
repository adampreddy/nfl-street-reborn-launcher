//! The content update pipeline: check, fetch, promote.
//!
//! One implementation that the launcher window and `rebornctl update content`
//! both drive. Before this the six steps lived in the CLI, and the window
//! would have grown a second copy with its own bugs.
//!
//! Three calls, because they run at three different times:
//!
//! - [`Updater::check`] is cheap and runs at startup. With [`Depth::Quick`]
//!   nothing on disk is hashed unless a newer version is offered; the
//!   manifest cached at the last promotion is compared with the pin in the
//!   signed release, and if they agree there is nothing to fetch and nothing
//!   to read.
//! - [`Updater::fetch`] is a background download. It only ever writes under
//!   `downloads/` and `staging/`, so it can run while the game is up.
//! - [`Updater::promote`] swaps files into the live tree, so the window calls
//!   it only when the game is closed. A roster is not done when its file
//!   lands but when the image carries it, so promoting the roster runs the
//!   Reborn Builder, and the result of that is part of the receipt.
//!
//! The [`ContentCheck`] is kept between the calls. It carries the plans that
//! the later steps are made from, so a fetch can never be for a different
//! manifest than the check saw.

use std::path::{Path, PathBuf};

use crate::content::{
    BlobSource, ContentService, FetchState, InstalledVersion, PlanOutcome, PlanRefusal, Promoted,
    PromotionReceipt, PromotionRefusal, SignatureVerifier, StagedVerdict, TrustedRelease,
    UpdatePlan,
};
use crate::game_patch::{self, Xdelta3};
use crate::inventory::ComponentId;
use crate::library::RebuildState;
use crate::manifest::Sha256Hex;
use crate::origin::{FetchedRelease, Origin};
use crate::platform::{DataDir, Platform};
use crate::roster::{self, ApplyOutcome, ApplyReceipt, ApplyRefusal, ApplyRequest, BuilderReport, ROSTER_FILE};
use crate::Result;

/// Where releases, component manifests and blobs come from.
///
/// [`Origin`] over HTTP in the product; an in-memory one in tests, so the
/// pipeline is proven without a socket.
pub trait ReleaseSource: BlobSource {
    fn fetch_release(&self, verifier: &dyn SignatureVerifier) -> Result<FetchedRelease>;
    fn fetch_component(&self, pinned_url: &str) -> Result<Vec<u8>>;
    /// For messages: where this source is.
    fn describe(&self) -> String;
}

impl ReleaseSource for Origin {
    fn fetch_release(&self, verifier: &dyn SignatureVerifier) -> Result<FetchedRelease> {
        Origin::fetch_release(self, verifier)
    }

    fn fetch_component(&self, pinned_url: &str) -> Result<Vec<u8>> {
        Origin::fetch_component(self, pinned_url)
    }

    fn describe(&self) -> String {
        self.base().to_string()
    }
}

/// The components the content system delivers, in the order they are
/// promoted. The game patch comes before the roster because the roster is
/// applied to the image the patch builds; the roster is last because it is
/// applied to whatever image is in place by then.
///
/// The launcher updates through its installer and controller-db has no
/// artifact, so neither is here.
pub const UPDATABLE: [ComponentId; 7] = [
    ComponentId::Runtime,
    ComponentId::OfflineContent,
    ComponentId::OnlineContent,
    ComponentId::Textures,
    ComponentId::Mods,
    ComponentId::GamePatch,
    ComponentId::Roster,
];

/// The data directory a component's manifest must install into.
///
/// Derived from the component identity, never taken from the manifest: this
/// is the check that stops a `runtime` manifest writing into `saves`. `None`
/// for the two components the content system does not deliver.
pub fn install_dir(id: ComponentId) -> Option<DataDir> {
    match id {
        ComponentId::Runtime => Some(DataDir::Runtime),
        ComponentId::OfflineContent => Some(DataDir::LibraryOffline),
        ComponentId::OnlineContent => Some(DataDir::LibraryOnline),
        ComponentId::Textures => Some(DataDir::LibraryTextures),
        ComponentId::Mods => Some(DataDir::ModsPackages),
        ComponentId::Roster => Some(DataDir::LibraryRoster),
        ComponentId::GamePatch => Some(DataDir::LibraryGamePatch),
        // Matches `ComponentId::install_root`: `LibraryGameFull`, never
        // `LibraryOffline` (the BOOTED image a roster apply rewrites in
        // place). Also not in `UPDATABLE`: recovery is resolved on demand by
        // its own call path, never by this pipeline's automatic
        // check/fetch/promote loop.
        ComponentId::GameFull => Some(DataDir::LibraryGameFull),
        ComponentId::Launcher | ComponentId::ControllerDb => None,
    }
}

/// How hard a check looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Depth {
    /// Compare what is installed with what is offered. No live file is
    /// hashed unless a newer version is offered, so this is what runs at
    /// startup and on a timer.
    Quick,
    /// Hash the live tree against the offered manifest even when the versions
    /// match, so a damaged file is found and re-fetched. The Repair button,
    /// and what `rebornctl update content` does.
    Verify,
}

/// What to check.
#[derive(Debug, Clone)]
pub struct CheckRequest<'a> {
    pub components: &'a [ComponentId],
    pub depth: Depth,
}

impl Default for CheckRequest<'_> {
    fn default() -> Self {
        Self {
            components: &UPDATABLE,
            depth: Depth::Quick,
        }
    }
}

/// The signed release's own version and sequence, and its notes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseTag {
    pub version: String,
    pub sequence: u64,
    /// The publisher's words for the update card, when the release has any.
    pub notes: Option<String>,
}

/// What a check found for one component.
#[derive(Debug)]
pub enum ComponentState {
    /// Installed and current. `verified` says whether every live file was
    /// hashed against the manifest (a [`Depth::Verify`] check) or only the
    /// version compared (a [`Depth::Quick`] one).
    UpToDate { version: String, verified: bool },
    /// Something to fetch: a first install, a newer version, or a repair.
    Available(Box<UpdatePlan>),
    /// The release does not carry this component. Not a fault: a roster-only
    /// origin has no runtime to offer.
    NotCarried,
    /// Not delivered by the content system at all.
    NotUpdatable,
    /// The release offers something this install must not take: an older
    /// sequence, the wrong install root, or unsafe paths.
    Refused(PlanRefusal),
    /// The component could not be checked: its manifest did not arrive or
    /// did not verify, or its pointer is unreadable. The message is for the
    /// user. The other components are still reported.
    Problem(String),
}

#[derive(Debug)]
pub struct ComponentCheck {
    pub component: ComponentId,
    pub installed: Option<InstalledVersion>,
    pub state: ComponentState,
}

impl ComponentCheck {
    pub fn plan(&self) -> Option<&UpdatePlan> {
        match &self.state {
            ComponentState::Available(plan) => Some(plan),
            _ => None,
        }
    }
}

/// The result of a check: the release that was opened and one entry per
/// requested component.
#[derive(Debug)]
pub struct ContentCheck {
    pub origin: String,
    pub release: ReleaseTag,
    /// Length of the release manifest whose signature verified, for the
    /// "verified over N bytes" line.
    pub signed_bytes: u64,
    /// The component names the release carries, for a message that says what
    /// it does carry when it does not carry what was asked for.
    pub carried: Vec<String>,
    pub components: Vec<ComponentCheck>,
}

impl ContentCheck {
    /// The components with something to fetch, with their plans.
    pub fn available(&self) -> impl Iterator<Item = (&ComponentCheck, &UpdatePlan)> {
        self.components.iter().filter_map(|c| c.plan().map(|p| (c, p)))
    }

    pub fn has_updates(&self) -> bool {
        self.available().next().is_some()
    }

    /// What has to come over the network, across every available component.
    pub fn download_bytes(&self) -> u64 {
        self.available().map(|(_, p)| p.download_bytes()).sum()
    }

    /// Files to fetch, across every available component.
    pub fn download_files(&self) -> u64 {
        self.available().map(|(_, p)| p.fetches.len() as u64).sum()
    }

    /// Whether every available update is a repair of the installed version
    /// rather than a move to a new one. False when nothing is available.
    pub fn is_repair_only(&self) -> bool {
        let mut any = false;
        for (_, plan) in self.available() {
            any = true;
            if !plan.is_repair() {
                return false;
            }
        }
        any
    }

    /// Every refusal and problem, one line each, naming the component.
    pub fn problems(&self) -> Vec<String> {
        self.components
            .iter()
            .filter_map(|c| match &c.state {
                ComponentState::Refused(r) => Some(format!("{}: {}", c.component.as_str(), r.message())),
                ComponentState::Problem(m) => Some(format!("{}: {m}", c.component.as_str())),
                _ => None,
            })
            .collect()
    }

    pub fn component(&self, id: ComponentId) -> Option<&ComponentCheck> {
        self.components.iter().find(|c| c.component == id)
    }
}

/// What a fetch did for one component.
#[derive(Debug)]
pub enum FetchOutcomeState {
    /// Every planned file is in staging and hashes to the manifest, on disk,
    /// now. The only state a promotion accepts.
    Staged { files: u64, bytes: u64 },
    /// Some files did not arrive whole. Incomplete parts are kept and the
    /// next fetch resumes them; corrupt ones were deleted and are fetched
    /// again from the start. Nothing was swapped.
    Incomplete {
        incomplete: Vec<String>,
        corrupt: Vec<String>,
        remaining_bytes: u64,
    },
    /// The download reported complete but the staged set does not hash to
    /// the manifest on re-check. Something touched staging between the two.
    NotReady { missing: Vec<String>, wrong: Vec<String> },
}

#[derive(Debug)]
pub struct ComponentFetch {
    pub component: ComponentId,
    pub version: String,
    pub state: FetchOutcomeState,
}

#[derive(Debug, Default)]
pub struct FetchOutcome {
    pub components: Vec<ComponentFetch>,
}

impl FetchOutcome {
    pub fn all_staged(&self) -> bool {
        self.components
            .iter()
            .all(|c| matches!(c.state, FetchOutcomeState::Staged { .. }))
    }

    pub fn staged(&self) -> impl Iterator<Item = &ComponentFetch> {
        self.components
            .iter()
            .filter(|c| matches!(c.state, FetchOutcomeState::Staged { .. }))
    }

    pub fn not_staged(&self) -> impl Iterator<Item = &ComponentFetch> {
        self.components
            .iter()
            .filter(|c| !matches!(c.state, FetchOutcomeState::Staged { .. }))
    }
}

/// How the image is rebuilt after content lands: the Builder for a roster,
/// the delta tool for a game patch.
#[derive(Debug, Clone, Default)]
pub struct ImageHook {
    /// The Builder's console build, when the caller knows where it is.
    pub builder: Option<PathBuf>,
    /// The delta tool (xdelta3), when the caller knows where it is.
    pub xdelta: Option<PathBuf>,
    /// Searched for both tools when they are not given, after the install
    /// root, its `tools` folder and its `game-import` bundle. The 2.1 tools
    /// folder goes here; the core does not read environment variables to
    /// find it.
    pub extra_dirs: Vec<PathBuf>,
    /// Where the Builder writes portraits. `None` sends them to a scratch
    /// directory under staging, which is right for a test root and wrong
    /// for a real one.
    pub textures_root: Option<PathBuf>,
}

/// What happened to the image after a game patch landed.
#[derive(Debug)]
pub enum GamePatchResult {
    Applied(game_patch::ApplyReceipt),
    /// The pristine image is already the newest build.
    UpToDate { sha256: String },
    /// No delta tool in any of the directories searched. The patch files are
    /// live; the image is as it was. [`Updater::apply_current_game_patch`]
    /// finishes the job once the tool is present.
    NoPatcher { looked: Vec<PathBuf> },
    Refused(game_patch::Refusal),
}

impl GamePatchResult {
    /// Whether the image is the newest build now.
    pub fn reached_the_image(&self) -> bool {
        matches!(self, GamePatchResult::Applied(_) | GamePatchResult::UpToDate { .. })
    }

    pub fn message(&self) -> String {
        match self {
            GamePatchResult::Applied(r) => format!(
                "Applied {} game update(s), {} through {}; the game is now build {}…",
                r.steps.len(),
                r.steps.first().map(String::as_str).unwrap_or("?"),
                r.steps.last().map(String::as_str).unwrap_or("?"),
                &r.to_sha256[..12]
            ),
            GamePatchResult::UpToDate { sha256 } => format!("The game is already the newest build ({}…).", &sha256[..12]),
            GamePatchResult::NoPatcher { looked } => {
                let mut m = String::from(
                    "The delta tool (xdelta3) was not found, so the game update is downloaded but not                      applied. Looked in:",
                );
                for d in looked {
                    m.push_str("
  ");
                    m.push_str(&d.display().to_string());
                }
                m
            }
            GamePatchResult::Refused(r) => r.message(),
        }
    }
}

/// What replaying the custom soundtrack after an image rebuild did.
#[derive(Debug)]
pub enum SoundtrackResult {
    /// The songs are back on the new image.
    Replayed { slots: Vec<u8>, skipped: Vec<(u8, String)> },
    /// The new image already carried them.
    Unchanged,
    /// It could not be replayed now; the sentence says why and what to do.
    Refused(String),
}

/// What happened to the image after a roster landed.
#[derive(Debug)]
pub enum RosterResult {
    Applied(ApplyReceipt),
    /// The Builder ran and the image already carried this roster.
    Unchanged { image: PathBuf, report: BuilderReport },
    /// No Builder in any of the directories searched. The roster file is
    /// live; the image does not carry it yet. [`Updater::apply_current_roster`]
    /// finishes the job once a Builder is present.
    NoBuilder { looked: Vec<PathBuf> },
    /// The Builder was found and the apply was refused: no image yet, the
    /// image in use, the Builder failed. Same remedy as above.
    Refused(ApplyRefusal),
    /// No roster has ever been installed, so there is nothing to apply. Not
    /// a fault: a game patch on an install without a roster ends here.
    NothingToApply,
}

impl RosterResult {
    /// Whether the image carries the current roster now (trivially, when
    /// there is none).
    pub fn reached_the_image(&self) -> bool {
        matches!(
            self,
            RosterResult::Applied(_) | RosterResult::Unchanged { .. } | RosterResult::NothingToApply
        )
    }

    pub fn message(&self) -> String {
        match self {
            RosterResult::NothingToApply => "No roster is installed, so there was nothing to apply.".to_string(),
            RosterResult::Applied(r) => format!(
                "Applied: {} players, {} teams, {} portrait(s) written to the image.",
                r.report.players, r.report.teams, r.report.portraits
            ),
            RosterResult::Unchanged { image, report } => format!(
                "Unchanged: {} already carries this roster ({} players, {} teams).",
                image.display(),
                report.players,
                report.teams
            ),
            RosterResult::NoBuilder { looked } => {
                let mut m = String::from(
                    "No Reborn Builder console build was found, so the roster is live but the \
                     image does not carry it yet. Looked in:",
                );
                for d in looked {
                    m.push_str("\n  ");
                    m.push_str(&d.display().to_string());
                }
                m
            }
            RosterResult::Refused(r) => r.message(),
        }
    }
}

/// What a promotion did for one component.
// The applied arm carries the receipt and three results; a Box would only
// move the size onto the heap for a value built once per promotion.
#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum PromoteState {
    Applied {
        receipt: PromotionReceipt,
        /// Present for the game-patch component only.
        game_patch: Option<GamePatchResult>,
        /// Present for the roster component, and for the game-patch
        /// component when its patch built a new image (the roster is
        /// re-applied to it).
        roster: Option<RosterResult>,
        /// Present when a roster or game patch rebuilt the image and the
        /// player has a custom soundtrack switched on: it is replayed onto
        /// the new image, so an update never silently drops their songs.
        soundtrack: Option<SoundtrackResult>,
    },
    /// Nothing was moved.
    Refused(PromotionRefusal),
    /// The swap failed part way and every move was reversed. The previous
    /// version is live and the staged bytes are kept, so a retry costs
    /// nothing over the network.
    RolledBack { reason: String, restored: usize },
}

#[derive(Debug)]
pub struct ComponentPromotion {
    pub component: ComponentId,
    pub version: String,
    pub state: PromoteState,
}

#[derive(Debug, Default)]
pub struct PromoteOutcome {
    pub components: Vec<ComponentPromotion>,
}

impl PromoteOutcome {
    pub fn all_applied(&self) -> bool {
        self.components
            .iter()
            .all(|c| matches!(c.state, PromoteState::Applied { .. }))
    }
}

/// What [`Updater::run`] did, end to end.
#[derive(Debug)]
pub struct RunOutcome {
    pub check: ContentCheck,
    /// `None` when the check found nothing to fetch.
    pub fetch: Option<FetchOutcome>,
    /// `None` when there was nothing to fetch.
    pub promote: Option<PromoteOutcome>,
}

/// The pipeline over one data root, one source, and one set of trusted keys.
#[derive(Debug)]
pub struct Updater<'a> {
    platform: &'a Platform,
    service: ContentService<'a>,
    source: &'a dyn ReleaseSource,
    verifier: &'a dyn SignatureVerifier,
}

impl<'a> Updater<'a> {
    pub fn new(
        platform: &'a Platform,
        data_root: &Path,
        source: &'a dyn ReleaseSource,
        verifier: &'a dyn SignatureVerifier,
    ) -> Self {
        Self {
            platform,
            service: ContentService::new(platform, data_root),
            source,
            verifier,
        }
    }

    pub fn service(&self) -> &ContentService<'a> {
        &self.service
    }

    pub fn data_root(&self) -> &Path {
        self.service.layout().data_root()
    }

    /// Open the release and work out what each requested component needs.
    ///
    /// Fails only when the release itself cannot be opened: no network, or a
    /// signature that does not verify. Anything wrong with one component is
    /// reported in that component's entry so the others are still seen.
    pub fn check(&self, req: &CheckRequest<'_>) -> Result<ContentCheck> {
        let fetched = self.source.fetch_release(self.verifier)?;
        let release = fetched.trusted.release();
        // Without this record a rebuilt soundtrack and a corrupted download
        // look the same: both are "not the shipped hash".
        let state_path = crate::music::music_root(self.data_root()).join("state.json");
        let rebuilds = RebuildState::load(&state_path)?;

        let mut components = Vec::with_capacity(req.components.len());
        for &id in req.components {
            let installed = match self.service.installed(id.as_str()) {
                Ok(p) => p.map(|p| p.current),
                Err(e) => {
                    components.push(ComponentCheck {
                        component: id,
                        installed: None,
                        state: ComponentState::Problem(e.to_string()),
                    });
                    continue;
                }
            };
            let state = match self.check_one(&fetched.trusted, id, installed.as_ref(), req.depth, &rebuilds) {
                Ok(s) => s,
                Err(e) => ComponentState::Problem(e.to_string()),
            };
            components.push(ComponentCheck {
                component: id,
                installed,
                state,
            });
        }

        Ok(ContentCheck {
            origin: self.source.describe(),
            release: ReleaseTag {
                version: release.version.clone(),
                sequence: release.sequence,
                notes: release.notes.clone(),
            },
            signed_bytes: fetched.bytes.len() as u64,
            carried: release.components.keys().cloned().collect(),
            components,
        })
    }

    fn check_one(
        &self,
        trusted: &TrustedRelease,
        id: ComponentId,
        installed: Option<&InstalledVersion>,
        depth: Depth,
        rebuilds: &RebuildState,
    ) -> Result<ComponentState> {
        let Some(dir) = install_dir(id) else {
            return Ok(ComponentState::NotUpdatable);
        };
        let name = id.as_str();
        let Some(pinned) = trusted.release().components.get(name) else {
            return Ok(ComponentState::NotCarried);
        };

        // The manifest cached at the last promotion, when the release still
        // pins exactly those bytes. Then the network is not needed for the
        // manifest at all, and the pin is the trust check.
        let manifest_bytes = match self.cached_manifest(name, &pinned.sha256)? {
            Some(bytes) => bytes,
            None => self.source.fetch_component(&pinned.url)?,
        };

        if depth == Depth::Quick {
            if let Some(current) = installed {
                let component = trusted.accept_component(name, &manifest_bytes)?;
                if component.version == current.version && component.sequence == current.sequence {
                    return Ok(ComponentState::UpToDate {
                        version: component.version,
                        verified: false,
                    });
                }
            }
        }

        Ok(match self.service.plan(trusted, name, &manifest_bytes, dir, rebuilds)? {
            PlanOutcome::UpToDate { version } => ComponentState::UpToDate {
                version,
                verified: true,
            },
            PlanOutcome::Refused(r) => ComponentState::Refused(r),
            PlanOutcome::Update(plan) => ComponentState::Available(plan),
        })
    }

    fn cached_manifest(&self, component: &str, pin: &Sha256Hex) -> Result<Option<Vec<u8>>> {
        let path = self.service.layout().manifest_path(component);
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(source) => return Err(crate::Error::Io { path, source }),
        };
        if Sha256Hex::from_bytes(&bytes) == *pin {
            Ok(Some(bytes))
        } else {
            Ok(None)
        }
    }

    /// Download and stage every available component. Safe to call again: each
    /// call resumes from what is on disk.
    pub fn fetch(&self, check: &ContentCheck) -> Result<FetchOutcome> {
        let blobs: &dyn BlobSource = self.source;
        let mut out = FetchOutcome::default();
        for (cc, plan) in check.available() {
            let report = self.service.download(plan, blobs)?;
            let state = if !report.is_complete() {
                FetchOutcomeState::Incomplete {
                    incomplete: report.incomplete().into_iter().map(str::to_string).collect(),
                    corrupt: report.corrupt().into_iter().map(str::to_string).collect(),
                    remaining_bytes: report.remaining_bytes(plan),
                }
            } else {
                match self.service.verify_staged(plan)? {
                    StagedVerdict::Complete => FetchOutcomeState::Staged {
                        files: report
                            .results
                            .values()
                            .filter(|s| matches!(s, FetchState::Staged))
                            .count() as u64,
                        bytes: plan.download_bytes(),
                    },
                    StagedVerdict::NotReady { missing, wrong } => FetchOutcomeState::NotReady { missing, wrong },
                }
            };
            out.components.push(ComponentFetch {
                component: cc.component,
                version: plan.version().to_string(),
                state,
            });
        }
        Ok(out)
    }

    /// Swap every available component's staged set into the live tree, in
    /// [`UPDATABLE`] order. A component whose staging is not complete is
    /// refused, not skipped, so the caller sees it by name.
    ///
    /// Call only when the game is closed: a file the game holds open cannot
    /// be replaced, and the swap would roll back.
    pub fn promote(&self, check: &ContentCheck, hook: &ImageHook) -> Result<PromoteOutcome> {
        let mut out = PromoteOutcome::default();
        for (cc, plan) in check.available() {
            let state = match self.service.promote(plan)? {
                Promoted::Applied(receipt) => {
                    let (game_patch, roster) = match cc.component {
                        ComponentId::Roster => (None, Some(self.apply_current_roster(hook)?)),
                        ComponentId::GamePatch => {
                            let gp = self.apply_current_game_patch(hook)?;
                            // A new image carries no roster; put the current
                            // one back. Only when an image was actually built.
                            let roster = match &gp {
                                GamePatchResult::Applied(_) => Some(self.apply_current_roster(hook)?),
                                _ => None,
                            };
                            (Some(gp), roster)
                        }
                        _ => (None, None),
                    };
                    let image_changed = matches!(roster, Some(RosterResult::Applied(_)))
                        || matches!(game_patch, Some(GamePatchResult::Applied(_)));
                    // The component is already live and its receipt written
                    // by now; a soundtrack that cannot be put back is a
                    // problem to report, not a reason to call the whole
                    // promotion failed and never record it as installed.
                    let soundtrack = if image_changed {
                        match self.apply_current_soundtrack() {
                            Ok(s) => s,
                            Err(e) => {
                                tracing::warn!("the custom soundtrack was not put back after the update: {e}");
                                Some(SoundtrackResult::Refused("Your custom soundtrack could not be put back after this update. Open Music and apply it again.".to_string()))
                            }
                        }
                    } else { None };
                    PromoteState::Applied {
                        receipt,
                        game_patch,
                        roster,
                        soundtrack,
                    }
                }
                Promoted::Refused(r) => PromoteState::Refused(r),
                Promoted::RolledBack { reason, restored, .. } => PromoteState::RolledBack {
                    reason,
                    restored: restored.len(),
                },
            };
            out.components.push(ComponentPromotion {
                component: cc.component,
                version: plan.version().to_string(),
                state,
            });
        }
        Ok(out)
    }

    /// Replay the custom soundtrack onto the booted image, when the player
    /// has one switched on. `None` when there is nothing to replay.
    ///
    /// Called after a roster or game patch rebuilt the image, which puts the
    /// disc's own songs back; without this the player's soundtrack would
    /// vanish on every update. Slots whose audio is gone are skipped and
    /// named, never dropped silently.
    pub fn apply_current_soundtrack(&self) -> Result<Option<SoundtrackResult>> {
        use crate::soundtrack::apply::{self as sapply, Outcome};
        use crate::soundtrack::state::Soundtrack;
        let root = self.data_root();
        let st = Soundtrack::load(root)?;
        if !st.enabled {
            return Ok(None);
        }
        let base_pack = st.base_pack(root).ok().flatten();
        let effective = st.effective(root, base_pack.as_ref(), false);
        if Soundtrack::is_all_stock(&effective) {
            return Ok(None);
        }
        Ok(Some(match sapply::apply(self.platform, root, &mut |_| {})? {
            Outcome::Applied(a) => SoundtrackResult::Replayed { slots: a.slots_changed, skipped: a.skipped },
            Outcome::Unchanged { .. } => SoundtrackResult::Unchanged,
            Outcome::Refused(r) => SoundtrackResult::Refused(r.message()),
        }))
    }

    /// Apply the roster that is live in `library/roster` to the image.
    ///
    /// Idempotent: the Builder produces the same bytes for the same roster,
    /// and that is reported as unchanged. So this is safe to call after a
    /// roster promotion, after a game import puts a new image in place, and
    /// from a Repair button, and it is how a roster that landed before the
    /// image existed reaches it later.
    pub fn apply_current_roster(&self, hook: &ImageHook) -> Result<RosterResult> {
        let roster_file = self
            .data_root()
            .join(DataDir::LibraryRoster.relative())
            .join(ROSTER_FILE);
        if !roster_file.is_file() {
            return Ok(RosterResult::NothingToApply);
        }
        let builder = match &hook.builder {
            Some(b) => b.clone(),
            None => {
                let dirs = roster::builder_dirs(self.platform, &hook.extra_dirs);
                match roster::find_builder(self.platform, &dirs) {
                    Some(b) => b,
                    None => return Ok(RosterResult::NoBuilder { looked: dirs }),
                }
            }
        };
        let req = ApplyRequest {
            data_root: self.data_root(),
            roster: &roster_file,
            builder: &builder,
            textures_root: hook.textures_root.as_deref(),
            accept_warnings: true,
            output: None,
        };
        Ok(match roster::apply(self.platform, &req)? {
            ApplyOutcome::Applied(r) => RosterResult::Applied(r),
            ApplyOutcome::Unchanged { image, report, .. } => RosterResult::Unchanged { image, report },
            ApplyOutcome::Refused(r) => RosterResult::Refused(r),
        })
    }

    /// Apply the game patches that are live in `library/game-patch` to the
    /// pristine image, and re-establish the booted image from the result.
    ///
    /// Idempotent: an image already at the head of the chain is reported up
    /// to date. The roster is NOT re-applied here; [`Updater::promote`]
    /// does that after a patch builds a new image, and a caller using this
    /// directly should call [`Updater::apply_current_roster`] next.
    pub fn apply_current_game_patch(&self, hook: &ImageHook) -> Result<GamePatchResult> {
        let exe = match &hook.xdelta {
            Some(x) => x.clone(),
            None => {
                let dirs = game_patch::xdelta_dirs(self.platform, &hook.extra_dirs);
                match game_patch::find_xdelta(self.platform, &dirs) {
                    Some(x) => x,
                    None => return Ok(GamePatchResult::NoPatcher { looked: dirs }),
                }
            }
        };
        let patcher = Xdelta3::new(self.platform, exe);
        let req = game_patch::ApplyRequest {
            data_root: self.data_root(),
            patcher: &patcher,
        };
        Ok(match game_patch::apply(self.platform, &req)? {
            game_patch::Outcome::Applied(r) => GamePatchResult::Applied(r),
            game_patch::Outcome::UpToDate { sha256, .. } => GamePatchResult::UpToDate { sha256 },
            game_patch::Outcome::Refused(r) => GamePatchResult::Refused(r),
        })
    }

    /// Check, fetch and promote in one go, for a caller that has already
    /// established the game is closed. Stops after the check when there is
    /// nothing to fetch.
    pub fn run(&self, req: &CheckRequest<'_>, hook: &ImageHook) -> Result<RunOutcome> {
        let check = self.check(req)?;
        if !check.has_updates() {
            return Ok(RunOutcome {
                check,
                fetch: None,
                promote: None,
            });
        }
        let fetch = self.fetch(&check)?;
        let promote = self.promote(&check, hook)?;
        Ok(RunOutcome {
            check,
            fetch: Some(fetch),
            promote: Some(promote),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::BlobRequest;
    use crate::manifest::{Component, ManifestFile, COMPONENT_SCHEMA};
    use std::collections::BTreeMap;
    use std::io::Read;
    use std::sync::Mutex;

    /// Accepts exactly one signature.
    #[derive(Debug)]
    struct FixedSignature(&'static [u8]);

    impl SignatureVerifier for FixedSignature {
        fn verify(&self, _signed_bytes: &[u8], signature: &[u8]) -> Result<bool> {
            Ok(signature == self.0)
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Serving {
        Whole,
        FirstHalf,
    }

    /// A whole origin in memory: one signed release, its component
    /// manifests by URL, and blobs by manifest path. Logs what was asked for,
    /// so a test can say "nothing was fetched" and mean it.
    #[derive(Debug)]
    struct Memory {
        release: Vec<u8>,
        signature: Vec<u8>,
        manifests: BTreeMap<String, Vec<u8>>,
        blobs: BTreeMap<String, Vec<u8>>,
        serving: Mutex<Serving>,
        manifest_requests: Mutex<Vec<String>>,
        blob_requests: Mutex<Vec<(String, u64)>>,
    }

    impl Memory {
        fn manifest_requests(&self) -> usize {
            self.manifest_requests.lock().unwrap().len()
        }
        fn blob_requests(&self) -> Vec<(String, u64)> {
            self.blob_requests.lock().unwrap().clone()
        }
        fn serve(&self, s: Serving) {
            *self.serving.lock().unwrap() = s;
        }
    }

    impl BlobSource for Memory {
        fn open(&self, req: &BlobRequest<'_>) -> Result<Box<dyn Read>> {
            self.blob_requests
                .lock()
                .unwrap()
                .push((req.file.path.clone(), req.offset));
            let all = self
                .blobs
                .get(&req.file.path)
                .unwrap_or_else(|| panic!("no blob for {:?}", req.file.path));
            let from = (req.offset as usize).min(all.len());
            let body = match *self.serving.lock().unwrap() {
                Serving::Whole => all[from..].to_vec(),
                Serving::FirstHalf => {
                    let end = (all.len() / 2).max(from);
                    all[from..end].to_vec()
                }
            };
            Ok(Box::new(std::io::Cursor::new(body)))
        }
    }

    impl ReleaseSource for Memory {
        fn fetch_release(&self, verifier: &dyn SignatureVerifier) -> Result<FetchedRelease> {
            let trusted = TrustedRelease::open(verifier, &self.release, &self.signature)?;
            Ok(FetchedRelease {
                trusted,
                bytes: self.release.clone(),
            })
        }
        fn fetch_component(&self, pinned_url: &str) -> Result<Vec<u8>> {
            self.manifest_requests.lock().unwrap().push(pinned_url.to_string());
            Ok(self
                .manifests
                .get(pinned_url)
                .unwrap_or_else(|| panic!("no manifest at {pinned_url}"))
                .clone())
        }
        fn describe(&self) -> String {
            "memory://".to_string()
        }
    }

    /// One component's worth of files, as the publisher would lay them out.
    struct Built {
        component: &'static str,
        version: &'static str,
        sequence: u64,
        install_root: String,
        files: Vec<(&'static str, &'static [u8])>,
    }

    fn built(id: ComponentId, version: &'static str, sequence: u64, files: Vec<(&'static str, &'static [u8])>) -> Built {
        Built {
            component: id.as_str(),
            version,
            sequence,
            install_root: install_dir(id).unwrap().relative().to_string(),
            files,
        }
    }

    /// A release at `sequence` carrying the given components.
    fn origin(release_sequence: u64, components: Vec<Built>) -> Memory {
        let mut manifests = BTreeMap::new();
        let mut blobs = BTreeMap::new();
        let mut pins = Vec::new();
        for b in components {
            let files = b
                .files
                .iter()
                .map(|(p, bytes)| {
                    blobs.insert(p.to_string(), bytes.to_vec());
                    ManifestFile {
                        path: p.to_string(),
                        bytes: bytes.len() as u64,
                        sha256: Sha256Hex::from_bytes(bytes),
                    }
                })
                .collect();
            let component = Component {
                schema: COMPONENT_SCHEMA.to_string(),
                component: b.component.to_string(),
                version: b.version.to_string(),
                sequence: b.sequence,
                install_root: b.install_root.clone(),
                customizable: false,
                files,
            };
            let bytes = serde_json::to_vec(&component).unwrap();
            let url = format!("mem://{}-{}.json", b.component, b.version);
            pins.push(format!(
                r#""{}":{{"url":"{url}","sha256":"{}"}}"#,
                b.component,
                Sha256Hex::from_bytes(&bytes)
            ));
            manifests.insert(url, bytes);
        }
        let release = format!(
            r#"{{"schema":"nsr.release.v1","version":"3.0.0","sequence":{release_sequence},"components":{{{}}}}}"#,
            pins.join(",")
        );
        Memory {
            release: release.into_bytes(),
            signature: b"good".to_vec(),
            manifests,
            blobs,
            serving: Mutex::new(Serving::Whole),
            manifest_requests: Mutex::new(Vec::new()),
            blob_requests: Mutex::new(Vec::new()),
        }
    }

    const GOOD: FixedSignature = FixedSignature(b"good");

    fn runtime_v1() -> Built {
        built(
            ComponentId::Runtime,
            "1.0.0",
            1,
            vec![("Dolphin.exe", b"dolphin one"), ("Sys/GC/font.bin", b"font")],
        )
    }

    fn quick(components: &[ComponentId]) -> CheckRequest<'_> {
        CheckRequest {
            components,
            depth: Depth::Quick,
        }
    }

    fn live(data_root: &Path, id: ComponentId, rel: &str) -> PathBuf {
        data_root.join(install_dir(id).unwrap().relative()).join(rel)
    }

    /// `game-full` is recovery: 1.36 GB a normal install never needs, for the
    /// player whose own copy could not be converted. It is resolved through
    /// this pipeline like every other component, and what keeps it away from
    /// everybody else is not a second code path but one exclusion, here:
    /// `UPDATABLE` does not list it, so the automatic check/fetch/promote
    /// loop the window runs at startup never asks for it however many
    /// releases pin it.
    ///
    /// This is the whole gate, which is why it is asserted beside the check
    /// that a caller naming the component explicitly still works. The
    /// publisher deliberately does carry the pin forward
    /// (`publish::release::tests::
    /// the_full_disc_carries_forward_so_recovery_does_not_expire`): keeping
    /// it out of later releases gated nothing, because a pin is a line of
    /// JSON, and it quietly retired recovery on the next ordinary publish.
    #[test]
    fn game_full_is_recovery_the_automatic_loop_never_asks_for() {
        assert!(
            !UPDATABLE.contains(&ComponentId::GameFull),
            "the automatic update loop would fetch 1.36 GB of recovery on every install"
        );

        let dir = tempfile::tempdir().unwrap();
        let platform = Platform::detect();
        let src = origin(
            1,
            vec![
                runtime_v1(),
                built(ComponentId::GameFull, "2026.09.13.3", 1, vec![("NSR.iso", b"a whole disc")]),
            ],
        );
        let updater = Updater::new(&platform, dir.path(), &src, &GOOD);

        let automatic = updater.check(&quick(&UPDATABLE)).unwrap();
        assert!(
            automatic.component(ComponentId::GameFull).is_none(),
            "the ordinary update loop put the recovery disc in front of a player who never asked"
        );

        // Asked for by name, it resolves: the release pins it, so the engine
        // accepts its manifest and plans it like anything else.
        let check = updater.check(&quick(&[ComponentId::GameFull])).unwrap();
        let plan = check
            .component(ComponentId::GameFull)
            .unwrap()
            .plan()
            .expect("recovery is available on demand");
        assert_eq!(plan.version(), "2026.09.13.3");
        assert!(updater.fetch(&check).unwrap().all_staged());
        assert!(updater.promote(&check, &ImageHook::default()).unwrap().all_applied());

        // It lands in `library/game-full`, not on the booted image and not in
        // the pristine layer. Turning the landed file into a pristine image is
        // a separate step, which is what makes that layering safe.
        let landed = live(dir.path(), ComponentId::GameFull, "NSR.iso");
        assert_eq!(std::fs::read(&landed).unwrap(), b"a whole disc");
        assert_eq!(landed, dir.path().join("library").join("game-full").join("NSR.iso"));
        assert!(!dir.path().join("library").join("offline").join("NSR.iso").exists());
        assert!(!dir.path().join("library").join("imported").exists());
    }

    /// A release that does not pin `game-full` cannot be talked into serving
    /// it: the pin is the only door, and without one the answer is
    /// `NotCarried`, not an error.
    ///
    /// Reachable, and so worth pinning, even though the publisher now carries
    /// the pin forward: an origin that has never published the disc has no
    /// pin to carry. That covers a mirror, an origin set in settings, and the
    /// product's own origin before the first `publish game-full` runs. The
    /// recovery command turns this into a sentence for the player rather than
    /// propagating it.
    #[test]
    fn game_full_is_not_carried_by_a_release_that_does_not_pin_it() {
        let dir = tempfile::tempdir().unwrap();
        let platform = Platform::detect();
        let src = origin(1, vec![runtime_v1()]);
        let updater = Updater::new(&platform, dir.path(), &src, &GOOD);

        let check = updater.check(&quick(&[ComponentId::GameFull])).unwrap();
        assert!(matches!(
            check.component(ComponentId::GameFull).unwrap().state,
            ComponentState::NotCarried
        ));
        assert!(!check.has_updates());
        assert!(check.problems().is_empty(), "not being offered recovery is not a fault");
    }

    #[test]
    fn a_check_reports_every_state_by_name_and_sums_what_is_to_fetch() {
        let dir = tempfile::tempdir().unwrap();
        let platform = Platform::detect();
        let src = origin(1, vec![runtime_v1()]);
        let updater = Updater::new(&platform, dir.path(), &src, &GOOD);

        let asked = [ComponentId::Runtime, ComponentId::Textures, ComponentId::Launcher];
        let check = updater.check(&quick(&asked)).unwrap();

        assert_eq!(check.release, ReleaseTag { version: "3.0.0".into(), sequence: 1, notes: None });
        assert_eq!(check.carried, vec!["runtime".to_string()]);
        assert!(matches!(check.component(ComponentId::Runtime).unwrap().state, ComponentState::Available(_)));
        assert!(matches!(check.component(ComponentId::Textures).unwrap().state, ComponentState::NotCarried));
        assert!(matches!(check.component(ComponentId::Launcher).unwrap().state, ComponentState::NotUpdatable));
        assert!(check.has_updates());
        assert_eq!(check.download_files(), 2);
        assert_eq!(check.download_bytes(), ("dolphin one".len() + "font".len()) as u64);
        assert!(!check.is_repair_only(), "a first install is not a repair");
        assert!(check.problems().is_empty());
        assert!(!live(dir.path(), ComponentId::Runtime, "Dolphin.exe").exists(), "a check writes nothing live");
    }

    #[test]
    fn fetch_then_promote_goes_live_and_the_next_quick_check_touches_neither_disk_nor_network() {
        let dir = tempfile::tempdir().unwrap();
        let platform = Platform::detect();
        let src = origin(1, vec![runtime_v1()]);
        let updater = Updater::new(&platform, dir.path(), &src, &GOOD);

        let check = updater.check(&quick(&[ComponentId::Runtime])).unwrap();
        let fetched = updater.fetch(&check).unwrap();
        assert!(fetched.all_staged(), "{fetched:?}");
        assert!(!live(dir.path(), ComponentId::Runtime, "Dolphin.exe").exists(), "a fetch writes nothing live");

        let promoted = updater.promote(&check, &ImageHook::default()).unwrap();
        assert!(promoted.all_applied(), "{promoted:?}");
        assert_eq!(std::fs::read(live(dir.path(), ComponentId::Runtime, "Dolphin.exe")).unwrap(), b"dolphin one");
        assert_eq!(std::fs::read(live(dir.path(), ComponentId::Runtime, "Sys/GC/font.bin")).unwrap(), b"font");
        assert!(updater.service().layout().manifest_path("runtime").is_file(), "the live manifest is cached");

        let manifests_before = src.manifest_requests();
        let blobs_before = src.blob_requests().len();
        let again = updater.check(&quick(&[ComponentId::Runtime])).unwrap();
        match &again.component(ComponentId::Runtime).unwrap().state {
            ComponentState::UpToDate { version, verified } => {
                assert_eq!(version, "1.0.0");
                assert!(!verified, "a quick check does not hash the live tree");
            }
            other => panic!("expected up to date, got {other:?}"),
        }
        assert_eq!(again.component(ComponentId::Runtime).unwrap().installed.as_ref().unwrap().version, "1.0.0");
        assert!(!again.has_updates());
        assert_eq!(src.manifest_requests(), manifests_before, "the cached manifest matched the pin, so none was fetched");
        assert_eq!(src.blob_requests().len(), blobs_before);
    }

    #[test]
    fn a_newer_release_is_planned_even_by_a_quick_check_and_only_the_changed_file_is_fetched() {
        let dir = tempfile::tempdir().unwrap();
        let platform = Platform::detect();
        let v1 = origin(1, vec![runtime_v1()]);
        {
            let updater = Updater::new(&platform, dir.path(), &v1, &GOOD);
            let run = updater.run(&quick(&[ComponentId::Runtime]), &ImageHook::default()).unwrap();
            assert!(run.promote.unwrap().all_applied());
        }

        let v2 = origin(
            2,
            vec![built(
                ComponentId::Runtime,
                "1.0.1",
                2,
                vec![("Dolphin.exe", b"dolphin two"), ("Sys/GC/font.bin", b"font")],
            )],
        );
        let updater = Updater::new(&platform, dir.path(), &v2, &GOOD);
        let check = updater.check(&quick(&[ComponentId::Runtime])).unwrap();
        let plan = check.component(ComponentId::Runtime).unwrap().plan().expect("an update is available");
        assert_eq!(plan.from.as_ref().unwrap().version, "1.0.0");
        assert_eq!(plan.version(), "1.0.1");
        assert_eq!(plan.fetches.len(), 1, "the unchanged font is already the new version of itself");
        assert_eq!(check.download_bytes(), "dolphin two".len() as u64);

        let run = updater.run(&quick(&[ComponentId::Runtime]), &ImageHook::default()).unwrap();
        let promoted = run.promote.unwrap();
        match &promoted.components[0].state {
            PromoteState::Applied { receipt, roster, game_patch, .. } => {
                assert_eq!(receipt.previous.as_ref().unwrap().version, "1.0.0", "rollback is available");
                assert!(roster.is_none() && game_patch.is_none(), "the image hooks run for their own components only");
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(std::fs::read(live(dir.path(), ComponentId::Runtime, "Dolphin.exe")).unwrap(), b"dolphin two");
        assert_eq!(
            v2.blob_requests().iter().map(|(p, _)| p.as_str()).collect::<Vec<_>>(),
            vec!["Dolphin.exe"],
            "one blob fetch, for the one changed file; a check fetches no blobs"
        );
    }

    #[test]
    fn a_verify_check_finds_a_damaged_file_that_a_quick_check_reports_as_current() {
        let dir = tempfile::tempdir().unwrap();
        let platform = Platform::detect();
        let src = origin(1, vec![runtime_v1()]);
        let updater = Updater::new(&platform, dir.path(), &src, &GOOD);
        updater.run(&quick(&[ComponentId::Runtime]), &ImageHook::default()).unwrap();

        std::fs::write(live(dir.path(), ComponentId::Runtime, "Sys/GC/font.bin"), b"scribbled").unwrap();

        let q = updater.check(&quick(&[ComponentId::Runtime])).unwrap();
        assert!(matches!(
            q.component(ComponentId::Runtime).unwrap().state,
            ComponentState::UpToDate { verified: false, .. }
        ));

        let v = updater
            .check(&CheckRequest {
                components: &[ComponentId::Runtime],
                depth: Depth::Verify,
            })
            .unwrap();
        let plan = v.component(ComponentId::Runtime).unwrap().plan().expect("a repair is available");
        assert!(plan.is_repair());
        assert!(v.is_repair_only());
        assert_eq!(plan.fetches.len(), 1);
        assert_eq!(plan.fetches[0].path, "Sys/GC/font.bin");
        assert_eq!(plan.fetches[0].reason, crate::content::FetchReason::Damaged);

        let run = updater
            .run(
                &CheckRequest {
                    components: &[ComponentId::Runtime],
                    depth: Depth::Verify,
                },
                &ImageHook::default(),
            )
            .unwrap();
        assert!(run.promote.unwrap().all_applied());
        assert_eq!(std::fs::read(live(dir.path(), ComponentId::Runtime, "Sys/GC/font.bin")).unwrap(), b"font");

        let clean = updater
            .check(&CheckRequest {
                components: &[ComponentId::Runtime],
                depth: Depth::Verify,
            })
            .unwrap();
        assert!(matches!(
            clean.component(ComponentId::Runtime).unwrap().state,
            ComponentState::UpToDate { verified: true, .. }
        ));
    }

    #[test]
    fn a_half_served_fetch_is_incomplete_promotion_refuses_it_and_the_retry_resumes() {
        let dir = tempfile::tempdir().unwrap();
        let platform = Platform::detect();
        let src = origin(1, vec![runtime_v1()]);
        src.serve(Serving::FirstHalf);
        let updater = Updater::new(&platform, dir.path(), &src, &GOOD);

        let check = updater.check(&quick(&[ComponentId::Runtime])).unwrap();
        let fetched = updater.fetch(&check).unwrap();
        match &fetched.components[0].state {
            FetchOutcomeState::Incomplete { incomplete, corrupt, remaining_bytes } => {
                assert_eq!(incomplete.len(), 2);
                assert!(corrupt.is_empty());
                assert!(*remaining_bytes > 0);
            }
            other => panic!("{other:?}"),
        }
        assert!(fetched.not_staged().count() == 1 && fetched.staged().count() == 0);

        let promoted = updater.promote(&check, &ImageHook::default()).unwrap();
        assert!(matches!(promoted.components[0].state, PromoteState::Refused(_)), "{promoted:?}");
        assert!(!live(dir.path(), ComponentId::Runtime, "Dolphin.exe").exists());
        assert!(updater.service().installed("runtime").unwrap().is_none());

        src.serve(Serving::Whole);
        let fetched = updater.fetch(&check).unwrap();
        assert!(fetched.all_staged(), "{fetched:?}");
        assert!(
            src.blob_requests().iter().any(|(_, offset)| *offset > 0),
            "the retry resumed the kept part instead of starting over: {:?}",
            src.blob_requests()
        );
        let promoted = updater.promote(&check, &ImageHook::default()).unwrap();
        assert!(promoted.all_applied(), "{promoted:?}");
        assert_eq!(std::fs::read(live(dir.path(), ComponentId::Runtime, "Dolphin.exe")).unwrap(), b"dolphin one");
    }

    #[test]
    fn promoting_the_roster_runs_the_hook_and_a_missing_builder_is_reported_not_hidden() {
        let dir = tempfile::tempdir().unwrap();
        let platform = Platform::detect();
        let src = origin(
            1,
            vec![built(ComponentId::Roster, "2026.09.06", 1, vec![(ROSTER_FILE, b"PK-not-really-a-zip")])],
        );
        let updater = Updater::new(&platform, dir.path(), &src, &GOOD);
        let nowhere = dir.path().join("no-tools-here");
        let hook = ImageHook {
            builder: None,
            xdelta: None,
            extra_dirs: vec![nowhere.clone()],
            textures_root: None,
        };

        let run = updater.run(&quick(&[ComponentId::Roster]), &hook).unwrap();
        let promoted = run.promote.unwrap();
        match &promoted.components[0].state {
            PromoteState::Applied { roster: Some(RosterResult::NoBuilder { looked }), .. } => {
                assert!(looked.contains(&nowhere), "{looked:?}");
                assert!(looked.len() > 1, "the install root and its tools folder come first: {looked:?}");
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(
            std::fs::read(live(dir.path(), ComponentId::Roster, ROSTER_FILE)).unwrap(),
            b"PK-not-really-a-zip",
            "the roster file itself is live"
        );
        let result = updater.apply_current_roster(&hook).unwrap();
        assert!(!result.reached_the_image());
        assert!(result.message().contains("does not carry it yet"), "{}", result.message());

        // A Builder that is present but has no image to work on is a refusal
        // with a message, not a success and not a panic.
        let tools = dir.path().join("tools");
        std::fs::create_dir_all(&tools).unwrap();
        let name = format!("{}{}", roster::BUILDER_NAME, platform.paths.exe_suffix());
        std::fs::write(tools.join(&name), b"not a program").unwrap();
        let hook = ImageHook {
            builder: None,
            xdelta: None,
            extra_dirs: vec![tools],
            textures_root: None,
        };
        match updater.apply_current_roster(&hook).unwrap() {
            RosterResult::Refused(ApplyRefusal::NoImage { .. }) => {}
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_release_that_does_not_verify_stops_the_check_before_any_manifest_is_read() {
        let dir = tempfile::tempdir().unwrap();
        let platform = Platform::detect();
        let src = origin(1, vec![runtime_v1()]);
        let wrong = FixedSignature(b"someone else");
        let updater = Updater::new(&platform, dir.path(), &src, &wrong);
        assert!(updater.check(&quick(&[ComponentId::Runtime])).is_err());
        assert_eq!(src.manifest_requests(), 0);
        assert!(src.blob_requests().is_empty());
    }

    #[test]
    fn a_stale_release_is_refused_by_name_and_the_other_components_are_still_checked() {
        let dir = tempfile::tempdir().unwrap();
        let platform = Platform::detect();
        let newer = origin(
            2,
            vec![
                built(ComponentId::Runtime, "1.0.1", 2, vec![("Dolphin.exe", b"two")]),
                built(ComponentId::Mods, "1.0.0", 1, vec![("pack.zip", b"pack")]),
            ],
        );
        Updater::new(&platform, dir.path(), &newer, &GOOD)
            .run(&quick(&[ComponentId::Runtime]), &ImageHook::default())
            .unwrap();

        let older = origin(
            1,
            vec![
                built(ComponentId::Runtime, "1.0.0", 1, vec![("Dolphin.exe", b"one")]),
                built(ComponentId::Mods, "1.0.0", 1, vec![("pack.zip", b"pack")]),
            ],
        );
        let updater = Updater::new(&platform, dir.path(), &older, &GOOD);
        let check = updater.check(&quick(&[ComponentId::Runtime, ComponentId::Mods])).unwrap();
        assert!(matches!(
            check.component(ComponentId::Runtime).unwrap().state,
            ComponentState::Refused(PlanRefusal::Stale { .. })
        ));
        assert!(check.component(ComponentId::Mods).unwrap().plan().is_some(), "mods is still offered");
        let problems = check.problems();
        assert_eq!(problems.len(), 1);
        assert!(problems[0].starts_with("runtime:"), "{problems:?}");
        assert_eq!(std::fs::read(live(dir.path(), ComponentId::Runtime, "Dolphin.exe")).unwrap(), b"two");
    }

    #[test]
    fn run_stops_after_the_check_when_there_is_nothing_to_fetch() {
        let dir = tempfile::tempdir().unwrap();
        let platform = Platform::detect();
        let src = origin(1, vec![runtime_v1()]);
        let updater = Updater::new(&platform, dir.path(), &src, &GOOD);
        let first = updater.run(&CheckRequest::default(), &ImageHook::default()).unwrap();
        assert!(first.fetch.is_some() && first.promote.is_some());
        assert_eq!(first.check.components.len(), UPDATABLE.len());

        let second = updater.run(&CheckRequest::default(), &ImageHook::default()).unwrap();
        assert!(second.fetch.is_none() && second.promote.is_none());
        assert!(!second.check.has_updates());
        for c in &second.check.components {
            match (c.component, &c.state) {
                (ComponentId::Runtime, ComponentState::UpToDate { .. }) => {}
                (_, ComponentState::NotCarried) => {}
                other => panic!("{other:?}"),
            }
        }
    }

    /// The soundtrack replay after an image rebuild: nothing to do when the
    /// switch is off or every slot is stock; otherwise the songs go back on.
    #[test]
    fn soundtrack_replays_onto_a_re_established_image() {
        use crate::soundtrack::state::Soundtrack;
        use crate::soundtrack::testing::{synthetic_disc, synthetic_pathfind, synthetic_stream, synthetic_template, SYN_PT};
        use crate::soundtrack::{apply as sapply, pack, Slot};

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let pf = synthetic_pathfind(256);
        let tpl = synthetic_template();
        let src = root.join("src");
        std::fs::create_dir_all(&src).unwrap();
        let iso = synthetic_disc(&src, &[("pathfind.dat", &pf), ("template.dat", &tpl)]);
        let sha = crate::library::hash_file(&iso).unwrap();
        let pristine_dir = crate::game_image::pristine_dir(root, sha.as_str());
        std::fs::create_dir_all(&pristine_dir).unwrap();
        let pristine = pristine_dir.join(crate::game_image::IMAGE_FILE);
        std::fs::rename(&iso, &pristine).unwrap();
        let platform = Platform::detect();
        crate::game_image::establish(&platform, root, &pristine).unwrap();
        let booted = crate::game_image::booted_path(root);
        let size = std::fs::metadata(&pristine).unwrap().len() + 0x40000;

        let origin = crate::origin::Origin::new("http://127.0.0.1:9/");
        let verifier = crate::signing::Ed25519Verifier::embedded();
        let updater = Updater::new(&platform, root, &origin, &verifier);

        // nothing custom: nothing to replay
        assert!(updater.apply_current_soundtrack().unwrap().is_none());

        // a custom slot, applied
        let blob: Vec<u8> = (0..40).flat_map(|_| synthetic_stream(SYN_PT, 4800, 512)).collect();
        let pack_path = root.join("p.nsrmusic");
        pack::write_pack(&pack_path, "P", "", "", &[pack::SongInput { slot: Slot::new(5).unwrap(), blob, title: "Five".into(), artist: String::new(), album: String::new() }]).unwrap();
        sapply::stage_pack(root, &pack_path).unwrap();
        sapply::set_enabled(root, true).unwrap();
        match sapply::apply_with_size(&platform, root, Some(size), &mut |_| {}).unwrap() {
            sapply::Outcome::Applied(a) => assert_eq!(a.slots_changed, vec![5]),
            other => panic!("{other:?}"),
        }
        let custom = crate::library::hash_file(&booted).unwrap();

        // an update re-establishes the pristine image: the songs are gone
        crate::game_image::establish(&platform, root, &pristine).unwrap();
        assert_eq!(crate::library::hash_file(&booted).unwrap(), sha);

        // the replay puts them back. The toy disc needs its toy size, so the
        // test drives apply_with_size the way apply_current_soundtrack drives
        // apply, and checks the wrapper's own decisions separately.
        match sapply::apply_with_size(&platform, root, Some(size), &mut |_| {}).unwrap() {
            sapply::Outcome::Applied(a) => assert_eq!(a.slots_changed, vec![5]),
            other => panic!("{other:?}"),
        }
        assert_eq!(crate::library::hash_file(&booted).unwrap(), custom, "same bytes as before the update");

        // the switch off means no replay
        sapply::set_enabled(root, false).unwrap();
        assert!(updater.apply_current_soundtrack().unwrap().is_none());
        // (Driving the wrapper with a custom slot would repack the toy disc at
        // the real 1.46 GB size; apply_with_size above covers that path.)
        let _ = Soundtrack::load(root).unwrap();
    }
}
