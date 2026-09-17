# Roster false-positive: rostered players told "Your game file isn't NFL Street Reborn"

Branch `tool-lookup`, built on HEAD `8101666`.

## The defect

`crates/reborn-ui/src/main.rs`'s launch gate (`may_play_before_boot` / `verdict_now`)
and `launch_check`'s Play-page report both judged the **booted** image
(`library/offline/NSR.iso`) with `reborn_core::game_identity::identify`, which
hashes the file's own bytes and asks the game-patch chain about them.

Per `game_image.rs`'s own module header, the booted image "becomes a real file
the first time a roster is applied, because the Builder writes a new file and
that file is renamed over the link." Those bytes are, by design, never a node
of the patch chain. So every player who applied a league roster got
`Verdict::Unconverted` at every Play press: refused at Play with "Your game
file isn't NFL Street Reborn", and their HD texture pack withheld/deleted by
the texture-sync policy that treats `Unconverted` as `Withhold`.

`crates/reborn-cli/src/main.rs`'s `may_play_before_boot` (the console
equivalent, `rebornctl play`) had the exact same bug: it also called raw
`identify` on the booted image.

## The fix

Added `reborn_core::game_identity::identify_booted(data_root, tool, booted) ->
Verdict` in `crates/reborn-core/src/game_identity.rs`, mirroring the
provenance-based fix already proven for the online image
(`online_image::built_from`/`wanted`, consumed by
`dolphin_settings::netplay_copy_is_reborns_own`).

Order, exactly as specified:
1. Game ID via `read_game_id` (unreadable/wrong game short-circuits as before).
2. The booted image's own hash against the chain (`classify`). Anything other
   than `Unconverted` (a genuine `RebornBuild`, `Convertible`, or
   `DifferentGame`) returns immediately — an install with no roster yet is
   unaffected, byte for byte.
3. Only if that is `Unconverted`: fall back to `game_image::find_pristine`'s
   `folder_sha256` (the hash the importer verified, trusted by name, never
   rehashed) against the chain. Only a genuine `is_reborn()` result from this
   step is used; a pristine that is merely the chain's unmodified root does
   not get promoted to "Reborn's own" through this path.
4. Otherwise `Unconverted`, same as before.

The classification logic lives in a private `booted_verdict(data_root,
game_id, booted)` so it is unit-testable without a working DolphinTool to
spawn (the same split `identify`/`classify` already uses in this file).
`identify_booted` is the thin wrapper that reads the game ID and delegates.

Comment on the module documents the accepted tradeoff verbatim: this is
weaker than hashing, because someone who replaces `library/offline/NSR.iso`
in place keeps a "Reborn" answer once the own-hash check has failed. This
mirrors the hole already accepted in the online-image fix, and is not closed
here.

### Call sites changed

- `crates/reborn-ui/src/main.rs`: `verdict_now` (both branches) and
  `launch_check`'s inline verdict computation (both branches) now call
  `identify_booted` instead of `identify`. Both judge `settings.game_path`,
  i.e. the actual booted image. `may_play_before_boot`, `launch`, `play_now`,
  and `dolphin_settings::netplay_open`'s `chosen_disc_verified` check all
  route through `verdict_now`, so they are fixed transitively.
- `crates/reborn-cli/src/main.rs`: `may_play_before_boot` (used by
  `rebornctl play`) now calls `identify_booted`. This was not named in the
  task, but its own doc comment describes it as "the window's
  `may_play_before_boot`, transcribed onto the paths a console build has,"
  and it had the identical bug against the identical file
  (`iso`/`game_path`). Left unfixed, `rebornctl play` would still refuse
  every rostered league player from the command line. No guard test in
  `reborn-cli` scans for the literal string `game_identity::identify` inside
  this function, so nothing needed to change on the test side.

### Call sites deliberately left alone

Every other `game_identity::identify` call site judges an image the player
just browsed to, or one that has not yet had a roster applied to it, i.e. a
*candidate*, not the established booted layer — a raw hash is exactly right
there:

- `crates/reborn-ui/src/main.rs`: `onboarding_confirm_game` (the wizard's
  final Continue-button check on whatever image the wizard landed on),
  `settings_set_path` (the Settings page's check of whatever path the player
  just typed or browsed to, before it is ever stored as `game_path`).
- `crates/reborn-ui/src/setup_tools.rs`: `setup_import_game`,
  `setup_import_existing_game`, `setup_migrate_install`,
  `setup_download_reborn_game` — every door onto `settings.game_path` during
  setup/import/recovery. These are exercised by
  `every_writer_of_game_path_verifies_the_image`, which asserts each of them
  still calls `game_identity::identify` literally; none were touched.

## Tests

Added to `crates/reborn-core/src/game_identity.rs` (`booted_verdict` /
`identify_booted`), all against real files and a real chain built around a
genuine SHA-256 (not the mnemonic stand-ins `classify`'s own tests use,
because this logic actually hashes files on disk):

- `a_rostered_booted_image_is_a_reborn_build_when_its_pristine_is_a_chain_node`
  — **the reproducing test.** A booted file whose own bytes are not a chain
  node, but whose pristine folder is. Before the fix: **RED**, confirmed by
  temporarily disabling the fallback and running
  `cargo test -p reborn-core --features publish game_identity`:

  ```
  thread '...a_rostered_booted_image_is_a_reborn_build_when_its_pristine_is_a_chain_node' panicked at
  crates\reborn-core\src\game_identity.rs:341:9:
  a rostered booted image whose pristine is a Reborn build must read as Reborn's own, not Unconverted
  test result: FAILED. 11 passed; 1 failed
  ```

  After restoring the fallback: GREEN.
- `a_booted_image_whose_own_hash_is_the_chain_head_stays_a_reborn_build` — the
  no-roster case must not regress: own hash alone, no pristine folder at all.
- `a_booted_image_with_no_reborn_provenance_is_unconverted` — neither the
  booted bytes nor the pristine are chain nodes: original protection holds.
- `a_booted_image_with_no_pristine_at_all_is_unconverted` — no pristine at
  all: never a Reborn build.
- `identify_booted_with_a_missing_tool_is_unreadable` — the wrapper's own
  tool-spawn failure path, mirroring `identify`'s existing test.

## Verification

- `cargo test -p reborn-core --features publish` — 969 passed, 0 failed (12
  in `game_identity`, up from 7).
- `cargo test -p reborn-ui` — 86 passed, 0 failed, including the guard tests
  `the_launch_gate_reuses_the_verdict_instead_of_rehashing_the_whole_disc` and
  `the_play_page_can_tell_the_player_their_game_is_not_reborn` (the latter's
  substring check on `"game_identity::identify"` still passes because that
  string is a literal prefix of `"game_identity::identify_booted"`, and the
  guard remains capable of failing: it fails if the call is removed
  entirely).
- `cargo test -p reborn-cli` — 19 passed, 0 failed, including
  `play_asks_what_the_disc_is_before_it_boots_it`.
- `cargo check -p reborn-cli` — clean.
- `node Tools/check-shell-pages.mjs` — PASS (every shell page survives every
  answer).
- `node Tools/check-harness-args.mjs` — PASS (all 75 rows send exactly what
  their command declares).
- All three touched files confirmed LF-only (`\r\n` count = 0) both before
  and after editing.

## Files changed

- `crates/reborn-core/src/game_identity.rs` — new `identify_booted` +
  `booted_verdict` + 5 tests.
- `crates/reborn-ui/src/main.rs` — `verdict_now` and `launch_check` call
  `identify_booted`.
- `crates/reborn-cli/src/main.rs` — `may_play_before_boot` calls
  `identify_booted`.
