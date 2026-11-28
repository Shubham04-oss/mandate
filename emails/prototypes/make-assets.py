# Builds the tryout pictures in assets/ from web/public/email/moon-dawn-v2-*.jpg.
# Run from emails/: python3 -I prototypes/make-assets.py
from pathlib import Path

from PIL import Image, ImageChops, ImageFilter, ImageStat

SRC = Path(__file__).resolve().parents[2] / "web/public/email"
OUT = Path(__file__).resolve().parent / "assets"
OUT.mkdir(exist_ok=True)
PAGE = {"night": (0x04, 0x05, 0x0F), "light": (0xF3, 0xF4, 0xFA)}


def load(field):
    return Image.open(SRC / f"moon-dawn-v2-{field}.jpg").convert("RGB")


def save(im, name, q=78):
    im.save(OUT / name, "JPEG", quality=q, optimize=True, progressive=True, subsampling=0)
    print(name, im.size, (OUT / name).stat().st_size, "bytes")


def mix(a, b, t):
    return tuple(round(x + (y - x) * t) for x, y in zip(a, b))


def taller_light():
    """A: 1:1 instead of 2:1, by extending the sky upward, so 100% auto covers the figure at
    phone width; the foot fades into the sky's own colour, the figure's fallback, so a figure
    taller than the picture (narrow phones) runs on seamlessly."""
    src = load("light")
    w, h = src.size
    extra = w - h
    sky_tone = tuple(round(v) for v in ImageStat.Stat(src.crop((0, 0, w, 24))).mean)
    deep = tuple(round(v * 0.8) for v in sky_tone)
    # Each column continues its own top colour, blurred so the extension has no seam or banding.
    top = src.crop((0, 0, w, 24)).resize((w, 1), Image.Resampling.BOX).resize((w, extra))
    top = top.filter(ImageFilter.GaussianBlur(12))
    grad = Image.linear_gradient("L").resize((w, extra))  # 0 at the top, 255 at the seam
    sky = Image.composite(top, Image.new("RGB", (w, extra), deep), grad)
    # Faint grain, like the photo's own, so JPEG does not band the smooth extension into blocks.
    grain = Image.effect_noise((w, extra), 4).convert("RGB")
    sky = Image.blend(sky, ImageChops.add(sky, grain, 1.0, -128), 0.5)
    im = Image.new("RGB", (w, h + extra))
    im.paste(sky, (0, 0))
    im.paste(src, (0, extra))
    px = im.load()
    start = int(im.height * 0.84)
    for y in range(start, im.height):
        t = ((y - start) / (im.height - 1 - start)) ** 1.2
        for x in range(w):
            px[x, y] = mix(px[x, y], deep, t)
    save(im, "a-light-tall.jpg", 72)
    print("  fallback (sky, faded foot)", "#%02x%02x%02x" % deep)


def strip(field):
    """C: a 3.25:1 strip around the dawn arc and the ridge (~100 px tall on a phone)."""
    im = load(field).crop((0, 170, 1072, 500))
    save(im, f"c-strip-{field}.jpg")


def fade(field):
    """E: the picture fades into the page colour at the bottom, so text can follow on the page."""
    im = load(field)
    w, h = im.size
    start = int(h * 0.62)
    px = im.load()
    for y in range(start, h):
        t = ((y - start) / (h - 1 - start)) ** 1.4
        for x in range(w):
            px[x, y] = mix(px[x, y], PAGE[field], t)
    save(im, f"e-fade-{field}.jpg")


taller_light()
for f in ("night", "light"):
    strip(f)
    fade(f)
