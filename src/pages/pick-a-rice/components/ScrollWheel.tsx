import { useEffect, useState } from 'react';
import { motion } from 'framer-motion';
import styles from './ScrollWheel.module.css';
import { OutlinedText } from './OutlinedText';
import RimSvg from '@/assets/scroll-wheel/rim.svg?react';
import GridSvg from '@/assets/scroll-wheel/grid.svg?react';
import installRiceSvg from '@/assets/scroll-wheel/install-rice.svg';
import dotfilesSvg from '@/assets/scroll-wheel/dotfiles.svg';
import tryAnotherSvg from '@/assets/scroll-wheel/try-another.svg';
import enterMessageSvg from '@/assets/boot/enter-message.svg';
import closeMessageSvg from '@/assets/boot/close-message.svg';
import githubMessageSvg from '@/assets/boot/github-message.svg';
import {
  MORPH_TRANSITION,
  POSITIONS,
  PREVIEW_OPTIONS,
  RICE_ITEM_PITCH,
  type PreviewOption,
  type ScrollState,
  SCREEN_FADE_TRANSITION,
  SHRUNKEN_TEXT_VARIANTS,
  usePreviewOption,
  useScroll,
  useView,
} from '../view';
import { MENU_ITEMS, type MenuItem } from '../menuOptions';
import type { BootItem } from './BootScreen';

const MotionRim = motion.create(RimSvg);

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
const PILL_SIZE = 25;
const CREAM_CENTER = 88;
/** The arc as a whole stays at a fixed y — individual beads still trace
 *  the arc's vertical contour (SLOTS[slot].dy) as they change slots.
 *  Figma nudges the whole arc down by 15px for indexes 4+; STEP = 0
 *  locks that drift out because it read as distracting during scroll.
 *  Re-enable with STEP > 0 to get the Figma-exact offset back. */
const PILL_Y_BASE = 18.5;
const PILL_Y_STEP = 0;
const RIM_ROTATE_PER_PX = 0.1;
const BOOT_WHEEL_ITEMS: BootItem[] = ['enter', 'close', 'github'];
const BOOT_DOT_OPTION: Record<BootItem, PreviewOption> = {
  enter: 'leave',
  close: 'install',
  github: 'dots',
};

export function ScrollWheel({
  menuItem = null,
  bootItem = null,
}: {
  menuItem?: MenuItem | null;
  bootItem?: BootItem | null;
}) {
  const view = useView();
  const previewOption = usePreviewOption();
  const scroll = useScroll();
  const pressed = usePressedDirections();
  const menuIndex = menuItem ? MENU_ITEMS.indexOf(menuItem) : -1;
  const bootIndex = bootItem ? BOOT_WHEEL_ITEMS.indexOf(bootItem) : -1;
  const menuActive = menuIndex >= 0;
  const bootActive = bootIndex >= 0;
  const activeScroll = menuActive
    ? { offset: menuIndex * RICE_ITEM_PITCH, index: menuIndex, total: MENU_ITEMS.length }
    : bootActive
      ? { offset: bootIndex * RICE_ITEM_PITCH, index: bootIndex, total: BOOT_WHEEL_ITEMS.length }
    : scroll;
  const controlledWheel = menuActive || bootActive;
  const pickingContentVisible = menuActive || (!bootActive && view === 'picking');
  const bootContentVisible = bootActive;
  const previewContentVisible = !controlledWheel && view === 'preview';
  const dotsVisible = previewContentVisible || bootContentVisible;
  const dotOption = bootItem ? BOOT_DOT_OPTION[bootItem] : previewOption;

  return (
    <motion.div
      className={styles.root}
      initial={false}
      animate={POSITIONS[view].scrollWheel}
      transition={MORPH_TRANSITION}
    >
      <MotionRim
        className={styles.rim}
        style={controlledWheel ? undefined : { rotate: activeScroll.offset * RIM_ROTATE_PER_PX }}
        animate={controlledWheel ? { rotate: activeScroll.offset * RIM_ROTATE_PER_PX } : undefined}
        transition={MORPH_TRANSITION}
      />

      <div className={styles.inner}>
        <div className={styles.grid}>
          <div className={styles.gridBleed}>
            <GridSvg />
          </div>
        </div>

        <motion.div
          className={styles.content}
          initial={false}
          animate={{ opacity: pickingContentVisible ? 1 : 0 }}
          transition={SCREEN_FADE_TRANSITION}
        >
          <BeadIndicator scroll={activeScroll} />
          <OutlinedText className={`${styles.tag} ${styles.tagTop}`}>niri</OutlinedText>
          <OutlinedText className={`${styles.tag} ${styles.tagBottom}`}>dms</OutlinedText>
          <OutlinedText className={`${styles.tag} ${styles.tagPlus}`}>+</OutlinedText>
          <KeyIndicator pressed={pressed} />
        </motion.div>

        <motion.div
          className={styles.content}
          initial={false}
          animate={previewContentVisible ? 'visible' : 'hidden'}
          variants={SHRUNKEN_TEXT_VARIANTS}
        >
          <PreviewWheelGraphic option={previewOption} />
        </motion.div>

        <motion.div
          className={styles.content}
          initial={false}
          animate={bootContentVisible ? 'visible' : 'hidden'}
          variants={SHRUNKEN_TEXT_VARIANTS}
        >
          {bootItem && <BootWheelGraphic item={bootItem} />}
        </motion.div>
      </div>

      <motion.div
        className={styles.previewDots}
        initial={false}
        animate={dotsVisible ? 'visible' : 'hidden'}
        variants={SHRUNKEN_TEXT_VARIANTS}
      >
        <PreviewDots option={dotOption} />
      </motion.div>
    </motion.div>
  );
}

