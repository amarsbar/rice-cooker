import { motion } from 'framer-motion';
import styles from './SoundButton.module.css';
import soundBgSvg from '@/assets/figma/sound-bg.svg';
import soundIconSvg from '@/assets/figma/sound-icon.svg';
import { MORPH_TRANSITION, POSITIONS, useView } from '../view';

/** Figma node 168:6743 / 168:6842 — teal speaker sitting inside the green tab. */
export function SoundButton() {
  const view = useView();
  return (
    <>
      <motion.div
        className={styles.ring}
        initial={false}
        animate={POSITIONS[view].soundRing}
        transition={MORPH_TRANSITION}
      >
        <img src={soundBgSvg} alt="" />
      </motion.div>
      <motion.div
        className={styles.inner}
        initial={false}
        animate={POSITIONS[view].soundInner}
        transition={MORPH_TRANSITION}
      >
        <img src={soundIconSvg} alt="" className={styles.icon} />
      </motion.div>
    </>
  );
}
