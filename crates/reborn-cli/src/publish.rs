//! The publisher's side of updates: turn a tree into an origin a launcher can
//! update from.
//!
//! Three verbs, in the order they are used once:
//!
//!   keygen     the publisher key and the rotation key, generated together
//!   component  hash one tree into the origin: its manifest and its blobs
//!   release    assemble release.json from components already in the origin,
//!              sign it, and prove the signed file verifies before saying so
//!
//! The origin is a directory. Every file it holds is served as-is by any
//! static host, which is the whole design (`origin.rs`): a bucket behind a
//! CDN, or `python -m http.server` on a laptop for the proof. Nothing here
//! talks to a network.
//!
//! # What is never done here
//!
//! A published file is never rewritten. A blob is content-addressed and so
//! immutable by construction; a component manifest that already exists with
//! different bytes is refused rather than replaced, because a client holding
//! the old release pins the old hash and a swapped file reads as tampering
//! on every machine that already has it. Bump the sequence and publish a new
//! version instead. `release.json` is the one file that changes, and only to
//! a strictly higher sequence.
//!
//! # The bytes are audited, never the ledger
//!
//! A scan already hashed every source file. The copy in the origin is hashed
//! again, because the copy is what gets served and a disk, a scanner or a
//! sync client can alter it between the two. A release refuses to pin a
//! component whose blobs are not all present at their published length, so
//! a release never names content the origin does not hold. And the signed
//! `release.json` is read back off the disk and verified with the public key
//! before the tool reports success, which is the only proof that what a
//! launcher will fetch is what was signed.

use std::path::{Path, PathBuf};

use clap::Subcommand;

/// Publish the league build (members only): the league disc and its
/// texture folder, staged with league.json, published to the PRIVATE
/// blob store, then a release carrying every other pin forward. This
/// publishes to the local origin only; upload afterwards with
/// `publish upload --private --bucket reborn-league` FIRST, then
/// `publish upload`. If the default stage `<origin>-stage\league-game-<v>`
/// would overlap the disc, texture folder or origin (the iso or textures
/// sit in the origin's parent), the publish is refused: pass `--stage`
/// somewhere outside them.
#[derive(clap::Args, Debug)]
pub struct LeagueGameArgs {
    /// The league disc: a plain GameCube image with a league game id.
    #[arg(long)]
    iso: PathBuf,
    /// Named after the league game id, or holding a folder that is.
    #[arg(long)]
    textures: PathBuf,
    #[arg(long)]
    origin: PathBuf,
    #[arg(long)]
    version: String,
    /// Where the component is staged. Defaults to
    /// `<origin>-stage\league-game-<version>`; must not overlap the
    /// origin, the disc folder or the texture folder.
    #[arg(long)]
    stage: Option<PathBuf>,
    /// Defaults to one above the highest league-game sequence.
    #[arg(long)]
    sequence: Option<u64>,
    /// The publisher.key file. Without it the release step is only described.
    #[arg(long)]
    key: Option<PathBuf>,
    #[arg(long)]
    release_version: Option<String>,
    #[arg(long)]
    release_sequence: Option<u64>,
    #[arg(long, conflicts_with = "notes_file")]
    notes: Option<String>,
    #[arg(long)]
    notes_file: Option<PathBuf>,
    /// Stage, publish and sign. Without this everything is checked and
    /// nothing is written.
    #[arg(long)]
    apply: bool,
}

/// Publish the league seed (members only) as seed/league.s01. Local
/// origin only; upload with `publish upload --private --bucket
/// reborn-league` first, then `publish upload`.
#[derive(clap::Args, Debug)]
pub struct LeagueContentArgs {
    /// The captured league seed (a .s01 save state, at least 1 MiB).
    #[arg(long)]
    seed: PathBuf,
    /// make_seed.py's capture record, when there is one.
    #[arg(long)]
    seed_json: Option<PathBuf>,
    #[arg(long)]
    origin: PathBuf,
    #[arg(long)]
    version: String,
    /// Defaults to `<origin>-stage\league-content-<version>`; must not
    /// overlap the origin or the seed's folder (pass `--stage` if so).
    #[arg(long)]
    stage: Option<PathBuf>,
    #[arg(long)]
    sequence: Option<u64>,
    #[arg(long)]
    key: Option<PathBuf>,
    #[arg(long)]
    release_version: Option<String>,
    #[arg(long)]
    release_sequence: Option<u64>,
    #[arg(long, conflicts_with = "notes_file")]
    notes: Option<String>,
    #[arg(long)]
    notes_file: Option<PathBuf>,
    #[arg(long)]
    apply: bool,
}

