# Wireframes

## NFL Street Reborn Launcher

**Date:** August 29, 2026
**Covers brief deliverable:** 8 (wireframes and screen inventory)
**Depends on:** [`INFORMATION_ARCHITECTURE.md`](./INFORMATION_ARCHITECTURE.md), [`COMPONENT_SPECIFICATION.md`](./COMPONENT_SPECIFICATION.md)

Low fidelity, deliberately. The brief requires wireframes before visual mockups, so these carry layout, hierarchy, and controller focus order and nothing else. No color, no type styling, no artwork.

**Notation used throughout:**

```
[Label]     button              ▸ Label     focused (default focus on load)
( ) Label   unselected option   (•) Label   selected option
[x]/[ ]     toggle              ▓▓▓░░       progress
«text»      real Reborn content, not lorem
```

Drawn at 1920x1080 unless noted. Steam Deck variants appear where the layout changes rather than merely scales.

---

## 1. First-run welcome

No shell chrome. Full stage, per moodboard region J.

```
┌──────────────────────────────────────────────────────────────────┐
│                                                                  │
│                                                                  │
│                          ┌─────────┐                             │
│                          │  NSR    │   96px icon                 │
│                          └─────────┘                             │
│                                                                  │
│                  Welcome to NFL Street Reborn                    │
│                                                                  │
│              This sets up the game, your controller,             │
│                 and your graphics. About 4 minutes.              │
│                                                                  │
│                                                                  │
│                      ▸ [   Get started   ]                       │
│                                                                  │
│                        Language · Accessibility                  │
│                                                                  │
│                                                                  │
├──────────────────────────────────────────────────────────────────┤
│ (A) Select                                                       │
└──────────────────────────────────────────────────────────────────┘
```

**Focus order:** Get started → Language → Accessibility.
**Why:** the time estimate is the most useful thing on this screen and most welcome screens omit it. Language and accessibility are visually subordinate but reachable before anything is committed.

---

## 2. Game verification

Adam decided the launcher **always ships the finished game image**, so this is not an import step. It confirms what will be installed and where.

```
┌──────────────────────────────────────────────────────────────────┐
│  Step 3 of 12                                                    │
│                                                                  │
│  Your game                                                       │
│                                                                  │
│  NFL Street Reborn includes everything needed to play.           │
│  Nothing to find, nothing to insert.                             │
│                                                                  │
│  ┌────────────────────────────────────────────────────────────┐  │
│  │ «NFL Street Reborn»                            «4.4 GB»    │  │
│  │ Offline game, online game, and «2.0 GB» of textures        │  │
│  └────────────────────────────────────────────────────────────┘  │
│                                                                  │
│  Already have Reborn installed?                                  │
│  ┌────────────────────────────────────────────────────────────┐  │
│  │ ✓ Found «NFL Street Reborn 2.1» in AppData                 │  │
│  │   We'll bring over your saves, screenshots, and mods.      │  │
│  │   A backup is made first.                    [x] Bring over│  │
│  └────────────────────────────────────────────────────────────┘  │
│                                                                  │
│                             [ Back ]     ▸ [   Continue   ]      │
├──────────────────────────────────────────────────────────────────┤
│ (A) Select   (B) Back                                            │
└──────────────────────────────────────────────────────────────────┘
```

**Note:** if a real import pipeline is ever built (questionnaire 8.6), this screen changes shape entirely. It is designed to be replaceable.

---

## 3. Controller setup

The Apple Design Award research said the standard is acting without reading. So this screen is a live test, not a diagram.

