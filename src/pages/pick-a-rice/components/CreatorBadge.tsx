import { useEffect, useState } from 'react';
import { motion } from 'framer-motion';
import styles from './CreatorBadge.module.css';
import cloudSvg from '@/assets/figma/creator-cloud.svg';
import cloudAltSvg from '@/assets/figma/creator-cloud-post.svg';
import decorSvg from '@/assets/figma/creator-decor.svg';
import {
  MORPH_TRANSITION,
  POSITIONS,
  SCREEN_FADE_TRANSITION,
  SHRUNKEN_TEXT_VARIANTS,
  useView,
} from '../view';

type DirKey = 'up' | 'left' | 'down' | 'right';

/** Creator bubble. Position animates with the card morph; inner content
 *  swaps between picking (niri/dms tag + WASD indicator), preview
 *  (tilted "previewing" label), and post-install ("rice installed !"
 *  with paint-splat accents). Cloud outline sits a shade darker in
 *  picking (#C7D8BF) and nudges lighter (#D0DACB) in the shrunken
 *  states — rendered as two stacked images that crossfade. */
export function CreatorBadge() {
  const view = useView();
  const pressed = usePressedDirection();

  return (
    <motion.div
      className={styles.badge}
      initial={false}
      animate={POSITIONS[view].creatorBadge}
      transition={MORPH_TRANSITION}
    >
      <motion.img
        src={cloudSvg}
        alt=""
        className={styles.cloud}
        initial={false}
        animate={{ opacity: view === 'picking' ? 1 : 0 }}
        transition={SCREEN_FADE_TRANSITION}
      />
      <motion.img
        src={cloudAltSvg}
        alt=""
        className={styles.cloud}
        initial={false}
        animate={{ opacity: view === 'picking' ? 0 : 1 }}
        transition={SCREEN_FADE_TRANSITION}
      />

      <div className={styles.inner}>
        <div className={styles.decor}>
          <div className={styles.decorBleed}>
            <img src={decorSvg} alt="" />
          </div>
        </div>

        {/* Picking content — fades with the main morph. */}
        <motion.div
          className={styles.content}
          initial={false}
          animate={{ opacity: view === 'picking' ? 1 : 0 }}
          transition={SCREEN_FADE_TRANSITION}
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
        </motion.div>

        {/* Preview content — fades in after the morph. */}
        <motion.div
          className={styles.content}
          initial={false}
          animate={view === 'preview' ? 'visible' : 'hidden'}
          variants={SHRUNKEN_TEXT_VARIANTS}
        >
          <p className={styles.previewing}>previewing</p>
        </motion.div>

        {/* Post-install content — fades in after the morph. */}
        <motion.div
          className={styles.content}
          initial={false}
          animate={view === 'post-install' ? 'visible' : 'hidden'}
          variants={SHRUNKEN_TEXT_VARIANTS}
        >
          <p className={`${styles.installText} ${styles.installRice}`}>rice</p>
          <p className={`${styles.installText} ${styles.installInstalled}`}>installed</p>
          <p className={`${styles.installText} ${styles.installBang}`}>!</p>
        </motion.div>
      </div>
    </motion.div>
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
