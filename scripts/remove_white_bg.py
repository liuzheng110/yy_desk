# -*- coding: utf-8 -*-
"""Remove the opaque white background around the app icon (rounded-square content)
and produce a transparent-background icon with feathered edges.

Usage: python scripts/remove_white_bg.py <input.png> <output.png>
"""
import sys
from collections import deque

from PIL import Image

WHITE_THRESHOLD = 247  # RGB channels all >= this => background white


def is_white(px):
    return px[0] >= WHITE_THRESHOLD and px[1] >= WHITE_THRESHOLD and px[2] >= WHITE_THRESHOLD and px[3] > 0


def main():
    src, dst = sys.argv[1], sys.argv[2]
    im = Image.open(src).convert("RGBA")
    w, h = im.size
    data = im.load()

    # 1) BFS flood fill from the four corners to mark the outside-white background.
    bg = bytearray(w * h)  # 1 = background
    q = deque()
    for x, y in ((0, 0), (w - 1, 0), (0, h - 1), (w - 1, h - 1)):
        if is_white(data[x, y]) and not bg[y * w + x]:
            bg[y * w + x] = 1
            q.append((x, y))
    while q:
        x, y = q.popleft()
        for nx, ny in ((x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)):
            if 0 <= nx < w and 0 <= ny < h and not bg[ny * w + nx] and is_white(data[nx, ny]):
                bg[ny * w + nx] = 1
                q.append((nx, ny))

    # 2) Outside background -> fully transparent; edge pixels -> alpha feathering.
    for y in range(h):
        row = y * w
        for x in range(w):
            idx = row + x
            if bg[idx]:
                r, g, b, a = data[x, y]
                data[x, y] = (r, g, b, 0)
                continue
            # pixel adjacent to background: feather the white contribution into alpha
            if (x > 0 and bg[idx - 1]) or (x < w - 1 and bg[idx + 1]) or \
               (y > 0 and bg[idx - w]) or (y < h - 1 and bg[idx + w]):
                r, g, b, a = data[x, y]
                white_part = min(r, g, b)
                if white_part > 0:
                    new_a = max(0, min(255, 255 - white_part))
                    # keep RGB, scale alpha only (white halo removed via transparency)
                    data[x, y] = (r, g, b, new_a)

    im.save(dst, "PNG")
    # report stats
    white = sum(1 for v in bg if v)
    print(f"size={w}x{h} bg_pixels={white} saved={dst}")


if __name__ == "__main__":
    main()
