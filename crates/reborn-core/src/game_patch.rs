//! Game updates as deltas between pristine builds.
//!
//! The installer ships one delta from the stock disc to the current Reborn
//! build, and the import applies it once. After that the player's disc is
//! gone, so a new build cannot be made the same way. Instead the `game-patch`
//! content component carries a **chain**: one xdelta per consecutive pair of
//! Reborn builds, each about a megabyte, plus `chain.json` naming the hashes
//! at both ends of every step. A launcher hashes its pristine image, finds
//! that hash in the chain, and applies every step after it in order. The
//! content system fetches only the steps a player does not already have,
//! because the chain is append-only and its files are content-addressed.
//!
//! Every step is verified by hash before the next one runs, and the final
//! image is verified before it replaces anything. A player whose pristine
//! image is not in the chain (an older Reborn build imported from a 2.1
//! install) is told so and asked to import from the disc; nothing of theirs
//! is touched.
//!
//! Rollback of a game update is not carried here. The prior pristine build
//! is not retained (1.46 GB per player), and a reverse delta is a later
//! addition if it is ever needed. Today the rollback is an import from the
//! disc, which is what it was before this component existed.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::game_image::{self, Establish, EstablishOutcome, Pristine, IMAGE_FILE};
use crate::library::hash_file;
use crate::manifest::Sha256Hex;
use crate::platform::{DataDir, Error, Platform, Result};

pub const CHAIN_FILE: &str = "chain.json";
pub const CHAIN_SCHEMA: &str = "nsr.game-patch-chain.v1";
/// Where the deltas sit inside the component, relative to its root.
pub const PATCH_DIR: &str = "patches";
/// The delta tool's program name, without suffix.
pub const XDELTA_NAME: &str = "xdelta3";
/// The folder beside the launcher holding the import bundle, which carries
/// the delta tool.
pub const BUNDLE_DIR: &str = "game-import";
pub const DISC_ID: &str = "GN7E69";

fn refuse(message: impl Into<String>) -> Error {
    Error::PlainIo(std::io::Error::new(std::io::ErrorKind::InvalidData, message.into()))
}

fn is_sha256_hex(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

/// One delta: from one pristine build to the next.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Step {
    pub from_sha256: String,
    pub to_sha256: String,
    pub to_bytes: u64,
    /// Path of the xdelta, relative to the component root. Always under
    /// [`PATCH_DIR`].
    pub patch: String,
    /// What players see: the build's label.
    pub label: String,
}

/// The ordered chain of deltas. Linear: each step starts where the previous
/// one ended, so any pristine build in it has exactly one route to the head.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Chain {
    pub schema: String,
    pub disc_id: String,
    pub steps: Vec<Step>,
}

impl Default for Chain {
    fn default() -> Self {
        Self::new()
    }
}

/// What the chain says about one pristine hash.
#[derive(Debug, PartialEq, Eq)]
pub enum Route<'a> {
    /// The hash is the head of the chain: nothing to apply.
    Current,
    /// The steps to apply, in order.
    Steps(Vec<&'a Step>),
    /// Not a build the chain knows.
    Unknown,
}

impl Chain {
    pub fn new() -> Self {
        Self {
            schema: CHAIN_SCHEMA.to_string(),
            disc_id: DISC_ID.to_string(),
            steps: Vec::new(),
        }
    }

