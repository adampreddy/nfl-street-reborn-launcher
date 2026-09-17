# Pack and disc reachability

Date: 2026-09-17. To be implemented on a branch off `integration`.
Approved by Adam in chat, 2026-09-17.

## What Adam asked for

> "The user needs right pack and right disc everytime"

And, when asked whether the deep check should hash the pack:

> "Well all that matters is if the user's textures load at all. this is a path issue not a texture pack issue. we can determine if the pack is correct by if they can see stuff and the version #"

That reframing is the spec. This is not a content-integrity feature. It is a
reachability feature: prove the pack Dolphin will read is present at the path
Dolphin will read it from, and report which version that is.

**On the disc half of "right pack and right disc":** most of it already
shipped in the verified-game-image-law work now merged into `integration`.
All five commands that can make an image the booted game verify it, and all
three of the launcher's own boot paths gate on it. This spec adds only the
one thing that work deliberately left open, in section 4: plain wording on
the button that hands the player Dolphin directly. So the disc work here is
small; the pack work is the substance.

Two further decisions, both Adam's:

- Opening Dolphin directly stays unrestricted. It gets plain wording that
  Reborn's textures apply to any NFL Street 2 disc booted there.
- When a check fails, re-point or recompose silently. The player sees nothing
  unless the repair itself fails.

## Why this is the right shape

The product has already shipped this exact class of bug, invisibly, for its
whole life. Every runtime component update went to `<root>\runtime` while the
launcher ran `<root>\app\runtime`, so none of them reached anybody. The
content was never wrong. The path was. It was found by comparing two file
sizes on one machine, not by any check the product performed, and it is fixed
only as of launcher 3.0.13.

Nothing in the product performs the equivalent check for textures.

## What exists today, measured

Read on `integration` in `target/merge-tree`, 2026-09-17.

- `Composer::verify` (`crates/reborn-core/src/mods.rs:2284`) checks a composed
  tree against the plan that built it, and is called from `sync_active`
  (`mods.rs:2825`). Overlays are hashed in full. **The pristine side, which is
  the overwhelming majority of the pack, is checked by count only**, and the
  doc comment states this is deliberate because a package manifest was the
  only trustworthy source of expected hashes for those files.
- That justification has since stopped holding: the component manifest the
  signed release pins carries `{path, bytes, sha256}` for every file
  (confirmed against the live `components/textures-1.3.1.json`). Per-file
  hashes exist; `mods.rs` does not consult them. **This spec still does not
  hash them**, per Adam's reframing, but the gap is recorded here because it
  is the reason a wrong pack can pass unnoticed today.
- `ComposedRecord` (`mods.rs:1711`) carries `pristine_identity`, added
  2026-09-12 so a composed tree pointing into a swapped-out pristine rebuilds
  once. So the hardlink-staleness case is partly addressed already.
- The composed texture path is reached by a directory junction naming the
  composed root by path (`mods.rs:2121`, `mods.rs:3931`). A junction is the
  thing that silently stops resolving when a folder moves.
- `<runtime>\Sys\GameSettings\GN7.ini` is the GlobalGame layer and beats both
  `Dolphin.ini` and `GFX.ini`, where the launcher writes everything. Any
  claim about an effective Dolphin setting that reads only the launcher's own
  files is unsound.

### Both open questions, now resolved, and they simplify the design

Verified 2026-09-17, and each correction removes work:

- **`HiresTextures` already wins.** `settings_law.rs:194` locks it to `True`
  in `GAME_SETTINGS`, and `GAME_SETTINGS` is
  `<user dir>/GameSettings/<game id>.ini` (`settings_law.rs:73-76`), which is
  Dolphin's LocalGame layer. LocalGame beats the shipped
  `Sys/GameSettings/GN7.ini` GlobalGame layer. The earlier note that GN7.ini
  beats where the launcher writes is true of `Dolphin.ini` and `GFX.ini`, which
  are the Base layer, and false of GameSettings. So this needs a cheap
  assertion, not a resolver.
- **There is no `LoadPath` to resolve.** `resolve_portable_paths` does not
  exist in this tree; that name came from a stale note. `LoadPath` is stripped
  by `user_template.rs:228`, so Dolphin falls back to its own default,
  `<user dir>/Load`, relative to whatever the launcher passes with `-u`. No
  absolute path is stored, so none can drift. Stripping it was the fix.

**Therefore the real failure shape is the runtime bug's exact twin:** the pack
is composed into one user directory and the launch passes `-u` pointing at
another. Nothing compares the two. That, not path resolution, is what this
spec must check.

## Design

### 1. One reachability probe

A new function in `reborn-core` answers one question and returns one value:
will the pack load, and which version is it.

```
PackReachability {
    launch_user_dir: PathBuf,  // what -u will pass
    composed_user_dir: PathBuf,// what the compose targeted
    textures_dir: PathBuf,     // <launch_user_dir>/Load/Textures/GN7E69
    reachable: bool,           // resolves AND contains files
    hires_enabled: bool,       // in the layer that wins, not the one we wrote
    installed_version: Option<String>,
    pinned_version: Option<String>,
}
```

Four checks, all cheap. No directory walk beyond "does it contain files", no
hashing.

1. **Prove the launch's user directory is the one the pack was composed
   into.** The launch resolves a user dir and passes it with `-u`; the compose
   targeted a user dir too. Compare them as the same object, not as two
   strings that look alike. This is the runtime bug's shape, and it is the
   check that matters most.
2. **Prove `<user dir>/Load/Textures/GN7E69` is reachable**: it resolves (a
   broken junction does not) and it is non-empty.
3. **Assert `HiresTextures` is on in the LocalGame layer.** Already locked
   there, so this is a cheap confirmation that catches a regression rather
   than a resolver that recomputes Dolphin's layering.
4. **Read the installed pack version and the version the signed release
   pins.** A manifest read, not a content walk.

### 2. Repair, silently

When the probe says the pack will not load, re-point the junction or recompose
from pristine, then probe again. Report nothing on success. Surface a message
only when the repair fails, which is the one case where the player genuinely
has to know.

The repair reuses `Composer` rather than adding a second composition path.

### 3. One line on the Play card

State the pack version that is actually loading. This converts "can the user
see stuff" from a support conversation into something the launcher has already
answered, and it makes a wrong version visible without anyone having to ask.

### 4. Plain wording on Open Dolphin

Say that Reborn's textures apply to any NFL Street 2 disc booted there. No
gating, no verdict, no refusal. This is the last remaining route to the
original "old names with new faces" symptom and Adam's decision is to name it
rather than police it.

## Testing

- Unit tests for the probe against a fabricated user folder: reachable,
  missing directory, present-but-empty, broken junction, and a load path that
  disagrees with where the launcher composed.
- A test that the probe reads the GlobalGame layer, not the launcher's own
  files, for `HiresTextures`. This is the assertion that would have caught the
  runtime bug's shape.
- A test that repair is attempted and re-probed before the result is reported.
- A test that a successful repair produces no player-facing message.
- Guard tests in this repo must use `code_only()` before asserting, and each
  new guard must be demonstrated failing with its target removed and comments
  left intact. Six guards in the game-image-law branch were at some point
  incapable of failing, in five different shapes.

## Out of scope

- Hashing the pack. Adam's call: version plus visible-result is the measure.
- Gating direct Dolphin.
- The oversized-upscale problem (849 files, 2.83 GB) and the open presnap
  frame-rate drop. Both are content and performance questions, not
  reachability, and Adam's standing instruction is to stop touching the pack
  unless positive.
