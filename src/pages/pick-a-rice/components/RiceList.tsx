import { useEffect, useRef, type UIEvent } from 'react';
import styles from './RiceList.module.css';
import { RICE_ITEM_COUNT, RICE_ITEM_PITCH } from '../view';
import { RiceItem } from './RiceItem';

export interface RiceNavRequest {
  index: number;
  version: number;
}

interface RiceListProps {
  active: boolean;
  navRequest: RiceNavRequest;
  onScrollOffsetChange: (offset: number) => void;
}

const ITEMS = Array.from({ length: RICE_ITEM_COUNT }, (_, i) => i);

export function RiceList({
  active,
  navRequest,
  onScrollOffsetChange,
}: RiceListProps) {
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!ref.current || !active || navRequest.version === 0) return;
    ref.current.scrollTo({
      top: navRequest.index * RICE_ITEM_PITCH,
      behavior: 'smooth',
    });
  }, [active, navRequest]);

  useEffect(() => {
    if (!ref.current || !active) return;
    onScrollOffsetChange(ref.current.scrollTop);
  }, [active, onScrollOffsetChange]);

  const reportScrollOffset = (event: UIEvent<HTMLDivElement>) => {
    if (!active) return;
    onScrollOffsetChange(event.currentTarget.scrollTop);
  };

  return (
    <div
      ref={ref}
      className={styles.list}
      onScroll={reportScrollOffset}
    >
      <div className={styles.track}>
        {ITEMS.map((i) => (
          <RiceItem key={i} />
        ))}
      </div>
    </div>
  );
}
