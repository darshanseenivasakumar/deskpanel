import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { SPRINGS, springEasing } from '../lib/spring';

// Mirrors src-tauri/src/controls (only the fields the pill needs).
type TimerInfo =
  | { state: 'idle'; total_ms: number }
  | { state: 'running'; total_ms: number; end_epoch_ms: number }
  | { state: 'paused'; total_ms: number; remaining_ms: number };
type Controls = {
  media: { title: string; artist: string; app: string; playing: boolean; art_id: string } | null;
  timer: TimerInfo;
  stopwatch: { running: boolean; start_epoch_ms: number | null; banked_ms: number; laps: number[] };
};

const $ = (id: string) => document.getElementById(id) as HTMLElement;
const pill = $('pill');
const ctl = (op: string, args: Record<string, unknown> = {}) => invoke('ctl', { cmd: { op, ...args } }).catch(() => {});

const spring = springEasing(SPRINGS.snappy);
pill.style.setProperty('--ease', spring.easing);
pill.style.setProperty('--dur', `${spring.duration}ms`);

const COLLAPSED = { w: 220, h: 36, r: 18 };
const EXPANDED = { w: 340, h: 88, r: 28 };
const TOP = 8;

/** Clip the window to the pill shape, so the rest of the window never blocks clicks. */
function region(s: { w: number; h: number; r: number }) {
  const x = (window.innerWidth - s.w) / 2;
  return invoke('pill_region', { x, y: TOP, w: s.w, h: s.h, r: s.r });
}

// ---- hover expand (L4): grow the region BEFORE expanding, shrink it AFTER collapsing ----
let gen = 0;
const log = (msg: string) => invoke('js_log', { msg: `pill ${msg}` }).catch(() => {});
// re-render when shown/hidden so the countdown ticker also only runs while the pill is visible
listen<boolean>('pill-shown', () => { if (s) render(s); });
pill.addEventListener('pointerenter', async () => {
  const g = ++gen;
  log('hover -> expand');
  await region(EXPANDED);
  if (g === gen) pill.classList.add('expanded');
});
pill.addEventListener('pointerleave', () => {
  const g = ++gen;
  pill.classList.remove('expanded');
  setTimeout(() => { if (g === gen) region(COLLAPSED); }, spring.duration);
});

// ---- content ----
const pad = (n: number) => String(n).padStart(2, '0');
const mmss = (ms: number) => { const s = Math.max(0, Math.ceil(ms / 1000)); return `${pad(Math.floor(s / 60))}:${pad(s % 60)}`; };
const swFmt = (ms: number) => { const t = Math.floor(ms / 100); return `${pad(Math.floor(t / 600))}:${pad(Math.floor(t / 10) % 60)}.${t % 10}`; };
const PLAY = 'M8 5v14l11-7z', PAUSE = 'M7 5h4v14H7zM13 5h4v14h-4z';
const PREV = 'M19 6v12l-8-6zM11 6v12L3 12z', NEXT = 'M5 6v12l8-6zM13 6v12l8-6z';
// Solid glyphs (the pill fills its icons): stop-square = reset, flag = lap.
const RESET = 'M7 7h10v10H7z', LAP = 'M6 4h2v16H6zM8 4h10l-2.5 4L18 12H8z';

let s: Controls | null = null;
let mode: 'timer' | 'stopwatch' | 'media' | 'none' = 'none';
let lastArt = '';
let ticker = 0;

function pickMode(c: Controls): typeof mode {
  if (c.timer.state === 'running' || c.timer.state === 'paused') return 'timer';
  if (c.stopwatch.running) return 'stopwatch';
  if (c.media) return 'media';
  return 'none';
}

function tick() {
  if (!s) return;
  const now = Date.now();
  if (mode === 'timer') {
    const t = s.timer;
    const rem = t.state === 'running' ? t.end_epoch_ms - now : t.state === 'paused' ? t.remaining_ms : t.total_ms;
    $('p-time').textContent = mmss(rem);
    ($('p-ring-prog') as unknown as SVGCircleElement).style.strokeDashoffset = String(62.83 * (1 - Math.max(0, rem / t.total_ms)));
  } else if (mode === 'stopwatch') {
    const w = s.stopwatch;
    $('p-time').textContent = swFmt(w.banked_ms + (w.running && w.start_epoch_ms ? now - w.start_epoch_ms : 0));
  }
}

