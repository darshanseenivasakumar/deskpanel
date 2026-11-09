"""Compose docs/images/hero.png from the README screenshots (see make-screenshots.ps1)."""
from PIL import Image, ImageDraw, ImageFont

IMG = "docs/images/"
panels = [("On your desktop", "panel-dark.png"), ("Popped over an app", "panel-popped.png"), ("Light theme", "panel-light.png")]
def rounded(im, r=30, inset=5):
    """Clip to the panel's rounded shape (the screenshot crop includes a few px of wallpaper)."""
    im = im.crop((inset, inset, im.width - inset, im.height - inset))
    mask = Image.new("L", im.size, 0)
    ImageDraw.Draw(mask).rounded_rectangle((0, 0, im.width - 1, im.height - 1), r, fill=255)
    out = Image.new("RGBA", im.size, (0, 0, 0, 0))
    out.paste(im, (0, 0), mask)
    return out


imgs = [(label, rounded(Image.open(IMG + f).convert("RGBA"))) for label, f in panels]

PAD, GAP, TOP, BOTTOM = 56, 40, 96, 72
w = sum(i.width for _, i in imgs) + GAP * (len(imgs) - 1) + PAD * 2
h = max(i.height for _, i in imgs) + TOP + BOTTOM

# soft vertical gradient background (dark graphite -> deep indigo)
bg = Image.new("RGBA", (w, h))
px = bg.load()
for y in range(h):
    t = y / (h - 1)
    c = (int(20 + 18 * t), int(21 + 20 * t), int(28 + 46 * t), 255)
    for x in range(w):
        px[x, y] = c

font = ImageFont.truetype(r"C:\Windows\Fonts\segoeuib.ttf", 28)
small = ImageFont.truetype(r"C:\Windows\Fonts\segoeui.ttf", 20)
d = ImageDraw.Draw(bg)
d.text((PAD, 28), "DeskPanel", font=font, fill=(245, 245, 247, 255))
d.text((PAD + 150, 36), "Apple-style control panel that lives on your Windows desktop, under your apps", font=small, fill=(170, 175, 195, 255))

x = PAD
for label, im in imgs:
    # soft shadow
    sh = Image.new("RGBA", (im.width + 80, im.height + 80), (0, 0, 0, 0))
    ImageDraw.Draw(sh).rounded_rectangle((40, 52, 40 + im.width, 52 + im.height), 30, fill=(0, 0, 0, 130))
    from PIL import ImageFilter
    sh = sh.filter(ImageFilter.GaussianBlur(18))
    bg.alpha_composite(sh, (x - 40, TOP - 52 + 0))
    bg.alpha_composite(im, (x, TOP))
    tw = d.textlength(label, font=small)
    d.text((x + (im.width - tw) / 2, TOP + im.height + 22), label, font=small, fill=(200, 205, 225, 255))
    x += im.width + GAP

bg.convert("RGB").save(IMG + "hero.png", optimize=True)
print("hero", bg.size)
