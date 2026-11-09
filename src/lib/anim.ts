import { invoke } from '@tauri-apps/api/core';
import { SPRINGS, springEasing } from './spring';

export type AnimName = 'appear' | 'disappear' | 'pop' | 'drop';

export function jsLog(msg: string) {
  invoke('js_log', { msg }).catch(() => {});
}

/**
 * Only transform + opacity are animated: both are composited on the GPU, so they never trigger
 * layout or paint. The shadow change on lift is a one-off class toggle, not an animated property.
 */
function keyframes(name: AnimName, fromHidden: boolean): { frames: Keyframe[]; timing: KeyframeAnimationOptions } {
  switch (name) {
    case 'appear': {
      const s = springEasing(SPRINGS.snappy);
      return {
        frames: [
          { opacity: 0, transform: 'translateX(14px) scale(0.92)', offset: 0 },
          { opacity: 1, transform: 'translateX(0) scale(1)', offset: 1 },
        ],
        timing: { duration: s.duration, easing: s.easing },
      };
    }
    case 'disappear':
      return {
        frames: [
          { opacity: 1, transform: 'scale(1)' },
          { opacity: 0, transform: 'scale(0.94)' },
        ],
        timing: { duration: 180, easing: 'cubic-bezier(0.4, 0, 1, 1)' },
      };
    case 'pop': {
      if (fromHidden) {
        const s = springEasing(SPRINGS.snappy);
        return {
          frames: [
            { opacity: 0, transform: 'scale(0.9)' },
            { opacity: 1, transform: 'scale(1)' },
          ],
          timing: { duration: s.duration, easing: s.easing },
        };
      }
      // Already visible on the desktop: a quick "lift" towards the viewer and settle.
      const s = springEasing(SPRINGS.lift);
      return {
        frames: [{ transform: 'scale(0.97)' }, { transform: 'scale(1)' }],
        timing: { duration: s.duration, easing: s.easing },
      };
    }
    case 'drop':
      return {
        frames: [{ transform: 'scale(1)' }, { transform: 'scale(0.98)', offset: 0.4 }, { transform: 'scale(1)' }],
        timing: { duration: 240, easing: 'ease-out' },
      };
  }
}

/** Frame-time recorder: proves (or disproves) smoothness at the display refresh rate. */
function recordFrames(): () => number[] {
  const deltas: number[] = [];
  let last = performance.now();
  let on = true;
  const tick = (now: number) => {
    deltas.push(now - last);
    last = now;
    if (on) requestAnimationFrame(tick);
  };
  requestAnimationFrame((now) => {
    last = now;
    requestAnimationFrame(tick);
  });
  return () => {
    on = false;
    return deltas;
  };
}

let current: Animation | null = null;

export async function playAnim(el: HTMLElement, name: AnimName): Promise<void> {
  const fromHidden = Number(getComputedStyle(el).opacity) < 0.5;
  const { frames, timing } = keyframes(name, fromHidden);
  el.classList.toggle('lifted', name === 'pop');
  if (name === 'drop') el.classList.remove('lifted');

  // Interrupting? Commit the current visual state so the next animation starts from there.
  if (current) {
    current.commitStyles();
    current.cancel();
  }
  const stop = recordFrames();
  const a = el.animate(frames, { ...timing, fill: 'forwards' });
  current = a;
  const budget = Number(timing.duration) + 400;
  jsLog(`anim ${name} start dur=${timing.duration} vis=${document.visibilityState} epoch=${Date.now()}`);
  try {
    // Never wait forever: if the timeline stalls (throttled/hidden webview), finish anyway.
    const timedOut = await Promise.race([
      a.finished.then(() => false),
      new Promise<boolean>((r) => setTimeout(() => r(true), budget)),
    ]);
    if (timedOut) {
      jsLog(`anim ${name} TIMEOUT playState=${a.playState} t=${a.currentTime} vis=${document.visibilityState}`);
      a.finish();
    }
    // Bake the end state into inline style and drop the animation, so finished
    // fill-forwards animations never pile up on the element.
    a.commitStyles();
    a.cancel();
  } catch {
    /* cancelled by a newer animation */
  }
  if (current === a) current = null;
  const deltas = stop();
  invoke('frame_stats', { anim: name, deltas }).catch(() => {});
}
