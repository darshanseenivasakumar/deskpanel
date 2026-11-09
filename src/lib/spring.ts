/**
 * Apple-style springs as CSS `linear()` easings.
 *
 * We simulate a damped spring once (SwiftUI parameters: `response` = period of the undamped
 * oscillation in seconds, `damping` = damping fraction, 1 = critically damped), sample the curve,
 * and hand the browser a piecewise-linear easing. The compositor then runs the animation at the
 * display's refresh rate (144 Hz here) with zero JavaScript per frame.
 */
export type Spring = { response: number; damping: number };

export const SPRINGS = {
  /** Default Apple feel: quick, a hint of overshoot. */
  snappy: { response: 0.42, damping: 0.8 },
  /** Panel lift when popping over apps. */
  lift: { response: 0.35, damping: 0.72 },
  /** No overshoot, for things leaving the screen. */
  smooth: { response: 0.3, damping: 1 },
} satisfies Record<string, Spring>;

export type Easing = { easing: string; duration: number };

const cache = new Map<string, Easing>();

export function springEasing({ response, damping }: Spring): Easing {
  const key = `${response}/${damping}`;
  const hit = cache.get(key);
  if (hit) return hit;

  const omega = (2 * Math.PI) / response; // natural angular frequency
  const k = omega * omega; // stiffness (mass = 1)
  const c = 2 * damping * omega; // damping coefficient
  const dt = 1 / 1000;
  let x = 0;
  let v = 0;
  const samples: number[] = [];
  let t = 0;
  let settledAt = 0;
  // Semi-implicit Euler at 1 kHz; stop once within 0.5% and nearly still for 50 ms.
  // (A 0.1% tolerance added ~300 ms of invisible tail during which input was queued.)
  for (; t < 3; t += dt) {
    const a = -k * (x - 1) - c * v;
    v += a * dt;
    x += v * dt;
    samples.push(x);
    if (Math.abs(x - 1) < 0.005 && Math.abs(v) < 0.05) {
      if (!settledAt) settledAt = t;
      if (t - settledAt > 0.05) break;
    } else {
      settledAt = 0;
    }
  }
  const duration = Math.round(samples.length); // ms (one sample per ms)
  const points = 48;
  const stops: string[] = [];
  for (let i = 0; i <= points; i++) {
    const idx = Math.min(samples.length - 1, Math.round((i / points) * (samples.length - 1)));
    const y = i === points ? 1 : samples[idx];
    stops.push(`${y.toFixed(4)} ${((i / points) * 100).toFixed(2)}%`);
  }
  const out = { easing: `linear(${stops.join(', ')})`, duration };
  cache.set(key, out);
  return out;
}
