import { useState, type PointerEvent } from 'react';
import { motion } from 'framer-motion';
import SoundButtonSvg from '@/assets/icon-buttons/sound.svg?react';
import styles from './SoundButton.module.css';
import { MORPH_TRANSITION, POSITIONS, useView } from '../view';
import { playRiceSound, setRiceSoundEnabled } from '../sounds';

export function SoundButton() {
  const view = useView();
  const [enabled, setEnabled] = useState(true);
  const [pressed, setPressed] = useState(false);

  const press = (event: PointerEvent<HTMLButtonElement>) => {
    if (event.button !== 0) return;
    event.preventDefault();
    event.stopPropagation();
    event.currentTarget.setPointerCapture(event.pointerId);
    playRiceSound('soundToggle');
    setPressed(true);
  };

  const release = (event: PointerEvent<HTMLButtonElement>) => {
    if (event.button !== 0) return;
    event.stopPropagation();
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
    setPressed(false);
    setEnabled((current) => {
      const next = !current;
      setRiceSoundEnabled(next);
      return next;
    });
  };

  const cancel = (event: PointerEvent<HTMLButtonElement>) => {
    event.stopPropagation();
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
    setPressed(false);
  };

  return (
    <motion.button
      type="button"
      tabIndex={-1}
      aria-label="Toggle sound"
      aria-pressed={enabled}
      className={`${styles.button} ${enabled ? '' : styles.off} ${pressed ? styles.pressed : ''}`}
      initial={false}
      animate={POSITIONS[view].soundButton}
      transition={MORPH_TRANSITION}
      onPointerDown={press}
      onPointerUp={release}
      onPointerCancel={cancel}
      onMouseDown={(event) => event.preventDefault()}
      onClick={(event) => event.stopPropagation()}
    >
      <svg className={styles.ring} viewBox="0 0 42 42" fill="none" aria-hidden="true">
        <circle cx="21" cy="21" r="20.5833" strokeWidth="0.833333" />
      </svg>
      <span className={styles.disc} />
      <SoundButtonSvg className={styles.glyph} aria-hidden="true" />
    </motion.button>
  );
}
