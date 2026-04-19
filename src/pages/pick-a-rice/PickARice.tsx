import { useState } from 'react';
import styles from './PickARice.module.css';
import { ViewProvider, type View } from './view';
import { DotsBackground } from './components/DotsBackground';
import { GreenTab } from './components/GreenTab';
import { RiceCard } from './components/RiceCard';
import { ScreenContent } from './components/ScreenContent';
import { PreviewContent } from './components/PreviewContent';
import { SoundButton } from './components/SoundButton';
import { CloseIcon } from './components/CloseIcon';
import { BottomDrop } from './components/BottomDrop';
import { CreatorBadge } from './components/CreatorBadge';

export function PickARice() {
  const [view, setView] = useState<View>('picking');
  /** Debug-toggle trigger. Restricted to the bare stage (not child clicks) so
   *  future interactive children don't have to remember to `stopPropagation`. */
  const toggleOnBareStage = (e: React.MouseEvent<HTMLDivElement>) => {
    if (e.target !== e.currentTarget) return;
    setView((v) => (v === 'picking' ? 'preview' : 'picking'));
  };

  return (
    <ViewProvider view={view}>
      <div className={styles.stage} onClick={toggleOnBareStage}>
        <DotsBackground />
        <GreenTab />
        <RiceCard>
          <ScreenContent />
          <PreviewContent />
        </RiceCard>
        <SoundButton />
        <CloseIcon />
        <BottomDrop />
        <CreatorBadge name="Ricename" creator="creatorname" />
      </div>
    </ViewProvider>
  );
}
