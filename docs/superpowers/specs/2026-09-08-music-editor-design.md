# Custom soundtrack: editor, packs, and soundtrack mods

Date: 2026-09-08. Target: the September 12 launch, as a launcher Update exe
plus content release sequence 4.

Approved in conversation on 2026-09-08. This document is the design; the
implementation plan is written from it.

## 1. Goal

Players replace any of the 21 songs on the NFL Street 2 disc with their own
audio, share and apply `.nsrmusic` packs, switch back to the original
soundtrack, and pick the NFL Street 1 soundtrack from the mod browser. The
song title, artist, and album shown on the in-game banner come from the
file's tags, from an online lookup, or from the user, in that order.

## 2. Scope

Ships on 09-12:

- Music page in the launcher: 21 song cards, Replace, Reset, Details, one
  Apply changes bar with progress.
- Add a song from mp3, m4a, flac, ogg, wav, or aiff.
- Details filled from embedded tags, else from an automatic lookup by cleaned
  file name (iTunes Search, then MusicBrainz), else typed. Always editable.
- Import `.nsrmusic` (format 2) and `.nsmp` (format 1) packs. Export
  `.nsrmusic`.
- Soundtrack packages in the Mods page's Soundtrack tab, exclusive group:
  `reborn.soundtrack.original` and `reborn.soundtrack.nfl-street-1`.
- One disc rebuild per Apply. Repair keeps treating the rebuilt image as the
  user's. The soundtrack replays after a roster or game update.
- `rebornctl music` with the same verbs, plus `write-tree` for the dev tree.
- The Tkinter editor `tools/music_editor.py` is retired.

Deferred: reordering slots, opus and wma input, album art, audio
fingerprinting, hosted user packs, more than 21 songs, downloading songs.

Refused on purpose: searching for and downloading songs. The launcher never
fetches audio. Users bring their own files. The project hosts only the two
soundtrack packages it authors.

## 3. Facts the design rests on

All reverse-engineered and confirmed in game earlier in the project. The
Python tools under `nflstreet2_clean/tools` implement them and stay the
byte-level oracle for the Rust port.

Music container `pathfind.dat` (EA TERF, big endian, 3 sub-files, all raw):

- DIR1 at 0x800, entries of {offset, size} BE u32. Sub-file 2 is the ASF:
  offset field at 0x818, size field at 0x81C. Its data starts at
  0x1000 + offset = 0x13000 and runs to end of file.
- DATA INFO_SIZE at 0x1004 grows by the same delta as the ASF.
- PFDx blob at 0x2000. Position table at 0x2000 + u32BE at 0x2034 (file
  offset 0x1138C): 876 entries of 8 bytes BE, w0 = (SCHl offset - 0x13000) >> 7,
  w1 = segment length in ms = round(samples / 24).
- 876 SCHl streams. Slot s (1..21) owns 40 consecutive streams starting at
  index 3 when s == 1, else 44 + (s - 2) * 40. Every SCHl is 128-byte aligned.
- The ASF is padded to a 0x800 multiple.

Banner table in `template.dat` (EATX, uncompressed sub-file, in place):

- Field definitions at 0x17954, tags stored reversed: DINS, ITNS, LANS, RANS,
  XINS. Records at 0x1799C, 21 records, stride 156: +0 SNID u32BE, +4 title
  50 bytes, +54 album 50 bytes, +104 artist 50 bytes, +154 SNIX. Strings are
  null-terminated latin1, at most 49 bytes.
- The Rust writer locates the table by the five field tags in order and
  verifies SNID 1..21 before writing. The roster Builder rewrites
  template.dat, so a hardcoded offset is not trusted.

Audio (EA-XA v2, 24 kHz, stereo stored as [L frames][R frames] per block):

- Coefficients [0, 240, 460, 392] and [0, 0, -208, -220]. ADPCM frame is 15
  bytes: byte 0 = coef << 4 | (shift - 8), then 14 nibble bytes, 28 samples.
  PCM frame is 0xEE + 2 history samples + 28 samples BE, 61 bytes.
- Block 0: 3 PCM + 55 ADPCM frames = 1008 bytes, 1624 samples. Then a cycle
  of six 57-frame blocks (855 + 1 pad = 856 bytes, 1596 samples) and one
  58-frame block (870 bytes, 1624 samples).
- Stream: SCHl {u32LE size = 8 + PT} PT; SCCl {12} u32BE block count; SCDl
  {u32LE size} u32BE samples, u32BE 0, u32BE bytes per channel, L, R; SCEl {8}.
