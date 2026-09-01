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

use std::path::PathBuf;

use clap::{Parser, Subcommand};

use reborn_core::devices::DeviceScanner;
use reborn_core::migration::{LegacyInstall, MigrationPlan};
use reborn_core::runtime::{GameBuild, GraphicsPreset, RuntimePackager};
use reborn_core::session::{build_gcpad, LaunchRequest, Launched, Session};
use reborn_core::supervisor::{SessionEnd, Startup, Supervisor};
use reborn_core::{DataDir, Platform};

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
}

fn main() -> std::process::ExitCode {
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
        } => play(&platform, &iso, &runtime, &preset, !windowed, dry_run),
    }
}

const OK: std::process::ExitCode = std::process::ExitCode::SUCCESS;
const PROBLEM: std::process::ExitCode = std::process::ExitCode::FAILURE;

fn status(platform: &Platform) -> reborn_core::Result<std::process::ExitCode> {
    println!("Platform      {}", reborn_core::CURRENT_PLATFORM);

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
    let root = match from {
        Some(p) => p,
        None => default_legacy_root(platform)?,
    };

    let Some(install) = LegacyInstall::detect(&root) else {
        println!("No 2.1 install found at {}", root.display());
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

    if let Some(reason) = plan.blocked {
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
    for step in &plan.steps {
        let dest = data.join(step.to.relative());
        print!("{}. {} ... ", step.order, step.what);
        let copied = reborn_core::migration::copy_tree_no_clobber(&step.from, &dest)?;
        total += copied;
        println!("{copied} file(s)");
    }
    println!();
    println!("Copied {total} file(s). Nothing in the 2.1 install was changed or removed.");
    Ok(OK)
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
    };

    // A dry run writes the configuration and starts nothing, which is the same
    // sequence up to the spawn. Sharing it is the point: a dry run that
    // exercised different code would prove nothing about a real launch.
    if dry_run {
        let written = Session::prepare(platform, &req)?;
        for f in &written.files {
            println!("wrote {}", f.display());
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
            ..
        } => {
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

/// Where the 2.1 installer puts things by default.
fn default_legacy_root(platform: &Platform) -> reborn_core::Result<PathBuf> {
    // 2.1 is a Windows-only Inno Setup package, so this is the only layout that
    // has ever existed. On other platforms the answer is "there isn't one",
    // and --from covers anybody who moved it.
    Ok(platform
        .paths
        .install_root()?
        .parent()
        .map(|p| p.join("NFL Street Reborn Xbox Dev Mode 2.1"))
        .unwrap_or_else(|| PathBuf::from("NFL Street Reborn Xbox Dev Mode 2.1")))
}

fn yes_no(b: bool) -> &'static str {
    if b {
        "yes"
    } else {
        "no"
    }
}
