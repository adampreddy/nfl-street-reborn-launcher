# Running updates, step by step

How to go from "the code exists" to "players receive updates". The Reborn
Publisher app (`crates/reborn-publisher`, eight pages: Status, Roster,
Launcher, Game, Content, Set, History, Keys) is how this is done day to day
now. Every operation below reads the same way: here is the page, here is
what **Plan** shows you (it writes nothing), here is what **Run** does (one
click, with a "Dry run" checkbox that does everything except write and
upload), here is what **Verify** proves (it reads the bucket back fresh and
checks the actual claim, not just that something happened). Then, in a
smaller note, here is the console command for when the app cannot run.

The console path is not a lesser copy kept out of nostalgia. `rebornctl`,
built from this repository, calls the exact same `reborn_core::publish`
functions the app calls, and `Tools/e2e-update.py`, `Tools/e2e-roster.py`,
`Tools/e2e-game-patch.py` and `Tools/e2e-upload.py` drive it directly, so it
stays the regression net for everything the app does. It is also what a
from-scratch bucket bring-up and a scripted release still reach for.

Two things worth knowing before you reach for either path:

- **The console never writes to History.** Only the app does: every Plan,
  Run and Verify it runs becomes one line in
  `<origin>\publisher-journal.jsonl` (`ops::run_operation`, the one door
  into the core every page uses), and the History page reads that file.
  `rebornctl publish component`, `roster`, `game-patch`, `release` and
  `upload` write to the origin and the bucket the same way, but none of
  them touch that journal. A publish done on the console is real, and it
  will not show up in History. If that gap in the record matters for what
  you are about to do, use the app, or write down what you did somewhere
  the app cannot lose it.
- **Upload order is enforced by the code, not by which command you type
  first.** Blobs and component manifests always go up before
  `release.json` and `release.json.sig`; for the launcher, the installer
  always goes up before `latest.json`; for the full installer set, the zip
  always goes up before `download.json`. This used to be something a
  person had to get right by hand with `rclone`; it is now a property of
  the upload plan's own type (`UploadPlan` always yields blob-shaped items
  before release-shaped ones, whatever order they were added in), true on
  both the app and `rebornctl publish upload`. Manual `rclone` is still
  documented below as the deepest fallback, and it is still on you to run
  those commands in order if you ever use it.

Where this stands today: the publisher, the client, and the proof between
them all work (`py -3 Tools\e2e-update.py`), and the launcher window checks
for updates on its own, live since 2026-09-06 (section 8). Nothing here
needs a server that runs code: the origin is a folder of static files.

## What can be updated, and how

| You change... | It reaches players by... | Cost to a player |
|---|---|---|
| a texture, added, changed or deleted | the content system, `textures` component (Content page) | only the changed files |
| **the game image** (a new Reborn build) | the content system, `game-patch` component (Game page): an xdelta from the previous build to the new one, applied by each launcher to its own pristine image, then the roster re-applied. See 7b | about 1 MB per build (measured: 5,336 bytes for a few changed bytes; 1.1 MB between two real consecutive builds) |
| Dolphin, its INI files, ReShade presets | the content system, `runtime` component (Content page) | only the changed files |
| a curated mod package | the content system, `mods` component (Content page) | only the changed files |
| **a roster**, published from the website editor | the content system, `roster` component (Roster page): the `.rebornroster` travels, and each launcher applies it to its own image with the Reborn Builder. Measured: 6.6 s including 455 portraits | 0.5 MB, or about 30 MB with portraits |
| **a brand new player's very first download** | not a component: `launcher/download.json` (Set page) points the site's download button at a zip of the spanned installer, the game-import bundle, the runtime and the texture pack. See 7d | the whole set, roughly 2 GB, once |
| the launcher itself: a page, a button, a rule, this tool | a launcher release (Launcher page). The launcher checks `latest.json` on its own, downloads the installer, verifies its hash, runs it silently, exits so its files can be replaced, and the installer starts it again. The player does nothing. What YOU publish is the launcher-only installer (`build.ps1 -LauncherOnly`, one file) plus `latest.json`, both into the bucket's `launcher/` folder. See 7c | the installer, one file, about 37 MB |
| which keys are trusted | a launcher release (Launcher page), as above | same |
| **memory card gear** (a new person in a slot, a site gear change) | not a component: after every roster apply, before every game and after a content update that changed the image, each launcher runs the Reborn Builder's `card-sync` over the cards in its Dolphin user folder, in-game edits kept, a recovery copy under `recovery/cards` first; `rebornctl cards sync` by hand. Needs a roster whose export carries `history.json` | a few seconds, only when the card or the roster changed |
| saves themselves, a player's own soundtrack | never touched, by design | none |
| the Xbox copy | no update path, by decision; the wizard is a file mover | not applicable |

