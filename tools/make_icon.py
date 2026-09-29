"""Draw the original OpenPremier OP monogram (GPL-3.0-or-later).

The icon deliberately uses flat geometry so the mark stays crisp at taskbar and launcher sizes.
Run: python tools/make_icon.py
"""
from pathlib import Path

from PIL import Image, ImageDraw

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "assets" / "icons"
S = 1024
VIOLET = (52, 16, 107, 255)
WHITE = (255, 255, 255, 255)


def draw():
    img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    d.rounded_rectangle([40, 40, S - 40, S - 40], radius=210, fill=VIOLET)

    # O: deliberately wide and geometric for recognition at 16 px.
    d.ellipse([145, 278, 548, 746], fill=WHITE)
    d.ellipse([258, 387, 435, 637], fill=VIOLET)

    # P: a strong vertical stem and a compact bowl, optically aligned with the O.
    d.rounded_rectangle([558, 278, 680, 746], radius=28, fill=WHITE)
    d.rounded_rectangle([618, 278, 872, 558], radius=132, fill=WHITE)
    d.rounded_rectangle([680, 382, 776, 466], radius=42, fill=VIOLET)
    return img


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    big = draw()
    for size in (16, 32, 48, 64, 128, 256, 512):
        big.resize((size, size), Image.LANCZOS).save(OUT / f"openpremier-{size}.png")
    big.save(OUT / "openpremier-1024.png")
    big.resize((256, 256), Image.LANCZOS).save(
        OUT / "openpremier.ico", sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)]
    )


if __name__ == "__main__":
    main()
