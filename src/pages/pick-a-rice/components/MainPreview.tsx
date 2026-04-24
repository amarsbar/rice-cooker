import styles from './MainPreview.module.css';
import themePreview from '@/assets/figma/theme-preview.png';
import frameCorner from '@/assets/figma/frame-corner.svg';
import { useView } from '../view';

interface MainPreviewProps {
  themeName: string;
  creatorName: string;
}

/** Main dark-grey-framed theme preview. Picking-state only; fades out during
 *  the post-install morph. */
export function MainPreview({ themeName, creatorName }: MainPreviewProps) {
  const view = useView();
  const visible = view === 'picking';
  // preview and post-install hide the main preview entirely.
  return (
    <div
      className={styles.wrap}
      style={{ opacity: visible ? 1 : 0, pointerEvents: visible ? 'auto' : 'none' }}
    >
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
