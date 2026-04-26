import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import styles from './PickARice.module.css';
import {
  RICE_ITEM_COUNT,
  RICE_ITEM_PITCH,
  ViewProvider,
  PreviewOptionProvider,
  ScrollProvider,
  ThemeProvider,
  PREVIEW_OPTIONS,
  THEME_CYCLE,
  type PreviewOption,
  type View,
} from './view';
import { GreenTab } from './components/GreenTab';
import { RiceCard } from './components/RiceCard';
import { ScreenContent } from './components/ScreenContent';
import { PreviewContent } from './components/PreviewContent';
import { ClosePin } from './components/ClosePin';
import { SoundButton } from './components/SoundButton';
import { ThemeKnob } from './components/ThemeKnob';
import { ScrollWheel } from './components/ScrollWheel';
import { PhysicalControls, type PhysicalControl } from './components/PhysicalControls';

const clampRiceIndex = (index: number) => Math.max(0, Math.min(RICE_ITEM_COUNT - 1, index));
type HoldDirection = -1 | 0 | 1;
const cyclePreviewOption = (option: PreviewOption, delta: -1 | 1) => {
  const index = PREVIEW_OPTIONS.indexOf(option);
  return PREVIEW_OPTIONS[(index + delta + PREVIEW_OPTIONS.length) % PREVIEW_OPTIONS.length];
};

export function PickARice() {
  const [view, setView] = useState<View>('picking');
  const [previewOption, setPreviewOption] = useState<PreviewOption>('install');
  const [focusedRiceIndex, setFocusedRiceIndex] = useState(0);
  const [riceScrollOffset, setRiceScrollOffset] = useState(0);
  const [riceNavRequest, setRiceNavRequest] = useState({ index: 0, version: 0 });
  const [riceHoldDirection, setRiceHoldDirection] = useState<HoldDirection>(0);
  const [pressedControls, setPressedControls] = useState<ReadonlySet<PhysicalControl>>(new Set());
  const requestedRiceIndexRef = useRef(0);
  const [cycleIdx, setCycleIdx] = useState(0);
  const theme = THEME_CYCLE[cycleIdx];
  const advance = useCallback(() => setCycleIdx((i) => (i + 1) % THEME_CYCLE.length), []);
  const themeValue = useMemo(() => ({ theme, advance }), [advance, theme]);
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
    if (view === 'preview') {
      setPreviewOption((option) => cyclePreviewOption(option, -1));
      return;
    }
    requestFocusedRice(requestedRiceIndexRef.current - 1);
  }, [requestFocusedRice, view]);

  const focusNextRice = useCallback(() => {
    if (view === 'preview') {
      setPreviewOption((option) => cyclePreviewOption(option, 1));
      return;
    }
    requestFocusedRice(requestedRiceIndexRef.current + 1);
  }, [requestFocusedRice, view]);

  const syncRiceScroll = useCallback((offset: number) => {
    const index = clampRiceIndex(Math.round(offset / RICE_ITEM_PITCH));
    requestedRiceIndexRef.current = index;
    setRiceScrollOffset(offset);
    setFocusedRiceIndex(index);
  }, []);

  const startRiceHold = useCallback((direction: -1 | 1) => {
    if (view === 'picking') setRiceHoldDirection(direction);
  }, [view]);

  const stopRiceHold = useCallback(() => setRiceHoldDirection(0), []);

  useEffect(() => {
    if (view !== 'picking') setRiceHoldDirection(0);
  }, [view]);

  const applyFocusedRice = useCallback(() => {
    setPreviewOption('install');
    setView((current) => (current === 'picking' ? 'preview' : 'picking'));
  }, []);

  const cycleOnBareStage = (e: React.MouseEvent<HTMLDivElement>) => {
    if (e.target !== e.currentTarget) return;
    setPreviewOption('install');
    setView((current) => (current === 'picking' ? 'preview' : 'picking'));
  };

  return (
    <ThemeProvider value={themeValue}>
      <ViewProvider view={view}>
        <PreviewOptionProvider value={previewOption}>
          <ScrollProvider value={scroll}>
            <div className={styles.stage} data-theme={theme} onClick={cycleOnBareStage}>
              <PhysicalControls
                onPrevious={focusPreviousRice}
                onNext={focusNextRice}
                onApply={applyFocusedRice}
                onHoldStart={startRiceHold}
                onHoldEnd={stopRiceHold}
                onPressedChange={setPressedControls}
              />
              <GreenTab />
              <RiceCard>
                <ScreenContent
                  holdDirection={riceHoldDirection}
                  navRequest={riceNavRequest}
                  pressedControls={pressedControls}
                  onScrollOffsetChange={syncRiceScroll}
                />
                <PreviewContent
                  themeName="themename"
                  creatorName="creatorname"
                  onApply={applyFocusedRice}
                />
              </RiceCard>
              <ClosePin />
              <SoundButton />
              <ThemeKnob />
              <ScrollWheel />
            </div>
          </ScrollProvider>
        </PreviewOptionProvider>
      </ViewProvider>
    </ThemeProvider>
  );
}
