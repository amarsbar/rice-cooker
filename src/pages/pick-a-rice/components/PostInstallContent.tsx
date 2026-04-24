import styles from './PostInstallContent.module.css';
import deleteIcon from '@/assets/figma/delete-icon.svg';
import { useView } from '../view';

interface PostInstallContentProps {
  themeName: string;
}

/** Figma 350:7342 children — theme name label, delete button + sparkle
 *  dots in the top-left corner, and the big central DONE pill. Shown
 *  in post-install state only. */
export function PostInstallContent({ themeName }: PostInstallContentProps) {
  const view = useView();
  /** Only rendered visible when the rice has finished installing. */
  const visible = view === 'post-install';
  return (
    <div
      className={styles.wrap}
      style={{ opacity: visible ? 1 : 0, pointerEvents: visible ? 'auto' : 'none' }}
    >
      <p className={styles.themeName}>{themeName}</p>

      <button type="button" className={styles.deleteBtn} aria-label="Delete rice">
        <img src={deleteIcon} alt="" className={styles.deleteIcon} />
      </button>
      <span className={`${styles.sparkle} ${styles.sparkleRight}`} />
      <span className={`${styles.sparkle} ${styles.sparkleBottom}`} />

      <div className={styles.doneBtn}>
        <div className={styles.doneCluster}>
          {'DONE'.split('').map((c, i) => (
            <span key={i} className={styles.doneLetter}>
              {c}
            </span>
          ))}
        </div>
      </div>
    </div>
  );
}
