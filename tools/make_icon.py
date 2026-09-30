"""Draw the original OpenPremier OP monogram (GPL-3.0-or-later).

A medium-weight geometric "OP" on the violet rounded square: strokes are about 15 % of the
letter height, the O is slightly narrower than it is tall and overshoots the P, and the P has a
round bowl, so the mark reads like set type rather than a heavy logo. The PNG sizes, the Windows
icon and the SVG are produced from the same geometry.

Run: python tools/make_icon.py [--preview out.png]
"""
import sys
from pathlib import Path

from PIL import Image, ImageDraw

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "assets" / "icons"
SVG = ROOT / "assets" / "branding" / "openpremier.svg"
S = 1024
VIOLET = (52, 16, 107, 255)
WHITE = (255, 255, 255, 255)


def geometry(weight=0.155):
    """Shapes in 1024-unit space."""
    cap = 420.0
    top = 512 - cap / 2
    base = 512 + cap / 2
    over = 7.0  # round letters overshoot the flat ones
    sv = cap * weight  # vertical strokes
    sh = sv * 0.88  # horizontal strokes are optically thinner
    o_w, o_h = 356.0, cap + 2 * over
    gap = 46.0
    p_w = 292.0
    bowl_h = cap * 0.56
    total = o_w + gap + p_w
    x = 512 - total / 2
    o = {
        "cx": x + o_w / 2,
        "cy": 512.0,
        "rx": o_w / 2,
        "ry": o_h / 2,
        "irx": o_w / 2 - sv,
        "iry": o_h / 2 - sh,
    }
    px = x + o_w + gap
    r = bowl_h / 2
    ri = (bowl_h - 2 * sh) / 2
    p = {
        "x0": px,
        "top": top,
        "base": base,
        "stem": sv,
        "right": px + p_w,
        "bowl_bottom": top + bowl_h,
        "r": r,
        "ri": ri,
        "sh": sh,
        "sv": sv,
    }
    return o, p


def draw(weight=0.155, scale=4):
    """The icon at 1024 px, drawn at `scale` times the size and reduced for smooth edges."""
    n = S * scale
    k = float(scale)
    img = Image.new("RGBA", (n, n), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    d.rounded_rectangle([40 * k, 40 * k, (S - 40) * k, (S - 40) * k], radius=210 * k, fill=VIOLET)
    o, p = geometry(weight)
    # O
    d.ellipse([(o["cx"] - o["rx"]) * k, (o["cy"] - o["ry"]) * k, (o["cx"] + o["rx"]) * k, (o["cy"] + o["ry"]) * k], fill=WHITE)
    d.ellipse([(o["cx"] - o["irx"]) * k, (o["cy"] - o["iry"]) * k, (o["cx"] + o["irx"]) * k, (o["cy"] + o["iry"]) * k], fill=VIOLET)
    # P: bowl (outer, then the counter), then the stem
    x0, top, bb, right, r = p["x0"], p["top"], p["bowl_bottom"], p["right"], p["r"]
    d.rounded_rectangle([x0 * k, top * k, right * k, bb * k], radius=r * k, fill=WHITE)
    d.rectangle([x0 * k, top * k, (x0 + r) * k, bb * k], fill=WHITE)
    xi0 = x0 + p["stem"]
    xi1 = right - p["sv"]
    yi0, yi1 = top + p["sh"], bb - p["sh"]
    ri = (yi1 - yi0) / 2
    d.rounded_rectangle([xi0 * k, yi0 * k, xi1 * k, yi1 * k], radius=ri * k, fill=VIOLET)
    d.rectangle([xi0 * k, yi0 * k, min(xi0 + ri, xi1 - ri) * k, yi1 * k], fill=VIOLET)
    d.rectangle([x0 * k, top * k, (x0 + p["stem"]) * k, p["base"] * k], fill=WHITE)
    return img.resize((S, S), Image.LANCZOS)


def svg(weight=0.155):
    o, p = geometry(weight)
    f = lambda v: f"{v:.1f}".rstrip("0").rstrip(".")
    cx, cy = o["cx"], o["cy"]
    o_path = (
        f"M{f(cx - o['rx'])} {f(cy)}a{f(o['rx'])} {f(o['ry'])} 0 1 0 {f(2 * o['rx'])} 0"
        f"a{f(o['rx'])} {f(o['ry'])} 0 1 0 {f(-2 * o['rx'])} 0Z"
        f"M{f(cx - o['irx'])} {f(cy)}a{f(o['irx'])} {f(o['iry'])} 0 1 0 {f(2 * o['irx'])} 0"
        f"a{f(o['irx'])} {f(o['iry'])} 0 1 0 {f(-2 * o['irx'])} 0Z"
    )
    x0, top, bb, right, r, base = p["x0"], p["top"], p["bowl_bottom"], p["right"], p["r"], p["base"]
    xi0 = x0 + p["stem"]
    xi1 = right - p["sv"]
    yi0, yi1 = top + p["sh"], bb - p["sh"]
    ri = (yi1 - yi0) / 2
    p_path = (
        f"M{f(x0)} {f(top)}H{f(right - r)}A{f(r)} {f(r)} 0 0 1 {f(right - r)} {f(bb)}"
        f"H{f(xi0)}V{f(base)}H{f(x0)}Z"
        f"M{f(xi0)} {f(yi0)}H{f(xi1 - ri)}A{f(ri)} {f(ri)} 0 0 1 {f(xi1 - ri)} {f(yi1)}H{f(xi0)}Z"
    )
    return (
        '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024" role="img" '
        'aria-labelledby="title desc">\n'
        '  <title id="title">OpenPremier</title>\n'
        '  <desc id="desc">Original white OP monogram on a flat violet rounded square.</desc>\n'
        '  <rect x="40" y="40" width="944" height="944" rx="210" fill="#34106b"/>\n'
        f'  <path fill="#fff" fill-rule="evenodd" d="{o_path}"/>\n'
        f'  <path fill="#fff" fill-rule="evenodd" d="{p_path}"/>\n'
        "</svg>\n"
    )


def preview(path):
    """Variants side by side at three sizes, to compare weights."""
    weights = [0.24, 0.17, 0.155, 0.14]
    sheet = Image.new("RGBA", (len(weights) * 300, 420), (40, 40, 44, 255))
    for i, w in enumerate(weights):
        big = draw(w)
        sheet.paste(big.resize((256, 256), Image.LANCZOS), (i * 300 + 22, 10))
        sheet.paste(big.resize((64, 64), Image.LANCZOS), (i * 300 + 22, 290))
        sheet.paste(big.resize((32, 32), Image.LANCZOS), (i * 300 + 110, 306))
        sheet.paste(big.resize((16, 16), Image.LANCZOS), (i * 300 + 170, 314))
    sheet.save(path)


def main():
    if len(sys.argv) > 2 and sys.argv[1] == "--preview":
        preview(sys.argv[2])
        return
    OUT.mkdir(parents=True, exist_ok=True)
    big = draw()
    for size in (16, 32, 48, 64, 128, 256, 512):
        big.resize((size, size), Image.LANCZOS).save(OUT / f"openpremier-{size}.png")
    big.save(OUT / "openpremier-1024.png")
    big.resize((256, 256), Image.LANCZOS).save(
        OUT / "openpremier.ico",
        sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)],
    )
    SVG.write_text(svg(), encoding="utf-8", newline="\n")


if __name__ == "__main__":
    main()
