import { motion } from 'framer-motion';
import styles from './BottomDrop.module.css';
import dropShapeSvg from '@/assets/figma/drop-shape.svg';
import dropInnerSvg from '@/assets/figma/drop-inner.svg';
import { MORPH_TRANSITION, POSITIONS, useView } from '../view';

/** Figma nodes 168:6756+6757 / 168:6855+6856 — mint-green drop shape with
 *  the small marker circle inside. */
export function BottomDrop() {
  const view = useView();
  return (
    <>
      <motion.div
        className={styles.shape}
        initial={false}
        animate={POSITIONS[view].dropShape}
        transition={MORPH_TRANSITION}
      >
        <div className={styles.shapeBleed}>
          <img src={dropShapeSvg} alt="" />
        </div>
      </motion.div>
      <motion.div
        className={styles.inner}
        initial={false}
        animate={POSITIONS[view].dropInner}
        transition={MORPH_TRANSITION}
      >
        <img src={dropInnerSvg} alt="" />
      </motion.div>
    </>
  );
}
