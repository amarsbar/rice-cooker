import styles from './DashedFrame.module.css';

/** Figma node 168:7306 — 1px yellow dashed rounded rectangle, 462 × 259, at
 *  (12, 54) within the card. Rendered as inline SVG so the dash pattern is
 *  independent of the browser's DPR-dependent default dashed-border pattern. */
export function DashedFrame() {
  return (
    <svg
      className={styles.frame}
      width="462"
      height="259"
      viewBox="0 0 462 259"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
      aria-hidden="true"
    >
      <rect
        x="0.5"
        y="0.5"
        width="461"
        height="258"
        rx="11.5"
        ry="11.5"
        stroke="#ffc501"
        strokeWidth="1"
        strokeDasharray="4 4"
      />
    </svg>
  );
}
