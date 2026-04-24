import styles from './RiceItem.module.css';
import themePreview from '@/assets/figma/theme-preview.png';
import frameCorner from '@/assets/figma/frame-corner.svg';

interface RiceItemProps {
  themeName: string;
  creatorName: string;
}

/** A single rice preview — dark-grey frame with the theme screenshot,
 *  theme/creator pills, corner flourishes, pagination dots, and carousel
 *  arrows. Flow-positioned (has intrinsic height), designed to stack
 *  inside a scrollable <RiceList>. */
export function RiceItem({ themeName, creatorName }: RiceItemProps) {
  return (
    <div className={styles.item}>
      <div className={styles.outer} />
      <div className={styles.inner}>
        <img src={themePreview} alt="" className={styles.image} />
      </div>

      <img src={frameCorner} alt="" className={`${styles.corner} ${styles.cornerTr}`} />
      <img src={frameCorner} alt="" className={`${styles.corner} ${styles.cornerTl}`} />
      <img src={frameCorner} alt="" className={`${styles.corner} ${styles.cornerBr}`} />
      <img src={frameCorner} alt="" className={`${styles.corner} ${styles.cornerBl}`} />

      <div className={`${styles.pill} ${styles.pillTop}`}>
        <p>{themeName}</p>
      </div>
      <div className={`${styles.pill} ${styles.pillBottom}`}>
        <p>{creatorName}</p>
      </div>

      <ArrowButton direction="left" />
      <ArrowButton direction="right" />

      <span className={`${styles.dot} ${styles.dotTl}`} />
      <span className={`${styles.dot} ${styles.dotTr}`} />
      <span className={`${styles.dot} ${styles.dotBl}`} />
      <span className={`${styles.dot} ${styles.dotBr}`} />
    </div>
  );
}

function ArrowButton({ direction }: { direction: 'left' | 'right' }) {
  return (
    <button
      type="button"
      className={`${styles.arrowButton} ${direction === 'left' ? styles.arrowButtonLeft : styles.arrowButtonRight}`}
      aria-label={direction === 'left' ? 'Previous rice' : 'Next rice'}
    >
      <span className={`${styles.arrow} ${direction === 'left' ? styles.arrowLeft : styles.arrowRight}`} />
    </button>
  );
}
