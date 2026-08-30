# Component Specification

## NFL Street Reborn Launcher

**Date:** August 29, 2026
**Covers brief deliverable:** 7 (component specification)
**Depends on:** [`DESIGN_TOKENS.md`](./DESIGN_TOKENS.md), [`INFORMATION_ARCHITECTURE.md`](./INFORMATION_ARCHITECTURE.md)

The brief requires anatomy and states for every component, with mouse, keyboard, controller, disabled, loading, error, and high-contrast behavior where applicable.

---

## 0. Rules that apply to everything

These are stated once here rather than repeated in every component.

**Focus versus selection are different and look different.**

| | Appearance |
|---|---|
| Focus | `focus-ring-outer` 3 px white with 1 px black inset, 2 px offset, plus lift to `surface-3`, plus 1.02 scale, `motion-instant` |
| Selection | 4 px sheared `red-500` left edge, `red-500` at 12% fill, `border-strong` |
| Both | Both render. The ring sits outside the selection border |

**One state layer at a time**, from the Material 3 research. If an element is focused and then hovered, hover shows until the pointer leaves, then focus returns. They never composite into a third appearance.

**Every interactive element is at least 44x44** if a controller can reach it. Mouse-only affordances may be 24 px.

**Disabled is never the only signal.** A disabled control always carries a reason nearby, as text. "Queue" disabled is accompanied by "Your online game needs repair," not a silent gray button.

**High contrast mode:** all `surface-*` collapse to `#000000`, all borders become `#FFFFFF` at 1 px minimum, `text-secondary` and `text-tertiary` promote to `text-primary`, and the focus ring becomes 4 px solid `#FFFF00`. Brand red remains for fills but gains a 2 px white border.

**Reduced motion:** all transitions to 0 ms, hero parallax off, progress bars still animate because they convey rate, spinners become a static indeterminate bar.

---

## 1. Buttons

### Anatomy

```text
┌──────────────────────────────┐
│  [icon]  Label      [badge]  │   height 44 (48 for hero)
└──────────────────────────────┘
   12px    label     8px
```

Padding `space-3` horizontal, icon at 20 px, gap `space-2`, `radius-md`.

### Variants

| Variant | Rest | Hover | Pressed | Focused | Disabled |
|---|---|---|---|---|---|
| **Primary** | `red-500` fill, `#FFF` label | `red-400` | `red-600`, 0.98 scale | + ring | `surface-3` fill, `text-disabled` |
| **Secondary** | `surface-2` fill, `border-default`, `text-primary` | `surface-3` | `surface-2`, 0.98 | + ring | `surface-2`, `text-disabled`, `border-subtle` |
| **Tertiary** | transparent, `text-secondary` | `surface-2` fill | `surface-1` | + ring | `text-disabled` |
| **Destructive** | transparent, `error` label, `error` border | `error` at 12% fill | `error` at 20% | + ring | `text-disabled` |

**Destructive is never a filled button.** Per the token rules, filled red is reserved for primary actions. A destructive action is outlined red with an icon, so it can never be mistaken for the primary at a glance.

### States

- **Loading:** label is replaced by a spinner plus the present-participle label ("Installing"). Width is held so the layout does not jump. Not interactive, but focus is retained.
- **Error:** buttons do not carry error state. The surrounding context does.
- **Keyboard:** Enter and Space activate. Tab moves.
- **Controller:** A activates. Never bound to X or Y if destructive.

### Hold-to-confirm

Destructive buttons in a confirm dialog use hold-to-confirm. 800 ms, a ring fills clockwise around the button, releasing early cancels and the ring drains in `motion-fast`. Screen readers announce "hold to confirm." Keyboard equivalent is holding Enter. There is always a visible alternative for users who cannot hold, a two-step "Type DELETE" style confirm reached from the overflow.

---

## 2. Navigation item

```text
┌────────────────────┐
│ [icon]  Label   ● │   height 44 desktop, 52 deck
└────────────────────┘
```

The dot is an activity indicator, present only when that section has something pending (a downloading mod, an update, a held match).

| State | Appearance |
|---|---|
| Rest | `text-secondary`, transparent |
| Hover | `surface-2` fill |
| Active (current section) | `surface-2` fill, `text-primary`, 3 px `red-500` left bar |
| Focused | + ring |
| Active and focused | left bar plus ring, both visible |

The red left bar is the one place brand red marks state rather than an action. It is acceptable because it is a location marker, not a status, and it never coexists with error red in the rail.

**Compact rail:** icon only. The label appears as a tooltip on hover **and** as an always-visible label on focus, since focus must never depend on hover.

