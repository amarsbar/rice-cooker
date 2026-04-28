import styles from './ClosingCircles.module.css';
import { getRiceScreenshot } from '../riceScreenshots';

const RING_DURATION_MS = 8000;
const RING_COUNT = 10;
const RINGS = Array.from(
  { length: RING_COUNT },
  (_, i) => -(i / RING_COUNT) * RING_DURATION_MS,
);

export function ClosingCircles({ active, riceName }: { active: boolean; riceName?: string }) {
  if (!active) return null;
  const image = getRiceScreenshot(riceName);

  return (
    <div className={styles.wrap} aria-hidden="true">
      {RINGS.map((delay) => (
        <span key={delay} className={styles.ring} style={{ '--delay': `${delay}ms` } as React.CSSProperties} />
      ))}
      <div className={styles.rice}>
        <img src={image} alt="" />
      </div>
    </div>
  );
}
