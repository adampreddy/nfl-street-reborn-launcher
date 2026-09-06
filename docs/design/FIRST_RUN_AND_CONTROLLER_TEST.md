# Setup and controller testing

The updated executable includes installation detection and file browsing inside setup, automatic controller scanning, and a live input tester on both the Controllers page and the controller setup step.

Use **Controllers → Start controller test**, then press a button on your controller to make it visible. Select the device in the list. Button tiles show pressure, including analog triggers; stick axes show raw positions. **Esc** or **Finish controller test** returns to normal launcher navigation. A and B do not navigate while the test is running.

The tester reads Windows WebView's Gamepad API. It checks input reaching the launcher, not Dolphin's in-game bindings. Some adapters may be visible to Dolphin but unavailable to this tester. Devices without a standard mapping display numbered buttons and axes. No calibration or bindings are changed by the tester.

Setup now saves the confirmed image for Play, checks that it is a readable, nonempty supported image file, and preserves the selected graphics preset. This is not a content hash or game-integrity check. Completion requires detection of both the game and Dolphin. Controller rescanning clears the previous selection, and the backend rejects selection if the device identity changed before confirmation.

Verification: 12 launcher Rust tests passed, including missing/empty/directory/wrong-extension image cases. Browser tests cover installation detection, failed path saves, confirmation of a chosen image, automatic controller scanning, stale-selection invalidation, live button/trigger/stick readings, unplug/reconnect, nonstandard input labels, Escape handling, navigation isolation, and active-session lockout. Existing launcher regression checks also pass.

Screenshots without “native” in their name use simulated input and fixture installations. Physical-controller and full-game testing remain to be done. The complete installer has not been rebuilt; use the standalone executable delivered here.
