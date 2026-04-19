import styles from './CardHeader.module.css';
import logoSvg from '@/assets/figma/logo.svg';
import dividerSvg from '@/assets/figma/divider-dotted.svg';

interface CardHeaderProps {
  step: number;
  total: number;
  title?: string;
}

export function CardHeader({ step, total, title = 'Pick a rice' }: CardHeaderProps) {
  return (
    <>
      <div className={styles.logo}>
        <div className={styles.logoBleed}>
          <img src={logoSvg} alt="" />
        </div>
      </div>
      <p className={styles.title}>{title}</p>
      <div className={`${styles.pill} ${styles.pillStep}`}>
        <p>{step}</p>
      </div>
      <div className={`${styles.pill} ${styles.pillSlash}`}>
        <p>/</p>
      </div>
      <div className={`${styles.pill} ${styles.pillTotal}`}>
        <p>{total}</p>
      </div>
      <img src={dividerSvg} alt="" className={styles.divider} />
    </>
  );
}