---

## 3. Hero action (Play)

The single most important control in the product.

```text
╔══════════════════════════════════╗
║   ▶   Play Now                   ║   height 48, min-width 220
╚══════════════════════════════════╝
```

Primary variant at `control-height-lg`, `text-base` label, play glyph at 24 px. Sits on hero artwork over `scrim-hero`, never on bare art.

**States:**

| Condition | Label | Variant |
|---|---|---|
| Ready | Play Now | Primary |
| First run after update | Play Now, with a subtitle "First start after an update takes a moment" | Primary |
| Build invalid | Repair and Play | Primary, warning icon |
| Runtime missing | Install runtime, 240 MB | Primary |
| Launching | Starting, spinner | Loading |
| Game running | Game running, secondary action becomes "Bring to front" | Disabled with reason |

Default focus on both Home and Play. Never loses focus to a carousel.

---

## 4. Option card

The onboarding and mode-selection primitive.

**Origin and how it differs.** The first draft of this component was moodboard
region C, which is a screenshot of Discord's server onboarding, with the colors
changed. The anti-vibecoding review flagged that as violation V1. Two changes
were made, both of which are functional improvements rather than cosmetic
distancing:

1. **The selection mark is a filled left edge, not a corner badge.** A small
   corner checkmark is the weakest possible selection signal at television and
   Steam Deck distance. A full-height left edge is visible from across a room,
   and it carries the Reborn shear from `DESIGN_TOKENS.md` section 11, which a
   corner badge cannot.
2. **Layout is width-driven, not fixed to a grid.** Below 900 px of content
   width the group stacks to a single column instead of forcing a two-by-two
   grid into a space that cannot hold it. That matters because Steam Deck at
   1280 px with a 200 px rail leaves under 1000 px, and Windows at 150%
   scaling leaves less.

### Anatomy

```text
╱┌──────────────────────────────────┐
╱│ [icon]  Title                    │
╱│         One or two lines of      │   min-height 88
╱│         supporting text.         │
╱└──────────────────────────────────┘
 ↑
 sheared selection edge, 4px, red-500, present only when selected
```

`radius-lg`, `surface-2`, `border-default`, padding `space-4`. Icon 28 px,
top-aligned. The selection edge sits outside the card's left border so it does
not eat into the content box.

### States

| State | Appearance |
|---|---|
| Rest | `surface-2`, `border-default` |
| Hover | `surface-3` |
| Focused | + neutral two-tone ring, 1.02 scale |
| Selected | 4 px sheared `red-500` left edge, `red-500` 12% fill, `border-strong` |
| Selected and focused | both. The ring sits outside the selection edge |
| Disabled | `surface-1`, `text-disabled`, reason shown below the card |

**Three signals for selection**, edge plus fill plus border weight, which
satisfies "never color alone" and stays readable in grayscale.

### Layout

| Content width | Arrangement |
|---|---|
| 900 px and above | Two columns, up to four cards |
| Below 900 px | Single column stack, full width |
| Any width, 2 cards | Side by side above 900 px, stacked below |

Never more than four cards. Beyond four it becomes a list, because a controller
user cannot hold more than four spatial positions without hunting.

### Behavior

**Controller:** D-pad or stick moves between cards, A selects. In single-select
groups **selection never follows focus**; the user must press A. Focus-selects
makes it impossible to browse options without committing to each one in turn.

**Keyboard:** arrow keys move within the group, Space or Enter selects, Tab
leaves the group. Radio-group semantics for single select, checkbox semantics
for multi.

**Screen reader:** announces group label, position, and selected state.

---

## 5. Content card (mods, music packs)

```text
┌────────────────────────────┐
│                            │
│        artwork 16:9        │
│                            │
├────────────────────────────┤
│ Title                      │
│ by Creator      [Offline]  │
│ ┌──────────┐  142 MB       │
│ │ Install  │               │
│ └──────────┘               │
└────────────────────────────┘
```

`radius-md`, `surface-2`, artwork flush to the card edges, content padded `space-3`.

**The `[Offline]` chip is mandatory on music packs** and marks them as offline-only. This is the highest-value single label in the product, because "why did my music not work online" is the most predictable support question.

| State | Appearance |
|---|---|
| Rest | `surface-2`, `border-default` |
| Hover | `surface-3`, `elev-2` |
| Focused | + ring, 1.02 scale, `elev-2` |
| Installed | Install becomes Installed, check icon, secondary variant |
| Update available | Install becomes Update, `warning` dot on artwork |
| Installing | Progress bar replaces the button, percentage and rate below |
| Failed | `error` left border 3 px, message replaces description, Retry replaces Install |
| Conflicts | `warning` chip, detail in the sheet |