- Segment recipe (extend_slot_40big.py): 40 segments, seg_samples =
  ceil(total / 40). Trial-encode segment 0 unpadded, target = round up to
  0x800 plus 0x800. Refuse when target >= 0x3E000 (about 6:20 at 24 kHz).
  Pad every segment with silent blocks to exactly target. Patch PT tag 0x85 to
  the actual sum of SCDl sample fields (EA-exact accounting; a mismatch frees
  a virgin descriptor and stalls playback).
- PT headers are taken per stream from the target slot's 40 stock streams in
  the pristine image, so a track is encoded for a slot.
- The five DOL audio guards are already in the shipped disc. No DOL work.

Disc layout (build_iso_fields.py):

- Bytes 0 .. dol_off + dol_len are copied verbatim (boot.bin, bi2,
  apploader, DOL). dol_off at 0x420. DOL length from its header, the maximum
  of section offset + size over 7 text and 11 data sections.
- fst_off = align(dol_off + dol_len, 0x800), written at 0x424; FST size at
  0x428 and 0x42C. Flat FST, source order preserved. files_start =
  max(0x328000, align(fst_off + fst_size)). Every file 0x800 aligned. Disc
  padded to 0x57058000.
- In the shipped disc, pathfind.dat is followed by crowddat.dat and the
  704 MB chatdata.dat, so a longer song shifts two files. That is why the
  FST is regenerated rather than patched.
- A repack with no changes must reproduce the source disc byte for byte.

NFL Street 1 (from the USA `.ciso`, extracted with DolphinTool):

- `femusic.dat` is a TERF with 14 SCHl streams, one full song each, 5 to
  11 MB. PT: 0x84 = 32000 Hz, 0x82 = 2 channels, 0xA0 = 0x0A (EA-XA), no PCM
  frames in block 0. Same frame format; the decoder handles both.
- `template.dat` holds the same EATX table (field tags at 0x1E758) with 14
  titles, albums, and artists.

## 4. Storage and state

Everything under `music/` is user data and is never deleted by repair. This
is already the rule in `DataDir` and `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`.

```
music/
  soundtrack.json          the switch, the base, the 21 slots
  state.json               RebuildState (existing; roster and music share it)
  sources/<track-id>.<ext> a copy of each user file
  packs/<file>.nsrmusic    imported and exported packs, and soundtrack mods
  encoded/<track-id>-s<slot>.asf
                           cached 40-segment blob, keyed by track and slot
```

`soundtrack.json` (schema `nsr.soundtrack.v2`):

```json
{
  "schema": "nsr.soundtrack.v2",
  "enabled": true,
  "base": { "kind": "stock" }
        | { "kind": "package", "id": "reborn.soundtrack.nfl-street-1",
            "version": "1.0.0", "pack": "packs/nfl-street-1.nsrmusic" },
  "slots": {
    "3": { "source": { "kind": "user_file", "track": "a1b2c3d4e5f6a7b8",
                       "file": "sources/a1b2c3d4e5f6a7b8.mp3" },
           "title": "…", "artist": "…", "album": "…", "duration_ms": 251000 },
    "7": { "source": { "kind": "pack_stream", "pack": "packs/friend.nsrmusic",
                       "slot": 7 },
           "title": "…", "artist": "…", "album": "…", "duration_ms": 190000 },
    "9": { "source": { "kind": "stock" } }
  },
  "pending": { "…same shape as slots…" },
  "base_sha256": "<pristine image hash the last apply started from>",
  "rebuilt_sha256": "<image hash the last apply produced>"
}
```

- `slots` is what the booted disc has. `pending` is staged and not built.
  Apply moves pending into slots after the swap succeeds.
- A slot listed as `stock` is an explicit reset that overrides the base. A
  slot absent from `slots` follows the base.
- Effective slot n = slots[n] if present, else the base pack's slot n if the
  base is a package and the pack has n, else the disc's own song.
- `track` ids are the first 16 hex of the SHA-256 of the source file.
- `rebuilt_sha256` is also recorded into `state.json` under `NSR.iso`, the
  existing handshake that makes repair say "customized, not damaged".
- The existing `music.rs` model with `STREAM_SLOTS = 40` is wrong: 40 is the
  segment count per song, and the slots are 1..21. That module is replaced.

## 5. The disc pipeline

Module `reborn_core::soundtrack` with submodules `gcm` (read FST, repack),
`pathfind` (TERF sizes, PFDx table), `asf` (stream scan, slot splice),
`eatx` (banner table), `ea_xa` (decoder and encoder), `segments` (40-segment
layout), `decode` (symphonia and resampling), `tags`, `lookup`, `pack`,
`state`, `apply`.

Apply, given the effective 21 slots:

1. Refuse while a game session is active or the booted image is locked, the
   way roster apply does.
