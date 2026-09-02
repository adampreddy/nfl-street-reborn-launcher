//! Walk first run end to end, against this machine.
//!
//! The tests prove each stage in isolation with a fake journal. This walks the
//! whole flow once, in order, using the real device scan and the real mod
//! library, and prints what the Ready screen would print. It exists because
//! both of the data-loss bugs found so far were found by running against the
//! real machine rather than by a test, and because a flow that passes stage by
//! stage can still be impossible to complete in sequence.
//!
//! Writes into a temporary directory and deletes it. It cannot touch a real
//! first run journal, so it is safe to run at any time.
//!
//!     cargo run -p reborn-core --example onboarding_walk

use reborn_core::devices::DeviceScanner;
use reborn_core::mods::ModPackage;
use reborn_core::onboarding::{
    Advance, Environment, Next, Onboarding, PrepTask, RecordedPreset, Satisfaction, Stage,
};
use reborn_core::platform::Platform;
use reborn_core::supervisor::SystemClock;

fn main() {
    let dir = std::env::temp_dir().join(format!("reborn-onboarding-walk-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("could not make a scratch directory");

    let platform = Platform::detect();
    let mut flow = Onboarding::open_at(&dir, Environment::nothing_found(), SystemClock)
        .expect("could not open a journal in the scratch directory");

    println!("Walking first run in {}\n", dir.display());

    // ---- welcome ----------------------------------------------------------
    step(&mut flow, "reach welcome", |f| f.reach(Stage::Welcome));
    step(&mut flow, "acknowledge", |f| {
        f.satisfy(Stage::Welcome, Satisfaction::Acknowledged)
    });

    // ---- the game image ---------------------------------------------------
    //
    // Weighed, not asserted. Nothing here knows where the image is, so this
    // reports it as unmeasured, which is what the Ready screen then prints.
    step(&mut flow, "confirm game", |f| {
        f.satisfy(
            Stage::GameVerification,
            Satisfaction::GameConfirmed { total_bytes: None },
        )
    });

    // ---- graphics ---------------------------------------------------------
    step(&mut flow, "choose graphics", |f| {
        f.satisfy(
            Stage::SystemCheck,
            Satisfaction::Graphics {
                chosen: RecordedPreset::Balanced,
                // Absent on purpose: nothing measures this machine yet.
                recommended: None,
                detected: None,
            },
        )
    });

    // ---- the controller, from a real scan ---------------------------------
    let scan = DeviceScanner::scan();
    println!(
        "  scan: {} playable pad(s), search {}",
        scan.playable_pads().len(),
        if scan.is_complete() {
            "complete"
        } else {
            "PARTIAL, XInput only"
        }
    );
    match Satisfaction::controller_on_port(&scan, 0) {
        Ok(answer) => {
            if let Satisfaction::Controller { device, family, .. } = &answer {
                println!("  pad on port 1: {device}  family {family:?}");
            }
            step(&mut flow, "accept controller", |f| {
                f.satisfy(Stage::ControllerSetup, answer.clone())
            });
        }
        Err(why) => {
            // A normal, recurring state: a wireless pad goes to sleep. The
            // decline path has to work, because otherwise first run cannot be
            // finished on a machine with no pad awake.
            println!("  no pad: {why}");
            step(&mut flow, "decline controller", |f| {
                f.satisfy(
                    Stage::ControllerSetup,
                    Satisfaction::NoController {
                        because: "no pad was awake during the walk".into(),
                    },
                )
            });
        }
    }

    // ---- mods, from the real library --------------------------------------
    let library = read_library(&platform);
    println!("  library: {} package(s)", library.len());
    if library.is_empty() {
        step(&mut flow, "decline mods", |f| {
            f.satisfy(Stage::ModSelection, Satisfaction::NoMods)
        });
    } else {
        let answer = Satisfaction::mods_chosen(&library, false);
        if let Satisfaction::ModsChosen { packages, .. } = &answer {
            println!("  turning on: {}", packages.join(", "));
        }
        step(&mut flow, "choose mods", |f| {
            f.satisfy(Stage::ModSelection, answer.clone())
        });
    }

    // ---- preparation ------------------------------------------------------
    step(&mut flow, "reach preparation", |f| {
        f.reach(Stage::Preparation)
    });
    for task in PrepTask::ALL {
        step(&mut flow, task.slug(), |f| f.record_task(*task));
    }
    step(&mut flow, "prepared", |f| {
        f.satisfy(Stage::Preparation, Satisfaction::Prepared)
    });

    // ---- finish -----------------------------------------------------------
    step(&mut flow, "reach completion", |f| f.reach(Stage::Completion));
    step(&mut flow, "finish", |f| f.finish());

    // ---- what the Ready screen would say ---------------------------------
    let s = flow.summary();
    println!("\nReady screen:");
    println!("  controller        {:?}", s.controller);
    println!("  declined because  {:?}", s.no_controller_because);
    println!("  graphics          {:?}", s.graphics);
    println!("  recommended       {:?}", s.graphics_was_recommended);
    println!("  mods chosen       {:?}", s.mods_chosen);
    println!("  conflicts shown   {:?}", s.mod_conflicts_shown);
    println!("  tasks done        {}", s.tasks_done.len());
    println!("  not measured      {:?}", s.absent());

    let done = flow.is_complete();
    println!("\ncomplete: {done}");
    match flow.next() {
        Next::AlreadyComplete { flow_version } => {
            println!("next: already complete, flow version {flow_version}")
        }
        other => println!("next: {other:?}"),
    }

    let _ = std::fs::remove_dir_all(&dir);

    // A walk that did not finish is a failure, not a report. Exiting non-zero
    // means this can be run in a check without anyone reading the output.
    if !done {
        eprintln!("\nFAILED: the flow did not reach completion");
        std::process::exit(1);
    }
    println!("\nPASS: first run can be completed in order on this machine");
}

/// Run one step and print what the journal did. A refusal stops the walk, since
/// everything after it would be reporting on a flow that never got there.
fn step<F>(flow: &mut Onboarding, what: &str, f: F)
where
    F: FnOnce(&mut Onboarding) -> reborn_core::Result<Advance>,
{
    match f(flow) {
        Ok(Advance::Recorded) => println!("  ok        {what}"),
        Ok(Advance::AlreadyRecorded) => println!("  already   {what}"),
        Ok(Advance::Refused(why)) => {
            eprintln!("  REFUSED   {what}: {}", why.message());
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("  ERROR     {what}: {e}");
            std::process::exit(1);
        }
    }
}

/// Every package in the launcher's own library, or an empty list if there is no
/// library yet. An unreadable package is skipped rather than fatal: this walk is
/// about the flow, not about the library's health.
fn read_library(platform: &Platform) -> Vec<ModPackage> {
    let Ok(root) = reborn_core::mods::packages_root(platform) else {
        return Vec::new();
    };
    let Ok(ids) = std::fs::read_dir(&root) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for id in ids.flatten() {
        let Ok(versions) = std::fs::read_dir(id.path()) else {
            continue;
        };
        for version in versions.flatten() {
            if let Ok(package) = ModPackage::load(&version.path()) {
                out.push(package);
            }
        }
    }
    out
}
