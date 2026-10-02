# Online page redesign

Date: 2026-10-02 · Branch: `feat/launcher-glass-redesign` · Mockup: `assets/2026-10-02-online-page-mockup.html` (v11, 12 states)

## 1. Goal

Adam: the Online page should feel like a game menu, keep the leaderboard on screen, and speak the same UI
language as the rest of the redesigned launcher. **Only the look and layout change.** Every matchmaking,
lobby, private-room and live-game behaviour stays exactly as it is today: same commands, same arguments,
same order, same polling.

Success: every state in the mockup is reachable on the real page with real data; the existing behaviour
checks (`check-online-mm`, `check-online-restored`, `check-live-now`) pass, updated only where they assert
old layout or old wording that this spec changes on purpose (§8).

## 2. Layout (1280×800, spec §8 grid of the glass redesign)

```
┌ page heading: "Online" + purpose line ──────────── ● 38 players online  ● 3 live games ┐
│ ┌ mode area (left column) ─────────────────────┐ ┌ Leaderboard card ──────────────┐ │
│ │ [ Ranked (big, 2 rows) ] [ Casual         ]  │ │ # · avatar+badge · name/record │ │
│ │ [                      ] [ Private room   ]  │ │   · Elo / change   (top 10)    │ │
│ │ [ action row: main button · note          ]  │ │ …                              │ │
│ └──────────────────────────────────────────────┘ │ Full leaderboard ›             │ │
│ Live now                              See all ›  │                                │ │
│ [ game ] [ game ] [ game ]                       └────────────────────────────────┘ │
```

