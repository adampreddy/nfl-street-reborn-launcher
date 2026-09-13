# Launch handoff, 2026-09-12

Current follow-up: the launcher has moved to **stable**. The stuck Dolphin/session
hotfix is documented in [2026-09-12-session-state-hotfix.md](2026-09-12-session-state-hotfix.md).
The beta and release snapshot below describe earlier work.

Later local launcher polish is documented in [2026-09-12-launch-polish.md](2026-09-12-launch-polish.md). It includes the beta.28 local build with the redesigned Stats page, the staged matching Dolphin runtime and verified default-soundtrack patch, and verification. The published versions below are unchanged by that work.

The state of the NFL Street Reborn Launcher on launch day, written for whoever
picks it up next (a person or an agent). Facts here were read from the
bucket, the repo and this PC at about 15:00 ET on 2026-09-12.

## 1. Where the code is

- **The launcher is the `integration` branch in the worktree
  `C:\Users\Adam\Desktop\Reborn Installer\NFL Street Reborn Launcher\target\merge-tree`**,
  HEAD `81e6fd7`. Every beta since beta.8 was built from here. Build and test
  from this folder.
- The main checkout (`...\NFL Street Reborn Launcher`, branch `main`, `56830b7`)
  is behind: it lacks the card gear sync hooks, the music editor, the
  Publisher redesign, the paths overhaul and every fix of 09-11 and 09-12.
  **Stable 3.0.0 must be built from `integration`, not from `main`.** Land
  `integration` on `main` when Adam says.
- Other worktrees (all merged into `integration` already): `target\music-tree`
  (`music`), `target\stats-tree` (`stats-identity`), `target\publisher-tree`
  (`publisher-redesign`), `target\paths-tree` (`paths-overhaul`; its `target`,
  `Tools\node_modules` and `dist` are junctions into `merge-tree`).
- Another agent (Codex, "GPTAstra") edits the main tree concurrently. Never
  `git add -A`; add files by name. Untracked leftovers in `merge-tree`:
  `MERGE-REPORT.md`, `Tools\target\`.

## 2. What is live

| Thing | Version | Notes |
|---|---|---|
| Beta launcher (`launcher/latest-beta.json`) | **3.0.1-beta.26** (13:14) | From `integration` 81e6fd7. Beta.24 full installer set (zip 2.99 GB, Setup + 2 bins) is on the bucket for fresh installs. |
| Stable launcher (`launcher/latest.json`) | 3.0.0 | The 09-03 build; **not** the release. The real stable 3.0.0 is still to be built and published from the Publisher's Launcher page. |
| Content release | 3.0.1 **seq 14** (13:18) | game-patch 2026.09.11 (NSRV3 98bfe6bf...), mods 1.1.0, roster 2026.09.12, runtime 1.1.3, textures 1.2.1. Published by CLI, so it is not in the Publisher's History. |
| mods 1.1.0 | 14 packages | 12 JSGME packs (xbox-controls at 1.2.0) plus `reborn.soundtrack.reborn` (Adam's 21 songs, no pauses) and `reborn.soundtrack.original`. Players turn a soundtrack on from Mods > Soundtrack; it is not on by default. |
| Publisher | build of 09-11 13:47 | Installed at `%APPDATA%\NFL Street Reborn Launcher\publisher\app`. Drop folders `C:\Reborn Publisher\{textures\GN7E69, runtime, mods\packages}`. |
| Site | join page live | `https://nflstreetreborn.com/join.php?c=<code>` hands an invite to the launcher; the rest of the site is 503-gated until launch. |

## 3. The layout (paths overhaul, merged 09-12)

Spec `docs/superpowers/specs/2026-09-12-install-layout-and-path-law-design.md`,
plan `docs/superpowers/plans/2026-09-12-install-layout-and-path-law.md`.

- Data root: stable `%APPDATA%\NFL Street Reborn`, beta `%APPDATA%\NFL Street
  Reborn Beta`; the program lives in `<root>\app`. The `... Launcher[ Beta]`
  roots of beta.8 to beta.23 are adopted (files moved, absolute paths inside
  them rebased) at the window's first start, never by `rebornctl`.
