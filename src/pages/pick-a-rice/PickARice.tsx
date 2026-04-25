import { useCallback, useMemo, useState } from 'react';
import styles from './PickARice.module.css';
import {
  RICE_ITEM_COUNT,
  RICE_ITEM_PITCH,
  ViewProvider,
  ScrollProvider,
  ThemeProvider,
  THEME_CYCLE,
  type View,
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
const clampRiceIndex = (index: number) => Math.max(0, Math.min(RICE_ITEM_COUNT - 1, index));

export function PickARice() {
  const [view, setView] = useState<View>('picking');
  const [focusedRiceIndex, setFocusedRiceIndex] = useState(0);
  const [cycleIdx, setCycleIdx] = useState(0);
  const theme = THEME_CYCLE[cycleIdx]!;
  const advance = () => setCycleIdx((i) => (i + 1) % THEME_CYCLE.length);
  const scroll = useMemo(
    () => ({
      offset: focusedRiceIndex * RICE_ITEM_PITCH,
      index: focusedRiceIndex,
      total: RICE_ITEM_COUNT,
    }),
    [focusedRiceIndex],
  );

  const focusPreviousRice = useCallback(() => {
    setFocusedRiceIndex((index) => clampRiceIndex(index - 1));
  }, []);

  const focusNextRice = useCallback(() => {
    setFocusedRiceIndex((index) => clampRiceIndex(index + 1));
  }, []);

  const applyFocusedRice = useCallback(() => {
    setView((current) => {
      if (current === 'picking') return 'preview';
      if (current === 'preview') return 'post-install';
      return current;
    });
  }, []);

  const cycleOnBareStage = (e: React.MouseEvent<HTMLDivElement>) => {
    if (e.target !== e.currentTarget) return;
    setView((v) => CYCLE[(CYCLE.indexOf(v) + 1) % CYCLE.length]!);
  };

  return (
    <ThemeProvider value={{ theme, advance }}>
      <ViewProvider view={view}>
        <ScrollProvider value={scroll}>
          <div className={styles.stage} data-theme={theme} onClick={cycleOnBareStage}>
            <PhysicalControls
              onPrevious={focusPreviousRice}
              onNext={focusNextRice}
              onApply={applyFocusedRice}
            />
            <GreenTab />
            <RiceCard>
              <ScreenContent focusedIndex={focusedRiceIndex} />
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
