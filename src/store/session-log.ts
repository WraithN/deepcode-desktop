import { generateId } from '@/lib/id';

export type LogLevel = 'info' | 'warn' | 'error' | 'debug';

export interface LogEntry {
  id: string;
  timestamp: string;
  level: LogLevel;
  source: string;
  message: string;
  detail?: Record<string, unknown>;
}

class SessionLogStore {
  private logs = new Map<string, LogEntry[]>();
  private listeners = new Set<() => void>();
  private maxLogsPerSession = 500;

  add(
    conversationId: string,
    level: LogLevel,
    source: string,
    message: string,
    detail?: Record<string, unknown>,
  ): void {
    const entry: LogEntry = {
      id: generateId(),
      timestamp: `${new Date().toLocaleTimeString('en-US', { hour12: false })}.${String(new Date().getMilliseconds()).padStart(3, '0')}`,
      level,
      source,
      message,
      detail,
    };

    const sessionLogs = this.logs.get(conversationId) || [];
    sessionLogs.push(entry);
    if (sessionLogs.length > this.maxLogsPerSession) {
      sessionLogs.splice(0, sessionLogs.length - this.maxLogsPerSession);
    }
    this.logs.set(conversationId, sessionLogs);
    this.notify();
  }

  getLogs(conversationId: string): LogEntry[] {
    return this.logs.get(conversationId) || [];
  }

  clear(conversationId: string): void {
    this.logs.delete(conversationId);
    this.notify();
  }

  subscribe(listener: () => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  private notify(): void {
    for (const listener of this.listeners) {
      listener();
    }
  }
}

export const sessionLog = new SessionLogStore();
