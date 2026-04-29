import { motion, type Transition } from 'framer-motion';
import styles from './InstallSuccessOverlay.module.css';
import MenuDotsSvg from '@/assets/screen/four-dots.svg?react';
import escIcon from '@/assets/screen/esc.svg';
import riceInstalled from '@/assets/install-success/rice-installed.svg';
import revertAnytime from '@/assets/install-success/revert-anytime.svg';
import { StarSvg } from './PreviewStars';

const APPEAR_DELAY_MS = 200;
const SCALE_MS = 350;
const HOLD_MS = 3000;
const REVERT_STAGGER_MS = 500;
const STAR_STAGGER_MS = 100;
const SCALE_EASE = [0.68, -0.6, 0.32, 1.6] as const;
const SCALE_DURATION_MS = SCALE_MS * 2 + HOLD_MS;
const STARS = [
  { left: 155, top: 97 },
  { left: 444, top: 106 },
  { left: 79, top: 212 },
  { left: 364, top: 309 },
] as const;

const MAX_STAGGER_MS = Math.max(REVERT_STAGGER_MS, (STARS.length - 1) * STAR_STAGGER_MS);
const SUCCESS_MS = APPEAR_DELAY_MS + MAX_STAGGER_MS + SCALE_DURATION_MS;
const SCALE_TIMES = [0, SCALE_MS / SCALE_DURATION_MS, (SCALE_MS + HOLD_MS) / SCALE_DURATION_MS, 1];
const SCALE_KEYFRAMES = [0, 1, 1, 0];

export const INSTALL_SUCCESS_MS = SUCCESS_MS;

const scaleTransition = (staggerMs = 0): Transition => ({
  delay: (APPEAR_DELAY_MS + staggerMs) / 1000,
  duration: SCALE_DURATION_MS / 1000,
  times: SCALE_TIMES,
  ease: [SCALE_EASE, 'linear', SCALE_EASE],
});

export function InstallSuccessOverlay() {
  return (
    <div className={styles.wrap} aria-hidden="true">
      <motion.div
        className={styles.menuGroup}
        initial={{ scale: 0 }}
        animate={{ scale: SCALE_KEYFRAMES }}
        transition={scaleTransition()}
      >
        <MenuDotsSvg className={styles.menuIcon} />
        <p className={styles.menuLabel}>Menu</p>
        <img src={escIcon} alt="" className={styles.escIcon} />
      </motion.div>
      <motion.img
        src={revertAnytime}
        alt=""
        className={styles.revertAnytime}
        initial={{ scale: 0 }}
        animate={{ scale: SCALE_KEYFRAMES }}
        transition={scaleTransition(REVERT_STAGGER_MS)}
      />
      <motion.img
        src={riceInstalled}
        alt=""
        className={styles.riceInstalled}
        initial={{ scale: 0 }}
        animate={{ scale: SCALE_KEYFRAMES }}
        transition={scaleTransition()}
      />
      {STARS.map((star, index) => (
        <motion.span
          key={index}
          className={styles.star}
          style={{ left: star.left, top: star.top }}
          initial={{ scale: 0 }}
          animate={{ scale: SCALE_KEYFRAMES }}
          transition={scaleTransition((index + 1) * STAR_STAGGER_MS)}
        >
          <StarSvg />
        </motion.span>
      ))}
    </div>
  );
}
