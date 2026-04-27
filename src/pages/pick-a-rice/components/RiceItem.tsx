import styles from './RiceItem.module.css';
import placeholderRice from '@/assets/rices/placeholder-rice.webp';
import caelestiaScreenshot from '@/assets/rice-screenshots/caelestia.png';
import dmsScreenshot from '@/assets/rice-screenshots/dankmaterialshell.png';
import noctaliaScreenshot from '@/assets/rice-screenshots/noctalia-dark-1.png';
import type { RiceListRow } from '@/shared/backend';

const SCREENSHOTS: Record<string, string> = {
  caelestia: caelestiaScreenshot,
  dms: dmsScreenshot,
  noctalia: noctaliaScreenshot,
};

export function RiceItem({ rice }: { rice: RiceListRow }) {
  const image = SCREENSHOTS[rice.name] ?? placeholderRice;
  return (
    <div className={styles.item}>
      <div className={styles.preview}>
        <img src={image} alt="" className={styles.image} />
      </div>
    </div>
  );
}
