import { useEffect, useRef } from 'react';
import { useTheme } from '../view';

/** Renders `children` into a DPR-scaled canvas with a round-joined stroke
 *  painted underneath the fill. Reads font + ink from the span's computed
 *  style; outline from `--outline-color` / `--outline-width` custom props.
 *  Re-paints on theme change because the outline colour is driven by
 *  per-theme tokens. */
export function OutlinedText({
  children,
  className,
}: {
  children: string;
  className?: string;
}) {
  const ref = useRef<HTMLSpanElement>(null);
  const { theme } = useTheme();

  useEffect(() => {
    let cancelled = false;
    (async () => {
      await document.fonts.ready;
      const el = ref.current;
      if (cancelled || !el) return;
      const cs = getComputedStyle(el);
      const stroke = parseFloat(cs.getPropertyValue('--outline-width'));
      if (!stroke) return;
      const canvas = paint(children, {
        font: `${cs.fontWeight} ${cs.fontSize} ${cs.fontFamily}`,
        ink: cs.color,
        outline: cs.getPropertyValue('--outline-color').trim() || cs.color,
        stroke,
      });
      el.replaceChildren(canvas);
    })();
    return () => {
      cancelled = true;
    };
  }, [children, theme]);

  return <span ref={ref} className={className} aria-label={children} />;
}

function paint(
  text: string,
  { font, ink, outline, stroke }: { font: string; ink: string; outline: string; stroke: number },
): HTMLCanvasElement {
  const m = document.createElement('canvas').getContext('2d')!;
  m.font = font;
  const metrics = m.measureText(text);
  const size = parseFloat(font.match(/(\d+(?:\.\d+)?)px/)?.[1] ?? '16');
  const asc = metrics.actualBoundingBoxAscent || size * 0.85;
  const desc = metrics.actualBoundingBoxDescent || size * 0.25;
  const w = Math.ceil(metrics.width + stroke * 2);
  const h = Math.ceil(asc + desc + stroke * 2);
  const dpr = window.devicePixelRatio || 1;
  const c = document.createElement('canvas');
  c.width = w * dpr;
  c.height = h * dpr;
  c.style.width = `${w}px`;
  c.style.height = `${h}px`;
  c.style.display = 'block';
  c.style.margin = `-${stroke}px`;
  const ctx = c.getContext('2d')!;
  ctx.scale(dpr, dpr);
  ctx.font = font;
  ctx.lineJoin = 'round';
  ctx.lineCap = 'round';
  ctx.lineWidth = stroke * 2;
  ctx.strokeStyle = outline;
  ctx.fillStyle = ink;
  ctx.strokeText(text, stroke, asc + stroke);
  ctx.fillText(text, stroke, asc + stroke);
  return c;
}
