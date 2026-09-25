# Online and Results Protocol

> **Precedence.** Where this document disagrees with
> [`RECONCILIATION_RULINGS.md`](./RECONCILIATION_RULINGS.md), **the ruling wins.**
> Five auditors found 172 internal contradictions across this document set and an
> adjudicator issued one ruling per conflict. The rulings are applied to this file
> progressively; until that is finished, read the rulings first for anything
> touching safety, verdicts, schemas or identifiers.

## NFL Street Reborn Launcher

**Date:** August 29, 2026
**Covers:** `CLAUDE_START_HERE.md` step 6, and questionnaire areas 13, 15, 16, and 17
**Depends on:** [`SYSTEM_ARCHITECTURE.md`](SYSTEM_ARCHITECTURE.md), [`DOLPHIN_INTEGRATION.md`](DOLPHIN_INTEGRATION.md), [`COMPONENT_MAP.md`](../discovery/COMPONENT_MAP.md), [`INFORMATION_ARCHITECTURE.md`](../design/INFORMATION_ARCHITECTURE.md), [`WIREFRAMES.md`](../design/WIREFRAMES.md)

This document answers one question: **when two people finish a ranked game, how does the system know who won, and how much of that answer can it prove?**

Everything else here (queue, lobby, chat, presence, preflight, Elo) is scaffolding around that question. The honest headline is that the winner chain is currently **three working halves that are not connected to each other**, and closing it needs new fork surface, one new measurement, and a backend that does not exist yet.

**How to read this document.** Section 1 corrects five statements in earlier documents, two of which are new in this revision. Sections 4.1, 4.2, 5.4, 7.4, 10.2, 10.4, 11 and 12 are grounded in files that were opened and parsed, and they say which file and which line. Sections 4.3, 5, 7.2, 9.5, 9.6, 10.1 and the account and retention rules in 6.4 are **proposals for things that do not exist**, and each one says so at its head. Where a fact could not be established from a file, the text says "unverified" rather than guessing.

---

## 1. Corrections to earlier documents

Written first, because five earlier statements would mislead anyone implementing from them.

| Document | Statement | Correction | Evidence |
|---|---|---|---|
| `COMPONENT_MAP.md` section 6 | "The bottom three links are **built and working**" | The bottom three links are three working halves with no joints. **`NflsPlayHide` exports nothing about identity.** Its header declares exactly two functions, `Update()` and `MaskSide()`, and neither returns a port, a player id, or a team; the resolved port and team are frame-scoped locals that are discarded. `NflsStatTracker` emits no port, no NetPlay player id, and no side ownership | `NflsPlayHide.h` declares `void Update();` and `int MaskSide();` and nothing else; the only NetPlay symbol in `NflsStatTracker.cpp` is `IsNetPlayRunning()` |
| `DOLPHIN_INTEGRATION.md` section 7 phase 3 | Rebase is risky because "the fork carries a custom fixed-delay NetPlay path" | **False for the selected fork.** Preservation commit `eca379f` touches no NetPlay source file, and "Fair Input Delay" is stock upstream Dolphin's menu label for its own `fixeddelay` network mode. The genuinely custom lockstep transport lives in a different, unpreserved tree | `NetPlayDialog.cpp` line 184; `NetplaySettings.cpp` line 56; `git show --stat eca379f` |
| `DOLPHIN_INTEGRATION.md` section 9 | "Fair Input Delay NetPlay: Preserved" listed as a Reborn behavior | It is upstream behavior, preserved by not touching it. Nothing to regress and nothing to credit | same |
| `PROJECT_QUESTIONNAIRE.md` 17.9, labeled "Research, ANSWERED" | "Production is the in-emulator `NflsStatTracker`. The `Stat Tracking\Start.dol.statread1` DOL hook is a separate research path needing a modified `Start.dol` and an ISO rebuild. Do not confuse them" | **The opposite is true, and this is the most consequential wrong statement in the earlier documents, because it makes an ISO rebuild look optional.** They are one pipeline, and the DOL hook is a hard dependency of production capture. `NflsStatTracker` reads the instruction at the hook site, rejects anything that is not a branch, and derives the stat ring's write pointer and buffer address from the `lis`/`ori` pairs inside the cave the branch targets. No hook means no `players[]` at all. See section 10.2 | `NflsStatTracker.cpp` line 37 `constexpr u32 HOOK_SITE = 0x8005A960;`; line 107 `const u32 instr = Memory::Read_U32(HOOK_SITE);`; line 108 `if ((instr >> 26) != 18)` returns empty; line 113 `const u32 cave = HOOK_SITE + li;` |
| `SYSTEM_ARCHITECTURE.md` section 2 and 5.6, `DOLPHIN_INTEGRATION.md` section 9 | `NflsStatTracker` is "**Reuse unchanged**" and "Stat capture: Preserved unchanged" | **This document requires a source change to it**, so those three lines are now out of date. Section 6.1 adds one additive top-level `reborn_match` key, which is new emission code in `NflsStatTracker.cpp`. The schema string stays `nsr.game.v1`, every existing key keeps its meaning and position, and the hashed set is untouched, so the correct verdict is **WRAP, additive**, not unchanged. Any plan built on "no Dolphin source change is needed for capture" is wrong | `SYSTEM_ARCHITECTURE.md` line 67 and line 206; `DOLPHIN_INTEGRATION.md` line 245; this document section 6.1 and section 13 |

The rebase risk is real but it is attached to the wrong repository. **The correct reason to defer a rebase is `NflsUiSplit` and the ExGL renderer changes, not netcode.**

---

## 2. Responsibility split

Gameplay is peer to peer. The backend never sees a controller input, a frame, or a game packet.

```text
  ┌── RAILWAY BACKEND ─────────────┐        ┌── DOLPHIN NETPLAY (P2P) ──────┐
  │ Discord OAuth, own JWT         │        │ traversal / direct connect    │
  │ queue, lobby, chat             │        │ pad mapping, join order       │
  │ match context issue + nonce    │        │ SyncIdentifier disc compare   │
  │ host-code relay                │        │ fixed-delay input lockstep    │
  │ result adjudication            │        │ halt on divergence            │
  │ Elo, ladder, moderation        │        │ knows nothing about accounts  │
  └───────────────┬────────────────┘        └───────────────┬───────────────┘
                  │ JSON over WSS / HTTPS                   │ UDP peer to peer
                  ▼                                         ▼
  ┌── LAUNCHER (process A) ────────────────────────────────────────────────┐
  │ holds the JWT in the OS credential vault                               │
  │ writes the match context file, generates Dolphin config, boots Dolphin │
  │ tails the session event stream, relays the host code                   │
  │ reads the capture, builds the submission, uploads it                   │
  └────────────────────────────────────────────────────────────────────────┘
```

**The backend is the referee, never a participant.** It holds the nonce, it holds both submissions, and it is the only thing that can compare them. It cannot observe the game, so every fact it asserts about the game came from a client and must be corroborated by the other client or held.

| Concern | Owner | Why not elsewhere |
|---|---|---|
| Account identity | Backend | Discord token must never leave the server (the prototype already gets this right) |
| Matchmaking, Elo | Backend | Needs both players and durable state |
| Transport, sync, input | Dolphin NetPlay | Already exists, already correct, and is the part nobody should rewrite before September 12 |
| Match context delivery | Launcher | Dolphin has **no NetPlay CLI surface at all**. The only channels the launcher can use at launch are `-u` (user folder), `-C` (config overrides) and the config files inside that folder. The full accepted flag list is in 4.1 |
| Result upload | Launcher and supervisor | Survives a game crash; makes the credential rotatable |
| Game memory capture | Dolphin (`NflsStatTracker`) | The knowledge of where the score lives is in that file and nowhere else |

---

## 3. The six-link identity chain

The chain that has to hold for "this Discord account won 36 to 24" to be true.

```text
  1  Discord account
        │  backend exchanges the code server side, issues its OWN JWT
        ▼
  2  matchmaking participant  (seat 0 / seat 1 under a match id)
        │  ??? NOTHING BINDS THESE TODAY
        ▼
  3  NetPlay player id        (PlayerId u8, host is always pid 1)
        │  NflsPlayHide does this, inside one function
        ▼
  4  local controller port    (NetPlay_InGamePadToLocalPad(p) < 4)
        │  NflsPlayHide does this, inside the same function
        ▼
  5  in-game team             (*(*(0x803EAB60)) + port*0xC  ->  0 or 1)
        │  ??? NEVER MEASURED AGAINST teams[]
        ▼
  6  final score              (NflsStatTracker teams[2]{name, score})
```

| Link | State | What exists | What is missing |
|---|---|---|---|
| 1 | **Design proven, not deployed** | Prototype `routes/auth.js`, scope `identify` only, server-side code exchange, own JWT issued | The production backend |
| 2 | **Prototype only** | `socket/queue.js` creates a match and assigns `role: 'host' \| 'joiner'` | Postgres schema, seat concept, nonce |
| 2 -> 3 | **Nothing** | The only per-participant string NetPlay carries is the nickname (`NETPLAY_NICKNAME`, `Player::name`) | The binding itself. See section 5.3 |
| 3 | **Exists, upstream** | `PlayerId` is `u8`; `PadMappingArray` is `std::array<PlayerId, 4>`; `AssignNewUserAPad` gives the first free slot in join order; `IsHost()` is `pid == 1` | Nothing, but it is not observable from outside Dolphin |
| 3 -> 4 | **Exists** | `NflsPlayHide.cpp` loops `p` in `{0,1}` calling `CSIDevice_GCController::NetPlay_InGamePadToLocalPad(p) < 4` | The value is a local and is thrown away every frame |
| 4 -> 5 | **Exists** | `local_team = u8[ *(*(0x803EAB60)) + port*0xC ]`, verified in source, gives 0 or 1 | Same: local and discarded |
| 5 -> 6 | **Never measured** | `NflsStatTracker` derives `side` as entity index / 7 across 14 entities at `ENTITY_ROOT 0x803EA3F8`, stride `0x14D8`, and that side flag is **measured as inverted** relative to `teams[]` (default map is `map0=1, map1=0`, not identity) | The mapping from the play-call registry's team value to the tracker's `teams[]` index. **This one measurement is what closes the chain** |
| 6 | **Exists** | `teams[2]{name, score}`, `sides_verified`, `per_player_matched`, `players[]` | Nothing, but it carries no identity at all |

**The specific missing measurement, stated so it can be done in one sitting.** `NflsPlayHide` reads a play-call registry record; `NflsStatTracker` reads the entity array. They are different structures. Knowing "I am team 1" does not yet tell you whether team 1 is `teams[0]` or `teams[1]`. The test is a single two-instance NetPlay game where each side records its own registry team value alongside the final `teams[]` array, run at least twice with the team picks swapped so a coincidence cannot pass.

