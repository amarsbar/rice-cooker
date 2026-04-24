import styles from './PreviewContent.module.css';
import backIcon from '@/assets/figma/back-icon.svg';
import githubIcon from '@/assets/figma/github-icon.svg';
import { useView } from '../view';

interface PreviewContentProps {
  themeName: string;
  creatorName: string;
}

/** Figma 350:7160 children — preview mode shown before the user commits.
 *  Back and GitHub buttons on the left, big central APPLY pill, theme
 *  name at top, "by creator name" at bottom. Shown only in the preview
 *  state. */
export function PreviewContent({ themeName, creatorName }: PreviewContentProps) {
  const view = useView();
  const visible = view === 'preview';
  return (
    <div
      className={styles.wrap}
      style={{ opacity: visible ? 1 : 0, pointerEvents: visible ? 'auto' : 'none' }}
    >
      <p className={`${styles.label} ${styles.themeName}`}>{themeName}</p>
      <p className={`${styles.label} ${styles.creatorName}`}>{creatorName}</p>

      <button type="button" className={styles.iconBtn} data-pos="back" aria-label="Back">
        <img src={backIcon} alt="" className={styles.iconImg} />
      </button>
      <span className={`${styles.sparkle} ${styles.sparkleBackRight}`} />
      <span className={`${styles.sparkle} ${styles.sparkleBackBottom}`} />

      <button type="button" className={styles.iconBtn} data-pos="github" aria-label="View on GitHub">
        <img src={githubIcon} alt="" className={styles.iconImg} />
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
    </div>
  );
}