- A 2.1 install found at the stable root is tucked into `<root>\Reborn 2.1\`
  at first start and never deleted. **On Adam's PC a stable install would
  tuck his dev tree** (`Games`, `Tools`, the live `User` pack): rename that
  root first.
- Textures are composed straight into `<user folder>\Load\Textures\GN7E69`
  (a real folder with `composed.json`); the tree is built beside as
  `.building` and swapped in whole, so a failed build never leaves the game
  without textures. No junction, no `mods\active` any more.
- Path law: at every launch, Dolphin.ini path keys that are relative or point
  inside any Dolphin user folder are removed; a cardless folder gets
  `SlotA = 1` (a raw card Dolphin creates on first boot).
- The user-folder template ships from the tuned reference `User` folder:
  `build.ps1 -UserTemplate <User>` is required for full and runtime builds.

## 4. What changed on 09-11 and 09-12 (newest first)

- `81e6fd7` adoption rebases the paths written under the old root (beta.26).
- `6ac2466` installer redirects a retired self-update folder (beta.25).
- `faf4908` paths overhaul: one data root per channel, tuck, adopt, compose
  into the user folder, path law, template.
- `4304e0d` composed texture tree built beside and swapped whole (beta.24).
- `2fa4a78` a launch gives a cardless user folder a memory card (beta.23).
  The runtime template's Dolphin.ini shipped `SlotA = 255`, so fresh installs
  never saved and the card gear sync had no card.
- `62fa237` the launcher checks for its own update every five minutes and on
  window focus (beta.22).
- `3b5fdec` `e9c2ce3` `2c1f9cc` `b87e4f4` music: segments padded with zeros
  after SCEl (the game plays declared samples, so silent-block padding was a
  pause every segment), tight stride, "too big for the disc" refusal, plain
  punctuation on the disc, bulk add, mouse drag reorder, renamed originals
  kept (beta.18 to beta.21).
- `0bcf57c` the B/X notice on the first-run controller step and Controllers
  only (beta.17).
- `4ae689c` `d387de2` `657faef` the code review batch: single instance and
  invite hand-off on focus, guarded content recheck, two-press uninstall,
  player-facing netplay refusals, 401 only in the server's own words, runtime
  template never taken as the player's folder, MSAA no longer forced, B/X
  normalize once, and the Publisher fixes (beta.16). Ledger:
  `docs/superpowers/plans/2026-09-11-code-review-triage.md`.
- Publisher installer with the review fixes: `dist\Reborn Publisher Setup 3.0.0.exe`.

## 5. Laws and traps

- A build that ships a new runtime or texture tree MUST be followed by a
  content release pinning those trees, or the updater drags installs back.
- Publish the runtime only from `build.ps1 -RuntimeOnly -UserTemplate` output;
  the full payload's runtime is dirty after any walk (ReShade logs). The later
  polish fix gives runtime-only staging its own folder and preserves
  `target\installer-payload`.
- Never run two cargo builds at once (memory). A running `rebornctl.exe`
  (pack rebuilds) makes the launcher build fail with "Access is denied";
  finish CLI jobs first. Long builds go through `schtasks /create /sc once
  /st 23:59 /sd 10/10/2026 /f` plus `/run` with output to log files.
- `Start-Process -Wait` in the publish wrapper hung once after the build had
  finished (00:29 to 00:44); killing the wrapper and rerunning was clean.
  Defender holding the fresh Setup exe made ISCC need a retry twice.
- `Tools\e2e-upload.py` and `e2e-publisher.py` store FAKE R2 credentials in
  the real vault; since 09-11 they restore the real ones from
  `C:\Users\Adam\Reborn Keys\rclone.conf` in `finally`. Check `rebornctl
  publish status` after a run anyway.
- A stats test with its queue directly in a temp folder writes `%TEMP%\games`
  and makes nine supervisor tests fail (fixed; if it recurs, delete that
  folder).
- The launcher's log file only records warnings and errors; an empty `logs`
  folder means nothing went wrong, not that logging is broken.
- The disc image has one size: 21 long songs can outgrow it. The Music page
  refuses with the overflow in MB before writing.
- HTML5 drag and drop never fires in the Tauri window on Windows; the Music
  list uses pointer events.
- In Windows PowerShell 5.1 never `2>&1` a native command; write multi-line
  scripts with the Write tool (heredocs and sed mangle backslashes).
- Never run the stable Setup on Adam's PC. No Windows Sandbox. No em dashes in
  anything player-facing. US English.

## 6. How to build and publish

- **Beta self-update:** `Tools\publish-beta-launcher.ps1 -Version 3.0.1-beta.N`
  builds the launcher-only Update exe, verifies, uploads the exe then the
  manifest, and reads both back. Wrap it in a scheduled task (examples:
  `C:\Users\Adam\.claude\jobs\f4d73a58\tmp\publish_beta24_launcher.ps1`).
- **Full beta set:** `build.ps1 -Channel beta -Version ... -Runtime ...
  -UserTemplate ... -Textures ... -GamePatch ... -ModPackages ...
  -PromptPackages ...` then rclone (see `upload_beta24.ps1` in the jobs
  folder). Verify the bucket copies by size and sha.
- **Stable 3.0.0:** the same full build from `integration` with `-Channel
  stable -Version 3.0.0`, one walk of the installer on a clean PC, publish from
  the Publisher's Launcher page (it writes `latest.json`), then point the site
  download link. Early 3.0.0 installs will not self-update until a 3.0.1
  stable is published, so build it right the first time.
- **Content:** the Publisher's Textures, runtime, mods page publishes from the
  drop folders; roster and game patch have their own pages. CLI publishes
  work but are not journalled.
- **Verification before any build:** `cargo test -p reborn-core --lib`,
  `cargo test -p reborn-ui`, `cargo test -p reborn-publisher`, then the page
  checkers in `Tools\` (`check-shell-pages.mjs`, `review-onboarding.mjs`,
  `review-players.mjs`, `check-online-restored.mjs`, `check-music-page.mjs`,
  `check-update-notice.mjs`, `check-publisher-pages.mjs`) and the full
  `run-browser-tests.mjs` (which chains the players and finish reviews).
  `Tools\test-all.ps1` runs the lot. Real-window proof uses WebView2 CDP
  (`WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9333` plus
  its own `WEBVIEW2_USER_DATA_FOLDER`) with the `drive-*.mjs` scripts.

## 7. Open items, in order

1. Stable 3.0.0 from `integration` with `-UserTemplate`, a walk, the
   Publisher's Launcher page, the site link. Consider a content release
   labelled 3.0.0.
2. Watch a beta.24 to beta.26 self-update on a real PC: it proves the
   installer redirect plus the path rebase together. Not yet observed.
3. The PC where "all the textures went away" after the roster update: the
   specific cause was never seen (the launcher keeps no info-level log). Two
   structural fixes shipped since (swap-in compose in beta.24, composition
   into the user folder in beta.26). If it recurs, get that PC's `versions`
   folder and the file count of `<user folder>\Load\Textures\GN7E69`.
4. Content gap seen in the paths walk: the `Menu Slideshow` override folder
   is in no texture pack, so the main menu shows the disc's "MISSING TEXTURES"
   slide. Regenerate from `projects\menu_re\build\slideshow\hd_overrides\`
   and publish textures 1.2.2.
5. The baked-soundtrack disc (sha 37eeffd1..., the other session's work) is
   not published as a game patch, and the dev FIELDS ISO moved on again at
   14:36 (sha fb69842a...), so the launcher discs on this PC (37eeffd1) are
   already behind the dev tree. Decide whether the disc's own soundtrack
   should be the Reborn one; if so publish the patch NSRV3 to the current
   FIELDS disc and re-pin the game-patch release. The null function-pointer crash fix is likewise only in `root/`
   and the FIELDS disc, so NSRV3 players still have the crash until a game
   patch ships.
6. At handoff time, Reborn soundtrack on by default was not built. The later
   polish work staged and verified a soundtrack-only patch; see the linked
   polish handoff. Public release is still pending.
7. Adam's PC: the beta roots renamed aside at 14:32
   (`*.before-fresh-20260912-143218`, 19 GB) hold his card, textures and
   imported disc; the stale `%APPDATA%\NFL Street Reborn Launcher` root would
   be adopted by a stable install. His fixed pack is
   `C:\Users\Adam\Downloads\NFLStreetReborn-fixed.nsrmusic`.
8. Info-level logging of content updates would have answered item 3 in a
   minute; worth adding after launch.

## 8. Memory

The agent memory index at `C:\Users\Adam\.claude\projects\C--Users-Adam\memory\MEMORY.md`
carries one line per topic; the launcher topics are
`project_reborn_update_system.md`, `project_reborn_install_layout_path_law.md`,
`project_reborn_publisher.md`, `project_reborn_music_editor_design.md`,
`project_reborn_card_gear_sync_design.md` and `project_reborn_launcher_status.md`.