### 3.1 What the chain cannot do, even when closed

**Play-call hiding is presentation only and runs on the client.** `NflsPlayHide` writes no emulated memory by design, which is exactly what makes it sync-safe, and also exactly what makes it defeatable: a modified client simply does not draw the mask. The runtime hash check in preflight catches an unmodified user running the wrong build; it does not catch a determined one. **Ranked integrity rests on the ladder, on both-client corroboration, and on moderation, not on the emulator.** Saying otherwise in the UI would be a lie.

---

## 4. How NetPlay is actually driven

### 4.1 What exists today, verified in the selected fork

`CommandLineParse.cpp` `CreateParser()` accepts exactly:

```
-u --user            -m --movie           -e --exec
-n --nand_title      -C --config          -s --save_state
GUI only: -d --debugger  -l --logger  -b --batch  -c --confirm
always:   -v --video_backend  -a --audio_emulation
```

That list is the whole parser. Note that `-C` is `append`, so it may be repeated, and that `-d`, `-l`, `-b` and `-c` exist only when the parser is built with `ParserOptions::IncludeGUIOptions`. Verified against `Source/Core/UICommon/CommandLineParse.cpp` lines 83 to 121 in the selected fork.

**There is no flag that opens, hosts, joins, or starts a NetPlay session.** `-C <System>.<Section>.<Key>=<Value>` is the only NetPlay surface the launcher can touch at launch, and it reaches every `[NetPlay]` key. The complete set, taken from `Source/Core/Core/Config/NetplaySettings.h` rather than from memory, is 29 keys:

```
TRAVERSAL_SERVER   TRAVERSAL_PORT     TRAVERSAL_CHOICE   HOST_CODE
INDEX_URL          HOST_PORT          ADDRESS            CONNECT_PORT
LISTEN_PORT        NICKNAME           USE_UPNP           ENABLE_QOS
USE_INDEX          INDEX_REGION       INDEX_NAME         INDEX_PASSWORD
ENABLE_CHUNKED_UPLOAD_LIMIT           CHUNKED_UPLOAD_LIMIT
BUFFER_SIZE        CLIENT_BUFFER_SIZE SAVEDATA_LOAD      SAVEDATA_WRITE
SAVEDATA_SYNC_ALL_WII                 SYNC_CODES         RECORD_INPUTS
STRICT_SETTINGS_SYNC                  NETWORK_MODE       GOLF_MODE_OVERLAY
HIDE_REMOTE_GBAS
```

**Three of those matter more than the transport keys and are easy to miss:** `SAVEDATA_LOAD`, `SAVEDATA_WRITE` and `SYNC_CODES`. A ranked build must pin all three explicitly in its generated block, because save-data sync redirects the memory-card path (`NetPlayConfigLoader.cpp` sets `MAIN_GCI_FOLDER_A_PATH_OVERRIDE` for non-hosting clients when `savedata_load` is on) and code sync would let a host push AR/Gecko codes to a peer.

Live values in the selected runtime's `Dolphin.ini`:

| Key | Value | Note |
|---|---|---|
| `TraversalChoice` | `traversal` | Depends on a third-party server today |
| `TraversalServer` | `stun.dolphin-emu.org` | Dolphin's public server, not ours |
| `TraversalPort` | `0x1876` (6262) | |
| `HostPort`, `ConnectPort` | `0x0a42` (2626) | |
| `BufferSize` | `3` | Static. Questionnaire 15.9 assumes ping-derived delay; **no such mechanism exists** |
| `NetworkMode` | `fixeddelay` | Upstream value, upstream label |
| `Nickname` | `YourName` | Default, never set by anything |
| `HostCode` | `1db46c2c` | **Stale.** This field is the joiner's target, not a generated host code |
| `UseIndex`, `UseUPNP` | `False` | |

### 4.2 The three candidate mechanisms, and why two are dead

| Candidate | Verdict | Reason |
|---|---|---|
| Port `netplay_uia.ps1` | **Dead** | 531 lines of Windows UI Automation with seven modes, driving `NetPlay Setup`, the `Connect`/`Host` tabs, and a separate `NetPlay` session window; it moves windows off-screen with `SetWindowPos` because minimizing renames them out of the UIA tree. AT-SPI and macOS AX are not equivalents, and macOS additionally requires a user-granted accessibility permission. This is Windows-only by construction |
| Use the newer copy's CLI flags | **Dead, and dangerous** | The `StreetLauncherProject` copy calls `--no-gui`, `--netplay-host`, `--netplay-traversal`, `--netplay-client`, `--netplay-host-code` and parses an `XXXX-XXXX` code from stdout. **None of those flags exists in `CommandLineParse.cpp`**, the code is 8 characters (`NETPLAY_CODE_SIZE = 8`), and it is never printed. Treating "newest copy" as "most correct" ships a provably dead path |
| New fork surface | **The only option** | Specified in 4.3 |

That copy's own author already wrote the reason down: "Mainline Dolphin has NO netplay CLI, so we drive the NetPlay dialog via [UI Automation] ... and, for hosting, reads the traversal host code straight out of the session window (it is NEVER written to Dolphin.ini)."

### 4.3 Proposed new fork surface: the NetPlay session driver

One new source pair, `Source/Core/Core/NflsSession.cpp/.h`, plus a small hook in `MainWindow` startup. **PROPOSED, not built.**

| Responsibility | Detail |
|---|---|
| Read the match context | Once at boot, from a fixed filename inside the per-launch user folder. See section 5 |
| Apply NetPlay settings | Host or join per the context's `role`, using values the launcher may also have set with `-C`. Belt and braces on purpose |
| Drive the session headlessly | No game list, no NetPlay browser, no dialog the user can interact with |
| Publish a session event stream | Append-only JSON Lines to `nsr_session.jsonl` in the same folder, flushed per line |
| Auto-start | Start the game when both peers are present, the pad map is settled, and `SyncIdentifierComparison` is `SameGame` |
| Classify the end | Exit code plus a final event line |

The event stream is the piece that does not exist in any form today and is the reason the host-code relay works at all:

```jsonl
{"t":"2026-09-12T18:04:03Z","e":"session_open","role":"host"}
{"t":"...","e":"host_code","code":"a1b2c3d4"}
{"t":"...","e":"peer_joined","pid":2,"nickname":"nsr:7f2a..."}
{"t":"...","e":"sync_identifier","comparison":"SameGame","game_id":"FR4N31"}
{"t":"...","e":"pad_map","map":[1,2,0,0]}
{"t":"...","e":"game_start"}
{"t":"...","e":"desync","frame":41207}
{"t":"...","e":"peer_left","pid":2,"reason":"disconnect"}
{"t":"...","e":"session_close","code":0}
```

`host_code` is load-bearing. **Verified: the 8-character traversal code reaches only a `QLabel` in the live session dialog, is never written to `Dolphin.ini`, and is never printed to stdout.** Without this line, hosting cannot be automated on any platform.

### 4.4 Host-code relay, with the deadlock already fixed

A real two-machine test on June 2, 2026 found a relay deadlock: the code was relayed with `socket.to(matchId)`, but players only joined the `matchId` room inside `match:ready`, which fires after Dolphin connects, so the room was empty when the host sent the code. **Both sockets must join the match room at match creation, before either client launches anything.** That fix is free and it must survive the rebuild.

```text
 host launcher            backend                joiner launcher
      │  (both already in room <match_id>)             │
      │ boot Dolphin (role host)                       │ waits, shows "Connecting"
      │ tail nsr_session.jsonl                         │
      │ ── host_code ──────────>│                      │
      │                         │── match:host_code ──>│
      │                         │                      │ boot Dolphin (role joiner)
      │                         │                      │ -C Main.NetPlay.HostCode=<code>
      │<──────── peer_joined ───┴──── peer_joined ────>│
```

### 4.5 Pad assignment: record it, do not fight it

NetPlay assigns pads in join order to the first free slot, and `SetPadMapping` on `NetPlayServer` is reachable only from the NetPlay dialog. The backend's `expected_pad` is therefore **an expectation, not an instruction**, in v1.

| Version | Behavior |
|---|---|
| v1 | Backend states the expected assignment. The driver **records the observed `pad_map`** into the session stream and into the capture identity block. Backend compares expected against observed and holds on mismatch |
| Later | Host-side driver calls `SetPadMapping` to enforce it. This is genuinely new fork surface and does not belong on the September 12 path |

---

## 5. The match context object

### 5.1 Shape, PROPOSED

**This entire section is a proposal, not a specification of anything that exists.** No part of this object, no reader for it, and no enforcement of any field is present in either Dolphin tree, in the prototype backend, or in the launcher. The "Enforced by" column below states where enforcement **would** live if this proposal is accepted; read it as a design assignment, not as a description of running code. Where a mechanism does not exist at all today, the row says so.

```json
{
  "schema": "nsr.match.v1",
  "match_id": "01J9Z8K2Q7X4T5V6W8Y0A1B2C3",
  "nonce": "b64url-32-bytes-unique-per-match-per-client",
  "mode": "ranked",
  "issued_at": "2026-09-12T18:03:41Z",
  "expires_at": "2026-09-12T18:18:41Z",
  "participants": [
    { "seat": 0, "display": "Adam",  "role": "host",   "expected_pad": 0, "nickname": "nsr:7f2a41c9" },
    { "seat": 1, "display": "Rival", "role": "joiner", "expected_pad": 1, "nickname": "nsr:0c93be55" }
  ],
  "local_seat": 0,
  "expect": {
    "image_sha256": "<sha256 of NSRONLINE image>",
    "game_id": "FR4N31",
    "runtime_version": "<semver + build hash from runtime/version.json>",
    "netplay_config_version": 1
  },
  "netplay": {
    "transport": "traversal",
    "traversal_server": "stun.dolphin-emu.org",
    "host_code": null,
    "network_mode": "fixeddelay",
    "buffer_size": 3,
    "strict_settings_sync": true
  },
  "sig": "<detached signature over the canonical form of every field above>"
}
```

Every field is new. **Nothing in the current match-found payload, the capture schema, or the prototype database has any of it.** Today's payload is exactly `{matchId, role, opponentId, mode}`.

