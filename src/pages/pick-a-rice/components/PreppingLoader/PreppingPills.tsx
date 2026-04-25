import type { CSSProperties } from 'react';
import { PREPPING, PILL_SIZE, PILL_OFFSETS, LIME, BROWN, BITMAP_WIDTH, BITMAP_HEIGHT } from './contract';

interface Props {
  style?: CSSProperties;
  className?: string;
}

/** 8 overlapping lime pills rendering "PREPPING". Figma 367:11772..11786. */
export function PreppingPills({ style, className }: Props) {
  return (
    <div
      className={className}
      style={{
        width: BITMAP_WIDTH,
        height: BITMAP_HEIGHT,
        position: 'relative',
        ...style,
      }}
    >
      {PREPPING.split('').map((letter, i) => (
        <div
          key={i}
          style={{
            position: 'absolute',
            left: PILL_OFFSETS[i],
            top: 0,
            width: PILL_SIZE,
            height: PILL_SIZE,
            borderRadius: PILL_SIZE,
            background: LIME,
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            color: BROWN,
            fontFamily: 'Capriola, Inter, sans-serif',
            fontSize: 22.722,
            lineHeight: 1,
          }}
        >
          {letter}
        </div>
      ))}
    </div>
  );
}
