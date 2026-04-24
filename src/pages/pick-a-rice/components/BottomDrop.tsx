import { motion } from 'framer-motion';
import styles from './BottomDrop.module.css';
import dropShapeSvg from '@/assets/figma/drop-shape.svg';
import dropBodySvg from '@/assets/figma/drop-body.svg';
import dropHeadSvg from '@/assets/figma/drop-head.svg';
import dropShapePostSvg from '@/assets/figma/drop-shape-post.svg';
import dropLeafPostSvg from '@/assets/figma/drop-leaf-post.svg';
import {
  MORPH_TRANSITION,
  POSITIONS,
  SCREEN_FADE_TRANSITION,
  useTheme,
  useView,
  type Theme,
} from '../view';

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
        <img src={dropShapeSvg} alt="" className={styles.shapePick} />
        <img src={dropBodySvg} alt="" className={styles.body} />
        <motion.img
          src={dropHeadSvg}
          alt=""
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
        <img src={dropShapePostSvg} alt="" className={styles.shapePost} />
        <img src={dropLeafPostSvg} alt="" className={styles.decorPost} />
      </motion.div>
    </motion.div>
  );
}
