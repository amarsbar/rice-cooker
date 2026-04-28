import { useEffect, useRef, useState, type CSSProperties } from 'react';
import styles from './MenuScreen.module.css';
import holdStyles from './HoldPaint.module.css';
import riceCookerText from '@/assets/menu/rice-cooker-text.svg';
import backIcon from '@/assets/icons/back.svg';
import externalIcon from '@/assets/menu/external.svg';
import KeyUp from '@/assets/menu/arrow-key.svg?react';
import EnterKeySvg from '@/assets/menu/enter-key.svg?react';
import topRightItems from '@/assets/menu/header-cluster.svg';
import verticalArrow from '@/assets/menu/vertical-arrow.svg';
import SocialWeb from '@/assets/menu/social/web.svg?react';
import SocialX from '@/assets/menu/social/x.svg?react';
import SocialGithub from '@/assets/menu/social/github.svg?react';
import SocialInstagram from '@/assets/menu/social/instagram.svg?react';
import { playRiceSound } from '../sounds';
import { MENU_ITEMS, type MenuItem } from '../menuOptions';
import { keyToControl, type PhysicalControl } from './PhysicalControls';
const creditUrls = [
  'https://www.butterfly.so/',
  'https://x.com/bflycomputer',
  'https://www.instagram.com/bflycomputer/',
  'https://github.com/amarsbar/rice-cooker/',
] as const;
const creditItemCount = creditUrls.length;
const BACK_TEXT = 'BACK';
const REVERT_TEXT = 'REVERT';
const BUBBLE_STAGGER_MS = 60;
const BACK_HIDE_MS = BACK_TEXT.length * BUBBLE_STAGGER_MS;
const REVERT_HOLD_STEP_MS = 160;
const REVERT_HOLD_MS = (REVERT_TEXT.length + 1) * REVERT_HOLD_STEP_MS;

const rows = {
  revert: {
    title: 'Revert',
    description: 'will take you back to your original rice.',
    letters: REVERT_TEXT,
  },
  submit: {
    title: 'Submit a rice',
    description: 'share a rice of your own with us (and everyone).',
    letters: 'SUBMIT',
  },
  credits: {
    title: 'Credits',
    description: 'made for fun by two brothers at butterfly.',
    letters: '',
  },
} as const;

const cx = (...classes: Array<string | false | undefined>) => classes.filter(Boolean).join(' ');
const backIconStyle = { '--back-icon': `url("${backIcon}")` } as CSSProperties;

function Letters({
  text,
  ghost,
  revertHolding,
  bubbleAnimation,
}: {
  text: string;
  ghost?: boolean;
  revertHolding?: boolean;
  bubbleAnimation?: 'show' | 'hide' | 'hidden';
}) {
  return (
    <div className={styles.letters}>
      {[...text].map((letter, index) => {
        const bubbleIndex = bubbleAnimation === 'hide' ? text.length - 1 - index : index;
        return (
          <span
            className={cx(
              styles.letter,
              ghost && styles.letterGhost,
              revertHolding && holdStyles.holdPaint,
              bubbleAnimation === 'show' && styles.letterBubbleShow,
              bubbleAnimation === 'hide' && styles.letterBubbleHide,
              bubbleAnimation === 'hidden' && styles.letterBubbleHidden,
            )}
            key={`${letter}-${index}`}
            style={{
              '--bubble-delay': `${bubbleIndex * BUBBLE_STAGGER_MS}ms`,
              '--hold-delay': `${(index + 1) * REVERT_HOLD_STEP_MS}ms`,
            } as CSSProperties}
          >
            {letter}
          </span>
        );
      })}
    </div>
  );
}

function MenuRow({
  item,
  active,
  ghost,
  top,
  onHover,
  creditIndex = 0,
  onCreditHover,
  revertHolding,
}: {
  item: Exclude<MenuItem, 'back'>;
  active: boolean;
  ghost?: boolean;
  top: number;
  onHover: () => void;
  creditIndex?: number;
  onCreditHover?: (index: number) => void;
  revertHolding?: boolean;
}) {
  const row = rows[item];

  if (!active && !ghost) {
    return (
      <div className={styles.row} style={{ top }} onMouseEnter={onHover}>
        <span className={styles.rowLabel}>{row.title}</span>
      </div>
    );
  }

  return (
    <div
      className={cx(styles.row, styles.rowExpanded, active && styles.rowActive)}
      style={{ top }}
      onMouseEnter={onHover}
    >
      <span className={styles.expandedTitle}>{row.title}</span>
      <span className={styles.expandedDescription}>{row.description}</span>
      {item === 'credits' ? (
        <CreditLinks activeIndex={creditIndex} onHover={onCreditHover} />
      ) : (
        <Letters text={row.letters} ghost={ghost} revertHolding={item === 'revert' && revertHolding} />
      )}
      {active && item === 'revert' && <span className={styles.holdLabel}>HOLD</span>}
      {active && (
        <img
          src={externalIcon}
          alt=""
          className={cx(styles.externalIcon, item === 'revert' && styles.externalIconRevert)}
        />
      )}
    </div>
  );
}

