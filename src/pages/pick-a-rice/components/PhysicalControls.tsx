import { useCallback, useEffect, useRef, useState } from 'react';
import { motion } from 'framer-motion';
import styles from './PhysicalControls.module.css';
import downButton from '@/assets/figma/physical-down.svg';
import upButton from '@/assets/figma/physical-up.svg';
import enterIcon from '@/assets/figma/enter-icon.svg';
import { MORPH_TRANSITION, POSITIONS, useView } from '../view';

type Control = 'down' | 'up' | 'enter';
type ControlAction = () => void;

interface PhysicalControlsProps {
  onPrevious: ControlAction;
  onNext: ControlAction;
  onApply: ControlAction;
}

const KEY_TO_CONTROL: Record<string, Control> = {
  s: 'down',
  ArrowDown: 'down',
  w: 'up',
  ArrowUp: 'up',
  Enter: 'enter',
  e: 'enter',
};

const keyToControl = (key: string): Control | undefined =>
  KEY_TO_CONTROL[key] ?? KEY_TO_CONTROL[key.toLowerCase()];

const REPEATING_CONTROLS = new Set<Control>(['down', 'up']);
const REPEAT_DELAY_MS = 260;
const REPEAT_INTERVAL_MS = 130;

export function PhysicalControls({ onPrevious, onNext, onApply }: PhysicalControlsProps) {
  const view = useView();
  const cardPosition = POSITIONS[view].card;
  const [pressed, setPressed] = useState<Set<Control>>(new Set());
  const repeatControlRef = useRef<Control | null>(null);
  const repeatDelayRef = useRef<ReturnType<typeof window.setTimeout> | null>(null);
  const repeatIntervalRef = useRef<ReturnType<typeof window.setInterval> | null>(null);

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

  const stopRepeat = useCallback((control?: Control) => {
    if (control && repeatControlRef.current !== control) return;
    repeatControlRef.current = null;
    if (repeatDelayRef.current !== null) {
      window.clearTimeout(repeatDelayRef.current);
      repeatDelayRef.current = null;
    }
    if (repeatIntervalRef.current !== null) {
      window.clearInterval(repeatIntervalRef.current);
      repeatIntervalRef.current = null;
    }
  }, []);

  const startRepeat = useCallback((control: Control) => {
    if (!REPEATING_CONTROLS.has(control)) return;
    stopRepeat();
    repeatControlRef.current = control;
    repeatDelayRef.current = window.setTimeout(() => {
      repeatDelayRef.current = null;
      runControlAction(control);
      repeatIntervalRef.current = window.setInterval(() => {
        runControlAction(control);
      }, REPEAT_INTERVAL_MS);
    }, REPEAT_DELAY_MS);
  }, [runControlAction, stopRepeat]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const control = keyToControl(event.key);
      if (!control) return;
      event.preventDefault();
      if (event.repeat) return;
      setControl(control, true);
      runControlAction(control);
      startRepeat(control);
    };
    const onKeyUp = (event: KeyboardEvent) => {
      const control = keyToControl(event.key);
      if (!control) return;
      event.preventDefault();
      setControl(control, false);
      stopRepeat(control);
    };
    const clear = () => {
      stopRepeat();
      setPressed((current) => (current.size ? new Set() : current));
    };

    window.addEventListener('keydown', onKeyDown);
    window.addEventListener('keyup', onKeyUp);
    window.addEventListener('blur', clear);
    return () => {
      window.removeEventListener('keydown', onKeyDown);
      window.removeEventListener('keyup', onKeyUp);
      window.removeEventListener('blur', clear);
      stopRepeat();
    };
  }, [runControlAction, setControl, startRepeat, stopRepeat]);

  const buttonClass = (control: Control, positionClass: string) =>
    `${styles.button} ${positionClass} ${pressed.has(control) ? styles.pressed : ''}`;

  const pointerDown = (control: Control) => (event: React.PointerEvent<HTMLButtonElement>) => {
    event.stopPropagation();
    event.currentTarget.setPointerCapture(event.pointerId);
    setControl(control, true);
    runControlAction(control);
    startRepeat(control);
  };

  const pointerUp = (control: Control) => (event: React.PointerEvent<HTMLButtonElement>) => {
    event.stopPropagation();
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
    setControl(control, false);
    stopRepeat(control);
  };

  return (
    <motion.div
      className={styles.controls}
      initial={false}
      animate={{
        left: cardPosition.left + cardPosition.width - 250,
        top: cardPosition.top,
      }}
      transition={MORPH_TRANSITION}
    >
      <span className={`${styles.stem} ${styles.downStem}`} />
      <span className={`${styles.stem} ${styles.upStem}`} />
      <span className={`${styles.stem} ${styles.enterStem}`} />

      <button
        type="button"
        className={buttonClass('down', styles.downButton)}
        aria-label="Down"
        onPointerDown={pointerDown('down')}
        onPointerUp={pointerUp('down')}
        onPointerCancel={pointerUp('down')}
        onClick={(event) => event.stopPropagation()}
      >
        <span className={styles.cap}>
          <img src={downButton} alt="" className={styles.controlImage} />
        </span>
      </button>

      <button
        type="button"
        className={buttonClass('up', styles.upButton)}
        aria-label="Up"
        onPointerDown={pointerDown('up')}
        onPointerUp={pointerUp('up')}
        onPointerCancel={pointerUp('up')}
        onClick={(event) => event.stopPropagation()}
      >
        <span className={styles.cap}>
          <img src={upButton} alt="" className={styles.controlImage} />
        </span>
      </button>

      <button
        type="button"
        className={buttonClass('enter', styles.enterButton)}
        aria-label="Enter"
        onPointerDown={pointerDown('enter')}
        onPointerUp={pointerUp('enter')}
        onPointerCancel={pointerUp('enter')}
        onClick={(event) => event.stopPropagation()}
      >
        <span className={`${styles.cap} ${styles.enterCap}`}>
          <img src={enterIcon} alt="" className={styles.enterIcon} />
        </span>
      </button>
    </motion.div>
  );
}
