import { motion } from 'framer-motion';
import type { ReactNode } from 'react';
import styles from './RiceCard.module.css';
import cardBg from '@/assets/figma/card-bg.png';
import { MORPH_TRANSITION, POSITIONS, useView } from '../view';

/** Outer card. Size/position morphs between the picking and shrunken states. */
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
          ? 'var(--c-card-bg)'
          : 'var(--c-card-bg-shrunken)',
      }}
    >
      <img src={cardBg} alt="" className={styles.bg} />
      {children}
    </motion.div>
  );
}
