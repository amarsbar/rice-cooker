import type { CSSProperties } from 'react';
import styles from './BootScreen.module.css';
import sadMessage from '@/assets/boot/sad-message.svg';
import enterIcon from '@/assets/boot/door-enter.svg';
import closeIcon from '@/assets/boot/close.svg';
import githubIcon from '@/assets/boot/github.svg';
import pointerArrow from '@/assets/pointer.svg';
import stickerTriangle from '@/assets/boot/sticker-triangle.svg';
import stickerDrop from '@/assets/boot/sticker-drop.svg';
import stickerGear from '@/assets/boot/sticker-gear.svg';
import forceIcon from '@/assets/boot/force-icon.svg';
import WarningRisk from '@/assets/boot/warning-risk.svg?react';

export const BOOT_ITEMS = ['enter', 'close', 'github'] as const;
export type BootItem = (typeof BOOT_ITEMS)[number];

const WORDS: Record<BootItem, string> = {
  enter: 'ENTER',
  close: 'CLOSE',
  github: 'GITHUB',
};
export const BOOT_FORCE_HOLD_STEP_MS = 1000;
export const BOOT_FORCE_HOLD_LETTERS = WORDS.enter.length;
export const BOOT_FORCE_HOLD_MS = BOOT_FORCE_HOLD_LETTERS * BOOT_FORCE_HOLD_STEP_MS;

const wordPosition: Record<BootItem, { left: number; top: number; padding: number }> = {
  enter: { left: 202.51, top: 229, padding: 8 },
  close: { left: 203, top: 274.88, padding: 10 },
  github: { left: 202.51, top: 324.76, padding: 8 },
};

const iconPosition: Record<BootItem, { idle: { left: number; top: number }; active: { left: number; top: number } }> = {
  enter: { idle: { left: 156.26, top: 232 }, active: { left: 164.28, top: 232 } },
  close: { idle: { left: 156.26, top: 279.88 }, active: { left: 166.26, top: 279.88 } },
  github: { idle: { left: 156.57, top: 327.76 }, active: { left: 164.28, top: 327.76 } },
};

const icons: Record<BootItem, string> = {
  enter: enterIcon,
  close: closeIcon,
  github: githubIcon,
};

const optionClass: Record<BootItem, string> = {
  enter: styles.optionEnter,
  close: styles.optionClose,
  github: styles.optionGithub,
};

const LETTER_SIZE = 30;
const LETTER_OVERLAP = 7.5;
const POINTER_SIZE = 8;
const POINTER_GAP = 5;

function getPointerStyle(item: BootItem, side: 'top' | 'bottom'): CSSProperties {
  const position = wordPosition[item];
  const wordWidth = WORDS[item].length * (LETTER_SIZE - LETTER_OVERLAP) + LETTER_OVERLAP;
  const pillWidth = wordWidth + position.padding * 2;
  const centerX = position.left + pillWidth / 2;
  const pillTop = position.top;
  const pillBottom = position.top + position.padding * 2 + LETTER_SIZE;
  const tucked =
    (item === 'enter' && side === 'top') || (item === 'github' && side === 'bottom');

  return {
    left: centerX - POINTER_SIZE / 2,
    top: tucked
      ? pillTop + position.padding + LETTER_SIZE / 2 - POINTER_SIZE / 2
      : side === 'top'
        ? pillTop - POINTER_GAP - POINTER_SIZE
        : pillBottom + POINTER_GAP,
    width: POINTER_SIZE,
    height: POINTER_SIZE,
    transform: side === 'bottom' ? 'rotate(180deg)' : 'rotate(0deg)',
  };
}

