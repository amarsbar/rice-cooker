import { motion } from 'framer-motion';
import styles from './PostInstallContent.module.css';
import DeleteBtnSvg from '@/assets/icon-buttons/delete.svg?react';
import { SHRUNKEN_TEXT_VARIANTS, useView } from '../view';

interface PostInstallContentProps {
  themeName: string;
  onApply: () => void;
}

/** Figma 350:7342 children — theme name label, delete button in the
 *  top-left corner, and the big central DONE pill. Fades in 150ms after
 *  the card morph completes. */
export function PostInstallContent({ themeName, onApply }: PostInstallContentProps) {
  const view = useView();
  const active = view === 'post-install';
  return (
    <motion.div
      className={styles.wrap}
      initial={false}
      animate={active ? 'visible' : 'hidden'}
      variants={SHRUNKEN_TEXT_VARIANTS}
      style={{ pointerEvents: active ? 'auto' : 'none' }}
    >
      <p className={styles.themeName}>{themeName}</p>

      <button type="button" className={styles.deleteBtn} aria-label="Delete rice">
        <DeleteBtnSvg />
      </button>

      <button
        type="button"
        className={styles.doneBtn}
        onClick={(event) => {
          event.stopPropagation();
          onApply();
        }}
      >
        <div className={styles.doneCluster}>
          {'DONE'.split('').map((c, i) => (
            <span key={i} className={styles.doneLetter}>
              {c}
            </span>
          ))}
        </div>
      </button>
    </motion.div>
  );
}
