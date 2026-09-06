# Launcher rebuild, September 5, 2026

The active player interface is now implemented in the six HTML entry pages,
`launcher.css`, and `app-shell.js`. The older `styles.css` and `onboarding.css`
still provide base styles for existing controls. `shell.css` is an unused legacy
stylesheet; `harness.html` remains the developer command interface.

## Player experience

- Play leads with existing Reborn artwork, the logo, and one primary action.
- Missing game files offer setup. Game preferences and installation details sit
  in a disclosure below the controller, mod, and graphics shortcuts.
- Mods present exclusive choices together, with prompt settings and synchronization
  details collapsed below the mod selection.
- Settings provides native file/folder browsing and explicit Save actions. A
  failed write retains the last saved value.
- Setup resumes its journal and can be restarted after an explicit acknowledgment.
- Recovery shows the newest session and offers file verification from a selected
  component manifest. Automatic repair remains unavailable in the backend.

## Behavior and preservation

The existing Rust core still owns game preparation, controller bindings, mod
composition, save locations, migration, and session records. This change does not
replace Dolphin, import a game automatically, or rewrite game content.

The installation override is persisted in settings and shared by setup and launch.
Preflight and Play resolve the same explicit or saved User folder. Browser pages
hydrate saved paths before constructing their first requests. The launch guard
prevents a second launch in the same process even after page navigation, and its
state is available to the interface. The existing cross-process session lock is
still enforced by the core.

The launcher reports a session as starting or active while waiting for the core.
It does not claim that Dolphin has started before receiving evidence. A refused
launch is shown as an error and re-enables Play.

## Navigation and rendering

Tab, Enter, and Escape work alongside standard-mapped browser gamepads. D-pad or
left stick moves focus; the primary face button activates; the secondary button
backs out. Held activation buttons are ignored after focus returns. Input polling
stops acting while the window is inactive or a tracked game session is active.
Focus is restored across page redraws, including temporarily disabled controls.

Layouts are checked at 800 x 600, 960 x 600, 1280 x 800, and 1920 x 1080. Reduced
motion preferences are respected. Gamepad support has browser-simulated coverage;
physical controller validation remains a separate hardware check.

## Verification

Run the core and existing checks:

```powershell
.\Tools\test-all.ps1
```

Run browser interactions and screenshot captures (Microsoft Edge on Windows):

```powershell
npm install --prefix Tools
.\Tools\test-all.ps1 -Browser
```

The browser suite uses an isolated fixture bridge and never launches Dolphin,
updates the application, or writes to the player's settings. Its screenshots
default to `target/browser-review`. Set `REBORN_TEST_BROWSER=chrome` to use Chrome.

The native Windows executable has also been opened against the installed game
and checked through its actual WebView. A live game session, physical gamepad,
network updater, and rebuilt full installer are separate from these checks.
