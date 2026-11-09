import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

// ---------- types mirrored from src-tauri/src/controls ----------
type TimerInfo =
  | { state: 'idle'; total_ms: number }
  | { state: 'running'; total_ms: number; end_epoch_ms: number }
  | { state: 'paused'; total_ms: number; remaining_ms: number };
type Stopwatch = { running: boolean; start_epoch_ms: number | null; banked_ms: number; laps: number[] };
type Media = {
  title: string; artist: string; app: string; playing: boolean;
  position_ms: number; duration_ms: number; updated_epoch_ms: number; art_id: string;
};
type Controls = {
  volume: number | null; muted: boolean; brightness: number | null;
  wifi: boolean | null; wifi_name: string | null; bluetooth: boolean | null;
  dark: boolean; power: string; media: Media | null;
  timer: TimerInfo; stopwatch: Stopwatch; error: string | null;
};
type Stats = {
  cpu: number; ram_used_gb: number; ram_total_gb: number; gpu: number | null; gpu_temp_c: number | null;
  down_bps: number; up_bps: number; battery: number | null; charging: boolean;
};

const $ = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;
const ctl = (op: string, args: Record<string, unknown> = {}) => invoke('ctl', { cmd: { op, ...args } }).catch(() => {});

let state: Controls | null = null;

// ---------- formatting ----------
const pad = (n: number) => String(n).padStart(2, '0');
function mmss(ms: number) {
  const s = Math.max(0, Math.ceil(ms / 1000));
  const h = Math.floor(s / 3600);
  return h ? `${h}:${pad(Math.floor((s % 3600) / 60))}:${pad(s % 60)}` : `${pad(Math.floor(s / 60))}:${pad(s % 60)}`;
}
function swFmt(ms: number) {
  const t = Math.floor(ms / 100);
  return `${pad(Math.floor(t / 600))}:${pad(Math.floor(t / 10) % 60)}.${t % 10}`;
}
function rate(bps: number) {
  if (bps >= 1e6) return `${(bps / 1e6).toFixed(1)}M`;
  if (bps >= 1e3) return `${Math.round(bps / 1e3)}K`;
  return `${Math.round(bps)}B`;
}
const POWER_LABEL: Record<string, string> = {
  efficiency: 'Best efficiency', balanced: 'Balanced', performance: 'Best performance', custom: 'Custom', unknown: '—',
};
const POWER_NEXT: Record<string, string> = { efficiency: 'balanced', balanced: 'performance', performance: 'efficiency' };

// ---------- sliders (volume / brightness) ----------
const dragging = new Set<string>();
const lastLocal = new Map<string, number>(); // ignore stale echoes right after a drag

function slider(id: string, onValue: (v: number) => void) {
  const el = $(id);
  let lastSent = 0;
  const valueAt = (x: number) => {
    const r = el.getBoundingClientRect();
    return Math.min(1, Math.max(0, (x - r.left) / r.width));
  };
  const apply = (v: number, force: boolean) => {
    el.style.setProperty('--v', String(v));
    const now = performance.now();
    if (force || now - lastSent > 33) { // ~30 Hz to the OS
      lastSent = now;
      onValue(v);
    }
  };
  el.addEventListener('pointerdown', (e) => {
    if ((e.target as HTMLElement).closest('.knob-ico')) return;
    el.setPointerCapture(e.pointerId);
    dragging.add(id);
    el.classList.add('dragging');
    apply(valueAt(e.clientX), true);
  });
  el.addEventListener('pointermove', (e) => {
    if (dragging.has(id)) apply(valueAt(e.clientX), false);
  });
  const end = (e: PointerEvent) => {
    if (!dragging.has(id)) return;
    apply(valueAt(e.clientX), true);
    dragging.delete(id);
    el.classList.remove('dragging');
    lastLocal.set(id, performance.now());
  };
  el.addEventListener('pointerup', end);
  el.addEventListener('pointercancel', end);
  el.addEventListener('wheel', (e) => {
    const cur = Number(getComputedStyle(el).getPropertyValue('--v')) || 0;
    apply(Math.min(1, Math.max(0, cur + (e.deltaY < 0 ? 0.02 : -0.02))), true);
    lastLocal.set(id, performance.now());
  }, { passive: true });
}

