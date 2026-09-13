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
pub const REVISION: u32 = 1;

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
    /// Write this value once, where the restore had no evidence to work from.
    /// For a key an old build welded in at a real cost, dropping the force is
    /// not enough: the cost stays until something puts the right value back.
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
    with_ui(player("GFX.ini", "Settings", "MSAA", &["1"],
        "Anti-aliasing. 4x is the Reborn baseline (Adam, 2026-09-10), lower if the game stutters."),
        Ui { id: "msaa", kind: "msaa", default: "0x00000004" }),
    player("GFX.ini", "Settings", "SSAA", &["False"], "Supersampling."),
    player("GFX.ini", "Settings", "MaxAnisotropy", &["4"],
        "Anisotropic filtering. Note the launcher was writing this into [Settings], where Dolphin does not read it; Dolphin's own key is [Enhancements] MaxAnisotropy."),
    player("GFX.ini", "Enhancements", "MaxAnisotropy", &[],
        "Where Dolphin actually keeps anisotropic filtering. Never written by any shipped build, so there is no forced value to recognise."),
    player("GFX.ini", "Settings", "ShaderCompilationMode", &["2"], "Shader compilation strategy: a stutter-versus-wait trade."),
    player("GFX.ini", "Settings", "WaitForShadersBeforeStarting", &["True"], "Whether to wait at boot or stutter in play."),
    player("GFX.ini", "Settings", "DumpTextures", &["False"], "A modder's switch."),
    player("GFX.ini", "Settings", "DumpBaseTextures", &["False"], "A modder's switch."),
    player("GFX.ini", "Settings", "DumpMipTextures", &["False"], "A modder's switch."),
    player("GFX.ini", "Enhancements", "ArbitraryMipmapDetection", &["False"], "A per-upload CPU cost, not a correctness requirement."),
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
    seeded("Dolphin.ini", "Core", "GFXBackend", "D3D", &["D3D"],
        "ReShade's hook is dxgi.dll, so D3D is what the effects need. If the player picks another backend the choice is honored and the consequence is stated, not overridden."),
    seeded("GFX.ini", "Hacks", "EFBAccessEnable", "True", &["True"],
        "The per-game layer locks this; seeding the global layer keeps a fresh install consistent without taking the key twice."),
    seeded("Logger.ini", "Options", "WriteToFile", "False", &["False"], "The freeze instrumentation left file logging on: a disk write per line."),
    seeded("Logger.ini", "Options", "Verbosity", "1", &["1"], "As WriteToFile."),
    seeded("Logger.ini", "Options", "WriteToConsole", "False", &["False"], "As WriteToFile."),
    seeded("Logger.ini", "Options", "WriteToWindow", "False", &["False"], "As WriteToFile."),
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

/// The player-owned keys the launcher's own Settings page offers.
pub fn player_fields() -> impl Iterator<Item = &'static Key> {
    KEYS.iter().filter(|k| k.owner == Owner::Player && k.ui.is_some())
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
    /// Config keys put back to the player's own value, as `<file>/<section>/<key>`.
    pub restored: Vec<String>,
    /// Config keys given their repair value because no evidence survived.
    pub repaired: Vec<String>,
    /// Files actually rewritten.
    pub files: Vec<std::path::PathBuf>,
}

impl Reconciled {
    pub fn is_empty(&self) -> bool {
        self.pruned.is_empty() && self.restored.is_empty() && self.repaired.is_empty()
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
///    [`Key::forced_values`] was chosen by somebody and is left alone.
/// 3. **Repair.** Where the restore found no evidence, write
///    [`Key::repair_once`] if the key has one. Un-forcing a key does not undo
///    what is already in the file, and `SyncGPU = True` costs frames in every
///    install that a build through 3.0.1 ever touched.
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

    /// The locked set is the whole compromise. It is written out by name here
    /// so that adding to it means editing this test, which means saying out
    /// loud that the launcher is taking another setting away from the player.
    #[test]
    fn the_locked_set_is_exactly_these_eleven_keys_and_each_states_why() {
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
            ("Dolphin.ini", "Analytics", "Enabled"),
            ("Dolphin.ini", "Analytics", "PermissionAsked"),
            // Added 2026-09-13: exclusive fullscreen hard-hangs Windows when an
            // outside capture overlay (Print Screen, Steam's F12) grabs the
            // screen. Locked, not seeded, because every affected player already
            // has a GFX.ini and a seeded key would never reach them.
            ("GFX.ini", "Settings", "BorderlessFullscreen"),
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

    /// A key cannot be both the launcher's and the player's. The old code had
    /// exactly this contradiction: MSAA was offered as a preference in GFX.ini
    /// and pinned to 1 in the per-game layer that outranks it.
    #[test]
    fn no_key_is_locked_in_one_place_and_offered_in_another() {
        for field in player_fields() {
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

    /// The forced-value sets came from `git log -S` over runtime.rs, and the
    /// values genuinely moved between shipped builds. A single-value check
    /// would skip the installs stamped by the older ones.
    #[test]
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
        assert_eq!(gfx.get("Settings", "ShaderCompilationMode"), Some("0"), "live value was ours; theirs restored");
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