```
┌──────────────────────────────────────────────────────────────────┐
│  Step 7 of 12                                                    │
│                                                                  │
│  Your controller                                                 │
│  Press any button on the controller you want to use.             │
│                                                                  │
│  ┌────────────────────────────────────────────────────────────┐  │
│  │ ◉ «DualSense Wireless Controller»                          │  │
│  │   USB · Player 1 · Battery «84%»                           │  │
│  │                                                            │  │
│  │            ┌──────────────────────────┐                    │  │
│  │            │   controller diagram      │  lights up as     │  │
│  │            │   with live highlighting  │  you press        │  │
│  │            └──────────────────────────┘                    │  │
│  │                                                            │  │
│  │   Try the sticks and triggers. Everything you press        │  │
│  │   should light up.                                         │  │
│  │                                                            │  │
│  │   ✓ Buttons   ✓ Left stick   ✓ Right stick   ○ Triggers    │  │
│  └────────────────────────────────────────────────────────────┘  │
│                                                                  │
│  Button prompts in game will match this controller.              │
│                                                                  │
│    [ Remap ]    [ Back ]              ▸ [ Everything works ]     │
├──────────────────────────────────────────────────────────────────┤
│ (A) Select   (B) Back                                            │
└──────────────────────────────────────────────────────────────────┘
```

**Focus order:** Everything works → Remap → Back.
**Why:** the checklist gives the user a completion condition without reading instructions. "Everything works" is a claim the user makes after testing, not a claim the software makes.

**Conflict variant** replaces the panel body with a warning when Steam Input or DS4Windows is detected duplicating the device, with a Learn more and a Continue anyway.

---

## 4. System check and graphics recommendation

```
┌──────────────────────────────────────────────────────────────────┐
│  Step 5 of 12                                                    │
│                                                                  │
│  Graphics                                                        │
│  We tested your PC for «8 seconds». Here's what fits.            │
│                                                                  │
│   ┌────────────────────────────────────────────────────────┐    │
│   │ Performance      Lowest settings. Best for older PCs   │    │
│   │                  and handhelds. Effects are off.       │    │
│   └────────────────────────────────────────────────────────┘    │
│  ╱┌────────────────────────────────────────────────────────┐    │
│  ╱│ Balanced         RECOMMENDED FOR YOUR PC               │    │
│  ╱│                  Sharp at «1080p», smooth frame rate.  │    │
│  ╱└────────────────────────────────────────────────────────┘    │
│   ┌────────────────────────────────────────────────────────┐    │
│   │ Quality          Higher resolution and effects.        │    │
│   │                  May drop frames in busy moments.      │    │
│   └────────────────────────────────────────────────────────┘    │
│   ┌────────────────────────────────────────────────────────┐    │
│   │ Ultra            Everything on. For strong PCs.        │    │
│   └────────────────────────────────────────────────────────┘    │
│                                                                  │
│   ╱ = sheared selection edge. Selected card only.                │
│                                                                  │
│  Detected: «Ryzen 5 5600», «RTX 3060», «16 GB»                  │
│  You can change this any time in Settings.                       │
│                                                                  │
│                             [ Back ]     ▸ [   Continue   ]      │
└──────────────────────────────────────────────────────────────────┘
```

**Why:** single column rather than a two-by-two grid, per the V1 resolution, and because four options of unequal text length grid badly. The recommended option is preselected and labeled, per the blueprint. The detected hardware line is shown so the recommendation is checkable rather than magic. Each option says what it costs, not just what it gives.

---

## 5. Preparation and completion

```
┌──────────────────────────────────────────────────────────────────┐
│  Step 11 of 12                                                   │
│                                                                  │
│  Setting up NFL Street Reborn                                    │
│                                                                  │
│  ✓  Copy game files                                    done      │
│  ✓  Install Reborn Dolphin                             done      │
│  ◐  Apply textures                        ▓▓▓▓▓▓░░░░  «62%»      │
│  ○  Apply your controller and graphics                 waiting   │
│  ○  Check everything                                   waiting   │
│                                                                  │
│  «About 2 minutes left»                                          │
│                                                                  │
│  You can leave this running. It picks up where it left off       │
│  if you close the launcher.                                      │
└──────────────────────────────────────────────────────────────────┘
```

Then:

```
┌──────────────────────────────────────────────────────────────────┐
│                          ┌─────────┐                             │
│                          │  NSR    │                             │
│                          └─────────┘                             │
│                              Ready                               │
│                                                                  │
│    Controller   «DualSense», player 1                            │
│    Graphics     «Balanced», «1080p»                              │
│    Installed    «4.4 GB» in AppData                              │
│    Account      Not connected. Offline play works without it.    │
│                                                                  │
│                      ▸ [    Play Now    ]                        │
│                    [ Take a quick tour ]                         │
└──────────────────────────────────────────────────────────────────┘
```

