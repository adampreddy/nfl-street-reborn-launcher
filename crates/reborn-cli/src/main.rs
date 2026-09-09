//! `rebornctl`, the command line front end for the launcher core.
//!
//! # What this is for
//!
//! The UI is not written yet, and a library that only its own tests have ever
//! run is a library nobody has actually checked. This binary runs the same code
//! against the real machine: the real install, the real 2.1 folder, the real
//! runtime. Every test in `reborn-core` can pass while the seams are still
//! wrong about this particular Windows box.
//!
//! It is not a throwaway. The Tauri front end will call the same entry points,
//! so anything reachable here is reachable there, and a subcommand that behaves
//! sensibly under a terminal is one less thing to debug through a web view.
//!
//! # Rules it follows
//!
//! Nothing here writes unless asked. Every destructive subcommand plans first
//! and needs `--apply` to do anything, because the thing being migrated is
//! somebody's saves.
//!
//! Exit status is `0` when there is nothing to do about the answer, `1` when
//! there is something the person running it must act on, and `2` for misuse.
//! A refusal is not a crash: a subsystem that cannot do what was asked says
//! which piece is missing and exits `1`, so a script can tell "your files are
//! damaged" from "you typed the flag wrong".
//!
//! Two properties this file keeps deliberately:
//!
//! **Anything that walks a large tree says how large it is first.** The
//! measured install has 35,015 files, including a 27,025 file texture pack, and
//! a scan of it is minutes of silence. Where the core takes a progress observer
//! it is given one; where it does not, the size of the work is printed before
//! the wait starts, so a person can tell a long job from a hung one.
//!
//! **A partial answer says so.** Following `devices`, which reports the
//! backends it could not search, every listing here names what it could not
//! read rather than presenting a shorter list as the whole truth.
//!
//! Several commands ask the core for a data directory, and
//! [`Platform::data_dir`] creates the directory it resolves. So a read-only
//! command can leave behind an empty `stats/queue` or `config`. That is the
//! layout being created, not content being written, and it is the only thing
//! any command here does without `--apply`.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use clap::{Parser, Subcommand};

use reborn_core::content::{ContentLayout, ContentService, Journal};
use reborn_core::devices::DeviceScanner;
use reborn_core::inventory::{
    scan_component, ComponentId, ComponentSpec, ScanControl, ScanObserver, ScanOutcome, ScanPhase,
    ScanProgress, SerializedComponent, UnsignedReleaseBuilder,
};
use reborn_core::jsgme::{Conversion, JsgmeLibrary, Occupied};
use reborn_core::library::{FileVerdict, RebuildState, Verifier, VerifyReport};
use reborn_core::manifest::Component;
use reborn_core::migration::{LegacyInstall, MigrationPlan};
use reborn_core::mods::{
    activation_path, detect_conflicts, packages_root, Activation, ComposeRequest, Composer,
    Composition, GameId, LinkMethod, ModId, ModPackage, PackageVersion, COMPOSED_MARKER,
};
use reborn_core::onboarding::{Environment, Next, Onboarding};
use reborn_core::platform::VolumeKind;
use reborn_core::runtime::{GameBuild, GraphicsPreset, RuntimePackager};
use reborn_core::session::{build_gcpad, resolve_user_dir, LaunchRequest, Launched, Session};
use reborn_core::stats::{capture_folders_on_disk, EntryState, StatsQueue};
use reborn_core::supervisor::{SessionEnd, Startup, Supervisor};
use reborn_core::{DataDir, Platform};

/// The publisher's verbs. Its own file because it is the one part of this
/// binary that holds a signing key in memory, and that is worth being able to
/// read in isolation.
mod publish;

#[derive(Parser, Debug)]
#[command(
    name = "rebornctl",
    about = "Inspect and operate an NFL Street Reborn install",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Where everything lives on this machine, and whether a launcher is running.
    Status,

    /// Whether a newer launcher is published, and fetch it. Installs nothing.
    Update {
        #[command(subcommand)]
        what: UpdateCommand,
    },

    /// Publish content: keys, component manifests and blobs, and a signed release.
    Publish {
        #[command(subcommand)]
        what: publish::PublishCommand,
    },

    /// Apply the published roster to the installed game image, or re-apply it.
    Roster {
        #[command(subcommand)]
        what: RosterCommand,
    },

    /// The game update chain: apply what is installed to the pristine image.
    GamePatch {
        #[command(subcommand)]
        what: GamePatchCommand,
    },

    /// Show what migrating a 2.1 install would do. Copies nothing without --apply.
    Migrate {
        /// The 2.1 install root. Defaults to the standard location.
        #[arg(long)]
        from: Option<PathBuf>,
        /// Actually copy. Without this the command only prints the plan.
        #[arg(long)]
        apply: bool,
    },

    /// Check a runtime folder for files that must not ship.
    AuditRuntime {
        /// The runtime folder to inspect.
        path: PathBuf,
    },

    /// Print the GCPadNew.ini the launcher would generate.
    Controller {
        /// Ask this Reborn Dolphin what it calls the devices. Strongly preferred.
        #[arg(long)]
        runtime: Option<PathBuf>,
        /// Write to this path instead of standard output.
        #[arg(long)]
        out: Option<PathBuf>,
        /// How many ports to configure.
        #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u8).range(1..=4))]
        ports: u8,
    },

    /// Boot the game, watch it, and record how the session ended.
    Play {
        /// The game image to boot.
        #[arg(long)]
        iso: PathBuf,
        /// The Reborn Dolphin executable.
        #[arg(long)]
        runtime: PathBuf,
        /// performance, balanced, quality, or ultra.
        #[arg(long, default_value = "balanced")]
        preset: String,
        /// Start windowed instead of fullscreen.
        #[arg(long)]
        windowed: bool,
        /// The Dolphin user folder to boot against. Defaults to the existing
        /// install's User folder, which is where settings, memory cards and
        /// texture packs already live.
        #[arg(long)]
        user_dir: Option<PathBuf>,
        /// Write the generated configuration and stop, without launching.
        #[arg(long)]
        dry_run: bool,
    },

    /// List controllers, named the way the emulator names them.
    Devices {
        /// Ask this Reborn Dolphin what it calls the devices. Strongly preferred.
        #[arg(long)]
        runtime: Option<PathBuf>,
    },

    /// Recent play sessions, including ones that ended badly.
    Sessions,

    /// The mod library: what is on disk, what is turned on, what would change.
    Mods {
        #[command(subcommand)]
        what: ModsCommand,
    },

    /// Check an installed component against a manifest, without changing it.
    Verify {
        /// The `nsr.component.v1` manifest to check against.
        #[arg(long)]
        manifest: PathBuf,
        /// The tree to check. Defaults to the manifest's own install root under
        /// the data root.
        #[arg(long)]
        root: Option<PathBuf>,
        /// The record of what the launcher's own rebuilds produced. Defaults to
        /// music/state.json under the data root.
        #[arg(long)]
        state: Option<PathBuf>,
    },

    /// Say exactly what a repair would re-fetch, and why each file is in the set.
    Repair {
        #[arg(long)]
        manifest: PathBuf,
        #[arg(long)]
        root: Option<PathBuf>,
        #[arg(long)]
        state: Option<PathBuf>,
        /// Try to carry the repair out. See the note this prints: nothing can
        /// fetch yet, and it says which piece is missing.
        #[arg(long)]
        apply: bool,
    },

    /// Volumes the operating system will admit to, and which the Xbox wizard
    /// may write to.
    Drives,

    /// Where first run got to, and what it still has to ask.
    Onboarding {
        /// The 2.1 install, which decides whether the migration stages exist.
        #[arg(long)]
        from: Option<PathBuf>,
    },

    /// Match results waiting to be sent.
    Stats {
        #[command(subcommand)]
        what: StatsCommand,
    },

    /// Build an unmodded copy of a texture tree, by un-applying JSGME.
    ///
    /// Composition layers mods over a pristine tree, and a live 2.1 install has
    /// none: JSGME applies by overwriting in place, so its tree is the base with
    /// whatever is applied baked in. Copying it as-is would make those mods
    /// permanent.
    Pristine {
        /// The install holding User and MODS.
        #[arg(long)]
        from: PathBuf,
        /// The build to copy, for example GN7E69.
        #[arg(long)]
        build: String,
        /// Where the pristine copy goes. Defaults to the launcher's library.
        #[arg(long)]
        to: Option<PathBuf>,
        /// Copy and un-apply. Without this, nothing is written.
        #[arg(long)]
        apply: bool,
    },

    /// Hash a tree and produce the component manifest that describes it.
    Inventory {
        /// The tree to describe. Scan a pristine tree: a mod file sitting in it
        /// becomes content the manifest claims is genuine.
        #[arg(long)]
        root: PathBuf,
        /// launcher, runtime, offline-content, online-content, textures, mods,
        /// or controller-db.
        #[arg(long)]
        component: String,
        #[arg(long)]
        version: String,
        #[arg(long)]
        sequence: u64,
        /// Required for textures, launcher and controller-db, where the install
        /// root does not follow from the component name.
        #[arg(long)]
        install_root: Option<String>,
        /// Directory to write `<component>.json` into. Required with --apply.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Also write an unsigned release pinning this manifest here.
        #[arg(long)]
        release_out: Option<PathBuf>,
        /// The address the manifest will be served from. There is no default:
        /// the release pins it and a guessed url makes a release nobody can use.
        #[arg(long)]
        pin_url: Option<String>,
        /// Write the manifest. Without this the tree is still hashed and
        /// reported, and no file is created.
        #[arg(long)]
        apply: bool,
    },

    /// The custom soundtrack: imported audio, the record, and what is missing.
    Music,

    /// Installed component versions, and any update the last run left half done.
    Content {
        /// Put an interrupted swap back. Without this the state is only reported.
        #[arg(long)]
        apply: bool,
    },
}

/// Reading and composing the mod library.
///
/// `apply` and `unapply` are the same operation with a different package list,
/// because that is what the core does: un-applying a mod is rebuilding the
/// composed tree without it, never restoring a displaced file. The 2.1 product
/// used JSGME, which moves the original into `!BACKUP` and makes that the only
/// copy in existence.
#[derive(Subcommand, Debug)]
enum UpdateCommand {
    /// Read the release manifest and compare it with this build.
    Check {
        /// The manifest URL. Defaults to the release page for this channel.
        #[arg(long)]
        source: Option<String>,
    },
    /// Download the newer installer into the downloads folder and verify its
    /// hash. Nothing is installed; the path is printed.
    Download {
        #[arg(long)]
        source: Option<String>,
    },
    /// Update one content component from a signed origin: plan, fetch, verify,
    /// and swap it live. Without --apply, only the plan is printed.
    Content {
        /// The origin base URL, the directory holding release.json. Defaults
        /// to the product's content server.
        #[arg(long)]
        origin: Option<String>,
        /// runtime, offline-content, online-content, textures, or mods.
        #[arg(long)]
        component: String,
        /// A public key to trust for THIS run, as 64 hex characters. For
        /// proving a release before its key is compiled into a launcher. A
        /// shipped launcher trusts only the keys built into it.
        #[arg(long)]
        trust: Option<String>,
        /// Use this data root instead of the real one. For tests: a promotion
        /// into the real root is a real update.
        #[arg(long)]
        data_root: Option<PathBuf>,
        /// Fetch and swap. Without this, nothing is downloaded.
        #[arg(long)]
        apply: bool,
        /// xdelta3, for a game patch. Found in the import bundle beside the
        /// launcher when absent.
        #[arg(long)]
        xdelta: Option<PathBuf>,
        /// The Builder's console build, for a roster. Found automatically
        /// when absent.
        #[arg(long)]
        builder: Option<PathBuf>,
        /// The install root whose shipped runtime and textures seed the data
        /// root before the check. Defaults to the real one when no
        /// --data-root is given, and to nothing when one is: a test root is
        /// seeded only when told to.
        #[arg(long)]
        install_root: Option<PathBuf>,
    },
}

#[derive(Subcommand, Debug)]
enum RosterCommand {
    /// Run the Reborn Builder over the live image with a roster, then swap
    /// the result in. About three seconds. The game must be closed.
    Apply {
        /// The .rebornroster. Defaults to the one the roster component installed.
        #[arg(long)]
        roster: Option<PathBuf>,
        /// The Builder's console build. Found automatically when absent.
        #[arg(long)]
        builder: Option<PathBuf>,
        /// Use this data root instead of the real one. For tests.
        #[arg(long)]
        data_root: Option<PathBuf>,
        /// Where portraits are written. Defaults to the pristine texture tree
        /// on the real data root, and to a scratch folder with --data-root.
        #[arg(long)]
        textures: Option<PathBuf>,
        /// Build even if the site flagged problems. They are printed either way.
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Subcommand, Debug)]
enum GamePatchCommand {
    /// Walk the installed chain from the pristine image to its newest build,
    /// re-establish the booted image, and re-apply the roster. The game
    /// must be closed.
    Apply {
        /// Use this data root instead of the real one. For tests.
        #[arg(long)]
        data_root: Option<PathBuf>,
        /// xdelta3. Found in the import bundle beside the launcher when absent.
        #[arg(long)]
        xdelta: Option<PathBuf>,
        /// The Builder's console build, for the roster. Found automatically when absent.
        #[arg(long)]
        builder: Option<PathBuf>,
        /// Where portraits are written. Defaults to the pristine texture tree
        /// on the real data root, and to a scratch folder with --data-root.
        #[arg(long)]
        textures: Option<PathBuf>,
    },
}

#[derive(Subcommand, Debug)]
enum ModsCommand {
    /// Packages, which are active, the pristine trees, and the JSGME library.
    List {
        /// The 2.1 install holding the JSGME `MODS` folder.
        #[arg(long)]
        from: Option<PathBuf>,
        /// A build id to treat as installed, repeatable. Without this the
        /// recognized builds are the directory names under library/textures,
        /// and a texture whose build is not recognized is never imported.
        #[arg(long)]
        build: Vec<String>,
    },

    /// What importing the JSGME library into mods/packages would do.
    Import {
        #[arg(long)]
        from: Option<PathBuf>,
        #[arg(long)]
        build: Vec<String>,
        /// The version to stamp the imported packages with. JSGME records no
        /// version anywhere, so this is stated rather than derived.
        #[arg(long, default_value = "1.0.0")]
        version: String,
        /// Carry the import out.
        #[arg(long)]
        apply: bool,
    },

    /// Point the emulator's texture folder at the composed tree.
    ///
    /// Composition builds a tree the emulator does not read. Without this the
    /// mods are applied correctly and none of it reaches the game.
    Link {
        /// The build to link, for example GN7E69.
        #[arg(long)]
        game: String,
        /// The Dolphin user folder. Defaults to the one a launch would use.
        #[arg(long)]
        user_dir: Option<PathBuf>,
        /// Make the link. Without this, nothing is changed.
        #[arg(long)]
        apply: bool,
    },

    /// Make the game load what activation.json says is on.
    ///
    /// The same call a launch makes: verify the composed tree against the
    /// active list, rebuild it only if it differs, and point the emulator's
    /// texture folder at it. Idle when nothing changed.
    Sync {
        /// The build to sync, for example GN7E69.
        #[arg(long, default_value = "GN7E69")]
        game: String,
        /// The Dolphin user folder. Defaults to the one a launch would use.
        #[arg(long)]
        user_dir: Option<PathBuf>,
    },

    /// Turn a package on and rebuild the composed tree.
    Apply {
        /// The package id, for example jsgme.purple-football.
        #[arg(long)]
        id: String,
        /// Which version of it. Only needed when more than one is installed,
        /// because picking one for the user would decide which payload runs.
        #[arg(long)]
        version: Option<String>,
        /// The build to compose for. Defaults to the only pristine tree present.
        #[arg(long)]
        game: Option<String>,
        /// Write the activation and build the tree.
        #[arg(long)]
        apply: bool,
    },

    /// Turn a package off and rebuild the composed tree without it.
    Unapply {
        #[arg(long)]
        id: String,
        #[arg(long)]
        game: Option<String>,
        #[arg(long)]
        apply: bool,
    },

    /// Check that the composed tree is what the activation list says it is.
    Verify {
        #[arg(long)]
        game: Option<String>,
    },
}

/// The stats queue. Nothing here sends anything: see [`stats_list`].
#[derive(Subcommand, Debug)]
enum StatsCommand {
    /// Every capture in the queue and where it has got to.
    List,

    /// Take the captures Dolphin wrote into the queue.
    Ingest {
        /// Read one folder instead of all of them. Without this, every folder a
        /// launch reads is swept: the mode folders inside GameStats, the folder
        /// itself, and the folder older runtimes wrote.
        #[arg(long)]
        from: Option<PathBuf>,
        /// The launcher session that accounts for these results. Without it
        /// every capture is quarantined, because nothing accounts for it.
        #[arg(long)]
        session: Option<String>,
        /// Move them. The source file is deleted once its capture is safely in
        /// the queue, so this needs asking for.
        #[arg(long)]
        apply: bool,
    },

    /// Put stalled captures back in the queue.
    Retry {
        #[arg(long)]
        apply: bool,
    },

    /// Delete the payloads of results that reached the server.
    Prune {
        /// How long a finished result is kept. There is no default: retention is
        /// a policy and this tool does not invent one.
        #[arg(long)]
        keep_days: u64,
        #[arg(long)]
        apply: bool,
    },
}

fn main() -> std::process::ExitCode {
    // A reader that goes away is not a fault. `rebornctl status | head`
    // closes the pipe after the first lines, and every println! after that
    // fails with a broken pipe, which the standard library turns into a
    // panic with a stack trace on stderr. The player sees a crash message
    // from a command that did exactly what was asked. So a failed print to
    // stdout exits quietly, and every other panic keeps the default report.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let message = info
            .payload()
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| info.payload().downcast_ref::<&str>().copied())
            .unwrap_or("");
        if message.starts_with("failed printing to stdout") {
            std::process::exit(0);
        }
        default_hook(info);
    }));

    // The same log the window writes, from the same place, so a person running
    // a recovery command and a player pressing a button leave their detail in
    // one file rather than in two or in none. It says nothing and it cannot
    // fail a command.
    reborn_core::logging::start(&Platform::detect());

    let cli = Cli::parse();
    match run(cli) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::ExitCode::from(2)
        }
    }
}

fn run(cli: Cli) -> reborn_core::Result<std::process::ExitCode> {
    let platform = Platform::detect();
    match cli.command {
        Command::Status => status(&platform),
        Command::Update { what } => match what {
            UpdateCommand::Check { source } => update_check(&platform, source.as_deref(), false),
            UpdateCommand::Download { source } => update_check(&platform, source.as_deref(), true),
            UpdateCommand::Content {
                origin,
                component,
                trust,
                data_root,
                apply,
                xdelta,
                builder,
                install_root,
            } => update_content(
                &platform,
                origin.as_deref().unwrap_or(reborn_core::origin::DEFAULT_ORIGIN),
                &component,
                trust.as_deref(),
                data_root.as_deref(),
                apply,
                xdelta.as_deref(),
                builder.as_deref(),
                install_root.as_deref(),
            ),
        },
        Command::Migrate { from, apply } => migrate(&platform, from, apply),
        Command::AuditRuntime { path } => audit_runtime(&path),
        Command::Controller {
            runtime,
            out,
            ports,
        } => controller(&platform, runtime.as_deref(), out, ports),
        Command::Sessions => sessions(&platform),
        Command::Devices { runtime } => list_devices(&platform, runtime.as_deref()),
        Command::Play {
            iso,
            runtime,
            preset,
            windowed,
            dry_run,
            user_dir,
        } => play(
            &platform,
            &iso,
            &runtime,
            &preset,
            !windowed,
            dry_run,
            user_dir.as_deref(),
        ),
        Command::Mods { what } => match what {
            ModsCommand::List { from, build } => mods_list(&platform, from, &build),
            ModsCommand::Import {
                from,
                build,
                version,
                apply,
            } => mods_import(&platform, from, &build, &version, apply),
            ModsCommand::Link {
                game,
                user_dir,
                apply,
            } => mods_link(&platform, &game, user_dir, apply),
            ModsCommand::Sync { game, user_dir } => mods_sync(&platform, &game, user_dir),
            ModsCommand::Apply {
                id,
                version,
                game,
                apply,
            } => mods_activation(
                &platform,
                &id,
                version.as_deref(),
                game.as_deref(),
                Turn::On,
                apply,
            ),
            ModsCommand::Unapply { id, game, apply } => {
                mods_activation(&platform, &id, None, game.as_deref(), Turn::Off, apply)
            }
            ModsCommand::Verify { game } => mods_verify(&platform, game.as_deref()),
        },
        Command::Verify {
            manifest,
            root,
            state,
        } => verify(&platform, &manifest, root.as_deref(), state.as_deref()),
        Command::Repair {
            manifest,
            root,
            state,
            apply,
        } => repair(
            &platform,
            &manifest,
            root.as_deref(),
            state.as_deref(),
            apply,
        ),
        Command::Drives => drives(&platform),
        Command::Onboarding { from } => onboarding(&platform, from),
        Command::Stats { what } => match what {
            StatsCommand::List => stats_list(&platform),
            StatsCommand::Ingest {
                from,
                session,
                apply,
            } => stats_ingest(&platform, from, session.as_deref(), apply),
            StatsCommand::Retry { apply } => stats_retry(&platform, apply),
            StatsCommand::Prune { keep_days, apply } => stats_prune(&platform, keep_days, apply),
        },
        Command::Pristine {
            from,
            build,
            to,
            apply,
        } => pristine(&platform, &from, &build, to, apply),
        Command::Inventory {
            root,
            component,
            version,
            sequence,
            install_root,
            out,
            release_out,
            pin_url,
            apply,
        } => inventory(
            &root,
            &component,
            &version,
            sequence,
            install_root.as_deref(),
            out.as_deref(),
            release_out.as_deref(),
            pin_url.as_deref(),
            apply,
        ),
        Command::Music => music(&platform),
        Command::Content { apply } => content(&platform, apply),
        Command::Publish { what } => publish::run(what),
        Command::GamePatch { what } => match what {
            GamePatchCommand::Apply {
                data_root,
                xdelta,
                builder,
                textures,
            } => game_patch_apply(
                &platform,
                data_root.as_deref(),
                xdelta.as_deref(),
                builder.as_deref(),
                textures.as_deref(),
            ),
        },
        Command::Roster { what } => match what {
            RosterCommand::Apply {
                roster,
                builder,
                data_root,
                textures,
                yes,
            } => roster_apply(
                &platform,
                roster.as_deref(),
                builder.as_deref(),
                data_root.as_deref(),
                textures.as_deref(),
                yes,
            ),
        },
    }
}

