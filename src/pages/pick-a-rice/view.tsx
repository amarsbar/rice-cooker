import { createContext, useContext, type ReactNode } from 'react';

export type View = 'picking' | 'preview';

const ViewContext = createContext<View>('picking');

export function useView() {
  return useContext(ViewContext);
}

export function ViewProvider({ view, children }: { view: View; children: ReactNode }) {
  return <ViewContext.Provider value={view}>{children}</ViewContext.Provider>;
}

/** Target position/size for every moving element in each view state.
 *  All coordinates are pixel offsets within the 614.4 × 597.4 stage. */
export const POSITIONS = {
  picking: {
    card: { left: 0, top: 0, width: 500, height: 500 },
    greenTab: { left: 500, top: 328 },
    soundRing: { left: 509, top: 345 },
    soundInner: { left: 510.31, top: 346.31 },
    closeIcon: { left: 509, top: 271 },
    dropShape: { left: 327, top: 498 },
    dropInner: { left: 359, top: 504 },
    creatorCloud: { left: 414.406, top: 397.406 },
    creatorInner: { left: 426, top: 409 },
  },
  preview: {
    card: { left: 36, top: 136, width: 416, height: 229 },
    greenTab: { left: 452, top: 193 },
    soundRing: { left: 461, top: 210 },
    soundInner: { left: 462.31, top: 211.31 },
    closeIcon: { left: 461, top: 136 },
    dropShape: { left: 279, top: 363 },
    dropInner: { left: 311, top: 369 },
    creatorCloud: { left: 366.406, top: 262.406 },
    creatorInner: { left: 378, top: 274 },
  },
} as const;

/** Eased transition for the window morph (card size + position, external elements). */
export const MORPH_TRANSITION = { duration: 0.5, ease: [0.4, 0.0, 0.2, 1] } as const;

/** Screen-content crossfade. Same duration as morph so they finish together. */
export const SCREEN_FADE_TRANSITION = { duration: 0.5 } as const;

/** Preview-mode text fade. 150ms duration, delayed until after the morph
 *  completes in the picking → preview direction. */
export const PREVIEW_TEXT_VARIANTS = {
  visible: { opacity: 1, transition: { duration: 0.15, delay: 0.5 } },
  hidden: { opacity: 0, transition: { duration: 0.15 } },
} as const;

/** Same timing as PREVIEW_TEXT_VARIANTS but peaks at 0.4 for the dimmed
 *  "Preview state" label in the creator bubble (Figma opacity-40 text). */
export const PREVIEW_DIM_TEXT_VARIANTS = {
  visible: { opacity: 0.4, transition: { duration: 0.15, delay: 0.5 } },
  hidden: { opacity: 0, transition: { duration: 0.15 } },
} as const;
