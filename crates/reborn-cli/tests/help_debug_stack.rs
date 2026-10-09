//! Runs the compiled binary, so clap parses on the process's real main
//! stack rather than a test thread's. Adding subcommands once pushed an
//! unoptimized `rebornctl --help` over the 1 MiB main-thread stack.

use std::process::Command;

#[test]
fn help_exits_zero_for_the_top_level_and_the_big_subcommands() {
    for args in [
        vec!["--help"],
        vec!["publish", "--help"],
        vec!["publish", "league-game", "--help"],
        vec!["publish", "league-content", "--help"],
        vec!["publish", "roster", "--help"],
    ] {
        let out = Command::new(env!("CARGO_BIN_EXE_rebornctl")).args(&args).output().unwrap();
        assert!(
            out.status.success(),
            "rebornctl {args:?} exited {:?}: {}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(!out.stdout.is_empty(), "rebornctl {args:?} printed no help");
    }
}
