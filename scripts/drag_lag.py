"""How many px does the panel's wallpaper ring lag the real wallpaper mid-drag?
Usage: python scripts/drag_lag.py <shot.png> <ref.png> L T R B scale
Shifts the reference horizontally/vertically and reports the shift that best matches the ring
(0 = perfectly aligned)."""
import sys
from statistics import mean

from PIL import Image

shot, ref = Image.open(sys.argv[1]).convert("L"), Image.open(sys.argv[2]).convert("L")
L, T, R, B = map(int, sys.argv[3:7])
s = float(sys.argv[7])
W, H = shot.size
i = int(37.5 * s)
pts = [(x, T + i) for x in range(L + int(70 * s), R - int(70 * s), 2)] + [(L + i, y) for y in range(T + int(70 * s), B - int(70 * s), 2)]
pts = [(x, y) for x, y in pts if 12 <= x < W - 12 and 12 <= y < H - 12]
best = None
for dy in range(-10, 11):
    for dx in range(-10, 11):
        e = mean(abs(shot.getpixel((x, y)) - ref.getpixel((x + dx, y + dy))) for x, y in pts)
        if best is None or e < best[0]:
            best = (e, dx, dy)
print(f"ring best match at dx={best[1]} dy={best[2]} (error {best[0]:.2f}); lag = {max(abs(best[1]), abs(best[2]))} px")
