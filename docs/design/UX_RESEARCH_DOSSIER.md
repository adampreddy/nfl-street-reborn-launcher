# UX Research Dossier

## NFL Street Reborn Launcher

**Date:** August 29, 2026
**Governing brief:** [`CLAUDE_UX_RESEARCH_BRIEF.md`](./CLAUDE_UX_RESEARCH_BRIEF.md)
**Covers brief deliverables:** 1 (source matrix), 2 (moodboard decomposition), 3 (design principles), 4 (UX language guide)
**Also delivered since:** 5 (information architecture), see [`INFORMATION_ARCHITECTURE.md`](./INFORMATION_ARCHITECTURE.md) · 6 (design tokens), see [`DESIGN_TOKENS.md`](./DESIGN_TOKENS.md)
**Still outstanding:** 7 (component specification), 8 (wireframes), 9 (responsive and controller audit), 10 (anti-vibecoding review)

### Evidence discipline

The brief requires labeling observation, inference, recommendation, and unresolved choice. Two additional labels are used for sources:

- **Read directly:** I fetched the page and read it.
- **Via search summary:** surfaced by search, not fetched in full. Treated as a lead, not as proof.

Where the two conflicted, the primary source wins. One conflict occurred and is recorded in the source matrix.

---

## 1. Source matrix

### 1.1 Apple, awards and interface guidance

| Field | Detail |
|---|---|
| Work | 2026 Apple Design Award winners |
| Authority | Apple, the awarding body itself |
| URL | https://developer.apple.com/design/awards/ |
| Status | **Read directly** |
| Relevant winners | Interaction: **Sago Mini Jinja's Garden**. Inclusivity: **Guitar Wiz** (app), **Pine Hearts** (game). Visuals and Graphics: **Tide Guide** (app), **Cyberpunk 2077 Ultimate Edition** (game, macOS) |

**Conflict recorded:** a search summary reported *Moonlitt* as the app-category Interaction winner. The official Apple page lists Sago Mini Jinja's Garden for Interaction. The official page is treated as authoritative and the search claim is discarded.

**Why it works:** the Interaction winner is cited for controls that let a player act without reading instructions. Sago Mini Jinja's Garden uses swipe-to-move so children explore rather than parse a tutorial. Pine Hearts winning Inclusivity as a *game* matters more here than any app winner, because it shows a commercial game shipping accessibility as a design position rather than a settings page.

**Transfers to Reborn:** the "no tutorial required" standard maps exactly onto product principle 3 in the blueprint, "a controller should work without a tutorial." Our controller setup step should be a live input test the user can pass by doing, not a diagram they have to read.

**Does not transfer:** Apple's 2026 Liquid Glass material language. It is a platform aesthetic tied to Apple's rendering stack, it is explicitly the kind of glass treatment the anti-vibecoding guardrails prohibit, and we ship on Windows and Linux first.

---

### 1.2 Console and controller-first patterns

| Field | Detail |
|---|---|
| Work | Press Start: How Controllers Shaped Video Game Design |
| Authority | Figma, design publication |
| URL | https://www.figma.com/blog/press-start-video-game-navigation/ |
| Status | Via search summary |

**Observed principle:** the back action is bound to a hardware button, circle on PlayStation and B on Xbox, so consistently that many games omit a visible back control entirely. Top-level section switching is bound to shoulder buttons rather than to on-screen tabs.

**Transfers:** Reborn's seven main sections (Home, Play, Online, Mods, Music, Stats, Settings) should be reachable by shoulder buttons from anywhere, and B or circle must always mean back without a visible target. The bottom hint strip in the blueprint's shell diagram is where those bindings get shown.

**Does not transfer:** omitting the visible back control entirely. We support mouse and keyboard as first-class inputs, so a visible back affordance stays. It just should not be the only route.

---

| Field | Detail |
|---|---|
| Work | Steam Deck developer documentation and 10-foot interface practice |
| Authority | Valve (Steamworks), Wikipedia for the 10-foot definition, Solid Digital |
| URLs | https://partner.steamgames.com/doc/steamdeck · https://en.wikipedia.org/wiki/10-foot_user_interface · https://www.soliddigital.com/blog/designing-10-foot-user-experience |
| Status | Via search summary |

