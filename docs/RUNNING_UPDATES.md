# Running updates, step by step

How to go from "the code exists" to "players receive updates". Every command
is `rebornctl`, built from this repository. Nothing here needs a server that
runs code: the origin is a folder of static files.

Where this stands today: the publisher, the client, and the proof between
them all work (`py -3 Tools\e2e-update.py`). The launcher window does not yet
check on its own; `rebornctl update content` is the client until that wiring
lands. So steps 1 through 7 are real now, and step 8 describes what the
launcher will do once wired.

## What can be updated, and how

| You change... | It reaches players by... | Cost to a player |
|---|---|---|
| a texture, added, changed or deleted | the content system, `textures` component | only the changed files |
| **the game image** (a new Reborn build) | the content system, `game-patch` component: an xdelta from the previous build to the new one, applied by each launcher to its own pristine image, then the roster re-applied. See 7b | about 1 MB per build (measured: 5,336 bytes for a few changed bytes; 1.1 MB between two real consecutive builds) |
| Dolphin, its INI files, ReShade presets | the content system, `runtime` component | only the changed files |
| a curated mod package | the content system, `mods` component | only the changed files |
| **a roster**, published from the website editor | the content system, `roster` component: the `.rebornroster` travels, and each launcher applies it to its own image with the Reborn Builder. Measured: 6.6 s including 455 portraits | 0.5 MB, or about 30 MB with portraits |
| the launcher itself: a page, a button, a rule, this tool | a launcher release. The launcher checks `latest.json` on its own, downloads the installer, verifies its hash, runs it silently, exits so its files can be replaced, and the installer starts it again. The player does nothing. What YOU publish is the launcher-only installer (`build.ps1 -LauncherOnly`, one file) plus `latest.json` on a GitHub release tagged `v<version>`. See 7c | the installer, one file, about 25 MB |
| which keys are trusted | a launcher release, as above | same |
| memory cards, saves, a player's own soundtrack | never touched, by design | none |
| the Xbox copy | no update path, by decision; the wizard is a file mover | not applicable |

The launcher path needs an installer that comes out as ONE file, because
`latest.json` pins a single download. The full first-run installer carries
the texture pack and the import bundle and is spanned across three files,
so it never gets a `latest.json`. `build.ps1 -LauncherOnly` is the build
that does: the launcher, `rebornctl` and the Reborn Builder, nothing else,
installing over the full install and leaving everything else in place.

Deleting a file works: the next version's manifest simply does not list
it, and the client moves it aside into `prior/`, never deletes it, so
rollback brings it back. A file the launcher never shipped, for example
something a player dropped into the folder, is nobody's to remove and is
left alone.

Adding a new KIND of thing, another component, is a launcher change: the
component names are fixed in the launcher and it refuses any other.

## 0. Build the tool

From inside the repository, not from your home folder: `cargo` finds the
workspace by the directory it is run in.

```
cd "C:\Users\Adam\Desktop\Reborn Installer\NFL Street Reborn Launcher"
cargo build --release -p reborn-cli
```

That produces `target\release\rebornctl.exe`. Every `rebornctl` below means
that file, so either run the commands from the repository folder as
`target\release\rebornctl.exe ...`, or use its full path:

```
"C:\Users\Adam\Desktop\Reborn Installer\NFL Street Reborn Launcher\target\release\rebornctl.exe"
```

Check it is the right build before anything else:

```
target\release\rebornctl.exe publish --help
```

If that prints the three verbs (keygen, component, release) the tool is
current. `installer\windows\build.ps1` also builds this exe, but a copy from
before the publisher landed will not have them.

## 1. Generate the keys. Once, ever.

```
rebornctl publish keygen --out "C:\Users\Adam\Reborn Keys"
```

Pick a folder OUTSIDE the repository. It writes two files and prints two
lines:

- `publisher.key`: the everyday key. Stays on the machine that publishes.
  Back it up. Never commit it, never paste it anywhere.
- `rotation.key`: the spare. Copy it to a USB stick or print it, then delete
  it from the computer. It is only used if `publisher.key` is lost or leaks,
  and a spare that sat beside the everyday key leaks in the same breach.

The tool refuses to overwrite either file, so running it twice is safe.

## 2. Compile the public keys into the launcher

The launcher trusts only the keys built into it. Paste the two printed lines
into `TRUSTED_KEYS` in `crates/reborn-core/src/signing.rs`, everyday first:

