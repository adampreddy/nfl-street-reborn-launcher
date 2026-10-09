# Plan 3b, Task 10: manual offline check with a stand-in league disc

Date: 2026-10-09. Branch `feat/league-tab` at `929cd9a`. Nothing was published, uploaded or installed.

## How it was run (headless, sandboxed)

- **No GUI.** A throwaway `#[ignore]` test module (`zz_task10`, not committed) was compiled into the reborn-ui test binary. The one-line `mod` hook in `main.rs` was reverted right after the build.
- **Same path as `league_play_offline`.** The harness called the steps `launch(edition = "league")` takes:
  - `league_edition::ready_league_image(&Platform::detect())`
  - `session::standing_for_build(GameBuild::League, ..)`
  - `selected_user_dir(&p, None)`
  - `LaunchRequest { build: GameBuild::League, .. }`
  - `Session::run`
- **Steps skipped:** `content_updates::ensure_runtime` and `walk_image_to_head`. Both are about network or the retail image, and the runtime was passed explicitly.
- **Deviation:** `fullscreen: false`, so the desktop was not taken over.
- **Sandbox data root:** `APPDATA` and `LOCALAPPDATA` pointed at `...\scratchpad\league-sandbox\{Roaming,Local}`, so the data root was `league-sandbox\Roaming\NFL Street Reborn`. The harness asserted the data root path before doing anything.
- **Real data root untouched:** after the run, no file under the real `%APPDATA%\NFL Street Reborn` had a write time later than the start of the run.
- **Runtime:** a copy of the installed `app\runtime` (109 MB) in the sandbox. The real install was only read.
- **Membership cache:** `config\league.json` was written by `reborn_core::league_home::save_cache`:
  `{"member":true,"home":{"member":true,"team":{"id":0,"name":"Sandbox"}},"fetched_at_ms":1}`. The backend League switch is off, so a real sign-in cannot produce `member: true`.
- **Stand-in disc:** a COPY of `app\content\games\NSR.iso` (1,459,978,240 B) at `library\league\game\league.iso`.
  - Its first 6 bytes were patched `GN7E69` -> `NSRS04`.
  - Magic at 0x1C is still `C2339F3D`. Region at 0x458 is still `00000001`.
  - The image is a plain ISO, not NKit, so no DolphinTool conversion was needed.
- **League files:**
  - `library\league\league.json` = `{"schema":"nsr.league-game.v1","game_id":"NSRS04","iso":"game/league.iso","textures":"textures/NSRS04"}`
  - `library\league\textures\NSRS04` = 20 files copied from the real `library\textures\GN7E69`.
- **Retail mod:** `jsgme.aqua-football@1.0.0` (one GN7E69 payload, `GN7E69/tex1_128x128_m_ffc5151a141db330_14.dds`) was copied from the real mods store and turned on in the sandbox `mods\activation.json`.

## Results

| # | Check | Result |
|---|---|---|
| 1 | Stand-in ISO SHA-256 before | `45E4B02A9981191AADDFBEE1331963686C6C89E9E9AE1DF64A1B8E63D71ED278` |
| 2 | Dolphin boots the league ISO as NSRS04 | PASS |
| 3 | `Load\Textures\NSRS04\composed.json` lists the retail mod | PASS |
| 4 | `GameSettings\NSRS04.ini` settings + Dolphin.ini locks | PASS |
| 5 | ISO SHA-256 after the session | PASS (identical) |
| 6a | `game_id` ZZZZ99 -> WRONG_BUILD | PASS |
| 6b | membership cache deleted -> NOT_A_MEMBER | PASS |

### 2. Boot

- **Standing:** `standing_for_build` answered `Reborn`, so the league tree composes and is never withheld.
- **User dir:** `selected_user_dir` answered `None`, so the session resolved a fresh user dir at `<data root>\runtime\user`. That is `<sandbox user>` below.
- **Dolphin command line (Win32_Process, PID 24928):**
  ```
  "...\league-sandbox\runtime\NFL Street Reborn Dolphin.exe" -u "...\league-sandbox\Roaming\NFL Street Reborn\runtime\user"
    -e "...\league-sandbox\Roaming\NFL Street Reborn\library/league\game\league.iso" -b -C Dolphin.General.UseDiscordPresence=False
  ```
