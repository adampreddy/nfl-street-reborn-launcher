//! User-editable Dolphin settings. Compatibility-sensitive keys are deliberately absent.
#[derive(Debug)]
pub struct Field { pub id: &'static str, pub file: &'static str, pub section: &'static str, pub key: &'static str, pub default: &'static str, pub kind: &'static str }
pub const FIELDS: &[Field] = &[
    Field{id:"audio_backend",file:"Dolphin.ini",section:"DSP",key:"Backend",default:"Cubeb",kind:"backend"},
    Field{id:"volume",file:"Dolphin.ini",section:"DSP",key:"Volume",default:"100",kind:"volume"},
    Field{id:"muted",file:"Dolphin.ini",section:"DSP",key:"Muted",default:"False",kind:"bool"},
    Field{id:"audio_stretch",file:"Dolphin.ini",section:"Core",key:"AudioStretch",default:"False",kind:"bool"},
    Field{id:"audio_latency",file:"Dolphin.ini",section:"Core",key:"AudioLatency",default:"20",kind:"latency"},
    Field{id:"fullscreen",file:"Dolphin.ini",section:"Display",key:"Fullscreen",default:"False",kind:"bool"},
    Field{id:"always_on_top",file:"Dolphin.ini",section:"Display",key:"KeepWindowOnTop",default:"False",kind:"bool"},
    Field{id:"vsync",file:"GFX.ini",section:"Hardware",key:"VSync",default:"False",kind:"bool"},
    Field{id:"show_fps",file:"GFX.ini",section:"Settings",key:"ShowFPS",default:"False",kind:"bool"},
    // Anti-aliasing is the player's: 4x is the Reborn baseline (Adam, 2026-09-10), lower if the game stutters.
    Field{id:"msaa",file:"GFX.ini",section:"Settings",key:"MSAA",default:"0x00000004",kind:"msaa"},
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

pub fn overlay(file:&str, values:&std::collections::BTreeMap<String,String>)->crate::ini::Overlay {
 let mut result=crate::ini::Overlay::new();
 for field in FIELDS.iter().filter(|f|f.file==file){if let Some(value)=values.get(field.id){result.entry(field.section.into()).or_default().insert(field.key.into(),value.clone());}}
 result
}
pub fn fullscreen(platform:&crate::platform::Platform,fallback:bool)->crate::platform::Result<bool>{
 let saved=crate::settings::Settings::load(&crate::settings::settings_path(platform)?)?;
 Ok(saved.emulator_preferences.get("fullscreen").map(|v|v.eq_ignore_ascii_case("true")).unwrap_or(fallback))
}

/// Old portable installs store paths relative to the executable, even when
/// Dolphin is given a different User directory with -u. Bind User/... paths
/// to that selected profile before a bundled executable opens it.
pub fn resolve_portable_paths(user: &std::path::Path) -> crate::platform::Result<()> {
 let file=user.join("Config/Dolphin.ini");
 let text=match std::fs::read_to_string(&file){
  Ok(text)=>text,
  Err(e) if e.kind()==std::io::ErrorKind::NotFound=>return Ok(()),
  Err(e)=>return Err(e.into()),
 };
 let mut doc=crate::ini::IniDocument::parse(&text);
 let changes=portable_path_overlay(&doc,user);
 if !doc.merge(&changes).is_noop(){
  let backup=file.with_extension("ini.before-path-fix");
  if !backup.exists(){std::fs::write(&backup,&text)?;}
  std::fs::write(file,doc.render())?;
 }
 Ok(())
}

fn portable_path_overlay(doc:&crate::ini::IniDocument,user:&std::path::Path)->crate::ini::Overlay{
 let mut changes=crate::ini::Overlay::new();
 for (section,keys) in [
  ("General",&["LoadPath","DumpPath","NANDRootPath","ResourcePackPath","WiiSDCardPath"][..]),
  ("Core",&["MemcardAPath","MemcardBPath","GCIFolderAPathOverride","GCIFolderBPathOverride","AgpCartAPath","AgpCartBPath"][..]),
  ("GBA",&["BIOS","SavesPath","Rom1","Rom2","Rom3","Rom4"][..]),
 ]{
  for key in keys{
   let Some(value)=doc.get(section,key)else{continue};
   let normalized=value.trim().trim_matches('"').replace('\\',"/");
   let all:Vec<&str>=normalized.split('/').filter(|p|!p.is_empty()&&*p!=".").collect();
   // Two shapes are rebased onto THIS user folder. A portable one
   // ("User/Load"), and an absolute one that points inside ANOTHER
   // Dolphin user folder: a 2.1 Dolphin.ini carries
   // "C:/.../NFL Street Reborn/User/Load", and copied into a migrated
   // user folder it kept the game loading textures from the old install,
   // so no texture update ever reached it (Adam, 2026-09-11, second PC).
   let tail:Option<&[&str]>=if !normalized.starts_with('/')&&all.first().is_some_and(|p|p.eq_ignore_ascii_case("User")){
    Some(&all[1..])
   } else if is_absolute(&normalized)&&!under(&normalized,user){
    // Only a path that really points into another Dolphin user folder: the
    // "User" component must not be a Windows account folder
    // (C:/Users/User/Documents/...) and what follows must be one of Dolphin's
    // own subfolders, so a memory card kept somewhere unusual is left alone.
    all.iter().rposition(|p|p.eq_ignore_ascii_case("User")).and_then(|i|{
     let before_is_users_root=i>0&&all[i-1].eq_ignore_ascii_case("Users");
     let tail=&all[i+1..];
     let dolphin_folder=tail.first().is_some_and(|f|["Load","Dump","ResourcePacks","GC","Wii","StateSaves","Config","GameSettings","Logs","GBA"].iter().any(|k|k.eq_ignore_ascii_case(f)));
     (!before_is_users_root&&dolphin_folder).then_some(tail)
    })
   } else {None};
   let Some(tail)=tail else{continue};
   let mut resolved=user.to_path_buf();
   for part in tail{resolved.push(part);}
   changes.entry(section.into()).or_default().insert((*key).into(),resolved.to_string_lossy().replace('\\',"/"));
  }
 }
 changes
}

/// "C:/x" or "//server/x": a path that names its own root.
fn is_absolute(normalized:&str)->bool{
 normalized.starts_with('/')||normalized.as_bytes().get(1)==Some(&b':')
}

/// Whether `normalized` (forward slashes) is `user` or inside it.
fn under(normalized:&str,user:&std::path::Path)->bool{
 let u=user.to_string_lossy().replace('\\',"/");
 let a=normalized.trim_end_matches('/').to_ascii_lowercase();
 let b=u.trim_end_matches('/').to_ascii_lowercase();
 a==b||a.starts_with(&format!("{b}/"))
}

#[cfg(test)] mod portable_path_tests {
 use super::*;
 #[test] fn a_migrated_config_pointing_at_the_old_install_is_rebased_onto_this_user_folder(){
  let user=std::path::Path::new(r"C:\Users\Adam\AppData\Roaming\NFL Street Reborn Launcher\dolphin-users\migrated");
  let doc=crate::ini::IniDocument::parse("[General]\nLoadPath = C:/Users/Adam/AppData/Roaming/NFL Street Reborn/User/Load\nDumpPath = C:\\Users\\Adam\\AppData\\Roaming\\NFL Street Reborn\\User\\Dump\nResourcePackPath = D:/Packs\nWiiSDCardPath = None\n[Core]\nMemcardAPath = C:/Users/Adam/AppData/Roaming/NFL Street Reborn/User/GC/MemoryCardA.USA.raw\n");
  let changes=portable_path_overlay(&doc,user);
  let general=&changes["General"];
  assert_eq!(general["LoadPath"],"C:/Users/Adam/AppData/Roaming/NFL Street Reborn Launcher/dolphin-users/migrated/Load");
  assert_eq!(general["DumpPath"],"C:/Users/Adam/AppData/Roaming/NFL Street Reborn Launcher/dolphin-users/migrated/Dump");
  assert!(!general.contains_key("ResourcePackPath"),"a path with no user folder in it is someone's own choice");
  assert!(!general.contains_key("WiiSDCardPath"));
  assert_eq!(changes["Core"]["MemcardAPath"],"C:/Users/Adam/AppData/Roaming/NFL Street Reborn Launcher/dolphin-users/migrated/GC/MemoryCardA.USA.raw");
  // Already this user folder: nothing to change.
  let own=crate::ini::IniDocument::parse("[General]\nLoadPath = C:/Users/Adam/AppData/Roaming/NFL Street Reborn Launcher/dolphin-users/migrated/Load\n");
  assert!(portable_path_overlay(&own,user).is_empty());
  // A Windows account called User, and a card kept somewhere of the player's
  // own choosing: neither is a Dolphin user folder, so both stay as they are.
  let account=crate::ini::IniDocument::parse("[General]\nLoadPath = C:/Users/User/Documents/Dolphin Emulator/Load\n[Core]\nMemcardAPath = C:/Users/User/Documents/Dolphin Emulator/GC/MemoryCardA.USA.raw\nMemcardBPath = D:/Backups/User/Cards/MemoryCardB.USA.raw\n");
  assert!(portable_path_overlay(&account,user).is_empty(),"{:?}",portable_path_overlay(&account,user));
 }
 #[test] fn selected_profile_owns_legacy_paths_without_replacing_external_choices(){
  let doc=crate::ini::IniDocument::parse("[General]\nLoadPath = User/Load///\nDumpPath = .\\User\\Dump\\\nResourcePackPath = D:/Packs\nWiiSDCardPath = None\n[Core]\nMemcardBPath = User/GC/Card.raw\nSlotA = 255\n[DSP]\nVolume = 0\n");
  let changes=portable_path_overlay(&doc,std::path::Path::new("C:/Existing/User"));
  assert_eq!(changes["General"]["LoadPath"],"C:/Existing/User/Load");
  assert_eq!(changes["General"]["DumpPath"],"C:/Existing/User/Dump");
  assert_eq!(changes["Core"]["MemcardBPath"],"C:/Existing/User/GC/Card.raw");
  assert!(!changes["General"].contains_key("ResourcePackPath"));
  assert!(!changes["General"].contains_key("WiiSDCardPath"));
  assert!(!changes["Core"].contains_key("SlotA"));
  assert!(!changes.contains_key("DSP"));
 }
 #[test] fn path_fix_is_repeatable_and_keeps_original_settings(){
  let tmp=tempfile::tempdir().unwrap();let user=tmp.path().join("User");
  std::fs::create_dir_all(user.join("Config")).unwrap();
  let file=user.join("Config/Dolphin.ini");
  let original="[General]\nLoadPath = User/Load\n[DSP]\nVolume = 0\n";
  std::fs::write(&file,original).unwrap();
  resolve_portable_paths(&user).unwrap();let first=std::fs::read_to_string(&file).unwrap();
  resolve_portable_paths(&user).unwrap();assert_eq!(std::fs::read_to_string(&file).unwrap(),first);
  assert_eq!(std::fs::read_to_string(file.with_extension("ini.before-path-fix")).unwrap(),original);
  assert_eq!(crate::ini::IniDocument::parse(&first).get("DSP","Volume"),Some("0"));
 }
}