#[derive(Subcommand, Debug)]
pub enum PublishCommand {
    /// Generate the publisher key and the rotation key, together, once.
    Keygen {
        /// Directory to write publisher.key and rotation.key into. Refuses to
        /// overwrite either.
        #[arg(long)]
        out: PathBuf,
    },
    /// Hash one component tree into the origin: its manifest and its blobs.
    Component {
        /// The tree to publish. A pristine one: a mod file in it becomes
        /// content every client is told is genuine.
        #[arg(long)]
        root: PathBuf,
        /// launcher, runtime, offline-content, online-content, textures, mods,
        /// or controller-db.
        #[arg(long)]
        component: String,
        #[arg(long)]
        version: String,
        /// Strictly higher than any sequence this component has shipped with.
        #[arg(long)]
        sequence: u64,
        /// Required for textures: library/textures/<game id>.
        #[arg(long)]
        install_root: Option<String>,
        /// The origin directory. Created if absent.
        #[arg(long)]
        origin: PathBuf,
        /// Write. Without this the tree is hashed and the work is described.
        #[arg(long)]
        apply: bool,
    },
    /// Assemble release.json from components already in the origin, and sign it.
    Release {
        #[arg(long)]
        origin: PathBuf,
        /// The release label players see, for example 2.4.0.
        #[arg(long)]
        version: String,
        /// Strictly higher than the sequence of the release.json already in
        /// the origin, when there is one.
        #[arg(long)]
        sequence: u64,
        /// The publisher.key file from keygen.
        #[arg(long)]
        key: PathBuf,
        /// A component to pin, as name=version, once per component. Its
        /// manifest must already be in the origin from `publish component`.
        #[arg(long = "component", value_name = "NAME=VERSION")]
        components: Vec<String>,
        /// What changed, in your words, shown to players on the update card.
        /// Signed with the release. One line per change.
        #[arg(long, conflicts_with = "notes_file")]
        notes: Option<String>,
        /// The same, read from a text file: easier for several lines.
        #[arg(long)]
        notes_file: Option<PathBuf>,
        /// Write and sign. Without this everything is checked and nothing is
        /// written.
        #[arg(long)]
        apply: bool,
    },
    /// Fetch the roster the site published and push it: the component, then
    /// a release that carries every other pinned component forward. One
    /// command from "the site exported it" to "upload the origin".
    Roster {
        /// Where the site put it: a URL, or a local .rebornroster path.
        #[arg(long)]
        from: String,
        #[arg(long)]
        origin: PathBuf,
        /// The roster's label, for example 2026.09.06.
        #[arg(long)]
        version: String,
        /// Defaults to one above the highest roster sequence in the origin.
        #[arg(long)]
        sequence: Option<u64>,
        /// The publisher.key file. Without it the component is published and
        /// the release step is only described.
        #[arg(long)]
        key: Option<PathBuf>,
        /// The release label players see. Defaults to the one already served.
        #[arg(long)]
        release_version: Option<String>,
        /// Defaults to one above the release already served.
        #[arg(long)]
        release_sequence: Option<u64>,
        /// What changed, in your words, shown to players on the update card.
        /// Signed with the release. One line per change.
        #[arg(long, conflicts_with = "notes_file")]
        notes: Option<String>,
        /// The same, read from a text file: easier for several lines.
        #[arg(long)]
        notes_file: Option<PathBuf>,
        /// Write and sign. Without this everything is fetched and checked and
        /// nothing in the origin changes.
        #[arg(long)]
        apply: bool,
        /// A file holding the site's API token, for an export URL that needs
        /// a login. Not the token itself: a token on the command line is in
        /// every process list. The vault entry set by `publish token` is
        /// used when this is absent.
        #[arg(long)]
        token_file: Option<PathBuf>,
    },
    /// Publish a game update: the xdelta from the build players have to the
    /// new build, appended to the chain, then a release carrying every other
    /// pinned component forward. A player's launcher applies the delta to its
    /// pristine image and re-applies the roster; nothing is re-downloaded.
    GamePatch {
        /// The pristine image players have now: the head of the chain, or
        /// the build the installer patches to when the chain is empty.
        #[arg(long)]
        from: PathBuf,
        /// The new pristine build.
        #[arg(long)]
        to: PathBuf,
        /// The build's label, for example 2026.09.07. Players see it, and it
        /// is the component version.
        #[arg(long)]
        version: String,
        /// The tree the chain accumulates in: chain.json and patches/. Kept
        /// outside the origin so uploads never carry it. Defaults to
        /// `<origin>-stage\game-patch`.
        #[arg(long)]
        tree: Option<PathBuf>,
        /// xdelta3. Defaults to the import bundle under target\installer-payload
        /// when run from the repository, else the bundle beside rebornctl.
        #[arg(long)]
        xdelta: Option<PathBuf>,
        #[arg(long)]
        origin: PathBuf,
        /// Defaults to one above the highest game-patch sequence in the origin.
        #[arg(long)]
        sequence: Option<u64>,
        /// The publisher.key file. Without it the component is published and
        /// the release step is only described.
        #[arg(long)]
        key: Option<PathBuf>,
        /// The release label players see. Defaults to the one already served.
        #[arg(long)]
        release_version: Option<String>,
        /// Defaults to one above the release already served.
        #[arg(long)]
        release_sequence: Option<u64>,
        /// Encode, verify, write and sign. Without this both images are
        /// hashed and the work is described; no delta is made.
        #[arg(long)]
        apply: bool,
    },
    /// Publish the `game-full` recovery component: the head build's complete
    /// disc, for a player whose own copy cannot be converted. Carried forward
    /// into later releases like every other pin, so recovery does not expire;
    /// re-run this only when the disc itself changes. A normal install still
    /// never downloads it: nothing but the launcher's recovery button ever
    /// asks for this component.
    GameFull {
        /// The complete built disc. Must hash to the game-patch chain's
        /// current head.
        #[arg(long)]
        iso: PathBuf,
        /// The build's label, matching the game-patch step that produced it.
        #[arg(long)]
        version: String,
        /// The tree the game-patch chain accumulates in, read here only for
        /// its head. Defaults to `<origin>-stage\game-patch`.
        #[arg(long)]
        chain_tree: Option<PathBuf>,
        /// Where the one-file component tree is staged before publishing.
        /// Defaults to `<origin>-stage\game-full`.
        #[arg(long)]
        stage: Option<PathBuf>,
        #[arg(long)]
        origin: PathBuf,
        /// Defaults to one above the highest game-full sequence in the origin.
        #[arg(long)]
        sequence: Option<u64>,
        /// The publisher.key file. Without it the component is published and
        /// the release step is only described.
        #[arg(long)]
        key: Option<PathBuf>,
        /// The release label players see. Defaults to the one already served.
        #[arg(long)]
        release_version: Option<String>,
        /// Defaults to one above the release already served.
        #[arg(long)]
        release_sequence: Option<u64>,
        /// Stage, verify, write and sign. Without this the disc is hashed and
        /// checked against the chain head, and nothing is written.
        #[arg(long)]
        apply: bool,
    },
    /// Publish the league build (members only): the league disc and its
    /// texture folder, staged with league.json, published to the PRIVATE
    /// blob store, then a release carrying every other pin forward. This
    /// publishes to the local origin only; upload afterwards with
    /// `publish upload --private --bucket reborn-league` FIRST, then
    /// `publish upload`. If the default stage `<origin>-stage\league-game-<v>`
    /// would overlap the disc, texture folder or origin (the iso or textures
    /// sit in the origin's parent), the publish is refused: pass `--stage`
    /// somewhere outside them.
    LeagueGame(Box<LeagueGameArgs>),
    /// Publish the league seed (members only) as seed/league.s01. Local
    /// origin only; upload with `publish upload --private --bucket
    /// reborn-league` first, then `publish upload`.
    LeagueContent(Box<LeagueContentArgs>),
    /// Upload the origin's plan to the bucket: every blob, then every
    /// component manifest, then the two release files, in that order, and
    /// read back what the public host serves once it is all up.
    Upload {
        /// The origin directory to upload.
        #[arg(long)]
        origin: PathBuf,
        /// Overrides the production R2 endpoint. For pointing this run at a
        /// stand-in instead of the real bucket.
        #[arg(long)]
        endpoint: Option<String>,
        /// Overrides the production bucket name. For pointing this run at a
        /// stand-in instead of the real bucket.
        #[arg(long)]
        bucket: Option<String>,
        /// The public host the read-back reads from after the upload.
        /// Defaults to the production CDN.
        #[arg(long)]
        public_base: Option<String>,
        /// Upload the MEMBERS-ONLY blobs (`<origin>/private/blobs`) to the
        /// private bucket named by --bucket, and nothing else. Run this
        /// BEFORE the public upload that carries their manifests. Never
        /// reads back through the public CDN.
        #[arg(long)]
        private: bool,
        /// Write. Without this, every object the plan would send is listed
        /// and its size is given, and nothing is uploaded.
        #[arg(long)]
        apply: bool,
    },
    /// What the bucket serves, what this local origin holds, what the
    /// launcher's own update channel serves, and the gap between the first
    /// two.
    Status {
        /// The local origin to read. Defaults to `C:\Reborn Origin`.
        #[arg(long)]
        origin: Option<PathBuf>,
        /// The bucket's public host. Defaults to the production CDN.
        #[arg(long)]
        origin_base: Option<String>,
        /// The launcher's own update manifest. Defaults to the production one.
        #[arg(long)]
        launcher: Option<String>,
    },
    /// Keep the site's API token and the bucket's R2 write credentials in
    /// the platform credential vault, so `publish roster` and `publish
    /// upload` never need a secret typed on a command line or sitting in a
    /// file the repository could see.
    Token {
        /// A file holding the site's API token. Read once; delete it
        /// afterwards.
        #[arg(long)]
        from_file: Option<PathBuf>,
        /// A file holding the R2 access key id. Read once; delete it
        /// afterwards.
        #[arg(long)]
        r2_access_from_file: Option<PathBuf>,
        /// A file holding the R2 secret access key. Read once; delete it
        /// afterwards.
        #[arg(long)]
        r2_secret_from_file: Option<PathBuf>,
        /// Remove the stored site token. Does not touch the R2 credentials.
        #[arg(long)]
        forget: bool,
    },
}

