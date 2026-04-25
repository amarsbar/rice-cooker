import { PILL_COUNT, PILL_SIZE, BITMAP_WIDTH } from './contract';

export function getChopSpeed(durationMs: number): number {
  return 1400 / Math.max(50, durationMs);
}

export function getChopVelocityPxPerSec(durationMs: number): number {
  return 20 * getChopSpeed(durationMs);
}

export function getChopSpinRate(durationMs: number): number {
  return 0.5 * getChopSpeed(durationMs);
}

export function getStripCountForSpeed(durationMs: number): number {
  const raw = 3 + Math.sqrt(Math.max(1, durationMs)) / 5;
  return Math.round(Math.max(5, Math.min(36, raw)));
}

export interface Strip {
  index: number;
  xStart: number;
  width: number;
  pillIndex: number;
  chopTimeMs: number;
}

export function buildStripSchedule(durationMs: number): Strip[] {
  const stripCount = getStripCountForSpeed(durationMs);
  const sweepMs = durationMs;
  const baseWidth = BITMAP_WIDTH / stripCount;

  const strips: Strip[] = [];
  let xCursor = 0;
  for (let i = 0; i < stripCount; i++) {
    const w = i === stripCount - 1 ? BITMAP_WIDTH - xCursor : baseWidth;
    const center = xCursor + w / 2;
    const pillIndex = Math.min(PILL_COUNT - 1, Math.max(0, Math.floor(center / PILL_SIZE)));
    const chopOrder = stripCount - 1 - i;
    const chopTimeMs = stripCount === 1 ? 0 : (chopOrder / (stripCount - 1)) * sweepMs;
    strips.push({ index: i, xStart: xCursor, width: w, pillIndex, chopTimeMs });
    xCursor += w;
  }
  return strips;
}

export function sampleChopDirection(rand: () => number): { dx: number; dy: number } {
  const angle = rand() * Math.PI * 2;
  const lean = -0.22;
  const dx = Math.cos(angle) + lean;
  const dy = Math.sin(angle);
  const len = Math.hypot(dx, dy) || 1;
  return { dx: dx / len, dy: dy / len };
}

export function mulberry32(seed: number) {
  let a = (seed | 0) || 1;
  return () => {
    a |= 0;
    a = (a + 0x6d2b79f5) | 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}
