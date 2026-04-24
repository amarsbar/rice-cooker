import { motion } from 'framer-motion';
import styles from './ScreenContent.module.css';
import { POSITIONS, SCREEN_FADE_TRANSITION, useView } from '../view';
import { CardHeader } from './CardHeader';
import { RiceList } from './RiceList';

/** Half of the card's shrink delta between picking and the shrunken view.
 *  Translating the screen content by this amount keeps it visually
 *  centered inside the card as it shrinks around the content. */
const CENTER_OFFSET_X =
  -(POSITIONS.picking.card.width - POSITIONS['post-install'].card.width) / 2;
const CENTER_OFFSET_Y =
  -(POSITIONS.picking.card.height - POSITIONS['post-install'].card.height) / 2;

/** Picking-state card content — header on top, scrollable rice list below.
 *  Fades to 0 when the card morphs to preview/post-install and translates
 *  up/left so the visible region stays centered in the shrinking card. */
export function ScreenContent() {
  const view = useView();
  const shrunken = view !== 'picking';
  return (
    <motion.div
      className={styles.screen}
      initial={false}
      animate={{
        opacity: shrunken ? 0 : 1,
        x: shrunken ? CENTER_OFFSET_X : 0,
        y: shrunken ? CENTER_OFFSET_Y : 0,
      }}
      transition={SCREEN_FADE_TRANSITION}
    >
      <CardHeader />
      <RiceList />
    </motion.div>
  );
}