function setSlider(id: string, v: number | null) {
  const el = $(id);
  el.dataset.unavailable = String(v === null);
  if (v === null || dragging.has(id) || performance.now() - (lastLocal.get(id) ?? 0) < 400) return;
  el.style.setProperty('--v', String(v));
}

// ---------- timer / stopwatch / media render loops (only while needed) ----------
let tickHandle = 0;
let idleTotal: number | null = null; // minutes picked with the wheel while idle

function renderLive() {
  if (!state) return;
  const now = Date.now();
  // timer
  const t = state.timer;
  const ring = $('timer-ring') as unknown as SVGCircleElement;
  const C = 119.4;
  let remaining = 0;
  if (t.state === 'running') remaining = t.end_epoch_ms - now;
  else if (t.state === 'paused') remaining = t.remaining_ms;
  else remaining = idleTotal ?? t.total_ms;
  $('timer-digits').textContent = mmss(remaining);
  const frac = t.state === 'idle' ? 1 : Math.max(0, remaining / t.total_ms);
  ring.style.strokeDashoffset = String(C * (1 - frac));
  // stopwatch
  const sw = state.stopwatch;
  const el = sw.banked_ms + (sw.running && sw.start_epoch_ms ? now - sw.start_epoch_ms : 0);
  $('sw-digits').textContent = swFmt(el);
  // media progress
  const m = state.media;
  if (m && m.duration_ms > 0) {
    const pos = m.position_ms + (m.playing && m.updated_epoch_ms ? now - m.updated_epoch_ms : 0);
    $('progress').style.width = `${Math.min(100, (100 * pos) / m.duration_ms).toFixed(2)}%`;
  } else {
    $('progress').style.width = '0%';
  }
}

/** Rust says whether anyone can see the panel (hidden / covered panels still look "visible" to the page). */
let panelVisible = true;
export function setPanelVisible(v: boolean) {
  panelVisible = v;
  updateLoop();
}

/** Run the 10 Hz live loop only while something is moving and the panel can actually be seen. */
function updateLoop() {
  const s = state;
  const live = !!s && (s.timer.state === 'running' || s.stopwatch.running || !!s.media?.playing);
  if (live && panelVisible && !document.hidden) {
    if (!tickHandle) tickHandle = window.setInterval(renderLive, 100);
  } else if (tickHandle) {
    clearInterval(tickHandle);
    tickHandle = 0;
  }
  renderLive();
}

// ---------- render a snapshot ----------
let lastArt = '';
let errTimer = 0;

function render(c: Controls) {
  state = c;
  document.documentElement.dataset.theme = c.dark ? 'dark' : 'light';
  const wifi = $('wifi');
  wifi.dataset.on = String(c.wifi === true);
  wifi.dataset.unavailable = String(c.wifi === null);
  $('wifi-sub').textContent = c.wifi === null ? 'Unavailable' : c.wifi ? c.wifi_name ?? 'On' : 'Off';
  const bt = $('bt');
  bt.dataset.on = String(c.bluetooth === true);
  bt.dataset.unavailable = String(c.bluetooth === null);
  $('bt-sub').textContent = c.bluetooth === null ? 'Unavailable' : c.bluetooth ? 'On' : 'Off';
  $('theme-tile').dataset.on = String(c.dark);
  $('theme-sub').textContent = c.dark ? 'On' : 'Off';
  $('power-tile').dataset.on = String(c.power === 'performance');
  $('power-sub').textContent = POWER_LABEL[c.power] ?? c.power;

  setSlider('brightness', c.brightness === null ? null : c.brightness / 100);
  setSlider('volume', c.volume);
  $('volume').dataset.muted = String(c.muted);

  // media
  const m = c.media;
  $('track').textContent = m?.title || 'Not playing';
  $('artist').textContent = m ? m.artist || m.app.replace(/\.exe$/i, '') : '—';
  $('m-play-ico').setAttribute('d', m?.playing ? 'M7 5h4v14H7zM13 5h4v14h-4z' : 'M8 5v14l11-7z');
  if ((m?.art_id ?? '') !== lastArt) {
    lastArt = m?.art_id ?? '';
    invoke<string | null>('get_media_art').then((url) => {
      $('art').style.backgroundImage = url ? `url("${url}")` : '';
    });
  }

  // timer / stopwatch
  $('timer-tile').dataset.state = c.timer.state;
  if (c.timer.state !== 'idle') idleTotal = null;
  $('timer-ico').setAttribute('d', c.timer.state === 'running' ? 'M7 5h4v14H7zM13 5h4v14h-4z' : 'M8 5v14l11-7z');
  const sw = c.stopwatch;
  $('sw-toggle').textContent = sw.running ? 'Stop' : 'Start';
  $('sw-second').textContent = sw.running ? 'Lap' : 'Reset';
  $('sw-laps').textContent = sw.laps.length ? `· ${sw.laps.length} lap${sw.laps.length > 1 ? 's' : ''}` : '';

  if (c.error) {
    const e = $('err');
    e.textContent = c.error;
    e.classList.add('show');
    clearTimeout(errTimer);
    errTimer = window.setTimeout(() => e.classList.remove('show'), 3000);
  }
  updateLoop();
}

