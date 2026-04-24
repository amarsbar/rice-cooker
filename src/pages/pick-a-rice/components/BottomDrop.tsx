import { motion } from 'framer-motion';
import styles from './BottomDrop.module.css';
import dropShapeSvg from '@/assets/figma/drop-shape.svg';
import dropLeafSvg from '@/assets/figma/drop-leaf.svg';
import dropShapePostSvg from '@/assets/figma/drop-shape-post.svg';
import dropLeafPostSvg from '@/assets/figma/drop-leaf-post.svg';
import { MORPH_TRANSITION, POSITIONS, SCREEN_FADE_TRANSITION, useView } from '../view';

/** Mint drop shape with sprout decoration. Picking and shrunken use
 *  different Figma assets (shape's left tail is slightly longer in the
 *  shrunken one; the decoration in shrunken is a single consolidated SVG
 *  versus picking's separate leaf + three pip dots) so both variants are
 *  rendered inside a motion wrapper and crossfaded on view change. */
export function BottomDrop() {
  const view = useView();
  const isPicking = view === 'picking';
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
      >
        <img src={dropShapeSvg} alt="" className={styles.shapePick} />
        <div className={styles.leaf}>
          <img src={dropLeafSvg} alt="" className={styles.leafImg} />
        </div>
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