| Field | Purpose | Proposed enforcement point | Exists today? |
|---|---|---|---|
| `match_id` | Joins two submissions to one match | Backend rejects a submission whose match id it never issued | No. The backend does not exist |
| `nonce` | Proves the submission came from a client that received this context | Backend holds the expected nonce per seat; one use only | No |
| `mode` | Selects the rating path and the hold policy | Backend | Only as the prototype's `mode` string on a match row |
| `participants[].nickname` | The one string NetPlay carries per participant. See 5.3 | Session driver sets `NETPLAY_NICKNAME`; peer's value is recorded, not trusted | `NETPLAY_NICKNAME` exists upstream. Nothing sets it, and no driver exists |
| `expected_pad` | Cross-check against the observed pad map | Backend compares, holds on mismatch | No. Nothing observes the pad map from outside Dolphin |
| `expect.image_sha256` | Both clients ran the same online image | Launcher checks its own image before boot; peers additionally compare via `SyncIdentifier` | `SyncIdentifier` exists upstream. The launcher-side full-file check is new |
| `expect.runtime_version` | Both ran a supported runtime | Launcher checks, backend records | No. `runtime/version.json` is itself proposed |
| `expect.netplay_config_version` | Both ran the same generated NetPlay settings | See 5.4 | No, and 5.4 explains why it is currently unverifiable |
| `issued_at` / `expires_at` | Bounds the window a stale context can be replayed in | Driver refuses an expired context | No |
| `sig` | Distinguishes a backend-issued context from a hand-written file | Would require a signing key on the backend and a verifying key compiled into the fork. **Neither exists in either tree, and no signature verification code exists anywhere in Reborn source.** Until that is built, `sig` is an unenforced field | **No.** Treat this row as the weakest one in the table |

**`sig` is not load-bearing for integrity in v1, and the design must not lean on it.** The reason forging `nsr_match.json` buys an attacker nothing is the one given in 5.2: the backend only accepts submissions carrying a match id and nonce it issued itself. That argument holds with or without a signature. `sig` is a defense-in-depth field that makes a forged context fail earlier and more legibly, and it should be marked optional in v1 rather than blocking the ship.

### 5.2 Delivery, and why no token is ever passed

```text
  Discord access token ──> stays on the backend. Never leaves it. (already true in the prototype)
  Backend JWT          ──> launcher only, held in the OS credential vault, never written to disk in plaintext
  Match nonce          ──> the ONLY secret Dolphin ever sees, worth nothing outside this one match
```

**Delivery mechanism, PROPOSED:** the launcher writes `nsr_match.json` into the per-launch Dolphin user folder and passes that folder with `-u`. Dolphin reads it once at boot and never re-reads it. The launcher deletes it after the process exits.

Why a file and not a new CLI flag:

- `-u` already exists and is already how every existing driver launched Dolphin.
- The launcher already generates config per launch rather than editing in place (`DOLPHIN_INTEGRATION.md` section 6), so a per-launch folder is not new machinery.
- Reading a named file from the user folder is the pattern already used by `NflsPlayHide` and `NflsStatTracker`.
- A command line is visible in the process table on every platform. A file in a user-owned folder is not better secured, but it is not broadcast either.

**This is a data channel, not a permission channel, and that distinction is the whole reason it is safe to use a file.** The existing marker files (`nsr_track_offline.txt`, `playhide_test.txt`) are hazardous precisely because they are permission channels that a user can forge by touching an empty file. A forged `nsr_match.json` produces a match id the backend never issued, so the resulting submission is rejected on arrival. Forging it buys nothing.

### 5.3 Binding a NetPlay participant to an account

**Unresolved in the sense that no code exists, decided here as a proposal.**

The only per-participant string NetPlay carries is the nickname. The proposal is that the launcher sets `NETPLAY_NICKNAME` to an opaque per-match handle of the form `nsr:<8 hex>`, derived by the backend, not to the Discord username.

| Question | Proposed answer |
|---|---|
| What does the opponent see? | The opaque handle in Dolphin's own UI, which the user never sees, and the real display name in the launcher lobby, which they do |
| Why not the real name? | Nicknames are peer-visible and unauthenticated. A display name in that field invites impersonation and leaks identity to anyone who joins a session |
| What if the user edits it in Dolphin directly? | The driver overwrites it from the match context at boot. If the observed peer nickname does not match the context, the session driver records it and the backend holds the result |
| Is this authentication? | **No.** It is a label that makes a mismatch detectable. Nothing about NetPlay authenticates a peer |

### 5.4 Image and runtime verification

| Option for `image_sha256` | Verdict |
|---|---|
| Full-file SHA-256 computed by the launcher | **Recommended.** Stable, reproducible, already how the content manifest works, and independent of the emulator |
| Dolphin's `SyncIdentifier.sync_hash` | **Do not bind the backend to it.** It is a 20-byte peer-comparison hash, not a full-file hash, and its own source comment says "The way the hash is calculated may change with updates to Dolphin." Coupling a backend contract to an emulator implementation detail is a future outage |

Use both, for different jobs: the launcher's full-file hash satisfies the backend, and Dolphin's own `SyncIdentifierComparison` (`SameGame`, `DifferentHash`, `DifferentDiscNumber`, `DifferentRevision`, `DifferentRegion`, `DifferentGame`, `Unknown`) still guards the peer-to-peer start and is recorded in the session stream.

`netplay_config_version` is an integer the launcher increments whenever its generated `[NetPlay]` block changes. It is written into the context, echoed into the submission, and compared across both submissions.

**Correction on what `STRICT_SETTINGS_SYNC` actually does, verified in the selected fork's source.** It is a **convergence** switch, not a gate. When the host has it on, `ConfigLoaders/NetPlayConfigLoader.cpp` line 98 enters `if (m_settings.strict_settings_sync)` and **calls `layer->Set(...)` to push the host's values onto every client**: `GFX_HACK_VERTEX_ROUNDING`, `GFX_EFB_SCALE`, `GFX_HACK_COPY_EFB_SCALED`, `GFX_FAST_DEPTH_CALC`, `GFX_ENABLE_PIXEL_LIGHTING`, `GFX_WIDESCREEN_HACK`, the enhancement block, and a forced `GFX_MSAA = 1` with `GFX_SSAA = false`. There is **no refusal-to-start path anywhere**: `strict_settings_sync` appears in only five places in `Source/Core/Core/` and all five are set, serialize, or deserialize. Dolphin's own menu tooltip in `NetPlayDialog.cpp` line 176 says the same thing: "This will sync additional graphics settings, and force everyone to the same internal resolution."

The consequence for this document is direct. **Turning `STRICT_SETTINGS_SYNC` on makes a settings mismatch invisible rather than fatal**, because the client is silently overwritten instead of stopped. It is still worth forcing on, for determinism, but it provides **zero evidence** that a client used the launcher-generated block. **How to verify that both clients actually used the launcher-generated block, rather than a user-edited one, is unsolved**, and section 5.1's `netplay_config_version` row is an unenforced echo until it is. The only honest v1 position is that both clients self-report the version, the backend compares the two self-reports, and a mismatch holds; a client that lies about it is not detectable by any mechanism that exists.

---

## 6. The completed-result record

Three records, in the order they come into existence.

### 6.1 Capture, written by Dolphin

`nsr.game.v1` changes **additively and in exactly one way**: one new top-level key. The schema string stays `nsr.game.v1`, no existing key is renamed, removed, reordered or given a new meaning, and the hashed set is untouched. That is a source change to `NflsStatTracker.cpp`, so the verdict is **WRAP, additive**, and the earlier documents that call this component "reuse unchanged" are corrected in section 1.

The existing emission order, unchanged: `schema`, `source`, `captured_at` (UTC, stamped at game end, not upload time), `teams[2]{name,score}`, `style[2]`, `team_totals[2]` (emitted conditionally, `NflsStatTracker.cpp` line 709), `sides_verified` (emitted unconditionally in the current source, line 724), `per_player_matched`, `players[]{name, team, side, slot, digp, stats{12}}`.

**A consumer must still tolerate `sides_verified` being absent.** One capture on disk predates that unconditional emission and has no such key at all; see 7.4. Treat a missing `sides_verified` exactly as `false`, never as true.

The 12 per-player columns, with the raw four-character codes the DOL hook logs, and **none of them is yardage**:

| Column | 4CC | Column | 4CC |
|---|---|---|---|
| `pass_td` | `dtag` | `forced_fumbles` | `fflg` |
| `rush_td` | `dtug` | `fumble_rec` | `rflg` |
| `rec_td` | `dtcg` | `tackles` | `atdg` |
| `pick_six` | `tisg` | `sacks` | `kslg` |
| `fum_td` | `tflg` | `thrown_int` | `niag` |
| `interceptions` | `nisg` | `fumbles` | `ufug` |

The 15 team-total categories are `score, style, total_td, rush_td, pass_td, def_td, sacks, safeties, int, ff, first_downs, gamebreakers, gb_2pt, hot_spots, credits`.

**The one addition, and where it may safely go.** The identity block is a new top-level key, `reborn_match`, alongside `schema` rather than inside `teams`, `style`, `team_totals` or `players`:

```json
"reborn_match": {
  "match_id": "01J9Z8K2Q7X4T5V6W8Y0A1B2C3",
  "local_seat": 0,
  "observed_pad_map": [1, 2, 0, 0],
  "local_pad": 0,
  "local_registry_team": 1,
  "local_team_index": 1,
  "netplay_running": true,
  "runtime_version": "<semver + build hash>",
  "netplay_config_version": 1
}
```

Two reasons this placement is not arbitrary:

1. **The two clients disagree about it on purpose.** Each knows only itself, so each writes a different `local_seat` and `local_pad`. The league site's dedupe hash `si_capture_hash` is a SHA-256 over only `{teams, style, team_totals, players sorted by digp with each stats map ksorted}`, deliberately excluding `captured_at`, `source`, `schema` and path, so that both NetPlay clients' copies of one lockstep game hash identically. **An identity block inside the hashed set would destroy that property.** Outside it, it costs nothing.
2. The existing ingest endpoint validates that `schema` is exactly `"nsr.game.v1"`, that `players` is non-empty, and that there are exactly 2 teams. An additive top-level key passes all three. That is **inferred from reading `ingest.php`, not tested against the live endpoint**, and must be tested before the first ranked build.

`local_team_index` is the field that only exists once the section 3 measurement is done. Until then the fork should emit `local_registry_team` and omit `local_team_index` entirely rather than guess it.

### 6.2 Submission, built and sent by the launcher

```json
{
  "schema": "nsr.submission.v1",
  "match_id": "...",
  "seat": 0,
  "nonce": "...",
  "capture_hash": "<canonical hash, computed exactly as si_capture_hash>",
  "capture": { "...the full nsr.game.v1 object..." },
  "claim": { "winner_seat": 0, "seat0_score": 36, "seat1_score": 24 },
  "session": {
    "host_code_seen": true,
    "sync_comparison": "SameGame",
    "observed_pad_map": [1, 2, 0, 0],
    "peer_nickname": "nsr:0c93be55",
    "desync_frame": null,
    "exit": "clean",
    "exit_code": 0
  },
  "client": { "launcher_version": "...", "runtime_version": "...", "platform": "windows" },
  "submitted_at": "2026-09-12T18:41:07Z"
}
```

