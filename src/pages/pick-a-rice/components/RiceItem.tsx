import styles from './RiceItem.module.css';
import themePreview from '@/assets/figma/theme-preview.webp';

export function RiceItem() {
  return (
    <div className={styles.item}>
      <div className={styles.preview}>
        <img src={themePreview} alt="" className={styles.image} />
      </div>
    </div>
  );
}