**Observed principles:**

- A 10-foot interface uses larger text and elements and consequently shows **fewer items at once** than a desktop layout.
- Directional-only input makes traversing large interface areas expensive, so **a visible focus point is what keeps the user oriented**.
- **Hover tooltips must be replaced by information shown outright**, which benefits mouse users too.

**Transfers, and this one is load-bearing:** the moodboard's Steam library reference is dense, and density plus 10-foot legibility are in direct tension. The resolution is not one layout that compromises both. It is **two density modes bound to a breakpoint**, with the Steam Deck and television layout showing fewer, larger items and the desktop layout showing the dense grid. The blueprint already asks for reflow rather than shrink; this is the concrete rule behind it.

The hover-tooltip rule also directly satisfies one of the brief's prohibited outcomes, "hover-only interactions."

---

### 1.3 Material 3, states and focus

| Field | Detail |
|---|---|
| Work | Material Design 3, Applying States |
| Authority | Google |
| URL | https://m3.material.io/foundations/interaction/states/applying-states |
| Status | Via search summary |

**Observed:** Material 3 uses semi-transparent **state layers** for hover, focus, pressed, and dragged. Only one state layer applies at a time, and focus additionally gets **a visible focus ring** rather than relying on the layer alone. States should be visually similar to the resting component and not drastically alter it.

**Transfers:** the layered-state model is a good engineering pattern and we should adopt the "one state layer at a time" rule, which prevents the muddy compound states that make controller focus ambiguous.

**Does not transfer, deliberately:** Material's restraint about states not drastically altering the component. That is correct for a mouse-driven Android app and **wrong for a 10-foot controller interface**. The blueprint requires focus to combine border, elevation or scale, and a background change. Our focus state should be more assertive than Material's, and the ripple should not be used at all.

---

### 1.4 Accessibility, the hard requirements

| Field | Detail |
|---|---|
| Work | WCAG 2.2 |
| Authority | W3C |
| URL | https://www.w3.org/WAI/WCAG22/Understanding/ |
| Status | Via search summary, supported by TetraLogical and WebAIM overviews |

**Observed criteria that bind this product:**

- **2.4.11 Focus Appearance (AA):** the focus indicator must have at least **3:1 contrast** against adjacent colors, must be at least as large as a **2 px perimeter** of the unfocused element, and the focused element must not be entirely hidden.
- **1.4.11 Non-text Contrast:** icons, focus indicators, and control boundaries need **3:1** against their background.
- **2.5.8 Target Size (Minimum, AA): 24 CSS pixels.** 2.5.5 Enhanced is 44 px at AAA.

**Recommendation:** adopt **WCAG 2.2 AA as the internal bar**, with two deliberate exceptions where we go beyond it:

1. **Target size 44 px, not 24 px**, on every controller-reachable control. AAA here is cheap and the Steam Deck makes 24 px genuinely painful.
2. **Focus ring 3 px, not 2 px**, because our focus state also carries brand red and needs to survive on top of full-bleed hero artwork.

**Unresolved choice:** whether the focus ring is red or a neutral high-contrast color. Red is the brand accent, but a red ring over the red-heavy hero art may fail 3:1 locally. Resolved in the token deliverable, likely with a two-tone ring (dark inner, light outer) that passes on any backdrop.

---

| Field | Detail |
|---|---|
| Work | Game accessibility baselines |
| Authority | IGDA Game Accessibility SIG; AbleGamers Includification |
| URLs | https://igda-gasig.org/how/game-accessibility-top-ten-se/ · https://accessible.games/wp-content/uploads/2018/11/AbleGamers_Includification.pdf |
| Status | Via search summary |

**Observed baselines:** remappable controls, adjustable control sensitivity, axis inversion, and left-handed modes are treated as **baseline expectations, not features**. Visual baselines are 4.5:1 text contrast, UI scalable to 200%, never color alone, a colorblind mode, respecting reduced-motion, and a high-contrast mode.

**Transfers:** the blueprint's controller settings list (dead zones, calibration, trigger thresholds, rumble, sensitivity) already matches this. The gap is **left-handed and axis-inversion modes**, which the blueprint does not mention. Recommend adding them to the controller service.

