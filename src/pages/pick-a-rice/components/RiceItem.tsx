import styles from './RiceItem.module.css';
import placeholderRice from '@/assets/rices/placeholder-rice.webp';
import caelestiaScreenshot from '@/assets/rice-screenshots/caelestia.png';
import dmsScreenshot from '@/assets/rice-screenshots/dankmaterialshell.png';
import linuxRetroismScreenshot from '@/assets/rice-screenshots/linux-retroism.png';
import nandoroidScreenshot from '@/assets/rice-screenshots/nandoroid.png';
import noctaliaScreenshot from '@/assets/rice-screenshots/noctalia.png';
import whiskerScreenshot from '@/assets/rice-screenshots/whisker.png';
import zephyrScreenshot from '@/assets/rice-screenshots/zephyr.png';
import type { RiceListRow } from '@/shared/backend';

const SCREENSHOTS: Record<string, string> = {
  caelestia: caelestiaScreenshot,
  dms: dmsScreenshot,
  'linux-retroism': linuxRetroismScreenshot,
  nandoroid: nandoroidScreenshot,
  noctalia: noctaliaScreenshot,
  whisker: whiskerScreenshot,
  zephyr: zephyrScreenshot,
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
