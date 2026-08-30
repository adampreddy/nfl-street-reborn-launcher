# Security, Privacy and Licensing

> **Precedence.** Where this document disagrees with
> [`RECONCILIATION_RULINGS.md`](./RECONCILIATION_RULINGS.md), **the ruling wins.**
> Five auditors found 172 internal contradictions across this document set and an
> adjudicator issued one ruling per conflict. The rulings are applied to this file
> progressively; until that is finished, read the rulings first for anything
> touching safety, verdicts, schemas or identifiers.

## NFL Street Reborn Launcher

**Date:** August 29, 2026
**Covers:** `CLAUDE_START_HERE.md` step 6, the existing-work rule's licensing clause, and the third of the three architecture documents
**Depends on:** [`SYSTEM_ARCHITECTURE.md`](SYSTEM_ARCHITECTURE.md), [`DOLPHIN_INTEGRATION.md`](DOLPHIN_INTEGRATION.md), [`COMPONENT_INVENTORY.md`](../discovery/COMPONENT_INVENTORY.md), [`COMPONENT_MAP.md`](../discovery/COMPONENT_MAP.md), [`PROJECT_QUESTIONNAIRE.md`](../discovery/PROJECT_QUESTIONNAIRE.md)

Every claim below is marked **verified** (read from the artifact during this pass), **reported** (established by discovery and not re-read here), **inferred**, or **proposed**. Every existing component this document touches gets exactly one verdict: reuse, wrap, port, replace, or retire. The consolidated verdict table is **section 27**.

**Tables of proposed behavior carry a Marker column** (sections 12 and 13), so a specification for a system that does not exist yet cannot be read as a description of one that does.

---

## 0. Nine corrections to documents already written, four of them to this one

These are not new opinions. They are places where an existing document says something the artifacts do not support. Fixing them changes the shape of the work, so they come first.

**Corrections 0.5, 0.7, 0.8 and 0.9 were raised against an earlier draft of this document.** They are recorded rather than quietly edited out, because the earlier version has already been read and its framing was wrong in ways that changed the priority of the work.

| # | Document | What it says | What the artifact says | Consequence |
|---|---|---|---|---|
| 0.1 | `PROJECT_QUESTIONNAIRE.md` 3.5 and 14.10 | Preservation commit is "local only, not pushed"; rotation "blocks pushing the fork to any public remote" | **Verified:** `gh api repos/adampreddy/reborn-dolphin/branches` returns `nfls-reborn/preserve-2026-08-29`; the blob for `Source/Core/Core/NflsStatUpload.cpp` at ref `eca379f` is 14,359 bytes, sha `b1034d06639d9f3a20a06a7cee72b8f4828374b8`, and still contains `DEFAULT_TOKEN` | Rotation is no longer a precondition for a future push. It is **remediation for a credential that already exists in a remote git history**. `DOLPHIN_INTEGRATION.md` section 2 states this correctly; the questionnaire is the stale document |
| 0.2 | `SYSTEM_ARCHITECTURE.md` 2, `COMPONENT_MAP.md` 4, `DOLPHIN_INTEGRATION.md` 6 | The gate "cannot be defeated by placing a file in `User\`" once packaging strips it | **Verified:** `NflsStatTracker.cpp` checks two locations, `File::GetUserPath(D_USER_IDX) + "nsr_track_offline.txt"` **or** the bare filename `"nsr_track_offline.txt"` | Stripping `User\` closes one of two doors. See section 9 |
| 0.3 | Every document | "Three outstanding credential sets" | **Verified, and the copy is now named, which the earlier draft failed to do.** There are five `nfl-street-reborn-online` trees under `Downloads`, and their `server/routes/admin.js` comes in exactly two variants. The 208-line variant (sha256 prefix `f273540bf6c3cb70`) reads `process.env.ADMIN_SECRET` at line 9 and compares it to the `x-admin-secret` header; it is in `StreetLauncherProject\nfl-street-reborn-online` and `StreetLauncherProject(1)\StreetLauncherProject\nfl-street-reborn-online`. The 186-line variant (`e775e299161fa3c3`) has `router.use(requireAuth, requireAdmin)` at line 7, no `/live` route, no `ADMIN_SECRET` and no `x-admin-secret` anywhere; it is in the three `nfl-street-reborn-test\nfl-street-reborn-online` trees. `ADMIN_SECRET` is in none of the five `.env` files | The inventory is **four**. The fourth exists in two of five copies, and its value is generated at runtime rather than stored. See section 1 |
| 0.4 | `PROJECT_QUESTIONNAIRE.md` 22.1 | "No telemetry, no analytics" | **Verified:** the Xbox package's `Config\Dolphin.ini` sets `[Analytics] ID` only, with no `Enabled` and no `PermissionAsked`. Dolphin's defaults are both `false` (`MainSettings.cpp` lines 387 to 389), so the consent prompt appears and can be accepted | The position is **not held on the Xbox path**. See section 10 |
| 0.5 | **Withdrawn and replaced.** The earlier draft said `COMPONENT_INVENTORY.md` 14 lists ReShade and `dxgi.dll` "under the shipped installer copy" | That claim | **Verified: the inventory does not say that, so there was nothing to correct.** Line 426 attributes only "`reshade-shaders\` (28 MB in the installer copy)" to the installer, which is right: that folder measures 28,800,956 bytes across 62 files. `dxgi.dll` is listed at line 135, inside section 4 "Selected Reborn Dolphin runtime", which is also right. What **is** separately verified: no `dxgi.dll` exists anywhere under `Files\`, and it exists in the selected runtime at 5,487,616 bytes dated February 28 | The ReShade question splits in two. The **injector** has never shipped and is a first distribution. The **shader collection** ships today. See section 22 |
| 0.6 | `PROJECT_QUESTIONNAIRE.md` 7.9 and `COMPONENT_MAP.md` 9 | Console pairing credentials in the Windows credential store; Device Portal deployment | The locked decision is a file mover. No `.appx`, no `.msix`, no certificate, no deployment tooling exists in the Xbox package (reported) | There is **no credential class to store**. Questionnaire 7.9 should be retired, not implemented. See section 8 |
| 0.7 | This document's earlier draft, sections 1 and 2 | The ingest token is "compiled into every `Dolphin.exe` ever distributed", so "the population that holds the credential is the population that downloaded the game" | **Verified, and it inverts the framing.** The only publicly distributed binary is `C:\Users\Adam\Desktop\Reborn Installer\Files\Reborn Dolphin\NFL Street Reborn Dolphin.exe`, 14,537,728 bytes, sha256 `8705a96bee201168fb59ae52a01c2f8eb3d64c18851e664d94118c229c080146`, the exact hash this document already cites as the shipped build. A byte scan of that file returns **zero** occurrences of the 64-character token value and zero of `ingest.php`, `nflstreetreborn.com`, `nsr.game.v1`, `LeagueGameStats`, `uploaded.txt`, `nsr_ingest.txt`, `nsr_track_offline.txt`, `nsr_debug.txt`, `nsr_`, `Nfls`, `playhide`, `sides_verified`, `per_player_matched` and `pick_six`. The selected runtime `Dolphin.exe` (17,818,624 bytes, `617321bf...`) contains all of them, and the token exactly once; so does `Dolphin.exe.bak_2026-08-29_pre_uisplit` | **`NflsStatUpload` was never compiled into the shipped binary**, which is a 2021-dated Qt5 build predating all Reborn stat work. The token's known distribution is a private git remote plus hand-delivered runtime folders, **not** the installed user base. Rotation is still required and still gates publication; the blast radius is small and partly enumerable rather than "everyone who downloaded the game". See sections 1, 2 and open item 1 |
| 0.8 | This document's earlier draft, sections 1, 4 and 27 | "Three uncontrolled copies" of `server/.env` | **Verified: there are five**, all real regular files with distinct inodes, all 247 bytes, all sha256 `0493111044d5eaf7fb2a62a462834d01f4b268384d76065739134ce8a47c2860`. `Downloads\nfl-street-reborn-test\nfl-street-reborn-online\server\.env`; `Downloads\StreetLauncherProject\nfl-street-reborn-online\server\.env`; `Downloads\StreetLauncherProject\nfl-street-reborn-test\nfl-street-reborn-online\server\.env`; `Downloads\StreetLauncherProject(1)\StreetLauncherProject\nfl-street-reborn-online\server\.env`; `Downloads\StreetLauncherProject(1)\StreetLauncherProject\nfl-street-reborn-test\nfl-street-reborn-online\server\.env`. An enumeration of `nfl-street-reborn-online` directories returns the same five, so `SYSTEM_ARCHITECTURE.md` 202 and questionnaire 3.6 ("all three copies") are also low by two | **A remediation step that says "delete all three" leaves two live copies of `JWT_SECRET` and `DISCORD_CLIENT_SECRET` on disk.** Section 4 step 5 is rewritten to enumerate rather than to count |
| 0.9 | This document's earlier draft, sections 7 and 27; `SYSTEM_ARCHITECTURE.md` 184; `COMPONENT_MAP.md` 37 and 140 | "13 JSGME entries plus `!BACKUP` and `!INSTLOGS`"; "the 13 packages migrate"; "`MODS\` 13 packs" | **Verified by directory walk:** `Files\Reborn Dolphin\MODS` holds **13 directories in total**, of which **11 are mod packages** (Aqua, Blue, Camo, Green, Panda, Pink and Purple Football; PS Controls; Xbox Controls; Patriots Throwback; Saints Alternate) and **2 are `!BACKUP` and `!INSTLOGS`, both holding zero files**. Per-package counts are 1 each for the seven footballs, 19 for PS Controls, 19 for Xbox Controls, 5 for Patriots Throwback and 7 for Saints Alternate, summing to **exactly 57**, plus `JSGME.ini` for 58. `COMPONENT_INVENTORY.md` line 353 says "13 entries in `MODS\`" and then lists 11 packages plus the two control folders, which is **correct**; the statements derived from it are not | The 58 / 38 / 19 / 1 file counts were always right. The **entry count** was wrong, and the earlier draft's own arithmetic disagreed with it: 57 files across "13" packages is 57 files across 11. Corrected in sections 7 and 27 |

---

# Part A. Security

## 1. Credential inventory

Four names, not three. The fourth is not stored anywhere, which is the safest state any of them is in.

**Read the exposure column carefully.** The earlier draft of this section asserted a distribution the artifacts do not support (correction 0.7), and the corrected scope is narrower and better news than what was written before.

| # | Credential | Where it lives | Exposure surface | State |
|---|---|---|---|---|
| 1 | **League ingest bearer token** | `constexpr const char* DEFAULT_TOKEN` in `NflsStatUpload.cpp`, alongside `DEFAULT_URL = "https://nflstreetreborn.com/league/stats/ingest.php"` and `DEFAULT_LEAGUE_ID = 8` (**verified**, lines 41 to 43; the value is deliberately not reproduced in this document) | **Verified scope, measured by byte scan.** Present once in the selected runtime `Dolphin.exe` (`617321bf...`, 17,818,624 B) and once in `Dolphin.exe.bak_2026-08-29_pre_uisplit` (17,801,216 B). **Absent from the publicly distributed `NFL Street Reborn Dolphin.exe`** (`8705a96b...`, 14,537,728 B): zero occurrences. Also present in a remote git history at `eca379f` on a private repository. Distribution today is therefore the private git remote plus hand-delivered runtime folders, **not** the installed user base | **Live, unrotated** |
| 2 | `JWT_SECRET` | `server/.env`, **five** uncontrolled copies under `Downloads`, enumerated in correction 0.8 (**verified**: all 247 bytes, all sha256 `0493111044d5eaf7...`; six keys, verified in one copy) | Signs 30-day session tokens. A leak is a total account-impersonation break | **Live, development value** |
| 3 | `DISCORD_CLIENT_SECRET` | Same file, same five copies | A Discord application already exists, and four of the five copies sit inside working trees wired to third-party git remotes. See section 4.1 | **Live, development value** |
| 4 | `ADMIN_SECRET` | **Consumed** at `admin.js` line 9 in the 208-line variant only (2 of 5 copies). **Produced** at `server-app/src/main/instanceManager.js` line 7 by `crypto.randomBytes(32).toString('hex')`, injected into the spawned server's environment at line 43, and sent as the `X-Admin-Secret` header at line 94 (**verified**) | Guards `GET /admin/live`, which returns queue snapshots, user and match counts, and a 20-row leaderboard (**verified**, lines 7 to 26) | **Exists at runtime, stored nowhere.** Regenerated once per `server-app` session, held in process memory in both processes, never written to `.env` or to disk. The endpoint fails closed with 401 when the variable is unset |

`PORT`, `DISCORD_CLIENT_ID`, `DISCORD_REDIRECT_URI` and `CLIENT_URL` are configuration, not secrets. The redirect URI is `http://localhost:51234/callback` and `CLIENT_URL` is `http://localhost:5173`, both loopback development values (reported).

