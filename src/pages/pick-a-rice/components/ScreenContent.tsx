import { motion } from 'framer-motion';
import styles from './ScreenContent.module.css';
import { POSITIONS, SCREEN_FADE_TRANSITION, useView } from '../view';
import { CardHeader } from './CardHeader';
import { RiceList, type RiceNavRequest } from './RiceList';

const CENTER_OFFSET_X =
  -(POSITIONS.picking.card.width - POSITIONS['post-install'].card.width) / 2;
const CENTER_OFFSET_Y =
  -(POSITIONS.picking.card.height - POSITIONS['post-install'].card.height) / 2;

/** Picking-state card content: header on top, focused rice preview below.
 *  Fades + translates to stay centered while the card shrinks. */
interface ScreenContentProps {
  holdDirection: -1 | 0 | 1;
  navRequest: RiceNavRequest;
  onScrollOffsetChange: (offset: number) => void;
}

export function ScreenContent({
  holdDirection,
  navRequest,
  onScrollOffsetChange,
}: ScreenContentProps) {
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
      style={{ pointerEvents: shrunken ? 'none' : 'auto' }}
    >
      <CardHeader />
      <RiceList
        active={!shrunken}
        holdDirection={holdDirection}
        navRequest={navRequest}
        onScrollOffsetChange={onScrollOffsetChange}
      />
    </motion.div>
  );
}
