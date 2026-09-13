# Stable 3.0.3: retained game/save folders must not block launch

## Report

Adam reported: "Your game could not start" followed by the older-install-files
refusal from `supervisor::OLDER_INSTALL_IN_THE_WAY`. The affected PC's log and exact
folder conflict are not available. This error is separate from process inspection.

## Proven defect and correction

`LegacyInstall::detect` intentionally identifies any folder with `User` OR `Games`
as a possible migration source. Supervisor incorrectly reused that broad rule as a
launch prohibition, including in an established launcher installation. Startup also
tried to tuck these folders on every run. An existing `Reborn 2.1/User` or `Games`
archive makes those repeat moves collide indefinitely; closing the game cannot fix
that. Moving a configured active User directory would also invalidate stored paths.

`layout::legacy_blocks_launcher` now distinguishes an established launcher (an
existing settings file that successfully loads) from a raw old installation.
Configured roots can retain top-level User/Games and an older archived copy.
Startup leaves those folders and the saved paths alone. It separates only leftover
JSGME entries in the case-insensitive MODS/mods namespace.

Actual legacy mod entries in the launcher's working namespace still block writes
until separated. A failed move or an existing archived mod backup never causes
either copy to be overwritten or deleted. The refusal no longer asserts that the
older game is running; it describes the folder overlap and offers retry/separate
installation. Raw old installations without valid launcher settings remain protected.

The known launcher-owned MODS entries now also include `active`, used by earlier
launcher builds. The full test suite caught this omission: composition recovery and
foreign-directory protection must remain reachable through the Supervisor lock.

`LegacyInstall::detect` itself is unchanged, so migration discovery still finds old
game and save directories for onboarding.

## Verification

New tests cover a configured root with active User/Games plus matching archived
folders, repeated preparation with unchanged settings bytes and paths, actual
JSGME separation while launcher packages and saves remain in place, refusal on an
unresolved mod backup collision with both copies preserved, and real Supervisor
lock acquisition with retained game/save folders.

Full workspace test log: `target/legacy-startup-workspace-tests.log`.
Targeted layout test log: `target/legacy-startup-tests.log`.

Final full run: **996 passed, 0 failed, 4 existing ignored**. `git diff --check` passed.
The initial full run caught three tests blocked by the missing `mods/active`
classification. The ownership list was corrected and the complete suite rerun successfully.

## Delivery

Built stable **3.0.3**, launcher-only. It includes the 3.0.2 process-detection
correction and Claude's committed texture-prefetch correction. No installer has
been published or installed by this task. An affected-PC check remains necessary;
an actual unresolved mod collision will correctly still need migration attention.

Verified installer: `dist/NFL Street Reborn Launcher Update 3.0.3.exe`.
Size: **64,670,376 bytes**.
SHA-256: `41eaf05267d02b809c521bf35234bfa29462f719407deed819c863b9d4131f80`.
`dist/latest.json` matches stable 3.0.3, size and hash, with release notes.
Staged launcher matches the release executable; no beta channel marker is present.
Build log: `target/legacy-startup-stable-build.log`. Packaging succeeded after
two temporary Windows file-lock/resource-update failures and automatic retries.
