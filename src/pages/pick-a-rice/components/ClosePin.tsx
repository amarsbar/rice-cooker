import styles from './ClosePin.module.css';
import closePinSvg from '@/assets/figma/close-pin.svg';

/** Figma node 350:6576 — orange pin with an X in the circular head and a
 *  shaft tucking behind the green tab. The clickable hitbox is limited to
 *  the circular head via clip-path; the pin shaft below it is decorative. */
export function ClosePin() {
  const handleClose: React.MouseEventHandler<HTMLButtonElement> = (e) => {
    e.stopPropagation();
    window.rice.closeWindow();
  };

  return (
    <div className={styles.wrap}>
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