const OK: std::process::ExitCode = std::process::ExitCode::SUCCESS;
const PROBLEM: std::process::ExitCode = std::process::ExitCode::FAILURE;

/// The caller asked for something that cannot be parsed into a request at all.
///
/// Kept apart from [`PROBLEM`] so a script can tell "the flag is wrong" from
/// "your files are damaged". Both used to be worth a non-zero status and
/// nothing distinguished them.
///
/// A function rather than a constant because `ExitCode::from` is not const.
fn misuse() -> std::process::ExitCode {
    std::process::ExitCode::from(2)
}

/// Check for a newer launcher, and fetch it when asked.
fn update_check(
    platform: &Platform,
    source: Option<&str>,
    download: bool,
) -> reborn_core::Result<std::process::ExitCode> {
    let source = match source.map(str::trim).filter(|s| !s.is_empty()) {
        Some(s) => s.to_string(),
        None => match platform.paths.channel() {
            reborn_core::Channel::Beta => reborn_core::update::DEFAULT_BETA_SOURCE.to_string(),
            reborn_core::Channel::Stable => reborn_core::update::DEFAULT_SOURCE.to_string(),
        },
    };
    let current = env!("CARGO_PKG_VERSION");
    let http = reborn_core::net::UreqHttp::new(&format!("NFL Street Reborn Launcher/{current}"));
    println!("This build  {current}");
    println!("Source      {source}");
    let check = reborn_core::update::check(&http, &source, current);
    println!("{}", check.explain());
    let reborn_core::update::UpdateCheck::Available { asset, .. } = &check else {
        return Ok(match check {
            reborn_core::update::UpdateCheck::UpToDate { .. } => OK,
            _ => PROBLEM,
        });
    };
    if !download {
        println!("Run `rebornctl update download` to fetch it. Nothing is installed by this command.");
        return Ok(OK);
    }
    let dir = platform.data_dir(reborn_core::DataDir::Downloads)?;
    println!();
    println!("Downloading {} ({:.1} MB) into {}", asset.file_name, asset.bytes as f64 / 1_048_576.0, dir.display());
    let got = reborn_core::update::download(&http, asset, &dir)?;
    println!("Verified    sha256 {}", got.sha256.as_str());
    println!("Saved       {}", got.path.display());
    println!("Run that installer to update. It replaces the program files and leaves your data alone.");
    Ok(OK)
}

fn status(platform: &Platform) -> reborn_core::Result<std::process::ExitCode> {
    println!("Platform      {}", reborn_core::CURRENT_PLATFORM);
    // Named before the roots, because it is what decides them. A tester
    // pasting this output is telling us which launcher they mean.
    println!("Channel       {}", reborn_core::Channel::detect().explain());

    let install = platform.paths.install_root()?;
    let data = platform.paths.data_root()?;
    println!("Install root  {}", install.display());
    println!("Data root     {}", data.display());

    // The single most important property of the layout, checked on the real
    // machine rather than only in a test. The shipped 2.1 uninstaller deletes
    // the install root, and 2.1 keeps memory cards inside it.
    if data.starts_with(&install) {
        println!();
        println!("PROBLEM: the data root is inside the install root.");
        println!("Uninstalling would delete the player's memory cards.");
        return Ok(PROBLEM);
    }
    println!(
        "              (data root is outside the install root, so uninstall cannot eat saves)"
    );

    println!();
    println!("Data directories");
    for dir in DataDir::ALL {
        let path = data.join(dir.relative());
        let mark = if path.exists() { "present" } else { "-" };
        let class = match (dir.is_user_data(), dir.is_derived()) {
            (true, _) => "user data",
            (_, true) => "derived",
            _ => "",
        };
        println!("  {:<24} {:<8} {}", dir.relative(), mark, class);
    }

    println!();
    match Supervisor::acquire(platform)? {
        Startup::Owner(s) => {
            println!("Launcher      not running (this process took the lock)");
            if let Some(lost) = s.recover_unfinished()? {
                println!();
                println!("Recovered an unfinished session from {}.", lost.started_ms);
                println!("The launcher stopped while the game was running.");
            }
        }
        Startup::AlreadyRunning { lock_path } => {
            println!(
                "Launcher      RUNNING, lock held at {}",
                lock_path.display()
            );
        }
    }

    Ok(OK)
}

fn migrate(
    platform: &Platform,
    from: Option<PathBuf>,
    apply: bool,
) -> reborn_core::Result<std::process::ExitCode> {
    let (found, looked) = find_legacy(platform, from)?;

    let Some(install) = found else {
        println!("No 2.1 install found. Looked in:");
        for path in &looked {
            println!("  {}", path.display());
        }
        println!("Pass --from if it is somewhere else.");
        return Ok(OK);
    };

    println!("Found a 2.1 install at {}", install.root.display());
    println!("  memory cards  {}", yes_no(install.has_memory_cards));
    println!("  screenshots   {}", yes_no(install.has_screenshots));
    println!("  textures      {}", yes_no(install.has_textures));
    println!("  mods applied  {}", yes_no(install.has_applied_mods));
    println!("  mod backup    {}", yes_no(install.has_jsgme_backup));
    println!();

    let plan = MigrationPlan::build(&install);

    if let Some(reason) = &plan.blocked {
        println!("BLOCKED. Nothing will be copied.");
        println!();
        println!("{reason}");
        return Ok(PROBLEM);
    }

    if plan.steps.is_empty() {
        println!("Nothing to migrate.");
        return Ok(OK);
    }

    println!("Plan, in order:");
    for step in &plan.steps {
        let tag = if step.irreplaceable {
            "irreplaceable"
        } else {
            "replaceable"
        };
        println!("  {}. {} [{}]", step.order, step.what, tag);
        println!("     from {}", step.from.display());
        println!("     to   {}", step.to.relative());
        println!("     {}", step.why);
    }

    if !plan.skipped.is_empty() {
        println!();
        println!("Left behind:");
        for skip in &plan.skipped {
            println!("  {}: {}", skip.what, skip.why);
        }
    }

    if let Some(after) = plan.safe_after_step() {
        println!();
        println!("After step {after} finishes, an interruption costs nothing permanent.");
    }

    if plan.needs_mod_unapply {
        println!();
        println!("Mods are applied. They will be un-applied before textures are copied,");
        println!("or the pristine tree would be polluted with modded files.");
    }

    if !apply {
        println!();
        println!("Nothing was copied. Re-run with --apply to do it.");
        return Ok(OK);
    }

    let data = platform.paths.data_root()?;
    let mut total = 0u64;
    let mut refused = Vec::new();
    for step in &plan.steps {
        // The promise printed above has to be kept, and this tool cannot keep
        // it: nothing here un-applies a JSGME mod. jsgme.rs reads and never
        // writes, on purpose, because MODS/!BACKUP is the only copy of any
        // original those mods displaced.
        //
        // So the textures step is refused rather than run. Copying the live
        // texture tree while mods are applied would put modded files into
        // library/textures, which every later verify and compose treats as
        // pristine: a wrong answer that looks like a successful migration and
        // survives forever. The measured install has four mods applied.
        if step_is_refused(plan.needs_mod_unapply, step.to) {
            println!("{}. {} ... REFUSED", step.order, step.what);
            refused.push(step.what);
            continue;
        }

        let size = measure_tree(&step.from);
        println!(
            "{}. {} ... {} file(s), {} bytes",
            step.order,
            step.what,
            thousands(size.files),
            thousands(size.bytes)
        );
        // Flushed, because the copy below reports nothing while it runs and a
        // line held in the buffer makes a multi gigabyte step look like a hang.
        let _ = std::io::Write::flush(&mut std::io::stdout());

        let dest = data.join(step.to.relative());
        let copied = reborn_core::migration::copy_tree_no_clobber(&step.from, &dest)?;
        total += copied;
        println!("    copied {copied} file(s) into {}", dest.display());
    }
    println!();
    println!("Copied {total} file(s). Nothing in the 2.1 install was changed or removed.");

    if refused.is_empty() {
        return Ok(OK);
    }
    println!();
    println!("NOT DONE: {}.", refused.join(", "));
    println!();
    println!("Mods are applied, and nothing in this launcher can un-apply them yet:");
    println!("reborn-core reads the JSGME library and never writes to it, because");
    println!("MODS/!BACKUP is the only copy of whatever those mods displaced.");
    println!("Copying the texture tree now would file modded files as pristine, and");
    println!("every verify and every composed tree after that would trust them.");
    println!();
    println!("What to do: un-apply the mods in JSGME first, `rebornctl mods list`");
    println!("shows which are applied, then run this again. The steps above have");
    println!("already run and are not repeated: the copy never clobbers.");
    Ok(PROBLEM)
}

fn audit_runtime(path: &std::path::Path) -> reborn_core::Result<std::process::ExitCode> {
    if !path.is_dir() {
        eprintln!("not a directory: {}", path.display());
        return Ok(PROBLEM);
    }
    println!("Auditing {}", path.display());
    println!();

    let violations = RuntimePackager::audit(path)?;
    if violations.is_empty() {
        println!("SHIPPABLE. No forbidden files found.");
        return Ok(OK);
    }

    println!("NOT SHIPPABLE. {} problem(s):", violations.len());
    println!();
    for v in &violations {
        println!("  {}", v.path);
        println!("      {}", v.reason);
        println!();
    }
    Ok(PROBLEM)
}

fn list_devices(
    platform: &Platform,
    runtime: Option<&std::path::Path>,
) -> reborn_core::Result<std::process::ExitCode> {
    // Asking the runtime is the authoritative answer. The built in scan only
    // knows about XInput, and this machine shows why that is not enough: one
    // Xbox pad also reports through a WGInput backend the launcher has no way
    // to know about.
    let scan = resolve_devices(platform, runtime)?;

    let pads = scan.playable_pads();
    if pads.is_empty() {
        println!("No controllers found.");
    } else {
        println!("{} controller(s) to use, in port order:", pads.len());
        for (n, d) in pads.iter().enumerate() {
            println!(
                "  port {}  {:<34} {}",
                n + 1,
                d.device_string(),
                d.family.label()
            );
        }
    }

    let aside = scan.set_aside();
    if !aside.is_empty() {
        println!();
        println!("Also detected, not used:");
        for (d, why) in aside {
            println!("  {:<34} {}", d.device_string(), why);
        }
    }

    if !scan.is_complete() {
        println!();
        println!("Not everything was searched, so an empty result is inconclusive:");
        for (backend, reason) in &scan.not_searched {
            println!("  {}: {}", backend.source(), reason);
        }
        println!();
        println!("Pass --runtime <Dolphin.exe> for the authoritative list.");
    }
    Ok(OK)
}

fn controller(
    platform: &Platform,
    runtime: Option<&std::path::Path>,
    out: Option<PathBuf>,
    ports: u8,
) -> reborn_core::Result<std::process::ExitCode> {
    let scan = resolve_devices(platform, runtime)?;

    let Some((ini, assignment)) = build_gcpad(&scan, ports) else {
        println!("No controllers found, so there is nothing to write.");
        if !scan.is_complete() {
            println!();
            println!("Pass --runtime <Dolphin.exe> for the authoritative list.");
        }
        return Ok(PROBLEM);
    };

    for (port, device) in &assignment {
        println!("port {port} -> {device}");
    }
    println!();

    match out {
        Some(path) => {
            std::fs::write(&path, &ini).map_err(|source| reborn_core::Error::Io {
                path: path.clone(),
                source,
            })?;
            println!("Wrote {}", path.display());
        }
        None => print!("{ini}"),
    }
    Ok(OK)
}

/// Get the device list, preferring the runtime's own answer.
fn resolve_devices(
    platform: &Platform,
    runtime: Option<&std::path::Path>,
) -> reborn_core::Result<reborn_core::devices::Enumeration> {
    let scratch = platform.paths.data_root()?.join(DataDir::Cache.relative());
    Ok(match runtime {
        Some(exe) => match reborn_core::devices::scan_via_runtime(platform, exe, &scratch) {
            Ok(s) => s,
            Err(why) => {
                println!("Could not ask the runtime: {why}");
                println!("Falling back to the built in scan, which only sees XInput.");
                println!();
                DeviceScanner::scan()
            }
        },
        None => DeviceScanner::scan(),
    })
}
fn sessions(platform: &Platform) -> reborn_core::Result<std::process::ExitCode> {
    match Supervisor::acquire(platform)? {
        Startup::AlreadyRunning { .. } => {
            println!("A launcher is running. Session history is owned by that process.");
            Ok(OK)
        }
        Startup::Owner(s) => {
            if let Some(lost) = s.recover_unfinished()? {
                println!("Recovered an unfinished session (id {}).", lost.id);
                println!();
            }
            let history = s.history()?;
            if history.is_empty() {
                println!("No sessions recorded yet.");
                return Ok(OK);
            }
            println!("{} session(s), oldest first:", history.len());
            for r in &history {
                let dur = r
                    .duration_ms()
                    .map(|ms| format!("{:.1} min", ms as f64 / 60_000.0))
                    .unwrap_or_else(|| "unknown".to_string());
                println!(
                    "  {:<20} {:<8} {:<10} {:?}",
                    r.id,
                    format!("{:?}", r.build).to_lowercase(),
                    dur,
                    r.end.as_ref().map(|e| format!("{e:?}")).unwrap_or_default()
                );
            }
            Ok(OK)
        }
    }
}

/// Boot the game and stay with it until it exits.
///
/// This is Play Now with no UI in front of it. It does the same three things
/// the eventual front end will do, in the same order: take the single-instance
/// lock, write configuration the runtime will actually read, and hold the
/// session open until the game is gone.
fn play(
    platform: &Platform,
    iso: &std::path::Path,
    runtime: &std::path::Path,
    preset: &str,
    fullscreen: bool,
    dry_run: bool,
    user_dir: Option<&std::path::Path>,
) -> reborn_core::Result<std::process::ExitCode> {
    let graphics = match preset.to_lowercase().as_str() {
        "performance" => GraphicsPreset::Performance,
        "balanced" => GraphicsPreset::Balanced,
        "quality" => GraphicsPreset::Quality,
        "ultra" => GraphicsPreset::Ultra,
        other => {
            eprintln!("unknown preset {other:?}. Use performance, balanced, quality, or ultra.");
            return Ok(PROBLEM);
        }
    };

    let req = LaunchRequest {
        runtime_exe: runtime.to_path_buf(),
        game_path: iso.to_path_buf(),
        graphics,
        fullscreen,
        build: GameBuild::Offline,
        mods_disabled: false,
        user_dir: user_dir.map(|p| p.to_path_buf()),
        league_match: false,
    };

    // A dry run writes the configuration and starts nothing, which is the same
    // sequence up to the spawn. Sharing it is the point: a dry run that
    // exercised different code would prove nothing about a real launch.
    if dry_run {
        let written = Session::prepare(platform, &req)?;
        println!("{}", written.user_dir.explain());
        println!();
        for f in &written.files {
            println!("updated {}", f.display());
        }
        for f in &written.unchanged {
            println!("already correct {}", f.display());
        }
        println!("{} key(s) set", written.keys_set);

        // The button art, which is reconciled on every launch beside the
        // configuration. Printed even when nothing changed, because "the
        // profile is Xbox and already applied" is the answer a player wants
        // when the buttons look wrong.
        match &written.controls {
            Some(applied) => {
                println!();
                print!("Controls  {}", applied.profile.label());
                match &applied.turned_on {
                    Some(id) => print!(", from {}", id.as_str()),
                    // GameCube. Not a gap: the prompt textures are add-only, so
                    // having no pack uncovers what the disc itself drew.
                    None if applied.missing_package.is_none() => {
                        print!(", which applies no pack so the game draws its own art")
                    }
                    None => {}
                }
                if applied.already_correct {
                    print!(" (already applied)");
                }
                println!();
                for id in &applied.turned_off {
                    println!("  turned off {}", id.as_str());
                }
                if let Some(why) = &applied.missing_package {
                    println!("  {why}");
                }
            }
            None => {
                println!();
                println!("Controls  not chosen yet, so no button art is applied. First run");
                println!("          records it and the next launch applies it.");
            }
        }
        for (port, device) in &written.pads {
            println!("  port {port} -> {device}");
        }
        if written.no_controller {
            println!();
            println!("No controller detected.");
            println!();
            println!("If a controller is plugged in, it may have gone to sleep. Press its");
            println!("home button to wake it, then run this again.");
        }
        if !written.pads_authoritative {
            println!();
            println!("The controller list came from the built in scan, which only sees");
            println!("XInput, so the binding may be incomplete.");
        }
        println!();
        println!("Game     {}", iso.display());
        println!("Runtime  {}", runtime.display());
        println!(
            "Preset   {} ({}x internal)",
            graphics.label(),
            graphics.internal_resolution()
        );
        println!(
            "Display  {}",
            if fullscreen { "fullscreen" } else { "windowed" }
        );
        println!();
        println!("Dry run. Nothing was launched.");
        return Ok(OK);
    }

    println!("Starting. This process stays until the game exits.");

    match Session::run(platform, &req)? {
        Launched::Refused(r) => {
            eprintln!("{}", r.message());
            Ok(PROBLEM)
        }
        Launched::Finished {
            end,
            minutes,
            pads,
            pads_authoritative,
            user_dir,
            mods,
            stats,
            ..
        } => {
            println!("{}", user_dir.explain());
            println!("{}", mods.explain());
            if let Some(line) = stats.explain() {
                println!("{line}");
            }
            for (port, device) in &pads {
                println!("  port {port} -> {device}");
            }
            if !pads_authoritative {
                println!("(controller list from the built in scan, may be incomplete)");
            }
            println!();
            println!("Session ended after {minutes:.1} minutes.");

            match &end {
                SessionEnd::Exited { code: Some(0) } => {
                    println!("The game closed normally.");
                    Ok(OK)
                }
                SessionEnd::Exited { code } => {
                    println!("The game exited with status {}.", code.unwrap_or(-1));
                    println!("That is a failure, not a crash.");
                    Ok(PROBLEM)
                }
                other => {
                    println!("Something went wrong:");
                    match other.outcome() {
                        reborn_core::runtime::SessionOutcome::ShowRecovery { detail } => {
                            println!("  {detail}");
                        }
                        o => println!("  {o:?}"),
                    }
                    println!();
                    println!("Recorded. `rebornctl sessions` will show it.");
                    Ok(PROBLEM)
                }
            }
        }
    }
}

/// Whether a migration step must not run yet.
///
/// Only one step is ever refused, and only for one reason: the texture tree
/// cannot be copied into `library/textures` while JSGME mods are applied,
/// because the copy would file modded files as pristine and every later verify
/// would trust them.
///
/// A named function rather than a condition inside the loop so the rule can be
/// checked without a 2.1 install and a two gigabyte copy. The steps that carry
/// irreplaceable data, memory cards and screenshots, must never end up here.
fn step_is_refused(mods_are_applied: bool, to: DataDir) -> bool {
    mods_are_applied && to == DataDir::LibraryTextures
}

