import placeholderRice from '@/assets/rices/placeholder-rice.webp';
import caelestiaScreenshot from '@/assets/rices/caelestia.webp';
import dmsScreenshot from '@/assets/rices/dankmaterialshell.webp';
import linuxRetroismScreenshot from '@/assets/rices/linux-retroism.webp';
import nandoroidScreenshot from '@/assets/rices/nandoroid.webp';
import noctaliaScreenshot from '@/assets/rices/noctalia.webp';
import ryuShellScreenshot from '@/assets/rices/ryu-shell.webp';
import whiskerScreenshot from '@/assets/rices/whisker.webp';
import zephyrScreenshot from '@/assets/rices/zephyr.webp';

const SCREENSHOTS: Record<string, string> = {
  caelestia: caelestiaScreenshot,
  dms: dmsScreenshot,
  'linux-retroism': linuxRetroismScreenshot,
  nandoroid: nandoroidScreenshot,
  noctalia: noctaliaScreenshot,
  'ryu-shell': ryuShellScreenshot,
  whisker: whiskerScreenshot,
  zephyr: zephyrScreenshot,
};

export function getRiceScreenshot(name: string | undefined) {
  return name ? SCREENSHOTS[name] ?? placeholderRice : placeholderRice;
}
