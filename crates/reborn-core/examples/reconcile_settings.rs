//! Run the settings-law repair over a copy of a real Dolphin user folder and
//! report exactly what it changed.
//!
//! This exists because the repair edits files a player cannot get back. A unit
//! test against a fixture proves the logic; this proves it against the real
//! thing, including the Gecko codes, the hand-tuned keys and whatever else has
//! accumulated in an install that has been through every shipped build.
//!
//! Usage: cargo run -p reborn-core --example reconcile_settings -- <user dir> [game id ...]
//!
//! It writes to the folder you give it. Give it a COPY.

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(user_dir) = args.next() else {
        eprintln!("usage: reconcile_settings <user dir copy> [game id ...]");
        std::process::exit(2);
    };
    let ids: Vec<String> = args.collect();
    let ids: Vec<&str> = if ids.is_empty() {
        vec!["GN7E69", "FR4N31"]
    } else {
        ids.iter().map(String::as_str).collect()
    };

    let dir = std::path::Path::new(&user_dir);
    match reborn_core::settings_law::reconcile_in(dir, &ids) {
        Ok(done) => {
            println!("pruned {} per-game pin(s):", done.pruned.len());
            for key in &done.pruned {
                println!("  - {key}");
            }
            println!("restored {} config key(s) to the player's own value:", done.restored.len());
            for key in &done.restored {
                println!("  - {key}");
            }
            println!("repaired {} config key(s) with no surviving evidence:", done.repaired.len());
            for key in &done.repaired {
                println!("  - {key}");
            }
            println!("files rewritten: {}", done.files.len());
            for path in &done.files {
                println!("  - {}", path.display());
            }
        }
        Err(error) => {
            eprintln!("reconcile failed: {error}");
            std::process::exit(1);
        }
    }
}
