# Game updates without the disc: the `game-patch` component

Decided and built 2026-09-06. This is the breakdown of the decision, then
what was built and what it was measured to do.

## The problem

The installer no longer ships the game. It ships one xdelta (687 MB) from
the stock NFL Street 2 disc to the current Reborn build, and onboarding
applies it to the disc the player supplies. The player's disc is then out
of the picture: the import keeps the patched result and nothing else.

That leaves no way to ship a new Reborn build. A second xdelta from the
stock disc needs the stock disc again, and the whole point of the launcher
is that nobody hunts for a file twice.

## The options

| | What the player downloads per game update | What it needs on the player's PC | What it costs |
|---|---|---|---|
| **A. Keep the stock source** (1.4 GB `source.bin`) beside the import | one xdelta from stock, 687 MB | 1.4 GB retained, never played | 687 MB per player per update, and a copy of the stock disc image sitting on disk for no other reason |
| **B. Ask for the disc again** on every game update | 687 MB | the disc, again | the friction the launcher exists to remove; players who ripped once and deleted the rip are stuck |
| **C. Publish the whole new image** as a content component | 1.46 GB | nothing | a public URL serving the modded game (pulled for exactly this reason on 2026-09-06), and 1.46 GB per player per update |
| **D. Build-to-build deltas over a pristine copy** | about 1 MB per build | a pristine copy of the current build, plus the delta tool the bundle already ships | the layering below |

D was chosen. A player gets a new Reborn build for the price of a roster
update, and the origin never serves the game.

## What D requires: two layers

A delta applies only to bytes that are exactly what the publisher had.
Before this, the roster was applied *into* the imported image, so every
player's image was unique the moment a roster landed, and no delta could
ever be published for it. So the image is now two files:

- **Pristine**: `library/imported/<sha256>/NSR.iso`. What the import
  built, named by its hash, never written to again. What game patches
  apply to.
- **Booted**: `library/offline/NSR.iso`. What Dolphin opens. Made as a
  hardlink of the pristine file (no bytes copied, no extra disk) and
  replaced by a real file the first time a roster is applied: the Builder
  reads the pristine image and writes a new one, which is renamed over the
  link. The pristine bytes are untouched either way. `settings.game_path`
  points here.

Sub-decisions inside that:

1. **Hardlink, not copy, until a roster lands.** A copy would cost 1.46 GB
   and a few seconds per import for nothing. The one filesystem risk (a
   program writing into the booted file in place would write into the
   pristine one) does not arise: Dolphin only reads the image, the Builder
   writes to its own output path, and the launcher only ever renames over
   the booted path. On exFAT or FAT32, where hardlinks do not exist, the
   code copies and says so.
2. **`library/offline` for the booted image, not a new folder.** That is
   the offline image in the architecture documents already: the one the
   player plays and customizes, whose rebuilt state (`music/state.json`)
   verification consults. The roster's rebuild record goes on this file,
   as it did before.
3. **One pristine copy kept, the old one retired after a successful
   update.** Not two: 1.46 GB of disk per player to keep a build nobody
   boots. Rollback of a game update is therefore an import from the disc,
   which is what it was before this component existed. A reverse delta
   (also about 1 MB) is the cheap way to add rollback later if it is ever
   wanted; the chain format has room for it.
4. **A chain, not a single delta.** The component carries every delta
   between consecutive builds, in order, plus `chain.json`. A player whose
   pristine image is build 1 while the origin is at build 3 walks two
   steps; a player at build 2 walks one. The content system already fetches
   only the files a player lacks, so a player at build 2 downloads exactly
   the new delta. New installers carry an updated stock-to-current xdelta,
   so a fresh player never walks the chain at all.
5. **An image the chain does not know is refused, by name.** A player who
   imported an existing 2.1 image (a copy, not a patch from the disc) has a
   hash the chain never saw. The message says so and says what to do:
   import from the disc. Nothing of theirs is touched.
6. **The roster is re-applied after every game update.** A new pristine
   image carries no roster. The pipeline promotes `game-patch` before
   `roster`, applies the chain, re-establishes the booted image, then runs
   the Builder with the roster that is live. An install with no roster
   ends after the patch, and that is reported as fine, not as a failure.
7. **The publisher verifies its own delta before publishing it.** It
   decodes the delta over the old build and hashes the result. A delta that
   does not reproduce the new build byte for byte would otherwise be served
   to every player with a valid signature on it.

## What was built

- `reborn_core::game_image`: the two layers. `find_pristine`,
  `list_pristine`, `booted_path`, `establish` (hardlink or copy, clears the
  rebuild record), `retire_other_pristine`.
- `reborn_core::game_patch`: `Chain` (schema `nsr.game-patch-chain.v1`,
  validated: linear, lowercase hex, patch paths under `patches/`, no
  loops), `Patcher` (trait; `Xdelta3` runs the bundle's tool through the
  process seam), `apply` (hash the pristine image, route, decode each
  step, verify length and hash, swap, establish, retire).
- `roster::apply` now reads the pristine image and writes the booted one.
  An install without a pristine image (the old content-system layout)
  still rebuilds `library/offline` in place.
- `content::pipeline`: `game-patch` is the seventh updatable component,
  promoted before the roster; `ImageHook` carries both tools;
  `apply_current_game_patch` and `apply_current_roster` are idempotent.
- `ComponentId::GamePatch` (`game-patch`), `DataDir::LibraryGamePatch`
  (`library/game-patch`).
- `rebornctl publish game-patch --from <old.iso> --to <new.iso> --version
  <label> --origin ... --key ... --apply`: encodes, verifies, appends to
  the chain kept in `<origin>-stage\game-patch`, publishes the component,
  re-signs the release with every other pin carried forward.
- `rebornctl game-patch apply` for a by-hand run; `rebornctl update content
  --component game-patch --apply` does it as part of the update.
- The UI's three import paths (`setup_import_game`,
  `setup_import_existing_game`, `setup_migrate_install`) establish the
  booted image and point `settings.game_path` at it. When a roster is
  already installed it is applied to the fresh image then and there.

## What it was measured to do

`Tools/e2e-game-patch.py`, over the real 1.46 GB image with the real
xdelta3 and the real Builder, on 2026-09-06:

| Step | Result |
|---|---|
| `publish game-patch` A to B (hash both images, encode, decode back, hash, chain, component, release) | 5,336 byte delta for a few changed bytes, 8 s. A real consecutive build pair measured 1.1 MB. |
| a client at A walks to B | 5 s: pristine and booted are B, the old pristine folder is gone, nothing left in staging |
| a second publish (B to C); the client at B updates | it fetches exactly two blobs (the new `chain.json` and the one new delta) and walks one step |
| a fresh client still at A | walks both steps in one update |
| an image the chain does not know | refused by name, pristine and booted untouched, exit code 1 |
| a roster applied on top | goes into the booted copy; the pristine hash does not move |
| a later game update (C to D) on that install | rebuilds the booted image and re-applies the roster with the Builder, 11 s, and the rebuild record names the new booted hash |

Across the board: `cargo test --workspace` 624 passed, the six checkers
pass, `e2e-update.py` (12 steps) and `e2e-roster.py` (3 steps) pass on the
same build.

## What is deliberately not here

- Rollback of a game update (see sub-decision 3).
- Retaining the stock disc image (option A).
- Any change to what the installer ships: the stock-to-current xdelta is
  still made by `Tools/create-game-patch.py` and bundled by `build.ps1
  -GamePatch`. The chain's first `from` must be that xdelta's output hash,
  so publish the first game patch **from the image the installer builds**.
