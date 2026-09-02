# Reconciliation Rulings

## NFL Street Reborn Launcher

**Date:** August 30, 2026
**Status:** ISSUED, NOT YET APPLIED to the architecture documents.

### What this is

Five auditors swept the six architecture documents for internal
contradictions and found **172**. An adjudicator then issued **one ruling per
conflict**, grounded in the locked decisions, verified evidence from the real
files, and, where two safety rules conflicted, the option that cannot destroy
user data even if it costs a feature.

The run was cut short by a session token limit before the rulings could be
applied to the documents. The rulings themselves survived and are recorded
here so the adjudication does not have to be paid for twice.

**Until these are applied, the architecture documents still contradict each
other. Do not build from them. Build from this file plus the documents.**

### Ruling count by document

| Document | Rulings touching it |
|---|---:|
| `SECURITY_PRIVACY_LICENSING.md` | 81 |
| `CONTENT_AND_UPDATE_MODEL.md` | 76 |
| `SYSTEM_ARCHITECTURE.md` | 71 |
| `XBOX_DEV_MODE_INSTALLER.md` | 53 |
| `ONLINE_AND_RESULTS_PROTOCOL.md` | 45 |
| `DOLPHIN_INTEGRATION.md` | 25 |
| `PROJECT_QUESTIONNAIRE.md` | 25 |
| `INFORMATION_ARCHITECTURE.md` | 3 |
| `CLAUDE_START_HERE.md` | 3 |
| `COMPONENT_MAP.md` | 2 |
| `COMPONENT_INVENTORY.md` | 2 |
| `WIREFRAMES.md` | 1 |

---

## The rulings

### 1. Whether the Xbox wizard may ever write GC\ (memory card and SRAM)

**id:** `xbox-gc-memory-card`

**Ruling.** GC\ is read-only to the wizard. The wizard never creates, writes, overwrites, renames, moves or deletes any FILE under GC\ on a target drive, in any mode, under any condition, with exactly one exception: mode 5 Restore Xbox Save, which is a separately chosen mode on its own screen that names the target file, first copies the drive's current GC\ to <data root>/backups/xbox/<ISO timestamp>/ and verifies that copy by SHA-256, and only then writes. Delete both other write paths: no blank card is ever generated or written, and no validity check ever authorizes an action. The wizard MAY read GC\MemoryCardA.USA.raw and report what it found ("16 MB card present, 2 NFL Street saves", or "a file is present at this path that does not look like a memory card"); the report is informational and never changes what is written. The author's MemoryCardA.USA.raw (cbc27adb86d70bcd96226d55f0e8b14f38a40aae904979ceadef2b62b48065ef) and SRAM.raw (60ee4eb2c90c75b18155d221e4d198742e9873d6b36a3e3e97a6e362d3621a11) get verdict RETIRE: removed from the payload, absent from every manifest, therefore impossible to write. The wizard may create the empty directories GC/EUR, GC/JAP and GC/USA/Card A only if they do not already exist, and never writes a file inside any of them.

**Files to edit:** `docs/architecture/XBOX_DEV_MODE_INSTALLER.md`, `docs/architecture/SECURITY_PRIVACY_LICENSING.md`, `docs/architecture/SYSTEM_ARCHITECTURE.md`

---

### 2. GC\ read-only versus REPLACE with three write paths

**id:** `gc-memory-card-readonly-vs-replace`

**Ruling.** Same rule as xbox-gc-memory-card: GC\ is read-only to the wizard in every mode except a separately confirmed Restore Xbox Save, which backs the current card up to the PC and verifies that backup before writing. The shipped author card and SRAM are RETIRE, not REPLACE. SECURITY section 27's off-vocabulary row "REUSE, never write" becomes two rows: `Step 2\GC\MemoryCardA.USA.raw`, `SRAM.raw` = RETIRE (never shipped), and "the drive's own GC\ directory" is not an inherited component and gets no verdict, only the preservation rule.

**Files to edit:** `docs/architecture/SECURITY_PRIVACY_LICENSING.md`, `docs/architecture/XBOX_DEV_MODE_INSTALLER.md`

---

### 3. Whether the wizard may write GC\MemoryCardA.USA.raw and SRAM.raw on the target drive

**id:** `gc-memory-card-read-only-vs-written`

**Ruling.** No, except in Restore Xbox Save, which is a separate mode with its own confirm screen, a PC-side backup of the current card taken and SHA-256 verified before the write. Every other path (New, Update, Repair, restructuring, stale-file cleanup) treats GC\ as read-only. No blank card is ever written. The author's card and SRAM are RETIRED from the payload.

**Files to edit:** `docs/architecture/SECURITY_PRIVACY_LICENSING.md`, `docs/architecture/XBOX_DEV_MODE_INSTALLER.md`

---

### 4. GC\ read-only in SECURITY versus REPLACE with three write paths in XBOX

**id:** `gc-directory-read-only-vs-replace`

**Ruling.** GC\ is read-only to the wizard. Restore Xbox Save is the one exception and it is user-initiated, separately confirmed, and preceded by a verified PC-side backup of the card it is about to overwrite. The shipped `Step 2\GC\` files are RETIRE. No blank card, no rename-on-failed-validity, no automatic write of any kind.

**Files to edit:** `docs/architecture/SECURITY_PRIVACY_LICENSING.md`, `docs/architecture/XBOX_DEV_MODE_INSTALLER.md`

---

### 5. Whether the Xbox wizard may ever write to GC\

**id:** `xbox-gc-write-vs-readonly`

**Ruling.** No, except Restore Xbox Save. Full rule as in xbox-gc-memory-card: read-only in every automatic path, RETIRE the shipped author card and SRAM, no blank card, validity check is reporting only, empty GC subdirectories may be created only when absent.

**Files to edit:** `docs/architecture/SECURITY_PRIVACY_LICENSING.md`, `docs/architecture/XBOX_DEV_MODE_INSTALLER.md`

---

### 6. GC\ declared exempt from restructuring, then moved first during restructuring

**id:** `gc-exempt-vs-gc-moves-first`

**Ruling.** Restructuring never MOVES GC\. It COPIES GC\ to the destination first, verifies the copy by SHA-256 against the source, refuses to proceed if any file already exists at the destination GC\ path, and never removes the source. XBOX section 10's "GC\ moves first, before anything else" becomes "GC\ is copied first, verified at its destination, and the source is never removed", and section 8's "GC\ is exempt from every one of these paths" stands unchanged.

**Files to edit:** `docs/architecture/XBOX_DEV_MODE_INSTALLER.md`

---

### 7. Unqualified REUSE of the Xbox Step 2 payload versus regenerate-the-state

**id:** `xbox-payload-literal-reuse`

**Ruling.** SYSTEM_ARCHITECTURE section 5.7's single row "`Step 2 - Transfer to Xbox` payload | Reuse as the payload" is deleted and replaced by six rows: layout and configuration shape = REUSE; `GC\MemoryCardA.USA.raw` and `SRAM.raw` = RETIRE; `Config\TimePlayed.ini` = REPLACE with a launcher-generated file carrying `GN7E69 = 0x0000000000000000`; `Config\Dolphin.ini` `[Analytics]` block = REPLACE; `Config\Logger.ini` = REPLACE with `WriteToConsole = False` and `WriteToWindow = False`; `GameSettings\GN7E69.ini` = PORT into generated configuration.

**Files to edit:** `docs/architecture/SYSTEM_ARCHITECTURE.md`, `docs/architecture/XBOX_DEV_MODE_INSTALLER.md`

---

### 8. Whether the Step 2 payload is reused as-is

**id:** `xbox-payload-reuse-vs-regenerate`

**Ruling.** Reuse the layout and the configuration shape; regenerate or retire every file that carries state. Six per-file rows as in xbox-payload-literal-reuse. SYSTEM_ARCHITECTURE is the document that must change, not XBOX.

**Files to edit:** `docs/architecture/SYSTEM_ARCHITECTURE.md`, `docs/architecture/XBOX_DEV_MODE_INSTALLER.md`

---

### 9. Repair on a hash mismatch in the offline image: unconditional fail-closed versus recognize custom music

**id:** `repair-vs-custom-music`

**Ruling.** `customizable` is a MANDATORY field of `nsr.component.v1` and is true for `offline-content` only. Repair never re-fetches and never deletes a file in a `customizable: true` component whose local SHA-256 equals the launcher's own recorded rebuild hash in `music/state.json`. SECURITY section 5's "a hash mismatch on a managed file means refuse and offer repair" is scoped to files in components where `customizable` is false or absent. SECURITY's `reborn.manifest.v1` block is retired in favor of `nsr.component.v1`, which already carries `customizable`, so no second schema has to gain the field.

**Files to edit:** `docs/architecture/SECURITY_PRIVACY_LICENSING.md`, `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`

---

### 10. What repair does when the offline image hash does not match the manifest

**id:** `repair-deletes-custom-music`

**Ruling.** Leave it alone if the component is `customizable: true` and the local hash matches the launcher's recorded rebuild hash and a `music/state.json` record exists. Otherwise treat it as damage and re-fetch the whole artifact. `customizable` is mandatory in the one manifest schema; the word must appear in SECURITY, which currently does not contain it.

**Files to edit:** `docs/architecture/SECURITY_PRIVACY_LICENSING.md`, `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`

---

### 11. ingest.php: leave alone, reuse the design, or replace the trust model

**id:** `ingest-php-verdict`

**Ruling.** One verdict: `ingest.php` on the league site = REUSE. Its deployment is untouched and out of launcher scope, and its proven design is harvested by the new ladder endpoint: canonical `si_capture_hash` over `{teams, style, team_totals, players}`, idempotency through a UNIQUE key rather than a SELECT, "review" counted as success by the uploader, prefix-tolerant team resolution that resolves an ambiguous 14-character prefix to nothing, `SI_TARGET_SCORE = 36` as a floor, and the 512 KB payload cap. The object SECURITY wants to REPLACE is not `ingest.php`; it is the trust model of the NEW ladder endpoint, which is a new component and therefore carries no reuse verdict. That new endpoint must reject offline captures on its own authority and must accept a rating-affecting submission only against a match id and nonce the server itself issued.

**Files to edit:** `docs/architecture/SYSTEM_ARCHITECTURE.md`, `docs/architecture/ONLINE_AND_RESULTS_PROTOCOL.md`, `docs/architecture/SECURITY_PRIVACY_LICENSING.md`

---

### 12. ingest.php carries three verdicts

**id:** `ingest-php-three-verdicts`

**Ruling.** REUSE, one verdict, deployment untouched, design harvested by the new ladder endpoint. The trust-model replacement is a property of the new endpoint, not a verdict on `ingest.php`. The launcher never posts to `ingest.php` in any mode.

**Files to edit:** `docs/architecture/SYSTEM_ARCHITECTURE.md`, `docs/architecture/ONLINE_AND_RESULTS_PROTOCOL.md`, `docs/architecture/SECURITY_PRIVACY_LICENSING.md`

---

### 13. ingest.php verdicts, plus a routing table that has the launcher posting to it against a Decided row

**id:** `ingest-php-three-verdicts-and-launcher-posting`

**Ruling.** `ingest.php` = REUSE, deployment untouched. ONLINE section 10.1's routing table loses its league row entirely and becomes three rows: ranked or casual online launch with a match context = ladder only; anything else, including any launch with no match context = local only, nothing leaves the machine; a capture on disk that no launch this session accounts for = quarantine and surface, never guess. There is no league surface, no league id and no league destination in the launcher.

**Files to edit:** `docs/architecture/ONLINE_AND_RESULTS_PROTOCOL.md`, `docs/architecture/SYSTEM_ARCHITECTURE.md`, `docs/architecture/SECURITY_PRIVACY_LICENSING.md`

---

### 14. Whether the launcher ever routes a capture to the league site's ingest.php

**id:** `capture-routing-to-league-ingest`

**Ruling.** Never, in any mode. The routing table has exactly three outcomes: ladder only, local only, or quarantine. Routing is decided at launch, written down alongside the match context before Dolphin starts, and never re-derived from file contents at upload time. A capture is never sent to more than one destination.

**Files to edit:** `docs/architecture/ONLINE_AND_RESULTS_PROTOCOL.md`

---

### 15. Whether the launcher ever posts a capture to the league ingest

**id:** `league-ingest-routing-in-launcher`

**Ruling.** No. Remove the "Creation League launch, launched from the league surface with a league id" row from ONLINE section 10.1. The launcher has no league surface and passes no league id. Creation League ingest continues to work exactly as it does today, by whatever path it uses today, outside launcher scope.

**Files to edit:** `docs/architecture/ONLINE_AND_RESULTS_PROTOCOL.md`, `docs/architecture/SYSTEM_ARCHITECTURE.md`

---

### 16. How the offline-capture gate is closed

**id:** `netplay-gate-defeat-class`

**Ruling.** Three controls, all required, in this order. 1) Compile time: `NetplayGated()` becomes `#ifdef NSR_ALLOW_OFFLINE_CAPTURE return true; #else return NetPlay::IsNetPlayRunning(); #endif`, with the macro never defined in a release build; the same treatment applies to `nsr_debug.txt`, `playhide_test.txt` and `playhide_debug.txt`. Verdict on the `NetplayGated()` file check = REPLACE. 2) Packaging: the shipped `User\` tree is an ALLOWLIST asserted at build time, and the build fails if any file exists there that is not on the list. A blocklist of named files is not the control. 3) Server: the ladder endpoint refuses any submission with no server-issued match context. SYSTEM_ARCHITECTURE section 2's claim that moving upload makes the gate "impossible to defeat by file placement" is false as written and must be rewritten to name the compile-time constant as the control.

**Files to edit:** `docs/architecture/DOLPHIN_INTEGRATION.md`, `docs/architecture/SYSTEM_ARCHITECTURE.md`, `docs/architecture/SECURITY_PRIVACY_LICENSING.md`, `docs/architecture/ONLINE_AND_RESULTS_PROTOCOL.md`

---

### 17. What actually closes the offline stat-capture gate

**id:** `netplay-gate-strip-two-files`

