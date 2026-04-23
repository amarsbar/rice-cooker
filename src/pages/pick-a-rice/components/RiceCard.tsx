import type { ReactNode } from 'react';
import styles from './RiceCard.module.css';
import cardBg from '@/assets/figma/card-bg.png';

/** Figma node 350:6484 — 500 × 440 black card with a 9px mint stroke and a
 *  full-bleed background image (350:6485) clipped to the rounded rectangle. */
export function RiceCard({ children }: { children: ReactNode }) {
  return (
    <div className={styles.card}>
      <img src={cardBg} alt="" className={styles.bg} />
      {children}
    </div>
  );
}
