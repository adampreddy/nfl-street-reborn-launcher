//! Who owns a Dolphin setting: the launcher, or the player.
//!
//! # The bug this exists to fix
//!
//! The launcher had one verb. Every launch it merged its opinion over the
//! player's configuration files, and the only thing that survived was a
//! hand-written allowlist of about twenty keys covering audio, VSync and the
//! memory card slots. Nothing about graphics or controls was on it, so a
//! player who changed their internal resolution, their anti-aliasing, their
//! graphics backend or their button mapping found it back the way the
//! launcher liked it the next time they pressed Play.
//!
//! Worse than reverting it: seventeen of those keys were also written into
//! `GameSettings/<game id>.ini`, which is Dolphin's highest-precedence layer.
//! A change made in Dolphin's own Graphics window did not merely get undone
//! later, it did nothing at all in the session the player made it. The
//! dropdown moved and the game ignored it.
//!
//! # The law
//!
//! Every key the launcher writes is declared here, in exactly one of three
//! classes, and every write path consults this table rather than carrying its
//! own list.
//!
//! - [`Owner::Locked`] - the launcher forces it, every launch. Nine keys, each
//!   naming a concrete failure it prevents. Nothing joins this set without a
//!   `why` that says what breaks, and a test pins the set so it cannot quietly
//!   regrow.
//! - [`Owner::Player`] - the launcher never writes it. Not "writes it then
//!   restores it", not "writes it unless it detects a change": never. That is
//!   what makes the guarantee structural instead of a race against Dolphin,
//!   which rewrites `GFX.ini` from memory when it exits.
//! - [`Owner::Seeded`] - written only into a key that is absent, through
//!   [`crate::ini::IniDocument::merge_absent`]. A fresh install still gets the
//!   tuned default; an install where anybody has expressed a preference keeps
//!   it.
//!
//! # Why a value comparison would not work
//!
//! The obvious alternative is to remember what we wrote and back off when the
//! live value differs. It does not survive contact with Dolphin. The shipped
//! `User` template records that "Dolphin rewrites GFX.ini from memory when it
//! exits and silently reverted a hand edit to it", and Dolphin normalises as it
//! writes: `0x00000004` can come back as `4`, booleans change case. Any such
//! scheme either reads Dolphin's own rewrite as a player edit or reads a player
//! edit as drift. Not writing the key at all needs no comparison.
//!
//! # Undoing the damage already done
//!
//! [`IniDocument::merge`](crate::ini::IniDocument::merge) can add and update but
//! never remove, so every key any shipped build ever forced is still sitting in
//! every player's files, and the ones in `GameSettings` still outrank the
//! preference the launcher now offers. Retiring a key in the source therefore
//! changes nothing on its own. [`reconcile_once`] is the other half: a one-shot
//! pass that prunes the retired keys out of `GameSettings` and puts the
//! player's own value back in the config files where there is evidence of what
//! it was.
//!
//! That restore is gated on [`Key::forced_values`], the set of every literal a
//! shipped build ever wrote for that key, built from `git log -S` over
//! `runtime.rs` and not from memory. The values moved between builds:
//! `SyncGPU` was forced `True` through 3.0.1 and `False` from 3.0.2,
//! `DeferEFBCopies` went `False` then `True`, `MSAA` was `1` and then dropped,
//! and `InternalResolution` came from a per-preset table that returned 1, 3, 4
//! or 6 before it was flattened to a constant 3. Gating on today's value alone
//! would silently skip every install stamped by an older build, which is most
//! of them.

use crate::ini::Overlay;
use crate::platform::{DataDir, Platform, Result};
use std::path::Path;

/// The pseudo-file name for the per-game override layer,
/// `<user dir>/GameSettings/<game id>.ini`. It is not one path because the
/// launcher writes it for every game id the build can boot.
pub const GAME_SETTINGS: &str = "GameSettings";

/// Bumped when [`reconcile_once`] needs to run again over installs it has
/// already visited. The marker file holds the revision that last ran.
///
/// 4 (2026-09-19): the shipped per-game template enabled four cheat codes that
/// are already patched into the disc, so every install applied them a second
/// time on top of the baked ones. See [`BAKED_CODES`]. Prior revisions only
/// ever pruned `key = value` pins and never looked at the code lists, so the
/// installs that need this are the ones every earlier revision already visited.
///
/// 3 (2026-09-14): revision 2 wrote `ShaderCompilationMode = 0` into every
/// install. Blocking shader compilation on a cache the same release had just
/// invalidated is what players reported as a multi-second stall at every snap,
/// so the repair now lands on 2 and has to run again over the same installs.
///
/// 2 (2026-09-13): revision 1 pruned the per-game pins without reading the
/// layer underneath, so `GFX.ini`'s `MSAA = 0x00000004` and
/// `ArbitraryMipmapDetection = True` - values the pins had kept unreachable
/// since the product shipped - became live on every install that reconciled.
/// The installs that need the repair are exactly the ones revision 1 already
/// visited, so fixing the table is only half of it; the marker has to move too.
pub const REVISION: u32 = 4;

/// Cheat codes the shipped template switched on that the disc already carries.
///
/// Verified against `main.dol` in the 2026-09-19 image, not assumed: each of
/// the three Gecko hooks holds a branch into the cave region
/// (`0x800D411C -> 0x802C7D30`, `0x800D1BF4 -> 0x802C7D60`,
/// `0x800D1BC0 -> 0x802C7D90`), and all eleven names the Legends ActionReplay
/// code writes are already sitting at `0x8028E9C8`. Enabling them a second time
/// re-applies the same GameBreaker and scoring changes on top of the baked
/// ones.
///
/// Nobody saw it on the dev box: that profile has no `[Gecko_Enabled]` section
/// at all, so none of these were ever active there.
///
/// Matched by exact name, never by clearing the section, because a player may
/// have enabled a code of their own and that is theirs to keep. `OnFrame` is
/// deliberately absent: it carries the GameCube controls patch, which is live
/// and correct.
pub const BAKED_CODES: &[(&str, &str)] = &[
    ("Gecko_Enabled", "Remove GameBreaker 2"),
    ("Gecko_Enabled", "GB Takes Longer (v2)"),
    ("Gecko_Enabled", "Point Remover"),
    ("ActionReplay_Enabled", "Legends Text Code"),
];

/// Who decides a key's value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Owner {
    /// The launcher forces it on every launch. The game does not run correctly
    /// without it, and the failure is worse than a setting not sticking.
    Locked,
    /// The player decides. The launcher never writes it.
    Player,
    /// The launcher's tuned default, written only where the key is absent.
    Seeded,
}

/// How a player-owned key is offered in the launcher's own Settings page.
/// `None` means the key is still the player's, just not surfaced here; Dolphin
/// is where they change it.
#[derive(Debug, Clone, Copy)]
pub struct Ui {
    /// The id the UI and `settings.emulator_preferences` use.
    pub id: &'static str,
    /// The control kind the page renders.
    pub kind: &'static str,
    /// What the page shows when the key is not set anywhere.
    pub default: &'static str,
}

/// One declared key.
#[derive(Debug, Clone, Copy)]
pub struct Key {
    /// `Dolphin.ini`, `GFX.ini`, `Logger.ini`, or [`GAME_SETTINGS`].
    pub file: &'static str,
    pub section: &'static str,
    pub key: &'static str,
    pub owner: Owner,
    /// The value to write. Required for `Locked` and `Seeded`, always `None`
    /// for `Player`.
    pub value: Option<&'static str>,
    /// Where this key appears in the launcher's Settings page, if it does.
    pub ui: Option<Ui>,
    /// A shipped build once forced this key and no longer does, so it is still
    /// sitting in players' files and [`reconcile_once`] has to take it out.
    pub retired: bool,
    /// Every literal a shipped build ever wrote here. A live value in this set
    /// is the launcher's, not the player's, and may be replaced. A live value
    /// outside it was chosen by somebody and is never touched.
    pub forced_values: &'static [&'static str],
    /// Write this value once, where the restore had no evidence it could
    /// trust. For a key an old build welded in at a real cost, dropping the
    /// force is not enough: the cost stays until something puts the right
    /// value back. Declaring a repair also says the `.launcher-original`
    /// snapshot is not to be believed when it merely repeats a value a shipped
    /// build wrote, which for a key that sat under a per-game pin is the only
    /// thing that snapshot can say.
    pub repair_once: Option<&'static str>,
    /// What breaks without this. Mandatory for `Locked`.
    pub why: &'static str,
}

const fn locked(file: &'static str, section: &'static str, key: &'static str, value: &'static str, forced_values: &'static [&'static str], why: &'static str) -> Key {
    Key { file, section, key, owner: Owner::Locked, value: Some(value), ui: None, retired: false, forced_values, repair_once: None, why }
}

const fn seeded(file: &'static str, section: &'static str, key: &'static str, value: &'static str, forced_values: &'static [&'static str], why: &'static str) -> Key {
    Key { file, section, key, owner: Owner::Seeded, value: Some(value), ui: None, retired: false, forced_values, repair_once: None, why }
}

