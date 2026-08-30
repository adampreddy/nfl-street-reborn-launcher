# Controller, Responsive, and Accessibility Audit

## NFL Street Reborn Launcher

**Date:** August 29, 2026
**Covers brief deliverable:** 9 (responsive and controller audit, plus the accessibility audit named in the final handoff)
**Depends on:** [`WIREFRAMES.md`](./WIREFRAMES.md), [`DESIGN_TOKENS.md`](./DESIGN_TOKENS.md), [`COMPONENT_SPECIFICATION.md`](./COMPONENT_SPECIFICATION.md)

### What this audit is and is not

**Is:** a specification audit. Every design in the wireframes and component spec, checked against the seven required resolutions, a complete controller focus map, and WCAG 2.2 AA plus the game accessibility baselines.

**Is not:** a measurement of rendered screens. Nothing is implemented, so every contrast figure here is computed from token values rather than sampled from a running application. The brief requires reviewing screenshots at actual target resolutions and running contrast checks; that pass cannot happen until there is a build. It is listed as the first item under Outstanding.

---

## 1. Resolution matrix

The brief names seven targets. Each is checked for the failure that actually threatens it.

| Target | Resolution | Breakpoint | Verdict | Specific risk |
|---|---|---|---|---|
| Desktop 1080p | 1920x1080 | `bp-wide` | Pass | Play page foreground panel plus hero at 42% leaves 580 px for content. Adequate |
| Desktop 1440p | 2560x1440 | `bp-ultra` | Pass with a rule | Content caps at 2200 px and centers. Without the cap, settings descriptions run 200 characters |
| Ultrawide | 3440x1440 | `bp-ultra` | Pass with a rule | Same cap. Hero art needs a 21:9 crop or it letterboxes |
| Steam Deck | 1280x800 | `bp-deck` | **Pass, tightest case** | See section 2 |
| Small laptop window | 1100x700 | `bp-compact` | Pass with degradation | Icon-only rail, single content column, hero drops to a static banner |
| Windows high DPI | 1920x1080 at 150% | effective 1280x720 | **Risk** | See below |
| macOS Retina | 2x scaling | effective points | Pass | Token values are in logical pixels, so 2x is transparent |

### The Windows scaling risk

Windows at 150% on a 1080p display gives an effective 1280x720 viewport. That is 80 px shorter than Steam Deck, and it is a very common laptop configuration.

**Consequence:** the Settings page at `bp-deck` density shows about eight rows in 800 px. At 720 px it shows six. The Play page hero at 55% leaves only 324 px for the foreground panel, which is 40 px short of what the panel needs at the 10-foot type scale.

**Resolution:** hero height becomes `min(55%, viewport - 380px)` rather than a flat percentage. Below 760 px of viewport height the hero is clamped so the panel always fits. Added to the token spec as a constraint on the Play template.

**This is the only genuine layout failure found in the audit**, and it was found by treating a scaling factor as a resolution rather than as a zoom.

---

## 2. Steam Deck, the tightest case

1280x800 with the 10-foot type scale and 44 px targets is the binding constraint on the whole design. Worked through explicitly:

```
800 px vertical budget
 -  56  top bar
 -  40  hint strip
 =  704 available to content
```

| Page | Fits | Notes |
|---|---|---|
| Home | Yes | Hero 55% of 704 = 387 px, leaves 317 px for Play Now plus one rail. News drops below the fold, acceptable |
| Play | Yes | Hero clamped, panel 380 px |
| Online | Yes | Three mode cards at 160 px, preflight at 120 px, recent opponents below the fold |
| Mods | Yes | Two columns, one rail visible, second requires scroll |
| Music editor | **Tight** | Waveform, three sliders, three metadata fields, and the rebuild banner exceed 704 px. Resolution: metadata collapses into a secondary sheet at `bp-deck` |
| Stats box score | **Tight** | Player table needs horizontal scroll with a pinned name column |
| Settings | Yes | Two panes, about eight rows |
| Onboarding | Yes | Designed as a full stage with no chrome, so it has the full 800 px |

**Two resolutions required and specified:** the music editor moves metadata into a sheet, and the box score pins the name column and scrolls horizontally rather than shrinking type. Both are recorded in the wireframes.

---

## 3. Complete controller focus map

The brief requires the complete focus order and focus-return behavior for every primary flow.

### Global

| Input | Everywhere |
|---|---|
| A | Activate focused |
| B | Up one level. Focus returns to the originating item |
| LB / RB | Previous, next section. Focus lands on that section's default |
| LT / RT | Previous, next page within the section |
| View | Search. B returns focus to the exact element that was focused |
| Menu | Overflow for the focused item |