use reborn_core::inventory::{ComponentId, ComponentSpec, ScanObserver};
use reborn_core::publish::component::{publish_component, ComponentRequest};
use reborn_core::publish::game_full::{stage as stage_game_full, GameFullRequest};
use reborn_core::publish::game_patch::{publish_game_patch, GamePatchRequest};
use reborn_core::publish::keys::keygen;
use reborn_core::publish::release::{release, Pin, ReleaseRequest};
use reborn_core::publish::roster::{publish_roster, RosterRequest};
use reborn_core::publish::status::{read_status, PendingReason, Status};
use reborn_core::publish::upload::{
    plan_origin, upload as run_upload, UploadRequest, UreqTransport,
    DEFAULT_ATTEMPTS, DEFAULT_MULTIPART_THRESHOLD, DEFAULT_PART_SIZE,
};
use reborn_core::publish::vault::{
    bucket as vault_bucket, forget_site_token, r2_access, r2_secret, site_token, store_r2_access, store_r2_secret,
    store_site_token, R2_ACCESS_ACCOUNT, R2_SECRET_ACCOUNT, SITE_TOKEN_ACCOUNT,
};
use reborn_core::publish::roster::Push;
use reborn_core::publish::{Channel, Event, Outcome, Sink};
use reborn_core::signing::Ed25519Verifier;

/// The local origin `publish status` reads when `--origin` is not given:
/// where every publish from this machine has actually landed.
const DEFAULT_LOCAL_ORIGIN: &str = r"C:\Reborn Origin";

use super::{misuse, ScanLog, OK, PROBLEM};

/// Print an event where it went before the move: `Out` to stdout, `Err` to
/// stderr.
fn print(event: Event) {
    match event.channel {
        Channel::Out => println!("{}", event.line),
        Channel::Err => eprintln!("{}", event.line),
    }
}

/// The exit code an outcome earns. A dry run is a success.
fn exit_of<T>(outcome: &Outcome<T>) -> std::process::ExitCode {
    match outcome {
        Outcome::Done(_) | Outcome::Described => OK,
        Outcome::Refused(_) => PROBLEM,
    }
}

/// What comes next after a league publish: nothing here uploads. The private
/// store goes first so the release never points at a blob the members' store
/// does not hold yet. An unsigned release has to be signed before either
/// upload, so then the hint says only that.
fn league_hint(origin: &std::path::Path, signed: bool) -> String {
    if !signed {
        return "The release still needs signing (no --key): run the `publish release` command printed above, then upload."
            .to_string();
    }
    let o = origin.display();
    [
        "Next: upload the members-only blobs first, then the public files:".to_string(),
        format!("  rebornctl publish upload --private --bucket reborn-league --origin \"{o}\""),
        format!("  rebornctl publish upload --origin \"{o}\""),
    ]
    .join("\n")
}

/// Emit the hint for a landed publish; a refused or described run gets none.
fn league_next_step(outcome: &Outcome<Push>, origin: &std::path::Path, hint: &mut dyn FnMut(&str)) {
    if let Outcome::Done(push) = outcome {
        hint("");
        hint(&league_hint(origin, push.release.is_some()));
    }
}

