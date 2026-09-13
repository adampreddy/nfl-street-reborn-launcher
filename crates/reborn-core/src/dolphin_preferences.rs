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
    // Raising it softens the depth-driven ReShade effects, which the Settings page says next to the control.
    Field{id:"msaa",file:"GFX.ini",section:"Settings",key:"MSAA",default:"0x00000004",kind:"msaa"},
    // Internal resolution, the single biggest performance lever and the one a
    // player on a weak GPU is told to lower. It used to be pinned at 3 in the
    // per-game layer, so neither this page nor Dolphin's own could change it.
    // The ReShade depth filter is derived from it; see reshade::depth_filter_for.
    Field{id:"internal_resolution",file:"GFX.ini",section:"Settings",key:"InternalResolution",default:"3",kind:"resolution"},
    // Synchronize GPU thread: expensive, and it never prevented the freeze it
    // was forced on for. The player decides whether to spend the frames.
    Field{id:"sync_gpu",file:"Dolphin.ini",section:"Core",key:"SyncGPU",default:"False",kind:"bool"},
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

/// The disc keys. Removed only when they point at the launcher's own content,
/// or are relative, or sit inside a Dolphin user folder.
///
/// These used to be removed unconditionally on every launch, and on every
/// "Open Dolphin". That deleted the player's own game-list folders every time
/// they added one - in the very dialog the launcher had just opened for them -
/// and it deleted `Core/DefaultISO`, which `session::find_game_image` reads.
/// The original reason still holds for a 2.1 Dolphin.ini pointing at an old
/// install, and that is all this now covers.
pub const DISC_KEYS:&[(&str,&str)]=&[("General","ISOPath0"),("General","ISOPath1"),("General","ISOPath2"),("General","ISOPath3"),("Core","DefaultISO")];
const PATH_KEYS:&[(&str,&[&str])]=&[
 ("General",&["LoadPath","DumpPath","NANDRootPath","ResourcePackPath","WiiSDCardPath"]),
 ("Core",&["MemcardAPath","MemcardBPath","GCIFolderAPathOverride","GCIFolderBPathOverride","AgpCartAPath","AgpCartBPath"]),
 ("GBA",&["BIOS","SavesPath","Rom1","Rom2","Rom3","Rom4"]),
];
const DOLPHIN_SUBFOLDERS:&[&str]=&["Load","Dump","ResourcePacks","GC","Wii","StateSaves","Config","GameSettings","Logs","GBA"];

/// Section and key of every pair the law removes from `doc`.
pub fn path_law_removals(doc:&crate::ini::IniDocument,user:&std::path::Path)->Vec<(String,String)>{
 let mut out=Vec::new();
 // A disc key goes only when it names something that is not the player's:
 // a relative path, or one inside a Dolphin user folder (this one or the 2.1
 // install it was copied from). An absolute path somewhere else is a folder
 // they chose, and it stays.
 for (section,key) in DISC_KEYS{
  let Some(value)=doc.get(section,key) else{continue};
  let normalized=value.trim().trim_matches('"').replace('\\',"/");
  if normalized.is_empty(){out.push(((*section).to_string(),(*key).to_string()));continue;}
  if !is_absolute(&normalized)||under(&normalized,user)||inside_another_dolphin_user_folder(&normalized){
   out.push(((*section).to_string(),(*key).to_string()));
  }
 }
 // The count of game-list folders is meaningless once any ISOPath is gone,
 // and meaningful only alongside them.
 if out.iter().any(|(s,k)|s=="General"&&k.starts_with("ISOPath"))
  && !(0..4).any(|i|doc.get("General",&format!("ISOPath{i}")).is_some()&&!out.iter().any(|(s,k)|s=="General"&&k==&format!("ISOPath{i}")))
  && doc.get("General","ISOPaths").is_some()
 {
  out.push(("General".to_string(),"ISOPaths".to_string()));
 }
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
 #[test] fn a_relative_disc_key_and_one_inside_a_dolphin_user_folder_are_removed(){
  let doc=crate::ini::IniDocument::parse("[General]\nISOPaths = 1\nISOPath0 = Games\n[Core]\nDefaultISO = C:\\Users\\Adam\\AppData\\Roaming\\NFL Street Reborn\\dolphin-users\\old\\Games\\NSRV3.iso\n");
  let removed=keys(&path_law_removals(&doc,&user()));
  assert_eq!(removed,["General/ISOPath0","Core/DefaultISO","General/ISOPaths"]);
 }
 /// A game folder the player added in Dolphin's own dialog survives.
 ///
 /// The law used to delete every ISOPath on every launch, and again on every
 /// "Open Dolphin", so a folder added in the dialog the launcher had just
 /// opened was gone before the player got back. It also deleted DefaultISO,
 /// which session::find_game_image reads.
 #[test] fn a_player_game_folder_added_in_dolphin_survives(){
  let doc=crate::ini::IniDocument::parse("[General]\nISOPaths = 2\nISOPath0 = D:/Dumps\nISOPath1 = Games\n[Core]\nDefaultISO = D:/Dumps/NSRV3.iso\n");
  let removed=keys(&path_law_removals(&doc,&user()));
  assert_eq!(removed,["General/ISOPath1"],"only the relative one goes");
  assert!(!removed.contains(&"General/ISOPaths".to_string()),"a surviving folder still needs its count");
  assert!(!removed.contains(&"Core/DefaultISO".to_string()),"their own disc, and the launcher reads this key");
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

// `preserve_existing` lived here. It re-read the FIELDS keys out of the
// player's INI and folded them back over the launcher's overlay, so a Dolphin
// edit to one of those twenty-one keys survived. It was the only protection
// any player setting had, and it could not be extended: it worked by letting
// Dolphin's value win, which meant the launcher's own Settings page could
// never apply anything it covered.
//
// It is gone because the launcher no longer writes a key it does not own. See
// `crate::settings_law`: LOCKED is merged, SEEDED is merged only where the key
// is absent, and PLAYER is never written at all. There is nothing left to
// preserve against.

#[cfg(test)] mod external_edit_tests {

 /// The compatibility requirements are still stated, and a player's edit to a
 /// preference is no longer something the launcher has to defend against,
 /// because nothing above it overwrote the key in the first place.
 #[test] fn a_player_edit_stands_without_weakening_reborn_compatibility(){
  let mut doc=crate::ini::IniDocument::parse("[Display]\nFullscreen = False\n[Interface]\nConfirmStop = True\n[Core]\nMEM1Size = 25165824\nSyncGPU = True\n[Settings]\nInternalResolution = 4\n");
  let mut locked=crate::settings_law::locked_overlay("Dolphin.ini");
  locked.entry("Core".into()).or_default().insert("MEM1Size".into(),"67108864".into());
  doc.merge(&locked);
  doc.merge_absent(&crate::settings_law::seeded_overlay("Dolphin.ini"));
  assert_eq!(doc.get("Core","MEM1Size"),Some("67108864"),"the requirement wins");
  assert_eq!(doc.get("Core","SyncGPU"),Some("True"),"the player's, untouched");
  assert_eq!(doc.get("Settings","InternalResolution"),Some("4"),"the player's, untouched");
  assert_eq!(doc.get("Interface","ConfirmStop"),Some("True"),"seeded False, but theirs is already set");
 }
}
