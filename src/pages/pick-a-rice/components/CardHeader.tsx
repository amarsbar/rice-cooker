import styles from './CardHeader.module.css';
import MenuDotsSvg from '@/assets/screen/menu-dots.svg?react';
import screenDivider from '@/assets/screen/divider.svg';
import type { PhysicalControl } from './PhysicalControls';
import type { CSSProperties } from 'react';

const APPLY_LABEL = 'APPLY';
type BubbleAnimation = 'hide';

interface CardHeaderProps {
  pressedControls: ReadonlySet<PhysicalControl>;
  menuOpen?: boolean;
  navOnly?: boolean;
  applyAnimation?: BubbleAnimation;
}

export function CardHeader({
  pressedControls,
  menuOpen = false,
  navOnly = false,
  applyAnimation,
}: CardHeaderProps) {
  const labelClass = (className: string, active: boolean) =>
    `${styles.label} ${styles.navLabel} ${className} ${menuOpen ? styles.navMenuOpen : ''} ${active ? styles.navPressed : ''}`;
  const showApply = !navOnly || applyAnimation;

  return (
    <>
      {!navOnly && (
        <>
          <MenuDotsSvg className={styles.menuIcon} />
          <p className={`${styles.label} ${styles.menuLabel}`}>Menu</p>
        </>
      )}
      <p className={labelClass(styles.prevLabel, pressedControls.has('up'))}>Prev</p>
      <p className={labelClass(styles.nextLabel, pressedControls.has('down'))}>Next</p>
      {showApply && (
        <>
          <div className={styles.applyCluster}>
            {APPLY_LABEL.split('').map((char, index) => (
              <span
                key={index}
                className={`${styles.applyLetter} ${applyAnimation === 'hide' ? styles.applyLetterHide : ''}`}
                style={{ '--bubble-index': index } as CSSProperties}
              >
                {char}
              </span>
            ))}
          </div>
          {!navOnly && <img src={screenDivider} alt="" className={styles.divider} />}
        </>
      )}
    </>
  );
}