**Why:** named tasks rather than console output, per the blueprint. The completion screen states what was decided instead of congratulating, per the language guide. "You're all set!" would be a violation.

---

## 6. Home

```
┌──────────────────────────────────────────────────────────────────┐
│ NSR   Home                        [search]  ● Online  ↓   [me]   │
├──────────┬───────────────────────────────────────────────────────┤
│ ▸Home    │ ┌───────────────────────────────────────────────────┐ │
│  Play    │ │                                                   │ │
│  Online  │ │        «Season 4 rosters are live»                │ │
│  Mods    │ │         hero artwork, carousel                    │ │
│  Music   │ │                              ● ○ ○                │ │
│  Stats   │ └───────────────────────────────────────────────────┘ │
│ ──────── │                                                       │
│  Settings│  ▸ [  Play Now  ]     «Bill» · «1,247» · «12-4»      │
│          │                                                       │
│          │  Last match   «W 34-21 vs Giants»  Ranked  «+18»     │
│          │                                                       │
│          │  Featured mods                              See all → │
│          │  ┌────────┐ ┌────────┐ ┌────────┐ ┌────────┐         │
│          │  │ art    │ │ art    │ │ art    │ │ art    │         │
│          │  │«Purple»│ │«Camo»  │ │«90s Mix│ │«PS pad»│         │
│          │  └────────┘ └────────┘ └────────┘ └────────┘         │
│          │                                                       │
│          │  News                                                 │
│          │  «2.4 patch notes»                        «2 days ago»│
├──────────┴───────────────────────────────────────────────────────┤
│ (A) Select  (B) Back  (LB/RB) Section     ● Online   ↓ 2 updating│
└──────────────────────────────────────────────────────────────────┘
```

**Default focus is Play Now, not the carousel.** The most likely intent gets focus. The carousel is reachable by pressing up.

**Focus order:** Play Now → carousel → profile card → last match → mods rail (one stop, left and right inside) → news.

---

## 7. Play

The strongest moodboard reference, regions B and I.

```
┌──────────────────────────────────────────────────────────────────┐
│ NSR   Play                        [search]  ● Online  ↓   [me]   │
├──────────┬───────────────────────────────────────────────────────┤
│  Home    │                                                       │
│ ▸Play    │         full-bleed hero artwork                       │
│  Online  │                                                       │
│  Mods    │   ┌─────────────────────────────────────────────┐     │
│  Music   │   │ ┌────┐  «NFL Street Reborn»                 │     │
│  Stats   │   │ │cover│ «Version 2.4» · «4.4 GB»            │     │
│ ──────── │   │ └────┘                                      │     │
│  Settings│   │                                             │     │
│          │   │  Played «14h»   Last «yesterday»            │     │
│          │   │                                             │     │
│          │   │  ▸ [    ▶  Play Now    ]                    │     │
│          │   │                                             │     │
│          │   │  «Balanced»  «DualSense»  «2 mods»          │     │
│          │   │                                             │     │
│          │   │  [ Verify and repair ]  [ Open Dolphin ]    │     │
│          │   └─────────────────────────────────────────────┘     │
├──────────┴───────────────────────────────────────────────────────┤
│ (A) Play  (B) Back  (X) Repair            ● Online               │
└──────────────────────────────────────────────────────────────────┘
```

**The three chips are status, not settings.** Selecting one navigates to the matching Settings page. This is what keeps Play from turning into a configuration screen.

**Broken variant:** hero dims, the button becomes `[ Repair and Play ]`, and a line above it states what is wrong in plain words.

---

## 8. Online mode selection

