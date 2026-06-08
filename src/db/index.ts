import type { IDataStore } from './types';
import { mockDataStore } from './mock';
import { tauriDataStore } from './tauri-client';

export type { IDataStore, AuthUser, AuthSession, AuthStateChangeCallback } from './types';

function isTauri(): boolean {
  return !!(window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ || !!(window as unknown as Record<string, unknown>).__TAURI__;
}

export const db: IDataStore = isTauri() ? tauriDataStore : mockDataStore;