```rust
pub const TRUSTED_KEYS: &[[u8; PUBLIC_KEY_LEN]] = &[
    // everyday   <64 hex characters>
    [0x.., 0x.., ...],
    // rotation   <64 hex characters>
    [0x.., 0x.., ...],
];
```

**Done 2026-09-06.** Adam's everyday key `eac9ec…` and rotation key
`71a34c…` are compiled in, and the test
`the_build_as_shipped_trusts_exactly_the_everyday_and_rotation_keys` pins
both arrays to their hex. Proven the same day: a release signed with
`publisher.key` opened in a launcher with no `--trust` override.

If the keys are ever regenerated, paste the new lines over the old ones and
update that test's two hex strings; it fails loudly until both match.

Rebuild the installer (`installer\windows\build.ps1`) and ship it. That
launcher will accept releases signed by either key.

## 3. Decide what a component's tree is

A component is one folder, published as-is. Publish a PRISTINE tree: a mod
file sitting in it becomes content every client is told is genuine.

| Component | Tree to publish | `--install-root` |
|---|---|---|
| `textures` | the texture pack for one game id | `library/textures/GN7E69` (required) |
| `runtime` | `target\installer-payload\runtime` after a `build.ps1` run, which is Dolphin with the junk already stripped | follows from the name |
| `offline-content` | a folder holding only `NSR.iso` | follows from the name |

`build.ps1` already stages the runtime through an allowlist (no `Games`, no
`User`, no `.bak_*`, no logs) and drops `IntermediateCachePath` from
`ReShade.ini`. Publish that staged copy, not the raw Dolphin folder.

Versions and sequences:

- `--version` is the label people see, for example `1.0.1`.
- `--sequence` is a whole number that must be STRICTLY HIGHER than the last
  one published for that component. It is what stops an old release being
  replayed. Start at 1.

## 4. Publish each component into an origin folder

The origin is any empty folder. Run each command once without `--apply` to
see what it would do, then with it.

```
rebornctl publish component --root "C:\Users\Adam\AppData\Roaming\NFL Street Reborn\User\Load\Textures\GN7E69" ^
    --component textures --version 1.0.0 --sequence 1 ^
    --install-root library/textures/GN7E69 --origin "C:\Reborn Origin" --apply

rebornctl publish component --root "target\installer-payload\runtime" ^
    --component runtime --version 1.0.0 --sequence 1 --origin "C:\Reborn Origin" --apply

rebornctl publish component --root "C:\somewhere\holding\NSR.iso" ^
    --component offline-content --version 3.0.0 --sequence 1 --origin "C:\Reborn Origin" --apply
```

What it does: hashes every file, copies each into
`blobs\by-hash\<xx>\<sha256>` (one copy per distinct content, so shared
files are stored once), hashes the copy again, and writes
`components\<name>-<version>.json`. It prints how many blobs were new and how
many the origin already held.

A published manifest is never rewritten. If you change a tree, publish it as
a new version with a higher sequence.

## 5. Sign the release

```
rebornctl publish release --origin "C:\Reborn Origin" --version 3.0.0 --sequence 1 ^
    --key "C:\Users\Adam\Reborn Keys\publisher.key" ^
    --component textures=1.0.0 --component runtime=1.0.0 --component offline-content=3.0.0 ^
    --apply
```

It checks every pinned manifest round-trips and every blob it names is in
the origin at its published length, writes `release.json` and
`release.json.sig`, then reads both back off the disk and verifies them the
way a launcher will. The last line must say `Read back and verified`. If it
warns that the key is not in `TRUSTED_KEYS`, step 2 has not shipped yet.

The release `--sequence` must be strictly higher than the one already in the
origin. The release version is the label players see ("Reborn 3.0.0").

## 6. Test it locally before uploading anything

The full automated proof, on a loopback server, against a scratch data root:

```
py -3 Tools\e2e-update.py
```

Or by hand. Serve the origin:

```
py -3 -m http.server 8000 --directory "C:\Reborn Origin" --bind 127.0.0.1
```

Then, in another window, plan and apply against a THROWAWAY data root.
`--trust` is the everyday public key from step 1, so this works before step
2 has shipped:

```
rebornctl update content --origin http://127.0.0.1:8000/ --component textures ^
    --trust <everyday public hex> --data-root "C:\reborn-test-root"

rebornctl update content --origin http://127.0.0.1:8000/ --component textures ^
    --trust <everyday public hex> --data-root "C:\reborn-test-root" --apply
```

