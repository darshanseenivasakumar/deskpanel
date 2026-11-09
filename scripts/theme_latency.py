"""D4: how long after Windows broadcasts a theme change does the panel repaint?
Flips AppsUseLightTheme, broadcasts ImmersiveColorSet (exactly like Settings), then polls one panel
pixel in a tight loop (no process start-up per sample). Restores the original theme afterwards.
Usage: python scripts/theme_latency.py <x> <y>
"""
import ctypes
import sys
import time
import winreg

u, g = ctypes.windll.user32, ctypes.windll.gdi32
u.SetProcessDpiAwarenessContext(ctypes.c_void_p(-4))
x, y = int(sys.argv[1]), int(sys.argv[2])
KEY = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize"


def pixel():
    dc = u.GetDC(0)
    c = g.GetPixel(dc, x, y)
    u.ReleaseDC(0, dc)
    return c


def set_apps(v):
    with winreg.OpenKey(winreg.HKEY_CURRENT_USER, KEY, 0, winreg.KEY_SET_VALUE) as k:
        winreg.SetValueEx(k, "AppsUseLightTheme", 0, winreg.REG_DWORD, v)
    r = ctypes.c_size_t()
    u.SendMessageTimeoutW(0xFFFF, 0x1A, 0, ctypes.c_wchar_p("ImmersiveColorSet"), 2, 2000, ctypes.byref(r))


with winreg.OpenKey(winreg.HKEY_CURRENT_USER, KEY) as k:
    orig = winreg.QueryValueEx(k, "AppsUseLightTheme")[0]
import threading

before = pixel()
t0 = time.perf_counter()
done = {}
th = threading.Thread(target=lambda: (set_apps(0 if orig else 1), done.setdefault("t", time.perf_counter())))
th.start()  # the broadcast blocks until every window has handled it; poll meanwhile
changed_epoch = None
samples = 0
while time.perf_counter() - t0 < 3:
    samples += 1
    if pixel() != before:
        changed_epoch = time.time() * 1000
        lat = (time.perf_counter() - t0) * 1000
        break
else:
    lat = None
th.join()
time.sleep(0.5)
set_apps(orig)
time.sleep(0.8)
print(f"from_broadcast_start_ms={None if lat is None else round(lat)} broadcast_total_ms={round((done['t'] - t0) * 1000)} pixel_changed_epoch={None if changed_epoch is None else int(changed_epoch)} samples={samples} restored={orig}")
