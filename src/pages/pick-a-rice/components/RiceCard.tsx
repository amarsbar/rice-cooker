import { motion } from 'framer-motion';
import type { ReactNode } from 'react';
import styles from './RiceCard.module.css';
import cardBg from '@/assets/figma/card-bg.png';
import { MORPH_TRANSITION, POSITIONS, useView } from '../view';

/** Outer card. Size/position morph between picking (500 × 440 at origin)
 *  and the shrunken preview/post-install state (405 × 229 centered). One
 *  shared background image across all three views. */
export function RiceCard({ children }: { children: ReactNode }) {
  const view = useView();
  return (
    <motion.div
      className={styles.card}
      initial={false}
      animate={POSITIONS[view].card}
      transition={MORPH_TRANSITION}
    >
      <img src={cardBg} alt="" className={styles.bg} />
      {children}
    </motion.div>
  );
}
