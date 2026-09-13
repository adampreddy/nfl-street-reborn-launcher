# Verified game image law

Date: 2026-09-13. Branch to be cut off `integration`.
Approved by Adam in chat, 2026-09-13: launch-time gate plus all three doors; full-disc download as the fallback; three tiers by impersonation risk; Tier 2 framed as a fault to repair, never as a "custom" preference.

## The bug this comes from

A player on Discord, 2026-09-13:

> "when I install Reborn 3.0 ... when I do get in there and Quick Play, it's all scrambled roster names and pictures that don't match? the rosters are like old names with new faces"

They were playing their own unconverted NFL Street 2 with Reborn's texture pack painted over it. The launcher told them the import succeeded.

## What exists today, measured on this PC

All line numbers are `integration` at `target/merge-tree`, read 2026-09-13.

- `setup_import_game` (`reborn-ui/src/setup_tools.rs:166`) gates on game ID alone via `check_reborn_id` (`:377`), which only asserts `header["game_id"] == "GN7E69"`. Any modified US disc passes.
- The real verification is `game_import::import_with_progress`, which extracts the disc with DolphinTool and checks every `manifest.source_files` entry by size and SHA-256, failing with `"<path> does not match the supported USA release"` (`reborn-core/src/game_import.rs:83-84`). It also asserts `sys/boot.bin` starts `GN7E69` (`:74-75`).
- That entire error is discarded at the call site by `Err(_) =>` (`setup_tools.rs:169`), which falls back to `import_existing`, storing the player's disc verbatim.
- `game_patch::apply` routes on `hash_file(&pristine)`, the whole-file SHA-256 (`reborn-core/src/game_patch.rs:427`). A disc off the chain returns `Refusal::UnknownBase` (`:333`, `:436`).
- `patch_to_newest` maps that to `PatchOutcome::Unknown`, name `"unknown"` (`setup_tools.rs:121`).
- The wizard renders `unknown` as success: `case 'unknown': return ['Game File Imported!', 'This disc is not one Reborn recognizes, so it was kept exactly as it is. ... Continue to your controller.']` (`reborn-ui/ui/onboarding.js:403`), with a full green progress bar and an enabled Continue. `no-chain` has no case at all and falls through `default:` to `'Imported and ready.'`. That string is the only place in the codebase that mentions an unrecognized disc; nothing warns again afterwards.
- Three commands write `settings.game_path`, none verifying it:
  - `setup_import_game` (`setup_tools.rs:193`)
  - `settings_set_path("game", ...)` (`reborn-ui/src/main.rs:3979`), which does no validation of any kind and accepts any path
  - `setup_migrate_install` (`setup_tools.rs:498`), which inherits whatever ISO a 2.1 install had
- Nothing on the launch path verifies the image before passing it to the emulator.
- `open_runtime` (`main.rs:4347`) starts Dolphin with `-u <userdir>` and no game argument. It never reads or writes `game_path`. This is the existing, unrestricted "use it as an emulator" path.
- The live origin at `files.nflstreetreborn.com` serves release sequence 24. `chain.json` has root `78a987c7780071a84a4d89fa4cfac62e717551fcdc3c2539facbefed970d345e` and head `b5ea2dc58110...`, every node `to_bytes` 1,459,978,240, the full GameCube disc size.
- The texture component installs under `library/textures/GN7E69` (`reborn-core/src/mods.rs:4546`), keyed by game ID.

### Inferred, not verified in this session

- That a foreign `GN7E69` disc receives Reborn's textures at boot. This follows from the pack being keyed by game ID, but it was not observed in a live run.
- The exact emulator argument vector for Play (`-u -e <game> -b`) comes from project notes, not from a read in this session. Confirm before relying on it.
- The bug itself has not been reproduced end to end. The causal chain above is read from source, and reproduction is the first task of the plan.

## Design

### 1. `game_identity`, the single primitive

New module in `reborn-core`. One function hashes an image, reads its game ID, routes it against the installed chain, and returns exactly one verdict:

| Verdict | Meaning |
|---|---|
| `RebornBuild { label, at_head }` | A known node in `chain.json` |
| `Convertible` | The chain root; patchable to head |
| `Unconverted` | Game ID `GN7E69`, bytes nothing recognizes |
| `DifferentGame { game_id }` | Not `GN7E69` |
| `Unreadable { reason }` | Not a readable disc image |