`claim.winner_seat` is **derived by the launcher from `reborn_match` plus `teams[]`, not typed by the user.** The prototype's winner was entirely self-reported through a single "Game Over - Report Result" button and a manual pick; no capture data, score, or game memory touched the result path. That is the specific thing this document exists to replace.

### 6.3 Match result, stored by the backend

The prototype's `matches` table stores **no scores at all**, no nonce, and no reference to any capture. The league's `stat_imports` row stores fixture columns (`game_id`, `series_game_no`, `home_team_id`, `away_team_id`) that do not apply to a ladder. Neither shape fits, so this is new:

```sql
match_result
  match_id          text primary key
  mode              text     -- casual | ranked | private
  seat0_user_id     bigint
  seat1_user_id     bigint
  status            text     -- pending | accepted | held | void | forfeit
  reason            text     -- machine-readable hold or void reason, nullable
  winner_seat       smallint null
  seat0_score       int null
  seat1_score       int null
  capture_hash      text null    -- null until at least one submission arrives
  seat0_submission  uuid null
  seat1_submission  uuid null
  rated             boolean default false
  created_at        timestamptz
  resolved_at       timestamptz null
```

Submissions are stored whole, one row per seat, and never overwritten. **A held match keeps both submissions verbatim, because the moderator's job is to read the disagreement, not a summary of it.**

### 6.4 Visibility, retention, and account actions

The three records above are the only place these questions can be answered, because they are the only place the data exists. Questionnaire 17.4 is **Decided** and this document must honor it; 17.10 and 13.6 are the parts that are still open or unwritten, and they are called out as such.

**Visibility, per questionnaire 17.4 ("Offline private by default. Ranked and casual results are public"). Decided, and here is what it means at the record level:**

| Record | Ranked and casual | Offline |
|---|---|---|
| `match_result` row (mode, seats, winner, scores, status) | **Public.** Visible on the ladder and on both players' profiles | Not applicable. Offline games never create a `match_result` row, because there is no match |
| `rating_event` row | **Public.** A ladder without visible movement cannot be audited by the people on it | Not applicable |
| The stored `nsr.submission.v1` envelope, including the full capture and the `session` block | **Not public.** It carries `observed_pad_map`, exit codes, desync frames and the peer nickname. Visible to the two participants and to moderators only | Not applicable |
| An offline `nsr.game.v1` capture | Not applicable | **Private by default.** It stays on the player's machine unless the player is in a league that has asked for it. See 10.1 for the routing rule that keeps this true |
| A held match, while held | The **existence** of a held match is visible to both participants. The disagreement itself is moderator-only until it resolves | Not applicable |

**Retention: questionnaire 17.10 is Open and this document does not close it.** What it can do is state the shapes retention has to act on, so that whoever decides has a complete list: `match_result` rows, `rating_event` rows, stored submission envelopes (the largest by far, capped at 512 KB each per 7.3), the `uploaded.txt`-equivalent ledger on the client, and chat (deferred separately by 15.13, see 9.3). **Proposed starting position, not decided:** ladder-visible rows are kept for the life of the account, submission envelopes are kept only as long as a result can still be disputed or reversed plus a moderation window, and everything else is shortest-lived. Marked as an open item.

**Account actions: questionnaire 13.6 marks "unlink, sign out, export, delete" as Decided and all required.** This document owes an answer for the four record types it defines, because 6.3's rule that a held match keeps both submissions verbatim collides directly with deletion.

| Action | Effect on the records in this document |
|---|---|
| Sign out | Discards the launcher's JWT from the OS credential vault. Touches no server record. The player leaves any queue |
| Unlink Discord | Removes the Discord identity binding on the backend. **Does not delete match history**, because the other participant's completed games would otherwise change retroactively. The account becomes unreachable rather than absent |
| Export | Returns everything keyed to the user: `match_result` rows where `seat0_user_id` or `seat1_user_id` is the user, all `rating_event` rows for the user, and **the user's own submission envelopes only.** The opponent's envelope is the opponent's data and is not exported to the other side, even though both describe the same game |
| Delete | **Proposed: anonymize, do not erase.** The user row is deleted and every reference in `match_result` and `rating_event` becomes a tombstone id. Scores, winners and rating deltas survive, because deleting them would silently rewrite an opponent's record and the ladder. The user's own submission envelopes, including the raw capture and the `session` block, are **deleted outright**, since they are the part that is personal rather than shared |
| Delete while a match is held | **The one genuinely hard case.** A held match needs both submissions verbatim for a human to read. Proposed: deletion is accepted immediately, the held match is resolved to `void` rather than adjudicated, no rating moves, and the envelopes are then deleted with the rest. Voiding is the only option that does not either block deletion on a moderator or keep personal data after a deletion request |

Both the delete policy and the void-on-delete rule are **proposals in this document, not decisions**, and they are listed in the open items.

---

## 7. Two-client confirmation, and the accept/hold/cancel/forfeit rules

### 7.1 The principle

**Agreement is accepted automatically. Everything else is held, and Elo does not move.** Conservative by default, per the blueprint and per `COMPONENT_MAP` section 6. The design cost of being conservative is that a lot of games may hold; see the open items, because nobody knows how many.

The prototype already has the right shape and the wrong content: each player submits a winner id, equal votes complete the match and move Elo in ranked, unequal votes set `disputed` and nothing moves. Both the REST route and the socket handler implement that rule, **and they do not agree on how the vote column is chosen, which is a real exploit; see section 12.** **Reuse the shape, replace the input with the capture.**

### 7.2 The decision table, PROPOSED

**This table is a proposal for a backend that does not exist yet.** Nothing in it is implemented. The "Backend action" column is a design assignment, and several rows are additionally blocked on measurements and mechanisms that do not exist: every row keyed on the session stream (crash, desync, disconnect, clean quit) depends on the exit-code classification that open item 7 says has never been enumerated, and every row keyed on `winner_seat` depends on the section 3 measurement. The rows keyed on `sides_verified`, on an empty `players[]`, and on the 36-point floor are the exception: those three are inherited from the league pipeline, are already implemented there, and are the only rules in this table with production evidence behind them.

| Situation | Both submissions | Backend action | Elo | User-facing state |
|---|---|---|---|---|
| Agreement | Same `capture_hash`, same `winner_seat`, both nonces valid, pad map matches expectation | **Accept** | Moves in ranked | Won / Lost, with the delta |
| Agreement, different capture hash | Same `winner_seat`, different `capture_hash` | **Hold.** Two different games or a decode difference | No | Held for review |
| Conflict | Different `winner_seat` | **Hold**, store both verbatim | No | "Held for review. You and your opponent reported different scores." |
| Absence | One submission inside the window, other never arrives | **Hold** at window close. Never auto-award | No | "We only got one report. Held for review." |
| Clean quit before the game | Session closes before `game_start` | **Cancel.** No match existed | No | "Match canceled. Nothing counted." |
| Crash | Supervisor classifies a non-clean exit, no capture | **Hold**, tagged crash | No | "The game crashed. Held for review." |
| Desync | `desync` event, or Dolphin halts on divergence | **Hold**, tagged desync. Never rate | No | "The game desynced. Nothing counted." |
| Disconnect mid-game | `peer_left` before a capture exists | **Hold**, tagged disconnect | No | "Your opponent disconnected at 24-16. Held for review. Nothing has changed yet." |
| Forfeit | Only reachable through moderation, never automatically | Moderator sets `forfeit` | Moves, recorded as a moderation event | Stated plainly with the reason |
| `sides_verified` false | Capture arrives with `sides_verified: false` | **Hold.** Real, already observed | No | "We couldn't confirm which player controlled which team, so this result isn't counted." |
| No `players[]` | Capture has an empty players array | **Reject the capture**, hold the match | No | Held, tagged missing box score. See section 10 |
| Winner score below 36 | Highest score is under `SI_TARGET_SCORE = 36` | **Hold** | No | Held, tagged incomplete game |
| No capture at all | The Restart-at-target case | **Hold**, and it cannot be recovered | No | Held, tagged no capture |

**The forfeit rule is a deliberate reversal of the prototype.** Today the remaining player alone decides: `accidental` voids the match, `malicious` immediately awards a forfeit win and moves Elo in ranked, with no corroborating evidence required. That is an exploit with a UI. **No automatic forfeit ships. A disconnect holds and a human decides.**

### 7.3 Hold rules inherited from the league pipeline

`SYSTEM_ARCHITECTURE.md` 5.6 correctly says `ingest.php` is out of launcher scope as a hosting decision. **That must not be read as "do not reuse the design."** The parts of the results problem that are already solved and battle-tested live there:

| Proven mechanism | Why the new backend needs it |
|---|---|
| Canonical `si_capture_hash` over only `{teams, style, team_totals, players}` | Makes both NetPlay clients' copies of one game hash identically. Without it, two-client confirmation cannot compare captures at all |
| Idempotency through a **UNIQUE key**, not a SELECT | On July 27, 2026 at 22:46:46 UTC both clients passed the dedupe SELECT and the loser died with an uncaught duplicate-key exception and a 500, which the uploader read as a failure and retried forever. The fix routes all inserts through a helper that turns MySQL error 1062 into a normal "duplicate" answer. **A new backend that re-derives this will re-derive the outage** |
| "review" is a **success** for the uploader | The capture is stored and must not be resent. Only an explicit failure warrants a retry |
| Prefix-tolerant team resolution | Team names arrive truncated at 14 characters because the in-game text descriptor has `maxlen 0x0E`. An ambiguous 14-character prefix must resolve to **nothing**, never to a coin flip |
| `SI_TARGET_SCORE = 36` as a **floor** | NFL Street 2 is first to 36 and the final score overshoots when the winning play exceeds the remaining gap. A high score **below** 36 signals the game did not finish normally. It never decides who won |
| Payload cap of 512 KB | Bounded input, cheaply |

Also inherited as a warning: the top-level `style[]` array is in the inverted side order, not `teams[]` order, so any box score must read style from `team_totals[i].style`.

### 7.4 The already-observed hold cases, and the full capture corpus