**Also transfers, and it is a design constraint on the palette:** "never color alone" plus a **red** primary accent means competitive states cannot be communicated by hue. Win, loss, ranked, casual, private, online, and offline all need an icon or a text label alongside color. The blueprint already says this for online versus offline; it needs to apply to results too.

---

### 1.5 UX writing and error microcopy

| Field | Detail |
|---|---|
| Work | Error message microcopy practice |
| Authority | Practitioner sources, no single authority |
| URLs | https://medium.com/design-bootcamp/creating-human-centered-error-messages-a-ux-microcopy-guide-6f79f8a586b7 · https://www.justinmind.com/ux-design/microcopy |
| Status | Via search summary |

**Observed formula:** an error needs three parts, **what happened, why, and what to do next**, in plain words, positioned next to the thing that failed, blaming nobody. Named anti-patterns are vague ("Something went wrong"), accusatory ("You entered an invalid value"), and technical ("Error 422: unprocessable entity").

**Transfers with one amendment.** The brief already requires error copy to state what failed, what remains safe, and what to do next. That **four-part** shape is better than the industry three-part shape for this product specifically, because Reborn errors happen during operations that touch a user's saves, a 1.5 GB game build, or a ranked result. "What remains safe" is the part that stops a user from panicking and reinstalling. Keep the brief's version.

**Does not transfer:** the cheerful register in the cited examples ("Looks like there's a typo, try again!"). That is the cute-over-clear failure the brief prohibits.

---

### 1.6 Sources deliberately not used

- **PS3 XMB**, present in the moodboard. Studied as a caution, not a model. Its horizontal cross-media bar hides depth behind a single axis and is a known wayfinding problem at distance.
- **Discord onboarding case studies.** Searched, and what surfaced was student and speculative redesign work rather than authoritative material. Discord's actual onboarding is used as a **direct visual reference from the moodboard**, not via secondary analysis. Labeled honestly rather than padded with weak citations.
- **Awwwards, D&AD, Red Dot, iF.** The brief asks for these "where the actual interface and judging context can be verified." Not yet done. Recorded as an outstanding gap rather than filled with unverifiable award badges.

---

## 2. Moodboard decomposition

Source: [`Reborn Launcher Moodboard.png`](./Reborn%20Launcher%20Moodboard.png), ten regions.

Observation is what is visibly in the image. Inference is my reading of why it is there.

### Region A, Steam library page (Blender)

**Observed:** left rail listing installed titles at small type with per-item icons. Full-bleed hero header with the product logo over artwork. Green LAUNCH button, left-aligned, immediately under the hero. Below it a two-column split: an activity and news feed on the left, friends and metadata panels on the right. Type is small, line height tight, and a great deal fits above the fold.

**Inferred:** this is the density target for the **shell**, not for every page. Steam gets away with small type because it is a desktop-first application with a pointer.

**Transfers:** persistent left navigation, a hero that does not consume the whole viewport, and the principle that the primary action sits immediately below the hero rather than floating in it.

**Does not transfer:** the type size, at Steam Deck and television distance.

### Region B, Assassin's Creed II detail page

**Observed:** near-full-bleed character art bleeding to a light background. Box art thumbnail at the left of a foreground information panel. A tight metadata table (Time Played, Last Played, Release Date, Platform). Two buttons, Play and Game Options, with Play visually dominant. Body description text below.

**Inferred:** the metadata table earns its place because it answers "what is this and have I played it," which is exactly the question the Play page must answer.

**Transfers directly to Play Now:** hero art, a foreground panel, cover art, **one dominant action**, and compact metadata as a table rather than as decorative cards. This is the single strongest reference in the board for our Play page.

### Region C, Discord onboarding card

**Observed:** dark card. Small label "Question 1 of 1" plus a red "Required" tag. A single large question in plain language. Four option cards in a two-by-two grid, each with an icon at the left and one or two lines of text. **One card is selected and carries a checkmark badge at its corner plus a lighter fill.** A single filled primary button, "Finish," bottom left. Explanatory secondary text sits to the right of the button, not above it.

**Inferred:** the progress label is deliberately small and the question is deliberately large. The hierarchy says the question matters and the position in the sequence is reassurance, not the point.

