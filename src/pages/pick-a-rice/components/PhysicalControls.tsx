import { useEffect, useState } from 'react';
import styles from './PhysicalControls.module.css';
import downButton from '@/assets/figma/physical-down.svg';
import upButton from '@/assets/figma/physical-up.svg';
import enterIcon from '@/assets/figma/enter-icon.svg';

type Control = 'down' | 'up' | 'enter';

const KEY_TO_CONTROL: Record<string, Control> = {
  s: 'down',
  ArrowDown: 'down',
  w: 'up',
  ArrowUp: 'up',
  Enter: 'enter',
};

const keyToControl = (key: string): Control | undefined =>
  KEY_TO_CONTROL[key] ?? KEY_TO_CONTROL[key.toLowerCase()];

export function PhysicalControls() {
  const [pressed, setPressed] = useState<Set<Control>>(new Set());

  const setControl = (control: Control, active: boolean) => {
    setPressed((current) => {
      if (current.has(control) === active) return current;
      const next = new Set(current);
      if (active) next.add(control);
      else next.delete(control);
      return next;
    });
  };

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const control = keyToControl(event.key);
      if (control) setControl(control, true);
    };
    const onKeyUp = (event: KeyboardEvent) => {
      const control = keyToControl(event.key);
      if (control) setControl(control, false);
    };
    const clear = () => setPressed((current) => (current.size ? new Set() : current));

    window.addEventListener('keydown', onKeyDown);
    window.addEventListener('keyup', onKeyUp);
    window.addEventListener('blur', clear);
    return () => {
      window.removeEventListener('keydown', onKeyDown);
      window.removeEventListener('keyup', onKeyUp);
      window.removeEventListener('blur', clear);
    };
  }, []);

  const buttonClass = (control: Control, positionClass: string) =>
    `${styles.button} ${positionClass} ${pressed.has(control) ? styles.pressed : ''}`;

  const pointerDown = (control: Control) => (event: React.PointerEvent<HTMLButtonElement>) => {
    event.stopPropagation();
    event.currentTarget.setPointerCapture(event.pointerId);
    setControl(control, true);
  };

  const pointerUp = (control: Control) => (event: React.PointerEvent<HTMLButtonElement>) => {
    event.stopPropagation();
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
    setControl(control, false);
  };

  return (
    <div className={styles.controls}>
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
        <img src={downButton} alt="" className={styles.controlImage} />
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
        <img src={upButton} alt="" className={styles.controlImage} />
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
        <img src={enterIcon} alt="" className={styles.enterIcon} />
      </button>
    </div>
  );
}
