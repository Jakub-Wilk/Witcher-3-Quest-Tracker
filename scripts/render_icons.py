# /// script
# requires-python = ">=3.10"
# dependencies = ["resvg-py>=0.2", "pillow>=11"]
# ///
"""Renders assets/logo.svg into the app icons. Run with `uv run scripts/render_icons.py`.

- assets/icon.png: 256x256 window icon, embedded in the binary at compile time.
- assets/icon.ico: multi-size icon embedded into the .exe by build.rs.
"""

import io
from pathlib import Path

import resvg_py
from PIL import Image

ASSETS = Path(__file__).resolve().parent.parent / "assets"
SVG = ASSETS / "logo.svg"
ICO_SIZES = [16, 20, 24, 32, 40, 48, 64, 128, 256]


def render(size: int) -> Image.Image:
    png = resvg_py.svg_to_bytes(svg_path=str(SVG), width=size, height=size)
    return Image.open(io.BytesIO(bytes(png))).convert("RGBA")


def main() -> None:
    frames = {size: render(size) for size in ICO_SIZES}
    frames[256].save(ASSETS / "icon.png", optimize=True)
    # Each size is rendered from the vector source rather than downscaled, so small sizes stay crisp.
    frames[256].save(
        ASSETS / "icon.ico",
        sizes=[(s, s) for s in ICO_SIZES],
        append_images=[frames[s] for s in ICO_SIZES if s != 256],
    )
    print(f"Wrote {ASSETS / 'icon.png'} and {ASSETS / 'icon.ico'}")


if __name__ == "__main__":
    main()
