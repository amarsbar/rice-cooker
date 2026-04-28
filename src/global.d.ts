import type { BackendEvent, BackendRunRequest, BackendRunResult, RiceListRow } from './shared/backend';

/** Preload-exposed IPC surface. Shape is duplicated here so the web tsconfig
 *  doesn't pull the preload file (and transitively any node-only types) into
 *  the renderer program. Keep in sync with electron/preload/index.ts. */
declare global {
  interface Window {
    rice: {
      closeWindow(): void;
      backend: {
        list(): Promise<RiceListRow[]>;
        run(request: BackendRunRequest): Promise<BackendRunResult>;
        onEvent(callback: (event: BackendEvent) => void): () => void;
      };
    };
  }
}

export {};
