import styles from './RiceList.module.css';
import { RiceItem } from './RiceItem';

const PLACEHOLDER_RICES = [
  { themeName: 'Theme name', creatorName: 'by creatorname' },
  { themeName: 'Second rice', creatorName: 'by someone else' },
];

/** Scrollable list of rice items stacked inside the card, beneath the
 *  header. Simple vertical scroll; no snap, no fade — we'll iterate on
 *  that once this baseline is in place. */
export function RiceList() {
  return (
    <div className={styles.list}>
      {PLACEHOLDER_RICES.map((rice, i) => (
        <RiceItem key={i} themeName={rice.themeName} creatorName={rice.creatorName} />
      ))}
    </div>
  );
}
