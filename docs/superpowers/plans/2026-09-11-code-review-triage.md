# Code review triage, 2026-09-11

Adam asked for a review of the whole codebase ("find errors and such and
things to improve"). Three reviewers read the `integration` branch, one each
for the core crate, the launcher pages plus installer, and the Publisher
plus publish pipeline. Every finding below was checked against the code (and
where the finding was about layout, rendered in Chromium) before anything was
changed. Verdicts: FIXED, DISPROVED (the code or the render says otherwise),
DEFERRED (real, not for the 3.0.0 launch).

## Core crate

| # | Finding | Verdict |
|---|---------|---------|
| 1 | `session.rs` took the runtime's own `User` template as the player's folder when no folder was chosen (`user_dir_score > 0`; the template's inis score). A launch with nothing chosen booted inside the runtime package. | FIXED: a folder beside the emulator counts only with a memory card (score 100) or `portable.txt`. |
| 2 | `runtime.rs` merged `MSAA=1` into GameSettings on every launch, overriding the 4x anti-aliasing preference in GFX.ini. | FIXED: the tuple is gone. |
| 3 | `online_image.rs` refusals carried folder paths, and `netplay_open` showed them verbatim. | FIXED: player-facing sentences on the Online page; the paths go to the log. The Builder-missing case still blocks netplay on purpose, because a disc without the roster desyncs against every other player. |
| 4 | `backend.rs` treated any HTTP 401 as "token revoked" and cleared the token, even from a proxy. | FIXED: only a 401 in the server's own format (`ok: false`) retires the token; anything else defers the send. Tests updated. |
| 5 | `dolphin_preferences.rs` rebased any absolute path with a `User` component, including `C:\Users\User\...` and a memory card kept in an unusual folder. | FIXED: only when the component before `User` is not `Users` and the folder after it is one of Dolphin's own. Test added. |
| 6 | `controller_profiles::normalize_saved` ran at every launch and would undo a deliberate B/X swap. | FIXED: runs once per data root (`config/bx-swap-normalized` marker). |
| 7 | `content/pipeline.rs` failed the whole promotion when the soundtrack could not be put back, after the component was already live. | FIXED: logged, and reported as a soundtrack refusal on the page. |
| 8 | `stats.rs` wrote archive files non-atomically and let an archive failure stop the folder scan. | FIXED: tmp + rename; a failed archive is logged and the capture still queued. |
| 9 | `setup_tools.rs` failed an import that had already landed when the patch step hard-errored. | FIXED: the wizard reports "unknown" and the launcher tries again later. |
| 10 | `soundtrack_switch` saved the new soundtrack record before the rebuild; a failed rebuild left Music claiming a soundtrack the disc did not carry. | FIXED: the previous record is put back on failure. |
| 11 | "Open Dolphin" wrote the session record before the spawn; a failed spawn left it to be recovered as a crashed game. | FIXED: the record is finished on spawn failure. |
| 12 | `update::dir_switch` compared paths case-sensitively. | FIXED: lowercased compare, separator-normalized. |
| 13 | `open_website` duplicates `platform::imp::open_url`. | DEFERRED: two call sites, both work; not worth a change before launch. |
| 14 | The stats user agent used the crate version (3.1.0), not the build version. | FIXED: `reborn_core::build_version()` is the one source; the launcher delegates to it. |
| 15 | "tidied away" wording in `stats.rs`. | FIXED. |
| 16 | "creation_flags applied twice" in `imp.rs`. | DISPROVED: two separate call sites, one flag each. |

## Launcher pages and installer

| # | Finding | Verdict |
|---|---------|---------|
| 1 | Uninstall removed the launcher on one press. | FIXED: two presses; the first arms the button for 8 seconds and says so. |
| 2 | The 5-minute content recheck reloaded Play during a game or a sign-in. | FIXED: no check and no reload while a game runs, a dialog is open or the page is busy. |
| 3 | Both channels registered the invite scheme with `uninsdeletekey`; uninstalling a beta broke the stable launcher's links. | FIXED: only the stable uninstall removes the key. |
| 4 | The old-Programs cleanup would DelTree `%LOCALAPPDATA%\Programs` itself if the launcher had been installed there. | FIXED: only a folder below Programs that holds `reborn-launcher.exe`. |
| 5 | An invite link opened before setup went to Online with no game, and the code was lost. | FIXED: the hand-off waits for setup to be complete; the code stays put until then. |
| 6 | No single instance: an invite clicked while the launcher was open started a second launcher. | FIXED: `tauri-plugin-single-instance`; the second start hands the code over, the open window comes forward, and Play and Online read the code on focus. |
| 7 | Two quick presses on Sign in started two device-code flows. | FIXED: a `starting` guard. |
| 8 | The opened "Customize controls" fold clips the Swap and Restore buttons at 1280x800. | DISPROVED by rendering: the buttons end at 651 px inside a stage that ends at 658 px. The onboarding checker now asserts this. |
| 9 | The locked Display toggle stayed in the pad's focus order. | FIXED: `aria-disabled` elements are skipped and the locked link is out of the tab order; dead `button.play-overview-toggle` CSS removed. |
| 10 | The netplay poll re-enabled the form while Dolphin was still shutting down. | DISPROVED: `netplay_status` reports `closed` only after the emulator process has exited (`session.active` is cleared right after the process wait in `dolphin_settings.rs`), so the immediate re-enable is right, and `check-online-restored.mjs` asserts it. Left as it was. |
| 11 | `players.css` narrow-window padding could never apply. | FIXED: the media query is last. |
| 12 | `finish.css` used an undefined `--text`. | FIXED: `--fg`. |
| 13 | `build.ps1 -RuntimeOnly` still required the Builder. | FIXED. |
| 14 | `InstallDirIsSafe` accepted the other channel's program folder. | FIXED: only this channel's `{userappdata}\<DataDir>\app`. |
| 15 | An em dash in Controllers; the soundtrack "rebuilding" notice was cleared before it showed. | FIXED. |

## Publisher and publish pipeline

| # | Finding | Verdict |
|---|---------|---------|
| 1 | `e2e-upload.py` and `e2e-publisher.py` leave fake R2 credentials in the real vault. | FIXED: both restore the real ones from `rclone.conf` in `finally`, with a loud warning if they cannot. |
| 2 | Re-opening a page during its own publish left it "publishing" forever. | FIXED. |
| 3 | The remembered textures folder was keyed by component, not game id; the journal did not record the game id. | FIXED: `textures:<game id>` keys; `game_id` in the journal inputs; the game id change refreshes the folder. |
| 4 | `finish` flipped the phase to Done before appending the journal; Verify could read the previous run. | FIXED: journal first, then the flip. |
| 5 | A panic in a step left the operation busy until restart. | FIXED: `catch_unwind`, settled as a failure with the panic text. |
| 6 | Four sites discarded the "how the bucket was listed" note. | FIXED: all four say it. |
| 7 | `list_keys` never retried. | FIXED: three attempts with back-off on 429, 5xx and transport errors. |
| 8 | Per-part progress lines were prefixed `ERROR:`. | FIXED. |
| 9 | The Drop folders hint promised roster and game subfolders nothing reads. | FIXED: the hint and the settings doc name the three that are wired. |
| 10 | A proposed drop folder was stored as if chosen. | FIXED. |
| 11 | The Publisher installer left the old Programs install as an orphan. | FIXED: its own uninstaller runs first, the way the launcher's does. |
| 12 | Two jobs on the Game page shared the label "Build an ISO". | FIXED: "Game patch". |
| 13 | "cancelled" in a shown sentence. | FIXED: "canceled". |
| 14 | The 400 ms status poll carries the whole log. | FIXED: 1 s once the log passes 2000 lines. |

## Proof

- `cargo check --tests` for reborn-core, reborn-ui, reborn-publisher; `cargo test` for the touched core modules, reborn-ui and reborn-publisher.
- `Tools/check-shell-pages.mjs`, `Tools/check-publisher-pages.mjs`: PASS.
- `Tools/review-onboarding.mjs` (Chromium, 1280x800 folded and opened, 800x600) and `Tools/review-players.mjs`: PASS.
- Beta.16 built and published for the invite and single-instance changes to be tried between two PCs.
