import styles from './PickARice.module.css';
import { GreenTab } from './components/GreenTab';
import { RiceCard } from './components/RiceCard';
import { CardHeader } from './components/CardHeader';
import { MainPreview } from './components/MainPreview';
import { PeekPreview } from './components/PeekPreview';
import { ClosePin } from './components/ClosePin';
import { SoundButton } from './components/SoundButton';
import { BottomDrop } from './components/BottomDrop';
import { CreatorBadge } from './components/CreatorBadge';

export function PickARice() {
  return (
    <div className={styles.stage}>
      <GreenTab />
      <RiceCard>
        <CardHeader />
        <MainPreview themeName="Theme name" creatorName="by creatorname" />
        <PeekPreview themeName="Theme name" creatorName="Creatorname" />
      </RiceCard>
      <ClosePin />
      <SoundButton />
      <BottomDrop />
      <CreatorBadge />
    </div>
  );
}
