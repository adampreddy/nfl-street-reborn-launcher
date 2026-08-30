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

use reborn_core::controller::{ControllerProfile, DeviceFamily, DeviceIdentity, GcPadConfig};
use reborn_core::migration::{LegacyInstall, MigrationPlan};
use reborn_core::runtime::RuntimePackager;
use reborn_core::supervisor::{Startup, Supervisor};
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
        /// Write to this path instead of standard output.
        #[arg(long)]
        out: Option<PathBuf>,
        /// How many ports to configure.
        #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u8).range(1..=4))]
        ports: u8,
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
        Command::Controller { out, ports } => controller(out, ports),
        Command::Sessions => sessions(&platform),
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

fn controller(out: Option<PathBuf>, ports: u8) -> reborn_core::Result<std::process::ExitCode> {
    let profiles: Vec<ControllerProfile> = (1..=ports)
        .map(|port| {
            ControllerProfile::new(
                DeviceIdentity {
                    guid: format!("placeholder-guid-{port}"),
                    serial: None,
                    name: "Xbox Wireless Controller".to_string(),
                    family: DeviceFamily::Xbox,
                },
                port,
            )
        })
        .collect();

    // SDL device enumeration is not written yet, so the device string is a
    // placeholder. Said out loud rather than left to look real, because a
    // GCPadNew.ini with a made-up device line boots to no controller at all and
    // gives no clue why.
    let ini = GcPadConfig::generate(&profiles, |id| format!("SDL/0/{}", id.name));

    match out {
        Some(path) => {
            std::fs::write(&path, &ini).map_err(|source| reborn_core::Error::Io {
                path: path.clone(),
                source,
            })?;
            println!("Wrote {}", path.display());
            println!();
            println!("The Device lines are placeholders. Real device strings need SDL");
            println!("enumeration, which is not written yet, so this file will not bind");
            println!("a real pad.");
        }
        None => print!("{ini}"),
    }
    Ok(OK)
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