    /// Parse and validate. Refuses a chain whose steps are not linear, whose
    /// hashes are not hashes, or whose patch paths could leave the component
    /// root. A chain is served content: a launcher must not act on one that
    /// asks it to read outside its own folder.
    pub fn parse(bytes: &[u8]) -> Result<Chain> {
        let chain: Chain = serde_json::from_slice(bytes).map_err(|e| refuse(format!("chain.json does not parse: {e}")))?;
        if chain.schema != CHAIN_SCHEMA {
            return Err(refuse(format!(
                "chain.json has schema {:?}, expected {CHAIN_SCHEMA:?}",
                chain.schema
            )));
        }
        if chain.disc_id != DISC_ID {
            return Err(refuse(format!("chain.json is for disc {:?}, not {DISC_ID}", chain.disc_id)));
        }
        let mut previous: Option<&str> = None;
        for (i, step) in chain.steps.iter().enumerate() {
            if !is_sha256_hex(&step.from_sha256) || !is_sha256_hex(&step.to_sha256) {
                return Err(refuse(format!("chain.json step {i} carries a hash that is not 64 lowercase hex characters")));
            }
            if step.from_sha256 == step.to_sha256 {
                return Err(refuse(format!("chain.json step {i} goes from a build to itself")));
            }
            if let Some(p) = previous {
                if p != step.from_sha256 {
                    return Err(refuse(format!(
                        "chain.json is not linear: step {i} starts at {} but step {} ended at {p}",
                        &step.from_sha256[..12],
                        i - 1
                    )));
                }
            }
            if chain.steps[..i].iter().any(|s| s.to_sha256 == step.to_sha256 || s.from_sha256 == step.to_sha256) {
                return Err(refuse(format!("chain.json step {i} returns to an earlier build, which would loop")));
            }
            let p = Path::new(&step.patch);
            let clean = step.patch.starts_with(&format!("{PATCH_DIR}/"))
                && !step.patch.contains("..")
                && !step.patch.contains(':')
                && !step.patch.contains('\\')
                && p.components().all(|c| matches!(c, std::path::Component::Normal(_)));
            if !clean {
                return Err(refuse(format!(
                    "chain.json step {i} names a patch outside {PATCH_DIR}/: {:?}",
                    step.patch
                )));
            }
            if step.to_bytes == 0 {
                return Err(refuse(format!("chain.json step {i} says the result is zero bytes")));
            }
            previous = Some(&step.to_sha256);
        }
        Ok(chain)
    }

    pub fn load(path: &Path) -> Result<Option<Chain>> {
        match std::fs::read(path) {
            Ok(bytes) => Chain::parse(&bytes).map(Some),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(source) => Err(Error::Io {
                path: path.to_path_buf(),
                source,
            }),
        }
    }

    pub fn encode(&self) -> Result<Vec<u8>> {
        serde_json::to_vec_pretty(self).map_err(|e| refuse(format!("could not encode chain.json: {e}")))
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| Error::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        std::fs::write(path, self.encode()?).map_err(|source| Error::Io {
            path: path.to_path_buf(),
            source,
        })
    }

    /// The hash of the newest build.
    pub fn head(&self) -> Option<&str> {
        self.steps.last().map(|s| s.to_sha256.as_str())
    }

    /// The hash the chain starts from.
    pub fn base(&self) -> Option<&str> {
        self.steps.first().map(|s| s.from_sha256.as_str())
    }

    /// The build label of the node with this hash. The root has no label: it
    /// is the unmodified disc, not a Reborn build.
    pub fn label_for(&self, sha256: &str) -> Option<String> {
        self.steps.iter().find(|s| s.to_sha256 == sha256).map(|s| s.label.clone())
    }

    pub fn route(&self, from: &str) -> Route<'_> {
        if self.head() == Some(from) {
            return Route::Current;
        }
        match self.steps.iter().position(|s| s.from_sha256 == from) {
            Some(i) => Route::Steps(self.steps[i..].iter().collect()),
            None => Route::Unknown,
        }
    }

    /// Append a step. Refuses one that does not start at the head, because
    /// that is how a chain stops being a chain.
    pub fn push(&mut self, step: Step) -> Result<()> {
        if let Some(head) = self.head() {
            if head != step.from_sha256 {
                return Err(refuse(format!(
                    "the chain ends at {} and this step starts at {}; a game patch is always made \
                     from the build currently served",
                    &head[..12],
                    &step.from_sha256[..12]
                )));
            }
        }
        self.steps.push(step);
        // Re-validate through the same rules a client applies.
        Chain::parse(&self.encode()?)?;
        Ok(())
    }

    /// The patch file name for a step between these two builds.
    pub fn patch_name(from_sha256: &str, to_sha256: &str) -> String {
        format!("{PATCH_DIR}/{}-{}.xdelta", &from_sha256[..12], &to_sha256[..12])
    }
}

