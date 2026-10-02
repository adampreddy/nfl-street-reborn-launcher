use super::*;
use reborn_core::ini::{IniDocument, Overlay};

/// One core error, or one raw operating system error, as the sentence the
/// player reads.
///
/// `online.js` renders whatever these commands reject with, verbatim. Every
/// call here used to hand it `e.to_string()`, which for `reborn_core::Error`
/// is `io error at {path}: {source}` and for a bare `std::io::Error` is the
/// operating system's own wording and number. So a configuration file another
/// program had open, or a full disk, put a path and `os error 32` on the
/// Online page.
///
/// `Error::player_message` decides what is shown, once, for the whole
/// workspace: a refusal somebody wrote for a player comes through unchanged,
/// and a fault becomes one of two sentences with the detail going to the log.
/// Taking `impl Into<Error>` rather than `Error` is what lets the bare
/// `std::fs` failures beside them go through the same door.
fn player(e: impl Into<reborn_core::Error>) -> String {
    e.into().player_message().to_string()
}

use reborn_core::dolphin_preferences::{Field, FIELDS};
pub(crate) fn config_user(p: &Platform, runtime: Option<&str>) -> Result<(PathBuf,PathBuf),String> {
    let exe=runtime_for(p,runtime).ok_or("Dolphin was not found. Choose its location in Files and storage.")?;
    let prefs=reborn_core::settings::Settings::load(&reborn_core::settings::settings_path(p).map_err(player)?).map_err(player)?;
    let user=reborn_core::session::resolve_user_dir_for(p,prefs.user_dir.as_deref(),Some(&exe)).map_err(player)?;
    Ok((exe,user.path().to_path_buf()))
}
fn read_ini(path:&Path)->Result<IniDocument,String>{
    match std::fs::read_to_string(path){Ok(s)=>Ok(IniDocument::parse(&s)),Err(e) if e.kind()==std::io::ErrorKind::NotFound=>Ok(IniDocument::parse("")),Err(e)=>Err(player(e))}
}
fn checked_value(field:&Field,value:&str)->Result<String,String>{
    if value.len()>2048||value.chars().any(char::is_control){return Err("Enter a valid setting value.".into());}
    match field.kind{
        "bool"=>match value.to_lowercase().as_str(){"true"=>Ok("True".into()),"false"=>Ok("False".into()),_=>Err("Choose on or off.".into())},
        "volume"|"latency"=>{let n=value.parse::<u32>().map_err(|_|"Enter a whole number.")?;let max=if field.kind=="volume"{100}else{200};if n>max{return Err(format!("Choose a value from 0 to {max}."));}Ok(n.to_string())},
        "backend"=>if ["Cubeb","WASAPI (Exclusive Mode)"].contains(&value){Ok(value.into())}else{Err("Choose a supported audio output mode.".into())},
        // Spelled exactly as Dolphin spells them: CONFIG_NAME in
        // VideoBackends/*/VideoBackend.h. A name Dolphin does not recognise
        // falls back to the default backend silently, so a typo here would
        // look like the setting being ignored, which is the bug this control
        // exists to fix. Null and Software Renderer are debugging backends
        // that cannot play the game, so they are neither offered nor accepted.
        "video_backend"=>if ["D3D","D3D12","Vulkan","OGL"].contains(&value){Ok(value.into())}else{Err("Choose a supported graphics backend.".into())},
        "slot"=>if ["1","8","255"].contains(&value){Ok(value.into())}else{Err("Choose a supported memory card type.".into())},
        // Written the way Dolphin writes it, so the file reads the same whoever last saved it.
        "msaa"=>{let n=value.trim_start_matches("0x").trim_start_matches('0');let n=if n.is_empty(){"0"}else{n};if ["1","2","4","8"].contains(&n){Ok(format!("0x{:08x}",n.parse::<u32>().unwrap()))}else{Err("Choose 1×, 2×, 4× or 8× anti-aliasing.".into())}},
        // 1x to 6x native. Capped at 6 because the ReShade depth filter is
        // derived from this (reshade::depth_filter_for) and the buffers get
        // very large; Auto (0) is refused for the same reason, since there is
        // no multiplier to derive a filter from.
        "resolution"=>{let n=value.trim().parse::<u8>().map_err(|_|"Choose a resolution.")?;if (1..=6).contains(&n){Ok(n.to_string())}else{Err("Choose a resolution from 1× to 6× native.".into())}},
        "card"|"folder"=>{if !value.is_empty(){let path=Path::new(value);if !path.is_absolute(){return Err("Choose an absolute path.".into());}if field.kind=="card"&&(!path.is_file()||!path.extension().is_some_and(|e|e.eq_ignore_ascii_case("raw")||e.eq_ignore_ascii_case("gcp"))){return Err("Choose an existing .raw or .gcp memory card.".into());}if field.kind=="folder"&&!path.is_dir(){return Err("Choose an existing save folder.".into());}}Ok(value.into())},
        _=>Err("Unknown setting.".into())
    }
}
// Dolphin rewrites its in-memory settings on exit. Refuse edits while it is open.
pub(crate) fn require_closed(exe:&Path)->Result<(),String>{
    #[cfg(windows)] {
        reborn_core::runtime::ensure_runtime_idle(exe).map_err(player)?;
    }
    #[cfg(not(windows))] {let _=exe;return Err("Open Dolphin to change emulator settings on this platform.".into());}
    Ok(())
}
#[tauri::command]
pub async fn dolphin_preferences(runtime:Option<String>)->Result<serde_json::Value,String>{
    tauri::async_runtime::spawn_blocking(move|| {
        let p=Platform::detect();let (exe,user)=config_user(&p,runtime.as_deref())?;
        let mut values=serde_json::Map::new();
        for field in FIELDS {let doc=read_ini(&user.join("Config").join(field.file))?;values.insert(field.id.into(),serde_json::json!(doc.get(field.section,field.key).unwrap_or(field.default)));}
        let live=crate::live_settings::status(&user);
        if let Some(status)=&live {values.insert("volume".into(),status["volume"].to_string().into());values.insert("muted".into(),status["muted"].to_string().into());}
        let readonly_reason=require_closed(&exe).err();
        Ok(serde_json::json!({"values":values,"user_dir":user,"editable":readonly_reason.is_none(),"live_editable":if live.is_some(){vec!["volume","muted"]}else{vec![]},"readonly_reason":readonly_reason}))
    }).await.map_err(|e|e.to_string())?
}
#[tauri::command]
pub async fn dolphin_preference_set(runtime:Option<String>,key:String,value:String)->Result<(),String>{
    tauri::async_runtime::spawn_blocking(move|| {
        let p=Platform::detect();let (exe,user)=config_user(&p,runtime.as_deref())?;
        let field=FIELDS.iter().find(|f|f.id==key).ok_or("Unknown setting.")?;let value=checked_value(field,&value)?;
        let live=crate::live_settings::status(&user).is_some();
        let _guard=if live && ["volume","muted"].contains(&key.as_str()) {None} else {Some(LaunchGuard::acquire()?)};
        if live && ["volume","muted"].contains(&key.as_str()) {
            let wire=if key=="volume"{serde_json::json!(value.parse::<u32>().map_err(|_|"Invalid volume.")?)}else{serde_json::json!(value=="True")};
            crate::live_settings::send(&p,&user,serde_json::json!({"action":key,"value":wire}))?;
        } else {
            require_closed(&exe)?;
            let file=user.join("Config").join(field.file);let mut doc=read_ini(&file)?;let mut overlay=Overlay::new();overlay.entry(field.section.into()).or_default().insert(field.key.into(),value.clone());
            if !doc.merge(&overlay).is_noop(){std::fs::create_dir_all(file.parent().unwrap()).map_err(player)?;std::fs::write(&file,doc.render()).map_err(player)?;}
        }
        let settings_file=reborn_core::settings::settings_path(&p).map_err(player)?;let mut settings=reborn_core::settings::Settings::load(&settings_file).map_err(player)?;settings.emulator_preferences.insert(key,value);settings.save(&settings_file).map_err(player)?;Ok(())
    }).await.map_err(|e|e.to_string())?
}
#[tauri::command]
pub async fn dolphin_saves_folder(runtime:Option<String>)->Result<(),String>{
    let p=Platform::detect();let (_,user)=config_user(&p,runtime.as_deref())?;let dir=user.join("GC");std::fs::create_dir_all(&dir).map_err(player)?;open_in_file_manager(&p,&dir)
}
#[cfg(test)]
mod tests {
    use super::*;
    /// The exact line runtime 1.2.6 wrote into the joiner's status when the
    /// host ran 1.2.5, observed 09-25 on localhost through the session bridge.
    #[test]
    fn a_version_refusal_tells_both_players_to_update() {
        let observed = "Failed to connect: The server and client's NetPlay versions are incompatible.";
        let shown = netplay_error_for_player(observed).expect("the version refusal reached the page as Dolphin wrote it");
        assert!(shown.contains("different Reborn versions") && shown.contains("Check game content"), "{shown}");
        assert_eq!(netplay_error_for_player("Could not connect. Check the host code or address and try again."), None);
    }

