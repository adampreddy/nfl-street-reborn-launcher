# Information Architecture

## NFL Street Reborn Launcher

**Date:** August 29, 2026
**Covers brief deliverable:** 5 (information architecture)
**Depends on:** [`UX_RESEARCH_DOSSIER.md`](./UX_RESEARCH_DOSSIER.md), [`DESIGN_TOKENS.md`](./DESIGN_TOKENS.md)

The brief asks for navigation, hierarchy, back behavior, search, modal behavior, background-state visibility, controller shortcuts, and deep links across nine destinations. Each is specified below.

---

## 1. The shell

```text
┌────────────────────────────────────────────────────────────────────────┐
│ [NSR]   <page title>              [search]   [server]  [downloads] [me]│  56px
├──────────┬─────────────────────────────────────────────────────────────┤
│ Home     │                                                             │
│ Play     │                                                             │
│ Online   │                  active page content                        │
│ Mods     │                                                             │
│ Music    │                                                             │
│ Stats    │                                                             │
│ ──────── │                                                             │
│ Settings │                                                             │
├──────────┴─────────────────────────────────────────────────────────────┤
│ (A) Select   (B) Back   (LB/RB) Section        ● Online   ↓ 2 updating │  40px
└────────────────────────────────────────────────────────────────────────┘
```

**Rail:** 200 px at `bp-desktop` and wider, 64 px icon-only at `bp-compact`. On `bp-deck` it stays 200 px but rows grow to 52 px per the density table. Settings is separated by a divider because it is a utility, not a content destination.

**Top bar** holds page title, search, server status, downloads, and profile. No operating-system menu bar, per the blueprint.

**Hint strip** is persistent and is the only place button bindings are stated. It updates to the connected controller's glyph set. This is where the Figma controller research lands: bindings are shown once, consistently, rather than scattered as per-control hints.

### Why not tabs

The moodboard's dominant references (regions A, D, F) all use a persistent left rail. Tabs would force horizontal traversal across seven destinations, which the 10-foot research identifies as expensive with directional-only input. The rail plus shoulder-button cycling gives both a spatial map and a one-press jump.

---

## 2. Hierarchy

Three levels. Nothing goes deeper.

```text
L1  Section          Home Play Online Mods Music Stats Settings
L2  Page             Mods > Music Packs        Stats > Match detail
L3  Detail or sheet  a package, a box score, a controller's remap screen
```

**Rule:** if something needs a fourth level, it is a sheet over L3, not a new page. Depth is where controller users get lost, and the back stack from the console research only works if it is shallow enough to hold in your head.

Full map:

| L1 | L2 pages | L3 |
|---|---|---|
| Home | (single) | news item, announcement |
| Play | (single) | repair, graphics quick-set |
| Online | Casual, Ranked, Private | lobby, match detail |
| Mods | Browse, Installed, Import | package detail |
| Music | Soundtrack, Packs, Editor | slot editor, pack detail |
| Stats | Overview, Matches, Leaders, Head to head | match box score |
| Settings | Account, Controllers, Display, Graphics, Audio, Reborn Dolphin, Storage, Updates, Privacy, Diagnostics, About | device remap, diagnostic bundle |
| Onboarding | 12 steps | none, deliberately |
| Xbox wizard | 7 modes | drive picker, verify report |

---

## 3. Back behavior

**The rule: B and Escape always go up one level, never sideways, never out of the app.**

| Where | B does |
|---|---|
| L1 section root | nothing, and the hint strip omits Back |
| L2 page | returns to the section's default page |
| L3 detail | returns to L2 with focus restored to the originating item |
| Modal | dismisses, canceling |
| Blocking modal | nothing, it is not dismissible |
| Onboarding step | previous step, if the step is safely reversible |
| During a queue | leaves the queue, with confirmation |

**Focus restoration is mandatory.** Returning from L3 puts focus back on the exact card you opened, not on the first item. The blueprint calls this out and it is the single most common controller-navigation failure in launchers.

**Never a focus trap.** Every container is exitable with B. Verified per screen in the controller audit.

---

## 4. Controller shortcuts

Global, available everywhere unless a modal is open.

