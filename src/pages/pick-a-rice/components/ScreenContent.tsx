import { motion } from 'framer-motion';
import styles from './ScreenContent.module.css';
import { POSITIONS, SCREEN_FADE_TRANSITION, useView } from '../view';
import { CardHeader } from './CardHeader';
import { RiceList, type RiceNavRequest } from './RiceList';
import type { PhysicalControl } from './PhysicalControls';
import type { RiceListRow } from '@/shared/backend';

const CENTER_OFFSET_X =
  -(POSITIONS.picking.card.width - POSITIONS.preview.card.width) / 2;
const CENTER_OFFSET_Y =
  -(POSITIONS.picking.card.height - POSITIONS.preview.card.height) / 2;

/** Picking-state card content: header on top, focused rice preview below.
 *  Fades + translates to stay centered while the card shrinks. */
interface ScreenContentProps {
  rices: RiceListRow[];
  holdDirection: -1 | 0 | 1;
  navRequest: RiceNavRequest;
  pressedControls: ReadonlySet<PhysicalControl>;
  onScrollOffsetChange: (offset: number) => void;
}

export function ScreenContent({
  rices,
  holdDirection,
  navRequest,
  pressedControls,
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
      <CardHeader pressedControls={pressedControls} />
      <RiceList
        active={!shrunken}
        rices={rices}
        holdDirection={holdDirection}
        navRequest={navRequest}
        onScrollOffsetChange={onScrollOffsetChange}
      />
    </motion.div>
  );
}