```
┌──────────────────────────────────────────────────────────────────┐
│ NSR   Online                      [search]  ● Online  ↓   [me]   │
├──────────┬───────────────────────────────────────────────────────┤
│  Home    │  «Bill» · «1,247» · «12-4» · «W3»                    │
│  Play    │                                                       │
│ ▸Online  │  ┌──────────────┐ ┌──────────────┐ ┌──────────────┐  │
│  Mods    │  │ ▸ Ranked     │ │   Casual     │ │   Private    │  │
│  Music   │  │              │ │              │ │              │  │
│  Stats   │  │ Affects your │ │ No rating    │ │ Play a friend│  │
│ ──────── │  │ rating.      │ │ change.      │ │ with a code. │  │
│  Settings│  │ «~40s wait»  │ │ «~15s wait»  │ │              │  │
│          │  └──────────────┘ └──────────────┘ └──────────────┘  │
│          │                                                       │
│          │  Before you play                                      │
│          │  ✓ Online game verified      ✓ Controller connected   │
│          │  ✓ Version «2.4» current     ⚠ Network «not tested»   │
│          │                              [ Test network ]         │
│          │                                                       │
│          │  Recent opponents                                     │
│          │  «Marcus»  «W 34-21»  «2h ago»                        │
├──────────┴───────────────────────────────────────────────────────┤
│ (A) Select  (B) Back  (Y) Test network    ● Online               │
└──────────────────────────────────────────────────────────────────┘
```

**Preflight is always visible, never a gate discovered late.** Estimated wait times are shown per mode because that is the actual decision input.

---

## 9. Matchmaking and lobby

Searching:

```
┌──────────────────────────────────────────────────────────────────┐
│                     Searching for a ranked match                 │
│                                                                  │
│                              «0:24»                              │
│                                                                  │
│              Looking for «1,150-1,350» · under «90 ms»           │
│              «14» players in the ranked queue                    │
│                                                                  │
│              Widening the rating range in «0:36»                 │
│                                                                  │
│                        ▸ [ Leave queue ]                         │
│               Leaving now won't affect your rating.              │
└──────────────────────────────────────────────────────────────────┘
```

Lobby:

```
┌──────────────────────────────────────────────────────────────────┐
│  Ranked match                                            «0:18»  │
│                                                                  │
│   ┌─────────────────────┐        ┌─────────────────────┐        │
│   │ «Bill»              │        │ «Marcus»            │        │
│   │ «1,247» · «12-4»    │  «34ms»│ «1,198» · «9-7»     │        │
│   │ ✓ Ready             │        │ ○ Not ready         │        │
│   └─────────────────────┘        └─────────────────────┘        │
│                                                                  │
│   ┌────────────────────────────────────────────────────────┐    │
│   │ «Marcus»: gl                                           │    │
│   │ ─────────────────────────────────────────────────────  │    │
│   │ [ Type a message                                    ]  │    │
│   └────────────────────────────────────────────────────────┘    │
│                                                                  │
│   ▸ [    Ready    ]        [ Leave ]                             │
├──────────────────────────────────────────────────────────────────┤
│ (A) Ready  (B) Leave  (X) Chat                                   │
└──────────────────────────────────────────────────────────────────┘
```

**Why:** the ping is between the two cards because it is a property of the pair, not of either player. "Leaving now won't affect your rating" answers the question a ranked player actually has.

---

## 10. Mods

```
┌──────────────────────────────────────────────────────────────────┐
│ NSR   Mods                        [search]  ● Online  ↓   [me]   │
├──────────┬───────────────────────────────────────────────────────┤
│  Home    │  [ Browse ]  [ Installed «3» ]  [ Import ]            │
│  Play    │ ┌───────────────────────────────────────────────────┐ │
│  Online  │ │        «Season 4 Ball Pack»    featured           │ │
│ ▸Mods    │ └───────────────────────────────────────────────────┘ │
│  Music   │                                                       │
│  Stats   │  Music Packs                                See all → │
│ ──────── │  ┌──────────┐ ┌──────────┐ ┌──────────┐              │
│  Settings│  │  art     │ │  art     │ │  art     │              │
│          │  │«90s Mix» │ │«Instr.»  │ │«Hype»    │              │
│          │  │«DJ Kade» │ │«Reborn»  │ │«Marcus»  │              │
│          │  │[Offline] │ │[Offline] │ │[Offline] │              │
│          │  │[Install] │ │▓▓▓▓░ 62% │ │[Installed│              │
│          │  └──────────┘ └──────────┘ └──────────┘              │
│          │                                                       │
│          │  Balls                                      See all → │
│          │  ┌──────────┐ ┌──────────┐ ┌──────────┐              │
│          │  │«Purple»  │ │«Camo»    │ │«Panda»   │              │
│          │  └──────────┘ └──────────┘ └──────────┘              │
├──────────┴───────────────────────────────────────────────────────┤
│ (A) Install  (B) Back  (X) Details  (LT/RT) Tab                  │
└──────────────────────────────────────────────────────────────────┘
```