    /// The session plan names the seed only when one is installed, and carries the match's dice
    /// only alongside a seed. Emitting `boot_state` with no seed would make the fork refuse to
    /// boot at all, and omitting it when there is one would put both players on the title screen.
    #[test]
    fn the_session_plan_names_a_seed_only_when_one_is_installed() {
        let game = std::path::Path::new("C:/games/NSR.iso");
        let without = netplay_plan(game, "host", None, Some(7));
        assert_eq!(without["mode"], "host");
        assert!(without.get("boot_state").is_none(), "no seed installed must not name one: {without}");
        assert!(without.get("rng_seed").is_none(), "no seed state, nothing to reseed: {without}");

        let seed = std::path::Path::new("C:/data/library/online/seed/pickup.s01");
        let with = netplay_plan(game, "join", Some(seed), Some(0xDEAD_BEEF));
        assert_eq!(with["rng_seed"], 0xDEAD_BEEFu32, "the match's dice must reach the fork: {with}");
        assert!(netplay_plan(game, "join", Some(seed), None).get("rng_seed").is_none(), "a private room keeps the seed's own dice");
        assert_eq!(with["mode"], "join");
        assert_eq!(with["boot_state"], serde_json::json!(seed), "the seed must reach the fork: {with}");
        assert_eq!(with["game"], serde_json::json!(game), "the game must survive adding a seed");
    }
    /// The door every failure on this page goes through keeps a sentence that
    /// was written for the player, and summarizes one that was not.
    ///
    /// Both halves matter. `Session::prepare` refuses with "Close Reborn
    /// Dolphin before applying settings, then try again." when the emulator is
    /// already open, which is the most common way starting an online session
    /// fails; replacing that with "Something unexpected happened" is false
    /// advice. And a settings file another program has open must still never
    /// put its path or `os error 32` on the Online page.
    #[test]
    fn the_online_page_keeps_a_written_sentence_and_summarizes_a_fault() {
        let written = reborn_core::runtime::CLOSE_EMULATOR_FIRST;
        assert_eq!(
            player(reborn_core::Error::player(written)),
            written,
            "the instruction the player has to act on was replaced by a generic one"
        );

        let locked = std::io::Error::from_raw_os_error(32);
        let shown = player(locked);
        assert!(
            !shown.contains("os error") && !shown.contains("32"),
            "an operating system error number reached the Online page: {shown}"
        );
        assert!(
            !shown.contains(char::from(92u8)) && !shown.contains('/'),
            "a file path reached the Online page: {shown}"
        );
    }