/// What one delta application did.
#[derive(Debug, Clone)]
pub struct PatchRun {
    pub succeeded: bool,
    /// The last lines the tool printed, for a refusal message.
    pub tail: String,
}

/// Applies a delta: `out = source + patch`. The real one runs xdelta3; a
/// test one writes bytes it was told about.
pub trait Patcher: std::fmt::Debug {
    fn decode(&self, source: &Path, patch: &Path, out: &Path) -> Result<PatchRun>;
    /// For messages: what tool this is.
    fn describe(&self) -> String;
}

/// xdelta3, the tool the import bundle ships.
#[derive(Debug)]
pub struct Xdelta3<'a> {
    platform: &'a Platform,
    pub exe: PathBuf,
}

impl<'a> Xdelta3<'a> {
    pub fn new(platform: &'a Platform, exe: PathBuf) -> Self {
        Self { platform, exe }
    }
}

fn tail_of(run: &crate::platform::CapturedRun) -> String {
    let all = format!("{}\n{}", run.stdout, run.stderr);
    let lines: Vec<&str> = all.lines().filter(|l| !l.trim().is_empty()).collect();
    lines[lines.len().saturating_sub(12)..].join("\n")
}

impl Patcher for Xdelta3<'_> {
    fn decode(&self, source: &Path, patch: &Path, out: &Path) -> Result<PatchRun> {
        let args: Vec<OsString> = vec![
            "-d".into(),
            "-f".into(),
            "-s".into(),
            source.as_os_str().to_os_string(),
            patch.as_os_str().to_os_string(),
            out.as_os_str().to_os_string(),
        ];
        let run = self.platform.processes.run_capturing(&self.exe, &args, &Default::default())?;
        Ok(PatchRun {
            succeeded: run.succeeded(),
            tail: tail_of(&run),
        })
    }

    fn describe(&self) -> String {
        self.exe.display().to_string()
    }
}

/// Where the delta tool is looked for: the import bundle beside the
/// launcher, the install root, then whatever the caller adds.
pub fn xdelta_dirs(platform: &Platform, extra: &[PathBuf]) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(install) = platform.paths.install_root() {
        dirs.push(install.join(BUNDLE_DIR));
        dirs.push(install);
    }
    dirs.extend(extra.iter().cloned());
    dirs
}

pub fn find_xdelta(platform: &Platform, dirs: &[PathBuf]) -> Option<PathBuf> {
    let name = format!("{XDELTA_NAME}{}", platform.paths.exe_suffix());
    dirs.iter().map(|d| d.join(&name)).find(|p| p.is_file())
}

/// Where the component's files are, for one data root.
pub fn component_root(data_root: &Path) -> PathBuf {
    data_root.join(DataDir::LibraryGamePatch.relative())
}

#[derive(Debug)]
pub struct ApplyRequest<'a> {
    pub data_root: &'a Path,
    pub patcher: &'a dyn Patcher,
}

#[derive(Debug, Clone)]
pub struct ApplyReceipt {
    pub from_sha256: String,
    pub to_sha256: String,
    /// The labels of the steps applied, in order.
    pub steps: Vec<String>,
    pub pristine: PathBuf,
    pub booted: PathBuf,
    pub booted_how: Establish,
    /// Old pristine folders removed once the new image was in place.
    pub retired: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// The component is not installed, or its chain is missing.
    NoChain { path: PathBuf },
    NoPristine { dir: PathBuf },
    /// The pristine image is a build the chain does not know.
    UnknownBase { sha256: String, chain_base: Option<String>, chain_head: Option<String> },
    ImageInUse { path: PathBuf },
    CrossVolume { staging: PathBuf, library: PathBuf },
    MissingPatch { path: PathBuf },
    PatchFailed { label: String, tail: String },
    Truncated { label: String, expected: u64, got: u64 },
    Verification { label: String, expected: String, got: String },
}

