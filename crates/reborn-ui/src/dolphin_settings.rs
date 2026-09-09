use super::*;
use reborn_core::ini::{IniDocument, Overlay};

use reborn_core::dolphin_preferences::{Field, FIELDS};
fn config_user(p: &Platform, runtime: Option<&str>) -> Result<(PathBuf,PathBuf),String> {
    let exe=runtime_for(p,runtime).ok_or("Dolphin was not found. Choose its location in Files and storage.")?;
    let prefs=reborn_core::settings::Settings::load(&reborn_core::settings::settings_path(p).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
    let user=reborn_core::session::resolve_user_dir_for(p,prefs.user_dir.as_deref(),Some(&exe)).map_err(|e|e.to_string())?;
    Ok((exe,user.path().to_path_buf()))
}
fn read_ini(path:&Path)->Result<IniDocument,String>{
    match std::fs::read_to_string(path){Ok(s)=>Ok(IniDocument::parse(&s)),Err(e) if e.kind()==std::io::ErrorKind::NotFound=>Ok(IniDocument::parse("")),Err(e)=>Err(e.to_string())}
}
fn checked_value(field:&Field,value:&str)->Result<String,String>{
    if value.len()>2048||value.chars().any(char::is_control){return Err("Enter a valid setting value.".into());}
    match field.kind{
        "bool"=>match value.to_lowercase().as_str(){"true"=>Ok("True".into()),"false"=>Ok("False".into()),_=>Err("Choose on or off.".into())},
        "volume"|"latency"=>{let n=value.parse::<u32>().map_err(|_|"Enter a whole number.")?;let max=if field.kind=="volume"{100}else{200};if n>max{return Err(format!("Choose a value from 0 to {max}."));}Ok(n.to_string())},
        "backend"=>if ["Cubeb","WASAPI (Exclusive Mode)"].contains(&value){Ok(value.into())}else{Err("Choose a supported audio output mode.".into())},
        "slot"=>if ["1","8","255"].contains(&value){Ok(value.into())}else{Err("Choose a supported memory card type.".into())},
        "card"|"folder"=>{if !value.is_empty(){let path=Path::new(value);if !path.is_absolute(){return Err("Choose an absolute path.".into());}if field.kind=="card"&&(!path.is_file()||!path.extension().is_some_and(|e|e.eq_ignore_ascii_case("raw")||e.eq_ignore_ascii_case("gcp"))){return Err("Choose an existing .raw or .gcp memory card.".into());}if field.kind=="folder"&&!path.is_dir(){return Err("Choose an existing save folder.".into());}}Ok(value.into())},
        _=>Err("Unknown setting.".into())
    }
}
// Dolphin rewrites its in-memory settings on exit. Refuse edits while it is open.
fn require_closed(exe:&Path)->Result<(),String>{
    #[cfg(windows)] {
        use std::os::windows::process::CommandExt;
        let result=std::process::Command::new("tasklist.exe").args(["/FO","CSV","/NH"]).creation_flags(0x08000000).output().map_err(|e|e.to_string())?;
        if !result.status.success(){return Err("Could not check whether Dolphin is open. Try again.".into());}
        let name=exe.file_name().unwrap_or_default().to_string_lossy().to_lowercase();
        if String::from_utf8_lossy(&result.stdout).lines().any(|line|{let process=line.split(',').next().unwrap_or("").trim_matches('"').to_lowercase();process==name||process=="dolphin.exe"}){return Err("Close Dolphin before changing its settings. Your changes have not been applied.".into());}
    }
    #[cfg(not(windows))] {let _=exe;return Err("Open Dolphin to change emulator settings on this platform.".into());}
    Ok(())
}
#[tauri::command]
pub async fn dolphin_preferences(runtime:Option<String>)->Result<serde_json::Value,String>{
    tauri::async_runtime::spawn_blocking(move||{let p=Platform::detect();let (exe,user)=config_user(&p,runtime.as_deref())?;let mut values=serde_json::Map::new();for field in FIELDS {let doc=read_ini(&user.join("Config").join(field.file))?;values.insert(field.id.into(),serde_json::json!(doc.get(field.section,field.key).unwrap_or(field.default)));}let readonly_reason=require_closed(&exe).err();Ok(serde_json::json!({"values":values,"user_dir":user,"editable":readonly_reason.is_none(),"readonly_reason":readonly_reason}))}).await.map_err(|e|e.to_string())?
}
#[tauri::command]
pub async fn dolphin_preference_set(runtime:Option<String>,key:String,value:String)->Result<(),String>{
    tauri::async_runtime::spawn_blocking(move||{let _guard=LaunchGuard::acquire()?;let p=Platform::detect();let (exe,user)=config_user(&p,runtime.as_deref())?;require_closed(&exe)?;let field=FIELDS.iter().find(|f|f.id==key).ok_or("Unknown setting.")?;let value=checked_value(field,&value)?;let file=user.join("Config").join(field.file);let mut doc=read_ini(&file)?;let mut overlay=Overlay::new();overlay.entry(field.section.into()).or_default().insert(field.key.into(),value.clone());if !doc.merge(&overlay).is_noop(){std::fs::create_dir_all(file.parent().unwrap()).map_err(|e|e.to_string())?;std::fs::write(&file,doc.render()).map_err(|e|e.to_string())?;}let settings_file=reborn_core::settings::settings_path(&p).map_err(|e|e.to_string())?;let mut settings=reborn_core::settings::Settings::load(&settings_file).map_err(|e|e.to_string())?;settings.emulator_preferences.insert(key,value);settings.save(&settings_file).map_err(|e|e.to_string())?;Ok(())}).await.map_err(|e|e.to_string())?
}
#[tauri::command]
pub async fn dolphin_saves_folder(runtime:Option<String>)->Result<(),String>{
    let p=Platform::detect();let (_,user)=config_user(&p,runtime.as_deref())?;let dir=user.join("GC");std::fs::create_dir_all(&dir).map_err(|e|e.to_string())?;open_in_file_manager(&p,&dir)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn online_cards_and_save_writes_are_session_only() {
        let options=netplay_session_options("Player", "join", "direct", "127.0.0.1",2626);
        for required in ["Dolphin.Core.SlotA=255","Dolphin.Core.SlotB=255","Dolphin.NetPlay.WriteSaveData=False","Dolphin.NetPlay.SyncSaves=True","Dolphin.NetPlay.SyncCodes=True","Dolphin.NetPlay.StrictSettingsSync=True"] { assert!(options.iter().any(|v|v==required)); }
        assert!(options.iter().all(|v|!v.contains("MemcardAPath")&&!v.contains("MemcardBPath")));
    }
    #[test]fn settings_are_validated_and_protected(){assert!(FIELDS.iter().all(|f|!["InternalResolution","MSAA","SyncGPU","MEM1Size","RAMOverrideEnable","EFBAccessEnable"].contains(&f.key)));for id in ["volume","audio_latency"]{let f=FIELDS.iter().find(|f|f.id==id).unwrap();assert!(checked_value(f,"-1").is_err());assert!(checked_value(f,"999").is_err());assert!(checked_value(f,"20").is_ok());}let f=FIELDS.iter().find(|f|f.id=="muted").unwrap();assert_eq!(checked_value(f,"true").unwrap(),"True");assert!(checked_value(f,"true\n[Core]").is_err());}
    #[test]fn editing_one_setting_preserves_saves_and_unknown_keys(){let mut doc=IniDocument::parse("[Core]\nMemcardAPath = C:/Saves/My.raw\nSyncGPU = True\n[Custom]\nKeep = original\n[DSP]\nVolume = 100\n");let mut overlay=Overlay::new();overlay.entry("DSP".into()).or_default().insert("Volume".into(),"65".into());doc.merge(&overlay);assert_eq!(doc.get("Core","MemcardAPath"),Some("C:/Saves/My.raw"));assert_eq!(doc.get("Core","SyncGPU"),Some("True"));assert_eq!(doc.get("Custom","Keep"),Some("original"));assert_eq!(doc.get("DSP","Volume"),Some("65"));}
}

