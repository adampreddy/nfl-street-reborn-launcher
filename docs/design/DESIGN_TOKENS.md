# Design Tokens

## NFL Street Reborn Launcher

**Date:** August 29, 2026
**Covers brief deliverable:** 6 (design tokens)
**Depends on:** [`UX_RESEARCH_DOSSIER.md`](./UX_RESEARCH_DOSSIER.md)

The brief says "do not choose arbitrary values, explain the visual and accessibility rationale." Every value below carries its reason. Where a value comes from a standard, the standard is named.

### Decisions this encodes

| Decision | Value | Source |
|---|---|---|
| Primary accent | Reborn red | Adam |
| Focus ring | Neutral, two-tone | Adam |
| Target size | 44 px on controller-reachable controls | Adam, deferring to the research |
| Welcome gradient | Near-black to dark red | Adam |
| Left-handed and axis-inversion presets | **Not shipping.** Full remapping covers it | Adam |
| Icon system | **Stock set**, risk accepted and mitigated in section 11 | Adam |
| Reborn motif | **The shear angle from the logo's R** | Adam |

---

## 1. Color

### 1.1 The two-accent rule

This is the governing constraint and everything else follows from it.

**Red means Reborn and it means act. Teal means data. They are never interchangeable.**

Red is the brand and it fills primary actions. Teal is data series, live status, and progress. A red pixel on a chart would read as "loss" in a competitive stats context and be misread immediately. A teal button would read as informational and lose the call to action.

### 1.2 Neutrals

Near-black shell with layered surfaces, not flat black. Flat black kills the sense of depth the moodboard's settings and dashboard regions depend on.

| Token | Hex | Use |
|---|---|---|
| `surface-base` | `#0B0C0E` | Application shell, the lowest layer |
| `surface-1` | `#131519` | Page background, nav rail |
| `surface-2` | `#1A1D22` | Cards, panels, settings rows |
| `surface-3` | `#22262C` | Raised cards, hovered rows, popovers |
| `surface-4` | `#2B3038` | Dialogs, menus, the top layer |
| `border-subtle` | `#2A2F36` | Dividers, dense table rules |
| `border-default` | `#3A414A` | Card and input boundaries |
| `border-strong` | `#525B67` | Emphasis, selected-but-unfocused |

**Rationale:** four surface steps, not two, because the moodboard's settings region (D) needs dense panels separated by one-pixel borders while the dashboard region (E) needs cards at visibly different elevations. Two steps cannot serve both. `border-default` at `#3A414A` clears 3:1 against `surface-2`, satisfying WCAG 1.4.11 Non-text Contrast for control boundaries.

### 1.3 Text

| Token | Hex | Contrast on `surface-1` | Use |
|---|---|---|---|
| `text-primary` | `#F2F4F7` | ~16:1 | Headings, values, labels |
| `text-secondary` | `#A0A8B4` | ~7.4:1 | Descriptions, metadata |
| `text-tertiary` | `#7C8593` | ~4.6:1 | Timestamps, unit suffixes |
| `text-disabled` | `#525B67` | ~2.5:1 | Disabled only, never information |

**Rationale:** `text-tertiary` is deliberately held at roughly 4.6:1 rather than dropping to the 3:1 that large text would allow. The brief prohibits "low-contrast gray text," and this product is read at television distance. `text-disabled` intentionally fails AA because disabled controls are exempt, and anything that must be readable does not use it.

### 1.4 Reborn red

Sampled from the logo's `R` and adjusted so a white label on the fill clears 4.5:1.

| Token | Hex | Use |
|---|---|---|
| `red-700` | `#9E1519` | Welcome gradient terminus |
| `red-600` | `#C41E24` | Pressed primary |
| `red-500` | `#E0272E` | **Primary fill.** Play, Continue, Install |
| `red-400` | `#F04A50` | Hovered primary |
| `red-300` | `#FF7A7F` | Red *text* on dark surfaces |

**Rationale, and this is the important one:** `red-500` sits at roughly 3.9:1 against `surface-base`. That clears 3:1 for a control boundary but **fails 4.5:1 for body text**. So red-500 is a fill color only. White on red-500 measures roughly 5:1, which passes. When red is needed as text, use `red-300`, which clears 4.5:1.

Getting this backwards is the single most likely accessibility failure in a red-primary dark interface.

### 1.5 Teal, the data accent