impl Refusal {
    pub fn message(&self) -> String {
        match self {
            Refusal::NoChain { path } => format!(
                "No game updates are installed ({} is absent). Nothing was changed.",
                path.display()
            ),
            Refusal::NoPristine { dir } => format!(
                "No imported game image was found under {}. Import your disc first. Nothing was changed.",
                dir.display()
            ),
            Refusal::UnknownBase { sha256, chain_base, chain_head } => format!(
                "Your game image ({}…) is not a build this update knows (it covers {}… through {}…). \
                 It was imported from a copy rather than patched from your disc. Import from your disc \
                 to receive game updates. Nothing was changed.",
                &sha256[..12],
                chain_base.as_deref().map(|s| &s[..12]).unwrap_or("?"),
                chain_head.as_deref().map(|s| &s[..12]).unwrap_or("?")
            ),
            Refusal::ImageInUse { path } => format!(
                "{} is open in another program, so it was not replaced. Close the game and try again. \
                 Nothing was changed.",
                path.display()
            ),
            Refusal::CrossVolume { staging, library } => format!(
                "Staging ({}) and the library ({}) are on different volumes, so the new image could not \
                 be moved into place without a copy. Nothing was changed.",
                staging.display(),
                library.display()
            ),
            Refusal::MissingPatch { path } => format!(
                "The update names a patch file that is not installed: {}. Run the update again. Nothing \
                 was changed.",
                path.display()
            ),
            Refusal::PatchFailed { label, tail } => format!(
                "Applying the game update to {label} failed. The tool said:\n{tail}\nYour game is as it was."
            ),
            Refusal::Truncated { label, expected, got } => format!(
                "The game update to {label} produced {got} bytes, expected {expected}. It was discarded. \
                 Your game is as it was."
            ),
            Refusal::Verification { label, expected, got } => format!(
                "The game update to {label} did not verify: expected {}…, got {}…. It was discarded. \
                 Your game is as it was.",
                &expected[..12],
                &got[..12]
            ),
        }
    }
}

#[derive(Debug)]
pub enum Outcome {
    Applied(ApplyReceipt),
    /// The pristine image is already the head of the chain.
    UpToDate { sha256: String, pristine: PathBuf },
    Refused(Refusal),
}