pub fn run(cmd: PublishCommand) -> reborn_core::Result<std::process::ExitCode> {
    run_with(cmd, &mut |line| println!("{line}"))
}

fn run_with(cmd: PublishCommand, hint: &mut dyn FnMut(&str)) -> reborn_core::Result<std::process::ExitCode> {
    let mut sink = print;
    match cmd {
        PublishCommand::Keygen { out } => Ok(exit_of(&keygen(&out, &mut sink)?)),
        PublishCommand::Component {
            root,
            component,
            version,
            sequence,
            install_root,
            origin,
            apply,
        } => {
            let Some(id) = ComponentId::parse(&component) else {
                eprintln!("--component {component:?} is not a component name. Use one of:");
                for known in ComponentId::ALL {
                    eprintln!("  {}", known.as_str());
                }
                return Ok(misuse());
            };
            let spec = match install_root.as_deref() {
                Some(given) => ComponentSpec::with_install_root(id, &version, sequence, given),
                None => ComponentSpec::new(id, &version, sequence),
            };
            let spec = match spec {
                Ok(s) => s,
                Err(e) => {
                    println!("{e}");
                    if install_root.is_none() {
                        println!();
                        println!("Pass --install-root <relative path> to state it.");
                    }
                    return Ok(PROBLEM);
                }
            };
            let req = ComponentRequest { root: &root, spec, origin: &origin, apply };
            let log = ScanLog::new();
            Ok(exit_of(&publish_component(&req, &log as &dyn ScanObserver, &mut sink)?))
        }
        PublishCommand::Release {
            origin,
            version,
            sequence,
            key,
            components,
            notes,
            notes_file,
            apply,
        } => {
            let notes = read_notes(notes, notes_file.as_deref())?;
            if components.is_empty() {
                eprintln!("--component NAME=VERSION is required at least once. A release pinning");
                eprintln!("nothing is refused by the builder and useless to a client.");
                return Ok(misuse());
            }
            let mut pins = Vec::with_capacity(components.len());
            for spec in &components {
                let Some((name, ver)) = spec.split_once('=') else {
                    eprintln!("--component {spec:?}: expected NAME=VERSION.");
                    return Ok(misuse());
                };
                let Some(id) = ComponentId::parse(name) else {
                    eprintln!("--component {spec:?}: {name:?} is not a component name.");
                    return Ok(misuse());
                };
                pins.push(Pin { id, version: ver.to_string() });
            }
            let req = ReleaseRequest { origin: &origin, version: &version, sequence, key: &key, pins: &pins, notes: notes.as_deref(), apply };
            Ok(exit_of(&release(&req, &mut sink)?))
        }
        PublishCommand::Roster {
            from,
            origin,
            version,
            sequence,
            key,
            release_version,
            release_sequence,
            notes,
            notes_file,
            apply,
            token_file,
        } => {
            let notes = read_notes(notes, notes_file.as_deref())?;
            // The token: a file first, then the vault. Its value is never
            // printed; only whether one was sent.
            let token: Option<String> = match token_file.as_deref() {
                Some(p) => {
                    let t = std::fs::read_to_string(p).map_err(|e| reborn_core::Error::Io { path: p.to_path_buf(), source: e })?;
                    Some(t.trim().to_string()).filter(|t| !t.is_empty())
                }
                None => site_token(&reborn_core::Platform::detect()).unwrap_or(None),
            };
            let http = reborn_core::net::UreqHttp::new(concat!("rebornctl/", env!("CARGO_PKG_VERSION")));
            let req = RosterRequest {
                from: &from,
                origin: &origin,
                version: &version,
                sequence,
                key: key.as_deref(),
                release_version: release_version.as_deref(),
                release_sequence,
                notes: notes.as_deref(),
                apply,
                token: token.as_deref(),
            };
            let log = ScanLog::new();
            Ok(exit_of(&publish_roster(&http, &req, &log as &dyn ScanObserver, &mut sink)?))
        }
        PublishCommand::GamePatch {
            from,
            to,
            version,
            tree,
            xdelta,
            origin,
            sequence,
            key,
            release_version,
            release_sequence,
            apply,
        } => {
            let platform = reborn_core::Platform::detect();
            let req = GamePatchRequest {
                from: &from,
                to: &to,
                version: &version,
                tree: tree.as_deref(),
                xdelta: xdelta.as_deref(),
                origin: &origin,
                sequence,
                key: key.as_deref(),
                release_version: release_version.as_deref(),
                release_sequence,
                apply,
            };
            let log = ScanLog::new();
            Ok(exit_of(&publish_game_patch(&platform, &req, &log as &dyn ScanObserver, &mut sink)?))
        }
        PublishCommand::GameFull {
            iso,
            version,
            chain_tree,
            stage,
            origin,
            sequence,
            key,
            release_version,
            release_sequence,
            apply,
        } => {
            let req = GameFullRequest {
                iso: &iso,
                version: &version,
                chain_tree: chain_tree.as_deref(),
                stage: stage.as_deref(),
                origin: &origin,
                sequence,
                key: key.as_deref(),
                release_version: release_version.as_deref(),
                release_sequence,
                apply,
            };
            let log = ScanLog::new();
            Ok(exit_of(&stage_game_full(&req, &log as &dyn ScanObserver, &mut sink)?))
        }
        PublishCommand::LeagueGame(args) => {
            let LeagueGameArgs {
                iso,
                textures,
                origin,
                version,
                stage,
                sequence,
                key,
                release_version,
                release_sequence,
                notes,
                notes_file,
                apply,
            } = *args;
            let notes = read_notes(notes, notes_file.as_deref())?;
            let req = reborn_core::publish::league::LeagueGameRequest {
                iso: &iso,
                textures: &textures,
                version: &version,
                stage: stage.as_deref(),
                origin: &origin,
                sequence,
                key: key.as_deref(),
                release_version: release_version.as_deref(),
                release_sequence,
                notes: notes.as_deref(),
                apply,
            };
            let log = ScanLog::new();
            let outcome = reborn_core::publish::league::publish_league_game(&req, &log as &dyn ScanObserver, &mut sink)?;
            league_next_step(&outcome, &origin, hint);
            Ok(exit_of(&outcome))
        }
        PublishCommand::LeagueContent(args) => {
            let LeagueContentArgs {
                seed,
                seed_json,
                origin,
                version,
                stage,
                sequence,
                key,
                release_version,
                release_sequence,
                notes,
                notes_file,
                apply,
            } = *args;
            let notes = read_notes(notes, notes_file.as_deref())?;
            let req = reborn_core::publish::league::LeagueContentRequest {
                seed: &seed,
                seed_json: seed_json.as_deref(),
                version: &version,
                stage: stage.as_deref(),
                origin: &origin,
                sequence,
                key: key.as_deref(),
                release_version: release_version.as_deref(),
                release_sequence,
                notes: notes.as_deref(),
                apply,
            };
            let log = ScanLog::new();
            let outcome = reborn_core::publish::league::publish_league_content(&req, &log as &dyn ScanObserver, &mut sink)?;
            league_next_step(&outcome, &origin, hint);
            Ok(exit_of(&outcome))
        }
        PublishCommand::Upload {
            origin,
            endpoint,
            bucket,
            public_base,
            private,
            apply,
        } => upload_command(&origin, endpoint.as_deref(), bucket.as_deref(), public_base.as_deref(), private, apply, &mut sink),
        PublishCommand::Status { origin, origin_base, launcher } => {
            status_command(origin.as_deref(), origin_base.as_deref(), launcher.as_deref())
        }
        PublishCommand::Token {
            from_file,
            r2_access_from_file,
            r2_secret_from_file,
            forget,
        } => token_command(from_file.as_deref(), r2_access_from_file.as_deref(), r2_secret_from_file.as_deref(), forget),
    }
}

