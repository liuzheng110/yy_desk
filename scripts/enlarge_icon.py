# -*- coding: utf-8 -*-
"""Enlarge the transparent-background icon so the visible content fills the canvas.

The original icon kept a large transparent margin around the rounded-square
subject, which made it render smaller than other apps' icons on the Windows
desktop/taskbar. This script crops to the alpha bounding box and scales the
subject up to ~95% of the canvas, centered.

Usage: python scripts/enlarge_icon.py <input.png> <output.png> [fill_ratio=0.95]
"""
import sys

from PIL import Image

FILL_RATIO = float(sys.argv[3]) if len(sys.argv) > 3 else 0.95


def main():
    src, dst = sys.argv[1], sys.argv[2]
    im = Image.open(src).convert("RGBA")
    w, h = im.size

    alpha = im.getchannel("A")
    bbox = alpha.getbbox()  # (left, top, right, bottom) of non-zero alpha
    if bbox is None:
        print("no visible content; leaving unchanged")
        im.save(dst, "PNG")
        return

    l, t, r, b = bbox
    subject = im.crop((l, t, r, b))
    sw, sh = subject.size
    print(f"size={w}x{h} subject_bbox=({l},{t},{r},{b}) subject={sw}x{sh}")

    # Scale the subject so its longer side is fill_ratio of the canvas.
    scale = (w * FILL_RATIO) / max(sw, sh)
    new_w = max(1, round(sw * scale))
    new_h = max(1, round(sh * scale))
    subject = subject.resize((new_w, new_h), Image.LANCZOS)

    # Paste centered.
    canvas = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    canvas.paste(subject, ((w - new_w) // 2, (h - new_h) // 2), subject)
    canvas.save(dst, "PNG")
    print(f"enlarged to {new_w}x{new_h} at fill={FILL_RATIO} saved={dst}")


if __name__ == "__main__":
    main()
