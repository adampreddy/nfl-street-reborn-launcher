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
