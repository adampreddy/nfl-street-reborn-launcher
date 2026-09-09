# SDD ledger — plan: docs/superpowers/plans/2026-09-08-publisher-1-core-publish.md

Spec: docs/superpowers/specs/2026-09-08-reborn-publisher-design.md (read; binding authority).
Worktree: target/ship-tree, branch publisher-core from main 451a674.
Source being moved: crates/reborn-cli/src/publish.rs at c4504de (line numbers in the plan refer to it).

## Pre-flight scan (2026-09-08 evening)

| Pair / task | Produces vs consumes | Found |
|---|---|---|
| T1 -> T2..T6 | Sink, Outcome, refuse, out!/err!, io_err, thousands, manifest_path/url, next_sequence(&Path, ComponentId) | names and signatures agree in every consumer |
| T2 -> T4 | keys::read_key(&Path) -> Result<Option<[u8;32]>> | release.rs imports it by that path |
| T3 -> T4 tests, T5, T6 | ComponentRequest{root,spec,origin,apply}, Published, publish_component(&req,&dyn ScanObserver,Sink) -> Result<Outcome<Published>> | agree |
| T4 -> T5, T6 | CarryForward{origin,id,version,key,release_version,release_sequence,apply}, republish_pinning -> Result<Outcome<Signed>>; Described for no-key/no-release | T5/T6 map Described to Push{release:None}; agree |
| T5 -> T6 | roster::Push{component,release} | game_patch.rs imports it |
| T1..T6 -> T7 | every pub path listed in T7 Interfaces | agree; T7 uses super::ScanLog (private in main.rs, visible to a child module) |
| T7 -> e2e | asserted substrings listed in Global Constraints | every one is a verbatim println moved to out! |
| T1 self | out!/err! are #[macro_export] + pub use crate::{err,out} | valid on edition 2021; usable as crate::out! before definition order |
| T2 self | unused `collect` helper in tests | Ruling: delete it (the brief says so) — cosmetic — costs nothing |
| T3 self | Sha256Hex may not be Clone | plan gives the fallback (derive Clone, or store String); manifest.rs is uncommitted in MAIN only; the worktree copy is clean |
| T4 self | test uses `CarryForward { key: None, ..no_release }` after using no_release by reference | struct update moves the rest; no_release unused afterwards; fine |
| T5 self | FakeSite implements Http; the trait may have more required methods | brief tells the implementer to implement them with unimplemented!() |
| T6 self | pre-xdelta refusals only; the encode path is proven by e2e-game-patch in T7 | matches the plan's stated scope |
| T8 step 4 | merge into main (side effect outside the worktree) | Ruling: the implementer does NOT run the merge; the controller does it in finishing-a-development-branch with Adam's choice — a merge is a shared-branch side effect — costs one extra step at the end |

## Rulings
- Ruling: T8 step 4 (merge into main) is deferred to the finishing step — norms say ask before a merge — cost if wrong: a few minutes.