/// Walk the chain from the pristine image to its head, then re-establish the
/// booted image from the result and retire the old pristine folder.
///
/// The booted image after this is the new build without a roster; the
/// caller applies the current roster next (see
/// `content::pipeline::Updater::apply_current_roster`). Call only with the
/// game closed.
pub fn apply(platform: &Platform, req: &ApplyRequest<'_>) -> Result<Outcome> {
    let chain_path = component_root(req.data_root).join(CHAIN_FILE);
    let Some(chain) = Chain::load(&chain_path)? else {
        return Ok(Outcome::Refused(Refusal::NoChain { path: chain_path }));
    };
    let Some(Pristine { path: pristine, .. }) = game_image::find_pristine(req.data_root) else {
        return Ok(Outcome::Refused(Refusal::NoPristine {
            dir: game_image::imported_root(req.data_root),
        }));
    };
    let booted = game_image::booted_path(req.data_root);
    if booted.exists() && platform.files.is_locked(&booted) {
        return Ok(Outcome::Refused(Refusal::ImageInUse { path: booted }));
    }

    // The folder name says what the importer verified; the bytes say what is
    // there now. A delta over the wrong bytes produces garbage that the
    // final check would catch, but only after minutes of work.
    let base = hash_file(&pristine)?;
    let steps = match chain.route(base.as_str()) {
        Route::Current => {
            return Ok(Outcome::UpToDate {
                sha256: base.as_str().to_string(),
                pristine,
            })
        }
        Route::Unknown => {
            return Ok(Outcome::Refused(Refusal::UnknownBase {
                sha256: base.as_str().to_string(),
                chain_base: chain.base().map(str::to_string),
                chain_head: chain.head().map(str::to_string),
            }))
        }
        Route::Steps(steps) => steps,
    };
    let target = steps.last().expect("a route has at least one step");

    let library = game_image::imported_root(req.data_root);
    std::fs::create_dir_all(&library).map_err(|source| Error::Io {
        path: library.clone(),
        source,
    })?;
    let staging = req
        .data_root
        .join(DataDir::Staging.relative())
        .join("game-patch")
        .join(&target.to_sha256[..12]);
    std::fs::create_dir_all(&staging).map_err(|source| Error::Io {
        path: staging.clone(),
        source,
    })?;
    if !platform.files.same_volume(&staging, &library)? {
        return Ok(Outcome::Refused(Refusal::CrossVolume { staging, library }));
    }

    let mut source = pristine.clone();
    let mut intermediate: Option<PathBuf> = None;
    let mut labels = Vec::new();
    for (i, step) in steps.iter().enumerate() {
        let patch = component_root(req.data_root).join(step.patch.replace('/', std::path::MAIN_SEPARATOR_STR));
        if !patch.is_file() {
            discard(intermediate.take());
            return Ok(Outcome::Refused(Refusal::MissingPatch { path: patch }));
        }
        let out = staging.join(format!("step-{i}.iso"));
        if out.exists() {
            std::fs::remove_file(&out).map_err(|source| Error::Io {
                path: out.clone(),
                source,
            })?;
        }
        let run = req.patcher.decode(&source, &patch, &out)?;
        discard(intermediate.take());
        if !run.succeeded {
            let _ = std::fs::remove_file(&out);
            return Ok(Outcome::Refused(Refusal::PatchFailed {
                label: step.label.clone(),
                tail: run.tail,
            }));
        }
        let got = std::fs::metadata(&out).map_err(|source| Error::Io { path: out.clone(), source })?.len();
        if got != step.to_bytes {
            let _ = std::fs::remove_file(&out);
            return Ok(Outcome::Refused(Refusal::Truncated {
                label: step.label.clone(),
                expected: step.to_bytes,
                got,
            }));
        }
        let actual = hash_file(&out)?;
        if actual.as_str() != step.to_sha256 {
            let _ = std::fs::remove_file(&out);
            return Ok(Outcome::Refused(Refusal::Verification {
                label: step.label.clone(),
                expected: step.to_sha256.clone(),
                got: actual.as_str().to_string(),
            }));
        }
        labels.push(step.label.clone());
        intermediate = Some(out.clone());
        source = out;
    }
    let result = intermediate.take().expect("the last step left its output");

    // Into place under its own hash, then the booted copy, then the old
    // folder. In that order so a failure at any point leaves a playable
    // image: the old one until the new booted copy exists, the new one after.
    let dest_dir = game_image::pristine_dir(req.data_root, &target.to_sha256);
    std::fs::create_dir_all(&dest_dir).map_err(|source| Error::Io {
        path: dest_dir.clone(),
        source,
    })?;
    let dest = dest_dir.join(IMAGE_FILE);
    platform.files.atomic_replace(&result, &dest)?;
    let _ = std::fs::remove_dir(&staging);

    let established = match game_image::establish(platform, req.data_root, &dest)? {
        EstablishOutcome::Done(e) => e,
        EstablishOutcome::Refused(game_image::EstablishRefusal::BootedInUse { path }) => {
            return Ok(Outcome::Refused(Refusal::ImageInUse { path }))
        }
        EstablishOutcome::Refused(game_image::EstablishRefusal::NoPristine { path }) => {
            return Ok(Outcome::Refused(Refusal::NoPristine { dir: path }))
        }
    };
    let retired = game_image::retire_other_pristine(req.data_root, &dest);

    Ok(Outcome::Applied(ApplyReceipt {
        from_sha256: base.as_str().to_string(),
        to_sha256: target.to_sha256.clone(),
        steps: labels,
        pristine: dest,
        booted: established.path,
        booted_how: established.how,
        retired,
    }))
}

