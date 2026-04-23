import styles from './SoundButton.module.css';
import soundButtonSvg from '@/assets/figma/sound-button.svg';

/** Figma group 350:6571 — teal speaker sitting inside the green tab.
 *  Single SVG bakes in the outline ring, the filled teal disc, and the
 *  low-volume glyph so the three pieces can't drift out of alignment. */
export function SoundButton() {
  return <img src={soundButtonSvg} alt="" className={styles.button} />;
}
