import { app, BrowserWindow, ipcMain, shell } from 'electron';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const __dirname = dirname(fileURLToPath(import.meta.url));
const execFileAsync = promisify(execFile);

/** Identifiers compositor rules match against. The Wayland app_id Chromium
 *  advertises varies: "Electron" under a direct `electron` launch,
 *  "rice-cooker" under electron-vite dev (which spawns with a different
 *  argv[0]). We match either class plus the stable title to hit both cases. */
const APP_CLASS_REGEX = '^(Electron|rice-cooker)$';
const APP_TITLE = 'Rice Cooker';
const APP_TITLE_REGEX = '^(Rice Cooker)$';

app.setName('rice-cooker');

if (process.env['XDG_SESSION_TYPE'] === 'wayland') {
  app.commandLine.appendSwitch('ozone-platform', 'wayland');
  app.commandLine.appendSwitch('enable-features', 'UseOzonePlatform');
} else {
  app.commandLine.appendSwitch('ozone-platform-hint', 'auto');
}

app.commandLine.appendSwitch('enable-transparent-visuals');

/** Inject runtime windowrules so our transparent pixels aren't muddied by the
 *  compositor's blur/shadow/rounding, regardless of user config. Matching on
 *  class+title narrows to our own window; rules persist for the current
 *  compositor session and affect no other apps.
 *
 *  Note: Hyprland's CLI has no way to remove a specific runtime windowrule.
 *  Rules accumulate across dev reloads until `hyprctl reload` is run (which
 *  would nuke every runtime rule on the system, not just ours). Since each
 *  duplicate targets the same match, duplicates are functionally a no-op —
 *  only harmless rule-list bloat in the compositor session. */
async function injectCompositorRules(): Promise<void> {
  // Niri has no compositor blur/shadow and no runtime IPC for window rules,
  // so nothing to inject there; transparent pixels already pass through clean.
  if (!process.env['HYPRLAND_INSTANCE_SIGNATURE']) return;

  const match = `match:class ${APP_CLASS_REGEX}, match:title ${APP_TITLE_REGEX}`;
  const rules = [
    `no_blur on, ${match}`,
    `no_shadow on, ${match}`,
    `rounding 0, ${match}`,
    `border_size 0, ${match}`,
  ];
  for (const rule of rules) {
    try {
      await execFileAsync('hyprctl', ['keyword', 'windowrule', rule]);
    } catch (err) {
      console.warn('[rice-cooker] hyprctl windowrule failed:', rule, err);
    }
  }
}

/** UI scale factor - design is 666 x 574 @ 1x; multiplied for readability on
 *  high-res monitors. Applied both to window size and via zoomFactor so
 *  every pixel-positioned element scales uniformly. Override with
 *  RICE_SCALE env var (e.g. `RICE_SCALE=2 npm run dev`). */
const SCALE = Number(process.env['RICE_SCALE']) || 1.75;

