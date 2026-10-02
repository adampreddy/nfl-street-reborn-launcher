# Art drop-in

Direction A is art-led: the picture fills the window and the interface sits on
it. That means the art is not decoration, it is the design, and everything
below is about giving it room to work.

Everything currently in this folder is a **placeholder** cut from in-game
screenshots. Replace it and the launcher changes character without a code
change.

---

## What to drop where

| Path | Size | Format | Used for |
|---|---|---|---|
| `hero-home.jpg` | 2560 x 1440 | JPEG, quality 85 | The Home backdrop, full window |
| `hero-play.jpg` | 2560 x 1440 | JPEG, quality 85 | The Play backdrop |
| `hero-online.jpg` | 2560 x 1440 | JPEG, quality 85 | The Online backdrop |
| `cover.png` | 512 x 512 | PNG, transparent | The mark, composited into the scene |
| `fields/<slug>.jpg` | 720 x 960 (3:4) | JPEG, quality 85 | Field tiles |
| `teams/<slug>.png` | 256 x 256 | PNG, transparent | Team marks |
| `online/ranked.jpg`, `casual.jpg`, `private.jpg`, `matchup.jpg` | at most 1100 wide | JPEG, quality 90 | Online mode tiles and the matchup card (placeholders cut from the hero shots; Adam picks the final art) |

2560 wide rather than 1500, because the window resizes and a backdrop that
starts at window size goes soft the moment anyone maximises it.

---

## What makes a hero image work here

**Leave the left third quiet.** The title, the state line and the buttons sit
there. A busy left side means either unreadable type or a scrim so heavy it
kills the picture. Composition with the action to the right of centre is ideal.

**Keep the top 15 percent calm.** The rail's top and the mark sit against it.

**No text, and no game HUD.** This is the mistake I made repeatedly with the
placeholders: a frame carrying the game's own scoreboard, first-down marker or
replay controls reads as a screenshot of another interface rather than as art.
Every placeholder here is cropped to cut that band off, which is also why they
look tighter than the original frames.

**Dark or mid-tone.** The interface is light type on the picture. A bright sky
across the whole frame cannot be scrimmed without turning grey.

**Wide, not square.** Anything that only works at 16:9 will be cropped on a
Steam Deck at 16:10.

---

## What the code does to it

1. Cover-fits to the window, anchored slightly below centre.
2. Lifts saturation about 14 percent.
3. Two scrims: dark from the bottom, and dark from the left, so the type has
   contrast without flattening the picture.
4. The rail and the readout panel blur whatever is behind them.

If a piece of art needs different treatment, that is a per-image setting rather
than a reason to change the system.

---

## Naming

Lower case, hyphens, no spaces. `fields/the-yard.jpg`, `teams/bills.png`.
A field slug must match the name used in the field list, and a team slug must
match the team folder name in the decal library, lower cased.

`teams/titans.png` is absent on purpose: the Titans decals exist but were never
added to the live pack, so the interface falls back to initials. Drop the file
in and the fallback stops being used, with no code change.