The first prints the plan and downloads nothing. The second fetches, hashes,
stages, hashes again, and swaps. Run it a third time and it says up to date.
Note that Python's built-in server does not support `Range`, so a resumed
download is refused against it; the e2e script uses its own server that does.

Never run `--apply` without `--data-root` while testing: without it, the
real data root is the target and a promotion into it is a real update.

## 7. Host it

1. Cloudflare dashboard, R2, create a bucket, for example `reborn-content`.
2. Upload the CONTENTS of the origin folder, keeping the paths:
   `release.json`, `release.json.sig`, `components\...`, `blobs\...`.
   For a few files the dashboard works. For the blob store use `rclone`
   against the bucket's S3 endpoint:
   `rclone copy "C:\Reborn Origin" r2:reborn-content --progress`
   Two things about a bucket-scoped token, both found on 2026-09-06:
   - `rclone lsd r2:` answers 403. It lists every bucket in the account,
     which a token scoped to one bucket may not do. Test with
     `rclone lsd r2:reborn-content` instead.
   - Add `no_check_bucket = true` to the `[r2]` section. Without it rclone
     tries to confirm the bucket exists, cannot with a scoped token, assumes
     it is missing, and tries to CREATE it, which is what the 403 on upload
     is. And check the file ends with a newline before appending a line to
     it: a value glued onto `acl = private` fails every upload with a 501.
   Upload order matters: blobs and `components/` first, `release.json` and
   `release.json.sig` last, so a client mid-upload never sees a release that
   pins content still in flight. `rclone copy` with `--exclude release.json
   --exclude release.json.sig`, then `rclone copyto` the two release files.
3. Bucket settings, Custom Domains, add `files.nflstreetreborn.com`.
   Cloudflare adds the DNS record itself since you own the zone there. The
   first HTTPS request may fail for a few minutes while the certificate is
   issued; a 404 from `Server: cloudflare` means it is working.
4. Check in a browser: `https://files.nflstreetreborn.com/release.json`
   and `https://files.nflstreetreborn.com/release.json.sig` both load.
5. Prove it the same way as step 6, against the real host:
   `rebornctl update content --origin https://files.nflstreetreborn.com/ --component textures --data-root "C:\reborn-test-root"`
   Done 2026-09-06: runtime and roster installed from the live URL into a
   scratch root with only the compiled-in keys.
6. Cache the blobs at the edge. Without this every blob fetch is an R2 read
   (`cf-cache-status: DYNAMIC`); with it, repeat fetches are served by
   Cloudflare and never touch the bucket. Dashboard, the
   `nflstreetreborn.com` zone, **Caching, Cache Rules, Create rule**:
   - Name `files blobs`. When: Hostname equals `files.nflstreetreborn.com`
     AND URI Path starts with `/blobs/`. Then: Cache eligibility Eligible,
     Edge TTL Override, 1 year; Browser TTL Override, 1 year. Blobs are
     named by their own hash, so they can never change; a year is honest.
   - Leave `release.json` and `components/` uncached (they change on every
     publish); no rule needed, DYNAMIC is the correct answer for them.

R2 charges nothing for egress, which is the reason it was chosen: 20,000
players pulling gigabytes costs storage only.

## 7a. Roster pushes

A roster is the one thing that changes often, so it does not travel as a
new image. The `.rebornroster` the site exports travels, and every launcher
applies it to its own image with the Reborn Builder's console build.

One command, from the export to "upload the origin":

```
rebornctl publish roster --from <URL or file of the export> --origin "C:\Reborn Origin" --version 2026.09.06 --key "C:\Users\Adam\Reborn Keys\publisher.key" --apply
```

It fetches the export, refuses anything that is not a roster (a saved
error page, say), publishes it as the `roster` component with the next
sequence, and re-signs a release that carries every other pinned component
forward at its served version. Run it without `--apply` to see what it
would do. Then upload the origin folder. Half a megabyte without
portraits, about thirty with them.

`--from` can be a local path while the site has no Publish button yet.

A client picks it up like any component, and then applies it:
   ```
   rebornctl update content --origin <url> --component roster --apply
   ```
   `update content` runs the Builder itself after the roster file lands.
   To re-run it by hand, or after a rollback:
   ```
   rebornctl roster apply
   ```