**There are six real captures on disk, not one.** The corpus lives at `C:\Users\Adam\nflstreet2_clean\DOLPHIN ONLINE TEST\CREATION LEAGUE AND NEW DOLPHIN\User\LeagueGameStats\`. All six were parsed, not skimmed:

| File | Captured | Teams and score | `sides_verified` | `team_totals` | Players | README verdict |
|---|---|---|---|---|---|---|
| `Falcons_vs_Chargers_001.json` | 2026-08-05T04:58:39Z | Falcons 36, Chargers 24 | **false** | **absent** | 14 | **Fails two of four** |
| `Outlaws_vs_Shadow_Knights_001.json` | 2026-07-28T00:41:31Z | Outlaws 6, Shadow Knights 0 | true | present | 14 | Passes all four |
| `Outlaws_vs_Street_Crusade_002.json` | 2026-07-28T00:43:33Z | Outlaws 6, Street Crusade 0 | true | present | 14 | Passes all four |
| `Outlaws_vs_Street_Crusade_003.json` | 2026-07-28T00:45:56Z | Outlaws 0, Street Crusade 8 | true | present | 14 | Passes all four |
| `Outlaws_vs_Street_Crusade_004.json` | 2026-07-28T00:46:32Z | Outlaws 0, Street Crusade 6 | true | present | 14 | Passes all four |
| `_previous\Tri_City_Wolfp_vs_Shadow_Knights_001.json` | 2026-07-27T07:10:18Z | Tri City Wolfp 0, Shadow Knights 6 | **key absent entirely** | present | 14 | Fails one of four, by absence |

All six are listed in `uploaded.txt`, so all six were sent to the live endpoint. There is more than one file because the README's own test procedure asks for three games back to back and says the earlier capture "was moved to `User\LeagueGameStats\_previous\` so the folder starts empty and the count is unambiguous. Nothing was deleted." The pass condition it states is "THREE .json files", each with `sides_verified: true`, a `team_totals` block, 14 players, and both team names plus the final score.

**So the capture pipeline does work, and the earlier framing of this section was wrong.** Four of the six pass every criterion the README sets. The correct reading is narrower and more useful:

1. **`Falcons_vs_Chargers_001.json` is the one real failure**, and it is the interesting one: `style [0,0]`, `sides_verified: false`, `per_player_matched: 14`, **no `team_totals` block**, and 14 player lines in which `side 0` is attributed to Chargers while `teams[0]` is Falcons. **Under the deployed rules it holds**, and `WIREFRAMES.md` section 12 already renders it as a real held row rather than a mockup of one. The design has to show the failing state because the pipeline really does produce it, on real hardware, as recently as August 5, 2026.
2. **`Tri_City_Wolfp_vs_Shadow_Knights_001.json` has no `sides_verified` key at all**, which is why 6.1 requires every consumer to treat a missing key as `false` rather than as a pass. It predates the unconditional emission at `NflsStatTracker.cpp` line 724.
3. **Every one of the six would still hold under the rules in 7.2**, but mostly for a different reason than the Falcons case. The four passing Outlaws games finished 6, 6, 8 and 6, all far below `SI_TARGET_SCORE = 36`, so the incomplete-game floor holds them. The `_previous` game finished 0 to 6 and holds on both the floor and the missing key. **A conservative-by-default policy holds six out of six real captures**, and that is the number worth taking seriously, not the pass rate of the README's own criteria.

`sides_verified` becomes true only when the per-player aggregate of `total_td`, interceptions, forced fumbles and sacks matches the `team_totals` block strictly one way (`NflsStatTracker.cpp` line 654, `cap->sides_verified = (straight != swapped)`). A tie proves nothing and leaves the measured inverted default in place with `sides_verified` false. **With no `team_totals` block there is no verification at all**, which is exactly the Falcons case.

**One caution about generalizing from this corpus.** Every capture carries `"source": "dolphin"` and nothing else, so the file itself does not record whether NetPlay was running. The surrounding evidence is nevertheless clear: the README next to them describes a single-machine test run alone with the netplay gate defeated by `nsr_track_offline.txt`, that file is still present in that runtime, and all six are short deliberate test games. **None of the six reached 36, and none is known to have been produced by the two-client path this document specifies** (unverified in the files, strongly indicated by the README). They are strong evidence about the capture code and weak evidence about the ranked hold rate. See open item 9.

---

## 8. Elo

Harvested from `server/elo.js`, which is verified: K = 32, starting rating 1000, standard logistic expectation with a 400 divisor, integer rounding, applied only when `mode == 'ranked'`.

Recorded per rating movement, one row per player per match:

```sql
rating_event
  id, match_id, user_id, seat
  rating_before   int
  rating_after    int
  delta           int
  k_factor        int
  expected        numeric
  cause           text   -- accepted | moderation_award | moderation_reversal
  applied_at      timestamptz
```

**Deltas are stored so a reversal is a compensating event, never a recomputation of the ladder.** If a held match is later approved, or an accepted one reversed, the backend appends a new `rating_event` with the inverse or the correction. Recomputing history would rewrite everyone who played since.

Undecided, and marked Open in questionnaire 16.7: provisional-player behavior, decay, rating floor, whether casual and ranked share a pool (**recommended: they do not**), and whether a moderator-awarded forfeit uses the same K.

---

## 9. Queue, lobby, chat, preflight, presence

### 9.1 Queue

Prototype behavior, verified: a 5000 ms tick over separate ranked and unrated arrays, `RANKED_RATING_WINDOW = 150`, `QUEUE_TIMEOUT_MS = 90000` after which any pair is allowed.

**Questionnaire 15.3 records "Region and ping first, then Elo" as Decided, and 15.9 records input delay chosen from an approved range based on ping as Decided. Neither has any basis in the harvested code.** `queue.js` has no region field, no ping field, and no ping measurement anywhere in the server or the client, and `BufferSize` is a static 3. Both are **entirely new engineering**, not configuration of existing behavior, and they should be scheduled as such.

Minimum honest v1: keep the rating window and the widening timeout, add a coarse region tag chosen by the player, and treat ping-derived buffer size as a post-launch feature rather than a shipped promise.

### 9.2 Lobby

Per `INFORMATION_ARCHITECTURE.md`, the lobby is level 3 and shows both players, ready state, chat, mode and a countdown. Private lobbies in the prototype use a uuid room id plus a 6-character uppercase base36 room code plus a password, and **the password is stored in plaintext**. That does not ship; store a hash or drop passwords in favor of the code alone.

Deep links are fixed already: `reborn://online/casual`, `reborn://online/ranked`, `reborn://join/<code>`, `reborn://match/<id>`, with the rule that a deep link may navigate and pre-fill but may never install, delete, queue, or spend.

### 9.3 Chat

**There is no chat of any kind in the prototype:** no schema table, no socket events, no client component. All of it is new.

Minimum viable and nothing more: lobby-scoped, server-relayed, per-message length cap, per-user rate limit, mute and block, and a report action that attaches the last N messages to a moderation record. Retention is deferred by questionnaire 15.13. Proposal: chat persists into the countdown as read-only, and is not carried into the match, because nothing renders it there.

### 9.4 Preflight

The four-check strip is already fixed by `INFORMATION_ARCHITECTURE.md`: always visible, never a late gate. Mapping to real checks:

| Check | Real test | Red means |
|---|---|---|
| Online build verified | SHA-256 of the online image against the signed manifest | Repair before queueing |
| Runtime version | `runtime/version.json` against the backend's minimum | Update before queueing |
| Controller connected | SDL3 device present and mapped | Connect a controller |
| Network readiness | Traversal server reachable, and the backend socket connected | Explain which one failed, separately |

### 9.5 Discord Rich Presence

Questionnaire 13.3 marks Rich Presence **Decided, Must-have**, with six states: "Menus, offline play, casual queue, ranked queue, private lobby, live match." **Four of those six are subsystems defined in this document**, so this document owes the mapping. Nothing about presence exists in any harvested code today: **the entire feature is new**, and everything below is PROPOSED.

**Which process owns the presence connection: the launcher (process A), and only the launcher.** Three reasons, in order of weight:

1. **Dolphin already publishes its own Discord presence, and it is on by default.** Verified in the selected fork: `Source/Core/UICommon/DiscordPresence.cpp` calls `Discord_Initialize(DEFAULT_CLIENT_ID...)` at line 192 and supports a custom client id at line 207, and `Source/Core/Core/Config/UISettings.cpp` line 10 declares `MAIN_USE_DISCORD_PRESENCE{{System::Main, "General", "UseDiscordPresence"}, true}`. Two processes publishing presence for one account produce a race whose winner is whichever connected to the local Discord IPC socket last. **The generated per-launch config must therefore set `-C Dolphin.General.UseDiscordPresence=False` for online launches**, or presence will flicker between two applications during a match. That is a concrete, cheap, verified item for the generated config block in 5.4, not a detail.
2. The launcher is the only process that knows the state. Dolphin knows nothing about accounts, queues, lobbies, or modes, by design (section 2). A match state published by Dolphin could say "in a game" and never "ranked queue".
3. Process A exits while a game runs (`SYSTEM_ARCHITECTURE.md`). **Presence during a live match must therefore be published by the supervisor (process B), not by A**, or presence goes blank for the entire match, which is the one state anybody actually wants to see. This is the single non-obvious consequence and it needs to be designed in, not discovered.

Proposed state mapping, with what each publishes:

| Questionnaire state | Owner | Published details | Never published |
|---|---|---|---|
| Menus | A | "In the launcher" | Anything about the library or installed content |
| Offline play | B | "Playing offline" | Team names, score, field. Offline is private by default per 17.4 and 6.4 |
| Casual queue | A | "In the casual queue", elapsed timer | Rating, opponent |
| Ranked queue | A | "In the ranked queue", elapsed timer | **Rating.** Publishing a number invites harassment and it is on the public ladder anyway |
| Private lobby | A | "In a private lobby", party size, and a **join secret carrying the room code** so Discord's Ask to Join can produce `reborn://join/<code>` (questionnaire 13.9) | The room password, which 9.2 says should not exist. The `match_id`, the nonce, and the JWT are never presence payloads |
| Live match | B | "In a ranked match" or "In a casual match", elapsed timer | Score, opponent name, and anything from the `session` block. A live score in presence is a spoiler and a harassment vector, and it is not needed for any decided requirement |

**Interaction with `reborn://join/<code>`, which is the part most likely to be got wrong.** Discord's join flow hands the invited client a join secret, and the launcher turns that into the same `reborn://join/<code>` deep link a pasted invite would produce. That means the deep link is the single entry point and the presence path adds no second one. Two rules from 9.2 apply unchanged and must not be relaxed for presence: **a deep link may navigate and pre-fill but may never install, delete, queue, or spend**, and a join secret is a room code, never a credential. A join secret that arrives for a room that is full, closed, or ranked resolves to the lobby screen showing why it failed, not to a queue action.

