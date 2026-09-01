# Locked design spec

**Direction A, approved September 1 2026.** Art-led: the picture fills the
window and the interface sits on it.

This file exists because five earlier attempts failed for one mechanical
reason. The research on why AI-built interfaces read as generated puts it
plainly: *"Generic is what one prompt does when it has to pick taste, explore,
and write code all at once,"* and *"coding agents sidestep layouts that are
hard to implement right away."* Every earlier attempt decided taste while
typing CSS, and every one of them came out as flat fills, rounded rectangles
and a grid of equal cards, because those are the cheap things to type.

So taste is settled here, and implementation follows it. **If a value is not in
this file, it is not a decision anyone gets to make in a stylesheet.**

---

## 1. Colour

From `docs/design/DESIGN_TOKENS.md`, which is the authority and which two of my
earlier attempts contradicted.

| Token | Hex | Role |
|---|---|---|
| `--red` | `#E0272E` | **Primary fill, and selection.** Play, Continue, Install |
| `--red-hi` | `#F04A50` | Hover on a red fill |
| `--red-ink` | `#FF7A7F` | Red as **text**, only |
| `--teal` | `#22C7B4` | Data series, live status, progress. **Never a button** |

Two rules that are easy to invert and were inverted:

**`--red` is a fill, never text.** It measures about 3.9:1 on the shell, which
clears 3:1 for a control boundary and fails 4.5:1 for body text. White on red
measures about 5:1 and passes. Red text uses `--red-ink`.

**Selection is red, not blue.** The tokens say selection is a red left edge
plus red at 12 percent. An earlier version used `#0a84ff` throughout, borrowed
from the roster site. That site is a database and its accent is selection only;
this is a launcher whose whole job is one action.

### Surfaces

The shell is near-black rather than tinted, because the artwork supplies all
the colour and a tinted shell fights it.

| Token | Hex |
|---|---|
| `--bg` | `#0A0B0D` |
| `--panel` | `#131519` |
| `--panel-2` | `#1A1D22` |
| `--line` | `rgba(255,255,255,.10)` |
| `--line-soft` | `rgba(255,255,255,.06)` |

Hairlines are translucent, not solid hex, so they sit on artwork without
banding. This is lifted from `nsr_roster/public/assets/app.css` and it is the
one thing from that system that matters more here than there, because here
everything sits on a photograph.

### Text

| Token | Hex | On |
|---|---|---|
| `--fg` | `#F4F5F7` | Titles, values |
| `--fg-2` | `#A4ACB8` | Supporting |
| `--fg-3` | `#767F8C` | Labels, units |

---

## 2. Type

Two families. Both bundled as woff2, both OFL, so a Windows build and a macOS
build render identically. Nothing falls back to a system font.

| Role | Family | Use |
|---|---|---|
| Display | **Anton** | Hero titles only, 56px and up, uppercase, tracking `-0.01em` |
| UI | **Inter** | Everything else |
| Data | **Inter**, `font-variant-numeric: tabular-nums` | Counts, durations, device strings |

**Why Anton.** It is the shippable equivalent of Haettenschweiler, which is
what the approved mockup used and what this era of EA sports games shouted in.
One heavy weight, no temptation to pick a wrong one. Its weakness is stated
rather than hidden: Anton is a common poster face, and the defence is that
identity here comes from art and composition, not from the font. `SairaCondensed-800.woff2`
is bundled as the alternative if that trade ever needs revisiting.

**Anton never appears below 56px.** Ultra-condensed heavy type is unreadable
small, and using it for labels is how a poster face becomes a costume.

### Scale

| Step | Size / weight / tracking | Family |
|---|---|---|
| Hero | `clamp(56px, 6.5vw, 104px)` / 400 / `-0.01em` / `0.86` line | Anton |
| Page title | 28px / 700 / `-0.015em` | Inter |
| Section | 15px / 600 | Inter |
| Body | 13px / 400 | Inter |
| Label | 11px / 600 / `0.08em` / uppercase | Inter |
| Nav | 13px / 500 | Inter |

---

## 3. Spacing and size

4px base. `4 8 12 16 20 24 32 48`. Nothing else.

| Token | Value | Note |
|---|---|---|
| `--rail` | 236px | Full height, glass |
| `--tap` | 40px | Anything controller-reachable |
| `--r` | 8px | Standard radius |
| `--r-lg` | 12px | Panels |

---

## 4. The art layer

This is the part that is expensive, and therefore the part earlier attempts
skipped. It is not optional.

1. The backdrop **cover-fits the whole window**, anchored at `center 72%`.
2. Saturation `1.14`.
3. **Two scrims**: dark from the bottom, dark from the left. One flat overlay
   would have to be dark enough to kill the picture.
4. The mark is **composited into the scene** with a blurred copy of itself
   behind it, not placed in a box.
5. Chrome is **glass**: `backdrop-filter: blur(24px)` over a translucent tint,
   so the artwork reads through it.

Art rules for the source images live in `art/README.md`. The one that keeps
being violated: **no frame may carry the game's own HUD.** A launcher wearing
another interface's scoreboard reads as a screenshot.

---

## 5. Motion

One curve, three speeds, lifted from the roster site so the two products move
alike.

```
--ease: cubic-bezier(.2,.7,.3,1)
--t1: 90ms    state
--t2: 160ms   hover, glass
--t3: 220ms   arrivals
```

One arrival keyframe, `rise`, `translateY(6px)` plus opacity. Exits are
instant: leaving should never make anyone wait. `prefers-reduced-motion`
collapses everything to `0.01ms`.

---

## 6. Known risks, written down rather than discovered later

**Glass costs GPU.** `backdrop-filter` is real in WebView2 but it composites
every frame, and on a Steam Deck that competes with the emulator. If it
measures badly the fallback is a pre-blurred image slice, **not** a flat
panel. Swapping glass for a flat fill and still calling it glass is exactly
how this drifts back to generic.

**Anton has no lowercase discipline.** It is caps-only by use here. Any
sentence-case string in Anton will look wrong, so the hero takes short
uppercase strings and nothing else.

**Contrast on artwork is not verifiable from hex values.** Every ratio in this
file is computed against `--bg`. Text over a photograph depends on the
photograph. The scrims are specified to make it safe; it still needs checking
against each real image, and `art/README.md` is where that requirement lives.
