import { motion } from 'framer-motion';
import styles from './ThemeKnob.module.css';
import KnobBaseSvg from '@/assets/theme-knob/base.svg?react';
import KnobBodySvg from '@/assets/theme-knob/body.svg?react';
import KnobHandleSvg from '@/assets/theme-knob/handle.svg?react';
import { playRiceSound } from '../sounds';
import {
  MORPH_TRANSITION,
  POSITIONS,
  useTheme,
  useView,
  type Theme,
} from '../view';

const MotionHandle = motion.create(KnobHandleSvg);

/** Handle rotation per theme. */
const ROTATION_FOR_THEME: Record<Theme, number> = {
  t1: 40.15,
  t2: 0,
  t3: -36.87,
};

export function ThemeKnob() {
  const view = useView();
  const { theme, advance } = useTheme();
  const rotate = ROTATION_FOR_THEME[theme];

  return (
    <motion.div
      className={styles.group}
      initial={false}
      animate={POSITIONS[view].themeKnob}
      transition={MORPH_TRANSITION}
      style={{ pointerEvents: 'auto', cursor: 'pointer' }}
      data-capture-theme-knob
      onClick={() => {
        playRiceSound('themeClick');
        advance();
      }}
    >
      <KnobBaseSvg className={styles.base} />
      <KnobBodySvg className={styles.body} />
      <MotionHandle
        className={styles.handle}
        initial={false}
        animate={{ rotate }}
        transition={{ duration: 0.25, ease: 'easeOut' }}
      />
    </motion.div>
  );
}
