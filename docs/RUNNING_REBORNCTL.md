# Running rebornctl

**Delivery 1, August 30 2026. Windows x64.**

This is the launcher core with a command line in front of it. There is no
window yet. Everything here is the same code the eventual interface will call,
so anything that works here works there.

The binary is at `dist\rebornctl.exe`. It has no installer and no dependencies,
and it writes nothing until you ask it to.

---

## What you can do with it today

| Command | What it does | Writes anything? |
|---|---|---|
| `status` | Where everything lives, and whether a launcher is running | Creates its own data folder |
| `migrate` | Shows what moving your 2.1 install would do | No, unless you pass `--apply` |
| `audit-runtime` | Finds files that must not ship in a release | No |
| `controller` | Prints the GCPadNew.ini the launcher would generate | No, unless you pass `--out` |
| `play` | Boots the game, waits for it, records how it ended | Writes emulator config |
| `sessions` | Every recorded session, including the ones that ended badly | No |

---

## Start here

```
dist\rebornctl.exe status
```

Prints both roots and every data directory. The line worth reading is the one
confirming the data root sits outside the install root. That is the fix for the
2.1 uninstaller deleting memory cards, and `status` checks it on your actual
machine rather than trusting a test.

---

## Booting the game

```
dist\rebornctl.exe play ^
  --iso "C:\Users\Adam\AppData\Roaming\NFL Street Reborn\Games\NSR.iso" ^
  --runtime "C:\Users\Adam\AppData\Roaming\NFL Street Reborn\NFL Street Reborn Dolphin.exe"
```

Add `--dry-run` to write the configuration and stop without launching, which is
the safe way to see what it would do.

Options: `--preset performance|balanced|quality|ultra` (default balanced),
`--windowed`.

What happens, in order:

1. **The lock is taken first.** Two copies of the game booting the same portable
   user directory write the same memory card, and the loser's franchise is
   whatever gets flushed last. A second `play` refuses rather than racing.
2. **Any unfinished session from last time is recovered** and reported.
3. **Configuration is regenerated from scratch.** Anything you changed inside
   Dolphin is discarded every launch. That is deliberate: it is what makes the
   launcher's settings the only source of truth instead of one of two competing
   ones.
4. **The game is launched with `-u` and `-e`**, so it boots straight into the
   image with no game list, and a system-wide Dolphin install is untouched.
5. **The process stays until the game exits**, then classifies how it ended and
   records it.

A crash is distinguished from a non-zero exit. A non-zero exit is a failure and
returns you quietly. A crash carries its detail through to the recovery message.

Two settings in the generated config are not accidental. `EFBAccessEnable` is
**True**, because with EFB access off the NFL Challenge portraits come out black
and get baked into your saves. `UsePanicHandlers` is **False**, because the rule
is no error popups, ever.

---

## Moving a 2.1 install

```
dist\rebornctl.exe migrate --from "C:\Users\Adam\AppData\Roaming\NFL Street Reborn"
```

Prints the plan and copies nothing. Add `--apply` to actually do it.

The order is the point. Memory cards move first, then screenshots, then the
2 GB texture copy. The plan tells you the step after which an interruption
cannot cost you anything permanent, so you know when it is safe to walk away.
Nothing in the 2.1 install is changed or removed, and nothing already present at
the destination is overwritten, because migration may be run twice and a second
run must not replace a save you have been playing with an older copy of it.

If mods are applied but the JSGME `!BACKUP` folder cannot restore the originals,
migration **refuses**. Copying then would treat modded files as pristine, and
every later verification would measure against the wrong thing.

---

## Two bugs this binary found on its first real run

Both were live in code that had 79 passing tests.

**The data root was your live 2.1 install.** It resolved to
`%APPDATA%\NFL Street Reborn`, which is a real Dolphin user tree containing a
JSGME `MODS` folder with your mods in it. Windows paths are case insensitive, so
the launcher's `mods/` *was* that `MODS/`, and `mods/staging` and `mods/active`
are classed derived, meaning purgeable. A cache purge would have deleted files
out of the middle of your mod library. Launcher data now lives in
`%APPDATA%\NFL Street Reborn Launcher`, and the supervisor refuses to start in a
folder that looks like a 2.1 install.