/// The image netplay boots, built or refreshed if need be.
///
/// `Ok(None)` means there is no imported game to build one from, which is the
/// old shape of an install that was never imported: those keep booting what
/// they always booted, and they have no custom soundtrack to diverge with
/// either, because a soundtrack apply needs a pristine image too.
fn netplay_image(p: &Platform) -> Result<Option<PathBuf>, String> {
    use reborn_core::online_image::{ensure, Outcome, Refusal};
    let data_root = p.paths.data_root().map_err(|e| e.to_string())?;
    let settings = crate::content_updates::load_settings(p)?;
    let dirs = reborn_core::roster::builder_dirs(p, &crate::content_updates::image_hook(p, &settings).extra_dirs);
    match ensure(p, &data_root, None, &dirs).map_err(|e| e.to_string())? {
        Outcome::Current { path } | Outcome::Built { path, .. } => Ok(Some(path)),
        Outcome::Refused(Refusal::NoPristine) => Ok(None),
        Outcome::Refused(r) => Err(r.message()),
    }
}

#[tauri::command]
pub async fn netplay_open(runtime:Option<String>,game:Option<String>,preset:String,nickname:String,mode:String,connection:String,address:String,port:u16)->Result<serde_json::Value,String>{
 tauri::async_runtime::spawn_blocking(move||{
  let guard=LaunchGuard::acquire()?;
  let nickname=nickname.trim();let address=address.trim();
  if nickname.is_empty()||nickname.chars().count()>32||nickname.chars().any(char::is_control){return Err("Enter a player name of 1 to 32 characters.".into());}
  if !["host","join"].contains(&mode.as_str())||!["direct","traversal"].contains(&connection.as_str())||port==0{return Err("Choose valid connection settings.".into());}
  if mode=="join"&&(address.is_empty()||address.len()>253||!address.chars().all(|c|c.is_ascii_alphanumeric()||".:-_".contains(c))){return Err("Enter a valid host code or address.".into());}
  let p=Platform::detect();let legacy=default_legacy_root(&p);let (found,game_path)=find_runtime_and_game(&p,runtime.as_deref(),game.as_deref(),legacy.as_deref())?;
  require_closed(found.path())?;
  // Netplay is lockstep, so both sides must run the same bytes. The offline
  // image is the one a custom soundtrack rewrites, so it is never the one
  // netplay boots: reborn_core::online_image keeps a copy that is the
  // pristine game plus the roster and nothing else, and rebuilds it here
  // when a game patch or a roster push has moved it.
  let game_path=match netplay_image(&p){Ok(Some(path))=>path,Ok(None)=>game_path,Err(e)=>return Err(e)};
  // Fail before preparing settings when an older/custom runtime lacks the bridge.
  if !supports_launcher_netplay(found.path())? { return Err("This Dolphin build does not support launcher Netplay. Choose the Reborn Dolphin runtime included with the latest launcher in Files and storage.".into()); }
  let req=LaunchRequest{runtime_exe:found.path().to_path_buf(),game_path:game_path.clone(),graphics:preset_from(&preset),fullscreen:true,build:GameBuild::Offline,mods_disabled:false,user_dir:selected_user_dir(&p,None)?};
  let prepared=Session::prepare(&p,&req).map_err(|e|e.to_string())?;
  let user=prepared.user_dir.path();
  let directory=p.paths.data_root().map_err(|e|e.to_string())?.join("cache/netplay");
  std::fs::create_dir_all(&directory).map_err(|e|e.to_string())?;
  // Fixed, small bridge files are reused. Never duplicate a game, profile or card.
  let path=directory.join("session.json");
  for suffix in ["status","command"] { let _=std::fs::remove_file(path.with_extension(format!("json.{suffix}"))); }
  std::fs::write(&path,serde_json::to_vec(&serde_json::json!({"game":game_path,"mode":mode})).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
  std::fs::write(path.with_extension("json.alive"),b"alive").map_err(|e|e.to_string())?;
  let mut args=vec![std::ffi::OsString::from("-u"),user.as_os_str().to_os_string(),"--reborn-netplay-session".into(),path.as_os_str().to_os_string()];
  for value in netplay_session_options(nickname,&mode,&connection,address,port) { args.push("-C".into());args.push(value.into()); }
  let handle=p.processes.spawn(found.path(),&args,found.path().parent(),&Default::default()).map_err(|e|e.to_string())?;
  *NETPLAY.lock().map_err(|e|e.to_string())?=Some(NetplaySession{path:path.clone(),active:true,sequence:0,connection:connection.clone(),port,last_status:serde_json::json!({"phase":"connecting"}),started:std::time::Instant::now()});
  std::thread::spawn(move||{
    let _guard=guard;
    // The process seam's is_running only reports ownership of a handle.
    // Wait for the actual child exit; keep the heartbeat on a separate thread.
    let alive=std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let heartbeat=alive.clone();
    std::thread::spawn(move||{
      while heartbeat.load(std::sync::atomic::Ordering::SeqCst) {
        let _=std::fs::write(path.with_extension("json.alive"),b"alive");
        std::thread::sleep(std::time::Duration::from_secs(1));
      }
    });
    let _=p.processes.wait(handle);
    alive.store(false,std::sync::atomic::Ordering::SeqCst);
    if let Ok(mut state)=NETPLAY.lock(){if let Some(session)=state.as_mut(){session.active=false;}}
  });
  Ok(serde_json::json!({"started":true,"mode":mode,"game":game_path.file_name().unwrap_or_default().to_string_lossy()}))
 }).await.map_err(|e|e.to_string())?
}


struct NetplaySession { path:PathBuf, active:bool, sequence:u64, connection:String, port:u16, last_status:serde_json::Value, started:std::time::Instant }
static NETPLAY:std::sync::Mutex<Option<NetplaySession>>=std::sync::Mutex::new(None);

// Command-line layers are never saved by Dolphin. Personal cards remain intact,
// while hosts share the same card-free configuration with every participant.
fn netplay_session_options(nickname:&str,_mode:&str,connection:&str,address:&str,port:u16)->Vec<String>{
 let mut values=vec![
  "Dolphin.Core.SlotA=255".into(),"Dolphin.Core.SlotB=255".into(),
  "Dolphin.NetPlay.SyncSaves=True".into(),"Dolphin.NetPlay.WriteSaveData=False".into(),
  "Dolphin.NetPlay.SyncCodes=True".into(),"Dolphin.NetPlay.StrictSettingsSync=True".into(),
  "Dolphin.NetPlay.UseIndex=False".into(),"Dolphin.Display.RenderToMain=False".into(),
  "Dolphin.Analytics.PermissionAsked=True".into(),
  format!("Dolphin.NetPlay.Nickname={nickname}"),format!("Dolphin.NetPlay.TraversalChoice={connection}"),
  format!("Dolphin.NetPlay.ConnectPort={port}"),format!("Dolphin.NetPlay.HostPort={port}"),
 ];
 values.push(format!("Dolphin.NetPlay.{}={address}",if connection=="traversal"{"HostCode"}else{"Address"}));values
}

#[tauri::command]
pub fn netplay_status()->Result<serde_json::Value,String>{
 let mut state=NETPLAY.lock().map_err(|e|e.to_string())?;
 let Some(session)=state.as_mut() else{return Ok(serde_json::json!({"phase":"idle","active":false}));};
 if let Some(status)=std::fs::read(session.path.with_extension("json.status")).ok().and_then(|s|serde_json::from_slice::<serde_json::Value>(&s).ok()).filter(|s|s.is_object()) {session.last_status=status;}
 let mut status=session.last_status.clone();
 if !session.active && status["phase"]!="error" {status["phase"]="closed".into();}
 if session.active && status["phase"]=="connecting" && session.started.elapsed().as_secs()>45 {status["phase"]="error".into();status["error"]="Dolphin did not open the room. Check that your Reborn Dolphin runtime supports launcher Netplay.".into();}
 status["active"]=session.active.into();status["connection"]=session.connection.clone().into();status["port"]=session.port.into();Ok(status)
}

#[tauri::command]
pub fn netplay_action(action:String,ports:Option<Vec<u8>>)->Result<(),String>{
 if !["start","leave","ports"].contains(&action.as_str()){return Err("Unknown room action.".into());}
 if action=="ports" && !ports.as_ref().is_some_and(|p|p.len()==4){return Err("Assign exactly four player slots.".into());}
 let mut state=NETPLAY.lock().map_err(|e|e.to_string())?;
 let session=state.as_mut().filter(|s|s.active).ok_or("The room is closed.")?;
 session.sequence+=1;
 let payload=serde_json::json!({"id":session.sequence,"action":action,"ports":ports});
 let dest=session.path.with_extension("json.command");let temp=session.path.with_extension("json.command.tmp");
 std::fs::write(&temp,serde_json::to_vec(&payload).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
 Platform::detect().files.atomic_replace(&temp,&dest).map_err(|e|e.to_string())
}

// Windows GUI binaries do not reliably expose --help on stdout. Read the exact
// compiled CLI marker instead, with bounded memory and no emulator subprocess.
fn supports_launcher_netplay(path:&Path)->Result<bool,String>{
 use std::io::Read;
 let mut file=std::fs::File::open(path).map_err(|e|e.to_string())?;
 let marker=b"--reborn-netplay-session";let mut chunk=[0u8;65536];let mut pending=Vec::new();
 loop{let count=file.read(&mut chunk).map_err(|e|e.to_string())?;if count==0{return Ok(false);}
  pending.extend_from_slice(&chunk[..count]);if pending.windows(marker.len()).any(|w|w==marker){return Ok(true);}
  let keep=pending.len().saturating_sub(marker.len()-1);pending.drain(..keep);
 }
}
