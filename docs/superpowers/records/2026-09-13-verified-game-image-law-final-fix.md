# Final fix report: whole-branch review of `verified-game-image-law`

Branch `verified-game-image-law`, worktree `target/game-image-law`.
Base at start: `12479ce`. Implementation commit: `659a384`.

All nine findings were confirmed on inspection. None turned out to be wrong.
Critical 1's fix did not need a refactor of the mods system: the composer
already had a marker-gated removal (`Composer::discard`), so "withheld" is
expressible as a third state on the request rather than as new machinery.

## Commits

One implementation commit, `659a384`, plus this report.

The nine findings do not divide into separate commits that each compile.
Criticals 1, 2 and 3 and findings 4, 5, 6, 7 and 8 all touch
`crates/reborn-ui/src/main.rs`, and splitting by finding would require
either a non-compiling intermediate commit (core's `SyncRequest` changes
shape while the window still sets the old field) or hunk-level surgery on
one file to fake a split that the work does not actually have. One commit,
with the message naming each finding, is the honest shape.

---

## Critical 1: `textures_allowed = false` did not switch the pack off

### What I changed

`crates/reborn-core/src/mods.rs`

- `SyncRequest::manage_user_texture_folder: bool` became
  `SyncRequest::texture_policy: TexturePolicy`, a new three-state enum:
  - `Manage` — compose the active set. The normal case.
  - `LeaveAlone` — a texture author owns that folder. Nothing composed,
    nothing touched, nothing removed. Exactly the old `false`.
  - `Withhold` — the image is not one Reborn verified, so Reborn's art must
    not be painted on it. The composed tree is removed.