2. Source image = the booted image `library/offline/NSR.iso`. Read its FST,
   pathfind.dat, and template.dat. Stock blobs come from the pristine image
   `library/imported/<sha>/NSR.iso`, never from the booted one, so a reset is
   exact.
3. For every effective slot: user file or pack stream, take the cached or
   pack blob; stock, take the pristine slot's 40 streams. Splice the 21 blobs
   into the ASF in slot order, pad to 0x800, fix DIR1 size and INFO_SIZE,
   rewrite all 876 w0 and each changed slot's w1.
4. Write the 21 banner records. Blank fields keep the disc's own text only
   for stock slots; a user slot with no title gets the file name stem, and
   the page says so.
5. Stream a new ISO to `staging/music/NSR.iso.part`: verbatim head, new FST,
   every source file copied in order with pathfind.dat and template.dat
   substituted, padded to disc size. Same volume as the booted image.
6. Hash the result, record it into `state.json` and `soundtrack.json`, then
   rename it over the booted image. Undo the record if the rename fails.
   Same order as roster apply.

The switch off (`enabled: false`) and `reborn.soundtrack.original` with no
user slots do not run this pipeline: they re-establish the pristine image and
re-apply the installed roster through the existing `game_image::establish`
and `roster::apply`, which also clears the rebuild record.

Layering is pristine, then roster, then music. `content::pipeline` already
promotes game patches before rosters and re-applies the roster; music applies
after the roster through the same hook, whenever `enabled` is true and the
effective slots are not all stock. A slot whose pack or source file is gone is
skipped and listed, never dropped silently.

Time: about 1.5 GB copied once, so 10 to 20 seconds on an SSD. Nothing is
extracted to a work tree; the design doc's `library/work/` is not needed and
is not created.

## 6. Encoder and decoder

- Decode with symphonia (mp3, aac in m4a, flac, vorbis, wav, aiff). Anything
  else is refused with "convert it to MP3 first". Downmix to stereo, resample
  to 24 kHz with rubato's sinc resampler. Report the length in ms.
- Encode with a straight port of encode_ea_xa.py and extend_slot_40big.py
  (section 3). The per-frame search is the same: try 4 coefficient sets and
  shifts around the ideal, keep the least squared error. From a 24 kHz stereo
  WAV the Rust blob must equal the Python blob byte for byte.
- Songs longer than the slot ceiling are refused with the length and the
  limit in the message. No auto-trim in this release.
- The EA-XA decoder (needed for NFL Street 1 and for exporting a stock slot)
  is the inverse of the encoder and is checked by round trip.

## 7. Pack format `.nsrmusic`

A zip with stored entries: `pack.json` plus one game-ready 40-stream blob per
song. Format 2 of the existing `.nsmp` layout; format 1 files import unchanged.

```json
{
  "magic": "NSRMUSIC", "format": 2,
  "id": "9f2c…", "name": "…", "author": "…", "description": "…",
  "created": "2026-09-10", "game": "NFL Street 2 (GC)",
  "songs": {
    "3": { "blob": "slot3.asf", "sha256": "…", "bytes": 5734400,
           "duration_ms": 251000, "title": "…", "artist": "…", "album": "…" }
  }
}
```

- Slot keys are 1..21. Each blob is validated: 40 SCHl, 128-byte aligned
  stream starts, SCEl closes each stream, hash matches.
- Import stages every song in its own slot. The preview lists what each slot
  currently has and what it becomes. Apply is one rebuild.
- Export takes the selected slots' current blobs (cached, from a pack, or
  decoded from the disc) and the banner text.
- Legal note recorded here once: a pack carries audio. Sharing one is the
  user's choice; the launcher and the site host only the two project packs.

## 8. Soundtrack packages in the mod browser

Two `nsr.mod.v1` packages, `conflict_group: "soundtrack"`, `targets: []`
(no texture payload, so composition ignores them), `kind: "soundtrack"` and
`category: "music"` in the manifest for the store:

- `reborn.soundtrack.original`, one file `pack.json` with `songs: {}`.
  Enabling it sets the base to stock.
- `reborn.soundtrack.nfl-street-1`, one file `nfl-street-1.nsrmusic` with
  14 songs in slots 1..14, titles from the NFL Street 1 EATX table.
  Enabling it sets the base to that pack.

`mods_toggle` on a soundtrack package: enforce the exclusive group as today,
then set `soundtrack.base` and run Apply (or the stock path) under the
launch guard, and answer with the rebuild result. The card shows "Rebuilding
your game" while it runs and "Enabled" after. User slots on the Music page
stay on top of the base. Reset on a card removes the override.

