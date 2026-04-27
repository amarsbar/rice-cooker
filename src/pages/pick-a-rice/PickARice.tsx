import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import styles from './PickARice.module.css';
import {
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
import type { BackendRunRequest, RiceListRow } from '@/shared/backend';
import { GreenTab } from './components/GreenTab';
import { RiceCard } from './components/RiceCard';
import { ScreenContent } from './components/ScreenContent';
import { PreviewContent } from './components/PreviewContent';
import { ClosePin } from './components/ClosePin';
import { SoundButton } from './components/SoundButton';
import { ThemeKnob } from './components/ThemeKnob';
import { ScrollWheel } from './components/ScrollWheel';
import { PhysicalControls, type PhysicalControl } from './components/PhysicalControls';

const clampRiceIndex = (index: number, count: number) => Math.max(0, Math.min(count - 1, index));
type HoldDirection = -1 | 0 | 1;
const cyclePreviewOption = (option: PreviewOption, delta: -1 | 1) => {
  const index = PREVIEW_OPTIONS.indexOf(option);
  return PREVIEW_OPTIONS[(index + delta + PREVIEW_OPTIONS.length) % PREVIEW_OPTIONS.length];
};

export function PickARice() {
  const [view, setView] = useState<View>('picking');
  const [previewOption, setPreviewOption] = useState<PreviewOption>('install');
  const [rices, setRices] = useState<RiceListRow[]>([]);
  const [backendRunning, setBackendRunning] = useState(false);
  const [focusedRiceIndex, setFocusedRiceIndex] = useState(0);
  const [riceScrollOffset, setRiceScrollOffset] = useState(0);
  const [riceNavRequest, setRiceNavRequest] = useState({ index: 0, version: 0 });
  const [riceHoldDirection, setRiceHoldDirection] = useState<HoldDirection>(0);
  const [pressedControls, setPressedControls] = useState<ReadonlySet<PhysicalControl>>(new Set());
  const backendRunningRef = useRef(false);
  const requestedRiceIndexRef = useRef(0);
  const [cycleIdx, setCycleIdx] = useState(0);
  const theme = THEME_CYCLE[cycleIdx];
  const advance = useCallback(() => setCycleIdx((i) => (i + 1) % THEME_CYCLE.length), []);
  const themeValue = useMemo(() => ({ theme, advance }), [advance, theme]);
  const selectedRice = rices[focusedRiceIndex];
  const latestApplyStateRef = useRef({ backendRunning, previewOption, selectedRice, view });
  latestApplyStateRef.current = { backendRunning, previewOption, selectedRice, view };
  const scroll = useMemo(
    () => ({
      offset: riceScrollOffset,
      index: focusedRiceIndex,
      total: Math.max(rices.length, 1),
    }),
    [focusedRiceIndex, riceScrollOffset, rices.length],
  );

  useEffect(() => {
    let cancelled = false;
    window.rice.backend
      .list()
      .then((rows) => {
        if (cancelled) return;
        setRices(rows);
      })
      .catch((error: unknown) => {
        if (cancelled) return;
        console.error('[rice-cooker] backend list failed:', error);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const runBackend = useCallback(async (request: BackendRunRequest, afterSuccess?: () => void) => {
    if (backendRunningRef.current) return;
    backendRunningRef.current = true;
    setBackendRunning(true);
    try {
      const result = await window.rice.backend.run(request);
      if (result.ok) {
        afterSuccess?.();
      } else {
        console.error('[rice-cooker] backend command failed:', result);
      }
    } catch (error) {
      console.error('[rice-cooker] backend command failed:', error);
    } finally {
      backendRunningRef.current = false;
      setBackendRunning(false);
    }
  }, []);

  const requestFocusedRice = useCallback((nextIndex: number) => {
    const index = clampRiceIndex(nextIndex, rices.length);
    requestedRiceIndexRef.current = index;
    setRiceNavRequest((request) => ({ index, version: request.version + 1 }));
  }, [rices.length]);

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
    const index = clampRiceIndex(Math.round(offset / RICE_ITEM_PITCH), rices.length);
    requestedRiceIndexRef.current = index;
    setRiceScrollOffset(offset);
    setFocusedRiceIndex(index);
  }, [rices.length]);

  const startRiceHold = useCallback((direction: -1 | 1) => {
    if (view === 'picking') setRiceHoldDirection(direction);
  }, [view]);

  const stopRiceHold = useCallback(() => setRiceHoldDirection(0), []);

  useEffect(() => {
    if (view !== 'picking') setRiceHoldDirection(0);
  }, [view]);

  const applyFocusedRice = useCallback(() => {
    const { backendRunning, previewOption, selectedRice, view } = latestApplyStateRef.current;
    if (backendRunning || !selectedRice) return;

    if (view === 'picking') {
      setPreviewOption('install');
      setView('preview');
      void runBackend({ command: 'preview', name: selectedRice.name });
      return;
    }

    if (previewOption === 'leave') {
      void runBackend({ command: 'uninstall' }, () => setView('picking'));
    } else if (previewOption === 'install' && selectedRice.install_supported) {
      void runBackend({ command: 'try', name: selectedRice.name });
    } else if (previewOption === 'dots') {
      window.open(selectedRice.repo, '_blank', 'noopener,noreferrer');
    }
  }, [runBackend]);

  const cycleOnBareStage = (e: React.MouseEvent<HTMLDivElement>) => {
    if (e.target !== e.currentTarget) return;
    applyFocusedRice();
  };

  return (
    <ThemeProvider value={themeValue}>
      <ViewProvider view={view}>
        <PreviewOptionProvider value={previewOption}>
          <ScrollProvider value={scroll}>
            <div
              className={styles.stage}
              data-theme={theme}
              data-preview-option={previewOption}
              onClick={cycleOnBareStage}
            >
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
                  rices={rices}
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
