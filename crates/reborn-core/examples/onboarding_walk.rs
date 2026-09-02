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
use reborn_core::controller::ControlsProfile;
use reborn_core::mods::ModPackage;
use reborn_core::migration::LegacyInstall;
use reborn_core::onboarding::{
    Advance, Environment, Next, Onboarding, PrepTask, RecordedPreset, Satisfaction, Stage,
};
use reborn_core::platform::Platform;
use std::path::PathBuf;
use reborn_core::supervisor::SystemClock;

fn main() {
    let dir = std::env::temp_dir().join(format!("reborn-onboarding-walk-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("could not make a scratch directory");

    let platform = Platform::detect();

    // The real 2.1 install, when there is one. With it the flow is 9 stages
    // rather than 7, and the two extra ones are the ones that were stranding
    // people: migration_offer could not be answered from any interface, so the
    // flow dead ended on step 3 of 9.
    let legacy = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .or_else(|| {
            let guess = platform
                .paths
                .data_root()
                .ok()?
                .parent()?
                .join("NFL Street Reborn");
            guess.is_dir().then_some(guess)
        })
        .and_then(|root| LegacyInstall::detect(&root));

    let env = match &legacy {
        Some(install) => {
            println!("2.1 install found at {}", install.root.display());
            Environment::with_legacy_install(install.clone())
        }
        None => {
            println!("No 2.1 install found, so the migration stages do not apply.");
            Environment::nothing_found()
        }
    };

    let mut flow = Onboarding::open_at(&dir, env, SystemClock)
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

    // ---- migration --------------------------------------------------------
    //
    // Declined rather than accepted: accepting copies memory cards, and an
    // example that runs on demand must not move someone's saves as a side
    // effect of being run.
    if legacy.is_some() {
        step(&mut flow, "reach migration offer", |f| {
            f.reach(Stage::MigrationOffer)
        });
        step(&mut flow, "decline migration", |f| {
            f.satisfy(Stage::MigrationOffer, Satisfaction::MigrationDeclined)
        });
        // And the copy stage must now be moot rather than waiting. If it were
        // still outstanding, this is exactly where the flow would strand.
        let waiting = flow.next();
        assert!(
            !matches!(waiting, Next::Show { stage: Stage::MigrationCopy, .. }),
            "declining must make the copy moot, got {waiting:?}"
        );
        println!("  ok        migration copy is moot, not waiting");
    }

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

    // ---- the controls profile ---------------------------------------------
    //
    // Suggested from the pad that was actually found, then recorded. The
    // suggestion is the point: the common case needs no thought, and anyone who
    // wants GameCube art on an Xbox pad can still have it.
    let suggested = match flow.summary().controller_family {
        Some(family) => ControlsProfile::suggested_for(family),
        // No pad, so nothing to suggest from. The default is not a measurement.
        None => ControlsProfile::default(),
    };
    println!(
        "  controls: {} ({}), package {:?}",
        suggested.label(),
        suggested.describes(),
        suggested.package_id()
    );
    step(&mut flow, "choose controls", |f| {
        f.satisfy(
            Stage::ControlsProfile,
            Satisfaction::Controls { profile: suggested },
        )
    });

    // Mods are deliberately NOT here any more. They are not a first run
    // question: nothing is broken without an answer, and the answer changes
    // freely from the launcher afterwards.
    println!(
        "  library: {} package(s), chosen outside first run",
        read_library(&platform).len()
    );

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
    println!("  controls          {:?}", s.controls_profile);
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
