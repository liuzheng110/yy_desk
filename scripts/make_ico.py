# -*- coding: utf-8 -*-
"""Create a multi-resolution icon.ico (16..256) from a large transparent PNG.

Windows picks the nearest matching frame for the taskbar/desktop/titlebar,
so a multi-frame ICO renders consistently at every size.

Usage: python scripts/make_ico.py <input.png> <output.ico>
"""
import sys

from PIL import Image

SIZES = [(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)]


def main():
    src, dst = sys.argv[1], sys.argv[2]
    im = Image.open(src).convert("RGBA")
    largest = max(SIZES)
    base = im.resize(largest, Image.LANCZOS)
    base.save(dst, format="ICO", sizes=SIZES)
    print(f"saved={dst} frames={len(SIZES)} sizes={SIZES} from={im.size}")


if __name__ == "__main__":
    main()
