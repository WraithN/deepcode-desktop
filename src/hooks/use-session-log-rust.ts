import { useEffect } from 'react';
import { listen } from '@tauri-apps/api/event';
import { sessionLog } from '@/store/session-log';

interface SessionLogEntry {
  conversation_id: string;
  timestamp: string;
  level: 'debug' | 'info' | 'warn' | 'error';
  source: string;
  message: string;
  payload?: unknown;
}

export function useRustSessionLog() {
  useEffect(() => {
    const unlisten = listen<SessionLogEntry>('session:log', (event) => {
      const entry = event.payload;
      sessionLog.add(
        entry.conversation_id,
        entry.level,
        entry.source,
        entry.message,
        entry.payload,
      );
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);
}
