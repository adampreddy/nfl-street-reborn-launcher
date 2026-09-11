# Publisher redesign: one button per job (Track B ledger, 2026-09-11)

Branch `publisher-redesign`, worktree `target\publisher-tree` (its `target`
is a junction to `target\publisher-target`). Design: Adam chose "One
button per job" on 2026-09-10 night; the approved shape is in
`docs/superpowers/plans/2026-09-10-night-fixes.md`, Track B.

## What changed

- `crates/reborn-publisher/ui/job.js` replaces `panel.js` on every page.
  One primary button ("Publish textures 1.2.1"); the check runs (the
  page's plan command, writes nothing); a card says in plain words what
  changes for players with Cancel / Yes; Yes runs for real (dry run off)
  with a bar while bytes move; verify runs on its own; "Done. Players now
  get X. Verified from the server at HH:MM." A refusal or failure is one
  red sentence with Copy; the raw log sits behind a closed Details.
- A page draws ONLY states whose `op_status.operation` is its own.
  Another page's running job: one line "X is publishing. Open X to watch."
  and the button disabled. Another page's finished job: ignored. (The old
  panel showed whatever was done last on every page.)
- Core: `publish::Progress { done, total, label }` on `Event`;
  `upload::upload` attaches it to the lines it already prints; `ops.rs`
  keeps the last one in `OpState.progress`, cleared when a step starts.
- Sidebar: Home, Textures runtime mods, Launcher update, Roster, Roll
  back, then Advanced: Build an ISO, Full installer set, Keys and
  settings. Every page: one-sentence purpose plus a closed "How this
  works". Words: server, this computer, files; release numbers, hashes,
  manifests only inside Details.
- Home: "What players get right now" (one version per thing, launcher and
  content release kept apart, signature as one sentence), "Not yet on the
  server" with Upload now, "This computer" (only what is missing), Recent.
- Installer: `{userappdata}\NFL Street Reborn Launcher\publisher\app`.

## Proof

- `cargo test -p reborn-core --features publish -- publish::` 105 passed
  (new: `progress_rides_on_the_lines_from_zero_to_the_total_through_every_part`).
- `cargo test -p reborn-publisher` 44 passed (new:
  `a_progress_event_lands_in_op_status_and_the_next_step_clears_it`).
- `node Tools/check-publisher-pages.mjs` 70 ok (three modes per page plus
  the job contract on Roster with a scripted op_status).
- `node Tools/review-publisher.mjs` PASS in a real browser: card only after
  the check, no run from the check, bar at 50% = 480 of 962 px, refusal
  with Copy, failure log behind Details, another page busy / finished,
  Build anyway inside 1280x800 at 112 warnings. Screenshots in
  `target/publisher-shots/`.
- `py -3 Tools/e2e-publisher.py` 13 steps PASS.
- Real window: see the report (drive-publisher-content.mjs,
  drive-publisher-upload.mjs).

## Laws learned

- `[hidden]` needs `display: none !important` once any class sets
  `display`; a hidden flex card drew as an empty box.
- A fake DOM's `<select>` has no default value: set `.value` outright
  when a button's text is built from it.
- ⛔ `Tools/e2e-publisher.py` (like e2e-upload.py) stores the FAKE R2
  credentials into the real vault entries and never restores them. After
  running it: `rebornctl publish token --r2-access-from-file` /
  `--r2-secret-from-file` from temp files cut out of rclone.conf (never
  print the values), then prove with `rebornctl publish upload --origin
  "C:\Reborn Origin"` (no --apply): "Listed ... in one pass".
