import { useCallback, useEffect, useRef, useState } from 'react';
import { motion } from 'framer-motion';
import styles from './PhysicalControls.module.css';
import ArrowButton from '@/assets/physical-controls/up.svg?react';
import enterIcon from '@/assets/physical-controls/enter.svg';
import { MORPH_TRANSITION, POSITIONS, useView } from '../view';

export type PhysicalControl = 'down' | 'up' | 'enter';
type Control = PhysicalControl;
type ControlAction = () => void;
type HoldDirection = -1 | 1;

interface PhysicalControlsProps {
  onPrevious: ControlAction;
  onNext: ControlAction;
  onApply: ControlAction;
  onHoldStart: (direction: HoldDirection) => void;
  onHoldEnd: () => void;
  onPressedChange: (pressed: ReadonlySet<PhysicalControl>) => void;
}

const KEY_TO_CONTROL: Record<string, Control> = {
  s: 'down',
  ArrowDown: 'down',
  w: 'up',
  ArrowUp: 'up',
  Enter: 'enter',
  e: 'enter',
};

export const keyToControl = (key: string): Control | undefined =>
  KEY_TO_CONTROL[key] ?? KEY_TO_CONTROL[key.toLowerCase()];

const isInteractiveTarget = (target: EventTarget | null): boolean =>
  target instanceof HTMLElement &&
  (target.isContentEditable ||
    target.closest('input, textarea, select, [contenteditable="true"]') !== null);

const preventFocus = (event: React.MouseEvent<HTMLButtonElement>) => {
  event.preventDefault();
};

const HOLD_DIRECTION: Partial<Record<Control, HoldDirection>> = { up: -1, down: 1 };
const HOLD_DELAY_MS = 220;