**Loading skeleton:** artwork block, two text bars, one button block, all `surface-2` with a slow shimmer. Shimmer is disabled under reduced motion.

**Density:** at `bp-deck` a card shows at most two metadata chips and drops the description to one line, per the density table.

---

## 6. Progress and download

Three forms, used in different places.

**Determinate bar.** 4 px, `radius-full`, `surface-3` track, `teal-500` fill. Used inline on cards and in the downloads popover. Always paired with a percentage and, when known, a rate and a time remaining.

**Stepped progress.** Used in onboarding preparation and the Xbox wizard. A vertical list of named tasks with per-task states.

```text
✓  Verify game source            done
✓  Build offline game            done
◐  Install Reborn Dolphin        62%, 40 s left
○  Apply default textures        waiting
○  Verify final hashes           waiting
```

**Named tasks, not raw console output**, per the blueprint. A failed task shows `error` with a reason and a Retry that resumes rather than restarts.

**Indeterminate.** A 2 px bar with a traveling highlight. Used only where progress genuinely cannot be known. Under reduced motion it becomes a static bar with the label "Working."

**Never a spinner alone with no label.** Every busy state names what it is doing.

---

## 7. Controller row and live input tester

The controller row:

```text
┌──────────────────────────────────────────────────────────┐
│ [glyph] DualSense Wireless Controller                    │
│         USB · Player 1 · Battery 84%                     │
│                          [Test]  [Remap]  [⋯]           │
└──────────────────────────────────────────────────────────┘
```

Glyph reflects the detected family and drives the prompt pack. Connection type is stated because USB and Bluetooth behave differently and the blueprint's acceptance matrix tests them separately.

| State | Appearance |
|---|---|
| Connected | `success` dot, full opacity |
| Connected, unassigned | `warning` dot, "Not assigned to a player" |
| Disconnected | `text-disabled`, hollow dot, "Reconnect or remove" |
| Conflict detected | `warning` left border, "Steam Input may be duplicating this controller" plus a Learn more |

**Live input tester.** A controller diagram with every input highlighted as it is pressed, plus stick position dots and analog trigger bars.

- Pressing a button lights its glyph `teal-500` for 200 ms.
- Sticks show a live dot within a circle, plus the numeric deadzone boundary.
- Triggers show a filled bar with the current value.
- A "nothing detected" state after 5 seconds of silence, with troubleshooting.

**Accessibility:** the tester cannot be the only way to verify a controller, since it is inherently visual. It is paired with a text log ("A pressed. Left stick moved.") that a screen reader announces.

---

## 8. Settings controls

All rows share this anatomy:

```text
┌────────────────────────────────────────────────────────┐
│ Label                                    [ control ]   │
│ Plain-language description of what this does.          │
│ Currently 3x. Recommended for your PC: 3x.             │
│ ⟳ Changing this rebuilds your offline game, ~4 min.    │
└────────────────────────────────────────────────────────┘
```

Row height 44 minimum, padding `space-3`, separated by `border-subtle`, grouped with `space-6`.

**The description is always visible, never a tooltip.** This is the Steam Deck rule and the blueprint's requirement for advanced Dolphin settings, in one component.

**The cost line** (`⟳`) appears only when changing the setting requires a restart or a rebuild. It states the time.

| Control | Use | Focus behavior |
|---|---|---|
| **Toggle** | Boolean | 44x24 track, thumb slides `motion-fast`. Focus ring around the whole row |
| **Segmented** | 2 to 5 options | Ring around the group; left and right move within, A commits |
| **Select** | 6 or more | Opens a sheet at `bp-deck`, a popover at desktop. Never a native select |
| **Slider** | Continuous | Ring around the track. Left and right step, LB and RB step by 10. Value shown numerically and editable as text |
| **Keybind** | Single binding | Press A to enter capture, next input is captured, B cancels. Shows a conflict warning inline if already bound |
| **Path** | Storage location | Read-only field plus Browse. Shows free space |

**Slider rule:** every slider has a numeric readout that is itself editable. Sticks are imprecise and a slider alone makes exact values impossible with a controller.

---

## 9. Match card and stats table

**Match card:**

```text
┌──────────────────────────────────────────────────────┐
│ W  │ EAGLES  34 - 21  GIANTS  │ Ranked  +18  │ 2h ago│
└──────────────────────────────────────────────────────┘
```

