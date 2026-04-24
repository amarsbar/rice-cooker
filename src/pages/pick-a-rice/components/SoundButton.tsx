import { motion } from 'framer-motion';
import styles from './SoundButton.module.css';
import soundButtonSvg from '@/assets/figma/sound-button.svg';
import { MORPH_TRANSITION, POSITIONS, useView } from '../view';

/** Teal speaker (single consolidated SVG — outline ring + filled disc +
 *  glyph baked in) sitting inside the green tab. Position morphs with
 *  the card. */
export function SoundButton() {
  const view = useView();
  return (
    <motion.img
      src={soundButtonSvg}
      alt=""
      className={styles.button}
      initial={false}
      animate={POSITIONS[view].soundButton}
      transition={MORPH_TRANSITION}
    />
  );
}
