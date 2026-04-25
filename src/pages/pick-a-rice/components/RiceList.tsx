import { useEffect, useRef } from 'react';
import createScrollSnap from 'scroll-snap';
import styles from './RiceList.module.css';
import { RiceItem } from './RiceItem';
import type { ScrollState } from '../view';

const PLACEHOLDERS = Array.from({ length: 10 }, (_, i) => ({
  themeName: `Theme ${i + 1}`,
  creatorName: 'by creatorname',
}));

/** Pitch = item height (311) + gap (23). activeIndex is whichever item's
 *  top edge the scroll is closest to. */
const PITCH = 334;

export function RiceList({ onScroll }: { onScroll: (s: ScrollState) => void }) {
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!ref.current) return;
    const { unbind } = createScrollSnap(ref.current, {
      snapDestinationY: `${PITCH}px`,
      threshold: 0.1,
      duration: 250,
      timeout: 100,
      enableKeyboard: false,
    });
    return unbind;
  }, []);

  const handleScroll = (e: React.UIEvent<HTMLDivElement>) => {
    const offset = e.currentTarget.scrollTop;
    onScroll({
      offset,
      index: Math.max(0, Math.min(PLACEHOLDERS.length - 1, Math.round(offset / PITCH))),
      total: PLACEHOLDERS.length,
    });
  };

  return (
    <div ref={ref} className={styles.list} onScroll={handleScroll}>
      {PLACEHOLDERS.map((r, i) => (
        <RiceItem key={i} themeName={r.themeName} creatorName={r.creatorName} />
      ))}
    </div>
  );
}
