import placeholderRice from '@/assets/rices/placeholder-rice.webp';
import caelestiaScreenshot from '@/assets/rices/caelestia.png';
import dmsScreenshot from '@/assets/rices/dankmaterialshell.png';
import linuxRetroismScreenshot from '@/assets/rices/linux-retroism.png';
import nandoroidScreenshot from '@/assets/rices/nandoroid.png';
import noctaliaScreenshot from '@/assets/rices/noctalia.png';
import whiskerScreenshot from '@/assets/rices/whisker.png';
import zephyrScreenshot from '@/assets/rices/zephyr.png';

const SCREENSHOTS: Record<string, string> = {
  caelestia: caelestiaScreenshot,
  dms: dmsScreenshot,
  'linux-retroism': linuxRetroismScreenshot,
  nandoroid: nandoroidScreenshot,
  noctalia: noctaliaScreenshot,
  whisker: whiskerScreenshot,
  zephyr: zephyrScreenshot,
};

export function getRiceScreenshot(name: string | undefined) {
  return name ? SCREENSHOTS[name] ?? placeholderRice : placeholderRice;
}
