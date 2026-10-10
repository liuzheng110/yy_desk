# -*- coding: utf-8 -*-
"""Inspect every frame inside icon.ico: size and visible-content fill ratio."""
import sys
from PIL import Image

path = sys.argv[1]
ico = Image.open(path)
print(f"file={path}")
i = 0
while True:
    try:
        ico.seek(i)
    except EOFError:
        break
    w, h = ico.size
    rgba = ico.convert("RGBA")
    alpha = rgba.getchannel("A")
    bbox = alpha.getbbox()
    if bbox:
        l, t, r, b = bbox
        bw, bh = r - l, b - t
        fill = (bw * bh) / (w * h) * 100
        print(f"  frame {i}: {w}x{h} content=({l},{t},{r},{b}) fill={fill:.1f}%")
    else:
        print(f"  frame {i}: {w}x{h} content=none")
    i += 1