| Token | Hex | Use |
|---|---|---|
| `teal-500` | `#22C7B4` | Primary data series, live status, progress fill |
| `teal-400` | `#4EDCC9` | Hover, secondary series |
| `teal-700` | `#0E7A6E` | Chart fills under a line, low-emphasis |

`teal-500` clears roughly 8:1 on `surface-1`, so it is safe as both fill and text.

### 1.6 Semantic status

| Token | Hex | Icon required | Use |
|---|---|---|---|
| `success` | `#34D399` | check | Verified, installed, connected |
| `warning` | `#F5A623` | triangle | Needs attention, held for review |
| `error` | `#FF4D4F` | filled circle with x | Failed, corrupt, unreachable |
| `info` | `#7C8CA5` | circle with i | Neutral notice |

**The error-versus-brand problem, stated plainly:** `error` at `#FF4D4F` and `red-500` at `#E0272E` are close in hue. That is unavoidable in a red-brand product, and hue alone cannot carry the difference. The rule that resolves it:

- **Red-500 only ever appears as a filled action.** It is a button, never a status.
- **Error only ever appears with its icon and a word.** It is text, an icon, or a left border, never a filled button.

A user never has to distinguish the two by color, because they never appear in the same role.

### 1.7 Competitive and connection states

Every one carries a shape or a word as well as a color, per design principle 5. Grayscale the screen and all of these still read.

| State | Color | Second signal |
|---|---|---|
| Ranked | `#F5A623` amber | shield glyph plus the word "Ranked" |
| Casual | `#7C8CA5` slate | circle glyph plus "Casual" |
| Private | `#A78BFA` violet | lock glyph plus "Private" |
| Online | `teal-500` | filled dot |
| Offline | `#6B7480` | hollow dot |
| Win | `success` | "W" plus a positive rating delta |
| Loss | `error` | "L" plus a negative rating delta |
| Held for review | `warning` | triangle plus "Held for review" |

**Rationale:** ranked amber rather than red keeps competitive tier separate from brand and from error. Violet for private is the only hue in the system not otherwise used, so it never collides.

### 1.8 Focus

**Neutral, two-tone, per Adam's decision.**

| Token | Value |
|---|---|
| `focus-ring-outer` | `3px solid #FFFFFF` |
| `focus-ring-inner` | `1px solid #000000`, inset |
| `focus-ring-offset` | `2px` |

**Rationale:** a single-color ring cannot pass 3:1 against both a near-black settings page and a bright hero image. A white ring with a black inset always presents one of its two edges at high contrast against whatever is behind it. WCAG 2.2 Focus Appearance requires the indicator to be at least a 2 px perimeter; 3 px is used because the ring must survive being photographed at television distance, per design principle 3.

Red was rejected for focus because a red ring on the red-heavy hero art would fail 3:1 locally, and because red is already carrying selection.

**Focus versus selection are different things and look different:**

- **Focus** is where the stick is pointing right now. Neutral two-tone ring, plus a lift to `surface-3`, plus a 1.02 scale.
- **Selection** is what you have chosen. A 4 px sheared `red-500` left edge (see section 11), a `red-500` at 12% fill, and `border-strong`.

An option card can be focused, selected, or both, and all three read differently. The corner check badge that appeared in the first draft was replaced under anti-vibecoding violation V1; a full-height sheared edge reads at television distance where a corner mark does not.

### 1.9 Welcome gradient

Moodboard region J's composition with the hue moved, per Adam's decision.

```
linear-gradient(160deg, #0B0C0E 0%, #1A0E10 45%, #4A1216 80%, #9E1519 100%)
```

Near-black at the top where the icon and copy sit, warming into `red-700` at the lower right. The copy stays on the near-black end so it never needs a scrim.

---

## 2. Typography

### 2.1 Faces

| Role | Face | Fallback |
|---|---|---|
| UI | Inter | system-ui, Segoe UI, SF Pro, Cantarell |
| Numerals in data | Inter with `font-variant-numeric: tabular-nums` | same |
| Display | Reborn logo lockup as artwork | not a typeface |

**Rationale:** Inter is open licensed, so it can be embedded on all three platforms without a desktop license question, and it was drawn for small-size screen legibility, which is what moodboard regions A and D demand. Tabular numerals are mandatory anywhere a score, rating, or file size appears, because ragged digits in a box score are unreadable at a glance.

**Explicitly not doing:** a graffiti display face for UI text. The blueprint says NFL Street personality comes from art, motion, and content rather than covering every surface, and a display face in body copy is the fastest way to look amateur.