function createWindow(): void {
  const win = new BrowserWindow({
    width: Math.round(666 * SCALE),
    height: Math.round(574 * SCALE),
    title: APP_TITLE,
    transparent: true,
    frame: false,
    hasShadow: false,
    resizable: false,
    maximizable: false,
    fullscreenable: false,
    backgroundColor: '#00000000',
    show: false,
    webPreferences: {
      preload: join(__dirname, '../preload/index.cjs'),
      nodeIntegration: false,
      contextIsolation: true,
      sandbox: true,
    },
  });

  win.webContents.on('did-finish-load', () => win.webContents.setZoomFactor(SCALE));
  win.once('ready-to-show', () => win.show());

  const captureOut = process.env['RICE_CAPTURE_OUT'];
  if (captureOut) {
    /** Time for fonts + large images to load and the initial layout to paint. */
    const INITIAL_SETTLE_MS = 1500;
    /** Time for one card-morph + content-crossfade pass to finish. */
    const MORPH_SETTLE_MS = 900;

    const clickStage = () =>
      win.webContents.executeJavaScript(
        '(() => { const el = document.querySelector("[class*=stage]"); if (!el) return false; el.click(); return true; })()',
      );
    type CaptureView = 'picking' | 'preview';
    let captureView: CaptureView = 'picking';
    const clickStageToNext = async () => {
      const clicked = await clickStage();
      if (clicked) {
        captureView = captureView === 'picking' ? 'preview' : 'picking';
      }
      return clicked;
    };
    const pressKey = (key: string) =>
      win.webContents.executeJavaScript(
        `(() => {
          window.dispatchEvent(new KeyboardEvent('keydown', { key: '${key}' }));
          window.dispatchEvent(new KeyboardEvent('keyup', { key: '${key}' }));
        })()`,
      );

    win.webContents.once('did-finish-load', async () => {
      const { writeFile } = await import('node:fs/promises');
      const base = captureOut.replace(/\.png$/, '');
      await new Promise((r) => setTimeout(r, INITIAL_SETTLE_MS));

      const pickingImg = await win.webContents.capturePage();
      await writeFile(`${base}-picking.png`, pickingImg.toPNG());

      if (process.env['RICE_CAPTURE_ALL']) {
        for (const name of ['preview'] as const) {
          const clicked = await clickStageToNext();
          if (!clicked) {
            console.warn('[rice-cooker] capture: stage element not found');
            break;
          }
          await new Promise((r) => setTimeout(r, MORPH_SETTLE_MS));
          const img = await win.webContents.capturePage();
          await writeFile(`${base}-${name}.png`, img.toPNG());
        }
        if (process.env['RICE_CAPTURE_PREVIEW_OPTIONS']) {
          for (const name of ['dots', 'leave'] as const) {
            await pressKey('ArrowDown');
            await new Promise((r) => setTimeout(r, MORPH_SETTLE_MS));
            const img = await win.webContents.capturePage();
            await writeFile(`${base}-preview-${name}.png`, img.toPNG());
          }
        }
      }

      if (process.env['RICE_CAPTURE_THEMES']) {
        // Cycle back to picking and click the theme knob (sprout) to
        // advance through t2 → t1 → t3 → t2. Uses a transient dispatch
        // via document.querySelector to find the knob click target.
        const clickKnob = () =>
          win.webContents.executeJavaScript(
            `(() => {
               const el = document.querySelector('[style*="cursor"][style*="pointer"]');
               if (!el) return false;
               el.click();
               return true;
             })()`,
          );
        while (captureView !== 'picking') {
          const clicked = await clickStageToNext();
          if (!clicked) {
            console.warn('[rice-cooker] capture: stage element not found');
            return;
          }
          await new Promise((r) => setTimeout(r, MORPH_SETTLE_MS));
        }
        // The cycle is t2 (0) → t1 (1) → t2 (2) → t3 (3). Walk
        // sequentially from the initial t2, capturing t1 after 1 click
        // and t3 after 2 further clicks.
        const steps: { label: 't1' | 't3'; advance: number }[] = [
          { label: 't1', advance: 1 },
          { label: 't3', advance: 2 },
        ];
        for (const { label, advance } of steps) {
          for (let i = 0; i < advance; i++) {
            const clicked = await clickKnob();
            if (!clicked) {
              console.warn('[rice-cooker] capture: knob not found');
              return;
            }
            await new Promise((r) => setTimeout(r, MORPH_SETTLE_MS));
          }
          const img = await win.webContents.capturePage();
          await writeFile(`${base}-theme-${label}.png`, img.toPNG());
        }
      }

      app.exit(0);
    });
  }

  win.webContents.setWindowOpenHandler(({ url }) => {
    // Only hand off http(s) URLs to the OS handler. Any other scheme
    // (file:, javascript:, chrome:, data:, etc.) could be abused to reach
    // outside the app's trust boundary, so refuse outright.
    try {
      const { protocol } = new URL(url);
      if (protocol === 'http:' || protocol === 'https:') {
        shell.openExternal(url);
      }
    } catch {
      // Malformed URL — just deny.
    }
    return { action: 'deny' };
  });

  const devUrl = process.env['ELECTRON_RENDERER_URL'];
  if (devUrl) {
    win.loadURL(devUrl);
  } else {
    win.loadFile(join(__dirname, '../renderer/index.html'));
  }
}

ipcMain.on('window:close', (event) => {
  // Only accept the message from the main frame of a BrowserWindow we own,
  // and only if that frame is serving our own renderer bundle. This blocks
  // any future sub-frame, guest page, or redirected document from closing
  // the window via our IPC channel.
  const frame = event.senderFrame;
  if (!frame || frame.parent !== null) return;
  const win = BrowserWindow.fromWebContents(event.sender);
  if (!win) return;
  const devUrl = process.env['ELECTRON_RENDERER_URL'];
  const fromOwnRenderer = devUrl
    ? frame.url.startsWith(devUrl)
    : frame.url.startsWith('file://');
  if (!fromOwnRenderer) return;
  win.close();
});

app.whenReady().then(async () => {
  await injectCompositorRules();
  createWindow();
  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) createWindow();
  });
});

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') app.quit();
});
