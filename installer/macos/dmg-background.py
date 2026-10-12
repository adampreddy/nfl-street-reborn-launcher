"""Draw the .dmg window background (1x and @2x) from the launcher's own art.

Usage: python dmg-background.py <repo> <out dir>
Writes <out>/background.png and <out>/background@2x.png; dmgbuild joins them.
Light on purpose: Finder draws icon labels in black on any background image.
The layout (content size, icon centers) is shared with dmg-settings.py.
"""
import sys
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

# 28 px taller than the layout needs: Finder may or may not count its title
# bar in the window height, and the bottom stripes read either way.
W, H = 660, 428
APP_X, APPS_X, ICON_Y = 165, 495, 215
RED = (214, 32, 39, 255)
INK = (40, 42, 48, 255)

repo, out = Path(sys.argv[1]), Path(sys.argv[2])
out.mkdir(parents=True, exist_ok=True)


def font(size, bold=False):
    names = ["/System/Library/Fonts/SFNS.ttf", "/System/Library/Fonts/Helvetica.ttc"]
    for f in names:
        try:
            ft = ImageFont.truetype(f, size)
            if bold:
                try:
                    ft.set_variation_by_name("Semibold")
                except (OSError, ValueError):
                    pass
            return ft
        except OSError:
            pass
    return ImageFont.load_default()


def draw(s):
    w, h = W * s, H * s
    # Soft white-to-cool-gray, top to bottom.
    bg = Image.new("RGBA", (w, h))
    px = ImageDraw.Draw(bg)
    for y in range(h):
        t = y / h
        c = tuple(round(a + (b - a) * t) for a, b in zip((250, 250, 251), (231, 233, 237)))
        px.line((0, y, w, y), fill=c + (255,))

    # Street stripes: two slanted bars along the bottom edge.
    d = ImageDraw.Draw(bg)
    base = h - 50 * s
    d.polygon([(0, base), (w, base - 26 * s), (w, base - 18 * s), (0, base + 8 * s)], fill=RED)
    d.polygon([(0, base + 14 * s), (w, base - 12 * s), (w, h), (0, h)], fill=(28, 44, 92, 255))

    logo = Image.open(repo / "assets/NSRLOGO.png").convert("RGBA")
    lw = 250 * s
    logo = logo.resize((lw, round(logo.height * lw / logo.width)), Image.LANCZOS)
    bg.alpha_composite(logo, ((w - lw) // 2, 18 * s))

    # Chevron arrow from the app to Applications.
    y = ICON_Y * s
    for i, x in enumerate((290, 318, 346)):
        x *= s
        k = 0.45 + 0.275 * i  # fade in toward Applications
        a = tuple(round(245 + (c - 245) * k) for c in RED[:3]) + (255,)
        d.line([(x, y - 16 * s), (x + 16 * s, y), (x, y + 16 * s)], fill=a, width=6 * s, joint="curve")

    caption = "Drag NFL Street Reborn into Applications"
    f = font(15 * s, bold=True)
    tw = d.textlength(caption, font=f)
    d.text(((w - tw) / 2, 318 * s), caption, font=f, fill=INK)
    return bg.convert("RGB")


draw(1).save(out / "background.png")
draw(2).save(out / "background@2x.png")
