use super::*;
use reborn_core::controller::{ControllerProfile, GcInput};
use std::collections::BTreeMap;

fn choice_path(p: &Platform) -> Result<PathBuf,String> {
    Ok(p.paths.data_root().map_err(|e|e.to_string())?.join("config/setup-choice.json"))
}
pub fn fresh(p: &Platform) -> bool {
    choice_path(p).ok().and_then(|p|std::fs::read_to_string(p).ok()).as_deref()==Some("fresh")
}

#[tauri::command]
pub async fn setup_install_choice(mode: String) -> Result<(),String> {
    if !["fresh","migrate"].contains(&mode.as_str()) { return Err("Choose a new or existing installation.".into()); }
    let p=Platform::detect();
    let flow=open_flow(None)?;
    if !matches!(flow.next(), Next::Show { stage: Stage::Welcome | Stage::GameVerification, .. }) {
        return Err("Restart setup before changing the installation type.".into());
    }
    let file=choice_path(&p)?;
    std::fs::create_dir_all(file.parent().unwrap()).map_err(|e|e.to_string())?;
    std::fs::write(file,&mode).map_err(|e|e.to_string())?;
    Ok(())
}

fn installed_release(root: &Path) -> Option<serde_json::Value> {
    let file = root.join(".updater/versions.json");
    if std::fs::metadata(&file).ok()?.len() > 16384 { return None; }
    let record: serde_json::Value = serde_json::from_slice(&std::fs::read(file).ok()?).ok()?;
    release_from_record(&record)
}

fn release_from_record(record: &serde_json::Value) -> Option<serde_json::Value> {
    let read = |key: &str| record.get(key).and_then(|v|v.as_str()).map(str::trim)
        .filter(|s| !s.is_empty() && s.len() <= 80 && !s.chars().any(char::is_control));
    let game = read("iso"); let textures = read("textures");
    if game.is_none() && textures.is_none() && read("release").is_none() && read("version").is_none() { return None; }
    // Release 2.1 shipped with the week10 game component; older updaters only recorded component tags.
    let release = read("release").or_else(||read("version")).or_else(||game.filter(|v|v.eq_ignore_ascii_case("week10")).map(|_|"2.1"));
    Some(serde_json::json!({"release":release,"game":game,"textures":textures}))
}

#[tauri::command]
pub async fn setup_install_info() -> Result<serde_json::Value,String> {
    let p=Platform::detect();
    seed_prompt_packages(&p)?;
    let root=discover_install(&p);
    let mut cards=Vec::new();
    if let Some(root)=&root {
        if let Ok(entries)=std::fs::read_dir(root.join("User/GC")) {
            for entry in entries.flatten() {
                let file=entry.path();
                if file.is_file() && file.extension().is_some_and(|e|e.eq_ignore_ascii_case("raw")||e.eq_ignore_ascii_case("gcp")) {
                    cards.push(file.display().to_string());
                } else if file.is_dir() { cards.push(file.display().to_string()); }
            }
        }
    }
    Ok(serde_json::json!({"mode":if fresh(&p){"fresh"}else{"migrate"},"root":root,"memory_cards":cards,"shipped_game":shipped_game(),"installed_release":root.as_ref().and_then(|r|installed_release(r)),"settings_found":root.as_ref().is_some_and(|r|r.join("User/Config/GFX.ini").is_file()||r.join("User/Config/Dolphin.ini").is_file()),"games":root.as_ref().map(|r|legacy_games(r)).unwrap_or_default()}))
}

pub fn seed_prompt_packages(p:&Platform)->Result<(),String> {
    let install=std::env::current_exe().map_err(|e|e.to_string())?.parent().unwrap().to_path_buf();
    let target=reborn_core::mods::packages_root(p).map_err(|e|e.to_string())?;
    let shipped=install.join("content/prompt-packages");
    for id in ["jsgme.xbox-controls","jsgme.ps-controls"] {
        let source=shipped.join(id);
        if source.is_dir() {reborn_core::migration::copy_tree_no_clobber(&source,&target.join(id)).map_err(|e|e.to_string())?;}
    }
    // Every pack the installer carries, so the Mods library is full on first
    // run. Never over a pack that is already there: a player's own copy wins.
    if let Ok(entries)=std::fs::read_dir(install.join("content/mods/packages")) {
        for entry in entries.flatten() {
            let source=entry.path();
            if source.is_dir() {reborn_core::migration::copy_tree_no_clobber(&source,&target.join(entry.file_name())).map_err(|e|e.to_string())?;}
        }
    }
    Ok(())
}

/// The game the installer carries, if this one does: `content\games\NSR.iso`
/// beside the launcher. Not shipped since 2026-09-10 (Adam: "NSR being
/// included within this install is not allowed"); the code stays inert
/// without the file, and setup patches the player's own disc instead.
pub fn shipped_game()->Option<PathBuf> {
    let path=std::env::current_exe().ok()?.parent()?.join("content/games/NSR.iso");
    path.is_file().then_some(path)
}

/// The game patch chain the installer carries (`content\game-patch`), copied
/// beside the library so setup can move an imported disc to the newest build
/// with no download. Never over a chain an update delivered.
pub fn seed_game_patch(p:&Platform)->Result<(),String> {
    let shipped=std::env::current_exe().map_err(|e|e.to_string())?.parent().unwrap().join("content/game-patch");
    if !shipped.join("chain.json").is_file(){return Ok(());}
    let target=reborn_core::game_patch::component_root(&p.paths.data_root().map_err(|e|e.to_string())?);
    reborn_core::migration::copy_tree_no_clobber(&shipped,&target).map(|_|()).map_err(|e|e.to_string())
}

/// Move the disc setup just imported to the newest build, with the shipped
/// chain, before the player goes any further: an old install's disc becomes
/// the current one here rather than in a download after setup. A disc the
/// chain does not know is left as imported. Returns the build's label when
/// a patch was applied.
/// What the shipped patch chain did to the player's disc, so the wizard can
/// say it in words instead of "Imported" for every case. Adam saw "Game
/// File Imported!" on a disc that was already NSRV3 and read it as "it is
/// not patching at all".
pub enum PatchOutcome {
    /// The chain took the disc to its newest build; the label is that build's.
    Applied(String),
    /// The disc already is the chain's newest build.
    UpToDate,
    /// The chain does not know this disc (or refused it); kept as it came.
    Unknown(String),
    /// No chain or no xdelta3 shipped with this build; nothing could run.
    NoChain,
}
impl PatchOutcome {
    pub fn applied(&self)->Option<&str>{ if let PatchOutcome::Applied(l)=self {Some(l)} else {None} }
    pub fn name(&self)->&'static str{ match self { PatchOutcome::Applied(_)=>"patched", PatchOutcome::UpToDate=>"newest", PatchOutcome::Unknown(_)=>"unknown", PatchOutcome::NoChain=>"no-chain" } }
    pub fn note(&self)->Option<&str>{ if let PatchOutcome::Unknown(n)=self {Some(n)} else {None} }
}

fn patch_to_newest(p:&Platform,report:&dyn Fn(&str))->Result<PatchOutcome,String> {
    use reborn_core::game_patch::{apply,find_xdelta,xdelta_dirs,ApplyRequest,Outcome,Xdelta3,BUNDLE_DIR};
    seed_game_patch(p)?;
    let exe_dir=std::env::current_exe().map_err(|e|e.to_string())?.parent().unwrap().to_path_buf();
    let dirs=xdelta_dirs(p,&[exe_dir.join(BUNDLE_DIR)]);
    let Some(xdelta)=find_xdelta(p,&dirs) else {return Ok(PatchOutcome::NoChain)};
    let data=p.paths.data_root().map_err(|e|e.to_string())?;
    let patcher=Xdelta3::new(p,xdelta);
    report("Updating your game to the newest build…");
    // A hard failure here (xdelta3 dying, staging full) must not undo an
    // import that already landed: the disc stays as imported and the wizard
    // says so, instead of reporting the whole import as failed.
    match apply(p,&ApplyRequest{data_root:&data,patcher:&patcher}) {
        Ok(Outcome::Applied(r))=>Ok(PatchOutcome::Applied(r.steps.last().cloned().unwrap_or_default())),
        Ok(Outcome::UpToDate{..})=>Ok(PatchOutcome::UpToDate),
        Ok(Outcome::Refused(r))=>{eprintln!("game not patched during setup: {}",r.message());Ok(PatchOutcome::Unknown(r.message().to_string()))}
        Err(e)=>{eprintln!("game not patched during setup: {e}");Ok(PatchOutcome::Unknown("The update could not be applied right now; the launcher will try again after setup.".to_string()))}
    }
}

#[tauri::command]
pub async fn setup_use_existing(root: String) -> Result<(),String> {
    let p=Platform::detect();let root=reborn_core::session::user_path(&root);
    if LegacyInstall::detect(&root).is_none(){return Err("Choose the Reborn folder containing Games and User.".into());}
    let file=reborn_core::settings::settings_path(&p).map_err(|e|e.to_string())?;
    let mut saved=reborn_core::settings::Settings::load(&file).map_err(|e|e.to_string())?;
    saved.legacy_root=Some(root);
    saved.save(&file).map_err(|e|e.to_string())
}