fn yes_no(b: bool) -> &'static str {
    if b {
        "yes"
    } else {
        "no"
    }
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Group a byte count so a person can read it, without changing its value.
///
/// Exact digits, never rounded to "2.1 GB". Numbers printed here are compared
/// against manifests and against free space, and this project's recorded
/// failure is a plausible looking figure: 2,077,362,716 and "2.1 GB" are not
/// the same claim, and only one of them can be checked.
fn thousands(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// How many items a list prints before it starts counting instead.
const LIST_CAP: usize = 20;

/// Print a list, and say how much of it was left out.
///
/// A truncated list that does not admit it is the same failure as an unsearched
/// backend that does not admit it: the reader takes the short version for the
/// whole answer.
fn print_list(indent: &str, items: &[String]) {
    for item in items.iter().take(LIST_CAP) {
        println!("{indent}{item}");
    }
    if items.len() > LIST_CAP {
        println!(
            "{indent}and {} more, not listed",
            thousands((items.len() - LIST_CAP) as u64)
        );
    }
}

/// What a cheap walk found. Used only to say how big a job is before it starts.
#[derive(Debug, Default, Clone, Copy)]
struct TreeSize {
    files: u64,
    bytes: u64,
    /// Entries that could not be read at all.
    ///
    /// Counted rather than ignored, because a total that quietly omits a locked
    /// directory understates the wait it exists to predict.
    unreadable: u64,
}

impl TreeSize {
    fn add(&mut self, other: TreeSize) {
        self.files += other.files;
        self.bytes += other.bytes;
        self.unreadable += other.unreadable;
    }
}

/// Count files and bytes without hashing anything.
///
/// A progress estimate and nothing else, so it never fails: a directory it
/// cannot read is counted and skipped. `symlink_metadata` is used and links are
/// never followed, so a link pointing back up the tree cannot make this loop
/// forever.
fn measure_tree(dir: &Path) -> TreeSize {
    let mut size = TreeSize::default();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(next) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&next) else {
            size.unreadable += 1;
            continue;
        };
        for entry in entries {
            let Ok(entry) = entry else {
                size.unreadable += 1;
                continue;
            };
            let path = entry.path();
            let Ok(meta) = std::fs::symlink_metadata(&path) else {
                size.unreadable += 1;
                continue;
            };
            if meta.is_dir() {
                stack.push(path);
            } else if meta.is_file() {
                size.files += 1;
                size.bytes += meta.len();
            } else {
                size.unreadable += 1;
            }
        }
    }
    size
}

/// Say how much work is about to happen, and that nothing will be printed while
/// it does.
///
/// `library::Verifier` and `jsgme::JsgmeLibrary::scan` both hash every file and
/// neither takes a progress observer, so this line is the only thing between a
/// real wait and a person deciding the tool has hung.
fn announce_silent_work(what: &str, files: u64, bytes: u64) {
    println!(
        "{what}: {} file(s), {} bytes. All of them are hashed, and nothing is \
         printed until that finishes.",
        thousands(files),
        thousands(bytes)
    );
}

/// Prints scan progress to standard error, at most once a second.
///
/// Standard error, so a listing on standard output stays something a script can
/// read. Throttled, because the core calls this once per file found, once per
/// megabyte read and once per file finished: roughly 55,000 times for the 27,025
/// file texture component, which is more redraws than any terminal wants.
///
/// It never cancels. A terminal already has a way to stop a program, and the
/// core deliberately yields no manifest from a cancelled scan, so there would be
/// nothing to do with the result.
struct ScanLog {
    last: std::cell::Cell<Instant>,
    started: Instant,
}

impl ScanLog {
    fn new() -> Self {
        let now = Instant::now();
        Self {
            last: std::cell::Cell::new(now),
            started: now,
        }
    }

    fn seconds(&self) -> f64 {
        self.started.elapsed().as_secs_f64()
    }
}

impl std::fmt::Debug for ScanLog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScanLog").finish()
    }
}

impl ScanObserver for ScanLog {
    fn progress(&self, progress: &ScanProgress) -> ScanControl {
        let now = Instant::now();
        if now.duration_since(self.last.get()) < Duration::from_secs(1) {
            return ScanControl::Continue;
        }
        self.last.set(now);
        match progress.phase {
            ScanPhase::Enumerating => eprintln!(
                "  walking, {} file(s) found so far",
                thousands(progress.files_found)
            ),
            ScanPhase::Hashing => {
                // Absent only while enumerating, which the other arm covers, so
                // this never prints a denominator nobody measured.
                let total = progress
                    .files_total()
                    .map(thousands)
                    .unwrap_or_else(|| "?".to_string());
                match progress.fraction() {
                    Some(f) => eprintln!(
                        "  hashing {} of {} file(s), {:.0}% of the bytes",
                        thousands(progress.files_hashed),
                        total,
                        f * 100.0
                    ),
                    None => eprintln!(
                        "  hashing {} of {} file(s), {} bytes so far",
                        thousands(progress.files_hashed),
                        total,
                        thousands(progress.bytes_hashed)
                    ),
                }
            }
        }
        ScanControl::Continue
    }
}

/// Where a 2.1 install might be, in the order they are tried.
///
/// The first version of this looked in exactly one place, beside the launcher's
/// own install root. On the measured machine that folder does not exist: the
/// shipped 2.1 build lives in the sibling of the launcher's data root, so
/// `migrate` answered "no 2.1 install found" on a machine holding 35,015 files
/// of one. That is the worst shape of wrong answer, because it reads as
/// reassurance.
///
/// Every candidate still goes through [`LegacyInstall::detect`], which requires
/// the shape of an install rather than the name, so a wrong guess reads as
/// absent instead of being acted on.
fn legacy_candidates(platform: &Platform) -> reborn_core::Result<Vec<PathBuf>> {
    let data = platform.paths.data_root()?;
    let mut out = Vec::new();

    // The sibling of the launcher's own data root. platform::imp keeps those two
    // names apart on purpose, and this is the other half of that decision: the
    // folder the launcher must never write into is the folder migration reads.
    if let Some(parent) = data.parent() {
        out.push(parent.join("NFL Street Reborn"));
    }
    // What the Inno Setup package calls itself, kept because it is the name the
    // installer prints and somebody may have installed it there.
    if let Some(parent) = platform.paths.install_root()?.parent() {
        out.push(parent.join("NFL Street Reborn Xbox Dev Mode 2.1"));
    }

    // A candidate that contains the data root, or sits inside it, would make
    // migration a copy of a tree onto itself. Dropped here rather than checked
    // again at every call site.
    out.retain(|c| !data.starts_with(c) && !c.starts_with(&data));
    Ok(out)
}

/// Find the 2.1 install, and report everywhere that was looked.
///
/// The candidate list comes back even on success, because "found at the second
/// place we looked" is worth printing: somebody with two copies needs to know
/// which one is being read.
fn find_legacy(
    platform: &Platform,
    from: Option<PathBuf>,
) -> reborn_core::Result<(Option<LegacyInstall>, Vec<PathBuf>)> {
    let candidates = match from {
        Some(p) => vec![p],
        None => legacy_candidates(platform)?,
    };
    for candidate in &candidates {
        if let Some(found) = LegacyInstall::detect(candidate) {
            return Ok((Some(found), candidates));
        }
    }
    Ok((None, candidates))
}

/// Build ids the launcher actually has, from the directory names under
/// `library/textures`.
///
/// Derived, never assumed. [`JsgmeLibrary::scan`] takes this list for a measured
/// reason: `Xbox Controls` ships a `User/Load/Textures/v2/` working folder, `v2`
/// passes [`GameId::parse`] on shape alone, and a scan that trusted the name
/// would publish a package claiming to target a build nothing can compose for.
///
/// The second list is names that are present and are not usable build ids, so a
/// build that will be ignored is a stated fact rather than a shorter list.
fn recognized_builds(textures_root: &Path) -> (Vec<GameId>, Vec<String>) {
    let mut ids = Vec::new();
    let mut unusable = Vec::new();
    let Ok(entries) = std::fs::read_dir(textures_root) else {
        return (ids, unusable);
    };
    for entry in entries {
        let Ok(entry) = entry else {
            unusable.push("an entry that could not be read".to_string());
            continue;
        };
        if !entry.path().is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        match GameId::parse(&name) {
            Ok(id) => ids.push(id),
            Err(_) => unusable.push(name),
        }
    }
    ids.sort();
    unusable.sort();
    (ids, unusable)
}

/// Which build a composition is for.
#[derive(Debug)]
enum WhichGame {
    /// Settled, with how it was settled.
    Chosen(GameId, String),
    /// Nothing here can pick one, and this says what to do about it.
    Unclear(String),
}

/// Pick the build to compose for, or refuse to.
///
/// Guessing composes the wrong tree, so there is exactly one case where this
/// answers without being told: one pristine tree on disk means there is only one
/// answer to give.
fn which_game(platform: &Platform, given: Option<&str>) -> reborn_core::Result<WhichGame> {
    if let Some(name) = given {
        return Ok(match GameId::parse(name) {
            Ok(id) => WhichGame::Chosen(id, "as given".to_string()),
            Err(e) => WhichGame::Unclear(e.to_string()),
        });
    }
    let textures = platform
        .paths
        .data_root()?
        .join(DataDir::LibraryTextures.relative());
    let (builds, _) = recognized_builds(&textures);
    Ok(match builds.len() {
        1 => WhichGame::Chosen(
            builds[0].clone(),
            format!("the only build under {}", textures.display()),
        ),
        0 => WhichGame::Unclear(format!(
            "There is no pristine texture tree under {}, so there is no build to \
             compose for. Install the textures component first, or pass --game \
             <id> to name one anyway.",
            textures.display()
        )),
        _ => WhichGame::Unclear(format!(
            "{} builds are installed ({}). Pass --game <id>: composing the wrong \
             one builds a tree the game never loads.",
            builds.len(),
            builds
                .iter()
                .map(GameId::as_str)
                .collect::<Vec<_>>()
                .join(", ")
        )),
    })
}

/// Every package under `mods/packages`, and every directory there that is not
/// one.
///
/// A package that will not load is reported and the rest are still returned. One
/// malformed manifest hiding the other ten packages would make the listing lie
/// about what is installed.
fn installed_packages(root: &Path) -> (Vec<ModPackage>, Vec<(PathBuf, String)>) {
    let mut packages = Vec::new();
    let mut broken = Vec::new();
    let Ok(ids) = std::fs::read_dir(root) else {
        return (packages, broken);
    };
    for id_entry in ids {
        let Ok(id_entry) = id_entry else {
            broken.push((root.to_path_buf(), "an entry could not be read".to_string()));
            continue;
        };
        let id_dir = id_entry.path();
        if !id_dir.is_dir() {
            continue;
        }
        let Ok(versions) = std::fs::read_dir(&id_dir) else {
            broken.push((id_dir, "the directory could not be read".to_string()));
            continue;
        };
        for version in versions.flatten() {
            let dir = version.path();
            if !dir.is_dir() {
                continue;
            }
            if !dir.join(reborn_core::mods::PACKAGE_MANIFEST).exists() {
                broken.push((
                    dir,
                    format!(
                        "no {}, so this is a half finished import rather than a \
                         package",
                        reborn_core::mods::PACKAGE_MANIFEST
                    ),
                ));
                continue;
            }
            match ModPackage::load(&dir) {
                Ok(p) => packages.push(p),
                Err(e) => broken.push((dir, e.to_string())),
            }
        }
    }
    packages.sort_by(|a, b| {
        (a.id.as_str(), a.version.as_str()).cmp(&(b.id.as_str(), b.version.as_str()))
    });
    (packages, broken)
}

/// The packages the activation list names, in its order, and the ones it names
/// that are not installed.
///
/// The second list matters and has nowhere else to live: a plan built without
/// them composes a tree that lacks a mod the user turned on, and
/// [`reborn_core::mods::CompositionPlan`] has no field for "you asked for
/// something that is not here".
fn active_packages(
    activation: &Activation,
    installed: &[ModPackage],
) -> (Vec<ModPackage>, Vec<String>) {
    let mut ordered = Vec::new();
    let mut missing = Vec::new();
    for entry in &activation.active {
        match installed
            .iter()
            .find(|p| p.id == entry.id && p.version == entry.version)
        {
            Some(p) => ordered.push(p.clone()),
            None => missing.push(format!("{} {}", entry.id, entry.version)),
        }
    }
    (ordered, missing)
}

/// What composing will cost, in words rather than as an enum name.
fn link_method_note(method: LinkMethod) -> &'static str {
    match method {
        LinkMethod::Hardlink => {
            "hardlink, so the composed tree costs directory entries and no data bytes"
        }
        LinkMethod::Copy => {
            "copy, because the two trees are on different volumes or the filesystem \
             has no hardlinks, so every byte below is written again"
        }
    }
}

// ---------------------------------------------------------------------------
// Mods
// ---------------------------------------------------------------------------

/// Which way an activation is being moved.
///
/// A boolean argument at the call site reads as `true` with nothing saying what
/// is true, and this one decides whether a mod ends up in the tree the game
/// loads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Turn {
    On,
    Off,
}

/// Turn `--build` values into build ids, or print why they are not.
///
/// Returns `None` when the caller should exit with [`MISUSE`], because a build
/// id becomes a directory name under two roots and a bad one would point
/// composition outside them.
fn parse_builds(given: &[String]) -> Option<Vec<GameId>> {
    let mut out = Vec::new();
    for name in given {
        match GameId::parse(name) {
            Ok(id) => out.push(id),
            Err(e) => {
                eprintln!("--build {name}: {e}");
                return None;
            }
        }
    }
    Some(out)
}

/// Read the JSGME library, printing everything about where it came from.
///
/// `None` means there is nothing to read and the reason has already been
/// printed. The recognized build list is printed whatever it holds, including
/// empty, because an empty one is the difference between "your mods import" and
/// "every file in them is for a build this machine does not have".
fn scan_jsgme(
    platform: &Platform,
    from: Option<PathBuf>,
    extra_builds: Vec<GameId>,
) -> reborn_core::Result<Option<JsgmeLibrary>> {
    let (found, looked) = find_legacy(platform, from)?;
    let Some(install) = found else {
        println!("No 2.1 install found. Looked in:");
        for path in &looked {
            println!("  {}", path.display());
        }
        println!("Pass --from if it is somewhere else.");
        return Ok(None);
    };

    let mods_root = install.root.join("MODS");
    println!("2.1 install   {}", install.root.display());
    println!("JSGME MODS    {}", mods_root.display());
    if !mods_root.is_dir() {
        println!();
        println!("There is no MODS folder there, so JSGME was never installed or it");
        println!("has been removed. Nothing to read.");
        return Ok(None);
    }

    let textures = platform
        .paths
        .data_root()?
        .join(DataDir::LibraryTextures.relative());
    let (mut builds, unusable) = recognized_builds(&textures);
    let derived = builds.len();
    for id in extra_builds {
        if !builds.contains(&id) {
            builds.push(id);
        }
    }
    builds.sort();

    println!();
    if builds.is_empty() {
        println!("Recognized builds: none.");
        println!(
            "  Nothing under {} names a build, so every texture in every mod will",
            textures.display()
        );
        println!("  be reported as being for a build this machine does not have, and");
        println!("  none of it can be imported. Pass --build GN7E69 to say you have it.");
    } else {
        println!(
            "Recognized builds: {} ({} from {}, {} from --build)",
            builds
                .iter()
                .map(GameId::as_str)
                .collect::<Vec<_>>()
                .join(", "),
            derived,
            textures.display(),
            builds.len() - derived
        );
    }
    if !unusable.is_empty() {
        println!("  Ignored under library/textures, not usable as build ids:");
        print_list("    ", &unusable);
    }

    // The size of the wait, before the wait. JsgmeLibrary::scan hashes every
    // file in every mod folder and takes no progress observer, so this count is
    // the only warning a person gets. The folders JSGME reserves for itself
    // start with `!` and the scan does not read them, so they are not counted.
    let mut size = TreeSize::default();
    let mut folders = 0u64;
    if let Ok(entries) = std::fs::read_dir(&mods_root) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if !path.is_dir() || name.starts_with('!') {
                continue;
            }
            folders += 1;
            size.add(measure_tree(&path));
        }
    }
    println!();
    println!("{folders} mod folder(s) to read.");
    announce_silent_work("Hashing", size.files, size.bytes);
    if size.unreadable > 0 {
        println!(
            "  {} entry(s) could not be read, so that total is a floor.",
            thousands(size.unreadable)
        );
    }

    let started = Instant::now();
    let library = JsgmeLibrary::scan(&mods_root, &builds)?;
    println!("Read in {:.1} s.", started.elapsed().as_secs_f64());
    Ok(Some(library))
}

/// Print what a JSGME library holds. Returns true when something needs acting on.
fn report_jsgme(library: &JsgmeLibrary) -> bool {
    let mut problem = false;

    println!();
    println!("{} mod(s):", library.mods.len());
    for m in &library.mods {
        let targets = m
            .targets()
            .iter()
            .map(GameId::as_str)
            .collect::<Vec<_>>()
            .join(", ");
        let applied = library
            .applied
            .applied
            .iter()
            .any(|a| a.name.eq_ignore_ascii_case(&m.name));
        println!(
            "  {:<22} {:<26} {} file(s), {} bytes{}{}",
            m.name,
            m.id.as_str(),
            m.files.len(),
            thousands(m.total_bytes()),
            if targets.is_empty() {
                String::new()
            } else {
                format!(", targets {targets}")
            },
            if applied { ", APPLIED" } else { "" }
        );
        if !m.skipped.is_empty() {
            // Named rather than counted. A mod that silently imports 19 of its
            // 39 files looks like it worked.
            let first = m
                .skipped
                .first()
                .map(|s| format!("{}: {}", s.mirror_relative, s.why.message()))
                .unwrap_or_default();
            println!(
                "      {} file(s) not importable. First: {first}",
                m.skipped.len()
            );
        }
        if !m.unsupported.is_empty() {
            println!(
                "      {} entry(s) that are neither a file nor a directory, left alone",
                m.unsupported.len()
            );
        }
    }

    println!();
    if library.applied.nothing_applied() {
        println!("Nothing is applied. That is what a clean 2.1 install looks like.");
    } else {
        println!(
            "{} mod(s) recorded as applied:",
            library.applied.applied.len()
        );
        for a in &library.applied.applied {
            println!(
                "  {:<22} {} recorded path(s), {} backup(s){}{}",
                a.name,
                a.recorded_paths.len(),
                a.backup_paths.len(),
                if a.folder_present {
                    ""
                } else {
                    ", FOLDER GONE so JSGME cannot un-apply it"
                },
                a.unreadable
                    .as_ref()
                    .map(|why| format!(", log unreadable: {why}"))
                    .unwrap_or_default()
            );
        }
    }
    println!(
        "  !BACKUP   {} ({} entry(s))",
        if library.applied.backup_exists {
            "present"
        } else {
            "missing"
        },
        library.applied.backup_entries.len()
    );
    println!(
        "  !INSTLOGS {}",
        if library.applied.instlogs_exists {
            "present"
        } else {
            "missing"
        }
    );

    if let Some(blocker) = library.applied.blocker() {
        problem = true;
        println!();
        println!("PROBLEM: {blocker}");
    }

    if !library.overlaps.is_empty() {
        println!();
        println!(
            "{} path(s) more than one mod writes:",
            library.overlaps.len()
        );
        let lines: Vec<String> = library
            .overlaps
            .iter()
            .map(|o| {
                format!(
                    "{}{}",
                    o.message(),
                    if o.differing_case {
                        " The mods spell it with different letter case, so this \
                         is one file on Windows and two on Linux."
                    } else {
                        ""
                    }
                )
            })
            .collect();
        print_list("  ", &lines);
    }

    if !library.duplicate_ids.is_empty() {
        problem = true;
        println!();
        println!("PROBLEM: folders that produce the same package id:");
        for (id, folders) in &library.duplicate_ids {
            println!("  {id}: {}", folders.join(", "));
        }
        println!("One id is one mod to the activation list, so none of them can be");
        println!("imported until all but one is renamed.");
    }

    if !library.unusable_names.is_empty() {
        problem = true;
        println!();
        println!("Folders whose name yields no package id:");
        for (name, why) in &library.unusable_names {
            println!("  {name}: {why}");
        }
    }

    if !library.ignored.is_empty() {
        println!();
        println!(
            "{} entry(s) at the top of MODS that are not mod folders:",
            library.ignored.len()
        );
        let lines: Vec<String> = library
            .ignored
            .iter()
            .map(|p| {
                p.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| p.display().to_string())
            })
            .collect();
        print_list("  ", &lines);
    }

    if let Some(why) = &library.order_unreadable {
        println!();
        println!("JSGME.ini is there and could not be read: {why}");
        println!("So which copy of a contested file is on disk cannot be told from it.");
    } else if !library.applied_order.is_empty() {
        println!();
        println!("Applied order from JSGME.ini, lowest first:");
        for (name, n) in &library.applied_order {
            println!("  {n:>3}  {name}");
        }
    }

    problem
}