The launcher path needs an installer that comes out as ONE file, because
`latest.json` pins a single download. The full first-run installer carries
the texture pack and the import bundle and is spanned across three files,
so it never gets a `latest.json`; it gets `download.json` instead (7d).
`build.ps1 -LauncherOnly` is the build that does the one-file kind: the
launcher, `rebornctl` and the Reborn Builder, nothing else, installing over
the full install and leaving everything else in place.

Deleting a file works: the next version's manifest simply does not list
it, and the client moves it aside into `prior/`, never deletes it, so
rollback brings it back. A file the launcher never shipped, for example
something a player dropped into the folder, is nobody's to remove and is
left alone.

Adding a new KIND of thing, another component, is a launcher change: the
component names are fixed in the launcher and it refuses any other.

## 0. Get the tools

The app and the console tool are built together and ship together:
`installer\windows\build.ps1 -Publisher -Version <v>` compiles both
`reborn-publisher.exe` and `rebornctl.exe`, stages them side by side, and
produces `dist\Reborn Publisher Setup <v>.exe`, one file. Install it once
on this machine (it is Adam's own tool, on this PC; see the design doc
section 2), and it is never uploaded anywhere and never ships to a player,
never on the bucket and never in the player's zip. Installing it also
installs `rebornctl.exe` beside it, so the console fallback below is always
available wherever the app is.

To build just the console tool, for example on a machine where the app is
not installed yet:

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

If that prints `keygen`, `component`, `release`, `roster`, `game-patch`,
`upload`, `status` and `token`, the tool is current.

Publishing logic itself lives in `reborn_core::publish` (behind the
`publish` cargo feature): `keys`, `component`, `release`, `roster`,
`game_patch`, `upload`, `status`, `journal`, `vault`. `rebornctl publish`
builds a request, prints the core's events and maps the outcome to an exit
code; the app calls the same functions directly and streams the same
events into its own panel. The lines this runbook quotes are the core's
lines. Unit tests: `cargo test -p reborn-core --features publish publish::`.

## 1. Generate the keys. Once, ever.

The Keys page has a Generate button that does exactly this, into a folder
you choose, and refuses to overwrite either file, same as the console. The
console command, for a machine that has never had the app installed on it:

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

The Keys page shows which public keys a shipped launcher trusts and
whether the key file on this machine is actually one of them; that check
is the single most expensive mistake this step exists to prevent, a key
that signs fine but that no launcher in the field will accept.

If the keys are ever regenerated, paste the new lines over the old ones and
update that test's two hex strings; it fails loudly until both match.

Rebuild the installer (`installer\windows\build.ps1`) and ship it. That
launcher will accept releases signed by either key.

## 3. Decide what a component's tree is

A component is one folder, published as-is. Publish a PRISTINE tree: a mod
file sitting in it becomes content every client is told is genuine. The
Content page fills in the install root and computes the next sequence
number for you (below); this section is the background for what it is
doing, and what you still have to type by hand on the console.

| Component | Tree to publish | `--install-root` |
|---|---|---|
| `textures` | the texture pack for one game id | `library/textures/GN7E69` (required) |
| `runtime` | `target\installer-payload\runtime` after a `build.ps1` run, which is Dolphin with the junk already stripped | follows from the name |
| `mods` | a curated mod package tree | follows from the name |

`build.ps1` already stages the runtime through an allowlist (no `Games`, no
`User`, no `.bak_*`, no logs) and drops `IntermediateCachePath` from
`ReShade.ini`. Publish that staged copy, not the raw Dolphin folder.

Versions and sequences:

- `--version` is the label people see, for example `1.0.1`.
- `--sequence` is a whole number that must be STRICTLY HIGHER than the last
  one published for that component. It is what stops an old release being
  replayed. Start at 1. The Content page computes this for you and never
  offers a version the origin already holds.

## 4. Content: textures, runtime and mods

### The Content page

**Plan**: point it at a folder, a component (`textures`, `runtime` or
`mods`) and a version. It hashes the tree once (the same hash Run will use
again, so the two can never disagree about what tree Run touches), and
shows the file count and total size, how many blobs are new against how
many the origin already holds, the release sequence before and after
(computed, never typed), and one sentence on what a player already on the
previous version would actually fetch.