#[tauri::command]
pub async fn setup_import_game(app: tauri::AppHandle, image: String) -> Result<serde_json::Value,String> {
    tauri::async_runtime::spawn_blocking(move|| {
        use tauri::Emitter;
        let _guard=LaunchGuard::acquire()?;
        let p=Platform::detect();
        let data=p.paths.data_root().map_err(|e|e.to_string())?;
        // The bundle (manifest + reborn.xdelta) is the content component once
        // it has arrived, else the installer's folder beside the launcher. The
        // tools come from wherever this launcher carries them: the AppImage's
        // usr/bin on Linux, the same game-import folder on Windows.
        let exe_dir=std::env::current_exe().map_err(|e|e.to_string())?.parent().unwrap().to_path_buf();
        // A one-file launcher (Linux AppImage) fetches the patch as content;
        // first run reaches this step before any content check has run.
        if !reborn_core::game_patch::bundle_dir(&p,&data).join("manifest.json").is_file()
            || !reborn_core::game_patch::component_root(&data).join(reborn_core::game_patch::CHAIN_FILE).is_file() {
            let _=app.emit("game-import-progress",serde_json::json!({"stage":"Downloading the game patch…","percent":null}));
        }
        let bundle=crate::content_updates::ensure_import_bundle(&p)?;
        let tools=reborn_core::game_patch::find_import_tools(&p,&[exe_dir.join(reborn_core::game_patch::BUNDLE_DIR)]).unwrap_or_else(||bundle.clone());
        let source=reborn_core::session::user_path(&image);
        // Game ID is the sole content acceptance gate. Patch compatibility is
        // separate: preserve an existing/modified image if it cannot use this delta.
        check_reborn_id(&source)?;
        let (output,patched,bundle_reason)=match reborn_core::game_import::import_with_tools(&bundle,&tools,&source,&data,|stage|{let _=app.emit("game-import-progress",stage);}) {
            Ok(output)=>(output,true,None),
            Err(reason) => {
                // The why, in launcher.log: the wizard words it as "could not
                // convert", and a tester's report is that file. A Mac test on
                // 2026-10-08 failed on one altered file (loaddata.dat 4.8 MB,
                // the original is 12.4 MB) and nothing anywhere said so.
                tracing::warn!("import: the patch could not convert {}: {reason}", source.display());
                let _=app.emit("game-import-progress",serde_json::json!({"stage":"Importing your game file...","percent":null}));
                (import_existing(&p,&source)?,false,Some(reason))
            }
        };
        let settings=reborn_core::settings::settings_path(&p).map_err(|e|e.to_string())?;
        let stamp=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis();
        let user=data.join("dolphin-users").join(format!("fresh-{stamp}"));
        std::fs::create_dir_all(&user).map_err(|e|e.to_string())?;
        let _=app.emit("game-import-progress",serde_json::json!({"stage":"Setting up your game…","percent":99}));
        let booted=establish_booted(&p,&data,&output)?;
        // A disc the bundle could not patch (an old NSR.iso, say) may still be
        // one the shipped chain knows. Either way the player ends on the
        // newest build here, or on their own disc if nothing knows it.
        // Three ways to end on the newest build, told apart for the wizard:
        // the bundle patched an original disc ("bundle"), the chain moved an
        // older Reborn disc forward ("patched"), or the disc already was the
        // newest ("newest"). "unknown" is a disc nothing recognised.
        // ⛔ The bundle's output is the build the bundle was made for (9a6fde…),
        // not the newest: the chain has moved on since (seven builds by
        // 2026-10, GameBreaker persistence and port 3/4 uniforms among them),
        // and nothing else walks a fresh import forward until a game-patch
        // update happens to arrive. Found on the Deck port: a fresh install
        // played 9a6fde… with the newest build already on disk. So the
        // bundle's output is walked to the head here too; the wording stays
        // "bundle" because the player imported an original disc.
        let (outcome,label,note)=if patched {
            let o=patch_to_newest(&p,&|stage|{let _=app.emit("game-import-progress",serde_json::json!({"stage":stage,"percent":null}));})?;
            ("bundle".to_string(),Some(o.applied().map(String::from).unwrap_or_else(||"NSRV3".to_string())),o.note().map(String::from))
        } else {
            let o=patch_to_newest(&p,&|stage|{let _=app.emit("game-import-progress",serde_json::json!({"stage":stage,"percent":null}));})?;
            (o.name().to_string(),o.applied().map(String::from),o.note().map(String::from))
        };
        // A disc can patch clean and still not be a build Reborn recognizes as
        // its own (a modified NFL Street 2 shares every byte the patcher
        // checks except the ones that matter). Ask what actually landed and
        // let that override the outcome above rather than trust that patching
        // cleanly means the disc converted.
        //
        // The chain has to be on disk before we ask: patch_to_newest is the
        // only other caller of seed_game_patch, and the bundle branch above
        // never runs it, so a disc that just converted perfectly would find
        // no chain.json here. A missing chain reads to identify() as "not a
        // Reborn build" (Chain::load -> None -> classify's Unconverted), which
        // would condemn the exact disc this whole feature exists to bless.
        seed_game_patch(&p)?;
        let verdict=match dolphin_tool_path(&p){
            Some(tool)=>reborn_core::game_identity::identify(&data,&tool,&booted),
            None=>reborn_core::game_identity::Verdict::Unreadable{reason:DOLPHIN_TOOL_MISSING.to_string()},
        };
        let (outcome,label,note)=match &verdict {
            reborn_core::game_identity::Verdict::DifferentGame{game_id}=>
                ("different-game".to_string(),None,Some(format!("This is not NFL Street 2 (game ID {game_id}). Reborn's roster, textures and updates do not apply to it."))),
            reborn_core::game_identity::Verdict::Unconverted=>
                ("unconverted".to_string(),None,bundle_reason.clone().or_else(||Some("Reborn does not recognize this disc.".into()))),
            reborn_core::game_identity::Verdict::Unreadable{reason}=>
                ("unreadable".to_string(),None,Some(reason.clone())),
            // A disc Reborn DOES recognize, whose patch step refused or could
            // not run. `outcome` is "unknown" or "no-chain" then, and the
            // wizard renders both of those as "Your game file isn't NFL Street
            // Reborn" while Continue stays enabled and the Play badge stays
            // silent: three surfaces disagreeing about one file, and two of
            // them wrong. A patch that did not run is a patch failure, so it
            // gets patch-failure words rather than the verdict's vocabulary.
            reborn_core::game_identity::Verdict::RebornBuild{label:build,..} if outcome=="unknown"||outcome=="no-chain" =>
                ("not-updated".to_string(),Some(build.clone()).filter(|b|!b.is_empty()),note),
            reborn_core::game_identity::Verdict::RebornBuild{..}|reborn_core::game_identity::Verdict::Convertible=>(outcome,label,note),
        };
        // A disc that is NFL Street 2 but not a build Reborn can convert used
        // to end here: saved as the game, reported to the wizard, and left for
        // the launch gate to refuse. The player had done nothing wrong and had
        // nothing to do about it. So recovery runs now, without being asked,
        // because the answer is never in the player's hands and a prompt only
        // delays the one outcome anybody wants.
        //
        // Only `unconverted` recovers. `different-game` is somebody opening
        // another game and is theirs to keep; `unreadable` is a fault in the
        // tool rather than a verdict on the disc, and neither is worth 1.4 GB
        // of somebody's connection on a guess.
        //
        // On success the downloaded image becomes `booted`, so the save below
        // writes the game that works rather than the one that did not. On
        // failure nothing is lost: the disc stays the game, the words carry
        // both reasons, and the launch gate still refuses it.
        let (booted,outcome,label,note)=if outcome=="unconverted" {
            match download_reborn_game(&app) {
                Ok(downloaded)=>{
                    let v=match dolphin_tool_path(&p){
                        Some(tool)=>reborn_core::game_identity::identify(&data,&tool,&downloaded),
                        None=>reborn_core::game_identity::Verdict::Unreadable{reason:DOLPHIN_TOOL_MISSING.to_string()},
                    };
                    let build=match &v {
                        reborn_core::game_identity::Verdict::RebornBuild{label,..}=>Some(label.clone()).filter(|b|!b.is_empty()),
                        _=>None,
                    };
                    // The raw copy the import made of the player's disc goes:
                    // no update knows it, and left as a second original it was
                    // picked over the download whenever it was newer, so online
                    // refused "1070822a…" for good (Deck tester, 2026-10). Only
                    // a copy the chain does not know is removed; the player's
                    // own file was never this copy.
                    if let Some(dir)=output.parent(){
                        let folder=dir.file_name().map(|n|n.to_string_lossy().into_owned()).unwrap_or_default();
                        let known=reborn_core::game_patch::Chain::load(&reborn_core::game_patch::component_root(&data).join(reborn_core::game_patch::CHAIN_FILE)).ok().flatten()
                            .map(|c|!matches!(c.route(&folder),reborn_core::game_patch::Route::Unknown)).unwrap_or(false);
                        if !known && dir.starts_with(reborn_core::game_image::imported_root(&data)) {
                            if let Err(e)=std::fs::remove_dir_all(dir){eprintln!("could not remove the unconverted copy {}: {e}",dir.display());}
                        }
                    }
                    (downloaded,"downloaded".to_string(),build,
                     Some("Your NFL Street 2 disc checks out (game ID GN7E69). Its files differ from the retail release Reborn patches, so Reborn downloaded its ready-made game for you. Your own file was not changed.".to_string()))
                }
                Err(why)=>{
                    let first=note.clone().unwrap_or_else(||"Reborn does not recognize this disc.".into());
                    (booted,"unconverted".to_string(),label,Some(format!("{first} Reborn tried to download its own copy for you and could not: {why}")))
                }
            }
        } else { (booted,outcome,label,note) };
        let patched=patched||outcome=="patched";
        let mut saved=reborn_core::settings::Settings::load(&settings).map_err(|e|e.to_string())?;
        saved.game_path=Some(booted.clone());saved.user_dir=Some(user);saved.graphics_override=None;saved.migrated_texture_root=None;
        saved.save(&settings).map_err(|e|e.to_string())?;
        let _=app.emit("game-import-progress",serde_json::json!({"stage":if patched{"Game File Patched!"}else{"Game File Imported!"},"percent":100}));
        // The pristine after the chain ran, not the one the import landed.
        let pristine=reborn_core::game_image::find_pristine(&data).map(|p|p.path).unwrap_or(output);
        Ok(serde_json::json!({"path":booted,"pristine":pristine,"patched":patched,"outcome":outcome,"label":label,"note":note,"reason":bundle_reason}))
    }).await.map_err(|e|e.to_string())?
}

