import fail from '@/assets/fail.svg';
import styles from './FailureOverlay.module.css';

export const FAILURE_OVERLAY_MS = 2000;

export function FailureOverlay() {
  return (
    <div className={styles.wrap} aria-hidden="true">
      <img src={fail} alt="" className={styles.graphic} />
    </div>
  );
}