Every caller goes through this. The rule lives in one place, and adding a fourth writer of `game_path` cannot reopen the hole without failing the test in section 7.

### 2. The launch gate

`launch` verifies `game_path` before starting the emulator. This is the only part of the design that reaches players who are **already broken today**; it repairs them on their next Play without them having to know anything was wrong.

### 3. Three tiers, by impersonation risk

The danger is proportional to how much the disc looks like Reborn.

**Tier 1, `DifferentGame`.** No confusion is possible: a different game ID loads a different texture folder, so none of Reborn's art applies. Allow it, state the fact once, offer no repair because there is nothing to repair.

> This is not NFL Street 2. Reborn's roster, textures and updates do not apply to it.

**Tier 2, `Unconverted`.** The dangerous case, and the one the Discord report came from. It is NFL Street 2, so the pack at `library/textures/GN7E69` paints Reborn faces over someone else's roster. It is framed as a fault, because it almost always is one:

> **Your game file isn't NFL Street Reborn.**
> This is NFL Street 2, but not a Reborn build, so rosters, faces and stats won't match. This usually means setup couldn't convert your disc.
> [Repair my game] [Play it anyway]

Repair is primary. "Play it anyway" is an escape hatch, not a mode. On acceptance:

- The choice is recorded per image hash in `settings.unverified_game_ack`, never as a global "custom mode".
- **Reborn's texture pack is switched off for that image.** This is the load-bearing part. Without it the deliberate case reproduces the exact symptom this spec exists to remove; with it, a non-Reborn NFL Street 2 looks like what it actually is.
- The Play card keeps an unresolved badge, `Not a Reborn build - Repair`, with repair one click away. It never reads as a settled preference.

**Tier 3, `RebornBuild` / `Convertible`.** `Convertible` is patched to head. `RebornBuild` boots with no notice and takes updates.

`Unreadable` sits outside the tiers. It is not a disc image at all, so it is refused at the point of selection with the reason, and never written to `game_path`.

### 4. The three doors, closed

- `setup_import_game:169` stops swallowing `Err(_)`. The verifier's reason is carried to the wizard and shown. Failure routes to recovery, never to silent acceptance.
- `settings_set_path("game", ...)` verifies and applies the tier rules. A Tier 2 result requires the acknowledgment before it is written.
- `setup_migrate_install:498` verifies the ISO it inherits and runs the same recovery.

### 5. Recovery: the `game-full` component

A new component holding the head build's complete 1,459,978,240-byte ISO, content-addressed and Range-resumable like every other blob, so it reuses the existing content engine with no new network code.

It is **not listed in the default release manifest**. A normal install never fetches it and never pays for it. It is resolved on demand only when setup or the launch gate offers recovery:

> Your copy of NFL Street 2 could not be converted. Download the Reborn game (1.36 GB).

This keeps it out of "included within this install" per Adam's 2026-09-10 rule, while still guaranteeing nobody is stranded.

Publishing it is a `rebornctl` command alongside `publish game-patch`, pinned to the same disc hash the chain head names, so the download and the patch chain can never drift.

### 6. Wizard truth

`outcomeWords` gains real failure states. `unknown` and `no-chain` stop returning success strings with a green bar, and `no-chain` gains an explicit case instead of falling through `default:`.

### 7. Testing

- Unit tests per verdict against a synthetic chain, including the root, the head, a mid-chain node, an off-chain `GN7E69` image, and a non-`GN7E69` image.
- A command-body test asserting every writer of `game_path` calls the verifier, in the same shape as the existing `Supervisor::acquire` assertions at `main.rs:5711`.
- A regression test that an `Unconverted` image cannot produce any success outcome string, asserted against `outcomeWords`.
- A test that acknowledging Tier 2 disables the texture pack for that image.
- Reproduction first: import a deliberately non-matching `GN7E69` image and observe the wizard reporting success, before any fix lands.

## Out of scope

- `open_runtime` is untouched. Opening Dolphin directly stays unrestricted, for any game.
- Texture replacement by game ID is Dolphin behavior. A foreign `GN7E69` disc opened directly in Dolphin, outside Reborn's Play path, still picks up whatever is in the user folder. Reborn only governs what it boots itself.
- File-level disc repair (splicing only the failing files) was considered and rejected by Adam in favor of the full-disc download.
