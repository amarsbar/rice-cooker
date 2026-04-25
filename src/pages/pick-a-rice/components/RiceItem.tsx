import styles from './RiceItem.module.css';
import themePreview from '@/assets/figma/theme-preview.png';
import bottomOverlay from '@/assets/figma/screen-preview-bottom-overlay.png';

type RiceItemVariant = 'primary' | 'trailing';

interface RiceItemProps {
  variant: RiceItemVariant;
}

export function RiceItem({ variant }: RiceItemProps) {
  const trailing = variant === 'trailing';
  return (
    <div className={styles.item}>
      <div className={`${styles.preview} ${trailing ? styles.trailing : styles.primary}`}>
        <img src={themePreview} alt="" className={styles.image} />
        {trailing && <img src={bottomOverlay} alt="" className={styles.trailingOverlay} />}
      </div>
    </div>
  );
}
