import styles from './RiceItem.module.css';
import type { RiceListRow } from '@/shared/backend';
import { getRiceScreenshot } from '../riceScreenshots';

export function RiceItem({ rice }: { rice: RiceListRow }) {
  const image = getRiceScreenshot(rice.name);
  return (
    <div className={styles.item}>
      <div className={styles.preview}>
        <img src={image} alt="" className={styles.image} />
      </div>
    </div>
  );
}
