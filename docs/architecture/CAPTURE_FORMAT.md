# The capture format, read from the fork

**September 1 2026.** Read directly out of
`Source/Core/Core/NflsStatTracker.cpp` in the Reborn Dolphin fork, not inferred.

Written because the stats subsystem was built against a partial description of
this format. The twelve per-player fields were right; everything wrapped around
them was missing, including the one field that decides whether a capture can be
trusted at all.

---

## Where captures land

```
<Dolphin user folder>/LeagueGameStats/<tag>_<NNN>.json
```

The name starts as `game_NNN.json` and is **renamed to
`<TeamA>_vs_<TeamB>_NNN.json` once the team names are known**, so the league
never has to guess which game a file belongs to. A counter keeps repeat
matchups from overwriting each other, and the path is chosen once per capture
and reused, so a later and better decode updates the same file.

Consequence for the launcher: a file can be **renamed under you** between the
moment it appears and the moment it is complete. Watching this directory by
filename alone will double count.

---

## Why there are currently zero captures

Two gates, both in the fork:

**Netplay gate.** Capture runs only during NetPlay, because league games are
played online and local games would otherwise fill the folder with results that
belong to no series. The documented escape hatch is an empty file:

```
<Dolphin user folder>/nsr_track_offline.txt
```

Its presence is read **once and cached** in a `static`, so creating it while
Dolphin is running has no effect until the next launch.

**Game gate.** The disc ID must be one of `GN7E69` (stock GameCube NFL Street 2
USA), `FR4N31`, or `CR4E12`, the two custom Reborn builds which carry different
disc IDs but identical static globals.

This is why the folder on this machine is empty, and it is a real thing for the
launcher to own: capturing offline games is a setting, expressed as the presence
of that file, and it needs a restart to take effect.

---

## The record

```json
{
  "schema": "nsr.game.v1",
  "source": "dolphin",
  "captured_at": "2026-09-01T15:18:00Z",
  "teams": [
    { "name": "Bills",    "score": 34 },
    { "name": "Dolphins", "score": 21 }
  ],
  "style": [0, 0],
  "team_totals": [ { }, { } ],
  "sides_verified": true,
  "per_player_matched": 8,
  "players": [
    {
      "name": "Allen", "team": "Bills",
      "side": 0, "slot": 1, "digp": 1234,
      "stats": { }
    }
  ]
}
```

`team_totals` is present **only when the fork managed to read totals**, so a
consumer must treat it as optional rather than assuming two entries.

---

## The twelve per-player stats, and their in-game source

Confirmed against `COLS[]`. The second column is the game's own four character
tag, which is useful when tracing a value back through the DOL.

| Field | Game tag |
|---|---|
| `pass_td` | `dtag` |
| `rush_td` | `dtug` |
| `rec_td` | `dtcg` |
| `pick_six` | `tisg` |
| `fum_td` | `tflg` |
| `interceptions` | `nisg` |
| `forced_fumbles` | `fflg` |
| `fumble_rec` | `rflg` |
| `tackles` | `atdg` |
| `sacks` | `kslg` |
| `thrown_int` | `niag` |
| `fumbles` | `ufug` |

**There is no yardage of any kind.** No total yards, no passing, no rushing. A
box score screen was already designed around those and had to be discarded.

---

## `sides_verified` is the field that matters most

Quoted from the fork, because the reasoning is the whole point:

> Tell the server whether side to team attribution was CONFIRMED or merely
> assumed. Without this the server cannot distinguish a verified capture from a
> coin-flip, because a swapped capture is internally consistent and looks
> entirely valid.

So a capture with `sides_verified: false` may have **both teams' statistics
swapped**, and nothing inside the file will reveal it. Every score, every
player line, every total will be self consistent and wrong.

Rules that follow, and they are not optional:

- A record with `sides_verified: false` is **never published as authoritative**.
- Any interface showing such a record must mark it, in the interface, not only
  in a log.
- `per_player_matched` is a count, not a boolean. A capture can be partially
  matched, and a screen showing eight of eleven players as though it were
  complete is making a claim the file does not support.

This is the same class of problem as the earlier finding that a quarterback was
credited with sacks, which suggests defensive plays can be attributed by roster
slot rather than by player. Both mean the same thing for the launcher: **shape
validity is not correctness**, and the interface has to be able to say so.

---

## What the launcher still needs

- A watcher that survives the rename, keyed on content rather than filename.
- A setting that manages `nsr_track_offline.txt`, saying plainly that it needs
  a restart.
- Refusal to submit a record whose `sides_verified` is false without an
  explicit human decision.
- A content key for idempotency. `stats.rs` computes one, and its author was
  explicit that it has **not** been diffed byte for byte against the server's
  `ingest.php`, so it must not be sent as `capture_hash` until it has.