function renderStats(s: Stats) {
  $('s-cpu').textContent = `${Math.round(s.cpu)}%`;
  $('s-ram').textContent = `${s.ram_used_gb.toFixed(1)}G`;
  $('s-gpu').textContent = s.gpu === null ? '—' : `${Math.round(s.gpu)}%${s.gpu_temp_c ? ` ${Math.round(s.gpu_temp_c)}°` : ''}`;
  $('s-net').textContent = `↓${rate(s.down_bps)}`;
  $('s-net').title = `down ${rate(s.down_bps)}/s · up ${rate(s.up_bps)}/s`;
  $('s-bat').textContent = s.battery === null ? '—' : `${s.battery}%${s.charging ? '⚡' : ''}`;
}

// ---------- wiring ----------
export async function initControls() {
  $('wifi').addEventListener('click', () => state && ctl('wifi', { on: !state.wifi }));
  $('bt').addEventListener('click', () => state && ctl('bluetooth', { on: !state.bluetooth }));
  $('theme-tile').addEventListener('click', () => state && ctl('dark', { on: !state.dark }));
  $('power-tile').addEventListener('click', () => state && ctl('power', { mode: POWER_NEXT[state.power] ?? 'balanced' }));

  slider('volume', (v) => ctl('volume', { v }));
  slider('brightness', (v) => ctl('brightness', { v: Math.round(v * 100) }));
  document.querySelector('#volume .knob-ico')!.addEventListener('click', () => state && ctl('mute', { on: !state.muted }));

  $('m-play').addEventListener('click', () => ctl('media', { action: 'play_pause' }));
  $('m-next').addEventListener('click', () => ctl('media', { action: 'next' }));
  $('m-prev').addEventListener('click', () => ctl('media', { action: 'prev' }));

  $('timer-toggle').addEventListener('click', () => {
    const t = state?.timer;
    if (!t) return;
    if (t.state === 'idle') ctl('timer_start', { ms: idleTotal ?? t.total_ms });
    else if (t.state === 'running') ctl('timer_pause');
    else ctl('timer_resume');
  });
  $('timer-reset').addEventListener('click', () => ctl('timer_reset'));
  $('timer-chips').addEventListener('click', (e) => {
    const min = Number((e.target as HTMLElement).dataset.min);
    if (min) ctl('timer_start', { ms: min * 60_000 });
  });
  $('timer-digits').addEventListener('wheel', (e) => {
    if (state?.timer.state !== 'idle') return;
    const cur = Math.round((idleTotal ?? state.timer.total_ms) / 60_000);
    idleTotal = Math.min(180, Math.max(1, cur + (e.deltaY < 0 ? 1 : -1))) * 60_000;
    renderLive();
  }, { passive: true });

  $('sw-toggle').addEventListener('click', () => ctl(state?.stopwatch.running ? 'sw_stop' : 'sw_start'));
  $('sw-second').addEventListener('click', () => ctl(state?.stopwatch.running ? 'sw_lap' : 'sw_reset'));
  $('sw-digits').addEventListener('click', () => ctl(state?.stopwatch.running ? 'sw_stop' : 'sw_start'));

  document.addEventListener('visibilitychange', updateLoop);

  await listen<Controls>('controls', (e) => render(e.payload));
  await listen<Stats>('stats', (e) => renderStats(e.payload));
  const c = await invoke<Controls | null>('get_controls');
  if (c) render(c);
  const s = await invoke<Stats | null>('get_stats');
  if (s) renderStats(s);
}
