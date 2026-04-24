import { createContext, useContext, type ReactNode } from 'react';

export type View = 'picking' | 'preview' | 'post-install';

const ViewContext = createContext<View>('picking');

export function useView() {
  return useContext(ViewContext);
}

export function ViewProvider({ view, children }: { view: View; children: ReactNode }) {
  return <ViewContext.Provider value={view}>{children}</ViewContext.Provider>;
}

/** Scroll state for the rice list — consumed by <CreatorBadge> to drive the
 *  bead indicator and cloud rotation. RiceList owns the scroll events;
 *  PickARice stores the state and provides it here. */
export interface ScrollState {
  offset: number;
  index: number;
  total: number;
}

const ScrollContext = createContext<ScrollState>({ offset: 0, index: 0, total: 1 });

export function useScroll() {
  return useContext(ScrollContext);
}

export function ScrollProvider({ value, children }: { value: ScrollState; children: ReactNode }) {
  return <ScrollContext.Provider value={value}>{children}</ScrollContext.Provider>;
}

/** Palette / colour theme. Three fixed variants; the sprout knob (rendered
 *  inside <BottomDrop>) is the picker. t2 is the default, centre-of-knob
 *  theme — tokens defined in `:root` match it. t1 and t3 get applied via
 *  `[data-theme='t1'|'t3']` override blocks on the stage element. */
export type Theme = 't1' | 't2' | 't3';

interface ThemeCtxValue {
  theme: Theme;
  setTheme: (update: Theme | ((prev: Theme) => Theme)) => void;
}

const ThemeContext = createContext<ThemeCtxValue>({
  theme: 't2',
  setTheme: () => {},
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
  card: { left: 40.295, top: 105.295, width: 405, height: 229 },
  greenTab: { left: 445.295, top: 162.295, height: 81 },
  closePin: { left: 454.295, top: 105.295 },
  soundButton: { left: 454.295, top: 179.295 },
  dropShape: { left: 272.295, top: 332.295 },
  creatorBadge: { left: 359.705, top: 231.705 },
} as const;

export const POSITIONS = {
  picking: {
    card: { left: 0, top: 0, width: 500, height: 440 },
    greenTab: { left: 500, top: 270, height: 90 },
    closePin: { left: 508, top: 214 },
    soundButton: { left: 507, top: 287 },
    dropShape: { left: 317, top: 437 },
    creatorBadge: { left: 400, top: 337 },
  },
  preview: SHRUNKEN,
  'post-install': SHRUNKEN,
} as const;

export const MORPH_TRANSITION = { duration: 0.3, ease: [0.4, 0.0, 0.2, 1] as const } as const;
export const SCREEN_FADE_TRANSITION = { duration: 0.2 } as const;

export const SHRUNKEN_TEXT_VARIANTS = {
  visible: { opacity: 1, transition: { duration: 0.12 } },
  hidden: { opacity: 0, transition: { duration: 0.1 } },
} as const;
