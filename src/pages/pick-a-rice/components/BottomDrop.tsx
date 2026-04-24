import { useState } from 'react';
import { motion } from 'framer-motion';
import styles from './BottomDrop.module.css';
import dropShapeSvg from '@/assets/figma/drop-shape.svg';
import dropBodySvg from '@/assets/figma/drop-body.svg';
import dropHeadSvg from '@/assets/figma/drop-head.svg';
import dropShapePostSvg from '@/assets/figma/drop-shape-post.svg';
import dropLeafPostSvg from '@/assets/figma/drop-leaf-post.svg';
import { MORPH_TRANSITION, POSITIONS, SCREEN_FADE_TRANSITION, useView } from '../view';

/** Angles take the head from its resting left-pointing position to each
 *  pip dot. Cycle is center → top → center → bottom, indexed by click %. */
const HEAD_ROTATIONS = [0, 45, 0, -41] as const;

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
        <span className={`${styles.dot} ${styles.dotTop}`} />
        <span className={`${styles.dot} ${styles.dotBottom}`} />
        <span className={`${styles.dot} ${styles.dotLeft}`} />
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
