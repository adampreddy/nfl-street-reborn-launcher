# Reborn launcher — moodboard pass

## Menu refinement

The latest pass replaces the library breadcrumb and icon-led sidebar with a quiet title bar and condensed text menu. The Play action and section headings share the same bundled typeface. Home shortcuts are a compact cluster instead of three equal-width columns. Mods and controller profiles use rows with restrained selection states. Repeated helper labels and marketing copy were removed; the court artwork and original wordmark remain unchanged.

Browser interaction checks, shell-page resilience, mod rules, and setup readiness checks passed again for this refinement. Responsive screenshots were reviewed at 800 and 1280 pixels wide, with automated layout coverage at 960 and 1920 as well.

The Playnite and console game-detail references drove the composition: full-window court artwork, the original game wordmark, a compact play cluster, and quiet navigation. The dense settings reference drove the inline installation controls. Removed the oversized slogan, marketing copy, and three boxed dashboard cards. Mods now use a compact two-column selection layout; controller choices include button symbols.

The original Reborn wordmark is used unchanged. The court background is an AI-edited illustration derived from the existing gameplay screenshot, not an unmodified screenshot of the game. Created using the built-in image_gen tool; saved in the source at `crates/reborn-ui/ui/art/court-night.png`. The original screenshot remains available.

Art prompt:

> Use case: precise-object-edit. Asset type: clean gameplay artwork for the full-window background of the NFL Street Reborn desktop launcher. Edit the provided image. Remove ALL game HUD: scoreboards, scores and team emblems over the image, FPS badge, replay lettering, turbo meter, player-selection marker on the ground. Reconstruct the court and environment naturally behind those overlays. Preserve the distinctive early-2000s low-poly game-rendered characters, their actual faces, clothes, team graphics, poses, proportions and the recognizable court. Do not turn them into photographs or newly designed realistic athletes. Composition: create a wide 16:9 image, extend the left side with the same empty urban court and subtle chain-link fence, shifting the main Giants ball carrier and nearby defenders to the right half (main player around 65% width). The left 35% needs room for a launcher wordmark and controls; it should be mostly court and moody architecture with no foreground players. Make the image crisp, with warm court light, cool night shadows, retaining authentic game texture detail and restrained grain. No text overlays, no new logos, no UI, no buttons, no borders. Do not add random lens flares, particles or neon. This is one background asset, NOT a mockup of an interface.

Validation for this pass: browser flows passed for Play failure handling, path persistence and failed saves, mod selection, controller scanning, keyboard focus, simulated gamepad navigation, setup, recovery, and missing-game states. Layout reviewed at 800, 960, 1280, and 1920 pixels wide. Static shell-page, mod-rule, and onboarding-readiness checks passed. The executable was rebuilt with the new embedded assets. Full-game and physical-controller testing remain outside this pass.
