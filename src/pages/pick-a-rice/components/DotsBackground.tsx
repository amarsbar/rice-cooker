import { motion } from 'framer-motion';
import styles from './DotsBackground.module.css';
import { useView } from '../view';

/* Figma node 168:6834 — elliptical mask (735 × 544) with a grid of yellow
   dots (618 × 474) inside. Grid values extracted from the Figma asset:
   35 cols × 19 rows, anisotropic spacing (18 px horizontal, 26 px vertical),
   radius 3. The original Figma uses an alpha mask with a radial gradient to
   dim dots toward the edges; we collapse that into a per-dot opacity
   computed from normalized elliptical distance to the ellipse center.

   Cascade is radial: dots are bucketed into concentric rings by their
   normalized distance, and each ring fades in with a delay proportional to
   its distance from the center. */

const PATTERN_WIDTH = 735;
const PATTERN_HEIGHT = 544;
const COLS = 35;
const ROWS = 19;
const DOT_RADIUS = 3;
const COL_START = 68;
const ROW_START = 38;
const COL_SPACING = 18;
const ROW_SPACING = 26;
const ELLIPSE_CX = PATTERN_WIDTH / 2;
const ELLIPSE_CY = PATTERN_HEIGHT / 2;
const ELLIPSE_RX = PATTERN_WIDTH / 2;
const ELLIPSE_RY = PATTERN_HEIGHT / 2;
const MAX_OPACITY = 0.7;

/** Number of concentric rings (buckets by normalized elliptical distance).
 *  Higher = smoother cascade, but with 665 dots in total the differences are
 *  minor above ~15. */
const NUM_RINGS = 15;

/** Starts 150ms before the card finishes morphing. The card's shrinking
 *  silhouette covers the central dots until it clears, so visually the
 *  cascade appears to begin right as the morph ends. */
const CASCADE_START_DELAY = 0.2;
const RING_STAGGER = 0.04;
const RING_FADE_DURATION = 0.25;
const RING_HIDE_DURATION = 0.15;

interface Dot {
  x: number;
  y: number;
  opacity: number;
}

interface Ring {
  ringIdx: number;
  dots: Dot[];
}

const RINGS: Ring[] = (() => {
  const buckets: Dot[][] = Array.from({ length: NUM_RINGS }, () => []);
  for (let rowIdx = 0; rowIdx < ROWS; rowIdx++) {
    const y = ROW_START + rowIdx * ROW_SPACING;
    for (let colIdx = 0; colIdx < COLS; colIdx++) {
      const x = COL_START + colIdx * COL_SPACING;
      const dx = (x - ELLIPSE_CX) / ELLIPSE_RX;
      const dy = (y - ELLIPSE_CY) / ELLIPSE_RY;
      const d = Math.sqrt(dx * dx + dy * dy);
      if (d >= 1) continue;
      const ringIdx = Math.min(Math.floor(d * NUM_RINGS), NUM_RINGS - 1);
      buckets[ringIdx].push({ x, y, opacity: (1 - d) * MAX_OPACITY });
    }
  }
  return buckets.map((dots, ringIdx) => ({ ringIdx, dots }));
})();

const ringVariants = {
  hidden: { opacity: 0, transition: { duration: RING_HIDE_DURATION } },
  visible: (ringIdx: number) => ({
    opacity: 1,
    transition: {
      duration: RING_FADE_DURATION,
      delay: CASCADE_START_DELAY + ringIdx * RING_STAGGER,
      ease: 'easeOut' as const,
    },
  }),
};

export function DotsBackground() {
  const view = useView();
  const target = view === 'preview' ? 'visible' : 'hidden';
  return (
    <svg
      className={styles.dots}
      width={PATTERN_WIDTH}
      height={PATTERN_HEIGHT}
      viewBox={`0 0 ${PATTERN_WIDTH} ${PATTERN_HEIGHT}`}
      xmlns="http://www.w3.org/2000/svg"
      aria-hidden="true"
    >
      {RINGS.map(({ ringIdx, dots }) =>
        dots.length === 0 ? null : (
          <motion.g
            key={ringIdx}
            custom={ringIdx}
            initial={false}
            animate={target}
            variants={ringVariants}
          >
            {dots.map((dot, i) => (
              <circle
                key={i}
                cx={dot.x}
                cy={dot.y}
                r={DOT_RADIUS}
                fill="#ffc501"
                opacity={dot.opacity}
              />
            ))}
          </motion.g>
        ),
      )}
    </svg>
  );
}
