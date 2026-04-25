import { useEffect, useRef } from 'react';
import createScrollSnap from 'scroll-snap';
import styles from './RiceList.module.css';
import { RiceItem } from './RiceItem';
import type { ScrollState } from '../view';

const ITEM_COUNT = 10;
const ITEMS = Array.from({ length: ITEM_COUNT }, (_, i) => i);

const PITCH = 292;

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
      index: Math.max(0, Math.min(ITEM_COUNT - 1, Math.round(offset / PITCH))),
      total: ITEM_COUNT,
    });
  };

  return (
    <div ref={ref} className={styles.list} onScroll={handleScroll}>
      {ITEMS.map((i) => (
        <RiceItem key={i} variant={i === 0 ? 'primary' : 'trailing'} />
      ))}
    </div>
  );
}