fn controller(runtime: Option<&str>, device: &str) -> Result<ControllerProfile, String> {
    let p=Platform::detect();
    let scan=runtime.and_then(|exe| {
        let scratch=p.paths.data_root().ok()?.join("cache");
        reborn_core::devices::scan_via_runtime(&p,Path::new(exe),&scratch).ok()
    }).unwrap_or_else(DeviceScanner::scan);
    reborn_core::controller_profiles::adopt_migrated(&p,&scan).map_err(|e|e.to_string())?;
    let pads=scan.playable_pads();
    let (index,pad)=pads.iter().enumerate().find(|(_,p)|p.device_string()==device)
        .ok_or("That controller is no longer connected. Scan again.")?;
    let file=reborn_core::controller_profiles::path(&p).map_err(|e|e.to_string())?;
    let saved=reborn_core::controller_profiles::load(&file).map_err(|e|e.to_string())?;
    Ok(saved.into_iter().find(|p|p.identity.profile_key()==pad.identity().profile_key())
        .unwrap_or_else(||ControllerProfile::new(pad.identity(),(index+1) as u8)))
}

#[tauri::command]
pub async fn controller_slots() -> Result<serde_json::Value,String> {
    let p=Platform::detect();let saved=reborn_core::settings::Settings::load(&reborn_core::settings::settings_path(&p).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
    Ok(serde_json::json!({"slots":saved.controller_slots,"primary":saved.primary_controller}))
}

#[tauri::command]
pub async fn controller_assign_player(player:u8, device:Option<String>, runtime:Option<String>) -> Result<serde_json::Value,String> {
    tauri::async_runtime::spawn_blocking(move|| {
        if !(1..=4).contains(&player){return Err("Choose Player 1 through Player 4.".into());}
        let _guard=if session_active(){None}else{Some(LaunchGuard::acquire()?)};let p=Platform::detect();
        let scan=runtime.as_deref().and_then(|exe|reborn_core::devices::scan_via_runtime(&p,Path::new(exe),&p.paths.data_root().ok()?.join("cache")).ok()).unwrap_or_else(DeviceScanner::scan);
        let mut pads=scan.playable_pads();
        if device.as_ref().is_some_and(|d|!pads.iter().any(|p|p.device_string()==*d)){return Err("That controller disconnected. Scan again before assigning it.".into());}
        let file=reborn_core::settings::settings_path(&p).map_err(|e|e.to_string())?;
        let mut saved=reborn_core::settings::Settings::load(&file).map_err(|e|e.to_string())?;
        pads.sort_by_key(|pad|Some(pad.device_string()).as_ref()!=saved.primary_controller.as_ref());
        let mut slots=saved.controller_slots.unwrap_or_else(||std::array::from_fn(|i|pads.get(i).map(|p|p.device_string())));
        if let Some(device)=&device {for slot in &mut slots {if slot.as_ref()==Some(device){*slot=None;}}}
        let edited=device.clone();
        slots[usize::from(player-1)]=device;
        saved.primary_controller=slots[0].clone();saved.controller_slots=Some(slots.clone());
        let profiles=reborn_core::controller_profiles::load(&reborn_core::controller_profiles::path(&p).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
        crate::live_settings::controllers(&p,runtime.as_deref(),&saved,&profiles,&scan)?;
        saved.save(&file).map_err(|e|e.to_string())?;
        take_back_from_dolphin(&p,runtime.as_deref(),&scan,edited.as_deref(),Some(player));
        Ok(serde_json::json!({"slots":slots}))
    }).await.map_err(|e|e.to_string())?
}

#[tauri::command]
pub async fn setup_controller_mapping(runtime: Option<String>, device: String) -> Result<serde_json::Value,String> {
    tauri::async_runtime::spawn_blocking(move|| {
        let profile=controller(runtime.as_deref(),&device)?;
        // Which of these controls can actually be pressed on THIS pad, asked of
        // the runtime that will resolve them. A pad Windows has no gamepad
        // mapping for reports `Button 3` and `Axis 1-`, nothing in the default
        // binding exists on it, and this page used to tell its owner the
        // mapping was ready. `checked` is false when the runtime could not be
        // asked, and then nothing is called dead: no evidence, no accusation.
        let p=Platform::detect();
        let available=runtime.as_deref().and_then(|exe|{
            let scratch=p.paths.data_root().ok()?.join("cache");
            reborn_core::devices::dump_via_runtime(&p,Path::new(exe),&scratch).ok()
        }).and_then(|(_,inputs)|inputs.into_iter().find(|d|d.device==device)).map(|d|d.all_names()).unwrap_or_default();
        let dead=profile.dead_controls(&available);
        Ok(serde_json::json!({"device":device,"custom":!profile.overridden.is_empty(),"checked":!available.is_empty(),"rows":GcInput::ALL.iter().map(|input|
            serde_json::json!({"key":input.ini_key(),"label":input.label(),"binding":profile.mapping.get(*input),"dead":dead.contains(input)})).collect::<Vec<_>>()}))
    }).await.map_err(|e|e.to_string())?
}

/// A choice made on the Controllers page takes the pad back from a setup the
/// player made in Dolphin's own window, which Play otherwise keeps exactly as
/// written (`reborn_core::session::player_sections`). Without this the page
/// would save a mapping the game never sees.
fn take_back_from_dolphin(p:&Platform,runtime:Option<&str>,scan:&reborn_core::devices::Enumeration,device:Option<&str>,port:Option<u8>){
    let Ok((_,user))=crate::dolphin_settings::config_user(p,runtime) else {return};
    let config=user.join("Config");
    match reborn_core::session::release_player_sections(&config.join("GCPadNew.ini"),&config.join("Dolphin.ini"),scan,device,port){
        Ok(true)=>tracing::info!("controller: the Controllers page took {} back from a setup made in Dolphin",device.unwrap_or("a port")),
        Ok(false)=>{}
        Err(e)=>tracing::warn!("controller: a setup made in Dolphin could not be released: {e}"),
    }
}

#[tauri::command]
pub async fn setup_save_mapping(runtime: Option<String>, device: String, bindings: BTreeMap<String,String>, reset: bool) -> Result<(),String> {
    tauri::async_runtime::spawn_blocking(move|| {
        let _guard=if session_active(){None}else{Some(LaunchGuard::acquire()?)};
        let mut profile=controller(runtime.as_deref(),&device)?;
        if reset { profile.reset(&[]); }
        else {
            for (key,binding) in bindings {
                let input=GcInput::ALL.iter().find(|input|input.ini_key()==key).ok_or("Unknown control")?;
                if binding.is_empty() || binding.len()>160 || binding.contains(['\n','\r','\0']) {
                    return Err("Enter a valid Dolphin input binding.".into());
                }
                profile.rebind(*input,binding);
            }
        }
        let p=Platform::detect();
        let file=reborn_core::controller_profiles::path(&p).map_err(|e|e.to_string())?;
        let saved=reborn_core::settings::Settings::load(&reborn_core::settings::settings_path(&p).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
        let mut profiles=reborn_core::controller_profiles::load(&file).map_err(|e|e.to_string())?;
        profiles.retain(|p|p.identity.profile_key()!=profile.identity.profile_key());profiles.push(profile.clone());
        let scan=runtime.as_deref().and_then(|exe|reborn_core::devices::scan_via_runtime(&p,Path::new(exe),&p.paths.data_root().ok()?.join("cache")).ok()).unwrap_or_else(DeviceScanner::scan);
        crate::live_settings::controllers(&p,runtime.as_deref(),&saved,&profiles,&scan)?;
        reborn_core::controller_profiles::save(&file,profile).map_err(|e|e.to_string())?;
        take_back_from_dolphin(&p,runtime.as_deref(),&scan,Some(&device),None);
        Ok(())
    }).await.map_err(|e|e.to_string())?
}


// Only display adapter names and physical memory are read. No identifiers or network calls.
pub fn hardware_info() -> serde_json::Value {
    static INFO: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();
    INFO.get_or_init(|| {
        #[cfg(windows)] {
            use std::os::windows::process::CommandExt;
            let script = "$ErrorActionPreference='Stop'; $g=@(Get-CimInstance Win32_VideoController | ForEach-Object {$_.Name}); $m=(Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory; @{gpus=$g;memory_gb=[math]::Round($m/1GB)} | ConvertTo-Json -Compress";
            if let Ok(output)=std::process::Command::new("powershell.exe").args(["-NoProfile","-NonInteractive","-Command",script]).creation_flags(0x08000000).output() {
                if output.status.success() {
                    if let Ok(mut info)=serde_json::from_slice::<serde_json::Value>(&output.stdout) {
                        let gpu=info["gpus"].as_array().map(|a|a.iter().filter_map(|g|g.as_str()).filter(|g|!g.to_ascii_lowercase().contains("virtual")&&!g.to_ascii_lowercase().contains("parsec")).collect::<Vec<_>>().join(" · ")).unwrap_or_default();
                        let ram=info["memory_gb"].as_u64().unwrap_or(0);
                        let recommended=recommend_graphics(&gpu,ram);
                        info["recommended"]=serde_json::json!(recommended);
                        info["description"]=serde_json::json!(format!("{gpu} · {ram} GB RAM"));
                        return info;
                    }
                }
            }
        }
        serde_json::json!({"recommended":null})
    }).clone()
}
fn recommend_graphics(gpu:&str,ram:u64)->Option<&'static str> {
    if ram==0 || gpu.is_empty(){return None;}
    if ram<8{return Some("performance");}
    let g=gpu.to_ascii_lowercase();
    let high=["rtx 3060","rtx 3070","rtx 3080","rtx 3090","rtx 4060","rtx 4070","rtx 4080","rtx 4090","rtx 5060","rtx 5070","rtx 5080","rtx 5090","rx 6700","rx 6750","rx 6800","rx 6900","rx 6950","rx 7700","rx 7800","rx 7900","rx 9070"];
    if ram>=16 && !g.contains("laptop") && high.iter().any(|s|g.contains(s)){return Some("quality");}
    if ["geforce gtx","geforce rtx","radeon rx","intel arc"].iter().any(|s|g.contains(s)){return Some("balanced");}
    if ["intel","vega","radeon graphics","radeon 780m","radeon 680m"].iter().any(|s|g.contains(s)){return Some("performance");}
    None
}
#[tauri::command]
pub async fn setup_hardware_info()->Result<serde_json::Value,String> {
    tauri::async_runtime::spawn_blocking(hardware_info).await.map_err(|e|e.to_string())
}
#[cfg(test)]mod hardware_tests {
    use super::*;
    #[test]fn recommendation_is_conservative(){
        assert_eq!(recommend_graphics("",32),None);
        assert_eq!(recommend_graphics("Unknown adapter",32),None);
        assert_eq!(recommend_graphics("NVIDIA GeForce RTX 4070",32),Some("quality"));
        assert_eq!(recommend_graphics("NVIDIA GeForce RTX 4070 Laptop GPU",16),Some("balanced"));
        assert_eq!(recommend_graphics("NVIDIA GeForce RTX 3050",16),Some("balanced"));
        assert_eq!(recommend_graphics("Intel UHD Graphics",16),Some("performance"));
        assert_eq!(recommend_graphics("NVIDIA GeForce RTX 4070",4),Some("performance"));
    }
}

