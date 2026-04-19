import { motion } from 'framer-motion';
import styles from './CloseIcon.module.css';
import closePinSvg from '@/assets/figma/close-pin.svg';
import { MORPH_TRANSITION, POSITIONS, useView } from '../view';

/** Figma node 168:6748 / 168:6847 — orange pin-style close icon above the
 *  green tab. The clickable hitbox is limited to the circular head via
 *  clip-path; the pin shaft below it is purely decorative. */
export function CloseIcon() {
  const view = useView();

  const handleClose: React.MouseEventHandler<HTMLButtonElement> = (e) => {
    e.stopPropagation();
    window.rice.closeWindow();
  };

  return (
    <motion.div
      className={styles.wrap}
      initial={false}
      animate={POSITIONS[view].closeIcon}
      transition={MORPH_TRANSITION}
    >
      <img src={closePinSvg} alt="" />
      <button
        type="button"
        className={styles.hitbox}
        aria-label="Close window"
        onClick={handleClose}
      />
    </motion.div>
  );
}