**One correction to a common assumption:** the prototype's admin surface is not open. **In the 208-line `admin.js` variant** (`Downloads\StreetLauncherProject\nfl-street-reborn-online\server\routes\admin.js` and its twin under `StreetLauncherProject(1)`, sha256 prefix `f273540bf6c3cb70`), `router.use(requireAuth, requireAdmin)` sits at line 29, deliberately after `/live` (**verified**). Only `/live` uses the shared secret. **In the 186-line variant** (the three `nfl-street-reborn-test` trees, `e775e299161fa3c3`) the guard is at line 7 and there is no `/live` route at all, so every admin route is authenticated there. Both shapes are safe; they are simply different, and every line number in this document that refers to `admin.js` refers to the 208-line variant.

### 1.1 The `ADMIN_SECRET` consumer, and why it gets a verdict

The endpoint's own comment says the secret is "shared with server-app", which means a second process holds a copy. **Verified:** that process is the Electron `server-app`, and it does the right thing without being told to. It generates 32 random bytes once per session, passes the value to each spawned server as an environment variable, and presents it on the loopback request to `http://localhost:<port>/admin/live`. **There is no copy on disk, no copy in `.env`, and no copy in any repository.**

**Verdict: PORT the mechanism, not the value.** The generate-per-session, pass-by-environment, loopback-only shape is the pattern to keep. What must change is that the Railway deployment is not loopback, so the transport is public and the header alone is not sufficient: on Railway the `/live` route needs the same authenticated-admin treatment as every other admin route, or it needs to stay bound to an internal address. **The value itself is never stored anywhere, on any platform.** That is the storage verdict, and it is the same one every other credential in this document gets.

---

## 2. Why a compiled-in token cannot be secured

This is not a hardening problem. There is no configuration of a symmetric bearer token inside a distributed client binary that makes it a secret.

1. **Extraction is trivial.** A 64-character hex string in a `constexpr const char*` survives into `.rdata`. `strings` finds it. There is no obfuscation that helps, because the binary must reconstruct the plaintext to send it.
2. **Distribution would multiply it, and so far it has not.** A shipped symmetric token is the same value in every install, so the population that holds the credential becomes the population that downloaded the game. **Verified: that has not happened yet.** The uploader was never compiled into the publicly distributed `NFL Street Reborn Dolphin.exe` (correction 0.7). What exists instead is a small, partly enumerable set of holders: the private `adampreddy/reborn-dolphin` history, the selected runtime folder and its `.bak`, and any hand-delivered copy of that folder. **The argument in this section is therefore forward-looking, not retrospective: the reason not to ship a token is that shipping it is the step that makes it unrecoverable, and that step has not been taken.**
3. **It is already in a git remote.** **Verified:** the blob at `eca379f` on `adampreddy/reborn-dolphin` still contains it. The repository is private today (**verified:** `visibility: PRIVATE`, `licenseInfo.key: other`, `pushedAt 2026-08-30T02:04:11Z`), but section 18 shows that GPLv2+ requires making it public. **Publishing the repository publishes the historical value even if `HEAD` is clean.** Rewriting history does not fix this either, because unreachable objects persist for a period and because the value may already exist in a clone.
4. **The documented override is a second problem, not a mitigation.** `LoadConfig` reads `nsr_ingest.txt` from the Dolphin user folder unconditionally and validates neither the URL scheme nor the host (reported; the comment block at lines 33 to 40 is **verified**). Anyone who can write one file into the user folder can redirect every subsequent upload to an endpoint of their choosing. That file is not present in the selected runtime today.
5. **The blast radius is bounded on the client side and unmeasured on the server side.** Client side it is now bounded by measurement: the shipped binary does not contain the value, so the holder set is the private repository plus hand-delivered runtime folders. Server side there is still no `ingest.php` source, no server log, no rate-limit evidence and no audit trail in the inspected material, so **whether the value was ever used by anyone other than the developer cannot be determined from here.** **The instrument that closes the remaining gap is unchanged: rotation paired with server-side revocation of the old value.**

**Rotating the client without invalidating the old token on the server accomplishes nothing.** That pairing is the whole task.

---

## 3. What replaces it

| Property | Compiled-in token (today) | Launcher-held credential (proposed) |
|---|---|---|
| Who holds it | Every copy of `Dolphin.exe` | The launcher process only, per install |
| Where stored | `.rdata` in a shipped binary | OS credential vault: Windows Credential Manager, macOS Keychain, libsecret |
| Lifetime | Forever, until a rebuild ships | Short lived, refreshed |
| Rotation cost | A new emulator build to every user | A server-side revocation |
| Scope | Whole ingest endpoint | One account, or one install |
| Emulator's knowledge of it | Full | **None** |

**The structural change is that Dolphin stops holding a credential at all.** `NflsStatTracker` keeps writing `nsr.game.v1` JSON next to game memory, which is where the game-memory knowledge belongs. The supervisor reads the file, authenticates, retries, and owns the queue. This is already the design in `DOLPHIN_INTEGRATION.md` section 10; this document adds the reason it is a security control and not just tidier plumbing.

**What must not happen:** replacing a static token in the emulator with a static API key in the launcher. That reproduces the identical failure with an extra hop. If the launcher must present an identity before a user signs in, that identity has to be established at install or first run and be individually revocable, not shipped.

**Undecided, and honestly so:** the token lifetime, the refresh mechanism, whether the credential is per install or per account, and what identity the launcher presents before sign-in. There is no design and no prototype to point at. See open items.

---

## 4. Rotation plan

Sequenced by dependency, not by calendar. Steps 1 to 4 are each small; the reason they matter is **section 19's** blocking chain.

| Step | Action | Blocks |
|---|---|---|
| 1 | Issue a new ingest token server side and **revoke the old one**. Confirm the old value returns 401 | Everything below |
| 2 | Rotate `JWT_SECRET` to a generated value held only in Railway's environment. All existing sessions become invalid, which is the point | Backend go-live |
| 3 | Reset `DISCORD_CLIENT_SECRET` in the Discord developer portal. The secret lives **only** on the Railway backend, never in the desktop client, which is a public OAuth client | Sign-in |
| 4 | **Keep the existing generator, do not invent a stored value.** `server-app` already produces `ADMIN_SECRET` per session with `crypto.randomBytes(32)` (**verified**, section 1.1), so there is no placeholder to inherit and nothing to rotate. What the Railway deployment needs instead is a decision on `/live`: authenticated-admin like every other admin route, or bound to an internal address. **Do not add the value to `.env` in order to "have" it** | `/admin/live` on a non-loopback host |
| 5 | **Enumerate, then delete.** Run a recursive search for `server/.env` under `Downloads` and delete every hit, rather than deleting a remembered count. **Verified today that is five files**, listed in correction 0.8. Replace each with `.env.example` carrying key names and no values | Nothing, but it removes five uncontrolled copies. **A fixed count here is the failure mode: the earlier draft said three** |
| 5a | Answer the three questions in section 4.1 about the two third-party git remotes before treating step 5 as complete | Whether steps 2 and 3 are sufficient |
| 6 | **Preferred end state:** remove `NflsStatUpload` from the shipped runtime entirely, so there is no token in the binary to rotate a second time | Nothing. This is the real fix |

### 4.1 The two third-party git remotes

The scope brief frames these secrets as living "in a `.env` inside a `Downloads` folder wired to third-party git remotes", and the remotes were never named. They are now.

**Verified** by `git remote -v` in each working tree. Only two of the trees are git repositories, and both are at the `StreetLauncherProject` level rather than at `nfl-street-reborn-online`:

| Working tree | Remote | URL | Fetch | Push |
|---|---|---|---|---|
| `Downloads\StreetLauncherProject` | `origin` | `https://github.com/MiltyTheGreat/RebornOnline.git` | Yes | **Yes** |
| `Downloads\StreetLauncherProject` | `launcher` | `https://github.com/michaelpenixfan2390/nflstreetrebornlauncher.git` | Yes | **Yes** |
| `Downloads\StreetLauncherProject(1)\StreetLauncherProject` | `origin` | `https://github.com/MiltyTheGreat/RebornOnline.git` | Yes | **Yes** |

`Downloads\nfl-street-reborn-test` is **not** a git repository (no `.git`), so the fifth `.env` sits in an untracked folder with no remote at all.

**The good news, verified and worth stating because it changes the severity:** `.env` is gitignored in both tracked trees and **was never committed**. `git check-ignore -v` resolves `nfl-street-reborn-online/server/.env` to `nfl-street-reborn-online/.gitignore:8:server/.env` and `nfl-street-reborn-test/nfl-street-reborn-online/server/.env` to `nfl-street-reborn-test/nfl-street-reborn-online/.gitignore:2:server/.env`. `git log --all -- '*server/.env'` returns nothing and `git ls-files` matches nothing. **The secrets did not reach either remote through these working trees.**

**Three questions deleting the files does not answer, and they are the reason this subsection exists:**

1. **Who else holds these working trees?** Both remotes are set for **push as well as fetch**, which means these clones were configured to publish, not merely to follow. Two GitHub accounts are named in the configuration: `MiltyTheGreat` and `michaelpenixfan2390`. Whether either account holds a copy of `.env` obtained some other way (a chat attachment, a zip, a screen share) **cannot be determined from this machine.** Marked unverified.
2. **Who owns the Discord application?** `DISCORD_CLIENT_ID` and `DISCORD_CLIENT_SECRET` belong to a Discord application that already exists. **Whether the owning Discord account is Adam's or one of the two GitHub account holders' is not established here.** If it is not Adam's, step 3 ("reset the secret in the developer portal") cannot be performed by Adam, and the correct action is to create a new application under an owned account rather than to rotate someone else's. **This is a blocking unknown, not a detail.**
3. **Did the secrets ever reach the remotes by another path?** The gitignore evidence covers these two trees at their current `HEAD` and across `--all`. It does not cover a differently configured clone on another machine. **The only way to answer it is to look at the remotes**, which requires access this document does not have.

