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

/// The path law: a Reborn user folder is self-contained. Dolphin derives
/// every path from the `-u` folder, so Dolphin.ini carries no path key that
/// points inside a Dolphin user folder (this one or another) and none that
/// is relative. A 2.1 Dolphin.ini copied into a migrated folder kept the
/// game loading textures from the old install (Adam, 2026-09-11, second
/// PC); a relative `User/Load` resolves against the working directory.
/// Absolute paths outside every Dolphin user folder are a player's own
/// choice and stay. The disc keys go always: the launcher passes `-e`.
pub fn apply_path_law(user: &std::path::Path) -> crate::platform::Result<()> {
 let file=user.join("Config/Dolphin.ini");
 let text=match std::fs::read_to_string(&file){
  Ok(text)=>text,
  Err(e) if e.kind()==std::io::ErrorKind::NotFound=>return Ok(()),
  Err(e)=>return Err(e.into()),
 };
 let mut doc=crate::ini::IniDocument::parse(&text);
 let removals=path_law_removals(&doc,user);
 if removals.is_empty(){return Ok(());}
 for (section,key) in &removals{doc.remove(section,key);}
 let backup=file.with_extension("ini.before-path-law");
 if !backup.exists(){std::fs::write(&backup,&text)?;}
 std::fs::write(file,doc.render())?;
 Ok(())
}

const ALWAYS_REMOVED:&[(&str,&str)]=&[("General","ISOPaths"),("General","ISOPath0"),("General","ISOPath1"),("General","ISOPath2"),("General","ISOPath3"),("Core","DefaultISO")];
const PATH_KEYS:&[(&str,&[&str])]=&[
 ("General",&["LoadPath","DumpPath","NANDRootPath","ResourcePackPath","WiiSDCardPath"]),
 ("Core",&["MemcardAPath","MemcardBPath","GCIFolderAPathOverride","GCIFolderBPathOverride","AgpCartAPath","AgpCartBPath"]),
 ("GBA",&["BIOS","SavesPath","Rom1","Rom2","Rom3","Rom4"]),
];
const DOLPHIN_SUBFOLDERS:&[&str]=&["Load","Dump","ResourcePacks","GC","Wii","StateSaves","Config","GameSettings","Logs","GBA"];

/// Section and key of every pair the law removes from `doc`.
pub fn path_law_removals(doc:&crate::ini::IniDocument,user:&std::path::Path)->Vec<(String,String)>{
 let mut out=Vec::new();
 for (section,key) in ALWAYS_REMOVED{ if doc.get(section,key).is_some(){out.push(((*section).to_string(),(*key).to_string()));} }
 for (section,keys) in PATH_KEYS{
  for key in *keys{
   let Some(value)=doc.get(section,key) else{continue};
   let normalized=value.trim().trim_matches('"').replace('\\',"/");
   if normalized.is_empty()||normalized.eq_ignore_ascii_case("none"){continue;}
   if !is_absolute(&normalized)||under(&normalized,user)||inside_another_dolphin_user_folder(&normalized){
    out.push(((*section).to_string(),(*key).to_string()));
   }
  }
 }
 out
}

/// `.../User/<Dolphin subfolder>/...` where that `User` is not a Windows
/// account folder (`C:/Users/User/...`), or anything under `dolphin-users`.
fn inside_another_dolphin_user_folder(normalized:&str)->bool{
 let all:Vec<&str>=normalized.split('/').filter(|p|!p.is_empty()&&*p!=".").collect();
 if all.iter().any(|p|p.eq_ignore_ascii_case("dolphin-users")){return true;}
 all.iter().rposition(|p|p.eq_ignore_ascii_case("User")).is_some_and(|i|{
  let before_is_users_root=i>0&&all[i-1].eq_ignore_ascii_case("Users");
  let dolphin_folder=all.get(i+1).is_some_and(|f|DOLPHIN_SUBFOLDERS.iter().any(|k|k.eq_ignore_ascii_case(f)));
  !before_is_users_root&&dolphin_folder
 })
}

/// "C:/x" or "//server/x": a path that names its own root.
fn is_absolute(normalized:&str)->bool{
 normalized.starts_with('/')||normalized.as_bytes().get(1)==Some(&b':')
}

