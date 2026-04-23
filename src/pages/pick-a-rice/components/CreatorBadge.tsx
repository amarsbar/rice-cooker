import { useEffect, useState } from 'react';
import styles from './CreatorBadge.module.css';
import cloudSvg from '@/assets/figma/creator-cloud.svg';
import decorSvg from '@/assets/figma/creator-decor.svg';

type DirKey = 'up' | 'left' | 'down' | 'right';

/** Figma frame 350:6594 — the creator bubble. An outer bumpy cloud wraps a
 *  cream circle containing: a "1" pill, three yellow halo dots, the rice's
 *  tag text (niri, dms, +), and a WASD/arrow key indicator at the bottom.
 *  The indicator's "up" square glows orange while the corresponding key is
 *  held — matching Figma's 350:6752 pressed-state mock. */
export function CreatorBadge() {
  const pressed = usePressedDirection();
  return (
    <div className={styles.badge}>
      <img src={cloudSvg} alt="" className={styles.cloud} />
      <div className={styles.inner}>
        <div className={styles.decor}>
          <div className={styles.decorBleed}>
            <img src={decorSvg} alt="" />
          </div>
        </div>

        <div className={styles.stepPill}>
          <p>1</p>
        </div>

        <span className={`${styles.halo} ${styles.haloLg}`} />
        <span className={`${styles.halo} ${styles.haloMd}`} />
        <span className={`${styles.halo} ${styles.haloSm}`} />

        <p className={`${styles.tag} ${styles.tagTop}`}>niri</p>
        <p className={`${styles.tag} ${styles.tagBottom}`}>dms</p>
        <p className={`${styles.tag} ${styles.tagPlus}`}>+</p>

        <KeyIndicator pressed={pressed} />
      </div>
    </div>
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
  w: 'up',
  W: 'up',
  ArrowUp: 'up',
  a: 'left',
  A: 'left',
  ArrowLeft: 'left',
  s: 'down',
  S: 'down',
  ArrowDown: 'down',
  d: 'right',
  D: 'right',
  ArrowRight: 'right',
};

function usePressedDirection(): DirKey | null {
  const [pressed, setPressed] = useState<DirKey | null>(null);
  useEffect(() => {
    const onDown = (e: KeyboardEvent) => {
      const dir = KEY_MAP[e.key];
      if (dir) setPressed(dir);
    };
    const onUp = (e: KeyboardEvent) => {
      const dir = KEY_MAP[e.key];
      if (dir) setPressed((prev) => (prev === dir ? null : prev));
    };
    window.addEventListener('keydown', onDown);
    window.addEventListener('keyup', onUp);
    return () => {
      window.removeEventListener('keydown', onDown);
      window.removeEventListener('keyup', onUp);
    };
  }, []);
  return pressed;
}
