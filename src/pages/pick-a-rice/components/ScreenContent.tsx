import { motion } from 'framer-motion';
import styles from './ScreenContent.module.css';
import { POSITIONS, SCREEN_FADE_TRANSITION, useView } from '../view';
import { CardHeader } from './CardHeader';
import { CardPreviews } from './CardPreviews';

/** Half of the card's shrink delta between picking and preview. Translating
 *  the screen content by this amount keeps it visually centered inside the
 *  card as it shrinks around the content. */
const CENTER_OFFSET_X = -(POSITIONS.picking.card.width - POSITIONS.preview.card.width) / 2;
const CENTER_OFFSET_Y = -(POSITIONS.picking.card.height - POSITIONS.preview.card.height) / 2;

/** The screen's content while picking a rice — header, previews, dashed frame.
 *  Fades to 0 in preview mode and simultaneously translates up/left so the
 *  content stays centered in the card as the card shrinks around it. */
export function ScreenContent() {
  const view = useView();
  const isPreview = view === 'preview';
  return (
    <motion.div
      className={styles.screen}
      initial={false}
      animate={{
        opacity: isPreview ? 0 : 1,
        x: isPreview ? CENTER_OFFSET_X : 0,
        y: isPreview ? CENTER_OFFSET_Y : 0,
      }}
      transition={SCREEN_FADE_TRANSITION}
    >
      <CardHeader step={1} total={6} />
      <CardPreviews />
    </motion.div>
  );
}