**Presence is optional and off-switchable.** It is a Discord connection made on the player's behalf from their own machine; it needs a visible toggle in settings, it must degrade silently when Discord is not running, and it must never block a launch, a queue, or a result upload.

### 9.6 Account actions in the online surface

The record-level answers for unlink, sign out, export and delete are in 6.4, because that is where the records are defined. Two consequences land in this section: signing out or unlinking **removes the player from any queue and closes any lobby they host**, and presence stops immediately on sign-out, since there is no longer an account to publish for.

### 9.7 Live now and the match view

"Live now" on the Online page and the native match view (`live.html`) read the backend's public `GET /v1/live` and `GET /v1/live/{id}` through the `live_matches` and `live_match` commands, from the configured backend origin. Their images and the **Watch on web** button do not follow that setting: logos, portraits and avatars load only from `https://api.nflstreetreborn.com/live-assets/` and `https://cdn.discordapp.com/avatars/` (the static CSP `img-src` allows nothing else), and **Watch on web always opens the production page, `https://api.nflstreetreborn.com/live/<id>`**, because `open_website` allows only that prefix. Against a self-hosted or test backend the lists and scores still work, the images fall back to badges, and Watch on web shows the production site's page for that id (which will not exist there). The origin is kept in one place, `crates/reborn-ui/ui/live-common.js`, and a Rust test holds it to `identity::DEFAULT_ORIGIN` and to the CSP.

Both pages poll only while visible and while no game session is running: the list every 10 s, the match every 3 s, one request at a time. Errors back off from 5 s up to 60 s with jitter, a 429 waits at least 60 s, and the last good rows or view stay on screen under a one-line note.

---

## 10. Stat pipeline split

```text
  DOLPHIN                              LAUNCHER / SUPERVISOR
  ───────                              ─────────────────────
  reads game memory                    reads the JSON file
  writes User\LeagueGameStats\*.json   owns the credential (OS vault)
  writes the reborn_match block        builds the submission
  writes nsr_session.jsonl             retries with backoff, survives a game crash
  knows nothing about HTTP             routes to one or two destinations
```

`NflsStatUpload` is **retired from the shipping binary**, which is also how the compiled-in bearer token stops shipping. The logic worth keeping is ported, not discarded: FNV-1a content hashing, the "unchanged for one full scan" settle rule, a durable ledger keyed on content hash so a rename does not resend, the Accepted / Transport / Rejected classification, and stopping after repeated hard rejections instead of hammering forever.

Two facts about the existing uploader that must survive the port:

- The override file is `nsr_ingest.txt` and it has **three lines**: endpoint URL, bearer token, and optional league id. Verified in `NflsStatUpload.cpp`, which documents the three lines in its header comment and reads `lines[0]`, `lines[1]` and `lines[2]` into `s_url`, `s_token` and `s_league_id`, with `DEFAULT_LEAGUE_ID = 8` (Creation League) as the fallback. **`COMPONENT_INVENTORY.md` line 217 describes it as two lines** ("a local override file can supply URL on line 1 and bearer token on line 2") and is wrong by omission. **`SYSTEM_ARCHITECTURE.md` does not describe the override file at all**: it mentions only that "the emulator posts to the league endpoint using a token compiled into the binary" (line 61), so it is incomplete rather than incorrect, and correcting it means adding the override file, not fixing a count. A rotation procedure written from the two-line description leaves the league id at the compiled-in default of 8.
- The `User-Agent` header is **not cosmetic**. Measured against the live site, no User-Agent gets a 403 from the host firewall before PHP is reached; any non-empty value gets a 200.

### 10.1 One capture, two consumers

The capture file currently serves the Creation League and would now also serve the ladder. They have different requirements: the league needs a fixture and a series game number, the ladder needs a match id and two seats.

**Proposal: one capture format, two envelopes, and the launcher decides which destinations apply.** The capture stays `nsr.game.v1` so the league endpoint keeps working unchanged; the ladder receives it wrapped in `nsr.submission.v1`. Nothing in Dolphin needs to know there are two consumers.

**How the launcher decides, and why "both" is the wrong default.** The identical `nsr.game.v1` file is produced by an offline CPU game, a Creation League game, and a ranked ladder game. The file carries nothing that distinguishes them: `source` is the literal string `"dolphin"` in all six captures on disk, and once the netplay gate becomes a compiled-in setting rather than a defeatable marker file, that does not change. **The routing decision therefore cannot be made by inspecting the capture. It has to be made from what the launcher knows about the launch it started**, and the rule has to be an allowlist for the same reason the marker rule does in 10.4:

| The launcher started | Ladder destination | League destination |
|---|---|---|
| Ranked or casual online launch, with a match context in the per-launch folder | **Yes.** Wrapped in `nsr.submission.v1`, keyed on the `match_id` the backend issued | **No.** Never, under any circumstances, including when both players picked league teams |
| Creation League launch, launched from the league surface with a league id | No | **Yes**, exactly as today |
| Offline play, no match context and no league id | No | **No.** Offline is private by default per questionnaire 17.4. The capture stays on the machine |
| Anything else, including a capture found on disk that no launch this session accounts for | No | No. **Quarantine it and surface it, do not guess** |

**Two rules make that table safe rather than aspirational:**

1. **Routing is decided at launch, written down, and never re-derived at upload time.** The launcher records the intended destinations for the session before Dolphin starts, alongside the match context. A capture is matched to a session; a capture with no session is quarantined. Deriving destination from file contents after the fact is exactly the mistake that lets an offline game reach a league fixture.
2. **A capture is never sent to more than one destination in v1.** Ranked and league are mutually exclusive here. This is deliberately more restrictive than "one capture, two consumers" sounds, and it costs nothing today because no ranked game is also a league fixture.

**This closes the exact hazard the runtime README warns about:** "if they happen to pick two league teams, those fake results land on a real fixture." That warning is about the netplay gate, but the same hazard reappears one layer up if routing is inferred from team names or from the capture file rather than from the launch. A ranked ladder game played with league teams selected **must not** reach `ingest.php`, and the only way to guarantee that is to never send a ranked capture there at all.

### 10.2 The hard prerequisite nobody can design around

**This section is the reason for the fourth row in the section 1 corrections table.** `PROJECT_QUESTIONNAIRE.md` 17.9 is labeled "Research, ANSWERED" and says the in-emulator tracker and the `statread1` DOL hook are two separate paths and must not be confused. They are one pipeline, and reading them as separate makes an ISO rebuild look optional when it is a hard blocker for ranked play.

Per-player stats come from the `statread1` DOL hook. `NflsStatTracker` locates the ring by reading the branch at `HOOK_SITE 0x8005A960` (`NflsStatTracker.cpp` line 37), rejecting anything whose primary opcode is not 18 (`if ((instr >> 26) != 18)`, line 108), computing the cave address from the branch displacement (line 113), and then recovering the ring's write pointer and buffer base from the `lis`/`ori` pairs inside that cave (line 119 checks primary opcodes 15 and 24). **If the instruction there is not a branch, the hook is absent and there are no per-player stats at all.** The tracker is not merely helped by the DOL hook; it is a reader for it.

Verified by direct probe of the DOL inside each disc image:

| Image | `statread1` hook |
|---|---|
| `CR4E12.iso` | **Present** (branch, `0x483A3B80` to cave `0x803FE4E0`) |
| `GameCube Version\root\&&systemdata\Start.dol` | **Present** |
| `NSR.iso` (shipped offline) | **Absent** (plain `stw`, `0x907E0000`) |
| `FR4N31.iso` (existing online build) | **Absent** |
| `Files\Reborn Dolphin\Games\NSR.iso` | **Absent** |
| Xbox `nsrpatch.iso` | **Absent** |

**The existing online build cannot produce a box score.** Without the hook, `players[]` is empty, the ingest contract rejects the capture outright, `sides_verified` can never become true, and the identity chain has no scoreline to attach itself to. **NSRONLINE must carry the `statread1` hook. This is a build prerequisite, not a nice to have, and it is on the critical path for ranked play.**

### 10.3 Game ID consequences

Whether the online build keeps `FR4N31` is open (questionnaire 9.8), and it is not cosmetic:

- It names the texture folder.
- It names the `Sys/GameSettings` ini. The selected runtime ships `GN7.ini` and `FR4N31.ini` but **not** `CR4E12.ini`, while `dolphin-exgl\Data\Sys\GameSettings` contains only `GN7.ini`, so **rebuilding from the selected source tree silently drops `FR4N31.ini`**.
- `FR4N31.ini`'s own comment states the consequence: with no per-game override, `ImmediateXFBEnable` stays at the global default `True`, which Dolphin explicitly disables for NFL Street 2 because the game's XFB usage causes erratic frame pacing that reads as input lag while the FPS counter still shows 60.
- A new id must be added to **both** `GameGated()` in `NflsStatTracker` and the gated-id list in `NflsPlayHide`, or online matches silently lose stat capture and play-call hiding at once.

### 10.4 Marker files: the rule has to be an allowlist

`DOLPHIN_INTEGRATION.md`'s "must never ship" table lists two files. The real class is larger:

| Marker | Effect | Tree |
|---|---|---|
| `nsr_track_offline.txt` | Defeats the netplay-only stat gate. **Present in the selected runtime today** | selected fork |
| `nsr_debug.txt` | Prints gate state on screen. **Present today** | selected fork |
| `playhide_test.txt` | Forces the opponent-play mask at any play call with **no NetPlay** | selected fork |
| `playhide_debug.txt` | On-screen diagnostic every 20 frames | selected fork |
| `nfls2_netlockstep.txt`, `nfls2_delay.txt`, `nfls2_team.txt`, `nfls2_symmetrize_on.txt`, `nfls2_amputate_on.txt`, `nfls2_s1_dormant.txt`, `nfls2_norelax_elig.txt` | Enable an entire alternate transport and gameplay modifications | second tree |

**A packaging rule that strips two named files does not close this class.** Only an allowlist for the shipped `User\` folder, or compiling the gates in, does. The selected runtime today has both `nsr_track_offline.txt` and `nsr_debug.txt` present (0 bytes, dated July 27, 2026) and no `nsr_ingest.txt` override, so the compiled-in URL, token and league id 8 are live, exactly as the runtime's own README warns.

---

## 11. The second Dolphin tree

A second, entirely separate Dolphin working tree exists at `C:\Users\Adam\nflstreet2_clean\dolphin`, on modern mainline upstream (HEAD `144d19433a`), remote `https://github.com/michaelpenixfan2390/dolphin.git`, with a Release `Dolphin.exe` of 22,420,480 bytes dated June 25, 2026. **No launcher document mentions it**, and it holds roughly 90 KB of Reborn online C++ that is absent from `dolphin-exgl` and from preservation commit `eca379f`, all of it untracked:

