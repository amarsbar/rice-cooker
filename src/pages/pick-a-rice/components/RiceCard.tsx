import { motion } from 'framer-motion';
import type { ReactNode } from 'react';
import styles from './RiceCard.module.css';
import cardBg from '@/assets/figma/card-bg.png';
import { MORPH_TRANSITION, POSITIONS, useView } from '../view';

/** Outer card. Size/position morph between picking (500 × 440 at origin)
 *  and the shrunken preview/post-install state (405 × 229 centered). In
 *  the shrunken states the card bg drops to 80% opacity so the dot grid
 *  behind the character asset shows through. */
export function RiceCard({ children }: { children: ReactNode }) {
  const view = useView();
  const isPicking = view === 'picking';
  return (
    <motion.div
      className={styles.card}
      initial={false}
      animate={POSITIONS[view].card}
      transition={MORPH_TRANSITION}
      style={{
        background: isPicking
          ? 'var(--color-card-bg)'
          : 'var(--color-card-bg-shrunken)',
      }}
    >
      <img src={cardBg} alt="" className={styles.bg} />
      {children}
    </motion.div>
  );
}