const fn player(file: &'static str, section: &'static str, key: &'static str, forced_values: &'static [&'static str], why: &'static str) -> Key {
    Key { file, section, key, owner: Owner::Player, value: None, ui: None, retired: false, forced_values, repair_once: None, why }
}

/// A retired per-game pin: the launcher wrote it into the layer nothing can
/// beat, and now has to take it back out.
const fn retired_pin(section: &'static str, key: &'static str, forced_values: &'static [&'static str], why: &'static str) -> Key {
    Key { file: GAME_SETTINGS, section, key, owner: Owner::Player, value: None, ui: None, retired: true, forced_values, repair_once: None, why }
}

const fn with_ui(mut key: Key, ui: Ui) -> Key {
    key.ui = Some(ui);
    key
}

const fn with_repair(mut key: Key, value: &'static str) -> Key {
    key.repair_once = Some(value);
    key
}

/// Every key the launcher has an opinion about.
pub const KEYS: &[Key] = &[
    // ---------------------------------------------------------------- LOCKED
    //
    // Nine rows, seven requirements. Each names a failure the player cannot
    // recover from, or a standing product rule. The per-game layer is where
    // the emulation requirements go, because it is the only layer nothing else
    // can override; the two Dolphin.ini-only sections have no per-game
    // equivalent.
    locked(GAME_SETTINGS, "Core", "RAMOverrideEnable", "True", &["True"],
        "Reborn needs 64 MiB of emulated MEM1. The imported fields do not fit in the stock 24 MiB and the game hangs loading them."),
    locked(GAME_SETTINGS, "Core", "MEM1Size", "67108864", &["67108864"],
        "The 64 MiB itself. Paired with RAMOverrideEnable; either one alone does nothing."),
    locked(GAME_SETTINGS, "Video_Hacks", "EFBAccessEnable", "True", &["True"],
        "With EFB access off the NFL Challenge created-player portraits render as solid black and the black is written into the player's memory card save. It cannot be repaired from the launcher side, so this is the one graphics key the player does not get to break."),
    locked(GAME_SETTINGS, "Video_Settings", "HiresTextures", "True", &["True"],
        "Every texture mod the launcher advertises, and the HD pack it installs, are silently inert without it. A player turning this off sees the mods they enabled do nothing, with no error to explain it."),
    locked(GAME_SETTINGS, "Video_Settings", "CacheHiresTextures", "False", &["True", "False"],
        "Prefetching the HD pack into RAM measured 12 GB private bytes, 384 MB free and 28 GB of pagefile on the 24 GB dev box on 2026-09-11, reported as the game running slow. The pack is BC7 DDS now and loads on demand in under a millisecond."),
    locked("Dolphin.ini", "Core", "RAMOverrideEnable", "True", &["True"],
        "The same 64 MiB requirement, at the global layer, because a fresh install boots before any per-game file exists."),
    locked("Dolphin.ini", "Core", "MEM1Size", "67108864", &["67108864"],
        "The same 64 MiB requirement, at the global layer."),
    locked("Dolphin.ini", "Interface", "UsePanicHandlers", "False",  &["False"],
        "A Dolphin panic handler is an error popup, and the standing product rule is no error popups, ever."),
    locked("Dolphin.ini", "Interface", "SkipNKitWarning", "True", &["True"],
        "Every image Reborn ships is an NKit disc, and Dolphin asks before booting one: a modal that stops the boot until somebody ticks a box and clicks OK. In an ordinary Play the player at least sees that window; in a Netplay session Dolphin's own main window is never shown, so the dialog sits behind the launcher unnoticed and that player never boots. The other player boots, finishes compiling shaders and then waits forever on a black screen for input that cannot arrive, which is what \"Netplay gets stuck on compiling shaders\" was. The warning is about a property of a disc the launcher itself built, so it is not a choice to put to the player."),
    locked("Dolphin.ini", "Input", "BackgroundInput", "True", &["True"],
        "Dolphin throws away every controller press while its render window is not the focused one (Core::UpdateInputGate). Reported 2026-09-19 as 'after i open the app i cant get pass the press start screen': click anything else during the shader compile and the game opens unfocused, and a pad of any kind does nothing until the game is clicked with a mouse, which a player holding a controller does not think to do. Keyboard hotkeys have their own gate (MAIN_FOCUSED_HOTKEYS), so this lets the pad through and nothing else. Locked, not seeded, because every player already has the key set False."),
    locked("Dolphin.ini", "Analytics", "Enabled", "False", &["False"],
        "No telemetry. Stated explicitly rather than left to an upstream default that could change and quietly turn it on."),
    locked("Dolphin.ini", "Analytics", "PermissionAsked", "True", &["True"],
        "Suppresses Dolphin's own analytics prompt, which is a popup the player never asked to see."),
    locked("GFX.ini", "Settings", "BorderlessFullscreen", "True", &[],
        "Exclusive fullscreen hard-hangs Windows, not just the game. Reported 2026-09-13: Print Screen or F12 in fullscreen froze the machine, task-manager-only recovery. Neither key is a Reborn binding, so the trigger is an outside capture overlay, and Windows 11 gives Print Screen to the Snipping Tool while Steam gives F12 to its own. Against a D3D11 exclusive swapchain the capture forces a display mode switch and device reset, and with BackendMultithreading and ReShade on that same swapchain it deadlocks with DWM blocked behind it. Borderless has no mode switch, so the overlay just composites. Locked rather than seeded because every affected player already has a GFX.ini, and a seeded key would never reach them."),

    // ------------------------------------------------------- RETIRED PER-GAME
    //
    // Every one of these was forced into GameSettings by a shipped build. They
    // are the player's now, but retiring them in source does nothing on its
    // own: IniDocument::merge never removes a key, so they are still in every
    // install, still outranking Dolphin's own GUI. reconcile_once prunes them.
    retired_pin("Core", "SyncGPU", &["True", "False"],
        "Serialises the CPU and GPU threads and is one of the most expensive options in Dolphin. Forced True on 09-05 to chase the 0-FPS freeze; the 09-11 probe proved the freeze recurs with it on, so it was pure cost. The player's to spend."),
    retired_pin("Core", "GFXBackend", &["D3D"],
        "The real requirement is ReShade's dxgi.dll hook, not the game. A player whose driver works better on Vulkan or OpenGL keeps that choice; the consequence is stated, not overridden."),
    retired_pin("Video_Settings", "InternalResolution", &["1", "3", "4", "6"],
        "The single biggest performance lever, and the one a player on a weak GPU is told to lower. Pinned here it could not be lowered at all."),
    retired_pin("Video_Settings", "MSAA", &["1"],
        "Anti-aliasing. The launcher now offers it as a preference, and this stale pin is exactly what made that preference inert."),
    retired_pin("Video_Settings", "SSAA", &["False"],
        "Supersampling. Taste, and expensive, but the player's call."),
    retired_pin("Video_Hacks", "DeferEFBCopies", &["False", "True"],
        "A performance trade. The pinned value itself flipped between shipped builds, changing the game's behavior under players who had chosen otherwise."),
    retired_pin("Video_Hacks", "EFBToTextureEnable", &["True"],
        "An accuracy-versus-speed hack. Portraits stay correct through EFBAccessEnable, which is locked separately."),
    retired_pin("Video_Hacks", "ImmediateXFBEnable", &["True"],
        "A latency trade the player may want the other way on their display."),
    retired_pin("Video_Hacks", "XFBToTextureEnable", &["True"],
        "An accuracy-versus-speed hack."),
    retired_pin("Video_Settings", "DumpTextures", &["False"],
        "Pinned off, which meant a modder could not dump textures at all. Turning it on costs a disk write per texture, which is their decision to make."),
    retired_pin("Video_Settings", "DumpBaseTextures", &["False"], "As DumpTextures."),
    retired_pin("Video_Settings", "DumpMipTextures", &["False"], "As DumpTextures."),
    retired_pin("Video_Enhancements", "ArbitraryMipmapDetection", &["False"],
        "A per-upload CPU cost the HD pack makes mostly waste, but not a correctness requirement."),

    // ---------------------------------------------------------------- PLAYER
    //
    // Graphics. Forced into GFX.ini every launch until now.
    with_ui(player("GFX.ini", "Settings", "InternalResolution", &["1", "3", "4", "6"],
        "The player's resolution. See reshade::depth_filter_for: the ReShade depth filter is derived from this value, so freeing it and deriving the filter are one change."),
        Ui { id: "internal_resolution", kind: "resolution", default: "3" }),
    with_ui(with_repair(player("GFX.ini", "Settings", "MSAA", &["1", "4", "0x00000004"],
        "Anti-aliasing, and the key that proves why a prune has to be read against the layer under it. 4x was called the Reborn baseline (Adam, 2026-09-10) but was never once in effect: every shipped build pinned MSAA = 1 into the per-game layer, which outranks GFX.ini, so the template's 4x sat underneath doing nothing for the life of the product. Revision 1 pruned the pin and woke it, and players reported the game dropping into the 30s with a full field on screen the same day - multisampling is billed per covered sample and per silhouette edge, and eight players on a field is mostly silhouette edge. 4x also resolves the depth buffer the ReShade presets sample, which is the artifact the shipped template warns about in the same breath. The repair puts the game back on the value it has always actually run at. Raising it from the Settings page works now, for the first time, and the page states what it costs. Both spellings are listed because Dolphin rewrites GFX.ini from memory on exit and normalises 0x00000004 to 4."),
        "1"),
        Ui { id: "msaa", kind: "msaa", default: "1" }),
    player("GFX.ini", "Settings", "SSAA", &["False"], "Supersampling."),
    player("GFX.ini", "Settings", "MaxAnisotropy", &["4"],
        "Anisotropic filtering. Note the launcher was writing this into [Settings], where Dolphin does not read it; Dolphin's own key is [Enhancements] MaxAnisotropy."),
    player("GFX.ini", "Enhancements", "MaxAnisotropy", &[],
        "Where Dolphin actually keeps anisotropic filtering. Never written by any shipped build, so there is no forced value to recognise."),
    with_repair(player("GFX.ini", "Settings", "ShaderCompilationMode", &["2", "0"],
        "Shader compilation strategy. REVERTED to 2 on 2026-09-14 after shipping 0 on 09-13. 0 is Synchronous: the video thread BLOCKS until a specialised shader finishes compiling. 2 is Asynchronous Ubershaders: it never blocks, drawing with a general fallback shader until the specialised one is ready. The case for 0 was a day-field measurement on the dev box, which has had a warm shader cache for weeks. A player does not. Worse, the cache filename embeds a hash of the graphics host config (ShaderGenCommon.cpp GetDiskShaderCacheFileName, bits.msaa and friends), so a settings change silently opens an EMPTY cache rather than migrating the old one - and 3.0.9 changed settings and the compilation mode in the same release. A snap submits a step of around 366 new draws with new pipeline states all at once; under 0 with a cold cache that is a wall of blocking compiles. Players reported exactly that five hours after 3.0.9 went out: a multi-second drop at the hike, lasting until the throw, then a clean 60 once compiled, every play. Blocking the frame is never the right default for a player whose cache we just invalidated."),
        "2"),
    player("GFX.ini", "Settings", "WaitForShadersBeforeStarting", &["True"], "Whether to wait at boot or stutter in play."),
    player("GFX.ini", "Settings", "DumpTextures", &["False"], "A modder's switch."),
    player("GFX.ini", "Settings", "DumpBaseTextures", &["False"], "A modder's switch."),
    player("GFX.ini", "Settings", "DumpMipTextures", &["False"], "A modder's switch."),
    with_repair(player("GFX.ini", "Enhancements", "ArbitraryMipmapDetection", &["False", "True"],
        "A per-upload CPU cost, not a correctness requirement. The same shape as MSAA: the per-game pin held it off, the template underneath says True, and revision 1's prune woke it up. With CacheHiresTextures off the HD pack uploads during play, so the scan is paid mid-match for a pack whose mips are authored, which is what makes it waste. The repair puts it back off."), "False"),
    player("GFX.ini", "Hacks", "DeferEFBCopies", &["False", "True"], "A performance trade whose forced value flipped between shipped builds."),
    player("GFX.ini", "Hacks", "EFBToTextureEnable", &["True"], "An accuracy-versus-speed hack."),
    player("GFX.ini", "Hacks", "ImmediateXFBEnable", &["True"], "A latency trade."),
    player("GFX.ini", "Hacks", "XFBToTextureEnable", &["True"], "An accuracy-versus-speed hack."),
    // Core. SyncGPU gets a repair value: builds through 3.0.1 welded True into
    // every install at a real frame-rate cost, and merely dropping the force
    // would leave that cost in place forever.
    with_repair(player("Dolphin.ini", "Core", "SyncGPU", &["True", "False"],
        "One of the most expensive options in Dolphin, forced on to chase a freeze it was later proved not to fix."), "False"),
    player("Dolphin.ini", "General", "UseDiscordPresence", &["False"],
        "Dolphin's own Discord presence. Retired so the launcher can own presence later, but not a requirement."),

    // --------------------------------------------------------------- SEEDED
    //
    // The launcher's tuned defaults. Written only where the key is absent, so
    // a fresh install gets them and an existing one is left alone.
    // GFXBackend is seeded in Dolphin.ini ONLY, never in the per-game layer.
    // Seeding it there would put it straight back at the precedence Dolphin's
    // own Backend dropdown cannot beat, which is the pin we just retired.
    // Dolphin.ini is where Dolphin's GUI writes, so a fresh install gets D3D
    // and a player who switches backend keeps the switch.
    with_ui(seeded("Dolphin.ini", "Core", "GFXBackend", crate::dolphin_preferences::DEFAULT_VIDEO_BACKEND, &["D3D"],
        "ReShade's hook is dxgi.dll, so D3D is what the effects need. If the player picks another backend the choice is honored and the consequence is stated, not overridden. Retiring the per-game pin said that out loud on 2026-09-13 and still left the player with no way to switch: the launcher boots Dolphin with -u/-e, and -b on top of that in fullscreen, so \"the player never sees Dolphin's own interface in an ordinary flow\" (runtime::Launcher::args). Freeing a key is not the same as offering it. This page is the only surface there is, so the key carries a Ui now."),
        Ui { id: "video_backend", kind: "video_backend", default: crate::dolphin_preferences::DEFAULT_VIDEO_BACKEND }),
    seeded("GFX.ini", "Hacks", "EFBAccessEnable", "True", &["True"],
        "The per-game layer locks this; seeding the global layer keeps a fresh install consistent without taking the key twice."),
    locked("Logger.ini", "Options", "WriteToFile", "False", &["False"],
        "A shipped build must not write a debug log. The 2026-09 freeze instrumentation turned Dolphin's file logger on at Verbosity 4 and nothing turned it back off: measured on the dev install 2026-09-13, dolphin.log was 21.3 MB over 154,693 lines in a day, and every one of those lines is a synchronous disk write on the emulation thread. It costs most where the game is busiest, which is a full field of players, and it is one of the differences between this build and a stock Dolphin, which defaults the file logger off."),
    locked("Logger.ini", "Options", "Verbosity", "1", &["1", "4"],
        "The same instrumentation left Verbosity at 4, which is what makes the file logger expensive rather than merely on. Locked with it, because turning the writes off while leaving the verbosity would leave the next build one flag away from the same bill."),
    locked("Logger.ini", "Options", "WriteToConsole", "False", &["False"],
        "As WriteToFile. A console sink on a GUI build has no reader and still costs the format and the write."),
    locked("Logger.ini", "Options", "WriteToWindow", "False", &["False"],
        "As WriteToFile. The log window is not shown in an ordinary launch, so the sink only costs."),
];

