# Note for the Online-page redesign session (from the series work, feat/series)

The series plan (2026-10-02, Task 20) left `drawMatch` and `drawIdle` alone so the redesign can rewrite them without a merge
collision. Please carry these into the rewrite:

1. **Casual is open-ended now.** Every game in the room counts until the room closes, and the player with more wins takes
   the series. Any text that says "Casual · one game" (redesign spec §3.6) or "Casual is one game" (the idle tile) is
   now wrong. Suggested wording: "Casual · every game in the room counts" and "Casual: every game in the room counts,
   and it never touches your rating."
2. **Ranked is best of 3.** The launcher closes Dolphin itself once a game decides the series (2-0 or 2-1), so the
   player lands back on the Online page.
3. **The series tally for display is `match.series_live`** (`[seat0, seat1]`, from the live feed, unofficial). `match.series`
   only counts linked, accepted results and lags. Show a series line for ranked, and for casual once a game is decided.
4. **The series-over screen.** `draw()` has one new branch (after the room and queue branches, before idle): when
   `match.relay_state === 'complete'` and `seriesOverModel(match, null)` is not null, it renders
   `drawSeriesOver(seriesOverModel(match, liveSnapshot), { el, onFull, onFind, onRematch })` from
   `crates/reborn-ui/ui/series-over.js`. **`seriesOverModel` is the model to render** if you restyle the screen:
   `{ kind, title, headline, note, you, them, games: [{ n, mine, theirs }], unofficial, fullUrl }`. The module is pure
   (no imports), and `series-over.test.mjs` pins it.
5. **Classes `drawSeriesOver` relies on** (all existing, zero CSS added): `mm-state` (headline), `note` (Unofficial tag,
   forfeit note, each game's score), `mm-series` (the "you vs them" line), `mm-actions` (the button row), `primary`
   (Find another game), inside the existing `section.mm`. Plainly: `mm-state`, `mm-series` and `mm-actions` are class
   names the screen sets, but they have NO dedicated CSS today - the screen is styled only by the generic cascade
   (`section.mm` headings, `p`, `.note`, `button`, `.primary`). The redesign is free to style them.
   Please keep these classes, or tell the series session so `drawSeriesOver` can take a class map instead.
6. The snapshot's `match` now also carries `outcome` (`{ kind, winner_seat, official, left_seat }`; kind is
   open | win | forfeit | split | held | abandoned) and `rules` (the series rules the match started under). Both are null
   from an older server.
