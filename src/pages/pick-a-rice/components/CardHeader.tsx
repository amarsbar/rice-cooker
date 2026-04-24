import styles from './CardHeader.module.css';
import LogoSvg from '@/assets/figma/logo.svg?react';
import MenuDotsSvg from '@/assets/figma/menu-dots.svg?react';

/** Picking-state card header — logo + PICK A RICE letter pills + 3-dot menu.
 *  Visibility is managed by the parent <ScreenContent> fader. */
export function CardHeader() {
  return (
    <>
      <LogoSvg className={styles.logo} />
      <div className={styles.letters}>
        <LetterCluster chars="PICK" />
        <LetterCluster chars="A" />
        <LetterCluster chars="RICE" />
      </div>
      <MenuDotsSvg className={styles.menu} />
    </>
  );
}

function LetterCluster({ chars }: { chars: string }) {
  return (
    <div className={styles.cluster}>
      {chars.split('').map((c, i) => (
        <span key={i} className={styles.letter}>
          {c}
        </span>
      ))}
    </div>
  );
}