    /// Dolphin looks the backend up by name and falls back to its default
    /// silently when the name is not one it knows, so a misspelling this door
    /// let through would look exactly like the setting being ignored, which is
    /// the complaint the control was added for. The four names are
    /// `CONFIG_NAME` in `VideoBackends/*/VideoBackend.h`.
    #[test]
    fn the_graphics_backend_accepts_only_names_dolphin_knows() {
        let field = FIELDS.iter().find(|f| f.id == "video_backend").expect("offered on the page");
        for name in ["D3D", "D3D12", "Vulkan", "OGL"] {
            assert_eq!(checked_value(field, name).as_deref(), Ok(name), "{name} is a real backend");
        }
        // Null and Software Renderer are real names and still refused: they
        // are debugging backends that cannot play the game.
        for refused in ["Null", "Software Renderer", "Direct3D 11", "d3d", "OpenGL", ""] {
            assert!(checked_value(field, refused).is_err(), "{refused} must not reach Dolphin.ini");
        }
    }

    #[test]
    fn online_cards_and_save_writes_are_session_only() {
        let options=netplay_session_options("Player", "join", "direct", "127.0.0.1",2626);
        for required in ["Dolphin.Core.SlotA=255","Dolphin.Core.SlotB=255","Dolphin.NetPlay.WriteSaveData=False","Dolphin.NetPlay.SyncSaves=True","Dolphin.NetPlay.SyncCodes=True","Dolphin.NetPlay.StrictSettingsSync=True","Dolphin.Interface.SkipNKitWarning=True"] { assert!(options.iter().any(|v|v==required)); }
        assert!(options.iter().all(|v|!v.contains("MemcardAPath")&&!v.contains("MemcardBPath")));
    }

    /// A peer that waits for its shaders at boot leaves the other on a black
    /// screen until it is done (25 s cold on a fast PC, 09-25). Dolphin names
    /// the graphics system "Graphics"; a "GFX." key is silently ignored.
    #[test]
    fn online_peers_do_not_wait_for_shaders_at_boot() {
        let options=netplay_session_options("Player", "host", "traversal", "",2626);
        assert!(options.iter().any(|v|v=="Graphics.Settings.WaitForShadersBeforeStarting=False"));
        assert!(options.iter().all(|v|!v.starts_with("GFX.")));
    }

    /// A match boots the seed the server named; a quick game (regular) unless it said pickup, and
    /// nothing else can name a file.
    #[test]
    fn the_seed_is_the_servers_kind_and_regular_by_default() {
        assert_eq!(seed_kind_or_default(None),"regular");
        assert_eq!(seed_kind_or_default(Some("regular")),"regular");
        assert_eq!(seed_kind_or_default(Some(" pickup ")),"pickup");
        assert_eq!(seed_kind_or_default(Some("../evil")),"regular");
        assert_eq!(match_seed_kind(None,Some("ranked")),"ranked");
        assert_eq!(match_seed_kind(Some("regular"),Some(" ranked ")),"ranked");
        assert_eq!(match_seed_kind(Some("pickup"),Some("ranked")),"pickup","a ranked pickup room keeps its pickup game");
        assert_eq!(match_seed_kind(Some("regular"),Some("casual")),"regular");
        assert_eq!(match_seed_kind(Some("ranked"),Some("casual")),"regular","the server can never name ranked itself");
    }