### 2.2 Scale

Two scales, because density is a breakpoint and not a compromise. Ratio 1.25, base 14 on desktop and 16 on the 10-foot scale.

| Token | Desktop | 10-foot | Use |
|---|---|---|---|
| `text-2xs` | 11 | 13 | Uppercase metadata labels only |
| `text-xs` | 12 | 14 | Timestamps, chips, table units |
| `text-sm` | 14 | 16 | **Body default.** Settings rows, descriptions |
| `text-base` | 16 | 18 | Card titles, primary labels |
| `text-lg` | 20 | 23 | Section headings |
| `text-xl` | 25 | 29 | Page titles |
| `text-2xl` | 31 | 36 | Onboarding questions |
| `text-3xl` | 39 | 45 | Hero title |
| `text-stat` | 48 | 56 | Headline numerals only |

The 10-foot scale activates below 1400 px width or on any display flagged as a television. It is roughly a 1.15 multiplier, not a doubling, because the desktop scale was already sized for legibility rather than for maximum density.

### 2.3 Line height and measure

| Context | Line height | Max measure |
|---|---|---|
| Dense rows, chips, tables | 1.35 | none |
| Body and descriptions | 1.55 | 68 characters |
| Headings | 1.2 | 40 characters |
| Headline numerals | 1.0 | none |

**Rationale:** 68 characters for body is the conventional readable measure. Capping it stops settings descriptions from stretching across an ultrawide monitor, which was one of the responsive targets in the brief.

### 2.4 Uppercase

Uppercase is allowed at `text-2xs` and `text-xs` only, with 0.06em letter spacing, for category labels and metadata. Never for a sentence, a button, or a heading. The blueprint says use it sparingly and the brief prohibits shouting.

---

## 3. Spacing

Four-pixel base unit.

| Token | px | Use |
|---|---|---|
| `space-1` | 4 | Icon to label |
| `space-2` | 8 | Inside chips, tight stacks |
| `space-3` | 12 | Settings row padding, card internals |
| `space-4` | 16 | **Default gap.** Between cards, sections |
| `space-6` | 24 | Between setting groups |
| `space-8` | 32 | Page gutters at desktop |
| `space-12` | 48 | Between major page regions |
| `space-16` | 64 | Onboarding stage padding |

**Rationale:** the moodboard's settings region separates groups with spacing rather than with oversized cards, which is `space-6` doing the work a card would otherwise do. That is the specific mechanism that keeps Settings from becoming the "every metric is a decorative card" outcome the brief prohibits.

---

## 4. Size and target area

| Token | px | Applies to |
|---|---|---|
| `target-min` | 44 | **Anything controller-reachable.** Buttons, nav items, cards, settings rows, list items |
| `target-mouse` | 24 | Mouse-only affordances: close X, table sort header, inline link |
| `control-height-sm` | 28 | Dense inline controls inside a 44 px row |
| `control-height` | 36 | Standard input, select, segmented control |
| `control-height-lg` | 48 | Primary hero action |

**Rationale:** WCAG 2.2 Target Size (Minimum) is 24 px at AA. This product goes to 44 px, the AAA value, on anything a controller can land on, for two reasons: the Steam Deck has a touchscreen and 24 px is smaller than a thumb, and a larger target carries a larger focus ring, which is what is actually visible at distance.

**The cost is stated honestly:** a settings page fitting 14 rows at 24 px fits about 9 at 44 px. That is the tradeoff Adam accepted.

---

## 5. Radius

| Token | px | Use |
|---|---|---|
| `radius-xs` | 2 | Dense controls in advanced settings |
| `radius-sm` | 4 | Inputs, chips, table cells |
| `radius-md` | 8 | **Default.** Cards, buttons, settings panels |
| `radius-lg` | 12 | Dialogs, option cards, hero panel |
| `radius-full` | 9999 | Status dots and avatars only |

**Rationale:** medium radii per the blueprint, so the application does not resemble a mobile banking dashboard. `radius-full` is restricted to dots and avatars because the brief prohibits "floating pills." Advanced settings get `radius-xs` because sharper corners read as denser and more technical, which is the correct signal there.

---

## 6. Elevation and depth