- **Render window title about 45 s after start:**
  `NFL Street Reborn Dolphin 1.1.0 | JIT64 DC | Direct3D 11 | HLE | FPS: 60 - VPS: 60 - 100% | NSRS04`
  The game was running at full speed under ID NSRS04.
- **Dolphin's own files:** Dolphin created `Cache\NSRS04.uidcache` and the `D3D-specialized-{ps,vs}-NSRS04-461FF50.cache` files at launch time.
- **Stop:** only PID 24928 was killed, after about 50 s. `Session::run` returned `Crashed { detail: "exception 0xFFFFFFFF" }` and minutes=0.83. That is the forced kill, not a game fault.
- **Mods note:** `1 mod(s) on, tree rebuilt, 21 entries, texture folder in the user folder.`

### 3. Composed tree (`<sandbox user>\Load\Textures\NSRS04\composed.json`)

```json
{ "schema": "nsr.composed.v1", "game": "NSRS04",
  "record": { "game": "NSRS04",
    "pristine_root": "...\\league-sandbox\\Roaming\\NFL Street Reborn\\library/league\\textures\\NSRS04",
    "activation": [ { "id": "jsgme.aqua-football", "version": "1.0.0" } ],
    "total_entries": 21, "pristine_entries": 20,
    "overlays": [ { "relative": "tex1_128x128_m_ffc5151a141db330_14.dds", "id": "jsgme.aqua-football",
                    "sha256": "70a31f784825de3bd65049193a711e398fa849f3fa13451491152f9f54884d93" } ] } }
```

- The retail mod's GN7E69 payload was composed into NSRS04 through the payload fallback.
- The folder holds the 20 league textures plus that ball (1,398,276 B).

### 4. Settings

`<sandbox user>\GameSettings\NSRS04.ini`:

```ini
[Core]
MEM1Size = 67108864
RAMOverrideEnable = True

[Video_Hacks]
EFBAccessEnable = True

[Video_Settings]
CacheHiresTextures = False
HiresTextures = True
```

- **Dolphin.ini locks:** `<sandbox user>\Config\Dolphin.ini` has `MEM1Size = 67108864`, `RAMOverrideEnable = True` and `EnableCheats = True`.
- **GFX.ini:** `<sandbox user>\Config\GFX.ini` has `CacheHiresTextures = True` globally. The per-game `NSRS04.ini` overrides it to `False`.
- **The Sys layer does not apply:** the runtime's `Sys\GameSettings\GN7.ini` does not match NSRS04. It holds exactly the same keys that `NSRS04.ini` writes (RAM override, MEM1, HiresTextures, CacheHiresTextures=False, EFBAccessEnable), so nothing is lost.

### 5. Image untouched

After the session the hash was `45E4B02A9981191AADDFBEE1331963686C6C89E9E9AE1DF64A1B8E63D71ED278`, identical to (1). The image's LastWriteTime stayed at the time of the patch, before the boot.

### 6. Refusals (`ready_league_image`, which every league boot passes)

- **`league.json` `game_id` -> `ZZZZ99`:**
  `These league files are for a different league build. Update the launcher, then update the league files.` (`WRONG_BUILD`, kind `not_installed`)
  - No Dolphin was started.
  - The file was restored afterwards.
- **`config\league.json` deleted:**
  `League files are for league members. Sign in with the Discord account your team is linked to.` (`NOT_A_MEMBER`, kind `not_member`)
  - No Dolphin was started.
  - The same sentence also came back before the cache was first written.

## Not covered here

- **The Tauri command wrapper:** `league_play_offline` -> `launch(..)` with `play_anyway` and `launch_prefs`.
- **The Mods page's league note.** It needs the GUI.
- **`ensure_runtime`.**
- **Fullscreen.**
- **Controller gate:** no pad gate refusal came up. `player_one_blocked` was `None` with no pad set up in the fresh user dir.

The sandbox was deleted after the run.