#[tauri::command]
pub async fn setup_graphics_options() -> Result<serde_json::Value,String> {
    let p=Platform::detect();
    let saved=reborn_core::settings::Settings::load(&reborn_core::settings::settings_path(&p).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
    let previous=p.paths.data_root().map_err(|e|e.to_string())?.join("config/migrated-graphics.json");
    let previous=if fresh(&p){None}else{std::fs::read(previous).ok().and_then(|b|serde_json::from_slice::<serde_json::Value>(&b).ok())};
    Ok(serde_json::json!({"current":saved.graphics_override,"previous":previous}))
}
#[tauri::command]
pub async fn setup_save_graphics_options(mode:String,resolution:u32,msaa:u32,ssaa:bool)->Result<(),String>{
    // Resolution and supersampling are the player's now; see
    // reborn_core::settings_law. They used to be refused here and pinned in the
    // per-game layer, which is why neither this page nor Dolphin's own Graphics
    // window could change them.
    if !["preset","previous"].contains(&mode.as_str()){return Err("Choose a graphics option.".into());}
    if !(1..=6).contains(&resolution){return Err("Choose a resolution from 1× to 6× native.".into());}
    let p=Platform::detect();let file=reborn_core::settings::settings_path(&p).map_err(|e|e.to_string())?;
    let mut saved=reborn_core::settings::Settings::load(&file).map_err(|e|e.to_string())?;
    saved.graphics_override=match mode.as_str(){
        "preset"=>None,
        "previous"=>Some(serde_json::from_slice(&std::fs::read(p.paths.data_root().map_err(|e|e.to_string())?.join("config/migrated-graphics.json")).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?),
        _=>Some(reborn_core::settings::GraphicsOverride{label:"Custom".into(),resolution,msaa,ssaa,legacy_gfx:None,legacy_reshade:None})
    };
    if let Some(options)=saved.graphics_override.as_mut(){options.resolution=resolution;options.ssaa=ssaa;}
    saved.save(&file).map_err(|e|e.to_string())
}

fn discover_install(p:&Platform)->Option<PathBuf>{
    let mut candidates=Vec::new();
    // APPDATA can be isolated for previews; USERPROFILE still locates the real Roaming folder.
    for base in [std::env::var_os("APPDATA").map(PathBuf::from),std::env::var_os("USERPROFILE").map(|v|PathBuf::from(v).join("AppData/Roaming"))].into_iter().flatten(){
        for name in ["NFL Street Reborn","nfl-street-reborn-online","DOLPHIN ONLINE TEST"]{candidates.push(base.join(name));}
        if let Ok(entries)=std::fs::read_dir(base){for entry in entries.flatten(){let name=entry.file_name().to_string_lossy().to_lowercase();if name.contains("reborn")||name.contains("dolphin"){candidates.push(entry.path());}}}
    }
    if let Some(root)=default_legacy_root(p){candidates.insert(0,root);}
    if let Ok(exe)=std::env::current_exe(){if let Some(root)=exe.parent(){candidates.push(root.to_path_buf());if let Some(parent)=root.parent(){candidates.push(parent.to_path_buf());}}}
    for root in &candidates {if root.join("User").is_dir()&&root.join("Games").is_dir(){return Some(root.clone());}}
    // Limit discovery to the installed launcher's neighbours, never a drive-wide search.
    for root in candidates {if let Ok(entries)=std::fs::read_dir(root){for e in entries.flatten().take(100){let path=e.path();if path.join("User").is_dir()&&path.join("Games").is_dir(){return Some(path);}}}}
    None
}
fn legacy_games(root:&Path)->Vec<String>{
    let mut games=std::fs::read_dir(root.join("Games")).into_iter().flatten().flatten().map(|e|e.path()).filter(|p|p.is_file()&&p.extension().is_some_and(|e|["iso","rvz","gcm","ciso","gcz"].iter().any(|x|e.eq_ignore_ascii_case(x)))).map(|p|p.display().to_string()).collect::<Vec<_>>();
    games.sort_by_key(|p|(!p.to_ascii_lowercase().ends_with("nsr.iso"),p.clone()));games
}
pub(super) fn check_reborn_id(source:&Path)->Result<(),String>{
    let tool=dolphin_tool_path(&Platform::detect()).ok_or_else(||DOLPHIN_TOOL_MISSING.to_string())?;
    let mut cmd=std::process::Command::new(tool);cmd.arg("header").arg("-i").arg(source).arg("-j");
    #[cfg(windows)]{use std::os::windows::process::CommandExt;cmd.creation_flags(0x08000000);}
    let out=cmd.output().map_err(|e|e.to_string())?;
    let header=serde_json::from_slice::<serde_json::Value>(&out.stdout).map_err(|_|"The game file could not be read. Choose an image with game ID GN7E69.".to_string())?;
    if !out.status.success()||header["game_id"].as_str()!=Some("GN7E69"){return Err("Choose a game file with game ID GN7E69 (NFL Street 2 USA).".into());}Ok(())
}
fn import_existing(p:&Platform,source:&Path)->Result<PathBuf,String>{import_existing_progress(p,source,&|_,_,_|{})}
fn import_existing_progress(p:&Platform,source:&Path,report:&dyn Fn(&str,Option<u8>,u64))->Result<PathBuf,String>{
    use std::io::{Read,Write};
    check_reborn_id(source)?;
    let size=std::fs::metadata(source).map_err(|e|e.to_string())?.len().max(1);
    let hash=reborn_core::game_import::hash_progress(source,|n|report("Reading your game…",Some((25*n/size) as u8),n))?;
    let target=p.paths.data_root().map_err(|e|e.to_string())?.join("library/imported").join(&hash).join(source.file_name().ok_or("Choose a game file")?);
    std::fs::create_dir_all(target.parent().unwrap()).map_err(|e|e.to_string())?;
    if target.exists(){
        if reborn_core::game_import::hash_progress(&target,|n|report("Verifying your game…",Some((25+70*n/size) as u8),n))?!=hash{return Err("The stored game copy has changed. Your original is unchanged.".into());}
    }else{
        let temp=target.with_extension("importing");let mut input=std::fs::File::open(source).map_err(|e|e.to_string())?;let mut output=std::fs::File::create(&temp).map_err(|e|e.to_string())?;
        let mut buffer=vec![0u8;1024*1024];let mut copied=0;
        loop{let n=input.read(&mut buffer).map_err(|e|e.to_string())?;if n==0{break;}output.write_all(&buffer[..n]).map_err(|e|e.to_string())?;copied+=n as u64;report("Copying your game…",Some((25+45*copied/size) as u8),copied);}
        output.sync_all().map_err(|e|e.to_string())?;drop(output);
        if reborn_core::game_import::hash_progress(&temp,|n|report("Verifying your game…",Some((70+25*n/size) as u8),n))?!=hash{return Err("The game copy could not be verified. Your original is unchanged.".into());}
        std::fs::rename(temp,&target).map_err(|e|e.to_string())?;
    }Ok(target)
}

/// The image the player plays is `library/offline/NSR.iso`, a link to the
/// pristine import until a roster is applied into it. The pristine file is
/// never opened by the game, so game updates can be applied to it as deltas.
/// When a roster is already installed it is applied now, so the new image
/// carries it; a Builder that is missing is not an error here (the roster
/// is re-applied on the next update).
fn establish_booted(p:&Platform,data:&Path,pristine:&Path)->Result<PathBuf,String>{
    use reborn_core::game_image::{establish,EstablishOutcome};
    let booted=match establish(p,data,pristine).map_err(|e|e.to_string())?{EstablishOutcome::Done(e)=>e.path,EstablishOutcome::Refused(r)=>return Err(r.message())};
    let roster=data.join(reborn_core::DataDir::LibraryRoster.relative()).join(reborn_core::roster::ROSTER_FILE);
    if roster.is_file(){
        let mut dirs=reborn_core::roster::builder_dirs(p,&[]);
        if let Some(exe_dir)=std::env::current_exe().ok().and_then(|e|e.parent().map(Path::to_path_buf)){dirs.push(exe_dir.join("tools"));dirs.push(exe_dir);}
        if let Ok(appdata)=std::env::var("APPDATA"){dirs.push(PathBuf::from(appdata).join("NFL Street Reborn").join("Tools"));}
        if let Some(builder)=reborn_core::roster::find_builder(p,&dirs){
            // The pack root's PARENT: the Builder appends the game id. See mods::builder_textures_arg.
            let textures=reborn_core::mods::builder_textures_arg(p,&reborn_core::mods::GameId::parse("GN7E69").map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
            match reborn_core::roster::apply(p,&reborn_core::roster::ApplyRequest{data_root:data,roster:&roster,builder:&builder,output:None,textures_root:Some(&textures),accept_warnings:true}){
                Ok(reborn_core::roster::ApplyOutcome::Refused(r))=>eprintln!("roster not applied to the imported game: {}",r.message()),
                Err(e)=>eprintln!("roster not applied to the imported game: {e}"),
                Ok(_)=>{}
            }
        }
    }
    Ok(booted)
}

#[tauri::command]
pub async fn setup_import_existing_game(image:String)->Result<String,String>{
    tauri::async_runtime::spawn_blocking(move||{let _guard=LaunchGuard::acquire()?;let p=Platform::detect();let pristine=import_existing(&p,&reborn_core::session::user_path(&image))?;
        let data=p.paths.data_root().map_err(|e|e.to_string())?;let game=establish_booted(&p,&data,&pristine)?;
        patch_to_newest(&p,&|_|{})?;
        // `patch_to_newest` returning `Unknown` is non-fatal to importing, but
        // that must not make an unrecognized disc the booted game in silence:
        // ask what actually landed and refuse, or record an acknowledgment,
        // the same way every other writer of `settings.game_path` does.
        let verdict=match dolphin_tool_path(&p){
            Some(tool)=>reborn_core::game_identity::identify(&data,&tool,&game),
            None=>reborn_core::game_identity::Verdict::Unreadable{reason:DOLPHIN_TOOL_MISSING.to_string()},
        };
        let file=reborn_core::settings::settings_path(&p).map_err(|e|e.to_string())?;
        // `Consent::NotAsked`, the same rule `may_adopt` applies to an
        // inherited 2.1 disc one click away: setup never adopts a disc that is
        // not a Reborn build. It has a real repair (the full download) right
        // there, so the honest answer is the fault, not a silent escape hatch.
        record_verdict(&file,&game,&verdict,crate::Consent::NotAsked)?;
        let mut saved=reborn_core::settings::Settings::load(&file).map_err(|e|e.to_string())?;saved.game_path=Some(game.clone());
        if fresh(&p)&&saved.user_dir.is_none(){let user=p.paths.data_root().map_err(|e|e.to_string())?.join("dolphin-users/fresh");std::fs::create_dir_all(&user).map_err(|e|e.to_string())?;saved.user_dir=Some(user);}
        saved.save(&file).map_err(|e|e.to_string())?;Ok(game.display().to_string())
    }).await.map_err(|e|e.to_string())?
}

/// How much is on disk under `dir`, for a progress reading measured rather
/// than reported.
///
/// `ContentService::download` has no progress callback, and a 1.36 GB
/// download that says nothing for ten minutes reads as a hung launcher. The
/// staging tree is the download, so its size is the progress.
fn tree_bytes(dir:&Path)->u64{
    let mut total=0u64;let mut stack=vec![dir.to_path_buf()];
    while let Some(d)=stack.pop(){
        let Ok(entries)=std::fs::read_dir(&d) else {continue};
        for entry in entries.flatten(){
            let path=entry.path();
            if path.is_dir(){stack.push(path);}
            else if let Ok(meta)=entry.metadata(){total=total.saturating_add(meta.len());}
        }
    }
    total
}

/// The landed download, as a pristine image.
///
/// `library/game-full` is where the component installs. It is neither layer
/// of the image model: the pristine layer is `library/imported/<sha256>`,
/// and that is what the booted image is made from and what every future game
/// delta applies to (`reborn_core::game_image`). So landing the file is not
/// the end of it, and this is the separate step that finishes the job.
///
/// A hardlink costs no bytes and leaves the component's live tree matching
/// the manifest that was just promoted, so a later repair check still finds
/// the file it installed. Where the filesystem has no links (exFAT), the
/// file is moved instead: a second 1.36 GB copy to hold a landing spot open
/// is a worse trade than re-downloading in the rare case somebody recovers
/// twice on such a disk.
fn land_as_pristine(p:&Platform,data:&Path,landed:&Path,sha:&str)->Result<PathBuf,String>{
    let dir=reborn_core::game_image::pristine_dir(data,sha);
    let target=dir.join(reborn_core::game_image::IMAGE_FILE);
    let same_size=|a:&Path,b:&Path|match (std::fs::metadata(a),std::fs::metadata(b)){(Ok(x),Ok(y))=>x.len()==y.len(),_=>false};
    // Named by the hash of the bytes we just verified, so a folder that is
    // already there holds this build. A different length means an earlier
    // run died part way through; that is the one case worth redoing.
    if target.is_file(){
        // The bytes, not the size: a pristine file can be the same length and
        // hold a different build (a Deck's read "1070822a…" in a folder named
        // for another hash, 2026-10), and keeping it would undo the download.
        let intact=same_size(&target,landed)&&reborn_core::library::hash_file(&target).map(|h|h.as_str()==sha).unwrap_or(false);
        if intact{return Ok(target);}
        std::fs::remove_file(&target).map_err(|e|e.to_string())?;
    }
    std::fs::create_dir_all(&dir).map_err(|e|e.to_string())?;
    if let Err(e)=p.files.hardlink(landed,&target){
        eprintln!("hardlink refused ({e}); moving the downloaded game instead");
        let _=std::fs::remove_file(&target);
        std::fs::rename(landed,&target).map_err(|e|format!("The downloaded game could not be put in place: {e}"))?;
    }
    Ok(target)
}

/// Download the complete built disc, for a player whose own copy could not
/// be converted. Returns the path of the game Reborn will boot.
///
/// # Why this goes through the ordinary content pipeline
///
/// `game-full` is 1.36 GB that a normal install must never fetch, and one
/// exclusion is what keeps that true, not a download stack of its own:
/// `pipeline::updatable` does not list it, so the automatic
/// check/fetch/promote loop never asks for it however many releases pin it.
/// Naming it here, on a button press, is the only way it is fetched at all.
///
/// It is otherwise an ordinary pinned component and carries forward into
/// every later release, so recovery stays available to the player who needs
/// it long after the release that published the disc. An earlier design kept
/// it out of the carry-forward instead, which gated nothing (a pin is a line
/// of JSON) and quietly retired recovery on the next ordinary publish.
///
/// # Why landing the file is not the end of it
///
/// The component installs into `library/game-full`, which is neither layer
/// of the image model. A downloaded disc is a PRISTINE image, so it is
/// linked into `library/imported/<sha256>` and the booted image is
/// established from there, exactly as an import does. That is what lets the
/// next game update reach it as a delta.
#[tauri::command]
pub async fn setup_download_reborn_game(app: tauri::AppHandle) -> Result<String,String> {
    tauri::async_runtime::spawn_blocking(move|| {
        let _guard=LaunchGuard::acquire()?;
        Ok(download_reborn_game(&app)?.display().to_string())
    }).await.map_err(|e|e.to_string())?
}

/// Download the Reborn game and make it the booted image, returning it.
///
/// Split out of the command above so `setup_import_game` can run recovery
/// itself the moment a disc turns out not to be convertible, without the
/// player being asked to go and press a button. The guard is the reason this
/// is a plain function and not a call to the command: `LaunchGuard::acquire`
/// is a compare_exchange, so a caller that already holds it would be told the
/// launcher is busy by its own recovery step.
pub(crate) fn download_reborn_game(app:&tauri::AppHandle)->Result<PathBuf,String>{
    use tauri::Emitter;
    use std::sync::atomic::{AtomicBool,Ordering};
    use reborn_core::content::pipeline::{CheckRequest,ComponentState,Depth,FetchOutcomeState,ImageHook,PromoteState,Updater};
    let p=Platform::detect();
    let data=p.paths.data_root().map_err(|e|e.to_string())?;
    let say=|stage:&str,percent:Option<u8>|{let _=app.emit("game-import-progress",serde_json::json!({"stage":stage,"percent":percent}));};
    // The chain the installer carries, before anything asks what a disc
    // is: `game_identity::identify` reads `Unconverted` when there is no
    // chain to consult, which would condemn the very build this command
    // downloads. Same reason `setup_import_game` seeds it.
    seed_game_patch(&p)?;
    // A one-file launcher (Linux AppImage) ships no chain: download it.
    crate::content_updates::ensure_patch_chain(&p)?;

    say("Looking for the Reborn game…",Some(0));
    let settings_file=reborn_core::settings::settings_path(&p).map_err(|e|e.to_string())?;
    let settings=reborn_core::settings::Settings::load(&settings_file).map_err(|e|e.to_string())?;
    let verifier=crate::content_updates::verifier()?;
    let origin=reborn_core::origin::Origin::new(&crate::content_updates::origin_url(&settings));
    let updater=Updater::new(&p,&data,&origin,&verifier);
    // Verify, not Quick: a pointer left by an earlier attempt must not
    // report a game file that is no longer there as installed.
    let check=updater.check(&CheckRequest{components:&[ComponentId::GameFull],depth:Depth::Verify}).map_err(|e|e.to_string())?;
    match &check.component(ComponentId::GameFull).ok_or("The Reborn game download could not be looked up.")?.state {
        // Still reachable, and kept: an origin that has never published
        // the disc carries no pin for it, which covers a mirror, a
        // `content_origin` set in settings, and the product's own origin
        // before the first `rebornctl publish game-full` ever runs. What
        // it no longer means is "try again in a moment": now that the pin
        // carries forward, this state does not clear itself, so the words
        // say so and ask for it to be reported.
        ComponentState::NotCarried=>return Err("Your update source is not offering NFL Street Reborn, so there is nothing to download and nothing was changed. The download is meant to always be available, so please report this.".into()),
        ComponentState::NotUpdatable=>return Err("This build of the launcher cannot download the Reborn game.".into()),
        ComponentState::Problem(why)=>return Err(why.clone()),
        ComponentState::Refused(r)=>return Err(r.message()),
        ComponentState::UpToDate{..}|ComponentState::Available(_)=>{}
    }

    if check.has_updates() {
        let total=check.download_bytes();
        let staging=check.available().next().map(|(_,plan)|updater.service().layout().staging_dir(plan.component(),plan.version()))
            .ok_or("The Reborn game download could not be prepared.")?;
        say("Downloading NFL Street Reborn…",Some(0));
        let done=std::sync::Arc::new(AtomicBool::new(false));
        let watcher={
            let (done,app,staging)=(done.clone(),app.clone(),staging.clone());
            std::thread::spawn(move||{
                while !done.load(Ordering::SeqCst) {
                    std::thread::sleep(std::time::Duration::from_millis(750));
                    if done.load(Ordering::SeqCst){break;}
                    let percent=if total>0 {(90*tree_bytes(&staging)/total).min(90) as u8} else {0};
                    let _=app.emit("game-import-progress",serde_json::json!({"stage":"Downloading NFL Street Reborn…","percent":percent}));
                }
            })
        };
        let fetched=updater.fetch(&check);
        done.store(true,Ordering::SeqCst);
        let _=watcher.join();
        let fetched=fetched.map_err(|e|e.to_string())?;
        if !fetched.all_staged() {
            let why=fetched.not_staged().map(|c|match &c.state{
                FetchOutcomeState::Incomplete{remaining_bytes,..}=>format!("{} MB still to come",(remaining_bytes+1_048_575)/1_048_576),
                FetchOutcomeState::NotReady{..}=>"the downloaded file did not verify".to_string(),
                FetchOutcomeState::Staged{..}=>String::new(),
            }).collect::<Vec<_>>().join("; ");
            return Err(format!("The download did not finish ({why}). What arrived is kept, so trying again carries on from there."));
        }
        say("Installing NFL Street Reborn…",Some(92));
        let promoted=updater.promote(&check,&ImageHook::default()).map_err(|e|e.to_string())?;
        if !promoted.all_applied() {
            let why=promoted.components.iter().filter_map(|c|match &c.state{
                PromoteState::Refused(r)=>Some(r.message()),
                PromoteState::RolledBack{reason,..}=>Some(reason.clone()),
                PromoteState::Applied{..}=>None,
            }).collect::<Vec<_>>().join(" ");
            return Err(format!("The downloaded game could not be put in place. {why}"));
        }
    }

    // The component publishes one file, under the name the booted image
    // ships as: see `publish::game_full::ISO_FILE_NAME`, which is
    // documented as matching `game_image::IMAGE_FILE` so that this step
    // needs no rename.
    let landed=data.join(reborn_core::DataDir::LibraryGameFull.relative()).join(reborn_core::game_image::IMAGE_FILE);
    if !landed.is_file(){return Err("The download left no game file behind. Try again.".into());}

    // What actually arrived, before any of it becomes the game. A
    // downloaded disc that is not a Reborn build means the origin served
    // something wrong, and the whole point of this feature is that such
    // an image never quietly becomes the booted game. So this refuses
    // rather than recording an acknowledgment: nobody chose this file.
    say("Checking the download…",Some(94));
    let verdict=match dolphin_tool_path(&p){
        Some(tool)=>reborn_core::game_identity::identify(&data,&tool,&landed),
        None=>reborn_core::game_identity::Verdict::Unreadable{reason:DOLPHIN_TOOL_MISSING.to_string()},
    };
    if !verdict.is_reborn() {
        return Err(match &verdict {
            reborn_core::game_identity::Verdict::Unreadable{reason}=>format!("The downloaded game could not be read: {reason} Nothing was changed."),
            _=>"The downloaded game is not a Reborn build, so it was not installed and nothing was changed. Try again, and report it if it happens twice.".to_string(),
        });
    }
    // Whether what was served is the chain's newest build. It need not be:
    // the pin is published once and carried forward, so the disc can sit
    // several game-patch builds behind by the time anybody downloads it.
    let at_head=matches!(verdict,reborn_core::game_identity::Verdict::RebornBuild{at_head:true,..});

    // A second pass over the file, for the folder the pristine layer
    // names it by. `identify` hashes to answer what the disc is but does
    // not hand the hash back, and asking it the question through any
    // other door is what every guard in this feature exists to stop.
    say("Setting up your game…",Some(97));
    let sha=reborn_core::library::hash_file(&landed).map_err(|e|e.to_string())?.as_str().to_string();
    let pristine=land_as_pristine(&p,&data,&landed,&sha)?;
    let booted=establish_booted(&p,&data,&pristine)?;

    // Walk the downloaded disc to the chain head, exactly as both imports
    // do. The check above asked the content engine for `game-full` and
    // nothing else, so no game patch has moved this image, and the pin
    // carrying forward into every later release is precisely what lets a
    // disc published months ago be what a player receives today. Doing it
    // here rather than leaving it to the automatic update loop keeps the
    // ordering something this command decides rather than something it
    // hopes for.
    //
    // Non-fatal, like both imports, and for a stronger reason here:
    // `identify` has already proven this is a Reborn build, so a refusal
    // cannot mean "a disc nothing recognizes". It means a missing
    // xdelta3, a file held open, or a broken game-patch component, and
    // none of those make what was just established unplayable. Failing
    // would strand a player who has downloaded 1.36 GB behind a fault
    // the ordinary game-patch update clears on its own, and every refusal
    // in `game_patch::apply` leaves the image as it was. The card's words
    // stay true either way: it says the game was downloaded and verified,
    // never that it is the newest build.
    let moved=patch_to_newest(&p,&|stage|say(stage,Some(98)))?;
    if !at_head&&!matches!(moved,PatchOutcome::Applied(_)|PatchOutcome::UpToDate) {
        eprintln!(
            "the downloaded game is behind the newest build and could not be moved forward now: {}",
            moved.note().unwrap_or("no delta tool or update chain is installed")
        );
    }

    let mut saved=reborn_core::settings::Settings::load(&settings_file).map_err(|e|e.to_string())?;
    saved.game_path=Some(booted.clone());
    // A verified Reborn build retires the escape hatch: the
    // acknowledgment was given for the image that could not be
    // converted, and that image is not the game any more.
    saved.unverified_game_ack=None;
    if fresh(&p)&&saved.user_dir.is_none(){let user=data.join("dolphin-users/fresh");std::fs::create_dir_all(&user).map_err(|e|e.to_string())?;saved.user_dir=Some(user);}
    saved.save(&settings_file).map_err(|e|e.to_string())?;
    say("NFL Street Reborn is ready",Some(100));
    Ok(booted)
}

/// Whether a 2.1 installation's disc may be adopted as this install's game.
/// Only a build Reborn recognizes, or an unmodified disc it can convert.
/// Anything else goes to recovery rather than quietly becoming the game.
fn may_adopt(verdict:&reborn_core::game_identity::Verdict)->bool{
    verdict.is_reborn()||matches!(verdict,reborn_core::game_identity::Verdict::Convertible)
}

#[tauri::command]
pub async fn setup_migrate_install(app:tauri::AppHandle,root:String,image:String)->Result<serde_json::Value,String>{
    let cards=tauri::async_runtime::spawn_blocking(move||->Result<Vec<String>,String>{
        use tauri::Emitter;
        let last=std::cell::Cell::new(std::time::Instant::now()-std::time::Duration::from_secs(1));
        let report=|stage:&str,percent:Option<u8>,bytes:u64|{if last.get().elapsed().as_millis()>=100 || percent==Some(100){last.set(std::time::Instant::now());let _=app.emit("install-import-progress",serde_json::json!({"stage":stage,"percent":percent,"bytes":bytes}));}};
        let _guard=LaunchGuard::acquire()?;let p=Platform::detect();let root=reborn_core::session::user_path(&root);
        let flow=open_flow(Some(root.display().to_string()))?;
        if !matches!(flow.next(), Next::Show{stage:Stage::GameVerification,..}){return Err("Return to the game import step before importing an installation.".into());}
        if !root.join("User").is_dir(){return Err("Choose the old installation folder containing User.".into());}
        for name in ["Dolphin.exe","NFL Street Reborn Dolphin.exe","runtime/Dolphin.exe"]{
            reborn_core::runtime::ensure_runtime_idle(&root.join(name)).map_err(|e|e.to_string())?;
        }
        // The bundled runtime by whichever name this build gave the emulator.
        // 3.1.0 renamed Dolphin.exe to NFL Street Reborn Dolphin.exe, and a
        // fixed file name here refused every import against that runtime.
        let _=crate::content_updates::ensure_runtime(&p);
        let runtime=match reborn_core::session::resolve_runtime(&p,None,None){Ok(reborn_core::session::Runtime::Bundled(path))=>path,_=>return Err("The new launcher runtime is missing. Reinstall the complete package.".into())};
        let source=reborn_core::session::user_path(&image);check_reborn_id(&source)?;
        let data=p.paths.data_root().map_err(|e|e.to_string())?;
        let folder=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis();
        let user=data.join("dolphin-users").join(format!("migrated-{folder}"));
        let mut copied=0;
        report("Copying saves and settings…",None,0);
        // Saves and settings, not the 2 GB texture pack: the installer ships
        // the pack, the composed folder is built from it at every launch, and
        // the 2.1 pack stays where it is, intact.
        let bytes=reborn_core::migration::copy_tree_excluding(&root.join("User"),&user,&["Load/Textures"],&mut |n|{copied+=n;report("Copying saves and settings…",None,copied);}).map_err(|e|e.to_string())?;
        rebase_user_paths(&root,&user)?;
        let pristine=import_existing_progress(&p,&source,&report)?;
        report("Setting up your game…",Some(96),copied);
        let game=establish_booted(&p,&data,&pristine)?;
        // The old install's disc becomes the newest build now, offline, with
        // the shipped chain: "I already have Reborn" changes the disc rather
        // than copying the old one over.
        patch_to_newest(&p,&|stage|report(stage,None,copied))?;
        // The copied memory cards, brought up to date with the installed
        // roster while the game is known to be closed. Facts for the page,
        // never an error: the import itself has already succeeded, and a
        // roster that is not installed yet simply says nothing (the first
        // content update and every launch run the same sync).
        report("Bringing your saves up to date…",None,copied);
        let cards=match reborn_core::cards::after_apply(&p,&data,&user,&[]){Ok(outcome)=>outcome.facts(),Err(e)=>{eprintln!("memory cards after the import: {e}");vec![reborn_core::cards::NOT_SYNCED.to_string()]}};
        let pad_ini=std::fs::read_to_string(user.join("Config/GCPadNew.ini")).unwrap_or_default();
        std::fs::create_dir_all(data.join("config")).map_err(|e|e.to_string())?;
        std::fs::write(data.join("config/migrated-controller.ini"),&pad_ini).map_err(|e|e.to_string())?;
        let pad_doc=reborn_core::ini::IniDocument::parse(&pad_ini);
        let scan=reborn_core::devices::scan_via_runtime(&p,&runtime,&data.join("cache")).unwrap_or_else(|_|DeviceScanner::scan());
        if let Some(pad)=scan.playable_pads().first(){
            let mut profile=ControllerProfile::new(pad.identity(),1);
            for input in GcInput::ALL{if let Some(binding)=pad_doc.get("GCPad1",input.ini_key()){if !binding.is_empty(){profile.rebind(*input,binding.to_string());}}}
            reborn_core::controller_profiles::save(&reborn_core::controller_profiles::path(&p).map_err(|e|e.to_string())?,profile).map_err(|e|e.to_string())?;
        }
        let gfx=std::fs::read_to_string(user.join("Config/GFX.ini")).unwrap_or_default();
        let options=reborn_core::settings::GraphicsOverride{label:"Previous settings".into(),resolution:3,msaa:1,ssaa:false,legacy_gfx:Some(gfx),legacy_reshade:migrate_effects(&root,&data.join("migrated-effects").join(folder.to_string()))?};
        std::fs::create_dir_all(data.join("config")).map_err(|e|e.to_string())?;
        std::fs::write(data.join("config/migrated-graphics.json"),serde_json::to_vec_pretty(&options).unwrap()).map_err(|e|e.to_string())?;
        const UNVERIFIED_MIGRATED_GAME:&str="The game file in your old installation isn't an NFL Street Reborn build, so it was not brought over. Choose your NFL Street 2 disc on the next step and Reborn will convert it.";
        let inherited=match dolphin_tool_path(&p){
            Some(tool)=>reborn_core::game_identity::identify(&data,&tool,&game),
            None=>reborn_core::game_identity::Verdict::Unreadable{reason:DOLPHIN_TOOL_MISSING.to_string()},
        };
        let game=if may_adopt(&inherited){game.clone()}else{return Err(UNVERIFIED_MIGRATED_GAME.to_string())};
        let file=reborn_core::settings::settings_path(&p).map_err(|e|e.to_string())?;let mut saved=reborn_core::settings::Settings::load(&file).map_err(|e|e.to_string())?;
        saved.legacy_root=Some(root.clone());saved.user_dir=Some(user.clone());saved.game_path=Some(game.clone());saved.runtime_exe=Some(runtime.clone());saved.graphics_override=Some(options);saved.manage_texture_folder=Some(true);
        saved.save(&file).map_err(|e|e.to_string())?;
        // Only recognizable old screenshots are moved; other game files and textures remain untouched.
        if let Ok(entries)=std::fs::read_dir(&root){for e in entries.flatten(){let source=e.path();let name=e.file_name().to_string_lossy().into_owned();if source.is_file()&&name.starts_with("NFL Street Reborn Dolphin ")&&source.extension().is_some_and(|e|["png","jpg","jpeg","bmp"].iter().any(|x|e.eq_ignore_ascii_case(x))){let dest=root.join("Old Screenshots");std::fs::create_dir_all(&dest).map_err(|e|e.to_string())?;let mut target=dest.join(&name);let mut suffix=1;
            while target.exists(){target=dest.join(format!("{} ({suffix}).{}",source.file_stem().unwrap().to_string_lossy(),source.extension().unwrap().to_string_lossy()));suffix+=1;}
            std::fs::rename(&source,target).map_err(|e|e.to_string())?;}}}
        let mut flow=open_flow(None)?;
        for (stage,answer) in [(Stage::GameVerification,Satisfaction::GameConfirmed{total_bytes:Some(std::fs::metadata(&game).map_err(|e|e.to_string())?.len())}),(Stage::MigrationOffer,Satisfaction::ImportingCurrent{source:root}),(Stage::MigrationCopy,Satisfaction::Migrated{bytes})]{
            flow.reach(stage).map_err(|e|e.to_string())?;
            if let Advance::Refused(why)=flow.satisfy(stage,answer).map_err(|e|e.to_string())?{return Err(why.message());}
        }
        report("Import complete. Continue to your controller.",Some(100),copied);
        Ok(cards)
    }).await.map_err(|e|e.to_string())??;
    Ok(serde_json::json!({"imported":true,"cards":cards}))
}

fn migrate_effects(root:&Path,destination:&Path)->Result<Option<String>,String>{
    let Ok(text)=std::fs::read_to_string(root.join("ReShade.ini"))else{return Ok(None)};
    let doc=reborn_core::ini::IniDocument::parse(&text);let mut overlay=reborn_core::ini::Overlay::new();
    let general=overlay.entry("GENERAL".into()).or_default();
    std::fs::create_dir_all(destination).map_err(|e|e.to_string())?;
    for key in ["PresetPath","StartupPresetPath"]{if let Some(value)=doc.get("GENERAL",key){let path=PathBuf::from(value.trim_matches('"'));let source=if path.is_absolute(){path}else{root.join(path)};if source.is_file(){let target=destination.join(format!("{key}.ini"));std::fs::copy(&source,&target).map_err(|e|e.to_string())?;general.insert(key.into(),target.display().to_string());}}}
    for key in ["EffectSearchPaths","TextureSearchPaths"]{if let Some(value)=doc.get("GENERAL",key){let mut copied=Vec::new();for (i,value) in value.split(',').enumerate(){if value.trim().is_empty(){continue;}let recursive=value.trim().ends_with("**");let path=PathBuf::from(value.trim().trim_end_matches('*').trim_end_matches(['/', '\\']));let source=if path.is_absolute(){path}else{root.join(path)};if source.is_dir(){let target=destination.join(format!("{key}-{i}"));if std::fs::canonicalize(&source).ok()==std::fs::canonicalize(root).ok(){
            // A common search path is just ".". Copy its shader/image files,
            // rather than duplicating the games and saves beside them.
            std::fs::create_dir_all(&target).map_err(|e|e.to_string())?;
            for entry in std::fs::read_dir(&source).map_err(|e|e.to_string())?.flatten(){let file=entry.path();if file.is_file()&&file.extension().is_some_and(|e|["fx","fxh","png","dds","jpg","jpeg","bmp"].iter().any(|x|e.eq_ignore_ascii_case(x))){std::fs::copy(&file,target.join(entry.file_name())).map_err(|e|e.to_string())?;}}
        }else{reborn_core::migration::copy_tree_no_clobber(&source,&target).map_err(|e|e.to_string())?;}copied.push(format!("{}{}",target.display(),if recursive{"/**"}else{""}));}}if !copied.is_empty(){general.insert(key.into(),copied.join(","));}}}
    let mut result=reborn_core::ini::IniDocument::parse("");result.merge(&overlay);Ok(Some(result.render()))
}



// Make the copied configuration use the copied saves, including memory cards
// explicitly stored outside the old User folder.
fn rebase_user_paths(root:&Path,user:&Path)->Result<(),String>{
    let old=root.join("User");
    for folder in ["Config","GameSettings"]{
        if let Ok(entries)=std::fs::read_dir(user.join(folder)){for entry in entries.flatten(){
            let file=entry.path();if !file.extension().is_some_and(|e|e.eq_ignore_ascii_case("ini")){continue;}
            if let Ok(text)=std::fs::read_to_string(&file){
                let rebased=text.replace(&old.display().to_string(),&user.display().to_string()).replace(&old.display().to_string().replace('\\',"/"),&user.display().to_string().replace('\\',"/"));
                if text!=rebased{std::fs::write(file,rebased).map_err(|e|e.to_string())?;}
            }
        }}
    }
    let file=user.join("Config/Dolphin.ini");let Ok(text)=std::fs::read_to_string(&file)else{return Ok(())};
    let mut doc=reborn_core::ini::IniDocument::parse(&text);let mut overlay=reborn_core::ini::Overlay::new();
    for key in ["MemoryCardAPath","MemoryCardBPath","GCIFolderAPath","GCIFolderBPath"]{
        if let Some(value)=doc.get("Core",key).filter(|v|!v.trim().is_empty()){
            let path=PathBuf::from(value.trim_matches('"'));let source=if path.is_absolute(){path}else{root.join(path)};
            if source.starts_with(user){continue;}
            let target=user.join("GC").join(format!("Imported-{key}{}",if source.is_file(){".raw"}else{""}));
            if source.is_file(){std::fs::create_dir_all(target.parent().unwrap()).map_err(|e|e.to_string())?;std::fs::copy(&source,&target).map_err(|e|e.to_string())?;}
            else if source.is_dir(){reborn_core::migration::copy_tree_no_clobber(&source,&target).map_err(|e|e.to_string())?;}else{continue;}
            overlay.entry("Core".into()).or_default().insert(key.into(),target.display().to_string());
        }
    }
    if !doc.merge(&overlay).is_noop(){std::fs::write(file,doc.render()).map_err(|e|e.to_string())?;}Ok(())
}

#[cfg(test)] mod migration_path_tests {
    use super::*;
    #[test] fn prior_release_reads_game_and_texture_versions_independently() {
        let versions=release_from_record(&serde_json::json!({"iso":"week10","textures":"week9"})).unwrap();
        assert_eq!(versions["release"],"2.1");assert_eq!(versions["game"],"week10");assert_eq!(versions["textures"],"week9");
        assert!(release_from_record(&serde_json::json!({"iso":123,"textures":" "})).is_none());
        assert!(release_from_record(&serde_json::json!({"iso":"week10\ninvalid"})).is_none());
        assert_eq!(release_from_record(&serde_json::json!({"iso":" week10 "})).unwrap()["game"],"week10");
    }
    #[test] fn imported_configuration_uses_copied_saves() {
        let base=std::env::temp_dir().join(format!("reborn-migration-paths-{}-{}",std::process::id(),now_ms()));
        let root=base.join("old");let user=base.join("new/User");
        std::fs::create_dir_all(root.join("User/GC")).unwrap();std::fs::create_dir_all(user.join("Config")).unwrap();std::fs::create_dir_all(user.join("GC")).unwrap();
        let card=base.join("external.raw");std::fs::write(&card,b"save data").unwrap();
        let config=format!("[Core]\nMemoryCardAPath = {}\nMemoryCardBPath = {}\n",card.display(),root.join("User/GC/B.raw").display());
        std::fs::write(user.join("Config/Dolphin.ini"),config).unwrap();
        rebase_user_paths(&root,&user).unwrap();
        let text=std::fs::read_to_string(user.join("Config/Dolphin.ini")).unwrap();let doc=reborn_core::ini::IniDocument::parse(&text);
        assert_eq!(std::fs::read(doc.get("Core","MemoryCardAPath").unwrap()).unwrap(),b"save data");
        assert_eq!(PathBuf::from(doc.get("Core","MemoryCardBPath").unwrap()),user.join("GC/B.raw"));
        assert_eq!(std::fs::read(&card).unwrap(),b"save data");
        std::fs::remove_dir_all(base).unwrap();
    }
}

