import { app, BrowserWindow, ipcMain, shell } from 'electron';
import { execFile, spawn, type ChildProcessWithoutNullStreams } from 'node:child_process';
import { existsSync } from 'node:fs';
import { promisify } from 'node:util';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import type {
  BackendRunRequest,
  BackendRunResult,
  RiceListRow,
} from '../../src/shared/backend';

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
const RAW_TAIL_LIMIT = 100;
let activeBackendChild: ChildProcessWithoutNullStreams | null = null;

function backendBin(): string {
  const override = process.env['RICE_COOKER_BACKEND'];
  if (override) return override;
  if (app.isPackaged) return 'rice-cooker-backend';

  for (const candidate of [
    join(process.cwd(), 'backend/target/debug/rice-cooker-backend'),
    join(process.cwd(), 'backend/target/release/rice-cooker-backend'),
  ]) {
    if (existsSync(candidate)) return candidate;
  }
  return 'rice-cooker-backend';
}

async function backendList(): Promise<RiceListRow[]> {
  const { stdout } = await execFileAsync(backendBin(), ['list'], {
    maxBuffer: 1024 * 1024,
  });
  return JSON.parse(stdout) as RiceListRow[];
}

function parseBackendEvent(line: string): BackendRunResult['events'][number] | null {
  try {
    const value = JSON.parse(line) as unknown;
    if (value && typeof value === 'object' && 'type' in value) {
      return value as BackendRunResult['events'][number];
    }
  } catch {
    return null;
  }
  return null;
}

function pushRawTail(rawTail: string[], line: string): void {
  if (!line.trim()) return;
  rawTail.push(line);
  if (rawTail.length > RAW_TAIL_LIMIT) rawTail.splice(0, rawTail.length - RAW_TAIL_LIMIT);
}

function backendArgs(request: BackendRunRequest): string[] {
  if (request.command === 'uninstall') return ['uninstall'];
  if (!request.name) throw new Error(`${request.command} requires a rice name`);
  return [request.command, request.name];
}