#[cfg(test)] mod tests {
 use super::*;
 #[test] fn user_choices_override_managed_defaults_but_not_compatibility() {
  let values=std::collections::BTreeMap::from([("fullscreen".into(),"False".into()),("confirm_stop".into(),"True".into()),("volume".into(),"65".into()),("InternalResolution".into(),"4".into()),("SyncGPU".into(),"False".into())]);
  let changes=overlay("Dolphin.ini",&values);assert_eq!(changes["Display"]["Fullscreen"],"False");assert_eq!(changes["Interface"]["ConfirmStop"],"True");assert_eq!(changes["DSP"]["Volume"],"65");assert!(!changes.contains_key("Core"));assert!(overlay("GFX.ini",&values).is_empty());
 }
}

/// Dolphin can edit these preferences too. Its current values are authoritative;
/// saved launcher values only restore preferences when the INI no longer has them.
pub fn preserve_existing(file:&str,doc:&crate::ini::IniDocument,changes:&mut crate::ini::Overlay){
 for field in FIELDS.iter().filter(|f|f.file==file){if let Some(value)=doc.get(field.section,field.key){changes.entry(field.section.into()).or_default().insert(field.key.into(),value.into());}}
}
#[cfg(test)] mod external_edit_tests {
 use super::*;
 #[test] fn dolphin_edits_win_without_weakening_reborn_compatibility(){
  let doc=crate::ini::IniDocument::parse("[Display]\nFullscreen = False\n[Interface]\nConfirmStop = True\n[Core]\nMEM1Size = 25165824\n");
  let prefs=std::collections::BTreeMap::from([("fullscreen".into(),"True".into()),("confirm_stop".into(),"False".into())]);
  let mut changes=overlay("Dolphin.ini",&prefs);changes.entry("Core".into()).or_default().insert("MEM1Size".into(),"67108864".into());preserve_existing("Dolphin.ini",&doc,&mut changes);
  assert_eq!(changes["Display"]["Fullscreen"],"False");assert_eq!(changes["Interface"]["ConfirmStop"],"True");assert_eq!(changes["Core"]["MEM1Size"],"67108864");
 }
}