function CreditLinks({
  activeIndex,
  onHover = () => {},
}: {
  activeIndex: number;
  onHover?: (index: number) => void;
}) {
  return (
    <div className={styles.creditLinks}>
      <a
        href={creditUrls[0]}
        target="_blank"
        rel="noreferrer"
        tabIndex={-1}
        className={cx(styles.creditLink, activeIndex === 0 && styles.creditLinkActive)}
        onMouseEnter={() => onHover(0)}
      >
        <SocialWeb className={styles.creditIconWeb} />
      </a>
      <a
        href={creditUrls[1]}
        target="_blank"
        rel="noreferrer"
        tabIndex={-1}
        className={cx(styles.creditLink, activeIndex === 1 && styles.creditLinkActive)}
        onMouseEnter={() => onHover(1)}
      >
        <SocialX className={styles.creditIconLarge} />
      </a>
      <a
        href={creditUrls[2]}
        target="_blank"
        rel="noreferrer"
        tabIndex={-1}
        className={cx(styles.creditLink, activeIndex === 2 && styles.creditLinkActive)}
        onMouseEnter={() => onHover(2)}
      >
        <SocialInstagram className={styles.creditIconLarge} />
      </a>
      <a
        href={creditUrls[3]}
        target="_blank"
        rel="noreferrer"
        tabIndex={-1}
        className={cx(styles.creditLink, activeIndex === 3 && styles.creditLinkActive)}
        onMouseEnter={() => onHover(3)}
      >
        <SocialGithub className={styles.creditIconGithub} />
      </a>
    </div>
  );
}

function ArrowKey({
  className,
  down,
  pressed,
}: {
  className: string;
  down?: boolean;
  pressed: boolean;
}) {
  return (
    <KeyUp
      aria-hidden="true"
      className={cx(styles.asset, styles.arrowKeySvg, className, down && styles.arrowKeyDownSvg, pressed && styles.keyPressed)}
    />
  );
}

function EnterKey({ pressed }: { pressed: boolean }) {
  return (
    <EnterKeySvg aria-hidden="true" className={cx(styles.asset, styles.enterKey, pressed && styles.keyPressed)} />
  );
}

function Guide({ pressedControl }: { pressedControl: PhysicalControl | null }) {
  return (
    <>
      <div className={styles.guideAssets}>
        <p className={styles.keyHint}>use your keys!</p>
        <img src={topRightItems} alt="" className={cx(styles.asset, styles.topRightItems)} />
        <ArrowKey className={styles.keyUp} pressed={pressedControl === 'up'} />
        <ArrowKey className={styles.keyDown} down pressed={pressedControl === 'down'} />

        <EnterKey pressed={pressedControl === 'enter'} />

        <img src={verticalArrow} alt="" className={cx(styles.asset, styles.verticalArrow1)} />
        <img src={verticalArrow} alt="" className={cx(styles.asset, styles.verticalArrow2)} />
        <img src={verticalArrow} alt="" className={cx(styles.asset, styles.verticalArrow3)} />
      </div>
    </>
  );
}

