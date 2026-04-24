import { motion } from 'framer-motion';
import styles from './BottomDrop.module.css';
import DropShapeSvg from '@/assets/figma/drop-shape.svg?react';
import DropBodySvg from '@/assets/figma/drop-body.svg?react';
import DropHeadSvg from '@/assets/figma/drop-head.svg?react';
import DropShapePostSvg from '@/assets/figma/drop-shape-post.svg?react';
import DropLeafPostSvg from '@/assets/figma/drop-leaf-post.svg?react';
import {
  MORPH_TRANSITION,
  POSITIONS,
  SCREEN_FADE_TRANSITION,
  useTheme,
  useView,
  type Theme,
} from '../view';

const MotionDropHead = motion.create(DropHeadSvg);

/** Sprout head rotation per theme. Values come from Figma: t1 = +40.15°
 *  (knob up, points toward t1), t2 = 0° (centre), t3 = -36.87° (knob
 *  down). The sprout is both decoration and the theme picker — clicking
 *  it advances to the next theme in CYCLE. */
const ROTATION_FOR_THEME: Record<Theme, number> = {
  t1: 40.15,
  t2: 0,
  t3: -36.87,
};

const CYCLE: Theme[] = ['t2', 't1', 't3'];

export function BottomDrop() {
  const view = useView();
  const { theme, setTheme } = useTheme();
  const isPicking = view === 'picking';
  const rotate = ROTATION_FOR_THEME[theme];

  const advance = () => setTheme((t) => CYCLE[(CYCLE.indexOf(t) + 1) % CYCLE.length]!);

  return (
    <motion.div
      className={styles.group}
      initial={false}
      animate={POSITIONS[view].dropShape}
      transition={MORPH_TRANSITION}
    >
      <motion.div
        className={styles.variant}
        initial={false}
        animate={{ opacity: isPicking ? 1 : 0 }}
        transition={SCREEN_FADE_TRANSITION}
        style={{ pointerEvents: isPicking ? 'auto' : 'none', cursor: 'pointer' }}
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

      <motion.div
        className={styles.variant}
        initial={false}
        animate={{ opacity: isPicking ? 0 : 1 }}
        transition={SCREEN_FADE_TRANSITION}
      >
        <DropShapePostSvg className={styles.shapePost} />
        <DropLeafPostSvg className={styles.decorPost} />
      </motion.div>
    </motion.div>
  );
}
