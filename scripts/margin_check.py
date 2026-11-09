"""Compare screenshots with/without the panel inside the window's transparent margin.
A positive mean delta in a margin zone = the 'transparent' area is lightening the desktop (a veil).
Usage: python scripts/margin_check.py with.png without.png L T R B scale
"""
import sys
from PIL import Image

a = Image.open(sys.argv[1]).convert("RGB")
b = Image.open(sys.argv[2]).convert("RGB")
L, T, R, B = map(int, sys.argv[3:7])
m = int(40 * float(sys.argv[7]))


def stats(box, name):
    pa, pb = list(a.crop(box).get_flattened_data()), list(b.crop(box).get_flattened_data())
    d = [sum(x) / 3 - sum(y) / 3 for x, y in zip(pa, pb)]
    print(f"{name:30s} mean delta {sum(d) / len(d):+6.2f}   max {max(d):+6.1f}   min {min(d):+6.1f}")


stats((L + m + 30, B - m + 6, R - m - 30, B - 2), "bottom margin (shadow zone)")
stats((L + 2, T + m + 40, L + m - 6, B - m - 40), "left margin")
stats((R - m + 6, T + m + 40, R - 2, B - m - 40), "right margin")
stats((L + m + 60, T + 2, R - m - 60, T + m - 8), "top margin")
stats((L - 60, T + 100, L - 10, B - 100), "outside window (control)")
# Outermost 3 px of the window: must be ~0 if the shadow fades out before the edge (not clipped).
stats((L + m, B - 3, R - m, B), "EDGE ring bottom (must be ~0)")
stats((L, T + m, L + 3, B - m), "EDGE ring left (must be ~0)")
stats((R - 3, T + m, R, B - m), "EDGE ring right (must be ~0)")