| File | Size | What it is |
|---|---|---|
| `NflsSymmetrize.cpp` | 50,429 B | untracked |
| `NflsLink.cpp` | 20,483 B | A custom UDP fixed-delay lockstep transport that **replaces** Dolphin NetPlay: role by who binds the port first (Host = Team0), epoch-barrier arming, per-frame `{epoch, ackEpoch, frame, armFrame, in_down, guard}`, a truly blocking remote-input read that never fabricates input |
| `NflsCaves.h` | 7,404 B | untracked |
| `NflsGuard.cpp` | 6,415 B | Form-B guard digest |
| `NflsDesync.cpp` | untracked | **The only desync signal in any Reborn code.** Latches the first divergent frame, flashes a persistent OSD, appends a `{"desync":...}` line to the guard trace |
| `NflsSoak.cpp` | untracked | |
| `NflsPlayHide.cpp` | 4,556 B | A **variant adapted to the newer upstream API**, not established as newer in time. See the note below |
| `VideoCommon/NetPlayPlayCallUI.cpp/.h` | untracked | An unwired shell: its commit callback is documented as wired to `NetPlayClient::SendPlayCallCommit`, and **no such symbol exists anywhere in that tree** |
| `Data/Sys/GameSettings/FR4N31.ini` | untracked | |

**On that `NflsPlayHide.cpp`, because "newer" was asserted here before and is not supported.** The two files carry the **identical mtime, July 22 2026 23:48**, so nothing on disk establishes chronological order. Diffing them shows the 279-byte delta is almost entirely an **API adaptation to modern upstream**: `void Update()` becomes `void Update(Core::System& system)`, `#include "Core/System.h"` is added, and every `Memory::Read_U32`/`Read_U8` call becomes `mem.Read_*` on an `auto& mem = system.GetMemory();`. The rest is comment text, including one block that reads as a **stale leftover rather than a newer revision**: "The sheet side is PLAYER-FIXED: P1 = port 0 = LEFT sheet, P2 = port 1 = RIGHT (by player, NOT by possession). Derive the side straight from the local port." **Both files then execute the same team-based logic**, reading `local_team` out of the play-call registry record at `reghdr + port * 0xC`, which is not what that comment describes. Correct verdict: it is the same logic ported to the newer Dolphin API, its comments are less trustworthy than the selected fork's, and **which one was written first is unverified**.

Recorded determinism results for that tree: 5,685 frames bit-identical (June 19, 2026), 10,109 frames on the redesigned netcode (June 22, 2026), and 2,502 frames through a live down after a gating fix. Both instances were booted into a shared save state using the real CLI flags `-s`, `-e`, `-u`.

| Verdict | Reasoning |
|---|---|
| **Do not ship `NflsLink` for September 12** | It replaces the transport that currently works, on a fork that is not the selected one, with an ownership question attached to the remote |
| **Preserve it immediately** | Roughly half the Reborn emulator work, and specifically the half this document is about, is unpreserved and untracked on one machine. `DOLPHIN_INTEGRATION.md`'s phase 0 claims preservation is done; it is done for the wrong half |
| **Harvest `NflsDesync`'s idea, not its code** | The shipping path needs a desync signal and stock NetPlay does not give the launcher one. That idea is worth porting even though its implementation depends on the custom transport |

**The ownership question, stated precisely, because it was overstated here before.** Verified by `git remote -v` in each tree:

| Tree | Remote | URL |
|---|---|---|
| `nflstreet2_clean\dolphin` (second Dolphin tree) | `origin` | `https://github.com/michaelpenixfan2390/dolphin.git` |
| `nfl-street-reborn-online` prototype | `origin` | `https://github.com/MiltyTheGreat/RebornOnline.git` |
| `nfl-street-reborn-online` prototype | `launcher` | `https://github.com/michaelpenixfan2390/nflstreetrebornlauncher.git` |
| `NFL Street Reborn Launcher` (this repository) | `origin` | `https://github.com/adampreddy/nfl-street-reborn-launcher.git`, and **no other remote** |

So the earlier claim that `michaelpenixfan2390` "also appears on the launcher repository" is **false**: this repository has exactly one remote and it is the user's own. What is true is narrower and still serious. **Two third-party GitHub owners are involved, not one**, and the second one was never named in the ownership paragraph: `michaelpenixfan2390` owns the second Dolphin tree's origin and a separate launcher repository the prototype still points at, and **`MiltyTheGreat` owns the prototype's own origin**. The unresolved ownership and licensing question therefore reaches the emulator itself and not just the Electron prototype, and it reaches two parties. The memory record of the June 2 two-machine test additionally states that a collaborator ran the matchmaking server behind a tunnel, which is a live operational dependency on a third party.

**This is directly on the September 12 path.** Dolphin is GPLv2+, so shipping a binary obliges publishing corresponding source, and the second tree's Reborn C++ has an unresolved contributor question attached to it. That is one more reason the section 13 verdict for `NflsLink` and its siblings is preserve, not ship.

---

## 12. Security defects in the harvested prototype that must not be copied

Every one of these is real, present in the code being harvested, and cheap to reintroduce by copying a file.

| Defect | Consequence | Fix |
|---|---|---|
| Socket handlers trust a client-supplied `userId` even though `index.js` already verified the JWT into `socket.userId` | Any authenticated user can act as any other | Use `socket.userId` only. Never accept an identity from a payload |
| `io.use` swallows a failed `jwt.verify` in an empty catch and calls `next()` anyway | **Unauthenticated sockets connect** | Reject on verify failure |
| **`server/socket/match.js`**, inside the `match:result` handler, chooses the vote column from `match.player1_id === winnerId` rather than from the submitter | **One player can fill both votes**, which defeats two-client confirmation entirely. Reporting yourself the winner writes `player1_vote` if you are player 1, but reporting your opponent the winner writes `player2_vote`, so a single client can satisfy both columns and complete the match alone | Choose the column from the authenticated submitter, exactly as the REST route already does. **Note the file: it is the socket handler, not `routes/matches.js`; see the note below** |
| `custom_rooms` stores the lobby password in plaintext | Credential disclosure on any database read | Hash it, or drop passwords for room codes |

**Do not patch `routes/matches.js` for the vote-column defect. It is already correct there.** An earlier draft of this document named the wrong file, and acting on that would have patched working code and left the real bug in place.

Verified across **all five prototype copies on disk** (`StreetLauncherProject`, `StreetLauncherProject(1)`, and the `nfl-street-reborn-test` variant inside each, plus the standalone `Downloads\nfl-street-reborn-test`):

| File | Line | Code | Verdict |
|---|---|---|---|
| `server/routes/matches.js` | 32 in every copy | `const voteKey = match.player1_id === req.user.id ? 'player1_vote' : 'player2_vote';` | **Correct.** Keys on the authenticated submitter, which is exactly the fix prescribed above |
| `server/socket/match.js` | 55 in the `nfl-street-reborn-online` copies, 66 in the `nfl-street-reborn-test` copies | `const voteKey = match.player1_id === winnerId ? 'player1_vote' : 'player2_vote';` | **Defective.** Keys on the claimed winner |

The lesson generalizes past this one bug: **the REST route and the socket handler in this prototype implement the same rule twice and do not agree**, so any harvest must read both paths, not one. The socket path is also the one an Electron client would actually call.

Add to that the credential position the inventory understates. The exposure is **four places, and the rotation is two-sided**: the league deploy README records that `_inc/config.php` `DB_PASS` was exposed in a chat transcript on July 27, 2026 and still needs rotation, and it states that the Dolphin `DEFAULT_TOKEN` and the server's `league/_inc/ingest_token.php` match as shipped. **Rotating the compiled-in token without the server side breaks ingest for every existing build.**

---

## 13. Reuse verdicts

Every existing component this document touches, with exactly one verdict.

**On the word PORT in the prototype rows.** The locked decision is that the online client is **rebuilt from scratch** and the three `nfl-street-reborn-online` copies are **reference only**, and `SYSTEM_ARCHITECTURE.md` 5.6 says "harvest, then rebuild against Postgres." Nothing below overrides that. **PORT here means the formula, the constants and the state shape move; the files do not.** No prototype JavaScript file is copied into the new backend, because every one of them carries at least one of the section 12 defects and all of them are written against SQLite through `better-sqlite3` with synchronous prepared statements that do not survive the move to Postgres. Where only a number or a rule survives, the note says so.