export function PhysicalControls({
  onPrevious,
  onNext,
  onApply,
  onHoldStart,
  onHoldEnd,
  onPressedChange,
}: PhysicalControlsProps) {
  const view = useView();
  const cardPosition = POSITIONS[view].card;
  const [pressed, setPressed] = useState<Set<Control>>(new Set());
  const holdControlRef = useRef<Control | null>(null);
  const holdDelayRef = useRef<ReturnType<typeof window.setTimeout> | null>(null);
  const holdingRef = useRef(false);

  const runControlAction = useCallback((control: Control) => {
    if (control === 'up') onPrevious();
    else if (control === 'down') onNext();
    else onApply();
  }, [onPrevious, onNext, onApply]);

  const setControl = useCallback((control: Control, active: boolean) => {
    setPressed((current) => {
      if (current.has(control) === active) return current;
      const next = new Set(current);
      if (active) next.add(control);
      else next.delete(control);
      return next;
    });
  }, []);

  const stopHold = useCallback((control?: Control) => {
    if (control && holdControlRef.current !== control) return;
    holdControlRef.current = null;
    if (holdDelayRef.current !== null) {
      window.clearTimeout(holdDelayRef.current);
      holdDelayRef.current = null;
    }
    if (holdingRef.current) {
      holdingRef.current = false;
      onHoldEnd();
    }
  }, [onHoldEnd]);

  const startHold = useCallback((control: Control) => {
    const direction = HOLD_DIRECTION[control];
    if (!direction) return;
    stopHold();
    holdControlRef.current = control;
    holdDelayRef.current = window.setTimeout(() => {
      holdDelayRef.current = null;
      holdingRef.current = true;
      onHoldStart(direction);
    }, HOLD_DELAY_MS);
  }, [onHoldStart, stopHold]);

  useEffect(() => {
    onPressedChange(pressed);
  }, [onPressedChange, pressed]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const control = keyToControl(event.key);
      if (!control) return;
      if (isInteractiveTarget(event.target)) return;
      event.preventDefault();
      if (event.repeat) return;
      setControl(control, true);
      runControlAction(control);
      startHold(control);
    };
    const onKeyUp = (event: KeyboardEvent) => {
      const control = keyToControl(event.key);
      if (!control) return;
      if (isInteractiveTarget(event.target)) return;
      event.preventDefault();
      setControl(control, false);
      stopHold(control);
    };
    const clear = () => {
      stopHold();
      setPressed((current) => (current.size ? new Set() : current));
    };

    window.addEventListener('keydown', onKeyDown);
    window.addEventListener('keyup', onKeyUp);
    window.addEventListener('blur', clear);
    return () => {
      window.removeEventListener('keydown', onKeyDown);
      window.removeEventListener('keyup', onKeyUp);
      window.removeEventListener('blur', clear);
      stopHold();
    };
  }, [runControlAction, setControl, startHold, stopHold]);

  const buttonClass = (control: Control, positionClass: string) =>
    `${styles.button} ${positionClass} ${pressed.has(control) ? styles.pressed : ''}`;

  const stemClass = (control: Control, positionClass: string) =>
    `${styles.stem} ${positionClass} ${pressed.has(control) ? styles.stemPressed : ''}`;

  const pointerDown = (control: Control) => (event: React.PointerEvent<HTMLButtonElement>) => {
    event.preventDefault();
    event.stopPropagation();
    event.currentTarget.setPointerCapture(event.pointerId);
    setControl(control, true);
    runControlAction(control);
    startHold(control);
  };

  const pointerUp = (control: Control) => (event: React.PointerEvent<HTMLButtonElement>) => {
    event.stopPropagation();
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
    setControl(control, false);
    stopHold(control);
  };

  const buttonKey = (control: Control, active: boolean) => (event: React.KeyboardEvent<HTMLButtonElement>) => {
    if (event.key !== 'Enter' && event.key !== ' ') return;
    event.preventDefault();
    event.stopPropagation();
    if (active && event.repeat) return;
    setControl(control, active);
    if (active) {
      runControlAction(control);
      startHold(control);
    } else {
      stopHold(control);
    }
  };

  return (
    <motion.div
      className={styles.controls}
      initial={false}
      animate={{
        left: cardPosition.left + cardPosition.width - 241,
        top: cardPosition.top,
      }}
      transition={MORPH_TRANSITION}
    >
      <span className={stemClass('up', styles.upStem)} />
      <span className={stemClass('down', styles.downStem)} />
      <span className={stemClass('enter', styles.enterStem)} />

      <button
        type="button"
        tabIndex={-1}
        className={buttonClass('up', styles.upButton)}
        aria-label="Previous"
        onPointerDown={pointerDown('up')}
        onMouseDown={preventFocus}
        onPointerUp={pointerUp('up')}
        onPointerCancel={pointerUp('up')}
        onKeyDown={buttonKey('up', true)}
        onKeyUp={buttonKey('up', false)}
        onClick={(event) => event.stopPropagation()}
      >
        <span className={styles.cap}>
          <ArrowButton aria-hidden="true" className={styles.controlImage} />
        </span>
      </button>

      <button
        type="button"
        tabIndex={-1}
        className={buttonClass('down', styles.downButton)}
        aria-label="Next"
        onPointerDown={pointerDown('down')}
        onMouseDown={preventFocus}
        onPointerUp={pointerUp('down')}
        onPointerCancel={pointerUp('down')}
        onKeyDown={buttonKey('down', true)}
        onKeyUp={buttonKey('down', false)}
        onClick={(event) => event.stopPropagation()}
      >
        <span className={styles.cap}>
          <ArrowButton aria-hidden="true" className={`${styles.controlImage} ${styles.controlImageDown}`} />
        </span>
      </button>

      <button
        type="button"
        tabIndex={-1}
        className={buttonClass('enter', styles.enterButton)}
        aria-label="Enter"
        onPointerDown={pointerDown('enter')}
        onMouseDown={preventFocus}
        onPointerUp={pointerUp('enter')}
        onPointerCancel={pointerUp('enter')}
        onKeyDown={buttonKey('enter', true)}
        onKeyUp={buttonKey('enter', false)}
        onClick={(event) => event.stopPropagation()}
      >
        <span className={`${styles.cap} ${styles.enterCap}`}>
          <img src={enterIcon} alt="" className={styles.enterIcon} />
        </span>
      </button>
    </motion.div>
  );
}
