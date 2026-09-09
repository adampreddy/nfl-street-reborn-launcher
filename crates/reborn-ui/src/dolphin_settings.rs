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
// Old installations can have save/code synchronization disabled. A manual
// session must begin with the same game state and deterministic settings.
fn netplay_sync_overlay() -> Overlay {
    let mut overlay = Overlay::new();
    let net = overlay.entry("NetPlay".into()).or_default();
    for key in ["SyncSaves", "SyncCodes", "StrictSettingsSync"] {
        net.insert(key.into(), "True".into());
    }
    overlay
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
    fn netplay_reenables_synchronization_without_replacing_save_locations() {
        let mut doc = IniDocument::parse("[NetPlay]\nSyncSaves = False\nSyncCodes = False\nStrictSettingsSync = False\nNickname = Player\n[Core]\nMemcardAPath = C:/Saves/My.raw\nMEM1Size = 67108864\n");
        doc.merge(&netplay_sync_overlay());
        for key in ["SyncSaves", "SyncCodes", "StrictSettingsSync"] {
            assert_eq!(doc.get("NetPlay", key), Some("True"));
        }
        assert_eq!(doc.get("NetPlay", "Nickname"), Some("Player"));
        assert_eq!(doc.get("Core", "MemcardAPath"), Some("C:/Saves/My.raw"));
        assert_eq!(doc.get("Core", "MEM1Size"), Some("67108864"));
    }
    #[test]fn settings_are_validated_and_protected(){assert!(FIELDS.iter().all(|f|!["InternalResolution","MSAA","SyncGPU","MEM1Size","RAMOverrideEnable","EFBAccessEnable"].contains(&f.key)));for id in ["volume","audio_latency"]{let f=FIELDS.iter().find(|f|f.id==id).unwrap();assert!(checked_value(f,"-1").is_err());assert!(checked_value(f,"999").is_err());assert!(checked_value(f,"20").is_ok());}let f=FIELDS.iter().find(|f|f.id=="muted").unwrap();assert_eq!(checked_value(f,"true").unwrap(),"True");assert!(checked_value(f,"true\n[Core]").is_err());}
    #[test]fn editing_one_setting_preserves_saves_and_unknown_keys(){let mut doc=IniDocument::parse("[Core]\nMemcardAPath = C:/Saves/My.raw\nSyncGPU = True\n[Custom]\nKeep = original\n[DSP]\nVolume = 100\n");let mut overlay=Overlay::new();overlay.entry("DSP".into()).or_default().insert("Volume".into(),"65".into());doc.merge(&overlay);assert_eq!(doc.get("Core","MemcardAPath"),Some("C:/Saves/My.raw"));assert_eq!(doc.get("Core","SyncGPU"),Some("True"));assert_eq!(doc.get("Custom","Keep"),Some("original"));assert_eq!(doc.get("DSP","Volume"),Some("65"));}
}

/// Start Dolphin with NetPlay configured, and record the session.
///
/// The session record is written before the emulator starts and finished when
/// it stops, for the same reason a launch does it: NetPlay is the one path
/// that produces `GameStats\Online` captures, and where a result goes is
/// decided at launch and written down, never worked out from the file
/// afterwards. This command used to spawn the emulator with no record and no
/// collection at all, so every online game a player played through the
/// launcher was captured to disk and then either never delivered or adopted by
/// the next offline session and stamped with that session's id and exit.
///
/// `build` stays [`GameBuild::Offline`] deliberately. That field names which
/// game image is booting, not how the two players reach each other: NetPlay
/// runs the customizable offline disc, and its game id decides which
/// `GameSettings` file is written and which mod library is composed. Whether
/// the game was online is carried by the folder Dolphin writes into, which the
/// queue records and the envelope sends.
///
/// `league_match` is false because this dialog never asks. Recording "not a
/// league fixture" is the only honest answer to a question the player was not
/// put, and asserting a fixture nobody claimed is the one direction that
/// corrupts league standings. The result still reaches the server, marked
/// online.
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
  let req=LaunchRequest{runtime_exe:found.path().to_path_buf(),game_path:game_path.clone(),graphics:preset_from(&preset),fullscreen:true,build:GameBuild::Offline,mods_disabled:false,user_dir:selected_user_dir(&p,None)?,league_match:false};
  // The lock, before anything is written or started, and anything left
  // unfinished recovered. The same first two steps as a launch.
  let Startup::Owner(supervisor)=Supervisor::acquire(&p).map_err(|e|e.to_string())? else { return Err(already_running()); };
  let _=supervisor.recover_unfinished().map_err(|e|e.to_string())?;
  let prepared=Session::prepare(&p,&req).map_err(|e|e.to_string())?;
  let user=prepared.user_dir.path();let file=user.join("Config/Dolphin.ini");let mut doc=read_ini(&file)?;let mut overlay=netplay_sync_overlay();let net=overlay.entry("NetPlay".into()).or_default();
  net.insert("Nickname".into(),nickname.into());net.insert("TraversalChoice".into(),connection.clone());net.insert("ConnectPort".into(),port.to_string());net.insert("HostPort".into(),port.to_string());
  if mode=="join"{net.insert(if connection=="traversal"{"HostCode"}else{"Address"}.into(),address.into());}
  // Make the chosen game discoverable in Dolphin's Host tab without replacing existing folders.
  let count=doc.get("General","ISOPaths").and_then(|v|v.parse::<usize>().ok()).unwrap_or(0).min(10000);
  if let Some(parent)=game_path.parent(){let path=parent.display().to_string();if !(0..count).any(|i|doc.get("General",&format!("ISOPath{i}"))==Some(path.as_str())){let general=overlay.entry("General".into()).or_default();general.insert(format!("ISOPath{count}"),path);general.insert("ISOPaths".into(),(count+1).to_string());}}
  doc.merge(&overlay);std::fs::write(&file,doc.render()).map_err(|e|e.to_string())?;
  let args=vec![std::ffi::OsString::from("-u"),user.as_os_str().to_os_string()];
  let user_dir=user.to_path_buf();
  // Written before the emulator, so the id every capture from this session is
  // filed under exists before there is anything to file.
  let record=supervisor.begin_with_intent(req.build,Some(SessionIntent{league_match:req.league_match})).map_err(|e|e.to_string())?;
  let handle=p.processes.spawn(found.path(),&args,found.path().parent(),&Default::default()).map_err(|e|e.to_string())?;
  std::thread::spawn(move||{
   // The lock and the guard are held across the wait AND across the collection
   // and the send, the way Session::run holds them: the queue and the session
   // log are shared state, and nothing may start a second session in the middle
   // of filing this one's results.
   let _guard=guard;
   let exit=p.processes.wait(handle).ok();
   // Reported through the session record, never to a screen. The window has
   // already been told the game started, and a game that was played is not a
   // failure however collecting or sending it went.
   let _=reborn_core::session::finish_and_collect(&p,&supervisor,record,exit.as_ref(),&user_dir);
  });
  Ok(serde_json::json!({"started":true,"mode":mode,"game":game_path.file_name().unwrap_or_default().to_string_lossy()}))
 }).await.map_err(|e|e.to_string())?
}