/// Which plan an upload sends.
#[derive(Debug, PartialEq, Eq)]
enum PlanKind {
    Origin,
    Private,
}

/// Everything `publish upload` decides from its flags, before the vault is
/// touched.
#[derive(Debug, PartialEq, Eq)]
struct Target {
    /// The bucket to write to; `None` keeps the production default.
    bucket: Option<String>,
    plan: PlanKind,
    /// The public host to read back from; always `None` for a private upload.
    read_back: Option<String>,
}

/// The canonical-name rule lives in the core, shared with the Publisher's
/// League files page.
use reborn_core::publish::upload::{check_private_bucket, valid_bucket_name};

/// Decide the bucket, the plan and the read-back from the flags. The public
/// bucket is refused for a private upload by name, so a typo can never send
/// league blobs where the CDN serves them.
fn upload_target(private: bool, bucket: Option<&str>, public_base: Option<&str>) -> Result<Target, String> {
    if let Some(name) = bucket {
        if !valid_bucket_name(name) {
            return Err(format!("--bucket {name:?} is not a bucket name (lowercase letters, digits and hyphens, 3 to 63 characters)."));
        }
    }
    if !private {
        let read_back = public_base.map(str::to_string).unwrap_or_else(|| reborn_core::origin::DEFAULT_ORIGIN.to_string());
        return Ok(Target { bucket: bucket.map(str::to_string), plan: PlanKind::Origin, read_back: Some(read_back) });
    }
    let Some(name) = bucket else {
        return Err("--private needs --bucket <private bucket>, for example --bucket reborn-league.".to_string());
    };
    if public_base.is_some() {
        return Err("--private never reads back through a public host; drop --public-base.".to_string());
    }
    check_private_bucket(name, &[])?;
    Ok(Target { bucket: Some(name.to_string()), plan: PlanKind::Private, read_back: None })
}

/// Build the bucket to upload to (the vault's credentials, with `--endpoint`
/// and `--bucket` overriding its address), plan the origin against what the
/// bucket already holds, and upload it.
fn upload_command(
    origin: &Path,
    endpoint: Option<&str>,
    bucket_name: Option<&str>,
    public_base: Option<&str>,
    private: bool,
    apply: bool,
    sink: Sink<'_>,
) -> reborn_core::Result<std::process::ExitCode> {
    let target = match upload_target(private, bucket_name, public_base) {
        Ok(t) => t,
        Err(why) => {
            eprintln!("{why}");
            return Ok(misuse());
        }
    };
    let platform = reborn_core::Platform::detect();
    let Some(mut bucket) = vault_bucket(&platform)? else {
        eprintln!("R2 credentials are not stored in the vault ({R2_ACCESS_ACCOUNT}, {R2_SECRET_ACCOUNT}).");
        eprintln!("Run these first; neither value is ever typed on this command line:");
        eprintln!("  rebornctl publish token --r2-access-from-file <file>");
        eprintln!("  rebornctl publish token --r2-secret-from-file <file>");
        return Ok(misuse());
    };
    if let Some(e) = endpoint {
        bucket.endpoint = e.to_string();
    }
    if let Some(b) = &target.bucket {
        bucket.bucket = b.clone();
    }

    let transport = UreqTransport::new();
    let (already, how) = reborn_core::publish::upload::already_on_bucket(&transport, &bucket)?;
    sink(Event::out(how));
    let plan = match target.plan {
        PlanKind::Private => reborn_core::publish::upload::plan_private(origin, &*already)?,
        PlanKind::Origin => plan_origin(origin, &*already)?,
    };

    let http = reborn_core::net::UreqHttp::new(concat!("rebornctl/", env!("CARGO_PKG_VERSION")));
    let req = UploadRequest {
        bucket: &bucket,
        plan: &plan,
        multipart_threshold: DEFAULT_MULTIPART_THRESHOLD,
        part_size: DEFAULT_PART_SIZE,
        attempts: DEFAULT_ATTEMPTS,
        public_base: target.read_back.as_deref(),
        apply,
    };
    Ok(exit_of(&run_upload(&transport, &http, &req, sink)?))
}