| Token | Composition | Use |
|---|---|---|
| `elev-0` | no shadow, `border-subtle` | Dense table rows, settings rows |
| `elev-1` | no shadow, `border-default` | Cards at rest, panels |
| `elev-2` | `0 2px 8px rgba(0,0,0,0.4)` plus `border-default` | Focused or hovered card |
| `elev-3` | `0 8px 24px rgba(0,0,0,0.55)` plus `border-strong` | Popovers, menus |
| `elev-4` | `0 16px 48px rgba(0,0,0,0.7)` plus `border-strong` | Dialogs, blocking update screen |

**Rationale:** the blueprint says shadows are for floating dialogs, menus, media overlays, and major cards, and that dense panels separate with one-pixel borders. So `elev-0` and `elev-1` carry no shadow at all. Shadow starts only when something genuinely floats.

### Blur and scrim

| Token | Value | Allowed where |
|---|---|---|
| `scrim-hero` | `linear-gradient(to top, rgba(11,12,14,0.95), rgba(11,12,14,0) 70%)` | Under text on hero art |
| `scrim-modal` | `rgba(11,12,14,0.72)` | Behind dialogs |
| `blur-overlay` | `backdrop-filter: blur(16px)` | **Only** over full-bleed artwork or a transient overlay |

**Explicit prohibition:** no backdrop blur in Settings, forms, tables, or Stats. The brief names excessive glassmorphism as a prohibited outcome, and the blueprint restricts glass to artwork and transient overlays. This is the enforcement point.

---

## 7. Motion

| Token | Duration | Easing | Use |
|---|---|---|---|
| `motion-instant` | 90ms | `cubic-bezier(0.2, 0, 0, 1)` | Focus ring appearing |
| `motion-fast` | 140ms | `cubic-bezier(0.2, 0, 0, 1)` | Hover, card lift, toggle |
| `motion-base` | 220ms | `cubic-bezier(0.2, 0, 0, 1)` | Page transition, panel open |
| `motion-slow` | 360ms | `cubic-bezier(0.4, 0, 0.2, 1)` | Dialog, blocking screen |
| `motion-hero` | 8000ms | `linear` | Slow hero parallax, desktop only |

**Rationale:** focus must feel immediate because it tracks a stick and any lag reads as input lag, so 90ms is deliberately below the threshold where motion is perceived as animation. The shared decelerating curve means everything settles rather than bounces. Nothing overshoots, because a launcher is a tool.

**Reduced motion:** when the system requests it, `motion-hero` is disabled entirely, everything else collapses to 0ms, and the focus ring appears instantly. Focus and selection remain fully visible, because they are state, not decoration. The Performance graphics preset applies the same rules regardless of the system setting, and also disables video heroes.

---

## 8. Breakpoints

| Token | Range | Layout |
|---|---|---|
| `bp-compact` | under 1100 | Icon-only nav rail, single content column |
| `bp-deck` | 1100 to 1399 | **Steam Deck 1280x800.** 10-foot type scale, reduced item counts, icon rail with labels on focus |
| `bp-desktop` | 1400 to 1919 | Full nav rail with labels, dense grid |
| `bp-wide` | 1920 to 2559 | Dense grid, more columns |
| `bp-ultra` | 2560 and up | Content max width 2200 px, centered, gutters absorb the rest |

**Rationale:** `bp-ultra` caps content width rather than stretching. The brief lists ultrawide as a responsive target and the failure mode there is a settings description running 200 characters wide, which the 68-character measure and this cap both prevent.

**Television** is detected separately from width, because a 1080p television and a 1080p monitor are the same pixel count at wildly different viewing distances. When flagged, the 10-foot type scale and reduced item counts apply regardless of resolution.

---

## 9. Density as a mode

Design principle 7 says density is a breakpoint, not a compromise. Concretely:

| | Desktop | Deck and television |
|---|---|---|
| Type scale | desktop column | 10-foot column |
| Settings rows visible | about 12 | about 8 |
| Mod grid columns | 5 | 3 |
| Rail item height | 44 px | 52 px |
| Hero height | 42% of viewport | 55% of viewport |
| Metadata chips per card | up to 4 | up to 2 |

The same page at 2560x1440 and 1280x800 differs in **item count**, not only in scale. That is the testable form of the principle.

---

## 10. What these tokens forbid

Cross-checked against the brief's prohibited outcomes. Each row names the token rule that makes the failure impossible rather than merely discouraged.

