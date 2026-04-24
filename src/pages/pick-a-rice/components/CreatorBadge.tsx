import { useEffect, useState } from 'react';
import { motion } from 'framer-motion';
import styles from './CreatorBadge.module.css';
import cloudSvg from '@/assets/figma/creator-cloud.svg';
import cloudAltSvg from '@/assets/figma/creator-cloud-post.svg';
import decorSvg from '@/assets/figma/creator-decor.svg';
import {
  MORPH_TRANSITION,
  POSITIONS,
  SCREEN_FADE_TRANSITION,
  SHRUNKEN_TEXT_VARIANTS,
  useScroll,
  useView,
} from '../view';

type DirKey = 'up' | 'left' | 'down' | 'right';

/** Bead slot geometry — offset from active-pill center + visible size. */
const SLOTS: Record<number, { dx: number; dy: number; size: number }> = {
  [-3]: { dx: -51.19, dy: 14.64, size: 6.873 },
  [-2]: { dx: -40.29, dy: 7.54, size: 9.612 },
  [-1]: { dx: -24.45, dy: 1.4, size: 14.564 },
  [0]: { dx: 0, dy: 0, size: 14.564 },
  [1]: { dx: 24.45, dy: 1.4, size: 14.564 },
  [2]: { dx: 40.29, dy: 7.54, size: 9.612 },
  [3]: { dx: 51.19, dy: 14.64, size: 6.873 },
};
const PILL_W = 25;
const PILL_H = 23.12;
const CREAM_CENTER = 88;
const PILL_Y_BASE = 17.56; // pill center y when leftCount = 0 (matches Figma rice 1)
const PILL_Y_STEP = 5;
const CLOUD_ROTATE_PER_PX = 0.5;

export function CreatorBadge() {
  const view = useView();
  const scroll = useScroll();
  const isPicking = view === 'picking';
  const pressed = usePressedDirection();

  return (
    <motion.div
      className={styles.badge}
      initial={false}
      animate={POSITIONS[view].creatorBadge}
      transition={MORPH_TRANSITION}
    >
      <motion.img
        src={cloudSvg}
        alt=""
        className={styles.cloud}
        initial={false}
        animate={{ opacity: isPicking ? 1 : 0 }}
        style={{ rotate: scroll.offset * CLOUD_ROTATE_PER_PX }}
        transition={SCREEN_FADE_TRANSITION}
      />
      <motion.img
        src={cloudAltSvg}
        alt=""
        className={styles.cloud}
        initial={false}
        animate={{ opacity: isPicking ? 0 : 1 }}
        transition={SCREEN_FADE_TRANSITION}
      />

      <div className={styles.inner}>
        <div className={styles.decor}>
          <div className={styles.decorBleed}>
            <img src={decorSvg} alt="" />
          </div>
        </div>

        <motion.div
          className={styles.content}
          initial={false}
          animate={{ opacity: view === 'picking' ? 1 : 0 }}
          transition={SCREEN_FADE_TRANSITION}
        >
          <BeadIndicator />
          <p className={`${styles.tag} ${styles.tagTop}`}>niri</p>
          <p className={`${styles.tag} ${styles.tagBottom}`}>dms</p>
          <p className={`${styles.tag} ${styles.tagPlus}`}>+</p>
          <KeyIndicator pressed={pressed} />
        </motion.div>

        <motion.div
          className={styles.content}
          initial={false}
          animate={view === 'preview' ? 'visible' : 'hidden'}
          variants={SHRUNKEN_TEXT_VARIANTS}
        >
          <p className={styles.previewing}>previewing</p>
        </motion.div>

        <motion.div
          className={styles.content}
          initial={false}
          animate={view === 'post-install' ? 'visible' : 'hidden'}
          variants={SHRUNKEN_TEXT_VARIANTS}
        >
          <p className={`${styles.installText} ${styles.installRice}`}>rice</p>
          <p className={`${styles.installText} ${styles.installInstalled}`}>installed</p>
          <p className={`${styles.installText} ${styles.installBang}`}>!</p>
        </motion.div>
      </div>
    </motion.div>
  );
}

function BeadIndicator() {
  const { index, total } = useScroll();
  const leftCount = Math.min(index, 3);
  const pillY = PILL_Y_BASE + leftCount * PILL_Y_STEP;
  return (
    <>
      {Array.from({ length: total }, (_, i) => (
        <Bead key={i} num={i + 1} slot={i - index} pillY={pillY} />
      ))}
    </>
  );
}

function Bead({ num, slot, pillY }: { num: number; slot: number; pillY: number }) {
  const clamped = Math.max(-3, Math.min(3, slot));
  const s = SLOTS[clamped]!;
  const active = slot === 0;
  const visible = Math.abs(slot) <= 3;
  const w = active ? PILL_W : s.size;
  const h = active ? PILL_H : s.size;
  const cx = CREAM_CENTER + s.dx;
  const cy = pillY + s.dy;
  return (
    <motion.div
      className={styles.bead}
      initial={false}
      animate={{
        left: cx - w / 2,
        top: cy - h / 2,
        width: w,
        height: h,
        scale: visible ? 1 : 0,
      }}
      transition={{ duration: 0.3, ease: 'easeOut' }}
    >
      <span className={styles.beadNum} style={{ opacity: active ? 1 : 0 }}>
        {num}
      </span>
    </motion.div>
  );
}

function KeyIndicator({ pressed }: { pressed: DirKey | null }) {
  const cls = (dir: DirKey) =>
    `${styles.key} ${styles[`key_${dir}`]} ${pressed === dir ? styles.keyPressed : ''}`;
  return (
    <>
      <span className={cls('up')} />
      <span className={cls('down')} />
      <span className={cls('right')} />
      <span className={cls('left')} />
    </>
  );
}

const KEY_MAP: Record<string, DirKey> = {
  w: 'up', W: 'up', ArrowUp: 'up',
  a: 'left', A: 'left', ArrowLeft: 'left',
  s: 'down', S: 'down', ArrowDown: 'down',
  d: 'right', D: 'right', ArrowRight: 'right',
};

function usePressedDirection(): DirKey | null {
  const [pressed, setPressed] = useState<DirKey | null>(null);
  useEffect(() => {
    const down = (e: KeyboardEvent) => {
      const dir = KEY_MAP[e.key];
      if (dir) setPressed(dir);
    };
    const up = (e: KeyboardEvent) => {
      const dir = KEY_MAP[e.key];
      if (dir) setPressed((p) => (p === dir ? null : p));
    };
    window.addEventListener('keydown', down);
    window.addEventListener('keyup', up);
    return () => {
      window.removeEventListener('keydown', down);
      window.removeEventListener('keyup', up);
    };
  }, []);
  return pressed;
}
