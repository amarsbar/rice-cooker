import styles from './GreenTab.module.css';

/** Figma node 350:6483 — mint rectangle that protrudes past the card's right
 *  edge, backing the close pin and sound button. Only the top-right corner
 *  is rounded so it visually tucks under the card's rounded-radius edge. */
export function GreenTab() {
  return <div className={styles.tab} />;
}
