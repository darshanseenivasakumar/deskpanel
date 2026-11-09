"""Stage 6 image checks.
  ring  <shot.png> <ref.png> L T R B scale   -> how well the wallpaper ring/glass lines up with the
                                               real wallpaper (mean |delta| on the 4px backdrop ring)
  offset <capture.png> <ref.png> L T         -> best (dx,dy) aligning the raw pop capture with what is
                                               really on screen behind the panel, and its error
"""
import sys
from statistics import mean

from PIL import Image


def ring(shot, ref, L, T, R, B, s):
    a, b = Image.open(shot).convert("RGB"), Image.open(ref).convert("RGB")
    W, H = a.size
    i = int(37.5 * s)  # middle of the 4px ring (panel inset 40, ring 36..40 css)
    pts = [(x, T + i) for x in range(L + int(70 * s), R - int(70 * s))]
    pts += [(L + i, y) for y in range(T + int(70 * s), B - int(70 * s))]
    pts = [(x, y) for x, y in pts if 0 <= x < W and 0 <= y < H]
    d = [abs(sum(a.getpixel(p)) - sum(b.getpixel(p))) / 3 for p in pts]
    print(f"ring mean|delta|={mean(d):.2f} max={max(d):.1f} n={len(d)}")


def offset(cap, ref, L, T):
    c = Image.open(cap).convert("L")
    r = Image.open(ref).convert("L")
    w, h = c.size
    best = None
    for dy in range(-4, 5):
        for dx in range(-4, 5):
            box = (L + dx, T + dy, L + dx + w, T + dy + h)
            rc = r.crop(box)
            # compare a central area (edges can differ: taskbar, other windows moving)
            cx, cy = w // 4, h // 4
            pa = list(c.crop((cx, cy, w - cx, h - cy)).get_flattened_data())
            pb = list(rc.crop((cx, cy, w - cx, h - cy)).get_flattened_data())
            err = mean(abs(x - y) for x, y in zip(pa[::7], pb[::7]))
            if best is None or err < best[0]:
                best = (err, dx, dy)
    print(f"best offset dx={best[1]} dy={best[2]} mean|delta|={best[0]:.2f}")


if __name__ == "__main__":
    if sys.argv[1] == "ring":
        ring(sys.argv[2], sys.argv[3], *map(int, sys.argv[4:8]), float(sys.argv[8]))
    else:
        offset(sys.argv[2], sys.argv[3], int(sys.argv[4]), int(sys.argv[5]))