**Transfers to onboarding, nearly literally in structure:** step label small, question large, two to four option cards, selection indicated by fill plus border plus a check badge (three signals, which also satisfies "never color alone"), one dominant continue action, secondary explanation subordinate to it.

**Does not transfer:** Discord's blurple, its rounded pill buttons, and its icon style. Structure yes, skin no.

### Region D, near-black settings panel

**Observed:** true near-black background. Narrow left rail of category labels in lowercase (appearance, accessibility, video, audio, metadata, local processing, instances, privacy, advanced), one highlighted with a subtle filled pill. Content is grouped by small headings with a paragraph of explanatory text under each. Controls are inline segmented buttons (4k, 8k, 1440p, 720p, 480p, 360p, 540p), dropdowns, and toggles at the right edge. Type is small and monospace-adjacent. Dense, calm, almost no color.

**Inferred:** the explanatory paragraph under each setting is what makes the density tolerable. It is not a tooltip, it is always visible, which matches the Steam Deck rule from region A's platform.

**Transfers to Settings, strongly:** category rail, always-visible plain-language description per setting, segmented controls for small enumerations, right-aligned toggles, restrained color. This is also the answer for advanced Dolphin settings, where the blueprint demands every unfamiliar option carry a plain-language description.

### Region E, dark teal data dashboard

**Observed:** near-black surfaces with large numerals (432,502 · 29,242 · $232.40 · 600 · 215) as the dominant visual element. A single teal accent used for the primary data series, progress arcs, and one filled control. Sparklines and a smooth area chart. A calendar widget. Layered cards at varying elevation with soft shadows.

**Inferred:** the teal is used sparingly enough that it reads as "this is the data" rather than as decoration.

**Transfers to Stats:** headline numerals for record, Elo, and streak. Teal or cyan as the single data accent. Restrained charts.

**Direct conflict resolved:** this board region is teal-primary and our brand accent is red. **Recommendation: teal stays the data color, red stays the brand and action color.** They must not be interchangeable. Red on a chart would read as "loss" or "error" and would be misinterpreted immediately in a competitive stats context.

### Region F, NBA 2K discovery page

**Observed:** dark shell with a slim left rail (Discover, Explore, Library). Search field at the top. Large hero carousel with pagination dots, showing a mod pack with player art. Below it a three-across row of content cards, each with artwork, a bold title, and two lines of description. Further down, a "2026 Playoffs" section with a "See All" link and a dense grid of smaller cards. A "Recents" list in the left column with small thumbnails. A "Downloads" item pinned at the bottom of the rail.

**Inferred:** this is the closest analogue in the whole board to what Home and Mods need to be, because it is the same content shape: curated packs with art, by creator, with an install action.

**Transfers to Home and Mods:** hero carousel, horizontal rails, "See All" as the route from rail to full catalog, downloads pinned in the navigation rather than in a separate manager. That last one is useful, because the blueprint explicitly rejected a dedicated download manager page but still needs download state visible.

### Region G, Steam app concept

**Observed:** labeled "Steam App Concept | WIP." Very dense. Top tab bar, a large blue promotional banner pair, then multiple tightly packed grids of cover art with almost no padding, and a horizontal filmstrip along the bottom.

**Inferred:** included as a density ceiling rather than a target. At this density there is no room for a controller focus ring.

**Transfers:** the browse or grid view for the full catalog, at desktop breakpoints only.

**Explicitly does not transfer:** this density on Steam Deck. It would violate both the focus-visibility and target-size rules.

### Region H, PS3 XMB game page

**Observed:** the PS3 cross-media bar with a horizontal icon row, a large game thumbnail, and a long wall of unformatted description text at the right over a photographic background.

**Inferred:** included for era and tone, not as a structural model.

**Transfers:** nothing structurally. Studied as an anti-pattern: unformatted text walls at 10 feet, and depth hidden behind a single horizontal axis.

### Region I, Nintendo Switch Mario Odyssey page

**Observed:** full-bleed key art dominating the frame. Bottom-left foreground panel with box art, title, year, developer, file size (5.26 GB), a five-star rating, genre and platform chips, then a metadata table (Time Played, Last Played, Completion Status). A green Play button with a play glyph. Description text below, and a scrollbar indicating more content. A close X at the top right of the panel.

