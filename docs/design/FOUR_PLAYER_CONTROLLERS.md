# Four-player controller setup

The Controllers page exposes four Dolphin GameCube ports. Each tab selects an input device, displays the connected model when recognized, and provides the GameCube buttons, sticks, D-pad, and triggers. Mappings save automatically per device; button textures remain shared by the game.

`Settings.controller_slots` is optional for backward compatibility. Without saved assignments, connected devices use the existing primary-controller-first order. Once a player assignment is changed, four explicit slots are saved. Assigning a device elsewhere moves it rather than duplicating it. A disconnected device keeps its reserved slot.

At launch, `build_gcpad_for_slots` generates the selected profiles and clears unused GCPad sections. `controller_port_overlay` enables the corresponding emulated controller ports in Dolphin.ini. Configuration changes use the existing launch guard; saves are not written into an active game session.

Live testing uses the browser Gamepad API. When multiple devices are connected, users explicitly select the live input to test; browser indexes must not be assumed to match Dolphin device indexes. Capture reads only this selected input and waits for neutral controls first. Unknown/nonstandard devices can use a Dolphin binding expression. Saved identity follows Dolphin's backend/device index, so identical physical devices can change identity if the operating system reorders them.

Validation:

- `node Tools/review-players.mjs`: four tabs, device moves, persistent assignments, disconnected slots, isolated input capture, manual binding edits, cancellation, reset, shared textures, and responsive layout using four simulated controllers.
- `node Tools/review-onboarding.mjs`: regression checks for the shared controller workspace in onboarding.
- `node Tools/run-browser-tests.mjs`: launcher navigation and page flows, followed by four-player checks.
- `cargo test --release -p reborn-core --lib`: includes independent four-port mapping, port activation, disconnected slots, duplicate prevention, and persisted settings.
- `cargo test --release -p reborn-ui --bin reborn-launcher`: native UI command checks.

Four physical controllers in a running multiplayer match still require hardware validation.
