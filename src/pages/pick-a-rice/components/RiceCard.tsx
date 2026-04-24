import { motion } from 'framer-motion';
import type { ReactNode } from 'react';
import styles from './RiceCard.module.css';
import cardBg from '@/assets/figma/card-bg.png';
import { MORPH_TRANSITION, POSITIONS, SCREEN_FADE_TRANSITION, useView } from '../view';

/** Outer card. Size/position morph between picking (500 × 440 at origin)
 *  and the shrunken preview/post-install state (405 × 229 centered).
 *  Background swaps between picking's character image and the shrunken
 *  states' faint dot grid. */
export function RiceCard({ children }: { children: ReactNode }) {
  const view = useView();
  const isPicking = view === 'picking';
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
        animate={{ opacity: isPicking ? 1 : 0 }}
        transition={SCREEN_FADE_TRANSITION}
      />
      <motion.div
        className={styles.grid}
        initial={false}
        animate={{ opacity: isPicking ? 0 : 1 }}
        transition={SCREEN_FADE_TRANSITION}
      />
      {children}
    </motion.div>
  );
}