/// Everything the mod system can see.
fn mods_list(
    platform: &Platform,
    from: Option<PathBuf>,
    extra_builds: &[String],
) -> reborn_core::Result<std::process::ExitCode> {
    let Some(extra) = parse_builds(extra_builds) else {
        return Ok(misuse());
    };
    let mut problem = false;

    let data = platform.paths.data_root()?;
    let pkg_root = packages_root(platform)?;
    let activation_file = activation_path(platform)?;
    let activation = Activation::load(&activation_file)?;
    let (packages, broken) = installed_packages(&pkg_root);

    println!("Packages     {}", pkg_root.display());
    println!("Activation   {}", activation_file.display());
    println!();

    if packages.is_empty() {
        println!("No packages are installed, so nothing can be turned on yet.");
        println!("`rebornctl mods import` says what importing the JSGME library would do.");
    } else {
        println!("{} package(s):", packages.len());
        for p in &packages {
            println!(
                "  {:<28} {:<8} {:<22} targets {:<16} {} file(s){}{}",
                p.id.as_str(),
                p.version.as_str(),
                p.name,
                p.targets
                    .iter()
                    .map(GameId::as_str)
                    .collect::<Vec<_>>()
                    .join(", "),
                p.files.len(),
                p.conflict_group
                    .as_ref()
                    .map(|g| format!(", group {g}"))
                    .unwrap_or_default(),
                if activation.is_active(&p.id) {
                    ", ON"
                } else {
                    ""
                }
            );
        }
    }

    if !broken.is_empty() {
        problem = true;
        println!();
        println!(
            "{} directory(s) under mods/packages that will not load:",
            broken.len()
        );
        for (path, why) in &broken {
            println!("  {}", path.display());
            println!("      {why}");
        }
    }

    let (_, missing) = active_packages(&activation, &packages);
    if !missing.is_empty() {
        problem = true;
        println!();
        println!("PROBLEM: the activation list names package(s) that are not installed:");
        print_list("  ", &missing);
        println!("A rebuild would leave them out of the tree without saying so.");
    }

    let textures = data.join(DataDir::LibraryTextures.relative());
    let (builds, _) = recognized_builds(&textures);
    println!();
    println!("Pristine trees under {}", textures.display());
    if builds.is_empty() {
        println!("  none, so there is nothing to compose over yet");
    }
    for build in &builds {
        let composed = data
            .join(DataDir::ModsActive.relative())
            .join(build.as_str());
        let state = if !composed.exists() {
            "not composed".to_string()
        } else if composed.join(COMPOSED_MARKER).exists() {
            format!(
                "composed, carries {COMPOSED_MARKER}. `rebornctl mods verify --game {}` checks it",
                build.as_str()
            )
        } else {
            format!(
                "a directory with no {COMPOSED_MARKER}, so the launcher did not \
                 create it and will not delete it"
            )
        };
        println!("  {:<10} {}", build.as_str(), state);
    }

    println!();
    if let Some(library) = scan_jsgme(platform, from, extra)? {
        if report_jsgme(&library) {
            problem = true;
        }
    }

    Ok(if problem { PROBLEM } else { OK })
}

/// Build an unmodded copy of a texture tree.
///
/// Dry by default. The plan is printed in full first, including every path being
/// reversed and why, because this is the tree every future composition is
/// layered over and a wrong one is invisible until a mod cannot be turned off.
fn pristine(
    platform: &Platform,
    from: &Path,
    build: &str,
    to: Option<PathBuf>,
    apply: bool,
) -> reborn_core::Result<std::process::ExitCode> {
    let build = match GameId::parse(build) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("--build: {e}");
            return Ok(misuse());
        }
    };

    let Some(install) = LegacyInstall::detect(from) else {
        println!("No NFL Street Reborn install at {}.", from.display());
        println!("This needs the folder holding User and MODS.");
        return Ok(PROBLEM);
    };

    let library = JsgmeLibrary::scan(&install.root.join("MODS"), std::slice::from_ref(&build))?;
    let destination = match to {
        Some(path) => path,
        None => platform
            .paths
            .data_root()?
            .join(DataDir::LibraryTextures.relative())
            .join(build.as_str()),
    };

    println!("Install    {}", from.display());
    println!("Build      {build}");
    println!("From       {}", install.root.display());
    println!("To         {}", destination.display());
    println!();

    let plan = library.pristine_plan(from, &build, &destination)?;

    if let Some(why) = &plan.blocked {
        println!("BLOCKED. Nothing was written.");
        println!();
        println!("{why}");
        return Ok(PROBLEM);
    }

    println!(
        "{} file(s), {} bytes to copy.",
        thousands(plan.files_to_copy as u64),
        thousands(plan.bytes_to_copy)
    );
    println!();

    if plan.reversing() == 0 {
        println!("Nothing is applied, so the tree is already unmodded and is copied as it is.");
    } else {
        println!(
            "{} path(s) to un-apply, from {} applied mod(s):",
            plan.reversing(),
            library.applied.applied.len()
        );
        println!();
        println!(
            "  {} restored from a backup, which is the original file JSGME displaced",
            plan.restore.len()
        );
        for (_, tail) in plan.restore.iter().take(4) {
            println!("      {tail}");
        }
        if plan.restore.len() > 4 {
            println!("      and {} more", plan.restore.len() - 4);
        }
        println!();
        println!(
            "  {} removed, because no backup exists. JSGME backs up only what it",
            plan.remove.len()
        );
        println!("      DISPLACES, so no backup means the mod added the file and it was");
        println!("      never part of the base tree.");
        for tail in plan.remove.iter().take(4) {
            println!("      {tail}");
        }
        if plan.remove.len() > 4 {
            println!("      and {} more", plan.remove.len() - 4);
        }
    }

    if !apply {
        println!();
        println!("Nothing was written. Re-run with --apply to do it.");
        return Ok(OK);
    }

    println!();
    println!("Copying, then un-applying. The install is only read.");
    let report = plan.execute()?;

    println!();
    println!(
        "{} file(s) copied, {} bytes. {} restored, {} removed.",
        thousands(report.files_copied as u64),
        thousands(report.bytes_copied),
        report.restored,
        report.removed
    );

    if !report.restored_absent.is_empty() {
        println!();
        println!(
            "{} original(s) were put back at a path the live tree no longer has,",
            report.restored_absent.len()
        );
        println!("so the pristine tree is not simply a copy of it. That happens when a");
        println!("mod-written file was later deleted or moved out of the live textures:");
        println!("the backup still holds the original, and the original belongs in a");
        println!("pristine tree whether or not the live tree kept it.");
    }

    if !report.is_exact() {
        println!();
        println!(
            "{} path(s) an install log names were not in the tree, so there was",
            report.missing.len()
        );
        println!("nothing to reverse for them. The log and the tree disagree, which is");
        println!("worth knowing even though nothing failed:");
        for tail in report.missing.iter().take(8) {
            println!("  {tail}");
        }
        if report.missing.len() > 8 {
            println!("  and {} more", report.missing.len() - 8);
        }
        return Ok(PROBLEM);
    }

    println!();
    println!("The copy is unmodded. Nothing in the install was changed.");
    Ok(OK)
}

/// Point the emulator's texture folder at the composed tree.
///
/// Dry by default, and the plan says exactly which folder would move where,
/// because this changes the shape of a live install.
fn mods_link(
    platform: &Platform,
    game: &str,
    user_dir: Option<PathBuf>,
    apply: bool,
) -> reborn_core::Result<std::process::ExitCode> {
    let game = match GameId::parse(game) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("--game: {e}");
            return Ok(misuse());
        }
    };

    // The folder a launch would actually use, so the link lands where the game
    // will look rather than where a fresh directory would be made.
    let user = reborn_core::session::resolve_user_dir(platform, user_dir.as_deref())?;
    let composed = reborn_core::mods::composed_root(platform, &game)?;
    let at = user
        .path()
        .join("Load")
        .join("Textures")
        .join(game.as_str());

    println!("Build      {game}");
    println!("User dir   {}", user.path().display());
    println!("  {}", user.explain());
    println!();
    println!("Link       {}", at.display());
    println!("        -> {}", composed.display());
    println!();

    if !composed.is_dir() {
        println!("BLOCKED. Nothing was changed.");
        println!();
        println!("There is no composed tree yet. Run `mods apply` first, or the game");
        println!("would be pointed at nothing and look like a broken install.");
        return Ok(PROBLEM);
    }

    if at.exists() {
        let existing = std::fs::symlink_metadata(&at)
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false);
        if existing {
            println!("It is already a link. Re-linking would only repoint it.");
        } else {
            let (files, _) = match std::fs::read_dir(&at) {
                Ok(entries) => (entries.count(), 0u64),
                Err(_) => (0, 0),
            };
            println!(
                "That folder holds {files} entry(s) and would be RENAMED, not deleted."
            );
            println!("Nothing here removes a texture, so getting this wrong costs a rename.");
        }
        println!();
    }

    if !apply {
        println!("Nothing was changed. Re-run with --apply to do it.");
        return Ok(OK);
    }

    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    match reborn_core::mods::link_textures(user.path(), &game, &composed, now_ms)? {
        reborn_core::mods::TextureLink::LeftAlone { at } => {
            println!("Left alone: {}", at.display());
            println!();
            println!("Nothing was linked. This command only reports it; a launch reads the");
            println!("setting and would leave the folder alone too.");
        }
        reborn_core::mods::TextureLink::AlreadyLinked { .. } => {
            println!("Already pointing at the composed tree. Nothing was changed.");
        }
        reborn_core::mods::TextureLink::Linked { moved_aside, .. } => {
            println!("Linked.");
            if let Some(aside) = moved_aside {
                println!();
                println!("The folder that was there is now at:");
                println!("  {}", aside.display());
                println!();
                println!("Nothing was deleted. Rename it back to undo this.");
            }
        }
    }

    Ok(OK)
}

/// Make the game load the active set. See `reborn_core::mods::sync_active`.
fn mods_sync(
    platform: &Platform,
    game: &str,
    user_dir: Option<PathBuf>,
) -> reborn_core::Result<std::process::ExitCode> {
    let game = match GameId::parse(game) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("--game: {e}");
            return Ok(misuse());
        }
    };
    let Startup::Owner(lock) = Supervisor::acquire(platform)? else {
        eprintln!("Another launcher is running and owns this install.");
        return Ok(PROBLEM);
    };
    let user = reborn_core::session::resolve_user_dir(platform, user_dir.as_deref())?;
    let req = reborn_core::mods::SyncRequest::in_data_root(platform, game, user.path())?;
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    println!("Build      {}", req.game);
    println!("User dir   {}", user.path().display());
    println!("Composed   {}", req.composed_root.display());
    println!();

    match reborn_core::mods::sync_active(platform, &lock, &req, now_ms)? {
        reborn_core::mods::SyncOutcome::Refused(why) => {
            println!("REFUSED. Nothing was changed.");
            println!("{why}");
            Ok(PROBLEM)
        }
        reborn_core::mods::SyncOutcome::Done(report) => {
            println!("{}", report.explain());
            for entry in &report.active {
                let state = if report.missing.contains(entry) {
                    "listed, NOT installed"
                } else {
                    "on"
                };
                println!("  {}@{}  {state}", entry.id, entry.version);
            }
            if let reborn_core::mods::TextureLink::Linked {
                moved_aside: Some(aside),
                ..
            } = &report.link
            {
                println!();
                println!("The folder that was there is now at {}.", aside.display());
                println!("Nothing was deleted. Rename it back to undo the link.");
            }
            Ok(OK)
        }
    }
}

/// What importing the JSGME library would do.
fn mods_import(
    platform: &Platform,
    from: Option<PathBuf>,
    extra_builds: &[String],
    version: &str,
    apply: bool,
) -> reborn_core::Result<std::process::ExitCode> {
    let Some(extra) = parse_builds(extra_builds) else {
        return Ok(misuse());
    };
    let version = match PackageVersion::parse(version) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("--version: {e}");
            return Ok(misuse());
        }
    };

    let Some(library) = scan_jsgme(platform, from, extra)? else {
        return Ok(PROBLEM);
    };
    let pkg_root = packages_root(platform)?;

    println!();
    println!("Importing into {}", pkg_root.display());
    println!(
        "Every package would be stamped version {}. JSGME records no version \
         anywhere, so that is a choice and not a reading.",
        version
    );

    let conversion = library.conversion_plan(&pkg_root, &version)?;
    let plan = match &conversion {
        Conversion::Refused(refusal) => {
            println!();
            println!("BLOCKED. Nothing was written.");
            println!();
            println!("{}", refusal.message());
            return Ok(PROBLEM);
        }
        Conversion::Planned(plan) => plan,
    };

    println!();
    println!(
        "{} package(s), {} file(s), {} bytes to copy:",
        plan.packages.len(),
        thousands(plan.file_count() as u64),
        thousands(plan.total_bytes())
    );
    for planned in &plan.packages {
        println!(
            "  {:<28} {:<22} {} file(s), {} bytes",
            planned.package.id.as_str(),
            planned.package.name,
            planned.files.len(),
            thousands(planned.total_bytes())
        );
        println!("      manifest {}", planned.manifest_path.display());
    }

    if !plan.skipped.is_empty() {
        println!();
        println!("{} mod(s) would not convert:", plan.skipped.len());
        for (name, why) in &plan.skipped {
            println!("  {name}: {why}");
        }
    }

    let occupied = plan.occupied();
    if !occupied.is_empty() {
        println!();
        println!(
            "{} destination(s) already hold a file. Overwriting would replace an \
             already imported package and skipping would leave a payload that \
             does not match its manifest, so this is reported and not resolved:",
            occupied.len()
        );
        let lines: Vec<String> = occupied
            .iter()
            .map(|c| c.to.display().to_string())
            .collect();
        print_list("  ", &lines);
    }

    if !apply {
        println!();
        println!("Nothing was written. Re-run with --apply to do it.");
        return Ok(OK);
    }

    // Occupied::Refuse rather than Replace. A destination that already holds
    // a file usually means the import has already been run, and replacing a
    // package the player may have composed from is not a decision this
    // command should take on a bare --apply. The plan printed the occupied
    // paths above, so anyone who means it can clear them.
    println!();
    println!("Importing.");
    let report = plan.execute(Occupied::Refuse)?;

    println!();
    println!(
        "{} package(s) imported, {} file(s), {} bytes.",
        report.imported.len(),
        thousands(report.files_copied as u64),
        thousands(report.bytes_copied)
    );

    if !report.failed.is_empty() {
        println!();
        println!("{} package(s) did NOT import:", report.failed.len());
        for (id, why) in &report.failed {
            println!("  {}: {why}", id.as_str());
        }
        println!();
        println!("Each of those left nothing behind. The rest are complete.");
        return Ok(PROBLEM);
    }

    println!();
    println!("Every package is on disk with its manifest, and MODS was not touched.");
    Ok(OK)
}

/// Print a composition plan.
fn report_plan(plan: &reborn_core::mods::CompositionPlan, missing: &[String]) {
    println!();
    println!("Build         {}", plan.game.as_str());
    println!("Pristine      {}", plan.pristine_root.display());
    println!("Composed      {}", plan.composed_root.display());
    println!("Method        {}", link_method_note(plan.method));
    println!(
        "Files         {} total, {} from packages, {} pristine, {} bytes",
        thousands(plan.entries.len() as u64),
        thousands(plan.overlay_count() as u64),
        thousands(plan.pristine_count() as u64),
        thousands(plan.total_bytes())
    );

    println!();
    if plan.activation.is_empty() {
        println!("No packages are active, so the composed tree is the pristine tree.");
    } else {
        println!("Active, in order:");
        for entry in &plan.activation {
            println!("  {} {}", entry.id.as_str(), entry.version.as_str());
        }
    }

    if !plan.not_targeted.is_empty() {
        println!();
        println!("On, and with no payload for this build, so they change nothing here:");
        for id in &plan.not_targeted {
            println!("  {}", id.as_str());
        }
    }

    if !plan.soft_conflicts.is_empty() {
        println!();
        println!("Declared conflicts. These do not stop composition:");
        for conflict in &plan.soft_conflicts {
            println!("  {}", conflict.message());
        }
    }

    if !missing.is_empty() {
        println!();
        println!("PROBLEM: active package(s) that are not installed, so the tree");
        println!("would not contain them:");
        print_list("  ", missing);
    }
}

/// Turn one package on or off, and rebuild the composed tree.
///
/// The order under `--apply` is: record the selection, then build the tree. That
/// way round on purpose. The activation list plus the pristine tree is the whole
/// state of the mod system and the composed tree is derived from it, so an
/// interruption between the two leaves the intent recorded and the tree stale,
/// and the next rebuild converges on what the user asked for. Composing first
/// would leave a tree holding a mod the record says is off, and the next rebuild
/// would take it out again, undoing an apply that reported success.
fn mods_activation(
    platform: &Platform,
    id: &str,
    version: Option<&str>,
    game: Option<&str>,
    turn: Turn,
    apply: bool,
) -> reborn_core::Result<std::process::ExitCode> {
    let mod_id = match ModId::parse(id) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("--id: {e}");
            return Ok(misuse());
        }
    };
    let game_id = match which_game(platform, game)? {
        WhichGame::Chosen(id, why) => {
            println!("Build         {} ({why})", id.as_str());
            id
        }
        WhichGame::Unclear(why) => {
            println!("{why}");
            return Ok(PROBLEM);
        }
    };

    let pkg_root = packages_root(platform)?;
    let (installed, broken) = installed_packages(&pkg_root);
    if !broken.is_empty() {
        println!();
        println!(
            "{} directory(s) under mods/packages will not load:",
            broken.len()
        );
        for (path, why) in &broken {
            println!("  {}: {why}", path.display());
        }
    }

    let activation_file = activation_path(platform)?;
    let mut activation = Activation::load(&activation_file)?;

    match turn {
        Turn::On => {
            let matching: Vec<&ModPackage> = installed.iter().filter(|p| p.id == mod_id).collect();
            let chosen = match (matching.len(), version) {
                (0, _) => {
                    println!();
                    println!("No package with the id {mod_id} is installed under");
                    println!("{}.", pkg_root.display());
                    println!("`rebornctl mods list` shows what is there.");
                    return Ok(PROBLEM);
                }
                (_, Some(wanted)) => {
                    let wanted = match PackageVersion::parse(wanted) {
                        Ok(v) => v,
                        Err(e) => {
                            eprintln!("--version: {e}");
                            return Ok(misuse());
                        }
                    };
                    match matching.iter().find(|p| p.version == wanted) {
                        Some(p) => *p,
                        None => {
                            println!();
                            println!("{mod_id} has no version {wanted} installed. Present:");
                            for p in &matching {
                                println!("  {}", p.version.as_str());
                            }
                            return Ok(PROBLEM);
                        }
                    }
                }
                (1, None) => matching[0],
                (_, None) => {
                    println!();
                    println!("{} versions of {mod_id} are installed:", matching.len());
                    for p in &matching {
                        println!("  {}", p.version.as_str());
                    }
                    println!("Pass --version: picking one here would decide which payload runs.");
                    return Ok(PROBLEM);
                }
            };

            if !chosen.targets_game(&game_id) {
                println!();
                println!(
                    "{mod_id} has no payload for {}. It can still be turned on, and \
                     it will change nothing in this tree.",
                    game_id.as_str()
                );
            }
            activation.activate(mod_id.clone(), chosen.version.clone());
            println!();
            println!("Turning on  {mod_id} {}", chosen.version.as_str());
        }
        Turn::Off => {
            if !activation.deactivate(&mod_id) {
                println!();
                println!("{mod_id} is not in the activation list, so there is nothing to");
                println!("turn off. The composed tree is unchanged.");
                return Ok(OK);
            }
            println!();
            println!("Turning off {mod_id}");
            println!(
                "Un-applying is a rebuild without it, not a restore. Nothing was \
                 displaced, so there is nothing to put back."
            );
        }
    }

    let (ordered, missing) = active_packages(&activation, &installed);
    for conflict in detect_conflicts(&ordered, &game_id) {
        if conflict.is_hard() {
            println!();
            println!("{}", conflict.message());
        }
    }

    let request = ComposeRequest::in_data_root(platform, game_id, ordered)?;
    let composition = Composer::plan(platform, &request)?;
    let plan = match &composition {
        Composition::Refused(refusal) => {
            println!();
            println!("BLOCKED. Nothing was written.");
            println!();
            println!("{}", refusal.message());
            return Ok(PROBLEM);
        }
        Composition::Planned(plan) => plan,
    };
    report_plan(plan, &missing);

    if !apply {
        println!();
        println!("Nothing was written. Re-run with --apply to do it.");
        return Ok(if missing.is_empty() { OK } else { PROBLEM });
    }

    let Startup::Owner(supervisor) = Supervisor::acquire(platform)? else {
        println!();
        println!("Another launcher is running, and it owns the composed tree.");
        println!("Close it first: two processes composing one tree interleave writes");
        println!("into the directory the game is about to load.");
        return Ok(PROBLEM);
    };

    activation.save(&activation_file)?;
    println!();
    println!("Recorded the selection in {}", activation_file.display());

    let report = Composer::apply(platform, &supervisor, plan)?;
    println!(
        "Composed {} entry(s): {} linked, {} copied, {} bytes written, {} from packages.",
        thousands(report.entries()),
        thousands(report.linked),
        thousands(report.copied),
        thousands(report.bytes_copied),
        thousands(report.overlaid)
    );
    if let Some(failure) = &report.link_failure {
        println!();
        println!("Hardlinks are not available here, so the tree was copied instead.");
        println!("The first failure was: {failure}");
    }

    // Checked immediately, because "it reported success" and "the tree on disk
    // is what the plan said" are different claims and only the second one is
    // worth anything to whoever plays next.
    let verification = Composer::verify(plan)?;
    if verification.matches_the_plan() {
        println!();
        println!("Verified. The tree is what the plan said.");
        return Ok(if missing.is_empty() { OK } else { PROBLEM });
    }
    println!();
    println!("PROBLEM: the tree does not match the plan that just built it:");
    println!("{}", verification.report());
    Ok(PROBLEM)
}

