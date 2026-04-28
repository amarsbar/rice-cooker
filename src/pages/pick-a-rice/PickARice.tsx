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
import { ClosingCircles } from './components/ClosingCircles';
import { Antenna } from './components/Antenna';
import { PreviewStars } from './components/PreviewStars';
import { MenuScreen } from './components/MenuScreen';
import { playRiceSound } from './sounds';
import { MENU_ITEMS, type MenuItem } from './menuOptions';

const clampRiceIndex = (index: number, count: number) => Math.max(0, Math.min(count - 1, index));
type HoldDirection = -1 | 0 | 1;
const DOWNLOAD_DURATION_MS = 1000;
const MENU_FADE_MS = 100;
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
  const [menuOpen, setMenuOpen] = useState(false);
  const [pickerExiting, setPickerExiting] = useState(false);
  const [pickerEntering, setPickerEntering] = useState(false);
  const [menuExiting, setMenuExiting] = useState(false);
  const [menuItem, setMenuItem] = useState<MenuItem>(MENU_ITEMS[0]);
  const requestedRiceIndexRef = useRef(0);
  const lastRiceSoundTargetRef = useRef(0);
  const menuTransitionTimeoutRef = useRef<ReturnType<typeof window.setTimeout> | null>(null);
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
  const pickerTransitioning = pickerExiting || pickerEntering;

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

  const playMoveForTarget = useCallback((nextIndex: number) => {
    const index = clampRiceIndex(nextIndex, rices.length);
    if (index === lastRiceSoundTargetRef.current) return;
    const sound = index < lastRiceSoundTargetRef.current ? 'moveUp' : 'moveDown';
    lastRiceSoundTargetRef.current = index;
    playRiceSound(sound);
  }, [rices.length]);

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
    if (index !== requestedRiceIndexRef.current) playMoveForTarget(index);
    requestedRiceIndexRef.current = index;
    setRiceNavRequest((request) => ({ index, version: request.version + 1 }));
  }, [playMoveForTarget]);

  const focusPreviousRice = useCallback(() => {
    if (menuOpen || pickerTransitioning) return;
    if (view === 'preview') {
      playRiceSound('moveUp');
      setPreviewOption((option) => cyclePreviewOption(option, -1));
      return;
    }
    if (view === 'downloading') return;
    requestFocusedRice(requestedRiceIndexRef.current - 1);
  }, [menuOpen, pickerTransitioning, requestFocusedRice, view]);

  const focusNextRice = useCallback(() => {
    if (menuOpen || pickerTransitioning) return;
    if (view === 'preview') {
      playRiceSound('moveDown');
      setPreviewOption((option) => cyclePreviewOption(option, 1));
      return;
    }
    if (view === 'downloading') return;
    requestFocusedRice(requestedRiceIndexRef.current + 1);
  }, [menuOpen, pickerTransitioning, requestFocusedRice, view]);

  const syncRiceScroll = useCallback((offset: number) => {
    const index = clampRiceIndex(Math.round(offset / RICE_ITEM_PITCH), rices.length);
    requestedRiceIndexRef.current = index;
    setRiceScrollOffset(offset);
    setFocusedRiceIndex(index);
  }, [rices.length]);

  const startRiceHold = useCallback((direction: -1 | 1) => {
    if (menuOpen || pickerTransitioning) return;
    if (view === 'picking') setRiceHoldDirection(direction);
  }, [menuOpen, pickerTransitioning, view]);

  const stopRiceHold = useCallback(() => setRiceHoldDirection(0), []);

  useEffect(() => {
    if (view !== 'picking') setRiceHoldDirection(0);
  }, [view]);

  const closeMenu = useCallback(() => {
    if (!menuOpen || menuExiting) return;
    if (menuTransitionTimeoutRef.current !== null) window.clearTimeout(menuTransitionTimeoutRef.current);
    setMenuExiting(true);
    menuTransitionTimeoutRef.current = window.setTimeout(() => {
      setMenuOpen(false);
      setMenuExiting(false);
      setPickerEntering(true);
      menuTransitionTimeoutRef.current = null;
    }, MENU_FADE_MS);
  }, [menuExiting, menuOpen]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== 'Escape') return;
      event.preventDefault();
      if (menuOpen) {
        closeMenu();
        return;
      }
      if (pickerTransitioning) return;
      setMenuItem(MENU_ITEMS[0]);
      setPickerExiting(true);
      if (menuTransitionTimeoutRef.current !== null) window.clearTimeout(menuTransitionTimeoutRef.current);
      menuTransitionTimeoutRef.current = window.setTimeout(() => {
        setMenuOpen(true);
        setPickerExiting(false);
        menuTransitionTimeoutRef.current = null;
      }, MENU_FADE_MS);
    };

    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [closeMenu, menuOpen, pickerTransitioning]);

  useEffect(() => {
    if (!pickerEntering || menuOpen) return;
    const frameId = window.requestAnimationFrame(() => setPickerEntering(false));
    return () => window.cancelAnimationFrame(frameId);
  }, [menuOpen, pickerEntering]);

  useEffect(() => () => {
    if (menuTransitionTimeoutRef.current !== null) window.clearTimeout(menuTransitionTimeoutRef.current);
  }, []);

  useEffect(() => {
    if (view !== 'downloading') return;

    const timeoutId = window.setTimeout(() => {
      setPreviewOption('install');
      setView('preview');
    }, DOWNLOAD_DURATION_MS);

    return () => window.clearTimeout(timeoutId);
  }, [view]);

  const startDownload = useCallback(() => {
    playRiceSound('applyRice');
    setPreviewOption('install');
    setView('downloading');
  }, []);

  const applyFocusedRice = useCallback(() => {
    if (menuOpen || pickerTransitioning) return;
    const { backendRunning, previewOption, selectedRice, view } = latestApplyStateRef.current;
    if (backendRunning || !selectedRice) return;

    if (view === 'picking') {
      startDownload();
      void runBackend({ command: 'preview', name: selectedRice.name });
      return;
    }

    if (view === 'preview') {
      if (previewOption === 'install') {
        if (selectedRice.install_supported) {
          void runBackend({ command: 'try', name: selectedRice.name });
        }
        return;
      }

      if (previewOption === 'leave') {
        playRiceSound('revert');
        setPreviewOption('install');
        void runBackend({ command: 'uninstall' }, () => setView('picking'));
        return;
      }

      if (previewOption === 'dots') {
        window.open(selectedRice.repo, '_blank', 'noopener,noreferrer');
      }
    }
  }, [menuOpen, pickerTransitioning, runBackend, startDownload]);

  const cycleOnBareStage = (e: React.MouseEvent<HTMLDivElement>) => {
    if (menuOpen || pickerTransitioning) return;
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
              <Antenna />
              <RiceCard menuOpen={menuOpen}>
                {menuOpen ? (
                  <div className={`${styles.menuContent} ${menuExiting ? styles.menuContentExiting : ''}`}>
                    <MenuScreen
                      active={menuItem}
                      onActiveChange={setMenuItem}
                      onBack={closeMenu}
                    />
                  </div>
                ) : (
                  <div className={`${styles.pickerContent} ${pickerTransitioning ? styles.pickerContentHidden : ''}`}>
                    <ScreenContent
                      rices={rices}
                      holdDirection={riceHoldDirection}
                      navRequest={riceNavRequest}
                      pressedControls={pressedControls}
                      onScrollOffsetChange={syncRiceScroll}
                      onRiceStepStart={playMoveForTarget}
                    />
                    <PreviewContent
                      themeName="themename"
                      creatorName="creatorname"
                      installSupported={selectedRice?.install_supported ?? true}
                      onApply={applyFocusedRice}
                    />
                    <ClosingCircles active={view === 'downloading'} />
                  </div>
                )}
              </RiceCard>
              <ClosePin />
              <SoundButton />
              <ThemeKnob />
              <ScrollWheel menuItem={menuOpen ? menuItem : null} />
              <PreviewStars active={view === 'preview' && !menuOpen} />
            </div>
          </ScrollProvider>
        </PreviewOptionProvider>
      </ViewProvider>
    </ThemeProvider>
  );
}
