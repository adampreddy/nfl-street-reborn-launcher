# Anti-Vibecoding Review

## NFL Street Reborn Launcher

**Date:** August 29, 2026
**Covers brief deliverable:** 10 (anti-vibecoding review)
**Scope:** the design specifications produced so far. No UI has been implemented, so this reviews specifications, not screens.

The brief says: "Before any design is approved, perform a written review against the guardrails below and identify every violation." Identifying violations is the point, so this document leads with the ones found rather than with the passes.

---

## Violations found

### V1. The moodboard itself contains a prohibited pattern, and one region was nearly copied

**Guardrail:** "Unmodified component-library examples" and "Copying Steam, Discord, Playnite, Nintendo, or PlayStation literally."

**Finding:** the onboarding option card specification in [`COMPONENT_SPECIFICATION.md`](./COMPONENT_SPECIFICATION.md) section 4 is derived very closely from moodboard region C, which is a screenshot of Discord's actual server-onboarding screen. Icon left, title, two lines of support text, check badge top-right, two-by-two grid, one filled primary bottom-left, secondary explanation beside it. That is Discord's layout with the colors changed.

**Severity:** medium. Structure is legitimately transferable and the moodboard is a directional authority, but "structure yes, skin no" is a thin defense when the structure is this specific.

**Resolution: applied.** Both changes are in [`COMPONENT_SPECIFICATION.md`](./COMPONENT_SPECIFICATION.md) section 4.

1. The corner check badge is replaced by a 4 px full-height sheared left edge. It is visible at television distance where a corner mark is not, and it carries the Reborn shear, which a badge cannot.
2. Layout is width-driven rather than a fixed grid. Below 900 px of content width the group stacks. That is not a stylistic choice: Steam Deck at 1280 px minus a 200 px rail leaves under 1000 px, and Windows at 150% scaling leaves less.

Both are functional improvements that happen to remove the resemblance. The graphics-preset wireframe is redrawn to match.

**Status: CLOSED.**

---

### V2. Home is close to the prohibited "hero plus rails of identical cards" shape

**Guardrail:** "A left sidebar plus an arbitrary grid of identical cards on every page" and "Every metric presented as a decorative card."

**Finding:** Home as wireframed is a left rail, a hero carousel, then rails of uniform cards. That is the shape of every content app shipped in the last decade, and the fact that moodboard region F does it does not make it distinctive.

**Mitigating factors that are real:** Home is not a grid of identical cards. It carries a persistent Play Now, a profile card, and a last-match row, which are three distinct component types before any rail appears. Rails appear on Home and Mods only, not on Play, Online, Music, Stats, or Settings. Six page templates exist, which is the actual defense against this guardrail.

**Severity:** low, but worth naming because "we have six templates" is exactly what every project says.

**Resolution:** the Home hero should carry live product state, the user's next match or the current server population, rather than being a generic promotional carousel. That makes it a dashboard element rather than a storefront banner and it is the difference between Home being useful and Home being a marketing page.

**Status: OPEN, recommendation made.**

---

### V3. Icon system is unresolved and the default path is prohibited

**Guardrail:** "Generic Lucide icons as the entire identity system" and "Mixing unrelated icon styles."

**Finding:** no icon system is chosen. The default path for a React and Tailwind project is Lucide, which the brief names explicitly as prohibited.

**Severity:** medium, and it is the single most likely way this product ends up looking generated.

**Resolution: Adam chose a stock set and accepted the risk.** Mitigation is specified in [`DESIGN_TOKENS.md`](./DESIGN_TOKENS.md) section 12: one family only, icons never carry meaning alone, and the motif rather than the icons does the identifying. The twelve glyphs remain a later upgrade.

**Status: CLOSED, with recorded risk. This is the weakest point in the visual system.**

---

### V4. Wireframe content is partly invented

**Guardrail:** "Marketing filler, fake testimonials, fabricated statistics, lorem ipsum, or invented marketplace content" and the safeguard "Use realistic Reborn content and data as early as possible."

**Finding:** the wireframes use real Reborn material where it exists (mod names from the actual `MODS` folder, `GN7E69`, real settings like EFB copies drawn from documented Reborn behavior). But player ratings, match scores, wait times, and queue populations are **invented**. `«1,247»`, `«34 ms»`, `«14 players in queue»` are all made up.

**Severity:** low at wireframe stage, **high if it survives into mockups.** Invented numbers make a design look plausible in review and then break when real data arrives. A five-digit Elo, a three-hour playtime, a 47-character mod title, and an empty queue all change the layout.

