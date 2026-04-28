import applyRiceSoundUrl from '@/assets/sounds/apply-rice.wav?url';
import enterMenuSoundUrl from '@/assets/sounds/enter-menu.wav?url';
import exitMenuSoundUrl from '@/assets/sounds/exit-menu.wav?url';
import forcedBootSoundUrl from '@/assets/sounds/forced-entry.mp3?url';
import moveDownSoundUrl from '@/assets/sounds/move-down.wav?url';
import moveUpSoundUrl from '@/assets/sounds/move-up.wav?url';
import revertSoundUrl from '@/assets/sounds/revert.wav?url';
import soundToggleSoundUrl from '@/assets/sounds/sound-toggle.wav?url';
import themeClickSoundUrl from '@/assets/sounds/theme-click.wav?url';

type RiceSound =
  | 'applyRice'
  | 'enterMenu'
  | 'exitMenu'
  | 'forcedBoot'
  | 'moveDown'
  | 'moveUp'
  | 'revert'
  | 'soundToggle'
  | 'themeClick';

const SOUND_URLS: Record<RiceSound, string> = {
  applyRice: applyRiceSoundUrl,
  enterMenu: enterMenuSoundUrl,
  exitMenu: exitMenuSoundUrl,
  forcedBoot: forcedBootSoundUrl,
  moveDown: moveDownSoundUrl,
  moveUp: moveUpSoundUrl,
  revert: revertSoundUrl,
  soundToggle: soundToggleSoundUrl,
  themeClick: themeClickSoundUrl,
};
const SOUND_VOLUME = 0.15;

const audioBySound = new Map<RiceSound, HTMLAudioElement>();
let soundEnabled = true;

function getAudio(sound: RiceSound) {
  const existing = audioBySound.get(sound);
  if (existing) return existing;

  const audio = new Audio(SOUND_URLS[sound]);
  audio.preload = 'auto';
  audio.volume = SOUND_VOLUME;
  audioBySound.set(sound, audio);
  return audio;
}

export function playRiceSound(sound: RiceSound) {
  if (typeof Audio === 'undefined') return;
  if (!soundEnabled && sound !== 'soundToggle') return;

  const audio = getAudio(sound);
  audio.currentTime = 0;
  void audio.play().catch(() => undefined);
}

export function setRiceSoundEnabled(enabled: boolean) {
  soundEnabled = enabled;
}