    /// A room keeps Dolphin's own log, session-only: the 09-26 real match left nothing to read.
    #[test]
    fn online_rooms_write_dolphins_own_log() {
        let options=netplay_session_options("Player", "host", "traversal", "",2626);
        for required in ["Logger.Options.WriteToFile=True","Logger.Logs.NETPLAY=True","Logger.Logs.BOOT=True"] { assert!(options.iter().any(|v|v==required),"{required}"); }
    }
    /// Only the locked requirements are kept out of the player's hands.
    ///
    /// This assertion used to also forbid `InternalResolution` and `SyncGPU`,
    /// which is why the launcher could not offer the two settings players most
    /// often want to change. They are the player's now; see
    /// `reborn_core::settings_law`. `MEM1Size`, `RAMOverrideEnable` and
    /// `EFBAccessEnable` stay out, because the game does not run correctly
    /// without them and the portrait damage is written into the player's save.
    #[test]fn settings_are_validated_and_protected(){assert!(FIELDS.iter().all(|f|!["MEM1Size","RAMOverrideEnable","EFBAccessEnable","HiresTextures","CacheHiresTextures"].contains(&f.key)));let m=FIELDS.iter().find(|f|f.id=="msaa").unwrap();assert_eq!(checked_value(m,"4").unwrap(),"0x00000004");assert_eq!(checked_value(m,"0x00000002").unwrap(),"0x00000002");assert!(checked_value(m,"3").is_err());assert!(checked_value(m,"16").is_err());for id in ["volume","audio_latency"]{let f=FIELDS.iter().find(|f|f.id==id).unwrap();assert!(checked_value(f,"-1").is_err());assert!(checked_value(f,"999").is_err());assert!(checked_value(f,"20").is_ok());}let f=FIELDS.iter().find(|f|f.id=="muted").unwrap();assert_eq!(checked_value(f,"true").unwrap(),"True");assert!(checked_value(f,"true\n[Core]").is_err());}
    #[test]fn editing_one_setting_preserves_saves_and_unknown_keys(){let mut doc=IniDocument::parse("[Core]\nMemcardAPath = C:/Saves/My.raw\nSyncGPU = True\n[Custom]\nKeep = original\n[DSP]\nVolume = 100\n");let mut overlay=Overlay::new();overlay.entry("DSP".into()).or_default().insert("Volume".into(),"65".into());doc.merge(&overlay);assert_eq!(doc.get("Core","MemcardAPath"),Some("C:/Saves/My.raw"));assert_eq!(doc.get("Core","SyncGPU"),Some("True"));assert_eq!(doc.get("Custom","Keep"),Some("original"));assert_eq!(doc.get("DSP","Volume"),Some("65"));}
}

/// The image netplay boots, built or refreshed if need be.
///
/// `Ok(None)` means there is no imported game to build one from, which is the
/// old shape of an install that was never imported: those keep booting what
/// they always booted, and they have no custom soundtrack to diverge with
/// either, because a soundtrack apply needs a pristine image too.
pub(crate) fn netplay_image(p: &Platform) -> Result<Option<PathBuf>, String> {
    use reborn_core::online_image::{ensure, Outcome, Refusal};
    let data_root = p.paths.data_root().map_err(player)?;
    let settings = crate::content_updates::load_settings(p)?;
    let dirs = reborn_core::roster::builder_dirs(p, &crate::content_updates::image_hook(p, &settings).extra_dirs);
    match ensure(p, &data_root, None, &dirs).map_err(player)? {
        Outcome::Current { path } | Outcome::Built { path, .. } => Ok(Some(path)),
        Outcome::Refused(Refusal::NoPristine) => Ok(None),
        // Player-facing words only: the folders that were searched and the
        // image's path belong in the log, not on the Online page.
        Outcome::Refused(r @ Refusal::NoBuilder { .. }) => {
            eprintln!("netplay copy not built: {}", r.message());
            Err("The Reborn Builder that applies your roster is missing from this install, so the online copy of your game could not be prepared. Reinstall the launcher to put it back.".to_string())
        }
        Outcome::Refused(r @ Refusal::InUse { .. }) => {
            eprintln!("netplay copy not rebuilt: {}", r.message());
            Err("The online copy of your game is open in another program, so it could not be refreshed. Close the game and try again.".to_string())
        }
        Outcome::Refused(r) => Err(r.message()),
    }
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
/// The seed save state an online match boots into, when one is installed.
///
/// Netplay otherwise starts at the title screen and both players walk the menus by hand.
/// The fork loads this in `Core::CpuThread` before the CPU runs its first instruction, so
/// both peers begin lockstep from identical RAM (`State::LoadAtBoot`). It is therefore only
/// safe while every peer loads the SAME bytes: the seed ships as a hash-checked component
/// pinned to the online image, and a disc update must retire the seed with it.
///
/// Absent is not an error. A launcher whose content predates the seed simply boots the disc,
/// exactly as it does today, rather than refusing to open a room.
/// The session plan the fork reads once at boot.
///
/// `boot_state` is present only when a seed is installed. The fork treats a named-but-missing
/// file as a hard error rather than falling back to the disc, because a peer that could not
/// load it would start from a different machine state and desync on the first frame.
fn netplay_plan(game:&std::path::Path,mode:&str,seed:Option<&std::path::Path>,rng_seed:Option<u32>)->serde_json::Value{
 let mut plan=serde_json::json!({"game":game,"mode":mode});
 if let Some(seed)=seed {
  plan["boot_state"]=serde_json::json!(seed);
  // Only with a seed state: the fork writes it right after that load (State::SetBootRngSeed).
  if let Some(r)=rng_seed { plan["rng_seed"]=serde_json::json!(r); }
 }
 plan
}

/// The seed state a match boots from: `regular` (a quick game) unless the server named `pickup`.
/// The name is one of the two the online-content component ships, never a path from outside.
pub(crate) fn seed_kind_or_default(kind:Option<&str>)->&'static str{
 match kind.map(str::trim) { Some("pickup")=>"pickup", _=>"regular" }
}