| Component | Path or location | Verdict | Note |
|---|---|---|---|
| `NflsPlayHide` | `dolphin-exgl\Source\Core\Core\` | **WRAP** | Keep the mask behavior untouched. Add a published, latched port and team alongside `MaskSide()`. Do not change what it draws |
| `NflsStatTracker` | same | **WRAP, additive** | Schema string and hashed set unchanged. One additive top-level `reborn_match` block. **This is a source change**, and it contradicts `SYSTEM_ARCHITECTURE.md` 2 and 5.6 and `DOLPHIN_INTEGRATION.md` 9, which call it unchanged. Corrected in section 1 |
| `NflsStatUpload` | same | **PORT** | Logic moves to the launcher and supervisor. Compiled out of the shipping binary, which is how the token stops shipping |
| Dolphin NetPlay core | upstream, unmodified | **REUSE** | Transport, sync, pad mapping, `SyncIdentifier`. Do not touch it before September 12 |
| `Dolphin.ini` `[NetPlay]` block | selected runtime | **REPLACE** | Generated per launch, plus `-C` overrides. The stale `HostCode 1db46c2c` goes |
| `netplay_uia.ps1` | June 2 online copy | **RETIRE** | Windows UI Automation cannot port. Keep it as the calibration record of what the dialog looks like |
| `StreetLauncherProject` NetPlay CLI path | June 4 online copy | **RETIRE** | Calls flags that do not exist. Provably dead |
| `server/elo.js` | prototype | **PORT the formula, rebuild the code** | The four numbers survive verbatim: K = 32, start 1000, 400 divisor, integer rounding. The file does not. Storage is rebuilt on Postgres with `rating_event` rows |
| `server/socket/queue.js` | prototype | **PORT the rules, rebuild the code** | `RANKED_RATING_WINDOW = 150`, `QUEUE_TIMEOUT_MS = 90000` and the 5000 ms tick survive as tuning values. The match-found payload is replaced entirely by the match context, and region and ping (9.1) are new engineering with no prototype ancestor |
| `server/socket/match.js`, `routes/matches.js` | prototype | **PORT the shape, rebuild the code** | Only the two-client confirmation shape survives: two votes, agreement completes, disagreement holds. The self-reported winner is replaced by the derived one, the quit-verdict auto-forfeit is removed, and **the vote-column bug in `socket/match.js` must not be carried across; `routes/matches.js` line 32 already has it right** |
| `server/db/schema.sql` | prototype | **REPLACE** | New tables. `matches` stores no scores, no nonce, and no capture reference |
| `server/routes/auth.js` | prototype | **REUSE the design** | Discord `identify` scope, server-side exchange, own JWT. Already correct |
| `server/middleware/auth.js`, `index.js` `io.use` | prototype | **REPLACE** | Contains the empty-catch authentication bypass |
| `custom.js` private rooms | prototype | **PORT the room-code idea, rebuild the code** | The 6-character uppercase base36 room code survives as a concept. The plaintext password does not survive at all |
| `ingest.php` and `stat_import.php` | league site | **REUSE the design, leave the deployment alone** | Canonical hash, unique-key idempotency, hold rules, 36-point floor, prefix-tolerant team resolution |
| `nsr.game.v1` schema | Dolphin | **REUSE, additive only** | **Additive, not unchanged**: one new top-level `reborn_match` key, outside the hashed set. The schema **string** stays exactly `nsr.game.v1`, because bumping the version breaks the live endpoint's exact-string check. See 6.1 |
| Discord Rich Presence | nothing exists | **NEW** | Questionnaire 13.3, Decided, Must-have. Owned by the launcher and the supervisor, never by Dolphin. Requires `-C Dolphin.General.UseDiscordPresence=False` on online launches. See 9.5 |
| Dolphin's own `DiscordPresence` | upstream, `Source/Core/UICommon/` | **RETIRE for online launches** | Left in the binary, disabled per launch via config. Two presence publishers for one account race on the local IPC socket |
| `nsr_track_offline.txt`, `nsr_debug.txt`, `playhide_test.txt`, `playhide_debug.txt` | selected runtime `User\` | **RETIRE** | Replaced by compiled-in gates and a packaging allowlist |
| `NflsLink`, `NflsGuard`, `NflsSymmetrize`, `NetPlayPlayCallUI` | second tree | **PRESERVE, do not ship** | Untracked. Preserve before anything else touches that machine |
| `NflsDesync` | second tree | **PORT the idea** | The shipping path needs a desync signal; the implementation depends on a transport we are not shipping |
| Discord application | already exists (`server/.env`) | **REUSE** | Client id and secret exist. Both need rotation |
| Traversal via `stun.dolphin-emu.org` | selected runtime config | **REUSE for now** | See open items. Running our own is deferred |

---

## 14. What has to be true before ranked play ships

In dependency order, not schedule order.

```
 0  Preserve the second Dolphin tree                 it is untracked, on one machine
 1  Rotate credentials, both sides                   token, DB_PASS, JWT_SECRET, Discord secret
 2  Build NSRONLINE with the statread1 hook          without it there is no box score at all
 3  Measure registry team -> teams[] index           the one missing link in the chain
 4  NflsSession driver + event stream                the only way to drive NetPlay off Windows
 5  Match context reader in the fork                 signed, expiring, nonce-bearing
 6  reborn_match block in the capture                additive, outside the hashed set
 7  Backend: auth, queue, lobby, context issue       Postgres, Railway
 8  Capture routing rule, decided at launch          keeps a ranked game off a league fixture
 9  Backend: submission + adjudication + Elo         the decision table in section 7.2
10  Account actions: unlink, sign out, export, delete  questionnaire 13.6, Decided, all required
11  Two-machine end-to-end test on the SELECTED runtime
12  Chat, Rich Presence, preflight polish, moderation surface
```

Step 8 is placed before the backend adjudication work on purpose: it is cheap, it is entirely launcher-side, and it is the one item on this list whose failure mode writes bad data into a live production system that other people depend on. Step 10 is placed after adjudication because deletion has to know what a held match is before it can decide what to do with one.

Step 11 has never been done. The June 2 two-machine test used a different Dolphin build (identified as `2603a`) and a different fork, and the memory record explicitly lists the post-fix real two-machine run as still pending.

---

## Open items

1. **The registry-team to `teams[]` mapping has never been measured.** Until it is, the identity chain has a hole in the middle and no result can be derived rather than reported. This is the single highest-value unknown in the document.
2. **`NSRONLINE` does not currently carry the `statread1` hook.** Verified by probe against `FR4N31.iso`. Without it there are no per-player stats, `sides_verified` can never be true, and the existing ingest contract rejects the capture with "Capture has no players."
3. **Every field of the match context is new.** Nothing is partially built. No nonce, match id, expected port assignment, expected image hash, runtime version, config version or timestamp exists anywhere today.
4. **The match context delivery mechanism does not exist.** `DOLPHIN_INTEGRATION.md` section 6 asserts online launches receive it by controlled local file, but no file, reader, path, format or version exists in either fork. The reader is new fork surface.
5. **Nothing binds a NetPlay participant to an account.** The nickname proposal in 5.3 is a proposal. What the opponent sees, and what happens when a user edits it in Dolphin directly, is undecided.
6. **Desync detection does not exist on the shipping path.** Stock Dolphin halts on divergence, but nothing reports that to the launcher, and **Dolphin's exit code on a NetPlay desync is unknown**. The only desync latch in any Reborn code lives in the unshipped second fork.
7. **No exit-code classification exists at all.** `SYSTEM_ARCHITECTURE.md` section 2 says the supervisor is the only thing that can honestly report a crash, but clean quit, crash, NetPlay disconnect and desync have never been enumerated. **Rows in the section 7.2 decision table that depend on that distinction are currently unimplementable.**
8. **Every timeout and grace period is unknown.** How long the backend waits for a second submission, how long a lobby countdown runs, how long after a disconnect a claim can be made. Questionnaire 16.10 and 15.12 mark these Open and say they need real-world testing that has not happened. The only verified number is the prototype's `QUEUE_TIMEOUT_MS = 90000`.
9. **Nobody knows how often results would hold.** All six real captures on disk would hold under the section 7.2 rules, but that number is not the evidence it looks like: four of them hold only on the 36-point floor because they are short deliberate test games, and none of them is known to be a NetPlay game. See 7.4 for the full corpus. **The capture code is in better shape than an earlier draft of this document claimed; the hold rate for real ranked games remains completely unmeasured.** If holds are common, a conservative-by-default policy means most ranked games never rate. This needs measurement on real, full-length, two-client games before September 12, not after.
10. **Region and ping matchmaking, and ping-derived input delay, are recorded as Decided but do not exist.** No region field, no ping field, no ping measurement anywhere in the harvested code, and a static `BufferSize` of 3.
11. **Traversal ownership is undecided.** Today it points at Dolphin's public `stun.dolphin-emu.org`. Running our own on Railway, keeping the public one, or falling back to direct IP is unresolved. The privacy consequence (peer to peer exposes each player's IP address to the opponent) has approved disclosure copy in the UX guide but no stated mitigation.
12. **Nothing verifies that both clients used the launcher-generated NetPlay configuration, and `STRICT_SETTINGS_SYNC` does not help.** Verified in source (5.4): it is a convergence switch that pushes the host's graphics settings onto clients, **not a gate that refuses to start on a mismatch**. There is no refusal path anywhere in the fork. So turning it on makes a mismatch invisible rather than fatal, and `netplay_config_version` is an unenforced self-report on both sides. What a config version is, how the backend checks it, and whether any check is possible at all without new fork surface are all undefined.
13. **Elo policy beyond K = 32 and a 1000 start is undecided:** provisional handling, decay, floor, shared or separate casual pool, forfeit K, and the exact behavior when a held result is later approved or reversed.
14. **Chat is entirely new.** No schema, no events, no component, no moderation surface, and retention is deferred.
15. **Roughly half the Reborn emulator work is unpreserved.** The second tree at `nflstreet2_clean\dolphin` holds about 90 KB of untracked online C++ including the only desync signal that exists. Preservation commit `eca379f` covers the other half.
16. **The additive `reborn_match` block is inferred to pass the live ingest endpoint, not tested.** The validation is a schema exact-string check plus non-empty players plus exactly two teams, so it should pass, and that must be confirmed against the live endpoint before any ranked build.
17. **No two-machine match has been tested end to end on the selected runtime.** The one real two-machine test used a different build and a different fork.
18. **macOS and Linux have no verified NetPlay path.** The UI Automation driver cannot port, the replacement interface is unbuilt, and neither platform has ever been run, only built. Whatever ships will ship unverified on two of three platforms.
19. **Discord Rich Presence is entirely unbuilt, and questionnaire 13.3 marks it Decided and Must-have.** Section 9.5 proposes the owner (launcher for menu and queue states, supervisor for live match, never Dolphin), the per-state payloads, and the `reborn://join/<code>` interaction. **None of it exists**, no Discord application id has been chosen for it (13.7 is still "Confirm" and says the existing app must be rotated either way), and the supervisor's ownership of the live-match state is a consequence of process A exiting that has never been designed against. The verified prerequisite is cheap and specific: `Main.General.UseDiscordPresence` defaults to `true`, so online launches must override it.
20. **Account deletion policy is proposed here, not decided.** Questionnaire 13.6 marks unlink, sign out, export and delete Decided and all required, but nothing states what deletion does to a `match_result` row that another player also appears on, to `rating_event` history, or to the two verbatim submissions a held match is required to keep. Section 6.4 proposes anonymize-not-erase for shared rows, hard delete for the user's own envelopes, and **void rather than adjudicate for a match held at the moment of deletion**. All three need a real decision, and the third one has a user-visible consequence for the opponent.
21. **Retention is Open (questionnaire 17.10) and this document does not close it.** Section 6.4 lists the shapes it has to act on and proposes a starting position. Submission envelopes are the large, personal, short-lived class and the one most in need of a number.
22. **Result visibility is Decided (questionnaire 17.4) but has never been applied to these record shapes.** Section 6.4 maps it: ladder rows public, submission envelopes participant-and-moderator only, offline captures private and never routed off the machine. The mapping is new in this document and has not been reviewed against the UX copy.
23. **Capture routing is proposed, not built, and it is the hazard the runtime README warns about moved one layer up.** The identical `nsr.game.v1` file is produced offline, in a league game, and in a ranked game, and `source` is the literal string `"dolphin"` in all six captures on disk, so the file cannot be self-classifying. Section 10.1 proposes deciding destinations at launch, recording them, quarantining any capture with no matching session, and never sending one capture to two destinations. **Nothing implements this**, and getting it wrong puts a ranked ladder game played with league teams onto a real league fixture.