/// `may_adopt` is Door 3's decision: whether a 2.1 installation's disc may
/// become this install's game. Every verdict is pinned here so widening the
/// condition (or dropping the `Convertible` arm) compiles and passes
/// everything else while quietly adopting an image Reborn cannot vouch for.
#[cfg(test)] mod door3_tests {
    use super::*;
    #[test] fn may_adopt_refuses_an_unreadable_image() {
        assert!(!may_adopt(&reborn_core::game_identity::Verdict::Unreadable{reason:"truncated".to_string()}),
            "an Unreadable image that may_adopt accepted would mean a file that is not a disc becomes this install's game");
    }
    #[test] fn may_adopt_refuses_an_unconverted_disc() {
        assert!(!may_adopt(&reborn_core::game_identity::Verdict::Unconverted),
            "an Unconverted disc that may_adopt accepted would silently carry a player's own modified NFL Street 2 into this install as its game, with no acknowledgment ever recorded, which is exactly the disc this task's Task 1 tests exist to catch");
    }
    #[test] fn may_adopt_refuses_a_different_game() {
        assert!(!may_adopt(&reborn_core::game_identity::Verdict::DifferentGame{game_id:"GAFE01".to_string()}),
            "a disc for a different game entirely must never become this install's game just because a 2.1 folder pointed at it");
    }
    #[test] fn may_adopt_accepts_a_convertible_disc() {
        assert!(may_adopt(&reborn_core::game_identity::Verdict::Convertible),
            "an unmodified retail disc must be adoptable; refusing it would send every ordinary migration to recovery for no reason");
    }
    #[test] fn may_adopt_accepts_a_reborn_build() {
        assert!(may_adopt(&reborn_core::game_identity::Verdict::RebornBuild{label:"3.0.2".to_string(),at_head:true}),
            "a genuine Reborn build must be adoptable; refusing it would send every ordinary migration to recovery for no reason");
    }
}
