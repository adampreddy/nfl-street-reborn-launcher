//! Inspect and clear credentials the launcher has stored.
//!
//! ```text
//! cargo run -p reborn-core --example credentials -- check <name>
//! cargo run -p reborn-core --example credentials -- delete <name>
//! ```
//!
//! # Why this exists
//!
//! A test in `lib.rs` used to assert that the credential seam was an
//! unimplemented stub, and called `store("discord", "secret")` to prove it
//! returned an error. When the seam became real that call started succeeding,
//! so every full test run wrote a credential named
//! `discord.NFL Street Reborn Launcher` holding the literal string `secret`
//! into the developer's own Windows Credential Manager.
//!
//! That is worse than clutter. The launcher will later read `discord` expecting
//! a real token, find `secret`, and fail somewhere far away from the cause.
//!
//! `cmdkey /delete` refuses the target name, which contains spaces, so the
//! reliable way to remove it is through the same API that created it: keyring
//! composes the service and account into that target string itself, and asking
//! it to delete guarantees the name matches whatever it wrote.
//!
//! Kept rather than thrown away, because anything that can store a secret
//! should come with a way to see and remove it.

use reborn_core::Platform;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let platform = Platform::detect();

    let (action, name) = match (args.first().map(String::as_str), args.get(1)) {
        (Some(a), Some(n)) => (a, n.clone()),
        _ => {
            eprintln!("usage: credentials <check|delete> <name>");
            eprintln!();
            eprintln!("names this project is known to have written:");
            eprintln!("  discord               from the old stub test");
            eprintln!("  nsr-launcher-selftest from the current persistence test");
            std::process::exit(2);
        }
    };

    match action {
        "check" => match platform.credentials.retrieve(&name) {
            Ok(Some(v)) => {
                // The length, never the value. Printing a secret to a terminal
                // that may be logged would be its own problem.
                println!("{name}: present, {} byte(s)", v.len());
            }
            Ok(None) => println!("{name}: absent"),
            Err(e) => {
                eprintln!("{name}: could not be read: {e}");
                std::process::exit(1);
            }
        },
        "delete" => match platform.credentials.delete(&name) {
            Ok(()) => {
                println!("{name}: deleted");
                // Confirm rather than trust. A delete that silently did nothing
                // is the failure this whole example exists because of.
                match platform.credentials.retrieve(&name) {
                    Ok(None) => println!("{name}: confirmed gone"),
                    Ok(Some(_)) => {
                        eprintln!("{name}: STILL PRESENT after delete");
                        std::process::exit(1);
                    }
                    Err(e) => eprintln!("{name}: could not confirm: {e}"),
                }
            }
            Err(e) => {
                eprintln!("{name}: could not be deleted: {e}");
                std::process::exit(1);
            }
        },
        other => {
            eprintln!("unknown action {other:?}. Use check or delete.");
            std::process::exit(2);
        }
    }
}
