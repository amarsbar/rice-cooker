import { motion } from 'framer-motion';
import styles from './PreviewInstallIntro.module.css';

const INTRO_MS = 2000;
const SCALE_MS = 350;
const STAGGER_MS = 40;
const SCALE_EASE = [0.68, -0.6, 0.32, 1.6] as const;
const WORDS = ['try', 'it', 'out', '!'] as const;

export const PREVIEW_INSTALL_INTRO_MS = INTRO_MS;

export function PreviewInstallIntro() {
  return (
    <div className={styles.wrap} aria-hidden="true">
      {WORDS.map((word, index) => {
        const delay = (index * STAGGER_MS) / 1000;
        const duration = INTRO_MS / 1000 - delay;
        const scaleTime = SCALE_MS / 1000 / duration;

        return (
          <motion.span
            key={word}
            className={`${styles.pill} ${styles[`pill_${word === '!' ? 'bang' : word}`]}`}
            initial={{ scale: 0 }}
            animate={{ scale: [0, 1, 1, 0] }}
            transition={{
              delay,
              duration,
              times: [0, scaleTime, 1 - scaleTime, 1],
              ease: [SCALE_EASE, 'linear', SCALE_EASE],
            }}
          >
            {word}
          </motion.span>
        );
      })}
    </div>
  );
}
