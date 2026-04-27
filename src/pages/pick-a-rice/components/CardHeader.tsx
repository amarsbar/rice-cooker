import styles from './CardHeader.module.css';
import MenuDotsSvg from '@/assets/screen/menu-dots.svg?react';
import screenDivider from '@/assets/screen/divider.svg';
import type { PhysicalControl } from './PhysicalControls';

const APPLY_LABEL = 'APPLY';

interface CardHeaderProps {
  pressedControls: ReadonlySet<PhysicalControl>;
}

export function CardHeader({ pressedControls }: CardHeaderProps) {
  const labelClass = (className: string, active: boolean) =>
    `${styles.label} ${styles.navLabel} ${className} ${active ? styles.navPressed : ''}`;

  return (
    <>
      <MenuDotsSvg className={styles.menuIcon} />
      <p className={`${styles.label} ${styles.menuLabel}`}>Menu</p>
      <p className={labelClass(styles.prevLabel, pressedControls.has('up'))}>Prev</p>
      <p className={labelClass(styles.nextLabel, pressedControls.has('down'))}>Next</p>
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