Authoring: `rebornctl music pack-from-street1 --disc <extracted NFL Street 1
files dir> --out nfl-street-1.nsrmusic` decodes the 14 streams, resamples
32 kHz to 24 kHz, encodes each into its slot, and copies the titles.
Delivery: both packages are published into the `mods` content component as
release sequence 4. If that component's tree turns out not to be the
packages tree, they ship as a new `soundtracks` component with the same
promotion rules; decided when the publisher is run.

## 9. Song details

Order, with the user able to change anything at every step:

1. Embedded tags (lofty). Title, artist, album, and length.
2. If any of title or artist is missing: clean the file name (drop leading
   track numbers, bracketed and parenthesized noise, "official", "lyrics",
   "audio", underscores) and look it up. iTunes Search
   (`itunes.apple.com/search`, media music, entity song, limit 8) first;
   MusicBrainz recording search when iTunes has no result. Both through
   the `Http` trait with the launcher's user agent. The best match is
   pre-filled and marked "found online".
3. A "Not right?" search field runs the same lookup on typed text and lists
   the candidates.

The dialog says the lookup goes online. No lookup runs without a file being
added or a search being typed. Lookups are best effort: a network failure
leaves the fields empty with a one-line note, never an error dialog.

## 10. Launcher wiring

Commands, in `crates/reborn-ui/src/music_page.rs`, state held in a static
mutex like `content_updates.rs`:

- `music_status` phase idle, encoding, building, done, problem; effective
  slots; pending; base; enabled; last note.
- `music_inspect` (path) tags, length, cleaned query, lookup result.
- `music_lookup` (query) candidates.
- `music_stage_file` (path, slot, title, artist, album), `music_stage_stock`
  (slot), `music_stage_details` (slot, title, artist, album), `music_unstage`
  (slot).
- `music_pack_preview` (path), `music_stage_pack` (path), `music_export`
  (path, slots, name, author, description).
- `music_set_enabled` (bool) and `music_apply` (confirm), both under the
  launch guard, both refusing while a session is active.

Page: `crates/reborn-ui/ui/music.html` and `music.js`, built from the store's
card grid, section tabs, and dialog, with the shared stylesheets. A Music
entry joins the inline nav on every shell page. `Tools/check-music-page.mjs`
loads the page with full, null, and rejecting answers like
`check-shell-pages.mjs`, and music.html joins that checker's page list.

CLI, `rebornctl music`: `status`, `inspect <file>`, `lookup <query>`,
`add <file> --slot N [--title --artist --album]`, `reset <slot>`,
`details <slot> …`, `import <pack>`, `export <out> --slots …`, `enable`,
`disable`, `apply`, `pack-from-street1`, and `write-tree --tree <root>`
which writes the staged slots into a root tree's pathfind.dat and
template.dat and stops, so `build_iso_fields.py` stays the dev disc builder.

## 11. Refusals and errors

Refusals are states with a sentence, not errors: session active, image
locked, song too long, unsupported format, pack invalid, slot out of range,
source or pack missing at apply time, no pristine image, online build
selected. Every message names what to do next. Real failures (I/O, a
truncated ISO, a hash that does not match after the swap) surface inline on
the page and in the log, never as a popup.

## 12. Verification

Byte-level oracles, run before any listening:

1. `gcm` repack of the FIELDS 12-19 disc with no changes is byte-identical.
2. Apply of a pack through Rust yields the same pathfind.dat and template.dat
   as `music_pack.py import` on the same inputs.
3. Encoding a 24 kHz stereo WAV yields the same 40-segment blob as
   `extend_slot_40big.py`.
4. Decoder round trip on a stock slot and on an NFL Street 1 stream.

Then `cargo test --workspace`, `cargo clippy --workspace --all-targets`, the
checkers, `Tools/test-all.ps1`, a real-window run on a scratch data root,
and one in-game boot by Adam of a custom song and of the NFL Street 1 pack:
banner text, full play, clean advance. Ship with `build.ps1 -LauncherOnly`,
latest.json on the bucket, and content release sequence 4.

Work happens in a git worktree; another agent edits the main tree.

## 13. Timeline and cut lines

- 09-09: gcm, pathfind, asf, eatx, pack read, apply from pack, the switch,
  CLI. Oracles 1 and 2.
- 09-10: ea_xa, segments, decode, tags, lookup, NFL Street 1 pack. Oracles
  3 and 4.
- 09-11: Music page, soundtrack packages in the Mods page, replay hook, Adam's
  boot test.
- 09-12: fixes, ship before 17:00 ET.

If 09-10 slips, pack import, the switch, and the two soundtrack packages
still ship; adding songs from files follows in the next update.
