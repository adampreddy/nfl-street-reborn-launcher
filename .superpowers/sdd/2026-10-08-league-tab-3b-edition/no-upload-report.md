# League no-upload gate (ruling 10-10, option A)

Only league FIXTURE games upload. League offline and league friendlies keep their result local.

Mechanism
- `SessionIntent.league_edition` (serde default, skipped when false) is set where a session begins: session.rs (`req.build == League`) and the netplay path in dolphin_settings.rs. The recorded build stays `Offline` for League (wire format unchanged), so the intent is the only place this is known.
- `SessionFacts.league_edition` is copied from the intent by `session_facts`.
- `BackendSubmitter::submit`: if the session is league edition and not (`league_match` with a fixture id), it returns the new `SubmitOutcome::Skipped` before any envelope or HTTP post.
- `StatsQueue::submit_pending` handles `Skipped`: entry becomes `Quarantined { NotUploaded }` (no ledger row, no attempt counted, capture bytes kept, never pending, never retried). `SubmitReport.skipped` lists them.
- Local display: captures are archived at ingest, before submit, so the player's own Stats page is unaffected. The results row shows a player-safe sentence.

Tests (RED confirmed by compile failure first): offline and friendly never post (FakeHttp saw zero posts), fixture game posts with intent.league + fixture_id, retail and no-facts unchanged, intent round trip and old-record default, skipped entry kept and not retried.

Results: reborn-core 1184 passed, reborn-ui all passed, cargo check --workspace --tests clean.

Concern: a league room that lost its fixture tag (non matchmade) is treated as a friendly by design. Entries already queued from earlier league sessions have no `league_edition` flag, so they upload as before.

## Review fixes (second commit)
1. Skipped games are now `EntryState::NotUploaded { reason }` (terminal, not Quarantined): pruned by `prune_terminal` after keep_for_ms, not counted as quarantined in `queue_counts` (UI label "local"), `queue.quarantined()` excludes them. Trace: the Stats page reads the archive (`StatsQueue::archive`, written in `ingest_all` before submit, dedupe by content), not the queue payload, so pruning the payload does not touch local stats.
2. `rebornctl stats status` uses `queue.quarantined()` and the exit code, so NotUploaded is excluded by construction. `StatsIngestView.quarantined` counts enqueue-time `Enqueued::Quarantined` (validation/no-session) only, which a skip never produces.
3. `live_plays::keeps_results_local` (league edition, no fixture) makes `match_ctx` None, so the live feed never streams a league friendly. `dolphin_settings::session_match_for` stamps `match_id` only for a matchmade session (rng_seed) that may upload, so the exit/series report (`exit_report_applies` with None) skips league friendlies and Direct/private games.
4. Tests: the facts loop now includes league_edition+league_match+no fixture; end-to-end queue + BackendSubmitter test (zero posts, final state NotUploaded, fixture session accepted); source scans that session.rs and netplay_open set league_edition; session_match_for and match_ctx tests.
Results: reborn-core 1186 passed, reborn-ui 188 passed, check --workspace --tests clean.

## Live snapshot bearer (third commit)
`live_view::snapshot` takes `token: Option<&str>`; a non-blank token goes as `Authorization: Bearer` (via `get_text_as`), none or blank keeps the plain public `get_text`. The `live_match` command reads the token with `identity::token` (the helper the mm and stats calls use). Tests: bearer present when signed in, no authenticated request when signed out or blank, retail works either way, and a source scan that neither the core nor the UI snapshot path logs or prints the token. reborn-core 1188, reborn-ui 188 passed, check --workspace --tests clean.
