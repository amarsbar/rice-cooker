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
 *  card morph completes. Buttons reuse the shared icon-button hover
 *  tokens from tokens.css. */
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
        <span className={`${styles.icon} ${styles.iconBack}`} />
      </button>
      <span className={`${styles.sparkle} ${styles.sparkleBackRight}`} />
      <span className={`${styles.sparkle} ${styles.sparkleBackBottom}`} />

      <button
        type="button"
        className={`${styles.iconBtn} ${styles.iconBtnGithub}`}
        aria-label="View on GitHub"
      >
        <span className={`${styles.icon} ${styles.iconGithub}`} />
      </button>
      <span className={`${styles.sparkle} ${styles.sparkleGhTop}`} />
      <span className={`${styles.sparkle} ${styles.sparkleGhBottom}`} />

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