/// The seed a matchmade game boots from: a ranked match's regular game takes `ranked` (Adam 10-02: Ranked plays on the
/// settings of his Reborn 2.1 S01 - Difficulty Hard, GameBreakers Off; ranked.s01 is regular.s01 with those two
/// values), anything else is the server's kind. Never reaches the server: rooms and snapshots keep pickup|regular.
pub(crate) fn match_seed_kind(kind:Option<&str>,mode:Option<&str>)->&'static str{
 match (seed_kind_or_default(kind),mode.map(str::trim)) { ("regular",Some("ranked"))=>"ranked", (k,_)=>k }
}

/// A content install that predates ranked.s01 boots ranked from regular.s01; the server's seed comparison still holds
/// a match whose two peers would boot different files.
fn netplay_boot_state(p:&Platform,seed_kind:Option<&str>)->Option<std::path::PathBuf>{
 let dir=p.paths.data_root().ok()?.join("library/online/seed");
 let name=match seed_kind.map(str::trim) { Some("ranked")=>"ranked", other=>seed_kind_or_default(other) };
 let seed=dir.join(format!("{name}.s01"));
 if seed.is_file() { return Some(seed); }
 let fallback=dir.join("regular.s01");
 (name=="ranked" && fallback.is_file()).then_some(fallback)
}

/// What matchmaking's `ready` sends: this PC's answers, measured, never assumed. `seed_sha256` is
/// the one the server compares - two seeds that differ, or one that is absent, hold the match
/// before either Dolphin starts, because they would desync on frame one. `traversal_ok` is one
/// hello to the traversal server, the same first packet Dolphin sends (about 1.5 s at worst).
/// Cheap on purpose: it does not rebuild the online image (netplay_open does that at launch).
pub(crate) fn mm_preflight(runtime:Option<&str>,game:Option<&str>,seed_kind:Option<&str>)->serde_json::Value{
 let p=Platform::detect();let legacy=default_legacy_root(&p);
 let found=find_runtime_and_game(&p,runtime,game,legacy.as_deref());
 let runtime_ok=found.as_ref().map(|(f,_)|supports_launcher_netplay(f.path()).unwrap_or(false)).unwrap_or(false);
 let seed=netplay_boot_state(&p,seed_kind).and_then(|s|reborn_core::game_patch::sha256_of(&s).ok()).map(|h|h.to_string());
 // What this PC would boot, named precisely enough for the server to refuse a mismatch BEFORE
 // two Dolphins start: the installed component versions, and the online copy's provenance
 // (the pristine it was built from and the roster applied). 09-26: the two online copies had
 // different disc hashes (the fork's comparison 1), the game froze on one side, and nothing
 // had said so. Absent on an older launcher; the server only compares what both sides sent.
 let content=reborn_core::content::ContentService::for_platform(&p).ok();
 let version=|component:&str| content.as_ref().and_then(|c|c.installed(component).ok().flatten()).map(|ptr|ptr.current.version);
 let data=p.paths.data_root().ok();
 // What the online copy WILL be built from when the room opens (netplay_open rebuilds it to
 // this), not what an older copy's record says: 09-26 a repaired image was refused on the
 // stale record of a copy that the room would have rebuilt seconds later.
 let source=data.as_ref().and_then(|d|reborn_core::online_image::wanted(d).ok().flatten().or_else(||reborn_core::online_image::built_from(d)));
 // Is the game image the head of the installed patch chain? 09-26: a game-patch component
 // said 2026.09.26.2 while the image it should have patched was still NSRV3 (node 2 of 14),
 // so "same version" on both sides was true and the discs still differed. `None` = no chain
 // or no image to ask about; the server treats only `false` as a refusal.
 let pristine=data.as_ref().and_then(|d|reborn_core::game_image::find_pristine(d)).map(|x|x.folder_sha256);
 let chain_head=data.as_ref().and_then(|d|reborn_core::game_patch::Chain::load(&reborn_core::game_patch::component_root(d).join(reborn_core::game_patch::CHAIN_FILE)).ok().flatten()).and_then(|c|c.head().map(str::to_string));
 let pristine_at_tip=match (&pristine,&chain_head){(Some(a),Some(b))=>Some(a==b),_=>None};
 serde_json::json!({"runtime_ok":runtime_ok,"image_ok":found.is_ok(),"seed_ok":seed.is_some(),"seed_sha256":seed,"backend_ok":true,
  "roster_version":version("roster"),"game_patch_version":version("game-patch"),"runtime_version":version("runtime"),
  "pristine_sha256":source.as_ref().map(|s|s.pristine_sha256.clone()),"roster_sha256":source.as_ref().and_then(|s|s.roster_sha256.clone()),
  "pristine_at_tip":pristine_at_tip,"chain_head":chain_head,
  "traversal_ok":reborn_core::traversal_probe::probe(reborn_core::traversal_probe::DEFAULT_SERVER,std::time::Duration::from_millis(1500))})
}

