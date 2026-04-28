import { app, BrowserWindow, ipcMain, screen, shell, type WebContents } from 'electron';
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

const APP_CLASS_PATTERN = /^(electron|Electron|rice-cooker)$/;
const APP_TITLE = 'Rice Cooker';
const HYPRLAND_WINDOW_EFFECTS = [
  ['no_blur', 'on'],
  ['no_shadow', 'on'],
  ['rounding', '0'],
  ['border_size', '0'],
] as const;

app.setName('rice-cooker');

if (process.env['XDG_SESSION_TYPE'] === 'wayland') {
  app.commandLine.appendSwitch('ozone-platform', 'wayland');
  app.commandLine.appendSwitch('enable-features', 'UseOzonePlatform');
} else {
  app.commandLine.appendSwitch('ozone-platform-hint', 'auto');
}

app.commandLine.appendSwitch('enable-transparent-visuals');

// Live props avoid runtime windowrule accumulation, but Hyprland reloads can drop them.
async function applyHyprlandWindowProps(): Promise<void> {
  if (!process.env['HYPRLAND_INSTANCE_SIGNATURE']) return;

  try {
    const { stdout } = await execFileAsync('hyprctl', ['clients', '-j'], {
      maxBuffer: 1024 * 1024,
    });
    const clients = JSON.parse(stdout) as unknown;
    if (!Array.isArray(clients)) return;

    for (const client of clients) {
      if (!client || typeof client !== 'object') continue;
      const fields = client as { address?: unknown; class?: unknown; title?: unknown };
      if (
        typeof fields.address !== 'string' ||
        typeof fields.class !== 'string' ||
        typeof fields.title !== 'string' ||
        !APP_CLASS_PATTERN.test(fields.class) ||
        fields.title !== APP_TITLE
      ) {
        continue;
      }

      const target = `address:${fields.address}`;
      for (const [prop, value] of HYPRLAND_WINDOW_EFFECTS) {
        try {
          await execFileAsync('hyprctl', ['dispatch', 'setprop', target, prop, value]);
        } catch (err) {
          console.warn(`[rice-cooker] hyprctl setprop ${prop} failed:`, err);
        }
      }
    }
  } catch (err) {
    console.warn('[rice-cooker] hyprctl live window props failed:', err);
  }
}

const BASE_SIZE = { width: 666, height: 574 } as const;
const FIGMA_HEIGHT_RATIO = 600 / 1080;
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
  switch (request.command) {
    case 'uninstall':
      return ['uninstall'];
    case 'install':
    case 'preview':
      if (!request.name) throw new Error(`${request.command} requires a rice name`);
      return [request.command, request.name];
    default:
      throw new Error(`unknown backend command: ${String((request as { command?: unknown }).command)}`);
  }
}

function runBackend(request: BackendRunRequest, sender?: WebContents): Promise<BackendRunResult> {
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
        sender?.send('backend:event', event);
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
      void applyHyprlandWindowProps();
      resolve({ ok: exitCode === 0 && succeeded && !failed, events, rawTail, exitCode });
    });
  });
}

function createWindow(): void {
  const scale =
    Number(process.env['RICE_SCALE']) ||
    Math.max(
      0.95,
      Math.min(
        2.15,
        (screen.getDisplayNearestPoint(screen.getCursorScreenPoint()).workAreaSize.height *
          FIGMA_HEIGHT_RATIO) /
          BASE_SIZE.height,
      ),
    );

  const win = new BrowserWindow({
    width: Math.round(BASE_SIZE.width * scale),
    height: Math.round(BASE_SIZE.height * scale),
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

  win.webContents.on('did-finish-load', () => win.webContents.setZoomFactor(scale));
  win.once('ready-to-show', () => {
    win.show();
    void applyHyprlandWindowProps();
  });

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

ipcMain.handle('backend:run', (event, request: BackendRunRequest) => runBackend(request, event.sender));

app.whenReady()
  .then(() => {
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
