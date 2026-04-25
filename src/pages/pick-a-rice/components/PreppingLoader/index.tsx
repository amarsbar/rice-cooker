import { useEffect, useMemo, useRef, useState } from 'react';
import * as planck from 'planck';
import {
  BITMAP_WIDTH,
  BITMAP_HEIGHT,
  SLICE_RADIUS,
  STAGE_W,
  STAGE_H,
  BITMAP_LEFT,
  BITMAP_TOP,
} from './contract';
import { PreppingPills } from './PreppingPills';
import { getPreppingSvgUrl } from './usePreppingImage';
import {
  buildStripSchedule,
  getChopVelocityPxPerSec,
  getChopSpinRate,
  sampleChopDirection,
  mulberry32,
  type Strip,
} from './chopSchedule';

export interface PreppingLoaderProps {
  playing: boolean;
  durationMs: number;
  onComplete?: () => void;
  seed?: number;
}

/** Figma 367:11763 — the PREPPING loader that plays inside the preview card's
 *  content area (387×211, 9px border stripped from the card). Planck.js-driven
 *  chop with collisions. Fixed to Figma coords; parent should supply a 387×211
 *  container. */
export function PreppingLoader({
  playing,
  durationMs,
  onComplete,
  seed = 1,
}: PreppingLoaderProps) {
  const centerX = STAGE_W / 2;
  const centerY = STAGE_H / 2;

  const durationMsRef = useRef(durationMs);
  durationMsRef.current = durationMs;
  const onCompleteRef = useRef(onComplete);
  onCompleteRef.current = onComplete;

  const schedule = useMemo(() => buildStripSchedule(durationMs), [durationMs]);
  const bitmapUrl = getPreppingSvgUrl();

  const [intactWidth, setIntactWidth] = useState<number>(BITMAP_WIDTH);
  const [pieces, setPieces] = useState<Array<{ strip: Strip; body: planck.Body }>>([]);
  const [, setTick] = useState(0);
  const [runKey, setRunKey] = useState(0);

  const worldRef = useRef<planck.World | null>(null);
  const rafRef = useRef<number | null>(null);
  const completedRef = useRef(false);

  useEffect(() => {
    if (!playing) {
      setPieces([]);
      setIntactWidth(BITMAP_WIDTH);
      completedRef.current = false;
      return;
    }

    const world = new planck.World(planck.Vec2(0, 0));
    worldRef.current = world;
    completedRef.current = false;
    setPieces([]);
    setIntactWidth(BITMAP_WIDTH);
    setRunKey((k) => k + 1);

    // `cancelled` closes the loop + pending timers even if a dispatched-but-
    // -not-yet-executed RAF or setTimeout fires after cleanup. Without this,
    // the browser can run one orphan frame per cleanup, which leaks bodies
    // and visible state when the loader unmounts mid-animation under
    // AnimatePresence.
    let cancelled = false;
    const timers: number[] = [];
    let lastPhysicsMs = performance.now();

    const rand = mulberry32(seed ^ Math.floor(durationMsRef.current));
    const lastChopMs = schedule.reduce((m, s) => Math.max(m, s.chopTimeMs), 0);
    const finalStripIndex = schedule.reduce(
      (finalIndex, strip, index) =>
        strip.chopTimeMs > schedule[finalIndex]!.chopTimeMs ? index : finalIndex,
      0,
    );

    // Degenerate schedule guard. Impossible under getStripCountForSpeed's
    // minimum=5 clamp, but keeps the contract honest.
    if (schedule.length === 0 || lastChopMs === 0) {
      console.warn(
        '[PreppingLoader] degenerate schedule (len=%d, lastChopMs=%d); firing onComplete immediately',
        schedule.length,
        lastChopMs,
      );
      completedRef.current = true;
      const tc = window.setTimeout(() => {
        if (!cancelled) onCompleteRef.current?.();
      }, 100);
      timers.push(tc);
      return () => {
        cancelled = true;
        timers.forEach((t) => window.clearTimeout(t));
        worldRef.current = null;
      };
    }

    schedule.forEach((strip, scheduleIndex) => {
      const t = window.setTimeout(() => {
        if (cancelled || worldRef.current !== world) return;

        setIntactWidth((w) => Math.min(w, strip.xStart));

        const stripPxX = BITMAP_LEFT + strip.xStart + strip.width / 2;
        const stripPxY = BITMAP_TOP + BITMAP_HEIGHT / 2;
        const worldX = stripPxX - centerX;
        const worldY = stripPxY - centerY;

        const body = world.createBody({
          type: 'dynamic',
          position: planck.Vec2(worldX, worldY),
          angle: 0,
          linearDamping: 1.5,
          angularDamping: 2.0,
          allowSleep: true,
        });
        body.createFixture({
          shape: new planck.Box(strip.width / 2, BITMAP_HEIGHT / 2),
          density: 1,
          friction: 0.3,
          restitution: 0.2,
        });

        const dir = sampleChopDirection(rand);
        const vel = getChopVelocityPxPerSec(durationMsRef.current);
        body.setLinearVelocity(planck.Vec2(dir.dx * vel, dir.dy * vel));
        const spin = getChopSpinRate(durationMsRef.current);
        body.setAngularVelocity((rand() - 0.5) * 2 * spin);

        // Functional updater — avoids the ref/state desync the old
        // `piecesRef.current = [...]; setPieces(piecesRef.current)` pattern
        // produced when multiple strip timers fired in the same frame.
        setPieces((prev) => [...prev, { strip, body }]);

        if (scheduleIndex === finalStripIndex && !completedRef.current) {
          completedRef.current = true;
          // Push the trailing handle into the same timers array so an unmount
          // during AnimatePresence exit cancels it cleanly.
          const finalT = window.setTimeout(() => {
            if (cancelled || worldRef.current !== world) return;
            onCompleteRef.current?.();
          }, 100);
          timers.push(finalT);
        }
      }, strip.chopTimeMs);
      timers.push(t);
    });

    const loop = (now: number) => {
      // Belt and braces: cancelled OR world swapped → bail.
      if (cancelled || worldRef.current !== world) return;
      // Step by real frame dt (clamped) so throttled tabs don't run the sim
      // at half speed. Previously the loop called world.step(1/60, ...)
      // regardless of wall-clock elapsed time.
      const dt = Math.min(1 / 30, Math.max(1 / 240, (now - lastPhysicsMs) / 1000));
      lastPhysicsMs = now;
      try {
        world.step(dt, 6, 2);
      } catch (err) {
        console.warn('[PreppingLoader] physics step failed', err);
        completedRef.current = true;
        onCompleteRef.current?.();
        return;
      }
      setTick((t) => (t + 1) & 0xffff);
      rafRef.current = requestAnimationFrame(loop);
    };
    rafRef.current = requestAnimationFrame(loop);

    return () => {
      cancelled = true;
      timers.forEach((t) => window.clearTimeout(t));
      if (rafRef.current != null) cancelAnimationFrame(rafRef.current);
      rafRef.current = null;
      // Explicitly destroy bodies so Planck releases its internal pools; GC
      // alone would eventually free them but this is deterministic.
      for (let b = world.getBodyList(); b; ) {
        const next = b.getNext();
        world.destroyBody(b);
        b = next;
      }
      worldRef.current = null;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [playing, seed]);

  if (!playing) {
    return (
      <div style={{ position: 'relative', width: STAGE_W, height: STAGE_H }}>
        <div style={{ position: 'absolute', left: BITMAP_LEFT, top: BITMAP_TOP }}>
          <PreppingPills />
        </div>
      </div>
    );
  }

  return (
    <div
      style={{
        position: 'relative',
        width: STAGE_W,
        height: STAGE_H,
        overflow: 'visible',
      }}
    >
      {/* Intact region uses the same SVG bitmap as the chopped pieces so the
          pixels on either side of a chop boundary come from an identical
          renderer. Previously this drew <PreppingPills /> (React DOM text)
          and each piece drew from the SVG, producing a visible font/AA pop
          at the cut instant. */}
      <div
        style={{
          position: 'absolute',
          left: BITMAP_LEFT,
          top: BITMAP_TOP,
          width: intactWidth,
          height: BITMAP_HEIGHT,
          backgroundImage: `url("${bitmapUrl}")`,
          backgroundRepeat: 'no-repeat',
          backgroundSize: `${BITMAP_WIDTH}px ${BITMAP_HEIGHT}px`,
          backgroundPosition: '0px 0px',
        }}
      />

      {pieces.map((piece) => {
        const pos = piece.body.getPosition();
        const angle = piece.body.getAngle();
        if (!Number.isFinite(pos.x) || !Number.isFinite(pos.y) || !Number.isFinite(angle)) {
          return null;
        }
        const screenX = pos.x + centerX;
        const screenY = pos.y + centerY;
        return (
          <div
            key={`${runKey}-${piece.strip.index}`}
            style={{
              position: 'absolute',
              left: 0,
              top: 0,
              width: piece.strip.width,
              height: BITMAP_HEIGHT,
              borderRadius: SLICE_RADIUS,
              overflow: 'hidden',
              backgroundImage: `url("${bitmapUrl}")`,
              backgroundRepeat: 'no-repeat',
              backgroundSize: `${BITMAP_WIDTH}px ${BITMAP_HEIGHT}px`,
              backgroundPosition: `-${piece.strip.xStart}px 0px`,
              transform: `translate(${screenX - piece.strip.width / 2}px, ${screenY - BITMAP_HEIGHT / 2}px) rotate(${angle}rad)`,
              transformOrigin: `${piece.strip.width / 2}px ${BITMAP_HEIGHT / 2}px`,
              willChange: 'transform',
            }}
          />
        );
      })}
    </div>
  );
}
