import { createContext, useContext, type ReactNode } from 'react';

export type View = 'picking' | 'preview' | 'post-install';

const ViewContext = createContext<View>('picking');

export function useView() {
  return useContext(ViewContext);
}

export function ViewProvider({ view, children }: { view: View; children: ReactNode }) {
  return <ViewContext.Provider value={view}>{children}</ViewContext.Provider>;
}

/** Target coords (stage-relative) for every element that moves. Preview and
 *  post-install share the "shrunken" layout — the card morphs once from
 *  picking → preview and stays put going preview → post-install. Shrunken
 *  values were derived by centering Figma node 350:7158 (the content box,
 *  519.41 × 326.41) inside the 600 × 537 stage and adding each element's
 *  offset from its Figma parent. */
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

/** Shared morph timing — card + external elements + creator badge all use
 *  these so the transition stays visually synchronized. */
export const MORPH_TRANSITION = 'all 0.5s cubic-bezier(0.4, 0, 0.2, 1)';

/** Content inside the card (picking header/preview vs preview vs
 *  post-install content) crossfades at half the morph duration to keep the
 *  two layers from overlapping for long. */
export const CONTENT_FADE_TRANSITION = 'opacity 0.25s ease-out';
