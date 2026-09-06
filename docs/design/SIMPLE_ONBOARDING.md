# Simple onboarding

Open **NFL Street Reborn Launcher - Simple Onboarding.exe**.

Onboarding now uses a focused, Discord-inspired layout: one question per screen, large selectable choices, a compact progress indicator, a clear Continue action, and Finish later. The regular launcher sidebar is removed during setup.

Detected installations show a short confirmation. Paths and alternate file pickers live under Installation details; when detection fails, choosing an installation folder is the main action. Controller troubleshooting and the live tester are optional disclosures. The final review shows essential choices, with the full report available on request.

The existing backend still controls step order, resumability, migration, preparation, and validation. This pass simplifies the interface rather than bypassing those checks. Controller navigation and reduced-motion support remain available.

Browser regression and setup-readiness checks passed, including conditional steps, failed saves, editing a rejected game path, automatic scanning, and stale controller selection. Screenshots of the welcome and question screens use fixture installations. The existing setup journal was not reset to generate previews.

For an already configured installation, Settings → Resume game setup opens the completed setup page. Restarting remains an explicit choice there. This is a standalone launcher build; the full installer has not been rebuilt.
