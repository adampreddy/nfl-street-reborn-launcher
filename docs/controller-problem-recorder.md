# Recording a controller problem

In the updated launcher, open **Controllers → Record controller problem**.

1. Close the game. Use your mouse to open the recorder.
2. Describe what happens and choose whether it affects the launcher, game, or both. Select the connection type and any controller software you use, if known.
3. Choose the affected controller in “Controller seen by the game.” If it is missing, leave “Not listed” selected; browser input can still be recorded.
4. Click **Begin recording**. Each prompt asks for a direction or button. Release the controls, click **Record input**, then follow the “Now” instruction. Only move the affected controller. Skip controls your device does not have.
5. Complete the prompts, or choose **Review report** to finish early. Choose **Save report** and attach the JSON file when reporting the problem.

The full guided recording takes about two minutes. Canceling the file picker keeps the report available for another save attempt. Closing the recorder discards the unsaved recording and asks Dolphin's input reader to stop.

## What the report contains

- The player's description, affected screen category, connection type, and controller-software answer.
- Browser controller names and layouts, plus raw axes and button values sampled approximately 20 times per second during each five-second prompt.
- Focus state and controller presence in every sample, to distinguish disconnects and background-window behavior.
- Saved launcher calibration for raw browser layouts.
- Dolphin's detected input expression for the selected controller and the same prompt, or an explicit unavailable/no-input/error status.
- Launcher version, operating system/architecture, the selected runtime's SHA-256 fingerprint, runtime controller/input names, saved controller bindings, tuning, player assignments, and allowlisted fields from the game's controller configuration. The saved custom user directory is respected when reading that configuration.

No automatic upload occurs. The recorder does not export general settings, logs, credentials, memory cards, profile serial-number fields, arbitrary config sections, or screenshots. It does not record keyboard events. Text the player enters in the description is included. The native input reader is restricted to a selected controller; keyboard/mouse devices cannot be selected.

Reports use schema `nsr.controller-report.v1`. Raw recording size, sample counts, device counts, dimensions, field names, and numeric ranges are validated before saving. An unavailable runtime still permits a browser-only report. Recording does not change mappings, assignments, or game files.

## Replaying a report

From the launcher repository:

```powershell
node Tools/replay-controller-report.mjs "C:\reports\reborn-controller-report.json"
```

The tool reads JSON only and replays its samples through the production launcher input reader. For each prompt it prints the expected direction, observed normalized directions, generated menu events, and Dolphin's reported binding. It flags an observed direction different from the prompt. Unknown raw layouts without calibration are marked unnormalized; the tool does not guess their meanings.

Compare the runtime binding with `context.saved_mappings` and `context.game_pad_config` to determine whether the game mapping references the actual control. Multiple visible browser devices and their indices help identify virtual/physical duplicates. The runtime fingerprint identifies precisely which executable supplied the bindings.

This is a guided input recording made in the launcher. It does not record the game's current menu selection, disc patches, or gameplay state, and replay does not prove that a physical controller or an in-game menu works correctly. A report can narrow those failures to a stage of the input path without requiring the developer to own that controller.

## Verification

- Full Rust workspace: 1,216 passed. The browser-generated JSON fixture was explicitly passed through the Rust export validator during that run. Six tests were ignored by the normal suite, including two opt-in runtime tests; the new recorder's real-runtime lifecycle test was run separately and passed.
- Full browser regression suite passed, including the recorder tests.
- Recorder tests cover guided prompts, raw/runtime evidence, disconnects, wrong-direction replay, stored calibration, unknown layouts, canceled/failed saves and retry, cancellation during recording and startup, unavailable runtime, focus retention while typing during a slow scan, no mapping writes, and 800px/1280px layouts.
- A real Dolphin process successfully supplied context and a runtime fingerprint, started capture, and honored session-scoped shutdown. A stale dialog token did not stop the current session.

The Windows build remains a local test candidate. It has not been published or installed over a player's launcher.