/// Read the bucket, the local origin and the launcher channel, print the
/// picture, and exit non-zero only when something could not be read or
/// verified. `pending` work is informational, not a problem: it is exactly
/// what this command exists to surface.
fn status_command(origin: Option<&Path>, origin_base: Option<&str>, launcher: Option<&str>) -> reborn_core::Result<std::process::ExitCode> {
    let origin = origin.map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from(DEFAULT_LOCAL_ORIGIN));
    let origin_base = origin_base.map(str::to_string).unwrap_or_else(|| reborn_core::origin::DEFAULT_ORIGIN.to_string());
    let launcher = launcher.map(str::to_string).unwrap_or_else(|| reborn_core::update::DEFAULT_SOURCE.to_string());

    let http = reborn_core::net::UreqHttp::new(concat!("rebornctl/", env!("CARGO_PKG_VERSION")));
    let verifier = Ed25519Verifier::embedded();
    let status = read_status(&http, &origin, &origin_base, &launcher, &verifier);
    print_status(&status);
    Ok(if status.problems.is_empty() { OK } else { PROBLEM })
}

fn print_status(status: &Status) {
    match &status.local {
        Some(local) => {
            println!("Local origin    {} sequence {}", local.version, local.sequence);
            for (id, version) in &local.pins {
                println!("                pin   {:<14} {version}", id.as_str());
            }
            for (id, version, sequence) in &local.components_on_disk {
                if !local.pins.iter().any(|(pid, pv)| pid == id && pv == version) {
                    println!("                on disk, unpinned  {:<14} {version} (sequence {sequence})", id.as_str());
                }
            }
        }
        None => println!("Local origin    could not be read; see problems below"),
    }
    println!();
    match &status.hosted {
        Some(hosted) => {
            println!(
                "Hosted release  {} sequence {} ({})",
                hosted.version,
                hosted.sequence,
                if hosted.verified { "signature verified" } else { "SIGNATURE NOT VERIFIED" }
            );
            for (id, version) in &hosted.pins {
                println!("                pin   {:<14} {version}", id.as_str());
            }
        }
        None => println!("Hosted release  could not be read; see problems below"),
    }
    println!();
    match &status.launcher {
        Some(launcher) => println!(
            "Launcher        {} ({}, {} byte(s))",
            launcher.version, launcher.file_name, launcher.bytes
        ),
        None => println!("Launcher        could not be read; see problems below"),
    }
    println!();
    if status.pending.is_empty() {
        println!("Pending         nothing; the bucket serves every pin this local origin has signed.");
    } else {
        println!("Pending         {} component(s):", status.pending.len());
        for p in &status.pending {
            let reason = match p.reason {
                PendingReason::NotServed => "the bucket does not serve it at all",
                PendingReason::OlderOnBucket => "the bucket serves an older version, with no newer local sequence to explain it",
                PendingReason::LocalReleaseNewer => "a newer local release has not been uploaded yet",
            };
            println!(
                "                {:<14} local {}  hosted {}  ({reason})",
                p.component.as_str(),
                p.local,
                p.hosted.as_deref().unwrap_or("(none)")
            );
        }
    }
    if !status.problems.is_empty() {
        println!();
        println!("Problems:");
        for line in &status.problems {
            println!("  {line}");
        }
    }
}

/// Store or forget the site token, and store the two R2 credential halves,
/// in the platform vault. With no flag at all, report each of the three as
/// "stored" or "missing" and nothing else: never a length, never a value.
fn token_command(
    from_file: Option<&Path>,
    r2_access_from_file: Option<&Path>,
    r2_secret_from_file: Option<&Path>,
    forget: bool,
) -> reborn_core::Result<std::process::ExitCode> {
    let platform = reborn_core::Platform::detect();
    if forget {
        forget_site_token(&platform)?;
        println!("Forgot the site token ({SITE_TOKEN_ACCOUNT}).");
        return Ok(OK);
    }

    let mut stored_anything = false;
    let mut bad_file = false;

    let mut try_store = |path: Option<&Path>, label: &str, account: &str, store: &dyn Fn(&str) -> reborn_core::Result<()>| -> reborn_core::Result<()> {
        let Some(path) = path else { return Ok(()) };
        if store_one_line_secret(path, store)? {
            println!("{label} ({account}): stored.");
            stored_anything = true;
        } else {
            println!("{} does not hold one value on one line. Nothing was stored.", path.display());
            bad_file = true;
        }
        Ok(())
    };
    try_store(from_file, "Site token", SITE_TOKEN_ACCOUNT, &|v| store_site_token(&platform, v))?;
    try_store(r2_access_from_file, "R2 access key id", R2_ACCESS_ACCOUNT, &|v| store_r2_access(&platform, v))?;
    try_store(r2_secret_from_file, "R2 secret access key", R2_SECRET_ACCOUNT, &|v| store_r2_secret(&platform, v))?;

    if bad_file {
        return Ok(PROBLEM);
    }
    if stored_anything {
        return Ok(OK);
    }

    let site = site_token(&platform)?.is_some();
    let access = r2_access(&platform)?.is_some();
    let secret = r2_secret(&platform)?.is_some();
    println!("Site token           ({SITE_TOKEN_ACCOUNT}): {}", if site { "stored" } else { "missing" });
    println!("R2 access key id     ({R2_ACCESS_ACCOUNT}): {}", if access { "stored" } else { "missing" });
    println!("R2 secret access key ({R2_SECRET_ACCOUNT}): {}", if secret { "stored" } else { "missing" });
    println!();
    println!("Pass --from-file, --r2-access-from-file, or --r2-secret-from-file <file> to set one,");
    println!("--forget to remove the site token.");
    Ok(OK)
}

