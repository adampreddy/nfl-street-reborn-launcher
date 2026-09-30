# Mica spike result (2026-09-30)
WIN11_MICA_OK: yes
WIN10_ACRYLIC_OK: untested
TITLEBAR_OK: yes
Notes:
1. Mica visible: yes, but subtle. Dark-mode Mica is a flat ~(32,32,32) wallpaper-tinted fill, not a blur of windows behind. The sidebar went from opaque (37,38,35) to (20,20,23) once the spike made nav/main translucent, so it is the OS backdrop showing through. Body-only tint (the brief's CSS) is not enough: `.shell-nav` and `main` carry their own opaque backgrounds and must be made translucent in Task 3. Screenshots: target/spike/01-normal.png, 02-moved-left.png, 03-moved-right.png (v1/ = body-only tint, sidebar still opaque).
2. Title bar: normal OS decorations with Mica fill (32,32,32) and working min/max/close, no black band, no white flash seen in stills (01-normal.png, 06-restored-after-maximise.png). Focus-change flash not checkable by stills.
3. Maximise/restore by ShowWindow: OK, 05-maximised.png, 06-restored-after-maximise.png. Win+Z snap layouts and resize flicker: needs Adam.
4. Minimise then restore: OK, Mica preserved, 04-restored-after-minimise.png.
5. Fullscreen Dolphin round-trip: needs Adam (not run; would launch the game against Adam's data).
6. Windows light/dark toggle: needs Adam. This PC is in dark mode (AppsUseLightTheme=0); light-mode Mica will be lighter, so our tint must be verified there.
7. Win 10 Acrylic: untested, no Win 10 machine. Acrylic drag lag unknown.
Spike deviation: tauri.conf identifier changed to com.nflstreetreborn.launcher.micaspike on the spike branch only, because single-instance otherwise forwarded to Adam's running beta launcher. Spike commit is on spike/mica-window only.
