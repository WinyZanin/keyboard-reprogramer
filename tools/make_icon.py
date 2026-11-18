"""Generates assets/icon.ico: a keycap with the play/pause symbol.

Usage: python tools/make_icon.py
Each size is drawn at 8x and downscaled with LANCZOS for smooth edges.
"""
from pathlib import Path

from PIL import Image, ImageDraw

SIZES = [16, 20, 24, 32, 40, 48, 64, 128, 256]
SS = 8  # supersampling

SHADOW = (0, 0, 0, 70)
CAP_SIDE = (31, 41, 55, 255)      # keycap side
CAP_TOP = (55, 65, 81, 255)       # top face
CAP_TOP_HI = (75, 85, 99, 255)    # highlight at the top of the face
OUTLINE = (107, 114, 128, 255)    # outline: makes the key stand out on dark backgrounds
GLYPH = (45, 212, 191, 255)       # play/pause in teal


def draw(size: int) -> Image.Image:
    s = size * SS
    img = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    u = s / 32  # unit: 32x32 grid

    # Small icons fill more of the area (less margin) to stay legible.
    m = 1.0 if size <= 24 else 1.5
    r_out = 6 * u

    # shadow, keycap side and top face
    d.rounded_rectangle([m * u, (m + 1.2) * u, s - m * u, s - (m - 1.2) * u], r_out, fill=SHADOW)
    d.rounded_rectangle([m * u, m * u, s - m * u, s - m * u], r_out, fill=CAP_SIDE,
                        outline=OUTLINE, width=max(int(0.7 * u), SS))
    face = [(m + 2.6) * u, (m + 1.6) * u, s - (m + 2.6) * u, s - (m + 4.2) * u]
    d.rounded_rectangle(face, 4.2 * u, fill=CAP_TOP)
    if size >= 32:
        d.rounded_rectangle([face[0] + 1.2 * u, face[1] + 0.8 * u, face[2] - 1.2 * u, face[1] + 3.2 * u],
                            1.6 * u, fill=CAP_TOP_HI)

    # play/pause centered on the face
    cx = (face[0] + face[2]) / 2
    cy = (face[1] + face[3]) / 2 + 0.3 * u
    h = 10.5 * u if size > 24 else 13 * u
    tri_w = h * 0.8
    bar_w = h * 0.22
    gap = h * 0.22
    total = tri_w + gap + bar_w * 2 + gap * 0.8
    x0 = cx - total / 2
    d.polygon([(x0, cy - h / 2), (x0 + tri_w, cy), (x0, cy + h / 2)], fill=GLYPH)
    bx = x0 + tri_w + gap
    for i in range(2):
        x = bx + i * (bar_w + gap * 0.8)
        d.rounded_rectangle([x, cy - h / 2, x + bar_w, cy + h / 2], bar_w * 0.25, fill=GLYPH)

    return img.resize((size, size), Image.LANCZOS)


def main() -> None:
    root = Path(__file__).resolve().parent.parent
    out = root / "assets" / "icon.ico"
    images = [draw(n) for n in SIZES]
    images[-1].save(out, format="ICO", sizes=[(n, n) for n in SIZES], append_images=images[:-1])
    images[-1].save(root / "assets" / "icon.png")
    print(f"generated {out}")


if __name__ == "__main__":
    main()