**Inferred:** this is region B's pattern again, executed on a console. The repetition across two regions is the board telling us this is the Play page, not a suggestion.

**Transfers:** confirms the Play Now composition, and adds two useful details, a visible file size and a completion or last-played line. Playtime tracking is already on the accepted small-touches list, so it has a home here.

### Region J, Reborn welcome card

**Observed:** deep blue to indigo gradient background. The NSR app icon centered near the top with a rounded-square mask. Two lines of centered text, "Welcome to NFL Street Reborn" then "Let's customize your experience." Nothing else.

**Inferred:** this is the only region that is actually Reborn's own, so it is the clearest statement of intended tone: warm, short, no chrome.

**Transfers:** the onboarding welcome screen composition and its register.

**Conflict, and it needs a decision:** this card is **blue and indigo**, which is where the blueprint's electric-blue proposal came from. Adam has since chosen Reborn red as the primary accent. **Recommendation: keep the composition, change the gradient.** A deep near-black to dark red gradient, or near-black with a red edge glow. The centered icon, the two-line copy, and the emptiness are what make this screen work, not the hue.

### Cross-region synthesis

| Page | Primary reference | Secondary |
|---|---|---|
| Shell | A (Steam library) | F (2K rail) |
| Play Now | B and I (console detail pages) | A (action placement) |
| Onboarding | C (Discord cards) | J (welcome tone) |
| Settings | D (near-black settings) | none |
| Stats | E (dark dashboard) | B (metadata tables) |
| Home and Mods | F (2K discovery) | G (grid density, desktop only) |

**Observation worth stating plainly:** six distinct page templates, not one layout repeated. The board is itself an argument against the "left sidebar plus an arbitrary grid of identical cards on every page" outcome the brief prohibits.

---

## 3. Product design principles

Ten principles. Each is testable, which means a reviewer can hold a screen against it and get a yes or no rather than an opinion.

**1. Every screen answers one question.**
Play answers "can I play right now." Online answers "who am I playing." Stats answers "how am I doing." A screen that answers two questions is two screens.
*Test:* state the screen's question in one sentence. If the sentence needs "and," split it.

**2. The primary action is unmistakable and there is exactly one.**
From regions B and I, both of which give the play action a distinct fill and a glyph, with everything else secondary.
*Test:* screenshot the page, blur it, and point at the primary action. If two candidates survive the blur, the page fails.

**3. Focus is never ambiguous, and never a color change alone.**
Focus combines a 3 px ring at 3:1 minimum contrast, a background change, and a scale or elevation shift. It survives on top of full-bleed artwork.
*Test:* photograph the screen at 10 feet, in grayscale. The focused element must still be identifiable.

**4. Red means Reborn and it means act. Teal means data. Neither ever means the other.**
Red is the brand and the primary action. Teal or cyan is data, live status, and progress. Error red must be distinguishable from brand red by icon and text, not hue.
*Test:* find any red pixel on a chart, or any teal button that performs an action. Both are failures.

**5. State is communicated at least twice.**
Every status, win, loss, ranked, casual, private, online, offline, installed, updating, needs repair, carries a shape or an icon or a word in addition to color.
*Test:* render the page in grayscale. Every state must still be readable.

**6. Nothing important is revealed only by hovering.**
From the Steam Deck rule. If information matters, it is on the page. Tooltips supplement, they never carry.
*Test:* disconnect the mouse. Anything you can no longer learn is a defect.

**7. Density is a breakpoint, not a compromise.**
Desktop gets the dense grid from regions A and G. Steam Deck and television get fewer, larger items. There is no single middle setting that serves both.
*Test:* the same page at 2560x1440 and at 1280x800 should differ in item count, not only in scale.

**8. Advanced settings are compact but never unexplained.**
Every setting carries a plain-language description that is always visible, plus its current value and whether changing it requires a restart or a rebuild.
*Test:* pick any Dolphin setting. A player who has never heard of Dolphin can say what it does from the screen alone.

