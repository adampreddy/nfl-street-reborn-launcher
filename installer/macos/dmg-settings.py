# dmgbuild settings for the player's .dmg. Run by build-release.sh:
#   dmgbuild -s dmg-settings.py -D app=<.app> -D art=<dir> -D icon=<.icns> "NFL Street Reborn" <dmg>
# dmgbuild writes the window layout itself (no Finder scripting), so the
# build stays headless. Icon centers match dmg-background.py.
import os

app = defines["app"]  # noqa: F821 (dmgbuild provides `defines`)
art = defines["art"]  # noqa: F821
name = os.path.basename(app)

format = "UDZO"
files = [app]
symlinks = {"Applications": "/Applications"}
icon = defines["icon"]  # noqa: F821
background = os.path.join(art, "background.png")

window_rect = ((200, 120), (660, 428))
default_view = "icon-view"
show_status_bar = show_tab_view = show_toolbar = show_pathbar = show_sidebar = False
icon_size = 112
text_size = 13
icon_locations = {name: (165, 215), "Applications": (495, 215)}
hide_extensions = [name]
