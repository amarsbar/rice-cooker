import { motion } from 'framer-motion';
import type { ReactNode } from 'react';
import styles from './RiceCard.module.css';
import cardBg from '@/assets/figma/card-bg.png';
import { MORPH_TRANSITION, POSITIONS, SCREEN_FADE_TRANSITION, useView } from '../view';

/** Outer card. Size/position morph between picking (500 × 440 at origin)
 *  and the shrunken preview/post-install state (405 × 229 centered). The
 *  character bg shows only in picking — per design the shrunken states
 *  render as a solid dark field (no dot grid, no character). */
export function RiceCard({ children }: { children: ReactNode }) {
  const view = useView();
  return (
    <motion.div
      className={styles.card}
      initial={false}
      animate={POSITIONS[view].card}
      transition={MORPH_TRANSITION}
    >
      <motion.img
        src={cardBg}
        alt=""
        className={styles.bg}
        initial={false}
        animate={{ opacity: view === 'picking' ? 1 : 0 }}
        transition={SCREEN_FADE_TRANSITION}
      />
      {children}
    </motion.div>
  );
}
