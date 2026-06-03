import { create } from 'zustand';
import { useWebSocketStore } from './websocketStore';

export interface SessionLogEntry {
  id: number;
  conversationId: string;
  timestamp: string;
  level: 'debug' | 'info' | 'warn' | 'error';
  source: string;
  message: string;
  payload?: unknown;
}

interface LogState {
  logs: SessionLogEntry[];
  filteredLogs: SessionLogEntry[];
  filterLevel: 'all' | 'debug' | 'info' | 'warn' | 'error';

  appendLog: (log: SessionLogEntry) => void;
  loadHistory: (conversationId: string) => Promise<void>;
  setFilterLevel: (level: 'all' | 'debug' | 'info' | 'warn' | 'error') => void;
}

export const useLogStore = create<LogState>((set, get) => ({
  logs: [],
  filteredLogs: [],
  filterLevel: 'all',

  appendLog: (log: SessionLogEntry) => {
    set((state) => {
      const newLogs = [...state.logs, log];
      return {
        logs: newLogs,
        filteredLogs: state.filterLevel === 'all'
          ? newLogs
          : newLogs.filter((l) => l.level === state.filterLevel),
      };
    });
  },

  loadHistory: async (conversationId: string) => {
    const ws = useWebSocketStore.getState();
    const logs = await ws.sendRequest<SessionLogEntry[]>('session.logLoad', { conversationId });

    set((state) => {
      const newLogs = [...logs, ...state.logs];
      return {
        logs: newLogs,
        filteredLogs: state.filterLevel === 'all'
          ? newLogs
          : newLogs.filter((l) => l.level === state.filterLevel),
      };
    });
  },

  setFilterLevel: (level) => {
    set((state) => ({
      filterLevel: level,
      filteredLogs: level === 'all'
        ? state.logs
        : state.logs.filter((l) => l.level === level),
    }));
  },
}));