**Ruling.** A compile-time constant, plus a packaging allowlist for the shipped `User\` folder, plus server-side refusal. Stripping two named files closes neither of the two verified defeat paths for one marker, let alone the class. DOLPHIN section 6's "must never ship" table is re-titled "the shipped `User\` allowlist" and states the rule rather than listing files.

**Files to edit:** `docs/architecture/DOLPHIN_INTEGRATION.md`, `docs/architecture/SYSTEM_ARCHITECTURE.md`, `docs/architecture/ONLINE_AND_RESULTS_PROTOCOL.md`, `docs/architecture/SECURITY_PRIVACY_LICENSING.md`

---

### 18. The set of files that must never ship, and the claim that stripping User\ closes the gate

**id:** `netplay-gate-defeat-paths-and-marker-list`

**Ruling.** There is no canonical list of files; there is an allowlist of what MAY be in the shipped `User\` tree, asserted by packaging, and a compiled-in gate that makes any marker inert. All four documents converge on that one sentence. Where a list is still useful (diagnostics, migration off an existing install), it is the eleven markers ONLINE section 10.4 enumerates plus anything future work adds, and it is explicitly labeled non-exhaustive.

**Files to edit:** `docs/architecture/DOLPHIN_INTEGRATION.md`, `docs/architecture/SYSTEM_ARCHITECTURE.md`, `docs/architecture/SECURITY_PRIVACY_LICENSING.md`, `docs/architecture/ONLINE_AND_RESULTS_PROTOCOL.md`

---

### 19. The single verdict for NflsStatTracker

**id:** `statTracker-four-verdicts`

**Ruling.** WRAP, additive. It is a source change to `NflsStatTracker.cpp`: one new top-level `reborn_match` key, emitted alongside `schema`. The schema string stays exactly `nsr.game.v1`, no existing key is renamed, removed, reordered or redefined, and the `si_capture_hash` hashed set is untouched. SYSTEM_ARCHITECTURE lines 67 and 206, DOLPHIN_INTEGRATION line 245 and SECURITY section 27 all change from "reuse unchanged" to "WRAP, additive". Any plan built on "no Dolphin source change is needed for capture" is wrong.

**Files to edit:** `docs/architecture/SYSTEM_ARCHITECTURE.md`, `docs/architecture/DOLPHIN_INTEGRATION.md`, `docs/architecture/SECURITY_PRIVACY_LICENSING.md`, `docs/architecture/ONLINE_AND_RESULTS_PROTOCOL.md`

---

### 20. NflsStatTracker carries four verdicts across the set

**id:** `nflsstattracker-four-verdicts`

**Ruling.** WRAP, additive, in all four documents. It is a source change. The hashed set and the schema string are explicitly out of scope of that change, which is what preserves both NetPlay clients hashing one game identically.

**Files to edit:** `docs/architecture/SYSTEM_ARCHITECTURE.md`, `docs/architecture/DOLPHIN_INTEGRATION.md`, `docs/architecture/SECURITY_PRIVACY_LICENSING.md`, `docs/architecture/ONLINE_AND_RESULTS_PROTOCOL.md`

---

### 21. Whether identifiers may be written into the nsr.game.v1 capture file

**id:** `capture-identity-block`

**Ruling.** The `reborn_match` block ships, with a closed field allowlist and a permanent prohibition. Allowed: `match_id`, `local_seat`, `observed_pad_map`, `local_pad`, `local_registry_team`, `local_team_index`, `netplay_running`, `runtime_version`, `netplay_config_version`. Permanently forbidden in the capture file: account id, Discord id, display name, NetPlay nickname, IP address, OS username, machine identifier, and the match `nonce`. The nonce and every account identity live in the `nsr.submission.v1` envelope the launcher builds, never in the file. SECURITY section 12's sentence changes from "must not add identifiers" to "must not add PERSONAL identifiers; a per-match correlation id is permitted under the allowlist above". SECURITY section 15 gains one redaction row: `reborn_match.match_id` is KEPT in a diagnostic bundle, account and Discord ids remain "remove entirely".

**Files to edit:** `docs/architecture/SECURITY_PRIVACY_LICENSING.md`, `docs/architecture/ONLINE_AND_RESULTS_PROTOCOL.md`

---

### 22. Whether identifiers may be written into nsr.game.v1, and the NflsStatTracker verdict

**id:** `identity-block-in-capture-file`

**Ruling.** Yes for the nine allowlisted `reborn_match` fields, never for personal identifiers or the nonce. NflsStatTracker = WRAP, additive. Both rules are stated in SECURITY and in ONLINE with identical wording so neither can drift.

**Files to edit:** `docs/architecture/SECURITY_PRIVACY_LICENSING.md`, `docs/architecture/ONLINE_AND_RESULTS_PROTOCOL.md`, `docs/architecture/SYSTEM_ARCHITECTURE.md`, `docs/architecture/DOLPHIN_INTEGRATION.md`

---

### 23. Whether nsr.game.v1 gains a reborn_match identity block

**id:** `capture-identity-block-in-file`

**Ruling.** It does, with the nine-field allowlist, outside the hashed set, alongside `schema`. Personal identifiers and the nonce are forbidden. `local_team_index` is emitted only after the registry-team to `teams[]` measurement is done; until then the fork emits `local_registry_team` and omits `local_team_index` entirely rather than guessing.

**Files to edit:** `docs/architecture/SECURITY_PRIVACY_LICENSING.md`, `docs/architecture/ONLINE_AND_RESULTS_PROTOCOL.md`

---

### 24. Whether identifiers may be added to the nsr.game.v1 capture file

**id:** `capture-payload-identity-injection`

**Ruling.** Only the nine allowlisted `reborn_match` fields. SECURITY's protected property is restated as "no personal identifier" and keeps its full force for IP address, OS username, machine identifier and account identifier. The redaction spec gains an explicit KEEP row for `match_id` and keeps its REMOVE ENTIRELY row for account and Discord ids.

**Files to edit:** `docs/architecture/SECURITY_PRIVACY_LICENSING.md`, `docs/architecture/ONLINE_AND_RESULTS_PROTOCOL.md`

---

### 25. Where the nsr.game.v1 capture is written and queued

**id:** `capture-file-canonical-location`

**Ruling.** Dolphin writes to `<per-launch user dir>/LeagueGameStats/*.json`, unchanged. The launcher, which already passes `-u <per-launch user dir>` and therefore already owns that directory, moves the file into `stats/queue/` after the process exits, then records it in `stats/local.db`. SECURITY section 12's Stored column is rewritten to describe both hops. No second source change to `NflsStatTracker` is authorized.

**Files to edit:** `docs/architecture/SECURITY_PRIVACY_LICENSING.md`, `docs/architecture/SYSTEM_ARCHITECTURE.md`, `docs/architecture/ONLINE_AND_RESULTS_PROTOCOL.md`

---

### 26. Which process writes the uploaded.txt content-hash ledger once NflsStatUpload is removed

**id:** `uploaded-txt-owner`

**Ruling.** The launcher and supervisor. Dolphin writes the capture JSON and `nsr_session.jsonl` and nothing else. DOLPHIN_INTEGRATION section 10's Dolphin column drops "writes uploaded.txt". The ledger becomes a table in `stats/local.db` keyed on the canonical capture content hash (the same `si_capture_hash` canonicalization), and that hash is the idempotency key on the new ladder endpoint. Verdict on the `uploaded.txt` ledger stays PORT.

**Files to edit:** `docs/architecture/DOLPHIN_INTEGRATION.md`, `docs/architecture/SYSTEM_ARCHITECTURE.md`, `docs/architecture/ONLINE_AND_RESULTS_PROTOCOL.md`

---

### 27. build_iso_fields.py and build_iso_with_game12.py: Wrap versus PORT

**id:** `build-iso-wrap-vs-port`

**Ruling.** PORT, with CONTENT's definition made normative: the FST and `Game.toc` writing logic and the two gate rules move into a new launcher-owned repacker; neither script file ships. Constraint from Decided row 12.12: the repacker is Python inside the bundled runtime for v1, not a Rust rewrite, because that row records the native rewrite as "the follow-up, not the launch path". `music_editor.py` stays WRAP and its `run([PY, 'build_iso_with_game12.py'])` call at line 113 is replaced with a call into the repacker. Two rules carry forward by restatement: "an unchanged ISO is a FAILED BUILD, never a silent fallback: divergence WARNS and ABORTS", and collect every gate failure and report them together before writing anything. SYSTEM_ARCHITECTURE section 5.1's "Wrap" is wrong and changes; CONTENT's miscitation of section 5.5 is corrected, since 5.5 names only `music_editor.py`, `music_pack.py`/`.nsmp`, `extend_slot_40big.py`, `fix_pfdx_toc.py` and `edit_music_meta.py`.

**Files to edit:** `docs/architecture/SYSTEM_ARCHITECTURE.md`, `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`

---

### 28. Whether the two ISO builders are wrapped and run on a player machine, or ported

**id:** `iso-builders-wrap-vs-port`

**Ruling.** PORT. Neither script ships. The repacker is new launcher-owned Python inside the bundled runtime, carrying the two gate rules forward. `music_editor.py` remains WRAP. Decided row 12.12 is honored: no Rust rewrite on the launch path.

**Files to edit:** `docs/architecture/SYSTEM_ARCHITECTURE.md`, `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`

---

### 29. The verdict for the tools that produce the customized offline image

**id:** `build-iso-tools-wrap-vs-port`

**Ruling.** PORT for both scripts; WRAP for `music_editor.py`'s engine; the repacker is bundled Python, not Rust, for v1.

**Files to edit:** `docs/architecture/SYSTEM_ARCHITECTURE.md`, `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`

---

### 30. build_iso_fields.py and build_iso_with_game12.py: Wrap versus PORT

**id:** `build-iso-scripts-wrap-vs-port`

**Ruling.** PORT. SYSTEM_ARCHITECTURE section 5.1 changes; CONTENT is right on the verdict and must fix its citation of section 5.5, which does not name either script.

**Files to edit:** `docs/architecture/SYSTEM_ARCHITECTURE.md`, `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`

---

### 31. Mod package count

**id:** `mod-count-13-vs-11`

**Ruling.** Eleven migratable packages, 57 files, 20,534,456 bytes, inside 13 directories, of which `!BACKUP` and `!INSTLOGS` are JSGME control folders holding zero files each, plus `JSGME.ini` at 0 bytes. Every occurrence of "13 packages" becomes "11 packages in 13 directories". SYSTEM_ARCHITECTURE section 5.4 is the only one of the six still wrong.

**Files to edit:** `docs/architecture/SYSTEM_ARCHITECTURE.md`, `docs/discovery/PROJECT_QUESTIONNAIRE.md`

---

### 32. How many JSGME mod packages migrate

**id:** `mod-package-count-13-vs-11`

**Ruling.** Eleven, in 13 directories, 57 files, 20,534,456 bytes. Questionnaire row 11.6 is re-marked from "13 JSGME entries" to "11 packages plus 2 empty control folders in 13 directories".

**Files to edit:** `docs/architecture/SYSTEM_ARCHITECTURE.md`, `docs/discovery/PROJECT_QUESTIONNAIRE.md`

---

### 33. Mod package count, 13 versus 11

**id:** `mod-package-count`

**Ruling.** Eleven packages, 13 directories, 57 files. One sentence, used identically everywhere the number appears.

**Files to edit:** `docs/architecture/SYSTEM_ARCHITECTURE.md`, `docs/discovery/PROJECT_QUESTIONNAIRE.md`

---

### 34. !BACKUP retired outright versus required as a migration input

**id:** `backup-retire-vs-migration-dependency`

**Ruling.** `JSGME.exe`, `!BACKUP` and `!INSTLOGS` = RETIRE, and that verdict is about the PRODUCT, not about the user's disk. No verdict in any of the six documents authorizes deleting anything from a machine. Migration reads `!BACKUP` on the user's existing 2.1 install as step 1 (un-apply through JSGME's own restore data), and only after a successful un-apply is that data no longer needed. If un-apply cannot complete, do not partially restore: re-fetch the pristine `textures` component, which is per-file and therefore only re-fetches what actually differs.

**Files to edit:** `docs/architecture/SECURITY_PRIVACY_LICENSING.md`, `docs/architecture/SYSTEM_ARCHITECTURE.md`, `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`

---

### 35. Two incompatible mod activation models

**id:** `mod-activation-model`

**Ruling.** Composition only. `library/textures/<gameid>/` is pristine, manifest-verified and READ ONLY; nothing ever writes into it. Four directories: `mods/staging/<id>/<version>/` (extract and hash-verify a downloaded package before it is trusted), `mods/packages/<id>/<version>/` (validated payloads), `mods/active/<gameid>/` (the composed tree, hardlinks over pristine, the only tree Dolphin loads), and `mods/activation.json` (ordered active ids and versions). Deactivation deletes the composed tree and rebuilds it from `activation.json`. SECURITY section 7's "Allowed install root: `library/textures/<game-id>/`" becomes `mods/staging/<id>/<version>/`, and its "restores precisely what was displaced" reverse map is deleted, because nothing is ever displaced.

**Files to edit:** `docs/architecture/SECURITY_PRIVACY_LICENSING.md`, `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`, `docs/architecture/SYSTEM_ARCHITECTURE.md`

---

### 36. Whether the pristine tree is ever written to, and what deactivate restores

**id:** `mod-activation-compose-vs-displace`

**Ruling.** Never written to. Deactivate deletes `mods/active/<gameid>/` and rebuilds it from `activation.json`. There is no reverse map and no backup folder, because nothing is displaced. Composition requires the pristine and composed trees on the same volume (a hardlink cannot cross filesystems); where hardlinks are unavailable, notably exFAT and FAT32, the fallback is a full copy and that cost must be budgeted as the common case on a handheld.

**Files to edit:** `docs/architecture/SECURITY_PRIVACY_LICENSING.md`, `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`

---

### 37. Hardlinked composed tree versus staging, swap and reverse map

**id:** `mod-activation-two-models`

**Ruling.** Composed tree. SECURITY section 7's install mechanics paragraph is rewritten to describe extract-verify-promote into `mods/packages/`, then recompose `mods/active/<gameid>/`. "JSGME's `!BACKUP` is the right idea implemented badly" is replaced by "JSGME's `!BACKUP` exists only because JSGME writes into the base tree; composition removes the need for it entirely."

**Files to edit:** `docs/architecture/SECURITY_PRIVACY_LICENSING.md`, `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`, `docs/architecture/SYSTEM_ARCHITECTURE.md`

---

### 38. SECURITY's package refusal rule rejects the container layout CONTENT specifies

**id:** `mod-container-entries-rejected`

**Ruling.** Two zones. CONTAINER MEMBERS are a closed allowlist of exact paths: `package.json`, `art/cover.png`, `LICENSE.txt`, `CHANGELOG.md`. They are not listed in `files[]` and are validated by name and type. PAYLOAD ENTRIES are every member under `files/`; each must appear in `manifest.files[]` with a matching SHA-256, must sit under `files/<gameid>/` for a `gameid` in `targets`, and must end in `.png` or `.dds`. Any archive member that is neither a container member on the closed list nor a payload entry under `files/` rejects the whole package. All of SECURITY section 7's traversal refusals apply to every member of both zones. `manifest.files[].path` stays relative to `files/`.

**Files to edit:** `docs/architecture/SECURITY_PRIVACY_LICENSING.md`, `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`

---

### 39. The .nsmod container's own members are rejected by the entry allowlist

**id:** `mod-container-vs-entry-allowlist`

**Ruling.** Same two-zone rule. The extension allowlist `.png`, `.dds` applies to payload entries only; container members are allowlisted by exact path and validated by type, with `art/cover.png` required to be a PNG under a size cap.

**Files to edit:** `docs/architecture/SECURITY_PRIVACY_LICENSING.md`, `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`

---

### 40. The .nsmod manifest schema, required fields and payload path shape

**id:** `mod-package-manifest-format`

**Ruling.** `nsr.mod.v1` as written in CONTENT section 13 is the format, verbatim, including `schema`, `magic: "NSMD"`, `targets[]` (an array, not a single target game id), `conflict_group`, `online_safe` and `files[{path, sha256, bytes}]`. SECURITY section 7's field list is replaced by it. The extraction target is `mods/staging/<id>/<version>/`, with the version segment, matching `mods/packages/<id>/<version>/`. The three caps SECURITY requires (entry count, uncompressed total checked against the declared total before extraction begins, and compression ratio) become required fields of the import policy and the format cannot ship with any of them unset; refuse whole on breach. ESCALATE TO ADAM for the three values: nothing measured supports any particular number, and the only measured inputs are that the largest existing package is 19 files and the largest is 3,670,932 bytes.

**Files to edit:** `docs/architecture/SECURITY_PRIVACY_LICENSING.md`, `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`

---

### 41. Whether mod packages are individually signed

**id:** `mod-package-signing`

**Ruling.** They are not. Packages are hash-pinned by `catalog/packages.json`, which is itself SHA-256-pinned inside the signed release manifest. SECURITY section 7's "First-party packages are signed with the section 5 key" is retired. A sideloaded package has no catalog entry, therefore no pin, and the UI labels it "not verified by Reborn".

**Files to edit:** `docs/architecture/SECURITY_PRIVACY_LICENSING.md`, `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`

---

### 42. Two incompatible signed-manifest designs and two schema names

**id:** `signed-manifest-two-designs`

**Ruling.** One design: CONTENT's. `nsr.release.v1` is the only signed object, with a detached Ed25519 signature over the exact bytes served; it pins each `nsr.component.v1` by SHA-256; component manifests are never individually signed. `reborn.manifest.v1` is retired as a schema name and SECURITY section 5's block is rewritten as an `nsr.component.v1` example. SECURITY's security fields are added rather than dropped: `sequence` becomes mandatory at both levels, `component` and `install_root` stay mandatory in `nsr.component.v1` and the client refuses any component manifest whose `component` or `install_root` differs from what it asked for. `min_launcher` is retired in favor of the existing `requires.launcher` range, which is the same control.

**Files to edit:** `docs/architecture/SECURITY_PRIVACY_LICENSING.md`, `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`

---

### 43. Two incompatible signed-manifest designs, only one carrying anti-rollback

**id:** `two-manifest-signing-designs`

**Ruling.** CONTENT's two-level design wins on structure; SECURITY's `sequence` is added to it as mandatory at both levels. One signature per release, over the release manifest, detached, Ed25519, over the exact bytes served. `reborn.manifest.v1` is retired.

**Files to edit:** `docs/architecture/SECURITY_PRIVACY_LICENSING.md`, `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`

---

### 44. Manifest schema, levels, signing granularity, and a third shape in XBOX

**id:** `two-incompatible-signed-manifest-designs`

**Ruling.** Two levels, one signature per release, schemas `nsr.release.v1` and `nsr.component.v1`. `reborn.manifest.v1` is retired. XBOX section 6's `{profile, confirmed_on_hardware, game_dir, user_dir, entries, empty_dirs}` object is NOT a manifest and must stop being called one: it is a destination map for a foreign filesystem. It is renamed `nsr.xboxprofile.v1`, gains `schema` and `profile_version`, and its per-entry `size` becomes `bytes`. It consumes the `xbox-payload` component manifest; it does not replace it.

**Files to edit:** `docs/architecture/SECURITY_PRIVACY_LICENSING.md`, `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`, `docs/architecture/XBOX_DEV_MODE_INSTALLER.md`

---

### 45. Two schema names and two signing granularities for the same artifact

**id:** `two-manifest-schemas`

**Ruling.** `nsr.release.v1` plus `nsr.component.v1`; one signature per release; `reborn.manifest.v1` retired. Every document uses those two names and no others for content manifests.

**Files to edit:** `docs/architecture/SECURITY_PRIVACY_LICENSING.md`, `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`

---

### 46. Schema, signing granularity and anti-rollback field

**id:** `two-signed-manifest-designs`

**Ruling.** CONTENT's two-level design with SECURITY's `sequence`, `component` and `install_root` retained as mandatory. One signature per release. `min_launcher` retired in favor of `requires.launcher`.

**Files to edit:** `docs/architecture/SECURITY_PRIVACY_LICENSING.md`, `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`

---

### 47. Field names and required members of a per-file manifest row

**id:** `manifest-per-file-record-field-names`

**Ruling.** One row shape: `{ "path", "sha256", "bytes", "enc_bytes" }`, with `enc_bytes` present only for per-file-delivered components. The field is `bytes`, never `size`. SECURITY section 5 and XBOX section 6 both change `size` to `bytes`. CONTENT's rule is normative: `bytes` is the byte length of the file, never the size it occupies on disk, and the packaging step must re-stat every listed file and fail the build on any row where `bytes` does not equal the length on disk.

**Files to edit:** `docs/architecture/SECURITY_PRIVACY_LICENSING.md`, `docs/architecture/XBOX_DEV_MODE_INSTALLER.md`, `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`

---

### 48. The per-file byte-length field is bytes in one design and size in two others

**id:** `manifest-size-field-name`

**Ruling.** `bytes`, everywhere, defined as the byte length of the file and never the allocated size, with a build-time re-stat assertion. SECURITY and XBOX change.

**Files to edit:** `docs/architecture/SECURITY_PRIVACY_LICENSING.md`, `docs/architecture/XBOX_DEV_MODE_INSTALLER.md`

---

### 49. files[].path relative to install_root or to the data root

**id:** `manifest-path-semantics`

**Ruling.** `files[].path` is ALWAYS relative to the manifest's own `install_root`, never data-root relative, never absolute. SECURITY section 5's example changes from `"path": "library/offline/NSR.iso"` with `"install_root": "library/offline"` to `"path": "NSR.iso"`. The client refuses any `path` that contains `..`, a leading separator, a drive letter or a UNC prefix, or that normalizes outside `install_root`.

**Files to edit:** `docs/architecture/SECURITY_PRIVACY_LICENSING.md`

---

### 50. A monotonic sequence floor is mandatory in one design and absent from the other's schema

**id:** `anti-rollback-sequence-absent`

**Ruling.** `sequence`, a monotonically increasing integer independent of the human-readable version, is a MANDATORY field of `nsr.release.v1` and of every `nsr.component.v1`, and it is added to CONTENT section 5's "what the manifest must contain" requirements table. The client persists the highest `sequence` it has ever successfully applied, per component and for the release, and refuses any manifest with a lower one as if the signature had failed. Equal means already applied: verify local hashes, do not reinstall. The floor never decreases automatically, not by an update, not by a manifest field, not by a server instruction.

**Files to edit:** `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`, `docs/architecture/SECURITY_PRIVACY_LICENSING.md`

---

### 51. The anti-rollback field exists only in the security manifest

**id:** `anti-rollback-sequence-missing`

**Ruling.** Add `sequence` to `nsr.release.v1` and `nsr.component.v1` as mandatory, and to CONTENT's requirements table. `release` and `previous_release` stay: they name where a rollback goes, which is a different job from refusing a replay.

**Files to edit:** `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`, `docs/architecture/SECURITY_PRIVACY_LICENSING.md`

---

### 52. The monotonic anti-rollback field required by the update trust chain

**id:** `manifest-missing-antirollback-sequence`

**Ruling.** Mandatory `sequence` at both manifest levels, persisted per component, refuse-below-floor with fail-closed handling. `versions/<component>/current.json` gains a `sequence` field alongside `version` and `previous`, so the floor survives a restart.

**Files to edit:** `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`, `docs/architecture/SECURITY_PRIVACY_LICENSING.md`

---

### 53. What the client does when a manifest signature fails

**id:** `signature-failure-behavior`

**Ruling.** Scope the two behaviors to two different objects, because both are right about their own object. A manifest FETCHED from the network that is missing, unsigned, fails signature verification, or carries a `sequence` below the floor: refuse the update, keep running on the installed and already verified content, block nothing, and surface it in exactly one non-blocking place, the downloads indicator reading "Update check failed", with the detail in diagnostics. A hash mismatch on an installed, manifest-listed, non-customizable file, or a missing local manifest for content already installed: fail closed, refuse to use that component, offer repair. There is never a fallback to unsigned in either case. CONTENT's "shows nothing blocking" survives; its "nothing user-facing, diagnostics only" does not.

**Files to edit:** `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`, `docs/architecture/SECURITY_PRIVACY_LICENSING.md`

---

### 54. What the user sees when a manifest is missing or its signature fails

**id:** `manifest-failure-user-facing`

**Ruling.** Nothing blocking, and one non-blocking indication: the downloads indicator shows "Update check failed" with the detail in diagnostics. CONTENT section 17's event row changes from "Nothing user-facing. Diagnostics only" to "Downloads indicator, non-blocking. Diagnostics carry the detail." Installed-content hash failures remain fail closed with a repair offer.

**Files to edit:** `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`, `docs/architecture/SECURITY_PRIVACY_LICENSING.md`

---

### 55. Client behavior on a missing, unsigned, or signature-failing manifest

**id:** `manifest-verification-failure-behavior`

**Ruling.** Refuse the update and keep the installed content, non-blocking, indicator plus diagnostics. Fail closed on installed content that fails its hash or has no local manifest. No fallback to unsigned, ever.

**Files to edit:** `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`, `docs/architecture/SECURITY_PRIVACY_LICENSING.md`

---

### 56. Outage versus refuse-and-offer-repair on manifest failure

**id:** `manifest-failure-behavior`

**Ruling.** Outage semantics for a fetch (non-blocking, indicator plus diagnostics, keep playing); fail-closed semantics for installed content (refuse the component, offer repair). Both sentences appear in both documents.

**Files to edit:** `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`, `docs/architecture/SECURITY_PRIVACY_LICENSING.md`

---

### 57. Automatic rollback after a failed health check versus a floor that may never decrease

**id:** `auto-rollback-vs-monotonic-floor`

**Ruling.** Two different objects, no conflict once separated. The `sequence` floor governs which manifests the client will ACCEPT from the network and never decreases automatically. A failed health check governs which locally verified payload is ACTIVE: the launcher restores the retained prior payload automatically and without asking, marks the failed version quarantined locally so it is not re-applied, and leaves the floor at the highest accepted `sequence`. A deliberate downgrade below the floor stays a user action on a screen that names the version and says it is going backwards. Content components whose rollback is a re-download can never roll back automatically: they mark invalid and the Play button becomes `[ Repair and Play ]`.

**Files to edit:** `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`, `docs/architecture/SECURITY_PRIVACY_LICENSING.md`

---

### 58. Whether one prior version of every component, including the two disc images, is retained

**id:** `prior-version-retention-for-images`

**Ruling.** Do not retain prior copies of the two disc images. Rollback for `offline-content` and `online-content` is a re-download of the prior release's artifact, and the host must keep the previous release's blobs available. SYSTEM_ARCHITECTURE section 5.4's "one prior version retained for rollback" is scoped to `launcher`, `runtime`, `textures`, `mods`, `controller-db` and `xbox-payload`. The UI states the size before the download starts, per the four-part error rule.

**Files to edit:** `docs/architecture/SYSTEM_ARCHITECTURE.md`, `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`, `docs/discovery/COMPONENT_MAP.md`

---

### 59. How many manifests the launcher retains and ships

**id:** `manifest-history-retention`

**Ruling.** Two different things that the documents conflate, and both are required. MANIFEST HISTORY: the launcher bundle ships every release manifest and component manifest it has ever published, as a compact history index under `manifests/history/` inside the `launcher` component, carrying only `{release, sequence, component, version, files[{path, sha256, bytes}]}`. This is what makes the Stale class possible on the Xbox drive and on the PC. PRIOR PAYLOAD: `versions/<component>/prior/` retains exactly one prior installed payload, per the ruling on disc images. CONTENT gains the history concept; XBOX's requirement is satisfied.

**Files to edit:** `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`, `docs/architecture/XBOX_DEV_MODE_INSTALLER.md`, `docs/architecture/SYSTEM_ARCHITECTURE.md`

---

### 60. Whether previous release manifests are retained and shipped

**id:** `previous-manifests-must-ship`

**Ruling.** Yes. Full manifest history ships inside the launcher bundle as a hash-and-path-only index; exactly one prior installed PAYLOAD is retained per component. The two are named differently everywhere so they cannot be confused again.

**Files to edit:** `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`, `docs/architecture/XBOX_DEV_MODE_INSTALLER.md`

---

### 61. How a file present on disk but absent from the manifest is classified

**id:** `extra-file-classification-model`

**Ruling.** One four-class model on both surfaces, PC and Xbox: Current (in the manifest, hash matches) skip; Mismatched (in the manifest, hash differs) rewrite; Stale (not in the current manifest but named by a manifest the launcher shipped) offer removal on a screen that lists it with a count and byte total, DEFAULT KEEP; Foreign (in no manifest the launcher knows) leave alone, always, and report as "not ours, left in place". Extra files inside a mod-composed tree are Expected and are left alone. CONTENT's two-class model is replaced by this one; XBOX's four classes stand except for the default.

**Files to edit:** `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`, `docs/architecture/XBOX_DEV_MODE_INSTALLER.md`

---

### 62. Report only versus remove by default for unmanifested files

**id:** `unmanifested-file-policy-pc-vs-xbox`

**Ruling.** Report, list, and require an explicit confirmation, with the DEFAULT set to keep, on both surfaces. XBOX section 9.5's "Default is remove" changes to "Default is keep". Foreign files are never offered for removal at all.

**Files to edit:** `docs/architecture/XBOX_DEV_MODE_INSTALLER.md`, `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`

---

### 63. Preservation contract says default non-destructive, stale-file policy says default remove

**id:** `stale-file-default-remove-vs-non-destructive`

**Ruling.** Default keep. The removal screen names the files, shows the count and byte total, and focuses "Keep them". XBOX section 9.5's default changes; section 8's contract is unchanged and wins.

**Files to edit:** `docs/architecture/XBOX_DEV_MODE_INSTALLER.md`

---

### 64. The never-touch-what-the-wizard-did-not-place contract and its two exceptions

**id:** `xbox-preserve-contract-absolutism`

**Ruling.** XBOX's confirmed-exception form is correct in shape and is adopted, with three amendments. Silent deletion never happens. Anything the wizard did not write is only ever touched from a screen that names it, shows a count and a byte total, and DEFAULTS TO KEEP. `GC\` is exempt from every one of these paths and is never inside a restructuring or stale-file set, and restructuring copies it rather than moving it. SECURITY section 8's absolute PRESERVE line "anything the wizard did not place" is rewritten to the same three sentences so the two documents state one contract.

**Files to edit:** `docs/architecture/SECURITY_PRIVACY_LICENSING.md`, `docs/architecture/XBOX_DEV_MODE_INSTALLER.md`

---

### 65. Existing components that receive no verdict in any of the six documents

**id:** `components-with-no-verdict`

**Ruling.** Five components get verdicts, and the consolidated table lives in SYSTEM_ARCHITECTURE section 5. `NflsUiSplit` = REUSE (ships, verified present in `Source/Core/VideoCommon/CMakeLists.txt` lines 96 to 97, Windows internals behind existing `_WIN32` guards, explicitly not a release gate). Shipped runtime `NFL Street Reborn Dolphin.exe` (`8705a96b...`, 14,537,728 bytes, Qt5) = REPLACE by the selected runtime `617321bf...`, and the note records that this is a runtime migration and not a file swap. Electron `server-app` = RETIRE (nothing in the shipped product spawns or supervises a local server; only its generate-per-session `ADMIN_SECRET` pattern is PORTed). `Stat Tracking\Start.dol.statread1` = REUSE (it is the production hook and must be baked into the online image build). The `Stat Tracking` Python chain (`gcmem.py`, `tracker.py`, `parse_gamestats.py`, `decode_sav.py`, `lzo1x.py`) = RETIRE from the product; `NflsStatTracker` is the production reader. The "Handheld package" = RETIRE as a delivery target for v1, with its path named: verified this pass at `C:\Users\Adam\Downloads\NFL Street Reborn Handheld 2.1`, 2,818 files, 3,015,661,205 bytes, same two-step shape as the Xbox package including a `Step 2 - Transfer to Device\GC\` that ships the identical author card `cbc27adb...65ef` and SRAM `60ee4eb2...1a11`. Because the destination map is data, a Handheld profile can be added later as a second `nsr.xboxprofile.v1` without a code change.

**Files to edit:** `docs/architecture/SYSTEM_ARCHITECTURE.md`, `docs/architecture/DOLPHIN_INTEGRATION.md`, `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`, `docs/architecture/ONLINE_AND_RESULTS_PROTOCOL.md`, `docs/architecture/SECURITY_PRIVACY_LICENSING.md`, `docs/architecture/XBOX_DEV_MODE_INSTALLER.md`

---

### 66. Preservation declared complete while roughly 90 KB of Reborn online C++ is untracked in a second tree

**id:** `second-dolphin-tree-preservation`

**Ruling.** Phase 0 is NOT done. DOLPHIN_INTEGRATION section 7 changes "Phase 0, done" to "Phase 0, half done" and gains a step 0: preserve the untracked Reborn C++ in `C:\Users\Adam\nflstreet2_clean\dolphin` (`NflsSymmetrize.cpp`, `NflsLink.cpp`, `NflsCaves.h`, `NflsGuard.cpp`, `NflsDesync.cpp`, `NflsSoak.cpp`, the adapted `NflsPlayHide.cpp` variant, `VideoCommon/NetPlayPlayCallUI.cpp/.h`, `Data/Sys/GameSettings/FR4N31.ini`) to a second private branch before anything else touches that machine. Section 2's scope sentence names both trees. Product verdicts: `NflsLink`, `NflsGuard`, `NflsSymmetrize`, `NetPlayPlayCallUI` and the adapted `NflsPlayHide` variant = RETIRE from the product; `NflsDesync` = PORT (the idea, not the implementation, which depends on a transport that is not shipping). RETIRE from the product never authorizes deleting anything from disk.

**Files to edit:** `docs/architecture/DOLPHIN_INTEGRATION.md`, `docs/architecture/ONLINE_AND_RESULTS_PROTOCOL.md`, `docs/architecture/SYSTEM_ARCHITECTURE.md`

---

### 67. Preservation complete for all Reborn C++ versus a second untracked tree

**id:** `preservation-scope`

**Ruling.** Preservation covers `dolphin-exgl` only. Say so in DOLPHIN_INTEGRATION section 2 and add the second tree with its file list, its remote (`https://github.com/michaelpenixfan2390/dolphin.git`), and step 0 of phase 0. Preserve before anything else.

**Files to edit:** `docs/architecture/DOLPHIN_INTEGRATION.md`, `docs/architecture/ONLINE_AND_RESULTS_PROTOCOL.md`

---

### 68. Whether the fork preservation work is complete

**id:** `preservation-phase0-done-vs-half`

**Ruling.** Not complete. Phase 0 is re-labeled "half done" and gains preserve-the-second-tree as its first step, which is also step 0 of ONLINE's ship checklist. Both documents carry the same step so neither can claim done alone.

**Files to edit:** `docs/architecture/DOLPHIN_INTEGRATION.md`, `docs/architecture/ONLINE_AND_RESULTS_PROTOCOL.md`

---

### 69. Whether Fair Input Delay NetPlay is Reborn work

**id:** `fair-input-delay-reborn-feature`

**Ruling.** It is stock upstream Dolphin and is not a Reborn feature. It gets NO verdict, because there is no Reborn component. Remove it from DOLPHIN_INTEGRATION section 9's preserved-behaviors table and from questionnaire row 4.2's cannot-regress list. Rewrite section 7 phase 3's rebase-deferral reason to name `NflsUiSplit`'s hooks into `RenderBase.cpp` and `VertexManagerBase.cpp` and the ExGL renderer changes, and to drop "custom fixed-delay NetPlay path" entirely. The genuinely custom lockstep transport is `NflsLink` in the second tree and it is not shipping.

**Files to edit:** `docs/architecture/DOLPHIN_INTEGRATION.md`, `docs/discovery/PROJECT_QUESTIONNAIRE.md`

---

### 70. Fixed-delay NetPlay credited to the fork and used as the reason to defer the rebase

**id:** `fixed-delay-netplay-attribution`

**Ruling.** Not Reborn work, no verdict, removed from the preserved-behaviors table. The rebase deferral stands but its stated reason changes to `NflsUiSplit`'s renderer hooks and the ExGL renderer changes.

**Files to edit:** `docs/architecture/DOLPHIN_INTEGRATION.md`

---

### 71. Whether Fair Input Delay is a Reborn feature that cannot regress

**id:** `fair-input-delay-decided-nonfeature`

**Ruling.** It is not. Remove it from questionnaire row 4.2 and from DOLPHIN_INTEGRATION section 9. Row 4.2 is re-marked as a reversal of a Decided row, with the evidence line, per the Decided-row reversal mechanism.

**Files to edit:** `docs/architecture/DOLPHIN_INTEGRATION.md`, `docs/discovery/PROJECT_QUESTIONNAIRE.md`

---

### 72. Which PC texture tree is the textures component, and whether the 19-PNG gap is real

**id:** `textures-component-19-png`

**Ruling.** CONTENT is right on identity: the `textures` component is the pristine DDS tree, 2,763 files, 2,077,362,716 bytes, containing NO PNG. The 19 PNG files in the selected runtime tree are an applied mod, verified byte-identical to `MODS\Xbox Controls` (19 files, 1,849,569 bytes), and under composition they live in `mods/packages/reborn.prompts.xbox/` and are composed into `mods/active/GN7E69/`. XBOX section 4's "0 Xbox-only, 0 PC-only" is arithmetically correct and semantically misleading and must be restated: it compares the Xbox set against a runtime tree with a mod applied. Against the pristine component, the Xbox PNG set carries 19 base names the component does not, and those 19 are exactly `reborn.prompts.xbox`. So both trees are pristine-plus-prompt-pack, in two encodings, and neither document was wrong about its own measurement.

**Files to edit:** `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`, `docs/architecture/XBOX_DEV_MODE_INSTALLER.md`, `docs/architecture/SYSTEM_ARCHITECTURE.md`

---

### 73. Whether the 19 PNGs are part of the managed component or an applied mod

**id:** `which-pc-texture-tree-is-pristine`

**Ruling.** An applied mod. The component is the 2,763-file DDS tree at 2,077,362,716 bytes. Every measurement in every document must name the folder it used; "the PC folder" is ambiguous because two runtimes exist on this machine and they differ.

**Files to edit:** `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`, `docs/architecture/XBOX_DEV_MODE_INSTALLER.md`

---

### 74. Which on-disk tree is the shipped textures component

**id:** `textures-component-which-tree`

**Ruling.** The 2,763-file DDS tree at 2,077,362,716 bytes. The Xbox tree is a SEPARATE component, `xbox-payload`, and XBOX's combined-size sentence is recomputed against the pristine figures rather than against a runtime tree with a mod applied.

**Files to edit:** `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`, `docs/architecture/XBOX_DEV_MODE_INSTALLER.md`

---

### 75. 2,763 versus 2,782 for the textures component

**id:** `textures-component-file-count`

**Ruling.** 2,763 DDS files, 2,077,362,716 bytes, no PNG. XBOX's 2,782-file / 2,079,212,285-byte figure describes the runtime tree with `reborn.prompts.xbox` applied and must be labeled as such wherever it appears, including the 3,607,137,741-byte combined figure, which is recomputed against pristine trees. XBOX open item 11 stays CLOSED, with its conclusion restated as "the 19 are a prompt pack present on both sides, not a platform difference".

**Files to edit:** `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`, `docs/architecture/XBOX_DEV_MODE_INSTALLER.md`

---

### 76. The Xbox wizard depends on manifest machinery the content model does not provide

**id:** `xbox-payload-not-a-component`

**Ruling.** Add `xbox-payload` as a manifest-backed component with `install_root: library/xbox`, `platforms: ["any"]`, `delivery: per-file`, and `optional: true` so it is fetched only when the user opens the Xbox wizard rather than bundled into every install. Its content is the Xbox-encoding pristine PNG texture tree plus the generated `Config\`, `GameSettings\`, cover and `NSR.png` files. The 19 prompt PNGs come from `reborn.prompts.xbox` and are composed by the wizard exactly as they are composed on the PC. Manifest history is provided by the ruling on manifest-history-retention, which satisfies XBOX's Stale class. CONTENT section 16's "there is no update mechanism for the Xbox copy" changes to "the drive is updated by re-running the wizard against the current `xbox-payload` manifest".

**Files to edit:** `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`, `docs/architecture/XBOX_DEV_MODE_INSTALLER.md`, `docs/architecture/SYSTEM_ARCHITECTURE.md`

---

### 77. Whether the Xbox PNG tree is expressible in the component and manifest model

**id:** `manifest-platform-vocabulary-omits-xbox`

**Ruling.** It is, once `xbox-payload` exists as a component with `platforms: ["any"]`. The platform vocabulary stays `["win-x64", "linux-x64", "mac-arm64", "mac-x64"]` or `["any"]` and does not gain an Xbox token, because `platforms` describes which host OS downloads a component, and the Xbox payload is data a host copies, not code a host runs.

**Files to edit:** `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`, `docs/architecture/XBOX_DEV_MODE_INSTALLER.md`

---

### 78. How many independently updateable components exist

**id:** `component-count-seven-vs-eight`

**Ruling.** Seven manifest-backed components in `nsr.release.v1.components{}`: `launcher`, `runtime`, `offline-content`, `online-content`, `textures`, `xbox-payload`, `controller-db`. Plus the mods `catalog`, pinned by SHA-256 in the same signed manifest and not a member of `components{}`. `news config` is RETIRED as a component and folded into the launcher as a cached, fail-soft feed. The prose count and the schema now agree: seven in `components{}`, one catalog pin. Questionnaire row 18.1's count of eight survives with a different eighth member, so this is a membership change rather than a count reversal, and it still needs Adam's sign-off recorded in the reversal table.

**Files to edit:** `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`, `docs/architecture/SYSTEM_ARCHITECTURE.md`, `docs/discovery/PROJECT_QUESTIONNAIRE.md`

---

### 79. Component identifiers and the six-versus-seven mismatch in the components map

**id:** `component-id-naming-patch-vs-content`

**Ruling.** Canonical ids, kebab-case, used in manifests and in all prose: `launcher`, `runtime`, `offline-content`, `online-content`, `textures`, `xbox-payload`, `controller-db`, plus `catalog` for mods. The questionnaire's "offline patch", "online patch", "controller db" and "news config" wording is retired. CONTENT section 5's `components{}` gains `xbox-payload`, making it seven entries, matching the diagram.

**Files to edit:** `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`, `docs/architecture/SYSTEM_ARCHITECTURE.md`, `docs/discovery/PROJECT_QUESTIONNAIRE.md`

---

### 80. The launcher swap renames across INSTALL ROOT and DATA ROOT, violating the same-volume precondition

**id:** `launcher-swap-crosses-volume`

**Ruling.** The launcher's prior version never lives under DATA ROOT. Sequence: stage and verify the new bundle under `staging/launcher/<version>/` in DATA ROOT; the supervisor COPIES the verified bundle into `INSTALL ROOT/app.staging/` and re-verifies every hash at the destination; the launcher exits; the supervisor renames `INSTALL ROOT/app` to `INSTALL ROOT/app.prior` and `INSTALL ROOT/app.staging` to `INSTALL ROOT/app`, both same-directory renames on one volume; the supervisor starts the new launcher and waits for an explicit ready handshake; on failure it reverses both renames. `versions/launcher/prior/` is deleted from the layout. Precondition 1 then applies to DATA ROOT only and is never violated.

**Files to edit:** `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`, `docs/architecture/SYSTEM_ARCHITECTURE.md`

---

### 81. Launcher self-update renames between INSTALL ROOT and DATA ROOT

**id:** `launcher-swap-crosses-roots`

**Ruling.** Copy across roots, rename only within INSTALL ROOT. `INSTALL ROOT/app.staging` and `INSTALL ROOT/app.prior` replace `versions/launcher/prior/`. The one-volume precondition is restated as applying to DATA ROOT only.

**Files to edit:** `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`, `docs/architecture/SYSTEM_ARCHITECTURE.md`

---

### 82. GameSettings\GN7E69.ini receives three verdicts, and the Xbox copy reused verbatim has already drifted

**id:** `gamesettings-ini-three-verdicts`

**Ruling.** One verdict: PORT into generated configuration. The file is never copied forward from any of the three drifted copies (Windows payload 1,650 bytes, Xbox package 1,405 bytes, development runtime 1,731 bytes). The launcher generates it for the PC path; the `xbox-payload` component carries a generated copy for the Xbox path. The generated file MUST contain the `[Video_Hacks]` `EFBAccessEnable = True` block, which the Xbox copy does not have and which NFL Challenge portrait capture depends on. The two developer Gecko codes in the 1,731-byte copy are not shipped. XBOX section 6's "GameSettings\GN7E69.ini 1,405" row changes to the generated file, whose length is not yet known and is marked unverified, and the confirm-screen total is computed from the manifest rather than hardcoded. XBOX open item 9 (`EFBAccessEnable` absent from the Xbox `GFX.ini`) is a different file, stays open, and gains a cross-reference so the two are never conflated again.

**Files to edit:** `docs/architecture/XBOX_DEV_MODE_INSTALLER.md`, `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`, `docs/architecture/SECURITY_PRIVACY_LICENSING.md`

---

### 83. Whether fixed disks may be listed and whether a single removable drive may be preselected

**id:** `drive-picker-rules`

**Ruling.** Never preselect, ever, including when exactly one removable drive is present. Never list a non-removable fixed disk, and there is NO "Show all drives" toggle in v1, not in Settings and not anywhere else. SECURITY section 8's conditional preselect rule and XBOX section 7's toggle both change. Enumeration uses volume ROLE, not `GetDriveType`: on Windows `SetupDiGetClassDevs` plus `IOCTL_STORAGE_QUERY_PROPERTY` for `BusType` and `RemovableMedia`, on Linux `/sys/block/*/removable` plus udev `ID_BUS=usb`, on macOS `DAMediaRemovable` plus `DADeviceInternal`.

**Files to edit:** `docs/architecture/SECURITY_PRIVACY_LICENSING.md`, `docs/architecture/XBOX_DEV_MODE_INSTALLER.md`, `docs/architecture/SYSTEM_ARCHITECTURE.md`

---

### 84. Whether a single removable drive may be preselected

**id:** `drive-picker-preselection`

**Ruling.** No. Preselect nothing, ever, regardless of drive count. The confirm screen focuses Back, not Copy to drive. SECURITY section 8's "never preselect when more than one removable drive is present" drops the condition.

**Files to edit:** `docs/architecture/SECURITY_PRIVACY_LICENSING.md`, `docs/architecture/XBOX_DEV_MODE_INSTALLER.md`

---

### 85. Whether a non-removable fixed disk can ever be shown as a write target

**id:** `fixed-disk-enumeration`

**Ruling.** Never, and there is no toggle. Refusal is by volume role, not by size: a 2 TB external USB drive is a legitimate target and a 128 GB internal NVMe is not, and the role is determined by bus type and removable-media flags rather than by `GetDriveType`.

**Files to edit:** `docs/architecture/SECURITY_PRIVACY_LICENSING.md`, `docs/architecture/XBOX_DEV_MODE_INSTALLER.md`

---

### 86. Whether a format-the-drive operation may exist at all

**id:** `usb-format-path`

**Ruling.** No format, wipe or clean-the-drive operation exists anywhere in the product in v1. Not behind a confirm, not behind hold-to-confirm, not behind an advanced toggle, and not as a separate mode. XBOX section 8's "if and when the filesystem question is answered, a guided format can be added as its own mode" is deleted, because a future feature does not belong in a shipping safety contract. INFORMATION_ARCHITECTURE line 112 drops "format a USB drive" from the hold-to-confirm set. Questionnaire row 7.3's "never format without separate confirmation" is re-marked "never format".

**Files to edit:** `docs/architecture/SECURITY_PRIVACY_LICENSING.md`, `docs/architecture/XBOX_DEV_MODE_INSTALLER.md`, `docs/design/INFORMATION_ARCHITECTURE.md`, `docs/discovery/PROJECT_QUESTIONNAIRE.md`

---

### 87. Whether a removable, writable, non-system volume may be refused on filesystem grounds

**id:** `filesystem-refusal-before-write`

**Ruling.** Yes. SECURITY section 8's refusal table gains a filesystem row and adopts XBOX section 8's behavior table verbatim: exFAT proceed with a note, NTFS proceed with a caution line, FAT32 proceed with a caution line stating the real constraint (the payload fits, since the largest file is 1,459,978,240 bytes against a 4 GiB limit, but 2,782 files in one flat directory is heavy and console support is unconfirmed), anything else or undetectable refuse to write and say why, offering Back rather than Continue anyway. The filesystem is read before the first byte is written and recorded in the journal.

**Files to edit:** `docs/architecture/SECURITY_PRIVACY_LICENSING.md`, `docs/architecture/XBOX_DEV_MODE_INSTALLER.md`

---

### 88. Staged-write crash safety asserted flatly versus withdrawn for exFAT and FAT32

**id:** `staged-rename-atomicity`

**Ruling.** XBOX's conditioned form wins. SECURITY section 8's row becomes: "Write to temporary names, verify by hash, then rename into place. On NTFS the rename is expected to be atomic and the UI may say the copy is safe to interrupt. On exFAT and FAT32 no crash-safety claim is made, the UI says do not unplug the drive during the copy, and Resume re-hashes rather than trusting the journal." The engine never varies by filesystem; only the promise does.

**Files to edit:** `docs/architecture/SECURITY_PRIVACY_LICENSING.md`, `docs/architecture/XBOX_DEV_MODE_INSTALLER.md`

---

### 89. Whether write-temp-verify-rename is guaranteed crash-safe on the USB target

**id:** `staged-rename-crash-safety-claim`

**Ruling.** Not guaranteed. State the promise per detected filesystem, per XBOX section 9.2's table. CONTENT section 10's atomicity claim is scoped to DATA ROOT (NTFS, ext4, APFS) and stands as written, because it describes a different volume; add that scoping sentence so the two are not read as contradicting each other.

**Files to edit:** `docs/architecture/SECURITY_PRIVACY_LICENSING.md`, `docs/architecture/XBOX_DEV_MODE_INSTALLER.md`, `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`

---

### 90. Whether uninstall can reach the data root and delete memory cards

**id:** `uninstall-cannot-reach-saves-asserted-unconditionally`

**Ruling.** The "impossible" claim is false as written and must be requalified everywhere it appears. Separate roots make it impossible ONLY IF the replacement Inno script sets, and packaging asserts, all four of: `UsePreviousAppDir=no`, `UsePreviousData=no`, `DisableDirPage=yes`, and no `DelTree` of any kind. SYSTEM_ARCHITECTURE section 4 rule 1 and SECURITY section 27's `DelTree` row both gain that conditional clause and name the four directives.

**Files to edit:** `docs/architecture/SYSTEM_ARCHITECTURE.md`, `docs/architecture/SECURITY_PRIVACY_LICENSING.md`, `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`

---

### 91. The data-root layout differs across three documents

**id:** `data-root-layout-divergence`

**Ruling.** SYSTEM_ARCHITECTURE section 4 is the single authority and is rewritten to contain the full set, which every other document then references rather than restates: `downloads/<component>/<version>/`, `staging/<component>/<version>/`, `library/{offline,online,textures/<gameid>,xbox,work}/`, `runtime/<platform>/`, `mods/{staging/<id>/<version>,packages/<id>/<version>,active/<gameid>}/`, `mods/activation.json`, `music/{sources,packs}/`, `music/state.json`, `music/queue.json`, `saves/`, `stats/queue/`, `stats/local.db`, `versions/<component>/current.json`, `versions/<component>/prior/`, `recovery/<timestamp>/`, `backups/xbox/<ISO timestamp>/`, `xbox/journal-<volume-identity>.json`, `cache/`, `logs/`, `logs/diagnostics/`, `config/`, `journal.db`. `versions/launcher/prior/` does not exist; the launcher's prior version lives in INSTALL ROOT.

**Files to edit:** `docs/architecture/SYSTEM_ARCHITECTURE.md`, `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`, `docs/architecture/XBOX_DEV_MODE_INSTALLER.md`, `docs/architecture/SECURITY_PRIVACY_LICENSING.md`

---

### 92. The DATA ROOT on-disk layout

**id:** `data-root-layout`

**Ruling.** One canonical layout in SYSTEM_ARCHITECTURE section 4, as enumerated in data-root-layout-divergence. `mods/staging/<id>/<version>/` exists and is the extract-and-verify quarantine; top-level `staging/<component>/<version>/` exists and is the component staging directory; they are different directories with different jobs and both are named.

**Files to edit:** `docs/architecture/SYSTEM_ARCHITECTURE.md`, `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`, `docs/architecture/SECURITY_PRIVACY_LICENSING.md`

---

### 93. The data-root directory set differs in every document that draws it

**id:** `data-root-directory-set`

**Ruling.** Adopt the canonical list. `library/textures/<gameid>/` is named explicitly in SYSTEM_ARCHITECTURE section 4, replacing the vague "Reuse, move under `library/`" row. `recovery/<timestamp>/` and `backups/` both exist and are different: `recovery/` holds pre-operation recovery points, `backups/xbox/` holds user-initiated Xbox save backups.

**Files to edit:** `docs/architecture/SYSTEM_ARCHITECTURE.md`, `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`, `docs/architecture/XBOX_DEV_MODE_INSTALLER.md`, `docs/architecture/SECURITY_PRIVACY_LICENSING.md`

---

### 94. The data-root directory layout drifts across documents

**id:** `filesystem-layout-drift`

**Ruling.** One canonical layout in SYSTEM_ARCHITECTURE section 4; every other document references it and states only the directories it adds, with the addition also landing in section 4 in the same edit.

**Files to edit:** `docs/architecture/SYSTEM_ARCHITECTURE.md`, `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`, `docs/architecture/XBOX_DEV_MODE_INSTALLER.md`

---

### 95. Two crash-recovery journal formats and locations

**id:** `two-journal-formats`

**Ruling.** Two journals, deliberately, doing two jobs. `journal.db` (SQLite, DATA ROOT) owns the content state machine. `<data root>/xbox/journal-<volume-identity>.json` owns an interrupted copy to a foreign volume and stays JSON, on the PC, keyed on volume identity. Both carry a `schema` string and a schema version, both discard an unrecognized schema rather than parsing it, and both adopt XBOX's rule verbatim: a journal is a cache that can only ever save work, never authorize it. CONTENT section 10 gains that sentence.

**Files to edit:** `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`, `docs/architecture/XBOX_DEV_MODE_INSTALLER.md`

---

### 96. The Xbox destination profile has no schema or version field, yet resume keys on both

**id:** `xbox-profile-missing-version-fields`

**Ruling.** The profile object gains `"schema": "nsr.xboxprofile.v1"`, `"profile_version": <monotonic integer>` and `"manifest_id"` (the release label plus the `xbox-payload` component version it was generated against). Its per-entry `size` becomes `bytes`. XBOX section 9.5's discard rules then have real fields to key on.

**Files to edit:** `docs/architecture/XBOX_DEV_MODE_INSTALLER.md`

---

### 97. CONTENT proposes a drive version stamp and delegates to XBOX, which never answers

**id:** `xbox-version-stamp-orphan`

**Ruling.** Adopt it. The wizard writes `<user root>/nsr_drive.json`, roughly 200 bytes, carrying `{schema: "nsr.drivestamp.v1", release, xbox_payload_version, profile_version, written_at}`. It is a hint, never authority: content classification remains by manifest hash, and a stamp that disagrees with the content loses. It is listed in the manifest like any other file the wizard writes.

**Files to edit:** `docs/architecture/XBOX_DEV_MODE_INSTALLER.md`, `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`

---

### 98. Where the record of what a prepared drive contains lives

**id:** `xbox-staleness-record-location`

**Ruling.** Both. The journal stays on the PC at `<data root>/xbox/journal-<volume-identity>.json`, because the failure being recovered from is often the drive being yanked. A small `nsr_drive.json` stamp is also written to the drive, as a hint only. XBOX section 6's destination layout gains the stamp row.

**Files to edit:** `docs/architecture/XBOX_DEV_MODE_INSTALLER.md`, `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`

---

### 99. Whether the Xbox drive has any update or staleness mechanism

**id:** `xbox-update-path-exists-or-not`

**Ruling.** It does. Update and Repair are manifest-driven against the `xbox-payload` component, and staleness is detected by hash against the current manifest with the on-drive stamp as a hint. CONTENT section 16's "there is no update mechanism for the Xbox copy, and there is currently no way for a user to learn that theirs is stale" is factually superseded and must be rewritten to describe the re-run-the-wizard path.

**Files to edit:** `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`, `docs/architecture/XBOX_DEV_MODE_INSTALLER.md`

---

### 100. The size the Xbox wizard writes to a drive

**id:** `xbox-payload-size-2-9gb`

**Ruling.** 3,013,595,002 bytes, 3.01 GB, is the figure for today's payload, and the number the user sees is computed from the `xbox-payload` manifest rather than hardcoded, so it stays correct when the generated `GameSettings\GN7E69.ini` changes its length. The "2.9 GB" figure is retired from questionnaire row 7.1 and WIREFRAMES line 682. Three quantities are always named when used: the extracted package on disk (3,024,024,332 bytes), what lands on the drive (3,013,595,002 bytes), and the payload without the disc image (1,553,616,762 bytes).

**Files to edit:** `docs/architecture/XBOX_DEV_MODE_INSTALLER.md`, `docs/design/WIREFRAMES.md`, `docs/discovery/PROJECT_QUESTIONNAIRE.md`

---

### 101. How many Xbox wizard modes exist

**id:** `xbox-seven-modes-reduced`

**Ruling.** Five modes: New Xbox Installation, Update Existing Xbox Installation, Repair Xbox USB, Back Up Xbox Save, Restore Xbox Save. Modes 6 (Deploy or Reinstall Xbox App) and 7 (Test Xbox Connection) are RETIRED: no package, no certificate, no Device Portal, and no console connection exists. Questionnaire row 7.2 is re-marked from Decided-seven to Decided-five with the reason, in the Decided-row reversal table. Restore Xbox Save is the one mode permitted to write GC\, under the memory-card ruling.

**Files to edit:** `docs/architecture/XBOX_DEV_MODE_INSTALLER.md`, `docs/discovery/PROJECT_QUESTIONNAIRE.md`, `docs/design/INFORMATION_ARCHITECTURE.md`

---

### 102. Five cards in one paragraph and six in the next

**id:** `xbox-five-cards-then-six`

**Ruling.** Six cards: the five modes plus one "How to install the app on your Xbox" card that opens instructions and performs no automation. XBOX section 5's "Five cards, not seven" becomes "Six cards, not seven: five modes and one instructions card." INFORMATION_ARCHITECTURE lines 68 and 234 change to six.

**Files to edit:** `docs/architecture/XBOX_DEV_MODE_INSTALLER.md`, `docs/design/INFORMATION_ARCHITECTURE.md`

---

### 103. Architecture documents unilaterally reversing rows the questionnaire marks Decided

**id:** `questionnaire-decided-reversals`

**Ruling.** One mechanism plus one register. No architecture document may reverse a Decided row without naming the row number inline, stating the evidence, and tagging it "Needs Adam's sign-off". SYSTEM_ARCHITECTURE, as the umbrella document, gains a "Decided-row reversals" table listing every reversal across the set with row, document, section, evidence and status. The reversals ruled here: 18.1 component membership; 9.2 NSRONLINE name retired; 7.2 seven modes to five; 7.9 console pairing credentials retired; 17.9 statread1 is not a separate path; 15.3 and 15.9 region, ping and ping-derived delay de-scoped to post-launch; 16.5 no automatic forfeit; 8.1, 8.2 and 8.3 retained base and import retired by the locked ship-the-image decision; 11.6 eleven packages; 4.2 Fair Input Delay removed from the cannot-regress list; 7.1 the 2.9 GB figure. NOT reversed and to be restored: 14.9 (the launcher never posts to `ingest.php`) and 12.12 (no native rewrite on the launch path).

**Files to edit:** `docs/architecture/SYSTEM_ARCHITECTURE.md`, `docs/architecture/ONLINE_AND_RESULTS_PROTOCOL.md`, `docs/architecture/XBOX_DEV_MODE_INSTALLER.md`, `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`, `docs/discovery/PROJECT_QUESTIONNAIRE.md`

---

### 104. Decided rows reversed, two of them recorded nowhere

**id:** `decided-rows-reversed`

**Ruling.** Same mechanism and the same register. The two unrecorded ones are named explicitly: ONLINE section 10.1 reverses Decided row 14.9 and is itself reversed by this ruling set (the launcher never posts to `ingest.php`), and questionnaire row 8.3 "Import once, forever, retain a verified private base" is retired by the locked ship-the-image decision, which SYSTEM_ARCHITECTURE section 4 already acted on without recording the reversal.

**Files to edit:** `docs/architecture/SYSTEM_ARCHITECTURE.md`, `docs/architecture/ONLINE_AND_RESULTS_PROTOCOL.md`, `docs/discovery/PROJECT_QUESTIONNAIRE.md`

---

### 105. Whether a verified user-provided base is retained and an import pipeline exists

**id:** `retained-base-and-import-reversal`

**Ruling.** No retained base and no import pipeline. The locked decision to ship the finished game image removes both. Questionnaire rows 8.1, 8.2 and 8.3 are retired, and row 2.2's onboarding "import" step is replaced by the corrected preparation task list in WIREFRAMES section 5 (copy game files, install Reborn Dolphin, apply textures, apply your controller and graphics, check everything).

**REVERSED, 2026-09-02, by Adam directly.** "preparation doesnt have to do any installing, just configuring". The five-task list becomes two: **save your controller and graphics settings**, then **check the settings are in place**.

Copy game files and Install Reborn Dolphin were installer work. The installer exists to put files on the disk, and Dolphin is bundled and forced on Windows, so a first run step claiming to copy game files was either doing it twice or not doing it at all. Apply textures moved to launch, beside the configuration regeneration that already runs on every boot, so a player who changes their button art later does not have to re-run first run to see it.

Both remaining tasks call `Session::prepare`, the same code a launch calls, so first run cannot configure a machine differently from the way every later boot will. The check is a second `prepare` pass: it only writes what differs, so a pass that changes nothing proves the first one landed, and a pass that changes something means another program is rewriting the configuration.

Pinned by `preparation_only_configures`, which fails if any task label promises to copy, install, download or extract. `library/base/` does not exist.

**Files to edit:** `docs/architecture/SYSTEM_ARCHITECTURE.md`, `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`, `docs/discovery/PROJECT_QUESTIONNAIRE.md`

---

### 106. Whether an explicit quit in ranked can produce an automatic forfeit

**id:** `automatic-forfeit-on-quit`

**Ruling.** No automatic forfeit ships. A disconnect or an explicit quit HOLDS the match, Elo does not move, and a human resolves it. Forfeit is reachable only through moderation. Questionnaire row 16.5 is re-marked as reversed, with the reason recorded in the reversal register.

**Files to edit:** `docs/architecture/ONLINE_AND_RESULTS_PROTOCOL.md`, `docs/discovery/PROJECT_QUESTIONNAIRE.md`

---

### 107. Whether region and ping matchmaking and ping-derived input delay ship

**id:** `region-ping-decided-descoped`

**Ruling.** De-scoped to post-launch, and recorded as a reversal of Decided rows 15.3 and 15.9 in the register. v1 ships the harvested behavior plus one addition: `RANKED_RATING_WINDOW = 150`, `QUEUE_TIMEOUT_MS = 90000`, the 5000 ms tick, a coarse player-chosen region tag used as a soft preference, and a fixed `BufferSize` of 3. Ping measurement and ping-derived buffer size are post-launch features and must not be described as shipped promises.

**Files to edit:** `docs/architecture/ONLINE_AND_RESULTS_PROTOCOL.md`, `docs/discovery/PROJECT_QUESTIONNAIRE.md`

---

### 108. The governing protocol document still states the Xbox goal is Device Portal deployment

**id:** `start-here-device-portal`

**Ruling.** CLAUDE_START_HERE.md is edited. Line 150 becomes "The new goal is one safe PC-side wizard that prepares the USB drive. The user installs the app on the console themselves. No Device Portal, no app deployment, no certificate." Line 266 drops "Device Portal pairing" from the step 4 coverage list. XBOX open item 15's stale-documents table grows from five entries to six by adding CLAUDE_START_HERE.md with those two line numbers.

**Files to edit:** `CLAUDE_START_HERE.md`, `docs/architecture/XBOX_DEV_MODE_INSTALLER.md`

---

### 109. NSRONLINE.iso is half-renamed across the document set

**id:** `nsronline-half-rename`

**Ruling.** Complete the rename in one pass, and give it a destination name so it can be completed. The on-disk name inside `library/online/` is `FR4N31.iso` unless and until the online build changes its game ID, in which case the file is named `<game-id>.iso` and nothing else. Edit all occurrences in one commit: SYSTEM_ARCHITECTURE section 4's layout line; ONLINE section 5.1's `image_sha256` comment, section 10.2, section 14 step 2 and open item 2; CLAUDE_START_HERE lines 203 and 237; questionnaire rows 9.2, 9.6 and 9.8; COMPONENT_MAP's layout. That is seven files, not six: CONTENT's occurrence list omits CLAUDE_START_HERE's two.

**Files to edit:** `docs/architecture/SYSTEM_ARCHITECTURE.md`, `docs/architecture/ONLINE_AND_RESULTS_PROTOCOL.md`, `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`, `CLAUDE_START_HERE.md`, `docs/discovery/PROJECT_QUESTIONNAIRE.md`, `docs/discovery/COMPONENT_MAP.md`

---

### 110. The online image identifier is retired in one document and used in three others

**id:** `nsronline-half-renamed`

**Ruling.** Retired. The artifact is `FR4N31.iso` today and `<game-id>.iso` in general. Complete the rename across all seven files in one commit; do not leave it partial.

**Files to edit:** `docs/architecture/SYSTEM_ARCHITECTURE.md`, `docs/architecture/ONLINE_AND_RESULTS_PROTOCOL.md`, `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`, `CLAUDE_START_HERE.md`, `docs/discovery/PROJECT_QUESTIONNAIRE.md`

---

### 111. The name of the online disc image artifact

**id:** `nsronline-half-renamed-identifier`

**Ruling.** `<game-id>.iso`, which is `FR4N31.iso` on today's evidence. `NSRONLINE` is retired everywhere including in prose sentences such as "NSRONLINE must carry the statread1 hook", which becomes "the shipped online image must carry the statread1 hook". Questionnaire row 9.2 is re-marked.

**Files to edit:** `docs/architecture/SYSTEM_ARCHITECTURE.md`, `docs/architecture/ONLINE_AND_RESULTS_PROTOCOL.md`, `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`, `docs/discovery/PROJECT_QUESTIONNAIRE.md`

---

### 112. Verdict tables use at least a dozen words outside the five-word protocol

**id:** `verdict-vocabulary`

**Ruling.** Exactly five verdict words, uppercase, one per component, no compounds and no qualifiers in the verdict cell: REUSE, WRAP, PORT, REPLACE, RETIRE. Normative definitions: REUSE, the artifact ships or runs as it is; WRAP, the artifact still ships and gains a new caller or an additive extension; PORT, the logic, constants or rules move to a new implementation and the original file does not ship; REPLACE, a new implementation takes over the same job and the original does not ship; RETIRE, the job goes away and nothing replaces it. Every qualifier ("never write", "and extend", "relocated", "conditionally", "as precedent", "from C", "for now", "the idea", "the design", "the shape") moves into the Note column. "NEW", "Harvest", "Study", "Normalize", "Rebuild", "Preserve" and "not touched" are not verdicts: a component with no existing artifact is marked `n/a (new)` and gets no verdict, and a component a feature does not touch is simply absent from that feature's table. One sentence is added to the vocabulary: a verdict is about the PRODUCT, not about the disk, and no verdict ever authorizes deleting a file from a machine.

**Files to edit:** `docs/architecture/SYSTEM_ARCHITECTURE.md`, `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`, `docs/architecture/ONLINE_AND_RESULTS_PROTOCOL.md`, `docs/architecture/SECURITY_PRIVACY_LICENSING.md`, `docs/architecture/XBOX_DEV_MODE_INSTALLER.md`, `docs/architecture/DOLPHIN_INTEGRATION.md`

---

### 113. netplay_uia.ps1 gets three dispositions

**id:** `netplay-uia-verdict`

**Ruling.** RETIRE, one word. Note: kept out of the product as the calibration record of what Dolphin's NetPlay dialog looks like, which is a preservation obligation and not a verdict. SYSTEM_ARCHITECTURE section 5.6's "Study, then replace" is not a verdict and changes to RETIRE with that note. XBOX section 2's "not touched" row is removed entirely, since a component the Xbox feature does not touch does not belong in a table titled "every existing component this feature touches".

**Files to edit:** `docs/architecture/SYSTEM_ARCHITECTURE.md`, `docs/architecture/ONLINE_AND_RESULTS_PROTOCOL.md`, `docs/architecture/XBOX_DEV_MODE_INSTALLER.md`

---

### 114. NflsStatUpload carries four dispositions

**id:** `nflsstatupload-verdict`

**Ruling.** Two components, one verdict each. `NflsStatUpload.cpp` (the file, the token, `LoadConfig`, the HTTP calls) = RETIRE: compiled out of the shipping binary, so the job of posting from inside Dolphin ceases to exist. `NflsStatUpload`'s upload LOGIC = PORT to the launcher and supervisor: FNV-1a content hashing, the unchanged-for-one-full-scan settle rule, the durable ledger keyed on content hash, the Accepted / Transport / Rejected classification, and stopping after repeated hard rejections. SYSTEM_ARCHITECTURE's compound "Retire from C, port the logic to B" becomes those two rows. The interim if the removal slips (a compile-time disable of the uploader in release builds) is a note under the RETIRE row, not a fourth verdict.

**Files to edit:** `docs/architecture/SYSTEM_ARCHITECTURE.md`, `docs/architecture/ONLINE_AND_RESULTS_PROTOCOL.md`, `docs/architecture/SECURITY_PRIVACY_LICENSING.md`

---

### 115. NflsStatUpload is RETIRE in one document and PORT in another

**id:** `nflsstatupload-retire-vs-port`

**Ruling.** The file is RETIRE; the upload logic is PORT. Two rows, two components, two verdicts, in all three documents.

**Files to edit:** `docs/architecture/SYSTEM_ARCHITECTURE.md`, `docs/architecture/ONLINE_AND_RESULTS_PROTOCOL.md`, `docs/architecture/SECURITY_PRIVACY_LICENSING.md`

---

### 116. NflsStatUpload RETIRE versus PORT under a one-verdict protocol

**id:** `nflsstatupload-verdicts`

**Ruling.** `NflsStatUpload.cpp` = RETIRE. `NflsStatUpload` upload logic = PORT. Stated identically in SYSTEM_ARCHITECTURE section 2, ONLINE section 13 and SECURITY section 27.

**Files to edit:** `docs/architecture/SYSTEM_ARCHITECTURE.md`, `docs/architecture/ONLINE_AND_RESULTS_PROTOCOL.md`, `docs/architecture/SECURITY_PRIVACY_LICENSING.md`

---

### 117. NflsPlayHide carries three verdicts and one false capability claim

**id:** `nflsplayhide-verdict`

**Ruling.** WRAP, one word, in all three documents: the mask behavior is untouched and a published, latched port and team are added alongside `MaskSide()`. The capability claim is corrected everywhere: `NflsPlayHide.h` declares exactly `void Update();` and `int MaskSide();`, and the resolved port and team are frame-scoped locals that are discarded, so it exports nothing about identity. SYSTEM_ARCHITECTURE section 5.6's "Already resolves port to in-game team" and DOLPHIN section 7 phase 2's "It already solves the hard half" both become "it already contains the resolution logic and exports none of it". The second tree's adapted `NflsPlayHide.cpp` variant (4,556 bytes, identical mtime to the selected fork's copy, so chronological order is unverified) = RETIRE from the product, with the preservation obligation.

**Files to edit:** `docs/architecture/SYSTEM_ARCHITECTURE.md`, `docs/architecture/DOLPHIN_INTEGRATION.md`, `docs/architecture/ONLINE_AND_RESULTS_PROTOCOL.md`

---

### 118. NflsPlayHide has three different verdict words

**id:** `nflsplayhide-verdicts`

**Ruling.** WRAP. "Reuse and extend" and "Preserved and extended" are the same verdict written in off-protocol words and both change.

**Files to edit:** `docs/architecture/SYSTEM_ARCHITECTURE.md`, `docs/architecture/DOLPHIN_INTEGRATION.md`, `docs/architecture/ONLINE_AND_RESULTS_PROTOCOL.md`

---

### 119. The NflsPlayHide verdict and what it already provides

**id:** `playhide-verdict-and-capability`

**Ruling.** WRAP, and it provides no identity export today. Correct the claim in SYSTEM_ARCHITECTURE section 5.6 and DOLPHIN section 7 phase 2. Also record, in all three, what the chain still cannot do even when closed: play-call hiding is presentation only, runs on the client, writes no emulated memory by design, and is therefore defeatable by a modified client, so ranked integrity rests on the ladder, both-client corroboration and moderation, never on the emulator.

**Files to edit:** `docs/architecture/SYSTEM_ARCHITECTURE.md`, `docs/architecture/DOLPHIN_INTEGRATION.md`, `docs/architecture/ONLINE_AND_RESULTS_PROTOCOL.md`

---

### 120. PS Controls and Xbox Controls: reuse as prompt packs versus PORT to .nsmod

**id:** `prompt-packs-verdict`

**Ruling.** PORT, one word. They become `.nsmod` packages `reborn.prompts.ps` (19 files, 2,057,035 bytes) and `reborn.prompts.xbox` (19 files, 1,849,569 bytes), owned by the controller service and selected by detected device family rather than browsed in the Mods catalog. SYSTEM_ARCHITECTURE section 5.3's "Reuse as prompt packs, since that is all they ever were" moves entirely into the Note column under a PORT verdict.

**Files to edit:** `docs/architecture/SYSTEM_ARCHITECTURE.md`, `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`, `docs/architecture/SECURITY_PRIVACY_LICENSING.md`

---

### 121. Prompt packs reused in one document and ported in two others

**id:** `prompt-pack-verdicts`

**Ruling.** PORT. Two of the eleven cannot carry a different verdict from the other nine when they migrate through the same format and the same migration sequence.

**Files to edit:** `docs/architecture/SYSTEM_ARCHITECTURE.md`, `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`

---

### 122. The verdict on the PS Controls and Xbox Controls mods

**id:** `prompt-packs-reuse-vs-port`

**Ruling.** PORT to `.nsmod`, owned by the controller service, not browsed in Mods. SYSTEM_ARCHITECTURE section 5.3 changes.

**Files to edit:** `docs/architecture/SYSTEM_ARCHITECTURE.md`, `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`

---

### 123. rebornsetupv10.iss carries four dispositions

**id:** `inno-iss-verdict`

**Ruling.** Three components, three verdicts. Inno Setup, the tool = REUSE. `rebornsetupv10.iss`, the script = REPLACE: a new script is written and only the `AppId` GUID `{DCD6B9B9-11DE-4AC0-A7F5-256E7AC55A7E}` carries over, and only alongside `UsePreviousAppDir=no`, `UsePreviousData=no`, `DisableDirPage=yes`, no `DelTree` anywhere, and a build-time assertion of all four. Its named procedures `BackupUserData`, `RestoreUserData`, `CleanTexturesOnUpdate`, the uninstall `DelTree({app})` and `ShellExec('runas', JSGME.exe)` = RETIRE, listed individually because each is a named verified defect. SYSTEM_ARCHITECTURE section 5.4's "Wrap and reduce" is retired as a phrase.

**Files to edit:** `docs/architecture/SYSTEM_ARCHITECTURE.md`, `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`, `docs/architecture/SECURITY_PRIVACY_LICENSING.md`, `docs/architecture/XBOX_DEV_MODE_INSTALLER.md`

---

### 124. The verdict on rebornsetupv10.iss

**id:** `iss-wrap-and-reduce-vs-replace`

**Ruling.** Tool = REUSE, script = REPLACE, four named procedures = RETIRE. XBOX section 2's "REPLACE as precedent" row is reworded: it is not a verdict on the Inno script, it is a note recording two verified defects the Xbox wizard must not repeat (wildcards passed to `FileCopy`, which does not expand them, and `DelTree` over a folder containing memory cards).

**Files to edit:** `docs/architecture/SYSTEM_ARCHITECTURE.md`, `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`, `docs/architecture/SECURITY_PRIVACY_LICENSING.md`, `docs/architecture/XBOX_DEV_MODE_INSTALLER.md`

---

### 125. NSR_UpdateChecker retired for having no source located, while another document reads that source

**id:** `updatechecker-no-source-located`

**Ruling.** Verdict stays RETIRE; the reason is false and changes. The source exists and was re-verified this pass at `C:\Users\Adam\source\repos\NSR_UpdateChecker\Program.cs`, namespace `NFLStreetRebornLauncher`, with `UPDATE_URL = "https://nflstreetreborn.com/api/version.txt"`, `DOWNLOAD_URL = "https://nflstreetreborn.com/download"`, `CURRENT_VERSION = "2.1"` hardcoded, and `startInfo.WorkingDirectory = appPath` at line 96. The reason becomes: it downloads and installs nothing, and its working-directory choice is what makes the bare-filename netplay-gate defeat path reachable. Questionnaire row 18.10 ("no source located, endpoint unknown") closes.

**Files to edit:** `docs/architecture/SYSTEM_ARCHITECTURE.md`, `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`, `docs/discovery/PROJECT_QUESTIONNAIRE.md`

---

### 126. Retired for no source located while its constants are quoted from source

**id:** `update-checker-source-located`

**Ruling.** RETIRE, with the corrected reason. The source path, namespace, three constants and line 96 are all verified; delete "no source located" from both documents.

**Files to edit:** `docs/architecture/SYSTEM_ARCHITECTURE.md`, `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`

---

### 127. Whether the shipped emulator posts with a compiled-in token

**id:** `shipped-binary-token-claim`

**Ruling.** It does not, and both statements in SYSTEM_ARCHITECTURE must be rewritten. Verified by byte scan: the publicly distributed `NFL Street Reborn Dolphin.exe` (`8705a96b...`, 14,537,728 bytes, a 2021-dated Qt5 build) contains zero occurrences of the token value and zero of `ingest.php`, `LeagueGameStats`, `nsr_`, `Nfls` or `pick_six`. The token IS present once in the selected runtime `Dolphin.exe` (`617321bf...`), once in its `.bak`, and in the private git history at `eca379f`. Section 2 line 61 becomes "the selected runtime posts to the league endpoint using a token compiled into that binary; the publicly distributed build predates all Reborn stat work and contains no uploader". The risk register keeps severity High, because the blocking chain to lawful GPLv2+ distribution is unchanged, and changes its evidence cell to name the holder set: a private repository plus hand-delivered runtime folders, not the installed user base.

**Files to edit:** `docs/architecture/SYSTEM_ARCHITECTURE.md`, `docs/architecture/SECURITY_PRIVACY_LICENSING.md`

---

### 128. The Dolphin [Analytics] block gets REUSE, RETIRE and REPLACE in overlapping rows

**id:** `analytics-id-verdict`

**Ruling.** One component, one verdict: the `[Analytics]` block in every shipped payload = REPLACE. It is written as exactly three lines, `Enabled = False`, `PermissionAsked = True`, and NO `ID` key at all. On PC the launcher writes it every launch, since configuration is generated. On Xbox it ships corrected inside the `xbox-payload` component, because that path has no launcher. SECURITY section 27's two overlapping rows (REUSE the PC block, RETIRE the shared ID) collapse into this one row.

**Files to edit:** `docs/architecture/SECURITY_PRIVACY_LICENSING.md`, `docs/architecture/XBOX_DEV_MODE_INSTALLER.md`

---

### 129. Omit the analytics ID or generate one per install

**id:** `analytics-id-omit-vs-generate`

**Ruling.** Omit. Ship no `ID` key on any platform. XBOX section 2's "Generate per install, or omit the key" resolves to omit, which also removes the problem that the Xbox path has no launcher to generate anything.

**Files to edit:** `docs/architecture/SECURITY_PRIVACY_LICENSING.md`, `docs/architecture/XBOX_DEV_MODE_INSTALLER.md`

---

### 130. NflsUiSplit marked not portable while all four Reborn sources compiled clean on Linux

**id:** `nflsuisplit-portability`

**Ruling.** Both statements are true and the table column is mislabeled. Rename DOLPHIN section 5's column from "Portable" to "Has Windows-specific internals" and rewrite the `NflsUiSplit` row: it compiles on all three platforms (verified in CI), its Windows-specific code sits behind existing `_WIN32` guards, and off Windows it compiles to a no-op. It is verified present in `Source/Core/VideoCommon/CMakeLists.txt` at lines 96 and 97, so section 7's risk row changes from "Not in the CMake build" to "was absent from the CMake build until August 29, 2026; keep it out of the release gate".

**Files to edit:** `docs/architecture/DOLPHIN_INTEGRATION.md`

---

### 131. Whether NflsUiSplit is a feature that cannot regress

**id:** `uisplit-cannot-regress-vs-not-gating`

**Ruling.** Two different lists, and it belongs on one of them. `NflsUiSplit` is PRESERVED, meaning it stays in the tree and in both build systems and removing it is a regression, and it is NOT a RELEASE GATE, meaning a platform where it misbehaves does not block the release and it is disabled there. Questionnaire row 4.2 is re-titled "Behaviors that must not be removed", and a separate, shorter "Release gates" list is added containing `NflsPlayHide` and `NflsStatTracker` stat capture and not containing `NflsUiSplit`. XBOX section 3's "No" column is a scope statement about a different, non-Reborn binary on the console and is not a regression.

**Files to edit:** `docs/architecture/DOLPHIN_INTEGRATION.md`, `docs/discovery/PROJECT_QUESTIONNAIRE.md`, `docs/architecture/XBOX_DEV_MODE_INSTALLER.md`

---

### 132. Two fully-qualified names for the Discord presence setting

**id:** `discord-presence-config-key`

**Ruling.** The command-line form is `-C Dolphin.General.UseDiscordPresence=False` and it is the only correct one. Rewrite ONLINE open item 19 to say: the setting is `Config::MAIN_USE_DISCORD_PRESENCE`, declared as `{System::Main, "General", "UseDiscordPresence"}` with default `true`, which serializes to `[General] UseDiscordPresence` in `Dolphin.ini` and is addressed on the command line as `Dolphin.General.UseDiscordPresence`. `Main.General.UseDiscordPresence` is the C++ enum spelling and would silently no-op as a `-C` argument.

**Files to edit:** `docs/architecture/ONLINE_AND_RESULTS_PROTOCOL.md`

---

### 133. No document states the values for SAVEDATA_LOAD and SAVEDATA_WRITE, which redirect and write the memory-card path during online play

**id:** `netplay-savedata-memory-card-redirect`

**Ruling.** Every online launch pins all three explicitly in its generated block: `-C Main.NetPlay.SyncSaves=False`, `-C Main.NetPlay.WriteSaveData=False`, `-C Main.NetPlay.SyncCodes=False`. All three default to TRUE, so omitting them is the dangerous case, exactly like `[AutoUpdate] UpdateTrack`. SECURITY section 12's row "Memory card saves, leaves the machine: No" is only true once these are pinned, and gains that clause. ONLINE section 4.1 gains the three key names, the three values and the three defaults.

**Files to edit:** `docs/architecture/ONLINE_AND_RESULTS_PROTOCOL.md`, `docs/architecture/SECURITY_PRIVACY_LICENSING.md`, `docs/architecture/DOLPHIN_INTEGRATION.md`

---

### 134. Private lobby password: hash it or remove the concept

**id:** `lobby-password-hash-vs-drop`

**Ruling.** Drop it. Private lobbies are joined by the 6-character uppercase base36 room code plus the host's accept, and there is no password field anywhere in the schema, the API or the UI. SECURITY section 27's row changes from REPLACE-with-a-hash to RETIRE. ONLINE section 9.2's "store a hash or drop passwords" resolves to drop, and section 9.5's "the room password, which 9.2 says should not exist" becomes consistent.

**Files to edit:** `docs/architecture/ONLINE_AND_RESULTS_PROTOCOL.md`, `docs/architecture/SECURITY_PRIVACY_LICENSING.md`

---

### 135. Whether reborn://repair may start the read-only hash scan

**id:** `reborn-repair-deep-link-scope`

**Ruling.** It may not. `reborn://repair` navigates to Verify and Repair and starts nothing, including the read-only scan. SECURITY section 6's "The read-only scan may start" is deleted so the row reads exactly like the other eight.

