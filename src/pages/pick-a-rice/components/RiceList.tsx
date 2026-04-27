import { useEffect, useRef, type UIEvent } from 'react';
import styles from './RiceList.module.css';
import { RICE_ITEM_PITCH } from '../view';
import { RiceItem } from './RiceItem';
import type { RiceListRow } from '@/shared/backend';

export interface RiceNavRequest {
  index: number;
  version: number;
}

interface RiceListProps {
  active: boolean;
  rices: RiceListRow[];
  holdDirection: -1 | 0 | 1;
  navRequest: RiceNavRequest;
  onScrollOffsetChange: (offset: number) => void;
}

const HOLD_SPEED = RICE_ITEM_PITCH / 220;
const SNAP_DELAY_MS = 120;

export function RiceList({
  active,
  rices,
  holdDirection,
  navRequest,
  onScrollOffsetChange,
}: RiceListProps) {
  const ref = useRef<HTMLDivElement>(null);
  const snapTimerRef = useRef<ReturnType<typeof window.setTimeout> | null>(null);
  const wasHoldingRef = useRef(false);
  const maxScroll = Math.max(0, (rices.length - 1) * RICE_ITEM_PITCH);

  const clearSnap = () => {
    if (snapTimerRef.current === null) return;
    window.clearTimeout(snapTimerRef.current);
    snapTimerRef.current = null;
  };

  const snap = (el: HTMLDivElement) => {
    el.scrollTo({
      top: Math.max(
        0,
        Math.min(maxScroll, Math.round(el.scrollTop / RICE_ITEM_PITCH) * RICE_ITEM_PITCH),
      ),
      behavior: 'smooth',
    });
  };

  useEffect(() => {
    if (!ref.current || !active || navRequest.version === 0) return;
    ref.current.scrollTo({
      top: navRequest.index * RICE_ITEM_PITCH,
      behavior: 'smooth',
    });
  }, [active, navRequest]);

  useEffect(() => {
    if (!ref.current || !active || holdDirection === 0) return;
    clearSnap();
    let frame = 0;
    let last = performance.now();
    const tick = (now: number) => {
      const el = ref.current;
      if (!el) return;
      el.scrollTop = Math.max(
        0,
        Math.min(maxScroll, el.scrollTop + holdDirection * HOLD_SPEED * (now - last)),
      );
      last = now;
      frame = requestAnimationFrame(tick);
    };
    frame = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(frame);
  }, [active, holdDirection, maxScroll]);

  useEffect(() => {
    if (ref.current && active && holdDirection === 0 && wasHoldingRef.current) snap(ref.current);
    wasHoldingRef.current = holdDirection !== 0;
  }, [active, holdDirection]);

  useEffect(() => {
    if (!ref.current || !active) return;
    onScrollOffsetChange(ref.current.scrollTop);
  }, [active, onScrollOffsetChange]);

  useEffect(() => () => clearSnap(), []);

  const reportScrollOffset = (event: UIEvent<HTMLDivElement>) => {
    if (!active) return;
    const el = event.currentTarget;
    onScrollOffsetChange(el.scrollTop);
    if (holdDirection !== 0) return;
    clearSnap();
    snapTimerRef.current = window.setTimeout(() => snap(el), SNAP_DELAY_MS);
  };

  return (
    <div
      ref={ref}
      className={styles.list}
      onScroll={reportScrollOffset}
    >
      <div className={styles.track}>
        {rices.map((rice) => (
          <RiceItem key={rice.name} rice={rice} />
        ))}
      </div>
    </div>
  );
}
