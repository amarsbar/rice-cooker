import { motion } from 'framer-motion';
import styles from './PreviewContent.module.css';
import { SHRUNKEN_TEXT_VARIANTS, useView } from '../view';

interface PreviewContentProps {
  themeName: string;
  creatorName: string;
}

/** Figma 350:7160 children — preview mode shown before the user commits.
 *  Back and GitHub buttons on the left, big central APPLY pill, theme
 *  name at top, "by creator name" at bottom. Fades in 150ms after the
 *  card morph completes.
 *
 *  The back/github circles pair with two decorative 12 × 12 yellow dots.
 *  Default state (350:7955 / 350:7607) paints bg #fade26 + icon #3F3833
 *  + sparkles #fade26; hover state (350:7613 / 350:7601) flips bg to
 *  #73a94a + icon #E8FF76 + sparkles #73A94A. The dots are children of
 *  the button so `:hover` on the button propagates CSS variables down to
 *  both the icon mask and the sparkles. */
export function PreviewContent({ themeName, creatorName }: PreviewContentProps) {
  const view = useView();
  return (
    <motion.div
      className={styles.wrap}
      initial={false}
      animate={view === 'preview' ? 'visible' : 'hidden'}
      variants={SHRUNKEN_TEXT_VARIANTS}
    >
      <p className={`${styles.label} ${styles.themeName}`}>{themeName}</p>
      <p className={`${styles.label} ${styles.creatorName}`}>{creatorName}</p>

      <button type="button" className={`${styles.iconBtn} ${styles.iconBtnBack}`} aria-label="Back">
        <span className={`${styles.iconMask} ${styles.iconMaskBack}`} />
        <span className={`${styles.sparkle} ${styles.sparkleBackRight}`} />
        <span className={`${styles.sparkle} ${styles.sparkleBackBottom}`} />
      </button>

      <button type="button" className={`${styles.iconBtn} ${styles.iconBtnGithub}`} aria-label="View on GitHub">
        <span className={`${styles.iconMask} ${styles.iconMaskGithub}`} />
        <span className={`${styles.sparkle} ${styles.sparkleGhTop}`} />
        <span className={`${styles.sparkle} ${styles.sparkleGhBottom}`} />
      </button>

      <div className={styles.applyBtn}>
        <div className={styles.applyCluster}>
          {'APPLY'.split('').map((c, i) => (
            <span key={i} className={styles.applyLetter}>
              {c}
            </span>
          ))}
        </div>
      </div>
    </motion.div>
  );
}
