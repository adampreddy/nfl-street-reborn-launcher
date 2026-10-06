//! Import a player's disc by its files, independent of container and disc layout.
use std::{fs::File, io::{Read,Write}, path::{Path,PathBuf}, process::Command};
use serde::{Deserialize,Serialize};
#[derive(Debug,Clone,PartialEq,Eq,Serialize)]
pub struct ImportProgress { pub stage:String, pub percent:Option<u8> }
use sha2::{Digest,Sha256};

#[derive(Debug, Deserialize)]
pub struct SourceFile { pub path:String, pub bytes:u64, pub sha256:String }
#[derive(Debug, Deserialize)]
pub struct PatchManifest {
    pub schema:String, pub disc_id:String, pub source_files:Vec<SourceFile>,
    pub source_sha256:String, pub output_sha256:String, pub output_bytes:u64, pub patch:String,
}
pub fn hash(path:&Path)->Result<String,String> { hash_progress(path, |_|{}) }
pub fn hash_progress(path:&Path,progress:impl Fn(u64))->Result<String,String> {
    let mut read=0;
    let mut file=File::open(path).map_err(|e|e.to_string())?;
    let mut hash=Sha256::new();let mut buf=vec![0;1024*1024];
    loop {let n=file.read(&mut buf).map_err(|e|e.to_string())?;if n==0{break}hash.update(&buf[..n]);read+=n as u64;progress(read);}
    Ok(hex::encode(hash.finalize()))
}
fn run(exe:&Path,args:&[&std::ffi::OsStr],log:&Path,tick:impl Fn())->Result<(),String> {
    let mut cmd=Command::new(exe);cmd.args(args);
    #[cfg(windows)] {use std::os::windows::process::CommandExt;cmd.creation_flags(0x08000000);}
    cmd.stdout(std::process::Stdio::null()).stderr(File::create(log).map_err(|e|e.to_string())?);
    let mut child=cmd.spawn().map_err(|e|format!("Could not start {}: {e}",exe.display()))?;
    let status=loop {
        match child.try_wait() {
            Ok(Some(status))=>break status,
            Ok(None)=>{tick();std::thread::sleep(std::time::Duration::from_millis(150));},
            Err(e)=>{let _=child.kill();let _=child.wait();return Err(e.to_string());}
        }
    };
    if !status.success(){return Err(format!("{} failed: {}",exe.file_name().unwrap().to_string_lossy(),std::fs::read_to_string(log).unwrap_or_default()));}
    Ok(())
}
fn contained(root:&Path,relative:&str)->Result<PathBuf,String> {
    let p=Path::new(relative);
    if p.components().any(|c|!matches!(c,std::path::Component::Normal(_))){return Err("The patch contains an invalid file path.".into());}
    Ok(root.join(p))
}
pub fn import(bundle:&Path,source:&Path,data:&Path,progress:impl Fn(&str))->Result<PathBuf,String> {
    import_with_progress(bundle,source,data,|p|progress(&p.stage))
}
pub fn import_with_progress(bundle:&Path,source:&Path,data:&Path,progress:impl Fn(ImportProgress))->Result<PathBuf,String> {
    import_with_tools(bundle,bundle,source,data,progress)
}
/// `bundle` holds the patch manifest and `reborn.xdelta`; `tools` holds
/// DolphinTool and xdelta3. One folder on Windows (the installer's
/// `game-import`), two on Linux (the content component, and the AppImage's
/// own `usr/bin`).
pub fn import_with_tools(bundle:&Path,tools:&Path,source:&Path,data:&Path,progress:impl Fn(ImportProgress))->Result<PathBuf,String> {
    let last=std::cell::RefCell::new(None);
    let report=|stage:&str,percent:Option<u8>| {
        let update=ImportProgress{stage:stage.into(),percent};
        if last.borrow().as_ref()!=Some(&update) {
            *last.borrow_mut()=Some(update.clone());
            progress(update);
        }
    };
    let manifest:PatchManifest=serde_json::from_slice(&std::fs::read(bundle.join("manifest.json"))
        .map_err(|_|"The game patch is not installed. Install the complete Reborn package.")?).map_err(|e|e.to_string())?;
    if manifest.schema!="nsr.game-patch.v1" || manifest.disc_id!="GN7E69" || manifest.source_files.is_empty()
        || manifest.output_sha256.len()!=64 || !manifest.output_sha256.bytes().all(|b|b.is_ascii_hexdigit()) {
        return Err("The installed game patch is invalid.".into());
    }
    let patch=contained(bundle,&manifest.patch)?;
    let nonce=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let work=data.join("cache").join(format!("game-import-{nonce}"));
    std::fs::create_dir_all(&work).map_err(|e|e.to_string())?;
    struct Scratch(PathBuf);impl Drop for Scratch{fn drop(&mut self){let _=std::fs::remove_dir_all(&self.0);}}
    let _scratch=Scratch(work.clone());
    let extracted=work.join("disc");
    report("Reading your disc…",None);
    run(&tools.join(format!("DolphinTool{}",std::env::consts::EXE_SUFFIX)),&["extract".as_ref(),"-i".as_ref(),source.as_os_str(),"-o".as_ref(),extracted.as_os_str(),"-q".as_ref()],&work.join("extract.log"),||{
        let total=manifest.source_files.iter().map(|f|f.bytes).sum::<u64>().max(1);
        let read=manifest.source_files.iter().map(|f|std::fs::metadata(extracted.join(&f.path)).map(|m|m.len().min(f.bytes)).unwrap_or(0)).sum::<u64>();
        if read>0{report("Reading your disc…",Some((19*read/total) as u8));}
    })?;
    let boot=std::fs::read(extracted.join("sys/boot.bin")).map_err(|_|"This is not a readable GameCube disc.")?;
    if boot.get(..6)!=Some(b"GN7E69".as_slice()){return Err("Choose NFL Street 2 (USA) for GameCube. This is a different game or region.".into());}
    report("Checking NFL Street 2 (USA)…",Some(20));
    let total=manifest.source_files.iter().map(|f|f.bytes).sum::<u64>().max(1);
    let mut checked=0;
    let canonical=work.join("source.bin");
    let mut out=File::create(&canonical).map_err(|e|e.to_string())?;
    for item in &manifest.source_files {
        let file=contained(&extracted,&item.path)?;
        if std::fs::metadata(&file).map_err(|_|format!("The disc is missing {}.",item.path))?.len()!=item.bytes || hash_progress(&file,|n|report("Checking NFL Street 2 (USA)…",Some((20+35*(checked+n)/total) as u8)))?!=item.sha256 {
            return Err(format!("{} does not match the supported USA release. Choose an unmodified copy of NFL Street 2.",item.path));
        }
        std::io::copy(&mut File::open(&file).map_err(|e|e.to_string())?,&mut out).map_err(|e|e.to_string())?;
        checked+=item.bytes;
    }
    out.flush().map_err(|e|e.to_string())?;drop(out);
    if hash_progress(&canonical,|n|report("Preparing the patch…",Some((55+10*n/total) as u8)))?!=manifest.source_sha256{return Err("The verified source could not be prepared.".into());}
    report("Applying patch...",Some(65));
    let output=work.join("NSR.iso");
    run(&tools.join(format!("xdelta3{}",std::env::consts::EXE_SUFFIX)),&["-d".as_ref(),"-s".as_ref(),canonical.as_os_str(),patch.as_os_str(),output.as_os_str()],&work.join("patch.log"),||{
        let bytes=std::fs::metadata(&output).map(|m|m.len().min(manifest.output_bytes)).unwrap_or(0);
        report("Applying patch...",Some((65+14*bytes/manifest.output_bytes.max(1)) as u8));
    })?;
    report("Verifying your game…",Some(80));
    if std::fs::metadata(&output).map_err(|e|e.to_string())?.len()!=manifest.output_bytes || hash_progress(&output,|n|report("Verifying your game…",Some((80+18*n/manifest.output_bytes.max(1)) as u8)))?!=manifest.output_sha256 {
        return Err("The patched game did not pass verification. Your original game has not been changed.".into());
    }
    let destination=data.join("library/imported").join(&manifest.output_sha256).join("NSR.iso");
    std::fs::create_dir_all(destination.parent().unwrap()).map_err(|e|e.to_string())?;
    if destination.exists() {
        if hash(&destination)?!=manifest.output_sha256{return Err("An existing imported game was changed. It has been left untouched.".into());}
    } else {std::fs::rename(&output,&destination).map_err(|e|e.to_string())?;}
    report("Saving your game…",Some(99));
    Ok(destination)
}

#[cfg(test)]mod tests {
    use super::*;
    #[test]fn patch_paths_cannot_escape(){
        for path in ["../save.raw","/absolute"]{assert!(contained(Path::new("bundle"),path).is_err());}
        // A drive letter is only a root on Windows. On Linux `C:/save.raw` is a
        // relative path into a folder called `C:`, which stays inside the bundle.
        if cfg!(windows){assert!(contained(Path::new("bundle"),"C:/save.raw").is_err());}
    }
}