| Input | Action |
|---|---|
| A / Cross | Activate focused |
| B / Circle | Back |
| X / Square | Contextual secondary, labeled in the hint strip |
| Y / Triangle | Contextual tertiary, labeled |
| LB / RB | Previous, next L1 section |
| LT / RT | Previous, next L2 page within a section |
| L stick / D-pad | Move focus |
| R stick | Scroll the content region |
| View / Share | Search |
| Menu / Options | Overflow for the focused item |
| Guide-equivalent | nothing, reserved to the OS |

**Rule:** X and Y are never bound to anything destructive. Destructive actions live in the overflow menu and require hold-to-confirm.

**Hold-to-confirm** is 800 ms with a filling ring, used only for: delete a mod, restore the entire soundtrack, reset a settings group, format a USB drive, delete account data. Nothing else.

---

## 5. Search

One search, in the top bar, opened by View or Ctrl+F or clicking it.

**Scope is global and grouped by source:**

```text
Search: "reso"
  Settings    Internal resolution            Graphics
  Settings    Resolution behavior            Display
  Mods        Higher Resolution Ball Pack    not installed
```

Settings results are the important case. The blueprint requires finding settings by visible label **and common alternate terminology**, so each setting carries a synonym list. Someone typing "fps" finds Frame limit. Someone typing "lag" finds Input delay and Network test. Someone typing "widescreen" finds Aspect ratio.

Search never searches game content, player names, or stats. Those have their own filters inside Stats, because a global search returning both a settings row and a box score would be noise.

---

## 6. Modals

Four kinds, and the distinction is about what the user can do next.

| Kind | Dismiss | Use |
|---|---|---|
| **Sheet** | B, Escape, click outside | Package detail, box score, remap. Content, not decision |
| **Dialog** | B, Escape, explicit cancel | A decision with two or three outcomes |
| **Confirm** | Explicit only, no click-outside | Destructive. Hold-to-confirm on the primary |
| **Blocking** | Not dismissible | Required update, first-run preparation |

**Rules that apply to all four:**

- Focus moves into the modal on open and lands on **the safest action**, never on the destructive one.
- Focus returns to the trigger on close.
- Only one modal at a time. A modal cannot open a modal. If a flow needs that, it is a wizard, not a modal.
- Scrim is `scrim-modal`, no backdrop blur, per the token rules.

**Blocking modals are rationed.** Exactly two are allowed: a required online version update when the user is trying to play online, and first-run preparation. Everything else degrades instead of blocking, per the offline-first requirement.

---

## 7. Background state

The blueprint accepted background work but rejected a downloads manager page and a notification center. So background state lives in exactly two places and nowhere else.

**Downloads indicator, top bar.** Shows a count and aggregate progress. Opens a popover listing active and queued work with per-item progress, pause, and cancel. It is a popover, not a page.

**Hint strip, bottom right.** One line of the most important current activity: `↓ 2 updating` or `● Online` or `⚠ Repair needed`.

**Plus inline.** A mod that is installing shows progress on its own card. A game build rebuilding shows progress on the Play page. State appears where the object lives.

**No toasts for background work.** Toasts are for things the user just did. Background completion updates the indicator and the object; it does not interrupt.

**Server outage** shows as a banner on Online only, not app-wide. Home, Play, Mods, Music, Stats, and Settings continue working, which is the offline-first requirement made concrete.

---

## 8. Destinations

### Home
Answers: what is new, and can I play.

Hero carousel (news, releases, featured). Play Now as a persistent primary. Profile card with record and Elo. Last match result. Online status and player count. Featured mods and music rails. Patch notes. Server status. Community link.

Default focus: **Play Now.** Not the carousel. The most likely intent gets the focus.

### Play
Answers: can I play right now, and with what.

Full-bleed hero, foreground panel with cover art, one dominant Play button, and compact status chips for graphics preset, controller, and active mods. Metadata table: playtime, last played, version, size. Repair action, visible but secondary. Open NFL Street Reborn Dolphin, tertiary.

Chips are **status, not settings**. Selecting one navigates to the relevant Settings page rather than opening an inline editor. This is what keeps Play from becoming a configuration screen.

If the offline build is invalid, the Play button becomes Repair and states why.

### Online
Answers: who am I playing, and am I ready.

Three mode cards (Casual, Ranked, Private) using the option-card pattern from moodboard region C. Below them: your Elo and record, recent opponents, and a preflight strip.

