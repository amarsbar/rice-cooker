import placeholderRice from '@/assets/rices/placeholder-rice.webp';
import styles from './ClosingCircles.module.css';

const RING_DURATION_MS = 8000;
const RING_COUNT = 10;
const RINGS = Array.from(
  { length: RING_COUNT },
  (_, i) => -(i / RING_COUNT) * RING_DURATION_MS,
);

export function ClosingCircles({ active }: { active: boolean }) {
  if (!active) return null;

  return (
    <div className={styles.wrap} aria-hidden="true">
      {RINGS.map((delay) => (
        <span key={delay} className={styles.ring} style={{ '--delay': `${delay}ms` } as React.CSSProperties} />
      ))}
      <div className={styles.rice}>
        <img src={placeholderRice} alt="" />
      </div>
    </div>
  );
}