**Action: treat the values as third-party-exposed until questions 1 and 3 are answered.** That does not change the plan (steps 2, 3 and 4 rotate them regardless); it changes whether a rotation is a hygiene step or an incident response, and it changes who has to perform step 3.

**If step 6 does not land before September 12**, the interim is a compile-time disable of the uploader in release builds. The tracker still writes JSON, the launcher still uploads, and the shipped binary contains **no credential at all**. That is strictly better than shipping a rotated token, because a rotated token is still a shipped token.

The launcher repository is clean: no occurrence of the token in the working tree or in any reachable revision, and `.gitignore` already blocks `.env`, `*.pem`, `*.p12`, `*.pfx`, `credentials.json`, `*.iso` and `*.raw` (reported). Keep it that way; that `.gitignore` also anticipates section 5.

---

## 5. Manifest signing and private key custody

### Why hashing is mandatory, with evidence

All four disc images in the selected runtime are **exactly 1,459,978,240 bytes** and differ in content: `CR4E12.iso`, `FR4N31.iso`, `NSR.iso` and `TOTALLY NOT MALWARE.iso` (reported). **File size cannot distinguish a build.** Any manifest, repair or update design that compares sizes is a design that cannot tell a correct install from a wrong one.

### Proposed shape

```text
manifest.json                        signature: manifest.json.sig (detached)
{
  "schema":       "reborn.manifest.v1",
  "component":    "offline-content",
  "install_root": "library/offline",
  "version":      "2.2.0",
  "sequence":     7,
  "min_launcher": "1.0.0",
  "created":      "<RFC3339 UTC>",
  "files": [
    { "path": "library/offline/NSR.iso",
      "size": 1459978240,
      "sha256": "<hex>" },
    ...
  ]
}
```

- **Detached signature over the manifest**, not over every file. The manifest carries the hashes; signing it once transfers trust to all of them.
- **Ed25519 proposed.** Small keys, small signatures, no parameter choices to get wrong.
- **The public key is compiled into the launcher.** This is the one key that is safe to ship, because it verifies and cannot sign.
- **Fail closed, with one scoped exception.** A signature failure, a missing manifest, or a hash mismatch on a file in a component where `customizable` is false or absent means refuse and offer repair. There is **no fallback to unsigned**. A client that degrades to unsigned on error has no signature scheme.
- **The exception exists because repair would otherwise delete custom music.** `customizable` is a mandatory field of `nsr.component.v1` and is true for `offline-content` only. A rebuilt offline image never matches its shipped hash, so an unqualified fail-closed rule would treat every user's custom soundtrack as damage and re-fetch over it. Repair therefore leaves a file alone when its component is `customizable: true` **and** its local SHA-256 equals the launcher's own recorded rebuild hash in `music/state.json`. A mismatch that matches nothing recorded is still damage, and the whole artifact is re-fetched. See [`CONTENT_AND_UPDATE_MODEL.md`](./CONTENT_AND_UPDATE_MODEL.md) section 12.

### What the signature has to cover, and why `version` is not enough (proposed)

`version` is a display string. A display string cannot carry a security decision, and three of the fields above exist only because of that.

| Field | Why it is in the signed payload |
|---|---|
| `component` | **Anti-substitution.** Without it, a validly signed `offline-content` manifest can be presented to the client as `music-pack` or `textures`, and every signature check still passes. The client must refuse a manifest whose `component` is not the one it asked for |
| `install_root` | **Anti-relocation.** The target root is a security decision, so it is signed rather than chosen by the caller. A manifest that claims a root the client did not request is refused, which is the same refusal that section 7 applies to package entries |
| `sequence` | **Anti-rollback.** A monotonically increasing integer, independent of the human-readable `version`. The client stores the highest `sequence` it has ever accepted per component and **refuses any manifest with a lower one**, even a perfectly signed one. See section 10 |
| `min_launcher` | Lets a content manifest refuse to install into a launcher too old to handle it, rather than half-installing and failing later |
| `schema` | Lets the format change without a client guessing which shape it is holding |

**The rule, stated once: a signature proves who wrote the manifest. It does not prove that this manifest is the one the client should accept right now.** Component identity, install root and sequence are what turn a valid signature into a valid decision, and all three have to be inside the signed bytes to be worth anything.

### Custody options

| Option | Pro | Con | Fit for a single developer with 14 days |
|---|---|---|---|
| Key in a CI secret | Signing is automatic on release | CI compromise is key compromise; the key exists on a machine nobody controls | Convenient, weakest |
| Key on the developer's machine, signing is a local release step | No third party ever holds it | Release cannot be fully automated; a lost machine is a lost key | **Recommended** |
| Hardware token | Key never extractable | Costs money, and the budget question (14.8) is unanswered | Not now |

**Recommended: an offline key, held on removable media, with signing as an explicit local release step, plus a written recovery plan for key loss.** If the key is lost, the recovery path is publishing a new public key in a new launcher build, which every existing install must obtain out of band. That is painful, which is why the plan needs to exist before it is needed.

**Manifest signing is not code signing.** It protects content integrity inside the product. It does nothing about SmartScreen or Gatekeeper. See section 25.

---

## 6. Deep link security

Nothing registers a URI scheme today. **Verified by absence:** no `setAsDefaultProtocolClient` call and no protocol registration in the prototype's main process or `package.json` (reported). `reborn://` is entirely new surface, which means it inherits no flaw and gets no free pass.

**The rule, carried forward from `INFORMATION_ARCHITECTURE.md` section 9 and unchanged in intent: a deep link may navigate and may pre-fill, but it may never install, delete, queue, or spend. `reborn://mod/<id>` opens the package; it does not install it. Anything else would make a Discord message into a remote-install primitive.**

| Route | Permitted effect | Explicitly forbidden |
|---|---|---|
| `reborn://play` | Navigate to Play. Starting the game is a user action on that screen | Auto-launching from a link |
| `reborn://online/casual` | Navigate to Online, pre-select casual | Entering the queue |
| `reborn://online/ranked` | Navigate, prompt sign-in, pre-select ranked | Entering the queue, or completing sign-in without the consent screen |
| `reborn://join/<code>` | Navigate to the lobby screen with the code pre-filled | Joining |
| `reborn://mod/<id>` | Open the package detail page | Downloading, installing, enabling |
| `reborn://music/<id>` | Open the pack detail page | Installing, rebuilding |
| `reborn://match/<id>` | Open the box score | Any write, including dispute filing |
| `reborn://settings/<category>` | Navigate to the category | Changing any value |
| `reborn://repair` | Open Verify and Repair. The read-only scan may start | **Replacing a single file without a confirm** |

### Handling rules (proposed)

1. **Parse and validate before navigating.** Reject anything that is not one of the nine routes. An unknown route lands on Home, silently. Do not show an error dialog naming the unrecognized input, because that turns a link into a way to render attacker-chosen text.
2. **Treat every path segment as untrusted.** `<id>`, `<code>` and `<category>` are opaque tokens matched against known values, never interpolated into a filesystem path, a shell command, a SQL string, or a webview URL.
3. **Single instance.** A second launch forwards the URI to the running instance and exits. Two copies racing over the same data root is a corruption bug before it is a security bug.
4. **Refuse while busy.** If a destructive or long-running operation is in flight (repair, music rebuild, USB write), a deep link queues the navigation and does not interrupt.
5. **A link never resumes a flow.** If the user was mid-purchase or mid-install and a link arrives, the flow restarts from a screen that requires a fresh confirm.

**Open:** registration scope (per user or per machine), behavior when a second copy claims the scheme, Linux desktop entry `MimeType`, macOS `CFBundleURLTypes`. Nothing registers the scheme today, so there is no precedent and no migration.

---

## 7. Mod package validation and path traversal refusal

### The catalog makes the strictest policy free

**Verified** by walking `Files\Reborn Dolphin\MODS`: **13 directories in total, of which 11 are mod packages and 2 are the `!BACKUP` and `!INSTLOGS` control folders, both empty (zero files each)**. The tree holds **58 files: 38 `.png`, 19 `.dds` and 1 `.ini`**. Per package: 1 file each for Aqua, Blue, Camo, Green, Panda, Pink and Purple Football; 19 for PS Controls; 19 for Xbox Controls; 5 for Patriots Throwback; 7 for Saints Alternate. **That sums to exactly 57.** The only file outside `Load/Textures/GN7E69/` is `JSGME.ini` itself, which is the manager's own state and not package content, and it is 0 bytes. So **57 of 57 package files are texture overlays under one root**, spread across **11** packages, not 13. Correction 0.9 records where the wrong entry count was carried.

**A format that allowlists two extensions under one root covers 100 percent of what exists. Path-traversal refusal therefore costs nothing in compatibility, which removes the usual reason projects loosen it.**

### Proposed policy

| Rule | Value |
|---|---|
| Container | Zip, no encryption, no external attributes honored |
| Manifest | Required, at the root, naming id, version, display name, author, target game id, declared file list with hashes |
| Allowed install root | `library/textures/<game-id>/` only, mapped from the package's own relative paths |
| Allowed extensions | `.png`, `.dds` for v1. Adding one later is a version bump, not a config toggle |
| Entry count cap | Proposed, unset. Must exist |
| Uncompressed size cap | Proposed, unset. Must exist, and must be checked against the declared total before extraction begins |
| Compression ratio cap | Must exist. This is the zip-bomb control |

### Refusals, applied to every archive entry before a single byte is written

```text
REJECT the whole package if any entry:
  contains ".."  in any position, before or after normalization
  is absolute ("/x", "C:\x", "\\server\share")
  contains a drive letter or a UNC prefix
  contains a NUL byte or a control character
  uses "\" as a separator
  normalizes to a path outside the install root
  is a symlink, hardlink, reparse point, or device node
  is a directory entry that collides with a file entry
  collides case-insensitively with another entry
  uses a Windows reserved name (CON, PRN, AUX, NUL, COM1-9, LPT1-9)
  ends in a dot or a space
  is absent from the manifest, or present with a different hash
```

**Refuse whole, never sanitize.** A package with one traversing entry is rejected in full. Silently rewriting the path installs something the author did not describe, and the user has no way to know which of the two happened.

### Install mechanics

Extract to `mods/staging/<id>/`, verify every hash against the manifest, then atomically swap into `mods/active/`. Record the exact reverse map so uninstall removes precisely what was added and restores precisely what was displaced. **JSGME's `!BACKUP` is the right idea implemented badly, and the current installer `DelTree`s it on update (verified: `rebornsetupv10.iss` line 199 removes the entire `MODS` folder).** Losing the restore data is how an overlay manager becomes a one-way operation.

**Open:** whether community packages are signed, and by whom. First-party packages are signed with the section 5 key. The mod package format itself is Open per questionnaire 11.7, so everything above is **proposed**, not a description of anything built.

---

## 8. USB write safety