fn discard(path: Option<PathBuf>) {
    if let Some(p) = path {
        let _ = std::fs::remove_file(p);
    }
}

/// The hash of an image, as the chain spells it.
pub fn sha256_of(path: &Path) -> Result<Sha256Hex> {
    hash_file(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn sha(bytes: &[u8]) -> String {
        Sha256Hex::from_bytes(bytes).as_str().to_string()
    }

    fn step(from: &[u8], to: &[u8], label: &str) -> Step {
        Step {
            from_sha256: sha(from),
            to_sha256: sha(to),
            to_bytes: to.len() as u64,
            patch: Chain::patch_name(&sha(from), &sha(to)),
            label: label.to_string(),
        }
    }

    /// Applies a delta by looking the (source hash, patch name) pair up in a
    /// table. A real xdelta is not needed to prove the walk, the checks, and
    /// the swap; e2e-game-patch.py proves the real tool.
    #[derive(Debug)]
    struct Table {
        outputs: BTreeMap<(String, String), Vec<u8>>,
        lie: bool,
    }

    impl Patcher for Table {
        fn decode(&self, source: &Path, patch: &Path, out: &Path) -> Result<PatchRun> {
            let key = (sha(&std::fs::read(source).unwrap()), patch.file_name().unwrap().to_string_lossy().into_owned());
            match self.outputs.get(&key) {
                Some(bytes) => {
                    let bytes = if self.lie { b"garbage".to_vec() } else { bytes.clone() };
                    std::fs::write(out, bytes).unwrap();
                    Ok(PatchRun { succeeded: true, tail: String::new() })
                }
                None => Ok(PatchRun { succeeded: false, tail: "no such delta".into() }),
            }
        }
        fn describe(&self) -> String {
            "table".into()
        }
    }

    /// A data root with a pristine image, a booted copy, and a chain with
    /// its patch files present.
    fn install(data_root: &Path, pristine: &[u8], chain: &Chain) -> PathBuf {
        let platform = Platform::detect();
        let dir = game_image::pristine_dir(data_root, &sha(pristine));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join(IMAGE_FILE);
        std::fs::write(&p, pristine).unwrap();
        game_image::establish(&platform, data_root, &p).unwrap();
        chain.save(&component_root(data_root).join(CHAIN_FILE)).unwrap();
        for s in &chain.steps {
            let f = component_root(data_root).join(&s.patch);
            std::fs::create_dir_all(f.parent().unwrap()).unwrap();
            std::fs::write(&f, b"delta bytes").unwrap();
        }
        p
    }

    const B1: &[u8] = b"reborn build one, a whole game";
    const B2: &[u8] = b"reborn build two, a whole game with a fix";
    const B3: &[u8] = b"reborn build three, a whole game with two fixes";

    fn chain_1_2_3() -> Chain {
        let mut c = Chain::new();
        c.push(step(B1, B2, "2026.09.07")).unwrap();
        c.push(step(B2, B3, "2026.09.08")).unwrap();
        c
    }

    fn table() -> Table {
        let mut outputs = BTreeMap::new();
        outputs.insert((sha(B1), Chain::patch_name(&sha(B1), &sha(B2)).split('/').last().unwrap().to_string()), B2.to_vec());
        outputs.insert((sha(B2), Chain::patch_name(&sha(B2), &sha(B3)).split('/').last().unwrap().to_string()), B3.to_vec());
        Table { outputs, lie: false }
    }

    #[test]
    fn a_chain_is_linear_validated_and_routes_from_any_build_in_it() {
        let c = chain_1_2_3();
        assert_eq!(c.base(), Some(sha(B1).as_str()));
        assert_eq!(c.head(), Some(sha(B3).as_str()));
        assert_eq!(c.route(&sha(B3)), Route::Current);
        assert_eq!(c.route(&sha(B1)), Route::Steps(vec![&c.steps[0], &c.steps[1]]));
        assert_eq!(c.route(&sha(B2)), Route::Steps(vec![&c.steps[1]]));
        assert_eq!(c.route(&sha(b"never built")), Route::Unknown);

        let mut broken = c.clone();
        assert!(broken.push(step(B1, b"side branch", "x")).is_err(), "a step must start at the head");
        assert!(broken.push(step(B3, B1, "loop")).is_err(), "returning to an earlier build loops");

        let mut evil = c.clone();
        evil.steps[0].patch = "../../saves/card.raw".into();
        assert!(Chain::parse(&evil.encode().unwrap()).is_err(), "a patch path must stay under patches/");
        let mut wrong_disc = c.clone();
        wrong_disc.disc_id = "FR4N31".into();
        assert!(Chain::parse(&wrong_disc.encode().unwrap()).is_err());
        assert!(Chain::parse(&c.encode().unwrap()).is_ok());
    }

    #[test]
    fn apply_walks_every_step_verifies_each_and_leaves_one_pristine_and_a_booted_copy() {
        let dir = tempfile::tempdir().unwrap();
        let platform = Platform::detect();
        let old = install(dir.path(), B1, &chain_1_2_3());
        let t = table();

        let receipt = match apply(&platform, &ApplyRequest { data_root: dir.path(), patcher: &t }).unwrap() {
            Outcome::Applied(r) => r,
            other => panic!("{other:?}"),
        };
        assert_eq!(receipt.from_sha256, sha(B1));
        assert_eq!(receipt.to_sha256, sha(B3));
        assert_eq!(receipt.steps, vec!["2026.09.07", "2026.09.08"]);
        assert_eq!(std::fs::read(&receipt.pristine).unwrap(), B3);
        assert_eq!(receipt.pristine, game_image::pristine_dir(dir.path(), &sha(B3)).join(IMAGE_FILE));
        assert_eq!(std::fs::read(&receipt.booted).unwrap(), B3, "the booted copy is the new build");
        assert!(!old.exists(), "the old pristine folder is retired");
        assert_eq!(receipt.retired.len(), 1);
        assert_eq!(game_image::list_pristine(dir.path()).len(), 1);
        assert!(
            std::fs::read_dir(dir.path().join("staging").join("game-patch")).map(|d| d.count()).unwrap_or(0) == 0,
            "no intermediate image is left in staging"
        );

        match apply(&platform, &ApplyRequest { data_root: dir.path(), patcher: &t }).unwrap() {
            Outcome::UpToDate { sha256, .. } => assert_eq!(sha256, sha(B3)),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_build_the_chain_does_not_know_is_refused_and_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let platform = Platform::detect();
        let mine = install(dir.path(), b"a 2.1 image with its own roster", &chain_1_2_3());
        match apply(&platform, &ApplyRequest { data_root: dir.path(), patcher: &table() }).unwrap() {
            Outcome::Refused(Refusal::UnknownBase { chain_base, chain_head, .. }) => {
                assert_eq!(chain_base.unwrap(), sha(B1));
                assert_eq!(chain_head.unwrap(), sha(B3));
            }
            other => panic!("{other:?}"),
        }
        assert!(mine.exists());
        assert_eq!(std::fs::read(game_image::booted_path(dir.path())).unwrap(), b"a 2.1 image with its own roster");
    }

    #[test]
    fn a_step_that_does_not_verify_is_discarded_and_nothing_moves() {
        let dir = tempfile::tempdir().unwrap();
        let platform = Platform::detect();
        let old = install(dir.path(), B1, &chain_1_2_3());
        let mut t = table();
        t.lie = true;
        match apply(&platform, &ApplyRequest { data_root: dir.path(), patcher: &t }).unwrap() {
            Outcome::Refused(Refusal::Truncated { label, .. } | Refusal::Verification { label, .. }) => {
                assert_eq!(label, "2026.09.07", "the first step is the one that failed");
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(std::fs::read(&old).unwrap(), B1);
        assert_eq!(std::fs::read(game_image::booted_path(dir.path())).unwrap(), B1);
        assert_eq!(game_image::list_pristine(dir.path()).len(), 1);

        // A missing patch file is named, not guessed around.
        let t = table();
        let c = chain_1_2_3();
        std::fs::remove_file(component_root(dir.path()).join(&c.steps[1].patch)).unwrap();
        match apply(&platform, &ApplyRequest { data_root: dir.path(), patcher: &t }).unwrap() {
            Outcome::Refused(Refusal::MissingPatch { path }) => assert!(path.ends_with(Path::new(&c.steps[1].patch))),
            other => panic!("{other:?}"),
        }
        assert_eq!(std::fs::read(&old).unwrap(), B1, "the first step's output was discarded with the refusal");
        assert_eq!(game_image::list_pristine(dir.path()).len(), 1);
    }

    #[test]
    fn with_no_chain_or_no_image_the_refusal_says_which() {
        let dir = tempfile::tempdir().unwrap();
        let platform = Platform::detect();
        let t = table();
        assert!(matches!(
            apply(&platform, &ApplyRequest { data_root: dir.path(), patcher: &t }).unwrap(),
            Outcome::Refused(Refusal::NoChain { .. })
        ));
        chain_1_2_3().save(&component_root(dir.path()).join(CHAIN_FILE)).unwrap();
        assert!(matches!(
            apply(&platform, &ApplyRequest { data_root: dir.path(), patcher: &t }).unwrap(),
            Outcome::Refused(Refusal::NoPristine { .. })
        ));
        for r in [
            Refusal::NoChain { path: "c".into() },
            Refusal::NoPristine { dir: "d".into() },
            Refusal::UnknownBase { sha256: sha(B1), chain_base: Some(sha(B2)), chain_head: Some(sha(B3)) },
            Refusal::ImageInUse { path: "i".into() },
            Refusal::CrossVolume { staging: "s".into(), library: "l".into() },
            Refusal::MissingPatch { path: "p".into() },
            Refusal::PatchFailed { label: "x".into(), tail: "t".into() },
            Refusal::Truncated { label: "x".into(), expected: 2, got: 1 },
            Refusal::Verification { label: "x".into(), expected: sha(B1), got: sha(B2) },
        ] {
            let m = r.message();
            assert!(m.contains("Nothing was changed") || m.contains("as it was"), "{r:?}: {m}");
        }
    }

    #[test]
    fn the_delta_tool_is_found_in_the_bundle_beside_the_launcher_first() {
        let platform = Platform::detect();
        let dir = tempfile::tempdir().unwrap();
        let bundle = dir.path().join(BUNDLE_DIR);
        std::fs::create_dir_all(&bundle).unwrap();
        let name = format!("{XDELTA_NAME}{}", platform.paths.exe_suffix());
        assert_eq!(find_xdelta(&platform, &[dir.path().to_path_buf()]), None);
        std::fs::write(bundle.join(&name), b"").unwrap();
        assert_eq!(find_xdelta(&platform, &[bundle.clone()]), Some(bundle.join(&name)));
        let dirs = xdelta_dirs(&platform, &[dir.path().to_path_buf()]);
        assert_eq!(dirs.last(), Some(&dir.path().to_path_buf()));
    }
}