/// Check the composed tree against the activation list.
fn mods_verify(
    platform: &Platform,
    game: Option<&str>,
) -> reborn_core::Result<std::process::ExitCode> {
    let game_id = match which_game(platform, game)? {
        WhichGame::Chosen(id, why) => {
            println!("Build         {} ({why})", id.as_str());
            id
        }
        WhichGame::Unclear(why) => {
            println!("{why}");
            return Ok(PROBLEM);
        }
    };

    let pkg_root = packages_root(platform)?;
    let (installed, broken) = installed_packages(&pkg_root);
    let activation = Activation::load(&activation_path(platform)?)?;
    let (ordered, missing) = active_packages(&activation, &installed);

    if !broken.is_empty() {
        println!();
        println!(
            "{} directory(s) under mods/packages will not load:",
            broken.len()
        );
        for (path, why) in &broken {
            println!("  {}: {why}", path.display());
        }
    }

    let request = ComposeRequest::in_data_root(platform, game_id, ordered)?;
    let composition = Composer::plan(platform, &request)?;
    let plan = match &composition {
        Composition::Refused(refusal) => {
            println!();
            println!("Cannot even plan the tree, so there is nothing to compare against:");
            println!("{}", refusal.message());
            return Ok(PROBLEM);
        }
        Composition::Planned(plan) => plan,
    };
    report_plan(plan, &missing);

    println!();
    println!(
        "Checking {}. Every file the packages contribute is hashed; the pristine \
         side is checked by count, which is what catches one that went missing.",
        plan.composed_root.display()
    );
    let verification = Composer::verify(plan)?;
    if verification.matches_the_plan() {
        println!();
        println!("The composed tree matches the activation list.");
        return Ok(if missing.is_empty() { OK } else { PROBLEM });
    }

    println!();
    println!("{} discrepancy(s):", verification.discrepancies.len());
    println!("{}", verification.report());
    println!();
    println!("Every one of these has the same answer, a rebuild:");
    println!("  rebornctl mods apply --id <one that is on> --apply");
    println!("Rebuilding costs directory entries and loses nothing, because the");
    println!("launcher never displaced anything to build this tree.");
    Ok(PROBLEM)
}

// ---------------------------------------------------------------------------
// Verify and repair
// ---------------------------------------------------------------------------

/// A manifest, the tree it describes, and what verifying found.
#[derive(Debug)]
struct Checked {
    component: Component,
    root: PathBuf,
    report: VerifyReport,
}

/// Verify a component, printing where every input came from.
///
/// `None` means the manifest could not be used and the reason is already
/// printed. Every path is stated rather than implied, because "verify said the
/// runtime is fine" is worthless without knowing which folder it looked at.
fn check_component(
    platform: &Platform,
    manifest_path: &Path,
    root: Option<&Path>,
    state: Option<&Path>,
) -> reborn_core::Result<Option<Checked>> {
    let raw = match std::fs::read(manifest_path) {
        Ok(bytes) => bytes,
        Err(e) => {
            println!("Cannot read {}: {e}", manifest_path.display());
            return Ok(None);
        }
    };
    let component = match Component::parse(&raw) {
        Ok(c) => c,
        Err(e) => {
            println!(
                "{} is not an nsr.component.v1 manifest: {e}",
                manifest_path.display()
            );
            println!("`rebornctl inventory` produces one from a tree on disk.");
            return Ok(None);
        }
    };

    // Checked before anything is resolved against it. A manifest path of
    // `../../saves` would otherwise be hashed out of somebody's memory cards and
    // reported as a component file.
    let offenders = component.traversal_offenders();
    if !offenders.is_empty() {
        println!(
            "REFUSED. {} path(s) in that manifest escape the install root, \
             starting with {:?}.",
            offenders.len(),
            offenders.first().copied().unwrap_or_default()
        );
        println!("Nothing was read. That manifest is not one to verify against.");
        return Ok(None);
    }

    let data = platform.paths.data_root()?;
    let tree = match root {
        Some(given) => given.to_path_buf(),
        None => match ContentLayout::new(&data).live_root(&component) {
            Ok(p) => p,
            Err(e) => {
                println!("Cannot resolve where that component lives: {e}");
                return Ok(None);
            }
        },
    };

    // library::RebuildState documents itself as music/state.json, and the path is
    // derived from the same DataDir the music module derives it from rather than
    // spelled again here.
    let state_path = match state {
        Some(p) => p.to_path_buf(),
        None => reborn_core::music::music_root(&data).join("state.json"),
    };
    let rebuild = match RebuildState::load(&state_path) {
        Ok(s) => s,
        Err(e) => {
            println!("Cannot read {}: {e}", state_path.display());
            println!();
            println!("That file records what the launcher's own rebuilds produced, and");
            println!("without it a rebuilt soundtrack cannot be told from a corrupted");
            println!("download. Fix or move it aside, do not verify past it: a repair");
            println!("run now would treat the user's own music as damage.");
            return Ok(None);
        }
    };

    println!("Manifest      {}", manifest_path.display());
    println!(
        "Component     {} version {}, sequence {}",
        component.component, component.version, component.sequence
    );
    println!(
        "Customizable  {}. {}",
        yes_no(component.customizable),
        if component.customizable {
            "A file the user rebuilt is not damage."
        } else {
            "Every mismatch here is damage."
        }
    );
    println!("Tree          {}", tree.display());
    println!(
        "Rebuild log   {} ({} recorded)",
        state_path.display(),
        rebuild.rebuilt.len()
    );

    let service = ContentService::new(platform, &data);
    match service.installed(&component.component) {
        Ok(Some(pointer)) => {
            println!(
                "Installed     {} (sequence {}){}",
                pointer.current.version,
                pointer.current.sequence,
                pointer
                    .previous
                    .as_ref()
                    .map(|p| format!(", rollback to {}", p.version))
                    .unwrap_or_else(|| ", nothing to roll back to".to_string())
            );
            if pointer.current.version != component.version {
                println!(
                    "              The manifest describes {}, so this is a check \
                     against a version that is not the one recorded as installed.",
                    component.version
                );
            }
        }
        Ok(None) => println!(
            "Installed     nothing is recorded as installed for {}",
            component.component
        ),
        // Printed rather than fatal: the verify below reads the tree itself and
        // does not need the pointer to be right.
        Err(e) => println!("Installed     the record could not be read: {e}"),
    }

    if !tree.exists() {
        println!();
        println!("That tree does not exist, so every file will read as missing.");
    }

    println!();
    announce_silent_work(
        "Verifying",
        component.files.len() as u64,
        component.total_bytes(),
    );
    let started = Instant::now();
    let report = Verifier::new(&tree, &rebuild).verify(&component)?;
    println!("Checked in {:.1} s.", started.elapsed().as_secs_f64());

    Ok(Some(Checked {
        component,
        root: tree,
        report,
    }))
}

/// Print the verdicts. Returns whether anything needs repairing.
fn report_verdicts(checked: &Checked) -> bool {
    let mut intact = 0u64;
    let mut missing = Vec::new();
    let mut damaged = Vec::new();
    let mut modified = Vec::new();

    for (path, verdict) in &checked.report.verdicts {
        match verdict {
            FileVerdict::Intact => intact += 1,
            FileVerdict::Missing => missing.push(path.clone()),
            FileVerdict::Damaged { expected, actual } => damaged.push(format!(
                "{path}\n      expected {expected}\n      found    {actual}"
            )),
            FileVerdict::UserModified => modified.push(path.clone()),
        }
    }

    println!();
    println!("Intact        {}", thousands(intact));
    println!("Missing       {}", thousands(missing.len() as u64));
    println!("Damaged       {}", thousands(damaged.len() as u64));
    println!(
        "Yours         {} (changed, recorded as a rebuild, left alone)",
        thousands(modified.len() as u64)
    );

    if !modified.is_empty() {
        println!();
        println!("Files the user changed and the launcher recorded. A repair never");
        println!("touches these, which is what keeps a custom soundtrack from being");
        println!("downloaded over:");
        print_list("  ", &modified);
    }
    if !missing.is_empty() {
        println!();
        println!("Missing:");
        print_list("  ", &missing);
    }
    if !damaged.is_empty() {
        println!();
        println!("Damaged:");
        print_list("  ", &damaged);
    }

    let needed = !checked.report.is_healthy();
    if !needed {
        println!();
        println!(
            "HEALTHY. {} is what the manifest describes.",
            checked.root.display()
        );
    }
    needed
}

fn verify(
    platform: &Platform,
    manifest: &Path,
    root: Option<&Path>,
    state: Option<&Path>,
) -> reborn_core::Result<std::process::ExitCode> {
    let Some(checked) = check_component(platform, manifest, root, state)? else {
        return Ok(PROBLEM);
    };
    if report_verdicts(&checked) {
        println!();
        println!(
            "{} file(s) would be repaired, {} bytes. `rebornctl repair` lists them.",
            thousands(checked.report.to_repair().len() as u64),
            thousands(checked.report.repair_bytes(&checked.component))
        );
        return Ok(PROBLEM);
    }
    Ok(OK)
}

fn repair(
    platform: &Platform,
    manifest: &Path,
    root: Option<&Path>,
    state: Option<&Path>,
    apply: bool,
) -> reborn_core::Result<std::process::ExitCode> {
    let Some(checked) = check_component(platform, manifest, root, state)? else {
        return Ok(PROBLEM);
    };
    if !report_verdicts(&checked) {
        println!();
        println!("Nothing to repair.");
        return Ok(OK);
    }

    let to_repair = checked.report.to_repair();
    let bytes = checked.report.repair_bytes(&checked.component);
    println!();
    println!(
        "Repair set: {} file(s), {} bytes to fetch.",
        thousands(to_repair.len() as u64),
        thousands(bytes)
    );
    for path in to_repair.iter().take(LIST_CAP) {
        let reason = match checked.report.verdicts.get(*path) {
            Some(FileVerdict::Missing) => "absent",
            Some(FileVerdict::Damaged { .. }) => "on disk and explained by nothing",
            _ => "in the set",
        };
        let size = checked
            .component
            .files
            .iter()
            .find(|f| f.path == *path)
            .map(|f| f.bytes)
            .unwrap_or(0);
        println!("  {path}  {} bytes, {reason}", thousands(size));
    }
    if to_repair.len() > LIST_CAP {
        println!(
            "  and {} more, not listed",
            thousands((to_repair.len() - LIST_CAP) as u64)
        );
    }

    println!();
    println!("CANNOT FETCH. Nothing on disk was changed.");
    println!();
    println!("A repair is an update whose fetch set happens to be the damaged files,");
    println!("so it needs the same two things an update needs, and this build has");
    println!("neither:");
    println!();
    println!("  1. A signature verifier. content::TrustedRelease is the only way into");
    println!("     the update path and it cannot be opened without a signature that");
    println!("     verified over the served bytes. reborn-core ships no verifier on");
    println!("     purpose: one that returned true would make every call site look");
    println!("     checked while nothing was checked.");
    println!("  2. Somewhere to fetch from. content::BlobSource is a trait the caller");
    println!("     implements, and nothing implements it yet.");
    println!();
    println!("Until both exist, the repairable copy is whatever the user has: the");
    println!("2.1 installer image, or a backup. The list above is exactly what to");
    println!("put back, and `rebornctl verify` will say when it is right.");
    if apply {
        println!();
        println!("--apply changed nothing, because there is nothing to apply with.");
    }
    Ok(PROBLEM)
}

// ---------------------------------------------------------------------------
// Drives
// ---------------------------------------------------------------------------

/// Volumes, and which of them the Xbox wizard may write to.
///
/// Both halves are printed. "No drives found" and "one drive found and it is
/// your system disk" are different facts, and a picker that shows neither leaves
/// somebody believing their stick is unplugged.
fn drives(platform: &Platform) -> reborn_core::Result<std::process::ExitCode> {
    let volumes = match platform.drives.enumerate_volumes() {
        Ok(v) => v,
        Err(e) => {
            println!("Cannot enumerate volumes: {e}");
            println!();
            println!("So an empty list here would be a claim, not a measurement. The");
            println!("Xbox wizard is unavailable until this seam answers.");
            return Ok(PROBLEM);
        }
    };

    println!(
        "{} volume(s) the operating system will admit to:",
        volumes.len()
    );
    for volume in &volumes {
        let drive = &volume.drive;
        println!(
            "  {:<12} {:<10} {:<10} {:>18} total {:>18} free  {}",
            drive.mount_point.display().to_string(),
            match volume.kind {
                VolumeKind::Removable => "removable",
                VolumeKind::Fixed => "fixed",
            },
            drive.filesystem.clone().unwrap_or_else(|| "?".to_string()),
            thousands(drive.total_bytes),
            thousands(drive.free_bytes),
            drive
                .label
                .clone()
                .unwrap_or_else(|| "no label".to_string())
        );
    }
    if volumes.iter().all(|v| v.kind == VolumeKind::Fixed) && !volumes.is_empty() {
        println!();
        println!("Every one of those is a fixed disk. A USB hard disk that Windows");
        println!("calls fixed will not be offered, and that is deliberate: the other");
        println!("direction is a wizard writing about 3 GB into the middle of C:.");
    }

    println!();
    match reborn_core::xbox::survey(platform) {
        Err(e) => {
            println!("The Xbox wizard could not survey the drives: {e}");
            return Ok(PROBLEM);
        }
        Ok(survey) => {
            if survey.is_empty() {
                println!("No removable drive was found, so the Xbox wizard has nothing to");
                println!("offer. Plug one in and run this again.");
            }
            let selectable = survey.selectable();
            if !selectable.is_empty() {
                println!(
                    "{} drive(s) the Xbox wizard may write to:",
                    selectable.len()
                );
                for offer in selectable {
                    println!(
                        "  {}  {} bytes free",
                        offer.drive.mount_point.display(),
                        thousands(offer.drive.free_bytes)
                    );
                    println!(
                        "      {}",
                        reborn_core::xbox::filesystem_verdict(offer.drive.filesystem.as_deref())
                            .note()
                    );
                    if offer.drive.volume_identity.is_none() {
                        println!(
                            "      This drive reports no stable identity, so an \
                             interrupted copy starts over instead of resuming."
                        );
                    }
                }
            }
            let refused = survey.refused();
            if !refused.is_empty() {
                println!();
                println!("{} drive(s) found and not offered:", refused.len());
                for offer in refused {
                    println!("  {}", offer.drive.mount_point.display());
                    println!(
                        "      {}",
                        offer
                            .why_not()
                            .unwrap_or_else(|| "no reason recorded".to_string())
                    );
                }
            }
        }
    }

    println!();
    println!("Ejecting is not done from in here. reborn-core refuses it rather than");
    println!("pretending: it needs a driver level call this crate will not make. Use");
    println!("the operating system's own eject once a copy finishes.");
    Ok(OK)
}

// ---------------------------------------------------------------------------
// Onboarding
// ---------------------------------------------------------------------------

/// Where first run got to.
///
/// Read only, and it stays that way. Recording a stage from a command line would
/// latch a decision the player never made, and two of the stages cannot be
/// re-entered once they have run.
fn onboarding(
    platform: &Platform,
    from: Option<PathBuf>,
) -> reborn_core::Result<std::process::ExitCode> {
    let (found, looked) = find_legacy(platform, from)?;
    let env = match &found {
        Some(install) => {
            println!("2.1 install   {}", install.root.display());
            Environment::with_legacy_install(install.clone())
        }
        None => {
            println!("2.1 install   none found. Looked in:");
            for path in &looked {
                println!("              {}", path.display());
            }
            println!("              The migration stages do not exist without one, and");
            println!("              first run happens once, so a wrong answer here is");
            println!("              permanent. Pass --from if it is somewhere else.");
            Environment::nothing_found()
        }
    };

    let journal_dir = platform.data_dir(DataDir::Config)?;
    println!("Journal       {}", journal_dir.display());

    let flow = Onboarding::open(platform, env)?;
    let mut problem = false;

    println!();
    if flow.progress().is_truncated() {
        problem = true;
        println!("PROBLEM: the journal stops at a line that will not parse, so what it");
        println!("does not say proves nothing. Every stage that writes files will ask to");
        println!("be reconciled rather than run again.");
        println!();
    }

    // Not a problem, and deliberately not silent. These answers were real when
    // they were given and no longer apply to anything, so saying nothing would
    // be pretending the flow never changed.
    let superseded = flow.progress().superseded();
    if !superseded.is_empty() {
        println!(
            "First run has changed since this machine last ran it. {} answer(s) no",
            superseded.len()
        );
        println!("longer apply and were carried past, not counted:");
        for stage in superseded {
            println!("  {stage}");
        }
        println!();
    }

    match flow.next() {
        Next::AlreadyComplete { flow_version } => {
            println!("First run is finished (flow version {flow_version}) and is not shown");
            println!("again. Settings is where these change now.");
        }
        Next::Show {
            stage,
            position,
            total,
            resumed,
        } => {
            println!(
                "Next: {} , step {position} of {total}{}",
                stage.slug(),
                if resumed {
                    ", already seen once, so this is Continue rather than Get started"
                } else {
                    ""
                }
            );
        }
        Next::Reconcile {
            stage,
            position,
            total,
            what_to_do,
        } => {
            problem = true;
            println!(
                "Next: {} , step {position} of {total}, and it needs reconciling.",
                stage.slug()
            );
            println!("{what_to_do}");
        }
    }

    println!();
    println!("Stages that apply to this machine:");
    for planned in flow.plan() {
        let stage = planned.stage;
        let answered = flow.progress().satisfaction(stage).is_some();
        let mut notes = Vec::new();
        if let Some(why) = flow.moot(stage) {
            notes.push(format!("nothing to do: {why}"));
        }
        if stage.is_irreversible() {
            notes.push("writes files outside the journal".to_string());
        }
        if stage.skip_rule().is_declinable() {
            notes.push("can be declined".to_string());
        }
        if flow.progress().was_reached(stage) && !answered {
            notes.push("was reached and never answered".to_string());
        }
        println!(
            "  {}/{}  {:<20} {:<10} {}",
            planned.position,
            planned.total,
            stage.slug(),
            if answered { "answered" } else { "open" },
            notes.join("; ")
        );
    }

    let set_aside = flow.set_aside();
    if !set_aside.is_empty() {
        println!();
        println!("Stages this machine does not have:");
        for (stage, why) in set_aside {
            println!("  {:<20} {why}", stage.slug());
        }
    }

    if let Some(plan) = flow.migration_plan() {
        println!();
        match plan.blocked {
            Some(reason) => {
                problem = true;
                println!("The migration this offers is BLOCKED:");
                println!("  {reason}");
            }
            None => println!(
                "The migration this offers has {} step(s). `rebornctl migrate` shows them.",
                plan.steps.len()
            ),
        }
    }

    let remaining = flow.preparation_remaining();
    if !remaining.is_empty() {
        println!();
        println!("Preparation tasks not recorded as done:");
        for task in remaining {
            println!("  {:<24} {}", task.slug(), task.label());
        }
    }

    let summary = flow.summary();
    println!();
    println!("What first run has decided so far:");
    println!(
        "  controller   {}",
        summary
            .controller
            .clone()
            .unwrap_or_else(|| "not recorded".to_string())
    );
    if let Some(family) = summary.controller_family {
        println!("               {}", family.label());
    }
    if let Some(port) = summary.controller_port {
        println!("               port {port}");
    }
    if let Some(because) = &summary.no_controller_because {
        println!("               declined: {because}");
    }
    println!(
        "  graphics     {}",
        summary
            .graphics
            .map(|g| g.preset().label().to_string())
            .unwrap_or_else(|| "not recorded".to_string())
    );
    println!(
        "  hardware     {}",
        summary
            .detected_hardware
            .clone()
            .unwrap_or_else(|| "never measured".to_string())
    );
    println!(
        "  migrated     {}",
        match (&summary.migrated_from, summary.migrated_bytes) {
            (Some(from), Some(bytes)) =>
                format!("{} bytes from {}", thousands(bytes), from.display()),
            (Some(from), None) => format!("chosen: {}", from.display()),
            _ => "nothing".to_string(),
        }
    );
    println!(
        "  game size    {}",
        summary
            .game_bytes
            .map(|b| format!("{} bytes", thousands(b)))
            .unwrap_or_else(|| "never measured".to_string())
    );
    if !summary.tasks_done.is_empty() {
        println!(
            "  tasks done   {}",
            summary
                .tasks_done
                .iter()
                .map(|t| t.slug())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }

    let absent = summary.absent();
    if !absent.is_empty() {
        println!();
        println!(
            "Nothing measured these, so they must be shown as absent rather than \
             filled in: {}",
            absent.join(", ")
        );
    }

    println!();
    println!("Nothing here writes to the journal. A stage recorded from a terminal");
    println!("would latch a decision the player never made, and two of the stages");
    println!("cannot be re-entered once they have run.");

    Ok(if problem { PROBLEM } else { OK })
}

// ---------------------------------------------------------------------------
// Stats queue
// ---------------------------------------------------------------------------

/// Milliseconds since the epoch, or `None` when the clock cannot be read.
///
/// `None` rather than zero. Zero is 1970 and would make every entry look
/// ancient, which is the direction that deletes things.
fn now_ms() -> Option<u64> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_millis() as u64)
}