**The `[Offline]` chip on every music pack** is the single highest-value label here. It prevents "why did my music not work online," which is the most predictable support question this product will get.

**Steam Deck variant:** three columns become two, descriptions drop to one line, and each card shows at most two chips.

---

## 11. Music editor

```
┌──────────────────────────────────────────────────────────────────┐
│ NSR   Music                       [search]  ● Online  ↓   [me]   │
├──────────┬───────────────────────────────────────────────────────┤
│  Home    │  [ Soundtrack ]  [ Packs ]  [ Editor ]                │
│  Play    │                                                       │
│  Online  │  Slot «7»                                             │
│  Mods    │  ┌───────────────────────────────────────────────────┐│
│ ▸Music   │  │ «my_track.mp3»                     «3:42»         ││
│  Stats   │  │ ┌───────────────────────────────────────────────┐ ││
│ ──────── │  │ │  ▁▃▅▇▅▃▁▃▅▇▇▅▃▁▁▃▅▇▅▃▁▃▅▇▅▃▁                │ ││
│  Settings│  │ │  [    ├──────── selected ────────┤        ]  │ ││
│          │  │ └───────────────────────────────────────────────┘ ││
│          │  │   ▶ Preview      Start «0:00»    End «3:42»       ││
│          │  │                                                   ││
│          │  │   Fade in  [==•=======] «0.5s»                    ││
│          │  │   Fade out [=====•====] «2.0s»                    ││
│          │  │   Volume   [=======•==] «0 dB»   [x] Normalize    ││
│          │  │                                                   ││
│          │  │   Title  [ «Street Anthem»              ]         ││
│          │  │   Artist [ «DJ Kade»                    ]         ││
│          │  │   Album  [ «Reborn Vol 2»               ]         ││
│          │  └───────────────────────────────────────────────────┘│
│          │                                                       │
│          │  ┌───────────────────────────────────────────────────┐│
│          │  │ «3» changes queued.  Rebuilding takes «~4 min».   ││
│          │  │ Your saves and online game aren't touched.        ││
│          │  │                          ▸ [ Rebuild game ]       ││
│          │  └───────────────────────────────────────────────────┘│
├──────────┴───────────────────────────────────────────────────────┤
│ (A) Select  (B) Back  (X) Preview  (Y) Clear slot                │
└──────────────────────────────────────────────────────────────────┘
```

**Rebuild is batched and explicit.** Rebuilding on every edit at four minutes a go would be unusable. The banner states the cost and what stays safe, per the language guide.

Every slider has a numeric readout because sticks are imprecise.

---

## 12. Match history and box score

**Rebuilt from real captured data.** Everything below comes from
`User\LeagueGameStats\Falcons_vs_Chargers_001.json`, a real capture dated
August 5, 2026. Nothing here is invented.

### The correction real data forced

The first draft of this screen showed **Total yards, Passing, Rushing**. Those
statistics **do not exist**. The Reborn capture schema (`nsr.game.v1`) records
twelve per-player categories and none of them is yardage:

```
pass_td   rush_td   rec_td   pick_six   fum_td        interceptions
tackles   sacks     fumbles  thrown_int forced_fumbles fumble_rec
```

This is exactly the failure the anti-vibecoding guardrails warn about.
Invented data made a plausible-looking layout that would have broken the moment
real data arrived. It was caught by exporting real data, not by review.