**Files to edit:** `docs/architecture/SECURITY_PRIVACY_LICENSING.md`, `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`

---

### 136. Whether nsr.match.v1 is signed and whether the fork verifies it

**id:** `match-context-sig-required-or-optional`

**Ruling.** Optional and unenforced in v1, and not a ship prerequisite. ONLINE section 14 step 5 changes from "signed, expiring, nonce-bearing" to "expiring, nonce-bearing; `sig` present but unverified in v1". The security argument that carries the design is section 5.2's: the backend accepts a rating-affecting submission only against a match id and nonce it issued itself, so forging `nsr_match.json` produces a submission that is rejected on arrival, with or without a signature.

**Files to edit:** `docs/architecture/ONLINE_AND_RESULTS_PROTOCOL.md`

---

### 137. expect.image_sha256 as a full-file hash against an online image kept as RVZ

**id:** `image-sha256-vs-rvz-container`

**Ruling.** Two fields, because one cannot do both jobs. `expect.image_file_sha256` is the SHA-256 of the file the launcher will actually boot, which under the recommended container plan is the RVZ, and it is what the launcher checks against the signed manifest. `expect.image_disc_sha1` is the `DolphinTool verify` disc SHA-1, and it is what the backend compares across both clients, because it is the only identity that survives container conversion. Rename the field and add the second one in ONLINE section 5.1 and section 9.4; CONTENT section 9's "keep the online image as RVZ permanently" stands.

