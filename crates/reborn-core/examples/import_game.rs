//! Exercise the production import pipeline in an isolated directory.
fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    assert_eq!(args.len(),3,"usage: import_game BUNDLE SOURCE SCRATCH_DATA");
    let result=reborn_core::game_import::import(std::path::Path::new(&args[0]),std::path::Path::new(&args[1]),std::path::Path::new(&args[2]),|stage|println!("{stage}"));
    match result { Ok(path)=>println!("VERIFIED: {}",path.display()),Err(e)=>{eprintln!("{e}");std::process::exit(1);} }
}