/// Read `path`, require it to hold exactly one non-empty line with no
/// whitespace in it, and hand that line to `store`. `false` means the file
/// did not hold one clean line and nothing was stored; the caller names the
/// file when reporting that. Never logs the line itself.
fn store_one_line_secret(path: &Path, store: impl FnOnce(&str) -> reborn_core::Result<()>) -> reborn_core::Result<bool> {
    let text = std::fs::read_to_string(path).map_err(|e| reborn_core::Error::Io { path: path.to_path_buf(), source: e })?;
    let value = text.trim();
    if value.is_empty() || value.chars().any(char::is_whitespace) {
        return Ok(false);
    }
    store(value)?;
    Ok(true)
}

/// `--notes` as given, or `--notes-file` read whole. Neither is no notes.
fn read_notes(notes: Option<String>, file: Option<&std::path::Path>) -> reborn_core::Result<Option<String>> {
    match file {
        Some(p) => std::fs::read_to_string(p)
            .map(Some)
            .map_err(|e| reborn_core::Error::Io { path: p.to_path_buf(), source: e }),
        None => Ok(notes),
    }
}

#[cfg(test)]
mod private_upload_tests {
    use super::{upload_target, PlanKind};

    #[test]
    fn a_private_upload_needs_a_named_bucket() {
        assert!(upload_target(true, None, None).is_err());
        assert!(upload_target(true, Some("  "), None).is_err());
    }

    #[test]
    fn a_private_upload_never_reads_back_through_the_public_cdn() {
        assert!(upload_target(true, Some("reborn-league"), Some("https://files.nflstreetreborn.com/")).is_err());
    }

