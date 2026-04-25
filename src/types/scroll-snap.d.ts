/** scroll-snap@5.0.2 ships `dist/index.d.ts` with internal `declare module "index"`
 *  names that TS can't treat as a real module (TS2306). tsconfig paths redirects
 *  the `scroll-snap` import to this shim so the runtime import still resolves
 *  from node_modules via Vite. */
export interface ScrollSnapSettings {
  snapDestinationX?: string | number;
  snapDestinationY?: string | number;
  timeout?: number;
  duration?: number;
  threshold?: number;
  snapStop?: boolean;
  easing?: (t: number) => number;
  showArrows?: boolean;
  enableKeyboard?: boolean;
}

export default function createScrollSnap(
  element: HTMLElement,
  settings?: ScrollSnapSettings,
  callback?: () => void,
): { bind: () => void; unbind: () => void };