/// online.
#[tauri::command]
pub async fn netplay_open(runtime:Option<String>,game:Option<String>,preset:String,nickname:String,mode:String,connection:String,address:String,port:u16,rng_seed:Option<u32>,seed_kind:Option<String>)->Result<serde_json::Value,String>{
 tauri::async_runtime::spawn_blocking(move||{
  let guard=LaunchGuard::acquire_session()?;
  let nickname=nickname.trim();let address=address.trim();
  if nickname.is_empty()||nickname.chars().count()>32||nickname.chars().any(char::is_control){return Err("Enter a player name of 1 to 32 characters.".into());}
  if !["host","join"].contains(&mode.as_str())||!["direct","traversal"].contains(&connection.as_str())||port==0{return Err("Choose valid connection settings.".into());}
  if mode=="join"&&(address.is_empty()||address.len()>253||!address.chars().all(|c|c.is_ascii_alphanumeric()||".:-_".contains(c))){return Err("Enter a valid host code or address.".into());}
  let p=Platform::detect();let legacy=default_legacy_root(&p);let (found,game_path)=find_runtime_and_game(&p,runtime.as_deref(),game.as_deref(),legacy.as_deref())?;
  require_closed(found.path())?;
  // Boots through the same gate every other path does: `may_play_before_boot`
  // asks what this image is before anything below applies the texture pack
  // or starts the emulator, so an unverified disc gets neither one here
  // either.
  let chosen_disc=may_play_before_boot(&p,&game_path)?;
  // Netplay is lockstep, so both sides must run the same bytes. The offline
  // image is the one a custom soundtrack rewrites, so it is never the one
  // netplay boots: reborn_core::online_image keeps a copy that is the
  // pristine game plus the roster and nothing else, and rebuilds it here
  // when a game patch or a roster push has moved it.
  //
  // Resolved BEFORE the texture sync, not after, because these are the bytes
  // the sync has to be judging. The gate above read the disc the player
  // chose; this is the file the emulator is actually handed.
  let online=match netplay_image(&p){Ok(Some(path))=>Some(path),Ok(None)=>None,Err(e)=>return Err(e)};
  // The file the emulator is handed, not the one the player picked. A netplay
  // copy that is not Reborn's own is a ruling on the bytes about to boot, so
  // it turns a verified disc into NotReborn. A chosen disc nobody could read
  // stays Unknown: the copy says nothing about a question that was never
  // answered, and the pack is left alone rather than taken.
  // The pack follows the disc the player chose, exactly as Play rules it. A room used to
  // downgrade a verified disc to NotReborn when the online copy's provenance record was
  // missing or unrecognized, and Withhold takes the pack OFF, so a player who has textures
  // in every offline game lost them the moment they joined a friend (09-26, second PC).
  // Adam: "make it so the textures work all the time". The online copy is still rebuilt
  // from the pristine and the roster above; only the texture ruling no longer reads its record.
  let standing=chosen_disc;
  let game_path=online.unwrap_or(game_path);
  // The same texture sync Play runs, and it runs UNCONDITIONALLY.
  //
  // It used to be wrapped in `if verified`, which is the exact boolean
  // semantics this law exists to abolish: skipping the sync composes nothing
  // and leaves whatever is already in Load/Textures/GN7E69 exactly where it
  // is. For anybody who has ever booted successfully that is Reborn's entire
  // pack, HiresTextures is a Locked True key, and netplay never calls
  // Session::run, so the removal that protects every other path never ran
  // here at all. A player could point Settings at their own disc, host a
  // room, and watch Reborn's faces on somebody else's roster: the reported
  // bug, through the product's own interface.
  //
  // Passing the verdict instead of gating the call is the whole fix.
  // NotReborn now means Withhold, which takes the pack off, rather than
  // "skip". A sync problem is logged, not fatal: the room still opens.
  if let Err(e)=crate::sync_active_judged(&p,runtime.as_deref(),Some(standing)) { eprintln!("netplay: texture sync before launch did not finish: {e}"); }
  // Fail before preparing settings when an older/custom runtime lacks the bridge.
  if !supports_launcher_netplay(found.path())? { return Err("This Dolphin build does not support launcher Netplay. Choose the Reborn Dolphin runtime included with the latest launcher in Files and storage.".into()); }
  let req=LaunchRequest{play_with_unset_controller:true,runtime_exe:found.path().to_path_buf(),game_path:game_path.clone(),graphics:preset_from(&preset),fullscreen:true,build:GameBuild::Offline,mods_disabled:false,user_dir:selected_user_dir(&p,None)?,league_match:false,textures:standing};
  // The lock, before anything is written or started, and anything left
  // unfinished recovered. The same first two steps as a launch.
  // The waiting acquire, the same one a launch takes. A lock this launcher's
  // own background work holds for a moment is not a second launcher, and
  // refusing an online session over it tells the player to close something
  // that is not open.
  let Startup::Owner(supervisor)=Supervisor::acquire_waiting(&p).map_err(player)? else { return Err(already_running()); };
  let _=supervisor.recover_unfinished().map_err(player)?;
  let prepared=Session::prepare(&p,&req).map_err(player)?;
  let user=prepared.user_dir.path();
  let directory=p.paths.data_root().map_err(player)?.join("cache/netplay");
  std::fs::create_dir_all(&directory).map_err(player)?;
  // Fixed, small bridge files are reused. Never duplicate a game, profile or card.
  let path=directory.join("session.json");
  for suffix in ["status","command"] { let _=std::fs::remove_file(path.with_extension(format!("json.{suffix}"))); }
  // The plan the fork reads once at boot. `boot_state` is added only when a seed is
  // installed; the fork treats a named-but-missing file as a hard error, because a peer that
  // could not load it would start from a different machine state and desync on frame one.
  let plan=netplay_plan(&game_path,&mode,netplay_boot_state(&p,seed_kind.as_deref()).as_deref(),rng_seed);
  let session_bytes=serde_json::to_vec(&plan).map_err(|_|"Could not prepare the NetPlay room. Try again.".to_string())?;
  std::fs::write(&path,session_bytes).map_err(player)?;
  std::fs::write(path.with_extension("json.alive"),b"alive").map_err(player)?;
  let mut args=vec![std::ffi::OsString::from("-u"),user.as_os_str().to_os_string(),"--reborn-netplay-session".into(),path.as_os_str().to_os_string()];
  for value in netplay_session_options(nickname,&mode,&connection,address,port) { args.push("-C".into());args.push(value.into()); }
  let user_dir=user.to_path_buf();
  // Written before the emulator, so the id every capture from this session is
  // filed under exists before there is anything to file.
  let record=supervisor.begin_with_intent(req.build,Some(SessionIntent{league_match:req.league_match})).map_err(player)?;
  let handle=p.processes.spawn(found.path(),&args,found.path().parent(),&Default::default()).map_err(player)?;
  *NETPLAY.lock().map_err(netplay_lock_poisoned)?=Some(NetplaySession{path:path.clone(),user_dir:Some(user_dir.clone()),active:true,sequence:0,connection:connection.clone(),port,last_status:serde_json::json!({"phase":"connecting"}),started:std::time::Instant::now(),matchmade:rng_seed.is_some()});
  std::thread::spawn(move||{
    // The lock and the guard are held across the wait AND across the
    // collection: the queue and the session log are shared state, and
    // nothing may start a second session in the middle of filing this one's
    // results.
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
    let exit=p.processes.wait(handle).ok();
    alive.store(false,std::sync::atomic::Ordering::SeqCst);
    if let Ok(mut state)=NETPLAY.lock(){if let Some(session)=state.as_mut(){session.active=false;}}
    let collected=reborn_core::session::finish_and_collect(&p,&supervisor,record,exit.as_ref(),&user_dir);
    // Both given back before the network work. A send is one request per queued
    // game with the client's full timeout each, so against a slow or unreachable
    // backend it is minutes, and for those minutes pressing Play said a game
    // session was already active and this page said to close the emulator. The
    // emulator had already exited: both were false.
    drop(supervisor);
    drop(_guard);
    // Reported through the session record, never to a screen. The window has
    // already been told the game started, and a game that was played is not a
    // failure however collecting or sending it went.
    let _=collected.send(&p);
  });
  Ok(serde_json::json!({"started":true,"mode":mode,"game":game_path.file_name().unwrap_or_default().to_string_lossy()}))
 }).await.map_err(|e|e.to_string())?
}