**Run**: publishes the component (writes its manifest and any new blobs),
signs a release that carries every other pinned component forward
unchanged (or, for an origin that has never signed one yet, bootstraps the
very first release from this component's own version), and uploads exactly
what changed, in the enforced order: the new blobs, then the new manifest,
then `release.json` and `release.json.sig`. Dry run does the hashing and
the checking and skips every write and every upload.

**Verify**: reads the bucket back fresh over HTTPS and confirms it now
pins the version and sequence this Run just signed, with a signature that
verifies.

**Never publish the `runtime` component from a runtime the launcher has
already started.** Dolphin and ReShade write `ReShade.log`, `ReShade.log1`,
`ReShade.ini.launcher-original`, a `User` folder, a `Dump` folder, and a
non-empty `ReShadePreset.ini` only once they have actually run against
that exact folder. `runtime 1.0.0` and `runtime 1.0.1` were both published
from a folder the launcher had already staged and started, which meant
every fresh install downloaded those extra files as part of the component
and was then offered a pointless "update" whose only effect was undoing
the launcher's own staging, before a player had even played. Publish from
the `runtime` folder of a FRESH `build.ps1` payload, never one the
launcher has pointed at and run.

The Content page refuses to let this happen: both Plan and Run scan the
folder first for exactly those marks, name every one they find, and stop
before anything is written. **The console has no such check.**
`rebornctl publish component --component runtime ...` will hash and
publish a dirty folder without complaint. If you use the console for a
runtime publish, check yourself, at the top level of the folder you are
about to publish: no `ReShade.log*`, no `ReShade.ini.launcher-original`, no
non-empty `ReShadePreset.ini`, no `User\`, no `Dump\`.

Console fallback:

```
rebornctl publish component --root <folder> --component textures --version <v> --sequence <n> --install-root library/textures/GN7E69 --origin "C:\Reborn Origin" --apply
rebornctl publish component --root <folder> --component runtime --version <v> --sequence <n> --origin "C:\Reborn Origin" --apply
rebornctl publish component --root <folder> --component mods --version <v> --sequence <n> --origin "C:\Reborn Origin" --apply

rebornctl publish release --origin "C:\Reborn Origin" --version <v> --sequence <n> --key "C:\Users\Adam\Reborn Keys\publisher.key" --component textures=<v> --component runtime=<v> --component mods=<v> --apply

rebornctl publish upload --origin "C:\Reborn Origin" --apply
```

`--sequence` on the console has to be typed by hand, one higher than the
last one this component used (the Content page computes it). `textures`
needs `--install-root` (`library/textures/GN7E69` offline,
`library/textures/FR4N31` online); `runtime` and `mods` have a fixed one
built in. Remember: none of this will appear in History unless it also ran
through the app.

## 5. Sign the release

```
rebornctl publish release --origin "C:\Reborn Origin" --version 3.0.0 --sequence 1 ^
    --key "C:\Users\Adam\Reborn Keys\publisher.key" ^
    --component textures=1.0.0 --component runtime=1.0.0 --component mods=1.0.0 ^
    --apply
```

It checks every pinned manifest round-trips and every blob it names is in
the origin at its published length, writes `release.json` and
`release.json.sig`, then reads both back off the disk and verifies them the
way a launcher will. The last line must say `Read back and verified`. If it
warns that the key is not in `TRUSTED_KEYS`, step 2 has not shipped yet.

Every page below signs a release as part of its own Run; you run this by
hand only for a genuine bootstrap (an origin that has never signed a
release at all: the Content page's own Run detects that case and
bootstraps it automatically, using the component's own version as the
release version and sequence 1) or when reconstructing something by hand
that a page's own Plan has already refused to touch.

The release `--sequence` must be strictly higher than the one already in the
origin. The release version is the label players see ("Reborn 3.0.0").

## 6. Test it locally before uploading anything

The full automated proof, on a loopback server, against a scratch data root:

```
py -3 Tools\e2e-update.py
```

That proves the client side: what a launcher does once content is hosted.
The publishing side of the same pipeline, the upload itself, has its own
proof:

```
py -3 Tools\e2e-upload.py
```

(needs `target\debug\rebornctl.exe`), against `Tools\fake-s3.py`, a
loopback stand-in for the R2 bucket that checks every request's signature
the way the real bucket would. It proves: a first upload sends blobs and
manifests before `release.json` and its signature; `rebornctl publish
status` reports nothing pending once the bucket matches the local origin;
a failed multipart part is retried and a wrong secret in the vault is
rejected before anything reaches the "bucket".

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
download is refused against it; the e2e scripts use their own servers that do.

Never run `--apply` without `--data-root` while testing: without it, the
real data root is the target and a promotion into it is a real update.

## 7. Host it

The bucket itself is set up once, ever, and none of it depends on whether
the app or the console does the day-to-day publishing that follows: it is
infrastructure, not a publish.

1. Cloudflare dashboard, R2, create a bucket, for example `reborn-content`.
2. Point the app (or the console) at it: on the Keys page, the origin
   folder (`C:\Reborn Origin` by default), the R2 bucket endpoint and name,
   and the R2 access key and secret. The two secrets live in the Windows
   credential vault, never in a settings file; Keys offers to import them
   from `rclone.conf` the first time it finds one there. The console
   equivalent, once:
   ```
   rebornctl publish token --r2-access-from-file <file holding the access key id>
   rebornctl publish token --r2-secret-from-file <file holding the secret>
   ```
   Neither value is ever typed on a command line or written anywhere this
   tool keeps; each is read once from the file you name and the file
   remains yours to delete afterward.

   Getting the CONTENTS of the origin onto the bucket, day to day, is
   already part of each page's own Run above (sections 4, 7a, 7b, 7c, 7d):
   it uploads exactly what that operation touched, in the enforced order,
   as one click. You do not normally run a separate upload step.

   Two situations still call for one, on purpose:
   - **The Status page's Local origin panel** lists every component and
     release sitting in `C:\Reborn Origin` that the bucket does not yet
     serve, with the reason (never served at all, an older version on the
     bucket, or a local release signed but never uploaded), and its own
     Run button uploads all of it in the same enforced order. The console
     equivalent is `rebornctl publish upload --origin "C:\Reborn Origin"
     --apply`; `rebornctl publish status` shows the same pending list
     first, without writing anything.
   - **The very first bring-up of a bucket that has never served
     anything**: publish each component once (section 4), sign the first
     release (section 5), then run the upload step above once for
     everything at once. This is the one case where a bulk, whole-origin
     upload really is the shape of the job, and it is exactly what the
     manual `rclone` recipe below still does.

   Manual `rclone`, kept as the deepest fallback for when neither the app
   nor `rebornctl publish upload` can reach the bucket (both go over the
   same S3 API, so reaching for this is a genuinely different situation,
   not a routine choice):
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
   Upload order still matters here, by hand: blobs and `components/` first,
   `release.json` and `release.json.sig` last, so a client mid-upload never
   sees a release that pins content still in flight. `rclone copy` with
   `--exclude release.json --exclude release.json.sig`, then `rclone
   copyto` the two release files.
3. Bucket settings, Custom Domains, add `files.nflstreetreborn.com`.
   Cloudflare adds the DNS record itself since you own the zone there. The
   first HTTPS request may fail for a few minutes while the certificate is
   issued; a 404 from `Server: cloudflare` means it is working.
4. Check in a browser: `https://files.nflstreetreborn.com/release.json`
   and `https://files.nflstreetreborn.com/release.json.sig` both load.
5. Prove it the same way as step 6, against the real host: the Status page
   reads `release.json`, `release.json.sig` and `launcher/latest.json` from
   the live host the same way a player's launcher does
   (`rebornctl publish status` on the console, or
   `rebornctl update content --origin https://files.nflstreetreborn.com/ --component textures --data-root "C:\reborn-test-root"`
   for the client side specifically). Done 2026-09-06: runtime and roster
   installed from the live URL into a scratch root with only the
   compiled-in keys.
6. Cache the blobs at the edge. Without this every blob fetch is an R2 read
   (`cf-cache-status: DYNAMIC`); with it, repeat fetches are served by
   Cloudflare and never touch the bucket. Dashboard, the
   `nflstreetreborn.com` zone, **Caching, Cache Rules, Create rule**:
   - Name `files blobs`. When: Hostname equals `files.nflstreetreborn.com`
     AND URI Path starts with `/blobs/`. Then: Cache eligibility Eligible,
     Edge TTL Override, 1 year; Browser TTL Override, 1 year. Blobs are
     named by their own hash, so they can never change; a year is honest.
   - Leave `release.json`, `components/` and `launcher/` uncached (they
     change on every publish); no rule needed, DYNAMIC is the correct
     answer for them.

R2 charges nothing for egress, which is the reason it was chosen: 20,000
players pulling gigabytes costs storage only.

## 7a. Roster pushes

A roster is the one thing that changes often, so it does not travel as a
new image. The `.rebornroster` the site exports travels, and every launcher
applies it to its own image with the Reborn Builder's console build.

### The Roster page

**Plan**: choose the source, the site's own export (fetched with the
vaulted API token, `Store` it once on the Keys page) or a `.rebornroster`
file, and a version (defaults to today's date). It reads the file's own
player, team and portrait counts, and shows the component sequence and the
release sequence before and after.

**Run**: publishes the roster component, signs a release carrying every
other pinned component forward, and uploads it, blobs and the manifest
before the release files. Dry run still fetches and reads the file, since
there is no way to publish a file that was never read, but writes and sends
nothing.

**Verify**: reads the hosted release back over HTTPS, then runs the exact
check a player's launcher runs against a scratch data root seeded from
that release, and confirms the roster comes back pending at its real size
while every other component reports zero fetches, the same "just the
roster, nothing else" outcome a real update should produce.

Console fallback:

```
rebornctl publish roster --from <URL or file of the export> --origin "C:\Reborn Origin" --version 2026.09.06 --key "C:\Users\Adam\Reborn Keys\publisher.key" --apply
rebornctl publish upload --origin "C:\Reborn Origin" --apply
```

If the export URL needs a login, store the site's token once with
`rebornctl publish token --from-file <file>`, or pass `--token-file` on
this command; never type it on the command line. `--from` can also be a
local path. This will not appear in History unless it also ran through
the app.

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

`reborn-builder-cli.exe` has to ship beside Dolphin; today the launcher
finds it in `<install>\tools`, beside the launcher, or in the 2.1 tools
folder.

### The memory cards

The game keeps each player's gear in the profile save on the memory card,
one row per player id, and overlays those rows on the disc's. So a roster
that puts a new person in a slot leaves every member's card wearing the
previous occupant's gear until the card itself is brought up to date.

The launcher does that with the Builder's `card-sync` verb, over every
`GC\MemoryCard*.raw` in the Dolphin user folder a launch uses:

- after every `roster apply`, by the command line and by the window after
  a content update that applied a roster or built a new image;
- before every game, under the single-instance lock, so the cards the game
  is about to open carry the installed roster (free when neither the card
  nor the roster changed since the last run: both hashes are kept beside
  the Builder's state under `library\roster\cards`);
- after every game, harvest only: in-game gear edits are recorded and no
  card byte is written;
- at the end of "I already have Reborn", on the copied cards.

A card the game holds open is refused, never written. Before the first
write of a run the card is copied to `recovery\cards\<name>.<seconds>.bak`,
newest three kept. Every outcome is a sentence on the Play page or in the
update notice, never a popup. The Builder needs the roster update to carry
`history.json` (the 2.1 floor and every gear change since); with an older
export every profile the launcher never wrote is left as it was, and the
sentence says so.

By hand, or to look without writing:

```
rebornctl cards sync --dry-run
rebornctl cards sync
```

Proof on this machine: `py -3 Tools\e2e-cards.py <a roster with history.json>`.

## 7b. Game updates: a new Reborn build

The installer patches the player's own disc once and keeps the result as a
**pristine** image under `library/imported/<hash>/NSR.iso`. The image the
game boots is `library/offline/NSR.iso`, a link to the pristine one until
a roster goes into it. Game updates are deltas between pristine builds, so
they are about a megabyte and the origin never serves the game. The
decision and its alternatives are in
`docs/architecture/GAME_PATCH_COMPONENT.md`.

### The Game page

Two sections. **Build ISO** is not a publish at all: base image, roster
file, output path, and it runs the Reborn Builder to produce an ISO for
local testing or inspection, showing the Builder's own warnings with a
"Build anyway" button that never moves once it has appeared (the Builder's
own hard-won rule). Also here: `dump` (read a roster back out of an ISO)
and `card-sync` (fold a memory card's in-game gear and face edits back
into a roster). None of this touches the origin or the bucket.

**Game patch** is the publish. **Plan**: give it the new build's ISO. It
hashes the build the chain currently ends at (never hard-coded: read from
the chain itself, falling back to the FIELDS 12-19 base the first time
there is nothing to read), hashes the new build, and refuses if the base
on record is not actually the chain's tip, showing both hashes.

**Run**: encodes the xdelta, decodes it back and hashes the result before
writing anything, appends the step to the chain, publishes the
`game-patch` component, and signs a release carrying every other pin
forward. This writes to the LOCAL origin only; it does not upload. Follow
it with the Status page's own Upload (or `rebornctl publish upload
--origin "C:\Reborn Origin" --apply` on the console) to actually put the
new chain step on the bucket.

**Verify**: re-hashes every blob the newest published `game-patch`
manifest names, against the origin's own blob store. This proves the
local copy is intact; it does not check the bucket, so treat a completed
upload (the Status page again, or `rebornctl publish status` afterward,
showing nothing pending) as the real proof players can reach it.

Console fallback:

```
rebornctl publish game-patch --from <the build players have>.iso --to <the new build>.iso --version 2026.09.07 --origin "C:\Reborn Origin" --key "C:\Users\Adam\Reborn Keys\publisher.key" --apply
rebornctl publish upload --origin "C:\Reborn Origin" --apply
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
describes.

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
first-run set.

### The Launcher page

**Plan**: type (or accept the proposed) version. It reads what the bucket
currently serves the same way a player's launcher would, and warns,
without refusing outright, when the version you gave would not be newer
than what is hosted, since a build that is not newer is a build no
launcher would ever be offered. It also shows the repository's current
commit, whether the working tree is dirty, and whether the Reborn Builder
is where it expects.

**Run**: builds `installer\windows\build.ps1 -LauncherOnly -Version <v>`
itself, streaming every line it prints as it prints it (the build takes
minutes; the panel stays live the whole time), then hashes the exe it
produced ITSELF rather than trusting what the script's own `latest.json`
claims, refusing to upload anything if the two disagree.

**The exe uploads before the manifest, never the other way around, and
this is enforced by the upload plan's own type, not by the order this page
happens to call things in.** `UploadPlan` always yields every blob-shaped
item before every release-shaped one, whatever order they were added in;
the exe is added as a blob, `latest.json` as a release file. A launcher
mid-download can never be pointed at a manifest naming a file that is not
there yet.

**Verify**: reads `latest.json` back over HTTPS exactly as a launcher
still on the version this build replaces would, confirms it is offered
the new version, downloads the installer it names, and checks that
download against the manifest too.

Console fallback, and this is still a two-step, hand-ordered process: there
is no single `rebornctl publish` verb for the launcher's own update
channel (it is a different mechanism from the content origin, unlike
everything else in this guide), and `rebornctl publish upload` does not
reach it either, since that command only knows about `C:\Reborn Origin`,
never the bucket's `launcher/` folder.

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
from this file; the download page hands out the full set (7d).

Publish, in this order, the exe first and the manifest last:

```
<rclone> --config "C:\Users\Adam\Reborn Keys\rclone.conf" copyto "dist\NFL Street Reborn Launcher Update 3.0.1.exe" "r2:reborn-content/launcher/NFL Street Reborn Launcher Update 3.0.1.exe"
<rclone> --config "C:\Users\Adam\Reborn Keys\rclone.conf" copyto dist\latest.json r2:reborn-content/launcher/latest.json
```

That order is on you to get right by hand on this path. On the app it is
not a choice you get to make.

`update.rs` reads `https://files.nflstreetreborn.com/launcher/latest.json`
and the manifest names the exe beside it. Not a GitHub release: the
repository is private, so its release assets are not fetchable by a
player's launcher. `-UpdateBase <url>` writes another host into the
manifest if the file ever lives elsewhere. `launcher/` is outside the
`blobs/*` cache rule, so a new manifest is seen at once.

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

This will not appear in History either way, since the app's Run above
still goes through `ops::run_operation`, but the console path above never
does.

## 7d. The full first-run installer set

This is what a brand new player downloads, and it is a different thing
from the launcher's own self-update in 7c: the spanned installer exe and
its numbered slices, the game-import bundle, the Dolphin runtime, the
texture pack and, optionally, the offline WebView2 runtime, all built by
the full `build.ps1` (no `-LauncherOnly`, no `-Publisher`) and packed into
one zip, plus `launcher/download.json`, which is what the site's download
button now reads instead of a constant baked into `_site/shell.php`. Once
this ships, a new set never needs a separate site deploy.

### The Set page

**Plan**: point it at the four inputs (the game-import bundle, the
runtime tree, the texture tree, the prompt packages) and a version. It
measures each one (a plain walk for file count and size; nothing is
hashed yet, these are build inputs, not published blobs), shows what
`download.json` currently points at and warns if the version you typed
would replace it under the exact same label, and lists exactly what a zip
for this version would be packed from: the setup exe plus its own
numbered `.bin` slices, never a wildcard match against whatever else
happens to be sitting in `dist`.

**Run**: runs the full `build.ps1` with every one of those inputs (about
twenty minutes; every line streams into the panel as it prints), then,
before uploading anything, reopens the zip it just produced and confirms
its entries are EXACTLY the expected set, no more and no less. A zip
missing a slice, or one that swept in a stray file from a different
build, is refused right there rather than uploaded. Only then does it
upload the zip, and only after that succeeds does it write and upload
`download.json`, in that order: `download.json` can never point at a zip
that is not there yet, the same guarantee the Launcher page's exe-then-
manifest order relies on.

**Verify**: reads `download.json` back over the public host, re-hashes
the local copy of the zip still sitting in `dist` against what the
pointer claims, and sends a HEAD request (no re-download of a
multi-gigabyte file) to the zip's own URL to confirm it is reachable
there at the size the pointer names.

Two guards worth knowing about, both learned the expensive way. **The
game-import bundle must never sit inside `target\installer-payload`**:
`build.ps1` deletes that folder before staging a fresh build into it, so
building from a bundle inside it would delete the bundle before it could
be copied in. This happened once and cost the only staged copy of a 687 MB
bundle. Both the Set page and `build.ps1` itself refuse this before
anything is built, naming the exact path. And **the zip is packed from an
exact file list**, the setup exe's own name plus its own numbered slices,
never a wildcard, because a wildcard once swept another build's slices
into a 5 GB zip alongside a good one.

Console fallback: there is no single `rebornctl publish` verb for this.
Build it by hand, then construct `download.json` yourself:

```
.\installer\windows\build.ps1 -Version 3.0.0 -GamePatch <bundle folder> -Runtime <runtime folder> -Textures <texture folder> -PromptPackages <prompt packages folder> -OfflineWebView2 -OfflineSuffix ""
```

That produces `dist\NFL Street Reborn Launcher 3.0.0.zip`. Hash it, then
write `download.json` beside it (`schema` is always `nsr.download.v1`;
`url` is the same percent-encoding rule `latest.json`'s own asset URL
uses, so the two never disagree about how a space in the name comes out):

```json
{
  "schema": "nsr.download.v1",
  "version": "3.0.0",
  "url": "https://files.nflstreetreborn.com/launcher/NFL%20Street%20Reborn%20Launcher%203.0.0.zip",
  "bytes": 2161338617,
  "sha256": "<the zip's sha256, lowercase hex>",
  "published": "2026-09-09T14:03:00+00:00"
}
```

Then upload, the zip first, `download.json` last:

```
<rclone> --config "C:\Users\Adam\Reborn Keys\rclone.conf" copyto "dist\NFL Street Reborn Launcher 3.0.0.zip" "r2:reborn-content/launcher/NFL Street Reborn Launcher 3.0.0.zip"
<rclone> --config "C:\Users\Adam\Reborn Keys\rclone.conf" copyto dist\download.json r2:reborn-content/launcher/download.json
```

This will not appear in History either way, the same reason as 7c: the
console path never reaches the app's journal.

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

This is exactly section 4 (or 7a, 7b, 7c, 7d) again, for whichever page
fits what changed: open it, Plan, look at what is new, Run. Only what
changed is republished; the sequence and the carried-forward pins are
computed, never guessed.

Say twelve textures changed, done by hand on the console:

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
    --component textures=1.0.1 --component runtime=1.0.0 --component mods=1.0.0 ^
    --apply
```

Upload the new files: the new component manifest, the new blobs,
`release.json` and `release.json.sig`.

```
rebornctl publish upload --origin "C:\Reborn Origin" --apply
```

That sends only what is not already on the bucket. A player on 1.0.0
downloads twelve files.

## 10. If publisher.key is lost or leaks

1. Sign the next release with the spare: `--key rotation.key`. Every shipped
   launcher already trusts it, so nothing breaks for players.
2. Run keygen into a fresh folder for a new pair.
3. Paste the new pair into `TRUSTED_KEYS` and ship a launcher.
4. Once that launcher has reached players, sign with the new everyday key and
   take the new spare offline.

The Keys page carries a reminder of its own for step 2's lead-up: it flags
when `rotation.key` still sits beside `publisher.key` on the same disk,
since a spare that never left home leaks in the same breach as the
everyday key. Move it to a USB stick or paper before that reminder is the
only thing standing between you and starting this whole list.

## Rules that are easy to forget

- Never commit `*.key`. The repository's `.gitignore` refuses them.
- A sequence never goes down and never repeats. The app computes it for
  every page above; on the console it is still typed by hand, so keep a
  note of the last one used per component and per release if you use that
  path.
- A published manifest or blob is never edited in place. New version, higher
  sequence.
- Publish pristine trees. The textures folder you test in is fine as long as
  no mod has been applied into it at the time. Never publish `runtime` from
  a folder the launcher has started (section 4); the Content page checks
  for you, the console does not.
- Test with `--data-root` pointing at a throwaway folder. Every time.
- The console never writes to History. If the audit trail matters for what
  you are about to do, use the app.
- Upload order (blobs and manifests before release files; the installer
  before its manifest; the zip before `download.json`) is enforced by the
  code on both the app and `rebornctl publish upload`. Manual `rclone` is
  the one place left where it is still on you.

## League files (members only)

`league-game` (league disc + league textures) and `league-content` (league seed) are
MEMBERS ONLY. Their manifests are public and signed like everything else; their blobs
live in the PRIVATE bucket `reborn-league` (no public domain, no r2.dev), and the
Reborn backend hands members short-lived signed URLs (`POST /v1/league/download`).

Local origin: members-only blobs are in `C:\Reborn Origin\private\blobs\by-hash\`,
NEVER `blobs\`. Public `publish upload` and any rclone push of `blobs\` cannot see them.

### One-time setup

1. In Cloudflare R2 create the bucket `reborn-league`. Give it NO public domain and keep
   `r2.dev` access OFF.
2. Widen the Publisher's R2 write token so it can also write `reborn-league`.
3. Create a second token, read-only and scoped to `reborn-league` only, for the backend.
   On Railway, service `reborn-api`, set `R2_ACCOUNT_ID`, `R2_ACCESS_KEY_ID`,
   `R2_SECRET_ACCESS_KEY` and `LEAGUE_BLOB_BUCKET` (= `reborn-league`). Never write the
   values into a doc, commit or chat.
4. The Publisher setting `league_bucket_name` defaults to `reborn-league`. It must be a
   canonical bucket name, not the production bucket and not your public bucket.
5. Rebuild the Publisher AND `rebornctl` from the `feat/league-publish` branch before the
   first league pin. An older build refuses to re-sign a release that pins `league-game`
   or `league-content` (it does not silently drop them).

### Publish

Both commands are a dry run (a check, nothing written) until you add `--apply`.

    rebornctl publish league-content --seed <league.s01> [--seed-json <seed.json>] --origin "C:\Reborn Origin" --version 1.0.0 --key "<publisher.key>" --notes-file notes.md --apply
    rebornctl publish league-game --iso <league.iso> --textures <folder named after the league id> --origin "C:\Reborn Origin" --version 2026.10.08 --key "<publisher.key>" --notes-file notes.md --apply
    rebornctl publish upload --origin "C:\Reborn Origin" --private --bucket reborn-league --apply   # FIRST
    rebornctl publish upload --origin "C:\Reborn Origin" --apply                                    # THEN

Or in the Publisher: open League files and, in the league build or league seed section,
choose the files and type a version (until then the button reads "Type a version first").
Press "Publish league build <version>" (or "Publish league seed <version>"). That runs the
dry-run check and shows the plan; press "Yes, publish" to apply. The page uploads the private bucket first and the public bucket
second, and never starts the public upload if the private one fails.

What the commands refuse (read the message, fix the input):
- A compressed disc (RVZ, WIA, GCZ, CISO): convert it to a plain `.iso` first.
- A file without the GameCube magic at 0x1C.
- Retail game ids (`GN7E69`, `FR4N31`) and the placeholder `LGXXXX`. The disc's game id
  (bytes 0..6) must be the league id.
- A textures folder not named for the game id and not containing one; a folder with zero
  texture files; symlinks or junctions anywhere in the inputs.
- A seed under 1 MiB.
- A stage folder overlapping the origin or the sources. The default stage is
  `<origin>-stage\league-<name>-<version>`; when that would overlap the ISO, textures or
  seed folder, pass `--stage <empty folder elsewhere>`.

rclone fallback, same order:

    rclone copy "C:\Reborn Origin\private\blobs" r2:reborn-league/blobs
    then the usual public order: blobs -> components -> release.json + .sig

### Recovery and rollback

- If the private upload fails (Publisher or CLI), do NOT use Home -> Upload. That would
  push the public manifest and release without the members-only blobs and break every
  league install. Publish a NEW league version to retry.
- League components cannot be rolled back (History refuses them, the CLI too): their
  blobs are not in the public bucket. Publish a new version instead.

LAWS
- Never rclone `private\` into `r2:reborn-content`. Never give `reborn-league` a public domain.
- Never HTTP-probe a league blob URL (or any unpublished URL): Cloudflare caches the 404.
- Sign only with a rebornctl / Publisher that knows `league-game` and `league-content`.
- The disc's game id (bytes 0..6) must be the league id. Retail ids and `LGXXXX` are refused.
