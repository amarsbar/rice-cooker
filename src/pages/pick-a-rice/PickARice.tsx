import { useState } from 'react';
import styles from './PickARice.module.css';
import {
  ViewProvider,
  ScrollProvider,
  ThemeProvider,
  THEME_CYCLE,
  type View,
  type ScrollState,
} from './view';
import { GreenTab } from './components/GreenTab';
import { RiceCard } from './components/RiceCard';
import { ScreenContent } from './components/ScreenContent';
import { PreviewContent } from './components/PreviewContent';
import { PostInstallContent } from './components/PostInstallContent';
import { ClosePin } from './components/ClosePin';
import { SoundButton } from './components/SoundButton';
import { BottomDrop } from './components/BottomDrop';
import { CreatorBadge } from './components/CreatorBadge';
import { PhysicalControls } from './components/PhysicalControls';

const CYCLE: View[] = ['picking', 'preview', 'post-install'];

export function PickARice() {
  const [view, setView] = useState<View>('picking');
  const [scroll, setScroll] = useState<ScrollState>({ offset: 0, index: 0, total: 10 });
  const [cycleIdx, setCycleIdx] = useState(0);
  const theme = THEME_CYCLE[cycleIdx]!;
  const advance = () => setCycleIdx((i) => (i + 1) % THEME_CYCLE.length);

  const cycleOnBareStage = (e: React.MouseEvent<HTMLDivElement>) => {
    if (e.target !== e.currentTarget) return;
    setView((v) => CYCLE[(CYCLE.indexOf(v) + 1) % CYCLE.length]!);
  };

  return (
    <ThemeProvider value={{ theme, advance }}>
      <ViewProvider view={view}>
        <ScrollProvider value={scroll}>
          <div className={styles.stage} data-theme={theme} onClick={cycleOnBareStage}>
            <PhysicalControls />
            <GreenTab />
            <RiceCard>
              <ScreenContent onScroll={setScroll} />
              <PreviewContent themeName="theme name" creatorName="by creator name" />
              <PostInstallContent themeName="theme name" />
            </RiceCard>
            <ClosePin />
            <SoundButton />
            <BottomDrop />
            <CreatorBadge />
          </div>
        </ScrollProvider>
      </ViewProvider>
    </ThemeProvider>
  );
}