**Files to edit:** `docs/architecture/ONLINE_AND_RESULTS_PROTOCOL.md`, `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`

---

### 138. What online_minimum is compared against

**id:** `online-gate-version-space`

**Ruling.** Two distinct objects, named differently everywhere. `online_minimum` is a RELEASE LABEL: the backend's value is authoritative, the manifest field is a publish-time mirror, the higher of the two wins, comparison is over the published release ordering. It is the only thing the blocking screen shows. `runtime/<platform>/version.json`'s semver plus build hash is a separate check: it is carried in the match context's `expect` block, compared exactly against the peer, and recorded by the backend. ONLINE section 9.4's preflight row changes to "release label against `online_minimum`, plus an exact runtime match against the peer", and ONLINE gains the term `online_minimum`, which it currently never mentions.

**Files to edit:** `docs/architecture/ONLINE_AND_RESULTS_PROTOCOL.md`, `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`

---

### 139. The forced-online gate is a release label in one document and a runtime semver in another

**id:** `forced-online-gate-identifier`

**Ruling.** Release label, always, for the gate. The runtime semver plus build hash is a separate peer-and-backend equality check and is never called the gate. Both terms appear in both documents with those definitions.

**Files to edit:** `docs/architecture/ONLINE_AND_RESULTS_PROTOCOL.md`, `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`