**Preflight is always visible, never a gate you discover late.** Four checks: online build verified, runtime version, controller connected, network readiness. Each green, amber, or red with a word. Queueing with a red check explains the problem and offers the fix.

Lobby is L3: both players, ready state, chat, mode, and a countdown.

### Mods
Answers: what can I add, and what do I have.

Browse (hero plus rails plus grid), Installed, Import. Package detail as a sheet with screenshots, author, size, version, changelog, conflicts.

Every package states whether it is **online-safe**. Music packs are marked offline-only, which prevents the most likely confusion in the whole product.

### Music
Answers: what is my soundtrack, and how do I change it.

Soundtrack shows all 21 slots as a list with title, artist, duration, and source. Packs browses and imports `.nsmp`. Editor is the slot-level work: import, preview, trim, fades, normalize, metadata.

**Rebuild is explicit and batched.** Queue changes, then one Rebuild action that states the time cost and what stays untouched. Never rebuild silently on each edit; a 4-minute rebuild per song change would be unusable.

### Stats
Answers: how am I doing.

Overview with headline numerals (record, Elo, streak, form) using `text-stat` and teal. Matches list with filters. Leaders. Head to head. Box score at L3.

**Online and offline are labeled and filterable, never merged.** Offline is private by default and marked as such.

### Settings
Answers: how do I change this.

Category rail, always-visible plain-language descriptions, current value, recommended value, and restart-or-rebuild cost stated inline. Per-group reset. Global reset only in Diagnostics behind hold-to-confirm.

Reborn Dolphin advanced settings are a category like any other, not hidden behind a developer toggle, per Adam's decision.

### Onboarding
Twelve steps, one question per screen, no nested navigation. Progress label `Controller setup · 7 of 12`. Resumable and idempotent. Back allowed on reversible steps only.

**It uses the shell chrome not at all.** No rail, no top bar, no hint strip except the button legend. It is a full-stage flow, per moodboard regions C and J.

### Xbox Dev Mode wizard
Seven modes as option cards, then a linear flow. Drive picker never auto-selects when more than one removable drive exists. Every destructive step is its own confirm.

**Scope is a file mover**, per Adam: prepare the USB, verify every file, preserve `GC\` saves, safe eject. No Device Portal, no app deployment.

---

## 9. Deep links

Scheme `reborn://`. Every link starts the launcher, authenticates if required, navigates, and **never bypasses content or security checks**.

| Link | Destination | If unavailable |
|---|---|---|
| `reborn://play` | Play, starts the game | Repair prompt if the build is invalid |
| `reborn://online/casual` | Online, casual queue | Preflight failure explained, no queue |
| `reborn://online/ranked` | Online, ranked queue | Sign-in first, then preflight |
| `reborn://join/<code>` | Private lobby | Expired or full is stated plainly |
| `reborn://mod/<id>` | Package detail | Unknown or unpublished is stated |
| `reborn://music/<id>` | Music pack detail | as above |
| `reborn://match/<id>` | Box score | Not found, or held for review |
| `reborn://settings/<category>` | Settings category | falls back to Settings root |
| `reborn://repair` | Verify and repair | always available |

**Security rule:** a deep link may navigate and may pre-fill, but it may never install, delete, queue, or spend. `reborn://mod/<id>` opens the package; it does not install it. Anything else would make a Discord message into a remote-install primitive.

---

## 10. Focus order

Reading order, top-left to bottom-right, with three exceptions.

1. **Default focus is the safest primary**, not the first element. Home focuses Play Now. A confirm dialog focuses Cancel.
2. **Rails are one focus stop** until entered. Left and right move within a rail; up and down leave it. This prevents a horizontal rail from swallowing vertical traversal.
3. **The nav rail is reachable by pressing left from the leftmost content item**, and remembers which section you were on.

**Offscreen content must announce itself.** A horizontal rail with more items shows a partial card at the edge plus a gradient. A vertical list shows a scroll indicator. From the 10-foot research: if the user cannot tell more exists, it does not exist.

---

## Open items

1. **Search synonym lists** are specified as a requirement but not authored. Each setting needs one.
2. **Chat placement** inside the lobby is unresolved. Blueprint says pre-match text chat; whether it persists into the match countdown is undecided.
3. **Xbox wizard entry point** is unplaced. It belongs somewhere in Settings, but Settings is already eleven categories.
4. Nothing here is implemented. This is a specification.
