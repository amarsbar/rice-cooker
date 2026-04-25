import styles from './RiceList.module.css';
import { RICE_ITEM_COUNT, RICE_ITEM_PITCH } from '../view';
import { RiceItem } from './RiceItem';

const ITEMS = Array.from({ length: RICE_ITEM_COUNT }, (_, i) => i);

export function RiceList({ focusedIndex }: { focusedIndex: number }) {
  return (
    <div className={styles.list}>
      <div
        className={styles.track}
        style={{ transform: `translateY(${-focusedIndex * RICE_ITEM_PITCH}px)` }}
      >
        {ITEMS.map((i) => (
          <RiceItem key={i} variant={i === focusedIndex ? 'primary' : 'trailing'} />
        ))}
      </div>
    </div>
  );
}