Left edge is a 4 px result bar, `success` for a win and `error` for a loss, and the W or L letter is present so grayscale still reads. Mode is a labeled chip. Rating delta is signed and colored, with the sign carrying the meaning.

| State | Appearance |
|---|---|
| Complete, verified | as above |
| Held for review | `warning` bar, "Held for review" replaces the delta |
| Incomplete | `text-tertiary`, "Did not finish" |
| Offline | slate bar, "Offline" chip, no rating delta |

**Stats table:** `text-sm`, tabular numerals mandatory, `border-subtle` row rules, no zebra striping. Sortable headers are mouse and keyboard targets at 24 px; on controller, sort is in the overflow rather than by focusing headers, because traversing a header row to sort is slow.

Sticky header on scroll. Numeric columns right-aligned. The leader in each column gets `text-primary` weight 600 while others stay `text-secondary`, which is a cheaper and more readable emphasis than a highlight fill.

---

## 10. Feedback surfaces

| Surface | Duration | Dismissible | Use |
|---|---|---|---|
| **Toast** | 5 s | yes | Confirming something the user just did |
| **Banner** | until resolved | no | A condition affecting the page, e.g. Online is unavailable |
| **Inline** | until resolved | no | A problem with a specific object |
| **Dialog** | until answered | yes | A decision |
| **Blocking** | until resolved | no | Required update, first-run preparation |

**Toasts never carry background-work completion.** That updates the indicator and the object instead. Toasts are for "Mod installed," "Copied," "Queue left."

Toasts stack to a maximum of three, oldest dropping. They appear bottom-right at desktop and bottom-center at `bp-deck`. They are never the only notification of a failure; a failure also marks its object.

**Blocking update screen:**

```text
              [NSR logo]

        Ranked needs version 2.4

   ████████████████░░░░░░░░  68%
   142 MB of 210 MB · about 40 seconds

   Offline play, mods, and music still work.
             [ Play offline instead ]
```

Even the blocking screen offers a route out to the thing that still works. That is the offline-first requirement holding under the worst case.

---

## 11. Empty, offline, loading, failed, recovery

The brief requires these designed, not left as implementation afterthoughts.

**Empty.** An icon at 48 px in `text-tertiary`, a one-line statement of what would be here, and one action.
Good: "No matches yet. Play a game and it will show up here." plus Play Now.
Bad: an illustration and "Nothing to see!"

**Offline.** Never a blank page. The page renders from cache with a banner naming what is unavailable and what still works. Stale content is labeled with when it was last updated.

**Loading.** Skeletons matching the final layout, never a centered spinner on a blank page. Skeletons hold the exact dimensions of the real content so nothing shifts.

**Failed.** `error` icon, what failed, why, **what remains safe**, and one or two actions. The four-part rule from the language guide, rendered.

```text
  ⊗  Couldn't install Purple Football

     The download finished but the file didn't match
     its checksum, so nothing was changed.

     Your game, saves, and other mods are untouched.

     [ Try again ]   [ Report this ]
```

**Recovery (after a crash).** A dedicated page, not a dialog, with six actions from the blueprint: Relaunch Game, Verify and Repair, Start with Mods Disabled, Start with Performance Graphics, Open NFL Street Reborn Dolphin, Create Diagnostic Bundle. Plain-language likely cause. Never auto-restarts. After three crashes with the same signature it leads with Start with Mods Disabled instead of Relaunch.

---

## 12. Coverage check

| Brief requirement | Section |
|---|---|
| Primary, secondary, destructive buttons | 1 |
| Navigation items | 2 |
| Hero actions | 3 |
| Content cards | 5 |
| Option cards | 4 |
| Mod and music cards | 5 |
| Download and progress states | 6 |
| Controller rows and live input tester | 7 |
| Toggles, sliders, selects, keybinds, advanced rows | 8 |
| Match cards and statistics tables | 9 |
| Empty, offline, loading, failed, recovery | 11 |
| Toasts, banners, dialogs, blocking screens | 10 |
| Mouse, keyboard, controller, disabled, loading, error, high contrast | 0 plus per component |

---

## Open items

1. **Icon set is unchosen.** The brief prohibits generic Lucide icons as the entire identity system. Recommendation is a licensed or drawn set for the twelve highest-traffic glyphs (play, repair, install, controller, music, stats, ranked, casual, private, online, warning, error) with a conventional set beneath them.
2. **Controller diagram artwork** for the live tester needs to exist per device family: Xbox, DualShock 4, DualSense, Switch Pro, GameCube, generic.
3. **Hold-to-confirm needs a non-hold alternative** designed, not just referenced.
4. Nothing here is implemented.
