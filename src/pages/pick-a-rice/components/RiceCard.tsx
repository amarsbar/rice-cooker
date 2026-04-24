import type { ReactNode } from 'react';
import styles from './RiceCard.module.css';
import cardBg from '@/assets/figma/card-bg.png';
import { POSITIONS, useView } from '../view';

/** Outer card. Size/position morph between picking (500 × 440 at origin)
 *  and post-install (405 × 229 centered). The full-bleed character bg is
 *  rendered only in the picking state — per design the post-install card
 *  is a plain dark field, no dot-grid or character behind the DONE pill. */
export function RiceCard({ children }: { children: ReactNode }) {
  const view = useView();
  const pos = POSITIONS[view].card;
  return (
    <div
      className={styles.card}
      style={{
        left: `${pos.left}px`,
        top: `${pos.top}px`,
        width: `${pos.width}px`,
        height: `${pos.height}px`,
      }}
    >
      <img
        src={cardBg}
        alt=""
        className={styles.bg}
        style={{ opacity: view === 'picking' ? 1 : 0 }}
      />
      {children}
    </div>
  );
}
