# Design data contract

**For designing the launcher in Figma.** September 1 2026.

## Why this exists

Five design attempts failed here, and the most expensive failure was not
visual. It was designing screens around data that does not exist: a player
rating, a queue wait time, a box score with total yards. When those hit code
they had to be cut, and the layouts went with them.

So this document is not a style guide. It is an inventory of **what data really
exists, with the real values**, every **state a screen has to survive**, and a
list of what is **not available and should not be drawn**.

Use the real strings below rather than placeholders. The ugly cases are the
point: "Patriots Throwback" is 19 characters and "Aqua Football" is 13, and a
mod row that only works at one length is a row that breaks on contact.

---

## 1. Hard constraints

| Constraint | Value | Why |
|---|---|---|
| Smallest target | **1280 x 800** | Steam Deck. Not 1920. Anything that only works wide is broken there |
| Minimum touch/focus size | **40 px** | Everything reachable by a controller |
| Input | **Controller first**, mouse second | A player on a Deck or a TV has no mouse |
| Window | Resizable, 960 x 600 minimum | |

**Every interactive element needs a visible focus state**, distinct from hover
and from selected. A controller user cannot find focus by moving a pointer, so
selected-but-unfocused and focused must look different or they lose their
place.

---

## 2. What actually exists, with real values

### Mods: 11, real names

```
Aqua Football        Blue Football       Camo Football
Green Football       Panda Football      Patriots Throwback
Pink Football        Purple Football     Saints Alternate
PS Controls          Xbox Controls
```

Each has one state: **applied** or **not applied**. **Four are currently
applied**: Aqua Football, Patriots Throwback, Saints Alternate, Xbox Controls.

An earlier version of this document said none were, which was wrong: it was
read from the shipped distribution's MODS folder, where `!INSTLOGS` is empty,
and asserted about the live install, where it holds four install logs. The
launcher had the same bug and reported 0 of 11.

Two states a mods screen therefore has to carry, and neither is hypothetical:

**More than one mod writing the same file.** 20 paths on this machine, and 7
mods all write the same football texture. JSGME copies them in order and keeps
whichever landed last without saying so, which is one of the reasons it is being
replaced. A mods list that shows seven footballs as independently "on" is
telling the player something untrue.

**Recorded paths with no backup.** Xbox Controls has 19 recorded paths and 0
backups. That is genuinely ambiguous: JSGME backs up only a file it displaces,
so an add-only mod legitimately has none, and the install logs cannot say which
case it is. So this must be surfaced as unknown, never as either safe or lost.

**They have no artwork.** Nine are recoloured footballs and two are button
prompts. If a mod tile needs an image, it has to be derived from the mod (a
coloured ball) or the tile has to work without one. Borrowing a gameplay
screenshot for "Aqua Football" is filler with a picture on it.

### Fields: 20 venues

6 have usable art so far. Names are yours to set; the placeholders I used were
invented and should be replaced with the real venue names.

### Teams: 32, with 31 marks

Real rendered helmet decals exist for 31. **Titans has none** and that is a
real gap in the decal pack, not a bug. So a team grid needs a **missing-mark
state**, currently falling back to "TIT".

### Sessions: 4 recorded, real

```
id                      build    duration
1788242738648-offline   offline  0.0 min
1788288004174-offline   offline  0.5 min
1788290098042-offline   offline  1.7 min
```

Each carries an end state: **Exited(code)**, **Crashed(detail)**,
**Terminated**, or **Interrupted**. The last three route to a recovery screen.
Interrupted means the launcher died with the game and is the only evidence that
happened, so it cannot be hidden.

### Controllers

Named the way the emulator names them, never invented:

```
XInput/0/Gamepad          an Xbox pad on Windows
DInput/0/Keyboard Mouse   keyboard fallback
WGInput/0/Xbox One Game Controller
```

**A wireless pad goes to sleep.** It has appeared and vanished several times
during this work. So "no controller" is a normal, recurring state and needs a
first-class treatment, not an error.

