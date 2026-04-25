import { motion } from 'framer-motion';
import styles from './SoundButton.module.css';
import SoundButtonSvg from '@/assets/icon-buttons/sound.svg?react';
import { MORPH_TRANSITION, POSITIONS, useView } from '../view';

const MotionSound = motion.create(SoundButtonSvg);

/** Sound button — outer ring + filled disc + speaker glyph, all driven
 *  by the theme's sound-body / sound-icon tokens (via SVG classes). */
export function SoundButton() {
  const view = useView();
  return (
    <MotionSound
      className={styles.button}
      initial={false}
      animate={POSITIONS[view].soundButton}
      transition={MORPH_TRANSITION}
    />
  );
}
