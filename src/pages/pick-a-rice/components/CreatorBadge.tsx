import { useEffect, useState } from 'react';
import styles from './CreatorBadge.module.css';
import cloudSvg from '@/assets/figma/creator-cloud.svg';
import cloudAltSvg from '@/assets/figma/creator-cloud-post.svg';
import decorSvg from '@/assets/figma/creator-decor.svg';
import { POSITIONS, useView } from '../view';

type DirKey = 'up' | 'left' | 'down' | 'right';

/** Creator bubble. Position shifts with the card morph; the inner content
 *  swaps between three layouts — the picking niri/dms tag with WASD key
 *  indicator, a large tilted "previewing" label while a rice is being
 *  previewed, and a "rice installed !" confirmation after apply. The cloud
 *  outline sits a shade darker in picking (#C7D8BF) and nudges lighter
 *  (#D0DACB) in the two shrunken states. */
export function CreatorBadge() {
  const view = useView();
  const pos = POSITIONS[view].creatorBadge;
  const isPicking = view === 'picking';
  const pressed = usePressedDirection();

  return (
    <div
      className={styles.badge}
      style={{ left: `${pos.left}px`, top: `${pos.top}px` }}
    >
      <img
        src={cloudSvg}
        alt=""
        className={styles.cloud}
        style={{ opacity: isPicking ? 1 : 0 }}
      />
      <img
        src={cloudAltSvg}
        alt=""
        className={styles.cloud}
        style={{ opacity: isPicking ? 0 : 1 }}
      />

      <div className={styles.inner}>
        <div className={styles.decor}>
          <div className={styles.decorBleed}>
            <img src={decorSvg} alt="" />
          </div>
        </div>

        {/* Picking-state content */}
        <div
          className={styles.content}
          style={{ opacity: view === 'picking' ? 1 : 0, pointerEvents: view === 'picking' ? 'auto' : 'none' }}
        >
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

        {/* Preview-state content */}
        <div
          className={styles.content}
          style={{ opacity: view === 'preview' ? 1 : 0, pointerEvents: view === 'preview' ? 'auto' : 'none' }}
        >
          <p className={styles.previewing}>previewing</p>
        </div>

        {/* Post-install-state content */}
        <div
          className={styles.content}
          style={{ opacity: view === 'post-install' ? 1 : 0, pointerEvents: view === 'post-install' ? 'auto' : 'none' }}
        >
          <span className={`${styles.accent} ${styles.accentA}`} />
          <span className={`${styles.accent} ${styles.accentB}`} />
          <span className={`${styles.accent} ${styles.accentC}`} />
          <span className={`${styles.accent} ${styles.accentD}`} />
          <span className={`${styles.accent} ${styles.accentE}`} />

          <p className={`${styles.installText} ${styles.installRice}`}>rice</p>
          <p className={`${styles.installText} ${styles.installInstalled}`}>installed</p>
          <p className={`${styles.installText} ${styles.installBang}`}>!</p>
        </div>
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