| Prohibited | Prevented by |
|---|---|
| Excessive glassmorphism | `blur-overlay` restricted to artwork and transient overlays |
| Random gradient accents | Exactly one gradient token exists, `welcome-gradient` |
| Floating pills everywhere | `radius-full` restricted to dots and avatars |
| Inconsistent radii and shadows | Five radius tokens, five elevation tokens, no ad hoc values |
| Focus as a slight color change | Focus is ring plus surface change plus scale, three signals |
| Hover-only interaction | No hover-only token exists; hover shares `surface-3` with focus |
| Low-contrast gray text | `text-tertiary` floored at roughly 4.6:1 |
| Color-only status | Every status token in 1.7 has a required second signal |
| Desktop as enlarged mobile | Two type scales and per-breakpoint item counts |
| Decorative metric cards | `space-6` group separation replaces card-per-metric |

---

## 11. The Reborn motif

The anti-vibecoding review found that the system as specified would produce a
competent, accessible, generic dark launcher. Red plus near-black plus a clean
sans is a good system and it is nobody's in particular. Adam approved a single
motif to fix that.

### The angle

The logo's `R` is sheared. That slant is the only distinctive geometry Reborn
owns, and it comes from the brand rather than from a mood.

| Token | Value |
|---|---|
| `motif-shear` | `12deg` |
| `motif-skew` | `transform: skewX(-12deg)` |

**Rationale for 12 degrees:** measured off the logo's `R` stem, rounded to a
whole number so it composes predictably. Steeper reads as a novelty font;
shallower is invisible at small sizes and at distance.

### Where it is used, and only here

Four places. All of them are functional elements that already exist, so the
motif costs no additional visual furniture.

| Element | Application |
|---|---|
| **Selection edge bar** | The 3 px left bar on a selected nav item, settings group, or option card is sheared rather than rectangular |
| **Section divider terminal** | Settings group dividers end in a sheared terminal instead of a square cut |
| **Progress leading edge** | The advancing end of a determinate progress fill is sheared. The trailing end is not |
| **Hero scrim** | The gradient under hero text runs on the shear angle rather than straight vertical |

### Where it is forbidden

- **Never on the focus ring.** Focus must be geometrically simple and instantly
  readable. A sheared ring reads as a rendering artifact.
- **Never on cards, buttons, inputs, or panels.** Skewed containers break text
  baselines and target-area hit testing.
- **Never on text.** No skewed type anywhere.
- **Never more than once in a single component.** A card with a sheared
  selection bar does not also get a sheared divider.

### Why this survives the guardrails

The blueprint says NFL Street personality should come from art, motion,
texture, typography and content rather than covering every surface in graffiti.
Four functional elements carrying one angle is a motif, not a texture. It is
also cheap to remove if it reads badly on real screens, because nothing depends
on it structurally.

**Unverified:** how the shear reads at 1280x800 on an actual Steam Deck, and
whether a 12 degree shear on a 4 px progress edge is visible at all. Both need
a rendered screen.

---

## 12. Icon system

**Decision: a stock set.** Adam accepted the risk rather than commissioning
twelve glyphs.

The anti-vibecoding brief names "generic Lucide icons as the entire identity
system" as a prohibited outcome, so accepting a stock set means the mitigation
has to be real rather than nominal.

### Mitigation

1. **One family only.** A single stock set, one stroke weight, one corner
   radius, one terminal style. The prohibited failure is *mixing* sets, and
   that is the one thing fully in our control.
2. **Icons are never the only label.** Every navigation item, status, and
   action carries a word except in the collapsed icon rail, where the label
   appears on focus. An icon set cannot carry an identity it is never asked to
   carry alone.
3. **The motif does the identifying, not the icons.** Section 11 exists partly
   because the icons will not distinguish the product.
4. **Twelve glyphs stay on the list as a later upgrade:** play, repair,
   install, controller, music, stats, ranked, casual, private, online, warning,
   error. Replacing those twelve later changes the product's character without
   touching the other hundred.

**Honest assessment:** this is the weakest point in the visual system. It is a
deliberate, recorded tradeoff rather than an oversight.

---

## Open items

1. **Inter needs confirming** as the shipped face, or a substitute chosen. It is open licensed and safe, but it is also a common choice and the brief warns against defaults reading as templated. Given that icons are now a stock set too, a distinctive heading face carries more weight than it did.
2. **The logo is raster only** at 4096x4096. Window icons and Deck-scale rendering will be soft until a vector exists.
3. **Contrast figures above are computed, not measured in situ.** They need verification against real rendered screens during the accessibility audit, deliverable 9.
4. **No implementation.** These are specifications. Nothing has been built.
