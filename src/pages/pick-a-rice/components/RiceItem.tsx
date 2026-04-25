import styles from './RiceItem.module.css';
import placeholderRice from '@/assets/rices/placeholder-rice.webp';

export function RiceItem() {
  return (
    <div className={styles.item}>
      <div className={styles.preview}>
        <img src={placeholderRice} alt="" className={styles.image} />
      </div>
    </div>
  );
}
