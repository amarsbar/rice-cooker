import { createContext, useContext, type ReactNode } from 'react';

export type View = 'picking' | 'preview';
export const PREVIEW_OPTIONS = ['leave', 'install', 'dots'] as const;
export type PreviewOption = (typeof PREVIEW_OPTIONS)[number];

const ViewContext = createContext<View>('picking');

export function useView() {
  return useContext(ViewContext);
}

export function ViewProvider({ view, children }: { view: View; children: ReactNode }) {
  return <ViewContext.Provider value={view}>{children}</ViewContext.Provider>;
}

/** Focus-derived rice position consumed by <ScrollWheel>. */
export interface ScrollState {
  offset: number;
  index: number;
  total: number;
}

export const RICE_ITEM_COUNT = 10;
export const RICE_ITEM_PITCH = 292;

const ScrollContext = createContext<ScrollState>({ offset: 0, index: 0, total: 1 });

export function useScroll() {
  return useContext(ScrollContext);
}

export function ScrollProvider({ value, children }: { value: ScrollState; children: ReactNode }) {
  return <ScrollContext.Provider value={value}>{children}</ScrollContext.Provider>;
}

const PreviewOptionContext = createContext<PreviewOption>('install');

export function usePreviewOption() {
  return useContext(PreviewOptionContext);
}

export function PreviewOptionProvider({
  value,
  children,
}: {
  value: PreviewOption;
  children: ReactNode;
}) {
  return <PreviewOptionContext.Provider value={value}>{children}</PreviewOptionContext.Provider>;
}

/** Palette / colour theme. Three fixed variants; <ThemeKnob> is the picker.
 *  t2 is the default, centre-of-knob
 *  theme — tokens defined in `:root` match it. t1 and t3 get applied via
 *  `[data-theme='t1'|'t3']` override blocks on the stage element.
 *
 *  The cycle goes centre → top → centre → bottom → centre → …, so t2 is
 *  visited between every non-default theme. That requires tracking a
 *  4-step cycle index rather than the theme alone (otherwise t2 can't
 *  remember which direction it came from). Consumers see the derived
 *  `theme` + an `advance()` that bumps the index. */
export type Theme = 't1' | 't2' | 't3';

export const THEME_CYCLE: readonly Theme[] = ['t2', 't1', 't2', 't3'] as const;

interface ThemeCtxValue {
  theme: Theme;
  advance: () => void;
}

const ThemeContext = createContext<ThemeCtxValue>({
  theme: 't2',
  advance: () => {},
});

export function useTheme() {
  return useContext(ThemeContext);
}

export function ThemeProvider({
  value,
  children,
}: {
  value: ThemeCtxValue;
  children: ReactNode;
}) {
  return <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>;
}

/** Target position/size for every moving element in each view state. */
const SHRUNKEN = {
  card: { left: 88.5, top: 118, width: 405, height: 229 },
  greenTab: { left: 493.5, top: 175, height: 81 },
  closePin: { left: 502.5, top: 118 },
  soundButton: { left: 502.5, top: 192 },
  themeKnob: { left: 320.5, top: 345 },
  scrollWheel: { left: 408, top: 244 },
} as const;

export const POSITIONS = {
  picking: {
    card: { left: 16, top: 33, width: 550, height: 399 },
    greenTab: { left: 564, top: 307, height: 90 },
    closePin: { left: 574, top: 251 },
    soundButton: { left: 571, top: 323 },
    themeKnob: { left: 386, top: 432 },
    scrollWheel: { left: 466, top: 374 },
  },
  preview: SHRUNKEN,
} as const;

export const MORPH_TRANSITION = { duration: 0.3, ease: [0.4, 0.0, 0.2, 1] as const } as const;
export const SCREEN_FADE_TRANSITION = { duration: 0.2 } as const;

export const SHRUNKEN_TEXT_VARIANTS = {
  visible: { opacity: 1, transition: { duration: 0.12 } },
  hidden: { opacity: 0, transition: { duration: 0.1 } },
} as const;