**2026-09-12.** The data root is `%APPDATA%\NFL Street Reborn` again, safely:
before the supervisor lock, `layout::prepare_data_root` moves every 2.1 entry
(JSGME's `MODS` entries included) into `Reborn 2.1\` by rename, and the
supervisor refuses only a root where that could not finish. The composed
texture tree lives in each Dolphin user folder now, so `mods/active` is gone.

**Migration was copying something irreplaceable into a purgeable directory.**
Screenshots were marked irreplaceable and copied to `cache`. A purge would have
deleted exactly what migration had just rescued.

---

## Custom soundtrack

The 21 songs on the offline disc, replaced, shared, and reset. The Music page
in the launcher drives the same commands. Every verb takes `--data-root <dir>`
to work on a scratch root instead of the real one.

```
rebornctl music status                       the 21 slots, what is staged, the switch
rebornctl music inspect <file> [--offline]   tags, length, and the online lookup, changing nothing
rebornctl music lookup <words...>            look a song up online (iTunes, then MusicBrainz)
rebornctl music add <file> --slot N [--title T --artist A --album L] [--offline]
rebornctl music import <pack>                stage every song a .nsrmusic or .nsmp carries
rebornctl music reset <slot>                 stage a slot back to the disc's own song
rebornctl music details <slot> --title T     stage new banner text
rebornctl music unstage <slot> | --all       drop staged changes
rebornctl music apply                        one rebuild of the disc from the staged slots
rebornctl music enable | disable             the switch; disable puts the pristine disc and the roster back
rebornctl music export <out> --slots 3,7,21 --name N --author A
rebornctl music write-tree --tree <root>     dev tree: write pathfind.dat + template.dat and stop
```

What `apply` does: reads the booted image's FST, splices the wanted songs into
the pristine image's `pathfind.dat` (stock slots come from the pristine image,
so a reset is exact), rewrites the PFDx position table, writes the 21 banner
records into the booted image's `template.dat` (so the roster survives),
streams a new ISO with a regenerated FST to `staging/music/NSR.iso.part`,
reads it back, records its hash in `music/state.json`, and renames it over
`library/offline/NSR.iso`. The pristine image under `library/imported` is never
written. About 3 seconds on an SSD with the disc in the file cache; up to half
a minute cold.

`write-tree` is for the development root under `nflstreet2_clean`: it writes
the two files into the tree and stops, so `tools/build_iso_fields.py` stays the
only disc builder there (the build sync law).

Songs longer than about 6:20 are refused; opus and wma files are refused with
"convert it to MP3 first". The record lives at `music/soundtrack.json`;
imported files are copied to `music/sources`, packs to `music/packs`, encoded
blobs to `music/encoded`. Nothing under `music/` is ever deleted by repair.

## The netplay copy of the game

Netplay is lockstep: both sides run the same bytes. The offline image is the
one a custom soundtrack rewrites, so netplay never boots it. It boots
`library/online/NSR.iso`, which is the pristine game plus the roster and
nothing else, rebuilt whenever a game patch or a roster push moves what it is
made of.

```
rebornctl netplay-image                     what it was built from, and whether it is current
rebornctl netplay-image --build             build or refresh it
rebornctl netplay-image --build --builder <path to reborn-builder-cli.exe>
```

`source.json` beside the image records the pristine hash and the roster hash,
so the freshness check is two small file reads rather than hashing 1.4 GB.
Measured on the development machine: about 6 seconds to build with a 569
player roster, and 0.2 seconds to confirm it is current. With no roster
installed the image is a hardlink of the pristine one and costs no disk.

The Online page starts netplay with this image, building it first if it has to.
Textures are not part of it: Dolphin reads those from a folder at run time, so
they can never make two players' discs differ.

## What is not here yet

Said plainly so nothing looks more finished than it is.

- **No window.** The Tauri interface is not written.
- **`controller` produces placeholder device lines.** Real device strings need
  SDL enumeration, which is not written, so that file will not bind a real pad.
  The command says so when you run it.
- **No mods, music, online, stats, or Xbox wizard.**
- **Nothing verifies a download**, because there is no update service yet. The
  verify and repair code exists and is tested, but nothing calls it.
- **macOS and Linux compile and are unproven.** The seams exist and the code
  builds; no one has run it.

---

## Rebuilding it

```
cargo build --release -p reborn-cli
cargo test
```

94 tests. They are not decoration: several encode things that were found the
hard way, including that no irreplaceable migration step may target a derived
directory, and that a session interrupted by the launcher dying still reaches
the player.
