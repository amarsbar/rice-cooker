import { motion } from 'framer-motion';
import { POSITIONS, useView } from '../view';
import styles from './Antenna.module.css';

const ANTENNA = {
  left: POSITIONS.preview.card.left - 16,
  top: POSITIONS.preview.card.top - 87,
} as const;

const EASE = [0.4, 0, 0.2, 1] as const;

export function Antenna() {
  const view = useView();
  if (view === 'picking') return null;

  const active = view === 'downloading';

  return (
    <div className={styles.antenna} style={ANTENNA} aria-hidden="true">
      <motion.span
        className={styles.base}
        initial={{ scaleX: 0 }}
        animate={{ scaleX: active ? 1 : 0 }}
        transition={{
          delay: active ? 0.3 : 0.18,
          duration: 0.18,
          ease: EASE,
        }}
      />
      <motion.span
        className={styles.stem}
        initial={{ scaleY: 0 }}
        animate={{ scaleY: active ? 1 : 0 }}
        transition={{
          delay: active ? 0.45 : 0,
          duration: active ? 0.32 : 0.28,
          ease: EASE,
        }}
      />
    </div>
  );
}
