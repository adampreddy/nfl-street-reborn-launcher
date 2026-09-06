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
| the game image (`NSR.iso`) | the content system, `offline-content` | the whole image, since one file is one blob |
| Dolphin, its INI files, ReShade presets | the content system, `runtime` component | only the changed files |
| a curated mod package | the content system, `mods` component | only the changed files |
| **a roster**, published from the website editor | the content system, `roster` component: the `.rebornroster` travels, and each launcher applies it to its own image with the Reborn Builder. Measured: 6.6 s including 455 portraits | 0.5 MB, or about 30 MB with portraits |
| the launcher itself: a page, a button, a rule, this tool | a launcher release. The launcher checks `latest.json` on its own, downloads the installer, verifies its hash, runs it silently, exits so its files can be replaced, and the installer starts it again. The player does nothing. What YOU publish is the installer plus `latest.json` on a GitHub release tagged `v<version>` | the installer, one file |
| which keys are trusted | a launcher release, as above | same |
| memory cards, saves, a player's own soundtrack | never touched, by design | none |
| the Xbox copy | no update path, by decision; the wizard is a file mover | not applicable |

One thing the launcher path needs that does not exist yet: an installer
build that comes out as ONE file, because `latest.json` pins a single
download and `build.ps1` writes it only for a single-file build. The full
first-run installer carries the texture pack and is spanned across three
files, so it gets no `latest.json`. A launcher-update build that leaves the
big content out (players get that through the content system anyway) is
the missing build mode. Until it exists, launcher self-update has nothing
to fetch.

Deleting a file works: the next version's manifest simply does not list
it, and the client moves it aside into `prior/`, never deletes it, so
rollback brings it back. A file the launcher never shipped, for example
something a player dropped into the folder, is nobody's to remove and is
left alone.

Adding a new KIND of thing, an eighth component, is a launcher change: the
seven names are fixed in the launcher and it refuses any other.

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

Then run `cargo test -p reborn-core signing`. One test,
`the_build_as_shipped_has_no_trust_anchor_yet_and_says_so`, will fail on
purpose: it asserts the keys are still empty, so the day they land is a
deliberate edit. Flip its two assertions to `has_trust_anchor()` and
`key_count() == 2`.

Or send the two PUBLIC lines to whoever is editing the code. Public keys are
not secret.

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
3. Bucket settings, Custom Domains, add `files.nflstreetreborn.com`.
   Cloudflare adds the DNS record itself since you own the zone there.
4. Check in a browser: `https://files.nflstreetreborn.com/release.json`
   and `https://files.nflstreetreborn.com/release.json.sig` both load.
5. Prove it the same way as step 6, against the real host:
   `rebornctl update content --origin https://files.nflstreetreborn.com/ --component textures --data-root "C:\reborn-test-root"`

R2 charges nothing for egress, which is the reason it was chosen: 20,000
players pulling gigabytes costs storage only.

## 7a. Roster pushes

A roster is the one thing that changes often, so it does not travel as a
new image. The `.rebornroster` the site exports travels, and every launcher
applies it to its own image with the Reborn Builder's console build.

What exists today:

1. Export from `nflstreetreborn.com/rdatabase` as you do now. Name the file
   `roster.rebornroster` in a folder by itself.
2. Publish it as a component, exactly like the others:
   ```
   rebornctl publish component --root <that folder> --component roster --version 2026.09.06 --sequence 1 --origin "C:\Reborn Origin" --apply
   ```
   Then a release pinning `roster=2026.09.06` alongside the unchanged
   components, and upload. Half a megabyte without portraits, about thirty
   with them.
3. A client picks it up like any component, and then applies it:
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

Not yet: the Publish button on the site and the `publish roster --from
<url>` shortcut that fetches what it published. Both are small. And
`reborn-builder-cli.exe` has to ship beside Dolphin; today the launcher
finds it in `<install>\tools`, beside the launcher, or in the 2.1 tools
folder.

## 8. What the launcher does once wired

This is the part not built yet, and it is the next piece of work.

- On start, if the player opted into automatic updates during first run, it
  fetches `release.json` and `.sig` and verifies the signature. A failed
  signature or an unreachable host is an outage, not an update: nothing is
  shown, the player plays, and diagnostics record it.
- It plans each component against what is on disk and fetches only what
  differs, in the background, into `downloads\` then `staging\`.
- It swaps when the game is closed, never while Dolphin is running. The hint
  strip says: "Update ready. 210 MB. Installs the next time you close the
  game."
- The one blocking screen appears only when the player tries to play online
  below the required version, and "Play offline instead" is always on it.

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
