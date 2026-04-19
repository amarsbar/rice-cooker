import { motion } from 'framer-motion';
import styles from './PreviewContent.module.css';
import { PREVIEW_TEXT_VARIANTS, useView } from '../view';

/** Preview-state screen content — the "Exit preview" button (168:7329) and
 *  the dimmed "Preview state" label (168:7330). Fades in 150ms after the
 *  card morph completes. */
export function PreviewContent() {
  const view = useView();
  return (
    <motion.div
      className={styles.preview}
      initial={false}
      animate={view === 'preview' ? 'visible' : 'hidden'}
      variants={PREVIEW_TEXT_VARIANTS}
    >
      <div className={styles.exitButton}>
        <p>Exit preview</p>
      </div>
      <p className={styles.previewLabel}>Preview state</p>
    </motion.div>
  );
}