**Resolution required before high-fidelity:** export real data. The stat pipeline already produces real box scores, the site has real ratings, and the `MODS` folder has real packages. Every mockup should use them, including the ugly cases.

**Resolution: partially closed.** Real captured data was exported and section 12 of the wireframes was rebuilt from it. Doing so immediately invalidated the box score: it showed Total yards, Passing and Rushing, none of which exist in the capture schema. Three further layout facts came out of real data, including that 60 of 98 stat cells in a real game are zero, which forced a zero-dimming rule.

**Status: PARTIALLY CLOSED.** Elo ratings, queue wait times and player counts on Home, Online and the lobby remain invented, because no online system exists to export them from.

---

### V5. Contrast and layout claims are computed, not observed

**Guardrail safeguard:** "Review screenshots at actual target resolutions, not only zoomed design canvases" and "Run contrast and text-size checks."

**Finding:** every contrast ratio in [`DESIGN_TOKENS.md`](./DESIGN_TOKENS.md) and [`CONTROLLER_AND_ACCESSIBILITY_AUDIT.md`](./CONTROLLER_AND_ACCESSIBILITY_AUDIT.md) is computed from hex values. No screen has been rendered or measured. The audit says this, but the numbers still read as verified because they are precise.

**Severity:** medium. Precise unverified numbers are worse than approximate honest ones.

**Resolution:** the audit's Outstanding section already lists it first. It cannot be closed without a build.

**Status: OPEN, blocked on implementation.**

---

## Guardrails passed, with the mechanism named

A pass is only meaningful if something structurally prevents the failure. "We will not do that" is not a mechanism.

| Prohibited outcome | Prevented by | Mechanism, not intent |
|---|---|---|
| Generic SaaS dashboard composition | Six page templates by task type | Play, Online, Settings, Stats, Music, and Mods have different structures, not one layout with different content |
| Excessive glassmorphism, blur, glow | `blur-overlay` token restricted to artwork and transient overlays | Settings, forms, and tables have no token that permits blur |
| Huge headings, little information above the fold | `text-3xl` restricted to hero titles; measure capped at 40 characters for headings | A heading physically cannot dominate a settings page |
| Default framework styling as final | Full token system authored before any component | There is no framework default to fall back to |
| Inconsistent radii, shadows, spacing, icon weights | 5 radius tokens, 5 elevation tokens, 8 spacing tokens | Any ad hoc value is visibly not from the set |
| Random gradient accents | Exactly one gradient token exists | `welcome-gradient` is the only one; a second would need a spec change |
| Every metric a decorative card | `space-6` group separation instead of cards | Settings and Stats group with spacing, per moodboard region D |
| Emoji as product iconography | Not permitted anywhere in the language guide | Zero emoji in any specified copy |
| Decorative charts | Stats spec requires charts to support a decision | Headline numerals carry the information; charts show trend only |
| Desktop as enlarged mobile | Two type scales plus per-breakpoint item counts | The same page differs in item count, not only scale |
| Hover-only interactions | No hover-only token; hover shares `surface-3` with focus | Anything reachable by hover is reachable by focus |
| Focus as a slight color change | Three-signal focus, 3 px two-tone ring plus surface plus scale | Specified as a composite, not a color |
| Raw Dolphin vocabulary in ordinary flows | Design principle 10 with a testable check | Search a screen's copy for Dolphin terms; zero outside Settings and About |
| Copy that sounds like an AI assistant | Banned-phrase list plus four-part error structure | Every error names what stays safe, which no generated copy does |
| Stock placeholder sports photography | Product art only | No stock imagery specified anywhere |

---

## Moodboard fidelity test

The brief's seven questions, answered honestly.

**1. Does the shell have Steam or Playnite-like clarity and useful density?**
Yes. Persistent rail, top bar with search and status, dense settings, and a bottom hint strip. Density is real at desktop and deliberately reduced at Deck.

**2. Does Play Now feel cinematic and game-first rather than dashboard-first?**
Yes. Full-bleed hero, foreground panel, one dominant action, status as chips rather than forms. It is the closest thing in the product to a console detail page, which is what moodboard regions B and I both point at.

**3. Does onboarding feel guided and conversational like Discord without copying Discord?**
**Now yes.** The structure was Discord's; the selection mark and the layout rule have both changed, and both changed for functional reasons that hold on their own.

