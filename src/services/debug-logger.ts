import { writeTextFile, mkdir, BaseDirectory } from '@tauri-apps/plugin-fs';

class DebugLogger {
  private initialized = false;
  private logFile = 'logs/debug.log';

  private async init(): Promise<void> {
    if (this.initialized) return;
    try {
      await mkdir('logs', { baseDir: BaseDirectory.AppLocalData, recursive: true });
      console.log('[DebugLogger] init success, log dir created');
    } catch (e) {
      console.error('[DebugLogger] mkdir failed:', e);
    }
    this.initialized = true;
  }

  async log(source: string, message: string, data?: Record<string, unknown>): Promise<void> {
    console.log(`[${source}]`, message, data || '');

    const entry = {
      timestamp: new Date().toISOString(),
      source,
      message,
      data: data || {},
    };
    const line = JSON.stringify(entry) + '\n';
    try {
      await this.init();
      await writeTextFile(this.logFile, line, { baseDir: BaseDirectory.AppLocalData, append: true });
    } catch (e) {
      console.error('[DebugLogger] write failed:', e);
    }
  }
}

export const debugLogger = new DebugLogger();
