import { motion } from 'framer-motion';
import type { ReactNode } from 'react';
import styles from './RiceCard.module.css';
import cardBg from '@/assets/screen/card-bg.png';
import { MORPH_TRANSITION, POSITIONS, useView } from '../view';

/** Outer card. Size/position morphs between the picking and shrunken states. */
export function RiceCard({ children, menuOpen = false }: { children: ReactNode; menuOpen?: boolean }) {
  const view = useView();
  const isPicking = view === 'picking';
  const backgroundColor = menuOpen
    ? 'var(--c-menu-card-bg)'
    : isPicking
      ? 'var(--c-card-bg)'
      : 'var(--c-card-bg-shrunken)';

  return (
    <motion.div
      className={styles.card}
      initial={false}
      animate={POSITIONS[view].card}
      transition={MORPH_TRANSITION}
      style={{ backgroundColor }}
    >
      <img src={cardBg} alt="" className={`${styles.bg} ${!isPicking ? styles.bgShrunken : ''}`} />
      {children}
    </motion.div>
  );
}
