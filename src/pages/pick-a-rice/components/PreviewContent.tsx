import { motion } from 'framer-motion';
import styles from './PreviewContent.module.css';
import BackBtnSvg from '@/assets/figma/btn-back.svg?react';
import GithubBtnSvg from '@/assets/figma/btn-github.svg?react';
import { SHRUNKEN_TEXT_VARIANTS, useView } from '../view';

interface PreviewContentProps {
  themeName: string;
  creatorName: string;
}

/** Figma 350:7160 children — preview mode shown before the user commits.
 *  Back and GitHub buttons on the left, big central APPLY pill, theme
 *  name at top, "by creator name" at bottom. Fades in 150ms after the
 *  card morph completes. */
export function PreviewContent({ themeName, creatorName }: PreviewContentProps) {
  const view = useView();
  const active = view === 'preview';
  return (
    <motion.div
      className={styles.wrap}
      initial={false}
      animate={active ? 'visible' : 'hidden'}
      variants={SHRUNKEN_TEXT_VARIANTS}
      style={{ pointerEvents: active ? 'auto' : 'none' }}
    >
      <p className={`${styles.label} ${styles.themeName}`}>{themeName}</p>
      <p className={`${styles.label} ${styles.creatorName}`}>{creatorName}</p>

      <button type="button" className={styles.backBtn} aria-label="Back">
        <BackBtnSvg />
      </button>
      <button type="button" className={styles.githubBtn} aria-label="View on GitHub">
        <GithubBtnSvg />
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
