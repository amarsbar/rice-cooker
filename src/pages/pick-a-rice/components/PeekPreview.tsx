import styles from './PeekPreview.module.css';
import themePreview from '@/assets/figma/theme-preview.png';
import peekCurveTr from '@/assets/figma/peek-curve-tr.svg';
import peekCurveTl from '@/assets/figma/peek-curve-tl.svg';
import peekCurveBr from '@/assets/figma/peek-curve-br.svg';
import peekCurveBl from '@/assets/figma/peek-curve-bl.svg';

interface PeekPreviewProps {
  themeName: string;
  creatorName: string;
}

/** Figma frame 350:6486 — the dimmed peek of the next rice below the main
 *  preview. Rendered at 30% opacity and clipped by the card so only the
 *  top of it shows through. Mirror of the main preview with lighter grey
 *  frame elements and curved flourishes instead of corner sprites. */
export function PeekPreview({ themeName, creatorName }: PeekPreviewProps) {
  return (
    <div className={styles.peek}>
      <div className={styles.imageWrap}>
        <img src={themePreview} alt="" className={styles.image} />
      </div>

      <img src={peekCurveTr} alt="" className={`${styles.curve} ${styles.curveTr}`} />
      <img src={peekCurveTl} alt="" className={`${styles.curve} ${styles.curveTl}`} />
      <img src={peekCurveBr} alt="" className={`${styles.curve} ${styles.curveBr}`} />
      <img src={peekCurveBl} alt="" className={`${styles.curve} ${styles.curveBl}`} />

      <div className={`${styles.pill} ${styles.pillTop}`}>
        <p>{themeName}</p>
      </div>
      <div className={`${styles.pill} ${styles.pillBottom}`}>
        <p>{creatorName}</p>
      </div>

      <ArrowButton direction="left" />
      <ArrowButton direction="right" />
    </div>
  );
}

function ArrowButton({ direction }: { direction: 'left' | 'right' }) {
  return (
    <div
      className={`${styles.arrowButton} ${direction === 'left' ? styles.arrowButtonLeft : styles.arrowButtonRight}`}
      aria-hidden="true"
    >
      <span className={`${styles.arrow} ${direction === 'left' ? styles.arrowLeft : styles.arrowRight}`} />
    </div>
  );
}