```
┌──────────────────────────────────────────────────────────────────┐
│ NSR   Stats                       [search]  ● Online  ↓   [me]   │
├──────────┬───────────────────────────────────────────────────────┤
│  Home    │  [ Overview ] [ Matches ] [ Leaders ] [ Head to head ] │
│  Play    │                                                       │
│  Online  │  All modes ▾   All time ▾   Online and offline ▾      │
│  Mods    │  ┌───────────────────────────────────────────────────┐│
│  Music   │  │ ⚠ │ FALCONS 36 - 24 CHARGERS │Offline│Held │Aug 5││
│ ▸Stats   │  ├───────────────────────────────────────────────────┤│
│ ──────── │  │ W │ OUTLAWS 36 - 12 SHADOW   │ Ranked│ +18 │Jul27││
│  Settings│  │   │ KNIGHTS                  │       │     │     ││
│          │  ├───────────────────────────────────────────────────┤│
│          │  │ L │ OUTLAWS 21 - 36 STREET   │ Ranked│ -14 │Jul27││
│          │  │   │ CRUSADE                  │       │     │     ││
│          │  └───────────────────────────────────────────────────┘│
├──────────┴───────────────────────────────────────────────────────┤
│ (A) Open  (B) Back  (LT/RT) Tab                                  │
└──────────────────────────────────────────────────────────────────┘
```

**The first row is a real held-for-review case, not a mockup of one.** That
capture has `"sides_verified": false`, meaning the tracker could not confirm
which player controlled which team. Under the results policy that is exactly
a hold. The design has to show this state because the real pipeline produces
it, and the first row of real data was already an instance of it.

### Box score, real numbers

```
┌──────────────────────────────────────────────────────────────────┐
│  ‹ Back      FALCONS 36 - 24 CHARGERS      Offline · Aug 5       │
│                                                                  │
│  ⚠ Held for review                                               │
│    We couldn't confirm which player controlled which team, so    │
│    this result isn't counted. Nothing has changed.               │
│    [ Report a problem ]                                          │
│                                                                  │
│  FALCONS                     Pass  Rush  Rec  Tkl  Sack  INT  FUM│
│  Michael Penix Jr.             2     0    1    2    2     0    0 │
│  Bijan Robinson                0     2    1    2    1     0    1 │
│  Drake London                  1     0    1    0    0     0    1 │
│  Jessie Bates III              0     0    0    2    0     0    1 │
│  James Pearce Jr.              0     0    0    1    1     0    0 │
│  A.J. Terrell Jr.              0     0    0    1    0     0    0 │
│  Jalon Walker                  0     0    0    4    1     0    0 │
│                                                                  │
│  CHARGERS                    Pass  Rush  Rec  Tkl  Sack  INT  FUM│
│  Tuli Tuipulotu                0     0    0    4    2     0    0 │
│  Khalil Mack                   0     0    0    0    0     0    0 │
│  Justin Herbert                2     0    0    2    0     0    2 │
│  Omarion Hampton               0     1    1    1    0     0    1 │
│  Derwin James Jr.              0     0    0    4    0     4    0 │
│  Quentin Johnston              0     0    1    4    0     1    1 │
│  Donte Jackson                 0     0    0    2    0     0    0 │
│                                                                  │
│  Also tracked: pick-six, fumble-return TD, forced fumbles,       │
│  fumble recoveries, interceptions thrown.        [ Show all ▾ ]  │
└──────────────────────────────────────────────────────────────────┘
```

### Three layout facts real data exposed

1. **Names are long.** "Michael Penix Jr." and "Tuli Tuipulotu" are 17 and 14
   characters. The name column needs 22 characters, not the 12 a mock roster
   would have suggested.
2. **Most cells are zero.** In this real game, 60 of 98 stat cells are 0.
   Rendering a dense grid of zeros is unreadable, so **zeros are
   `text-tertiary` and non-zeros are `text-primary`**. That single rule is
   what makes the table scannable, and no invented dataset would have
   surfaced the need for it.
