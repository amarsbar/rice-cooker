import styles from './SoundButton.module.css';
import soundButtonSvg from '@/assets/figma/sound-button.svg';
import { POSITIONS, useView } from '../view';

/** Teal speaker sitting inside the green tab. Position shifts with the
 *  card morph. */
export function SoundButton() {
  const view = useView();
  const pos = POSITIONS[view].soundButton;
  return (
    <img
      src={soundButtonSvg}
      alt=""
      className={styles.button}
      style={{ left: `${pos.left}px`, top: `${pos.top}px` }}
    />
  );
}