**9. Destructive and slow operations state what stays safe.**
Any operation touching saves, a game build, mods, or a ranked result says what it will do, what it will not touch, and what to do if it fails.
*Test:* read the copy. If it does not name what survives, it is incomplete.

**10. The user never sees the machinery.**
No Dolphin vocabulary in ordinary flows, no raw console output, no file paths, no hashes, no NetPlay browser. All of it remains reachable in Settings for the people who want it.
*Test:* search the screen's copy for Dolphin terms. In an ordinary flow, there should be zero outside Settings and the About page.

---

## 4. UX language guide

### Voice

Concise, confident, human, specific. It sounds like a competent person who runs this thing, not an assistant and not a brand. It never apologizes for the product and never celebrates the user for completing a routine step.

**Never used, per the brief:** "Let's dive in," "Unlock the power of," "Elevate your experience," "Seamlessly," "Supercharge," "Oops! Something went wrong," and "You're all set!" where a more informative completion exists.

**Also banned, added from this research:** cheerful deflection of real failures ("Looks like there's a typo, try again!"), technical codes shown alone, and blame directed at the user.

**Writing rules that apply everywhere:** US English. No em dashes. Sentence case for everything except proper nouns. Numerals for all numbers. Second person. Present tense. Active voice.

### Error copy has four parts

What failed. Why. What remains safe. What to do next.

The third part is specific to this product and is not optional. Reborn operations touch memory cards, a 1.5 GB game build, and competitive results, and a user who does not know their saves are fine will reinstall.

### By area

**Onboarding**

| | |
|---|---|
| Good | "Connect a controller. We'll set it up and match the button prompts to it." |
| Bad | "Let's get you set up! 🎮" |
| Good | "This takes about four minutes. You can change any of it later in Settings." |
| Bad | "You're all set!" |
| Good, on completion | "Ready. Xbox controller on player 1, Balanced graphics, 47.2 GB installed." |

The completion line names what was decided. That is what makes it informative rather than congratulatory.

**Play and launching**

| | |
|---|---|
| Good | "Play Now" |
| Bad | "Launch Game (Offline Build v2.1)" |
| Good | "Starting NFL Street Reborn. This takes a few seconds the first time after an update." |
| Bad | "Booting DOL via Dolphin runtime..." |

**Online matchmaking**

| | |
|---|---|
| Good | "Searching for a ranked match. 40 to 90 ms preferred." |
| Bad | "Finding you the perfect opponent..." |
| Good | "Opponent found. Bill (1,247). Ready up in 0:14." |
| Good, on cancel | "Left the queue. Your rating did not change." |

That last one is principle 9 applied to a non-destructive action. In a ranked context, "did the thing I just do cost me points" is always the user's real question.

**Mods and music**

| | |
|---|---|
| Good | "Installed. Enabled in your offline game. Online games always use the standard soundtrack." |
| Bad | "Mod applied successfully!" |
| Good | "Rebuilding your offline game. About 4 minutes. Your saves and online game are untouched." |
| Bad | "Patching ISO..." |

**Updates and background work**

| | |
|---|---|
| Good | "Update ready. 210 MB. Installs the next time you close the game." |
| Bad | "A new version is available. Update now?" |
| Good, when blocking | "Ranked needs version 2.4. Downloading now, 40 seconds left. Offline play still works." |

**Settings**

| | |
|---|---|
| Good | "Internal resolution. Higher looks sharper and costs frame rate. Currently 3x (1920x1584). Recommended for your PC: 3x." |
| Bad | "Internal Resolution: 3x" |
| Good | "Changing this rebuilds your offline game. About 4 minutes." |

**Errors and recovery**

| | |
|---|---|
| Good | "The game closed unexpectedly. Your saves and stats are safe. This usually means a mod conflict. Try starting with mods disabled." |
| Bad | "Oops! Something went wrong." |
| Bad | "Error 422: unprocessable entity" |
| Good | "Couldn't reach the Reborn servers. Offline play, mods, and music all still work. We'll keep trying in the background." |
| Good | "Your online game doesn't match the current version. We can repair it now, about 90 seconds. Your offline game and custom music aren't affected." |

Every one of those names what stays safe. That is the pattern.

**Privacy and account**