/// One line for a queue state, in the words the state actually means.
///
/// `Held` is a success. Reading it as a failure and retrying is what produced a
/// real outage on this project, so it says so here rather than looking like an
/// error next to the others.
fn describe_state(state: &EntryState) -> String {
    match state {
        EntryState::Queued => "waiting to be sent".to_string(),
        EntryState::Quarantined { reason } => format!("held back: {}", reason.message()),
        EntryState::Accepted => "sent and counted".to_string(),
        EntryState::Held { reason } => {
            format!("sent, and a person will decide. This is a success: {reason}")
        }
        EntryState::Rejected { reason } => format!(
            "refused by the server and kept on disk. `stats retry --apply` puts it back: {reason}"
        ),
        EntryState::Stalled { reason } => {
            format!("too many failed attempts; the next send pass puts it back: {reason}")
        }
    }
}

/// Short form of a content key, for a line a person reads.
fn short_key(key: &reborn_core::stats::CaptureKey) -> String {
    key.as_str().chars().take(12).collect()
}

fn stats_list(platform: &Platform) -> reborn_core::Result<std::process::ExitCode> {
    let queue = StatsQueue::open(platform)?;
    println!("Queue         {}", queue.dir().display());

    let entries = queue.entries()?;
    println!();
    if entries.is_empty() {
        println!("No results are in the queue.");
    } else {
        println!("{} result(s), the ledger applied over them:", entries.len());
        for entry in &entries {
            println!(
                "  {}  {}",
                short_key(&entry.key),
                describe_state(&entry.state)
            );
            match &entry.summary {
                Some(summary) => {
                    let teams = summary
                        .teams
                        .iter()
                        .map(|t| format!("{} {}", t.name, t.score))
                        .collect::<Vec<_>>()
                        .join(" against ");
                    println!(
                        "      {} , {} player(s), captured {}",
                        teams, summary.players, summary.captured_at
                    );
                    if !summary.sides_verified {
                        println!(
                            "      The capture does not say its sides were verified, so \
                             which team a player was on is not established."
                        );
                    }
                    if summary.needs_review {
                        println!("      Flagged for review: {}", summary.flags.join(", "));
                    }
                }
                // Never filled in with placeholder team names: a row that cannot
                // be described is shown as one that cannot be described.
                None => println!("      The capture could not be read, so there is no box score."),
            }
            // What the server said about a terminal result, such as "Recorded
            // as incomplete". Stored on the entry and shown to nobody until fix
            // wave 2: the state alone says "sent and counted", which is true
            // and is not the whole truth.
            if let Some(note) = &entry.note {
                println!("      {note}");
            }
            if entry.attempts > 0 {
                println!("      {} failed attempt(s)", entry.attempts);
            }
            if let Some(session) = &entry.session_id {
                println!("      session {session}");
            } else {
                println!("      no session accounts for it");
            }
            // Named, so a naming drift between the emulator and the launcher
            // is something a person reading this notices. "unknown" means the
            // capture was found somewhere neither side agreed on, and the mode
            // was recorded rather than guessed.
            match entry.capture_folder.as_deref() {
                Some(reborn_core::stats::UNKNOWN_MODE) => println!(
                    "      found in a folder this launcher does not know, so its mode is \
                     recorded as unknown rather than guessed"
                ),
                Some(mode) => println!("      {mode}"),
                None => println!("      from the folder older runtimes wrote, so its mode is not known"),
            }
            if let Some(path) = &entry.source_path {
                println!("      from {}", path.display());
            }
        }
    }

    let pending = queue.pending()?.len();
    let quarantined = queue.quarantined()?.len();
    let stalled = queue.stalled()?.len();
    let orphans = queue.orphaned_payloads()?;

    println!();
    println!("Waiting       {}", thousands(pending as u64));
    println!("Held back     {}", thousands(quarantined as u64));
    println!("Stalled       {}", thousands(stalled as u64));
    if !orphans.is_empty() {
        println!(
            "Orphans       {} payload(s) with no record beside them",
            thousands(orphans.len() as u64)
        );
        let lines: Vec<String> = orphans.iter().map(short_key).collect();
        print_list("  ", &lines);
    }

    println!();
    println!("Nothing here sends anything. stats::Submitter is the whole network");
    println!("seam and the caller implements it, so no url, no credential and no");
    println!("socket lives in this crate. Until something implements it, a queued");
    println!("result stays queued, which is the safe direction: the game is over and");
    println!("nobody else has a copy.");

    Ok(if quarantined > 0 || stalled > 0 || !orphans.is_empty() {
        PROBLEM
    } else {
        OK
    })
}

/// Take what Dolphin wrote into the queue.
///
/// Needs `--apply` because the source file is deleted once its capture is safely
/// in the queue. That order is the core's and is correct, but it still means a
/// file leaves the folder somebody may be looking at.
fn stats_ingest(
    platform: &Platform,
    from: Option<PathBuf>,
    session: Option<&str>,
    apply: bool,
) -> reborn_core::Result<std::process::ExitCode> {
    let queue = StatsQueue::open(platform)?;

    // With no --from, the same sweep a launch does: every folder a capture can
    // be in. This used to default to the parent GameStats folder and call the
    // single-folder ingest, and Dolphin writes into GameStats\Online and
    // GameStats\Offline. The parent exists, so the command did not even report
    // a missing folder. It found nothing, said there was nothing to do, and
    // left the captures a person was looking for one level down.
    let (user_dir, folders) = match &from {
        Some(one) => (None, vec![(one.clone(), None)]),
        None => {
            let user = resolve_user_dir(platform, None)?;
            println!("{}", user.explain());
            let dirs = capture_folders_on_disk(user.path());
            (Some(user.path().to_path_buf()), dirs)
        }
    };

    println!("Queue         {}", queue.dir().display());
    for (dir, mode) in &folders {
        println!(
            "Captures      {}{}",
            dir.display(),
            match mode {
                Some(m) => format!("  ({m})"),
                None => String::new(),
            }
        );
    }

    // What is there, per folder, so a person can see which folder a capture is
    // actually sitting in rather than a single total.
    let mut found: Vec<(PathBuf, Vec<PathBuf>)> = Vec::new();
    let mut missing = 0usize;
    for (dir, _) in &folders {
        if !dir.exists() {
            missing += 1;
            continue;
        }
        let mut files = Vec::new();
        for entry in std::fs::read_dir(dir).map_err(|source| reborn_core::Error::Io {
            path: dir.clone(),
            source,
        })? {
            let entry = entry.map_err(|source| reborn_core::Error::Io {
                path: dir.clone(),
                source,
            })?;
            let path = entry.path();
            let is_json = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.eq_ignore_ascii_case("json"))
                .unwrap_or(false);
            if path.is_file() && is_json {
                files.push(path);
            }
        }
        files.sort();
        if !files.is_empty() {
            found.push((dir.clone(), files));
        }
    }

    let total: usize = found.iter().map(|(_, f)| f.len()).sum();
    println!();
    if total == 0 {
        if missing == folders.len() {
            println!("None of those folders exists, so Dolphin has not written a capture to");
            println!("any of them. Nothing to do.");
        } else {
            println!("No capture files in any of those folders. Only *.json directly in a");
            println!("folder counts, so uploaded.txt and the _previous folder are left alone");
            println!("by design.");
        }
        return Ok(OK);
    }
    println!("{total} capture file(s):");
    for (dir, files) in &found {
        println!("  {}", dir.display());
        let lines: Vec<String> = files
            .iter()
            .map(|p| {
                p.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| p.display().to_string())
            })
            .collect();
        print_list("    ", &lines);
    }

    println!();
    match session {
        Some(id) => println!("These will be attributed to session {id}."),
        None => {
            println!("No --session was given, so every one of these is held back rather");
            println!("than queued. An offline game, a league game and a ranked game make");
            println!("byte identical captures, so with no session there is nowhere a");
            println!("result can safely be sent, and guessing is how an offline game");
            println!("lands on a real league fixture. `rebornctl sessions` lists the ids.");
        }
    }

    if !apply {
        println!();
        println!("Nothing was moved. Re-run with --apply to do it.");
        return Ok(OK);
    }

    // The same call a launch makes, so the recovery tool and the product can
    // never disagree about which folders hold a game somebody played.
    let results = match (&user_dir, &from) {
        (Some(user), _) => queue.ingest_all(user, session)?,
        (None, Some(one)) => queue.ingest_from(one, session)?,
        (None, None) => unreachable!("one of the two is always set"),
    };
    println!();
    let mut problem = false;
    for result in &results {
        match result.key() {
            Some(key) => println!("  {}  {}", short_key(key), result.message()),
            None => println!("  {}", result.message()),
        }
        if matches!(
            result,
            reborn_core::stats::Enqueued::CouldNotRead { .. }
                | reborn_core::stats::Enqueued::Quarantined(_)
                | reborn_core::stats::Enqueued::SourceKept { .. }
        ) {
            problem = true;
        }
    }
    println!();
    println!("{} file(s) handled.", results.len());
    Ok(if problem { PROBLEM } else { OK })
}

/// Put stalled AND refused results back in the queue.
///
/// Stalled entries are put back by every send pass on their own; this stays
/// because it is how a person does it now rather than at the end of the next
/// game.
///
/// Refused ones are the reason this command matters. A rejection writes a
/// ledger row and nothing else in the launcher can undo it, so one server-side
/// fault refusing a queue used to destroy every game in it permanently. This
/// is the only way back, and it is deliberately a person typing a command:
/// resending a considered refusal on a timer is the retry loop the whole
/// design exists to prevent.
fn stats_retry(platform: &Platform, apply: bool) -> reborn_core::Result<std::process::ExitCode> {
    let queue = StatsQueue::open(platform)?;
    let stalled = queue.stalled()?;
    let refused: Vec<_> = queue
        .entries()?
        .into_iter()
        .filter(|e| matches!(e.state, reborn_core::stats::EntryState::Rejected { .. }))
        .collect();
    println!("Queue         {}", queue.dir().display());
    println!();
    if stalled.is_empty() && refused.is_empty() {
        println!("Nothing is stalled, and nothing was refused.");
        return Ok(OK);
    }
    if !stalled.is_empty() {
        println!("{} stalled result(s):", stalled.len());
        for entry in &stalled {
            println!(
                "  {}  {}",
                short_key(&entry.key),
                describe_state(&entry.state)
            );
        }
    }
    if !refused.is_empty() {
        if !stalled.is_empty() {
            println!();
        }
        println!("{} refused result(s):", refused.len());
        for entry in &refused {
            println!(
                "  {}  {}",
                short_key(&entry.key),
                describe_state(&entry.state)
            );
        }
        println!();
        println!("A refusal means the server looked at the result and did not store it,");
        println!("so putting one back cannot duplicate anything. Results the server DID");
        println!("store are never touched by this.");
    }
    if !apply {
        println!();
        println!("Nothing was changed. Re-run with --apply to put them back in the queue.");
        return Ok(PROBLEM);
    }
    let moved = queue.retry_stalled()?;
    let unrefused = queue.requeue_rejected()?;
    println!();
    println!(
        "{} stalled and {} refused result(s) put back in the queue, attempt counts reset.",
        moved.len(),
        unrefused.len()
    );
    println!("Nothing expires a stalled capture on its own: the game happened, so the");
    println!("only ways out are sending it or the player deleting it.");
    Ok(OK)
}

fn stats_prune(
    platform: &Platform,
    keep_days: u64,
    apply: bool,
) -> reborn_core::Result<std::process::ExitCode> {
    let queue = StatsQueue::open(platform)?;
    let keep_ms = keep_days.saturating_mul(24 * 60 * 60 * 1000);
    println!("Queue         {}", queue.dir().display());
    println!("Keeping       results for {keep_days} day(s) after they reached the server");

    let entries = queue.entries()?;
    let terminal: Vec<_> = entries.iter().filter(|e| e.state.is_terminal()).collect();

    println!();
    if terminal.is_empty() {
        println!("No result has reached the server yet, so there is nothing this could");
        println!("delete. Queued, stalled and held back results are never pruned at any");
        println!("age, because nobody else has a copy of them.");
        return Ok(OK);
    }

    // Ages are worked out here only so the preview can show them. The core
    // re-decides with its own clock when --apply runs, and it is the authority.
    match now_ms() {
        Some(now) => {
            println!("{} result(s) that reached the server:", terminal.len());
            for entry in &terminal {
                let since = entry.finished_ms.unwrap_or(entry.first_seen_ms);
                let age_ms = now.saturating_sub(since);
                println!(
                    "  {}  {:.1} day(s) old, {}",
                    short_key(&entry.key),
                    age_ms as f64 / (24.0 * 60.0 * 60.0 * 1000.0),
                    if age_ms >= keep_ms {
                        "payload would be deleted"
                    } else {
                        "inside the window, kept"
                    }
                );
            }
        }
        None => {
            println!(
                "{} result(s) reached the server, and the system clock could not",
                terminal.len()
            );
            println!("be read, so their ages cannot be shown here.");
        }
    }

    println!();
    println!("Only the payload goes. The ledger row stays forever, because it is the");
    println!("only thing that stops a capture the player still has a copy of from");
    println!("being sent a second time once its payload is gone.");

    if !apply {
        println!();
        println!("Nothing was deleted. Re-run with --apply to do it.");
        return Ok(OK);
    }
    let removed = queue.prune_terminal(keep_ms)?;
    println!();
    println!("{} payload(s) deleted.", removed.len());
    Ok(OK)
}

// ---------------------------------------------------------------------------
// Inventory
// ---------------------------------------------------------------------------