### Per flow

**Home**
`Play Now` (default) → hero carousel (one stop, left and right inside) → profile card → last match → mods rail (one stop) → news list → rail (via left).
Return from any L3: focus lands on the originating item.

**Play**
`Play Now` (default) → graphics chip → controller chip → mods chip → Verify and repair → Open Dolphin → rail.
Selecting a chip navigates to Settings. Pressing B there returns to Play with focus back on that chip, not on Play Now.

**Online**
`Ranked` (default) → Casual → Private → preflight items (only the failing ones are focusable) → Test network → recent opponents → rail.
Rationale: passing preflight checks are not focusable because focusing something that does nothing wastes a stop.

**Queue → lobby**
Queue screen focuses `Leave queue`, the only action. On match found, focus moves to `Ready` after a 500 ms delay so a held button cannot accidentally ready you up. That delay is a real safeguard, not polish.

**Mods**
Tab row → hero → first card of first rail → within a rail left and right, between rails up and down → rail.
Opening a package sheet focuses `Install`. Closing returns to the card.

**Music editor**
Slot list → waveform (left and right scrub, LB and RB by 10 seconds) → Preview → trim start → trim end → fade in → fade out → volume → normalize → metadata fields → `Rebuild game`.
Text fields open the on-screen keyboard on Deck; B closes it without committing, A commits.

**Settings**
Category rail → first row → down through rows → per-group reset at the end of each group.
LT and RT move between categories without entering the content pane, which is the fast path.

**Onboarding**
Each step focuses its primary action. Option-card steps focus the recommended card. B goes to the previous step only if reversible; on irreversible steps the hint strip omits Back rather than showing a dead button.

**Crash recovery**
Focuses the most likely fix. After three same-signature crashes the order changes so safe mode leads.

**Xbox wizard**
Drive picker focuses **nothing preselected**. The first drive is focused but not selected. Confirm screen focuses `Back`, not `Copy to drive`, because the destructive action should never be one A-press from arrival.

### Focus rules verified across all flows

| Rule | Status |
|---|---|
| Every screen has a defined default focus | Pass, all 17 |
| Default focus is the safest or most likely action, never index 0 by accident | Pass |
| No focus trap. B exits every container | Pass |
| Focus restores on back navigation | Pass, specified per flow |
| Focus never lands on a non-interactive element | Pass |
| Focus never disappears after a modal closes | Pass |
| Horizontal rails are one stop until entered | Pass |
| Offscreen content is signaled | Pass, partial card plus gradient |
| Destructive actions are never default focus | Pass, Xbox confirm and all confirm dialogs focus Cancel |

---

## 4. WCAG 2.2 AA conformance

Checked against the criteria that actually bind a dark, controller-first desktop application.

| Criterion | Level | Status | Evidence |
|---|---|---|---|
| 1.4.3 Contrast (Minimum) | AA | Pass | `text-primary` ~16:1, `text-secondary` ~7.4:1, `text-tertiary` ~4.6:1 on `surface-1` |
| 1.4.11 Non-text Contrast | AA | Pass | `border-default` and focus ring both clear 3:1 |
| 1.4.1 Use of Color | A | Pass | Every status token carries a required second signal |
| 1.4.4 Resize Text | AA | Pass | Two type scales, logical-pixel tokens |
| 1.4.10 Reflow | AA | Pass | Reflow by breakpoint, not horizontal scroll, except the box score table which is a data table and exempt |
| 1.4.12 Text Spacing | AA | Pass | No fixed-height text containers specified |
| 2.1.1 Keyboard | A | Pass | Every controller action has a keyboard equivalent |
| 2.1.2 No Keyboard Trap | A | Pass | Section 3 |
| 2.4.3 Focus Order | A | Pass | Section 3 |
| 2.4.7 Focus Visible | AA | Pass | Three-signal focus |
| 2.4.11 Focus Appearance | AA | Pass | 3 px ring, exceeds the 2 px perimeter minimum, 3:1 via two-tone |
| 2.5.8 Target Size (Minimum) | AA | **Exceeded** | 44 px, the AAA value, on controller-reachable controls |
| 2.2.1 Timing Adjustable | A | **Risk** | See below |
| 3.3.1 Error Identification | A | Pass | Four-part error copy |
| 3.3.3 Error Suggestion | AA | Pass | Every error names a next action |
| 3.3.4 Error Prevention | AA | Pass | Hold-to-confirm plus explicit "will not" statements |
| 4.1.3 Status Messages | AA | Pass | Toasts and banners specified as live regions |

