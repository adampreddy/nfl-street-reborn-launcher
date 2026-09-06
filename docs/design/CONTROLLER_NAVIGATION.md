# Controller navigation update

Open **NFL Street Reborn Launcher - Controller Update.exe**.

| Input | Action |
| --- | --- |
| D-pad / left stick | Move between controls, using whole choice rows |
| A / Cross | Select; on a path input, open its Browse action |
| B / Circle | Cancel dropdown editing, close options, or return to the sidebar; from a secondary page's sidebar, return to Play |
| LB / RB or L1 / R1 | Previous / next page; disabled during the ordered setup flow |
| Right stick | Scroll the page |
| View + Menu or Share + Options | Leave the live controller tester |

Left moves from page content to the sidebar when no control remains on that row. Right returns to the last focused content control. Focus has an explicit outline and highlights an entire radio/checkbox choice. Dropdowns use A to begin editing, D-pad to choose, A to apply, and B to cancel.

The navigation requires centered sticks and released buttons after entering a page, reconnecting, returning to the window, or finishing a test. This prevents held inputs from activating a new screen. Controller mode survives page changes. Keyboard and mouse input remain available.

Validation: the expanded browser regression suite passes, including held-input protection, full-row movement, sidebar return, bumper navigation across actual pages, scrolling, closing options from a radio, and exiting live testing with the controller chord. Existing setup and launcher flows remain covered. Physical-controller testing is still needed to assess feel and hardware-specific mappings.

Navigation supports controllers exposed by WebView with a standard Gamepad mapping. Devices with raw mappings can be inspected in the live tester, but are not guessed into menu controls. Native Windows file dialogs and manual text entry may still require a keyboard or mouse. This update does not remap controls inside Dolphin.