---

### 140. The runtime directory is written three ways

**id:** `runtime-path-identifier`

**Ruling.** `runtime/<platform>/` everywhere, and the version file is `runtime/<platform>/version.json`. SYSTEM_ARCHITECTURE section 4's `runtime/` loses the ambiguity, and ONLINE sections 5.1 and 9.4 gain the platform segment.

**Files to edit:** `docs/architecture/SYSTEM_ARCHITECTURE.md`, `docs/architecture/ONLINE_AND_RESULTS_PROTOCOL.md`, `docs/architecture/DOLPHIN_INTEGRATION.md`

---

### 141. How the manifest signing trust anchor rotates after key loss or compromise

**id:** `signing-key-rotation-anchor`

**Ruling.** Adopt both proposals; they are complementary. TWO Ed25519 public keys are compiled into the launcher, a primary and a pre-published offline-held backup, and a manifest is accepted if it verifies under either. Both private keys are held offline on removable media and signing is an explicit local release step. Rotation means starting to sign with the backup key and shipping a new launcher that compiles in a new backup. SECURITY section 5's "if the key is lost, the recovery path is publishing a new public key in a new launcher build which every install must obtain out of band" is requalified to apply only when BOTH keys are lost.

**Files to edit:** `docs/architecture/SECURITY_PRIVACY_LICENSING.md`, `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`

