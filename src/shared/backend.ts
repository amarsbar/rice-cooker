export interface RiceListRow {
  name: string;
  display_name: string;
  description: string;
  repo: string;
  install_supported: boolean;
  installed: boolean;
  documented_system_effects: string[];
}

export type BackendCommand = 'preview' | 'try' | 'uninstall';

export interface BackendRunRequest {
  command: BackendCommand;
  name?: string;
}

export type BackendEvent =
  | { type: 'hello'; version: number; subcommand: string }
  | { type: 'step'; step: string; state: 'start' | 'done' }
  | { type: 'success'; active?: string }
  | { type: 'fail'; stage: string; reason: string; log_tail?: string; plugins?: string[] };

export interface BackendRunResult {
  ok: boolean;
  events: BackendEvent[];
  rawTail: string[];
  exitCode: number | null;
}