The target is not a blank drive. **The Xbox `Step 2 - Transfer to Xbox` payload holds `GC\MemoryCardA.USA.raw` at 16,777,216 bytes and `SRAM.raw` at 68 bytes** (reported), alongside `Cache\GameCovers`, `Config`, `GameSettings\GN7E69.ini`, `Load\Textures` and `NSR.png`. **A memory card is the one file on that drive a user cannot regenerate.**

| Rule | Reason |
|---|---|
| Enumerate **removable** volumes only, never fixed disks | The failure this prevents is writing 1.5 GB over someone's `D:` drive |
| **Never preselect** when more than one removable drive is present | Already the rule in `INFORMATION_ARCHITECTURE.md` section 8 |
| Refuse the volume holding the OS, the install root, or the data root, even if it enumerates as removable | Portable installs exist |
| Show capacity, free space, volume label and existing contents before the confirm | The user recognizes their own drive by its label, not by a letter |
| Check free space against the computed payload size **before** the first write | A half-written transfer is worse than a refused one |
| Write to temporary names, verify by hash, then rename into place | An interrupted copy never masquerades as a complete file |
| **`GC\` is read-only to the wizard.** It is never written, never deleted, never included in a "clean" step | This section's opening paragraph |
| There is no format, wipe or clean-the-drive operation. Not behind a confirm, not behind an advanced toggle | It has no legitimate use here |
| Flush and safe-eject, and say so | Removing a drive mid-flush is the most common real-world corruption |

### Preserve and replace lists (proposed)

```text
REPLACE (managed by the wizard)   PRESERVE (never written)
  Load/Textures/<game-id>/          GC/                (memory cards, SRAM)
  Config/*.ini                      anything the wizard did not place
  GameSettings/GN7E69.ini
  Cache/GameCovers/
  NSR.png
```

**Verified from the package:** `Config\Dolphin.ini` contains `ISOPath0 = E:\Games`, `GFXBackend = D3D12` and `SlotA = 1`; `Config\GCPadNew.ini` contains `Device = WGInput/0/Xbox One Game Controller`. Questionnaire 7.7 and 7.8 record both as unverified research items; **they are now verified and those items can close.**

**Because the design is a file mover, there is no Device Portal, no app deployment, no certificate and no console pairing. Questionnaire 7.9's "console pairing credentials, Windows credential store, never logged" describes a credential class that does not exist. Retire the item rather than carrying it into this document as a requirement.**

---

## 9. The netplay gate: compiled-in, not defeatable

### What the code actually does

```cpp
// NflsStatTracker.cpp, verified
bool NetplayGated()
{
  static const bool offline_ok =
      File::Exists(File::GetUserPath(D_USER_IDX) + "nsr_track_offline.txt") ||
      File::Exists("nsr_track_offline.txt");
  return offline_ok || NetPlay::IsNetPlayRunning();
}
```

Three properties, all **verified**:

1. **Two defeat paths.** The user path and the bare filename. The bare filename resolves against the **process working directory**, and `NSR_UpdateChecker`'s `LaunchDolphin()` sets `startInfo.WorkingDirectory = appPath` (verified, `Program.cs` line 96), where `appPath` is the folder holding the executable. **A file next to the game executable defeats the gate exactly as one in `User\` does.** Stripping `User\nsr_track_offline.txt` at packaging time closes one of two doors.
2. **Evaluated once per process.** `static const bool` means the gate state is fixed at first call and cannot change without restarting the emulator.
3. **The same two-path pattern guards `nsr_debug.txt`** (verified, near line 1075).

`GameGated()` accepts `GN7E69`, `FR4N31` and `CR4E12` (verified), so the offline path applies to every Reborn build, not just one.

### Why "compiled-in" has to mean compile-time

**Verified:** Dolphin's command line accepts `-C/--config` for arbitrary config overrides. The option is **defined** at `Source\Core\UICommon\CommandLineParse.cpp` line 95, `parser->add_option("-C", "--config").action("append").metavar("<System>.<Section>.<Key>=<Value>")`, and it is **consumed** at line 131, `if (options.is_set_by_user("config"))` inside `AddConfigLayer`. (An earlier draft cited line 131 as the definition. The claim was right; the citation pointed at the consumer.) **Any gate expressed as a Dolphin setting is defeatable from the command line, which is a downgrade from a file check, not an upgrade.**

Also **verified by absence:** a source-wide grep for `netplay-host`, `netplay-client` and `netplay-traversal` across `dolphin-exgl\Source` returns **zero hits**. The fork has no NetPlay command line options at all. The online prototype nonetheless launches Dolphin with `--no-gui --netplay-host --netplay-traversal` and `--netplay-client --netplay-traversal --netplay-host-code <code>` (reported), flags that do not exist, which is why `netplay_uia.ps1` UI automation exists. That has a licensing consequence; see section 18.

### Proposed fix

```cpp
bool NetplayGated()
{
#ifdef NSR_ALLOW_OFFLINE_CAPTURE
  return true;      // debug CI job only; never defined in a release build
#else
  return NetPlay::IsNetPlayRunning();
#endif
}
```

Release builds contain **no file check to find**. A build flag, not a file, enables offline capture for testing. Packaging still strips both text files, but as defense in depth rather than as the control. The same treatment applies to `nsr_debug.txt`.

### Why this is urgent rather than tidy

**Both override files are present and zero bytes in the selected runtime, dated 2026-07-27 20:38** (reported). The runtime's own README states the consequence in its own words: *"If it ships, every player's OFFLINE games against the CPU get captured and uploaded, and if they happen to pick two league teams, those fake results land on a real fixture."*

**The precondition for harm is met and the outcome is unknown.** That is a weaker statement than "the harm has occurred", and it is the strongest one the evidence supports.

`uploaded.txt` is **verified** to contain six entries: one unhashed legacy line (`Tri_City_Wolfp_vs_Shadow_Knights_001.json`) and five content-hashed lines, of which four carry Creation League team names (`Outlaws_vs_Shadow_Knights_001`, `Outlaws_vs_Street_Crusade_002`, `_003`, `_004`). The README explicitly told the tester to use NFL teams and not league teams, precisely to avoid resolving to a real fixture.

**What the ledger proves:** the client sent those payloads and recorded a response it treats as success. **What it does not prove:** that `ingest.php` accepted them, that it resolved them to a fixture, or that anything durable changed on the live site. The ledger is a client-side record of send-and-acknowledge; **it is not evidence of a server-side write.** No `ingest.php` source, server log or database row was inspected. **The disposition is unknown**, and it is carried as open item 2 rather than concluded in either direction.

### The server side is the only durable gate

Everything in this section is a **client-side** control, and this document has just finished demonstrating that a client-side gate has already failed once. **Verified again during this pass:** `CREATION LEAGUE AND NEW DOLPHIN\User\nsr_track_offline.txt` and `...\User\nsr_debug.txt` both exist, both 0 bytes, both dated 2026-07-27 20:38, and `...\User\LeagueGameStats\uploaded.txt` exists beside them. **The gate is open in the selected runtime right now.** Whether that folder is the one a release would be cut from is a packaging question this document does not answer, but the state is not hypothetical. **A control that is one file placement away from off is not the control that should be protecting a competitive record.**

**Requirement, proposed, and it belongs here rather than only in open item 7:** the replacement ingest endpoint must **reject offline captures server side**, on its own authority, and must not rely on the client having told the truth about where a result came from.

| Property | Requirement |
|---|---|
| Authority | The server decides whether a submission may affect a rating. **A field in the payload saying "this was online" is a claim, not an authorization** |
| Binding | A rating-affecting submission is accepted only against a context the **server itself** issued: a match id it created for a lobby it observed, tied to the authenticated account submitting it |
| Unsolicited submissions | A capture with no server-issued match context is accepted as **personal history only**, or refused. It never resolves to a fixture |
| Both participants | A fixture result requires submissions from, or corroboration across, the accounts the server placed in that match. One client cannot write both sides |
| Idempotency | The `uploaded.txt` content hash becomes the idempotency key, so a resend is a no-op rather than a second result |
| Held for review | `sides_verified: false` is a server-side hold, not a client-side hint. See section 13 |

**This is the control that makes the compile-time gate defense in depth rather than the only thing standing between a CPU game and a league standing.** `ONLINE_AND_RESULTS_PROTOCOL.md` section 10.4 reaches the same conclusion about routing from the launcher's own knowledge of the launch; the point here is narrower and stronger, which is that even a correct launcher must not be trusted by the server. The wire contract that expresses this does not exist yet (open item 7), but **the requirement is settled even though the schema is not.**

---

## 10. Update path trust chain

Today's update path has exactly one safe property, and it is worth naming before it is replaced.

**Verified from `C:\Users\Adam\source\repos\NSR_UpdateChecker\Program.cs`** (namespace `NFLStreetRebornLauncher`): `UPDATE_URL = "https://nflstreetreborn.com/api/version.txt"`, `DOWNLOAD_URL = "https://nflstreetreborn.com/download"`, `CURRENT_VERSION = "2.1"` hardcoded. It downloads a plain-text version string, compares it, and on mismatch calls `Process.Start(DOWNLOAD_URL)` to open a browser. **It never downloads or executes anything itself.** An attacker who controls that endpoint can make a false update claim and nothing more.

This closes an item both `COMPONENT_INVENTORY.md` section 12 and questionnaire 18.10 record as unknown.

**The launcher takes on an obligation the current checker does not have**, because it will download and execute. The consequences:

| Property | Update checker 2.1 | Launcher (required) |
|---|---|---|
| Fetches a version claim over TLS | Yes | Yes, and not sufficient |
| Downloads the artifact | No | Yes |
| Executes it | No | Yes |
| Verifies a signature before use | N/A | **Mandatory.** The signed manifest is the gate, not TLS |
| Trust anchor | Certificate chain to the host | **Pinned public key in the client** |
| Behavior on failure | Opens a page | Refuse, retain the previous version, offer repair |
| Refuses an older, validly signed manifest | N/A | **Mandatory.** Signature validity is not freshness. See below |

### Rollback and downgrade (proposed)

**Signature verification alone does not defend this chain, and the gap is worth stating precisely.** Release hosting is Open per 23.8, which means an attacker who controls hosting is inside the threat model by the document's own admission. Such an attacker cannot forge a manifest. **They can replay a real one.** Serving a genuinely signed prior manifest, one for a build with a known defect, a known crash, or a since-removed control, passes every requirement in the table above: valid signature, correct pinned key, matching hashes, fail-closed logic never triggers. The client downgrades itself and believes it verified something.

**The control is a monotonic version floor, and it has to be in the signed bytes.**

| Rule | Detail |
|---|---|
| Store the floor | The client persists the highest `sequence` it has ever **successfully applied**, per component, in its own data root |
| Refuse below the floor | A manifest whose `sequence` is lower than the stored floor is **refused as if the signature had failed**, with the same fail-closed handling and the same message class |
| Equal is a no-op | Equal `sequence` means already applied. Verify local hashes, do not reinstall |
| The floor never decreases automatically | Not by an update, not by a manifest field, not by a server instruction. **A signed manifest cannot lower the floor**, or it is not a floor |
| Deliberate downgrade is a user action | If a real regression needs a rollback, the user performs it explicitly from a screen that names the version and says it is going backwards. The floor is then reset locally, by the user, on that screen |
| Freshness, not just order | The floor stops replay of an older manifest. It does **not** stop an attacker freezing a user on the current version by withholding new ones. Mitigating that needs an expiry or a signed freshness claim, and **neither is designed. Marked open** |

**Release hosting is Open (23.8), and that choice determines what this chain actually protects.** Signature verification plus a monotonic floor is what makes the hosting choice non-critical; signature verification on its own is not, because a replayed signature is still a valid one. That is a good argument for doing both first, and for treating them as one control rather than two.

One more item retires here: `rebornsetupv10.iss` line 273 calls `ShellExec('runas', ExpandConstant('{app}\Reborn Mod Manager (JSGME).exe'), ...)`, elevating a 2010 third-party binary that ships without a license file (**verified**). **The launcher must never elevate a third-party binary.** Nothing in the new design requires elevation at all.

---

# Part B. Privacy

## 11. The position, and the one place it is not held

The drafted copy is correct and should be reused verbatim rather than rewritten:

> "Reborn collects no analytics and no usage data. Offline games stay on this PC. Ranked and casual results are public because they affect ratings."

> "Online play connects you directly to your opponent, so they can see your IP address. That's how the netcode works and it isn't unique to Reborn."

And from the crash page: *"Nothing is sent anywhere unless you choose to share it."*

### Where the position fails today

| Payload | `[Analytics]` block | Effect |
|---|---|---|
| `Files\Reborn Dolphin\User\Config\Dolphin.ini` | `ID = de6680a75ec16a1d97fb464825c34211`, `Enabled = False`, `PermissionAsked = True` (**verified**) | Correct. Prompt suppressed, reporting off |
| Selected runtime `User\Config\Dolphin.ini` | **Byte-identical three lines** (**verified**) | Correct |
| Xbox `Step 2\Config\Dolphin.ini` | `ID = d13f3034d7c4d1bc575151c5a3595c43`. **No `Enabled`. No `PermissionAsked`** (**verified**) | Dolphin's defaults apply (`MAIN_ANALYTICS_ENABLED` false, `MAIN_ANALYTICS_PERMISSION_ASKED` false, **verified** at `MainSettings.cpp` 387 to 389). **The consent prompt appears on the console and the user can accept it**, at which point Dolphin reports to `https://analytics.dolphin-emu.org/report` (**verified**, `DolphinAnalytics.cpp` line 43) |

**Two distinct problems.** The Xbox one is that the prompt exists at all. The shared one is that **every PC install ships the same analytics ID**, and every Xbox install ships the same different one. While `Enabled = False` that is inert; the moment anyone flips it, every Reborn install reports as one identity, which is a worse outcome than a per-install ID and a worse outcome than none.

**Fix, on every platform:** write all three keys explicitly, and ship **no** `ID`. Dolphin's own default for `MAIN_ANALYTICS_ID` is the empty string (**verified**, line 386). Since the launcher generates `Dolphin.ini` on every launch (per `DOLPHIN_INTEGRATION.md` section 6), this is a template change, not an ongoing maintenance burden. The Xbox path does not have a launcher, so its config must be corrected in the shipped payload.

---

## 12. Data inventory

**Read this table as a specification, not as a description.** The launcher does not exist yet, so no path in the Stored column exists on any machine today. Every row is marked, per the rule at the top of this document. **Proposed** means this document is asking for it; **verified** means the artifact was read during this pass; **reported** means discovery established it and it was not re-read here.

| Data | Produced by | Stored | Leaves the machine | Marker |
|---|---|---|---|---|
| Capture JSON (`nsr.game.v1`) | `NflsStatTracker` | `stats/queue/`, then `stats/local.db` | **Only if the result is online, or the user opts in** | Producer and schema **verified**; the two paths and the routing rule are **proposed**. Today the emulator writes to `User\LeagueGameStats` and uploads from there |
| Local match history | Launcher | `stats/local.db` | No | **Proposed.** No local history store exists |
| Memory card saves | Dolphin | `saves/` | No | Producer **verified** (`GC\MemoryCardA.USA.raw`, 16,777,216 B); the `saves/` root is **proposed**. Today they live in `User\GC` inside the install root, which is what the uninstaller deletes |
| Music sources | User | `music/sources/` | No | **Proposed.** Embedded music editing is a locked decision with no implementation |
| Mod state, settings, download queue | Launcher | SQLite in the data root | No | **Proposed.** Today mod state is JSGME's `!INSTLOGS`, which is empty |
| Session and refresh tokens | Launcher | **OS credential vault**, never plaintext on disk | Sent to the backend only | **Proposed**, and it is a deliberate reversal: the prototype puts a 30-day JWT in browser `localStorage` (**reported**, section 14) |
| Discord profile (id, username, avatar) | Discord, via the backend | Backend | Already public by design; see section 17 | Scope `identify` and the unauthenticated profile endpoints are **verified**; the backend storage shape is **proposed** |
| Diagnostic bundle | Launcher, **on explicit action only** | `logs/diagnostics/` | Only if the user shares it | **Proposed.** No bundle generator exists; the redaction spec in section 15 is likewise proposed |
| Crash signature counters | Launcher | Local only, per questionnaire 22.3 | **No** | Decision **reported** (questionnaire 22.3); the mechanism is **proposed** |

### The capture schema is a genuine privacy property

**Verified** against `WriteJson` and a real capture file. `nsr.game.v1` writes `schema`, `source`, `captured_at` (UTC), `teams[{name,score}]`, `style`, optional `team_totals`, `sides_verified`, `per_player_matched`, and `players[{name,team,side,slot,digp,stats{12}}]`. The 12 stats are `pass_td`, `rush_td`, `rec_td`, `pick_six`, `fum_td`, `interceptions`, `forced_fumbles`, `fumble_rec`, `tackles`, `sacks`, `thrown_int`, `fumbles`. **None of them is yardage.**

**No IP address, no OS username, no machine identifier and no account identifier is written.** That is worth protecting deliberately: **the new backend must not add identifiers to the payload. Identity belongs on the request, not in the file.** The file is the thing that sits on disk, gets copied into diagnostic bundles, and gets pasted into Discord.

---

## 13. Offline private by default, competitive results public

**All five rows are proposed defaults for a system that does not exist yet.** None of them is implemented anywhere today, and the current shipped-runtime behavior is the opposite of row one: with `nsr_track_offline.txt` present, an offline CPU game is captured and uploaded (section 9). The Marker column says what, if anything, was read from an artifact.

| Case | Default | Reason shown to the user | Marker |
|---|---|---|---|
| Offline game vs CPU | **Local only** | Never leaves the PC unless the user chooses | **Proposed.** Contradicted by current runtime behavior, which is the defect section 9 fixes |
| Offline game, local versus | **Local only** | Same | **Proposed.** Same defect applies |
| Casual online | **Public** | Affects casual rating | **Proposed.** The prototype's `users` table does carry `rating`, `wins` and `losses` and exposes them unauthenticated (**verified**, section 18), so public results are the existing posture |
| Ranked online | **Public** | Affects ranked rating | **Proposed**, same basis |
| Held for review | **Visible to the two participants only** until resolved | The capture is incomplete | **Proposed** as a policy. The **state** is real: `sides_verified: false` occurs in a captured file (**reported**, below) |

**The honest sentence is the reason, not the policy.** "Ranked and casual results are public because they affect ratings" is a claim a user can evaluate. "We may share your data with partners" is not.

**Held for review is a real state, not a theoretical one.** The captured file `Falcons_vs_Chargers_001.json` (Falcons 36, Chargers 24, `captured_at 2026-08-05T04:58:39Z`) has `sides_verified: false` with `per_player_matched: 14` and no `team_totals` (reported). A result whose sides could not be verified must not silently become a public, rating-affecting record.

**Open:** statistics retention (17.10) is unspecified. There is no existing period to inherit.

---

## 14. Discord OAuth and token handling

**The existing scope is already minimal.** **Verified:** `auth.js` line 21 requests `scope: 'identify'`, a single value. That returns the account id, username, global name and avatar. It does **not** return email (that needs `email`), guild membership (`guilds`), connections, or anything about messages. Keep it at exactly this.

Three prototype behaviors that must **not** carry forward:

| Prototype behavior | Evidence | Replacement |
|---|---|---|
| 30-day JWT lifetime | **Verified:** `jwt.sign(..., { expiresIn: '30d' })` at `auth.js` lines 65 and 118 | Short-lived access token plus a refresh token. Revocable server side |
| Token in browser `localStorage` | Reported: `AuthContext.jsx` lines 11, 26, 32, so it sits in plaintext in Electron's user data folder | **OS credential vault**, per questionnaire 13.5 |
| Soft socket authentication | Reported: the `io.use` middleware verifies the JWT inside a `try/catch` and calls `next()` regardless, so an invalid token still connects | **Fail closed.** No token, no socket |

Two more rules:

- **The Discord client secret never ships in the desktop client.** A desktop application is a public OAuth client and cannot keep a secret. Only the Railway backend holds it. Use the authorization code flow with PKCE and a single-use `state`.
- **The Discord token is never passed to Dolphin.** Already stated in `DOLPHIN_INTEGRATION.md` section 6; repeated here because it is a security invariant and not a convenience.

---

## 15. Diagnostic bundles and redaction

**Only on explicit action.** "Create a diagnostic file" is one of the six recovery actions on the crash page, and the page already says *"Nothing is sent anywhere unless you choose to share it."* No automatic crash upload, per questionnaire 22.1 and 22.3.

### The leak is measured, not hypothetical

**Verified:** exactly six files in the **currently shipped** payload contain the developer's Windows username.

```text
Files\Reborn Dolphin\JSGME.ini                    FullPath=C:\Users\Adam\Desktop\...
Files\Reborn Dolphin\NSR_UpdateChecker.exe        embedded Debug PDB path
Files\Reborn Dolphin\ReShade.ini                  IntermediateCachePath=C:\Users\Adam\...
Files\Reborn Dolphin\ReShade.log
Files\Reborn Dolphin\ReShade.log1
Files\Reborn Dolphin\User\Cache\gamelist.cache
```

The selected runtime leaks the same class plus a development folder name: `Dolphin.ini` `DefaultISO = C:\Users\Adam\AppData\Roaming\NFL Street Reborn\Games\NSR.iso` and `Qt.ini` `lastdir=C:/Users/Adam/Desktop/Dev Copy NFL Street Reborn 2/...` (reported).

**Every one of those is a config file or a log, which is exactly what questionnaire 22.4 puts inside a diagnostic bundle. The developer's own leak is a preview of the user's.**

### Proposed redaction spec (22.5 is Open; this is the spec it needs)

| Field class | Rule |
|---|---|
| Home directory path | Replace with the literal token `<HOME>` |
| OS username, wherever it appears, including inside longer strings | Replace with `<USER>` |
| Install root, data root | Replace with `<APP>`, `<DATA>` |
| Paths outside managed roots (user-chosen ISO, music sources) | Keep the **basename only**, drop the directory |
| Machine name, volume serial, hardware serials | **Remove**, do not mask |
| Account ids, Discord ids, any token or secret | **Remove entirely.** A masked token still confirms a token exists and its length |
| IP addresses, own and peer | **Remove** |
| GPU model, driver version, OS version and build, CPU model | **Keep.** This is the payload's whole purpose |
| Component versions, file hashes, mod list, graphics config, controller identity strings, exit code | **Keep** |

Three rules about the mechanism, which matter more than the field list:

1. **Redaction happens when the bundle is written, not when it is shared.** The file on disk is already redacted, so an accidental share cannot leak.
2. **Not reversible.** There is no key that recovers the original. If support needs a value, they ask the user for it.
3. **The bundle is a single file in `logs/diagnostics/`**, never written to the desktop, with the path shown so the user can open and read it before sharing. A preview pane is a nice-to-have; **redacted-at-write is the requirement**, because a preview the user skips must still be safe.

---

## 16. The peer-to-peer IP disclosure

**What is decided and can be stated now:**

- The disclosure appears **in onboarding**, before the first online action, not buried in settings (questionnaire 22.6).
- The drafted sentence is the one to ship, unchanged.
- **Raw IP addresses never become public profile data.** They are not stored in the match record, not shown in any UI, not returned by any endpoint, and not written to a server log beyond transient connection handling.

**What cannot be stated accurately yet, and must not be guessed:** the precise exposure. **Verified:** Dolphin's default traversal server is `stun.dolphin-emu.org` and its default lobby is `https://lobby.dolphin-emu.org` (`NetplaySettings.cpp` lines 17 and 22), and the prototype relays Dolphin-generated traversal codes over its own socket (reported, `match.js` line 45). Whether the final design lands a direct peer connection in all cases, falls back to a relay, or runs Reborn's own traversal service is undecided. **Until that is chosen, the sentence describing what the opponent sees and what a third party sees cannot be written correctly, so it should not be written at all.**

Two related unknowns, recorded rather than assumed: whether using `stun.dolphin-emu.org` for a third-party product's matchmaking at scale is acceptable to the Dolphin project, and what an alternative traversal host would cost. Neither has been established and no alternative is budgeted.

---

## 17. Chat retention

**There is no chat.** **Verified by absence:** a grep for chat and message across `server/socket` and `server/routes` returns only three error-message emits in `custom.js`. The schema has four tables (`users`, `matches`, `quit_reports`, `custom_rooms`, **verified**) and none stores a message, an IP address, or a consent record. **Chat retention is a fresh policy decision with no legacy behavior to preserve.**

Proposed, and marked as proposed:

| Item | Position |
|---|---|
| What is stored | Room id, author account id, timestamp, message body. **Nothing else.** No IP |
| Retention | Only the moderation window. **The exact period is Deferred** (questionnaire 15.13) |
| Deletion | A scheduled job, not a manual task, so the policy is what actually happens |
| Visibility | Lobby chat to the lobby. No global archive, no public search |
| Account deletion | Messages are anonymized, not orphaned. See section 18 |

One thing that must not be claimed: **Dolphin's own in-emulator NetPlay chat has not been examined.** Whether it is reachable in the Reborn flow, whether it is enabled, and whether it is logged anywhere are all unestablished. This section covers launcher-owned lobby chat only, and saying otherwise would be a guess.

One thing that must be fixed regardless: **`custom_rooms.password` is stored as plain `TEXT`** (**verified**, `schema.sql` line 42). A private lobby password is a shared secret, not an authenticator, so hashing it is arguably overkill; storing it in the clear next to the room record is still the kind of default that gets copied into the next table. Store a hash, compare on join.

---

## 18. Account export and deletion

**None exists.** **Verified:** the only deletion is `DELETE /admin/users/:id` behind `requireAuth` plus `requireAdmin`. `GET /users/:username` and `GET /users/leaderboard/ranked` are unauthenticated, so profile and rating data is **already public by design**.

Questionnaire 13.6 requires unlink, sign out, export, and delete. The hard part is deletion, and it needs to be said plainly rather than papered over.

| Action | Behavior | Said to the user |
|---|---|---|
| Sign out | Local tokens cleared from the vault, refresh token revoked server side | "Signed out on this device" |
| Unlink Discord | Discord id removed, account becomes unusable for sign-in until relinked | "You will not be able to sign in until you link again" |
| Export | On-demand JSON of everything keyed to the account: profile, match rows, ratings history, chat authored within the retention window | Generated on request, downloadable |
| Delete | **Anonymization, not erasure.** Display name replaced with a stable pseudonym, Discord id removed, avatar removed, chat authorship anonymized. **Match rows are preserved** | "Your matches stay, because other players' ratings were calculated from them. Your name and Discord link are removed" |

**A user who expects erasure and receives anonymization has been misled, so the deletion confirm must say which one it is.** Deleting the match rows would silently corrupt every opponent's rating history, which is a worse outcome for more people.

**Open:** there is no consent record and no data inventory per account today. If a policy exists, it needs a version and a stored record of what the user accepted. The prototype cannot answer any of this because it has none of it.

---

# Part C. Licensing

## 19. Dolphin GPLv2+ and the exact blocking chain

Dolphin is GPLv2 or later. Distributing a modified binary obliges attribution, availability of the **corresponding source** for the version distributed, and no additional restrictions.

```text
  compiled-in bearer token still live
              │
              ▼
  cannot make adampreddy/reborn-dolphin public
  (the blob at eca379f contains the value; publishing the
   repo publishes the historical value even if HEAD is clean)
              │
              ▼
  corresponding source is not available to recipients
              │
              ▼
  distributing the modified Dolphin binary is not lawful
              │
              ▼
  the September 12 release cannot ship the runtime
```

**Rotation is on the critical path whether or not it feels urgent. It takes minutes and it gates publication.** That statement already appears in `SYSTEM_ARCHITECTURE.md` section 8; what changes here is the reason. **Because the commit is already pushed (verified), rotation is remediation rather than a precondition.** History rewriting is not a substitute: unreachable objects persist, and a clone may already exist.

### What is shipped today, and why it is not enough

| Artifact | Status | Sufficient? |
|---|---|---|
| `Files\Reborn Dolphin\license.txt`, GPLv2 text, 17,987 bytes, dated 2021-10-31 (reported) | Ships | **No.** The license text is not the source, and it is not an offer |
| Selected runtime `COPYING` plus `LICENSES\` containing exactly `BSD-3-Clause.txt`, `CC0-1.0.txt`, `GPL-2.0-or-later.txt`, `MIT.txt` (**verified**) | Ships | **No**, same reason, and the folder is incomplete (section 22) |
| Any named repository, written offer, or build instructions | **Absent from both payloads** (reported) | The gap |

**What satisfies it:** a public repository at the exact commit that produced the shipped binary, named in the shipped documentation and in the launcher's About screen, together with the build instructions. **The cross-platform CI workflow already committed to the fork is most of those build instructions**, which makes this cheaper than it looks.

### One obligation that is about to grow

**Verified:** the fork has zero NetPlay command line options. The prototype passes `--netplay-host`, `--netplay-client` and `--netplay-traversal`, none of which exist, which is why `netplay_uia.ps1` UI automation exists (reported), and UI automation does not port off Windows. **Any launcher-driven NetPlay entry point is therefore a new modification to a GPLv2+ component, so it enlarges the corresponding-source obligation rather than merely inheriting it.** That is an argument for making the change deliberately and upstream-shaped, not an argument against making it. It also settles questionnaire 15.11, which records the question as Open.

---

## 20. Repository structure

| Repository | Visibility | License | Obligation | Verdict |
|---|---|---|---|---|
| `adampreddy/reborn-dolphin` | **PRIVATE** (**verified**), `licenseInfo.key: other`, `pushedAt 2026-08-30T02:04:11Z` | Declared as "other", which is **wrong** | **Must become public and must declare GPL-2.0-or-later** | Reuse, correct the license metadata |
| `adampreddy/nfl-street-reborn-launcher` | PRIVATE, no license set (reported) | None | Needs one before release, whatever it is | Reuse |
| Upstream `BlueSwordM/dolphin` | Public | GPLv2+ | The fork's `origin`, set for both fetch and push (**verified**) | Reuse |

**Keep them separate. Do not vendor the fork into the launcher repository.** The launcher is a separate program that runs Dolphin as a child process, which is the standard and defensible position; vendoring the source into the same tree invites the argument that they are one work. Questionnaire 23.3 already decides this, and the current structure already satisfies it.

One practical note: `origin` on the fork points at `BlueSwordM/dolphin` **for push as well as fetch** (verified). That is a foot-gun. Set `origin`'s push URL to the Reborn remote or make it explicitly invalid.

---

## 21. Attribution owed

| Owed to | For | Currently credited | Action |
|---|---|---|---|
| **BlueSwordM** | The custom Dolphin build | Yes, `README.txt` line 14 (**verified**) | Carry forward |
| **nfl_numbers** | Jersey number textures | Yes, `README.txt` line 15 (**verified**) | Carry forward |
| **Ludiyolo252** | Gecko codes "Remove GameBreaker 2" and "GB Takes Longer (v2)" | **No.** Credited only inside `GameSettings\GN7E69.ini` (**verified**), which no user reads | **Add** |
| **Ludiyolo258** | Gecko code "Point Remover" | **No**, same (**verified**) | **Add** |
| The Dolphin project | The emulator itself | Only via the GPL text | Add, by name, with the fork commit |
| ExGL | Renderer work carried in the fork | Not user-facing | Add |
| Brand and cover artwork authors | `NSR.png`, `CR4E12.png`, the logo files | **Provenance unknown** (questionnaire 1.9) | **Open.** See open items |

**Attribution belongs in one generated place, not a hand-maintained text file.** The evidence for that is section 22: the fork's own `Externals/licenses.md` went stale in two directions without anyone noticing. Proposed: an About > Credits and Licenses screen rendered from a machine-readable component register that lives in the repository and is checked in CI.

---

## 22. Third-party component register

| Component | Evidence | License | Ships on | Status | Action |
|---|---|---|---|---|---|
| **Dolphin (fork of `BlueSwordM/dolphin` at `7f4875e`)** | Runtime SHA-256 `617321bf37ac7b2f9bdfcd3af803a019e862591259c770e9e688d53834df1338` | **GPLv2+** | All | Shipping today | **Publish source.** Section 19 |
| **Qt6** | `Qt6Core.dll`, `Qt6Gui.dll`, `Qt6Widgets.dll` in the runtime folder (**verified**) | **LGPL** via Dolphin | Windows runtime | Shipping | **LGPL text is absent from `LICENSES\` (verified).** Add it. Section 23 |
| **SDL2** | `SDL_MAJOR_VERSION 2` in the fork (**verified**) | zlib per `licenses.md` | Via Dolphin, all | Shipping | Fine |
| **SDL3** | New launcher controller service | zlib, **unverified for SDL3 specifically** | Launcher, all | New | Verify and record |
| **FFmpeg (Dolphin's)** | `Externals/FFmpeg-bin/dolphin-build-x64.sh` configures `--enable-gpl` (**verified**, line 22) and `--disable-everything` (line 36) | **GPL build** | Windows runtime | Shipping | Consistent with Dolphin. Section 24 |
| **FFmpeg (launcher's)** | `music_editor.py` shells out to `ffmpeg` and `ffprobe` from PATH; nothing is bundled today (reported) | **Unchosen** | All | **New decision** | Section 24 |
| **ReShade injector, version 6.7.3.2148** | `dxgi.dll`, 5,487,616 bytes, dated Feb 28, in the selected runtime and **absent from everywhere under `Files\`** (**verified**). The version comes from the runtime's own `ReShade.log` line 1: `Initializing crosire's ReShade version '6.7.3.2148' (64-bit) loaded from ...\dxgi.dll` (**verified**). Note the installer copy's `ReShade.log` line 1 records **6.5.1.2008** loaded from a `Files\Reborn Dolphin\dxgi.dll` that no longer exists, so ReShade was run from that folder in the past even though the DLL does not ship (**verified**) | **Unverified.** No ReShade license file exists in either payload; the binary's embedded MIT, Apache-2.0 and SIL OFL notices cover its own dependencies, not ReShade | Windows only | **New to ship** | **Verify terms before release. Do not assert a license** |
| **ReShade shader collection (`reshade-shaders`)** | **Already distributed.** `Files\Reborn Dolphin\reshade-shaders` is 28,800,956 bytes across 62 files: 11 `.fx`, 26 `.fxh`, 25 `.png` textures. `rebornsetupv10.iss` **line 44** ships `Files\Reborn Dolphin\*` with `recursesubdirs createallsubdirs` and excludes only `User\*`, so the whole folder is in the installer (**all verified**). The selected runtime carries a larger collection, 40,793,846 bytes across 172 files, plus `reshade-presets` (81 files) that the installer folder does **not** contain (**verified**) | **Mixed and partly unstated.** Third-party authorship is explicit in the sources: `Shaders\Deband.fx` opens "Deband shader by haasn" and "Copyright (c) 2015 Niklas Haas". Of the 37 `.fx` and `.fxh` files, **13 carry a Copyright line, 6 name the MIT License, 1 names CC0, and 22 carry no license or copyright keyword at all**. **No `LICENSE` or `COPYING` file exists in the folder** (**all verified**; this is a keyword scan, not a license determination) | Windows, **shipping today** | **Already distributed** | **Treat as remediation, not review.** It is one of only two entries in this table whose terms are unverified **and** already in users' hands; the other is JSGME, which is scheduled for retirement, so this is the one that has to be resolved rather than removed. Enumerate per file, restore the upstream notices, and drop anything whose terms cannot be established |
| **ReShade presets (`reshade-presets`)** | 81 files in the selected runtime, and **absent from `Files\Reborn Dolphin\`** (**verified**), so unlike the shader collection it has never shipped. Contents are `NFL Street Reborn Reshade.ini`, a `stages\` folder of per-field `Field00.ini` through `Field19.ini`, and three dated `stages.bak_*` copies (**verified**) | **Reborn's own work on the evidence available.** No third-party preset filename was found in this folder. Presets are configuration, but they name and configure the third-party shaders in `reshade-shaders`, so they are only distributable to the extent those are | Windows only | **New to ship** | Confirm authorship of every preset before first distribution. Low risk on current evidence, **but not zero and not yet checked file by file** |
| **`ReshadeEffectShaderToggler.addon64`** | 659,968 bytes, 2024-12-23; embeds MinHook and Apache 2.0 text (reported) | **Unverified**, author not established locally | Windows only | **New to ship** | Same |
| **`NSRShaderlessMenu.addon64`** | Reborn work | Team's own, but it links ReShade's add-on API | Windows only | Shipping in the runtime | Check ReShade add-on terms alongside the injector |
| **JSGME** | 1,197,568 bytes, dated 2010-08-08, no license file (reported); elevated by the installer (**verified**, line 273) | Freeware, **unverified** | Shipping today | **Retire** | If it ships during any transition, the terms still matter |
| **`wit`** | `wit-v3.05a-r8638-cygwin64.zip` contains `gpl-2.0.txt`; no tool in `nflstreet2_clean\tools` invokes it and it is absent from the payload (reported) | GPLv2 | **Never shipped** | Developer tool | **Remove from the shipped-component list** |
| **Gecko codes** | `GameSettings\GN7E69.ini` in all three payloads (**verified**) | No stated license | All | Shipping | Credit Ludiyolo252 and Ludiyolo258 |
| **`nfl_numbers` textures** | Permission granted per `README.txt` (**verified**) | Permission, terms not written down | All | Shipping | Record the permission in the register |
| **Inter** | Proposed typeface. **No font file exists in the repository** | SIL OFL (unverified, nothing to check yet) | All | Open | Confirm or choose |
| **Icon set** | Family **not named** in `DESIGN_TOKENS.md` section 12 | Unknown | All | Open | Name it, then review |
| **Brand and cover artwork** | `NSR.png`, `CR4E12.png`, logo variants | **Unknown** (questionnaire 1.9) | All | Open | Establish authorship |
| **Python** | Bundled for music editing | PSF | All | New | Fine, include the text |
| **Tauri, React, Rust crates, Node dependencies** | New | Mixed permissive | All | New | Generate the register from lockfiles in CI |

### The fork's own manifest is stale in two ways that matter

**Verified:** `Externals/licenses.md` (4,885 bytes) names **Qt5** with an LGPLv3 link at lines 61 to 62 while the build links Qt6, and a case-insensitive grep for `ffmpeg` in that file returns **zero matches** even though `Externals/FFmpeg-bin` is bundled. **It lists 39 external projects, not 43**: the file is 80 lines, the entries occupy lines 3 to 79 at two lines each, and `grep -c '^- \['` returns 39. (An earlier draft carried 43 as reported. It is now verified at 39.)

**Fixing that file is part of the corresponding-source obligation being honest, not a cosmetic chore.** A source release whose own license manifest misstates two of its dependencies is a source release that a reviewer cannot rely on.

---

## 23. Qt6 and LGPL

Dolphin links Qt6 dynamically; three Qt6 DLLs sit in the runtime folder (**verified**). LGPL obligations for dynamic linking are modest and are already almost satisfied by the shape of the distribution:

| Obligation | Status |
|---|---|
| Ship the LGPL text | **Not satisfied.** `LICENSES\` contains exactly four files and **no LGPL text of any version** (**verified**), next to three Qt6 DLLs |
| Allow the user to relink against a modified Qt | Satisfied by dynamic linking to DLLs the user can replace |
| State that Qt is used and where to get it | Not satisfied; the manifest says Qt5 |

**Action, cheap and immediate: add the LGPL text to the runtime's `LICENSES\` folder, and correct `Externals/licenses.md` from Qt5 to Qt6.**

**The launcher carries no Qt obligation of its own.** It is Tauri with a system webview. This confirms questionnaire 23.6.

---

## 24. FFmpeg: LGPL versus GPL, and why the choice matters

There are **two** FFmpegs, and conflating them is the mistake to avoid.

| | Dolphin's FFmpeg | The launcher's FFmpeg |
|---|---|---|
| Where | `Externals/FFmpeg-bin`, bundled in the fork | Nothing bundled today |
| Configured | `--enable-gpl` (**verified**) plus `--disable-everything` | **Unchosen** |
| Invoked | In-process by Dolphin | Shelled out to as a separate executable by `music_editor.py` (reported) |
| License consequence | The FFmpeg build is GPL, and it is used by a GPL program. **Consistent. No new obligation** | **A fresh bundling decision with a fresh obligation** |

**Why the choice matters:** an `--enable-gpl` FFmpeg makes that binary GPL-licensed. Bundling it alongside a differently licensed launcher is defensible when it is a separate executable invoked at arm's length, which is exactly the current design. But **an LGPL build removes the argument entirely**, and the only thing it costs is a set of GPL-only components the project may not need at all.

**Recommendation: build or obtain an LGPL FFmpeg for the launcher (no `--enable-gpl`, no `--enable-nonfree`), and confirm it covers the nine declared input formats before committing.** If any required decoder turns out to be GPL-only, that is a decision to make explicitly and record, not to discover during packaging.

Either way, LGPL requires shipping the license text and preserving the user's ability to substitute their own FFmpeg. **Shelling out to a separate executable satisfies that cleanly, which is an additional reason to keep the shell-out design rather than embedding a library.** That preserves the existing `music_editor.py` contract too, so it is free.

**Open:** which build, and which codecs the nine formats actually require. Neither is decided.

---

## 25. Unsigned builds, disclosed honestly

**Verified by absence:** no `.pfx`, `.p12`, `.pem` or certificate exists in the launcher repository, the installer folder, or the Xbox package, and no `.appx`, `.msix` or deployment tooling exists anywhere (reported). Adam's position on signing is recorded verbatim in questionnaire 23.1: not available.

| Platform | What the user actually sees | Confidence |
|---|---|---|
| **Windows** | SmartScreen "Windows protected your PC". The user must click More info, then Run anyway. Reputation accrues **per publisher certificate**, so an unsigned build never accrues it and the warning never goes away | High. This is the current shipped experience |
| **macOS** | Gatekeeper refuses an unsigned, unnotarized application. The user route is right-click Open or an allow in Privacy and Security. On Apple Silicon a binary must be at least ad-hoc signed to execute at all | **Unverified.** No Mac exists in this project, per questionnaire 23.2 |
| **Linux** | No equivalent gate. AppImage and Flatpak carry their own trust conventions | High |

### How to disclose it

1. **On the download page and on the first-run screen**, in the user's own terms: the build is not code signed, here is what your OS will say, here is why (no certificate), here is what to click.
2. **Publish a SHA-256 for every release artifact** on the download page, and give the exact command to check it on each platform. **A published hash is the honest substitute for a signature**, and it is free.
3. **The signed manifest inside the product** (section 5) still protects everything after install, so the unsigned boundary is the first download only.
4. **Never instruct a user to disable protections.** No Defender exclusions, no "turn off SmartScreen", no `xattr -d com.apple.quarantine` as a headline instruction. Those turn a one-time warning into a permanent hole and they train users badly.

**This is a limitation being disclosed, not a problem being solved.** Saying so plainly is the only version of this that respects the user.

---

## 26. Shipping a complete game image

Recorded factually, as the user has already decided it.

**The product ships a finished commercial game image and always has.** `Files\Reborn Dolphin\Games\NSR.iso` is 1,459,978,240 bytes and the installer copies it verbatim; the Xbox package ships `nsrpatch.iso` at the identical size; the compiled installer output sits in `C:\Users\Adam\Desktop\Reborn Installer\Output\` as `NFL Street Reborn Setup.exe` (7,382,037 bytes) plus `NFL Street Reborn Setup-1.bin` (1,629,604,271 bytes), both dated January 11, 2026 (**verified**; the path is given so the figures can be re-checked, which an earlier draft omitted). The "patcher" is not a patcher: `fake_patch(user_iso)` at line 20 sleeps through hardcoded messages, and the file contains no `open(`, no `shutil` and no `hashlib`, so the file the user selects is never read (reported). This is a locked decision and it is not re-litigated here.

**One thing is inconsistent today and can be fixed at zero cost.** `README.txt` line 5 tells the user *"A legally dumped copy of NFL Street 2 for the GameCube, the USA version of the game!"* (**verified**) while the installer copies a finished image and the patcher never opens the user's file. **The user-facing text and the shipped behavior contradict each other. The honest fix is to stop making the claim, not to start enforcing it**, because enforcing it would contradict the locked decision and keeping it makes the product say something untrue.

**NFL and EA trademark and likeness exposure (questionnaire 1.8) is recorded as Open, and there is no evidence in the repository that any legal review has occurred.** That is stated as unanswered rather than resolved in either direction.

---

## 27. Component verdicts

Every existing component this document touches, one verdict each.

| Component | Verdict | Why |
|---|---|---|
| `NflsStatUpload.cpp` (token, `LoadConfig`, HTTP) | **RETIRE** from the runtime | Removing the credential from the client is the fix, not rotating it in place |
| `nsr_ingest.txt` override path | **RETIRE** | Unvalidated redirect primitive, and pointless once no token ships |
| `NetplayGated()` file check | **REPLACE** with a compile-time constant | Two defeat paths, and `-C/--config` makes a config key worse |
| `nsr_debug.txt` file check | **REPLACE** | Same two-path pattern |
| `NflsStatTracker` capture and `nsr.game.v1` schema | **REUSE unchanged** | Verified to contain no personal data. This is a property to protect |
| `uploaded.txt` content-hash ledger | **PORT** | The hashes become the idempotency key on the new endpoint. Idempotency is not authorization: the server-side authority check in section 9 is a separate requirement |
| PC payload `[Analytics]` block | **REUSE** | Already correct: `Enabled = False`, `PermissionAsked = True` |
| Xbox payload `[Analytics]` block | **REPLACE** | Missing both keys, so the consent prompt appears on the console |
| Shared analytics `ID` in all payloads | **RETIRE** | Ship no ID. Dolphin's own default is the empty string |
| `server/.env`, **five** copies (correction 0.8) | **RETIRE** | Values rotate; storage moves to the platform secret store. Delete by enumeration, not by count |
| `ADMIN_SECRET` consumer, `admin.js` `/live` (208-line variant) | **PORT** | Keep the shape. On Railway the transport is not loopback, so `/live` needs authenticated-admin treatment or an internal bind. See section 1.1 |
| `ADMIN_SECRET` producer, `server-app/src/main/instanceManager.js` | **PORT** | Generate per session with `crypto.randomBytes(32)`, pass by environment, never write to disk. The value already has a correct storage answer: nowhere |
| `requireAuth` plus `requireAdmin` guard | **REUSE** | Correct today, at line 29 |
| `io.use` soft socket authentication | **REPLACE** | Must fail closed |
| 30-day JWT in `localStorage` | **REPLACE** | Short-lived token in the OS credential vault |
| Discord OAuth `scope: 'identify'` | **REUSE** | Already the minimum |
| Existing Discord application | **REUSE**, rotate the secret | The app exists; the secret does not belong in a desktop client |
| `custom_rooms.password` as plain `TEXT` | **REPLACE** | Store a hash, compare on join |
| **11** JSGME mod packages (correction 0.9) | **PORT** | 57 texture files under one root, 11 packages plus 2 empty control folders in 13 directories; they migrate cleanly |
| `JSGME.exe`, `!BACKUP`, `!INSTLOGS` | **RETIRE** | 2010 binary, no license file, elevated by the installer |
| `rebornsetupv10.iss` `BackupUserData` / `RestoreUserData` | **RETIRE** | `FileCopy` does not expand wildcards; it has never worked |
| `rebornsetupv10.iss` uninstall `DelTree({app})` | **RETIRE** | Deletes memory cards. Separate roots make it impossible |
| `ShellExec('runas', JSGME.exe)` | **RETIRE** | Never elevate a third-party binary |
| `NSR_UpdateChecker.exe` | **RETIRE** | Debug build leaking a PDB path; unsigned plain-text version check |
| `license.txt` (GPLv2 text) | **REUSE and extend** | Necessary and not sufficient; add the repository and the offer |
| Runtime `COPYING` and `LICENSES\` | **REUSE and extend** | Add the LGPL text sitting next to three Qt6 DLLs |
| `Externals/licenses.md` | **REPLACE** | Names Qt5 while linking Qt6; silent on a bundled FFmpeg |
| `README.txt` acknowledgements | **REUSE and extend** | Add Ludiyolo252, Ludiyolo258, Dolphin, ExGL |
| `README.txt` "legally dumped copy" line | **RETIRE** | Contradicts what the product actually does |
| Dolphin's `--enable-gpl` FFmpeg | **REUSE** | Consistent with Dolphin's own license |
| Launcher FFmpeg | **New**, LGPL build recommended | A fresh bundling decision, not an inherited one |
| ReShade injector `dxgi.dll` (6.7.3.2148), `ReshadeEffectShaderToggler.addon64` | **REUSE on Windows, pending license verification** | New to ship, so review comes before first distribution |
| `reshade-shaders` collection, 62 files, 28,800,956 B | **REUSE on Windows, pending license verification** | **Already distributed** via `rebornsetupv10.iss` line 44. Review is remediation here, not prevention |
| `reshade-presets` | **REUSE on Windows** | 81 files in the selected runtime, **absent** from the installer source folder, so it is a first distribution rather than an existing one |
| `wit` zip | **RETIRE from the shipped-component list** | Developer tool, never shipped |
| Xbox `Step 2\GC\MemoryCardA.USA.raw`, `SRAM.raw` | **RETIRE** | The author's own saves. Removed from the payload and absent from every manifest, so the wizard cannot write them by accident. The drive's own `GC\` directory is not an inherited component and carries no verdict, only the preservation rule: read-only to the wizard except in Restore Xbox Save |
| Questionnaire 7.9 console pairing credentials | **RETIRE** | A file mover has nothing to pair with |
| `MiltyTheGreat/RebornOnline` and `michaelpenixfan2390/nflstreetrebornlauncher` remotes on the `Downloads` working trees | **RETIRE the local trees.** No verdict is available on the remotes themselves | `.env` is gitignored and was never committed in either tracked tree (**verified**), but both remotes are push-configured and this machine cannot see what they hold. See section 4.1 |
| Existing `ingest.php` trust model (client asserts, server accepts) | **REPLACE** | The server must decide what may affect a rating. See section 9 |

---

## Open items

Honest, unpadded. Each of these blocks something or is genuinely unknown.

1. **Blast radius of the compiled-in token is bounded on the client side and unmeasured on the server side.** The earlier draft called it simply unmeasurable, on the premise that the token shipped to every user. **That premise is wrong** (correction 0.7): the publicly distributed binary contains no uploader and no token. Client side the holder set is now the private `adampreddy/reborn-dolphin` history plus hand-delivered runtime folders. What remains unknown is the server side: no `ingest.php` source, no server logs, no rate-limit or audit evidence exists in the inspected material, so **whether the value was ever used by anyone other than the developer cannot be determined here.** Rotation with server-side revocation still closes it.
2. **Server-side disposition of the four Creation League test captures is unknown.** `uploaded.txt` proves they were sent and acknowledged by the client's definition. Whether they resolved to real fixtures on the live site cannot be verified from here.
3. **The replacement credential is undesigned.** Direction is settled (short-lived, OS vault). Lifetime, refresh mechanism, per-install versus per-account, and the pre-sign-in identity are all unspecified. No design, no prototype.
4. **Manifest signing key custody is Open** (18.8). No key material exists. Algorithm, storage, CI involvement, rotation and loss-recovery are unanswered. Section 5 recommends; it does not decide.
5. **The mod package format is Open** (11.7). Everything in section 7 is proposed policy against a format that does not exist. The entry-count, size and ratio caps have no values yet.
6. **Redaction rules are Open** (22.5). Section 15 proposes a spec. Whether the user sees a preview, and the exact bundle location, are not settled.
7. **The new stat ingest wire contract does not exist.** 14.9 decides "completely new backend"; there is no schema, no auth model, and no idempotency design beyond the existing content-hash ledger.
8. **Statistics retention (17.10) and chat moderation retention (15.13) are undecided.** There is no existing period to inherit.
9. **The peer-to-peer exposure is not characterized.** Direct in all cases, relay fallback, or a Reborn-operated traversal service is unchosen, so the precise disclosure sentence cannot be written yet.
10. **Whether using `stun.dolphin-emu.org` at scale is acceptable to the Dolphin project is not established**, and no alternative traversal host is chosen or budgeted.
11. **In-emulator NetPlay chat is unexamined.** Whether it is reachable, enabled, or logged in the Reborn configuration is unknown. Section 17 covers launcher-owned chat only.
12. **ReShade injector 6.7.3.2148 and `ReshadeEffectShaderToggler.addon64` redistribution terms are unverified.** The version is verified from the runtime's `ReShade.log` line 1; the **terms** are not. No license file exists in either payload and the embedded notices cover only their dependencies. Both need an external check **before first distribution**, which is still possible because neither has shipped. **Do not assert a license for either.** The shader collection is a harder case and is item 25, because it has already shipped.
13. **JSGME redistribution terms are unverified.** It is scheduled for retirement, but if it ships during any transition the terms still apply.
14. **The FFmpeg build for the launcher is unchosen**, and which codecs the nine declared input formats require has not been determined.
15. **The icon family is unnamed** in `DESIGN_TOKENS.md` section 12 and no font file exists in the repository, so Inter is proposed rather than confirmed. Two license line items with no artifact behind them.
16. **Brand and cover artwork provenance is unknown** (1.9). Whether any of it is original team work is not established.
17. **NFL and EA trademark and likeness exposure (1.8) has no evidence of legal review.** Recorded as unanswered.
18. **Release hosting is Open** (23.8), and it determines what the update trust chain protects.
19. **`reborn://` registration scope is unspecified** on all three platforms, and there is no precedent because nothing registers the scheme today.
20. **No account lifecycle design exists:** no consent record, no per-account data inventory, no export format, no deletion semantics beyond section 18's proposal.
21. **Whether the 2.1 installer keeps operating during the build** (24.7) is Open. That decides whether the save-deleting uninstaller and the non-functional backup stay live for another two weeks. It is a data-loss decision this document can surface but cannot answer.
22. **Whether any of this warrants user notification** (the four uploaded league captures, the Xbox analytics prompt, the shipped developer username paths) is a judgment call with no precedent. There is no incident policy, no prior disclosure, and no user-facing changelog convention to follow.
23. **macOS Gatekeeper behavior for an unsigned build is unverified.** No Mac exists in this project.
24. **Who else holds the `Downloads` working trees, and who owns the Discord application.** Both tracked trees are push-configured to `MiltyTheGreat/RebornOnline`, and one also to `michaelpenixfan2390/nflstreetrebornlauncher`. `.env` is gitignored and was never committed in either (**verified**), so the secrets did not reach the remotes through these trees. Whether they reached them another way, and whether the Discord application is owned by an account other than Adam's, **cannot be determined from this machine.** If the application is not Adam's, rotation step 3 cannot be performed and a new application is needed instead. See section 4.1.
25. **The `reshade-shaders` collection has already shipped with unestablished terms.** 62 files, 28,800,956 bytes, no `LICENSE` file, 22 of 37 shader sources carrying no license or copyright keyword at all, distributed via `rebornsetupv10.iss` line 44. It is one of only two entries in section 22 that are both unverified and already in users' hands, and the only one of the two that is not scheduled for retirement, so it is **remediation rather than review**. The ReShade injector is the more visible name and the easier problem, because it has not shipped yet.
26. **Update freshness has no control.** Section 10 adds a monotonic `sequence` floor that stops replay of an older signed manifest. **It does not stop an attacker who controls hosting from freezing a user on the current version by withholding new manifests.** An expiry field or a signed freshness claim would address it; neither is designed and neither is in the schema.
27. **Nothing in this document is implemented.** It is a proposal awaiting the step 7 decision checkpoint, in line with the other two architecture documents.
