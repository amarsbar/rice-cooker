import { motion } from 'framer-motion';
import styles from './GreenTab.module.css';
import { MORPH_TRANSITION, POSITIONS, useView } from '../view';

/** Figma node 168:6738 / 168:6840 — mint rectangle that protrudes from the
 *  card's right edge, backing the close pin and sound button. */
export function GreenTab() {
  const view = useView();
  return (
    <motion.div
      className={styles.tab}
      initial={false}
      animate={POSITIONS[view].greenTab}
      transition={MORPH_TRANSITION}
    />
  );
}
