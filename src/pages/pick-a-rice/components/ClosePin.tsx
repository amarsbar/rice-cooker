import { motion } from 'framer-motion';
import styles from './ClosePin.module.css';
import ClosePinSvg from '@/assets/figma/close-pin.svg?react';
import { MORPH_TRANSITION, POSITIONS, useView } from '../view';

/** Close-window pin — pin-shaped close icon that sits to the right of
 *  the green tab. The hitbox is clipped to the circular head so clicks
 *  on the pin's shaft don't fire. Position animates with the card morph. */
export function ClosePin() {
  const view = useView();
  return (
    <motion.div
      className={styles.wrap}
      initial={false}
      animate={POSITIONS[view].closePin}
      transition={MORPH_TRANSITION}
    >
      <ClosePinSvg className={styles.svg} />
      <button
        type="button"
        className={styles.hitbox}
        aria-label="Close window"
        onClick={(e) => {
          e.stopPropagation();
          window.rice?.closeWindow?.();
        }}
      />
    </motion.div>
  );
}