fn overlay_for(file: &str, owner: Owner) -> Overlay {
    let mut out = Overlay::new();
    for key in KEYS.iter().filter(|k| k.file == file && k.owner == owner) {
        if let Some(value) = key.value {
            out.entry(key.section.to_string())
                .or_default()
                .insert(key.key.to_string(), value.to_string());
        }
    }
    out
}

/// The keys the launcher forces in `file`, every launch.
pub fn locked_overlay(file: &str) -> Overlay {
    overlay_for(file, Owner::Locked)
}

/// The launcher's defaults for `file`, to be applied with
/// [`crate::ini::IniDocument::merge_absent`] and never with `merge`.
pub fn seeded_overlay(file: &str) -> Overlay {
    overlay_for(file, Owner::Seeded)
}

/// What goes into `GameSettings/<game id>.ini`: the locked requirements, and
/// nothing else. This is the layer Dolphin's own GUI cannot beat, so anything
/// here is a setting the player cannot change at all.
pub fn locked_game_settings() -> Overlay {
    locked_overlay(GAME_SETTINGS)
}

/// Keys a shipped build once pinned into the per-game layer and no longer
/// should. [`reconcile_once`] removes these.
pub fn retired_game_settings() -> impl Iterator<Item = &'static Key> {
    KEYS.iter().filter(|k| k.file == GAME_SETTINGS && k.retired)
}

/// Every key the launcher's own Settings page offers, whatever class it sits
/// in. `Seeded` belongs here as much as `Player` does, because seeding writes
/// only where the key is absent and so never fights a control; the video
/// backend is exactly that shape. Filtering on `Player` alone meant an offered
/// key could be `Locked` elsewhere with the cross-check below never seeing it.
pub fn offered_fields() -> impl Iterator<Item = &'static Key> {
    KEYS.iter().filter(|k| k.ui.is_some())
}

/// Look a key up by where it lives.
pub fn find(file: &str, section: &str, key: &str) -> Option<&'static Key> {
    KEYS.iter().find(|k| k.file == file && k.section == section && k.key == key)
}

