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

/// A bucket name goes straight into the path-style request path, so only a
/// canonical S3 name is allowed: lowercase letters, digits and hyphens, 3 to
/// 63 characters, no slash, dot or whitespace. Nothing is trimmed.
fn valid_bucket_name(name: &str) -> bool {
    let b = name.as_bytes();
    let edge = |c: u8| c.is_ascii_lowercase() || c.is_ascii_digit();
    (3..=63).contains(&b.len()) && edge(b[0]) && edge(b[b.len() - 1]) && b.iter().all(|&c| edge(c) || c == b'-')
}

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
    if name == reborn_core::publish::vault::PRODUCTION_BUCKET {
        return Err(format!("{name} is the PUBLIC bucket. Members-only blobs never go there."));
    }
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
