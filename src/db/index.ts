import type { IDataStore } from './types';
import { mockDataStore } from './mock';
import { sqliteDataStore } from './sqlite-client';

export type { IDataStore, AuthUser, AuthSession, AuthStateChangeCallback } from './types';

function isTauri(): boolean {
  return !!(window as any).__TAURI_INTERNALS__ || !!(window as any).__TAURI__;
}

export const db: IDataStore = isTauri() ? sqliteDataStore : mockDataStore;
