import { contextBridge, ipcRenderer } from 'electron';

const api = {
  closeWindow: () => ipcRenderer.send('window:close'),
};

contextBridge.exposeInMainWorld('rice', api);

export type RiceApi = typeof api;
