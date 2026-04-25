import styles from './RiceItem.module.css';
import themePreview from '@/assets/figma/theme-preview.png';

export function RiceItem() {
  return (
    <div className={styles.item}>
      <div className={styles.preview}>
        <img src={themePreview} alt="" className={styles.image} />
      </div>
    </div>
  );
}
