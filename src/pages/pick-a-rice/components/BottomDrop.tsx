import { motion } from 'framer-motion';
import styles from './BottomDrop.module.css';
import DropShapeSvg from '@/assets/figma/drop-shape.svg?react';
import DropBodySvg from '@/assets/figma/drop-body.svg?react';
import DropHeadSvg from '@/assets/figma/drop-head.svg?react';
import {
  MORPH_TRANSITION,
  POSITIONS,
  useTheme,
  useView,
  type Theme,
} from '../view';

const MotionDropHead = motion.create(DropHeadSvg);

/** Sprout head rotation per theme. Figma values: t1 = +40.15° (knob up
 *  ≈ theme 1), t2 = 0° (centre), t3 = -36.87° (knob down ≈ theme 3). */
const ROTATION_FOR_THEME: Record<Theme, number> = {
  t1: 40.15,
  t2: 0,
  t3: -36.87,
};

export function BottomDrop() {
  const view = useView();
  const { theme, advance } = useTheme();
  const rotate = ROTATION_FOR_THEME[theme];

  return (
    <motion.div
      className={styles.group}
      initial={false}
      animate={POSITIONS[view].dropShape}
      transition={MORPH_TRANSITION}
      style={{ pointerEvents: 'auto', cursor: 'pointer' }}
      onClick={advance}
    >
      <DropShapeSvg className={styles.shapePick} />
      <DropBodySvg className={styles.body} />
      <MotionDropHead
        className={styles.head}
        initial={false}
        animate={{ rotate }}
        transition={{ duration: 0.25, ease: 'easeOut' }}
      />
    </motion.div>
  );
}