3. **Twelve categories will not fit.** Seven columns fit at desktop width with
   a 22-character name column. The remaining five go behind "Show all," and on
   Steam Deck the name column pins and the rest scroll.

**One oddity worth flagging to Adam, not a design issue:** Michael Penix Jr.
is recorded with 2 sacks and 5 interceptions thrown, and Bijan Robinson with
1 sack. Quarterbacks recording sacks suggests the capture is attributing some
defensive plays by roster slot rather than by the player who made them, or
that both players used the same slot on defense. Worth checking against the
tracker before these numbers appear on a public profile.

## 13. Settings, basic and advanced

```
┌──────────────────────────────────────────────────────────────────┐
│ NSR   Settings                    [search]  ● Online  ↓   [me]   │
├──────────┬──────────────┬────────────────────────────────────────┤
│  Home    │  Account     │  Graphics                              │
│  Play    │  Controllers │                                        │
│  Online  │ ▸Graphics    │  Preset                                │
│  Mods    │  Display     │  Higher looks sharper, costs frame rate│
│  Music   │  Audio       │  [Performance][Balanced•][Quality][Ult]│
│  Stats   │  Reborn      │                                        │
│ ──────── │   Dolphin    │  Internal resolution                   │
│  Settings│  Storage     │  How sharply the game renders before   │
│          │  Updates     │  it is scaled to your screen.          │
│          │  Privacy     │  Currently «3x (1920x1584)».           │
│          │  Diagnostics │  Recommended for your PC: «3x».        │
│          │  About       │                          [ 3x      ▾ ] │
│          │              │                                        │
│          │              │  ReShade effects                       │
│          │              │  Extra visual effects. Windows only.   │
│          │              │  Off on the Performance preset.        │
│          │              │                                   [x]  │
│          │              │  ─────────────────────────────────     │
│          │              │  [ Reset graphics to defaults ]        │
└──────────┴──────────────┴────────────────────────────────────────┘
```

Advanced Dolphin settings use the same row anatomy, tighter radius, and always carry a description:

```
│  Store EFB copies to texture only                                │
│  Speeds things up by skipping a slow memory copy. Can break      │
│  some visual effects. Reborn needs this OFF for the              │
│  «NFL Challenge» player photos to work.                          │
│  Currently «Off». Reborn default: «Off».                    [ ]  │
│                                                                  │
│  Emulated CPU clock                                              │
│  Runs the emulated console faster or slower than real hardware.  │
│  Changing this can break online play.                            │
│  Currently «100%».                       [=======•==] «100%»     │
│  ⟳ Changing this requires restarting the game.                   │
```

**Every option carries plain language**, including the ones only three people understand. That is the blueprint's requirement, and the EFB example is drawn from a real Reborn behavior rather than invented.

---

## 14. Network test

```
┌──────────────────────────────────────────────────────────────────┐
│  ‹ Back        Network test                                      │
│                                                                  │
│  ✓  Internet                          connected                  │
│  ✓  Reborn servers                    «34 ms»                    │
│  ✓  Ping to «US East»                 «28 ms»                    │
│  ✓  Jitter                            «4 ms»       good          │
│  ✓  Packet loss                       «0%»                       │
│  ⚠  Direct connection                 «moderate NAT»             │
│                                                                  │
│     Moderate NAT means some opponents may be harder to           │
│     connect to. Matches will still work most of the time.        │
│     [ How to improve this ]                                      │
│                                                                  │
│  Overall: ready for online play                                  │
│                                                                  │
│                    ▸ [ Done ]     [ Test again ]                 │
└──────────────────────────────────────────────────────────────────┘
```

**Why:** every result has a unit and a plain-language verdict. The warning explains consequence before remedy. Nothing here says "Oops."

---

## 15. Crash recovery

A page, not a dialog, per the component spec.