What it does on the player's machine, in order: refuses if the game is
running; runs the Builder into staging with the player's own image as the
base; checks the length and hashes the result; records the new image hash
as a rebuild so verification keeps it instead of re-downloading 1.4 GB;
one rename swaps it in. Measured on the release build: 6.6 seconds for a
roster carrying 455 portraits, 4.4 of them inside the Builder. The Builder's
site warnings are printed, not fatal.

Applying the same roster twice reports `Unchanged` and swaps nothing. A
roster that names a player the image does not have is refused by the
Builder with the player's id, and the launcher relays it with "the game is
as it was".

Not yet: the Publish button on the site, which drops the export at a fixed
URL so `--from` can point at it. And `reborn-builder-cli.exe` has to ship
beside Dolphin; today the launcher finds it in `<install>\tools`, beside
the launcher, or in the 2.1 tools folder.

## 7b. Game updates: a new Reborn build

The installer patches the player's own disc once and keeps the result as a
**pristine** image under `library/imported/<hash>/NSR.iso`. The image the
game boots is `library/offline/NSR.iso`, a link to the pristine one until
a roster goes into it. Game updates are deltas between pristine builds, so
they are about a megabyte and the origin never serves the game. The
decision and its alternatives are in
`docs/architecture/GAME_PATCH_COMPONENT.md`.

One command, from two images to "upload the origin":

```
rebornctl publish game-patch --from <the build players have>.iso --to <the new build>.iso --version 2026.09.07 --origin "C:\Reborn Origin" --key "C:\Users\Adam\Reborn Keys\publisher.key" --apply
```

`--from` is the build players have now: the head of the chain, or for the
very first patch, the image the installer's own xdelta produces (the
`--target` you gave `Tools/create-game-patch.py`). `--to` is the new build.
`--version` is the label players see. It hashes both, encodes the delta
with the bundle's xdelta3 (`target\installer-payload\game-import`,
or pass `--xdelta`), **decodes it back and hashes the result** before
anything is written, appends the step to the chain kept in
`C:\Reborn Origin-stage\game-patch`, publishes the chain as the
`game-patch` component with the next sequence, and re-signs the release
with every other pin carried forward. Without `--apply` it hashes and
describes. Then upload the origin folder.

The chain is append-only. A player two builds behind walks two steps; a
player one behind fetches only the new delta, because that is the only
file they lack. A player whose image is not in the chain (imported from a
2.1 copy rather than patched from a disc) is told to import from the disc,
and nothing of theirs is touched. Publish a patch from any build other
than the head and the publisher refuses.

A client picks it up like any component:
   ```
   rebornctl update content --origin <url> --component game-patch --apply
   ```
   After the files land it hashes the pristine image, walks the chain
   (each step verified by length and hash), moves the result into
   `library/imported/<new hash>/`, re-links `library/offline/NSR.iso`,
   removes the old pristine folder, and re-applies the roster with the
   Builder. By hand, or after the tool was missing:
   ```
   rebornctl game-patch apply
   ```

Measured on 2026-09-06 over the real 1.46 GB image: publish 7 s (two
hashes, encode, decode-back, hash), client walk 6 s per step, plus the
roster re-apply (6.6 s). Disk on the player's PC: one pristine image and
the booted one; the booted one costs nothing until a roster is applied,
then it is a second 1.46 GB file.

Not carried: rollback of a game update. The old pristine folder is
removed once the new one is in place, so going back is an import from the
disc. A reverse delta is the cheap way to add it later.

## 7c. Launcher updates: the one-file build

The launcher updates itself from a small installer, never from the
first-run set. Build it:

```
.\installer\windows\build.ps1 -LauncherOnly -Version 3.0.1
```

It compiles both binaries, stages the launcher, `rebornctl.exe` and the
Reborn Builder's console build under `tools\` (from
`%APPDATA%\NFL Street Reborn\Tools\reborn-builder-cli.exe`, or `-Builder`),
and compiles `dist\NFL Street Reborn Launcher Update <version>.exe`, one
file, with `latest.json` beside it pinning that file's hash, size and name.
Nothing else ships: the runtime, the textures, the import bundle and the
content are already on the player's machine, and Inno replaces what it
carries and removes nothing. That is also why a fresh player cannot start
from this file; the download page hands out the full set.

Publish: upload the Update exe and `latest.json` to a GitHub release tagged
`v<version>` on the launcher repository. `latest.json` names the exe's URL
on that release, which is where `update.rs` fetches it from. `-UpdateBase
<url>` writes a different host into the manifest if the file is put
somewhere else.

