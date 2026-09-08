use super::*;
use reborn_core::ini::{IniDocument, Overlay};

struct Field { id: &'static str, file: &'static str, section: &'static str, key: &'static str, default: &'static str, kind: &'static str }
const FIELDS: &[Field] = &[
    Field{id:"volume",file:"Dolphin.ini",section:"DSP",key:"Volume",default:"100",kind:"volume"},
    Field{id:"muted",file:"Dolphin.ini",section:"DSP",key:"Muted",default:"False",kind:"bool"},
    Field{id:"audio_stretch",file:"Dolphin.ini",section:"Core",key:"AudioStretch",default:"False",kind:"bool"},
    Field{id:"audio_latency",file:"Dolphin.ini",section:"Core",key:"AudioLatency",default:"20",kind:"latency"},
    Field{id:"fullscreen",file:"Dolphin.ini",section:"Display",key:"Fullscreen",default:"False",kind:"bool"},
    Field{id:"always_on_top",file:"Dolphin.ini",section:"Display",key:"KeepWindowOnTop",default:"False",kind:"bool"},
    Field{id:"vsync",file:"GFX.ini",section:"Hardware",key:"VSync",default:"False",kind:"bool"},
    Field{id:"show_fps",file:"GFX.ini",section:"Settings",key:"ShowFPS",default:"False",kind:"bool"},
    Field{id:"show_ping",file:"GFX.ini",section:"Settings",key:"ShowNetPlayPing",default:"False",kind:"bool"},
    Field{id:"borderless",file:"GFX.ini",section:"Settings",key:"BorderlessFullscreen",default:"False",kind:"bool"},
    Field{id:"confirm_stop",file:"Dolphin.ini",section:"Interface",key:"ConfirmStop",default:"True",kind:"bool"},
    Field{id:"pause_unfocused",file:"Dolphin.ini",section:"Interface",key:"PauseOnFocusLost",default:"False",kind:"bool"},
    Field{id:"osd_messages",file:"Dolphin.ini",section:"Interface",key:"OnScreenDisplayMessages",default:"True",kind:"bool"},
    Field{id:"slot_a",file:"Dolphin.ini",section:"Core",key:"SlotA",default:"8",kind:"slot"},
    Field{id:"slot_b",file:"Dolphin.ini",section:"Core",key:"SlotB",default:"255",kind:"slot"},
    Field{id:"card_a",file:"Dolphin.ini",section:"Core",key:"MemcardAPath",default:"",kind:"card"},
    Field{id:"card_b",file:"Dolphin.ini",section:"Core",key:"MemcardBPath",default:"",kind:"card"},
    Field{id:"gci_a",file:"Dolphin.ini",section:"Core",key:"GCIFolderAPathOverride",default:"",kind:"folder"},
    Field{id:"gci_b",file:"Dolphin.ini",section:"Core",key:"GCIFolderBPathOverride",default:"",kind:"folder"},
];
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
    tauri::async_runtime::spawn_blocking(move||{let p=Platform::detect();let (exe,user)=config_user(&p,runtime.as_deref())?;let mut values=serde_json::Map::new();for field in FIELDS {let doc=read_ini(&user.join("Config").join(field.file))?;values.insert(field.id.into(),serde_json::json!(doc.get(field.section,field.key).unwrap_or(field.default)));}Ok(serde_json::json!({"values":values,"user_dir":user,"editable":require_closed(&exe).is_ok()}))}).await.map_err(|e|e.to_string())?
}
#[tauri::command]
pub async fn dolphin_preference_set(runtime:Option<String>,key:String,value:String)->Result<(),String>{
    tauri::async_runtime::spawn_blocking(move||{let _guard=LaunchGuard::acquire()?;let p=Platform::detect();let (exe,user)=config_user(&p,runtime.as_deref())?;require_closed(&exe)?;let field=FIELDS.iter().find(|f|f.id==key).ok_or("Unknown setting.")?;let value=checked_value(field,&value)?;let file=user.join("Config").join(field.file);let mut doc=read_ini(&file)?;let mut overlay=Overlay::new();overlay.entry(field.section.into()).or_default().insert(field.key.into(),value);if !doc.merge(&overlay).is_noop(){std::fs::create_dir_all(file.parent().unwrap()).map_err(|e|e.to_string())?;std::fs::write(&file,doc.render()).map_err(|e|e.to_string())?;}Ok(())}).await.map_err(|e|e.to_string())?
}
#[tauri::command]
pub async fn dolphin_saves_folder(runtime:Option<String>)->Result<(),String>{
    let p=Platform::detect();let (_,user)=config_user(&p,runtime.as_deref())?;let dir=user.join("GC");std::fs::create_dir_all(&dir).map_err(|e|e.to_string())?;open_in_file_manager(&p,&dir)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]fn settings_are_validated_and_protected(){assert!(FIELDS.iter().all(|f|!["InternalResolution","MSAA","SyncGPU","MEM1Size","RAMOverrideEnable","EFBAccessEnable"].contains(&f.key)));for id in ["volume","audio_latency"]{let f=FIELDS.iter().find(|f|f.id==id).unwrap();assert!(checked_value(f,"-1").is_err());assert!(checked_value(f,"999").is_err());assert!(checked_value(f,"20").is_ok());}let f=&FIELDS[1];assert_eq!(checked_value(f,"true").unwrap(),"True");assert!(checked_value(f,"true\n[Core]").is_err());}
    #[test]fn editing_one_setting_preserves_saves_and_unknown_keys(){let mut doc=IniDocument::parse("[Core]\nMemcardAPath = C:/Saves/My.raw\nSyncGPU = True\n[Custom]\nKeep = original\n[DSP]\nVolume = 100\n");let mut overlay=Overlay::new();overlay.entry("DSP".into()).or_default().insert("Volume".into(),"65".into());doc.merge(&overlay);assert_eq!(doc.get("Core","MemcardAPath"),Some("C:/Saves/My.raw"));assert_eq!(doc.get("Core","SyncGPU"),Some("True"));assert_eq!(doc.get("Custom","Keep"),Some("original"));assert_eq!(doc.get("DSP","Volume"),Some("65"));}
}
