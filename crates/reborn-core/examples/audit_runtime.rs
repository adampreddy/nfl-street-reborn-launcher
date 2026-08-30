//! Run the runtime packaging audit against a real folder.
//!
//! ```text
//! cargo run -p reborn-core --example audit_runtime -- "<path to runtime>"
//! ```
//!
//! Exists so the audit can be pointed at the actual selected runtime rather
//! than only at fixtures. A rule that passes its own tests and misses the real
//! folder is not a rule.

use reborn_core::runtime::RuntimePackager;

fn main() {
    let Some(dir) = std::env::args().nth(1) else {
        eprintln!("usage: audit_runtime <path to a Reborn Dolphin runtime folder>");
        std::process::exit(2);
    };
    let path = std::path::Path::new(&dir);

    if !path.is_dir() {
        eprintln!("not a directory: {}", path.display());
        std::process::exit(2);
    }

    println!("Auditing {}\n", path.display());

    match RuntimePackager::audit(path) {
        Ok(v) if v.is_empty() => {
            println!("SHIPPABLE. No forbidden files found.");
        }
        Ok(violations) => {
            println!("NOT SHIPPABLE. {} problem(s):\n", violations.len());
            for v in &violations {
                println!("  {}", v.path);
                println!("      {}\n", v.reason);
            }
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("audit failed: {e}");
            std::process::exit(2);
        }
    }
}
