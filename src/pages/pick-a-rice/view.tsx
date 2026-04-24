import { createContext, useContext, type ReactNode } from 'react';

export type View = 'picking' | 'preview' | 'post-install';

const ViewContext = createContext<View>('picking');

export function useView() {
  return useContext(ViewContext);
}

export function ViewProvider({ view, children }: { view: View; children: ReactNode }) {
  return <ViewContext.Provider value={view}>{children}</ViewContext.Provider>;
}

/** Target position/size for every moving element in each view state. All
 *  coordinates are pixel offsets in the 600 × 537 stage. Preview and
 *  post-install share the "shrunken" layout — the card morphs once from
 *  picking → preview and stays put going preview → post-install. */
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

/** Eased transition for the window morph (card size + position, external elements). */
export const MORPH_TRANSITION = { duration: 0.5, ease: [0.4, 0.0, 0.2, 1] as const } as const;

/** Screen-content crossfade. Same duration as morph so they finish together. */
export const SCREEN_FADE_TRANSITION = { duration: 0.5 } as const;

/** Delayed text-fade variants — used for preview/post-install content that
 *  should pop in AFTER the card morph finishes (picking → preview/post).
 *  Going the other way (preview/post → picking), text fades out fast. */
export const SHRUNKEN_TEXT_VARIANTS = {
  visible: { opacity: 1, transition: { duration: 0.15, delay: 0.5 } },
  hidden: { opacity: 0, transition: { duration: 0.15 } },
} as const;

/** Same timing as above but without the delay, for content that should
 *  crossfade together with the morph (e.g. the CreatorBadge cloud shade). */
export const CONTENT_FADE_VARIANTS = {
  visible: { opacity: 1, transition: { duration: 0.3 } },
  hidden: { opacity: 0, transition: { duration: 0.3 } },
} as const;
