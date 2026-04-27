import { contextBridge, ipcRenderer } from 'electron';
import type { BackendRunRequest, BackendRunResult, RiceListRow } from '../../src/shared/backend';

const api = {
  closeWindow: () => ipcRenderer.send('window:close'),
  backend: {
    list: () => ipcRenderer.invoke('backend:list') as Promise<RiceListRow[]>,
    run: (request: BackendRunRequest) =>
      ipcRenderer.invoke('backend:run', request) as Promise<BackendRunResult>,
  },
};

contextBridge.exposeInMainWorld('rice', api);

export type RiceApi = typeof api;
