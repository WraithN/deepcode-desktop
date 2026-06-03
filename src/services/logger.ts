import { writeTextFile, mkdir, BaseDirectory } from '@tauri-apps/plugin-fs';

function isTauri(): boolean {
  return !!(window as any).__TAURI_INTERNALS__ || !!(window as any).__TAURI__;
}
import type { AgentEvent } from '@/stores';

interface LogEntry {
  timestamp: string;
  type: 'trace' | 'generation' | 'observation' | 'event' | 'error';
  data: Record<string, unknown>;
}

class SessionLogger {
  private logDir = 'logs';
  private currentFile: string | null = null;
  private initialized = false;
  private baseDir = isTauri() ? BaseDirectory.AppLog : BaseDirectory.Temp;

  private async init(): Promise<void> {
    if (this.initialized) return;
    try {
      await mkdir(this.logDir, { baseDir: this.baseDir, recursive: true });
    } catch {
      // 目录可能已存在
    }
    this.initialized = true;
  }

  private getLogFile(sessionId: string): string {
    const date = new Date().toISOString().split('T')[0];
    return `${this.logDir}/session-${sessionId}-${date}.jsonl`;
  }

  private async append(entry: LogEntry): Promise<void> {
    if (!isTauri()) return; // 浏览器环境下不写入文件
    await this.init();
    const file = this.currentFile || this.getLogFile('default');
    const line = JSON.stringify(entry) + '\n';
    try {
      await writeTextFile(file, line, { baseDir: this.baseDir, append: true });
    } catch {
      // 文件不存在时创建
      await writeTextFile(file, line, { baseDir: this.baseDir, append: false });
    }
  }

  async startTrace(sessionId: string, metadata?: Record<string, unknown>): Promise<void> {
    this.currentFile = this.getLogFile(sessionId);
    await this.append({
      timestamp: new Date().toISOString(),
      type: 'trace',
      data: {
        id: sessionId,
        name: 'workspace-session',
        metadata: metadata || {},
      },
    });
  }

  async logGeneration(
    generationId: string,
    input: string,
    output?: string,
    metadata?: Record<string, unknown>,
  ): Promise<void> {
    await this.append({
      timestamp: new Date().toISOString(),
      type: 'generation',
      data: {
        id: generationId,
        input,
        output,
        metadata: metadata || {},
      },
    });
  }

  async logEvent(event: AgentEvent, metadata?: Record<string, unknown>): Promise<void> {
    await this.append({
      timestamp: new Date().toISOString(),
      type: 'event',
      data: {
        eventType: event.type,
        ...event,
        metadata: metadata || {},
      },
    });
  }

  async logError(error: Error | string, context?: Record<string, unknown>): Promise<void> {
    await this.append({
      timestamp: new Date().toISOString(),
      type: 'error',
      data: {
        message: error instanceof Error ? error.message : error,
        stack: error instanceof Error ? error.stack : undefined,
        context: context || {},
      },
    });
  }

  async logObservation(
    name: string,
    input?: string,
    output?: string,
    metadata?: Record<string, unknown>,
  ): Promise<void> {
    await this.append({
      timestamp: new Date().toISOString(),
      type: 'observation',
      data: {
        name,
        input,
        output,
        metadata: metadata || {},
      },
    });
  }
}

export const sessionLogger = new SessionLogger();
