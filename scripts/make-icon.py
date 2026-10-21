#!/usr/bin/env python3
"""Draw the MasseyCode app icon.

Writes the multi-resolution Windows icon (``assets/masseycode.ico``, embedded by
``build.rs`` through ``windres``) and a 1024 px PNG used by the macOS bundle
script (``scripts/make-macos-app.sh`` sizes it into a ``.icns``).

Requires Pillow: ``python -m pip install pillow``.  The generated files are
committed, so this only has to run when the artwork changes.
"""

from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "assets"

INK = (16, 18, 22, 255)
ACCENT = (74, 158, 255, 255)

FONT_CANDIDATES = [
    "C:/Windows/Fonts/segoeuib.ttf",
    "C:/Windows/Fonts/arialbd.ttf",
    "/System/Library/Fonts/Helvetica.ttc",
    "/Library/Fonts/Arial Bold.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
]


def load_font(size: int) -> ImageFont.FreeTypeFont:
    for path in FONT_CANDIDATES:
        if Path(path).exists():
            try:
                return ImageFont.truetype(path, size)
            except OSError:
                continue
    return ImageFont.load_default()


def draw(size: int) -> Image.Image:
    image = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    canvas = ImageDraw.Draw(image)
    scale = size / 256

    margin = round(8 * scale)
    radius = round(48 * scale)
    outline = max(1, round(10 * scale))
    canvas.rounded_rectangle(
        (margin, margin, size - margin, size - margin),
        radius=radius,
        fill=INK,
        outline=ACCENT,
        width=outline,
    )

    font = load_font(round(150 * scale))
    text = "M"
    left, top, right, bottom = canvas.textbbox((0, 0), text, font=font)
    canvas.text(
        ((size - (right - left)) / 2 - left, (size - (bottom - top)) / 2 - top),
        text,
        font=font,
        fill=ACCENT,
    )
    return image


def main() -> None:
    OUT.mkdir(exist_ok=True)

    ico = draw(256)
    ico.save(
        OUT / "masseycode.ico",
        sizes=[(256, 256), (128, 128), (64, 64), (48, 48), (32, 32), (16, 16)],
    )
    draw(1024).save(OUT / "masseycode-1024.png")
    print("wrote", OUT / "masseycode.ico")
    print("wrote", OUT / "masseycode-1024.png")


if __name__ == "__main__":
    main()
