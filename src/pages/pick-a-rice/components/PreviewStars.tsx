import { motion } from 'framer-motion';
import { useMemo } from 'react';
import styles from './PreviewStars.module.css';

interface PreviewStarsProps {
  active: boolean;
}

interface StarSpec {
  left: number;
  top: number;
  size: number;
}

const FIGMA_TO_STAGE = { x: 2.5, y: 12 };
const BURST_CENTER = { x: 288.5, y: 220.5 };
const TRANSLATE_START_PROGRESS = 0.6;
const TRANSLATE_DURATION = 2.5;
const SCALE_UP_DURATION = 0.35;
const SCALE_OUT_START = 1.5;
const SCALE_OUT_END = 3.2;
const START_STAGGER_MAX = 0.2;
const EXIT_STAGGER_MAX = 1;
const ENTRY_EASE = [0.16, 0.885, 0.25, 1.03] as const;
const LINEAR_EASE = [0, 0, 1, 1] as const;
const EXIT_EASE = [0.22, 1, 0.36, 1] as const;

const STARS: readonly StarSpec[] = [
  { left: 140, top: 4, size: 7 },
  { left: 522, top: 15, size: 7 },
  { left: 345, top: 4, size: 7 },
  { left: 230, top: 34, size: 15 },
  { left: 438, top: 34, size: 15 },
  { left: 55, top: 185, size: 7 },
  { left: 124, top: 63, size: 7 },
  { left: 30, top: 63, size: 15 },
  { left: 354, top: 413, size: 7 },
  { left: 582, top: 90, size: 7 },
  { left: 255, top: 364, size: 15 },
  { left: 161, top: 406, size: 7 },
  { left: 19, top: 273, size: 7 },
  { left: 67, top: 369, size: 10 },
  { left: 582, top: 200, size: 15 },
];

export function PreviewStars({ active }: PreviewStarsProps) {
  const delays = useMemo(() => {
    return Array.from({ length: STARS.length }, () => ({
      enter: Math.random() * START_STAGGER_MAX,
      exit: Math.random() * EXIT_STAGGER_MAX,
    }));
  }, [active]);

  if (!active) return null;

  return (
    <div className={styles.layer} aria-hidden="true">
      {STARS.map((star, index) => {
        const delay = delays[index]!;
        const translateDuration = TRANSLATE_DURATION;
        const startX = BURST_CENTER.x - (star.left + star.size / 2);
        const startY = BURST_CENTER.y - (star.top + star.size / 2);
        const translateX = startX * (1 - TRANSLATE_START_PROGRESS);
        const translateY = startY * (1 - TRANSLATE_START_PROGRESS);
        return (
          <motion.span
            key={`${star.left}-${star.top}-${index}`}
            className={styles.star}
            style={{
              left: star.left + FIGMA_TO_STAGE.x,
              top: star.top + FIGMA_TO_STAGE.y,
              width: star.size,
              height: star.size,
            }}
            initial={{ x: translateX, y: translateY }}
            animate={{
              x: [translateX, 0, 0],
              y: [translateY, 0, 0],
            }}
            transition={{
              x: {
                delay: delay.enter,
                duration: SCALE_OUT_END,
                times: [0, translateDuration / SCALE_OUT_END, 1],
                ease: [ENTRY_EASE, LINEAR_EASE],
              },
              y: {
                delay: delay.enter,
                duration: SCALE_OUT_END,
                times: [0, translateDuration / SCALE_OUT_END, 1],
                ease: [ENTRY_EASE, LINEAR_EASE],
              },
            }}
          >
            <motion.span
              className={styles.scaleIn}
              initial={{ scale: 0 }}
              animate={{ scale: 1 }}
              transition={{
                delay: delay.enter,
                duration: SCALE_UP_DURATION,
                ease: ENTRY_EASE,
              }}
            >
              <motion.span
                className={styles.scaleOut}
                initial={{ scale: 1 }}
                animate={{ scale: 0 }}
                transition={{
                  delay: SCALE_OUT_START + delay.exit,
                  duration: SCALE_OUT_END - SCALE_OUT_START,
                  ease: EXIT_EASE,
                }}
              >
                <StarSvg />
              </motion.span>
            </motion.span>
          </motion.span>
        );
      })}
    </div>
  );
}

function StarSvg() {
  return (
    <svg viewBox="0 0 15 15" fill="none" xmlns="http://www.w3.org/2000/svg">
      <path
        d="M15 7.49901C15 9.09228 13.3214 10.3835 11.2504 10.3832C10.9186 10.3831 10.5969 10.3486 10.2904 10.2862C10.3532 10.5937 10.3864 10.9167 10.3865 11.2498C10.3868 13.3212 9.09511 15.0002 7.50198 15C5.90916 14.9993 4.61751 13.3201 4.61719 11.2489C4.61716 10.9176 4.65097 10.5964 4.71296 10.2905C4.40583 10.353 4.08316 10.3872 3.75044 10.3871C1.67949 10.3867 0.000241152 9.09432 0 7.50099C5.63119e-06 5.90782 1.67877 4.61663 3.74956 4.61684C4.08164 4.61689 4.4038 4.65053 4.71042 4.71292C4.6478 4.40569 4.61356 4.083 4.61351 3.7502C4.61324 1.67893 5.90502 -8.56104e-05 7.49802 3.2739e-09C9.09098 0.00043918 10.3825 1.67983 10.3828 3.75107C10.3829 4.08204 10.3497 4.40298 10.2879 4.70867C10.5947 4.64626 10.9172 4.61285 11.2496 4.61288C13.3206 4.6132 14.9997 5.90565 15 7.49901Z"
        fill="currentColor"
      />
    </svg>
  );
}