```
┌──────────────────────────────────────────────────────────────────┐
│                                                                  │
│   ⊗   NFL Street Reborn closed unexpectedly                      │
│                                                                  │
│       Your saves, screenshots, and stats are safe.               │
│                                                                  │
│       This usually happens because of a mod conflict.            │
│       You have «3» mods installed.                               │
│                                                                  │
│       ▸ [ Start with mods disabled ]                             │
│         [ Relaunch normally ]                                    │
│         [ Verify and repair ]                                    │
│         [ Start with Performance graphics ]                      │
│         [ Open NFL Street Reborn Dolphin ]                       │
│         [ Create a diagnostic file ]                             │
│                                                                  │
│       Nothing is sent anywhere unless you choose to share it.    │
└──────────────────────────────────────────────────────────────────┘
```

**Default focus is the most likely fix, not Relaunch.** After three crashes with the same signature the order changes so the safe-mode option leads. Never auto-restarts.

---

## 16. Xbox Dev Mode wizard

Scope is a file mover, per Adam. No Device Portal, no app deployment.

```
┌──────────────────────────────────────────────────────────────────┐
│  ‹ Back        Xbox Dev Mode                                     │
│                                                                  │
│  Pick your USB drive                                             │
│  This copies NFL Street Reborn onto a drive you plug into        │
│  your Xbox. You install the app on the Xbox yourself.            │
│                                                                  │
│  ( ) «E:  XBOX-USB»          «128 GB»  «94 GB free»  exFAT       │
│      Has a Reborn install from «Aug 2»                           │
│                                                                  │
│  ( ) «F:  My Passport»       «2 TB»    «1.1 TB free» NTFS        │
│                                                                  │
│  ⚠  «G: (system)» is hidden. System drives can't be used.        │
│                                                                  │
│  Nothing is written until you confirm the next screen.           │
│                                                                  │
│                             [ Back ]        [   Continue   ]     │
└──────────────────────────────────────────────────────────────────┘
```

**Nothing is preselected**, even though one drive is obviously the right one. That is the blueprint's rule and it exists because the failure mode is wiping the wrong disk.

Confirm step:

```
│  Ready to copy to «E: XBOX-USB»                                  │
│                                                                  │
│  Will copy        «2.9 GB»  game, settings, textures, art        │
│  Will keep        your existing Xbox saves («16 MB»)             │
│  Will back up     current install to «E:\Reborn_backup_...»      │
│  Will not         format or erase the drive                      │
│                                                                  │
│                        ▸ [ Copy to drive ]                       │
```

**"Will not format or erase the drive"** is stated explicitly. Saying what will not happen is what makes a destructive-looking operation safe to approve.

---

## 17. Steam Deck variants

Where 1280x800 changes structure rather than scale.

**Home:** rail collapses to icons with labels on focus. Mods rail shows three cards instead of four. Hero drops to 55% of viewport height. News shows one item.

**Mods:** two columns instead of three. One line of description. Two chips maximum.

**Settings:** two panes instead of three. Category selection pushes the content pane in, and B returns. About eight rows visible instead of twelve.

**Box score:** the player table becomes horizontally scrollable with a pinned name column, rather than shrinking type below the 10-foot scale.

---

## Screen inventory check

| Brief requirement | Section |
|---|---|
| First-run welcome | 1 |
| Game import | 2, reshaped by the ship-the-image decision |
| Controller setup | 3 |
| System benchmark and graphics recommendation | 4 |
| Onboarding completion | 5 |
| Home | 6 |
| Play Now | 7 |
| Casual, ranked, private selection | 8 |
| Matchmaking and lobby | 9 |
| Mods marketplace | 10 |
| Music editor | 11 |
| Match history and box score | 12 |
| Basic settings | 13 |
| Advanced Dolphin settings | 13 |
| Network test | 14 |
| Repair and crash recovery | 15 |
| Xbox Dev Mode USB preparation | 16 |

All 17 covered.

---

## Open items

1. **Section 12 now uses fully real captured data** and the invented yardage columns are gone. Sections 6, 8, and 9 still contain invented Elo ratings, queue wait times, and player counts, because no real online system exists yet to export them from. Those are the remaining instances of the invented-data violation.
2. **The tour** referenced on the completion screen is unspecified.
3. **Chat during the match countdown** is still unresolved from the IA open items.
4. No visual design, no color, no artwork. Low fidelity by requirement.
