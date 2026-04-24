import { motion } from 'framer-motion';
import styles from './PostInstallContent.module.css';
import { SHRUNKEN_TEXT_VARIANTS, useView } from '../view';

interface PostInstallContentProps {
  themeName: string;
}

/** Figma 350:7342 children — theme name label, delete button in the
 *  top-left corner, and the big central DONE pill. Fades in 150ms after
 *  the card morph completes. */
export function PostInstallContent({ themeName }: PostInstallContentProps) {
  const view = useView();
  return (
    <motion.div
      className={styles.wrap}
      initial={false}
      animate={view === 'post-install' ? 'visible' : 'hidden'}
      variants={SHRUNKEN_TEXT_VARIANTS}
    >
      <p className={styles.themeName}>{themeName}</p>

      <button type="button" className={styles.deleteBtn} aria-label="Delete rice" />

      <div className={styles.doneBtn}>
        <div className={styles.doneCluster}>
          {'DONE'.split('').map((c, i) => (
            <span key={i} className={styles.doneLetter}>
              {c}
            </span>
          ))}
        </div>
      </div>
    </motion.div>
  );
}