function runBackend(request: BackendRunRequest): Promise<BackendRunResult> {
  if (activeBackendChild) throw new Error('a backend command is already running');

  const events: BackendRunResult['events'] = [];
  const rawTail: string[] = [];
  const child = spawn(backendBin(), backendArgs(request), {
    cwd: process.cwd(),
    env: process.env,
  });
  activeBackendChild = child;

  let stdoutBuffer = '';
  let stderrBuffer = '';
  const consume = (chunk: Buffer, isStdout: boolean) => {
    let buffer = (isStdout ? stdoutBuffer : stderrBuffer) + chunk.toString('utf8');
    let newline = buffer.indexOf('\n');
    while (newline !== -1) {
      const line = buffer.slice(0, newline).trimEnd();
      buffer = buffer.slice(newline + 1);
      const event = parseBackendEvent(line);
      if (event) {
        events.push(event);
      } else {
        pushRawTail(rawTail, line);
      }
      newline = buffer.indexOf('\n');
    }
    if (isStdout) stdoutBuffer = buffer;
    else stderrBuffer = buffer;
  };

  child.stdout.on('data', (chunk: Buffer) => consume(chunk, true));
  child.stderr.on('data', (chunk: Buffer) => consume(chunk, false));

  return new Promise((resolve, reject) => {
    child.on('error', (err) => {
      activeBackendChild = null;
      reject(err);
    });
    child.on('close', (exitCode) => {
      if (stdoutBuffer) pushRawTail(rawTail, stdoutBuffer);
      if (stderrBuffer) pushRawTail(rawTail, stderrBuffer);
      activeBackendChild = null;
      const failed = events.some((event) => event.type === 'fail');
      const succeeded = events.some((event) => event.type === 'success');
      resolve({ ok: exitCode === 0 && succeeded && !failed, events, rawTail, exitCode });
    });
  });
}

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
    const pressKey = (key: string) => {
      const keyLiteral = JSON.stringify(key);
      return win.webContents.executeJavaScript(
        `(() => new Promise((resolve) => {
          const el = document.querySelector('[data-preview-option]');
          if (!el) {
            resolve(false);
            return;
          }
          const before = el.getAttribute('data-preview-option');
          window.dispatchEvent(new KeyboardEvent('keydown', { key: ${keyLiteral}, cancelable: true }));
          window.dispatchEvent(new KeyboardEvent('keyup', { key: ${keyLiteral}, cancelable: true }));
          requestAnimationFrame(() => resolve(before !== el.getAttribute('data-preview-option')));
        }))()`,
      );
    };

    win.webContents.once('did-finish-load', async () => {
      try {
        const { writeFile } = await import('node:fs/promises');
        const base = captureOut.replace(/\.png$/, '');
        const wait = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));
        await wait(INITIAL_SETTLE_MS);

        const pickingImg = await win.webContents.capturePage();
        await writeFile(`${base}-picking.png`, pickingImg.toPNG());

        if (process.env['RICE_CAPTURE_ALL']) {
          for (const name of ['preview'] as const) {
            const clicked = await clickStageToNext();
            if (!clicked) throw new Error('capture: stage element not found');
            await wait(MORPH_SETTLE_MS);
            const img = await win.webContents.capturePage();
            await writeFile(`${base}-${name}.png`, img.toPNG());
          }
          if (process.env['RICE_CAPTURE_PREVIEW_OPTIONS']) {
            for (const name of ['dots', 'leave'] as const) {
              if (!(await pressKey('ArrowDown'))) throw new Error('capture: key dispatch failed');
              await wait(MORPH_SETTLE_MS);
              const img = await win.webContents.capturePage();
              await writeFile(`${base}-preview-${name}.png`, img.toPNG());
            }
          }
        }

        if (process.env['RICE_CAPTURE_THEMES']) {
          // Cycle back to picking and click the theme knob (sprout) to
          // advance through t2 → t1 → t3 → t2 via a dedicated capture hook.
          const clickKnob = () =>
            win.webContents.executeJavaScript(
              `(() => {
                 const el = document.querySelector('[data-capture-theme-knob]');
                 if (!el) return false;
                 el.click();
                 return true;
               })()`,
            );
          while (captureView !== 'picking') {
            const clicked = await clickStageToNext();
            if (!clicked) throw new Error('capture: stage element not found');
            await wait(MORPH_SETTLE_MS);
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
              if (!clicked) throw new Error('capture: knob not found');
              await wait(MORPH_SETTLE_MS);
            }
            const img = await win.webContents.capturePage();
            await writeFile(`${base}-theme-${label}.png`, img.toPNG());
          }
        }

        app.exit(0);
      } catch (err) {
        console.error('[rice-cooker] capture failed:', err);
        app.exit(1);
      }
    });
  }

  win.webContents.setWindowOpenHandler(({ url }) => {
    // Only hand off http(s) URLs to the OS handler. Any other scheme
    // (file:, javascript:, chrome:, data:, etc.) could be abused to reach
    // outside the app's trust boundary, so refuse outright.
    try {
      const { origin, pathname, protocol } = new URL(url);
      if (protocol === 'http:' || protocol === 'https:') {
        const safeUrl = `${origin}${pathname}`;
        void shell.openExternal(url).catch((err) => {
          console.warn('[rice-cooker] openExternal failed:', safeUrl, err);
        });
      }
    } catch {
      return { action: 'deny' };
    }
    return { action: 'deny' };
  });

  const devUrl = process.env['ELECTRON_RENDERER_URL'];
  const failLoad = (kind: 'loadURL' | 'loadFile', err: unknown) => {
    console.error(`[rice-cooker] ${kind} failed:`, err);
    app.exit(1);
  };
  if (devUrl) {
    void win.loadURL(devUrl).catch((err) => failLoad('loadURL', err));
  } else {
    void win.loadFile(join(__dirname, '../renderer/index.html')).catch((err) => failLoad('loadFile', err));
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

ipcMain.handle('backend:list', () => backendList());

ipcMain.handle('backend:run', (_event, request: BackendRunRequest) => runBackend(request));

app.whenReady()
  .then(async () => {
    await injectCompositorRules();
    createWindow();
    app.on('activate', () => {
      if (BrowserWindow.getAllWindows().length === 0) createWindow();
    });
  })
  .catch((err) => {
    console.error('[rice-cooker] startup failed:', err);
    app.exit(1);
  });

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') app.quit();
});