---

### 142. Key custody and rotation stated two ways, and custody called both unspecified and recommended

**id:** `signing-key-custody-and-rotation`

**Ruling.** Custody is DECIDED: an offline private key on removable media, signing as an explicit local release step, never in a CI secret. Rotation is DECIDED: two compiled-in public keys, primary and pre-published backup, either accepted. What remains OPEN is only the key MATERIAL and the operational plan: no key has been generated, no custody medium chosen, and no written loss-recovery procedure exists. CONTENT's open item 1 is narrowed to those three, rather than saying the design is missing.

**Files to edit:** `docs/architecture/SECURITY_PRIVACY_LICENSING.md`, `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`

---

### 143. What happens if the manifest signing key is lost

**id:** `signing-key-loss-recovery`

**Ruling.** Sign with the pre-published backup key and ship a new launcher carrying a new backup. No user reinstalls. The reinstall-and-obtain-out-of-band path applies only if both keys are lost, and SECURITY's sentence is requalified to say so. Two public keys are compiled in, not one.

**Files to edit:** `docs/architecture/SECURITY_PRIVACY_LICENSING.md`, `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`

---

### 144. Which credentials must be rotated

**id:** `credential-inventory-disagrees`

**Ruling.** Five names, in one canonical list in SECURITY section 1, which every other document points at instead of restating a count: (1) the league ingest bearer token in `NflsStatUpload.cpp`; (2) `JWT_SECRET`; (3) `DISCORD_CLIENT_SECRET`; (4) `ADMIN_SECRET`, which is generated per session by `crypto.randomBytes(32)` and stored nowhere, so it has nothing to rotate but does need the Railway `/live` transport decision; (5) the league site's `_inc/config.php` `DB_PASS`, recorded by ONLINE section 12 as exposed in a chat transcript on July 27, 2026 and still unrotated, marked "league site, outside launcher scope, same rotation run". Rotation of the ingest token must be paired with server-side revocation, and the league deploy README records that the Dolphin `DEFAULT_TOKEN` and the server's `league/_inc/ingest_token.php` match as shipped, so rotating one side alone breaks ingest for every existing build.

