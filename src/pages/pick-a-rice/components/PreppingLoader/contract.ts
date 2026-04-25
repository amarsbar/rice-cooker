export const PREPPING = 'PREPPING';
export const PILL_SIZE = 51.936;
export const PILL_COUNT = PREPPING.length;

/** Per-pill x offset inside the PREPPING bitmap (Figma 367:11772..11786).
 *  Pills are slightly overlapping — distances between centres are irregular
 *  (43.44, 43.01, 39.77, 43.82, 45.44, 40.58, 43 px). Using these exact
 *  values keeps the logo pixel-faithful to the design. */
export const PILL_OFFSETS = [0, 45.44, 88.45, 128.22, 172.04, 217.48, 258.06, 301.06] as const;

/** Width of the PREPPING text — left edge of first pill to right edge of last. */
export const BITMAP_WIDTH = PILL_OFFSETS[PILL_OFFSETS.length - 1] + PILL_SIZE;
export const BITMAP_HEIGHT = PILL_SIZE;

/** Default stage dimensions — Figma content area (405×229 card minus 9px border). */
export const STAGE_W = 387;
export const STAGE_H = 211;
/** PREPPING text position inside the stage (Figma 367:11772 left/top). */
export const BITMAP_LEFT = 17;
export const BITMAP_TOP = 79;

export const LIME = '#e8ff76';
export const BROWN = '#3F3833';

export const SLICE_RADIUS = 4;