## Progress
- Task 1: implementer DONE at f14c7f3 (haiku, 5 tests); review dispatched (sonnet) on package review-451a674..f14c7f3.diff
- Task 1: minor (deferred): Outcome::is_done/refused unused within T1 (used by T2+ tests per plan)
- Task 1: minor (deferred): next_sequence skips an unparseable manifest silently (same as the CLI it replaces)
- Task 1: complete (commits 451a674..f14c7f3, review clean)
- Task 2: implementer DONE at 4e9f6bc (haiku, 4 tests); review dispatched (sonnet) on package review-f14c7f3..4e9f6bc.diff
- Task 2: review clean; warning check resolved by the controller (cargo build -p reborn-core --features publish, see count in session)
- Task 2: minor (deferred): the CLI still carries its own keygen/read_key until Task 7 rewires it (by design)
- Task 2: complete (commits f14c7f3..4e9f6bc, review clean)
- Task 3: implementer DONE at 9584c12 (sonnet, 6 tests, 619 core tests green, no warnings); review dispatched (sonnet) on package review-4e9f6bc..9584c12.diff
- Task 3: minor (deferred): local `manifest_path` shadows the imported fn (inherited from the CLI); Published derives Clone, ComponentRequest derives Debug (harmless extras)
- Task 3: complete (commits 4e9f6bc..9584c12, review clean)
- Controller model switched to Opus 5 mid-plan (Adam, during Task 4). Commit trailers from Task 5 on: Co-Authored-By: Claude Opus 5. Tasks 1-4 carry the Fable 5.1 trailer; left as they are, since a commit records who wrote it.
- Task 4: first implementer killed by the model switch after writing release.rs (475 lines, uncommitted, its last message claimed 626 tests green); harness refused to resume it. Ruling: dispatch a FRESH implementer to verify the on-disk file against the brief line by line, fix what is wrong, then commit and report - Adam said "keep going", and the alternative (discarding a tested file) costs a rewrite for nothing - cost if wrong: the fresh implementer rubber-stamps a bad file, which the task review then catches.
- Task 4: fresh implementer verified the file unchanged and committed 83b191c (7 tests, 626 core green, no warnings); review dispatched (sonnet) on package review-9584c12..83b191c.diff. NOTE for the reviewer: the RED step's output is unavailable (the first implementer was killed before capturing it).
- Task 4: minor (deferred): republish_pinning sorts the Vec<String> and the Vec<Pin> independently by a recomputed "id=version" key (release.rs:301); a `label(id, version)` helper would remove the duplication and the implicit invariant. Not a defect today (keys equal by construction, sort is stable).
- Task 4: complete (commits 9584c12..83b191c, review clean; the reviewer diffed every moved literal against the CLI programmatically)
- Task 5: implementer DONE at 22af287 (sonnet, 4 roster tests, 630 core green, no warnings). DEVIATION reported: the staging dir is now unique per call (pid + atomic counter) instead of the CLI's version-keyed path, to fix a race the brief's three same-version tests hit under parallel cargo test. Review dispatched (sonnet) with that deviation flagged as the load-bearing check.
- Task 5: review "Needs fixes" - Important: the unique staging dir is never removed, so publish_roster leaks a temp dir per call (the CLI's version-keyed dir was self-bounding). Minors deferred: dead `if stage.exists()` guard (folded into the fix), CarryForward built twice (roster.rs:148 and :165).
- Task 5: fix round 1/5 dispatched (implementer resumed) - fix base 22af287
- Task 5: fix round 1/5 (2 addressed, 0 open: StageGuard Drop covers every return, refusal, ? and panic path; dead guard removed; commits 22af287..1e66853)
- Task 5: minor (deferred): CarryForward built twice in roster.rs (the Described arm and after the match); a single hoisted binding would do
- Task 5: complete (commits 83b191c..1e66853, review clean after 1 fix round)
- Task 6: implementer DONE at 16998e1 (sonnet, 5 tests, 636 core green, no warnings). Two declared deviations: (a) the brief's test code did not compile (E0716, temporaries dropped before use) so each was bound to a local first; (b) a CheckGuard Drop guard added, since the decode-verification check file could survive an early ? from Patcher::decode in the original CLI code. Review dispatched with both as load-bearing checks.
- Task 6: review found Important (plan-mandated): two refusals reachable WITHOUT xdelta3 are untested - "xdelta3 was not found" and "already in the chain". The brief's Step 1 test list omitted them while claiming to cover every pre-xdelta refusal.
- Task 6: Ruling: the finding stands, the plan text is what is wrong. Spec section 8 wants the refusals covered; both are constructible with fake files and cost one fix round. Cost if wrong: two extra tests nobody needed.
- Task 6: Ruling: also fold in an explicit drop of CheckGuard right after hash_file, restoring the CLI's delete-immediately-after-hash timing while keeping the leak fix. Reviewer noted the guard otherwise holds a decoded ISO (~1.4 GB) on disk through the publish and signing steps. Cost if wrong: none, the guard still runs on every path.
- Task 6: fix round 1/5 dispatched (implementer resumed) - fix base 16998e1
- Task 6: fix round 1/5 (2 addressed, 0 open: both new tests reach their real branch in the right order; drop(check_guard) sits right after the hash, guard still covers the decode window; commits 16998e1..e8ff129)
- Task 6: minor (deferred): CarryForward built twice here too, mirroring roster.rs; one shared helper could serve both files
- Task 6: complete (commits 1e66853..e8ff129, review clean after 1 fix round)
- Task 7: implementer DONE at 4c6bd7a (sonnet). ALL THREE e2e scripts PASS unchanged (update 13 steps, roster 3, game-patch 7), cargo test --workspace 673 passed, test-all -NoRust 19/19, no warnings. NO core line needed fixing: every moved println survived byte for byte through tasks 1-6. publish.rs went 1153 -> 385 lines. One deviation: three now-dead private consts deleted from the kept header (COMPONENTS_DIR/PUBLISHER_KEY/ROTATION_KEY) to satisfy the no-warnings rule. Transient: e2e-roster hit "No space left on device" once (another session filling the disk), retried clean, nothing edited to work around it.
- Task 7: review dispatched (opus, this is the plan's load-bearing task)
- Task 7: minor (deferred, plan-mandated): release pin parsing now runs BEFORE the key read, so a malformed --component prints no Origin/Release/Signing key preamble and exits 2 instead of 1. Intentional (parse failures stay in the CLI at exit 2); carry to the whole-branch review as a known error-path output difference.
- Task 7: minor (deferred, plan-mandated): the roster token is now resolved unconditionally, so --token-file <missing> errors even for a local --from, and the vault is read on every roster publish rather than only URL fetches. Forced by RosterRequest taking a pre-resolved Option<&str>.
- Task 7: minor (deferred): the `use` block sits mid-file after the enum (a consequence of "replace everything from line 210"); Error::Io inlined twice where io_err used to be; cargo fmt would rewrite 9 lines of the brief's verbatim text (no fmt gate exists).
- Task 7: minor (deferred): the report asserts the post-rewire e2e-update output is identical to its pasted baseline rather than pasting it again; roster and game-patch are pasted in full.
- Task 7: complete (commits e8ff129..4c6bd7a, review clean; reviewer verified streams, exit codes, token path and the constant deletion at the source, not just through the scripts)
- Task 8: review "Needs fixes". Critical: the NEXT_STEPS bullet landed under section "4", not "4c" (my doc has 4c physically ABOVE 4, which is what confused it). Important: "the Reborn Publisher app calls the same functions" is present tense about a crate that does not exist; that sentence came from MY brief.
- Task 8: Ruling: both findings stand. The overclaim is mine, not the implementer's, and a runbook that states a false present-tense fact is worse than no sentence. Fix: put the bullet inside 4c, and change the sentence to say the app WILL call them (plans 2 to 4). Cost if wrong: none, it is two lines of prose.
- Task 8: fix round 1/5 dispatched (implementer resumed) - fix base 31625a1
- Task 8: fix round 1/5 (2 addressed, 0 open; commits 31625a1..3da50ac)
- Task 8: complete (commits 4c6bd7a..3da50ac, review clean after 1 fix round)
- ALL 8 TASKS COMPLETE. Whole-branch review next over 451a674..3da50ac (11 commits).

## Final whole-branch review (opus): APPROVE FOR MERGE
Verified mechanically: 157 of 160 original println format strings byte-identical, the 3 differences are variable renames rendering the same characters; exactly 2 new production strings, both on paths the CLI could not reach. Channel discipline, exit-code mapping across all 4 outcome seams, and secret hygiene all confirmed at the source.
- Important #1: RosterRequest derives Debug while holding the site token. Not a live leak (nothing formats it) but plan 2/3 will, and spec 3.5 forbids it.
- Ruling: fix #1 plus the cheap API-shape and test items in ONE fix wave before merge; they get harder once plan 3 depends on the shapes. Cost if wrong: one extra commit.
- Ruling: DEFER Minor #11 (next_sequence skips an unparseable manifest, so a sequence can collide and a publish silently no-ops on clients) to PLAN 2, where next_sequence already grows a bucket half. It is pre-existing and a pure refactor is the wrong place. ⛔ PLAN 2 MUST refuse on a parse failure there.
- Ruling: DEFER Minor #7 and #9 (blob-copy progress only as text, no cancellation past hashing) to PLAN 3's design, recorded now because the request structs freeze here.
- Ruling: Minor #3, #4 stand as the two documented byte-for-byte exceptions, both improvements. #10, #12, and the fmt item stand as style.
- Final fix wave: all 7 items ADDRESSED, no printed byte changed (item 3's split re-derived independently: concatenation is associative, so 4 println lines equal 1 println of 3 embedded newlines for any stdout/stderr content). Commit d69d8b5.
- Out-of-scope note from the re-reviewer for PLAN 2: roster.rs:124 propagates a non-401 Http error as a raw Err to the CLI's top-level handler rather than through refuse(); if ureq's error Display ever included request headers, the bearer token would escape the sink. Not verified against the pinned ureq version. Pre-existing.
