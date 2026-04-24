import styles from './BottomDrop.module.css';
import dropShapeSvg from '@/assets/figma/drop-shape.svg';
import dropLeafSvg from '@/assets/figma/drop-leaf.svg';
import dropShapePostSvg from '@/assets/figma/drop-shape-post.svg';
import dropLeafPostSvg from '@/assets/figma/drop-leaf-post.svg';
import { POSITIONS, useView } from '../view';

/** Mint drop shape with its sprout decoration. Picking and post-install use
 *  different Figma assets (the shape's left tail is slightly longer in the
 *  post-install one; the decoration is a single consolidated SVG versus
 *  the picking version's separate leaf + three dots) so both groups are
 *  rendered at their own positions and crossfaded on view change. */
export function BottomDrop() {
  const view = useView();
  const isPicking = view === 'picking';
  const pickingPos = POSITIONS.picking.dropShape;
  const postPos = POSITIONS['post-install'].dropShape;
  return (
    <>
      <div
        className={styles.group}
        style={{
          left: `${pickingPos.left}px`,
          top: `${pickingPos.top}px`,
          opacity: isPicking ? 1 : 0,
        }}
      >
        <img src={dropShapeSvg} alt="" className={styles.shapePick} />
        <div className={styles.leaf}>
          <img src={dropLeafSvg} alt="" className={styles.leafImg} />
        </div>
        <span className={`${styles.dot} ${styles.dotTop}`} />
        <span className={`${styles.dot} ${styles.dotBottom}`} />
        <span className={`${styles.dot} ${styles.dotLeft}`} />
      </div>

      <div
        className={styles.group}
        style={{
          left: `${postPos.left}px`,
          top: `${postPos.top}px`,
          opacity: isPicking ? 0 : 1,
        }}
      >
        <img src={dropShapePostSvg} alt="" className={styles.shapePost} />
        <img src={dropLeafPostSvg} alt="" className={styles.decorPost} />
      </div>
    </>
  );
}