| | |
|---|---|
| Good | "Reborn collects no analytics and no usage data. Offline games stay on this PC. Ranked and casual results are public because they affect ratings." |
| Bad | "We value your privacy." |
| Good | "Online play connects you directly to your opponent, so they can see your IP address. That's how the netcode works and it isn't unique to Reborn." |

Plain disclosure, no euphemism, and the last clause removes the implication that something is wrong.

**Competitive results and disputes**

| | |
|---|---|
| Good | "Held for review. You and your opponent reported different scores. Your rating hasn't changed and won't until a moderator looks at it." |
| Bad | "Match result pending." |
| Good | "Won. +18. 2,214 to 2,232." |
| Good | "Your opponent disconnected at 24-16. Held for review. Nothing has changed yet." |

Never invent certainty the system does not have. "Held for review" is a better word than "pending" because it names who acts next.

---

## Outstanding gaps, stated honestly

1. **Awwwards, D&AD, Red Dot, and iF work is not yet cited.** The brief asks for it where judging context is verifiable. Not done.
2. **Discord's onboarding is referenced only through the moodboard image**, not through primary documentation. What surfaced in search was speculative student work and was rejected rather than cited.
3. **Dense desktop settings applications** were not researched as a category beyond moodboard region D.
4. **Sports and competitive interfaces** were not researched beyond region F. ESPN, Faceit, and similar rank and box-score presentations are worth a pass.
5. **Deliverables 5 through 10 are not started:** information architecture, design tokens, component specification, wireframes, responsive and controller audit, anti-vibecoding review.

Nothing above is implemented. No UI has been built, per the brief.

## Decisions, resolved by Adam August 29, 2026

1. **Focus ring: neutral two-tone.** Not brand red. Red is reserved for selection. Specified in [`DESIGN_TOKENS.md`](./DESIGN_TOKENS.md) section 1.8.
2. **Welcome screen gradient: near-black to dark red.** Moodboard region J composition kept, hue moved off blue. Token in section 1.9.
3. **Target size: 44 px on controller-reachable controls**, deferring to the research. 24 px retained for mouse-only affordances. Section 4. Costs roughly a third of the rows on a dense settings page, accepted.
4. **Left-handed and axis-inversion presets: not shipping.** Adam: someone can do it themselves. Full remapping already reaches both, so this removes one-click presets only, not the capability.

## Sources

- [Apple Design Awards, 2026 winners and finalists](https://developer.apple.com/design/awards/)
- [Apple reveals winners of the 2026 Apple Design Awards](https://www.apple.com/newsroom/2026/06/apple-reveals-winners-of-the-2026-apple-design-awards/)
- [Press Start: How Controllers Shaped Video Game Design, Figma](https://www.figma.com/blog/press-start-video-game-navigation/)
- [Steam Deck, Steamworks Documentation](https://partner.steamgames.com/doc/steamdeck)
- [10-foot user interface](https://en.wikipedia.org/wiki/10-foot_user_interface)
- [Designing for the 10-Foot User Experience, Solid Digital](https://www.soliddigital.com/blog/designing-10-foot-user-experience)
- [Material Design 3, Applying States](https://m3.material.io/foundations/interaction/states/applying-states)
- [Understanding WCAG 2.2, W3C](https://www.w3.org/WAI/WCAG22/Understanding/)
- [What's new in WCAG 2.2, TetraLogical](https://tetralogical.com/blog/2023/10/05/whats-new-wcag-2.2/)
- [WCAG 2.2 Overview and Feedback, WebAIM](https://webaim.org/blog/wcag-2-2-overview-and-feedback/)
- [Game Accessibility Top Ten, IGDA Game Accessibility SIG](https://igda-gasig.org/how/game-accessibility-top-ten-se/)
- [A Practical Guide to Game Accessibility, AbleGamers Includification](https://accessible.games/wp-content/uploads/2018/11/AbleGamers_Includification.pdf)
- [Creating human-centered error messages, a UX microcopy guide](https://medium.com/design-bootcamp/creating-human-centered-error-messages-a-ux-microcopy-guide-6f79f8a586b7)
- [Microcopy: examples and best practices for great UX, Justinmind](https://www.justinmind.com/ux-design/microcopy)
