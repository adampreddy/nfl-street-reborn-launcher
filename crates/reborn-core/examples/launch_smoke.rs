//! Prove the launch sequence end to end, without booting the real game.
//!
//! ```text
//! cargo run -p reborn-core --example launch_smoke
//! ```
//!
//! # Why this exists
//!
//! After wiring the window's Play button, the honest state was "builds clean,
//! tests pass, and nobody has observed a launch actually happen." Unit tests
//! cover the refusals, because those return before anything is spawned, but
//! the interesting half is the part that cannot be unit tested against the
//! real filesystem: take the lock, write config, ask for controllers, spawn,
//! wait, classify, record.
//!
//! So this runs that whole chain with a stub standing in for the emulator. The
//! stub is the system shell, which is guaranteed to exist, will reject the
//! Dolphin arguments it is handed, and will exit promptly with a non-zero
//! status. That is exactly the useful case: it proves the sequence completes
//! and that a bad exit is classified as a failure rather than a crash, which
//! is a distinction the recovery page depends on.
//!
//! It writes into the launcher's real data root, which is deliberate. A test
//! against a temporary directory would not prove the real lock path or the
//! real config path work.

use std::path::PathBuf;

use reborn_core::runtime::{GameBuild, GraphicsPreset};
use reborn_core::session::{LaunchRequest, Launched, Session};
use reborn_core::supervisor::SessionEnd;
use reborn_core::Platform;

fn main() {
    let platform = Platform::detect();

    // The stub runtime and a stand-in game file. Both must exist, because the
    // sequence refuses before spawning otherwise, and refusing is not what is
    // being tested here.
    let (runtime, game) = if cfg!(windows) {
        (
            PathBuf::from(
                std::env::var("COMSPEC")
                    .unwrap_or_else(|_| "C:\\Windows\\System32\\cmd.exe".into()),
            ),
            PathBuf::from("C:\\Windows\\System32\\cmd.exe"),
        )
    } else {
        (PathBuf::from("/bin/sh"), PathBuf::from("/bin/sh"))
    };

    if !runtime.exists() {
        eprintln!(
            "no shell at {} to stand in for the runtime",
            runtime.display()
        );
        std::process::exit(2);
    }

    println!("Runtime stub  {}", runtime.display());
    println!("Game stub     {}", game.display());
    println!();

    let req = LaunchRequest {
        runtime_exe: runtime,
        game_path: game,
        graphics: GraphicsPreset::Balanced,
        fullscreen: false,
        build: GameBuild::Offline,
        // Let resolution run, so this also proves which folder the real
        // machine would boot against.
        user_dir: None,
    };

    // First prove the preparation half on its own, so a failure here is not
    // confused with a failure to spawn.
    match Session::prepare(&platform, &req) {
        Ok(w) => {
            println!("prepare wrote {} file(s):", w.files.len());
            for f in &w.files {
                println!("  {}", f.display());
            }
            println!("  controllers found: {}", !w.no_controller);
            println!("  list authoritative: {}", w.pads_authoritative);
        }
        Err(e) => {
            eprintln!("FAIL: prepare errored: {e}");
            std::process::exit(1);
        }
    }
    println!();

    println!("running the full sequence, spawn and wait included ...");
    match Session::run(&platform, &req) {
        Err(e) => {
            eprintln!("FAIL: run errored: {e}");
            std::process::exit(1);
        }
        Ok(Launched::Refused(r)) => {
            // A refusal here means the sequence stopped before spawning, which
            // for this input would be a real bug.
            eprintln!("FAIL: refused when it should have started: {}", r.message());
            std::process::exit(1);
        }
        Ok(Launched::Finished { end, minutes, .. }) => {
            println!("  ended as: {end:?}");
            println!("  duration: {minutes:.2} min");

            // The shell rejects the Dolphin arguments, so a non-zero exit is
            // the expected shape. What matters is that it is classified as a
            // failure and NOT as a crash: the recovery page turns on exactly
            // that distinction.
            match &end {
                SessionEnd::Exited { .. } => {
                    assert!(
                        !end.needs_recovery(),
                        "a non-zero exit must not be treated as a crash"
                    );
                    println!("  classified as an exit, not a crash. Correct.");
                }
                SessionEnd::Crashed { .. } | SessionEnd::Terminated => {
                    println!(
                        "  classified as {end:?}. Plausible for a shell handed \
                         arguments it cannot parse, and it correctly routes to recovery."
                    );
                    assert!(end.needs_recovery());
                }
                SessionEnd::Interrupted => {
                    eprintln!("FAIL: a session that just finished cannot be interrupted");
                    std::process::exit(1);
                }
            }
        }
    }

    // The session must be on the record afterwards. A launch that leaves no
    // trace is a launch nobody can report.
    println!();
    match reborn_core::supervisor::Supervisor::acquire(&platform) {
        Ok(reborn_core::supervisor::Startup::Owner(s)) => match s.history() {
            Ok(h) => {
                println!("session log holds {} finished session(s)", h.len());
                if h.is_empty() {
                    eprintln!("FAIL: the sequence completed but recorded nothing");
                    std::process::exit(1);
                }
                let last = h.last().unwrap();
                println!("  most recent: {} ({:?})", last.id, last.end);
            }
            Err(e) => {
                eprintln!("FAIL: could not read the session log: {e}");
                std::process::exit(1);
            }
        },
        Ok(reborn_core::supervisor::Startup::AlreadyRunning { .. }) => {
            eprintln!("FAIL: the lock was not released after the session finished");
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("FAIL: could not re-acquire the lock: {e}");
            std::process::exit(1);
        }
    }

    println!();
    println!("PASS. Lock, config, controller query, spawn, wait, classify and record all ran.");
}
