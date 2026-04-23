import styles from './BottomDrop.module.css';
import dropShapeSvg from '@/assets/figma/drop-shape.svg';
import dropLeafSvg from '@/assets/figma/drop-leaf.svg';

/** Figma group 350:6583 + 350:6584 — mint drop shape tucked under the right
 *  edge of the card, with a rotated "sprout" glyph and three faint green
 *  dots sitting inside it. */
export function BottomDrop() {
  return (
    <>
      <img src={dropShapeSvg} alt="" className={styles.shape} />
      <div className={styles.leaf}>
        <img src={dropLeafSvg} alt="" className={styles.leafImg} />
      </div>
      <span className={`${styles.dot} ${styles.dotTop}`} />
      <span className={`${styles.dot} ${styles.dotBottom}`} />
      <span className={`${styles.dot} ${styles.dotLeft}`} />
    </>
  );
}
