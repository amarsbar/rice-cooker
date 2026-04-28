import type { CSSProperties } from 'react';
import styles from './BootScreen.module.css';
import sadMessage from '@/assets/boot/sad-card.svg';
import EnterIcon from '@/assets/boot/enter.svg?react';
import CloseIcon from '@/assets/boot/close.svg?react';
import GithubIcon from '@/assets/icons/github.svg?react';
import pointerArrow from '@/assets/icons/pointer.svg';
import stickerArch from '@/assets/boot/arch-sticker.svg';
import stickerHyprland from '@/assets/boot/hyprland-sticker.svg';
import stickerQuickshell from '@/assets/boot/quickshell-sticker.svg';
import forceIcon from '@/assets/boot/force-hold.svg';
import FlowerSvg from '@/assets/boot/flower.svg?react';
import WarningRisk from '@/assets/boot/warning-sticker.svg?react';

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

const icons = {
  enter: EnterIcon,
  close: CloseIcon,
  github: GithubIcon,
} as const;

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

      {BOOT_ITEMS.map((item) => {
        const Icon = icons[item];
        return (
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
            <Icon className={styles.optionIconImage} aria-hidden="true" />
          </button>
        );
      })}

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
            const held = index < enterHoldLetters;
            return (
              <span
                className={`${styles.wordLetter} ${held ? styles.wordLetterHeld : ''}`}
                key={index}
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
        <FlowerSvg className={`${styles.flower} ${styles.flowerPink}`} aria-hidden="true" />
        <FlowerSvg className={`${styles.flower} ${styles.flowerLime}`} aria-hidden="true" />
        <FlowerSvg className={`${styles.flower} ${styles.flowerYellow}`} aria-hidden="true" />
      </>
    );
  }

  return (
    <>
      <span className={styles.decoArch}>
        <img src={stickerArch} alt="" />
      </span>
      <img src={stickerHyprland} alt="" className={styles.decoHyprland} />
      <span className={styles.decoQuickshell}>
        <img src={stickerQuickshell} alt="" />
      </span>
    </>
  );
}
