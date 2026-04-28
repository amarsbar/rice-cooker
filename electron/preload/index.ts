import { contextBridge, ipcRenderer } from 'electron';
import type {
  BackendEvent,
  BackendRunRequest,
  BackendRunResult,
  RiceListRow,
} from '../../src/shared/backend';

const api = {
  closeWindow: () => ipcRenderer.send('window:close'),
  backend: {
    list: () => ipcRenderer.invoke('backend:list') as Promise<RiceListRow[]>,
    run: (request: BackendRunRequest) =>
      ipcRenderer.invoke('backend:run', request) as Promise<BackendRunResult>,
    onEvent: (callback: (event: BackendEvent) => void) => {
      const listener = (_event: Electron.IpcRendererEvent, backendEvent: BackendEvent) => {
        callback(backendEvent);
      };
      ipcRenderer.on('backend:event', listener);
      return () => ipcRenderer.removeListener('backend:event', listener);
    },
  },
};

contextBridge.exposeInMainWorld('rice', api);

export type RiceApi = typeof api;