function render(c: Controls) {
  s = c;
  mode = pickMode(c);
  pill.dataset.mode = mode;
  pill.classList.toggle('playing', !!c.media?.playing);
  if (mode === 'media' && c.media) {
    $('p-title').textContent = c.media.title || 'Now playing';
    $('p-sub').textContent = c.media.artist || c.media.app.replace(/\.exe$/i, '');
    $('p-b1-ico').setAttribute('d', PREV);
    $('p-b2-ico').setAttribute('d', c.media.playing ? PAUSE : PLAY);
    $('p-b3-ico').setAttribute('d', NEXT);
    $('p-b3').style.display = '';
    if (c.media.art_id !== lastArt) {
      lastArt = c.media.art_id;
      invoke<string | null>('get_media_art').then((u) => { $('p-art').style.backgroundImage = u ? `url("${u}")` : ''; });
    }
  } else if (mode === 'timer') {
    $('p-title').textContent = 'Timer';
    $('p-sub').textContent = c.timer.state === 'paused' ? 'Paused' : `${Math.round(c.timer.total_ms / 60000)} min`;
    $('p-b1-ico').setAttribute('d', RESET);
    $('p-b2-ico').setAttribute('d', c.timer.state === 'running' ? PAUSE : PLAY);
    $('p-b3-ico').setAttribute('d', '');
    $('p-b3').style.display = 'none';
  } else if (mode === 'stopwatch') {
    $('p-title').textContent = 'Stopwatch';
    $('p-sub').textContent = c.stopwatch.laps.length ? `${c.stopwatch.laps.length} laps` : 'Running';
    $('p-b1-ico').setAttribute('d', RESET);
    $('p-b2-ico').setAttribute('d', PAUSE);
    $('p-b3-ico').setAttribute('d', LAP);
    $('p-b3').style.display = '';
  }
  const live = (mode === 'timer' || mode === 'stopwatch') && shown;
  if (live && !ticker) ticker = window.setInterval(tick, 100);
  if (!live && ticker) { clearInterval(ticker); ticker = 0; }
  tick();
  updateMeter();
}

// b1 = prev / reset, b2 = play-pause / pause-resume / stop, b3 = next / lap
$('p-b1').addEventListener('click', () => {
  if (mode === 'media') ctl('media', { action: 'prev' });
  else if (mode === 'timer') ctl('timer_reset');
  else if (mode === 'stopwatch') ctl('sw_reset');
});
$('p-b2').addEventListener('click', () => {
  if (mode === 'media') ctl('media', { action: 'play_pause' });
  else if (mode === 'timer') ctl(s?.timer.state === 'running' ? 'timer_pause' : 'timer_resume');
  else if (mode === 'stopwatch') ctl('sw_stop');
});
$('p-b3').addEventListener('click', () => {
  if (mode === 'media') ctl('media', { action: 'next' });
  else if (mode === 'stopwatch') ctl('sw_lap');
});

// ---- level meter: discrete steps ~5x/s, only while playing and while the pill is on screen ----
let shown = false;
let meter = 0;
const bars = Array.from(document.querySelectorAll<HTMLElement>('.eq i'));
function updateMeter() {
  const run = shown && mode === 'media' && !!s?.media?.playing;
  if (run && !meter) {
    meter = window.setInterval(() => {
      for (const b of bars) b.style.transform = `scaleY(${(0.3 + Math.random() * 0.7).toFixed(2)})`;
    }, 200);
  } else if (!run && meter) {
    clearInterval(meter);
    meter = 0;
    for (const b of bars) b.style.transform = '';
  }
}

async function init() {
  await region(COLLAPSED);
  await listen<boolean>('pill-shown', (e) => { shown = e.payload; updateMeter(); });
  shown = await invoke<boolean>('is_pill_shown');
  await listen<Controls>('controls', (e) => render(e.payload));
  const c = await invoke<Controls | null>('get_controls');
  if (c) render(c);
}
init();