- `TreeStep` gained `Withheld { removed: bool }`, and `SyncReport::explain`
  gained wording for it ("removed, because this game file is not an NFL
  Street Reborn build").
- `sync_active`'s early return became a `match` on the policy. The
  `Withhold` arm calls `foreign_tree_refusal` and then `Composer::discard`,
  as instructed — no new removal logic.

`crates/reborn-core/src/session.rs`

- `textures_for_launch(bool, bool) -> bool` became
  `pub fn texture_policy_for_launch(bool, bool) -> TexturePolicy`. It is now
  public because a launch is not the only thing that composes into that
  folder (see Critical 3), and a second copy of the rule is how the two would
  drift apart.

`crates/reborn-ui/src/main.rs`

- `sync_view` maps `TreeStep::Withheld` to the wire value `"withheld"`, and
  the `link` field likewise, so the Mods page never reports a withheld tree
  as composed in the user folder.

### Why

`manage_user_texture_folder = false` means "a texture author owns this
folder, leave it alone", and leaving the folder alone is precisely what
fails to help: for any player who has ever booted successfully, the whole
composed pack is already sitting at `<user dir>/Load/Textures/GN7E69`
(`mods.rs`'s `composed_root`), and `HiresTextures` is a Locked `True` key
(`settings_law.rs:178`), so Dolphin loads it for whatever `GN7E69` disc
boots. Declining to rebuild changes nothing on disk. Removal is the only
thing that switches the pack off.

### How I ordered author-owned against withheld

**Author-owned wins outright.** `LeaveAlone` is checked first and returns
before `Withhold` is ever considered, both in `sync_active` and in
`texture_policy_for_launch`. A player on an unverified disc who has
`manage_texture_folder = false` keeps their folder untouched.

Three reasons, in order of weight:

1. Turning management off is a deliberate, documented assertion that the
   contents of that folder are the player's own. Reborn does not delete out
   of a folder it was told not to manage.
2. The composed marker is **not** proof the tree is free of the author's
   work. A texture author who started from Reborn's pack and edited files in
   place still has the marker sitting next to their edits, so a
   marker-gated delete is not as safe as it first looks. There is precedent
   in this project for exactly that loss.
3. The guarantee is still fully kept on the only path a player reaches
   without opting out. Management is on by default
   (`settings::manage_texture_folder` defaults to `true`), so the Discord
   case and every case like it is covered.

The residual hole is stated rather than hidden: a player who disables
management **and** points Reborn at an unconverted disc will keep whatever is
in that folder, including a stale Reborn pack. That is a knowingly taken
action with a setting whose documented meaning is "yours is the source of
truth".

Belt and braces underneath it: even in the `Withhold` arm, removal goes
through `Composer::discard`, which removes only a directory carrying
`COMPOSED_MARKER` or an empty one, and refuses anything else. A foreign tree
comes back as `SyncOutcome::Refused(...)`, not as a delete and not as an
error — a refusal is a state the caller must be able to show, and a launch
blocked over button art is worse than the art being stale.

### Covering tests

`crates/reborn-core/src/mods.rs`:

- `withholding_removes_the_pack_a_previous_launch_composed` — composes the
  pack first (the way a real player's first good boot does), then withholds,
  and asserts the composed root **no longer exists**. This is the test that
  would have caught the original defect: under the old boolean the folder
  survived untouched.
- `withholding_refuses_rather_than_delete_a_folder_that_is_not_ours`
- `withholding_with_nothing_composed_is_quiet`

`crates/reborn-core/src/session.rs`:

- `an_unverified_image_never_gets_rebornes_texture_pack` — rewritten to
  assert the **variant**, `TexturePolicy::Withhold`, not a boolean. A
  boolean assertion is what let the two meanings collapse together.
- `the_authors_folder_is_never_taken_away_even_on_an_unverified_image` —
  pins the ordering above in both directions.

```
test mods::tests::withholding_removes_the_pack_a_previous_launch_composed ... ok
test mods::tests::withholding_refuses_rather_than_delete_a_folder_that_is_not_ours ... ok
test mods::tests::withholding_with_nothing_composed_is_quiet ... ok
test session::tests::an_unverified_image_never_gets_rebornes_texture_pack ... ok
test session::tests::the_authors_folder_is_never_taken_away_even_on_an_unverified_image ... ok
```

Demonstration that the pin fails when the fix is removed (the `Withhold` arm
reduced to `let removed = false;`, i.e. back to what the boolean did, with
every comment left in place):

```
test mods::tests::withholding_refuses_rather_than_delete_a_folder_that_is_not_ours ... FAILED
test mods::tests::withholding_removes_the_pack_a_previous_launch_composed ... FAILED
  a withheld sync must report that it took the tree away
    left: Withheld { removed: false }
   right: Withheld { removed: true }
```

---

## Critical 2: the Tier 2 acknowledgment was granted silently

### What I changed

`crates/reborn-ui/src/main.rs`

- New `enum Consent { NotAsked, PlayItAnyway }`. An enum, not a `bool`,
  so a future caller cannot pass consent by writing a positional `true`.
- `record_verdict` takes a `Consent`. `PathChoice::Acknowledge` with
  `Consent::NotAsked` now returns `Err(UNVERIFIED_SETUP_GAME)` and writes
  nothing. The acknowledgment is written only on `Consent::PlayItAnyway`.
- The Tier 2 copy moved into named constants — `UNVERIFIED_GAME_HEADLINE`,
  `UNVERIFIED_GAME_DETAIL`, `REPAIR_ACTION` ("Repair my game"),
  `PLAY_ANYWAY_ACTION` ("Play it anyway") — and a new `UnverifiedGameView`
  carries them to the page, so the launch refusal, the Play badge and the
  dialog cannot drift into three descriptions of one disc.
- `settings_set_path` gained `acknowledge_unverified: Option<bool>`. Without
  it, an `Unconverted` image stores **nothing** and comes back as
  `PathSetView { value: None, unverified: Some(...) }` — a choice to put to
  the player, not an error and not a silent enrolment.

`crates/reborn-ui/ui/settings.js`

- `boxesBlock`'s save handler catches `r.unverified` and opens the choice.
- `unverifiedChoice()` draws the spec's card as a `<dialog>` (with
  `showModal?.()`, so a runtime without it still renders the card):
  headline, detail, then **[Repair my game]** as `class="primary"`,
  **[Play it anyway]** plain, and Cancel.
- `playItAnyway()` is the only call anywhere in the launcher that sets
  `acknowledgeUnverified: true`.
- `repairGame()` calls `setup_download_reborn_game` (see finding 8).

`crates/reborn-ui/ui/onboarding.js`

- The installation panel's `settings_set_path` caller surfaces a returned
  `unverified` as a refusal on the panel instead of silently doing nothing.

No surface says "custom" for this state; a test asserts that.

### Covering tests

`crates/reborn-ui/src/main.rs`:

- `an_unconverted_image_is_never_acknowledged_without_being_asked` —
  behavioural, against a real settings file: `NotAsked` refuses and leaves
  `unverified_game_ack` as `None`; `PlayItAnyway` records this exact image's
  SHA-256.
- `consent_only_applies_to_the_verdict_that_asks_for_it` — consent cannot
  manufacture an acknowledgment for a `RebornBuild`, and cannot talk an
  `Unreadable` file past the refusal.
- `nothing_but_a_confirmed_choice_writes_the_unverified_acknowledgment` —
  guard. Counts writes of the field across `main.rs`, `setup_tools.rs` and
  `dolphin_settings.rs` through `code_only`; exactly one is allowed. The
  needle is built with `concat!` so the test cannot count its own source,
  which is one of the ways a grep-style guard becomes incapable of failing.
- `the_tier_two_choice_reads_as_a_fault_to_repair` — the words are the
  spec's, no "custom", no em-dashes, and `UNVERIFIED_GAME` still starts with
  the headline and contains the detail.

`Tools/check-shell-pages.mjs`, driven with a fake core:

- "Choosing an unconverted image puts the choice to the player"
- "Nothing is acknowledged before the player answers"
- "Play it anyway is the only thing that acknowledges"

```
test tests::an_unconverted_image_is_never_acknowledged_without_being_asked ... ok
test tests::consent_only_applies_to_the_verdict_that_asks_for_it ... ok
test tests::nothing_but_a_confirmed_choice_writes_the_unverified_acknowledgment ... ok
test tests::the_tier_two_choice_reads_as_a_fault_to_repair ... ok

ok    Choosing an unconverted image puts the choice to the player
ok    Nothing is acknowledged before the player answers
ok    Play it anyway is the only thing that acknowledges
```

Demonstrations (fix removed, comments intact):

- Delete the `Consent::NotAsked` arm from `record_verdict`, so it
  acknowledges whoever asks, as it did before:
  ```
  an unconverted image was accepted with nobody asked: Ok(())
  ```
- Add a second, unconsented write of the field:
  ```
  the Tier 2 acknowledgment must be written in exactly one place, ...
    left: 2   right: 1
  ```
- Drop `settings.js`'s `if (r && r.unverified)` line so the page stores
  regardless:
  ```
  FAIL  Choosing an unconverted image puts the choice to the player
  FAIL  The escape hatch is offered
  ```

---

## Critical 3: `sync_active_now` composed with no verdict at all

### What I changed

`crates/reborn-ui/src/main.rs`

- `sync_active_now` sets `req.texture_policy = texture_policy_now(...)`
  instead of reading the preference alone.
- New `texture_policy_now(p, runtime_hint, preferences)`: resolves the image
  the way a launch does (`find_runtime_and_game_for`, falling back to the
  saved `game_path` when the emulator cannot be resolved — a missing
  emulator says nothing about the disc), asks `verdict_now`, then hands the
  answer to `reborn_core::session::texture_policy_for_launch`. The decision
  is the launch's own rule, not a second copy.
- It fails toward withholding, with an `eprintln!` saying why. An image the
  launcher cannot resolve or cannot read is not one it can vouch for; being
  wrong that way costs a rebuild on the next good launch, and being wrong
  the other way is the bug this law exists to remove.

This closes all three reachable callers named in the finding: `mods_sync`,
`mods_toggle` and `controls_set`.

### Covering test

`every_composer_of_the_texture_pack_asks_what_the_image_is` — guard, through
`code_only`: `sync_active_now` must call `texture_policy_now`;
`texture_policy_now` must call `verdict_now` **and**
`texture_policy_for_launch`.

```
test tests::every_composer_of_the_texture_pack_asks_what_the_image_is ... ok
```

Demonstration (the call replaced by the preference-only expression it used
to be, comments intact):

```
sync_active_now composes the texture pack without ever asking what the image is
```

---

## 4. `may_adopt` and `path_choice` disagreed one click apart

**Rule chosen: `may_adopt`'s, the stricter one.** Setup never adopts a disc
that is not a Reborn build, whichever screen asked.

Reasons: setup has a real repair one button away (the full-disc download),
the wizard already renders the Tier 2 words and offers that button, and the
acknowledgment is a deliberate escape hatch that belongs behind the Settings
dialog rather than behind a Next button or a review-screen Edit.

- `setup_import_existing_game` (`setup_tools.rs`) passes
  `Consent::NotAsked`, so an `Unconverted` disc is now refused with
  `UNVERIFIED_SETUP_GAME` instead of auto-acknowledged. The wizard's Edit
  path throws that into `showStepError`, which is how every other refusal
  reaches that screen.
- `onboarding_confirm_game` (`main.rs`) likewise.
- `setup_migrate_install` is unchanged: it already refused via `may_adopt`.

Covering test: `setup_never_adopts_a_disc_that_is_not_a_reborn_build`
(guard, `code_only`) plus the behavioural `record_verdict` test above, which
proves what `Consent::NotAsked` actually does.

```
test tests::setup_never_adopts_a_disc_that_is_not_a_reborn_build ... ok
```

Demonstration (flip `setup_import_existing_game` to `Consent::PlayItAnyway`):

```
setup_import_existing_game can still acknowledge a disc that is not a Reborn build without asking anybody
```

---

## 5. `rebornctl play` was an ungated boot path

**Gated**, as preferred. No spec change needed.

- `may_play` and `UNVERIFIED_GAME` moved from `reborn-ui/src/main.rs` into
  `reborn_core::game_identity`, and the window now `use`s them. One rule,
  one place, exactly as the spec's section 1 requires; the alternative was a
  second copy of four load-bearing lines in a second crate.
- New `game_identity::find_tool`, shaped like `game_patch::find_xdelta`: the
  bundled DolphinTool beside the installed program, then beside the running
  executable.
- `reborn-cli`'s `play` builds its request with `textures_allowed: false`,
  runs the dry-run branch, then gates: `may_play_before_boot` (a CLI twin of
  the window's, reading the same settings file so a player who took "Play it
  anyway" is not asked twice), refusing with the same sentence and exiting
  `PROBLEM`, or setting `textures_allowed` from the answer and saying out
  loud when the art is withheld.

The gate deliberately sits **after** the dry-run branch: `Session::prepare`
never reads `textures_allowed` (only `Session::run` composes), so a dry run
does not need to pay a whole-disc SHA-256.

Covering test: `play_asks_what_the_disc_is_before_it_boots_it` in
`reborn-cli`, using a local `code_only` copied from the window's.

```
test tests::play_asks_what_the_disc_is_before_it_boots_it ... ok
```

Demonstration (`req.textures_allowed = true;` restored, comments intact):

```
rebornctl play boots the game without asking whether the disc is a Reborn build
```

---

## 6. Every Play press paid a full-disc SHA-256

- `GAME_VERDICT_CACHE` hoisted out of `launch_check`'s body to module scope,
  so it is no longer a cache only one caller can hit.
- New `verdict_now(p, image)` fronts it: builds the path/size/mtime key,
  calls `cached_game_verdict`, and falls back to a fresh `identify` (with an
  `eprintln!`) when the metadata will not read. A missing key is a reason to
  work harder, never a reason to skip the question.
- `may_play_before_boot` and `texture_policy_now` both go through it.
- The acknowledgment's hash failure is no longer `.unwrap_or(false)`. It
  prints which file would not hash, so "it keeps telling me to repair" is
  diagnosable instead of being indistinguishable from "never acknowledged".

Covering test:
`the_launch_gate_reuses_the_verdict_instead_of_rehashing_the_whole_disc` —
guard, `code_only`: the gate must contain `verdict_now`, must **not**
contain `game_identity::identify`, and must contain an `eprintln!`;
`verdict_now` must contain `cached_game_verdict`. The existing
`cached_game_verdict_only_recomputes_when_its_key_changes` still pins the
cache's own behaviour.

```
test tests::the_launch_gate_reuses_the_verdict_instead_of_rehashing_the_whole_disc ... ok
```

Demonstration (gate reverted to an inline uncached `identify`):

```
the launch gate identifies the image without going through the cache
```

---

## 7. `no-chain` and `unknown` accused a verified disc

`setup_tools.rs`'s verdict override gained an arm before the catch-all:

```rust
Verdict::RebornBuild{label:build,..} if outcome=="unknown"||outcome=="no-chain" =>
    ("not-updated".to_string(), Some(build.clone()).filter(|b|!b.is_empty()), note),
```

A patch that refused or could not run is a patch failure, not a verdict on
the disc. `onboarding.js` gained the matching case:

> **Your game is ready** — This is an NFL Street Reborn build. The update to
> the newest version could not be applied right now, and the launcher will
> try again after setup.

With a genuine Reborn build routed away from `unknown`/`no-chain`, what is
left on those two outcomes is a disc that is **not** a Reborn build, so both
joined `blocked()`: Continue is disabled and the recovery download is
offered, which is what it exists for. That also covers `Convertible` plus a
missing xdelta3, which previously reported a failure headline while leaving
Continue enabled and offering no repair.

All three surfaces now agree about one disc: a verified build gets
patch-failure words, an enabled Continue and a silent Play badge; an
unrecognized disc gets the verdict's words, a disabled Continue, the
recovery button, and the Play badge.

Covering tests:

- `a_patch_that_did_not_run_is_reported_as_a_patch_failure` (guard).
- `outcome-words.test.mjs`: "a verified Reborn build whose update did not run
  is not accused of being someone else's disc".
- The existing `the_recovery_download_is_offered_only_for_a_disc_we_could_not_convert`
  was updated to pin the widened `blocked()` set, with its prose rewritten to
  say why the set widened.

```
test tests::a_patch_that_did_not_run_is_reported_as_a_patch_failure ... ok
✔ a verified Reborn build whose update did not run is not accused of being someone else's disc
```

Demonstration (arm deleted, comments intact):

```
a verified Reborn build whose patch step refused still reports the verdict's own wording,
which the wizard renders as "Your game file isn't NFL Street Reborn"
```

---

## 8. The refusal and the badge pointed at a page with no repair

`settings.js` gained two things on the Files & storage tab:

- `repairBlock()` — standing, drawn whenever `launch_check.game_verdict`
  is `"unconverted"`: the Tier 2 headline, the detail, and a primary
  **Repair my game** that calls `setup_download_reborn_game`, the same
  repair the wizard offers.
- `unverifiedChoice()` — the Critical 2 dialog, which offers the same
  Repair as its primary action.

`draw()` filters nulls out of `parts` before `replaceChildren` (a null there
renders as the word "null") and opens the dialog with `showModal?.()`.

`UNVERIFIED_GAME`'s "Repair it from the Settings page." and `home.js`'s
`settings.html#files` badge link now both land somewhere that can repair.

Covering tests:

- `the_page_every_refusal_points_at_can_repair_the_fault` (guard): the
  refusal still names the Settings page, the badge still links to
  `settings.html#files`, and `settings.js` contains both
  `setup_download_reborn_game` and the Tier 2 headline.
- `Tools/check-shell-pages.mjs`, loading Settings with `hash: "#files"` and
  `game_verdict: "unconverted"`: "Settings offers a repair for a disc that is
  not a Reborn build" and "Settings words that as a fault, never as a
  preference".

```
test tests::the_page_every_refusal_points_at_can_repair_the_fault ... ok
ok    Settings offers a repair for a disc that is not a Reborn build
ok    Settings words that as a fault, never as a preference
```

Demonstration (the `setup_download_reborn_game` call replaced with a no-op):

```
the page every refusal points at still cannot repair anything
```

---

## New guards, and the demonstration that each can fail

Every new source-scanning guard runs its body through `code_only()` (or, in
`reborn-cli`, a local copy of it, since that crate has no test module that
already carried one). Each was run with its target removed and every comment
left intact; each failed. The seven, and their failure messages, are quoted
under their findings above:

| Guard | Crate | Demonstrated failing |
|---|---|---|
| `every_composer_of_the_texture_pack_asks_what_the_image_is` | reborn-ui | yes |
| `nothing_but_a_confirmed_choice_writes_the_unverified_acknowledgment` | reborn-ui | yes |
| `setup_never_adopts_a_disc_that_is_not_a_reborn_build` | reborn-ui | yes |
| `a_patch_that_did_not_run_is_reported_as_a_patch_failure` | reborn-ui | yes |
| `the_page_every_refusal_points_at_can_repair_the_fault` | reborn-ui | yes |
| `the_launch_gate_reuses_the_verdict_instead_of_rehashing_the_whole_disc` | reborn-ui | yes |
| `play_asks_what_the_disc_is_before_it_boots_it` | reborn-cli | yes |

The behavioural pins were demonstrated the same way and are quoted under
Criticals 1 and 2.

One anti-pattern worth recording: a guard that greps a file set that
includes its own source will count its own needle and can never reach zero.
`nothing_but_a_confirmed_choice_writes_the_unverified_acknowledgment` hit
this on first run (it reported `left: 2` with only one real write) and now
assembles its needle with `concat!` so the literal never appears in the test
text. Its doc comment says so.

---

## Verification

Run on the committed tree, after every demonstration was reverted:

```
cargo test -p reborn-ui                      80 passed; 0 failed
cargo test -p reborn-core --features publish 940 passed; 0 failed; 4 ignored
                                             16 passed / 3 passed (integration)
cargo check -p reborn-cli                    Finished, no warnings
cargo test -p reborn-cli                     19 passed; 0 failed
node Tools/check-shell-pages.mjs             PASS: every shell page survives every answer
node --test crates/reborn-ui/ui/outcome-words.test.mjs   3 pass, 0 fail
```

`cargo check` across the three crates with `--tests` emits no warnings.
All touched files were checked for CRLF (`0` occurrences in each).

## Notes and residual risk

- **The author's-folder hole is deliberate and stated.** See the ordering
  section under Critical 1. A player who turns management off and then plays
  an unconverted disc keeps whatever is in `Load/Textures/GN7E69`.
- **`texture_policy_now` fails toward withholding.** On a machine where the
  emulator cannot be resolved and no saved `game_path` exists, toggling a mod
  will remove the composed tree. The next successful launch rebuilds it
  (eight thousand hardlinks, seconds), and the launch could not have started
  in that state anyway. The trade is stated in the function's own comment.
- **`prepare_launch` and first run's `PrepTask::WriteConfig` still pass
  `textures_allowed: true`.** That is not a hole: `Session::prepare` never
  reads the field, and only `Session::run` composes (session.rs:1010). Left
  alone rather than changed, because changing it would be motion without
  effect, and the guard tests read those command bodies.
- **`open_runtime` remains out of scope**, per the spec. Nothing here
  changes that.

---

# Addendum: the re-review's three findings

Commit `ba4e6fd`, on top of `659a384`. Three findings, all confirmed on
inspection. None turned out to be wrong.

## C1 (Critical): the law did not hold via the Online page

### What was there

`crates/reborn-ui/src/dolphin_settings.rs`, `netplay_open`:

```rust
let verified=may_play_before_boot(&p,&game_path)?;
...
if verified {
  if let Err(e)=crate::sync_active_now(&p,runtime.as_deref()) { eprintln!(...); }
}
```

The coordinator's reading is exactly right, and it is worse than it looks
because of a second fact: `netplay_open` never calls `Session::run` (zero
matches in the file). `Session::run` is the only thing that calls
`sync_mods_for_launch`, so the withholding removal that protects Play, the
desktop shortcut and `rebornctl play` has no reach into this command at all.
The `if verified` wrapper was the entire texture decision for online play, and
skip is not remove: `HiresTextures` is a Locked `True` key, the composed pack
is already in the user folder for anybody who has ever booted, and Dolphin
loads it for whatever `GN7E69` disc it is handed.

`textures_allowed: verified` in that command's `LaunchRequest` is inert, which
is the trap: it looks like the decision, and it is read by nothing.
`Session::prepare` never touches the field.

### What I changed

```rust
let chosen_disc_verified=may_play_before_boot(&p,&game_path)?;
let online=match netplay_image(&p){Ok(Some(path))=>Some(path),Ok(None)=>None,Err(e)=>return Err(e)};
let verified=match &online{
  Some(_)=>chosen_disc_verified&&netplay_copy_is_reborns_own(&p),
  None=>chosen_disc_verified,
};
let game_path=online.unwrap_or(game_path);
if let Err(e)=crate::sync_active_judged(&p,runtime.as_deref(),Some(verified)) { eprintln!(...); }
```

- The sync is **unconditional**. `Some(false)` goes through
  `session::texture_policy_for_launch` to `Withhold`, which removes the tree.
- `sync_active_judged` is a new entry point in `main.rs`; `sync_active_now` is
  now a one-line call into it, so there is one body and one rule. Its doc
  comment states plainly that `already_judged` is the answer to the question
  asked about a different file, never a way to skip asking it.

### The secondary: verifying the bytes that actually boot

The finding is real and I fixed it, but **not** by putting the online image
through `may_play_before_boot`, and that is worth the paragraph.

`library/online/NSR.iso` is Reborn's own derived artifact: `online_image.rs`'s
header says it is "the pristine image, plus the installed roster if there is
one, and nothing else", assembled by the Builder. The moment a roster is
applied its bytes are **deliberately** not any node of `chain.json`. So
`identify` would route it to `Unconverted`, and `may_play` would then refuse
online play outright for every league player with a roster installed. That is a
worse answer than the one it replaces, and it would have shipped as a total
outage of the Online page.

What *is* chain-routable is the pristine it was built from, and
`online_image`'s `source.json` already records that hash beside the image
(`Source::pristine_sha256`, which is the pristine's own folder name). So the
question is asked about the provenance:

```rust
fn netplay_copy_is_reborns_own(p:&Platform)->bool{
  ... let source = online_image::built_from(&data) ...
  game_identity::classify(chain.as_ref(), DISC_ID, &source.pristine_sha256).is_reborn()
}
```

No hashing at all. An online copy built from a Reborn build is Reborn's; one
built from the player's own unconverted disc is not, however many rosters were
applied on top of it. A missing or unreadable record answers false, the
fail-safe direction. New `pub fn online_image::built_from` exposes the record,
with the reasoning in its doc comment so the next reader does not reach for
`identify`.

This can only ever make netplay stricter than it was: it is ANDed with the
existing gate and never turns a refusal into an acceptance.

### Covering test

`every_composer_of_the_texture_pack_asks_what_the_image_is`, extended to scan
`netplay_open`, and extended in shape as well as in reach. Presence is not
enough for this one, because the defect was a wrapper, not a missing call. So
the call site is checked by **brace depth**, computed over the `code_only`
stripped body so a brace inside a comment cannot move it: the sync must sit at
the same depth as `may_play_before_boot`, which is the closure's own statement
level. It also asserts `netplay_copy_is_reborns_own` is present and appears
before the sync.

```
test tests::every_composer_of_the_texture_pack_asks_what_the_image_is ... ok
```

Demonstration, wrapping the call in `if verified { ... }` again, comments
intact:

```
assertion `left == right` failed: netplay_open's texture sync sits 1 level(s) deeper
than its gate, so it is inside a conditional again: skipping the sync leaves Reborn's
whole pack on a disc Reborn did not verify, and netplay never calls Session::run to
remove it later
  left: 3
 right: 2
```

Demonstration, dropping the provenance check (`Some(_)=>chosen_disc_verified,`):

```
netplay judges the disc the player chose and never the derived copy it actually boots,
so the bytes handed to the emulator are the one thing nothing asked about
```

---

## I1 (Important): a guard that could not fail, slice-shaped

### What was there

`command_body` stopped only at `\n#[tauri::command]`:

```rust
let to = rest.find("\n#[tauri::command]").unwrap_or(rest.len());
```

`setup_migrate_install` is the last command in `setup_tools.rs`, so there was
no stop marker after it and the slice ran to end of file, taking in
`mod migration_path_tests` and `mod door3_tests`. `door3_tests` names
`may_adopt` twelve more times, so the third assertion of
`setup_never_adopts_a_disc_that_is_not_a_reborn_build` was satisfied by the
test module it had accidentally swallowed.

The coordinator is right that `code_only` cannot see this. `code_only` answers
"is this line a comment?"; the question here is "is this text even inside the
item?".

### What I changed

```rust
let to = ["\n#[tauri::command]", "\n#[cfg(test)]"]
    .iter()
    .filter_map(|marker| rest.find(marker))
    .min()
    .unwrap_or(rest.len());
```

`command_body`'s doc comment now records why the second marker is there. This
only ever shrinks a slice, so it can only ever make an existing assertion
stricter, never looser.

### The mutation demonstration, both ways

Mutation: delete the real gate in `setup_migrate_install`, comments intact.

```rust
- let game=if may_adopt(&inherited){game.clone()}else{return Err(UNVERIFIED_MIGRATED_GAME.to_string())};
+ let game=game.clone();
```

With the fixed stop:

```
test tests::setup_never_adopts_a_disc_that_is_not_a_reborn_build ... FAILED
thread ... panicked at crates\reborn-ui\src\main.rs:6782:9:
the migrate door stopped applying the adoption rule
```

Then, with the SAME mutation still applied, reverting `command_body` to the old
single stop:

```
command_body reverted to the old stop
test tests::setup_never_adopts_a_disc_that_is_not_a_reborn_build ... ok
test result: ok. 1 passed; 0 failed
```

That is the finding proved in both directions: the assertion's ability to fail
came from the stop marker, not from the assertion.

Restored, fix reapplied:

```
test tests::setup_never_adopts_a_disc_that_is_not_a_reborn_build ... ok
```

### Other guards that slice a last-in-file item

I checked every `command_body` call site against the tail of each file it
reads:

- `setup_tools.rs` — the last `#[tauri::command]` is `setup_migrate_install` at
  line 727, followed by `#[cfg(test)] mod migration_path_tests` (850) and
  `mod door3_tests` (879). This was the one. Fixed by the new stop.
- `dolphin_settings.rs` — `netplay_open` (195) is followed by two more commands
  (317, 328), so it was correctly bounded, and the new `netplay_open` guard is
  bounded by the same marker.
- `main.rs` — its own test modules begin at `#[cfg(test)]`, which the new stop
  now also honours for whichever command happens to be last.

Two pre-existing helpers, at `main.rs:6330` and `main.rs:6996`, already used
`rest.find("\n#[cfg(test)]")` as their stop, which is what makes the omission
in `command_body` an oversight rather than a policy.

One further instance of the same *class* of defect turned up while writing the
I2 guard, and is recorded under I2 below: an assertion whose needle survived
the mutation it existed to catch. Both halves now fail on mutation.

---

## I2 (Important): an unreadable disc silently deleted a healthy player's textures

### What I chose, and why

**Kept `Withhold`. Fixed the silence.** I did not take the offered alternative
of returning `LeaveAlone` for `Unreadable` on the compose path.

The argument for `LeaveAlone` is that "I could not ask" is not "this is not
ours", and that is true as a statement about knowledge. It is not true as a
statement about risk. `Unreadable` has two causes and they point opposite ways:

- The **disc** will not read. Then Dolphin cannot boot it either, so it can
  never wear Reborn's faces, and either policy is harmless.
- **DolphinTool** is missing, blocked or quarantined. Then the disc may be
  perfectly bootable and perfectly foreign, and `LeaveAlone` hands it Reborn's
  entire pack. That is Critical 1, reopened in precisely the situation where
  the launcher knows least.

`Verdict::Unreadable { reason }` does not distinguish the two, and inferring it
from the reason string would be a guess about a message from another program.
So the policy stays fail-safe, and the fix goes at the actual defect, which was
never the direction: it was that a destructive outcome reported itself as a
clean success.

### What I changed

`crates/reborn-ui/src/main.rs`

- New `struct TextureDecision { policy, withheld_because }`. The reason travels
  with the decision, because only the decision knows it.
- Two named player sentences:
  - `TEXTURES_WITHHELD_NOT_REBORN` — "Your game file is not NFL Street Reborn,
    so Reborn's texture pack was removed from it. Repair your game on the
    Settings page to put the pack back."
  - `TEXTURES_WITHHELD_UNCHECKED` — "Reborn could not check your game file, so
    its texture pack was removed until it can. Repair your game on the Settings
    page, or reinstall the launcher if this keeps happening."

  Two sentences, not one, because the two cases send the player to completely
  different repairs. `texture_policy_now` picks between them by verdict.
- `ModSyncView` gained `withheld_note`, set only when the tree really was
  withheld. Not a `refusal`: a refusal means nothing changed, and here a great
  deal did.
- The author's-folder case reports nothing, because nothing was taken away.
  Saying "your pack was removed" there would be false.

`crates/reborn-ui/ui/mods.js`

- `if(r?.withheld_note)notice=r.withheld_note;` on the sync path, and the same
  on the `controls_set` path, where the view arrives nested under `synced`.

The launch path was already not silent: `SyncReport::explain` says "texture
folder removed, because this game file is not an NFL Street Reborn build", and
that reaches the Play page as `mods_note`. The Mods page was the hole.

### Covering tests

- `a_withheld_texture_pack_always_says_why` — behavioural, over
  `TextureDecision`: withheld carries its sentence, managed and author-owned
  carry none, the two sentences differ, each names a way out, neither says
  "custom", neither contains an em-dash.
- `the_reason_a_pack_was_removed_reaches_the_mods_page` — guard, both halves.

```
test tests::a_withheld_texture_pack_always_says_why ... ok
test tests::the_reason_a_pack_was_removed_reaches_the_mods_page ... ok
```

Demonstration, `mods.js` stops reading the field (both readers removed,
comments intact):

```
the Mods page never reads the reason, so a player who just lost their HD textures
sees nothing at all
```

Demonstration, `sync_view` discards the reason (`withheld_note: None`):

```
sync_view is handed the reason the pack went and then never uses it
```

**A second guard-that-could-not-fail, caught while writing this one.** The
first version of that second assertion was `view.contains("withheld_note")`.
The mutation `withheld_note: None` still contains the field name, so the guard
passed while the reason was thrown away, which is exactly the defect it existed
to catch. Changing the needle to the parameter name did not help either:
`withheld_because` appears in `sync_view`'s own signature whether or not the
body uses it. The assertion now **counts**:
`view.matches("withheld_because").count() >= 2`, which is the signature plus at
least one real use, and the comment above it records both failed attempts. Only
after that change did the mutation fail, as quoted above.

---

## Verification of the addendum

```
cargo test -p reborn-ui                      82 passed; 0 failed
cargo test -p reborn-core --features publish 940 passed; 0 failed; 4 ignored
                                             16 passed / 3 passed (integration)
cargo test -p reborn-cli                     19 passed; 0 failed
cargo check -p reborn-cli                    Finished
node Tools/check-shell-pages.mjs             PASS: every shell page survives every answer
node --test .../outcome-words.test.mjs       3 pass, 0 fail
```

`cargo check --tests` across all three crates: no warnings. No CRLF in any
touched file.

## Residual note

`netplay_open`'s `LaunchRequest` still carries `textures_allowed: verified`,
and that field is read by nothing on this path: `Session::prepare` ignores it
and `Session::run` is never called here. It is left set correctly rather than
removed, because a field left stale is a trap for the next reader, but the
thing that actually governs netplay's textures is now, and only, the
unconditional `sync_active_judged` call.

---

# Addendum 2: the harness regression the fix wave introduced

Commit `0bed99d`.

## What was wrong

`659a384` added `acknowledge_unverified: Option<bool>` to `settings_set_path`
and never updated `crates/reborn-ui/ui/app.js`, the developer harness. That
file has not been touched on this branch at all, so nothing in my own
verification set covered it: `node Tools/check-harness-args.mjs` is not one of
the six commands the task named, and I did not run it.

```
FAIL
  settings_set_path: does not send acknowledgeUnverified
```

This is a real click-time fault, not a lint. `check-harness-args.mjs` says why
in its own comment: a Tauri command's arguments are matched by NAME through
serde, and the key has to be present even for the argument to be read as
absent. A row missing a key gets a deserialization error on click, and nothing
before the click says so, because "the JavaScript parses, the Rust compiles,
and the two are only introduced to each other by a string". `home.js:91`
records the same error shape happening for real once before
("command launch missing required key modsDisabled").

## What I changed

One row in `app.js`:

```js
cmd: "settings_set_path",
...
args: () => ({ acknowledgeUnverified: false }),
inputs: [ { key: "which", ... }, { key: "value", ... } ],
```

Pinned in `args()` rather than added as an input box, which is the convention
the neighbouring `launch` row already uses for its own `Option<bool>`
(`leagueMatch: false`). The harness's other boolean rows use
`{ bool: true }` input boxes, but every one of those is a plain `bool` the
developer is meant to vary; this one is not.

The value is `false` on purpose, and the comment in the file says so: `true`
is the player pressing "Play it anyway" on a disc that is not a Reborn build,
and that choice belongs to the confirmed dialog on the Settings page, never to
a text box in a developer tool. `false` and absent behave identically in the
command, which tests `acknowledge_unverified == Some(true)`, so the row now
behaves exactly like every caller that is not the dialog: an unconverted image
comes back as the choice to make, and nothing is written.

Developer-facing only. No player-facing behaviour changed.

## The whole set, run once more on the committed tree

Nine commands, all run for real. **None of them needs `playwright`**, so there
is no skipped or degraded result to distinguish: the twenty-six Tools scripts
that do import playwright are all outside this set.

| Command | Result |
|---|---|
| `node Tools/check-harness-args.mjs` | PASS: all 75 rows send exactly what their command declares (exit 0) |
| `cargo test -p reborn-ui` | ok. 82 passed; 0 failed |
| `cargo test -p reborn-core --features publish` | ok. 940 passed; 0 failed; 4 ignored, plus 16 and 3 in the integration binaries |
| `cargo test -p reborn-cli` | ok. 19 passed; 0 failed |
| `cargo check -p reborn-cli` | Finished, no warnings |
| `node Tools/check-shell-pages.mjs` | PASS: every shell page survives every answer (exit 0) |
| `node --test crates/reborn-ui/ui/outcome-words.test.mjs` | 3 pass, 0 fail |
| `node Tools/check-mods-page.mjs` | PASS: mod store preserves core actions and failure states (exit 0) |
| `node Tools/check-page-call-args.mjs` | PASS: every page sends what its command declares (exit 0) |
| `node Tools/check-onboarding-ready.mjs` | PASS: a selection now enables Next (exit 0) |

`check-page-call-args.mjs` is worth naming separately: it is the same check as
`check-harness-args.mjs` but for the real pages rather than the harness, and it
passed throughout, which is why `settings.js`'s new `acknowledgeUnverified`
call site and `mods.js`'s new field read were never in question. The gap was
only ever the harness.

## What I would take from this

My verification set was the six commands the task named, and I treated that
list as the definition of "green". `app.js` is a caller of every command in the
window, so any signature change reaches it, and `Tools/` holds a checker that
says so in one second. Adding a parameter to a `#[tauri::command]` should pull
`check-harness-args.mjs` into the run by reflex, the same way touching a page
pulls in `check-shell-pages.mjs`.