export function MenuScreen({
  active,
  onActiveChange,
  onBack,
}: {
  active: MenuItem;
  onActiveChange: (item: MenuItem) => void;
  onBack: () => void;
}) {
  const [creditIndex, setCreditIndex] = useState(0);
  const [revertHolding, setRevertHolding] = useState(false);
  const [pressedControl, setPressedControl] = useState<PhysicalControl | null>(null);
  const [backLeaving, setBackLeaving] = useState(false);
  const previousActiveRef = useRef(active);
  const backLeavingTimeoutRef = useRef<ReturnType<typeof window.setTimeout> | null>(null);
  const revertHoldingTimeoutRef = useRef<ReturnType<typeof window.setTimeout> | null>(null);
  const backActiveState = active === 'back';
  const backVisualActive = backActiveState || backLeaving;
  const top = {
    revert: 163,
    submit: active === 'revert' || active === 'back' ? 293 : 205,
    credits: active === 'credits' ? 247 : 335,
  };

  useEffect(() => {
    const previousActive = previousActiveRef.current;
    previousActiveRef.current = active;

    if (previousActive === 'back' && active !== 'back') {
      setBackLeaving(true);
      if (backLeavingTimeoutRef.current !== null) window.clearTimeout(backLeavingTimeoutRef.current);
      backLeavingTimeoutRef.current = window.setTimeout(() => {
        setBackLeaving(false);
        backLeavingTimeoutRef.current = null;
      }, BACK_HIDE_MS);
      return;
    }

    if (active === 'back') {
      setBackLeaving(false);
      if (backLeavingTimeoutRef.current !== null) {
        window.clearTimeout(backLeavingTimeoutRef.current);
        backLeavingTimeoutRef.current = null;
      }
    }
  }, [active]);

  useEffect(() => () => {
    if (backLeavingTimeoutRef.current !== null) window.clearTimeout(backLeavingTimeoutRef.current);
    if (revertHoldingTimeoutRef.current !== null) window.clearTimeout(revertHoldingTimeoutRef.current);
  }, []);

  useEffect(() => {
    const move = (direction: -1 | 1) => {
      playRiceSound(direction > 0 ? 'moveDown' : 'moveUp');
      const currentIndex = MENU_ITEMS.indexOf(active);
      const next = MENU_ITEMS[(currentIndex + direction + MENU_ITEMS.length) % MENU_ITEMS.length];
      if (next === 'credits') setCreditIndex(direction > 0 ? 0 : creditItemCount - 1);
      onActiveChange(next);
    };

    const onKeyDown = (event: KeyboardEvent) => {
      const control = keyToControl(event.key);
      const direction = control === 'up' ? -1 : control === 'down' ? 1 : 0;
      if (control) setPressedControl(control);

      if (control === 'enter' && active === 'back') {
        event.preventDefault();
        onBack();
        return;
      }

      if (control === 'enter' && active === 'revert') {
        event.preventDefault();
        if (!event.repeat) {
          setRevertHolding(true);
          if (revertHoldingTimeoutRef.current !== null) window.clearTimeout(revertHoldingTimeoutRef.current);
          revertHoldingTimeoutRef.current = window.setTimeout(() => {
            setRevertHolding(false);
            revertHoldingTimeoutRef.current = null;
          }, REVERT_HOLD_MS);
        }
        return;
      }

      if (control === 'enter' && active === 'credits') {
        event.preventDefault();
        if (event.repeat) return;
        window.open(creditUrls[creditIndex], '_blank', 'noopener,noreferrer');
        return;
      }

      if (active === 'credits' && (event.key === 'ArrowLeft' || event.key === 'ArrowRight')) {
        event.preventDefault();
        setCreditIndex(
          (index) => (index + (event.key === 'ArrowLeft' ? -1 : 1) + creditItemCount) % creditItemCount,
        );
        return;
      }

      if (!direction) return;
      event.preventDefault();

      if (active === 'credits') {
        const nextCreditIndex = creditIndex + direction;
        if (nextCreditIndex >= 0 && nextCreditIndex < creditItemCount) {
          setCreditIndex(nextCreditIndex);
          return;
        }
      }

      move(direction);
    };

    const onKeyUp = (event: KeyboardEvent) => {
      const control = keyToControl(event.key);
      if (!control) return;
      setPressedControl((current) => (current === control ? null : current));
      if (control === 'enter') {
        if (revertHoldingTimeoutRef.current !== null) {
          window.clearTimeout(revertHoldingTimeoutRef.current);
          revertHoldingTimeoutRef.current = null;
        }
        setRevertHolding(false);
      }
    };

    const onBlur = () => {
      setPressedControl(null);
      if (revertHoldingTimeoutRef.current !== null) {
        window.clearTimeout(revertHoldingTimeoutRef.current);
        revertHoldingTimeoutRef.current = null;
      }
      setRevertHolding(false);
    };

    window.addEventListener('keydown', onKeyDown);
    window.addEventListener('keyup', onKeyUp);
    window.addEventListener('blur', onBlur);
    return () => {
      window.removeEventListener('keydown', onKeyDown);
      window.removeEventListener('keyup', onKeyUp);
      window.removeEventListener('blur', onBlur);
    };
  }, [active, backLeaving, creditIndex, onActiveChange, onBack]);

  useEffect(() => {
    if (active !== 'revert') {
      if (revertHoldingTimeoutRef.current !== null) {
        window.clearTimeout(revertHoldingTimeoutRef.current);
        revertHoldingTimeoutRef.current = null;
      }
      setRevertHolding(false);
    }
  }, [active]);

  return (
    <div className={styles.menu} onClick={(event) => event.stopPropagation()}>
      <div className={styles.version}>V0.1</div>
      <img src={riceCookerText} alt="" className={styles.logoImage} />

      <div className={styles.backGroup} onMouseEnter={() => onActiveChange('back')} onClick={onBack}>
        <span className={cx(styles.backCircle, backVisualActive && styles.backCircleActive)}>
          <span aria-hidden="true" className={styles.backIcon} style={backIconStyle} />
        </span>
        <Letters
          text={BACK_TEXT}
          bubbleAnimation={backActiveState ? 'show' : backLeaving ? 'hide' : 'hidden'}
        />
      </div>

      <MenuRow
        item="revert"
        active={active === 'revert'}
        ghost={backActiveState}
        top={top.revert}
        onHover={() => onActiveChange('revert')}
        revertHolding={revertHolding}
      />
      <MenuRow
        item="submit"
        active={active === 'submit'}
        top={top.submit}
        onHover={() => onActiveChange('submit')}
      />
      <MenuRow
        item="credits"
        active={active === 'credits'}
        top={top.credits}
        onHover={() => onActiveChange('credits')}
        creditIndex={creditIndex}
        onCreditHover={setCreditIndex}
      />

      <Guide pressedControl={pressedControl} />
    </div>
  );
}