/// `matchmade`: opened by matchmaking (the only caller that hands Dolphin an RNG seed), so the page keeps the
/// Direct connection dialog shut for it (10-02: the dialog popped up mid-match).
struct NetplaySession { path:PathBuf, user_dir:Option<PathBuf>, active:bool, sequence:u64, connection:String, port:u16, last_status:serde_json::Value, started:std::time::Instant, matchmade:bool }
static NETPLAY:std::sync::Mutex<Option<NetplaySession>>=std::sync::Mutex::new(None);

/// The Dolphin user folder of the NetPlay game running right now, so the live feed can read its play lines.
pub fn netplay_user_dir() -> Option<std::path::PathBuf> {
    let s = NETPLAY.lock().ok()?;
    let s = s.as_ref()?;
    if s.active { s.user_dir.clone() } else { None }
}

// A poisoned lock means a prior panic while the room's own state was being
// updated. Nothing here can trust that state afterward, and this is the one
// sentence that says so without implying a fix that may not exist.
fn netplay_lock_poisoned<T>(_: std::sync::PoisonError<T>) -> String {
    "The NetPlay room state is corrupted. Restart the launcher.".into()
}

// Command-line layers are never saved by Dolphin. Personal cards remain intact,
// while hosts share the same card-free configuration with every participant.
fn netplay_session_options(nickname:&str,_mode:&str,connection:&str,address:&str,port:u16)->Vec<String>{
 let mut values=vec![
  "Dolphin.Core.SlotA=255".into(),"Dolphin.Core.SlotB=255".into(),
  "Dolphin.NetPlay.SyncSaves=True".into(),"Dolphin.NetPlay.WriteSaveData=False".into(),
  "Dolphin.NetPlay.SyncCodes=True".into(),"Dolphin.NetPlay.StrictSettingsSync=True".into(),
  "Dolphin.NetPlay.UseIndex=False".into(),"Dolphin.Display.RenderToMain=False".into(),
  "Dolphin.Analytics.PermissionAsked=True".into(),"Dolphin.Interface.SkipNKitWarning=True".into(),
  // The launcher owns presence; Dolphin's own would race it on the IPC socket.
  "Dolphin.General.UseDiscordPresence=False".into(),
  // Peers boot together, so one still compiling shaders holds the other on a
  // black screen. Mode 2 ubershaders cover whatever is not compiled yet.
  "Graphics.Settings.WaitForShadersBeforeStarting=False".into(),
  // A room writes Dolphin's own log (User/Logs/dolphin.log). The 09-25/26 real matches left no
  // trace of why a start never happened: the profile's Logger.ini turns file logging off, and
  // that stays so for Play. Warning level (3): every PanicAlert is an ERROR line under its
  // caller's type, and notices ("Booting from disc", "Stop") still print. Not Info: at Info
  // NETPLAY writes one line per packet, ~270 a second in play (measured 09-26), each flushed.
  "Logger.Options.WriteToFile=True".into(),"Logger.Options.Verbosity=3".into(),
  "Logger.Logs.NETPLAY=True".into(),"Logger.Logs.BOOT=True".into(),"Logger.Logs.CORE=True".into(),"Logger.Logs.COMMON=True".into(),"Logger.Logs.MASTER=True".into(),
  format!("Dolphin.NetPlay.Nickname={nickname}"),format!("Dolphin.NetPlay.TraversalChoice={connection}"),
  format!("Dolphin.NetPlay.ConnectPort={port}"),format!("Dolphin.NetPlay.HostPort={port}"),
 ];
 values.push(format!("Dolphin.NetPlay.{}={address}",if connection=="traversal"{"HostCode"}else{"Address"}));values
}