function BootWheelGraphic({ item }: { item: BootItem }) {
  const cls = `${styles.bootGraphic} ${styles[`bootGraphic_${item}`]}`;
  const src = item === 'enter' ? enterMessageSvg : item === 'github' ? githubMessageSvg : closeMessageSvg;
  return <img className={cls} src={src} alt="" aria-hidden="true" />;
}

function PreviewWheelGraphic({ option }: { option: PreviewOption }) {
  const cls = `${styles.previewGraphic} ${styles[`previewGraphic_${option}`]}`;
  const src =
    option === 'leave' ? tryAnotherSvg : option === 'dots' ? dotfilesSvg : installRiceSvg;
  return <img className={cls} src={src} alt="" aria-hidden="true" />;
}

const PREVIEW_DOTS: Record<
  PreviewOption,
  Record<PreviewOption, { x: number; y: number; size: number }>
> = {
  leave: {
    leave: { x: 99.59, y: 16.59, size: 20 },
    install: { x: 122.97, y: 23.59, size: 10.754 },
    dots: { x: 138.3, y: 29.59, size: 9.414 },
  },
  install: {
    leave: { x: 76.59, y: 27.59, size: 10 },
    install: { x: 100.59, y: 19.59, size: 20 },
    dots: { x: 125.05, y: 27.16, size: 10.754 },
  },
  dots: {
    leave: { x: 64.09, y: 30.59, size: 7 },
    install: { x: 80.09, y: 23.59, size: 11 },
    dots: { x: 103.59, y: 16.59, size: 22 },
  },
};

function PreviewDots({ option }: { option: PreviewOption }) {
  return (
    <>
      {PREVIEW_OPTIONS.map((id) => {
        const dot = PREVIEW_DOTS[option][id];
        return (
          <motion.span
            key={id}
            className={styles.previewDot}
            animate={{
              left: dot.x - dot.size / 2,
              top: dot.y,
              width: dot.size,
              height: dot.size,
            }}
            transition={MORPH_TRANSITION}
          />
        );
      })}
    </>
  );
}

function BeadIndicator({ scroll }: { scroll: ScrollState }) {
  const { index, total } = scroll;
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
  const targetScale = visible ? (active ? 1 : s.size / PILL_SIZE) : 0;
  const cx = CREAM_CENTER + s.dx;
  const cy = pillY + s.dy;
  return (
    <motion.div
      className={styles.bead}
      initial={false}
      animate={{
        left: cx - PILL_SIZE / 2,
        top: cy - PILL_SIZE / 2,
        scale: targetScale,
      }}
      transition={{ duration: 0.3, ease: 'easeOut' }}
    >
      <span className={styles.beadNum} style={{ opacity: active ? 1 : 0 }}>
        {num}
      </span>
    </motion.div>
  );
}

function KeyIndicator({ pressed }: { pressed: Set<DirKey> }) {
  const cls = (dir: DirKey) =>
    `${styles.key} ${styles[`key_${dir}`]} ${pressed.has(dir) ? styles.keyPressed : ''}`;
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
  w: 'up', ArrowUp: 'up',
  a: 'left', ArrowLeft: 'left',
  s: 'down', ArrowDown: 'down',
  d: 'right', ArrowRight: 'right',
};

const keyToDir = (e: KeyboardEvent): DirKey | null =>
  KEY_MAP[e.key] ?? KEY_MAP[e.key.toLowerCase()] ?? null;

/** Tracks all held WASD/arrow keys so multiple beads can light up at
 *  once (e.g. W + A for an up-left diagonal). window.blur clears the
 *  set so Alt-Tab with a key held doesn't pin a bead on. */
function usePressedDirections(): Set<DirKey> {
  const [held, setHeld] = useState<Set<DirKey>>(new Set());
  useEffect(() => {
    const mut = (add: boolean) => (e: KeyboardEvent) => {
      const dir = keyToDir(e);
      if (!dir) return;
      setHeld((s) => {
        if (s.has(dir) === add) return s;
        const next = new Set(s);
        if (add) next.add(dir);
        else next.delete(dir);
        return next;
      });
    };
    const onDown = mut(true);
    const onUp = mut(false);
    const clear = () => setHeld((s) => (s.size ? new Set() : s));
    window.addEventListener('keydown', onDown);
    window.addEventListener('keyup', onUp);
    window.addEventListener('blur', clear);
    return () => {
      window.removeEventListener('keydown', onDown);
      window.removeEventListener('keyup', onUp);
      window.removeEventListener('blur', clear);
    };
  }, []);
  return held;
}
