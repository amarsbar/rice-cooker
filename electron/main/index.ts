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

function createWindow(): void {
  const win = new BrowserWindow({
    width: 615,
    height: 598,
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

  win.once('ready-to-show', () => win.show());

  const captureOut = process.env['RICE_CAPTURE_OUT'];
  if (captureOut) {
    /** Time for fonts + large images to load and the initial layout to paint. */
    const INITIAL_SETTLE_MS = 1500;
    /** Covers the full picking → preview sequence: 500ms morph + ~1s radial
     *  cascade + buffer. Should stay >= the sum of constants in view.tsx. */
    const ANIMATION_SETTLE_MS = 1800;

    win.webContents.once('did-finish-load', async () => {
      const { writeFile } = await import('node:fs/promises');
      await new Promise((r) => setTimeout(r, INITIAL_SETTLE_MS));
      const pickingImg = await win.webContents.capturePage();
      await writeFile(captureOut.replace(/\.png$/, '') + '-picking.png', pickingImg.toPNG());

      if (process.env['RICE_CAPTURE_BOTH']) {
        const clicked = await win.webContents.executeJavaScript(
          '(() => { const el = document.querySelector("[class*=stage]"); if (!el) return false; el.click(); return true; })()'
        );
        if (!clicked) {
          console.warn('[rice-cooker] capture: stage element not found, skipping preview shot');
        } else {
          await new Promise((r) => setTimeout(r, ANIMATION_SETTLE_MS));
          const previewImg = await win.webContents.capturePage();
          await writeFile(captureOut.replace(/\.png$/, '') + '-preview.png', previewImg.toPNG());
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
