import styles from './RiceList.module.css';
import { RiceItem } from './RiceItem';
import type { ScrollState } from '../view';

const PLACEHOLDERS = Array.from({ length: 6 }, (_, i) => ({
  themeName: `Theme ${i + 1}`,
  creatorName: 'by creatorname',
}));

/** Pitch = item height (311) + gap (23). activeIndex is whichever item's
 *  top edge the scroll is closest to. */
const PITCH = 334;

export function RiceList({ onScroll }: { onScroll: (s: ScrollState) => void }) {
  const handleScroll = (e: React.UIEvent<HTMLDivElement>) => {
    const offset = e.currentTarget.scrollTop;
    onScroll({
      offset,
      index: Math.max(0, Math.min(PLACEHOLDERS.length - 1, Math.round(offset / PITCH))),
      total: PLACEHOLDERS.length,
    });
  };
  return (
    <div className={styles.list} onScroll={handleScroll}>
      {PLACEHOLDERS.map((r, i) => (
        <RiceItem key={i} themeName={r.themeName} creatorName={r.creatorName} />
      ))}
    </div>
  );
}