fn marker(platform: &Platform) -> Result<std::path::PathBuf> {
    Ok(platform
        .paths
        .data_root()?
        .join(DataDir::Config.relative())
        .join("settings-law-revision"))
}

/// Whether [`reconcile_once`] still has work to do on this install.
pub fn reconcile_pending(platform: &Platform) -> Result<bool> {
    let path = marker(platform)?;
    let done = std::fs::read_to_string(&path)
        .ok()
        .and_then(|t| t.trim().parse::<u32>().ok())
        .unwrap_or(0);
    Ok(done < REVISION)
}

fn mark_reconciled(platform: &Platform) -> Result<()> {
    let path = marker(platform)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, REVISION.to_string())?;
    Ok(())
}

/// What one reconcile pass did, so the caller can log it honestly.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Reconciled {
    /// `GameSettings` pins removed, as `<game id>/<section>/<key>`.
    pub pruned: Vec<String>,
    /// Baked cheat codes switched back off, as `<game id>/<section>/<name>`.
    pub disabled: Vec<String>,
    /// Config keys put back to the player's own value, as `<file>/<section>/<key>`.
    pub restored: Vec<String>,
    /// Config keys given their repair value because no evidence survived.
    pub repaired: Vec<String>,
    /// Files actually rewritten.
    pub files: Vec<std::path::PathBuf>,
}

impl Reconciled {
    pub fn is_empty(&self) -> bool {
        self.pruned.is_empty()
            && self.disabled.is_empty()
            && self.restored.is_empty()
            && self.repaired.is_empty()
    }
}

/// Undo what the launcher wrote into this install while it had only one verb.
///
/// Runs once per install per [`REVISION`]. Three steps, in order:
///
/// 1. **Prune.** Take every [`Key::retired`] pin out of each game's
///    `GameSettings` file. Safe by construction: the launcher force-merged
///    exactly those keys on every launch, so no player-authored value could
///    have survived in them. Everything else in the file - Gecko codes,
///    ActionReplay blocks, `CPUThread`, values set through Dolphin's own
///    Properties dialog, comments, ordering - is untouched.
/// 2. **Restore.** For each retired key in `Dolphin.ini` and `GFX.ini`, if the
///    `.ini.launcher-original` snapshot taken before the launcher's first write
///    still exists, and the live value is one this launcher is known to have
///    forced, put the original value back. A live value outside
///    [`Key::forced_values`] was chosen by somebody and is left alone. So is a
///    snapshot value outside it - but a snapshot value *inside* it is the
///    launcher's own template echoed back, not a preference, and on a key that
///    names a repair it is discarded in favour of step 3.
/// 3. **Repair.** Where the restore found no evidence it could trust, write
///    [`Key::repair_once`] if the key has one. Un-forcing a key does not undo
///    what is already in the file: `SyncGPU = True` costs frames in every
///    install that a build through 3.0.1 ever touched, and `MSAA` has to land
///    on the value the game actually ran at rather than the one the template
///    had been carrying unreachably underneath the pin.
///
/// This is a one-shot repair, not a policy. A player who afterwards puts
/// `MSAA = 1` back into their own `GameSettings` file keeps it; a prune that
/// ran every launch would be the original bug wearing a different hat.
pub fn reconcile_once(platform: &Platform, user_dir: &Path, game_ids: &[&str]) -> Result<Reconciled> {
    if !reconcile_pending(platform)? {
        return Ok(Reconciled::default());
    }
    let out = reconcile_in(user_dir, game_ids)?;
    mark_reconciled(platform)?;
    Ok(out)
}

