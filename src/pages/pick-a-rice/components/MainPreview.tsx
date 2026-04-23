import styles from './MainPreview.module.css';
import themePreview from '@/assets/figma/theme-preview.png';
import frameCorner from '@/assets/figma/frame-corner.svg';

interface MainPreviewProps {
  themeName: string;
  creatorName: string;
}

/** Figma group 350:6512 — the large dark-grey-framed theme screenshot that
 *  sits at the vertical center of the card. Shows the current rice, the
 *  theme name pill above, the creator pill below, carousel arrows on each
 *  side, and four corner flourishes plus pagination dots. */
export function MainPreview({ themeName, creatorName }: MainPreviewProps) {
  return (
    <>
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
    </>
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
