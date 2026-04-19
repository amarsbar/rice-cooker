import { motion } from 'framer-motion';
import type { ReactNode } from 'react';
import styles from './RiceCard.module.css';
import { MORPH_TRANSITION, POSITIONS, useView } from '../view';

export function RiceCard({ children }: { children: ReactNode }) {
  const view = useView();
  return (
    <motion.div
      className={styles.card}
      initial={false}
      animate={POSITIONS[view].card}
      transition={MORPH_TRANSITION}
    >
      {children}
    </motion.div>
  );
}
