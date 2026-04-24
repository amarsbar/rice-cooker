import { useState } from 'react';
import { motion } from 'framer-motion';
import styles from './BottomDrop.module.css';
import dropShapeSvg from '@/assets/figma/drop-shape.svg';
import dropBodySvg from '@/assets/figma/drop-body.svg';
import dropHeadSvg from '@/assets/figma/drop-head.svg';
import dropShapePostSvg from '@/assets/figma/drop-shape-post.svg';
import dropLeafPostSvg from '@/assets/figma/drop-leaf-post.svg';
import { MORPH_TRANSITION, POSITIONS, SCREEN_FADE_TRANSITION, useView } from '../view';

const HEAD_ROTATIONS = [0, 38.5, 0, -38.5] as const;

export function BottomDrop() {
  const view = useView();
  const isPicking = view === 'picking';
  const [clicks, setClicks] = useState(0);
  const rotate = HEAD_ROTATIONS[clicks % HEAD_ROTATIONS.length];
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
        onClick={() => setClicks((c) => c + 1)}
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