/// The repair itself, without the once-per-install marker. Separated so it can
/// be tested against a copy of a real user folder.
pub fn reconcile_in(user_dir: &Path, game_ids: &[&str]) -> Result<Reconciled> {
    let mut out = Reconciled::default();

    for game_id in game_ids {
        let path = user_dir.join("GameSettings").join(format!("{game_id}.ini"));
        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        let mut doc = crate::ini::IniDocument::parse(&text);
        let mut changed = false;
        for key in retired_game_settings() {
            if doc.remove(key.section, key.key) {
                out.pruned.push(format!("{game_id}/{}/{}", key.section, key.key));
                changed = true;
            }
        }
        // Switch off the codes the disc already carries. Dolphin keeps these
        // lists as bare `$name` lines, so they go through the raw-line door
        // rather than `remove`, and only the names in `BAKED_CODES` are
        // dropped - anything else in the section is the player's.
        for section in ["Gecko_Enabled", "ActionReplay_Enabled"] {
            let body = doc.raw_lines(section);
            if body.is_empty() {
                continue;
            }
            let kept: Vec<String> = body
                .iter()
                .filter(|line| {
                    let name = line.trim().trim_start_matches('$').trim();
                    !BAKED_CODES
                        .iter()
                        .any(|(s, n)| *s == section && n.eq_ignore_ascii_case(name))
                })
                .cloned()
                .collect();
            if kept.len() == body.len() {
                continue;
            }
            for line in &body {
                let name = line.trim().trim_start_matches('$').trim();
                if BAKED_CODES.iter().any(|(s, n)| *s == section && n.eq_ignore_ascii_case(name)) {
                    out.disabled.push(format!("{game_id}/{section}/{name}"));
                }
            }
            // An enabled list with nothing left in it is the same to Dolphin as
            // no list at all, and that is what a clean profile looks like.
            let kept = if kept.iter().all(|l| l.trim().is_empty()) { Vec::new() } else { kept };
            doc.replace_raw_lines(section, &kept);
            changed = true;
        }
        if changed {
            let backup = path.with_extension("ini.launcher-original");
            if !backup.exists() {
                std::fs::write(&backup, &text)?;
            }
            std::fs::write(&path, doc.render())?;
            out.files.push(path);
        }
    }

    for file in ["Dolphin.ini", "GFX.ini"] {
        let path = user_dir.join("Config").join(file);
        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        let original = std::fs::read_to_string(path.with_extension("ini.launcher-original"))
            .ok()
            .map(|t| crate::ini::IniDocument::parse(&t));
        let mut doc = crate::ini::IniDocument::parse(&text);
        let mut changed = false;

        for key in KEYS.iter().filter(|k| k.file == file && k.owner == Owner::Player) {
            let Some(live) = doc.get(key.section, key.key).map(str::trim).map(str::to_string) else { continue };
            if !key.forced_values.contains(&live.as_str()) {
                // Somebody chose this. Not ours to touch, whatever it says.
                continue;
            }
            let mine = original.as_ref().and_then(|o| o.get(key.section, key.key)).map(str::trim).map(str::to_string);
            // A snapshot value the launcher itself is known to have written is
            // not evidence of a preference. `.ini.launcher-original` is a copy
            // taken before the launcher's first *merge*, but the file it copies
            // was already seeded whole from the shipped template, so for a key
            // nobody could reach it can only echo that template back. It did:
            // the 2026-09-13 report traced to `GFX.ini.launcher-original`
            // carrying the same `MSAA = 0x00000004` as the live file, so the
            // restore "found" the player's value, saw it already matched, and
            // the repair never ran. Where a key names a repair, that is the
            // value to land on; the snapshot only speaks when it says something
            // no shipped build ever wrote, which is the same test the live
            // value gets three lines above.
            let mine = match mine {
                Some(value)
                    if key.repair_once.is_some() && key.forced_values.contains(&value.as_str()) =>
                {
                    None
                }
                other => other,
            };
            let (value, bucket) = match mine {
                Some(value) => (value, &mut out.restored),
                None => match key.repair_once {
                    Some(value) => (value.to_string(), &mut out.repaired),
                    None => continue,
                },
            };
            if value == live {
                continue;
            }
            let mut one = Overlay::new();
            one.entry(key.section.to_string()).or_default().insert(key.key.to_string(), value);
            if !doc.merge(&one).is_noop() {
                bucket.push(format!("{file}/{}/{}", key.section, key.key));
                changed = true;
            }
        }

        if changed {
            std::fs::write(&path, doc.render())?;
            out.files.push(path);
        }
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// "after i open the app i cant get pass the press start screen". Dolphin
    /// drops ALL controller input while its render window is not focused
    /// (`Core::UpdateInputGate`, gated on `[Input] BackgroundInput`), and the
    /// file every player is given says False. Keyboard hotkeys have their own
    /// gate (`MAIN_FOCUSED_HOTKEYS`), so this lets the PAD through and nothing
    /// else. Locked, not seeded: every affected player already has the key.
    #[test]
    fn a_controller_still_works_when_the_game_window_is_not_the_focused_one() {
        let key = KEYS
            .iter()
            .find(|k| k.file == "Dolphin.ini" && k.section == "Input" && k.key == "BackgroundInput")
            .expect("the launcher does not manage BackgroundInput at all");
        assert_eq!(key.owner, Owner::Locked, "a seeded key never reaches a player who already has it");
        assert_eq!(key.value, Some("True"));
    }

    /// The locked set is the whole compromise. It is written out by name here
    /// so that adding to it means editing this test, which means saying out
    /// loud that the launcher is taking another setting away from the player.
    #[test]
    fn the_locked_set_is_exactly_these_seventeen_keys_and_each_states_why() {
        let mut locked: Vec<_> = KEYS
            .iter()
            .filter(|k| k.owner == Owner::Locked)
            .map(|k| (k.file, k.section, k.key))
            .collect();
        locked.sort();

        let mut expected = vec![
            (GAME_SETTINGS, "Core", "RAMOverrideEnable"),
            (GAME_SETTINGS, "Core", "MEM1Size"),
            (GAME_SETTINGS, "Video_Hacks", "EFBAccessEnable"),
            (GAME_SETTINGS, "Video_Settings", "HiresTextures"),
            (GAME_SETTINGS, "Video_Settings", "CacheHiresTextures"),
            ("Dolphin.ini", "Core", "RAMOverrideEnable"),
            ("Dolphin.ini", "Core", "MEM1Size"),
            ("Dolphin.ini", "Interface", "UsePanicHandlers"),
            // Added 2026-09-13: both shipped images are NKit, so Dolphin
            // opened its NKit warning before every boot and waited for a
            // click the player could not find.
            ("Dolphin.ini", "Interface", "SkipNKitWarning"),
            ("Dolphin.ini", "Analytics", "Enabled"),
            ("Dolphin.ini", "Analytics", "PermissionAsked"),
            // Added 2026-09-19: with this off, Dolphin throws away every
            // controller press while the game window is not the focused one.
            // A player who clicks anything else during the shader compile gets a
            // game that opens unfocused and a pad that does nothing, on every
            // kind of controller, until they click the game with a mouse.
            ("Dolphin.ini", "Input", "BackgroundInput"),
            // Added 2026-09-13: exclusive fullscreen hard-hangs Windows when an
            // outside capture overlay (Print Screen, Steam's F12) grabs the
            // screen. Locked, not seeded, because every affected player already
            // has a GFX.ini and a seeded key would never reach them.
            ("GFX.ini", "Settings", "BorderlessFullscreen"),
            // Added 2026-09-13, for exactly the reason the line above states and
            // for want of anybody applying it here the first time. These four
            // were SEEDED, and seeding writes only where a key is absent: the
            // freeze instrumentation had already set all four on every install
            // that ran it, so the seed reached none of them. dolphin.log was
            // still 21.3 MB a day later, a synchronous write per line at
            // Verbosity 4, on the emulation thread.
            ("Logger.ini", "Options", "WriteToFile"),
            ("Logger.ini", "Options", "Verbosity"),
            ("Logger.ini", "Options", "WriteToConsole"),
            ("Logger.ini", "Options", "WriteToWindow"),
        ];
        expected.sort();

        assert_eq!(locked, expected, "the locked set changed; say why in the table and here");
        for key in KEYS.iter().filter(|k| k.owner == Owner::Locked) {
            assert!(key.value.is_some(), "{}/{} is locked with no value", key.section, key.key);
            assert!(
                key.why.len() > 40,
                "{}/{} is locked without saying what breaks without it",
                key.section,
                key.key
            );
        }
    }

    /// The real cross-check: nothing the Settings page offers may be a key the
    /// launcher forces.
    ///
    /// `dolphin_preferences::FIELDS` is the list the UI actually renders, and
    /// it is maintained by hand, so it can drift from this table. It did:
    /// `BorderlessFullscreen` was locked on 2026-09-13 to stop a whole-PC hang
    /// while still being offered as a toggle defaulting to False, so switching
    /// it off appeared to work and was forced back on the next launch. The
    /// sibling test below only walks THIS table's `ui` rows, so it could not
    /// see that. This one walks the list the player actually sees.
    #[test]
    fn nothing_the_settings_page_offers_is_a_key_the_launcher_forces() {
        for field in crate::dolphin_preferences::FIELDS {
            for locked in KEYS.iter().filter(|k| k.owner == Owner::Locked) {
                assert!(
                    !(locked.section == field.section && locked.key == field.key),
                    "the Settings page offers {}/{} while the launcher forces it every launch: \
                     a control that silently reverts is the bug this module exists to stop",
                    field.section,
                    field.key,
                );
            }
        }
    }

    /// A key cannot be both the launcher's and the player's. The old code had
    /// exactly this contradiction: MSAA was offered as a preference in GFX.ini
    /// and pinned to 1 in the per-game layer that outranks it.
    #[test]
    fn no_key_is_locked_in_one_place_and_offered_in_another() {
        for field in offered_fields() {
            for locked in KEYS.iter().filter(|k| k.owner == Owner::Locked) {
                assert!(
                    !(locked.section == field.section && locked.key == field.key),
                    "{}/{} is offered to the player and locked by the launcher",
                    field.section,
                    field.key
                );
            }
        }
    }

    /// Player keys are never written, so a value would be meaningless. A value
    /// on a player row is the mistake this catches.
    #[test]
    fn a_player_key_never_carries_a_value() {
        for key in KEYS.iter().filter(|k| k.owner == Owner::Player) {
            assert!(
                key.value.is_none(),
                "{}/{} is the player's but carries a value the launcher would write",
                key.section,
                key.key
            );
        }
    }

    /// Every row is reachable and unique: two rows for the same key would make
    /// the overlay depend on table order.
    #[test]
    fn every_key_is_declared_once() {
        let mut seen = std::collections::BTreeSet::new();
        for key in KEYS {
            assert!(
                seen.insert((key.file, key.section, key.key)),
                "{}/{}/{} is declared twice",
                key.file,
                key.section,
                key.key
            );
            assert!(!key.why.is_empty(), "{}/{} has no why", key.section, key.key);
        }
    }

    /// The per-game layer is the one Dolphin's GUI cannot beat, so it carries
    /// the requirements and nothing else. Every taste key that used to live
    /// here is retired, not merely unwritten.
    #[test]
    fn the_game_settings_overlay_holds_only_requirements() {
        let overlay = locked_game_settings();
        let mut count = 0;
        for (section, keys) in &overlay {
            for key in keys.keys() {
                count += 1;
                let declared = find(GAME_SETTINGS, section, key).expect("in the table");
                assert_eq!(declared.owner, Owner::Locked);
            }
        }
        assert_eq!(count, 5, "five requirements belong in the per-game layer");
        assert!(overlay.get("Video_Settings").is_none_or(|s| !s.contains_key("InternalResolution")));
        assert!(overlay.get("Video_Settings").is_none_or(|s| !s.contains_key("MSAA")));
        assert!(overlay.get("Core").is_none_or(|s| !s.contains_key("SyncGPU")));
    }

    /// Print Screen / F12 in fullscreen hard-hung the whole PC (2026-09-13).
    ///
    /// The fix only works if it reaches players who already have a `GFX.ini`,
    /// which is all of them. `Seeded` writes only where the key is absent, so
    /// this one has to be `Locked` or the hang survives the update. Confirmed
    /// in the real renderer before shipping.
    #[test]
    fn borderless_fullscreen_is_locked_so_it_reaches_an_existing_install() {
        let key = find("GFX.ini", "Settings", "BorderlessFullscreen")
            .expect("BorderlessFullscreen must be declared");
        assert!(
            matches!(key.owner, Owner::Locked),
            "seeded would skip every existing GFX.ini, which is every affected player",
        );
        assert_eq!(key.value, Some("True"));
        assert_eq!(
            locked_overlay("GFX.ini").get("Settings").and_then(|s| s.get("BorderlessFullscreen")),
            Some(&"True".to_string()),
        );
    }

    /// The forced-value sets came from `git log -S` over runtime.rs, and the
    /// values genuinely moved between shipped builds. A single-value check
    /// would skip the installs stamped by the older ones.
    #[test]
    fn the_forced_value_sets_cover_every_shipped_build() {
        let sync = find(GAME_SETTINGS, "Core", "SyncGPU").unwrap();
        assert!(sync.forced_values.contains(&"True"), "builds through 3.0.1 forced True");
        assert!(sync.forced_values.contains(&"False"), "3.0.2 and later force False");

        let defer = find(GAME_SETTINGS, "Video_Hacks", "DeferEFBCopies").unwrap();
        assert!(defer.forced_values.contains(&"False") && defer.forced_values.contains(&"True"));

        let resolution = find("GFX.ini", "Settings", "InternalResolution").unwrap();
        for value in ["1", "3", "4", "6"] {
            assert!(
                resolution.forced_values.contains(&value),
                "the per-preset table returned {value} before it was flattened to 3"
            );
        }
    }

    /// The report behind the control: revision 1 retired the per-game
    /// `GFXBackend` pin so that "a player whose driver works better on Vulkan
    /// or OpenGL keeps that choice", and then nothing anywhere offered the
    /// choice. `runtime::Launcher::args` boots Dolphin with `-u`/`-e`, and
    /// `-b` on top in fullscreen, so Dolphin's own Graphics window is never on
    /// screen in an ordinary flow: this page is the only surface a player has.
    /// Freeing a key and offering it are two different jobs and only the first
    /// one had been done.
    ///
    /// Offering a `Seeded` key is safe for exactly one reason, and the second
    /// half of this test is that reason: seeding writes where the key is
    /// absent and nowhere else, so it can never take the switch back.
    #[test]
    fn the_backend_is_offered_and_a_switch_survives_the_next_launch() {
        let key = find("Dolphin.ini", "Core", "GFXBackend").expect("declared");
        assert!(matches!(key.owner, Owner::Seeded), "a fresh install still has to land on D3D");
        let ui = key.ui.expect("a key nothing offers is a key nobody can change");
        assert_eq!(ui.id, "video_backend");
        assert!(
            crate::dolphin_preferences::FIELDS.iter().any(|f| {
                f.id == ui.id && f.file == key.file && f.section == key.section && f.key == key.key
            }),
            "this table says the backend is offered, so the page that renders it has to agree",
        );

        // The player has switched to Vulkan. The next launch seeds and locks
        // as it does on every launch.
        let mut theirs = crate::ini::IniDocument::parse("[Core]\nGFXBackend = Vulkan\n");
        theirs.merge(&locked_overlay("Dolphin.ini"));
        theirs.merge_absent(&seeded_overlay("Dolphin.ini"));
        assert_eq!(
            theirs.get("Core", "GFXBackend"),
            Some("Vulkan"),
            "seeding took the player's switch back, which is the whole bug this module exists to stop",
        );

        // A fresh install, where nobody has said anything, still gets D3D.
        let mut fresh = crate::ini::IniDocument::parse("[Core]\n");
        fresh.merge_absent(&seeded_overlay("Dolphin.ini"));
        assert_eq!(fresh.get("Core", "GFXBackend"), Some(crate::dolphin_preferences::DEFAULT_VIDEO_BACKEND));

        // And the per-game layer must stay out of it, or the control is inert
        // the same way the MSAA control was.
        assert!(
            locked_game_settings().get("Core").is_none_or(|s| !s.contains_key("GFXBackend")),
            "pinned in the layer Dolphin's own GUI cannot beat is how this started",
        );
    }

    /// A per-game file shaped like the real one on this machine:
    /// `dolphin-users/migrated-1789249231540/GameSettings/GN7E69.ini`, which
    /// carries Gecko codes and an ActionReplay block above the pins, and a
    /// player-set `CPUThread = False` in among them.
    fn live_game_settings() -> &'static str {
        "[Gecko]\n\
         $Remove GameBreaker 2 [Ludiyolo252]\n\
         C20D411C 00000003\n\
         *Removes GameBreaker 2\n\
         [ActionReplay]\n\
         $Legends Text Code\n\
         0428E9C8 4368616D\n\
         [Core]\n\
         CPUThread = False\n\
         SyncGPU = True\n\
         RAMOverrideEnable = True\n\
         MEM1Size = 67108864\n\
         GFXBackend = D3D\n\
         [Video_Settings]\n\
         InternalResolution = 3\n\
         MSAA = 1\n\
         SSAA = False\n\
         HiresTextures = True\n\
         CacheHiresTextures = False\n\
         [Video_Hacks]\n\
         EFBAccessEnable = True\n\
         DeferEFBCopies = True\n\
         EFBToTextureEnable = True\n\
         ImmediateXFBEnable = True\n\
         XFBToTextureEnable = True\n"
    }

    fn user_dir_with(game_settings: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("GameSettings")).unwrap();
        std::fs::create_dir_all(dir.path().join("Config")).unwrap();
        std::fs::write(dir.path().join("GameSettings/GN7E69.ini"), game_settings).unwrap();
        dir
    }

    /// A profile seeded from the shipped template: the four baked codes are
    /// switched on, and so is one the player turned on themselves.
    fn game_settings_with_enabled_codes() -> String {
        format!(
            "{}[Gecko_Enabled]\n\
             $Remove GameBreaker 2\n\
             $GB Takes Longer (v2)\n\
             $Point Remover\n\
             $My Own Code\n\
             [ActionReplay_Enabled]\n\
             $Legends Text Code\n\
             [OnFrame_Enabled]\n\
             $Reborn GameCube controls (no X/B swap)\n",
            live_game_settings()
        )
    }

    fn enabled_in(dir: &Path, section: &str) -> Vec<String> {
        let text = std::fs::read_to_string(dir.join("GameSettings/GN7E69.ini")).unwrap();
        crate::ini::IniDocument::parse(&text)
            .raw_lines(section)
            .iter()
            .map(|l| l.trim().trim_start_matches('$').trim().to_string())
            .filter(|l| !l.is_empty())
            .collect()
    }

    /// The disc already branches to a cave at all three Gecko hook addresses
    /// and already holds the eleven Legends names, so a profile that also
    /// enables the codes applies them twice. Verified against `main.dol` in the
    /// 2026-09-19 image before this rule was written.
    #[test]
    fn codes_the_disc_already_carries_are_switched_off() {
        let dir = user_dir_with(&game_settings_with_enabled_codes());
        let done = reconcile_in(dir.path(), &["GN7E69"]).unwrap();

        assert_eq!(
            enabled_in(dir.path(), "Gecko_Enabled"),
            vec!["My Own Code".to_string()],
            "the three baked Gecko codes go, the player's own stays",
        );
        assert!(
            enabled_in(dir.path(), "ActionReplay_Enabled").is_empty(),
            "the Legends text is baked at 0x8028E9C8; enabling it writes the same names again",
        );
        assert_eq!(
            done.disabled.len(),
            4,
            "all four are reported, so the log says honestly what changed: {:?}",
            done.disabled,
        );
    }

    /// `OnFrame_Enabled` carries the GameCube controls patch, which is live and
    /// correct. Clearing whole sections instead of matching names would take it
    /// out and hand every GameCube player the X/B swap back.
    #[test]
    fn the_gamecube_controls_patch_is_left_enabled() {
        let dir = user_dir_with(&game_settings_with_enabled_codes());
        reconcile_in(dir.path(), &["GN7E69"]).unwrap();

        assert_eq!(
            enabled_in(dir.path(), "OnFrame_Enabled"),
            vec!["Reborn GameCube controls (no X/B swap)".to_string()],
        );
    }

    /// The code BODIES are not the problem and are not ours to delete. Only the
    /// enabled list moves, so anyone can switch a code back on by hand.
    #[test]
    fn disabling_a_code_leaves_its_body_intact() {
        let dir = user_dir_with(&game_settings_with_enabled_codes());
        reconcile_in(dir.path(), &["GN7E69"]).unwrap();

        let text = std::fs::read_to_string(dir.path().join("GameSettings/GN7E69.ini")).unwrap();
        assert!(text.contains("C20D411C 00000003"), "Gecko body must survive");
        assert!(text.contains("0428E9C8 4368616D"), "ActionReplay body must survive");
        assert!(text.contains("$Remove GameBreaker 2 [Ludiyolo252]"), "the [Gecko] entry stays");
    }

    /// A profile that never had the codes on must come out unchanged, so the
    /// pass is idempotent and a second run reports nothing.
    #[test]
    fn a_profile_with_no_enabled_codes_is_untouched() {
        let dir = user_dir_with(&game_settings_with_enabled_codes());
        reconcile_in(dir.path(), &["GN7E69"]).unwrap();
        let after_first = std::fs::read_to_string(dir.path().join("GameSettings/GN7E69.ini")).unwrap();

        let done = reconcile_in(dir.path(), &["GN7E69"]).unwrap();
        let after_second = std::fs::read_to_string(dir.path().join("GameSettings/GN7E69.ini")).unwrap();

        assert!(done.disabled.is_empty(), "nothing left to disable: {:?}", done.disabled);
        assert_eq!(after_first, after_second, "the pass must be idempotent");
    }

    /// The prune is key-scoped. Gecko codes, ActionReplay blocks and the
    /// player's own `CPUThread` are the positive control: if any of them moves,
    /// the prune is reaching past the table and must not ship.
    #[test]
    fn a_retired_pin_is_removed_from_an_old_install() {
        let dir = user_dir_with(live_game_settings());
        let done = reconcile_in(dir.path(), &["GN7E69"]).unwrap();

        let text = std::fs::read_to_string(dir.path().join("GameSettings/GN7E69.ini")).unwrap();
        let doc = crate::ini::IniDocument::parse(&text);

        // Gone: every taste key an older build pinned where Dolphin could not beat it.
        for (section, key) in [
            ("Core", "SyncGPU"),
            ("Core", "GFXBackend"),
            ("Video_Settings", "InternalResolution"),
            ("Video_Settings", "MSAA"),
            ("Video_Settings", "SSAA"),
            ("Video_Hacks", "DeferEFBCopies"),
            ("Video_Hacks", "EFBToTextureEnable"),
        ] {
            assert_eq!(doc.get(section, key), None, "{section}/{key} should have been pruned");
        }

        // Kept: the requirements, and everything that was never ours.
        assert_eq!(doc.get("Core", "RAMOverrideEnable"), Some("True"));
        assert_eq!(doc.get("Core", "MEM1Size"), Some("67108864"));
        assert_eq!(doc.get("Video_Hacks", "EFBAccessEnable"), Some("True"));
        assert_eq!(doc.get("Video_Settings", "CacheHiresTextures"), Some("False"));
        assert_eq!(doc.get("Core", "CPUThread"), Some("False"), "the player's freeze workaround");
        assert!(text.contains("$Remove GameBreaker 2 [Ludiyolo252]"), "Gecko codes must survive");
        assert!(text.contains("C20D411C 00000003"), "Gecko payload must survive");
        assert!(text.contains("$Legends Text Code"), "ActionReplay must survive");

        assert!(!done.pruned.is_empty());
        // The snapshot of what it looked like before we touched it.
        assert_eq!(
            std::fs::read_to_string(dir.path().join("GameSettings/GN7E69.ini.launcher-original")).unwrap(),
            live_game_settings(),
        );
    }

    /// The shipped `GFX.ini`, as a player actually receives it: the tuned
    /// template named by `build.ps1 -UserTemplate`, after
    /// [`crate::user_template::sanitize_gfx_ini`], which turns the dump flags
    /// and the FPS counter off and leaves "everything else, the anti-aliasing
    /// and filtering included" at the tuned value. Checked on 2026-09-13
    /// against `runtime/User/Config/GFX.ini` in the live install.
    fn shipped_gfx_ini() -> &'static str {
        "[Enhancements]\n\
         ArbitraryMipmapDetection = True\n\
         DisableCopyFilter = True\n\
         ForceTrueColor = True\n\
         ForceFiltering = True\n\
         MaxAnisotropy = 3\n\
         PostProcessingShader =\n\
         [Hacks]\n\
         BBoxEnable = False\n\
         DeferEFBCopies = True\n\
         EFBAccessEnable = True\n\
         EFBEmulateFormatChanges = False\n\
         EFBScaledCopy = True\n\
         EFBToTextureEnable = True\n\
         ImmediateXFBEnable = True\n\
         SkipDuplicateXFBs = True\n\
         XFBToTextureEnable = True\n\
         [Hardware]\n\
         VSync = True\n\
         [Settings]\n\
         BorderlessFullscreen = True\n\
         BackendMultithreading = True\n\
         CacheHiresTextures = True\n\
         DumpBaseTextures = False\n\
         DumpMipTextures = False\n\
         EnableGPUTextureDecoding = True\n\
         FastDepthCalc = True\n\
         HiresTextures = True\n\
         InternalResolution = 3\n\
         InternalResolutionFrameDumps = False\n\
         SaveTextureCacheToState = True\n\
         ShaderCompilationMode = 2\n\
         ShowFPS = False\n\
         ShowNetPlayPing = True\n\
         WaitForShadersBeforeStarting = True\n\
         EnablePixelLighting = True\n\
         MSAA = 0x00000004\n\
         SSAA = False\n\
         AspectRatio = 3\n\
         DumpTextures = False\n\
         MaxAnisotropy = 4\n"
    }

    /// The per-game section names, as `GameConfigLoader.cpp` maps them onto
    /// `GFX.ini`'s.
    fn base_section(section: &str) -> &str {
        match section {
            "Video_Settings" => "Settings",
            "Video_Enhancements" => "Enhancements",
            "Video_Hacks" => "Hacks",
            other => other,
        }
    }

    /// The value Dolphin would actually use, for the two layers an install
    /// has. `Common/Config/Enums.h` orders the search `LocalGame` before
    /// `Base`, so the per-game file wins wherever it has the key and
    /// `GFX.ini` answers everywhere it does not.
    fn effective(dir: &Path, section: &str, key: &str) -> Option<String> {
        let game = std::fs::read_to_string(dir.join("GameSettings/GN7E69.ini")).unwrap_or_default();
        let game_doc = crate::ini::IniDocument::parse(&game);
        if let Some(value) = game_doc.get(section, key) {
            return Some(value.trim().to_string());
        }
        let gfx = std::fs::read_to_string(dir.join("Config/GFX.ini")).unwrap_or_default();
        let gfx_doc = crate::ini::IniDocument::parse(&gfx);
        gfx_doc.get(base_section(section), key).map(|v| v.trim().to_string())
    }

    fn install() -> tempfile::TempDir {
        let dir = user_dir_with(live_game_settings());
        std::fs::write(dir.path().join("Config/GFX.ini"), shipped_gfx_ini()).unwrap();
        dir
    }

    /// Taking a pin out is not a no-op on behavior. It hands the key to the
    /// layer underneath, and the layer underneath is the shipped `GFX.ini`
    /// template, which has been carrying values nobody re-examined precisely
    /// because the pin made them unreachable.
    ///
    /// Reported by players on 2026-09-13, hours after the settings law went
    /// out: "it doesn't stutter anymore, it is just a general complete
    /// framerate drop into the 30s during play with everyone on screen." Both
    /// halves of that sentence are this one pass. `SyncGPU = True` was pruned,
    /// which is the stutter going away. `MSAA = 1` was pruned too, and the
    /// template's `MSAA = 0x00000004` underneath it woke up, so every install
    /// that reconciled switched 4x anti-aliasing on at once. Multisampling is
    /// billed per covered sample and per silhouette edge, which is why it
    /// reads as "with everyone on screen".
    #[test]
    fn a_prune_does_not_wake_the_value_the_pin_was_holding_down() {
        let dir = install();

        assert_eq!(
            effective(dir.path(), "Video_Settings", "MSAA").as_deref(),
            Some("1"),
            "before: the pin holds anti-aliasing off, which is how the game has always run",
        );

        reconcile_in(dir.path(), &["GN7E69"]).unwrap();

        assert_eq!(
            effective(dir.path(), "Video_Settings", "MSAA").as_deref(),
            Some("1"),
            "the prune turned 4x anti-aliasing on: the template's dead value became the live one",
        );
        assert_eq!(
            effective(dir.path(), "Video_Enhancements", "ArbitraryMipmapDetection").as_deref(),
            Some("False"),
            "the same defect with a smaller bill: a per-upload CPU scan the pin was holding off",
        );
    }

    /// Shader compilation never blocks the frame.
    ///
    /// This test asserted the opposite for one day. Revision 2 shipped
    /// `ShaderCompilationMode = 0` on the strength of a measurement taken on a
    /// box with a warm shader cache, and players - whose caches the same
    /// release invalidated, because the cache filename embeds the graphics host
    /// config - got a blocking compile at every snap instead. An install that
    /// revision 2 already stamped with 0 has to be carried back to 2, which is
    /// what revision 3 is for.
    #[test]
    fn the_repair_never_leaves_shader_compilation_blocking_the_frame() {
        let dir = install();
        // The shape revision 2 left behind: 0 in the player's own GFX.ini.
        let gfx = dir.path().join("Config/GFX.ini");
        let text = std::fs::read_to_string(&gfx).unwrap().replace("ShaderCompilationMode = 2", "ShaderCompilationMode = 0");
        std::fs::write(&gfx, text).unwrap();
        assert_eq!(effective(dir.path(), "Video_Settings", "ShaderCompilationMode").as_deref(), Some("0"));

        reconcile_in(dir.path(), &["GN7E69"]).unwrap();

        assert_eq!(
            effective(dir.path(), "Video_Settings", "ShaderCompilationMode").as_deref(),
            Some("2"),
            "0 blocks the video thread on a shader miss; a player with a cold cache stalls at every snap",
        );
    }

    /// Dolphin rewrites `GFX.ini` from memory when it exits and normalises as
    /// it writes, so the `0x00000004` the template shipped comes back as `4`
    /// after the first session. The repair has to recognise the value in the
    /// spelling the player's file is actually in, or it fires for nobody who
    /// has already played.
    #[test]
    fn the_repair_recognises_the_spelling_dolphin_writes_back() {
        let dir = install();
        let path = dir.path().join("Config/GFX.ini");
        std::fs::write(&path, shipped_gfx_ini().replace("MSAA = 0x00000004", "MSAA = 4")).unwrap();

        reconcile_in(dir.path(), &["GN7E69"]).unwrap();

        assert_eq!(
            effective(dir.path(), "Video_Settings", "MSAA").as_deref(),
            Some("1"),
            "Dolphin's own normalisation must not hide the value from the repair",
        );
    }

    /// Against the live install on this machine, which is the one the report
    /// came from. Revision 1 has already run here, so this folder is exactly
    /// the shape revision 2 has to repair: the `MSAA = 1` pin already gone out
    /// of `GameSettings`, and the woken `0x00000004` sitting in `GFX.ini`
    /// where nobody chose it. Copied first, because a test does not get to
    /// edit a player's configuration.
    #[test]
    fn the_repair_lands_on_the_live_install_on_this_machine_when_it_exists() {
        let live = Path::new(
            r"C:\Users\Adam\AppData\Roaming\NFL Street Reborn\dolphin-users\migrated-1789249231540",
        );
        if !live.join("Config/GFX.ini").is_file() {
            return;
        }

        let dir = tempfile::tempdir().unwrap();
        for sub in ["Config", "GameSettings"] {
            std::fs::create_dir_all(dir.path().join(sub)).unwrap();
            for entry in std::fs::read_dir(live.join(sub)).unwrap() {
                let entry = entry.unwrap();
                if entry.file_type().unwrap().is_file() {
                    std::fs::copy(entry.path(), dir.path().join(sub).join(entry.file_name())).unwrap();
                }
            }
        }

        let before = effective(dir.path(), "Video_Settings", "MSAA");
        if !matches!(before.as_deref(), Some("4" | "0x00000004")) {
            // Already repaired, by hand on 2026-09-13 or by a build carrying
            // this fix. The folder no longer has the defect, so there is
            // nothing to prove against it; that is not a failure.
            return;
        }

        reconcile_in(dir.path(), &["GN7E69"]).unwrap();

        assert_eq!(
            effective(dir.path(), "Video_Settings", "MSAA").as_deref(),
            Some("1"),
            "the repair has to reach the real file, not just the fixture",
        );
        assert_eq!(
            std::fs::read_to_string(dir.path().join("GameSettings/GN7E69.ini"))
                .unwrap()
                .contains("CPUThread = False"),
            true,
            "the player's own freeze workaround is not ours to touch",
        );
    }

    /// The standing guard, so this class of bug cannot come back wearing a
    /// different key. Every retired per-game pin is a value the launcher
    /// chose; pruning it hands that key to whatever the shipped template put
    /// in `GFX.ini`, which nobody has had reason to look at. Where the two
    /// disagree the prune is a silent behavior change, and the only thing that
    /// makes it safe is a `repair_once` saying where it should land.
    #[test]
    fn every_prune_that_changes_behavior_declares_where_it_lands() {
        let template = crate::ini::IniDocument::parse(shipped_gfx_ini());

        for pin in retired_game_settings() {
            let section = base_section(pin.section);
            let Some(under) = template.get(section, pin.key).map(str::trim) else { continue };
            if pin.forced_values.contains(&under) {
                continue; // the two layers agree; the prune changes nothing
            }
            let key = find("GFX.ini", section, pin.key).unwrap_or_else(|| {
                panic!("{section}/{} is pruned but never declared in GFX.ini", pin.key)
            });
            assert!(
                key.repair_once.is_some(),
                "pruning {}/{} hands the key to the template's {under}, which is not a value the \
                 pin ever had. Either the template is wrong or the key needs a repair_once.",
                pin.section,
                pin.key,
            );
            assert!(
                key.forced_values.contains(&under),
                "the repair for {section}/{} cannot fire: reconcile_in only replaces a live value \
                 listed in forced_values, and the template's {under} is not in it",
                pin.key,
            );
        }
    }

    /// A one-shot repair, not a standing policy. A player who puts a pin back
    /// themselves keeps it: a prune that ran every launch would be the original
    /// bug wearing a different hat.
    #[test]
    fn a_stale_pin_is_removed_once_and_then_left_alone() {
        let dir = user_dir_with(live_game_settings());
        reconcile_in(dir.path(), &["GN7E69"]).unwrap();

        let path = dir.path().join("GameSettings/GN7E69.ini");
        let mut doc = crate::ini::IniDocument::parse(&std::fs::read_to_string(&path).unwrap());
        let mut back = Overlay::new();
        back.entry("Video_Settings".into()).or_default().insert("MSAA".into(), "1".into());
        doc.merge(&back);
        std::fs::write(&path, doc.render()).unwrap();

        // The launcher does not run the repair again; Session::prepare guards
        // it on the revision marker. Proving the intent here: a second call is
        // what the marker prevents, and reconcile_in itself is not policing.
        let after = std::fs::read_to_string(&path).unwrap();
        assert!(after.contains("MSAA = 1"), "the player's own edit stands until something re-prunes");
    }

    /// The restore names specific keys and gates each on a value a shipped
    /// build is known to have written. Anything else was chosen by somebody.
    #[test]
    fn a_repair_restores_only_values_a_shipped_build_wrote() {
        let dir = user_dir_with(live_game_settings());
        let config = dir.path().join("Config");

        // Live: SyncGPU is the value 3.0.1 forced, MaxAnisotropy is a value no
        // build ever wrote, InternalResolution is one an early build did.
        std::fs::write(
            config.join("Dolphin.ini"),
            "[Core]\nSyncGPU = True\nCPUThread = False\n",
        ).unwrap();
        std::fs::write(
            config.join("GFX.ini"),
            "[Settings]\nInternalResolution = 3\nMaxAnisotropy = 16\nShaderCompilationMode = 2\n",
        ).unwrap();
        // What it looked like before the launcher ever touched it.
        std::fs::write(
            config.join("GFX.ini.launcher-original"),
            "[Settings]\nInternalResolution = 4\nMaxAnisotropy = 8\nShaderCompilationMode = 0\n",
        ).unwrap();

        let done = reconcile_in(dir.path(), &["GN7E69"]).unwrap();

        let gfx = crate::ini::IniDocument::parse(&std::fs::read_to_string(config.join("GFX.ini")).unwrap());
        assert_eq!(gfx.get("Settings", "InternalResolution"), Some("4"), "live value was ours; theirs restored");
        // Was Some("0") until 2026-09-14. Once a key names a repair, a snapshot
        // value that is itself one of OUR forced values is not evidence of a
        // preference - and 0 became one of ours when revision 2 shipped it. The
        // ambiguity is real and it is resolved deliberately in favour of the
        // repair, because 0 blocks the video thread on a shader miss and we do
        // not get to guess that a player chose that.
        assert_eq!(gfx.get("Settings", "ShaderCompilationMode"), Some("2"), "a snapshot value we ourselves shipped falls through to the repair");
        assert_eq!(gfx.get("Settings", "MaxAnisotropy"), Some("16"), "16 was never a value we wrote, so it is theirs");

        // No .launcher-original for Dolphin.ini, so there is nothing to
        // restore. SyncGPU still gets its repair value, because leaving True
        // there costs frames in every install an old build touched.
        let dolphin = crate::ini::IniDocument::parse(&std::fs::read_to_string(config.join("Dolphin.ini")).unwrap());
        assert_eq!(dolphin.get("Core", "SyncGPU"), Some("False"), "repaired, not merely unforced");
        assert_eq!(dolphin.get("Core", "CPUThread"), Some("False"), "never ours");
        assert!(done.repaired.iter().any(|k| k.contains("SyncGPU")));
    }

    /// A value outside the forced set is never touched, even for a key the
    /// launcher used to own.
    #[test]
    fn a_value_no_build_ever_wrote_is_left_alone() {
        let dir = user_dir_with(live_game_settings());
        std::fs::write(
            dir.path().join("Config/Dolphin.ini"),
            "[Core]\nSyncGPU = Maybe\n",
        ).unwrap();
        reconcile_in(dir.path(), &["GN7E69"]).unwrap();
        let doc = crate::ini::IniDocument::parse(
            &std::fs::read_to_string(dir.path().join("Config/Dolphin.ini")).unwrap(),
        );
        assert_eq!(doc.get("Core", "SyncGPU"), Some("Maybe"));
    }

    /// A content update and the Repair button must leave the player's ReShade
    /// tuning and Dolphin template alone, and must still repair the emulator.
    #[test]
    fn a_verify_pass_never_re_downloads_the_players_reshade_config() {
        use crate::library::is_player_editable;
        for editable in [
            "ReShade.ini",
            "NSRShaderlessMenu.ini",
            "reshade-presets/Reborn Launcher.ini",
            "reshade-presets/stages/Field18.ini",
            "User/Config/Dolphin.ini",
            r"User\Config\GFX.ini",
        ] {
            assert!(is_player_editable(editable), "{editable} is the player's to edit");
        }
        for theirs_not in [
            "Dolphin.exe",
            "dxgi.dll",
            "reshade-shaders/Shaders/qUINT_bloom.fx",
            "Sys/GC/font_western.bin",
            "",
        ] {
            assert!(
                !is_player_editable(theirs_not),
                "{theirs_not} must still be repaired: a corrupt emulator is worse than a reset setting",
            );
        }
    }

    /// Seeded keys are the launcher's defaults, so they need a value; locked
    /// keys need one too. The accessors would silently drop a row without it.
    #[test]
    fn every_written_key_has_something_to_write() {
        for key in KEYS.iter().filter(|k| k.owner != Owner::Player) {
            assert!(key.value.is_some(), "{}/{}/{} is written with no value", key.file, key.section, key.key);
        }
    }
}
