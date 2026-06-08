import type { IDataStore } from './types';
import { mockDataStore } from './mock';
import { tauriDataStore } from './tauri-client';
import { isTauri } from '@/lib/env';

export type { IDataStore, AuthUser, AuthSession, AuthStateChangeCallback } from './types';

export const db: IDataStore = isTauri() ? tauriDataStore : mockDataStore;
