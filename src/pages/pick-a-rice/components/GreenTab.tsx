import { motion } from 'framer-motion';
import styles from './GreenTab.module.css';
import { MORPH_TRANSITION, POSITIONS, useView } from '../view';

/** Mint tab that protrudes past the card's right edge. Shifts up and
 *  shrinks slightly (90→81 tall) when the card morphs to shrunken. */
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
