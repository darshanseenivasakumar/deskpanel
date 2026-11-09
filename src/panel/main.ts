import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { jsLog, playAnim, type AnimName } from '../lib/anim';
import { initControls, setPanelVisible } from './controls';

type Glass = { url: string; screen_w: number; screen_h: number; mode: string; build_ms: number };
type Geom = { x: number; y: number; scale: number };

const panel = document.getElementById('panel') as HTMLElement;
const glassEl = document.getElementById('glass') as HTMLElement;
const backdropEl = document.getElementById('backdrop') as HTMLElement;

let glass: Glass | null = null;
let geom: Geom = { x: 0, y: 0, scale: window.devicePixelRatio };

const frame = () => new Promise<void>((r) => requestAnimationFrame(() => r()));

/**
 * The PNG represents the whole monitor. Size it to the monitor in CSS px and shift it by the
 * panel's on-screen position, so the blurred wallpaper lines up with the real one behind it.
 */
function layoutGlass() {
  if (!glass) return;
  const s = geom.scale;
  const size = `${glass.screen_w / s}px ${glass.screen_h / s}px`;
  glassEl.style.backgroundImage = `url("${glass.url}")`;
  glassEl.style.backgroundSize = size;
  glassEl.style.backgroundPosition = `${-(geom.x / s + panel.offsetLeft)}px ${-(geom.y / s + panel.offsetTop)}px`;
}

/** Window-sized crop of the real wallpaper, shown only in the 4px ring around the panel. */
async function loadBackdrop() {
  const b = await invoke<{ url: string; w: number; h: number } | null>('get_backdrop');
  if (!b) return;
  const i = new Image();
  i.src = b.url;
  await i.decode();
  const s = geom.scale;
  backdropEl.style.backgroundImage = `url("${b.url}")`;
  backdropEl.style.backgroundSize = `${b.w / s}px ${b.h / s}px`;
  backdropEl.style.backgroundPosition = `${-backdropEl.offsetLeft}px ${-backdropEl.offsetTop}px`;
}

async function loadGlass() {
  const g = await invoke<Glass>('get_glass');
  // Never show an undecoded (transparent) glass.
  const i = new Image();
  i.src = g.url;
  await i.decode();
  glass = g;
  layoutGlass();
  await loadBackdrop();
}

async function refreshGeom() {
  geom = await invoke<Geom>('get_geom');
  layoutGlass();
}

async function init() {
  await listen<AnimName>('anim', async (e) => {
    jsLog(`event anim ${e.payload}`);
    try {
      await playAnim(panel, e.payload);
    } catch (err) {
      jsLog(`anim ${e.payload} threw ${err}`);
    }
    await invoke('anim_done', { anim: e.payload });
  });
  await listen('glass-changed', loadGlass);
  // Desktop (region-clipped child of the desktop) vs popped (top-level, real transparency).
  await listen<boolean>('surface', (e) => {
    document.body.classList.toggle('popped', !e.payload);
    refreshGeom();
  });
  await listen('geom-changed', refreshGeom);
  await listen('backdrop-changed', loadBackdrop);
  await listen<boolean>('panel-visible', (e) => setPanelVisible(e.payload));
  setPanelVisible(await invoke<boolean>('is_panel_visible'));
  await listen<boolean>('theme', (e) => { document.documentElement.dataset.theme = e.payload ? 'dark' : 'light'; });
  // Pop-over glass: blurred capture of what is behind the popped panel (window-sized).
  const snapEl = document.getElementById('glass-snap') as HTMLElement;
  await listen<{ url: string; w: number; h: number }>('snapshot', async (e) => {
    const { url, w, h } = e.payload;
    const i = new Image();
    i.src = url;
    await i.decode();
    const s = geom.scale;
    snapEl.style.backgroundImage = `url("${url}")`;
    snapEl.style.backgroundSize = `${w / s}px ${h / s}px`;
    snapEl.style.backgroundPosition = `${-panel.offsetLeft}px ${-panel.offsetTop}px`;
    document.body.classList.add('has-snap');
  });
  await listen<boolean>('surface', (e) => {
    if (e.payload) document.body.classList.remove('has-snap');
  });

  // Drag from any non-interactive part of the panel (system move loop in Rust).
  const INTERACTIVE = 'button, .slider, .tile.small, .row-toggle, .ring-btn, .chips, #sw-digits, #timer-digits';
  // Report every press first (capture phase): lets Rust undo Explorer activating the desktop.
  panel.addEventListener('pointerdown', () => { invoke('panel_pressed'); }, { capture: true });
  panel.addEventListener('pointerdown', (e) => {
    if (e.button !== 0 || (e.target as HTMLElement).closest(INTERACTIVE)) return;
    invoke('drag_start');
  });

  document.addEventListener('keydown', (e) => {
    if (e.key === 'Escape') invoke('esc');
  });
  document.getElementById('close')!.addEventListener('click', () => invoke('close_panel'));
  await initControls(); // fill real values before the first visible frame
  // Element positions (CSS px in the window) for the automated tests in scripts/stage5-test.ps1.
  const rects: Record<string, number[]> = {};
  for (const id of ['wifi', 'bt', 'theme-tile', 'power-tile', 'brightness', 'volume', 'm-play', 'timer-toggle', 'timer-digits', 'sw-toggle', 'sw-second', 'close']) {
    const r = document.getElementById(id)!.getBoundingClientRect();
    rects[id] = [r.left, r.top, r.width, r.height].map((n) => Math.round(n * 10) / 10);
  }
  jsLog(`RECTS ${JSON.stringify(rects)}`);

  await document.fonts.ready;
  // Size the window to the content (panel = grid height), then read back the final geometry.
  const grid = document.querySelector('.grid') as HTMLElement;
  await invoke('fit_height', { cssPx: grid.offsetHeight });
  await frame();
  geom = await invoke<Geom>('get_geom');
  await loadGlass();
  // Two frames: styles applied + glass painted. Only now may Rust make the window visible.
  await frame();
  await frame();
  await invoke('ready');
}

init().catch((err) => console.error('init failed', err));