/// Hash a tree and produce the manifest that describes it.
///
/// This is the half of the pipeline that was missing: [`crate::manifest`]
/// defines the schemas and `library` verifies against them, but nothing
/// produced one, so verify and repair had never run against real content.
///
/// It signs nothing, and the type it hands back says so.
#[allow(clippy::too_many_arguments)]
fn inventory(
    root: &Path,
    component: &str,
    version: &str,
    sequence: u64,
    install_root: Option<&str>,
    out: Option<&Path>,
    release_out: Option<&Path>,
    pin_url: Option<&str>,
    apply: bool,
) -> reborn_core::Result<std::process::ExitCode> {
    let Some(id) = ComponentId::parse(component) else {
        eprintln!("--component {component:?} is not a component name. Use one of:");
        for known in ComponentId::ALL {
            eprintln!("  {}", known.as_str());
        }
        return Ok(misuse());
    };

    let spec = match install_root {
        Some(given) => ComponentSpec::with_install_root(id, version, sequence, given),
        None => ComponentSpec::new(id, version, sequence),
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

    if apply && out.is_none() {
        eprintln!("--apply needs --out <dir>: there is nowhere to write the manifest.");
        return Ok(misuse());
    }
    if release_out.is_some() != pin_url.is_some() {
        eprintln!("--release-out and --pin-url go together. A release pins the manifest");
        eprintln!("at the address it will be served from, and there is no address to");
        eprintln!("invent: every client would refuse a release pinning a url that serves");
        eprintln!("nothing.");
        return Ok(misuse());
    }

    println!("Tree          {}", root.display());
    println!(
        "Component     {} version {}, sequence {}",
        spec.id().as_str(),
        spec.version(),
        spec.sequence()
    );
    println!("Install root  {}", spec.install_root());
    println!(
        "Customizable  {}. Derived from the component identity, never passed in.",
        yes_no(spec.id().is_customizable())
    );

    let size = measure_tree(root);
    println!();
    println!(
        "About to hash {} file(s), {} bytes. Progress goes to standard error.",
        thousands(size.files),
        thousands(size.bytes)
    );
    if size.unreadable > 0 {
        println!(
            "{} entry(s) could not be read while measuring, so that is a floor.",
            thousands(size.unreadable)
        );
    }
    println!("The tree must be still: a file whose length changes between the walk");
    println!("and the read is refused, because a manifest taken from a moving tree");
    println!("describes a state that never existed and fails its own verification.");

    let log = ScanLog::new();
    let outcome = scan_component(root, &spec, &log)?;
    let scan = match outcome {
        ScanOutcome::Cancelled { progress } => {
            println!();
            println!(
                "Cancelled after {} of {} file(s). No manifest was produced, and that",
                thousands(progress.files_hashed),
                progress
                    .files_total()
                    .map(thousands)
                    .unwrap_or_else(|| "?".to_string())
            );
            println!("is deliberate: a manifest built from a partial list verifies clean");
            println!("and leaves every file it never reached unmanaged.");
            return Ok(PROBLEM);
        }
        ScanOutcome::Complete(scan) => scan,
    };

    println!();
    println!(
        "Hashed {} file(s), {} bytes, in {:.1} s.",
        thousands(scan.progress.files_hashed),
        thousands(scan.progress.bytes_hashed),
        log.seconds()
    );

    if !scan.empty_dirs.is_empty() {
        println!();
        println!(
            "{} directory(s) hold no file at any depth:",
            scan.empty_dirs.len()
        );
        print_list("  ", &scan.empty_dirs);
        println!("nsr.component.v1 has no row for a directory, so these do not survive a");
        println!("fetch and install. If the game needs one of them to exist, that has to");
        println!("be handled outside the manifest.");
    }

    let serialized = SerializedComponent::new(scan.component)?;
    println!();
    println!(
        "Manifest      {} bytes, sha256 {}",
        thousands(serialized.bytes().len() as u64),
        serialized.sha256()
    );

    let Some(out_dir) = out else {
        println!();
        println!("Nothing was written. Re-run with --out <dir> --apply to write it.");
        return Ok(OK);
    };
    let manifest_path = out_dir.join(format!("{}.json", spec.id().as_str()));
    println!("Would write   {}", manifest_path.display());
    if let Some(path) = release_out {
        println!("Would write   {} (unsigned release)", path.display());
    }

    if !apply {
        println!();
        println!("Nothing was written. Re-run with --apply to do it.");
        return Ok(OK);
    }

    serialized.write_to(&manifest_path)?;
    println!();
    println!("Wrote {}", manifest_path.display());
    println!("Those exact bytes are what the hash above covers. Serializing again for");
    println!("the file would be how a release comes to pin a hash no published file");
    println!("has, which every client reports as tampering.");

    if let (Some(path), Some(url)) = (release_out, pin_url) {
        let release = UnsignedReleaseBuilder::new(spec.version(), spec.sequence())?
            .pin(url, &serialized)?
            .build()?;
        release.write_to(path)?;
        println!();
        println!("Wrote {}", path.display());
        println!("It is UNSIGNED and no client will accept it. The release object is the");
        println!("only signed thing in the product, the key is not here, and there is no");
        println!("path from this tool to a client that would accept anything. What has to");
        println!("happen next is a signature over the exact bytes of that file.");
    }

    Ok(OK)
}

// ---------------------------------------------------------------------------
// Music
// ---------------------------------------------------------------------------

/// The custom soundtrack: what the user imported, what is recorded, what is
/// missing.
fn music(platform: &Platform) -> reborn_core::Result<std::process::ExitCode> {
    let data = platform.paths.data_root()?;
    let sources = reborn_core::music::sources_dir(&data);
    let packs = reborn_core::music::packs_dir(&data);
    let record = reborn_core::music::soundtrack_path(&data);

    println!("Sources       {}", sources.display());
    println!("Packs         {}", packs.display());
    println!("Record        {}", record.display());

    let imported = reborn_core::music::scan_sources(&sources)?;
    println!();
    if imported.is_empty() {
        println!("No audio has been imported. That is what a fresh install looks like.");
    } else {
        let bytes: u64 = imported.iter().map(|s| s.bytes).sum();
        println!(
            "{} imported file(s), {} bytes:",
            imported.len(),
            thousands(bytes)
        );
        let lines: Vec<String> = imported
            .iter()
            .map(|s| format!("{} , {} bytes", s.relative_path, thousands(s.bytes)))
            .collect();
        print_list("  ", &lines);
        println!("Running times are absent because measuring one needs a decoder, which");
        println!("is the bundled FFmpeg and not this crate. A track list here shows no");
        println!("duration rather than a guessed one.");
    }

    let pack_count = std::fs::read_dir(&packs)
        .map(|entries| entries.flatten().filter(|e| e.path().is_file()).count())
        .unwrap_or(0);
    println!();
    println!("{pack_count} file(s) in the packs directory.");

    match reborn_core::music::SoundtrackState::load(&record)? {
        None => {
            println!();
            println!("No soundtrack record, so no custom soundtrack has been applied. The");
            println!("offline image should match its shipped hash.");
        }
        Some(state) => {
            println!();
            println!("Applied to    offline-content {}", state.base_version);
            println!("Base artifact {}", state.base_artifact_sha256);
            println!(
                "Rebuilt image {}",
                state
                    .rebuilt_sha256
                    .as_ref()
                    .map(|h| h.to_string())
                    // Absent is the honest state for edited but not built yet,
                    // and it is what stops a half finished edit claiming an
                    // image that does not exist.
                    .unwrap_or_else(|| "not built yet".to_string())
            );
            println!("Slots filled  {}", state.order.len());

            let report = state.verify(&data)?;
            let skipped = report.skipped();
            let present = report.present_user_slots();
            println!(
                "User audio    {} slot(s) present: {}",
                present.len(),
                present
                    .iter()
                    .map(u8::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            if report.is_fully_replayable() {
                println!("Every slot can be replayed onto a fresh base.");
            } else {
                println!();
                println!("{} slot(s) would be skipped on a rebuild:", skipped.len());
                for (slot, why) in &skipped {
                    println!("  slot {slot}: {why}");
                }
                println!("The row is kept either way. Dropping it would destroy the only");
                println!("record of what the user wanted there.");
            }
        }
    }

    println!();
    println!("Building a soundtrack is not implemented, and reborn-core says so with a");
    println!("typed refusal rather than a guess. Three things are missing and none is");
    println!("recorded anywhere in the repository: the per stream position entry layout");
    println!("inside pathfind.dat, the sample accounting extend_slot_40big.py performs");
    println!("when a slot grows, and the title, artist and album record layout in");
    println!("template.dat. Guessing any of them yields a disc that boots and plays");
    println!("silence, which is worse than a crash because it looks like success.");
    Ok(OK)
}

// ---------------------------------------------------------------------------
// Content
// ---------------------------------------------------------------------------

/// What is installed, and anything the last run left half done.
/// Update one content component from a signed origin.
///
/// The same six steps the launcher takes, printed one at a time: open the
/// signed release, fetch the pinned manifest, plan against what is on disk,
/// fetch what differs, hash the staged set again, swap. Every step that can
/// refuse says why and what remains safe, and nothing before `--apply` writes.
#[allow(clippy::too_many_arguments)]
fn update_content(
    platform: &Platform,
    origin_url: &str,
    component: &str,
    trust: Option<&str>,
    data_root: Option<&Path>,
    apply: bool,
    xdelta: Option<&Path>,
    builder: Option<&Path>,
    install_root: Option<&Path>,
) -> reborn_core::Result<std::process::ExitCode> {
    use reborn_core::content::pipeline::{
        install_dir, CheckRequest, ComponentState, Depth, FetchOutcomeState, ImageHook, PromoteState,
        Updater,
    };
    use reborn_core::origin::Origin;
    use reborn_core::signing::{decode_public_key, Ed25519Verifier};

    let Some(id) = ComponentId::parse(component) else {
        eprintln!("--component {component:?} is not a component name.");
        return Ok(misuse());
    };
    if install_dir(id).is_none() {
        println!("{component} is not updated this way. The launcher updates through its");
        println!("installer (rebornctl update check), and controller-db has no artifact yet.");
        return Ok(PROBLEM);
    }

    let verifier = match trust {
        Some(hex) => match decode_public_key(hex) {
            Some(key) => Ed25519Verifier::with_keys(&[key])?,
            None => {
                println!("--trust is not a public key: expected 64 hex characters.");
                return Ok(PROBLEM);
            }
        },
        None => Ed25519Verifier::embedded(),
    };
    if !verifier.has_trust_anchor() {
        println!("No publisher key is compiled into this build, so no release can be opened");
        println!("and nothing was fetched. Pass --trust <hex> to prove a release with a key");
        println!("that is not shipped yet.");
        return Ok(PROBLEM);
    }

    let origin = Origin::new(origin_url);
    println!("Origin        {}", origin.base());
    let root_owned = match data_root {
        Some(p) => p.to_path_buf(),
        None => platform.paths.data_root()?,
    };
    // What the installer shipped, into the data root by hardlink, before
    // anything is planned. See content::seed.
    let seed_from = match (install_root, data_root) {
        (Some(p), _) => Some(p.to_path_buf()),
        (None, None) => platform.paths.install_root().ok(),
        (None, Some(_)) => None,
    };
    if let Some(from) = seed_from {
        let r = reborn_core::content::seed::seed_from_install(platform, &root_owned, &from, "GN7E69")?;
        for s in &r.seeded {
            println!(
                "Seeded        {} from {}: {} file(s), {}",
                s.component.as_str(),
                s.from.display(),
                thousands(s.files as u64),
                match s.how {
                    reborn_core::content::seed::SeedHow::Linked => "linked",
                    reborn_core::content::seed::SeedHow::Copied => "copied",
                }
            );
        }
    }
    let updater = Updater::new(platform, &root_owned, &origin, &verifier);

    // Verify depth: this command is the repair tool as well as the update
    // tool, so the live tree is hashed against the manifest even when the
    // versions match. The window's startup check uses Quick.
    let check = updater.check(&CheckRequest {
        components: &[id],
        depth: Depth::Verify,
    })?;
    println!(
        "Release       {} sequence {}, signature verified over {} bytes",
        check.release.version,
        check.release.sequence,
        thousands(check.signed_bytes)
    );
    println!("Data root     {}", root_owned.display());

    let entry = check.component(id).expect("the one component asked for is reported");
    let plan = match &entry.state {
        ComponentState::UpToDate { version, .. } => {
            println!();
            println!("Up to date: {component} {version}, and every file checks out.");
            return Ok(OK);
        }
        ComponentState::NotCarried => {
            println!(
                "The release does not carry {component}. It carries: {}.",
                check.carried.join(", ")
            );
            return Ok(PROBLEM);
        }
        ComponentState::NotUpdatable => {
            println!("{component} is not updated this way.");
            return Ok(PROBLEM);
        }
        ComponentState::Refused(r) => {
            println!();
            println!("Refused: {}", r.message());
            return Ok(PROBLEM);
        }
        ComponentState::Problem(m) => {
            println!();
            println!("Problem: {m}");
            return Ok(PROBLEM);
        }
        ComponentState::Available(plan) => plan,
    };

    println!();
    match &plan.from {
        Some(v) => println!("Installed     {} sequence {}", v.version, v.sequence),
        None => println!("Installed     nothing"),
    }
    println!("Offered       {} sequence {}", plan.manifest.version, plan.manifest.sequence);
    println!(
        "Fetch         {} file(s), {} bytes",
        thousands(plan.fetches.len() as u64),
        thousands(plan.download_bytes())
    );
    println!("Already right {} file(s), not touched", thousands(plan.already_present.len() as u64));
    if !plan.preserved.is_empty() {
        println!(
            "Kept as yours {} file(s) you rebuilt, never fetched over",
            thousands(plan.preserved.len() as u64)
        );
    }
    println!("Retained      {} bytes of replaced files, so this can be rolled back", thousands(plan.retained_bytes));
    for f in plan.fetches.iter().take(12) {
        println!("  {:<8} {}  ({} bytes)", format!("{:?}", f.reason).to_lowercase(), f.path, thousands(f.bytes));
    }
    if plan.fetches.len() > 12 {
        println!("  ... and {} more", thousands((plan.fetches.len() - 12) as u64));
    }

    if !apply {
        println!();
        println!("Nothing was downloaded. Re-run with --apply to fetch and swap.");
        return Ok(OK);
    }

    println!();
    let fetched = updater.fetch(&check)?;
    let fetch = fetched
        .components
        .first()
        .expect("one component was available, so one was fetched");
    match &fetch.state {
        FetchOutcomeState::Staged { files, .. } => {
            println!("Downloaded    {} file(s) staged and hashed", thousands(*files));
            println!("Verified      the staged set hashes to the manifest, on disk, now");
        }
        FetchOutcomeState::Incomplete {
            incomplete, corrupt, ..
        } => {
            println!(
                "Not complete: {} incomplete (kept, the next run resumes them), {} corrupt (deleted,",
                incomplete.len(),
                corrupt.len()
            );
            println!("the next run fetches them again). Nothing was swapped; the live install is as it was.");
            for p in incomplete.iter().chain(corrupt.iter()).take(8) {
                println!("  {p}");
            }
            return Ok(PROBLEM);
        }
        FetchOutcomeState::NotReady { missing, wrong } => {
            println!(
                "Staged set is not ready: {} missing, {} wrong. Nothing was swapped.",
                missing.len(),
                wrong.len()
            );
            return Ok(PROBLEM);
        }
    }

    // A roster is not done when its file lands: it is done when the image
    // carries it. With a test root the portraits go to scratch, never into a
    // real texture tree.
    let textures_root = match (id, data_root) {
        (ComponentId::Roster | ComponentId::GamePatch, None) => {
            Some(reborn_core::mods::pristine_root(platform, &GameId::parse("GN7E69")?)?)
        }
        _ => None,
    };
    let hook = ImageHook {
        builder: builder.map(Path::to_path_buf),
        xdelta: xdelta.map(Path::to_path_buf),
        extra_dirs: tool_dirs(),
        textures_root,
    };
    let promoted = updater.promote(&check, &hook)?;
    let promotion = promoted
        .components
        .first()
        .expect("one component was available, so one was promoted");
    match &promotion.state {
        PromoteState::Applied {
            receipt: r,
            game_patch,
            roster,
        } => {
            println!(
                "Live          {} {}: {} file(s) swapped in, {} kept as yours",
                r.component,
                r.version,
                thousands(r.promoted.len() as u64),
                thousands(r.preserved.len() as u64)
            );
            if !r.retired.is_empty() {
                println!(
                    "Removed       {} file(s) this version no longer ships, moved aside (not deleted)",
                    thousands(r.retired.len() as u64)
                );
                for p in r.retired.iter().take(8) {
                    println!("  {p}");
                }
            }
            if let Some(p) = &r.previous {
                println!("Rollback to   {} sequence {} is available", p.version, p.sequence);
            }
            let mut code = OK;
            if let Some(gp) = game_patch {
                println!();
                println!("Applying the game update to the image.");
                code = print_game_patch_result(gp);
            }
            if let Some(result) = roster {
                println!();
                println!("Applying the roster to the game image.");
                let roster_code = print_roster_result(result);
                if code == OK {
                    code = roster_code;
                }
            }
            Ok(code)
        }
        PromoteState::Refused(r) => {
            println!("Not swapped: {}", r.message());
            Ok(PROBLEM)
        }
        PromoteState::RolledBack { reason, restored } => {
            println!("The swap failed part way and was reversed: {reason}");
            println!(
                "{restored} file(s) put back; the previous version is live and the staged bytes are kept,"
            );
            println!("so a retry costs nothing over the network.");
            Ok(PROBLEM)
        }
    }
}

/// Where the image tools may be, beyond the install root the core searches:
/// the import bundle and folder beside this program (rebornctl sits next to
/// the launcher), and the 2.1 tools folder, where a player who had the old
/// launcher may already have a Reborn Builder. Found here rather than in the
/// core, which reads no environment variables.
fn tool_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(exe_dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)) {
        dirs.push(exe_dir.join(reborn_core::game_patch::BUNDLE_DIR));
        dirs.push(exe_dir.join("tools"));
        dirs.push(exe_dir);
    }
    if let Ok(appdata) = std::env::var("APPDATA") {
        dirs.push(PathBuf::from(appdata).join("NFL Street Reborn").join("Tools"));
    }
    dirs
}

/// Apply the installed game-patch chain by hand. The pipeline does this
/// after promoting the component; this is for a player whose tool was
/// missing at the time, and for tests.
fn game_patch_apply(
    platform: &Platform,
    data_root: Option<&Path>,
    xdelta: Option<&Path>,
    builder: Option<&Path>,
    textures: Option<&Path>,
) -> reborn_core::Result<std::process::ExitCode> {
    use reborn_core::content::pipeline::GamePatchResult;
    use reborn_core::game_patch::{self, Xdelta3};

    let root = match data_root {
        Some(p) => p.to_path_buf(),
        None => platform.paths.data_root()?,
    };
    let exe = match xdelta {
        Some(p) => p.to_path_buf(),
        None => {
            let dirs = game_patch::xdelta_dirs(platform, &tool_dirs());
            match game_patch::find_xdelta(platform, &dirs) {
                Some(p) => p,
                None => {
                    println!("{}", GamePatchResult::NoPatcher { looked: dirs }.message());
                    println!("Pass --xdelta <path to xdelta3.exe>. Nothing was changed.");
                    return Ok(PROBLEM);
                }
            }
        }
    };
    println!("Data root     {}", root.display());
    println!("Tool          {}", exe.display());
    println!();
    let patcher = Xdelta3::new(platform, exe);
    let outcome = game_patch::apply(
        platform,
        &game_patch::ApplyRequest {
            data_root: &root,
            patcher: &patcher,
        },
    )?;
    let result = match outcome {
        game_patch::Outcome::Applied(r) => GamePatchResult::Applied(r),
        game_patch::Outcome::UpToDate { sha256, .. } => GamePatchResult::UpToDate { sha256 },
        game_patch::Outcome::Refused(r) => GamePatchResult::Refused(r),
    };
    let code = print_game_patch_result(&result);
    if !matches!(result, GamePatchResult::Applied(_)) {
        return Ok(code);
    }
    let roster_file = root
        .join(DataDir::LibraryRoster.relative())
        .join(reborn_core::roster::ROSTER_FILE);
    if !roster_file.is_file() {
        println!();
        println!("No roster is installed, so there was nothing to re-apply.");
        return Ok(OK);
    }
    println!();
    println!("Re-applying the roster to the new image.");
    roster_apply(platform, Some(&roster_file), builder, data_root, textures, true)
}

/// Report what the delta tool did to the image, and whether that counts as done.
fn print_game_patch_result(result: &reborn_core::content::pipeline::GamePatchResult) -> std::process::ExitCode {
    use reborn_core::content::pipeline::GamePatchResult;
    match result {
        GamePatchResult::Applied(r) => {
            println!(
                "Patched       {} step(s): {}",
                r.steps.len(),
                r.steps.join(", ")
            );
            println!("Was           {}", r.from_sha256);
            println!("Now           {}", r.to_sha256);
            println!("Pristine      {}", r.pristine.display());
            println!(
                "Booted        {} ({})",
                r.booted.display(),
                match r.booted_how {
                    reborn_core::game_image::Establish::Linked => "linked, no extra disk",
                    reborn_core::game_image::Establish::Copied => "copied",
                }
            );
            for p in &r.retired {
                println!("Removed       {}", p.display());
            }
            OK
        }
        GamePatchResult::UpToDate { .. } => {
            println!("{}", result.message());
            OK
        }
        GamePatchResult::NoPatcher { .. } => {
            println!("{}", result.message());
            println!("Pass --xdelta to `rebornctl game-patch apply` once the tool is in place; the patches are live.");
            PROBLEM
        }
        GamePatchResult::Refused(r) => {
            println!("Refused: {}", r.message());
            PROBLEM
        }
    }
}

/// Report what the Builder did to the image, and whether that counts as done.
fn print_roster_result(result: &reborn_core::content::pipeline::RosterResult) -> std::process::ExitCode {
    use reborn_core::content::pipeline::RosterResult;
    match result {
        RosterResult::NothingToApply => {
            println!("{}", result.message());
            OK
        }
        RosterResult::Applied(r) => {
            println!(
                "Applied       {} players, {} teams, {} portrait(s) written",
                thousands(r.report.players),
                thousands(r.report.teams),
                thousands(r.report.portraits)
            );
            println!("Image         {}", r.image.display());
            match &r.before {
                Some(b) => println!("Was           {b}"),
                None => println!("Was           nothing booted yet"),
            }
            println!("Now           {}", r.after);
            if let Some(base) = &r.report.base {
                println!("Base          {base}");
            }
            if !r.report.warnings.is_empty() {
                println!("Warnings      {} from the site:", thousands(r.report.warnings.len() as u64));
                for w in r.report.warnings.iter().take(8) {
                    println!("  {w}");
                }
                if r.report.warnings.len() > 8 {
                    println!("  ... and {} more", thousands((r.report.warnings.len() - 8) as u64));
                }
            }
            println!("Recorded as a rebuilt image, so verification keeps it rather than re-downloading it.");
            OK
        }
        RosterResult::Unchanged { image, report } => {
            println!(
                "Unchanged: {} already carries this roster ({} players, {} teams). Nothing was swapped.",
                image.display(),
                thousands(report.players),
                thousands(report.teams)
            );
            OK
        }
        RosterResult::NoBuilder { .. } => {
            println!("{}", result.message());
            println!("Pass --builder to `rebornctl roster apply` once a Builder is in place; the roster is live.");
            PROBLEM
        }
        RosterResult::Refused(r) => {
            println!("Refused: {}", r.message());
            PROBLEM
        }
    }
}

/// Apply a roster to the live image with the Reborn Builder.
///
/// Defaults are the published roster, a Builder found beside the launcher or
/// in the 2.1 tools folder, and the real data root. With `--data-root` the
/// portraits go to a scratch folder unless `--textures` says otherwise, so a
/// test run never writes into a real texture tree.
fn roster_apply(
    platform: &Platform,
    roster: Option<&Path>,
    builder: Option<&Path>,
    data_root: Option<&Path>,
    textures: Option<&Path>,
    yes: bool,
) -> reborn_core::Result<std::process::ExitCode> {
    use reborn_core::roster::{self, ApplyOutcome, ApplyRequest, ROSTER_FILE};

    let root = match data_root {
        Some(p) => p.to_path_buf(),
        None => platform.paths.data_root()?,
    };
    let roster_path = match roster {
        Some(p) => p.to_path_buf(),
        None => root.join(DataDir::LibraryRoster.relative()).join(ROSTER_FILE),
    };
    let builder_path = match builder {
        Some(p) => p.to_path_buf(),
        None => {
            let dirs = roster::builder_dirs(platform, &tool_dirs());
            match roster::find_builder(platform, &dirs) {
                Some(p) => p,
                None => {
                    println!("No Reborn Builder console build was found. Looked in:");
                    for d in &dirs {
                        println!("  {}", d.display());
                    }
                    println!("Pass --builder <path to reborn-builder-cli.exe>. Nothing was changed.");
                    return Ok(PROBLEM);
                }
            }
        }
    };
    let textures_owned;
    let textures_root: Option<&Path> = match (textures, data_root) {
        (Some(t), _) => Some(t),
        // A test root: never write portraits into a real tree by default.
        (None, Some(_)) => None,
        (None, None) => {
            textures_owned = reborn_core::mods::pristine_root(platform, &GameId::parse("GN7E69")?)?;
            Some(textures_owned.as_path())
        }
    };

    println!("Data root     {}", root.display());
    println!("Roster        {}", roster_path.display());
    println!("Builder       {}", builder_path.display());
    println!(
        "Portraits to  {}",
        textures_root
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "a scratch folder under staging, not kept".to_string())
    );

    let req = ApplyRequest {
        data_root: &root,
        roster: &roster_path,
        builder: &builder_path,
        textures_root,
        accept_warnings: yes,
    };
    match roster::apply(platform, &req)? {
        ApplyOutcome::Applied(r) => {
            println!();
            println!(
                "Applied       {} players, {} teams, {} portrait(s) written",
                thousands(r.report.players),
                thousands(r.report.teams),
                thousands(r.report.portraits)
            );
            println!("Image         {}", r.image.display());
            match &r.before {
                Some(b) => println!("Was           {b}"),
                None => println!("Was           nothing booted yet"),
            }
            println!("Now           {}", r.after);
            if let Some(base) = &r.report.base {
                println!("Base          {base}");
            }
            if !r.report.warnings.is_empty() {
                println!("Warnings      {} from the site:", thousands(r.report.warnings.len() as u64));
                for w in r.report.warnings.iter().take(8) {
                    println!("  {w}");
                }
                if r.report.warnings.len() > 8 {
                    println!("  ... and {} more", thousands((r.report.warnings.len() - 8) as u64));
                }
            }
            println!("Recorded as a rebuilt image, so verification keeps it rather than re-downloading it.");
            Ok(OK)
        }
        ApplyOutcome::Unchanged { image, report, .. } => {
            println!();
            println!(
                "Unchanged: {} already carries this roster ({} players, {} teams). Nothing was swapped.",
                image.display(),
                thousands(report.players),
                thousands(report.teams)
            );
            Ok(OK)
        }
        ApplyOutcome::Refused(r) => {
            println!();
            println!("Refused: {}", r.message());
            Ok(PROBLEM)
        }
    }
}