**4. Are settings compact, calm, and deliberate?**
Yes. Category rail, always-visible descriptions, inline controls, restrained color, group separation by spacing.

**5. Are statistics useful first and decorative second?**
Yes. Headline numerals, tabular figures, tables that stay tables. Emphasis by weight rather than by highlight fill. Teal is the only accent and it is reserved for data.

**6. Does the design retain a consistent Reborn identity across all five modes?**
**Partially.** Color, type, spacing, and focus behavior are consistent by construction. But the identity currently rests almost entirely on red plus near-black, and red plus near-black is not distinctive on its own. **The identity gap is the icon system (V3) and the absence of any Reborn-specific visual motif.** The logo's graffiti energy appears nowhere in the interface language.

**7. Would a user mistake this for a generic web template or AI-generated launcher?**
**Not yet answerable, and pretending otherwise would be the failure this review exists to catch.** The specifications are specific and unusual in places (the two-accent rule, the three-signal focus, the four-part error copy, six templates). But nothing has been rendered. The brief says if question 7 could plausibly be answered yes, the direction is not acceptable. At specification stage the honest answer is that it depends entirely on execution of V1, V3, and V6 below.

---

## V6. The identity gap, raised by question 6

Not in the brief's prohibited list, but it is the thing that decides question 7.

**Finding:** the design system as specified would produce a competent, accessible, dark launcher. It would not produce something recognizably NFL Street. Red plus near-black plus Inter plus 8 px radii is a good system and a generic one.

The blueprint says "NFL Street personality should come from art, motion, texture, typography, and content rather than covering every surface in graffiti." That is correct, and it has been followed so completely that **no personality mechanism has been specified at all.**

**Recommendation, one motif, used sparingly:**

The logo's `R` has a hard diagonal shear. That angle is the only distinctive geometry Reborn owns. Proposed uses, all functional rather than decorative:

- The **focus and selection left-edge bar** takes that shear rather than being a rectangle.
- **Section dividers** in Settings use a single sheared terminal rather than a plain rule.
- The **progress fill** leading edge takes the angle.
- **Hero scrims** take a subtle diagonal rather than a straight vertical gradient.

Four small places, none of them decoration, all of them repeating one angle that comes from the logo. That is a motif rather than a texture, and it survives the "not covering every surface in graffiti" constraint.

**Status: CLOSED.** Adam approved the shear motif. Specified in [`DESIGN_TOKENS.md`](./DESIGN_TOKENS.md) section 11 at 12 degrees, with the four permitted uses and four explicit prohibitions. Unverified on a rendered screen.

---

## Verdict

**Updated August 29, 2026 after Adam's decisions.**

| # | Item | Status |
|---|---|---|
| V1 | Option cards too close to Discord | **CLOSED.** Sheared left edge replaces the corner badge; layout is width-driven |
| V2 | Home close to the generic hero-plus-rails shape | Open, low severity, recommendation made |
| V3 | Icon system | **CLOSED** with recorded risk. Stock set, mitigated |
| V4 | Invented data | **PARTIALLY CLOSED.** Box score rebuilt from real captures. Elo and queue figures remain invented |
| V5 | Contrast computed, not measured | Open, blocked on implementation |
| V6 | No Reborn motif | **CLOSED.** Shear at 12 degrees, four uses, four prohibitions |

**No blockers remain for high-fidelity design.**

V2 is a low-severity recommendation. V5 cannot close without a build and is listed first in the audit's outstanding items. Everything else is closed.

**The brief still requires human review before generated design work is treated as approved. This document is the request for that review, not a substitute for it.**

The brief requires human review before treating generated design work as approved. **This document is the request for that review, not a substitute for it.**

---

## What Adam needs to decide

All three questions from the first version of this review are answered:
motif yes, stock icons, real data approved and pulled.

**What is left:**

1. **V1, the option cards.** I proposed two changes: single-column stacking below 900 px instead of always gridding, and moving the check badge from a corner mark to a filled left edge, which also reads better at 10 feet. Both are functional improvements rather than cosmetic distancing. Approve, or say if you would rather they stay as they are and accept the resemblance.
2. **A question the real data raised, not a design question.** Michael Penix Jr. is recorded with 2 sacks and 5 interceptions thrown, and Bijan Robinson with 1 sack, in a real capture. Quarterbacks recording sacks suggests the tracker may be attributing defensive plays by roster slot rather than by player. Worth checking before these numbers reach a public profile.
