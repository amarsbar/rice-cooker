import styles from './GreenTab.module.css';
import { POSITIONS, useView } from '../view';

/** Mint tab that protrudes past the card's right edge. Shifts up + shrinks
 *  slightly (90→81px tall) when the card morphs to post-install. */
export function GreenTab() {
  const view = useView();
  const pos = POSITIONS[view].greenTab;
  return (
    <div
      className={styles.tab}
      style={{ left: `${pos.left}px`, top: `${pos.top}px`, height: `${pos.height}px` }}
    />
  );
}