fn content(platform: &Platform, apply: bool) -> reborn_core::Result<std::process::ExitCode> {
    let service = ContentService::for_platform(platform)?;
    let journal_path = service.layout().journal_path();
    let mut problem = false;

    println!("Data root     {}", service.layout().data_root().display());
    println!("Journal       {}", journal_path.display());
    println!();

    println!("Installed components:");
    for id in ComponentId::ALL {
        match service.installed(id.as_str()) {
            Ok(Some(pointer)) => println!(
                "  {:<16} {} (sequence {}){}",
                id.as_str(),
                pointer.current.version,
                pointer.current.sequence,
                pointer
                    .previous
                    .as_ref()
                    .map(|p| format!(", rollback to {}", p.version))
                    .unwrap_or_else(|| ", nothing to roll back to".to_string())
            ),
            Ok(None) => println!("  {:<16} nothing installed", id.as_str()),
            Err(e) => {
                // One unreadable pointer must not hide the other six. It is the
                // only statement of what is installed, so it is reported as a
                // problem rather than treated as absent.
                problem = true;
                println!("  {:<16} PROBLEM: {e}", id.as_str());
            }
        }
    }

    let journal = Journal::load(&journal_path);
    println!();
    if journal.operations.is_empty() {
        println!("No operation is recorded in flight.");
    } else {
        println!("{} operation(s) recorded:", journal.operations.len());
        for op in &journal.operations {
            let state = match op.state {
                reborn_core::content::OperationState::Planned => "planned",
                reborn_core::content::OperationState::Downloading => "downloading",
                reborn_core::content::OperationState::Staged => "staged, nothing live touched",
                reborn_core::content::OperationState::Swapping => {
                    "SWAPPING, live files were being moved"
                }
                reborn_core::content::OperationState::Failed => "failed",
            };
            println!("  {:<16} {} , {state}", op.component, op.version);
        }
    }

    let interrupted = journal.interrupted();
    if interrupted.is_empty() {
        println!();
        println!("Nothing was left half done, so there is nothing to put back.");
        return Ok(if problem { PROBLEM } else { OK });
    }

    println!();
    println!("{} swap(s) were interrupted:", interrupted.len());
    for op in &interrupted {
        println!("  {} {}", op.component, op.version);
        println!("      live    {}", op.live_root.display());
        println!("      staged  {}", op.staging.display());
        println!("      prior   {}", op.prior.display());
    }
    println!();
    println!("These always resolve backwards, to the version that was working, even");
    println!("when the new files look complete: a swap nobody finished is not a swap");
    println!("to trust. The staged bytes stay where they are, so going forward again");
    println!("does not mean downloading anything twice.");

    if !apply {
        println!();
        println!("Nothing was changed. Re-run with --apply to put them back.");
        return Ok(PROBLEM);
    }

    let recovered = service.recover()?;
    println!();
    for swap in &recovered {
        println!(
            "{}: put back {} file(s), returned {} to staging{}",
            swap.component,
            swap.restored.len(),
            swap.returned_to_staging.len(),
            if swap.pointer_rolled_back {
                ", and the version record was rolled back too"
            } else {
                ""
            }
        );
        if let Some(unresolved) = &swap.unresolved {
            problem = true;
            println!("  {unresolved}");
        }
    }
    println!();
    println!("{} swap(s) resolved.", recovered.len());
    Ok(if problem { PROBLEM } else { OK })
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    /// Every subcommand that writes, by the path a person types.
    ///
    /// The list is spelled out rather than derived, because the property being
    /// checked is that a *new* writing subcommand cannot quietly arrive without
    /// `--apply`. A derived list would pass by definition.
    const WRITING: &[&[&str]] = &[
        &["migrate"],
        &["mods", "import"],
        &["mods", "apply"],
        &["mods", "unapply"],
        &["stats", "ingest"],
        &["stats", "retry"],
        &["stats", "prune"],
        &["inventory"],
        &["content"],
        &["repair"],
    ];

    /// A temporary directory that removes itself.
    ///
    /// Hand rolled because `tempfile` is a dev-dependency of `reborn-core` and
    /// not of this crate, and adding one to another crate's manifest is not this
    /// change's to make. If it is added, this can go.
    struct TempDir(PathBuf);

    static TEMP_COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

    impl TempDir {
        fn new(tag: &str) -> Self {
            let base = std::env::temp_dir().join(format!(
                "rebornctl-test-{tag}-{}-{}",
                std::process::id(),
                TEMP_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            ));
            std::fs::create_dir_all(&base).expect("a temporary directory");
            Self(base)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            // Best effort. A directory left in the system temp folder is not
            // worth failing a test over, and panicking while unwinding aborts.
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn find<'a>(root: &'a clap::Command, path: &[&str]) -> &'a clap::Command {
        let mut current = root;
        for name in path {
            current = current
                .find_subcommand(name)
                .unwrap_or_else(|| panic!("no subcommand {}", path.join(" ")));
        }
        current
    }

    /// clap's own consistency check. A duplicate long flag or a malformed
    /// definition panics at parse time, which means it panics for whoever runs
    /// the tool rather than here.
    #[test]
    fn the_command_tree_is_well_formed() {
        Cli::command().debug_assert();
    }

    /// The rule the whole file is built around: nothing writes unless asked.
    ///
    /// A subcommand that copies, deletes or composes and has no `--apply` would
    /// act the moment somebody ran it to look.
    #[test]
    fn every_writing_subcommand_takes_apply() {
        let root = Cli::command();
        for path in WRITING {
            let cmd = find(&root, path);
            assert!(
                cmd.get_arguments().any(|a| a.get_id() == "apply"),
                "`{}` can write and has no --apply",
                path.join(" ")
            );
        }
    }

    /// The only recovery tool this product has must look where the emulator
    /// actually writes.
    ///
    /// `stats ingest` defaulted to the parent `GameStats` folder and called the
    /// single-folder ingest. Dolphin writes into `GameStats\Online` and
    /// `GameStats\Offline`. The parent exists, so the command did not even
    /// report a missing folder: it found nothing, printed "Nothing to do", and
    /// left every real capture sitting one level down. That is the command a
    /// person reaches for when the queue is stuck, and it could not find
    /// anything at all.
    ///
    /// Its help text said it defaulted to `LeagueGameStats`, which had not been
    /// true since the mode folders arrived, so the one place a person could
    /// check what it was about to read was wrong too.
    ///
    /// Checked in two places, because the defect had two halves. The help text
    /// is read from clap, which is what a person actually sees. The default
    /// path is read from the source of the function, because it takes a
    /// `Platform` and a resolved user directory and cannot be called in a test
    /// without one; what has to be true is that the no-flag path reaches
    /// `ingest_all` and the flag path still reaches the single-folder call.
    /// The command line tool writes to the same log the window does.
    ///
    /// Every failure this tool decides not to print in full goes to
    /// `tracing::warn!` in the core, and until fix wave 4 nothing in the
    /// workspace subscribed, so those events were dropped. A person running a
    /// recovery command is exactly the person who needs them.
    ///
    /// # How this is bounded so it cannot find itself
    ///
    /// `fn main`'s text is cut at the `fn run(` that follows it, which is
    /// thousands of lines before this test, so the name written below is never
    /// inside what is searched.
    #[test]
    fn every_run_of_the_command_line_tool_writes_to_the_launchers_log() {
        let src = include_str!("main.rs");
        let body = {
            let from = src
                .find("\nfn main() -> std::process::ExitCode {")
                .expect("main is not in this file any more");
            let rest = &src[from..];
            &rest[..rest.find("\nfn run(").expect("run is not in this file any more")]
        };
        assert!(
            body.contains("logging::start"),
            "rebornctl never starts a log, so every warning the core raises while it runs \
             is dropped and the detail is gone"
        );
    }

    #[test]
    fn stats_ingest_defaults_to_every_folder_the_emulator_writes_to() {
        let root = Cli::command();
        let cmd = find(&root, &["stats", "ingest"]);
        let from = cmd
            .get_arguments()
            .find(|a| a.get_id() == "from")
            .expect("stats ingest has no --from any more");
        let help = from
            .get_help()
            .map(|h| h.to_string())
            .unwrap_or_default();
        assert!(
            !help.contains("LeagueGameStats"),
            "the help still promises a folder the emulator has not written to for two \
             releases: {help}"
        );
        assert!(
            help.contains("one folder"),
            "the help has to say --from narrows the sweep to one folder, because the \
             default is now every folder: {help}"
        );

        let src = include_str!("main.rs");
        let from_fn = src
            .find("\nfn stats_ingest(")
            .expect("stats_ingest is not in this file any more");
        let rest = &src[from_fn..];
        let body = &rest[..rest.find("\n/// Put stalled").unwrap_or(rest.len())];
        assert!(
            body.contains("ingest_all"),
            "with no --from, stats ingest still reads one folder the emulator never \
             writes to, so it can never find a stuck capture"
        );
        assert!(
            body.contains("ingest_from"),
            "--from no longer narrows the sweep to the one folder it names"
        );
    }

    /// Read-only subcommands must not grow an `--apply`, because a flag that
    /// does nothing teaches people to pass it everywhere.
    #[test]
    fn read_only_subcommands_have_no_apply() {
        let root = Cli::command();
        for path in [
            vec!["status"],
            vec!["sessions"],
            vec!["devices"],
            vec!["drives"],
            vec!["verify"],
            vec!["onboarding"],
            vec!["music"],
            vec!["mods", "list"],
            vec!["mods", "verify"],
            vec!["stats", "list"],
        ] {
            let cmd = find(&root, &path);
            assert!(
                !cmd.get_arguments().any(|a| a.get_id() == "apply"),
                "`{}` reads only and offers --apply",
                path.join(" ")
            );
        }
    }

    /// Parsed, not just declared. The flag has to default to off in the value
    /// the dispatch actually sees.
    #[test]
    fn apply_is_off_unless_it_is_given() {
        let off = Cli::try_parse_from(["rebornctl", "mods", "apply", "--id", "a.b"])
            .expect("parses without --apply");
        match off.command {
            Command::Mods {
                what: ModsCommand::Apply { apply, .. },
            } => assert!(!apply, "--apply defaulted to on"),
            other => panic!("parsed as {other:?}"),
        }

        let on = Cli::try_parse_from(["rebornctl", "mods", "apply", "--id", "a.b", "--apply"])
            .expect("parses with --apply");
        match on.command {
            Command::Mods {
                what: ModsCommand::Apply { apply, .. },
            } => assert!(apply, "--apply was ignored"),
            other => panic!("parsed as {other:?}"),
        }

        let migrate =
            Cli::try_parse_from(["rebornctl", "migrate"]).expect("migrate parses with no flags");
        match migrate.command {
            Command::Migrate { apply, .. } => assert!(!apply),
            other => panic!("parsed as {other:?}"),
        }
    }

    /// Retention is a policy and this tool does not own one.
    ///
    /// A default here would delete a player's box scores on a schedule nobody
    /// chose, and the only copy of a result that has not been sent is the one on
    /// disk.
    #[test]
    fn pruning_has_no_default_retention() {
        let root = Cli::command();
        let prune = find(&root, &["stats", "prune"]);
        let keep = prune
            .get_arguments()
            .find(|a| a.get_id() == "keep_days")
            .expect("prune takes keep-days");
        assert!(
            keep.get_default_values().is_empty(),
            "keep-days has a default, so a retention window was invented"
        );
        assert!(
            Cli::try_parse_from(["rebornctl", "stats", "prune", "--apply"]).is_err(),
            "pruning without --keep-days was accepted"
        );
    }

    /// A release pins a manifest at an address. Writing one with a guessed
    /// address produces a release every client refuses.
    #[test]
    fn a_release_cannot_be_written_without_the_url_it_pins() {
        let parsed = Cli::try_parse_from([
            "rebornctl",
            "inventory",
            "--root",
            ".",
            "--component",
            "runtime",
            "--version",
            "1.0.0",
            "--sequence",
            "1",
            "--release-out",
            "release.json",
        ])
        .expect("clap accepts the pair, the command refuses it");
        match parsed.command {
            Command::Inventory {
                release_out,
                pin_url,
                ..
            } => {
                assert!(release_out.is_some());
                assert!(
                    pin_url.is_none(),
                    "the test is about the case where the url is absent"
                );
            }
            other => panic!("parsed as {other:?}"),
        }
    }

    /// The digits are the measurement. Grouping must not change them.
    #[test]
    fn grouping_a_number_preserves_it() {
        for n in [
            0u64,
            1,
            9,
            10,
            999,
            1_000,
            1_001,
            27_025,
            1_459_978_240,
            2_077_362_716,
            u64::MAX,
        ] {
            let grouped = thousands(n);
            let stripped: String = grouped.chars().filter(|c| *c != ',').collect();
            assert_eq!(stripped, n.to_string(), "grouping changed {n}");
        }
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(1_000), "1,000");
        assert_eq!(thousands(2_077_362_716), "2,077,362,716");
    }

    /// Migration reads the 2.1 folder and writes the data root. A candidate that
    /// is the data root, or holds it, would make migration a copy of a tree onto
    /// itself, with the launcher's own `mods/` sitting on the player's JSGME
    /// `MODS/` on a case insensitive filesystem.
    #[test]
    fn no_migration_source_is_the_data_root() {
        let platform = Platform::detect();
        let data = platform.paths.data_root().expect("data root");
        for candidate in legacy_candidates(&platform).expect("candidates") {
            assert_ne!(candidate, data, "a candidate is the data root itself");
            assert!(
                !data.starts_with(&candidate),
                "{} holds the data root {}",
                candidate.display(),
                data.display()
            );
            assert!(
                !candidate.starts_with(&data),
                "{} is inside the data root {}",
                candidate.display(),
                data.display()
            );
        }
    }

    /// The 2.1 install is looked for where it actually is.
    ///
    /// This is the bug this list was widened to fix: the shipped build lives
    /// beside the launcher's data root, and only checking beside the install root
    /// meant `migrate` reported nothing to do on a machine holding 35,015 files
    /// of it.
    #[test]
    fn the_sibling_of_the_data_root_is_a_candidate() {
        let platform = Platform::detect();
        let data = platform.paths.data_root().expect("data root");
        let expected = data
            .parent()
            .expect("the data root has a parent")
            .join("NFL Street Reborn");
        let candidates = legacy_candidates(&platform).expect("candidates");
        assert!(
            candidates.contains(&expected),
            "{} is not among {:?}",
            expected.display(),
            candidates
        );
    }

    /// Build ids come off the disk. A name that cannot be one is reported rather
    /// than dropped, because a build that is silently unrecognized makes every
    /// texture in every mod unimportable with nothing saying why.
    #[test]
    fn build_ids_are_read_off_disk_and_bad_names_are_named() {
        let dir = TempDir::new("builds");
        for name in ["GN7E69", "FR4N31", "not a build id", "v2"] {
            std::fs::create_dir_all(dir.path().join(name)).expect("mkdir");
        }
        std::fs::write(dir.path().join("readme.txt"), b"not a directory").expect("write");

        let (ids, unusable) = recognized_builds(dir.path());
        let names: Vec<&str> = ids.iter().map(GameId::as_str).collect();
        assert_eq!(names, vec!["FR4N31", "GN7E69", "v2"]);
        assert_eq!(unusable, vec!["not a build id".to_string()]);

        let (none, _) = recognized_builds(&dir.path().join("absent"));
        assert!(
            none.is_empty(),
            "a directory that is not there is not a build list"
        );
    }

    fn test_package(id: &str, version: &str) -> ModPackage {
        ModPackage {
            schema: reborn_core::mods::MOD_SCHEMA.to_string(),
            magic: reborn_core::mods::MOD_MAGIC.to_string(),
            id: ModId::parse(id).expect("test id"),
            name: "Test".to_string(),
            version: PackageVersion::parse(version).expect("test version"),
            targets: vec![GameId::parse("GN7E69").expect("test build")],
            conflict_group: None,
            files: Vec::new(),
        }
    }

    /// An activation list naming a package that is not installed must be
    /// reported.
    ///
    /// Composition has no field for it: the plan is built from the packages it
    /// was handed, so a missing one silently produces a tree without a mod the
    /// user turned on, and the launcher would report success.
    #[test]
    fn an_active_package_that_is_not_installed_is_reported() {
        let installed = vec![test_package("reborn.ball.purple", "1.0.0")];
        let mut activation = Activation::default();
        activation.activate(
            ModId::parse("reborn.ball.purple").expect("id"),
            PackageVersion::parse("1.0.0").expect("version"),
        );
        activation.activate(
            ModId::parse("reborn.ball.aqua").expect("id"),
            PackageVersion::parse("1.0.0").expect("version"),
        );

        let (ordered, missing) = active_packages(&activation, &installed);
        assert_eq!(ordered.len(), 1, "only the installed one can be composed");
        assert_eq!(missing, vec!["reborn.ball.aqua 1.0.0".to_string()]);
    }

    /// A version that is installed under a different number is not a match.
    ///
    /// One id at two versions is two payloads, and quietly composing the wrong
    /// one would put files in the tree that no record describes.
    #[test]
    fn a_different_version_is_not_the_same_package() {
        let installed = vec![test_package("reborn.ball.purple", "2.0.0")];
        let mut activation = Activation::default();
        activation.activate(
            ModId::parse("reborn.ball.purple").expect("id"),
            PackageVersion::parse("1.0.0").expect("version"),
        );
        let (ordered, missing) = active_packages(&activation, &installed);
        assert!(ordered.is_empty());
        assert_eq!(missing, vec!["reborn.ball.purple 1.0.0".to_string()]);
    }

    /// The estimate that stops a long scan looking like a hang has to count the
    /// whole tree, and must not follow a link out of it.
    #[test]
    fn measuring_a_tree_counts_every_file_below_it() {
        let dir = TempDir::new("measure");
        std::fs::create_dir_all(dir.path().join("a").join("b")).expect("mkdir");
        std::fs::write(dir.path().join("one.dds"), [0u8; 10]).expect("write");
        std::fs::write(dir.path().join("a").join("two.dds"), [0u8; 20]).expect("write");
        std::fs::write(dir.path().join("a").join("b").join("three.dds"), [0u8; 30]).expect("write");

        let size = measure_tree(dir.path());
        assert_eq!(size.files, 3);
        assert_eq!(size.bytes, 60);
        assert_eq!(size.unreadable, 0);

        let absent = measure_tree(&dir.path().join("nope"));
        assert_eq!(absent.files, 0);
        assert_eq!(
            absent.unreadable, 1,
            "a directory that could not be read must be counted, or the estimate \
             understates the wait"
        );
    }

    /// The one step migration refuses, and the two it must never refuse.
    ///
    /// Copying the live texture tree while mods are applied files modded files
    /// as pristine, and every verify and composed tree afterwards trusts them.
    /// Refusing the saves step instead would be worse than the bug: the 2.1
    /// uninstaller deletes the folder those memory cards are in.
    #[test]
    fn only_the_texture_step_waits_for_mods_to_be_un_applied() {
        assert!(step_is_refused(true, DataDir::LibraryTextures));
        assert!(!step_is_refused(false, DataDir::LibraryTextures));
        for safe in [DataDir::Saves, DataDir::Screenshots] {
            assert!(
                !step_is_refused(true, safe),
                "{safe:?} carries irreplaceable data and must always move"
            );
        }
    }
}