### Other real content

| Thing | Count |
|---|---|
| Game images in the Games folder | 17 |
| Savestates | 31 |
| Screenshots | 188 |
| Texture pack files | 27,025 |

17 game images is worth designing for: a picker, not a single fixed game.

### Match capture: exactly 12 fields per player

```
pass_td   rush_td   rec_td   pick_six   fum_td   interceptions
forced_fumbles   fumble_rec   tackles   sacks   thrown_int   fumbles
```

**There is no yardage. No total yards, no passing, no rushing.** A box score
was already designed around those and had to be thrown away.

Also true and awkward: in a real capture a quarterback was credited with sacks,
which suggests defensive plays can be attributed by roster slot rather than by
player. A stats screen should be able to show a value as **suspect**.

---

## 3. What does NOT exist. Please do not design it.

- **No player rating or Elo.** Nothing produces one.
- **No matchmaking, no queue, no wait time, no player counts.**
- **No friends list and no presence.** There is no online system to read from.
- **No win-loss record.**
- **No last-match score.**
- **No mod marketplace, no browsing, no downloads.** Mods are what is on disk.
- **No achievements, no unlock progress.**
- **No music yet.** The soundtrack format could not be read, so track lists and
  durations are not available.

If a screen needs one of these to make sense, that screen cannot ship yet, and
it is better to know that in Figma than in CSS.

---

## 4. States every screen must have

Not decoration. Each of these is reachable today.

| State | When | Currently |
|---|---|---|
| **No runtime set** | Fresh install | Play is unavailable, and must say why |
| **No game image set** | Fresh install | Same |
| **No controller** | Pad asleep or unplugged | Recurs constantly |
| **Controller list incomplete** | No runtime, so only XInput was searched | Must be said, not implied |
| **Never played** | Before a first session | No duration to show |
| **Session interrupted** | Launcher died with the game | Recovery, with detail |
| **Game crashed** | Access violation etc | Recovery, with detail |
| **Game exited non-zero** | A failure, not a crash | Return quietly, no recovery |
| **Mods applied, no backup** | JSGME state | Migration BLOCKED, with reason |
| **Missing team mark** | Titans | Fallback |
| **Feature not built** | Online, Music, Stats | Say "not built", not an empty state |

**An empty state and a not-built state are different claims.** An empty state
says the feature works and has nothing in it. Six sections are not built, and
drawing them as empty would be a lie.

---

## 5. The eight sections

Fixed by the information architecture, and one is a utility rather than a
destination.

```
Home     Play     Online     Mods     Music     Stats     |     Settings
```

Online, Music and Stats have core logic but nothing designed. Home, Play, Mods
and Settings are the four that can be real immediately.

---

## 6. What the interface can call

Seven commands exist today. Ten more subsystems are built and tested but not
yet exposed, so if your design needs something below, say so and it becomes a
command rather than a compromise.

**Available now**

| Command | Returns |
|---|---|
| `paths` | Install root, data root, whether saves survive uninstall, the data directories |
| `devices` | Pads with port and device string, plus which backends went unsearched |
| `sessions` | Session history with duration and end state |
| `mods` | Real mod names and whether each is applied |
| `home_state` | Sessions, minutes, controllers, mods applied, whether a runtime is set |
| `prepare_launch` | Writes config and starts nothing, reports which user folder and what changed |
| `launch` | Starts the game, blocks, returns how it ended |

**Built in the core, not yet reachable from the interface**

`mods` composition (apply and unapply), `music`, `onboarding`, `content`
updates, `xbox` wizard, `stats` queue, `gamepad` navigation, `migration`,
`library` verify and repair, `ini` merging.

---

## 7. Two rules worth keeping from the failed attempts

**Never show a value the launcher did not measure.** The single most common
note on my own designs was that a plausible number is worse than a blank,
because it looks correct in review and breaks on real data.

**Say when an answer is partial.** "No controller found" and "no controller
found, but we only looked in one place" are different facts, and the second one
recurs constantly here.
