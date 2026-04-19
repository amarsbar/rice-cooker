import { motion } from 'framer-motion';
import styles from './CreatorBadge.module.css';
import cloudSvg from '@/assets/figma/creator-cloud.svg';
import decorSvg from '@/assets/figma/creator-decor.svg';
import sparkle1Svg from '@/assets/figma/sparkle-1.svg';
import sparkle2Svg from '@/assets/figma/sparkle-2.svg';
import flower1Svg from '@/assets/figma/flower-1.svg';
import flower2Svg from '@/assets/figma/flower-2.svg';
import {
  MORPH_TRANSITION,
  POSITIONS,
  PREVIEW_DIM_TEXT_VARIANTS,
  SCREEN_FADE_TRANSITION,
  useView,
} from '../view';

interface CreatorBadgeProps {
  name: string;
  creator: string;
}

interface DecorIconProps {
  outer: string;
  rotated: string;
  inner: string;
  bleed: string;
  src: string;
}

/** Matches the Figma nested structure for each decorative icon:
 *  outer (flex center) → rotated container → inner (sized) → bleed (inset) → img. */
function DecorIcon({ outer, rotated, inner, bleed, src }: DecorIconProps) {
  return (
    <div className={outer}>
      <div className={rotated}>
        <div className={inner}>
          <div className={bleed}>
            <img src={src} alt="" />
          </div>
        </div>
      </div>
    </div>
  );
}

/** Figma node 168:7829 / 168:7886 — cream circle with cloud halo showing the
 *  selected rice's name and creator. In preview mode the pill trio
 *  crossfades to a dimmed "Preview state" label (168:7943). */
export function CreatorBadge({ name, creator }: CreatorBadgeProps) {
  const view = useView();

  return (
    <>
      {/* Cloud halo (168:7830) */}
      <motion.div
        className={styles.cloud}
        initial={false}
        animate={POSITIONS[view].creatorCloud}
        transition={MORPH_TRANSITION}
      >
        <img src={cloudSvg} alt="" />
      </motion.div>

      {/* Cream inner circle (168:7867 / 168:7924) */}
      <motion.div
        className={styles.inner}
        initial={false}
        animate={POSITIONS[view].creatorInner}
        transition={MORPH_TRANSITION}
      >
        <div className={styles.decor}>
          <img src={decorSvg} alt="" />
        </div>

        {/* Sparkles + flowers — always visible in both states */}
        <DecorIcon
          outer={styles.sparkle1}
          rotated={styles.rot1}
          inner={styles.sparkle1Inner}
          bleed={styles.sparkle1Bleed}
          src={sparkle1Svg}
        />
        <DecorIcon
          outer={styles.sparkle2}
          rotated={styles.rot2}
          inner={styles.sparkle2Inner}
          bleed={styles.sparkle2Bleed}
          src={sparkle2Svg}
        />
        <DecorIcon
          outer={styles.flower1}
          rotated={styles.rot3}
          inner={styles.flower1Inner}
          bleed={styles.flower1Bleed}
          src={flower1Svg}
        />
        <DecorIcon
          outer={styles.flower2}
          rotated={styles.rot4}
          inner={styles.flower2Inner}
          bleed={styles.flower2Bleed}
          src={flower2Svg}
        />

        {/* Picking state: Ricename + By + creatorname pills */}
        <motion.div
          className={styles.pills}
          initial={false}
          animate={{ opacity: view === 'picking' ? 1 : 0 }}
          transition={SCREEN_FADE_TRANSITION}
        >
          <div className={`${styles.pill} ${styles.pillName}`}>
            <p>{name}</p>
          </div>
          <div className={`${styles.pill} ${styles.pillBy}`}>
            <p>By</p>
          </div>
          <div className={`${styles.pill} ${styles.pillCreator}`}>
            <p>{creator}</p>
          </div>
        </motion.div>

        {/* Preview state label (168:7943) — dimmed black, centered */}
        <motion.p
          className={styles.previewLabel}
          initial={false}
          animate={view === 'preview' ? 'visible' : 'hidden'}
          variants={PREVIEW_DIM_TEXT_VARIANTS}
        >
          Preview state
        </motion.p>
      </motion.div>
    </>
  );
}
