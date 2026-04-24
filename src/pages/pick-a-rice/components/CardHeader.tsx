import styles from './CardHeader.module.css';
import logoSvg from '@/assets/figma/logo.svg';
import menuDotsSvg from '@/assets/figma/menu-dots.svg';
import { useView } from '../view';

/** Picking-state card header — logo + PICK A RICE letter pills + 3-dot menu.
 *  The whole header fades out when the card morphs into post-install. */
export function CardHeader() {
  const view = useView();
  /** Only shown in picking — preview and post-install replace the header
   *  with the shrunken-card content. */
  const visible = view === 'picking';
  return (
    <div
      className={styles.header}
      style={{ opacity: visible ? 1 : 0, pointerEvents: visible ? 'auto' : 'none' }}
    >
      <img src={logoSvg} alt="" className={styles.logo} />
      <div className={styles.letters}>
        <LetterCluster chars="PICK" />
        <LetterCluster chars="A" />
        <LetterCluster chars="RICE" />
      </div>
      <img src={menuDotsSvg} alt="" className={styles.menu} />
    </div>
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
