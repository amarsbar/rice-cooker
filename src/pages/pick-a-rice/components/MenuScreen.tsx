import { useEffect, useRef, useState, type CSSProperties } from 'react';
import styles from './MenuScreen.module.css';
import logoRice from '@/assets/menu/logo-rice.svg';
import logoCooker from '@/assets/menu/logo-cooker.svg';
import backActive from '@/assets/menu/back-active.svg';
import backIdle from '@/assets/menu/back-idle.svg';
import externalIcon from '@/assets/menu/external.svg';
import keyUp from '@/assets/menu/key-up.svg';
import keyDown from '@/assets/menu/key-down.svg';
import wheel from '@/assets/menu/wheel.svg';
import prevMouse from '@/assets/menu/prev-mouse.svg';
import smallKeyLine from '@/assets/menu/small-key-line.svg';
import smallKeyBody from '@/assets/menu/small-key-body.svg';
import enterOuter from '@/assets/menu/enter-outer.svg';
import enterInner from '@/assets/menu/enter-inner.svg';
import enterSymbol from '@/assets/menu/enter-symbol.svg';
import lineH from '@/assets/menu/line-h.svg';
import linePrevTop from '@/assets/menu/line-prev-top.svg';
import lineSmallKey from '@/assets/menu/line-small-key.svg';
import lineLong from '@/assets/menu/line-long.svg';
import lineEnter from '@/assets/menu/line-enter.svg';
import tick from '@/assets/menu/tick.svg';
import arrowA from '@/assets/menu/arrow-a.svg';
import arrowB from '@/assets/menu/arrow-b.svg';
import arrowC from '@/assets/menu/arrow-c.svg';
import arrowLeft from '@/assets/menu/arrow-left.svg';
import arrowDown from '@/assets/menu/arrow-down.svg';
import arrowD from '@/assets/menu/arrow-d.svg';
import SocialWeb from '@/assets/menu/social-web.svg?react';
import SocialX from '@/assets/menu/social-x.svg?react';
import SocialGithub from '@/assets/menu/social-github.svg?react';
import SocialInstagram from '@/assets/menu/social-instagram.svg?react';
import { playRiceSound } from '../sounds';
import { MENU_ITEMS, type MenuItem } from '../menuOptions';
import { keyToControl } from './PhysicalControls';
const creditUrls = [
  'https://www.butterfly.so/',
  'https://x.com/bflycomputer',
  'https://www.instagram.com/bflycomputer/',
  'https://github.com/amarsbar/rice-cooker/',
] as const;
const creditItemCount = creditUrls.length;
const BACK_TEXT = 'BACK';
const BUBBLE_STAGGER_MS = 60;
const BACK_HIDE_MS = BACK_TEXT.length * BUBBLE_STAGGER_MS;

const rows = {
  revert: {
    title: 'Revert',
    description: 'will take you back to your original rice.',
    letters: 'REVERT',
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
              revertHolding && styles.letterRevertHolding,
              bubbleAnimation === 'show' && styles.letterBubbleShow,
              bubbleAnimation === 'hide' && styles.letterBubbleHide,
              bubbleAnimation === 'hidden' && styles.letterBubbleHidden,
            )}
            key={`${letter}-${index}`}
            style={{ '--bubble-delay': `${bubbleIndex * BUBBLE_STAGGER_MS}ms` } as CSSProperties}
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

function Guide() {
  return (
    <>
      <p className={cx(styles.guideLabel, styles.confirmLabel)}>CONFIRM</p>

      <div className={styles.guideAssets}>
        <p className={styles.keyHint}>use your keys!</p>
        <img src={keyUp} alt="" className={cx(styles.asset, styles.keyUp)} />
        <img src={keyDown} alt="" className={cx(styles.asset, styles.keyDown)} />

        <img src={wheel} alt="" className={cx(styles.asset, styles.wheel)} />
        <img src={prevMouse} alt="" className={cx(styles.asset, styles.prevMouse)} />
        <img src={smallKeyLine} alt="" className={cx(styles.asset, styles.smallKeyLine)} />
        <img src={smallKeyBody} alt="" className={cx(styles.asset, styles.smallKeyBody)} />

        <span className={styles.enterKey}>
          <img src={enterOuter} alt="" className={cx(styles.asset, styles.enterOuter)} />
          <img src={enterInner} alt="" className={cx(styles.asset, styles.enterInner)} />
          <img src={enterSymbol} alt="" className={cx(styles.asset, styles.enterSymbol)} />
        </span>

        <img src={lineH} alt="" className={cx(styles.asset, styles.lineH1)} />
        <img src={lineH} alt="" className={cx(styles.asset, styles.lineH2)} />
        <img src={linePrevTop} alt="" className={cx(styles.asset, styles.linePrevTop)} />
        <img src={lineSmallKey} alt="" className={cx(styles.asset, styles.lineSmallKey)} />
        <img src={lineLong} alt="" className={cx(styles.asset, styles.lineLong1)} />
        <img src={lineLong} alt="" className={cx(styles.asset, styles.lineLong2)} />
        <img src={lineEnter} alt="" className={cx(styles.asset, styles.lineEnter)} />
        <img src={tick} alt="" className={cx(styles.asset, styles.tick1)} />
        <img src={tick} alt="" className={cx(styles.asset, styles.tick2)} />
        <img src={arrowA} alt="" className={cx(styles.asset, styles.arrowA)} />
        <img src={arrowB} alt="" className={cx(styles.asset, styles.arrowB)} />
        <img src={arrowC} alt="" className={cx(styles.asset, styles.arrowC)} />
        <img src={arrowLeft} alt="" className={cx(styles.asset, styles.arrowLeft1)} />
        <img src={arrowLeft} alt="" className={cx(styles.asset, styles.arrowLeft2)} />
        <img src={arrowLeft} alt="" className={cx(styles.asset, styles.arrowLeft3)} />
        <img src={arrowDown} alt="" className={cx(styles.asset, styles.arrowDown1)} />
        <img src={arrowDown} alt="" className={cx(styles.asset, styles.arrowDown2)} />
        <img src={arrowDown} alt="" className={cx(styles.asset, styles.arrowDown3)} />
        <img src={arrowD} alt="" className={cx(styles.asset, styles.arrowD)} />
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
  const [backLeaving, setBackLeaving] = useState(false);
  const previousActiveRef = useRef(active);
  const backLeavingTimeoutRef = useRef<ReturnType<typeof window.setTimeout> | null>(null);
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

      if (control === 'enter' && active === 'back') {
        event.preventDefault();
        onBack();
        return;
      }

      if (control === 'enter' && active === 'revert') {
        event.preventDefault();
        if (!event.repeat) setRevertHolding(true);
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
      if (event.key === 'Enter') setRevertHolding(false);
    };

    const onBlur = () => setRevertHolding(false);

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
    if (active !== 'revert') setRevertHolding(false);
  }, [active]);

  return (
    <div className={styles.menu} onClick={(event) => event.stopPropagation()}>
      <div className={styles.version}>V0.1</div>
      <img src={logoRice} alt="" className={cx(styles.logoImage, styles.logoRice)} />
      <img src={logoCooker} alt="" className={cx(styles.logoImage, styles.logoCooker)} />

      <div className={styles.backGroup} onMouseEnter={() => onActiveChange('back')} onClick={onBack}>
        <span className={cx(styles.backCircle, backVisualActive && styles.backCircleActive)}>
          <img src={backVisualActive ? backActive : backIdle} alt="" className={styles.backIcon} />
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

      <Guide />
    </div>
  );
}