export function BootScreen({
  active,
  onActiveChange,
  onApply,
  enterHoldLetters = 0,
}: {
  active: BootItem;
  onActiveChange: (item: BootItem) => void;
  onApply: () => void;
  enterHoldLetters?: number;
}) {
  const word = WORDS[active];
  const wordStyle = {
    left: wordPosition[active].left,
    top: wordPosition[active].top,
    padding: wordPosition[active].padding,
  } as CSSProperties;

  return (
    <div className={styles.boot} onClick={(event) => event.stopPropagation()}>
      <p className={`${styles.navLabel} ${styles.prevLabel}`}>Prev</p>
      <p className={`${styles.navLabel} ${styles.nextLabel}`}>Next</p>
      <p className={`${styles.navLabel} ${styles.confirmLabel}`}>CONFIRM</p>

      <div className={styles.hero}>
        <img src={sadMessage} alt="" className={styles.sadMessage} />
      </div>

      <p className={styles.description}>Rice Cooker is only built for arch + hyprland + quickshell.</p>

      {BOOT_ITEMS.map((item) => (
        <button
          type="button"
          tabIndex={-1}
          key={item}
          className={`${styles.optionIcon} ${optionClass[item]} ${active === item ? styles.optionIconActive : ''}`}
          style={(active === item ? iconPosition[item].active : iconPosition[item].idle) as CSSProperties}
          onMouseDown={(event) => event.preventDefault()}
          onMouseEnter={() => onActiveChange(item)}
          onClick={(event) => {
            event.stopPropagation();
            onActiveChange(item);
            onApply();
          }}
          aria-label={WORDS[item]}
        >
          <img src={icons[item]} alt="" className={styles.optionIconImage} />
        </button>
      ))}

      <button
        type="button"
        tabIndex={-1}
        className={styles.wordPill}
        style={wordStyle}
        onMouseDown={(event) => event.preventDefault()}
        onClick={(event) => {
          event.stopPropagation();
          onApply();
        }}
      >
        <span className={styles.wordCluster}>
          {[...word].map((char, index) => {
            const held = active === 'enter' && index < enterHoldLetters;
            return (
              <span
                className={`${styles.wordLetter} ${held ? styles.wordLetterHeld : ''}`}
                key={`${char}-${index}`}
                style={{ zIndex: held ? index + 1 : 0 }}
              >
                {char}
              </span>
            );
          })}
        </span>
      </button>

      <Pointers active={active} />
      <Decorations active={active} />
    </div>
  );
}

function Pointers({ active }: { active: BootItem }) {
  return (
    <>
      <img src={pointerArrow} alt="" className={styles.pointer} style={getPointerStyle(active, 'top')} />
      <img src={pointerArrow} alt="" className={styles.pointer} style={getPointerStyle(active, 'bottom')} />
    </>
  );
}

function Decorations({ active }: { active: BootItem }) {
  if (active === 'enter') {
    return (
      <>
        <img src={forceIcon} alt="" className={styles.forceHint} />
        <div className={styles.warning}>
          <WarningRisk className={styles.warningRisk} aria-hidden="true" />
        </div>
      </>
    );
  }

  if (active === 'github') {
    return (
      <>
        <Flower className={`${styles.flower} ${styles.flowerPink}`} color="#FDB1E5" />
        <Flower className={`${styles.flower} ${styles.flowerLime}`} color="#E8FF76" />
        <Flower className={`${styles.flower} ${styles.flowerYellow}`} color="#FADE26" />
      </>
    );
  }

  return (
    <>
      <span className={styles.decoTriangle}>
        <img src={stickerTriangle} alt="" />
      </span>
      <img src={stickerDrop} alt="" className={styles.decoDrop} />
      <span className={styles.decoGear}>
        <img src={stickerGear} alt="" />
      </span>
    </>
  );
}

function Flower({ className, color }: { className: string; color: string }) {
  return (
    <svg className={className} viewBox="0 0 29.958 29.9551" fill="none" xmlns="http://www.w3.org/2000/svg">
      <path
        d="M8.09473 0C10.9633 0.000143025 13.4813 1.49369 14.9189 3.74414C16.368 1.49353 18.9074 0.000976562 21.7988 0.000976562C26.3045 0.00112102 29.9569 3.62456 29.957 8.09473C29.957 10.9632 28.4519 13.4813 26.1836 14.9189C28.4521 16.3679 29.958 18.9056 29.958 21.7969C29.958 26.3024 26.3047 29.9551 21.7988 29.9551C18.9065 29.955 16.3676 28.4484 14.9189 26.1787C13.4814 28.4474 10.9637 29.954 8.09473 29.9541C3.62418 29.9541 0.000137324 26.3013 0 21.7959C0 18.9051 1.49198 16.366 3.74219 14.917C1.49265 13.4792 0.000166821 10.9623 0 8.09473C0 3.6244 3.6241 0 8.09473 0ZM14.9189 12.4434C14.2842 13.4372 13.4391 14.282 12.4453 14.917C13.4396 15.5571 14.2849 16.4099 14.9199 17.4121C15.5597 16.4107 16.4117 15.5588 17.4131 14.9189C16.411 14.2837 15.5589 13.4378 14.9189 12.4434Z"
        fill={color}
      />
      <circle cx="15.0373" cy="15.0381" r="6.01509" fill="#1B161A" />
    </svg>
  );
}
