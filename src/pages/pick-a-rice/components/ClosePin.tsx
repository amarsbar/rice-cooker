import { motion } from 'framer-motion';
import styles from './ClosePin.module.css';
import closePinSvg from '@/assets/figma/close-pin.svg';
import { MORPH_TRANSITION, POSITIONS, useView } from '../view';

/** Orange pin with the X-in-circle head and a shaft that tucks behind the
 *  green tab. Moves between picking (right of card) and the shrunken view
 *  (above the smaller card). */
export function ClosePin() {
  const view = useView();

  const handleClose: React.MouseEventHandler<HTMLButtonElement> = (e) => {
    e.stopPropagation();
    window.rice.closeWindow();
  };

  return (
    <motion.div
      className={styles.wrap}
      initial={false}
      animate={POSITIONS[view].closePin}
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
