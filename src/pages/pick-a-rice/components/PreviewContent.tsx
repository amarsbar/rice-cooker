import { useEffect, useRef, useState } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import styles from './PreviewContent.module.css';
import BackBtnSvg from '@/assets/figma/btn-back.svg?react';
import GithubBtnSvg from '@/assets/figma/btn-github.svg?react';
import { SHRUNKEN_TEXT_VARIANTS, useView } from '../view';
import { PreppingLoader } from './PreppingLoader';

interface PreviewContentProps {
  themeName: string;
  creatorName: string;
}

/** Figma 350:7160 children — preview mode shown before the user commits.
 *  Back and GitHub buttons on the left, big central APPLY pill, theme
 *  name at top, "by creator name" at bottom. Fades in 150ms after the
 *  card morph completes.
 *
 *  Entering preview briefly shows the PREPPING loader (Figma 367:11763)
 *  before the preview content. For now it fires on every entry with a
 *  fixed 1.25s duration; once the real preview pipeline is wired up this
 *  will hook into actual load progress. */
export function PreviewContent({ themeName, creatorName }: PreviewContentProps) {
  const view = useView();
  const active = view === 'preview';
  const [loading, setLoading] = useState(false);
  const wasActiveRef = useRef(false);

  useEffect(() => {
    if (active && !wasActiveRef.current) setLoading(true);
    wasActiveRef.current = active;
  }, [active]);

  return (
    <motion.div
      className={styles.wrap}
      initial={false}
      animate={active ? 'visible' : 'hidden'}
      variants={SHRUNKEN_TEXT_VARIANTS}
      style={{ pointerEvents: active ? 'auto' : 'none' }}
    >
      <AnimatePresence>
        {loading && active && (
          <motion.div
            className={styles.loaderOverlay}
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: 0.18 }}
          >
            <PreppingLoader
              playing
              durationMs={1250}
              onComplete={() => setLoading(false)}
            />
          </motion.div>
        )}
      </AnimatePresence>

      {/* Preview UI — hidden while the PREPPING loader is up so only the chop
          animation + lime overlay show during the loading phase. */}
      {!loading && (
        <>
          <p className={`${styles.label} ${styles.themeName}`}>{themeName}</p>
          <p className={`${styles.label} ${styles.creatorName}`}>{creatorName}</p>

          <button type="button" className={styles.backBtn} aria-label="Back">
            <BackBtnSvg />
          </button>
          <button type="button" className={styles.githubBtn} aria-label="View on GitHub">
            <GithubBtnSvg />
          </button>

          <div className={styles.applyBtn}>
            <div className={styles.applyCluster}>
              {'APPLY'.split('').map((c, i) => (
                <span key={i} className={styles.applyLetter}>
                  {c}
                </span>
              ))}
            </div>
          </div>
        </>
      )}
    </motion.div>
  );
}