### The timing risk

Two places impose time limits that a user cannot extend:

1. **Lobby ready countdown.** A fixed countdown before a ranked match.
2. **Toast auto-dismiss at 5 seconds.**

**Toasts are fine** because they never carry the only copy of information; a failure always also marks its object.

**The lobby countdown is a real 2.2.1 concern.** It is arguably exempt under the real-time-event exception, since both players are waiting and the match is a synchronized event. But the honest position is that it is an exception being relied on, not a criterion being met.

**Mitigation specified:** the countdown is generous, at least 20 seconds, is announced at 10 and 5 seconds, and readying is a single A-press from default focus. A player who cannot act in 20 seconds is not excluded from the product, only from ranked queueing, and casual and private have no countdown.

---

## 5. Game accessibility baselines

From the IGDA and AbleGamers material in the research.

| Baseline | Status |
|---|---|
| Remappable controls | Pass, full remapping in Settings |
| Control sensitivity adjustment | Pass |
| Axis inversion | **Available via remapping, no preset.** Adam's decision |
| Left-handed mode | **Available via remapping, no preset.** Adam's decision |
| Dead zone configuration | Pass |
| Trigger threshold | Pass |
| Text contrast 4.5:1 | Pass |
| UI scalable to 200% | **Partial.** Two type scales give roughly 115%, not 200% |
| Never color alone | Pass |
| Colorblind mode | **Not required.** See below |
| Respect reduced motion | Pass |
| High contrast mode | Pass |
| Subtitles and captions | Not applicable, the launcher has no speech |

### Two honest gaps

**UI scaling to 200% is not met.** The design has two type scales, desktop and 10-foot, roughly a 1.15 multiplier. A user needing 200% text is not served. Meeting it properly means a third scale plus reflow testing at that scale, which is real work. **Recorded as a gap, not claimed as passing.**

**A colorblind mode is not required, and that is a deliberate claim rather than an omission.** The "never color alone" rule is applied universally: every status has an icon or a word, every result has a letter, every chart series has a direct label. If that rule holds, a separate colorblind mode has nothing left to fix. The rule must be verified in the implementation audit; if any status ships as color-only, this claim fails and a mode becomes necessary.

---

## 6. Screen reader support

| Element | Behavior |
|---|---|
| Nav items | Role, label, current-section state |
| Buttons | Label plus loading state ("Installing, 62 percent") |
| Option cards | Radio or checkbox semantics, selected state, group label |
| Progress | Live region, announcing at 25% intervals rather than continuously |
| Toasts | Polite live region |
| Errors | Assertive live region |
| Controller tester | Paired text log, since the diagram is inherently visual |
| Box score table | Proper table semantics with row and column headers |
| Hold-to-confirm | Announces "hold to confirm" plus a non-hold alternative |

**Platform reality:** Tauri uses the system webview, so this is standard web accessibility on Windows and macOS. On Linux, webview accessibility is weaker and inconsistent across distributions. **That is a known limitation and should be stated in release notes rather than discovered by a user.**

---

## 7. Findings summary

| # | Finding | Severity | Resolution |
|---|---|---|---|
| 1 | Windows 150% scaling gives 720 px height; Play panel does not fit | **High** | Hero clamped to `min(55%, viewport - 380px)` |
| 2 | Music editor exceeds Deck vertical budget | Medium | Metadata moves to a sheet at `bp-deck` |
| 3 | Box score table too wide for Deck | Medium | Pinned name column, horizontal scroll |
| 4 | Lobby countdown cannot be extended | Medium | Generous duration, announced, single-press ready. Exception relied on and disclosed |
| 5 | UI scaling reaches ~115%, not 200% | Medium | **Open gap.** Needs a third scale |
| 6 | Linux screen reader support is weak | Low | Platform limitation, disclose in release notes |
| 7 | Focus-selects on option cards would break browsing | Resolved in spec | Selection requires A |
| 8 | Ready-up could be triggered by a held button | Resolved in spec | 500 ms delay before focus moves to Ready |

Findings 1 through 3 were found by working the layouts against real pixel budgets rather than by inspection, which is why they are specific.

---

## Outstanding

1. **Nothing has been measured on a rendered screen.** Every contrast figure is computed from tokens. The brief requires screenshots at real resolutions and real contrast checks; that requires a build.
2. **No controller has been tested**, because there is nothing to test. The focus map is a specification.
3. **No Steam Deck or Mac exists** to verify the tightest cases on.
4. **200% text scaling is an open gap**, not a passing criterion.