/// Dolphin's refusal when host and joiner run different Reborn runtimes, in
/// words that say what to do. Players read "NetPlay versions are
/// incompatible" as a broken install, when one of them has simply not taken
/// the latest update yet (runtime 1.2.5 against 1.2.6, 09-24).
fn netplay_error_for_player(error:&str)->Option<&'static str>{
 error.contains("NetPlay versions are incompatible").then_some(
  "You and the host are on different Reborn versions. Both players: open Settings, press Check game content, install the update, then try again.")
}

#[tauri::command]
pub fn netplay_status()->Result<serde_json::Value,String>{
 let mut state=NETPLAY.lock().map_err(netplay_lock_poisoned)?;
 let Some(session)=state.as_mut() else{return Ok(serde_json::json!({"phase":"idle","active":false}));};
 if let Some(status)=std::fs::read(session.path.with_extension("json.status")).ok().and_then(|s|serde_json::from_slice::<serde_json::Value>(&s).ok()).filter(|s|s.is_object()) {session.last_status=status;}
 let mut status=session.last_status.clone();
 if !session.active && status["phase"]!="error" {status["phase"]="closed".into();}
 if session.active && status["phase"]=="connecting" && session.started.elapsed().as_secs()>45 {status["phase"]="error".into();status["error"]="Dolphin did not open the room. Check that your Reborn Dolphin runtime supports launcher Netplay.".into();}
 if let Some(e)=status["error"].as_str().and_then(netplay_error_for_player) {status["error"]=e.into();}
 status["active"]=session.active.into();status["connection"]=session.connection.clone().into();status["port"]=session.port.into();status["matchmade"]=session.matchmade.into();Ok(status)
}

#[tauri::command]
pub fn netplay_action(action:String,ports:Option<Vec<u8>>)->Result<(),String>{
 if !["start","leave","ports"].contains(&action.as_str()){return Err("Unknown room action.".into());}
 if action=="ports" && !ports.as_ref().is_some_and(|p|p.len()==4){return Err("Assign exactly four player slots.".into());}
 let mut state=NETPLAY.lock().map_err(netplay_lock_poisoned)?;
 let session=state.as_mut().filter(|s|s.active).ok_or("The room is closed.")?;
 session.sequence+=1;
 let payload=serde_json::json!({"id":session.sequence,"action":action,"ports":ports});
 let dest=session.path.with_extension("json.command");let temp=session.path.with_extension("json.command.tmp");
 let payload_bytes=serde_json::to_vec(&payload).map_err(|_|"Could not send the room command. Try again.".to_string())?;
 std::fs::write(&temp,payload_bytes).map_err(player)?;
 Platform::detect().files.atomic_replace(&temp,&dest).map_err(player)
}

// Windows GUI binaries do not reliably expose --help on stdout. Read the exact
// compiled CLI marker instead, with bounded memory and no emulator subprocess.
fn supports_launcher_netplay(path:&Path)->Result<bool,String>{
 use std::io::Read;
 let mut file=std::fs::File::open(path).map_err(player)?;
 let marker=b"--reborn-netplay-session";let mut chunk=[0u8;65536];let mut pending=Vec::new();
 loop{let count=file.read(&mut chunk).map_err(player)?;if count==0{return Ok(false);}
  pending.extend_from_slice(&chunk[..count]);if pending.windows(marker.len()).any(|w|w==marker){return Ok(true);}
  let keep=pending.len().saturating_sub(marker.len()-1);pending.drain(..keep);
 }
}