/// Whether `normalized` (forward slashes) is `user` or inside it.
fn under(normalized:&str,user:&std::path::Path)->bool{
 let u=user.to_string_lossy().replace('\\',"/");
 if u.is_empty(){return false;}
 let a=normalized.trim_end_matches('/').to_ascii_lowercase();
 let b=u.trim_end_matches('/').to_ascii_lowercase();
 a==b||a.starts_with(&format!("{b}/"))
}

#[cfg(test)] mod path_law_tests {
 use super::*;
 fn user()->std::path::PathBuf{ std::path::PathBuf::from(r"C:\Users\Adam\AppData\Roaming\NFL Street Reborn\dolphin-users\fresh-1") }
 fn keys(removed:&[(String,String)])->Vec<String>{ removed.iter().map(|(s,k)|format!("{s}/{k}")).collect() }
 #[test] fn paths_inside_this_or_another_dolphin_user_folder_are_removed(){
  let doc=crate::ini::IniDocument::parse("[General]\nLoadPath = C:/Users/Adam/AppData/Roaming/NFL Street Reborn/User/Load\nDumpPath = C:\\Users\\Adam\\AppData\\Roaming\\NFL Street Reborn\\dolphin-users\\fresh-1\\Dump\nResourcePackPath = D:/Packs\nWiiSDCardPath = None\n[Core]\nMemcardAPath = C:/Users/Adam/AppData/Roaming/NFL Street Reborn/User/GC/MemoryCardA.USA.raw\nMemcardBPath = D:/Backups/User/Cards/MemoryCardB.USA.raw\n");
  let removed=keys(&path_law_removals(&doc,&user()));
  assert_eq!(removed,["General/LoadPath","General/DumpPath","Core/MemcardAPath"]);
 }
 #[test] fn relative_paths_are_removed_and_foreign_absolute_ones_kept(){
  let doc=crate::ini::IniDocument::parse("[General]\nLoadPath = User/Load///\nDumpPath = .\\User\\Dump\\\nResourcePackPath = D:/Packs\n[Core]\nMemcardBPath = User/GC/Card.raw\nMemcardAPath = D:/cards/A.raw\nSlotA = 255\n[GBA]\nBIOS = None\nRom1 = D:/roms/a.gba\n");
  let removed=keys(&path_law_removals(&doc,&user()));
  assert_eq!(removed,["General/LoadPath","General/DumpPath","Core/MemcardBPath"]);
 }
 #[test] fn the_default_iso_and_iso_paths_are_always_removed(){
  let doc=crate::ini::IniDocument::parse("[General]\nISOPaths = 1\nISOPath0 = Games\n[Core]\nDefaultISO = C:\\Users\\Adam\\AppData\\Roaming\\NFL Street Reborn\\Games\\NSRV3.iso\n");
  let removed=keys(&path_law_removals(&doc,&user()));
  assert_eq!(removed,["General/ISOPaths","General/ISOPath0","Core/DefaultISO"]);
 }
 #[test] fn an_account_folder_called_user_is_not_a_dolphin_user_folder(){
  let doc=crate::ini::IniDocument::parse("[General]\nLoadPath = C:/Users/User/Documents/Dolphin Emulator/Load\n[Core]\nMemcardAPath = C:/Users/User/Documents/Dolphin Emulator/GC/MemoryCardA.USA.raw\n");
  assert!(path_law_removals(&doc,&user()).is_empty());
 }
 #[test] fn apply_writes_only_when_something_changed_and_keeps_one_backup(){
  let dir=tempfile::tempdir().unwrap();let user=dir.path().join("u");std::fs::create_dir_all(user.join("Config")).unwrap();
  let file=user.join("Config/Dolphin.ini");
  std::fs::write(&file,"[General]\nLoadPath = User/Load\n[DSP]\nVolume = 0\n").unwrap();
  apply_path_law(&user).unwrap();
  assert_eq!(std::fs::read_to_string(&file).unwrap(),"[General]\n[DSP]\nVolume = 0\n");
  assert!(user.join("Config/Dolphin.ini.before-path-law").is_file());
  let clean="[DSP]\nVolume = 0\n";std::fs::write(&file,clean).unwrap();
  let before=std::fs::metadata(&file).unwrap().modified().unwrap();
  apply_path_law(&user).unwrap();
  assert_eq!(std::fs::read_to_string(&file).unwrap(),clean);
  assert_eq!(std::fs::metadata(&file).unwrap().modified().unwrap(),before,"untouched when nothing to remove");
  assert!(apply_path_law(&dir.path().join("absent")).is_ok(),"no Dolphin.ini is not an error");
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
