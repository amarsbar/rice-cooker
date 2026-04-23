import styles from './SoundButton.module.css';
import soundRingSvg from '@/assets/figma/sound-ring.svg';
import soundIconSvg from '@/assets/figma/sound-icon.svg';

/** Figma group 350:6571 — teal speaker sitting inside the green tab. A thin
 *  teal ring (350:6572) wraps a filled teal circle (350:6573) with the
 *  low-volume icon centered inside. */
export function SoundButton() {
  return (
    <>
      <img src={soundRingSvg} alt="" className={styles.ring} />
      <div className={styles.inner}>
        <img src={soundIconSvg} alt="" className={styles.icon} />
      </div>
    </>
  );
}
