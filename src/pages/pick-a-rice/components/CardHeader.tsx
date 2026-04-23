import styles from './CardHeader.module.css';
import logoSvg from '@/assets/figma/logo.svg';
import menuDotsSvg from '@/assets/figma/menu-dots.svg';

/** Figma nodes 350:6545 (logo), 350:6548 (PICK A RICE letter pills), and
 *  350:6542 (3-dot menu) — the card's top row. */
export function CardHeader() {
  return (
    <>
      <img src={logoSvg} alt="" className={styles.logo} />
      <div className={styles.letters}>
        <LetterCluster chars="PICK" />
        <LetterCluster chars="A" />
        <LetterCluster chars="RICE" />
      </div>
      <img src={menuDotsSvg} alt="" className={styles.menu} />
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
