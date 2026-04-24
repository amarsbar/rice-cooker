import styles from './ClosePin.module.css';
import closePinSvg from '@/assets/figma/close-pin.svg';
import { POSITIONS, useView } from '../view';

/** Orange pin with the X-in-circle head and a shaft that tucks behind the
 *  green tab. Moves between picking (right of card) and post-install
 *  (above the shrunken card). */
export function ClosePin() {
  const view = useView();
  const pos = POSITIONS[view].closePin;

  const handleClose: React.MouseEventHandler<HTMLButtonElement> = (e) => {
    e.stopPropagation();
    window.rice.closeWindow();
  };

  return (
    <div
      className={styles.wrap}
      style={{ left: `${pos.left}px`, top: `${pos.top}px` }}
    >
      <img src={closePinSvg} alt="" />
      <button
        type="button"
        className={styles.hitbox}
        aria-label="Close window"
        onClick={handleClose}
      />
    </div>
  );
}