**Files to edit:** `docs/architecture/SECURITY_PRIVACY_LICENSING.md`, `docs/architecture/SYSTEM_ARCHITECTURE.md`, `docs/architecture/ONLINE_AND_RESULTS_PROTOCOL.md`, `docs/discovery/PROJECT_QUESTIONNAIRE.md`

---

### 145. Whether the online prototype working trees and their .env files may be deleted

**id:** `env-trees-retire-vs-preserve`

**Ruling.** The trees are kept; the `.env` files are deleted. SECURITY section 27's row "RETIRE the local trees" is wrong and becomes "RETIRE the five `server/.env` FILES", deleted by enumeration and each replaced with a `.env.example` carrying key names and no values. The working trees themselves are REUSE as reference material and are not deleted, consolidated, moved or promoted, per the locked decision and CLAUDE_START_HERE's rule against deleting duplicate folders before their differences are documented. This is a direct application of the vocabulary rule that a verdict is about the product, not about the disk.

**Files to edit:** `docs/architecture/SECURITY_PRIVACY_LICENSING.md`, `docs/architecture/SYSTEM_ARCHITECTURE.md`

---

### 146. How many prototype copies and server/.env files exist

**id:** `prototype-copy-count-three-vs-five`

**Ruling.** Five of each. SYSTEM_ARCHITECTURE section 5.6's "all three copies as reference" becomes five, and questionnaire row 3.6 changes the same way. The five `.env` files are enumerated by path in SECURITY correction 0.8 and deleted by enumeration, never by count. The locked decision is unaffected: whether there are three copies or five, all of them stay untouched as reference.

