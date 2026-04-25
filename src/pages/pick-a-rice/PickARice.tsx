import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
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
const SCROLL_SETTLE_MS = 180;

export function PickARice() {
  const [view, setView] = useState<View>('picking');
  const [focusedRiceIndex, setFocusedRiceIndex] = useState(0);
  const [riceScrollOffset, setRiceScrollOffset] = useState(0);
  const [riceNavRequest, setRiceNavRequest] = useState({ index: 0, version: 0 });
  const requestedRiceIndexRef = useRef(0);
  const scrollSettleTimerRef = useRef<ReturnType<typeof window.setTimeout> | null>(null);
  const [cycleIdx, setCycleIdx] = useState(0);
  const theme = THEME_CYCLE[cycleIdx]!;
  const advance = () => setCycleIdx((i) => (i + 1) % THEME_CYCLE.length);
  const scroll = useMemo(
    () => ({
      offset: riceScrollOffset,
      index: focusedRiceIndex,
      total: RICE_ITEM_COUNT,
    }),
    [focusedRiceIndex, riceScrollOffset],
  );

  const requestFocusedRice = useCallback((nextIndex: number) => {
    const index = clampRiceIndex(nextIndex);
    requestedRiceIndexRef.current = index;
    setRiceNavRequest((request) => ({ index, version: request.version + 1 }));
  }, []);

  const focusPreviousRice = useCallback(() => {
    if (view !== 'picking') return;
    requestFocusedRice(requestedRiceIndexRef.current - 1);
  }, [requestFocusedRice, view]);

  const focusNextRice = useCallback(() => {
    if (view !== 'picking') return;
    requestFocusedRice(requestedRiceIndexRef.current + 1);
  }, [requestFocusedRice, view]);

  const syncRiceScroll = useCallback((offset: number) => {
    const index = clampRiceIndex(Math.round(offset / RICE_ITEM_PITCH));
    setRiceScrollOffset(offset);
    setFocusedRiceIndex(index);
    if (scrollSettleTimerRef.current !== null) {
      window.clearTimeout(scrollSettleTimerRef.current);
    }
    scrollSettleTimerRef.current = window.setTimeout(() => {
      requestedRiceIndexRef.current = index;
      scrollSettleTimerRef.current = null;
    }, SCROLL_SETTLE_MS);
  }, []);

  useEffect(() => () => {
    if (scrollSettleTimerRef.current !== null) {
      window.clearTimeout(scrollSettleTimerRef.current);
    }
  }, []);

  const applyFocusedRice = useCallback(() => {
    setView((current) => {
      if (current === 'picking') return 'preview';
      if (current === 'preview') return 'post-install';
      return 'picking';
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
              <ScreenContent
                navRequest={riceNavRequest}
                onScrollOffsetChange={syncRiceScroll}
              />
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
