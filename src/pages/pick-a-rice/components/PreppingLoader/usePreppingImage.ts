import { PREPPING, PILL_SIZE, PILL_OFFSETS, LIME, BROWN, BITMAP_WIDTH, BITMAP_HEIGHT } from './contract';

let cachedUrl: string | null = null;

function buildSvg(): string {
  const parts: string[] = [];
  parts.push(
    `<svg xmlns="http://www.w3.org/2000/svg" width="${BITMAP_WIDTH}" height="${BITMAP_HEIGHT}" viewBox="0 0 ${BITMAP_WIDTH} ${BITMAP_HEIGHT}">`,
  );
  parts.push(`<style>text { font-family: Capriola, Inter, sans-serif; font-size: 22.722px; }</style>`);
  const r = PILL_SIZE / 2;
  for (let i = 0; i < PREPPING.length; i++) {
    const cx = PILL_OFFSETS[i] + r;
    const cy = PILL_SIZE / 2;
    parts.push(`<circle cx="${cx}" cy="${cy}" r="${r}" fill="${LIME}"/>`);
    parts.push(
      `<text x="${cx}" y="${cy + 7.8}" fill="${BROWN}" text-anchor="middle">${PREPPING[i]}</text>`,
    );
  }
  parts.push(`</svg>`);
  return parts.join('');
}

export function getPreppingSvgUrl(): string {
  if (!cachedUrl) {
    cachedUrl = `data:image/svg+xml;charset=utf-8,${encodeURIComponent(buildSvg())}`;
  }
  return cachedUrl;
}
