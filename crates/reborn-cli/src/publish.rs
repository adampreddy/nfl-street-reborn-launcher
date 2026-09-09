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
    /// Keep the site's API token in the platform credential vault, so
    /// `publish roster` can fetch a login-only export without the token ever
    /// appearing on a command line or in a file the repository could see.
    Token {
        /// A file holding the token. Read once; delete it afterwards.
        #[arg(long)]
        from_file: Option<PathBuf>,
        /// Remove the stored token.
        #[arg(long)]
        forget: bool,
    },
}

use reborn_core::inventory::{ComponentId, ComponentSpec, ScanObserver};
use reborn_core::publish::component::{publish_component, ComponentRequest};
use reborn_core::publish::game_patch::{publish_game_patch, GamePatchRequest};
use reborn_core::publish::keys::keygen;
use reborn_core::publish::release::{release, Pin, ReleaseRequest};
use reborn_core::publish::roster::{publish_roster, RosterRequest};
use reborn_core::publish::vault::{forget_site_token, site_token, store_site_token, SITE_TOKEN_ACCOUNT};
use reborn_core::publish::{Channel, Event, Outcome};

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

pub fn run(cmd: PublishCommand) -> reborn_core::Result<std::process::ExitCode> {
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
            apply,
        } => {
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
            let req = ReleaseRequest { origin: &origin, version: &version, sequence, key: &key, pins: &pins, apply };
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
            apply,
            token_file,
        } => {
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
        PublishCommand::Token { from_file, forget } => token_command(from_file.as_deref(), forget),
    }
}

/// Store or forget the site's API token in the platform vault.
fn token_command(from_file: Option<&Path>, forget: bool) -> reborn_core::Result<std::process::ExitCode> {
    let platform = reborn_core::Platform::detect();
    if forget {
        forget_site_token(&platform)?;
        println!("Forgot the site token ({SITE_TOKEN_ACCOUNT}).");
        return Ok(OK);
    }
    let Some(path) = from_file else {
        let present = site_token(&platform)?.is_some();
        println!(
            "Site token ({SITE_TOKEN_ACCOUNT}): {}. Pass --from-file <file> to set it, --forget to remove it.",
            if present { "stored" } else { "not stored" }
        );
        return Ok(OK);
    };
    let text = std::fs::read_to_string(path).map_err(|e| reborn_core::Error::Io { path: path.to_path_buf(), source: e })?;
    let token = text.trim();
    if token.is_empty() || token.chars().any(char::is_whitespace) {
        println!("{} does not hold one token on one line. Nothing was stored.", path.display());
        return Ok(PROBLEM);
    }
    store_site_token(&platform, token)?;
    println!(
        "Stored the site token ({SITE_TOKEN_ACCOUNT}), {} characters. Delete {} now; the vault is its home.",
        token.len(),
        path.display()
    );
    Ok(OK)
}
