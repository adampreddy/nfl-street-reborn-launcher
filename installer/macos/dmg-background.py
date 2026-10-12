"""Draw the .dmg window background (1x and @2x) in the launcher's own look.

Usage: python dmg-background.py <repo> <out dir>
Writes <out>/background.png and <out>/background@2x.png; dmgbuild joins them.
Colors and type follow crates/reborn-ui/ui/theme.css (DESIGN.md). Finder draws
icon labels in black on any background picture, even in Dark mode, so each
label sits on a light name tag. Layout is shared with dmg-settings.py.
"""
import io
import sys
from pathlib import Path

from fontTools.ttLib import TTFont
from PIL import Image, ImageDraw, ImageFilter, ImageFont

# 28 px taller than the layout needs: Finder may or may not count its title
# bar in the window height, and nothing important sits in that strip.
W, H = 660, 428
APP_X, APPS_X, ICON_Y, ICON = 165, 495, 215, 112
BG, BG_GLOW = (13, 14, 17), (35, 38, 45)  # --fallback-bg
FG_2 = (174, 181, 192)  # --fg-2
RED = (224, 39, 46)  # --red
TAG = (233, 235, 239)  # label name tag: light, so Finder's black label reads

repo, out = Path(sys.argv[1]), Path(sys.argv[2])
out.mkdir(parents=True, exist_ok=True)

# The launcher's Inter, unpacked from its woff2.
inter = io.BytesIO()
ttf = TTFont(repo / "crates/reborn-ui/ui/fonts/InterVariable.woff2")
ttf.flavor = None
ttf.save(inter)


def font(size, weight):
    inter.seek(0)
    f = ImageFont.truetype(inter, size)
    f.set_variation_by_axes([14, weight] if len(f.get_variation_axes()) > 1 else [weight])
    return f


def rounded(img, box, r, fill, outline=None):
    ImageDraw.Draw(img).rounded_rectangle(box, r, fill=fill, outline=outline)


def draw(s):
    w, h = W * s, H * s
    # --fallback-bg: radial #23262d at 50% 40% fading into #0d0e11.
    bg = Image.new("RGB", (w, h), BG)
    glow = Image.new("L", (w, h), 0)
    ImageDraw.Draw(glow).ellipse((w * 0.2, h * 0.1, w * 0.8, h * 0.7), fill=255)
    glow = glow.filter(ImageFilter.GaussianBlur(110 * s))
    bg = Image.composite(Image.new("RGB", (w, h), BG_GLOW), bg, glow).convert("RGBA")

    logo = Image.open(repo / "assets/NSRLOGO.png").convert("RGBA")
    lw = 236 * s
    logo = logo.resize((lw, round(logo.height * lw / logo.width)), Image.LANCZOS)
    bg.alpha_composite(logo, ((w - lw) // 2, 20 * s))

    # One glass card around both icons: --glass-1, --stroke, --r-lg, --sheen.
    card = Image.new("RGBA", (w, h))
    box = (48 * s, 140 * s, (W - 48) * s, 322 * s)
    rounded(card, box, 14 * s, (255, 255, 255, 12), (255, 255, 255, 23))
    ImageDraw.Draw(card).line((box[0] + 14 * s, box[1] + s, box[2] - 14 * s, box[1] + s),
                              fill=(255, 255, 255, 18), width=s)
    bg.alpha_composite(card)

    d = ImageDraw.Draw(bg)
    # Name tags where Finder draws each label, just under the icon.
    tag_top = (ICON_Y + ICON // 2 + 6) * s  # measured: Finder centers its label 74 pt below the icon center
    for x in (APP_X, APPS_X):
        rounded(bg, ((x - 78) * s, tag_top, (x + 78) * s, tag_top + 24 * s), 8 * s, TAG)

    # Chevrons in --red, fading in toward Applications.
    y = ICON_Y * s
    for i, x in enumerate((292, 318, 344)):
        k = 0.4 + 0.3 * i
        c = tuple(round(b + (r - b) * k) for r, b in zip(RED, BG_GLOW))
        x *= s
        d.line([(x, y - 14 * s), (x + 14 * s, y), (x, y + 14 * s)], fill=c, width=5 * s, joint="curve")

    caption = "Drag NFL Street Reborn into Applications"
    f = font(14 * s, 560)
    tw = d.textlength(caption, font=f)
    d.text(((w - tw) / 2, 340 * s), caption, font=f, fill=FG_2)
    return bg.convert("RGB")


draw(1).save(out / "background.png")
draw(2).save(out / "background@2x.png")