- Left column: the **mode area** (tiles + action row) fills the height above **Live now**.
- Right column (316px): the **Leaderboard** glass card, full height.
- Heading: `h1` "Online", purpose "Ranked, casual, or a private game with a friend." Right side:
  `● {n} players online` (green dot) and `● {n} live games` (red dot). "players online" is a button: it opens a
  small menu (menu.js) with who is online (today's `.mm-online` list: first 12 names + activity) and the
  existing **Show me as online** switch (`#mm-presence`, `presence_set_enabled`). Both exist today at the
  bottom of the section; they move here, unchanged.
- Pictures on the tiles and the matchup card are placeholders; Adam picks the final art. Each picture
  is a display copy (≤1100px wide JPEG) like the Play art.

## 3. The mode area, by state

The mode area shows exactly one of the states below. Which one is decided by the same precedence the page
uses today (`online-mm.js` 335–359): live match → room → ticket → idle. Held matches draw idle plus a notice.

### 3.1 Idle: the three tiles
- **Ranked** tile (big, spans two rows): name 56px Inter 800; "First to two wins. Moves your Elo.";
  bottom-right `#7` (34px), "1,184 Elo", "▲ 16 this week" (green; red ▼ for a drop; hidden when 0 or unknown).
- **Casual** tile: "One game. Elo stays put."
- **Private room** tile: "Host a room or join with a code."
- Ranked and Casual show a pill top-right: `● {queue_depth[mode]} searching` (from `snapshot.queue_depth`,
  which exists today). Hidden when the count is unknown. Private room shows no count (rooms are private).
- **Pick:** one tile is picked. Picked = 2px red border, picture in full colour. Not picked = 1px stroke,
  picture desaturated and darkened. The pick is stored in `localStorage mm:mode` as today; Private room is a
  third value stored only in page state (`mm:mode` keeps `ranked|casual` so the queue command is unchanged).
- **Action row** follows the pick:
  - Ranked: primary button "Find a ranked game", note "Matched by Elo".
  - Casual: "Find a casual game", note "Matched with anyone".
  - Private room: primary "Host a room" (= `mm_room_create {mode, public:false}`), a room-code field
    (`Room code`), a quiet **Join** button (= `mm_room_join {code}`, quiet until a code is typed), note
    "Send the code to a friend". Under the row, quiet line: "Not on our servers? **Direct connection ›**",
    which opens the existing direct (IP/host-code) Netplay form (`online.js`, unchanged) in a dialog.
    Also quiet: "**Open lobbies ({n}) ›**" when `mm_rooms` returns any public lobby, opening the existing
    lobby list (join one = `mm_room_join`), and "Open a public lobby" inside that list
    (`mm_room_create {public:true}`). Nothing that exists today is removed.
- Find a game runs today's `queueWhenCurrent()` unchanged (content check → fetch/promote → repair → `mm_queue`).
  While it runs the button reads "Checking your game files…" (the status lines it already writes).

### 3.2 Searching (ticket)
- The picked tile keeps colour; the others go dark (saturate 0, brightness .35).
- Pill on the picked tile: "Looking for players near {elo} Elo" (ranked) / "Looking for a casual game".
- Action row: a quiet glass bar "Searching… m:ss" (the existing `.mm-timer` logic) with "Ranked/Casual" at its
  right, one red line sweeping along its bottom; **Cancel** beside it (= `mm_leave_queue`). B / Esc = Cancel.

### 3.3 Match found (pairing, not yet accepted)
- The tiles give way to the **matchup card** (full mode-area width): your avatar, name, `#rank · Elo`,
  team logo (plays-as) | "MATCH FOUND", **VS** (red), "Ranked · first to two wins" / "Casual · one game" |
  the opponent the same way.
- Under the VS: **Accept** (primary) and **Decline** (quiet) = `mm_match_action accept true/false`.
  Under them: "Accept within 0:14" from `relay_deadline` when present, else nothing.
- No other countdown and no second progress bar anywhere (Adam, 10-02).

### 3.4 Accepted, waiting
- Your avatar gets a green tick. Under the VS one pill: "Accepted. Waiting for {opponent}…".

### 3.5 Getting ready / starting
- Both accepted: the pill carries the existing `RELAY` words (Getting ready, Connecting you both, Starting the
  game, Between games). Under it: "Your game opens on its own". **Leave match** = quiet button top-right of
  the card (`mm_match_action leave`), hidden in terminal states as today.
- One red line along the bottom of the card only while the relay is moving (accepted … running).
- A relay error shows as today's `p.refusal` text under the pill.

### 3.6 Playing a series
- "PLAYING", VS, "Game {n} · Series {a}–{b}" (when `best_of > 1`), "Ranked · first to two wins".
  Leave match top-right. Casual shows "Casual · one game" and no series line.

### 3.7 Your private lobby (room)
- Two glass cards side by side:
  - **Your room** (`Casual`/`Ranked` at right): ROOM CODE label, the code at 44px, **Copy code**
    (the mockup's **Copy invite link** is NOT built: lobby codes have no invite link today), PLAYERS: each member row = avatar, name (`you`), Ready (green) / Not ready.
  - **Chat**: messages (`name` bold + text), "Say something…" field (Enter sends, = `mm_room_action chat`).
- Action row: **Start** (primary, host only, enabled when everyone is ready), **I'm ready / I'm not ready**
  (toggle, = `ready`), **Leave room**. Note under it: "Start works once you're both ready. The host starts the game."

### 3.8 Held match
- Idle tiles plus one amber notice above them: title "Your last match is held for review", text chosen by
  `hold_reason` exactly as today (content-stale → repair notice, content-mismatch → names the opponent,
  else "Nothing changed on the leaderboard. Someone will look at it. You can play again now."), Dismiss.

### 3.9 Signed out / offline / disabled
- **Signed out:** the tiles blur behind one panel: "Sign in to play online", "Connect your Discord account to
  play online and track your stats!", **Sign in with Discord** (same device-code flow as Stats, `sign-in.js`).
  No rank, no Elo, no button row. Leaderboard and Live now stay visible.
- **Offline (no snapshot):** the same panel shape with "Can't reach the Reborn servers right now. Trying again."
  and no button.
- **Disabled:** today the matchmaking section empties and only private rooms remain. New: only the Private
  room tile is shown (full width of the mode area), with its action row. Behaviour unchanged.

## 4. Leaderboard card
- Header "Leaderboard" + "{n} ranked" muted. Column labels `#`, `PLAYER`, `ELO` (11px uppercase, fg-3).
- 10 rows, each: rank (top 3 white, rest fg-3), 32px avatar (Discord picture, else letter on `--av-*`) with an
  18px team-logo badge, name + record `W–L` (muted), Elo (bold, tabular) + change `▲ n` / `▼ n`.
- Your row: `rgba(red,.12)` fill, 3px red left edge, red rank. When you are outside the top 10, an 11th row
  shows you after a thin gap.
- Footer link **Full leaderboard ›** (red-ink) opens the web leaderboard (`open_website`).
- Signed out: no "you" row.
- Any missing field hides its spot (no record → no subline; no change → no arrow line).

## 5. Live now
- Section header "Live now" + **See all ›** (opens the web `/live` index).
- Up to 3 cards, glass, each: two team rows (logo, team name, player name muted, score 20px bold; the losing
  side dimmed) on a faint team-colour fade (`primary` at .3), footer: red dot, "Ranked · Game 2" /
  "Casual", and a small **Watch** button (`href=live.html?match=<id>`, `data-key=watch-<id>`, text "Watch").
- Empty: "No live games right now." Error: the existing `p.live-error[role=alert]` above the last good rows.
- `live-now.js` polling, backoff, image allow-list and row reuse are unchanged.

## 6. Controller and keyboard
- The three tiles are one row/column group (`data-nav`): the D-pad moves between tiles and **the pick follows
  focus**. A on a tile = the action row's primary. The focus ring is the launcher's white 2px ring, offset 4px
  so it sits outside the red pick border, shown only while a controller drives (`body.gamepad-input`).
- B / Esc: Cancel while searching; Decline in Match found; nothing otherwise (sidebar back as today).
- Focus is restored by `data-key` across the 1 s redraw (today the page loses button focus on every state
  change; this fixes it): `mode-ranked|casual|private`, `find`, `cancel`, `accept`, `decline`, `leave-match`,
  `room-start|ready|leave`, `copy-code`, `copy-invite`, `chat`.

## 7. Data the page needs

| Shown | Source | Exists? |
|---|---|---|
| players online | `snapshot.online.count` | yes |
| N searching per mode | `snapshot.queue_depth` | yes |
| live games count + cards | `live_matches` | yes |
| your rank / Elo | `stats_dashboard` (`you.elo`) | yes |
| your Elo change this week | backend | **no** — add `elo.week_delta` to the dashboard |
| leaderboard top 10 (+ you) | backend | **no** — new `GET /v1/leaderboard?limit=10` → `{total, rows:[{rank, player_id, display_name, avatar_url, team, wins, losses, elo, delta_week}], you}`; new Tauri command `leaderboard` |
| opponent rank/Elo/team in Match found | `snapshot.match` | partly — add `opponent_rank`, `opponent_elo`, `opponent_team` to the match view |
| accept deadline | `relay_deadline` | yes (forwarded, unused today) |

Rule: a value the server does not send hides its spot; nothing is invented on the client. With ELO_ENABLED
off (today), ranks/Elo/changes are hidden and the leaderboard card shows "Rankings start soon." in its place.

## 8. Wording changes (checks updated on purpose)
- "Quick Match" heading → gone (tiles). "Stop looking" → "Cancel". "Looking for a casual game" moves into the
  tile pill. "Ranked · best of 3" → "Ranked · first to two wins" (ruling: never label "Best of 3").
  "Series: you 1 – 0 Jock" → "Game 2 · Series 1–0". "Nothing has changed on the ladder" → "Nothing changed on
  the leaderboard". "Open a public lobby", "Join by code", "Leave match", "Accept", "Decline", "Start",
  "Leave lobby" → "Leave room". "Player name / Host code / Join game / Create room" (direct connection dialog) unchanged.
- `check-online-mm` assertions are updated to these strings; every behavioural assertion (commands, args,
  `#screen` hidden while busy, timer moves, presence_page sent, no page errors) stays.

## 9. Files
- `online.html` (structure), `pages/online.css` (new, theme tokens only), `online-mm.js` (render functions
  rewritten, state logic and commands untouched), `online.js` (form moves into a dialog; logic untouched),
  `live-now.js` (row markup + classes; logic untouched), new `leaderboard.js` (card + poll every 60 s),
  `main.rs` (`leaderboard` command), backend (§7). Art: display copies under `art/online/`.

## 10. Out of scope
- New matchmaking behaviour, new modes, region picking, party queue.
- The `live.html` watch page and the web leaderboard page (links only).