What a player's launcher does with it: checks `latest.json`, downloads the
exe into its downloads folder, verifies the hash, runs it with
`/SILENT /NORESTART /RELAUNCH=1 /DIR=<its own install root>`, exits so
its files can be replaced, and the installer starts it again. `/DIR` is
stated because the installer's `UsePreviousAppDir=no` (what keeps the
program files off the 2.1 data folder) would otherwise send a silent
upgrade to the default folder, and a player who chose another one would
get a second copy. `Tools\test-all.ps1` checks that `latest.json` pins the
file in `dist`; `-Sandbox` installs the full set on a clean Windows and
then this update over it, and checks the runtime, textures and import
bundle survived and the launcher came back.

## 8. What the launcher does (wired 2026-09-06)

The window drives the same pipeline as `rebornctl update content`
(`reborn_core::content::pipeline`); the state between the calls lives in
the launcher process (`crates/reborn-ui/src/content_updates.rs`), so a
page change never loses a download. Any page asks `content_status`.

- On the Play page, once first run is complete, it checks once per launcher
  session: `release.json` and `.sig`, signature verified with the compiled
  keys, then a quick comparison of the cached manifests with the release's
  pins. No file is hashed unless a newer version is offered. A failed
  signature or an unreachable host shows nothing on the Play page; Settings
  shows the reason under Game content.
- With something to fetch and automatic updates on, it downloads in the
  background into `downloads\` then `staging\` and shows "Game content
  update: 210 MB." With ask-first, the notice offers "View update", which is
  Settings.
- It swaps only when no game session is active, and takes the launch guard
  while swapping so Play waits. Until then the notice says "Update ready.
  210 MB. Installs the next time you close the game." and checks every five
  seconds. The game patch is applied and the roster re-applied as part of
  the swap.
- Afterwards the notice reads "Updated: textures 1.0.1, roster 2026.09.07.
  Roster applied: 569 players, 51 teams." with a Got it button, and the same
  summary waits in Settings until dismissed.
- Settings, Game content: Check now (quick), Verify and repair (hashes the
  live tree; 45 s for textures), Download, Install now, and the per-component
  lines with anything refused.

The origin is `https://files.nflstreetreborn.com/` (`origin::DEFAULT_ORIGIN`)
unless `content_origin` in `settings.json` says otherwise; that setting is
for tests and for anyone running their own origin. `Tools/check-content-notice.mjs`
drives the Play page notice through a fake Tauri in a real browser.

Not wired: the online-version gate ("Play offline instead"), which needs the
online build that does not exist yet.

## 9. Publishing an update later

Only what changed is republished. Say twelve textures changed:

```
rebornctl publish component --root <texture tree> --component textures ^
    --version 1.0.1 --sequence 2 --install-root library/textures/GN7E69 ^
    --origin "C:\Reborn Origin" --apply
```

It reports something like `12 new, 8,038 already in the origin`. Then a new
release pinning the new textures version and the unchanged others at their
existing versions, with a higher release sequence:

```
rebornctl publish release --origin "C:\Reborn Origin" --version 3.0.1 --sequence 2 ^
    --key "C:\Users\Adam\Reborn Keys\publisher.key" ^
    --component textures=1.0.1 --component runtime=1.0.0 --component offline-content=3.0.0 ^
    --apply
```

Upload the new files: the new component manifest, the new blobs,
`release.json` and `release.json.sig`. `rclone copy` only sends what is not
already there. A player on 1.0.0 downloads twelve files.

## 10. If publisher.key is lost or leaks

1. Sign the next release with the spare: `--key rotation.key`. Every shipped
   launcher already trusts it, so nothing breaks for players.
2. Run keygen into a fresh folder for a new pair.
3. Paste the new pair into `TRUSTED_KEYS` and ship a launcher.
4. Once that launcher has reached players, sign with the new everyday key and
   take the new spare offline.

## Rules that are easy to forget

- Never commit `*.key`. The repository's `.gitignore` refuses them.
- A sequence never goes down and never repeats. Keep a note of the last one
  used per component and per release.
- A published manifest or blob is never edited in place. New version, higher
  sequence.
- Publish pristine trees. The textures folder you test in is fine as long as
  no mod has been applied into it at the time.
- Test with `--data-root` pointing at a throwaway folder. Every time.