    #[test]
    fn a_private_upload_refuses_the_public_bucket_however_it_is_spelled() {
        for bad in [
            "reborn-content",
            "reborn-content/",
            "reborn-content/x",
            "Reborn-Content",
            "reborn-league/../reborn-content",
            " reborn-league",
            "reborn-league ",
            "reborn.league",
            "-reborn",
            "ab",
        ] {
            assert!(upload_target(true, Some(bad), None).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn the_league_bucket_is_private_and_never_read_back() {
        let t = upload_target(true, Some("reborn-league"), None).unwrap();
        assert_eq!(t.bucket.as_deref(), Some("reborn-league"));
        assert_eq!(t.plan, PlanKind::Private);
        assert_eq!(t.read_back, None);
    }

    #[test]
    fn a_plain_upload_keeps_the_production_bucket_and_reads_back() {
        let t = upload_target(false, Some("reborn-content"), None).unwrap();
        assert_eq!(t.bucket.as_deref(), Some("reborn-content"));
        assert_eq!(t.plan, PlanKind::Origin);
        assert!(t.read_back.is_some());
        assert!(upload_target(false, None, Some("https://x.test/")).unwrap().read_back.as_deref() == Some("https://x.test/"));
    }

    #[test]
    fn a_plain_upload_also_refuses_a_path_injecting_bucket() {
        for bad in ["reborn-content/", "a/b", "../x", " reborn-content", "Reborn"] {
            assert!(upload_target(false, Some(bad), None).is_err(), "{bad:?}");
        }
    }
}

#[cfg(test)]
mod league_tests {
    use super::{run, run_with, PublishCommand};
    use clap::Parser;
    use std::path::{Path, PathBuf};

    #[derive(clap::Parser)]
    struct Cli {
        #[command(subcommand)]
        cmd: PublishCommand,
    }

    /// Hand rolled: `tempfile` is not a dependency of this crate.
    struct Tmp(PathBuf);
    impl Tmp {
        fn new(tag: &str) -> Tmp {
            let p = std::env::temp_dir().join(format!("rebornctl-league-{tag}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&p);
            std::fs::create_dir_all(&p).unwrap();
            Tmp(p)
        }
    }
    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn fake_iso(path: &Path, id: &str) {
        let mut b = vec![0u8; 0x440];
        b[..6].copy_from_slice(id.as_bytes());
        b[0x1C..0x20].copy_from_slice(&[0xC2, 0x33, 0x9F, 0x3D]);
        std::fs::write(path, b).unwrap();
    }

    /// <root>/discs/league.iso, <root>/tex/LGTEST/a.png, origin <root>/work/origin.
    fn layout(root: &Path) -> (PathBuf, PathBuf, PathBuf) {
        let iso = root.join("discs").join("league.iso");
        std::fs::create_dir_all(iso.parent().unwrap()).unwrap();
        fake_iso(&iso, "LGTEST");
        let tex = root.join("tex");
        std::fs::create_dir_all(tex.join("LGTEST")).unwrap();
        std::fs::write(tex.join("LGTEST").join("a.png"), b"png").unwrap();
        let origin = root.join("work").join("origin");
        std::fs::create_dir_all(&origin).unwrap();
        (iso, tex, origin)
    }

    fn game(iso: &Path, tex: &Path, origin: &Path, extra: &[&str]) -> PublishCommand {
        let mut a: Vec<String> = vec![
            "x".into(), "league-game".into(),
            "--iso".into(), iso.display().to_string(),
            "--textures".into(), tex.display().to_string(),
            "--origin".into(), origin.display().to_string(),
            "--version".into(), "2026.10.08".into(),
        ];
        a.extend(extra.iter().map(|s| s.to_string()));
        Cli::try_parse_from(a).unwrap().cmd
    }

    fn code(c: std::process::ExitCode) -> String {
        format!("{c:?}")
    }

    #[test]
    fn league_commands_parse_with_the_contract_flags() {
        let g = Cli::try_parse_from([
            "x", "league-game", "--iso", "a.iso", "--textures", "t", "--origin", "o",
            "--version", "2026.10.08", "--notes-file", "n.md", "--apply",
        ]);
        assert!(g.is_ok(), "{:?}", g.err());
        let c = Cli::try_parse_from(["x", "league-content", "--seed", "league.s01", "--origin", "o", "--version", "1.0.0"]);
        assert!(c.is_ok(), "{:?}", c.err());
    }

    #[test]
    fn league_commands_require_origin_and_version_and_reject_both_notes() {
        assert!(Cli::try_parse_from(["x", "league-game", "--iso", "a", "--textures", "t", "--version", "1"]).is_err());
        assert!(Cli::try_parse_from(["x", "league-content", "--seed", "s", "--origin", "o"]).is_err());
        assert!(Cli::try_parse_from([
            "x", "league-content", "--seed", "s", "--origin", "o", "--version", "1",
            "--notes", "a", "--notes-file", "b",
        ]).is_err());
    }

    #[test]
    fn league_game_help_says_to_pass_stage_when_the_default_is_refused() {
        let mut cmd = <Cli as clap::CommandFactory>::command();
        let sub = cmd.find_subcommand_mut("league-game").unwrap();
        let help = sub.render_long_help().to_string();
        assert!(help.contains("--stage"), "{help}");
        assert!(help.contains("refus"), "{help}");
        for flag in ["--iso", "--textures", "--notes-file", "--origin", "--version", "--apply"] {
            assert!(help.contains(flag), "{flag} missing: {help}");
        }
    }

    #[test]
    fn a_dry_run_succeeds_and_writes_nothing() {
        let t = Tmp::new("dry");
        let (iso, tex, origin) = layout(&t.0);
        let before = std::fs::read_dir(t.0.join("work")).unwrap().count();
        let stage = t.0.join("stage");
        let cmd = game(&iso, &tex, &origin, &["--stage", &stage.display().to_string()]);
        assert_eq!(code(run(cmd).unwrap()), code(super::OK));
        assert_eq!(std::fs::read_dir(t.0.join("work")).unwrap().count(), before);
        assert!(!stage.exists());
    }

    #[test]
    fn the_default_stage_is_refused_clearly_when_the_iso_sits_beside_the_origin() {
        let t = Tmp::new("overlap");
        let (iso, tex, _) = layout(&t.0);
        // Origin directly beside the disc folder's contents: <root>/discs/origin,
        // so the default <origin>-stage lands inside the disc folder.
        let origin = t.0.join("discs").join("origin");
        std::fs::create_dir_all(&origin).unwrap();
        let cmd = game(&iso, &tex, &origin, &[]);
        assert_eq!(code(run(cmd).unwrap()), code(super::PROBLEM));
        assert!(!t.0.join("discs").join("origin-stage").exists());
    }

    #[test]
    fn a_non_disc_is_refused_and_a_bad_notes_file_is_an_error() {
        let t = Tmp::new("bad");
        let (iso, tex, origin) = layout(&t.0);
        std::fs::write(&iso, vec![0u8; 0x440]).unwrap();
        let stage = t.0.join("stage");
        let s = stage.display().to_string();
        assert_eq!(code(run(game(&iso, &tex, &origin, &["--stage", &s])).unwrap()), code(super::PROBLEM));
        fake_iso(&iso, "LGTEST");
        let missing = t.0.join("nope.md").display().to_string();
        assert!(run(game(&iso, &tex, &origin, &["--stage", &s, "--notes-file", &missing])).is_err());
    }

    #[test]
    fn league_content_dry_run_and_refusals() {
        let t = Tmp::new("content");
        let origin = t.0.join("work").join("origin");
        std::fs::create_dir_all(&origin).unwrap();
        let seed = t.0.join("seeds").join("league.s01");
        std::fs::create_dir_all(seed.parent().unwrap()).unwrap();
        std::fs::write(&seed, vec![1u8; 1024 * 1024 + 8]).unwrap();
        let stage = t.0.join("stage").display().to_string();
        let mk = |seed: &Path| {
            Cli::try_parse_from([
                "x", "league-content", "--seed", &seed.display().to_string(),
                "--origin", &origin.display().to_string(), "--version", "1.0.0", "--stage", &stage,
            ]).unwrap().cmd
        };
        assert_eq!(code(run(mk(&seed)).unwrap()), code(super::OK));
        assert!(!Path::new(&stage).exists());
        let tiny = t.0.join("seeds").join("tiny.s01");
        std::fs::write(&tiny, b"x").unwrap();
        assert_eq!(code(run(mk(&tiny)).unwrap()), code(super::PROBLEM));
    }

    #[test]
    fn a_signed_apply_prints_the_two_quoted_upload_commands_in_order() {
        let t = Tmp::new("hint dir");
        let (iso, tex, _) = layout(&t.0);
        let origin = t.0.join("my work").join("origin");
        std::fs::create_dir_all(&origin).unwrap();
        let key = match reborn_core::publish::keys::keygen(&t.0.join("keys"), &mut |_| {}).unwrap() {
            reborn_core::publish::Outcome::Done(w) => w.publisher_path,
            o => panic!("{o:?}"),
        };
        let stage = t.0.join("stage").display().to_string();
        let key_s = key.display().to_string();
        let cmd = game(&iso, &tex, &origin, &[
            "--stage", &stage, "--key", &key_s, "--release-version", "3.0.0", "--release-sequence", "1", "--apply",
        ]);
        let mut lines: Vec<String> = Vec::new();
        let c = run_with(cmd, &mut |l| lines.push(l.to_string())).unwrap();
        assert_eq!(code(c), code(super::OK));
        let o = origin.display();
        assert_eq!(
            lines.join("
"),
            format!(
                "\nNext: upload the members-only blobs first, then the public files:\n\
                 \x20 rebornctl publish upload --private --bucket reborn-league --origin \"{o}\"\n\
                 \x20 rebornctl publish upload --origin \"{o}\""
            )
        );
        assert!(o.to_string().contains(' '));
    }

    #[test]
    fn an_unsigned_apply_says_the_release_needs_signing_and_hints_no_upload() {
        let t = Tmp::new("nokey");
        let (iso, tex, origin) = layout(&t.0);
        let stage = t.0.join("stage").display().to_string();
        let cmd = game(&iso, &tex, &origin, &[
            "--stage", &stage, "--release-version", "3.0.0", "--release-sequence", "1", "--apply",
        ]);
        let mut lines: Vec<String> = Vec::new();
        run_with(cmd, &mut |l| lines.push(l.to_string())).unwrap();
        let all = lines.join("
");
        assert!(all.contains("still needs signing"), "{all}");
        assert!(!all.contains("publish upload"), "{all}");
    }
}
