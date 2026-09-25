//! Live check: can this PC reach the NetPlay traversal server? Prints yes/no and the time taken.
//! Run: cargo run -p reborn-core --example traversal_probe
fn main() {
    let t = std::time::Instant::now();
    let ok = reborn_core::traversal_probe::probe(reborn_core::traversal_probe::DEFAULT_SERVER, std::time::Duration::from_millis(1500));
    println!("traversal reachable: {ok} ({} ms)", t.elapsed().as_millis());
}
