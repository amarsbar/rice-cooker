import styles from './CardHeader.module.css';
import MenuDotsSvg from '@/assets/figma/menu-dots.svg?react';
import screenDivider from '@/assets/figma/screen-divider.svg';

const APPLY_LABEL = 'APPLY';

export function CardHeader() {
  return (
    <>
      <MenuDotsSvg className={styles.menuIcon} />
      <p className={`${styles.label} ${styles.menuLabel}`}>Menu</p>
      <p className={`${styles.label} ${styles.nextLabel}`}>Next</p>
      <p className={`${styles.label} ${styles.prevLabel}`}>Prev</p>
      <div className={styles.applyCluster}>
        {APPLY_LABEL.split('').map((char, index) => (
          <span key={index} className={styles.applyLetter}>
            {char}
          </span>
        ))}
      </div>
      <img src={screenDivider} alt="" className={styles.divider} />
    </>
  );
}