**Files to edit:** `docs/architecture/SYSTEM_ARCHITECTURE.md`, `docs/architecture/SECURITY_PRIVACY_LICENSING.md`, `docs/discovery/PROJECT_QUESTIONNAIRE.md`

---

### 147. Whether the existing Discord application is reused

**id:** `discord-app-reuse-vs-ownership`

**Ruling.** ESCALATE TO ADAM, with one question: "Is the Discord application behind `DISCORD_CLIENT_ID` and `DISCORD_CLIENT_SECRET` owned by an account you control?" Until it is answered, ONLINE section 13's flat REUSE is downgraded to "REUSE if owned; otherwise n/a (new application)", and rotation step 3 is marked blocked. If the owner is one of the two named third-party GitHub account holders rather than Adam, the correct action is to create a new application under an owned account, not to rotate someone else's. Questionnaire row 13.7 stays Confirm.

**Files to edit:** `docs/architecture/ONLINE_AND_RESULTS_PROTOCOL.md`, `docs/architecture/SECURITY_PRIVACY_LICENSING.md`, `docs/discovery/PROJECT_QUESTIONNAIRE.md`

---

### 148. Both documents describe Linux and macOS builds as incomplete object counts

**id:** `cross-platform-build-status-stale`

**Ruling.** Replace both statements with today's verified result: the fork BUILDS on Linux and macOS in CI from a clean checkout, Linux artifact about 9 MB and macOS artifact 94,920,839 bytes, with a gate step that fails the job if any of `NflsPlayHide`, `NflsStatTracker`, `NflsStatUpload` or `NflsUiSplit` is missing from either build tree. Neither binary has been run: "builds" is the claim, "works" is not, and that sentence ships with the statement. DOLPHIN section 3's failure ledger gains an eleventh entry for the third latent defect the probe surfaced: upstream `Resources.h` declared `QIcon` members without including `QIcon`. SYSTEM_ARCHITECTURE's risk row splits: building off Windows is now Low, running off Windows stays High with the mitigation "run them".

**Files to edit:** `docs/architecture/DOLPHIN_INTEGRATION.md`, `docs/architecture/SYSTEM_ARCHITECTURE.md`, `docs/discovery/PROJECT_QUESTIONNAIRE.md`

---

### 149. The state of the Linux and macOS Dolphin builds

**id:** `ci-build-status-stale`

**Ruling.** Both build in CI from a clean checkout; Linux artifact about 9 MB, macOS artifact 94,920,839 bytes; all four Reborn objects confirmed present by a gate step in both trees; neither binary has been run. Questionnaire row 4.7 ("Neither binary exists") closes. The `Resources.h` QIcon defect is added to the failure ledger.

**Files to edit:** `docs/architecture/DOLPHIN_INTEGRATION.md`, `docs/architecture/SYSTEM_ARCHITECTURE.md`, `docs/discovery/PROJECT_QUESTIONNAIRE.md`

---

### 150. Whether the Xbox wizard exists on Linux and macOS

**id:** `platform-agnostic-vs-xbox-scoping`

**Ruling.** It exists on all three platforms from the first commit. Drive enumeration, volume-role checks, filesystem detection and safe eject are one trait with three implementations, and XBOX section 7 already specifies all three. No platform gets a stub that silently does nothing: a backend that cannot be verified returns "no eligible drives found" and states why, and the UI says the platform is untested. XBOX open item 7 CLOSES.

**Files to edit:** `docs/architecture/XBOX_DEV_MODE_INSTALLER.md`, `docs/architecture/SYSTEM_ARCHITECTURE.md`

---

### 151. The population of 1,459,978,240-byte files stated as four, as 68, and as 22 distinct builds

**id:** `same-size-image-population`

**Ruling.** Three separate statements, each scoped. SECURITY section 5's sentence becomes "all FOUR disc images IN THE SELECTED RUNTIME are exactly 1,459,978,240 bytes and differ in content", which is true and is the point being made (size cannot distinguish a build). CONTENT section 1's population figure stands: 68 files of that exact size across the four scanned roots, of which 22 are byte-identical to `78a987c7...`, and that count is a floor, not a total. CONTENT section 4's phrase "22 distinct builds among the ones checked" is WRONG and becomes "22 byte-identical copies of one build, alongside 46 files of the same size that are not that artifact".

**Files to edit:** `docs/architecture/SECURITY_PRIVACY_LICENSING.md`, `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`

---

### 152. nsr_ingest.txt described as two lines and as three

**id:** `nsr-ingest-txt-line-count`

**Ruling.** Three lines: endpoint URL, bearer token, optional league id, with `DEFAULT_LEAGUE_ID = 8` as the fallback. COMPONENT_INVENTORY line 217's two-line description is wrong and changes. The file itself is RETIRE, since `NflsStatUpload` is compiled out of the shipping binary, and the line count matters only for the rotation procedure, which is why it is recorded rather than dropped. SYSTEM_ARCHITECTURE section 2 is incomplete rather than wrong and gains a sentence naming the override file.

**Files to edit:** `docs/architecture/SYSTEM_ARCHITECTURE.md`, `docs/architecture/SECURITY_PRIVACY_LICENSING.md`, `docs/discovery/COMPONENT_INVENTORY.md`

---

### 153. The nsr_ingest.txt override file line count

**id:** `nsr-ingest-override-line-count`

**Ruling.** Three lines, verified. COMPONENT_INVENTORY changes; SECURITY section 27's RETIRE row gains the line count so a reader of that table alone knows what is being retired.

**Files to edit:** `docs/architecture/SECURITY_PRIVACY_LICENSING.md`, `docs/discovery/COMPONENT_INVENTORY.md`

---

### 154. placeholder

**id:** `nflsuisplit-portability-note`

**Ruling.** Not applicable; see nflsuisplit-portability.

**Files to edit:** `docs/architecture/DOLPHIN_INTEGRATION.md`

---
